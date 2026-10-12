//! Channel and thread request bodies (3.0 phase 5 B).
//!
//! | Type | Endpoint |
//! |---|---|
//! | [`ModifyChannel`] | `PATCH /channels/{id}` ([`RestClient::update_channel`](crate::RestClient::update_channel)) |
//! | [`StartThreadFromMessage`] | `POST /channels/{id}/messages/{id}/threads` |
//! | [`StartThreadWithoutMessage`] | `POST /channels/{id}/threads` |
//! | [`StartForumThread`] + [`ForumThreadMessage`] | `POST /channels/{id}/threads` in a forum or media channel |
//!
//! The other channel bodies (`CreateGuildChannel`, `EditChannelPermission`,
//! `CreateDmChannel`, ...) are still in [`crate::model`]; they move here in
//! checkpoint 5C.

use serde::Serialize;

use super::attachment::MessageAttachment;
use super::component::Component;
use super::embed::Embed;
use super::fields::request_setters;
use crate::bitfield::{ChannelFlags, MessageFlags};
use crate::model::{
    AllowedMentions, ChannelId, ChannelType, DefaultReaction, ForumLayout, ForumSortOrder,
    ForumTag, ForumTagId, NullablePatch, Patch, PermissionOverwrite, StickerId, VideoQualityMode,
};

/// Body of Modify Channel (`PATCH /channels/{id}`).
///
/// One type for the three variants Discord documents: a group DM (`name`,
/// `icon`), a guild channel, and a thread (`archived`, `locked`, ...).
/// Every field is a patch: omitted keeps the current value. Fields Discord
/// lets you clear are [`NullablePatch`] (`clear_*` setters send `null`).
/// Discord ignores or rejects fields that do not apply to the channel's
/// type; see the docs' "Channel Type" column.
#[derive(Clone, Debug, Default, Serialize)]
#[non_exhaustive]
pub struct ModifyChannel {
    /// 1-100 character name (all channel kinds).
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub name: Patch<String>,
    /// Group DM icon as image data (`data:image/png;base64,...`).
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub icon: Patch<String>,
    /// Text ↔ announcement conversion only.
    #[serde(rename = "type", skip_serializing_if = "Patch::is_missing")]
    pub kind: Patch<ChannelType>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub position: NullablePatch<i64>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub topic: NullablePatch<String>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub nsfw: NullablePatch<bool>,
    /// Slowmode in seconds (0-21600); also on threads.
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub rate_limit_per_user: NullablePatch<u64>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub bitrate: NullablePatch<u64>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub user_limit: NullablePatch<u64>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub permission_overwrites: NullablePatch<Vec<PermissionOverwrite>>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub parent_id: NullablePatch<ChannelId>,
    /// Voice region id; `null` means automatic.
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub rtc_region: NullablePatch<String>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub video_quality_mode: NullablePatch<VideoQualityMode>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub default_auto_archive_duration: NullablePatch<u64>,
    /// Channel flags; `PINNED` only on forum and media threads.
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub flags: Patch<ChannelFlags>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub available_tags: Patch<Vec<ForumTag>>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub default_reaction_emoji: NullablePatch<DefaultReaction>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub default_thread_rate_limit_per_user: Patch<u64>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub default_sort_order: NullablePatch<ForumSortOrder>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub default_forum_layout: Patch<ForumLayout>,
    /// Thread only.
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub archived: Patch<bool>,
    /// Thread only: 60, 1440, 4320, or 10080 minutes.
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub auto_archive_duration: Patch<u64>,
    /// Thread only.
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub locked: Patch<bool>,
    /// Private thread only.
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub invitable: Patch<bool>,
    /// Forum and media thread only (up to 5).
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub applied_tags: Patch<Vec<ForumTagId>>,
}

impl ModifyChannel {
    /// A modification that changes nothing yet.
    pub fn new() -> Self {
        Self::default()
    }

    request_setters! {
        patch name: String;
        patch icon: String;
        patch kind: ChannelType;
        nullable position / clear_position: i64;
        nullable topic / clear_topic: String;
        nullable nsfw / clear_nsfw: bool;
        nullable rate_limit_per_user / clear_rate_limit_per_user: u64;
        nullable bitrate / clear_bitrate: u64;
        nullable user_limit / clear_user_limit: u64;
        nullable permission_overwrites / clear_permission_overwrites: Vec<PermissionOverwrite>;
        nullable parent_id / clear_parent_id: ChannelId;
        nullable rtc_region / clear_rtc_region: String;
        nullable video_quality_mode / clear_video_quality_mode: VideoQualityMode;
        nullable default_auto_archive_duration / clear_default_auto_archive_duration: u64;
        patch flags: ChannelFlags;
        patch available_tags: Vec<ForumTag>;
        nullable default_reaction_emoji / clear_default_reaction_emoji: DefaultReaction;
        patch default_thread_rate_limit_per_user: u64;
        nullable default_sort_order / clear_default_sort_order: ForumSortOrder;
        patch default_forum_layout: ForumLayout;
        patch archived: bool;
        patch auto_archive_duration: u64;
        patch locked: bool;
        patch invitable: bool;
        patch applied_tags: Vec<ForumTagId>;
    }
}

