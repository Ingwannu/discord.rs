use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{GenericId, PermissionsBitField, ThreadMetadata};

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `Attachment`.
pub struct Attachment {
    pub id: GenericId,
    pub filename: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ephemeral: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub waveform: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
#[non_exhaustive]
/// Typed Discord API enum for `ChannelType`.
pub enum ChannelType {
    /// Discord API enum variant `Text`.
    Text = 0,
    /// Discord API enum variant `Dm`.
    Dm = 1,
    /// Discord API enum variant `Voice`.
    Voice = 2,
    /// Discord API enum variant `GroupDm`.
    GroupDm = 3,
    /// Discord API enum variant `Category`.
    Category = 4,
    /// Discord API enum variant `News`.
    News = 5,
    /// Discord API enum variant `StageVoice`.
    StageVoice = 13,
    /// Discord API enum variant `GuildDirectory`.
    GuildDirectory = 14,
    /// Discord API enum variant `GuildForum`.
    GuildForum = 15,
    /// Discord API enum variant `GuildMedia`.
    GuildMedia = 16,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `PermissionOverwrite`.
pub struct PermissionOverwrite {
    pub id: GenericId,
    #[serde(rename = "type")]
    pub kind: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow: Option<PermissionsBitField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deny: Option<PermissionsBitField>,
}

/// Request body for creating a guild channel.
///
/// The outer `Option` controls omission. Use `Some(None)` to send JSON `null`
/// for nullable Discord fields such as `parent_id`, `rtc_region`, or
/// `default_reaction_emoji`.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct CreateGuildChannel {
    pub name: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitrate: Option<Option<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_limit: Option<Option<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limit_per_user: Option<Option<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Option<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_overwrites: Option<Option<Vec<PermissionOverwrite>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<Option<GenericId>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nsfw: Option<Option<bool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rtc_region: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_quality_mode: Option<Option<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_auto_archive_duration: Option<Option<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_reaction_emoji: Option<Option<DefaultReaction>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_tags: Option<Option<Vec<ForumTag>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_sort_order: Option<Option<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_forum_layout: Option<Option<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_thread_rate_limit_per_user: Option<Option<u64>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
/// Typed Discord API object for `EditChannelPermission`.
pub struct EditChannelPermission {
    #[serde(rename = "type")]
    pub kind: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow: Option<PermissionsBitField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deny: Option<PermissionsBitField>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `Channel`.
pub struct Channel {
    pub id: GenericId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<GenericId>,
    #[serde(rename = "type")]
    pub kind: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<GenericId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nsfw: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitrate: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limit_per_user: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_message_id: Option<GenericId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<GenericId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application_id: Option<GenericId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_overwrites: Option<Vec<PermissionOverwrite>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rtc_region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_quality_mode: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_auto_archive_duration: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_pin_timestamp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_metadata: Option<ThreadMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_message_sent: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_tags: Option<Vec<ForumTag>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applied_tags: Option<Vec<GenericId>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_reaction_emoji: Option<DefaultReaction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_thread_rate_limit_per_user: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_sort_order: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_forum_layout: Option<u64>,
}

impl Channel {
    /// A channel known only by ID, guild, and type, such as the channel of a
    /// DM message or of a component interaction. Every other field is
    /// unknown (`None`).
    ///
    /// Crate-private: `Channel` has no `Default` because its ID is
    /// required, and the cache needs these stubs to tie entries to an owner
    /// (see the gateway cache updates). Listing every field here, rather
    /// than a `..` fill, makes a newly added field a compile error until its
    /// stub value is chosen.
    #[cfg(feature = "gateway")]
    pub(crate) fn stub(id: GenericId, guild_id: Option<GenericId>, kind: u8) -> Self {
        Self {
            id,
            guild_id,
            kind,
            name: None,
            topic: None,
            parent_id: None,
            nsfw: None,
            bitrate: None,
            user_limit: None,
            rate_limit_per_user: None,
            last_message_id: None,
            owner_id: None,
            application_id: None,
            position: None,
            permission_overwrites: None,
            icon: None,
            rtc_region: None,
            video_quality_mode: None,
            default_auto_archive_duration: None,
            flags: None,
            last_pin_timestamp: None,
            thread_metadata: None,
            message_count: None,
            member_count: None,
            total_message_sent: None,
            available_tags: None,
            applied_tags: None,
            default_reaction_emoji: None,
            default_thread_rate_limit_per_user: None,
            default_sort_order: None,
            default_forum_layout: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `DefaultReaction`.
pub struct DefaultReaction {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji_id: Option<GenericId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `ForumTag`.
pub struct ForumTag {
    pub id: GenericId,
    pub name: String,
    #[serde(default)]
    pub moderated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji_id: Option<GenericId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji_name: Option<String>,
}

/// Request entry for moving or reparenting a guild channel.
///
/// The outer `Option` controls whether a field is omitted. Use `Some(None)` to
/// send JSON `null` for nullable Discord fields such as `parent_id`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModifyGuildChannelPosition {
    /// Channel to modify.
    pub id: GenericId,
    /// New sorting position, or `Some(None)` to send `null`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Option<i64>>,
    /// Whether to sync permission overwrites with the new parent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lock_permissions: Option<Option<bool>>,
    /// New parent category ID, or `Some(None)` to remove the parent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<Option<GenericId>>,
}

impl ModifyGuildChannelPosition {
    /// Entry for channel `id` that changes nothing until a field is set.
    pub fn new(id: GenericId) -> Self {
        Self {
            id,
            position: None,
            lock_permissions: None,
            parent_id: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `CreateDmChannel`.
pub struct CreateDmChannel {
    pub recipient_id: GenericId,
}

impl CreateDmChannel {
    /// Opens (or returns) the DM channel with `recipient_id`.
    pub fn new(recipient_id: GenericId) -> Self {
        Self { recipient_id }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
/// Typed Discord API object for `CreateGroupDmChannel`.
pub struct CreateGroupDmChannel {
    pub access_tokens: Vec<String>,
    pub nicks: HashMap<GenericId, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
/// Typed Discord API object for `AddGroupDmRecipient`.
pub struct AddGroupDmRecipient {
    pub access_token: String,
    pub nick: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `FollowedChannel`.
pub struct FollowedChannel {
    pub channel_id: GenericId,
    pub webhook_id: GenericId,
}
