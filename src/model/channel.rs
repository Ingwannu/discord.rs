use serde::{Deserialize, Serialize};

use super::{DefaultReaction, ForumTag, PermissionsBitField, Snowflake, ThreadMetadata};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Attachment`.
pub struct Attachment {
    pub id: Snowflake,
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

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `PermissionOverwrite`.
pub struct PermissionOverwrite {
    pub id: Snowflake,
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
    pub parent_id: Option<Option<Snowflake>>,
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

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Channel`.
pub struct Channel {
    pub id: Snowflake,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<Snowflake>,
    #[serde(rename = "type")]
    pub kind: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nsfw: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitrate: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limit_per_user: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_message_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application_id: Option<Snowflake>,
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
    pub applied_tags: Option<Vec<Snowflake>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_reaction_emoji: Option<DefaultReaction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_thread_rate_limit_per_user: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_sort_order: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_forum_layout: Option<u64>,
}