/// Body of Start Thread from Message
/// (`POST /channels/{id}/messages/{id}/threads`).
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct StartThreadFromMessage {
    /// 1-100 character thread name.
    pub name: String,
    /// 60, 1440, 4320, or 10080 minutes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_archive_duration: Option<u64>,
    /// Slowmode in seconds (0-21600).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limit_per_user: Option<u64>,
}

impl StartThreadFromMessage {
    /// A thread named `name`.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            auto_archive_duration: None,
            rate_limit_per_user: None,
        }
    }

    request_setters! {
        opt auto_archive_duration: u64;
        opt rate_limit_per_user: u64;
    }
}

/// Body of Start Thread without Message (`POST /channels/{id}/threads` in a
/// text or announcement channel).
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct StartThreadWithoutMessage {
    /// 1-100 character thread name.
    pub name: String,
    /// 60, 1440, 4320, or 10080 minutes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_archive_duration: Option<u64>,
    /// `PRIVATE_THREAD` (the default for now, Discord says) or
    /// `PUBLIC_THREAD` / `ANNOUNCEMENT_THREAD`.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<ChannelType>,
    /// Private threads only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invitable: Option<bool>,
    /// Slowmode in seconds (0-21600).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limit_per_user: Option<u64>,
}

impl StartThreadWithoutMessage {
    /// A thread named `name`.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            auto_archive_duration: None,
            kind: None,
            invitable: None,
            rate_limit_per_user: None,
        }
    }

    request_setters! {
        opt auto_archive_duration: u64;
        opt kind: ChannelType;
        opt invitable: bool;
        opt rate_limit_per_user: u64;
    }
}

/// Body of Start Thread in Forum or Media Channel
/// (`POST /channels/{id}/threads`): the thread and its first message.
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct StartForumThread {
    /// 1-100 character thread name.
    pub name: String,
    /// 60, 1440, 4320, or 10080 minutes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_archive_duration: Option<u64>,
    /// Slowmode in seconds (0-21600).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limit_per_user: Option<u64>,
    /// The first message of the thread.
    pub message: ForumThreadMessage,
    /// Forum tags to apply (up to 5).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applied_tags: Option<Vec<ForumTagId>>,
}

impl StartForumThread {
    /// A thread named `name` that starts with `message`.
    pub fn new(name: impl Into<String>, message: ForumThreadMessage) -> Self {
        Self {
            name: name.into(),
            auto_archive_duration: None,
            rate_limit_per_user: None,
            message,
            applied_tags: None,
        }
    }

    request_setters! {
        opt auto_archive_duration: u64;
        opt rate_limit_per_user: u64;
        opt applied_tags: Vec<ForumTagId>;
    }
}

/// The first message of a forum or media thread: Discord's "forum and media
/// thread message params", a subset of a message create (no nonce, reply,
/// tts, or poll). At least one of `content`, `embeds`, `components`,
/// `sticker_ids`, or an uploaded file is required.
#[derive(Clone, Debug, Default, Serialize)]
#[non_exhaustive]
pub struct ForumThreadMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embeds: Option<Vec<Embed>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_mentions: Option<AllowedMentions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub components: Option<Vec<Component>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sticker_ids: Option<Vec<StickerId>>,
    /// Metadata for the files sent with
    /// [`RestClient::create_forum_thread_with_files`](crate::RestClient::create_forum_thread_with_files).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachments: Option<Vec<MessageAttachment>>,
    /// Only `SUPPRESS_EMBEDS` and `SUPPRESS_NOTIFICATIONS`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<MessageFlags>,
}

impl ForumThreadMessage {
    /// An empty message; set at least one of the content fields.
    pub fn new() -> Self {
        Self::default()
    }

    request_setters! {
        opt content: String;
        opt embeds: Vec<Embed>;
        opt allowed_mentions: AllowedMentions;
        opt components: Vec<Component>;
        opt sticker_ids: Vec<StickerId>;
        opt attachments: Vec<MessageAttachment>;
        opt flags: MessageFlags;
    }
}
