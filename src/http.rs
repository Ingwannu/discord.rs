use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use reqwest::{header::HeaderMap, Client, Method, StatusCode};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use tracing::{debug, warn};

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
    ActivityInstance, AddGroupDmRecipient, AddGuildMember, Application, ApplicationCommand,
    ApplicationRoleConnectionMetadata, AuditLog, AuditLogQuery, Ban, BeginGuildPruneRequest,
    BulkGuildBanRequest, BulkGuildBanResponse, Channel, CreateChannelInvite, CreateDmChannel,
    CreateGroupDmChannel, CreateGuildChannel, CreateGuildRole, CreateMessage, CreateStageInstance,
    EditApplicationCommandPermissions, EditChannelPermission, FollowedChannel, Gateway, GatewayBot,
    GetGuildQuery, Guild, GuildApplicationCommandPermissions, GuildBansQuery, GuildIncidentsData,
    GuildMembersQuery, GuildOnboarding, GuildPreview, GuildPruneCount, GuildPruneResult,
    GuildTemplate, GuildWidget, GuildWidgetImageStyle, GuildWidgetSettings, Integration,
    InteractionCallbackResponse, Invite, InviteTargetUsersJobStatus, Member, Message,
    ModifyCurrentApplication, ModifyCurrentUserVoiceState, ModifyGuild, ModifyGuildChannelPosition,
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
use paths::{
    archived_threads_query, audit_log_query, bool_query, configured_application_id,
    entitlement_query, followup_webhook_path, get_guild_query, global_commands_path,
    guild_bans_query, guild_members_query, guild_prune_query, interaction_callback_path,
    invite_query, joined_archived_threads_query, poll_answer_voters_query, rate_limit_route_key,
    request_uses_bot_authorization, search_guild_members_query, subscription_query,
    thread_member_query, validate_token_path_segment,
};
use rate_limit::RateLimitState;
#[cfg(test)]
use rate_limit::RATE_LIMIT_BUCKET_RETENTION;

const API_BASE: &str = "https://discord.com/api/v10";
const MAX_RATE_LIMIT_RETRIES: usize = 5;

#[derive(Clone, Copy)]
enum RequestAuthorization<'a> {
    Auto,
    Bearer(&'a str),
    None,
}

/// Typed Discord API object for `RestClient`.
pub struct RestClient {
    client: Client,
    token: String,
    application_id: AtomicU64,
    rate_limits: Arc<RateLimitState>,
    route_gates: Arc<RateLimitRouteGates>,
    #[cfg(test)]
    base_url: String,
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
            application_id: AtomicU64::new(application_id),
            rate_limits: Arc::new(RateLimitState::default()),
            route_gates: Arc::new(RateLimitRouteGates::default()),
            #[cfg(test)]
            base_url: API_BASE.to_string(),
        }
    }

    #[cfg(test)]
    fn new_with_base_url(
        token: impl Into<String>,
        application_id: u64,
        base_url: impl Into<String>,
    ) -> Self {
        Self {
            client: client_config::default_http_client(),
            token: token.into(),
            application_id: AtomicU64::new(application_id),
            rate_limits: Arc::new(RateLimitState::default()),
            route_gates: Arc::new(RateLimitRouteGates::default()),
            base_url: base_url.into(),
        }
    }

    #[cfg(test)]
    fn api_base(&self) -> &str {
        &self.base_url
    }

    #[cfg(not(test))]
    fn api_base(&self) -> &str {
        API_BASE
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
        self.request_no_content(Method::POST, &path, Some(body))
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
        let route_key = rate_limit_route_key(&method, path);
        let mut rate_limit_retries = 0;

        loop {
            while let Some(wait_duration) = self.rate_limits.wait_duration(&route_key) {
                debug!(
                    "waiting for rate limit on {route_key} for {:?}",
                    wait_duration
                );
                sleep_for_retry_after(wait_duration.as_secs_f64()).await;
            }

            let response = self
                .request_once(authorization, method.clone(), path, body.as_ref())
                .await?;
            self.rate_limits.observe(
                &route_key,
                &response.headers,
                response.status,
                &response.body,
            );

            if response.status == StatusCode::TOO_MANY_REQUESTS {
                if rate_limit_retries >= MAX_RATE_LIMIT_RETRIES {
                    return Err(discord_rate_limit_error(&route_key, &response.body));
                }

                rate_limit_retries += 1;
                warn!(
                    "received rate limit for {route_key}, retrying ({rate_limit_retries}/{MAX_RATE_LIMIT_RETRIES})"
                );
                let payload = parse_body_value(response.body.clone());
                let retry_after = payload
                    .get("retry_after")
                    .and_then(Value::as_f64)
                    .unwrap_or(1.0);
                sleep_for_retry_after(retry_after).await;
                continue;
            }

            if !response.status.is_success() {
                return Err(discord_api_error(response.status, &response.body));
            }

            return Ok(response);
        }
    }

    async fn request_bytes_with_headers_authorized(
        &self,
        authorization: RequestAuthorization<'_>,
        method: Method,
        path: &str,
        body: Option<RequestBody>,
    ) -> Result<RawBytesResponse, DiscordError> {
        let route_key = rate_limit_route_key(&method, path);
        let route_gate = self.route_gates.gate_for(&route_key);
        let _route_permit = route_gate.lock().await;
        let mut rate_limit_retries = 0;

        loop {
            while let Some(wait_duration) = self.rate_limits.wait_duration(&route_key) {
                debug!(
                    "waiting for rate limit on {route_key} for {:?}",
                    wait_duration
                );
                sleep_for_retry_after(wait_duration.as_secs_f64()).await;
            }

            let response = self
                .request_once_bytes(authorization, method.clone(), path, body.as_ref())
                .await?;
            let response_text = String::from_utf8_lossy(&response.body);
            self.rate_limits.observe(
                &route_key,
                &response.headers,
                response.status,
                &response_text,
            );

            if response.status == StatusCode::TOO_MANY_REQUESTS {
                if rate_limit_retries >= MAX_RATE_LIMIT_RETRIES {
                    return Err(discord_rate_limit_error(&route_key, &response_text));
                }

                rate_limit_retries += 1;
                warn!(
                    "received rate limit for {route_key}, retrying ({rate_limit_retries}/{MAX_RATE_LIMIT_RETRIES})"
                );
                let payload = parse_body_value(response_text.into_owned());
                let retry_after = payload
                    .get("retry_after")
                    .and_then(Value::as_f64)
                    .unwrap_or(1.0);
                sleep_for_retry_after(retry_after).await;
                continue;
            }

            if !response.status.is_success() {
                return Err(discord_api_error(response.status, &response_text));
            }

            return Ok(response);
        }
    }

    async fn request_once(
        &self,
        authorization: RequestAuthorization<'_>,
        method: Method,
        path: &str,
        body: Option<&RequestBody>,
    ) -> Result<RawResponse, DiscordError> {
        let normalized_path = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{path}")
        };
        let url = format!("{}{}", self.api_base(), normalized_path);

        let mut request_builder = self.client.request(method, url).header(
            "User-Agent",
            concat!("DiscordBot (discordrs, ", env!("CARGO_PKG_VERSION"), ")"),
        );

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
        let response_text = response.text().await?;

        Ok(RawResponse {
            status,
            headers,
            body: response_text,
        })
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

        let mut request_builder = self.client.request(method, url).header(
            "User-Agent",
            concat!("DiscordBot (discordrs, ", env!("CARGO_PKG_VERSION"), ")"),
        );

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
    let duration = Duration::from_secs_f64(retry_after_seconds.max(0.0));
    tokio::time::sleep(duration).await;
}
