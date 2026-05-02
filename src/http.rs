use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use reqwest::{header::HeaderMap, Client, Method, StatusCode};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::Value;
use tracing::{debug, warn};

mod body;
mod messages;
mod paths;
mod rate_limit;
#[cfg(test)]
mod tests;

use crate::command::CommandDefinition;
use crate::error::DiscordError;
use crate::model::{
    ActivityInstance, AddGroupDmRecipient, AddGuildMember, AddLobbyMember, Application,
    ApplicationCommand, ApplicationRoleConnectionMetadata, ArchivedThreadsQuery, AuditLog,
    AuditLogQuery, AuthorizationInformation, AutoModerationRule, Ban, BeginGuildPruneRequest,
    BulkGuildBanRequest, BulkGuildBanResponse, Channel, CreateChannelInvite, CreateDmChannel,
    CreateGroupDmChannel, CreateGuildChannel, CreateGuildRole, CreateGuildSticker, CreateLobby,
    CreateMessage, CreateStageInstance, CreateTestEntitlement, CreateWebhook, CurrentUserGuild,
    CurrentUserGuildsQuery, EditApplicationCommandPermissions, EditChannelPermission, Entitlement,
    EntitlementQuery, FollowedChannel, Gateway, GatewayBot, GetGuildQuery, Guild,
    GuildApplicationCommandPermissions, GuildBansQuery, GuildIncidentsData, GuildMembersQuery,
    GuildOnboarding, GuildPreview, GuildPruneCount, GuildPruneResult, GuildScheduledEvent,
    GuildScheduledEventUser, GuildTemplate, GuildWidget, GuildWidgetImageStyle,
    GuildWidgetSettings, Integration, InteractionCallbackResponse, Invite,
    InviteTargetUsersJobStatus, JoinedArchivedThreadsQuery, LinkLobbyChannel, Lobby, LobbyMember,
    LobbyMemberUpdate, Member, Message, ModifyCurrentApplication, ModifyCurrentUser,
    ModifyCurrentUserVoiceState, ModifyGuild, ModifyGuildChannelPosition,
    ModifyGuildIncidentActions, ModifyGuildOnboarding, ModifyGuildRole, ModifyGuildRolePosition,
    ModifyGuildSticker, ModifyGuildWelcomeScreen, ModifyGuildWidgetSettings, ModifyLobby,
    ModifyStageInstance, ModifyUserVoiceState, ModifyWebhook, ModifyWebhookWithToken,
    PollAnswerVoters, Role, SearchGuildMembersQuery, SetVoiceChannelStatus, Sku, Snowflake,
    SoundboardSound, SoundboardSoundList, StageInstance, Sticker, StickerPack, StickerPackList,
    Subscription, SubscriptionQuery, ThreadListResponse, ThreadMember, ThreadMemberQuery,
    UpdateUserApplicationRoleConnection, User, UserApplicationRoleConnection, UserConnection,
    VanityUrl, VoiceRegion, VoiceState, Webhook, WebhookExecuteQuery, WebhookMessageQuery,
    WelcomeScreen,
};
use crate::types::{invalid_data_error, Emoji};
use body::{
    build_multipart_form, build_named_file_form, build_sticker_form, clone_json_body,
    multipart_body, named_file_multipart_body, parse_body_value, payload_named_file_multipart_body,
    serialize_body, RequestBody,
};
use paths::{
    archived_threads_query, audit_log_query, bool_query, configured_application_id,
    current_user_guilds_query, entitlement_query, execute_webhook_path,
    execute_webhook_path_with_query, followup_webhook_path, get_guild_query, global_commands_path,
    guild_bans_query, guild_members_query, guild_prune_query, interaction_callback_path,
    invite_query, joined_archived_threads_query, poll_answer_voters_query, rate_limit_route_key,
    request_uses_bot_authorization, search_guild_members_query, subscription_query,
    thread_member_query, validate_token_path_segment, webhook_message_path,
    webhook_message_path_with_query,
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
    #[cfg(test)]
    base_url: String,
}

/// Type alias for `DiscordHttpClient`.
pub type DiscordHttpClient = RestClient;

#[derive(Debug, serde::Deserialize)]
struct EmojiListResponse {
    #[serde(default)]
    items: Vec<Emoji>,
}

/// File data attached to a Discord multipart request.
///
/// The request body is sent with a `payload_json` part plus one `files[n]`
/// part per attachment.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileAttachment {
    /// Discord API payload field `filename`.
    pub filename: String,
    /// Discord API payload field `data`.
    pub data: Vec<u8>,
    /// Discord API payload field `content_type`.
    pub content_type: Option<String>,
}

/// Type alias for `FileUpload`.
pub type FileUpload = FileAttachment;

impl FileAttachment {
    /// Creates or returns `new` data.
    pub fn new(filename: impl Into<String>, data: impl Into<Vec<u8>>) -> Self {
        Self {
            filename: filename.into(),
            data: data.into(),
            content_type: None,
        }
    }

    /// Runs the `with_content_type` operation.
    pub fn with_content_type(mut self, content_type: impl Into<String>) -> Self {
        self.content_type = Some(content_type.into());
        self
    }
}

