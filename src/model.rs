use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::parsers::V2ModalSubmission;
use crate::types::Emoji;

mod channel;
mod permissions;
mod snowflake;
mod thread;
mod user;

pub use channel::{
    Attachment, Channel, ChannelType, CreateGuildChannel, EditChannelPermission,
    PermissionOverwrite,
};
pub use permissions::PermissionsBitField;
pub use snowflake::Snowflake;
pub use thread::{
    ArchivedThreadsQuery, JoinedArchivedThreadsQuery, ThreadListResponse, ThreadMember,
    ThreadMemberQuery,
};
pub use user::{
    AvatarDecorationData, ModifyCurrentUser, UpdateUserApplicationRoleConnection, User,
    UserApplicationRoleConnection, UserCollectibles, UserConnection, UserNameplate,
    UserPrimaryGuild,
};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `RoleTags`.
pub struct RoleTags {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integration_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub premium_subscriber: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
/// Typed Discord API object for `RoleColors`.
pub struct RoleColors {
    pub primary_color: u64,
    pub secondary_color: Option<u64>,
    pub tertiary_color: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Role`.
pub struct Role {
    pub id: Snowflake,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub colors: Option<RoleColors>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hoist: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub managed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mentionable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<PermissionsBitField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unicode_emoji: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<RoleTags>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
}

/// Lobby member object used by Discord's Lobby resource.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct LobbyMember {
    /// User ID of the lobby member.
    pub id: Snowflake,
    /// App-defined member metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
    /// Lobby member flag bitfield.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
}

/// Lobby object used by Discord matchmaking APIs.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Lobby {
    /// Lobby ID.
    pub id: Snowflake,
    /// Application that owns the lobby.
    pub application_id: Snowflake,
    /// App-defined lobby metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
    /// Current lobby members.
    #[serde(default)]
    pub members: Vec<LobbyMember>,
    /// Text channel linked to this lobby, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linked_channel: Option<Channel>,
}

/// Request body for creating a lobby.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CreateLobby {
    /// App-defined lobby metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
    /// Initial lobby members.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub members: Option<Vec<LobbyMember>>,
    /// Idle timeout in seconds before Discord shuts down an idle lobby.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idle_timeout_seconds: Option<u64>,
}

/// Request body for modifying a lobby.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyLobby {
    /// Replacement app-defined lobby metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
    /// Replacement lobby members.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub members: Option<Vec<LobbyMember>>,
    /// Idle timeout in seconds before Discord shuts down an idle lobby.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idle_timeout_seconds: Option<u64>,
}

/// Request body for adding or updating one lobby member.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct AddLobbyMember {
    /// App-defined member metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
    /// Lobby member flag bitfield.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
}

/// Request body entry for bulk lobby member updates.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct LobbyMemberUpdate {
    /// User ID to add, update, or remove.
    pub id: Snowflake,
    /// App-defined member metadata.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<HashMap<String, String>>,
    /// Lobby member flag bitfield.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
    /// Removes this member instead of upserting when true.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remove_member: Option<bool>,
}

/// Request body for linking or unlinking a channel from a lobby.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct LinkLobbyChannel {
    /// Channel to link. Omit this field to unlink any currently linked channel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<Snowflake>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `DefaultReaction`.
pub struct DefaultReaction {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ForumTag`.
pub struct ForumTag {
    pub id: Snowflake,
    pub name: String,
    #[serde(default)]
    pub moderated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Member`.
pub struct Member {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<User>,
    #[serde(default)]
    pub roles: Vec<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nick: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub joined_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<PermissionsBitField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deaf: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mute: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub premium_since: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub communication_disabled_until: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_decoration_data: Option<AvatarDecorationData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collectibles: Option<UserCollectibles>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `GetGuildQuery`.
pub struct GetGuildQuery {
    pub with_counts: Option<bool>,
}

/// Request body for adding an OAuth2-authorized user to a guild.
///
/// `access_token` must be a user OAuth2 token granted with the `guilds.join`
/// scope for the same application as the bot token used by the REST client.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct AddGuildMember {
    /// OAuth2 user access token with the `guilds.join` scope.
    pub access_token: String,
    /// Optional nickname to apply when the user joins.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nick: Option<String>,
    /// Optional initial guild roles for the member.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roles: Option<Vec<Snowflake>>,
    /// Whether the member should be muted in voice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mute: Option<bool>,
    /// Whether the member should be deafened in voice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deaf: Option<bool>,
}

/// Request body for modifying a guild.
///
/// The outer `Option` controls omission. Use `Some(None)` to send JSON `null`
/// for nullable Discord fields such as image data, linked channel IDs, or
/// description.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyGuild {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_level: Option<Option<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_message_notifications: Option<Option<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explicit_content_filter: Option<Option<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub afk_channel_id: Option<Option<Snowflake>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub afk_timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub splash: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discovery_splash: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banner: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_channel_id: Option<Option<Snowflake>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_channel_flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rules_channel_id: Option<Option<Snowflake>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_updates_channel_id: Option<Option<Snowflake>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_locale: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub features: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub premium_progress_bar_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safety_alerts_channel_id: Option<Option<Snowflake>>,
}

/// Request body for modifying a guild member.
///
/// The outer `Option` controls whether a field is omitted. Use `Some(None)` to
/// send JSON `null` for nullable Discord fields such as `channel_id` or
/// `communication_disabled_until`.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyGuildMember {
    /// New guild nickname, or `Some(None)` to clear it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nick: Option<Option<String>>,
    /// Replacement role IDs for the member.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roles: Option<Option<Vec<Snowflake>>>,
    /// Whether the member should be muted in voice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mute: Option<Option<bool>>,
    /// Whether the member should be deafened in voice.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deaf: Option<Option<bool>>,
    /// Voice channel to move the member to, or `Some(None)` to disconnect them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<Option<Snowflake>>,
    /// Timeout expiration timestamp, or `Some(None)` to remove the timeout.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub communication_disabled_until: Option<Option<String>>,
    /// Guild member flags bit set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<Option<u64>>,
}

/// Request body for modifying the current member in a guild.
///
/// The outer `Option` controls omission; `Some(None)` serializes JSON `null`
/// for fields Discord allows callers to clear.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyCurrentMember {
    /// New guild nickname, or `Some(None)` to clear it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nick: Option<Option<String>>,
    /// Data URI guild banner image, or `Some(None)` to clear it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banner: Option<Option<String>>,
    /// Data URI guild avatar image, or `Some(None)` to clear it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<Option<String>>,
    /// Guild member bio, or `Some(None)` to clear it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bio: Option<Option<String>>,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `GuildMembersQuery`.
pub struct GuildMembersQuery {
    pub limit: Option<u64>,
    pub after: Option<Snowflake>,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `SearchGuildMembersQuery`.
pub struct SearchGuildMembersQuery {
    pub query: String,
    pub limit: Option<u64>,
}

/// Request entry for moving or reparenting a guild channel.
///
/// The outer `Option` controls whether a field is omitted. Use `Some(None)` to
/// send JSON `null` for nullable Discord fields such as `parent_id`.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyGuildChannelPosition {
    /// Channel to modify.
    pub id: Snowflake,
    /// New sorting position, or `Some(None)` to send `null`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Option<i64>>,
    /// Whether to sync permission overwrites with the new parent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lock_permissions: Option<Option<bool>>,
    /// New parent category ID, or `Some(None)` to remove the parent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<Option<Snowflake>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `VoiceState`.
pub struct VoiceState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member: Option<Member>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default)]
    pub deaf: bool,
    #[serde(default)]
    pub mute: bool,
    #[serde(default)]
    pub self_deaf: bool,
    #[serde(default)]
    pub self_mute: bool,
    #[serde(default)]
    pub suppress: bool,
    #[serde(default)]
    pub self_stream: bool,
    #[serde(default)]
    pub self_video: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_to_speak_timestamp: Option<String>,
}

/// Request body for modifying the current user's guild voice state.
///
/// The outer `Option` controls omission. Use `Some(None)` to clear
/// `request_to_speak_timestamp`.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyCurrentUserVoiceState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suppress: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_to_speak_timestamp: Option<Option<String>>,
}

/// Request body for modifying another user's guild voice state.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyUserVoiceState {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suppress: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `VoiceServerUpdate`.
pub struct VoiceServerUpdate {
    pub guild_id: Snowflake,
    pub token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Guild`.
pub struct Guild {
    pub id: Snowflake,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable: Option<bool>,
    #[serde(default)]
    pub roles: Vec<Role>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub splash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub premium_tier: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_presences: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_members: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vanity_url_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_channel_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rules_channel_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_locale: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub afk_channel_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub afk_timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_level: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_message_notifications: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explicit_content_filter: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mfa_level: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_channel_flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_updates_channel_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nsfw_level: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub premium_subscription_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub premium_progress_bar_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safety_alerts_channel_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approximate_member_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approximate_presence_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incidents_data: Option<GuildIncidentsData>,
}

/// Active safety incident actions configured for a guild.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct GuildIncidentsData {
    /// Timestamp when invite creation will be re-enabled, or `null` when invite blocking is disabled.
    pub invites_disabled_until: Option<String>,
    /// Timestamp when direct messages will be re-enabled, or `null` when DM blocking is disabled.
    pub dms_disabled_until: Option<String>,
    /// Timestamp when Discord detected DM spam, if present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dm_spam_detected_at: Option<String>,
    /// Timestamp when Discord detected a raid, if present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raid_detected_at: Option<String>,
}

/// Request body for modifying guild incident actions.
///
/// Supplying `None` serializes `null`, which disables that incident action.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyGuildIncidentActions {
    /// Timestamp when invites should be re-enabled, or `null` to disable invite blocking.
    pub invites_disabled_until: Option<String>,
    /// Timestamp when direct messages should be re-enabled, or `null` to disable DM blocking.
    pub dms_disabled_until: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `CurrentUserGuild`.
pub struct CurrentUserGuild {
    pub id: Snowflake,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banner: Option<String>,
    #[serde(default)]
    pub owner: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<String>,
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approximate_member_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approximate_presence_count: Option<u64>,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `CurrentUserGuildsQuery`.
pub struct CurrentUserGuildsQuery {
    pub before: Option<Snowflake>,
    pub after: Option<Snowflake>,
    pub limit: Option<u64>,
    pub with_counts: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GuildPreview`.
pub struct GuildPreview {
    pub id: Snowflake,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub splash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discovery_splash: Option<String>,
    #[serde(default)]
    pub emojis: Vec<Emoji>,
    #[serde(default)]
    pub features: Vec<String>,
    pub approximate_member_count: u64,
    pub approximate_presence_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub stickers: Vec<Sticker>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `VanityUrl`.
pub struct VanityUrl {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uses: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GuildPruneCount`.
pub struct GuildPruneCount {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pruned: Option<u64>,
}

/// Type alias for `GuildPruneResult`.
pub type GuildPruneResult = GuildPruneCount;

/// Request body for beginning a guild prune.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct BeginGuildPruneRequest {
    /// Number of inactive days to prune, between 1 and 30.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub days: Option<u64>,
    /// Whether Discord should compute and return the prune count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compute_prune_count: Option<bool>,
    /// Role IDs to include in the prune.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_roles: Option<Vec<Snowflake>>,
    /// Deprecated Discord field for audit-log prune reason.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `BulkGuildBanRequest`.
pub struct BulkGuildBanRequest {
    pub user_ids: Vec<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete_message_seconds: Option<u64>,
}

/// Request body for banning a single guild member.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CreateGuildBan {
    /// Deprecated Discord field for message deletion duration in days.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete_message_days: Option<u64>,
    /// Message deletion duration in seconds, between 0 and 604800.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete_message_seconds: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `BulkGuildBanResponse`.
pub struct BulkGuildBanResponse {
    #[serde(default)]
    pub banned_users: Vec<Snowflake>,
    #[serde(default)]
    pub failed_users: Vec<Snowflake>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
/// Typed Discord API object for `ModifyGuildRolePosition`.
pub struct ModifyGuildRolePosition {
    pub id: Snowflake,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Option<i64>>,
}

/// Request body for creating a guild role.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CreateGuildRole {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<PermissionsBitField>,
    /// Deprecated integer RGB color. Prefer `colors` for new requests.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub colors: Option<RoleColors>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hoist: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unicode_emoji: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mentionable: Option<bool>,
}

/// Request body for modifying a guild role.
///
/// The outer `Option` controls omission. Use `Some(None)` to send JSON `null`
/// for nullable Discord fields such as `icon` or `unicode_emoji`.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyGuildRole {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Option<PermissionsBitField>>,
    /// Deprecated integer RGB color. Prefer `colors` for new requests.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<Option<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub colors: Option<Option<RoleColors>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hoist: Option<Option<bool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unicode_emoji: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mentionable: Option<Option<bool>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `VoiceRegion`.
pub struct VoiceRegion {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub optimal: bool,
    #[serde(default)]
    pub deprecated: bool,
    #[serde(default)]
    pub custom: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `AutoModerationRule`.
pub struct AutoModerationRule {
    pub id: Snowflake,
    pub guild_id: Snowflake,
    pub name: String,
    pub creator_id: Snowflake,
    pub event_type: u8,
    pub trigger_type: u8,
    #[serde(default)]
    pub trigger_metadata: AutoModerationTriggerMetadata,
    #[serde(default)]
    pub actions: Vec<AutoModerationAction>,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub exempt_roles: Vec<Snowflake>,
    #[serde(default)]
    pub exempt_channels: Vec<Snowflake>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `AutoModerationTriggerMetadata`.
pub struct AutoModerationTriggerMetadata {
    #[serde(default)]
    pub keyword_filter: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regex_patterns: Option<Vec<String>>,
    #[serde(default)]
    pub presets: Vec<u8>,
    #[serde(default)]
    pub allow_list: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mention_total_limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mention_raid_protection_enabled: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `AutoModerationAction`.
pub struct AutoModerationAction {
    #[serde(rename = "type")]
    pub kind: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<AutoModerationActionMetadata>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `AutoModerationActionMetadata`.
pub struct AutoModerationActionMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_message: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Message`.
pub struct Message {
    pub id: Snowflake,
    pub channel_id: Snowflake,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<User>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member: Option<Member>,
    #[serde(default)]
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edited_timestamp: Option<String>,
    #[serde(default)]
    pub mentions: Vec<User>,
    #[serde(default)]
    pub mention_roles: Vec<Snowflake>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pinned: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tts: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application: Option<serde_json::Value>,
    #[serde(default)]
    pub embeds: Vec<Embed>,
    #[serde(default)]
    pub reactions: Vec<Reaction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity: Option<MessageActivity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mention_everyone: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mention_channels: Option<Vec<ChannelMention>>,
    #[serde(default)]
    pub sticker_items: Vec<StickerItem>,
    #[serde(default)]
    pub stickers: Vec<Sticker>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role_subscription_data: Option<RoleSubscriptionData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_reference: Option<MessageReference>,
    #[serde(default)]
    pub message_snapshots: Vec<MessageSnapshot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub referenced_message: Option<Box<Message>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interaction_metadata: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interaction: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub components: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll: Option<Poll>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call: Option<MessageCall>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shared_client_theme: Option<SharedClientTheme>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `MessageActivity`.
pub struct MessageActivity {
    #[serde(rename = "type")]
    pub kind: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub party_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `MessageCall`.
pub struct MessageCall {
    #[serde(default)]
    pub participants: Vec<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_timestamp: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `MessageSnapshot`.
pub struct MessageSnapshot {
    pub message: MessageSnapshotMessage,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `MessageSnapshotMessage`.
pub struct MessageSnapshotMessage {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<u8>,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub embeds: Vec<Embed>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edited_timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
    #[serde(default)]
    pub mentions: Vec<User>,
    #[serde(default)]
    pub mention_roles: Vec<Snowflake>,
    #[serde(default)]
    pub stickers: Vec<Sticker>,
    #[serde(default)]
    pub sticker_items: Vec<StickerItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub components: Option<Vec<serde_json::Value>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `RoleSubscriptionData`.
pub struct RoleSubscriptionData {
    pub role_subscription_listing_id: Snowflake,
    pub tier_name: String,
    pub total_months_subscribed: u64,
    pub is_renewal: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `SharedClientTheme`.
pub struct SharedClientTheme {
    #[serde(default)]
    pub colors: Vec<String>,
    pub gradient_angle: u64,
    pub base_mix: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_theme: Option<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `MessagePin`.
pub struct MessagePin {
    pub pinned_at: String,
    pub message: Message,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ChannelPins`.
pub struct ChannelPins {
    #[serde(default)]
    pub items: Vec<MessagePin>,
    #[serde(default)]
    pub has_more: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
/// Typed Discord API object for `ChannelPinsQuery`.
pub struct ChannelPinsQuery {
    pub before: Option<String>,
    pub limit: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
/// Typed Discord API object for `AllowedMentions`.
pub struct AllowedMentions {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parse: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roles: Vec<Snowflake>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub users: Vec<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replied_user: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `SearchGuildMessagesResponse`.
pub struct SearchGuildMessagesResponse {
    #[serde(default)]
    pub doing_deep_historical_index: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documents_indexed: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_after: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default)]
    pub total_results: u64,
    #[serde(default)]
    pub messages: Vec<Vec<Message>>,
    #[serde(default)]
    pub threads: Vec<Channel>,
    #[serde(default)]
    pub members: Vec<ThreadMember>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
/// Typed Discord API object for `SearchGuildMessagesQuery`.
pub struct SearchGuildMessagesQuery {
    pub limit: Option<u64>,
    pub offset: Option<u64>,
    pub max_id: Option<Snowflake>,
    pub min_id: Option<Snowflake>,
    pub slop: Option<u64>,
    pub content: Option<String>,
    pub channel_ids: Vec<Snowflake>,
    pub author_types: Vec<String>,
    pub author_ids: Vec<Snowflake>,
    pub mentions: Vec<Snowflake>,
    pub mentions_role_ids: Vec<Snowflake>,
    pub mention_everyone: Option<bool>,
    pub replied_to_user_ids: Vec<Snowflake>,
    pub replied_to_message_ids: Vec<Snowflake>,
    pub pinned: Option<bool>,
    pub has: Vec<String>,
    pub embed_types: Vec<String>,
    pub embed_providers: Vec<String>,
    pub link_hostnames: Vec<String>,
    pub attachment_filenames: Vec<String>,
    pub attachment_extensions: Vec<String>,
    pub sort_by: Option<String>,
    pub sort_order: Option<String>,
    pub include_nsfw: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ApplicationCommandOptionChoice`.
pub struct ApplicationCommandOptionChoice {
    pub name: String,
    #[serde(default)]
    pub value: serde_json::Value,
}

impl ApplicationCommandOptionChoice {
    /// Creates a `new` value.
    pub fn new(name: impl Into<String>, value: impl Serialize) -> Self {
        Self {
            name: name.into(),
            value: serde_json::to_value(value).unwrap_or(serde_json::Value::Null),
        }
    }

    /// Creates a `try_new` value.
    pub fn try_new(
        name: impl Into<String>,
        value: impl Serialize,
    ) -> Result<Self, serde_json::Error> {
        Ok(Self {
            name: name.into(),
            value: serde_json::to_value(value)?,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `PollMedia`.
pub struct PollMedia {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji: Option<Emoji>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `PollAnswer`.
pub struct PollAnswer {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub answer_id: Option<u64>,
    pub poll_media: PollMedia,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `PollAnswerCount`.
pub struct PollAnswerCount {
    pub id: u64,
    pub count: u64,
    pub me_voted: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `PollResults`.
pub struct PollResults {
    pub is_finalized: bool,
    #[serde(default)]
    pub answer_counts: Vec<PollAnswerCount>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Poll`.
pub struct Poll {
    pub question: PollMedia,
    #[serde(default)]
    pub answers: Vec<PollAnswer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiry: Option<String>,
    #[serde(default)]
    pub allow_multiselect: bool,
    #[serde(default)]
    pub layout_type: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub results: Option<PollResults>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `PollAnswerVoters`.
pub struct PollAnswerVoters {
    #[serde(default)]
    pub users: Vec<User>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `CreatePoll`.
pub struct CreatePoll {
    pub question: PollMedia,
    #[serde(default)]
    pub answers: Vec<PollAnswer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_multiselect: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout_type: Option<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ChannelMention`.
pub struct ChannelMention {
    pub id: Snowflake,
    pub guild_id: Snowflake,
    #[serde(rename = "type")]
    pub kind: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `MessageReference`.
pub struct MessageReference {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fail_if_not_exists: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Ban`.
pub struct Ban {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<User>,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `GuildBansQuery`.
pub struct GuildBansQuery {
    pub limit: Option<u64>,
    pub before: Option<Snowflake>,
    pub after: Option<Snowflake>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Invite`.
pub struct Invite {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inviter: Option<User>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_type: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_user: Option<User>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_application: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approximate_presence_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approximate_member_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_scheduled_event: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roles: Vec<Role>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uses: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_age: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temporary: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
/// Typed Discord API object for `CreateChannelInvite`.
pub struct CreateChannelInvite {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_age: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temporary: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unique: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_type: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_user_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_application_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role_ids: Option<Vec<Snowflake>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
/// Typed Discord API object for `InviteTargetUsersJobStatus`.
pub struct InviteTargetUsersJobStatus {
    pub status: u8,
    pub total_users: u64,
    pub processed_users: u64,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
/// Typed Discord API object for `SetVoiceChannelStatus`.
pub struct SetVoiceChannelStatus {
    pub status: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Webhook`.
pub struct Webhook {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// Request body for creating an incoming channel webhook.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateWebhook {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<Option<String>>,
}

/// Request body for modifying a bot-authenticated webhook.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyWebhook {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<Snowflake>,
}

/// Request body for modifying a token-authenticated webhook.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyWebhookWithToken {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<Option<String>>,
}

/// Query options for executing a webhook.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct WebhookExecuteQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wait: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub with_components: Option<bool>,
}

/// Query options for webhook message get/edit/delete routes.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct WebhookMessageQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub with_components: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `AuditLogEntry`.
pub struct AuditLogEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_type: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changes: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `AuditLog`.
pub struct AuditLog {
    #[serde(default)]
    pub application_commands: Vec<ApplicationCommand>,
    #[serde(default)]
    pub audit_log_entries: Vec<AuditLogEntry>,
    #[serde(default)]
    pub auto_moderation_rules: Vec<AutoModerationRule>,
    #[serde(default)]
    pub guild_scheduled_events: Vec<GuildScheduledEvent>,
    #[serde(default)]
    pub integrations: Vec<Integration>,
    #[serde(default)]
    pub threads: Vec<Channel>,
    #[serde(default)]
    pub users: Vec<User>,
    #[serde(default)]
    pub webhooks: Vec<Webhook>,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `AuditLogQuery`.
pub struct AuditLogQuery {
    pub user_id: Option<Snowflake>,
    pub action_type: Option<u64>,
    pub before: Option<Snowflake>,
    pub after: Option<Snowflake>,
    pub limit: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ApplicationCommandOption`.
pub struct ApplicationCommandOption {
    #[serde(rename = "type")]
    pub kind: u8,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub autocomplete: Option<bool>,
    #[serde(default)]
    pub options: Vec<ApplicationCommandOption>,
    #[serde(default)]
    pub choices: Vec<ApplicationCommandOptionChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_length: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_length: Option<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
/// Typed Discord API object for `ApplicationIntegrationType`.
pub struct ApplicationIntegrationType(pub u8);

impl ApplicationIntegrationType {
    /// Public API item `GUILD_INSTALL`.
    pub const GUILD_INSTALL: Self = Self(0);
    /// Public API item `USER_INSTALL`.
    pub const USER_INSTALL: Self = Self(1);
}

impl Default for ApplicationIntegrationType {
    fn default() -> Self {
        Self::GUILD_INSTALL
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
/// Typed Discord API object for `InteractionContextType`.
pub struct InteractionContextType(pub u8);

impl InteractionContextType {
    /// Public API item `GUILD`.
    pub const GUILD: Self = Self(0);
    /// Public API item `BOT_DM`.
    pub const BOT_DM: Self = Self(1);
    /// Public API item `PRIVATE_CHANNEL`.
    pub const PRIVATE_CHANNEL: Self = Self(2);
}

impl Default for InteractionContextType {
    fn default() -> Self {
        Self::GUILD
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
/// Typed Discord API object for `ApplicationCommandHandlerType`.
pub struct ApplicationCommandHandlerType(pub u8);

impl ApplicationCommandHandlerType {
    /// Public API item `APP_HANDLER`.
    pub const APP_HANDLER: Self = Self(1);
    /// Public API item `DISCORD_LAUNCH_ACTIVITY`.
    pub const DISCORD_LAUNCH_ACTIVITY: Self = Self(2);
}

impl Default for ApplicationCommandHandlerType {
    fn default() -> Self {
        Self::APP_HANDLER
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ApplicationCommand`.
pub struct ApplicationCommand {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<Snowflake>,
    #[serde(rename = "type")]
    pub kind: u8,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_localizations: Option<HashMap<String, String>>,
    #[serde(default)]
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description_localizations: Option<HashMap<String, String>>,
    #[serde(default)]
    pub options: Vec<ApplicationCommandOption>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_member_permissions: Option<PermissionsBitField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dm_permission: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integration_types: Option<Vec<ApplicationIntegrationType>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contexts: Option<Vec<InteractionContextType>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handler: Option<ApplicationCommandHandlerType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nsfw: Option<bool>,
}

/// A running application Activity instance.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ActivityInstance {
    /// Application ID that owns the Activity.
    pub application_id: Snowflake,
    /// Activity instance ID.
    pub instance_id: String,
    /// Unique launch ID for this Activity session.
    pub launch_id: Snowflake,
    /// Location where the Activity instance is running.
    pub location: ActivityLocation,
    /// Users currently connected to the Activity instance.
    #[serde(default)]
    pub users: Vec<Snowflake>,
}

/// Location where an application Activity instance is running.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ActivityLocation {
    /// Unique location ID.
    pub id: String,
    /// Location kind, such as `gc` for guild channel or `pc` for private channel.
    pub kind: String,
    /// Channel ID for this location.
    pub channel_id: Snowflake,
    /// Guild ID for guild-channel Activity locations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<Snowflake>,
}

/// A single application command permission overwrite.
///
/// Discord accepts role, user, and channel overwrites for a guild command.
/// The `id` can also be one of Discord's documented permission constants,
/// such as the guild ID for `@everyone`.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ApplicationCommandPermission {
    /// Role, user, or channel ID targeted by the overwrite.
    pub id: Snowflake,
    /// Discord permission target type.
    #[serde(rename = "type")]
    pub kind: u8,
    /// Whether this target is allowed to use the command.
    pub permission: bool,
}

impl ApplicationCommandPermission {
    /// Permission overwrite target type for roles.
    pub const ROLE: u8 = 1;
    /// Permission overwrite target type for users.
    pub const USER: u8 = 2;
    /// Permission overwrite target type for channels.
    pub const CHANNEL: u8 = 3;

