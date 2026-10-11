use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{
    ApplicationId, AttachmentId, ChannelId, ChannelType, EmojiId, ForumLayout, ForumSortOrder,
    ForumTagId, GenericId, GuildId, MessageId, OverwriteType, Permissions, RoleId, ThreadMetadata,
    UserId, VideoQualityMode, WebhookId,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `Attachment`.
pub struct Attachment {
    pub id: AttachmentId,
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

/// Who a channel permission overwrite applies to: the overwrite's wire `id`
/// and `type` read together.
///
/// A role overwrite and a member overwrite share one `id` field on the wire,
/// so the ID's kind depends on `type`. Matching on this enum gives a
/// [`RoleId`] or [`UserId`] and makes it impossible to build a role
/// overwrite from a user ID by accident. Overwrite types Discord adds later
/// decode as [`OverwriteTarget::Unknown`] and serialize back unchanged.
///
/// ```
/// use discordrs::model::{OverwriteTarget, PermissionOverwrite};
/// use discordrs::UserId;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let overwrite: PermissionOverwrite =
///     serde_json::from_str(r#"{"id": "41771983423143936", "type": 1, "deny": "1024"}"#)?;
/// let user_id: UserId = "41771983423143936".parse()?;
/// assert_eq!(overwrite.target, OverwriteTarget::Member(user_id));
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OverwriteTarget {
    /// `type: 0`: a role (the guild's ID is its `@everyone` role).
    Role(RoleId),
    /// `type: 1`: a guild member, addressed by user ID.
    Member(UserId),
    /// A `type` this version of discord.rs does not know, kept as sent.
    ///
    /// [`OverwriteTarget::from_wire`] never produces this for
    /// [`OverwriteType::ROLE`] or [`OverwriteType::MEMBER`]; build `Role` or
    /// `Member` for those so that equality and matching work.
    Unknown {
        /// The raw overwrite `type`.
        kind: OverwriteType,
        /// The overwrite `id`, whose kind is unknown.
        id: GenericId,
    },
}

impl OverwriteTarget {
    /// Interprets a wire `id` according to the overwrite `type`.
    pub const fn from_wire(kind: OverwriteType, id: GenericId) -> Self {
        match kind {
            OverwriteType::ROLE => Self::Role(id.cast()),
            OverwriteType::MEMBER => Self::Member(id.cast()),
            _ => Self::Unknown { kind, id },
        }
    }

    /// The wire `id`: the role or user ID with its kind erased.
    pub const fn id(self) -> GenericId {
        match self {
            Self::Role(id) => id.erase(),
            Self::Member(id) => id.erase(),
            Self::Unknown { id, .. } => id,
        }
    }

    /// The wire `type`.
    pub const fn kind(self) -> OverwriteType {
        match self {
            Self::Role(_) => OverwriteType::ROLE,
            Self::Member(_) => OverwriteType::MEMBER,
            Self::Unknown { kind, .. } => kind,
        }
    }
}

impl From<RoleId> for OverwriteTarget {
    fn from(id: RoleId) -> Self {
        Self::Role(id)
    }
}

impl From<UserId> for OverwriteTarget {
    fn from(id: UserId) -> Self {
        Self::Member(id)
    }
}

/// A channel permission overwrite.
///
/// Reads and writes Discord's shape, `{"id", "type", "allow", "deny"}`;
/// `id` and `type` together become [`Self::target`].
// [Decision Log]
// - 목적과 의도: 덮어쓰기 `id`는 `type`에 따라 역할 ID이거나 사용자 ID다. 사용자 ID로 역할
//   덮어쓰기를 만드는 실수를 컴파일 시점에 막는다(설계 §1 다형 ID).
// - 기존 구현 및 제약 조건: 2.x와 A 단계는 `id: GenericId` + `kind: u8`였고, 와이어 형태
//   (`id`, `type`, 생략 가능한 `allow`/`deny`)는 바뀌면 안 된다.
// - 검토한 주요 대안: (1) `id`를 GenericId로 두고 접근자만 추가, (2) `#[serde(flatten)]` +
//   내부 태그 enum, (3) 비공개 와이어 구조체를 거치는 from/into 변환.
// - 선택한 방식: (3).
// - 다른 대안 대신 이 방식을 선택한 이유: (1)은 잘못된 조합을 막지 못한다. (2)는 serde가
//   flatten 필드를 중간 Content로 버퍼링해 느리고, 정수 `type`을 내부 태그로 쓸 수 없다.
//   (3)은 와이어 구조체가 기존 필드 정의와 같아 형태가 그대로 유지된다.
// - 장점, 단점 및 영향: 공개 필드 `id`/`kind`가 `target` 하나로 바뀐다(마이그레이션 노트).
//   모르는 `type`은 Unknown으로 보존되어 왕복한다.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "PermissionOverwriteWire", into = "PermissionOverwriteWire")]
pub struct PermissionOverwrite {
    /// The role or member the overwrite applies to (wire `id` and `type`).
    pub target: OverwriteTarget,
    /// Permissions explicitly allowed.
    pub allow: Option<Permissions>,
    /// Permissions explicitly denied.
    pub deny: Option<Permissions>,
}

impl PermissionOverwrite {
    /// An overwrite for `target` that allows and denies nothing yet.
    pub const fn new(target: OverwriteTarget) -> Self {
        Self {
            target,
            allow: None,
            deny: None,
        }
    }
}

/// The wire shape of [`PermissionOverwrite`], unchanged from 2.x.
#[derive(Clone, Copy, Serialize, Deserialize)]
struct PermissionOverwriteWire {
    id: GenericId,
    #[serde(rename = "type")]
    kind: OverwriteType,
    #[serde(skip_serializing_if = "Option::is_none")]
    allow: Option<Permissions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    deny: Option<Permissions>,
}

impl From<PermissionOverwriteWire> for PermissionOverwrite {
    fn from(wire: PermissionOverwriteWire) -> Self {
        Self {
            target: OverwriteTarget::from_wire(wire.kind, wire.id),
            allow: wire.allow,
            deny: wire.deny,
        }
    }
}

impl From<PermissionOverwrite> for PermissionOverwriteWire {
    fn from(overwrite: PermissionOverwrite) -> Self {
        Self {
            id: overwrite.target.id(),
            kind: overwrite.target.kind(),
            allow: overwrite.allow,
            deny: overwrite.deny,
        }
    }
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
    pub kind: Option<ChannelType>,
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
    pub parent_id: Option<Option<ChannelId>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nsfw: Option<Option<bool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rtc_region: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_quality_mode: Option<Option<VideoQualityMode>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_auto_archive_duration: Option<Option<u64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_reaction_emoji: Option<Option<DefaultReaction>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_tags: Option<Option<Vec<ForumTag>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_sort_order: Option<Option<ForumSortOrder>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_forum_layout: Option<Option<ForumLayout>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_thread_rate_limit_per_user: Option<Option<u64>>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
/// Typed Discord API object for `EditChannelPermission`.
///
/// `kind` must match the [`OverwriteTarget`] passed to
/// [`RestClient::edit_channel_permissions_typed`](crate::http::RestClient::edit_channel_permissions_typed);
/// [`EditChannelPermission::for_target`] fills it in. No `Default`: `type`
/// is required and 2.x's default (`0`, a role overwrite) was a guess.
pub struct EditChannelPermission {
    #[serde(rename = "type")]
    pub kind: OverwriteType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow: Option<Permissions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deny: Option<Permissions>,
}

impl EditChannelPermission {
    /// A body whose `type` matches `target`, allowing and denying nothing yet.
    pub const fn for_target(target: OverwriteTarget) -> Self {
        Self {
            kind: target.kind(),
            allow: None,
            deny: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `Channel`.
pub struct Channel {
    pub id: ChannelId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<GuildId>,
    #[serde(rename = "type")]
    pub kind: ChannelType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<ChannelId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nsfw: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitrate: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limit_per_user: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_message_id: Option<MessageId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<UserId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application_id: Option<ApplicationId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_overwrites: Option<Vec<PermissionOverwrite>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rtc_region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_quality_mode: Option<VideoQualityMode>,
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
    pub applied_tags: Option<Vec<ForumTagId>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_reaction_emoji: Option<DefaultReaction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_thread_rate_limit_per_user: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_sort_order: Option<ForumSortOrder>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_forum_layout: Option<ForumLayout>,
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
    pub(crate) fn stub(id: ChannelId, guild_id: Option<GuildId>, kind: ChannelType) -> Self {
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
    pub emoji_id: Option<EmojiId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `ForumTag`.
pub struct ForumTag {
    pub id: ForumTagId,
    pub name: String,
    #[serde(default)]
    pub moderated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji_id: Option<EmojiId>,
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
    pub id: ChannelId,
    /// New sorting position, or `Some(None)` to send `null`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Option<i64>>,
    /// Whether to sync permission overwrites with the new parent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lock_permissions: Option<Option<bool>>,
    /// New parent category ID, or `Some(None)` to remove the parent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<Option<ChannelId>>,
}

impl ModifyGuildChannelPosition {
    /// Entry for channel `id` that changes nothing until a field is set.
    pub fn new(id: ChannelId) -> Self {
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
    pub recipient_id: UserId,
}

impl CreateDmChannel {
    /// Opens (or returns) the DM channel with `recipient_id`.
    pub fn new(recipient_id: UserId) -> Self {
        Self { recipient_id }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
/// Typed Discord API object for `CreateGroupDmChannel`.
pub struct CreateGroupDmChannel {
    pub access_tokens: Vec<String>,
    pub nicks: HashMap<UserId, String>,
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
    pub channel_id: ChannelId,
    pub webhook_id: WebhookId,
}