impl RestClient {
    /// Creates or returns `new` data.
    pub fn new(token: impl Into<String>, application_id: u64) -> Self {
        Self {
            client: Client::new(),
            token: token.into(),
            application_id: AtomicU64::new(application_id),
            rate_limits: Arc::new(RateLimitState::default()),
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
            client: Client::new(),
            token: token.into(),
            application_id: AtomicU64::new(application_id),
            rate_limits: Arc::new(RateLimitState::default()),
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

    /// Runs the `application_id` operation.
    pub fn application_id(&self) -> u64 {
        self.application_id.load(Ordering::Relaxed)
    }

    /// Runs the `set_application_id` operation.
    pub fn set_application_id(&self, application_id: u64) {
        self.application_id.store(application_id, Ordering::Relaxed);
    }

    /// Runs the `get_channel` operation.
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

    /// Runs the `delete_channel` operation.
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

    /// Runs the `update_channel` operation.
    pub async fn update_channel(
        &self,
        channel_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<Channel, DiscordError> {
        self.update_channel_typed(channel_id, body).await
    }

    /// Runs the `update_channel_typed` operation.
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

    /// Runs the `get_guild` operation.
    pub async fn get_guild(&self, guild_id: impl Into<Snowflake>) -> Result<Guild, DiscordError> {
        self.get_guild_with_query(guild_id, &GetGuildQuery::default())
            .await
    }

    /// Runs the `get_guild_with_query` operation.
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

    /// Runs the `update_guild` operation.
    pub async fn update_guild(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<Guild, DiscordError> {
        self.update_guild_typed(guild_id, body).await
    }

    /// Runs the `update_guild_typed` operation.
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

    /// Runs the `modify_guild` operation.
    pub async fn modify_guild(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &ModifyGuild,
    ) -> Result<Guild, DiscordError> {
        self.update_guild_typed(guild_id, body).await
    }

    /// Runs the `get_guild_channels` operation.
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

    /// Runs the `create_guild_channel` operation.
    pub async fn create_guild_channel(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<Channel, DiscordError> {
        self.create_guild_channel_typed(guild_id, body).await
    }

    /// Runs the `create_guild_channel_typed` operation.
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

    /// Runs the `create_guild_channel_from_request` operation.
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

    /// Runs the `get_guild_members` operation.
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

    /// Runs the `get_guild_members_with_query` operation.
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

    /// Runs the `remove_guild_member` operation.
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

    /// Runs the `add_guild_member_role` operation.
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

    /// Runs the `remove_guild_member_role` operation.
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

    /// Runs the `create_role` operation.
    pub async fn create_role(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<Role, DiscordError> {
        self.create_role_typed(guild_id, body).await
    }

    /// Runs the `create_role_typed` operation.
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

    /// Runs the `update_role` operation.
    pub async fn update_role(
        &self,
        guild_id: impl Into<Snowflake>,
        role_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<Role, DiscordError> {
        self.update_role_typed(guild_id, role_id, body).await
    }

    /// Runs the `update_role_typed` operation.
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

    /// Runs the `delete_role` operation.
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

    /// Runs the `get_member` operation.
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

    /// Runs the `list_roles` operation.
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

    /// Runs the `create_webhook` operation.
    pub async fn create_webhook(
        &self,
        channel_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<Value, DiscordError> {
        self.create_webhook_raw(channel_id, body).await
    }

    /// Runs the `create_webhook_typed` operation.
    pub async fn create_webhook_typed<B>(
        &self,
        channel_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<Webhook, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(
            Method::POST,
            &format!("/channels/{}/webhooks", channel_id.into()),
            Some(body),
        )
        .await
    }

    /// Runs the `create_webhook_from_request` operation.
    pub async fn create_webhook_from_request(
        &self,
        channel_id: impl Into<Snowflake>,
        body: &CreateWebhook,
    ) -> Result<Webhook, DiscordError> {
        self.create_webhook_typed(channel_id, body).await
    }

    /// Runs the `create_webhook_raw` operation.
    pub async fn create_webhook_raw(
        &self,
        channel_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<Value, DiscordError> {
        self.request(
            Method::POST,
            &format!("/channels/{}/webhooks", channel_id.into()),
            Some(body),
        )
        .await
    }

    /// Runs the `get_channel_webhooks` operation.
    pub async fn get_channel_webhooks(
        &self,
        channel_id: impl Into<Snowflake>,
    ) -> Result<Vec<Value>, DiscordError> {
        self.get_channel_webhooks_raw(channel_id).await
    }

    /// Runs the `get_channel_webhooks_typed` operation.
    pub async fn get_channel_webhooks_typed(
        &self,
        channel_id: impl Into<Snowflake>,
    ) -> Result<Vec<Webhook>, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/channels/{}/webhooks", channel_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `get_channel_webhooks_raw` operation.
    pub async fn get_channel_webhooks_raw(
        &self,
        channel_id: impl Into<Snowflake>,
    ) -> Result<Vec<Value>, DiscordError> {
        let response = self
            .request(
                Method::GET,
                &format!("/channels/{}/webhooks", channel_id.into()),
                Option::<&Value>::None,
            )
            .await?;
        match response {
            Value::Array(webhooks) => Ok(webhooks),
            _ => Ok(vec![]),
        }
    }

    /// Runs the `execute_webhook` operation.
    pub async fn execute_webhook(
        &self,
        webhook_id: impl Into<Snowflake>,
        token: &str,
        body: &Value,
    ) -> Result<Value, DiscordError> {
        let path = execute_webhook_path(webhook_id.into(), token)?;
        self.request(Method::POST, &path, Some(body)).await
    }

    /// Runs the `execute_webhook_with_query` operation.
    pub async fn execute_webhook_with_query(
        &self,
        webhook_id: impl Into<Snowflake>,
        token: &str,
        query: &WebhookExecuteQuery,
        body: &Value,
    ) -> Result<Value, DiscordError> {
        let path = execute_webhook_path_with_query(webhook_id.into(), token, query, None)?;
        self.request(Method::POST, &path, Some(body)).await
    }

    /// Runs the `execute_slack_compatible_webhook` operation.
    pub async fn execute_slack_compatible_webhook(
        &self,
        webhook_id: impl Into<Snowflake>,
        token: &str,
        query: &WebhookExecuteQuery,
        body: &Value,
    ) -> Result<Value, DiscordError> {
        let path =
            execute_webhook_path_with_query(webhook_id.into(), token, query, Some("/slack"))?;
        self.request(Method::POST, &path, Some(body)).await
    }

    /// Runs the `execute_github_compatible_webhook` operation.
    pub async fn execute_github_compatible_webhook(
        &self,
        webhook_id: impl Into<Snowflake>,
        token: &str,
        query: &WebhookExecuteQuery,
        body: &Value,
    ) -> Result<Value, DiscordError> {
        let path =
            execute_webhook_path_with_query(webhook_id.into(), token, query, Some("/github"))?;
        self.request(Method::POST, &path, Some(body)).await
    }

    /// Runs the `execute_webhook_with_files` operation.
    pub async fn execute_webhook_with_files(
        &self,
        webhook_id: impl Into<Snowflake>,
        token: &str,
        body: &Value,
        files: &[FileAttachment],
    ) -> Result<Value, DiscordError> {
        let path = execute_webhook_path(webhook_id.into(), token)?;
        self.request_multipart(Method::POST, &path, body, files)
            .await
    }

    /// Runs the `get_webhook_message` operation.
    pub async fn get_webhook_message(
        &self,
        webhook_id: impl Into<Snowflake>,
        token: &str,
        message_id: &str,
    ) -> Result<Message, DiscordError> {
        let path = webhook_message_path(webhook_id.into(), token, message_id)?;
        self.request_typed(Method::GET, &path, Option::<&Value>::None)
            .await
    }

    /// Runs the `get_webhook_message_with_query` operation.
    pub async fn get_webhook_message_with_query(
        &self,
        webhook_id: impl Into<Snowflake>,
        token: &str,
        message_id: &str,
        query: &WebhookMessageQuery,
    ) -> Result<Message, DiscordError> {
        let path =
            webhook_message_path_with_query(webhook_id.into(), token, message_id, query, false)?;
        self.request_typed(Method::GET, &path, Option::<&Value>::None)
            .await
    }

    /// Runs the `edit_webhook_message` operation.
    pub async fn edit_webhook_message(
        &self,
        webhook_id: impl Into<Snowflake>,
        token: &str,
        message_id: &str,
        body: &CreateMessage,
    ) -> Result<Message, DiscordError> {
        let path = webhook_message_path(webhook_id.into(), token, message_id)?;
        self.request_typed(Method::PATCH, &path, Some(body)).await
    }

    /// Runs the `edit_webhook_message_with_query` operation.
    pub async fn edit_webhook_message_with_query(
        &self,
        webhook_id: impl Into<Snowflake>,
        token: &str,
        message_id: &str,
        query: &WebhookMessageQuery,
        body: &CreateMessage,
    ) -> Result<Message, DiscordError> {
        let path =
            webhook_message_path_with_query(webhook_id.into(), token, message_id, query, true)?;
        self.request_typed(Method::PATCH, &path, Some(body)).await
    }

    /// Runs the `edit_webhook_message_with_files` operation.
    pub async fn edit_webhook_message_with_files(
        &self,
        webhook_id: impl Into<Snowflake>,
        token: &str,
        message_id: &str,
        body: &CreateMessage,
        files: &[FileAttachment],
    ) -> Result<Message, DiscordError> {
        let path = webhook_message_path(webhook_id.into(), token, message_id)?;
        self.request_typed_multipart(Method::PATCH, &path, body, files)
            .await
    }

    /// Runs the `edit_webhook_message_with_files_and_query` operation.
    pub async fn edit_webhook_message_with_files_and_query(
        &self,
        webhook_id: impl Into<Snowflake>,
        token: &str,
        message_id: &str,
        query: &WebhookMessageQuery,
        body: &CreateMessage,
        files: &[FileAttachment],
    ) -> Result<Message, DiscordError> {
        let path =
            webhook_message_path_with_query(webhook_id.into(), token, message_id, query, true)?;
        self.request_typed_multipart(Method::PATCH, &path, body, files)
            .await
    }

    /// Runs the `delete_webhook_message` operation.
    pub async fn delete_webhook_message(
        &self,
        webhook_id: impl Into<Snowflake>,
        token: &str,
        message_id: &str,
    ) -> Result<(), DiscordError> {
        let path = webhook_message_path(webhook_id.into(), token, message_id)?;
        self.request_no_content(Method::DELETE, &path, Option::<&Value>::None)
            .await
    }

    /// Runs the `delete_webhook_message_with_query` operation.
    pub async fn delete_webhook_message_with_query(
        &self,
        webhook_id: impl Into<Snowflake>,
        token: &str,
        message_id: &str,
        query: &WebhookMessageQuery,
    ) -> Result<(), DiscordError> {
        let path =
            webhook_message_path_with_query(webhook_id.into(), token, message_id, query, false)?;
        self.request_no_content(Method::DELETE, &path, Option::<&Value>::None)
            .await
    }

    /// Runs the `create_dm_channel_typed` operation.
    pub async fn create_dm_channel_typed(
        &self,
        body: &CreateDmChannel,
    ) -> Result<Channel, DiscordError> {
        self.request_typed(Method::POST, "/users/@me/channels", Some(body))
            .await
    }

    /// Runs the `create_group_dm_channel_typed` operation.
    pub async fn create_group_dm_channel_typed(
        &self,
        body: &CreateGroupDmChannel,
    ) -> Result<Channel, DiscordError> {
        self.request_typed(Method::POST, "/users/@me/channels", Some(body))
            .await
    }

    /// Runs the `add_group_dm_recipient` operation.
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

    /// Runs the `remove_group_dm_recipient` operation.
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

    /// Runs the `create_interaction_response_typed` operation.
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

    /// Runs the `create_interaction_response_with_files` operation.
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

    /// Runs the `bulk_overwrite_global_commands_typed` operation.
    pub async fn bulk_overwrite_global_commands_typed(
        &self,
        commands: &[CommandDefinition],
    ) -> Result<Vec<ApplicationCommand>, DiscordError> {
        let path = global_commands_path(self.application_id())?;
        self.request_typed(Method::PUT, &path, Some(commands)).await
    }

    /// Runs the `create_global_command` operation.
    pub async fn create_global_command(
        &self,
        command: &CommandDefinition,
    ) -> Result<ApplicationCommand, DiscordError> {
        let path = global_commands_path(self.application_id())?;
        self.request_typed(Method::POST, &path, Some(command)).await
    }

    /// Runs the `get_global_commands` operation.
    pub async fn get_global_commands(&self) -> Result<Vec<ApplicationCommand>, DiscordError> {
        let path = global_commands_path(self.application_id())?;
        self.request_typed(Method::GET, &path, Option::<&Value>::None)
            .await
    }

    /// Runs the `get_current_application` operation.
    pub async fn get_current_application(&self) -> Result<Application, DiscordError> {
        self.request_typed(Method::GET, "/applications/@me", Option::<&Value>::None)
            .await
    }

    /// Runs the `edit_current_application` operation.
    pub async fn edit_current_application<B>(&self, body: &B) -> Result<Application, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(Method::PATCH, "/applications/@me", Some(body))
            .await
    }

    /// Runs the `edit_current_application_from_request` operation.
    pub async fn edit_current_application_from_request(
        &self,
        body: &ModifyCurrentApplication,
    ) -> Result<Application, DiscordError> {
        self.edit_current_application(body).await
    }

    /// Runs the `get_application_activity_instance` operation.
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

    /// Runs the `get_application_role_connection_metadata_records` operation.
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

    /// Runs the `update_application_role_connection_metadata_records` operation.
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

    /// Runs the `get_gateway_bot` operation.
    pub async fn get_gateway_bot(&self) -> Result<GatewayBot, DiscordError> {
        self.request_typed(Method::GET, "/gateway/bot", Option::<&Value>::None)
            .await
    }

    /// Runs the `get_gateway` operation.
    pub async fn get_gateway(&self) -> Result<Gateway, DiscordError> {
        self.request_typed(Method::GET, "/gateway", Option::<&Value>::None)
            .await
    }

    /// Runs the `bulk_overwrite_guild_commands_typed` operation.
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

    /// Runs the `pin_message` operation.
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

    /// Runs the `unpin_message` operation.
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

    /// Runs the `get_pinned_messages` operation.
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

    /// Runs the legacy `pin_message` route.
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

    /// Runs the legacy `unpin_message` route.
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

    /// Runs the `set_voice_channel_status` operation.
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

    /// Runs the `trigger_typing_indicator` operation.
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

    /// Runs the `edit_channel_permissions` operation.
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

    /// Runs the `edit_channel_permissions_typed` operation.
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

    /// Runs the `delete_channel_permission` operation.
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

    /// Runs the `create_thread_from_message` operation.
    pub async fn create_thread_from_message(
        &self,
        channel_id: impl Into<Snowflake>,
        message_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<Channel, DiscordError> {
        self.request_typed(
            Method::POST,
            &format!(
                "/channels/{}/messages/{}/threads",
                channel_id.into(),
                message_id.into()
            ),
            Some(body),
        )
        .await
    }

    /// Runs the `create_thread` operation.
    pub async fn create_thread(
        &self,
        channel_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<Channel, DiscordError> {
        self.request_typed(
            Method::POST,
            &format!("/channels/{}/threads", channel_id.into()),
            Some(body),
        )
        .await
    }

    /// Runs the `join_thread` operation.
    pub async fn join_thread(&self, channel_id: impl Into<Snowflake>) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::PUT,
            &format!("/channels/{}/thread-members/@me", channel_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `add_thread_member` operation.
    pub async fn add_thread_member(
        &self,
        channel_id: impl Into<Snowflake>,
        user_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::PUT,
            &format!(
                "/channels/{}/thread-members/{}",
                channel_id.into(),
                user_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `leave_thread` operation.
    pub async fn leave_thread(&self, channel_id: impl Into<Snowflake>) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!("/channels/{}/thread-members/@me", channel_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `remove_thread_member` operation.
    pub async fn remove_thread_member(
        &self,
        channel_id: impl Into<Snowflake>,
        user_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!(
                "/channels/{}/thread-members/{}",
                channel_id.into(),
                user_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `get_thread_member` operation.
    pub async fn get_thread_member(
        &self,
        channel_id: impl Into<Snowflake>,
        user_id: impl Into<Snowflake>,
        with_member: Option<bool>,
    ) -> Result<ThreadMember, DiscordError> {
        let query = bool_query("with_member", with_member);
        self.request_typed(
            Method::GET,
            &format!(
                "/channels/{}/thread-members/{}{}",
                channel_id.into(),
                user_id.into(),
                query
            ),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `get_thread_members` operation.
    pub async fn get_thread_members(
        &self,
        channel_id: impl Into<Snowflake>,
    ) -> Result<Vec<ThreadMember>, DiscordError> {
        self.list_thread_members(channel_id, &ThreadMemberQuery::default())
            .await
    }

    /// Runs the `list_thread_members` operation.
    pub async fn list_thread_members(
        &self,
        channel_id: impl Into<Snowflake>,
        query: &ThreadMemberQuery,
    ) -> Result<Vec<ThreadMember>, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!(
                "/channels/{}/thread-members{}",
                channel_id.into(),
                thread_member_query(query)
            ),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `get_public_archived_threads` operation.
    pub async fn get_public_archived_threads(
        &self,
        channel_id: impl Into<Snowflake>,
        limit: Option<u64>,
    ) -> Result<serde_json::Value, DiscordError> {
        let path = match limit {
            Some(l) => format!(
                "/channels/{}/threads/archived/public?limit={}",
                channel_id.into(),
                l
            ),
            None => format!("/channels/{}/threads/archived/public", channel_id.into()),
        };
        self.request(Method::GET, &path, Option::<&Value>::None)
            .await
    }

    /// Runs the `list_public_archived_threads` operation.
    pub async fn list_public_archived_threads(
        &self,
        channel_id: impl Into<Snowflake>,
        query: &ArchivedThreadsQuery,
    ) -> Result<ThreadListResponse, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!(
                "/channels/{}/threads/archived/public{}",
                channel_id.into(),
                archived_threads_query(query)
            ),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `list_private_archived_threads` operation.
    pub async fn list_private_archived_threads(
        &self,
        channel_id: impl Into<Snowflake>,
        query: &ArchivedThreadsQuery,
    ) -> Result<ThreadListResponse, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!(
                "/channels/{}/threads/archived/private{}",
                channel_id.into(),
                archived_threads_query(query)
            ),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `list_joined_private_archived_threads` operation.
    pub async fn list_joined_private_archived_threads(
        &self,
        channel_id: impl Into<Snowflake>,
        query: &JoinedArchivedThreadsQuery,
    ) -> Result<ThreadListResponse, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!(
                "/channels/{}/users/@me/threads/archived/private{}",
                channel_id.into(),
                joined_archived_threads_query(query)
            ),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `get_active_guild_threads` operation.
    pub async fn get_active_guild_threads(
        &self,
        guild_id: impl Into<Snowflake>,
    ) -> Result<ThreadListResponse, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/threads/active", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `get_guild_bans` operation.
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

    /// Runs the `get_guild_bans_with_query` operation.
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

    /// Runs the `get_guild_ban` operation.
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

    /// Runs the `create_guild_ban` operation.
    pub async fn create_guild_ban(
        &self,
        guild_id: impl Into<Snowflake>,
        user_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<(), DiscordError> {
        self.create_guild_ban_typed(guild_id, user_id, body).await
    }

    /// Runs the `create_guild_ban_typed` operation.
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

    /// Runs the `bulk_guild_ban` operation.
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

    /// Runs the `remove_guild_ban` operation.
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

    /// Runs the `modify_guild_member` operation.
    pub async fn modify_guild_member(
        &self,
        guild_id: impl Into<Snowflake>,
        user_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<(), DiscordError> {
        self.modify_guild_member_typed(guild_id, user_id, body)
            .await
    }

    /// Runs the `modify_guild_member_typed` operation.
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

    /// Runs the `modify_current_member_nick` operation.
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

    /// Runs the `modify_current_member` operation.
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

    /// Runs the `search_guild_members` operation.
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

    /// Runs the `search_guild_members_with_query` operation.
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

    /// Runs the `get_guild_audit_log` operation.
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

    /// Runs the `get_guild_audit_log_typed` operation.
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

    /// Runs the `modify_guild_role_positions` operation.
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

    /// Runs the `modify_guild_role_positions_typed` operation.
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

    /// Runs the `create_guild_role` operation.
    pub async fn create_guild_role(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &CreateGuildRole,
    ) -> Result<Role, DiscordError> {
        self.create_role_typed(guild_id, body).await
    }

    /// Runs the `modify_guild_role` operation.
    pub async fn modify_guild_role(
        &self,
        guild_id: impl Into<Snowflake>,
        role_id: impl Into<Snowflake>,
        body: &ModifyGuildRole,
    ) -> Result<Role, DiscordError> {
        self.update_role_typed(guild_id, role_id, body).await
    }

    /// Runs the `get_guild_role` operation.
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

    /// Runs the `get_guild_emojis` operation.
    pub async fn get_guild_emojis(
        &self,
        guild_id: impl Into<Snowflake>,
    ) -> Result<Vec<serde_json::Value>, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/emojis", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `get_guild_emojis_typed` operation.
    pub async fn get_guild_emojis_typed(
        &self,
        guild_id: impl Into<Snowflake>,
    ) -> Result<Vec<Emoji>, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/emojis", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `get_guild_emoji` operation.
    pub async fn get_guild_emoji(
        &self,
        guild_id: impl Into<Snowflake>,
        emoji_id: impl Into<Snowflake>,
    ) -> Result<serde_json::Value, DiscordError> {
        self.request(
            Method::GET,
            &format!("/guilds/{}/emojis/{}", guild_id.into(), emoji_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `get_guild_emoji_typed` operation.
    pub async fn get_guild_emoji_typed(
        &self,
        guild_id: impl Into<Snowflake>,
        emoji_id: impl Into<Snowflake>,
    ) -> Result<Emoji, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/emojis/{}", guild_id.into(), emoji_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `create_guild_emoji` operation.
    pub async fn create_guild_emoji(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<serde_json::Value, DiscordError> {
        self.request(
            Method::POST,
            &format!("/guilds/{}/emojis", guild_id.into()),
            Some(body),
        )
        .await
    }

    /// Runs the `create_guild_emoji_typed` operation.
    pub async fn create_guild_emoji_typed<B>(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<Emoji, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(
            Method::POST,
            &format!("/guilds/{}/emojis", guild_id.into()),
            Some(body),
        )
        .await
    }

    /// Runs the `modify_guild_emoji` operation.
    pub async fn modify_guild_emoji(
        &self,
        guild_id: impl Into<Snowflake>,
        emoji_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<serde_json::Value, DiscordError> {
        self.request(
            Method::PATCH,
            &format!("/guilds/{}/emojis/{}", guild_id.into(), emoji_id.into()),
            Some(body),
        )
        .await
    }

    /// Runs the `modify_guild_emoji_typed` operation.
    pub async fn modify_guild_emoji_typed<B>(
        &self,
        guild_id: impl Into<Snowflake>,
        emoji_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<Emoji, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(
            Method::PATCH,
            &format!("/guilds/{}/emojis/{}", guild_id.into(), emoji_id.into()),
            Some(body),
        )
        .await
    }

    /// Runs the `delete_guild_emoji` operation.
    pub async fn delete_guild_emoji(
        &self,
        guild_id: impl Into<Snowflake>,
        emoji_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!("/guilds/{}/emojis/{}", guild_id.into(), emoji_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `get_application_emojis` operation.
    pub async fn get_application_emojis(&self) -> Result<Vec<serde_json::Value>, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        let response = self
            .request(
                Method::GET,
                &format!("/applications/{application_id}/emojis"),
                Option::<&Value>::None,
            )
            .await?;
        Ok(response
            .get("items")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default())
    }

    /// Runs the `get_application_emojis_typed` operation.
    pub async fn get_application_emojis_typed(&self) -> Result<Vec<Emoji>, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        let response: EmojiListResponse = self
            .request_typed(
                Method::GET,
                &format!("/applications/{application_id}/emojis"),
                Option::<&Value>::None,
            )
            .await?;
        Ok(response.items)
    }

    /// Runs the `get_application_emoji` operation.
    pub async fn get_application_emoji(
        &self,
        emoji_id: impl Into<Snowflake>,
    ) -> Result<serde_json::Value, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request(
            Method::GET,
            &format!("/applications/{application_id}/emojis/{}", emoji_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `get_application_emoji_typed` operation.
    pub async fn get_application_emoji_typed(
        &self,
        emoji_id: impl Into<Snowflake>,
    ) -> Result<Emoji, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request_typed(
            Method::GET,
            &format!("/applications/{application_id}/emojis/{}", emoji_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `create_application_emoji` operation.
    pub async fn create_application_emoji(
        &self,
        body: &Value,
    ) -> Result<serde_json::Value, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request(
            Method::POST,
            &format!("/applications/{application_id}/emojis"),
            Some(body),
        )
        .await
    }

    /// Runs the `create_application_emoji_typed` operation.
    pub async fn create_application_emoji_typed<B>(&self, body: &B) -> Result<Emoji, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        let application_id = configured_application_id(self.application_id())?;
        self.request_typed(
            Method::POST,
            &format!("/applications/{application_id}/emojis"),
            Some(body),
        )
        .await
    }

    /// Runs the `modify_application_emoji` operation.
    pub async fn modify_application_emoji(
        &self,
        emoji_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<serde_json::Value, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request(
            Method::PATCH,
            &format!("/applications/{application_id}/emojis/{}", emoji_id.into()),
            Some(body),
        )
        .await
    }

    /// Runs the `modify_application_emoji_typed` operation.
    pub async fn modify_application_emoji_typed<B>(
        &self,
        emoji_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<Emoji, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        let application_id = configured_application_id(self.application_id())?;
        self.request_typed(
            Method::PATCH,
            &format!("/applications/{application_id}/emojis/{}", emoji_id.into()),
            Some(body),
        )
        .await
    }

    /// Runs the `delete_application_emoji` operation.
    pub async fn delete_application_emoji(
        &self,
        emoji_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request_no_content(
            Method::DELETE,
            &format!("/applications/{application_id}/emojis/{}", emoji_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `get_skus` operation.
    pub async fn get_skus(&self) -> Result<Vec<Sku>, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request_typed(
            Method::GET,
            &format!("/applications/{application_id}/skus"),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `get_sku_subscriptions` operation.
    pub async fn get_sku_subscriptions(
        &self,
        sku_id: impl Into<Snowflake>,
        query: &SubscriptionQuery,
    ) -> Result<Vec<Subscription>, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!(
                "/skus/{}/subscriptions{}",
                sku_id.into(),
                subscription_query(query)
            ),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `get_sku_subscription` operation.
    pub async fn get_sku_subscription(
        &self,
        sku_id: impl Into<Snowflake>,
        subscription_id: impl Into<Snowflake>,
    ) -> Result<Subscription, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!(
                "/skus/{}/subscriptions/{}",
                sku_id.into(),
                subscription_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `get_entitlements` operation.
    pub async fn get_entitlements(
        &self,
        query: &EntitlementQuery,
    ) -> Result<Vec<Entitlement>, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request_typed(
            Method::GET,
            &format!(
                "/applications/{application_id}/entitlements{}",
                entitlement_query(query)
            ),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `get_entitlement` operation.
    pub async fn get_entitlement(
        &self,
        entitlement_id: impl Into<Snowflake>,
    ) -> Result<Entitlement, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request_typed(
            Method::GET,
            &format!(
                "/applications/{application_id}/entitlements/{}",
                entitlement_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `consume_entitlement` operation.
    pub async fn consume_entitlement(
        &self,
        entitlement_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request_no_content(
            Method::POST,
            &format!(
                "/applications/{application_id}/entitlements/{}/consume",
                entitlement_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `create_test_entitlement` operation.
    pub async fn create_test_entitlement(
        &self,
        body: &CreateTestEntitlement,
    ) -> Result<Entitlement, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request_typed(
            Method::POST,
            &format!("/applications/{application_id}/entitlements"),
            Some(body),
        )
        .await
    }

    /// Runs the `delete_test_entitlement` operation.
    pub async fn delete_test_entitlement(
        &self,
        entitlement_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request_no_content(
            Method::DELETE,
            &format!(
                "/applications/{application_id}/entitlements/{}",
                entitlement_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `get_sticker` operation.
    pub async fn get_sticker(
        &self,
        sticker_id: impl Into<Snowflake>,
    ) -> Result<Sticker, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/stickers/{}", sticker_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `list_sticker_packs` operation.
    pub async fn list_sticker_packs(&self) -> Result<StickerPackList, DiscordError> {
        self.request_typed(Method::GET, "/sticker-packs", Option::<&Value>::None)
            .await
    }

    /// Runs the `get_sticker_pack` operation.
    pub async fn get_sticker_pack(
        &self,
        pack_id: impl Into<Snowflake>,
    ) -> Result<StickerPack, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/sticker-packs/{}", pack_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `get_guild_stickers` operation.
    pub async fn get_guild_stickers(
        &self,
        guild_id: impl Into<Snowflake>,
    ) -> Result<Vec<Sticker>, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/stickers", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `get_guild_sticker` operation.
    pub async fn get_guild_sticker(
        &self,
        guild_id: impl Into<Snowflake>,
        sticker_id: impl Into<Snowflake>,
    ) -> Result<Sticker, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/stickers/{}", guild_id.into(), sticker_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `create_guild_sticker` operation.
    pub async fn create_guild_sticker(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &Value,
        file: FileAttachment,
    ) -> Result<Sticker, DiscordError> {
        let path = format!("/guilds/{}/stickers", guild_id.into());
        let response = self
            .request_with_headers(
                Method::POST,
                &path,
                Some(RequestBody::StickerMultipart {
                    payload_json: body.clone(),
                    file,
                }),
            )
            .await?;
        serde_json::from_value(parse_body_value(response.body)).map_err(Into::into)
    }

    /// Runs the `create_guild_sticker_typed` operation.
    pub async fn create_guild_sticker_typed<B>(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &B,
        file: FileAttachment,
    ) -> Result<Sticker, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        let path = format!("/guilds/{}/stickers", guild_id.into());
        let response = self
            .request_with_headers(
                Method::POST,
                &path,
                Some(RequestBody::StickerMultipart {
                    payload_json: serialize_body(body)?,
                    file,
                }),
            )
            .await?;
        serde_json::from_value(parse_body_value(response.body)).map_err(Into::into)
    }

    /// Runs the `create_guild_sticker_from_request` operation.
    pub async fn create_guild_sticker_from_request(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &CreateGuildSticker,
        file: FileAttachment,
    ) -> Result<Sticker, DiscordError> {
        self.create_guild_sticker_typed(guild_id, body, file).await
    }

    /// Runs the `modify_guild_sticker` operation.
    pub async fn modify_guild_sticker(
        &self,
        guild_id: impl Into<Snowflake>,
        sticker_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<Sticker, DiscordError> {
        self.request_typed(
            Method::PATCH,
            &format!("/guilds/{}/stickers/{}", guild_id.into(), sticker_id.into()),
            Some(body),
        )
        .await
    }

    /// Runs the `modify_guild_sticker_typed` operation.
    pub async fn modify_guild_sticker_typed<B>(
        &self,
        guild_id: impl Into<Snowflake>,
        sticker_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<Sticker, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(
            Method::PATCH,
            &format!("/guilds/{}/stickers/{}", guild_id.into(), sticker_id.into()),
            Some(body),
        )
        .await
    }

    /// Runs the `modify_guild_sticker_from_request` operation.
    pub async fn modify_guild_sticker_from_request(
        &self,
        guild_id: impl Into<Snowflake>,
        sticker_id: impl Into<Snowflake>,
        body: &ModifyGuildSticker,
    ) -> Result<Sticker, DiscordError> {
        self.modify_guild_sticker_typed(guild_id, sticker_id, body)
            .await
    }

    /// Runs the `delete_guild_sticker` operation.
    pub async fn delete_guild_sticker(
        &self,
        guild_id: impl Into<Snowflake>,
        sticker_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!("/guilds/{}/stickers/{}", guild_id.into(), sticker_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `send_soundboard_sound` operation.
    pub async fn send_soundboard_sound(
        &self,
        channel_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::POST,
            &format!("/channels/{}/send-soundboard-sound", channel_id.into()),
            Some(body),
        )
        .await
    }

    /// Runs the `list_default_soundboard_sounds` operation.
    pub async fn list_default_soundboard_sounds(
        &self,
    ) -> Result<Vec<SoundboardSound>, DiscordError> {
        self.request_typed(
            Method::GET,
            "/soundboard-default-sounds",
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `list_guild_soundboard_sounds` operation.
    pub async fn list_guild_soundboard_sounds(
        &self,
        guild_id: impl Into<Snowflake>,
    ) -> Result<SoundboardSoundList, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/soundboard-sounds", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `get_guild_soundboard_sound` operation.
    pub async fn get_guild_soundboard_sound(
        &self,
        guild_id: impl Into<Snowflake>,
        sound_id: impl Into<Snowflake>,
    ) -> Result<SoundboardSound, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!(
                "/guilds/{}/soundboard-sounds/{}",
                guild_id.into(),
                sound_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `create_guild_soundboard_sound` operation.
    pub async fn create_guild_soundboard_sound(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<SoundboardSound, DiscordError> {
        self.request_typed(
            Method::POST,
            &format!("/guilds/{}/soundboard-sounds", guild_id.into()),
            Some(body),
        )
        .await
    }

    /// Runs the `modify_guild_soundboard_sound` operation.
    pub async fn modify_guild_soundboard_sound(
        &self,
        guild_id: impl Into<Snowflake>,
        sound_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<SoundboardSound, DiscordError> {
        self.request_typed(
            Method::PATCH,
            &format!(
                "/guilds/{}/soundboard-sounds/{}",
                guild_id.into(),
                sound_id.into()
            ),
            Some(body),
        )
        .await
    }

    /// Runs the `delete_guild_soundboard_sound` operation.
    pub async fn delete_guild_soundboard_sound(
        &self,
        guild_id: impl Into<Snowflake>,
        sound_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!(
                "/guilds/{}/soundboard-sounds/{}",
                guild_id.into(),
                sound_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `get_guild_invites` operation.
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

    /// Runs the `get_invite` operation.
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

    /// Runs the `get_invite_with_options` operation.
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

    /// Runs the `delete_invite` operation.
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

    /// Runs the `get_invite_target_users` operation.
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

    /// Runs the `get_invite_target_users_csv` operation.
    pub async fn get_invite_target_users_csv(&self, code: &str) -> Result<String, DiscordError> {
        String::from_utf8(self.get_invite_target_users(code).await?)
            .map_err(|_| invalid_data_error("invite target users response was not valid UTF-8"))
    }

    /// Runs the `update_invite_target_users` operation.
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

    /// Runs the `get_invite_target_users_job_status` operation.
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

    /// Runs the `get_guild_integrations` operation.
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

    /// Runs the `delete_guild_integration` operation.
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

    /// Runs the `get_poll_answer_voters` operation.
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

    /// Runs the `end_poll` operation.
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

    /// Runs the `create_channel_invite` operation.
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

    /// Runs the `get_channel_invites` operation.
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

    /// Runs the `create_channel_invite_typed` operation.
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

    /// Runs the `create_channel_invite_with_target_users_file` operation.
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

    /// Runs the `get_current_user` operation.
    pub async fn get_current_user(&self) -> Result<User, DiscordError> {
        self.request_typed(Method::GET, "/users/@me", Option::<&Value>::None)
            .await
    }

    /// Modifies the current bot user.
    pub async fn modify_current_user(
        &self,
        body: &ModifyCurrentUser,
    ) -> Result<User, DiscordError> {
        self.request_typed(Method::PATCH, "/users/@me", Some(body))
            .await
    }

    /// Runs the `get_user` operation.
    pub async fn get_user(&self, user_id: impl Into<Snowflake>) -> Result<User, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/users/{}", user_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `get_current_user_guilds` operation.
    pub async fn get_current_user_guilds(&self) -> Result<Vec<serde_json::Value>, DiscordError> {
        self.get_current_user_guilds_with_query(&CurrentUserGuildsQuery::default())
            .await
    }

    /// Runs the `get_current_user_guilds_with_query` operation.
    pub async fn get_current_user_guilds_with_query(
        &self,
        query: &CurrentUserGuildsQuery,
    ) -> Result<Vec<serde_json::Value>, DiscordError> {
        let query = current_user_guilds_query(query);
        self.request_typed(
            Method::GET,
            &format!("/users/@me/guilds{query}"),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `get_current_user_guilds_typed` operation.
    pub async fn get_current_user_guilds_typed(
        &self,
    ) -> Result<Vec<CurrentUserGuild>, DiscordError> {
        self.get_current_user_guilds_typed_with_query(&CurrentUserGuildsQuery::default())
            .await
    }

    /// Runs the `get_current_user_guilds_typed_with_query` operation.
    pub async fn get_current_user_guilds_typed_with_query(
        &self,
        query: &CurrentUserGuildsQuery,
    ) -> Result<Vec<CurrentUserGuild>, DiscordError> {
        let query = current_user_guilds_query(query);
        self.request_typed(
            Method::GET,
            &format!("/users/@me/guilds{query}"),
            Option::<&Value>::None,
        )
        .await
    }

    /// Returns the current OAuth2 user's guild member object for one guild.
    pub async fn get_current_user_guild_member(
        &self,
        bearer_token: &str,
        guild_id: impl Into<Snowflake>,
    ) -> Result<Member, DiscordError> {
        let bearer_token = validate_authorization_token("bearer_token", bearer_token)?;
        self.request_typed_with_authorization(
            RequestAuthorization::Bearer(bearer_token),
            Method::GET,
            &format!("/users/@me/guilds/{}/member", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `leave_guild` operation.
    pub async fn leave_guild(&self, guild_id: impl Into<Snowflake>) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!("/users/@me/guilds/{}", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Returns OAuth2 connections for the current user.
    pub async fn get_current_user_connections(
        &self,
        bearer_token: &str,
    ) -> Result<Vec<UserConnection>, DiscordError> {
        let bearer_token = validate_authorization_token("bearer_token", bearer_token)?;
        self.request_typed_with_authorization(
            RequestAuthorization::Bearer(bearer_token),
            Method::GET,
            "/users/@me/connections",
            Option::<&Value>::None,
        )
        .await
    }

    /// Returns the current user's application role connection for this client application.
    pub async fn get_current_user_application_role_connection(
        &self,
        bearer_token: &str,
    ) -> Result<UserApplicationRoleConnection, DiscordError> {
        let bearer_token = validate_authorization_token("bearer_token", bearer_token)?;
        let application_id = configured_application_id(self.application_id())?;
        self.request_typed_with_authorization(
            RequestAuthorization::Bearer(bearer_token),
            Method::GET,
            &format!("/users/@me/applications/{application_id}/role-connection"),
            Option::<&Value>::None,
        )
        .await
    }

    /// Updates the current user's application role connection for this client application.
    pub async fn update_current_user_application_role_connection(
        &self,
        bearer_token: &str,
        body: &UpdateUserApplicationRoleConnection,
    ) -> Result<UserApplicationRoleConnection, DiscordError> {
        let bearer_token = validate_authorization_token("bearer_token", bearer_token)?;
        let application_id = configured_application_id(self.application_id())?;
        self.request_typed_with_authorization(
            RequestAuthorization::Bearer(bearer_token),
            Method::PUT,
            &format!("/users/@me/applications/{application_id}/role-connection"),
            Some(body),
        )
        .await
    }

    /// Returns the bot application's OAuth2 application object.
    pub async fn get_current_bot_application_information(
        &self,
    ) -> Result<Application, DiscordError> {
        self.request_typed(
            Method::GET,
            "/oauth2/applications/@me",
            Option::<&Value>::None,
        )
        .await
    }

    /// Returns OAuth2 authorization information for a bearer token.
    pub async fn get_current_authorization_information(
        &self,
        bearer_token: &str,
    ) -> Result<AuthorizationInformation, DiscordError> {
        let bearer_token = validate_authorization_token("bearer_token", bearer_token)?;
        self.request_typed_with_authorization(
            RequestAuthorization::Bearer(bearer_token),
            Method::GET,
            "/oauth2/@me",
            Option::<&Value>::None,
        )
        .await
    }

    /// Creates a Discord lobby owned by the configured application.
    pub async fn create_lobby(&self, body: &CreateLobby) -> Result<Lobby, DiscordError> {
        self.request_typed(Method::POST, "/lobbies", Some(body))
            .await
    }

    /// Returns one lobby by ID.
    pub async fn get_lobby(&self, lobby_id: impl Into<Snowflake>) -> Result<Lobby, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/lobbies/{}", lobby_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Modifies one lobby.
    pub async fn modify_lobby(
        &self,
        lobby_id: impl Into<Snowflake>,
        body: &ModifyLobby,
    ) -> Result<Lobby, DiscordError> {
        self.request_typed(
            Method::PATCH,
            &format!("/lobbies/{}", lobby_id.into()),
            Some(body),
        )
        .await
    }

    /// Deletes one lobby.
    pub async fn delete_lobby(&self, lobby_id: impl Into<Snowflake>) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!("/lobbies/{}", lobby_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Adds or updates one lobby member.
    pub async fn add_lobby_member(
        &self,
        lobby_id: impl Into<Snowflake>,
        user_id: impl Into<Snowflake>,
        body: &AddLobbyMember,
    ) -> Result<LobbyMember, DiscordError> {
        self.request_typed(
            Method::PUT,
            &format!("/lobbies/{}/members/{}", lobby_id.into(), user_id.into()),
            Some(body),
        )
        .await
    }

    /// Adds, updates, or removes lobby members in bulk.
    pub async fn bulk_update_lobby_members(
        &self,
        lobby_id: impl Into<Snowflake>,
        body: &[LobbyMemberUpdate],
    ) -> Result<Vec<LobbyMember>, DiscordError> {
        self.request_typed(
            Method::POST,
            &format!("/lobbies/{}/members/bulk", lobby_id.into()),
            Some(body),
        )
        .await
    }

    /// Removes one lobby member.
    pub async fn remove_lobby_member(
        &self,
        lobby_id: impl Into<Snowflake>,
        user_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!("/lobbies/{}/members/{}", lobby_id.into(), user_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Removes the current OAuth2 user from a lobby.
    pub async fn leave_lobby(
        &self,
        bearer_token: &str,
        lobby_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        let bearer_token = validate_authorization_token("bearer_token", bearer_token)?;
        self.request_with_headers_authorized(
            RequestAuthorization::Bearer(bearer_token),
            Method::DELETE,
            &format!("/lobbies/{}/members/@me", lobby_id.into()),
            None,
        )
        .await
        .map(|_| ())
    }

    /// Links or unlinks a channel for the current OAuth2 user in a lobby.
    pub async fn link_lobby_channel(
        &self,
        bearer_token: &str,
        lobby_id: impl Into<Snowflake>,
        body: &LinkLobbyChannel,
    ) -> Result<Lobby, DiscordError> {
        let bearer_token = validate_authorization_token("bearer_token", bearer_token)?;
        self.request_typed_with_authorization(
            RequestAuthorization::Bearer(bearer_token),
            Method::PATCH,
            &format!("/lobbies/{}/channel-linking", lobby_id.into()),
            Some(body),
        )
        .await
    }

    /// Updates app-scoped moderation metadata for lobby messages.
    pub async fn update_lobby_message_moderation_metadata(
        &self,
        lobby_id: impl Into<Snowflake>,
        message_id: impl Into<Snowflake>,
        metadata: &HashMap<String, String>,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::PUT,
            &format!(
                "/lobbies/{}/messages/{}/moderation-metadata",
                lobby_id.into(),
                message_id.into()
            ),
            Some(metadata),
        )
        .await
    }

    /// Runs the `get_guild_webhooks` operation.
    pub async fn get_guild_webhooks(
        &self,
        guild_id: impl Into<Snowflake>,
    ) -> Result<Vec<Webhook>, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/webhooks", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `get_webhook` operation.
    pub async fn get_webhook(
        &self,
        webhook_id: impl Into<Snowflake>,
    ) -> Result<Webhook, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/webhooks/{}", webhook_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `get_webhook_with_token` operation.
    pub async fn get_webhook_with_token(
        &self,
        webhook_id: impl Into<Snowflake>,
        token: &str,
    ) -> Result<Webhook, DiscordError> {
        validate_token_path_segment("webhook_token", token, false)?;
        self.request_typed(
            Method::GET,
            &format!("/webhooks/{}/{}", webhook_id.into(), token),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `modify_webhook` operation.
    pub async fn modify_webhook(
        &self,
        webhook_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<Webhook, DiscordError> {
        self.request_typed(
            Method::PATCH,
            &format!("/webhooks/{}", webhook_id.into()),
            Some(body),
        )
        .await
    }

    /// Runs the `modify_webhook_typed` operation.
    pub async fn modify_webhook_typed<B>(
        &self,
        webhook_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<Webhook, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(
            Method::PATCH,
            &format!("/webhooks/{}", webhook_id.into()),
            Some(body),
        )
        .await
    }

    /// Runs the `modify_webhook_from_request` operation.
    pub async fn modify_webhook_from_request(
        &self,
        webhook_id: impl Into<Snowflake>,
        body: &ModifyWebhook,
    ) -> Result<Webhook, DiscordError> {
        self.modify_webhook_typed(webhook_id, body).await
    }

    /// Runs the `delete_webhook` operation.
    pub async fn delete_webhook(
        &self,
        webhook_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!("/webhooks/{}", webhook_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `modify_webhook_with_token` operation.
    pub async fn modify_webhook_with_token(
        &self,
        webhook_id: impl Into<Snowflake>,
        token: &str,
        body: &Value,
    ) -> Result<Webhook, DiscordError> {
        validate_token_path_segment("webhook_token", token, false)?;
        let path = format!("/webhooks/{}/{}", webhook_id.into(), token);
        self.request_typed(Method::PATCH, &path, Some(body)).await
    }

    /// Runs the `modify_webhook_with_token_typed` operation.
    pub async fn modify_webhook_with_token_typed<B>(
        &self,
        webhook_id: impl Into<Snowflake>,
        token: &str,
        body: &B,
    ) -> Result<Webhook, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        validate_token_path_segment("webhook_token", token, false)?;
        let path = format!("/webhooks/{}/{}", webhook_id.into(), token);
        self.request_typed(Method::PATCH, &path, Some(body)).await
    }

    /// Runs the `modify_webhook_with_token_from_request` operation.
    pub async fn modify_webhook_with_token_from_request(
        &self,
        webhook_id: impl Into<Snowflake>,
        token: &str,
        body: &ModifyWebhookWithToken,
    ) -> Result<Webhook, DiscordError> {
        self.modify_webhook_with_token_typed(webhook_id, token, body)
            .await
    }

    /// Runs the `delete_webhook_with_token` operation.
    pub async fn delete_webhook_with_token(
        &self,
        webhook_id: impl Into<Snowflake>,
        token: &str,
    ) -> Result<(), DiscordError> {
        validate_token_path_segment("webhook_token", token, false)?;
        let path = format!("/webhooks/{}/{}", webhook_id.into(), token);
        self.request_no_content(Method::DELETE, &path, Option::<&Value>::None)
            .await
    }

    /// Runs the `get_guild_scheduled_events` operation.
    pub async fn get_guild_scheduled_events(
        &self,
        guild_id: impl Into<Snowflake>,
    ) -> Result<Vec<GuildScheduledEvent>, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/scheduled-events", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `create_guild_scheduled_event` operation.
    pub async fn create_guild_scheduled_event(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<GuildScheduledEvent, DiscordError> {
        self.request_typed(
            Method::POST,
            &format!("/guilds/{}/scheduled-events", guild_id.into()),
            Some(body),
        )
        .await
    }

    /// Runs the `create_guild_scheduled_event_typed` operation.
    pub async fn create_guild_scheduled_event_typed<B>(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<GuildScheduledEvent, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(
            Method::POST,
            &format!("/guilds/{}/scheduled-events", guild_id.into()),
            Some(body),
        )
        .await
    }

    /// Runs the `get_guild_scheduled_event` operation.
    pub async fn get_guild_scheduled_event(
        &self,
        guild_id: impl Into<Snowflake>,
        event_id: impl Into<Snowflake>,
    ) -> Result<GuildScheduledEvent, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!(
                "/guilds/{}/scheduled-events/{}",
                guild_id.into(),
                event_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `modify_guild_scheduled_event` operation.
    pub async fn modify_guild_scheduled_event(
        &self,
        guild_id: impl Into<Snowflake>,
        event_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<GuildScheduledEvent, DiscordError> {
        self.request_typed(
            Method::PATCH,
            &format!(
                "/guilds/{}/scheduled-events/{}",
                guild_id.into(),
                event_id.into()
            ),
            Some(body),
        )
        .await
    }

    /// Runs the `modify_guild_scheduled_event_typed` operation.
    pub async fn modify_guild_scheduled_event_typed<B>(
        &self,
        guild_id: impl Into<Snowflake>,
        event_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<GuildScheduledEvent, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(
            Method::PATCH,
            &format!(
                "/guilds/{}/scheduled-events/{}",
                guild_id.into(),
                event_id.into()
            ),
            Some(body),
        )
        .await
    }

    /// Runs the `delete_guild_scheduled_event` operation.
    pub async fn delete_guild_scheduled_event(
        &self,
        guild_id: impl Into<Snowflake>,
        event_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!(
                "/guilds/{}/scheduled-events/{}",
                guild_id.into(),
                event_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `get_guild_scheduled_event_users` operation.
    pub async fn get_guild_scheduled_event_users(
        &self,
        guild_id: impl Into<Snowflake>,
        event_id: impl Into<Snowflake>,
        limit: Option<u64>,
    ) -> Result<Vec<GuildScheduledEventUser>, DiscordError> {
        let path = match limit {
            Some(l) => format!(
                "/guilds/{}/scheduled-events/{}/users?limit={}",
                guild_id.into(),
                event_id.into(),
                l
            ),
            None => format!(
                "/guilds/{}/scheduled-events/{}/users",
                guild_id.into(),
                event_id.into()
            ),
        };
        self.request_typed(Method::GET, &path, Option::<&Value>::None)
            .await
    }

    /// Runs the `get_auto_moderation_rules` operation.
    pub async fn get_auto_moderation_rules(
        &self,
        guild_id: impl Into<Snowflake>,
    ) -> Result<Vec<serde_json::Value>, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/auto-moderation/rules", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `get_auto_moderation_rules_typed` operation.
    pub async fn get_auto_moderation_rules_typed(
        &self,
        guild_id: impl Into<Snowflake>,
    ) -> Result<Vec<AutoModerationRule>, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/auto-moderation/rules", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `get_auto_moderation_rule` operation.
    pub async fn get_auto_moderation_rule(
        &self,
        guild_id: impl Into<Snowflake>,
        rule_id: impl Into<Snowflake>,
    ) -> Result<AutoModerationRule, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!(
                "/guilds/{}/auto-moderation/rules/{}",
                guild_id.into(),
                rule_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `create_auto_moderation_rule` operation.
    pub async fn create_auto_moderation_rule(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<serde_json::Value, DiscordError> {
        self.request(
            Method::POST,
            &format!("/guilds/{}/auto-moderation/rules", guild_id.into()),
            Some(body),
        )
        .await
    }

    /// Runs the `create_auto_moderation_rule_typed` operation.
    pub async fn create_auto_moderation_rule_typed<B>(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<AutoModerationRule, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(
            Method::POST,
            &format!("/guilds/{}/auto-moderation/rules", guild_id.into()),
            Some(body),
        )
        .await
    }

    /// Runs the `modify_auto_moderation_rule` operation.
    pub async fn modify_auto_moderation_rule(
        &self,
        guild_id: impl Into<Snowflake>,
        rule_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<serde_json::Value, DiscordError> {
        self.request(
            Method::PATCH,
            &format!(
                "/guilds/{}/auto-moderation/rules/{}",
                guild_id.into(),
                rule_id.into()
            ),
            Some(body),
        )
        .await
    }

    /// Runs the `modify_auto_moderation_rule_typed` operation.
    pub async fn modify_auto_moderation_rule_typed<B>(
        &self,
        guild_id: impl Into<Snowflake>,
        rule_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<AutoModerationRule, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(
            Method::PATCH,
            &format!(
                "/guilds/{}/auto-moderation/rules/{}",
                guild_id.into(),
                rule_id.into()
            ),
            Some(body),
        )
        .await
    }

    /// Runs the `delete_auto_moderation_rule` operation.
    pub async fn delete_auto_moderation_rule(
        &self,
        guild_id: impl Into<Snowflake>,
        rule_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!(
                "/guilds/{}/auto-moderation/rules/{}",
                guild_id.into(),
                rule_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }

    /// Runs the `get_global_command` operation.
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

    /// Runs the `edit_global_command` operation.
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

    /// Runs the `delete_global_command` operation.
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

    /// Runs the `get_guild_commands` operation.
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

    /// Runs the `create_guild_command` operation.
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

    /// Runs the `get_guild_command` operation.
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

    /// Runs the `edit_guild_command` operation.
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

    /// Runs the `delete_guild_command` operation.
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

    /// Runs the `get_guild_preview` operation.
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

    /// Runs the `get_guild_preview_typed` operation.
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

    /// Runs the `get_guild_prune_count` operation.
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

    /// Runs the `get_guild_prune_count_typed` operation.
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

    /// Runs the `begin_guild_prune` operation.
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

    /// Runs the `begin_guild_prune_typed` operation.
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

    /// Runs the `begin_guild_prune_with_body` operation.
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

    /// Runs the `begin_guild_prune_with_request` operation.
    pub async fn begin_guild_prune_with_request(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &BeginGuildPruneRequest,
    ) -> Result<GuildPruneResult, DiscordError> {
        self.begin_guild_prune_with_body(guild_id, body).await
    }

    /// Runs the `get_guild_vanity_url` operation.
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

    /// Runs the `get_guild_widget_settings` operation.
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

    /// Runs the `modify_guild_widget_settings` operation.
    pub async fn modify_guild_widget_settings(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<GuildWidgetSettings, DiscordError> {
        self.modify_guild_widget_settings_typed(guild_id, body)
            .await
    }

    /// Runs the `modify_guild_widget_settings_typed` operation.
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

    /// Runs the `modify_guild_widget` operation.
    pub async fn modify_guild_widget(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &ModifyGuildWidgetSettings,
    ) -> Result<GuildWidgetSettings, DiscordError> {
        self.modify_guild_widget_settings_typed(guild_id, body)
            .await
    }

    /// Runs the `get_guild_widget` operation.
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

    /// Runs the `get_guild_widget_typed` operation.
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

    /// Runs the `follow_announcement_channel` operation.
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

    /// Runs the `create_stage_instance` operation.
    pub async fn create_stage_instance(&self, body: &Value) -> Result<StageInstance, DiscordError> {
        self.create_stage_instance_typed(body).await
    }

    /// Runs the `create_stage_instance_typed` operation.
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

    /// Runs the `create_stage_instance_from_request` operation.
    pub async fn create_stage_instance_from_request(
        &self,
        body: &CreateStageInstance,
    ) -> Result<StageInstance, DiscordError> {
        self.create_stage_instance_typed(body).await
    }

    /// Runs the `get_stage_instance` operation.
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

    /// Runs the `modify_stage_instance` operation.
    pub async fn modify_stage_instance(
        &self,
        channel_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<StageInstance, DiscordError> {
        self.modify_stage_instance_typed(channel_id, body).await
    }

    /// Runs the `modify_stage_instance_typed` operation.
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

    /// Runs the `modify_stage_instance_from_request` operation.
    pub async fn modify_stage_instance_from_request(
        &self,
        channel_id: impl Into<Snowflake>,
        body: &ModifyStageInstance,
    ) -> Result<StageInstance, DiscordError> {
        self.modify_stage_instance_typed(channel_id, body).await
    }

    /// Runs the `delete_stage_instance` operation.
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

    /// Runs the `get_guild_welcome_screen` operation.
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

    /// Runs the `modify_guild_welcome_screen` operation.
    pub async fn modify_guild_welcome_screen(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<WelcomeScreen, DiscordError> {
        self.modify_guild_welcome_screen_typed(guild_id, body).await
    }

    /// Runs the `modify_guild_welcome_screen_typed` operation.
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

    /// Runs the `modify_guild_welcome_screen_config` operation.
    pub async fn modify_guild_welcome_screen_config(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &ModifyGuildWelcomeScreen,
    ) -> Result<WelcomeScreen, DiscordError> {
        self.modify_guild_welcome_screen_typed(guild_id, body).await
    }

    /// Runs the `get_guild_onboarding` operation.
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

    /// Runs the `modify_guild_onboarding` operation.
    pub async fn modify_guild_onboarding(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<GuildOnboarding, DiscordError> {
        self.modify_guild_onboarding_typed(guild_id, body).await
    }

    /// Runs the `modify_guild_onboarding_typed` operation.
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

    /// Runs the `modify_guild_onboarding_config` operation.
    pub async fn modify_guild_onboarding_config(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &ModifyGuildOnboarding,
    ) -> Result<GuildOnboarding, DiscordError> {
        self.modify_guild_onboarding_typed(guild_id, body).await
    }

    /// Runs the `modify_guild_incident_actions` operation.
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

    /// Runs the `get_guild_templates` operation.
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

    /// Runs the `get_guild_template` operation by template code.
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

    /// Runs the `create_guild_template` operation.
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

    /// Runs the `sync_guild_template` operation.
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

    /// Runs the `modify_guild_template` operation.
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

    /// Runs the `delete_guild_template` operation.
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

    /// Runs the `get_voice_regions` operation.
    pub async fn get_voice_regions(&self) -> Result<Vec<serde_json::Value>, DiscordError> {
        self.request_typed(Method::GET, "/voice/regions", Option::<&Value>::None)
            .await
    }

    /// Runs the `get_voice_regions_typed` operation.
    pub async fn get_voice_regions_typed(&self) -> Result<Vec<VoiceRegion>, DiscordError> {
        self.request_typed(Method::GET, "/voice/regions", Option::<&Value>::None)
            .await
    }

    /// Runs the `get_guild_voice_regions` operation.
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

    /// Runs the `get_current_user_voice_state` operation.
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

    /// Runs the `get_user_voice_state` operation.
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

    /// Runs the `modify_current_user_voice_state` operation.
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

    /// Runs the `modify_current_user_voice_state_from_request` operation.
    pub async fn modify_current_user_voice_state_from_request(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &ModifyCurrentUserVoiceState,
    ) -> Result<(), DiscordError> {
        self.modify_current_user_voice_state(guild_id, body).await
    }

    /// Runs the `modify_user_voice_state` operation.
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

    /// Runs the `modify_user_voice_state_from_request` operation.
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

    /// Runs the `create_followup_message` operation.
    pub async fn create_followup_message(
        &self,
        interaction_token: &str,
        body: &CreateMessage,
    ) -> Result<Message, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.create_followup_message_with_application_id(&application_id, interaction_token, body)
            .await
    }

    /// Runs the `create_followup_message_with_files` operation.
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

    /// Runs the `create_followup_message_with_application_id` operation.
    pub async fn create_followup_message_with_application_id(
        &self,
        application_id: &str,
        interaction_token: &str,
        body: &CreateMessage,
    ) -> Result<Message, DiscordError> {
        let path = followup_webhook_path(application_id, interaction_token, None)?;
        self.request_typed(Method::POST, &path, Some(body)).await
    }

    /// Runs the `create_followup_message_with_application_id_and_files` operation.
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

    /// Runs the `get_original_interaction_response` operation.
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

    /// Runs the `get_original_interaction_response_with_application_id` operation.
    pub async fn get_original_interaction_response_with_application_id(
        &self,
        application_id: &str,
        interaction_token: &str,
    ) -> Result<Message, DiscordError> {
        let path = followup_webhook_path(application_id, interaction_token, Some("@original"))?;
        self.request_typed(Method::GET, &path, Option::<&Value>::None)
            .await
    }

    /// Runs the `edit_original_interaction_response` operation.
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

    /// Runs the `edit_original_interaction_response_with_files` operation.
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

    /// Runs the `edit_original_interaction_response_with_application_id` operation.
    pub async fn edit_original_interaction_response_with_application_id(
        &self,
        application_id: &str,
        interaction_token: &str,
        body: &CreateMessage,
    ) -> Result<Message, DiscordError> {
        let path = followup_webhook_path(application_id, interaction_token, Some("@original"))?;
        self.request_typed(Method::PATCH, &path, Some(body)).await
    }

    /// Runs the `edit_original_interaction_response_with_application_id_and_files` operation.
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

    /// Runs the `delete_original_interaction_response` operation.
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

    /// Runs the `delete_original_interaction_response_with_application_id` operation.
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

    /// Runs the `edit_followup_message` operation.
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

    /// Runs the `edit_followup_message_with_files` operation.
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

    /// Runs the `edit_followup_message_with_application_id` operation.
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

    /// Runs the `edit_followup_message_with_application_id_and_files` operation.
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

    /// Runs the `delete_followup_message` operation.
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

    /// Runs the `delete_followup_message_with_application_id` operation.
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

    /// Runs the `request` operation.
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

    /// Runs the `request_multipart` operation.
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

    /// Runs the `request_typed` operation.
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

    /// Runs the `request_typed_multipart` operation.
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