    /// Creates a role permission overwrite.
    pub fn role(role_id: impl Into<Snowflake>, permission: bool) -> Self {
        Self {
            id: role_id.into(),
            kind: Self::ROLE,
            permission,
        }
    }

    /// Creates a user permission overwrite.
    pub fn user(user_id: impl Into<Snowflake>, permission: bool) -> Self {
        Self {
            id: user_id.into(),
            kind: Self::USER,
            permission,
        }
    }

    /// Creates a channel permission overwrite.
    pub fn channel(channel_id: impl Into<Snowflake>, permission: bool) -> Self {
        Self {
            id: channel_id.into(),
            kind: Self::CHANNEL,
            permission,
        }
    }
}

/// Permissions returned for one application command in one guild.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct GuildApplicationCommandPermissions {
    /// Command ID.
    pub id: Snowflake,
    /// Application ID that owns the command.
    pub application_id: Snowflake,
    /// Guild ID where the permissions apply.
    pub guild_id: Snowflake,
    /// Permission overwrites attached to the command.
    #[serde(default)]
    pub permissions: Vec<ApplicationCommandPermission>,
}

/// Request body for editing a guild application command's permissions.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct EditApplicationCommandPermissions {
    /// Permission overwrites to replace on the command.
    pub permissions: Vec<ApplicationCommandPermission>,
}

impl EditApplicationCommandPermissions {
    /// Creates a permission edit body from an iterator of overwrites.
    pub fn new(permissions: impl IntoIterator<Item = ApplicationCommandPermission>) -> Self {
        Self {
            permissions: permissions.into_iter().collect(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Application`.
pub struct Application {
    pub id: Snowflake,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub description: String,
    #[serde(default)]
    pub rpc_origins: Vec<String>,
    #[serde(default)]
    pub bot_public: bool,
    #[serde(default)]
    pub bot_require_code_grant: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terms_of_service_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub privacy_policy_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<User>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verify_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_sku_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approximate_guild_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approximate_user_install_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approximate_user_authorization_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect_uris: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interactions_endpoint_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role_connections_verification_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_webhooks_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_webhooks_status: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_webhooks_types: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_install_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_params: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integration_types_config: Option<HashMap<String, serde_json::Value>>,
}

/// Default OAuth2 install settings for an application install context.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApplicationInstallParams {
    pub scopes: Vec<String>,
    pub permissions: PermissionsBitField,
}

/// Default install settings for one application integration type.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ApplicationIntegrationTypeConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth2_install_params: Option<ApplicationInstallParams>,
}

/// Request body for editing the current application.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyCurrentApplication {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_install_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role_connections_verification_url: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_params: Option<ApplicationInstallParams>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integration_types_config: Option<HashMap<String, ApplicationIntegrationTypeConfig>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_image: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interactions_endpoint_url: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_webhooks_url: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_webhooks_status: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_webhooks_types: Option<Vec<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ApplicationRoleConnectionMetadata`.
pub struct ApplicationRoleConnectionMetadata {
    #[serde(rename = "type")]
    pub kind: u8,
    pub key: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_localizations: Option<HashMap<String, String>>,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description_localizations: Option<HashMap<String, String>>,
}

impl ApplicationCommand {
    /// Returns the command ID when Discord has assigned one.
    pub fn id_opt(&self) -> Option<&Snowflake> {
        self.id.as_ref()
    }

    /// Returns the creation timestamp once Discord has assigned an ID.
    pub fn created_at(&self) -> Option<u64> {
        self.id_opt().and_then(Snowflake::timestamp)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `InteractionContextData`.
pub struct InteractionContextData {
    pub id: Snowflake,
    pub application_id: Snowflake,
    pub token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<User>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member: Option<Member>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_permissions: Option<PermissionsBitField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_locale: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entitlements: Option<Vec<Entitlement>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<InteractionContextType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorizing_integration_owners: Option<HashMap<String, Snowflake>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `CommandInteractionOption`.
pub struct CommandInteractionOption {
    #[serde(rename = "type")]
    pub kind: u8,
    pub name: String,
    #[serde(default)]
    pub options: Vec<CommandInteractionOption>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focused: Option<bool>,
}

impl CommandInteractionOption {
    pub fn is_focused(&self) -> bool {
        self.focused.unwrap_or(false)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `CommandInteractionData`.
pub struct CommandInteractionData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<u8>,
    #[serde(default)]
    pub options: Vec<CommandInteractionOption>,
    #[serde(default)]
    pub resolved: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_id: Option<Snowflake>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ComponentInteractionData`.
pub struct ComponentInteractionData {
    pub custom_id: String,
    pub component_type: u8,
    #[serde(default)]
    pub values: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ChatInputCommandInteraction`.
pub struct ChatInputCommandInteraction {
    pub context: InteractionContextData,
    pub data: CommandInteractionData,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `UserContextMenuInteraction`.
pub struct UserContextMenuInteraction {
    pub context: InteractionContextData,
    pub data: CommandInteractionData,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `MessageContextMenuInteraction`.
pub struct MessageContextMenuInteraction {
    pub context: InteractionContextData,
    pub data: CommandInteractionData,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `AutocompleteInteraction`.
pub struct AutocompleteInteraction {
    pub context: InteractionContextData,
    pub data: CommandInteractionData,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ComponentInteraction`.
pub struct ComponentInteraction {
    pub context: InteractionContextData,
    pub data: ComponentInteractionData,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `ModalSubmitInteraction`.
pub struct ModalSubmitInteraction {
    pub context: InteractionContextData,
    pub submission: V2ModalSubmission,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `PingInteraction`.
pub struct PingInteraction {
    pub context: InteractionContextData,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[non_exhaustive]
/// Typed Discord API enum for `Interaction`.
pub enum Interaction {
    /// Discord API enum variant `Ping`.
    Ping(PingInteraction),
    /// Discord API enum variant `ChatInputCommand`.
    ChatInputCommand(ChatInputCommandInteraction),
    /// Discord API enum variant `UserContextMenu`.
    UserContextMenu(UserContextMenuInteraction),
    /// Discord API enum variant `MessageContextMenu`.
    MessageContextMenu(MessageContextMenuInteraction),
    /// Discord API enum variant `Autocomplete`.
    Autocomplete(AutocompleteInteraction),
    /// Discord API enum variant `Component`.
    Component(ComponentInteraction),
    /// Discord API enum variant `ModalSubmit`.
    ModalSubmit(ModalSubmitInteraction),
    /// Discord API enum variant `Unknown`.
    Unknown {
        /// Parsed interaction context shared with known variants.
        context: InteractionContextData,
        /// Raw Discord interaction type value.
        kind: u8,
        /// Raw interaction data object for unsupported variants.
        raw_data: serde_json::Value,
    },
}

impl Interaction {
    pub fn context(&self) -> &InteractionContextData {
        match self {
            Interaction::Ping(interaction) => &interaction.context,
            Interaction::ChatInputCommand(interaction) => &interaction.context,
            Interaction::UserContextMenu(interaction) => &interaction.context,
            Interaction::MessageContextMenu(interaction) => &interaction.context,
            Interaction::Autocomplete(interaction) => &interaction.context,
            Interaction::Component(interaction) => &interaction.context,
            Interaction::ModalSubmit(interaction) => &interaction.context,
            Interaction::Unknown { context, .. } => context,
        }
    }

    pub fn id(&self) -> &Snowflake {
        &self.context().id
    }

    pub fn token(&self) -> &str {
        &self.context().token
    }

    pub fn application_id(&self) -> &Snowflake {
        &self.context().application_id
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `InteractionCallbackResponse`.
pub struct InteractionCallbackResponse {
    #[serde(rename = "type")]
    pub kind: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
/// Typed Discord API object for `ReactionCountDetails`.
pub struct ReactionCountDetails {
    #[serde(default)]
    pub burst: u64,
    #[serde(default)]
    pub normal: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `CreateMessage`.
pub struct CreateMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub components: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embeds: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_mentions: Option<AllowedMentions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_reference: Option<MessageReference>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sticker_ids: Option<Vec<Snowflake>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub poll: Option<CreatePoll>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub enforce_nonce: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `CreateDmChannel`.
pub struct CreateDmChannel {
    pub recipient_id: Snowflake,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
/// Typed Discord API object for `CreateGroupDmChannel`.
pub struct CreateGroupDmChannel {
    pub access_tokens: Vec<String>,
    pub nicks: HashMap<Snowflake, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
/// Typed Discord API object for `AddGroupDmRecipient`.
pub struct AddGroupDmRecipient {
    pub access_token: String,
    pub nick: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `SessionStartLimit`.
pub struct SessionStartLimit {
    pub total: u32,
    pub remaining: u32,
    pub reset_after: u64,
    pub max_concurrency: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Gateway`.
pub struct Gateway {
    pub url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GatewayBot`.
pub struct GatewayBot {
    pub url: String,
    pub shards: u32,
    pub session_start_limit: SessionStartLimit,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for current OAuth2 authorization information.
pub struct AuthorizationInformation {
    /// Partial Discord application object for the current authorization.
    pub application: serde_json::Value,
    /// OAuth2 scopes granted to the authorization.
    #[serde(default)]
    pub scopes: Vec<String>,
    pub expires: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// User that authorized the application when the token includes `identify`.
    pub user: Option<User>,
}

fn is_false(v: &bool) -> bool {
    !v
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `EmbedField`.
pub struct EmbedField {
    pub name: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub inline: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `EmbedAuthor`.
pub struct EmbedAuthor {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `EmbedFooter`.
pub struct EmbedFooter {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `EmbedMedia`.
pub struct EmbedMedia {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxy_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Embed`.
pub struct Embed {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub footer: Option<EmbedFooter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<EmbedMedia>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<EmbedMedia>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video: Option<EmbedMedia>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<EmbedAuthor>,
    #[serde(default)]
    pub fields: Vec<EmbedField>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Reaction`.
pub struct Reaction {
    #[serde(default)]
    pub count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count_details: Option<ReactionCountDetails>,
    #[serde(default)]
    pub me: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji: Option<Emoji>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `StickerItem`.
pub struct StickerItem {
    pub id: Snowflake,
    pub name: String,
    #[serde(rename = "format_type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Sticker`.
pub struct Sticker {
    pub id: Snowflake,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pack_id: Option<Snowflake>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: String,
    #[serde(rename = "type")]
    pub kind: u8,
    pub format_type: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<User>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_value: Option<u64>,
}

/// Multipart form fields for creating a guild sticker.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CreateGuildSticker {
    pub name: String,
    pub description: String,
    pub tags: String,
}

/// JSON request body for modifying a guild sticker.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyGuildSticker {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `StickerPack`.
pub struct StickerPack {
    pub id: Snowflake,
    #[serde(default)]
    pub stickers: Vec<Sticker>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sku_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_sticker_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banner_asset_id: Option<Snowflake>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `StickerPackList`.
pub struct StickerPackList {
    #[serde(default)]
    pub sticker_packs: Vec<StickerPack>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `StageInstance`.
pub struct StageInstance {
    pub id: Snowflake,
    pub guild_id: Snowflake,
    pub channel_id: Snowflake,
    pub topic: String,
    pub privacy_level: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discoverable_disabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_scheduled_event_id: Option<Snowflake>,
}

/// Request body for creating a stage instance.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CreateStageInstance {
    pub channel_id: Snowflake,
    pub topic: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub privacy_level: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub send_start_notification: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_scheduled_event_id: Option<Snowflake>,
}

/// Request body for modifying a stage instance.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyStageInstance {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub privacy_level: Option<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GuildScheduledEvent`.
pub struct GuildScheduledEvent {
    pub id: Snowflake,
    pub guild_id: Snowflake,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator_id: Option<Snowflake>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub scheduled_start_time: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled_end_time: Option<String>,
    pub privacy_level: u8,
    pub status: u8,
    pub entity_type: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_metadata: Option<GuildScheduledEventEntityMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator: Option<User>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recurrence_rule: Option<GuildScheduledEventRecurrenceRule>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GuildScheduledEventEntityMetadata`.
pub struct GuildScheduledEventEntityMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GuildScheduledEventRecurrenceRuleNWeekday`.
pub struct GuildScheduledEventRecurrenceRuleNWeekday {
    pub n: i8,
    pub day: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GuildScheduledEventRecurrenceRule`.
pub struct GuildScheduledEventRecurrenceRule {
    pub start: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<String>,
    pub frequency: u8,
    pub interval: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_weekday: Option<Vec<u8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_n_weekday: Option<Vec<GuildScheduledEventRecurrenceRuleNWeekday>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_month: Option<Vec<u8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_month_day: Option<Vec<u8>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_year_day: Option<Vec<u16>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GuildScheduledEventUser`.
pub struct GuildScheduledEventUser {
    pub guild_scheduled_event_id: Snowflake,
    pub user: User,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member: Option<Member>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Sku`.
pub struct Sku {
    pub id: Snowflake,
    #[serde(rename = "type")]
    pub kind: u8,
    pub application_id: Snowflake,
    pub name: String,
    pub slug: String,
    pub flags: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependent_sku_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_labels: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_type: Option<u8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub premium: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_age_gate: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Entitlement`.
pub struct Entitlement {
    pub id: Snowflake,
    pub sku_id: Snowflake,
    pub application_id: Snowflake,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub promotion_id: Option<Snowflake>,
    #[serde(rename = "type")]
    pub kind: u8,
    pub deleted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gift_code_flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub starts_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ends_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consumed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscription_id: Option<Snowflake>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `CreateTestEntitlement`.
pub struct CreateTestEntitlement {
    pub sku_id: Snowflake,
    pub owner_id: Snowflake,
    pub owner_type: u8,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `EntitlementQuery`.
pub struct EntitlementQuery {
    pub user_id: Option<Snowflake>,
    pub sku_ids: Vec<Snowflake>,
    pub before: Option<Snowflake>,
    pub after: Option<Snowflake>,
    pub limit: Option<u64>,
    pub guild_id: Option<Snowflake>,
    pub exclude_ended: Option<bool>,
    pub exclude_deleted: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Subscription`.
pub struct Subscription {
    pub id: Snowflake,
    pub user_id: Snowflake,
    #[serde(default)]
    pub sku_ids: Vec<Snowflake>,
    #[serde(default)]
    pub entitlement_ids: Vec<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub renewal_sku_ids: Option<Vec<Snowflake>>,
    pub current_period_start: String,
    pub current_period_end: String,
    pub status: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canceled_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `SubscriptionQuery`.
pub struct SubscriptionQuery {
    pub before: Option<Snowflake>,
    pub after: Option<Snowflake>,
    pub limit: Option<u64>,
    pub user_id: Option<Snowflake>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `IntegrationAccount`.
pub struct IntegrationAccount {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `IntegrationApplication`.
pub struct IntegrationApplication {
    pub id: Snowflake,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot: Option<User>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Integration`.
pub struct Integration {
    pub id: Snowflake,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub syncing: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_emoticons: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expire_behavior: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expire_grace_period: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<User>,
    pub account: IntegrationAccount,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub synced_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscriber_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revoked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application: Option<IntegrationApplication>,
    #[serde(default)]
    pub scopes: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `SoundboardSound`.
pub struct SoundboardSound {
    pub name: String,
    pub sound_id: Snowflake,
    pub volume: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<Snowflake>,
    #[serde(default)]
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<User>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `SoundboardSoundList`.
pub struct SoundboardSoundList {
    #[serde(default)]
    pub items: Vec<SoundboardSound>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GuildWidgetSettings`.
pub struct GuildWidgetSettings {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<Snowflake>,
}

/// Request body for modifying guild widget settings.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyGuildWidgetSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<Option<Snowflake>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GuildWidgetChannel`.
pub struct GuildWidgetChannel {
    pub id: Snowflake,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GuildWidgetMember`.
pub struct GuildWidgetMember {
    pub id: Snowflake,
    pub username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discriminator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GuildWidget`.
pub struct GuildWidget {
    pub id: Snowflake,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instant_invite: Option<String>,
    #[serde(default)]
    pub channels: Vec<GuildWidgetChannel>,
    #[serde(default)]
    pub members: Vec<GuildWidgetMember>,
    #[serde(default)]
    pub presence_count: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Typed Discord API enum for `GuildWidgetImageStyle`.
pub enum GuildWidgetImageStyle {
    /// Discord API enum variant `Shield`.
    Shield,
    /// Discord API enum variant `Banner1`.
    Banner1,
    /// Discord API enum variant `Banner2`.
    Banner2,
    /// Discord API enum variant `Banner3`.
    Banner3,
    /// Discord API enum variant `Banner4`.
    Banner4,
}

impl GuildWidgetImageStyle {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Shield => "shield",
            Self::Banner1 => "banner1",
            Self::Banner2 => "banner2",
            Self::Banner3 => "banner3",
            Self::Banner4 => "banner4",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `FollowedChannel`.
pub struct FollowedChannel {
    pub channel_id: Snowflake,
    pub webhook_id: Snowflake,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
/// Typed Discord API object for `WelcomeScreenChannel`.
pub struct WelcomeScreenChannel {
    pub channel_id: Snowflake,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `WelcomeScreen`.
pub struct WelcomeScreen {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub welcome_channels: Vec<WelcomeScreenChannel>,
}

/// Request body for modifying a guild welcome screen.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyGuildWelcomeScreen {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<Option<bool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub welcome_channels: Option<Option<Vec<WelcomeScreenChannel>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<Option<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GuildTemplate`.
pub struct GuildTemplate {
    pub code: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub usage_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator: Option<User>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_guild_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serialized_source_guild: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_dirty: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GuildOnboarding`.
pub struct GuildOnboarding {
    pub guild_id: Snowflake,
    #[serde(default)]
    pub prompts: Vec<serde_json::Value>,
    #[serde(default)]
    pub default_channel_ids: Vec<Snowflake>,
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<u8>,
}

/// Request body for modifying guild onboarding.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
pub struct ModifyGuildOnboarding {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompts: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_channel_ids: Option<Vec<Snowflake>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Presence`.
pub struct Presence {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activities: Option<Vec<Activity>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_status: Option<ClientStatus>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
/// Typed Discord API object for `ClientStatus`.
pub struct ClientStatus {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desktop: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mobile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
/// Typed Discord API object for `ActivityType`.
pub struct ActivityType(pub u8);

impl ActivityType {
    /// Public API item `PLAYING`.
    pub const PLAYING: Self = Self(0);
    /// Public API item `STREAMING`.
    pub const STREAMING: Self = Self(1);
    /// Public API item `LISTENING`.
    pub const LISTENING: Self = Self(2);
    /// Public API item `WATCHING`.
    pub const WATCHING: Self = Self(3);
    /// Public API item `CUSTOM`.
    pub const CUSTOM: Self = Self(4);
    /// Public API item `COMPETING`.
    pub const COMPETING: Self = Self(5);
}

impl Default for ActivityType {
    fn default() -> Self {
        Self::PLAYING
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ActivityTimestamps`.
pub struct ActivityTimestamps {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ActivityParty`.
pub struct ActivityParty {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<Vec<u64>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ActivityAssets`.
pub struct ActivityAssets {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub large_image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub large_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub small_image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub small_text: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ActivitySecrets`.
pub struct ActivitySecrets {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub join: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spectate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_secret: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ActivityButton`.
pub struct ActivityButton {
    pub label: String,
    pub url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Activity`.
pub struct Activity {
    pub name: String,
    #[serde(default, rename = "type")]
    pub kind: ActivityType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamps: Option<ActivityTimestamps>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji: Option<Emoji>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub party: Option<ActivityParty>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assets: Option<ActivityAssets>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secrets: Option<ActivitySecrets>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buttons: Option<Vec<ActivityButton>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `UpdatePresence`.
pub struct UpdatePresence {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<u64>,
    #[serde(default)]
    pub activities: Vec<Activity>,
    pub status: String,
    #[serde(default)]
    pub afk: bool,
}

impl UpdatePresence {
    /// Creates a `online_with_activity` value.
    pub fn online_with_activity(name: impl Into<String>) -> Self {
        Self {
            since: None,
            activities: vec![Activity {
                name: name.into(),
                kind: ActivityType::PLAYING,
                ..Activity::default()
            }],
            status: "online".to_string(),
            afk: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `RequestGuildMembers`.
pub struct RequestGuildMembers {
    pub guild_id: Snowflake,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presences: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_ids: Option<Vec<Snowflake>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
}

/// Gateway request for ephemeral channel metadata.
///
/// Discord currently documents `status` and `voice_start_time` as requestable
/// fields for voice-channel metadata.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RequestChannelInfo {
    /// Guild whose channel metadata should be requested.
    pub guild_id: Snowflake,
    /// Metadata fields to request.
    pub fields: Vec<String>,
}

impl RequestChannelInfo {
    /// Channel status field name.
    pub const STATUS: &'static str = "status";
    /// Voice channel start-time field name.
    pub const VOICE_START_TIME: &'static str = "voice_start_time";

    /// Creates a channel-info request for explicit field names.
    pub fn new<I, S>(guild_id: impl Into<Snowflake>, fields: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            guild_id: guild_id.into(),
            fields: fields.into_iter().map(Into::into).collect(),
        }
    }

    /// Creates a request for Discord's documented voice metadata fields.
    pub fn voice_metadata(guild_id: impl Into<Snowflake>) -> Self {
        Self {
            guild_id: guild_id.into(),
            fields: vec![Self::STATUS.to_string(), Self::VOICE_START_TIME.to_string()],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `ThreadMetadata`.
pub struct ThreadMetadata {
    #[serde(default)]
    pub archived: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_archive_duration: Option<u64>,
    #[serde(default)]
    pub locked: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invitable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_timestamp: Option<String>,
}

// --- DiscordModel trait ---

/// Trait for all Discord data models that have a Snowflake ID.
///
/// Parallels discord.js's `Base` class, providing a common interface
/// for ID access and creation timestamp extraction.
pub trait DiscordModel: Send + Sync + 'static {
    /// Returns the Snowflake ID of this model.
    fn id(&self) -> &Snowflake;

    /// Returns the Snowflake ID when the model has one.
    ///
    /// Most Discord models always carry an ID, so the default implementation
    /// simply delegates to [`DiscordModel::id`]. Models that can exist before
    /// Discord assigns an ID, such as `ApplicationCommand`, override this.
    fn id_opt(&self) -> Option<&Snowflake> {
        Some(self.id())
    }

    /// Returns the creation timestamp as Unix milliseconds, extracted from the Snowflake ID.
    fn created_at(&self) -> Option<u64> {
        self.id_opt().and_then(Snowflake::timestamp)
    }
}

impl DiscordModel for User {
    fn id(&self) -> &Snowflake {
        &self.id
    }
}

impl DiscordModel for Guild {
    fn id(&self) -> &Snowflake {
        &self.id
    }
}

impl DiscordModel for Channel {
    fn id(&self) -> &Snowflake {
        &self.id
    }
}

impl DiscordModel for Message {
    fn id(&self) -> &Snowflake {
        &self.id
    }
}

impl DiscordModel for Role {
    fn id(&self) -> &Snowflake {
        &self.id
    }
}

impl DiscordModel for Attachment {
    fn id(&self) -> &Snowflake {
        &self.id
    }
}

#[cfg(test)]
#[path = "model/tests.rs"]
mod tests;
