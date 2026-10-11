//! Guild members and member-management request bodies.

use serde::{Deserialize, Serialize};

use super::{AvatarDecorationData, PermissionsBitField, Snowflake, User, UserCollectibles};

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
