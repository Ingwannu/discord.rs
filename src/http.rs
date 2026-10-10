use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use reqwest::{header::HeaderMap, Client, Method, StatusCode};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use tracing::{debug, warn};

mod application_identity;
mod attachment;
mod auto_moderation;
mod body;
mod client_config;
mod emoji;
mod lobby;
mod media_resources;
mod messages;
mod monetization;
mod oauth;
mod paths;
mod rate_limit;
mod scheduled_events;
#[cfg(test)]
mod tests;
mod threads;
mod webhook;

use crate::command::CommandDefinition;
use crate::error::DiscordError;
use crate::model::{
    ActivityInstance, AddGroupDmRecipient, AddGuildMember, AllowedMentions, Application,
    ApplicationCommand, ApplicationRoleConnectionMetadata, AuditLog, AuditLogQuery, Ban,
    BeginGuildPruneRequest, BulkGuildBanRequest, BulkGuildBanResponse, Channel,
    CreateChannelInvite, CreateDmChannel, CreateGroupDmChannel, CreateGuildChannel,
    CreateGuildRole, CreateMessage, CreateStageInstance, EditApplicationCommandPermissions,
    EditChannelPermission, FollowedChannel, Gateway, GatewayBot, GetGuildQuery, Guild,
    GuildApplicationCommandPermissions, GuildBansQuery, GuildIncidentsData, GuildMembersQuery,
    GuildOnboarding, GuildPreview, GuildPruneCount, GuildPruneResult, GuildTemplate, GuildWidget,
    GuildWidgetImageStyle, GuildWidgetSettings, Integration, InteractionCallbackResponse, Invite,
    InviteTargetUserIds, InviteTargetUsersJobStatus, Member, Message, ModifyCurrentApplication,
    ModifyCurrentUserVoiceState, ModifyGuild, ModifyGuildChannelPosition,
    ModifyGuildIncidentActions, ModifyGuildOnboarding, ModifyGuildRole, ModifyGuildRolePosition,
    ModifyGuildWelcomeScreen, ModifyGuildWidgetSettings, ModifyStageInstance, ModifyUserVoiceState,
    PollAnswerVoters, Role, SearchGuildMembersQuery, SetVoiceChannelStatus, Snowflake,
    StageInstance, VanityUrl, VoiceRegion, VoiceState, WelcomeScreen,
};
use crate::types::invalid_data_error;
pub use attachment::{FileAttachment, FileUpload};
use body::{
    build_multipart_form, build_named_file_form, build_sticker_form, clone_json_body,
    multipart_body, named_file_multipart_body, parse_body_value, payload_named_file_multipart_body,
    serialize_body, RequestBody,
};
pub use client_config::{RateLimitCallback, RateLimitInfo, RestClientBuilder};
use paths::{
    archived_threads_query, audit_log_query, bool_query, configured_application_id,
    entitlement_query, followup_webhook_path, get_guild_query, global_commands_path,
    guild_bans_query, guild_members_query, guild_prune_query, interaction_callback_path,
    invite_query, joined_archived_threads_query, poll_answer_voters_query,
    request_uses_bot_authorization, route_token, scrub_token, search_guild_members_query,
    subscription_query, thread_member_query, validate_token_path_segment, RequestRoute,
    RouteOperation,
};
use rate_limit::RateLimitState;

pub(crate) use paths::redact_path_tokens;
#[cfg(test)]
use rate_limit::RATE_LIMIT_BUCKET_RETENTION;

const API_BASE: &str = "https://discord.com/api/v10";
const DEFAULT_USER_AGENT: &str = concat!("DiscordBot (discordrs, ", env!("CARGO_PKG_VERSION"), ")");
const MAX_RATE_LIMIT_RETRIES: usize = 5;
const MAX_SERVER_ERROR_RETRIES: usize = 3;
const ROUTE_GATE_GC_THRESHOLD: usize = 512;
/// Discord's per-request cap for invite target-user bulk add/delete.
const MAX_INVITE_TARGET_USERS: usize = 1000;

#[derive(Clone, Copy)]
enum RequestAuthorization<'a> {
    Auto,
    Bearer(&'a str),
    None,
}

/// Typed Discord REST client with shared per-route and global rate-limit
/// state. Cloning is cheap: clones share the same rate-limit bookkeeping,
/// connection pool, and application id.
#[derive(Clone)]
pub struct RestClient {
    client: Client,
    token: String,
    application_id: Arc<AtomicU64>,
    rate_limits: Arc<RateLimitState>,
    route_gates: Arc<RateLimitRouteGates>,
    audit_log_reason: Option<String>,
    base_url: String,
    user_agent: String,
    default_allowed_mentions: Option<AllowedMentions>,
    rate_limit_callback: Option<RateLimitCallback>,
    /// Opt-in to replaying POST/PATCH after an ambiguous failure; see
    /// [`RestClientBuilder::retry_non_idempotent_requests`].
    retry_non_idempotent: bool,
    /// True when `client` was built by this crate with redirects disabled,
    /// which is what makes a connect failure proof that nothing was sent.
    managed_transport: bool,
}

/// Type alias for `DiscordHttpClient`.
pub type DiscordHttpClient = RestClient;

#[derive(Default)]
struct RateLimitRouteGates {
    routes: StdMutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl RateLimitRouteGates {
    fn gate_for(&self, route_key: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut routes = self
            .routes
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if routes.len() >= ROUTE_GATE_GC_THRESHOLD {
            // Gates whose only strong reference is the map itself have no
            // in-flight request; dropping them bounds the map without ever
            // discarding a gate that is currently serializing requests.
            routes.retain(|key, gate| key == route_key || Arc::strong_count(gate) > 1);
        }
        Arc::clone(
            routes
                .entry(route_key.to_string())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(()))),
        )
    }
}

impl RestClient {
    /// Creates a Discord REST client with bounded connect and request timeouts.
    pub fn new(token: impl Into<String>, application_id: u64) -> Self {
        Self {
            client: client_config::default_http_client(),
            token: token.into(),
            application_id: Arc::new(AtomicU64::new(application_id)),
            rate_limits: Arc::new(RateLimitState::default()),
            route_gates: Arc::new(RateLimitRouteGates::default()),
            audit_log_reason: None,
            base_url: API_BASE.to_string(),
            user_agent: DEFAULT_USER_AGENT.to_string(),
            default_allowed_mentions: None,
            rate_limit_callback: None,
            retry_non_idempotent: false,
            managed_transport: true,
        }
    }

    /// Returns a [`RestClientBuilder`] for a client with a custom API base,
    /// timeouts, user agent, proxy, rate-limit callback, or default
    /// allowed-mentions — the equivalent of discord.js's `RESTOptions`.
    pub fn builder(token: impl Into<String>, application_id: u64) -> RestClientBuilder {
        RestClientBuilder::new(token, application_id)
    }

    #[cfg(test)]
    fn new_with_base_url(
        token: impl Into<String>,
        application_id: u64,
        base_url: impl Into<String>,
    ) -> Self {
        let mut client = Self::new(token, application_id);
        client.base_url = base_url.into();
        client
    }

    /// Returns a client that sends `X-Audit-Log-Reason` with every mutating
    /// request it makes, so the reason appears in the guild audit log.
    ///
    /// The returned client shares rate-limit state and the connection pool
    /// with `self`; creating one is cheap and intended per call site:
    ///
    /// ```ignore
    /// client.with_reason("spam").remove_guild_member(guild_id, user_id).await?;
    /// ```
    pub fn with_reason(&self, reason: impl Into<String>) -> Self {
        let mut scoped = self.clone();
        scoped.audit_log_reason = Some(reason.into());
        scoped
    }

    /// Returns a client that injects the given allowed-mentions into message
    /// payloads that do not set `allowed_mentions` themselves. Like
    /// [`Self::with_reason`], the returned client shares rate-limit state and
    /// the connection pool with `self`.
    pub fn with_default_allowed_mentions(&self, allowed_mentions: AllowedMentions) -> Self {
        let mut scoped = self.clone();
        scoped.default_allowed_mentions = Some(allowed_mentions);
        scoped
    }

    /// Allowed-mentions injected into outgoing message payloads that do not
    /// set `allowed_mentions` themselves, if configured.
    pub fn default_allowed_mentions(&self) -> Option<&AllowedMentions> {
        self.default_allowed_mentions.as_ref()
    }

    fn api_base(&self) -> &str {
        &self.base_url
    }

    pub fn application_id(&self) -> u64 {
        self.application_id.load(Ordering::Relaxed)
    }

    pub fn set_application_id(&self, application_id: u64) {
        self.application_id.store(application_id, Ordering::Relaxed);
    }

