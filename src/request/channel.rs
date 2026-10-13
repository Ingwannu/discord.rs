//! Channel and thread request bodies (3.0 phase 5 B).
//!
//! | Type | Endpoint |
//! |---|---|
//! | [`ModifyChannel`] | `PATCH /channels/{id}` ([`RestClient::update_channel`](crate::RestClient::update_channel)) |
//! | [`StartThreadFromMessage`] | `POST /channels/{id}/messages/{id}/threads` |
//! | [`StartThreadWithoutMessage`] | `POST /channels/{id}/threads` |
//! | [`StartForumThread`] + [`ForumThreadMessage`] | `POST /channels/{id}/threads` in a forum or media channel |
//! | [`AvailableForumTag`] | an entry of `available_tags` in Modify Channel and Create Guild Channel |
//!
//! The other channel bodies (`CreateGuildChannel`, `EditChannelPermission`,
//! `CreateDmChannel`, ...) are still in [`crate::model`]; they move here in
//! checkpoint 5C.

use serde::{Deserialize, Serialize};

use super::attachment::MessageAttachment;
use super::component::Component;
use super::embed::Embed;
use super::fields::request_setters;
use crate::bitfield::{ChannelFlags, MessageFlags};
use crate::model::{
    AllowedMentions, ChannelId, ChannelType, DefaultReaction, EmojiId, ForumLayout, ForumSortOrder,
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
    /// The forum or media channel's whole tag list (up to 20): listed tags
    /// with an `id` are kept (and updated), tags without one are created,
    /// and tags left out are deleted.
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub available_tags: Patch<Vec<AvailableForumTag>>,
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
        patch available_tags: Vec<AvailableForumTag>;
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

/// A tag in `available_tags` of [`ModifyChannel`] and
/// [`CreateGuildChannel`](crate::model::CreateGuildChannel).
///
/// [`ForumTag`] is the tag Discord returns and always has an id; a request
/// also lists tags that do not exist yet, which Discord creates and only
/// requires a `name` for. Keep an existing tag with
/// `AvailableForumTag::from(&tag)` (it keeps the id), add one with
/// [`Self::new`]. At most one of `emoji_id` and `emoji_name` may be set; the
/// setters clear the other.
///
/// Derives `Deserialize` only because `CreateGuildChannel` (still in
/// `crate::model` until checkpoint 5C) does.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub struct AvailableForumTag {
    /// The existing tag's id; `None` creates a tag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<ForumTagId>,
    /// 0-20 characters.
    pub name: String,
    /// Whether only members with Manage Threads can apply the tag.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moderated: Option<bool>,
    /// A guild emoji.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emoji_id: Option<EmojiId>,
    /// A Unicode emoji.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emoji_name: Option<String>,
}

impl AvailableForumTag {
    /// A new tag (no id).
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: None,
            name: name.into(),
            moderated: None,
            emoji_id: None,
            emoji_name: None,
        }
    }

    request_setters! {
        opt moderated: bool;
    }

    /// Uses a guild emoji (and clears `emoji_name`).
    pub fn emoji_id(mut self, emoji_id: impl Into<EmojiId>) -> Self {
        self.emoji_id = Some(emoji_id.into());
        self.emoji_name = None;
        self
    }

    /// Uses a Unicode emoji (and clears `emoji_id`).
    pub fn emoji_name(mut self, emoji_name: impl Into<String>) -> Self {
        self.emoji_name = Some(emoji_name.into());
        self.emoji_id = None;
        self
    }
}

impl From<&ForumTag> for AvailableForumTag {
    /// Keeps an existing tag as it is.
    fn from(tag: &ForumTag) -> Self {
        Self {
            id: Some(tag.id),
            name: tag.name.clone(),
            moderated: Some(tag.moderated),
            emoji_id: tag.emoji_id,
            emoji_name: tag.emoji_name.clone(),
        }
    }
}

impl From<ForumTag> for AvailableForumTag {
    fn from(tag: ForumTag) -> Self {
        Self {
            id: Some(tag.id),
            name: tag.name,
            moderated: Some(tag.moderated),
            emoji_id: tag.emoji_id,
            emoji_name: tag.emoji_name,
        }
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{AvailableForumTag, ModifyChannel};
    use crate::model::{test_id, CreateGuildChannel, ForumTag};

    #[test]
    fn a_tag_list_keeps_existing_tags_and_adds_new_ones() {
        let existing = ForumTag {
            id: test_id(31),
            name: "bug".to_string(),
            moderated: false,
            emoji_id: Some(test_id(33)),
            emoji_name: None,
        };
        let body = ModifyChannel::new().available_tags(vec![
            AvailableForumTag::from(&existing),
            AvailableForumTag::new("idea").moderated(true),
            AvailableForumTag::from(existing.clone()).emoji_name("🐛"),
        ]);
        assert_eq!(
            serde_json::to_value(&body).unwrap(),
            json!({ "available_tags": [
                { "id": "31", "name": "bug", "moderated": false, "emoji_id": "33" },
                { "name": "idea", "moderated": true },
                { "id": "31", "name": "bug", "moderated": false, "emoji_name": "🐛" },
            ] })
        );
        // The emoji setters keep at most one of the two set.
        let tag = AvailableForumTag::new("x")
            .emoji_name("🐛")
            .emoji_id(test_id(34));
        assert_eq!((tag.emoji_id, tag.emoji_name), (Some(test_id(34)), None));
    }

    #[test]
    fn a_new_forum_channel_can_list_tags_without_ids() {
        let body = CreateGuildChannel {
            name: "forum".to_string(),
            available_tags: Some(Some(vec![AvailableForumTag::new("bug")])),
            ..CreateGuildChannel::default()
        };
        assert_eq!(
            serde_json::to_value(&body).unwrap(),
            json!({ "name": "forum", "available_tags": [{ "name": "bug" }] })
        );
        let parsed: CreateGuildChannel = serde_json::from_value(
            json!({ "name": "forum", "available_tags": [{ "name": "bug" }] }),
        )
        .unwrap();
        assert_eq!(
            parsed.available_tags,
            Some(Some(vec![AvailableForumTag::new("bug")]))
        );
    }
}