    pub async fn get_channel(
        &self,
        channel_id: impl Into<Snowflake>,
    ) -> Result<Channel, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/channels/{}", channel_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn delete_channel(
        &self,
        channel_id: impl Into<Snowflake>,
    ) -> Result<Channel, DiscordError> {
        self.request_typed(
            Method::DELETE,
            &format!("/channels/{}", channel_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn update_channel(
        &self,
        channel_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<Channel, DiscordError> {
        self.update_channel_typed(channel_id, body).await
    }

    pub async fn update_channel_typed<B>(
        &self,
        channel_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<Channel, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(
            Method::PATCH,
            &format!("/channels/{}", channel_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn get_guild(&self, guild_id: impl Into<Snowflake>) -> Result<Guild, DiscordError> {
        self.get_guild_with_query(guild_id, &GetGuildQuery::default())
            .await
    }

    pub async fn get_guild_with_query(
        &self,
        guild_id: impl Into<Snowflake>,
        query: &GetGuildQuery,
    ) -> Result<Guild, DiscordError> {
        let query = get_guild_query(query);
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}{query}", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn update_guild(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<Guild, DiscordError> {
        self.update_guild_typed(guild_id, body).await
    }

    pub async fn update_guild_typed<B>(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<Guild, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(
            Method::PATCH,
            &format!("/guilds/{}", guild_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn modify_guild(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &ModifyGuild,
    ) -> Result<Guild, DiscordError> {
        self.update_guild_typed(guild_id, body).await
    }

    pub async fn get_guild_channels(
        &self,
        guild_id: impl Into<Snowflake>,
    ) -> Result<Vec<Channel>, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/channels", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn create_guild_channel(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<Channel, DiscordError> {
        self.create_guild_channel_typed(guild_id, body).await
    }

    pub async fn create_guild_channel_typed<B>(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<Channel, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(
            Method::POST,
            &format!("/guilds/{}/channels", guild_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn create_guild_channel_from_request(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &CreateGuildChannel,
    ) -> Result<Channel, DiscordError> {
        self.create_guild_channel_typed(guild_id, body).await
    }

    /// Reorder or reparent guild channels.
    ///
    /// Discord returns no body on success. Only entries for channels being
    /// modified need to be included.
    pub async fn modify_guild_channel_positions(
        &self,
        guild_id: impl Into<Snowflake>,
        positions: &[ModifyGuildChannelPosition],
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::PATCH,
            &format!("/guilds/{}/channels", guild_id.into()),
            Some(positions),
        )
        .await
    }

    pub async fn get_guild_members(
        &self,
        guild_id: impl Into<Snowflake>,
        limit: Option<u64>,
    ) -> Result<Vec<Member>, DiscordError> {
        self.get_guild_members_with_query(
            guild_id,
            &GuildMembersQuery {
                limit,
                ..GuildMembersQuery::default()
            },
        )
        .await
    }

    pub async fn get_guild_members_with_query(
        &self,
        guild_id: impl Into<Snowflake>,
        query: &GuildMembersQuery,
    ) -> Result<Vec<Member>, DiscordError> {
        let query = guild_members_query(query);
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/members{query}", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Add an OAuth2-authorized user to a guild.
    ///
    /// Returns `Some(member)` when Discord creates the guild membership and
    /// `None` when Discord reports the user is already a member.
    pub async fn add_guild_member(
        &self,
        guild_id: impl Into<Snowflake>,
        user_id: impl Into<Snowflake>,
        body: &AddGuildMember,
    ) -> Result<Option<Member>, DiscordError> {
        self.request_optional_typed_no_content(
            Method::PUT,
            &format!("/guilds/{}/members/{}", guild_id.into(), user_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn remove_guild_member(
        &self,
        guild_id: impl Into<Snowflake>,
        user_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!("/guilds/{}/members/{}", guild_id.into(), user_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn add_guild_member_role(
        &self,
        guild_id: impl Into<Snowflake>,
        user_id: impl Into<Snowflake>,
        role_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::PUT,
            &format!(
                "/guilds/{}/members/{}/roles/{}",
                guild_id.into(),
                user_id.into(),
                role_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn remove_guild_member_role(
        &self,
        guild_id: impl Into<Snowflake>,
        user_id: impl Into<Snowflake>,
        role_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!(
                "/guilds/{}/members/{}/roles/{}",
                guild_id.into(),
                user_id.into(),
                role_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn create_role(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<Role, DiscordError> {
        self.create_role_typed(guild_id, body).await
    }

    pub async fn create_role_typed<B>(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<Role, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(
            Method::POST,
            &format!("/guilds/{}/roles", guild_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn update_role(
        &self,
        guild_id: impl Into<Snowflake>,
        role_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<Role, DiscordError> {
        self.update_role_typed(guild_id, role_id, body).await
    }

    pub async fn update_role_typed<B>(
        &self,
        guild_id: impl Into<Snowflake>,
        role_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<Role, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(
            Method::PATCH,
            &format!("/guilds/{}/roles/{}", guild_id.into(), role_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn delete_role(
        &self,
        guild_id: impl Into<Snowflake>,
        role_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!("/guilds/{}/roles/{}", guild_id.into(), role_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_member(
        &self,
        guild_id: impl Into<Snowflake>,
        user_id: impl Into<Snowflake>,
    ) -> Result<Member, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/members/{}", guild_id.into(), user_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn list_roles(
        &self,
        guild_id: impl Into<Snowflake>,
    ) -> Result<Vec<Role>, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/roles", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn create_dm_channel_typed(
        &self,
        body: &CreateDmChannel,
    ) -> Result<Channel, DiscordError> {
        self.request_typed(Method::POST, "/users/@me/channels", Some(body))
            .await
    }

    pub async fn create_group_dm_channel_typed(
        &self,
        body: &CreateGroupDmChannel,
    ) -> Result<Channel, DiscordError> {
        self.request_typed(Method::POST, "/users/@me/channels", Some(body))
            .await
    }

    pub async fn add_group_dm_recipient(
        &self,
        channel_id: impl Into<Snowflake>,
        user_id: impl Into<Snowflake>,
        body: &AddGroupDmRecipient,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::PUT,
            &format!(
                "/channels/{}/recipients/{}",
                channel_id.into(),
                user_id.into()
            ),
            Some(body),
        )
        .await
    }

    pub async fn remove_group_dm_recipient(
        &self,
        channel_id: impl Into<Snowflake>,
        user_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!(
                "/channels/{}/recipients/{}",
                channel_id.into(),
                user_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn create_interaction_response_typed(
        &self,
        interaction_id: impl Into<Snowflake>,
        interaction_token: &str,
        body: &InteractionCallbackResponse,
    ) -> Result<(), DiscordError> {
        let path = interaction_callback_path(interaction_id.into(), interaction_token)?;
        let body = self.interaction_response_with_default_allowed_mentions(body);
        self.request_no_content(Method::POST, &path, Some(body.as_ref()))
            .await
    }

    pub async fn create_interaction_response_with_files(
        &self,
        interaction_id: impl Into<Snowflake>,
        interaction_token: &str,
        body: &InteractionCallbackResponse,
        files: &[FileAttachment],
    ) -> Result<(), DiscordError> {
        let path = interaction_callback_path(interaction_id.into(), interaction_token)?;
        self.request_multipart_no_content(Method::POST, &path, body, files)
            .await
    }

    pub async fn bulk_overwrite_global_commands_typed(
        &self,
        commands: &[CommandDefinition],
    ) -> Result<Vec<ApplicationCommand>, DiscordError> {
        let path = global_commands_path(self.application_id())?;
        self.request_typed(Method::PUT, &path, Some(commands)).await
    }

    pub async fn create_global_command(
        &self,
        command: &CommandDefinition,
    ) -> Result<ApplicationCommand, DiscordError> {
        let path = global_commands_path(self.application_id())?;
        self.request_typed(Method::POST, &path, Some(command)).await
    }

    pub async fn get_global_commands(&self) -> Result<Vec<ApplicationCommand>, DiscordError> {
        let path = global_commands_path(self.application_id())?;
        self.request_typed(Method::GET, &path, Option::<&Value>::None)
            .await
    }

    pub async fn get_current_application(&self) -> Result<Application, DiscordError> {
        self.request_typed(Method::GET, "/applications/@me", Option::<&Value>::None)
            .await
    }

    pub async fn edit_current_application<B>(&self, body: &B) -> Result<Application, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(Method::PATCH, "/applications/@me", Some(body))
            .await
    }

    pub async fn edit_current_application_from_request(
        &self,
        body: &ModifyCurrentApplication,
    ) -> Result<Application, DiscordError> {
        self.edit_current_application(body).await
    }

    pub async fn get_application_activity_instance(
        &self,
        instance_id: &str,
    ) -> Result<ActivityInstance, DiscordError> {
        validate_token_path_segment("instance_id", instance_id, false)?;
        let application_id = configured_application_id(self.application_id())?;
        self.request_typed(
            Method::GET,
            &format!("/applications/{application_id}/activity-instances/{instance_id}"),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_application_role_connection_metadata_records(
        &self,
    ) -> Result<Vec<ApplicationRoleConnectionMetadata>, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request_typed(
            Method::GET,
            &format!("/applications/{application_id}/role-connections/metadata"),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn update_application_role_connection_metadata_records(
        &self,
        records: &[ApplicationRoleConnectionMetadata],
    ) -> Result<Vec<ApplicationRoleConnectionMetadata>, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request_typed(
            Method::PUT,
            &format!("/applications/{application_id}/role-connections/metadata"),
            Some(records),
        )
        .await
    }

    pub async fn get_gateway_bot(&self) -> Result<GatewayBot, DiscordError> {
        self.request_typed(Method::GET, "/gateway/bot", Option::<&Value>::None)
            .await
    }

    pub async fn get_gateway(&self) -> Result<Gateway, DiscordError> {
        self.request_typed(Method::GET, "/gateway", Option::<&Value>::None)
            .await
    }

    pub async fn bulk_overwrite_guild_commands_typed(
        &self,
        guild_id: impl Into<Snowflake>,
        commands: &[CommandDefinition],
    ) -> Result<Vec<ApplicationCommand>, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request_typed(
            Method::PUT,
            &format!(
                "/applications/{application_id}/guilds/{}/commands",
                guild_id.into()
            ),
            Some(commands),
        )
        .await
    }

    pub(crate) async fn send_message_json(
        &self,
        channel_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<Value, DiscordError> {
        self.request(
            Method::POST,
            &format!("/channels/{}/messages", channel_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn pin_message(
        &self,
        channel_id: impl Into<Snowflake>,
        message_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::PUT,
            &format!(
                "/channels/{}/messages/pins/{}",
                channel_id.into(),
                message_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn unpin_message(
        &self,
        channel_id: impl Into<Snowflake>,
        message_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!(
                "/channels/{}/messages/pins/{}",
                channel_id.into(),
                message_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_pinned_messages(
        &self,
        channel_id: impl Into<Snowflake>,
    ) -> Result<Vec<Message>, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/channels/{}/pins", channel_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Calls the legacy `pin_message` route.
    pub async fn pin_message_legacy(
        &self,
        channel_id: impl Into<Snowflake>,
        message_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::PUT,
            &format!("/channels/{}/pins/{}", channel_id.into(), message_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Calls the legacy `unpin_message` route.
    pub async fn unpin_message_legacy(
        &self,
        channel_id: impl Into<Snowflake>,
        message_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!("/channels/{}/pins/{}", channel_id.into(), message_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn set_voice_channel_status(
        &self,
        channel_id: impl Into<Snowflake>,
        body: &SetVoiceChannelStatus,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::PUT,
            &format!("/channels/{}/voice-status", channel_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn trigger_typing_indicator(
        &self,
        channel_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::POST,
            &format!("/channels/{}/typing", channel_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn edit_channel_permissions(
        &self,
        channel_id: impl Into<Snowflake>,
        overwrite_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::PUT,
            &format!(
                "/channels/{}/permissions/{}",
                channel_id.into(),
                overwrite_id.into()
            ),
            Some(body),
        )
        .await
    }

    pub async fn edit_channel_permissions_typed(
        &self,
        channel_id: impl Into<Snowflake>,
        overwrite_id: impl Into<Snowflake>,
        body: &EditChannelPermission,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::PUT,
            &format!(
                "/channels/{}/permissions/{}",
                channel_id.into(),
                overwrite_id.into()
            ),
            Some(body),
        )
        .await
    }

    pub async fn delete_channel_permission(
        &self,
        channel_id: impl Into<Snowflake>,
        overwrite_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!(
                "/channels/{}/permissions/{}",
                channel_id.into(),
                overwrite_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_guild_bans(
        &self,
        guild_id: impl Into<Snowflake>,
        limit: Option<u64>,
        before: Option<Snowflake>,
    ) -> Result<Vec<Ban>, DiscordError> {
        self.get_guild_bans_with_query(
            guild_id,
            &GuildBansQuery {
                limit,
                before,
                ..GuildBansQuery::default()
            },
        )
        .await
    }

    pub async fn get_guild_bans_with_query(
        &self,
        guild_id: impl Into<Snowflake>,
        query: &GuildBansQuery,
    ) -> Result<Vec<Ban>, DiscordError> {
        let query = guild_bans_query(query);
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/bans{}", guild_id.into(), query),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_guild_ban(
        &self,
        guild_id: impl Into<Snowflake>,
        user_id: impl Into<Snowflake>,
    ) -> Result<Ban, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/bans/{}", guild_id.into(), user_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn create_guild_ban(
        &self,
        guild_id: impl Into<Snowflake>,
        user_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<(), DiscordError> {
        self.create_guild_ban_typed(guild_id, user_id, body).await
    }

    pub async fn create_guild_ban_typed<B>(
        &self,
        guild_id: impl Into<Snowflake>,
        user_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<(), DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_no_content(
            Method::PUT,
            &format!("/guilds/{}/bans/{}", guild_id.into(), user_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn bulk_guild_ban(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &BulkGuildBanRequest,
    ) -> Result<BulkGuildBanResponse, DiscordError> {
        self.request_typed(
            Method::POST,
            &format!("/guilds/{}/bulk-ban", guild_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn remove_guild_ban(
        &self,
        guild_id: impl Into<Snowflake>,
        user_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!("/guilds/{}/bans/{}", guild_id.into(), user_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn modify_guild_member(
        &self,
        guild_id: impl Into<Snowflake>,
        user_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<(), DiscordError> {
        self.modify_guild_member_typed(guild_id, user_id, body)
            .await
    }

    pub async fn modify_guild_member_typed<B>(
        &self,
        guild_id: impl Into<Snowflake>,
        user_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<(), DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_no_content(
            Method::PATCH,
            &format!("/guilds/{}/members/{}", guild_id.into(), user_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn modify_current_member_nick(
        &self,
        guild_id: impl Into<Snowflake>,
        nick: Option<&str>,
    ) -> Result<(), DiscordError> {
        let body = serde_json::json!({ "nick": nick });
        self.request_no_content(
            Method::PATCH,
            &format!("/guilds/{}/members/@me/nick", guild_id.into()),
            Some(&body),
        )
        .await
    }

    pub async fn modify_current_member<B>(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<Member, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(
            Method::PATCH,
            &format!("/guilds/{}/members/@me", guild_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn search_guild_members(
        &self,
        guild_id: impl Into<Snowflake>,
        query: &str,
        limit: Option<u64>,
    ) -> Result<Vec<Member>, DiscordError> {
        self.search_guild_members_with_query(
            guild_id,
            &SearchGuildMembersQuery {
                query: query.to_string(),
                limit,
            },
        )
        .await
    }

    pub async fn search_guild_members_with_query(
        &self,
        guild_id: impl Into<Snowflake>,
        query: &SearchGuildMembersQuery,
    ) -> Result<Vec<Member>, DiscordError> {
        let query = search_guild_members_query(query);
        let path = format!("/guilds/{}/members/search{}", guild_id.into(), query);
        self.request_typed(Method::GET, &path, Option::<&Value>::None)
            .await
    }

    /// Fetches a guild audit log as raw JSON.
    #[deprecated(
        since = "2.0.1",
        note = "Use get_guild_audit_log_typed for a typed AuditLog"
    )]
    pub async fn get_guild_audit_log(
        &self,
        guild_id: impl Into<Snowflake>,
        user_id: Option<Snowflake>,
        action_type: Option<u64>,
        before: Option<Snowflake>,
        limit: Option<u64>,
    ) -> Result<serde_json::Value, DiscordError> {
        let query = audit_log_query(&AuditLogQuery {
            user_id,
            action_type,
            before,
            after: None,
            limit,
        });
        self.request(
            Method::GET,
            &format!("/guilds/{}/audit-logs{}", guild_id.into(), query),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_guild_audit_log_typed(
        &self,
        guild_id: impl Into<Snowflake>,
        query: &AuditLogQuery,
    ) -> Result<AuditLog, DiscordError> {
        let query = audit_log_query(query);
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/audit-logs{}", guild_id.into(), query),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn modify_guild_role_positions(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<Vec<Role>, DiscordError> {
        self.request_typed(
            Method::PATCH,
            &format!("/guilds/{}/roles", guild_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn modify_guild_role_positions_typed(
        &self,
        guild_id: impl Into<Snowflake>,
        positions: &[ModifyGuildRolePosition],
    ) -> Result<Vec<Role>, DiscordError> {
        self.request_typed(
            Method::PATCH,
            &format!("/guilds/{}/roles", guild_id.into()),
            Some(positions),
        )
        .await
    }

    pub async fn create_guild_role(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &CreateGuildRole,
    ) -> Result<Role, DiscordError> {
        self.create_role_typed(guild_id, body).await
    }

    pub async fn modify_guild_role(
        &self,
        guild_id: impl Into<Snowflake>,
        role_id: impl Into<Snowflake>,
        body: &ModifyGuildRole,
    ) -> Result<Role, DiscordError> {
        self.update_role_typed(guild_id, role_id, body).await
    }

    pub async fn get_guild_role(
        &self,
        guild_id: impl Into<Snowflake>,
        role_id: impl Into<Snowflake>,
    ) -> Result<Role, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/roles/{}", guild_id.into(), role_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Return the number of guild members assigned to each non-`@everyone` role.
    pub async fn get_guild_role_member_counts(
        &self,
        guild_id: impl Into<Snowflake>,
    ) -> Result<HashMap<Snowflake, u64>, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/roles/member-counts", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_guild_invites(
        &self,
        guild_id: impl Into<Snowflake>,
    ) -> Result<Vec<Invite>, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/invites", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_invite(&self, code: &str) -> Result<Invite, DiscordError> {
        let code = code.trim();
        validate_token_path_segment("invite_code", code, false)?;
        self.request_typed(
            Method::GET,
            &format!("/invites/{code}"),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_invite_with_options(
        &self,
        code: &str,
        with_counts: Option<bool>,
        with_expiration: Option<bool>,
        guild_scheduled_event_id: Option<Snowflake>,
    ) -> Result<Invite, DiscordError> {
        let code = code.trim();
        validate_token_path_segment("invite_code", code, false)?;
        self.request_typed(
            Method::GET,
            &format!(
                "/invites/{code}{}",
                invite_query(with_counts, with_expiration, guild_scheduled_event_id)
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn delete_invite(&self, code: &str) -> Result<Invite, DiscordError> {
        let code = code.trim();
        validate_token_path_segment("invite_code", code, false)?;
        self.request_typed(
            Method::DELETE,
            &format!("/invites/{code}"),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_invite_target_users(&self, code: &str) -> Result<Vec<u8>, DiscordError> {
        let code = code.trim();
        validate_token_path_segment("invite_code", code, false)?;
        self.request_bytes_with_authorization(
            RequestAuthorization::Auto,
            Method::GET,
            &format!("/invites/{code}/target-users"),
        )
        .await
    }

    pub async fn get_invite_target_users_csv(&self, code: &str) -> Result<String, DiscordError> {
        String::from_utf8(self.get_invite_target_users(code).await?)
            .map_err(|_| invalid_data_error("invite target users response was not valid UTF-8"))
    }

    pub async fn update_invite_target_users(
        &self,
        code: &str,
        target_users_file: &FileAttachment,
    ) -> Result<(), DiscordError> {
        let code = code.trim();
        validate_token_path_segment("invite_code", code, false)?;
        self.request_with_headers(
            Method::PUT,
            &format!("/invites/{code}/target-users"),
            Some(named_file_multipart_body(
                "target_users_file",
                target_users_file,
            )),
        )
        .await?;
        Ok(())
    }

    /// Adds one user to an invite's target users (`PUT
    /// /invites/{code}/target-users/{user.id}`). Requires being the inviter
    /// or `MANAGE_GUILD`.
    pub async fn add_invite_target_user(
        &self,
        code: &str,
        user_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        let code = code.trim();
        validate_token_path_segment("invite_code", code, false)?;
        self.request_no_content(
            Method::PUT,
            &format!("/invites/{code}/target-users/{}", user_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Removes one user from an invite's target users.
    pub async fn remove_invite_target_user(
        &self,
        code: &str,
        user_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        let code = code.trim();
        validate_token_path_segment("invite_code", code, false)?;
        self.request_no_content(
            Method::DELETE,
            &format!("/invites/{code}/target-users/{}", user_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Adds up to 1000 users to an invite's target users in one request,
    /// without rebuilding the whole list from a CSV.
    pub async fn bulk_add_invite_target_users(
        &self,
        code: &str,
        user_ids: &[Snowflake],
    ) -> Result<(), DiscordError> {
        self.bulk_invite_target_users(code, "bulk-add", user_ids)
            .await
    }

    /// Removes up to 1000 users from an invite's target users in one request.
    pub async fn bulk_delete_invite_target_users(
        &self,
        code: &str,
        user_ids: &[Snowflake],
    ) -> Result<(), DiscordError> {
        self.bulk_invite_target_users(code, "bulk-delete", user_ids)
            .await
    }

    async fn bulk_invite_target_users(
        &self,
        code: &str,
        action: &str,
        user_ids: &[Snowflake],
    ) -> Result<(), DiscordError> {
        let code = code.trim();
        validate_token_path_segment("invite_code", code, false)?;
        if user_ids.len() > MAX_INVITE_TARGET_USERS {
            return Err(invalid_data_error(format!(
                "invite target-user {action} accepts at most {MAX_INVITE_TARGET_USERS} users, got {}",
                user_ids.len()
            )));
        }
        let body = InviteTargetUserIds {
            user_ids: user_ids.to_vec(),
        };
        self.request_no_content(
            Method::POST,
            &format!("/invites/{code}/target-users/{action}"),
            Some(&body),
        )
        .await
    }

    pub async fn get_invite_target_users_job_status(
        &self,
        code: &str,
    ) -> Result<InviteTargetUsersJobStatus, DiscordError> {
        let code = code.trim();
        validate_token_path_segment("invite_code", code, false)?;
        self.request_typed(
            Method::GET,
            &format!("/invites/{code}/target-users/job-status"),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_guild_integrations(
        &self,
        guild_id: impl Into<Snowflake>,
    ) -> Result<Vec<Integration>, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/integrations", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn delete_guild_integration(
        &self,
        guild_id: impl Into<Snowflake>,
        integration_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!(
                "/guilds/{}/integrations/{}",
                guild_id.into(),
                integration_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_poll_answer_voters(
        &self,
        channel_id: impl Into<Snowflake>,
        message_id: impl Into<Snowflake>,
        answer_id: u64,
        after: Option<Snowflake>,
        limit: Option<u64>,
    ) -> Result<PollAnswerVoters, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!(
                "/channels/{}/polls/{}/answers/{answer_id}{}",
                channel_id.into(),
                message_id.into(),
                poll_answer_voters_query(after, limit)
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn end_poll(
        &self,
        channel_id: impl Into<Snowflake>,
        message_id: impl Into<Snowflake>,
    ) -> Result<Message, DiscordError> {
        self.request_typed(
            Method::POST,
            &format!(
                "/channels/{}/polls/{}/expire",
                channel_id.into(),
                message_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn create_channel_invite(
        &self,
        channel_id: impl Into<Snowflake>,
        body: Option<&Value>,
    ) -> Result<Invite, DiscordError> {
        self.request_typed(
            Method::POST,
            &format!("/channels/{}/invites", channel_id.into()),
            body,
        )
        .await
    }

    pub async fn get_channel_invites(
        &self,
        channel_id: impl Into<Snowflake>,
    ) -> Result<Vec<Invite>, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/channels/{}/invites", channel_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn create_channel_invite_typed(
        &self,
        channel_id: impl Into<Snowflake>,
        body: &CreateChannelInvite,
    ) -> Result<Invite, DiscordError> {
        self.request_typed(
            Method::POST,
            &format!("/channels/{}/invites", channel_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn create_channel_invite_with_target_users_file(
        &self,
        channel_id: impl Into<Snowflake>,
        body: &CreateChannelInvite,
        target_users_file: &FileAttachment,
    ) -> Result<Invite, DiscordError> {
        let response = self
            .request_with_headers(
                Method::POST,
                &format!("/channels/{}/invites", channel_id.into()),
                Some(payload_named_file_multipart_body(
                    body,
                    "target_users_file",
                    target_users_file,
                )?),
            )
            .await?;
        let value = parse_body_value(response.body);
        serde_json::from_value(value).map_err(Into::into)
    }

    pub async fn get_global_command(
        &self,
        command_id: impl Into<Snowflake>,
    ) -> Result<ApplicationCommand, DiscordError> {
        let path = global_commands_path(self.application_id())?;
        self.request_typed(
            Method::GET,
            &format!("{}/{}", path, command_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn edit_global_command(
        &self,
        command_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<ApplicationCommand, DiscordError> {
        let path = global_commands_path(self.application_id())?;
        self.request_typed(
            Method::PATCH,
            &format!("{}/{}", path, command_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn delete_global_command(
        &self,
        command_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        let path = global_commands_path(self.application_id())?;
        self.request_no_content(
            Method::DELETE,
            &format!("{}/{}", path, command_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_guild_commands(
        &self,
        guild_id: impl Into<Snowflake>,
    ) -> Result<Vec<ApplicationCommand>, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request_typed(
            Method::GET,
            &format!(
                "/applications/{application_id}/guilds/{}/commands",
                guild_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn create_guild_command(
        &self,
        guild_id: impl Into<Snowflake>,
        command: &CommandDefinition,
    ) -> Result<ApplicationCommand, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request_typed(
            Method::POST,
            &format!(
                "/applications/{application_id}/guilds/{}/commands",
                guild_id.into()
            ),
            Some(command),
        )
        .await
    }

    pub async fn get_guild_command(
        &self,
        guild_id: impl Into<Snowflake>,
        command_id: impl Into<Snowflake>,
    ) -> Result<ApplicationCommand, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request_typed(
            Method::GET,
            &format!(
                "/applications/{application_id}/guilds/{}/commands/{}",
                guild_id.into(),
                command_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn edit_guild_command(
        &self,
        guild_id: impl Into<Snowflake>,
        command_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<ApplicationCommand, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request_typed(
            Method::PATCH,
            &format!(
                "/applications/{application_id}/guilds/{}/commands/{}",
                guild_id.into(),
                command_id.into()
            ),
            Some(body),
        )
        .await
    }

    pub async fn delete_guild_command(
        &self,
        guild_id: impl Into<Snowflake>,
        command_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request_no_content(
            Method::DELETE,
            &format!(
                "/applications/{application_id}/guilds/{}/commands/{}",
                guild_id.into(),
                command_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_guild_preview(
        &self,
        guild_id: impl Into<Snowflake>,
    ) -> Result<serde_json::Value, DiscordError> {
        self.request(
            Method::GET,
            &format!("/guilds/{}/preview", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_guild_preview_typed(
        &self,
        guild_id: impl Into<Snowflake>,
    ) -> Result<GuildPreview, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/preview", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_guild_prune_count(
        &self,
        guild_id: impl Into<Snowflake>,
        days: Option<u64>,
        include_roles: &[Snowflake],
    ) -> Result<serde_json::Value, DiscordError> {
        let query = guild_prune_query(days, None, include_roles);
        self.request(
            Method::GET,
            &format!("/guilds/{}/prune{query}", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_guild_prune_count_typed(
        &self,
        guild_id: impl Into<Snowflake>,
        days: Option<u64>,
        include_roles: &[Snowflake],
    ) -> Result<GuildPruneCount, DiscordError> {
        let query = guild_prune_query(days, None, include_roles);
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/prune{query}", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn begin_guild_prune(
        &self,
        guild_id: impl Into<Snowflake>,
        days: Option<u64>,
        compute_prune_count: Option<bool>,
        include_roles: &[Snowflake],
    ) -> Result<serde_json::Value, DiscordError> {
        let query = guild_prune_query(days, compute_prune_count, include_roles);
        self.request(
            Method::POST,
            &format!("/guilds/{}/prune{query}", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn begin_guild_prune_typed(
        &self,
        guild_id: impl Into<Snowflake>,
        days: Option<u64>,
        compute_prune_count: Option<bool>,
        include_roles: &[Snowflake],
    ) -> Result<GuildPruneResult, DiscordError> {
        let query = guild_prune_query(days, compute_prune_count, include_roles);
        self.request_typed(
            Method::POST,
            &format!("/guilds/{}/prune{query}", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn begin_guild_prune_with_body<B>(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<GuildPruneResult, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(
            Method::POST,
            &format!("/guilds/{}/prune", guild_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn begin_guild_prune_with_request(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &BeginGuildPruneRequest,
    ) -> Result<GuildPruneResult, DiscordError> {
        self.begin_guild_prune_with_body(guild_id, body).await
    }

    pub async fn get_guild_vanity_url(
        &self,
        guild_id: impl Into<Snowflake>,
    ) -> Result<VanityUrl, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/vanity-url", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_guild_widget_settings(
        &self,
        guild_id: impl Into<Snowflake>,
    ) -> Result<GuildWidgetSettings, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/widget", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn modify_guild_widget_settings(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<GuildWidgetSettings, DiscordError> {
        self.modify_guild_widget_settings_typed(guild_id, body)
            .await
    }

    pub async fn modify_guild_widget_settings_typed<B>(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<GuildWidgetSettings, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(
            Method::PATCH,
            &format!("/guilds/{}/widget", guild_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn modify_guild_widget(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &ModifyGuildWidgetSettings,
    ) -> Result<GuildWidgetSettings, DiscordError> {
        self.modify_guild_widget_settings_typed(guild_id, body)
            .await
    }

    pub async fn get_guild_widget(
        &self,
        guild_id: impl Into<Snowflake>,
    ) -> Result<serde_json::Value, DiscordError> {
        self.request(
            Method::GET,
            &format!("/guilds/{}/widget.json", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_guild_widget_typed(
        &self,
        guild_id: impl Into<Snowflake>,
    ) -> Result<GuildWidget, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/widget.json", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Return the public PNG widget image for a guild.
    ///
    /// Discord documents this route as unauthenticated, so this helper omits
    /// the bot authorization header even when the client has a bot token.
    pub async fn get_guild_widget_image(
        &self,
        guild_id: impl Into<Snowflake>,
        style: Option<GuildWidgetImageStyle>,
    ) -> Result<Vec<u8>, DiscordError> {
        let query = style
            .map(|style| format!("?style={}", style.as_str()))
            .unwrap_or_default();
        self.request_bytes_with_authorization(
            RequestAuthorization::None,
            Method::GET,
            &format!("/guilds/{}/widget.png{query}", guild_id.into()),
        )
        .await
    }

    pub async fn follow_announcement_channel(
        &self,
        channel_id: impl Into<Snowflake>,
        webhook_channel_id: impl Into<Snowflake>,
    ) -> Result<FollowedChannel, DiscordError> {
        self.request_typed(
            Method::POST,
            &format!("/channels/{}/followers", channel_id.into()),
            Some(&serde_json::json!({ "webhook_channel_id": webhook_channel_id.into() })),
        )
        .await
    }

    pub async fn create_stage_instance(&self, body: &Value) -> Result<StageInstance, DiscordError> {
        self.create_stage_instance_typed(body).await
    }

    pub async fn create_stage_instance_typed<B>(
        &self,
        body: &B,
    ) -> Result<StageInstance, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(Method::POST, "/stage-instances", Some(body))
            .await
    }

    pub async fn create_stage_instance_from_request(
        &self,
        body: &CreateStageInstance,
    ) -> Result<StageInstance, DiscordError> {
        self.create_stage_instance_typed(body).await
    }

    pub async fn get_stage_instance(
        &self,
        channel_id: impl Into<Snowflake>,
    ) -> Result<StageInstance, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/stage-instances/{}", channel_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn modify_stage_instance(
        &self,
        channel_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<StageInstance, DiscordError> {
        self.modify_stage_instance_typed(channel_id, body).await
    }

    pub async fn modify_stage_instance_typed<B>(
        &self,
        channel_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<StageInstance, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(
            Method::PATCH,
            &format!("/stage-instances/{}", channel_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn modify_stage_instance_from_request(
        &self,
        channel_id: impl Into<Snowflake>,
        body: &ModifyStageInstance,
    ) -> Result<StageInstance, DiscordError> {
        self.modify_stage_instance_typed(channel_id, body).await
    }

    pub async fn delete_stage_instance(
        &self,
        channel_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!("/stage-instances/{}", channel_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_guild_welcome_screen(
        &self,
        guild_id: impl Into<Snowflake>,
    ) -> Result<WelcomeScreen, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/welcome-screen", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn modify_guild_welcome_screen(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<WelcomeScreen, DiscordError> {
        self.modify_guild_welcome_screen_typed(guild_id, body).await
    }

    pub async fn modify_guild_welcome_screen_typed<B>(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<WelcomeScreen, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(
            Method::PATCH,
            &format!("/guilds/{}/welcome-screen", guild_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn modify_guild_welcome_screen_config(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &ModifyGuildWelcomeScreen,
    ) -> Result<WelcomeScreen, DiscordError> {
        self.modify_guild_welcome_screen_typed(guild_id, body).await
    }

    pub async fn get_guild_onboarding(
        &self,
        guild_id: impl Into<Snowflake>,
    ) -> Result<GuildOnboarding, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/onboarding", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn modify_guild_onboarding(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<GuildOnboarding, DiscordError> {
        self.modify_guild_onboarding_typed(guild_id, body).await
    }

    pub async fn modify_guild_onboarding_typed<B>(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<GuildOnboarding, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(
            Method::PUT,
            &format!("/guilds/{}/onboarding", guild_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn modify_guild_onboarding_config(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &ModifyGuildOnboarding,
    ) -> Result<GuildOnboarding, DiscordError> {
        self.modify_guild_onboarding_typed(guild_id, body).await
    }

    pub async fn modify_guild_incident_actions(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &ModifyGuildIncidentActions,
    ) -> Result<GuildIncidentsData, DiscordError> {
        self.request_typed(
            Method::PUT,
            &format!("/guilds/{}/incident-actions", guild_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn get_guild_templates(
        &self,
        guild_id: impl Into<Snowflake>,
    ) -> Result<Vec<GuildTemplate>, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/templates", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Fetches the `get_guild_template` resource by template code.
    pub async fn get_guild_template_by_code(
        &self,
        template_code: &str,
    ) -> Result<GuildTemplate, DiscordError> {
        validate_token_path_segment("template_code", template_code, false)?;
        self.request_typed(
            Method::GET,
            &format!("/guilds/templates/{template_code}"),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn create_guild_template(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<GuildTemplate, DiscordError> {
        self.request_typed(
            Method::POST,
            &format!("/guilds/{}/templates", guild_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn sync_guild_template(
        &self,
        guild_id: impl Into<Snowflake>,
        template_code: &str,
    ) -> Result<GuildTemplate, DiscordError> {
        validate_token_path_segment("template_code", template_code, false)?;
        self.request_typed(
            Method::PUT,
            &format!("/guilds/{}/templates/{template_code}", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn modify_guild_template(
        &self,
        guild_id: impl Into<Snowflake>,
        template_code: &str,
        body: &Value,
    ) -> Result<GuildTemplate, DiscordError> {
        validate_token_path_segment("template_code", template_code, false)?;
        self.request_typed(
            Method::PATCH,
            &format!("/guilds/{}/templates/{template_code}", guild_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn delete_guild_template(
        &self,
        guild_id: impl Into<Snowflake>,
        template_code: &str,
    ) -> Result<GuildTemplate, DiscordError> {
        validate_token_path_segment("template_code", template_code, false)?;
        self.request_typed(
            Method::DELETE,
            &format!("/guilds/{}/templates/{template_code}", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_voice_regions(&self) -> Result<Vec<serde_json::Value>, DiscordError> {
        self.request_typed(Method::GET, "/voice/regions", Option::<&Value>::None)
            .await
    }

    pub async fn get_voice_regions_typed(&self) -> Result<Vec<VoiceRegion>, DiscordError> {
        self.request_typed(Method::GET, "/voice/regions", Option::<&Value>::None)
            .await
    }

    pub async fn get_guild_voice_regions(
        &self,
        guild_id: impl Into<Snowflake>,
    ) -> Result<Vec<VoiceRegion>, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/regions", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_current_user_voice_state(
        &self,
        guild_id: impl Into<Snowflake>,
    ) -> Result<VoiceState, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/voice-states/@me", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_user_voice_state(
        &self,
        guild_id: impl Into<Snowflake>,
        user_id: impl Into<Snowflake>,
    ) -> Result<VoiceState, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!(
                "/guilds/{}/voice-states/{}",
                guild_id.into(),
                user_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn modify_current_user_voice_state<B>(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<(), DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_no_content(
            Method::PATCH,
            &format!("/guilds/{}/voice-states/@me", guild_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn modify_current_user_voice_state_from_request(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &ModifyCurrentUserVoiceState,
    ) -> Result<(), DiscordError> {
        self.modify_current_user_voice_state(guild_id, body).await
    }

    pub async fn modify_user_voice_state<B>(
        &self,
        guild_id: impl Into<Snowflake>,
        user_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<(), DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_no_content(
            Method::PATCH,
            &format!(
                "/guilds/{}/voice-states/{}",
                guild_id.into(),
                user_id.into()
            ),
            Some(body),
        )
        .await
    }

    pub async fn modify_user_voice_state_from_request(
        &self,
        guild_id: impl Into<Snowflake>,
        user_id: impl Into<Snowflake>,
        body: &ModifyUserVoiceState,
    ) -> Result<(), DiscordError> {
        self.modify_user_voice_state(guild_id, user_id, body).await
    }

    pub(crate) async fn create_interaction_response_json(
        &self,
        interaction_id: impl Into<Snowflake>,
        interaction_token: &str,
        body: &Value,
    ) -> Result<(), DiscordError> {
        let path = interaction_callback_path(interaction_id.into(), interaction_token)?;
        self.request_no_content(Method::POST, &path, Some(body))
            .await
    }

    pub(crate) async fn create_followup_message_json(
        &self,
        interaction_token: &str,
        body: &Value,
    ) -> Result<Value, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.create_followup_message_json_with_application_id(
            &application_id,
            interaction_token,
            body,
        )
        .await
    }

    pub(crate) async fn create_followup_message_json_with_application_id(
        &self,
        application_id: &str,
        interaction_token: &str,
        body: &Value,
    ) -> Result<Value, DiscordError> {
        let path = followup_webhook_path(application_id, interaction_token, None)?;
        self.request(Method::POST, &path, Some(body)).await
    }

    pub async fn create_followup_message(
        &self,
        interaction_token: &str,
        body: &CreateMessage,
    ) -> Result<Message, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.create_followup_message_with_application_id(&application_id, interaction_token, body)
            .await
    }

    pub async fn create_followup_message_with_files(
        &self,
        interaction_token: &str,
        body: &CreateMessage,
        files: &[FileAttachment],
    ) -> Result<Message, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.create_followup_message_with_application_id_and_files(
            &application_id,
            interaction_token,
            body,
            files,
        )
        .await
    }

    pub async fn create_followup_message_with_application_id(
        &self,
        application_id: &str,
        interaction_token: &str,
        body: &CreateMessage,
    ) -> Result<Message, DiscordError> {
        let path = followup_webhook_path(application_id, interaction_token, None)?;
        self.request_typed(Method::POST, &path, Some(body)).await
    }

    pub async fn create_followup_message_with_application_id_and_files(
        &self,
        application_id: &str,
        interaction_token: &str,
        body: &CreateMessage,
        files: &[FileAttachment],
    ) -> Result<Message, DiscordError> {
        let path = followup_webhook_path(application_id, interaction_token, None)?;
        self.request_typed_multipart(Method::POST, &path, body, files)
            .await
    }

    pub async fn get_original_interaction_response(
        &self,
        interaction_token: &str,
    ) -> Result<Message, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.get_original_interaction_response_with_application_id(
            &application_id,
            interaction_token,
        )
        .await
    }

    pub async fn get_original_interaction_response_with_application_id(
        &self,
        application_id: &str,
        interaction_token: &str,
    ) -> Result<Message, DiscordError> {
        let path = followup_webhook_path(application_id, interaction_token, Some("@original"))?;
        self.request_typed(Method::GET, &path, Option::<&Value>::None)
            .await
    }

    pub async fn edit_original_interaction_response(
        &self,
        interaction_token: &str,
        body: &CreateMessage,
    ) -> Result<Message, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.edit_original_interaction_response_with_application_id(
            &application_id,
            interaction_token,
            body,
        )
        .await
    }

    pub async fn edit_original_interaction_response_with_files(
        &self,
        interaction_token: &str,
        body: &CreateMessage,
        files: &[FileAttachment],
    ) -> Result<Message, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.edit_original_interaction_response_with_application_id_and_files(
            &application_id,
            interaction_token,
            body,
            files,
        )
        .await
    }

    pub async fn edit_original_interaction_response_with_application_id(
        &self,
        application_id: &str,
        interaction_token: &str,
        body: &CreateMessage,
    ) -> Result<Message, DiscordError> {
        let path = followup_webhook_path(application_id, interaction_token, Some("@original"))?;
        self.request_typed(Method::PATCH, &path, Some(body)).await
    }

    pub async fn edit_original_interaction_response_with_application_id_and_files(
        &self,
        application_id: &str,
        interaction_token: &str,
        body: &CreateMessage,
        files: &[FileAttachment],
    ) -> Result<Message, DiscordError> {
        let path = followup_webhook_path(application_id, interaction_token, Some("@original"))?;
        self.request_typed_multipart(Method::PATCH, &path, body, files)
            .await
    }

    pub async fn delete_original_interaction_response(
        &self,
        interaction_token: &str,
    ) -> Result<(), DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.delete_original_interaction_response_with_application_id(
            &application_id,
            interaction_token,
        )
        .await
    }

    pub async fn delete_original_interaction_response_with_application_id(
        &self,
        application_id: &str,
        interaction_token: &str,
    ) -> Result<(), DiscordError> {
        let path = followup_webhook_path(application_id, interaction_token, Some("@original"))?;
        self.request_no_content(Method::DELETE, &path, Option::<&Value>::None)
            .await
    }

    /// Returns command permissions for every application command in one guild.
    pub async fn get_guild_application_command_permissions(
        &self,
        guild_id: impl Into<Snowflake>,
    ) -> Result<Vec<GuildApplicationCommandPermissions>, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request_typed(
            Method::GET,
            &format!(
                "/applications/{application_id}/guilds/{}/commands/permissions",
                guild_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }

    /// Returns command permissions for one application command in one guild.
    pub async fn get_application_command_permissions(
        &self,
        guild_id: impl Into<Snowflake>,
        command_id: impl Into<Snowflake>,
    ) -> Result<GuildApplicationCommandPermissions, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request_typed(
            Method::GET,
            &format!(
                "/applications/{application_id}/guilds/{}/commands/{}/permissions",
                guild_id.into(),
                command_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }

    /// Replaces all application command permissions in a guild using the legacy batch route.
    ///
    /// Discord marks this endpoint as disabled after Permissions v2, but it remains
    /// present in the official route table. Prefer [`Self::edit_application_command_permissions`]
    /// for active integrations.
    pub async fn batch_edit_application_command_permissions<B>(
        &self,
        bearer_token: &str,
        guild_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<Vec<GuildApplicationCommandPermissions>, DiscordError>
    where
        B: Serialize + ?Sized,
    {
        let bearer_token = validate_authorization_token("bearer_token", bearer_token)?;
        let application_id = configured_application_id(self.application_id())?;
        self.request_typed_with_authorization(
            RequestAuthorization::Bearer(bearer_token),
            Method::PUT,
            &format!(
                "/applications/{application_id}/guilds/{}/commands/permissions",
                guild_id.into()
            ),
            Some(body),
        )
        .await
    }

    /// Replaces command permissions using Discord's OAuth2 Bearer-token flow.
    ///
    /// Discord requires an access token with the
    /// `applications.commands.permissions.update` scope for this route. Bot
    /// tokens are not accepted by Discord for this write operation.
    pub async fn edit_application_command_permissions(
        &self,
        bearer_token: &str,
        guild_id: impl Into<Snowflake>,
        command_id: impl Into<Snowflake>,
        body: &EditApplicationCommandPermissions,
    ) -> Result<GuildApplicationCommandPermissions, DiscordError> {
        let bearer_token = validate_authorization_token("bearer_token", bearer_token)?;
        let application_id = configured_application_id(self.application_id())?;
        self.request_typed_with_authorization(
            RequestAuthorization::Bearer(bearer_token),
            Method::PUT,
            &format!(
                "/applications/{application_id}/guilds/{}/commands/{}/permissions",
                guild_id.into(),
                command_id.into()
            ),
            Some(body),
        )
        .await
    }

    pub async fn edit_followup_message(
        &self,
        interaction_token: &str,
        message_id: &str,
        body: &CreateMessage,
    ) -> Result<Message, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.edit_followup_message_with_application_id(
            &application_id,
            interaction_token,
            message_id,
            body,
        )
        .await
    }

    pub async fn edit_followup_message_with_files(
        &self,
        interaction_token: &str,
        message_id: &str,
        body: &CreateMessage,
        files: &[FileAttachment],
    ) -> Result<Message, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.edit_followup_message_with_application_id_and_files(
            &application_id,
            interaction_token,
            message_id,
            body,
            files,
        )
        .await
    }

    pub async fn edit_followup_message_with_application_id(
        &self,
        application_id: &str,
        interaction_token: &str,
        message_id: &str,
        body: &CreateMessage,
    ) -> Result<Message, DiscordError> {
        let path = followup_webhook_path(application_id, interaction_token, Some(message_id))?;
        self.request_typed(Method::PATCH, &path, Some(body)).await
    }

    pub async fn edit_followup_message_with_application_id_and_files(
        &self,
        application_id: &str,
        interaction_token: &str,
        message_id: &str,
        body: &CreateMessage,
        files: &[FileAttachment],
    ) -> Result<Message, DiscordError> {
        let path = followup_webhook_path(application_id, interaction_token, Some(message_id))?;
        self.request_typed_multipart(Method::PATCH, &path, body, files)
            .await
    }

    pub async fn delete_followup_message(
        &self,
        interaction_token: &str,
        message_id: &str,
    ) -> Result<(), DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.delete_followup_message_with_application_id(
            &application_id,
            interaction_token,
            message_id,
        )
        .await
    }

    pub async fn delete_followup_message_with_application_id(
        &self,
        application_id: &str,
        interaction_token: &str,
        message_id: &str,
    ) -> Result<(), DiscordError> {
        let path = followup_webhook_path(application_id, interaction_token, Some(message_id))?;
        self.request_no_content(Method::DELETE, &path, Option::<&Value>::None)
            .await
    }

    pub async fn request(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Value, DiscordError> {
        let response = self
            .request_with_headers(
                method,
                path,
                body.map(clone_json_body).map(RequestBody::Json),
            )
            .await?;
        Ok(parse_body_value(response.body))
    }

    pub async fn request_multipart<B>(
        &self,
        method: Method,
        path: &str,
        body: &B,
        files: &[FileAttachment],
    ) -> Result<Value, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        let response = self
            .request_with_headers(method, path, Some(multipart_body(body, files)?))
            .await?;
        Ok(parse_body_value(response.body))
    }

    async fn request_bytes_with_authorization(
        &self,
        authorization: RequestAuthorization<'_>,
        method: Method,
        path: &str,
    ) -> Result<Vec<u8>, DiscordError> {
        let response = self
            .request_bytes_with_headers_authorized(authorization, method, path, None)
            .await?;
        Ok(response.body)
    }

    pub async fn request_typed<T, B>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<T, DiscordError>
    where
        T: DeserializeOwned,
        B: serde::Serialize + ?Sized,
    {
        let body = body.map(serialize_body).transpose()?.map(RequestBody::Json);
        let response = self.request_with_headers(method, path, body).await?;
        let value = parse_body_value(response.body);
        serde_json::from_value(value).map_err(Into::into)
    }

    async fn request_optional_typed_no_content<T, B>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<Option<T>, DiscordError>
    where
        T: DeserializeOwned,
        B: serde::Serialize + ?Sized,
    {
        let body = body.map(serialize_body).transpose()?.map(RequestBody::Json);
        let response = self.request_with_headers(method, path, body).await?;
        if response.status == StatusCode::NO_CONTENT {
            return Ok(None);
        }

        let value = parse_body_value(response.body);
        serde_json::from_value(value).map(Some).map_err(Into::into)
    }

    async fn request_typed_with_authorization<T, B>(
        &self,
        authorization: RequestAuthorization<'_>,
        method: Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<T, DiscordError>
    where
        T: DeserializeOwned,
        B: serde::Serialize + ?Sized,
    {
        let body = body.map(serialize_body).transpose()?.map(RequestBody::Json);
        let response = self
            .request_with_headers_authorized(authorization, method, path, body)
            .await?;
        let value = parse_body_value(response.body);
        serde_json::from_value(value).map_err(Into::into)
    }

    pub async fn request_typed_multipart<T, B>(
        &self,
        method: Method,
        path: &str,
        body: &B,
        files: &[FileAttachment],
    ) -> Result<T, DiscordError>
    where
        T: DeserializeOwned,
        B: serde::Serialize + ?Sized,
    {
        let response = self
            .request_with_headers(method, path, Some(multipart_body(body, files)?))
            .await?;
        let value = parse_body_value(response.body);
        serde_json::from_value(value).map_err(Into::into)
    }

    async fn request_no_content<B>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<(), DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        let body = body.map(serialize_body).transpose()?.map(RequestBody::Json);
        self.request_with_headers(method, path, body).await?;
        Ok(())
    }

    async fn request_multipart_no_content<B>(
        &self,
        method: Method,
        path: &str,
        body: &B,
        files: &[FileAttachment],
    ) -> Result<(), DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_with_headers(method, path, Some(multipart_body(body, files)?))
            .await?;
        Ok(())
    }

    async fn request_with_headers(
        &self,
        method: Method,
        path: &str,
        body: Option<RequestBody>,
    ) -> Result<RawResponse, DiscordError> {
        self.request_with_headers_authorized(RequestAuthorization::Auto, method, path, body)
            .await
    }

    async fn request_with_headers_authorized(
        &self,
        authorization: RequestAuthorization<'_>,
        method: Method,
        path: &str,
        body: Option<RequestBody>,
    ) -> Result<RawResponse, DiscordError> {
        let response = self
            .request_bytes_with_headers_authorized(authorization, method, path, body)
            .await?;
        Ok(RawResponse {
            status: response.status,
            headers: response.headers,
            body: String::from_utf8_lossy(&response.body).into_owned(),
        })
    }

    /// Single transport entry point: serializes same-bucket requests behind
    /// a per-bucket gate, waits out known rate-limit windows, and retries
    /// 429s plus — when [`ReplayPolicy`] allows it — 5xx responses and
    /// transient transport failures.
    async fn request_bytes_with_headers_authorized(
        &self,
        authorization: RequestAuthorization<'_>,
        method: Method,
        path: &str,
        body: Option<RequestBody>,
    ) -> Result<RawBytesResponse, DiscordError> {
        let route = RequestRoute::new(&method, path);
        let route_key = route.key.as_str();
        let token = route_token(path);
        let replay =
            ReplayPolicy::for_request(&method, &route, body.as_ref(), self.retry_non_idempotent);
        let (mut gate_key, mut _gate_permit) = self.acquire_route_gate(&route).await;
        let mut rate_limit_retries = 0;
        let mut server_error_retries = 0;
        // When the first attempt was sent, and the error of an earlier
        // attempt that Discord may already have applied. Every later send is
        // a replay of that attempt and must stay within `replay`.
        let mut first_sent_at: Option<Instant> = None;
        let mut possibly_applied: Option<DiscordError> = None;

        loop {
            // A bucket learned since the gate was taken — from this request's
            // own response or another request's — moves serialization to the
            // bucket's gate, so requests that queued on the route gate before
            // the bucket was known cannot run beside requests on the bucket.
            if self.rate_limits.block_key_for(&route) != gate_key {
                drop(_gate_permit);
                (gate_key, _gate_permit) = self.acquire_route_gate(&route).await;
            }

            while let Some(wait_duration) = self.rate_limits.wait_duration(&route) {
                if let Some(error) = &possibly_applied {
                    if !replay.allows_replay(first_sent_at, wait_duration) {
                        return Err(error.clone());
                    }
                }
                debug!("waiting for rate limit on {route_key} for {wait_duration:?}");
                sleep_for_retry_after(wait_duration.as_secs_f64()).await;
            }
            if let Some(error) = &possibly_applied {
                if !replay.allows_replay(first_sent_at, Duration::ZERO) {
                    return Err(error.clone());
                }
            }

            first_sent_at.get_or_insert_with(Instant::now);
            let response = match self
                .request_once_bytes(authorization, method.clone(), path, body.as_ref())
                .await
            {
                Ok(response) => response,
                Err(error) => {
                    let error = scrub_error_token(error, token);
                    let backoff = server_error_backoff(server_error_retries + 1);
                    // A connection that was never established cannot have
                    // delivered the request; a timeout or a lost response may
                    // follow a request Discord already applied.
                    let unsent = self.connect_failure_is_unsent(&error);
                    let retry = error.is_retryable_transport()
                        && server_error_retries < MAX_SERVER_ERROR_RETRIES
                        && (unsent
                            || replay
                                .allows_replay(first_sent_at, Duration::from_secs_f64(backoff)));
                    if !retry {
                        return Err(error);
                    }
                    server_error_retries += 1;
                    warn!(
                        "transport error on {route_key}: {error}, retrying ({server_error_retries}/{MAX_SERVER_ERROR_RETRIES})"
                    );
                    if !unsent {
                        possibly_applied.get_or_insert(error);
                    }
                    sleep_for_retry_after(backoff).await;
                    continue;
                }
            };
            let response_text = String::from_utf8_lossy(&response.body);
            self.rate_limits
                .observe(&route, &response.headers, response.status, &response_text);

            if response.status == StatusCode::TOO_MANY_REQUESTS {
                let retry_after =
                    rate_limit::retry_after_seconds(&response.headers, &response_text);
                if let Some(callback) = &self.rate_limit_callback {
                    callback(RateLimitInfo {
                        route: route_key.to_string(),
                        retry_after,
                        global: rate_limit::is_global_rate_limit(&response.headers, &response_text),
                    });
                }

                if rate_limit_retries >= MAX_RATE_LIMIT_RETRIES {
                    return Err(discord_rate_limit_error(route_key, &response_text));
                }
                // Discord did not process this attempt, but an earlier
                // ambiguous one may have been applied; waiting out the 429
                // must not carry the replay past what `replay` allows.
                if let Some(error) = &possibly_applied {
                    let wait = rate_limit::wait_duration_from_seconds(retry_after)
                        .unwrap_or(Duration::ZERO);
                    if !replay.allows_replay(first_sent_at, wait) {
                        return Err(error.clone());
                    }
                }

                rate_limit_retries += 1;
                warn!(
                    "received rate limit for {route_key}, retrying ({rate_limit_retries}/{MAX_RATE_LIMIT_RETRIES})"
                );
                sleep_for_retry_after(retry_after).await;
                continue;
            }

            if response.status.is_server_error() {
                let error = discord_api_error(response.status, &scrub_token(&response_text, token));
                let backoff = server_error_backoff(server_error_retries + 1);
                // A 5xx does not prove the request was rejected; Discord may
                // have applied it before failing.
                if server_error_retries >= MAX_SERVER_ERROR_RETRIES
                    || !replay.allows_replay(first_sent_at, Duration::from_secs_f64(backoff))
                {
                    return Err(error);
                }
                server_error_retries += 1;
                warn!(
                    "received {} for {route_key}, retrying ({server_error_retries}/{MAX_SERVER_ERROR_RETRIES})",
                    response.status
                );
                possibly_applied.get_or_insert(error);
                sleep_for_retry_after(backoff).await;
                continue;
            }

            if !response.status.is_success() {
                return Err(discord_api_error(
                    response.status,
                    &scrub_token(&response_text, token),
                ));
            }

            return Ok(response);
        }
    }

    /// Locks the serialization gate for `route`'s current block key.
    ///
    /// The key is re-checked after the lock is acquired: if Discord reported
    /// the route's bucket while this request was queued, the request moves to
    /// the bucket's gate instead of running beside requests already on it.
    async fn acquire_route_gate(
        &self,
        route: &RequestRoute,
    ) -> (String, tokio::sync::OwnedMutexGuard<()>) {
        loop {
            let key = self.rate_limits.block_key_for(route);
            let permit = self.route_gates.gate_for(&key).lock_owned().await;
            if self.rate_limits.block_key_for(route) == key {
                return (key, permit);
            }
        }
    }

    /// Returns true when `error` proves the request never reached Discord.
    ///
    /// A connect failure only proves that when the transport does not follow
    /// redirects: with redirects, the original request may have succeeded
    /// and only the connection to the redirect target failed. The managed
    /// clients built by this crate disable redirects; a client supplied
    /// through [`RestClientBuilder::use_client`] carries no such guarantee.
    fn connect_failure_is_unsent(&self, error: &DiscordError) -> bool {
        self.managed_transport && is_connect_failure(error)
    }

    async fn request_once_bytes(
        &self,
        authorization: RequestAuthorization<'_>,
        method: Method,
        path: &str,
        body: Option<&RequestBody>,
    ) -> Result<RawBytesResponse, DiscordError> {
        let normalized_path = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{path}")
        };
        let url = format!("{}{}", self.api_base(), normalized_path);

        let mut request_builder = self
            .client
            .request(method.clone(), url)
            .header("User-Agent", &self.user_agent);

        if !matches!(
            body,
            Some(
                RequestBody::Multipart { .. }
                    | RequestBody::NamedFileMultipart { .. }
                    | RequestBody::PayloadAndNamedFileMultipart { .. }
                    | RequestBody::StickerMultipart { .. }
            )
        ) {
            request_builder = request_builder.header("Content-Type", "application/json");
        }

        if method != Method::GET {
            if let Some(reason) = &self.audit_log_reason {
                request_builder =
                    request_builder.header("X-Audit-Log-Reason", encode_audit_log_reason(reason));
            }
        }

        match authorization {
            RequestAuthorization::Auto if request_uses_bot_authorization(&normalized_path) => {
                request_builder =
                    request_builder.header("Authorization", format!("Bot {}", self.token));
            }
            RequestAuthorization::Bearer(token) => {
                request_builder =
                    request_builder.header("Authorization", format!("Bearer {token}"));
            }
            RequestAuthorization::Auto | RequestAuthorization::None => {}
        }

        if let Some(body) = body {
            request_builder = match body {
                RequestBody::Json(value) => request_builder.json(value),
                RequestBody::Multipart {
                    payload_json,
                    files,
                } => request_builder.multipart(build_multipart_form(payload_json, files)?),
                RequestBody::NamedFileMultipart { field_name, file } => {
                    request_builder.multipart(build_named_file_form(None, field_name, file)?)
                }
                RequestBody::PayloadAndNamedFileMultipart {
                    payload_json,
                    field_name,
                    file,
                } => request_builder.multipart(build_named_file_form(
                    Some(payload_json),
                    field_name,
                    file,
                )?),
                RequestBody::StickerMultipart { payload_json, file } => {
                    request_builder.multipart(build_sticker_form(payload_json, file)?)
                }
            };
        }

        let response = request_builder.send().await?;
        let status = response.status();
        let headers = response.headers().clone();
        let body = response.bytes().await?.to_vec();

        Ok(RawBytesResponse {
            status,
            headers,
            body,
        })
    }
}

impl RestClient {
    /// Fills `allowed_mentions` from the configured default when the typed
    /// message body leaves it unset. Borrow-through when nothing changes.
    pub(super) fn message_with_default_allowed_mentions<'a>(
        &self,
        body: &'a CreateMessage,
    ) -> Cow<'a, CreateMessage> {
        match (&self.default_allowed_mentions, &body.allowed_mentions) {
            (Some(default), None) => {
                let mut body = body.clone();
                body.allowed_mentions = Some(default.clone());
                Cow::Owned(body)
            }
            _ => Cow::Borrowed(body),
        }
    }

    /// Inserts the configured default `allowed_mentions` into a raw JSON
    /// object payload that does not already contain the key.
    pub(super) fn json_with_default_allowed_mentions<'a>(&self, body: &'a Value) -> Cow<'a, Value> {
        let Some(default) = &self.default_allowed_mentions else {
            return Cow::Borrowed(body);
        };
        let Some(object) = body.as_object() else {
            return Cow::Borrowed(body);
        };
        if object.contains_key("allowed_mentions") {
            return Cow::Borrowed(body);
        }
        let Ok(default_value) = serde_json::to_value(default) else {
            return Cow::Borrowed(body);
        };
        let mut object = object.clone();
        object.insert("allowed_mentions".to_string(), default_value);
        Cow::Owned(Value::Object(object))
    }

    /// Applies the default `allowed_mentions` to message-bearing interaction
    /// callbacks (`CHANNEL_MESSAGE_WITH_SOURCE`, `UPDATE_MESSAGE`) whose data
    /// object leaves it unset.
    fn interaction_response_with_default_allowed_mentions<'a>(
        &self,
        body: &'a InteractionCallbackResponse,
    ) -> Cow<'a, InteractionCallbackResponse> {
        const CHANNEL_MESSAGE_WITH_SOURCE: u8 = 4;
        const UPDATE_MESSAGE: u8 = 7;
        if self.default_allowed_mentions.is_none()
            || !matches!(body.kind, CHANNEL_MESSAGE_WITH_SOURCE | UPDATE_MESSAGE)
        {
            return Cow::Borrowed(body);
        }
        let Some(data) = body.data.as_ref() else {
            return Cow::Borrowed(body);
        };
        match self.json_with_default_allowed_mentions(data) {
            Cow::Borrowed(_) => Cow::Borrowed(body),
            Cow::Owned(data) => {
                let mut body = body.clone();
                body.data = Some(data);
                Cow::Owned(body)
            }
        }
    }

    /// Creates a guild via `POST /guilds`.
    ///
    /// Discord stopped allowing applications to create guilds on 2025-07-15
    /// and removed the route from its docs; kept only so 3.0 migrations get
    /// a deprecation warning instead of a missing method.
    #[deprecated(
        since = "3.0.0",
        note = "Discord removed guild creation and bot guild ownership on 2025-07-15; this route is no longer documented or usable by bots"
    )]
    pub async fn create_guild(
        &self,
        body: &crate::model::CreateGuild,
    ) -> Result<Guild, DiscordError> {
        self.request_typed(Method::POST, "/guilds", Some(body))
            .await
    }

    /// Deletes a guild via `DELETE /guilds/{id}`. Requires guild ownership,
    /// which bots lost when Discord transferred bot-owned guilds to users in
    /// 2025.
    #[deprecated(
        since = "3.0.0",
        note = "Discord removed guild creation and bot guild ownership on 2025-07-15; this route is no longer documented or usable by bots"
    )]
    pub async fn delete_guild(&self, guild_id: impl Into<Snowflake>) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!("/guilds/{}", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Creates a guild from a guild template via
    /// `POST /guilds/templates/{code}`, which shares the removed
    /// guild-creation capability.
    #[deprecated(
        since = "3.0.0",
        note = "Discord removed guild creation and bot guild ownership on 2025-07-15; this route is no longer documented or usable by bots"
    )]
    pub async fn create_guild_from_template(
        &self,
        template_code: &str,
        body: &crate::model::CreateGuildFromTemplate,
    ) -> Result<Guild, DiscordError> {
        let template_code = template_code.trim();
        paths::validate_template_code(template_code)?;
        self.request_typed(
            Method::POST,
            &format!("/guilds/templates/{template_code}"),
            Some(body),
        )
        .await
    }

    /// Sets the guild's required MFA level for moderation actions via
    /// `POST /guilds/{id}/mfa` and returns the updated level. Owner-only,
    /// so unusable by bots since bot-owned guilds were transferred in 2025.
    #[deprecated(
        since = "3.0.0",
        note = "Discord removed guild creation and bot guild ownership on 2025-07-15; this route is no longer documented or usable by bots"
    )]
    pub async fn modify_guild_mfa_level(
        &self,
        guild_id: impl Into<Snowflake>,
        level: u64,
    ) -> Result<crate::model::GuildMfaLevel, DiscordError> {
        self.request_typed(
            Method::POST,
            &format!("/guilds/{}/mfa", guild_id.into()),
            Some(&crate::model::GuildMfaLevel { level }),
        )
        .await
    }

    /// Sends an interaction response with `with_response=true` and returns
    /// the typed interaction callback resource, mirroring discord.js's
    /// `withResponse: true`.
    pub async fn create_interaction_response_with_result(
        &self,
        interaction_id: impl Into<Snowflake>,
        interaction_token: &str,
        response: &InteractionCallbackResponse,
    ) -> Result<crate::model::InteractionCallbackResult, DiscordError> {
        let path = interaction_callback_path(interaction_id.into(), interaction_token)?;
        self.request_typed(
            Method::POST,
            &format!("{path}?with_response=true"),
            Some(response),
        )
        .await
    }
}

/// Backoff schedule for 5xx and transport retries: 0.5s, 1s, 2s.
/// How long Discord is assumed to deduplicate an enforced message nonce.
///
/// Discord documents the check as covering "the past few minutes" without a
/// number. One minute stays well inside that wording while covering the
/// 0.5s/1s/2s server-error backoff and ordinary rate-limit waits.
const NONCE_DEDUP_WINDOW: Duration = Duration::from_secs(60);

/// Whether a request may be sent again after an attempt that Discord may
/// already have applied (a 5xx, a timeout, or a lost response).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReplayPolicy {
    /// Repeating the request cannot produce a second effect: the HTTP
    /// methods RFC 9110 defines as idempotent, or the caller opted in with
    /// [`RestClientBuilder::retry_non_idempotent_requests`]. A replayed
    /// DELETE whose first attempt succeeded surfaces Discord's 404, which is
    /// the accepted cost of retrying it.
    Always,
    /// A channel message create carrying `nonce` + `enforce_nonce: true`.
    /// Discord returns the original message for a repeated nonce, but only
    /// within its deduplication window, so the replay is bounded by
    /// [`NONCE_DEDUP_WINDOW`] measured from the first attempt.
    WithinNonceWindow,
    /// POST and PATCH in general. Discord PATCH bodies usually set absolute
    /// values, but nothing guarantees that for every route.
    Never,
}

impl ReplayPolicy {
    fn for_request(
        method: &Method,
        route: &RequestRoute,
        body: Option<&RequestBody>,
        retry_non_idempotent: bool,
    ) -> Self {
        if retry_non_idempotent
            || matches!(
                *method,
                Method::GET | Method::HEAD | Method::OPTIONS | Method::PUT | Method::DELETE
            )
        {
            Self::Always
        } else if route.operation == RouteOperation::CreateChannelMessage
            && body.is_some_and(RequestBody::enforces_nonce)
        {
            Self::WithinNonceWindow
        } else {
            Self::Never
        }
    }

    /// Returns true when a replay that starts after `upcoming_wait` is
    /// still allowed.
    fn allows_replay(self, first_sent_at: Option<Instant>, upcoming_wait: Duration) -> bool {
        match self {
            Self::Always => true,
            Self::WithinNonceWindow => first_sent_at.is_none_or(|sent| {
                sent.elapsed().saturating_add(upcoming_wait) < NONCE_DEDUP_WINDOW
            }),
            Self::Never => false,
        }
    }
}

/// Removes the route token from an error produced while sending a request.
fn scrub_error_token(error: DiscordError, token: Option<&str>) -> DiscordError {
    match (error, token) {
        (DiscordError::Http(mut http_error), Some(token)) => {
            http_error.scrub_secret(token);
            DiscordError::Http(http_error)
        }
        (error, _) => error,
    }
}

fn is_connect_failure(error: &DiscordError) -> bool {
    matches!(error, DiscordError::Http(http_error) if http_error.is_connect())
}

fn server_error_backoff(attempt: usize) -> f64 {
    0.5 * f64::from(1u32 << attempt.saturating_sub(1).min(8) as u32)
}

/// Percent-encodes an audit-log reason the way discord.js does
/// (`encodeURIComponent`): unreserved characters pass through, everything
/// else — including spaces and any non-ASCII UTF-8 byte — is `%XX`-encoded
/// so the value is always a valid HTTP header.
fn encode_audit_log_reason(reason: &str) -> String {
    paths::percent_encode_component(reason)
}

fn validate_authorization_token<'a>(name: &str, value: &'a str) -> Result<&'a str, DiscordError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(invalid_data_error(format!("{name} must not be empty")));
    }
    if value.chars().any(char::is_control) {
        return Err(invalid_data_error(format!(
            "{name} contains characters that are unsafe in an Authorization header"
        )));
    }
    Ok(value)
}

struct RawResponse {
    status: StatusCode,
    #[allow(dead_code)]
    headers: HeaderMap,
    body: String,
}

struct RawBytesResponse {
    status: StatusCode,
    headers: HeaderMap,
    body: Vec<u8>,
}

fn discord_api_error(status: StatusCode, body: &str) -> DiscordError {
    let payload = parse_body_value(body.to_string());
    let code = payload.get("code").and_then(Value::as_u64);
    let message = payload
        .get("message")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| payload.as_str().map(str::to_string))
        .unwrap_or_else(|| payload.to_string());
    DiscordError::api(status.as_u16(), code, message)
}

fn discord_rate_limit_error(route: &str, body: &str) -> DiscordError {
    let payload = parse_body_value(body.to_string());
    let retry_after = payload
        .get("retry_after")
        .and_then(Value::as_f64)
        .unwrap_or(1.0);
    DiscordError::rate_limit(route.to_string(), retry_after)
}

async fn sleep_for_retry_after(retry_after_seconds: f64) {
    let duration = rate_limit::wait_duration_from_seconds(retry_after_seconds)
        .unwrap_or(Duration::from_secs(1));
    tokio::time::sleep(duration).await;
}
