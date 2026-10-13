//! Gateway events: the [`Event`] envelope every handler and collector
//! receives, its decoded [`EventKind`], and the per-event payload types.
//!
//! ```text
//! [Decision Log]
//! - 목적과 의도: 수신 이벤트 하나를 핸들러·컬렉터·캐시가 복제 없이 공유하고, 원본 payload는
//!   필요할 때만 한 곳에 둔다(설계 §3 "이벤트 API", "4단계 실행 결정" 체크포인트 A).
//! - 기존 구현 및 제약 조건: 2.x `Event`는 4,848 B 인라인 enum이었고 variant마다 원본
//!   `Value`(raw)를 품었다. 저널·컬렉터·핸들러 레인이 이벤트를 통째로 복제했고(기준선 핫스팟
//!   ①③④), MESSAGE_UPDATE 병합은 raw의 키 집합을 읽었다. 핸들러 trait는 RPITIT + 비공개
//!   `ErasedEventHandler` 형태를 유지해야 한다(async-trait 없음, MSRV 1.88).
//! - 검토한 주요 대안: (1) enum 유지 + `Arc<Event>`만 전달, (2) 봉투 struct + 작은 `EventKind`
//!   + 선택적 `Box<RawValue>`, (3) 이벤트마다 별도 타입과 trait 객체. raw: (a) 항상 보관,
//!   (b) 알려진 이벤트는 옵션, 알 수 없음·디코드 실패는 항상, (c) 보관하지 않음.
//! - 선택한 방식: (2) + (b). `Event { kind, metadata, raw }`, 필드는 비공개이고 생성자
//!   `Event::decode`가 raw 규칙을 강제한다. 캐시와 공유하는 payload는 `Arc<T>`, 큰 비공유 payload는
//!   `Box<T>`, 작은 ID·삭제 이벤트는 인라인이다. `Debug`는 raw 내용을 출력하지 않는다.
//! - 다른 대안 대신 이 방식을 선택한 이유: (1)은 enum 크기와 raw 복제가 그대로다. (3)은 컬렉터의
//!   참조 필터와 `match`가 어려워진다. (a)는 MESSAGE_CREATE 이벤트 힙이 5배, GUILD_CREATE가
//!   3.8배가 된다(기준선 §5). (c)는 미래의 디스패치와 깨진 payload를 잃는다.
//! - 장점, 단점 및 영향: `size_of::<Event>()`가 4,848 B에서 192 B 이하로, 전달은 포인터 복사가
//!   된다(레이아웃 테스트로 고정). 공개 API가 깨진다: `match event` 대신 `match event.kind()`,
//!   `event.raw()`는 `Option<&RawValue>`이고 기본은 `None`이다(`keep_raw_events(true)`).
//!   일부 payload 래퍼 타입(`MessageEvent` 등)이 없어지고 모델 타입을 바로 담는다.
//! ```

use std::borrow::Cow;
use std::fmt;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use serde_json::Value;
use tokio::time::Instant;

use crate::error::DiscordError;
use crate::model::{
    Activity, ApplicationCommandPermission, ApplicationId, AuditLogActionType, AuditLogEntry,
    AuditLogEntryId, AutoModerationAction, AutoModerationEventType, AutoModerationRuleId,
    AutoModerationTriggerMetadata, AutoModerationTriggerType, Channel, ChannelId, ClientStatus,
    CommandPermissionsId, Entitlement, GenericId, GuildId, GuildScheduledEventEntityMetadata,
    Integration, IntegrationId, Interaction, InviteTargetType, Member, Message, MessageId,
    PartialUser, PollAnswerId, Presence, PresenceStatus, ReactionType, Role, RoleId,
    ScheduledEventEntityType, ScheduledEventId, ScheduledEventPrivacyLevel, ScheduledEventStatus,
    SoundboardSound, SoundboardSoundId, StageInstance, Sticker, Subscription, ThreadMember, User,
    UserId, VoiceChannelEffectAnimationType, VoiceServerUpdate, VoiceState,
};
use crate::types::Emoji;

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `ReadyApplication`.
pub struct ReadyApplication {
    pub id: ApplicationId,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `ReadyPayload`.
pub struct ReadyPayload {
    pub user: User,
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application: Option<ReadyApplication>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume_gateway_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `GuildDeletePayload`.
pub struct GuildDeletePayload {
    pub id: GuildId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable: Option<bool>,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `MemberEvent`.
pub struct MemberEvent {
    pub guild_id: GuildId,
    /// Shared with the cache, which stores this `Arc` as it is.
    pub member: Arc<Member>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `MemberRemovePayload`.
pub struct MemberRemovePayload {
    pub guild_id: GuildId,
    pub user: User,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `GuildMembersChunkPayload`.
pub struct GuildMembersChunkPayload {
    pub guild_id: GuildId,
    /// Shared with the cache, the external backend, and
    /// `Context::fetch_members`.
    #[serde(
        default,
        deserialize_with = "crate::model::shared_members",
        serialize_with = "crate::model::shared::seq_arc::serialize"
    )]
    pub members: Vec<Arc<Member>>,
    pub chunk_index: u64,
    pub chunk_count: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub not_found: Vec<UserId>,
    /// Each with `user_id` set from its partial `user`; shared with the
    /// cache and the external backend. As in GUILD_CREATE, a member,
    /// presence, or activity that does not decode is left out (and logged)
    /// instead of failing the chunk.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "crate::model::shared_presences",
        serialize_with = "crate::model::shared::seq_arc::serialize_option"
    )]
    pub presences: Option<Vec<Arc<Presence>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
/// Typed Discord API object for `RoleEvent`.
pub struct RoleEvent {
    pub guild_id: GuildId,
    /// Shared with the cache, which stores this `Arc` as it is.
    #[serde(deserialize_with = "crate::model::shared::arc::deserialize")]
    pub role: Arc<Role>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `RoleDeletePayload`.
pub struct RoleDeletePayload {
    pub guild_id: GuildId,
    pub role_id: RoleId,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `MessageDeletePayload`.
pub struct MessageDeletePayload {
    pub id: MessageId,
    pub channel_id: ChannelId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<GuildId>,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `BulkMessageDeleteEvent`.
pub struct BulkMessageDeleteEvent {
    pub ids: Vec<MessageId>,
    pub channel_id: ChannelId,
    pub guild_id: Option<GuildId>,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `ChannelPinsUpdateEvent`.
pub struct ChannelPinsUpdateEvent {
    pub channel_id: ChannelId,
    pub guild_id: Option<GuildId>,
    pub last_pin_timestamp: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
/// Typed Discord API object for `GuildBanEvent`.
pub struct GuildBanEvent {
    pub guild_id: GuildId,
    pub user: User,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `GuildEmojisUpdateEvent`.
pub struct GuildEmojisUpdateEvent {
    pub guild_id: GuildId,
    pub emojis: Vec<Emoji>,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `IntegrationEvent`.
pub struct IntegrationEvent {
    pub integration: Integration,
    pub guild_id: Option<GuildId>,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `IntegrationDeleteEvent`.
pub struct IntegrationDeleteEvent {
    pub id: Option<IntegrationId>,
    pub guild_id: Option<GuildId>,
    pub application_id: Option<ApplicationId>,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `PollVoteEvent`.
pub struct PollVoteEvent {
    pub user_id: Option<UserId>,
    pub channel_id: Option<ChannelId>,
    pub message_id: Option<MessageId>,
    pub guild_id: Option<GuildId>,
    pub answer_id: Option<PollAnswerId>,
}

#[derive(Clone, Debug, Deserialize)]
/// Typed Discord API object for `SoundboardSoundDeleteEvent`.
pub struct SoundboardSoundDeleteEvent {
    pub sound_id: SoundboardSoundId,
    pub guild_id: GuildId,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `SoundboardSoundsEvent`.
pub struct SoundboardSoundsEvent {
    pub guild_id: GuildId,
    pub soundboard_sounds: Vec<SoundboardSound>,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `InviteEvent`.
pub struct InviteEvent {
    pub guild_id: Option<GuildId>,
    pub channel_id: Option<ChannelId>,
    pub code: Option<String>,
    /// User who created the invite (INVITE_CREATE only).
    pub inviter: Option<User>,
    /// How many times the invite has been used (always 0 on creation).
    pub uses: Option<u64>,
    pub max_uses: Option<u64>,
    /// Invite lifetime in seconds; 0 means never expires.
    pub max_age: Option<u64>,
    pub temporary: Option<bool>,
    pub created_at: Option<String>,
    pub expires_at: Option<String>,
    /// Target type for voice-channel invites (stream or embedded app).
    pub target_type: Option<InviteTargetType>,
    pub target_user: Option<User>,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `ReactionEvent`.
pub struct ReactionEvent {
    pub user_id: Option<UserId>,
    pub channel_id: Option<ChannelId>,
    pub message_id: Option<MessageId>,
    pub guild_id: Option<GuildId>,
    pub member: Option<Member>,
    pub message_author_id: Option<UserId>,
    pub emoji: Option<Emoji>,
    pub burst: Option<bool>,
    pub burst_colors: Vec<String>,
    /// Normal or burst reaction. `None` when absent or not a valid reaction
    /// type (the original stays readable through [`Event::raw`] when raw
    /// payloads are kept).
    pub reaction_type: Option<ReactionType>,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `ReactionRemoveAllEvent`.
pub struct ReactionRemoveAllEvent {
    pub channel_id: Option<ChannelId>,
    pub message_id: Option<MessageId>,
    pub guild_id: Option<GuildId>,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `TypingStartEvent`.
pub struct TypingStartEvent {
    pub channel_id: Option<ChannelId>,
    pub guild_id: Option<GuildId>,
    pub user_id: Option<UserId>,
    pub timestamp: Option<u64>,
}

/// PRESENCE_UPDATE: a user's presence in a guild, and the partial user
/// object Discord sends with it.
///
/// The presence is built once when the dispatch is decoded and shared with
/// the cache and the external cache backend, which store this `Arc` as it is
/// (each used to build a copy with every activity cloned).
#[derive(Clone, Debug)]
pub struct PresenceUpdateEvent {
    pub guild_id: Option<GuildId>,
    /// The partial user: only `id` is guaranteed. The fields it carries
    /// patch the cached user (see `CacheHandle::apply_partial_user`).
    pub user: Option<PartialUser>,
    /// `user_id` (from `user`), `status`, `activities` (`None` when Discord
    /// sent none), and `client_status`. Discord does not validate the field
    /// types of this event, so a field of the wrong type reads as `None`
    /// and an activity that does not decode is left out of `activities`.
    pub presence: Arc<Presence>,
}

impl PresenceUpdateEvent {
    /// The user the presence belongs to.
    pub fn user_id(&self) -> Option<UserId> {
        self.presence.user_id
    }

    /// The overall status (`online`, `idle`, ...).
    pub fn status(&self) -> Option<&PresenceStatus> {
        self.presence.status.as_ref()
    }

    /// The user's activities, empty when there are none.
    pub fn activities(&self) -> &[Activity] {
        self.presence.activities.as_deref().unwrap_or_default()
    }

    /// The status per platform.
    pub fn client_status(&self) -> Option<&ClientStatus> {
        self.presence.client_status.as_ref()
    }
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `WebhooksUpdateEvent`.
pub struct WebhooksUpdateEvent {
    pub guild_id: Option<GuildId>,
    pub channel_id: Option<ChannelId>,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `GuildIntegrationsUpdateEvent`.
pub struct GuildIntegrationsUpdateEvent {
    pub guild_id: Option<GuildId>,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `ThreadEvent`.
pub struct ThreadEvent {
    /// Shared with the cache, which stores this `Arc` as it is.
    pub thread: Arc<Channel>,
    /// Set on THREAD_CREATE when the gateway marks the thread as newly
    /// created (as opposed to one the current user was just added to).
    pub newly_created: Option<bool>,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `ThreadMemberUpdateEvent`.
pub struct ThreadMemberUpdateEvent {
    pub guild_id: Option<GuildId>,
    pub thread_id: Option<ChannelId>,
    pub user_id: Option<UserId>,
    pub join_timestamp: Option<String>,
    pub flags: Option<u64>,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `ThreadMembersUpdateEvent`.
pub struct ThreadMembersUpdateEvent {
    pub guild_id: Option<GuildId>,
    pub thread_id: Option<ChannelId>,
    pub added_members: Option<Vec<ThreadMember>>,
    pub removed_member_ids: Option<Vec<UserId>>,
    pub member_count: Option<u64>,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `ThreadListSyncEvent`.
pub struct ThreadListSyncEvent {
    pub guild_id: Option<GuildId>,
    /// Parent channel ids whose threads are being synced; empty means the
    /// entire guild was synced.
    pub channel_ids: Vec<ChannelId>,
    pub threads: Vec<Channel>,
    /// Thread member objects for the threads the current user has joined.
    pub members: Vec<ThreadMember>,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `ReactionRemoveEmojiEvent`.
pub struct ReactionRemoveEmojiEvent {
    pub channel_id: Option<ChannelId>,
    pub message_id: Option<MessageId>,
    pub guild_id: Option<GuildId>,
    pub emoji: Option<Emoji>,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `GuildStickersUpdateEvent`.
pub struct GuildStickersUpdateEvent {
    pub guild_id: Option<GuildId>,
    pub stickers: Vec<Sticker>,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `ScheduledEvent`.
pub struct ScheduledEvent {
    pub id: Option<ScheduledEventId>,
    pub guild_id: Option<GuildId>,
    pub channel_id: Option<ChannelId>,
    pub creator_id: Option<UserId>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub scheduled_start_time: Option<String>,
    pub scheduled_end_time: Option<String>,
    pub privacy_level: Option<ScheduledEventPrivacyLevel>,
    pub status: Option<ScheduledEventStatus>,
    pub entity_type: Option<ScheduledEventEntityType>,
    /// See [`GuildScheduledEvent::entity_id`](crate::model::GuildScheduledEvent::entity_id).
    pub entity_id: Option<GenericId>,
    pub entity_metadata: Option<GuildScheduledEventEntityMetadata>,
    pub user_count: Option<u64>,
    pub image: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `GuildScheduledEventUserEvent`.
pub struct GuildScheduledEventUserEvent {
    pub guild_scheduled_event_id: ScheduledEventId,
    pub user_id: UserId,
    pub guild_id: GuildId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member: Option<Member>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<User>,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `ApplicationCommandPermissionsUpdateEvent`.
///
/// Decoded like [`GuildApplicationCommandPermissions`](crate::model::GuildApplicationCommandPermissions):
/// `id` is compared with `application_id` (when both are present) and the
/// permission targets are resolved against `guild_id` (when present), so
/// `@everyone` and all-channels entries arrive as
/// [`CommandPermissionTarget::Everyone`](crate::model::CommandPermissionTarget::Everyone)
/// and [`AllChannels`](crate::model::CommandPermissionTarget::AllChannels).
pub struct ApplicationCommandPermissionsUpdateEvent {
    /// The command, or the application for application-wide permissions.
    /// Without an `application_id` a present `id` is taken as a command.
    pub id: Option<CommandPermissionsId>,
    pub application_id: Option<ApplicationId>,
    pub guild_id: Option<GuildId>,
    pub permissions: Vec<ApplicationCommandPermission>,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `VoiceChannelEffectEvent`.
pub struct VoiceChannelEffectEvent {
    pub channel_id: Option<ChannelId>,
    pub guild_id: Option<GuildId>,
    pub user_id: Option<UserId>,
    pub emoji: Option<Emoji>,
    pub animation_type: Option<VoiceChannelEffectAnimationType>,
    pub animation_id: Option<u64>,
    pub sound_id: Option<SoundboardSoundId>,
    pub sound_volume: Option<f64>,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `VoiceChannelStartTimeUpdateEvent`.
pub struct VoiceChannelStartTimeUpdateEvent {
    pub channel_id: Option<ChannelId>,
    pub guild_id: Option<GuildId>,
    pub voice_channel_start_time: Option<String>,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `VoiceChannelStatusUpdateEvent`.
pub struct VoiceChannelStatusUpdateEvent {
    pub channel_id: Option<ChannelId>,
    pub guild_id: Option<GuildId>,
    pub status: Option<String>,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `ChannelInfoChannel`.
pub struct ChannelInfoChannel {
    /// Channel ID whose metadata was returned.
    pub id: ChannelId,
    /// Optional channel status value.
    pub status: Option<String>,
    /// Optional voice-channel start time value.
    pub voice_start_time: Option<String>,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `ChannelInfoEvent`.
pub struct ChannelInfoEvent {
    /// Guild ID for the channel-info dispatch.
    pub guild_id: GuildId,
    /// Channel metadata objects included by Discord.
    pub channels: Vec<ChannelInfoChannel>,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `RateLimitedEvent`.
pub struct RateLimitedEvent {
    pub opcode: Option<u64>,
    pub retry_after: Option<f64>,
    pub meta: Option<Value>,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `AutoModerationEvent`.
pub struct AutoModerationEvent {
    pub id: Option<AutoModerationRuleId>,
    pub guild_id: Option<GuildId>,
    pub name: Option<String>,
    pub creator_id: Option<UserId>,
    pub event_type: Option<AutoModerationEventType>,
    pub trigger_type: Option<AutoModerationTriggerType>,
    pub trigger_metadata: Option<AutoModerationTriggerMetadata>,
    pub actions: Vec<AutoModerationAction>,
    pub enabled: Option<bool>,
    pub exempt_roles: Vec<RoleId>,
    pub exempt_channels: Vec<ChannelId>,
    /// The action that was executed (AUTO_MODERATION_ACTION_EXECUTION only).
    pub action: Option<AutoModerationAction>,
    pub rule_id: Option<AutoModerationRuleId>,
    pub rule_trigger_type: Option<AutoModerationTriggerType>,
    pub user_id: Option<UserId>,
    pub channel_id: Option<ChannelId>,
    pub message_id: Option<MessageId>,
    pub alert_system_message_id: Option<MessageId>,
    pub content: Option<String>,
    pub matched_keyword: Option<String>,
    pub matched_content: Option<String>,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `AuditLogEntryEvent`.
pub struct AuditLogEntryEvent {
    pub guild_id: Option<GuildId>,
    pub entry: Option<AuditLogEntry>,
    pub id: Option<AuditLogEntryId>,
    pub user_id: Option<UserId>,
    /// See [`AuditLogEntry::target_id`](crate::model::AuditLogEntry::target_id).
    pub target_id: Option<GenericId>,
    pub action_type: Option<AuditLogActionType>,
    pub changes: Option<Vec<Value>>,
    pub options: Option<Value>,
    pub reason: Option<String>,
}

/// Where and when the gateway received an event.
///
/// `received_at` is taken in the gateway read loop when the frame arrives,
/// before the event waits in any queue, so interaction handlers can measure
/// Discord's three-second response window from it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventMetadata {
    name: Cow<'static, str>,
    shard_id: u32,
    shard_count: u32,
    sequence: Option<u64>,
    received_at: Instant,
}

impl EventMetadata {
    /// Metadata for dispatch `name` received at `received_at`, on shard 0 of
    /// 1 and without a sequence number. Known dispatch names are stored
    /// without allocating.
    pub fn new(name: &str, received_at: Instant) -> Self {
        Self {
            name: decode::intern_dispatch_name(name),
            shard_id: 0,
            shard_count: 1,
            sequence: None,
            received_at,
        }
    }

    /// Sets the receiving shard (`shard_id` of `shard_count`).
    pub fn with_shard(mut self, shard_id: u32, shard_count: u32) -> Self {
        self.shard_id = shard_id;
        self.shard_count = shard_count.max(1);
        self
    }

    /// Sets the gateway sequence number the dispatch carried.
    pub fn with_sequence(mut self, sequence: u64) -> Self {
        self.sequence = Some(sequence);
        self
    }

    /// The dispatch name (`t`), for example `"MESSAGE_CREATE"`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The receiving shard as `(shard_id, shard_count)`.
    pub fn shard(&self) -> (u32, u32) {
        (self.shard_id, self.shard_count)
    }

    /// The gateway sequence number (`s`), if the dispatch carried one.
    pub fn sequence(&self) -> Option<u64> {
        self.sequence
    }

    /// When the gateway read loop received the frame.
    pub fn received_at(&self) -> Instant {
        self.received_at
    }
}

/// Why a known dispatch could not be decoded into its typed payload.
///
/// The event still reaches handlers and collectors as
/// [`EventKind::DecodeFailed`], with the original payload in
/// [`Event::raw`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventDecodeError {
    message: Box<str>,
}

impl EventDecodeError {
    pub(crate) fn new(message: impl fmt::Display) -> Self {
        Self {
            message: message.to_string().into_boxed_str(),
        }
    }

    /// The decoder's error message.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for EventDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for EventDecodeError {}

impl From<serde_json::Error> for EventDecodeError {
    fn from(error: serde_json::Error) -> Self {
        Self::new(error)
    }
}

impl From<DiscordError> for EventDecodeError {
    fn from(error: DiscordError) -> Self {
        Self::new(error)
    }
}

/// The decoded payload of one gateway dispatch: one variant per dispatch
/// name.
///
/// Payload storage follows one rule, so the enum stays small (64 bytes on
/// 64-bit targets, fixed by a layout test) and delivery never copies a
/// payload:
///
/// - payloads the cache keeps as they are — messages, members, and
///   GUILD_CREATE's members and presences — are `Arc`s the cache shares;
/// - other large payloads are boxed;
/// - small id-only and delete events are stored inline.
///
/// [`Unknown`](Self::Unknown) is a dispatch this crate has no type for, and
/// [`DecodeFailed`](Self::DecodeFailed) a known one whose payload did not
/// match; both keep the payload in [`Event::raw`].
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum EventKind {
    /// READY.
    Ready(Box<ReadyPayload>),
    /// RESUMED.
    Resumed,
    /// GUILD_CREATE: an available guild split into the units the cache
    /// stores, or an unavailable guild (id only).
    GuildCreate(crate::model::GuildCreatePayload),
    /// GUILD_UPDATE: a typed patch for the cached guild.
    GuildUpdate(Box<crate::model::GuildUpdate>),
    /// GUILD_DELETE.
    GuildDelete(GuildDeletePayload),
    /// CHANNEL_CREATE.
    ChannelCreate(Arc<Channel>),
    /// CHANNEL_UPDATE.
    ChannelUpdate(Arc<Channel>),
    /// CHANNEL_DELETE.
    ChannelDelete(Box<Channel>),
    /// GUILD_MEMBER_ADD.
    MemberAdd(MemberEvent),
    /// GUILD_MEMBER_UPDATE: a typed patch, shared with the external cache
    /// backend's ordered worker.
    MemberUpdate(Arc<crate::model::MemberUpdate>),
    /// GUILD_MEMBER_REMOVE.
    MemberRemove(Box<MemberRemovePayload>),
    /// GUILD_MEMBERS_CHUNK.
    GuildMembersChunk(Box<GuildMembersChunkPayload>),
    /// GUILD_ROLE_CREATE.
    RoleCreate(Box<RoleEvent>),
    /// GUILD_ROLE_UPDATE.
    RoleUpdate(Box<RoleEvent>),
    /// GUILD_ROLE_DELETE.
    RoleDelete(RoleDeletePayload),
    /// MESSAGE_CREATE; the cache stores this `Arc`.
    MessageCreate(Arc<Message>),
    /// MESSAGE_UPDATE, a partial message (see
    /// [`MessageUpdate`](crate::model::MessageUpdate)); shared with the cache
    /// backend worker that merges it.
    MessageUpdate(Arc<crate::model::MessageUpdate>),
    /// MESSAGE_DELETE.
    MessageDelete(MessageDeletePayload),
    /// MESSAGE_DELETE_BULK.
    MessageDeleteBulk(BulkMessageDeleteEvent),
    /// CHANNEL_PINS_UPDATE.
    ChannelPinsUpdate(ChannelPinsUpdateEvent),
    /// GUILD_BAN_ADD.
    GuildBanAdd(Box<GuildBanEvent>),
    /// GUILD_BAN_REMOVE.
    GuildBanRemove(Box<GuildBanEvent>),
    /// GUILD_EMOJIS_UPDATE.
    GuildEmojisUpdate(GuildEmojisUpdateEvent),
    /// GUILD_INTEGRATIONS_UPDATE.
    GuildIntegrationsUpdate(GuildIntegrationsUpdateEvent),
    /// ENTITLEMENT_CREATE.
    EntitlementCreate(Box<Entitlement>),
    /// ENTITLEMENT_UPDATE.
    EntitlementUpdate(Box<Entitlement>),
    /// ENTITLEMENT_DELETE.
    EntitlementDelete(Box<Entitlement>),
    /// SUBSCRIPTION_CREATE.
    SubscriptionCreate(Box<Subscription>),
    /// SUBSCRIPTION_UPDATE.
    SubscriptionUpdate(Box<Subscription>),
    /// SUBSCRIPTION_DELETE.
    SubscriptionDelete(Box<Subscription>),
    /// INTEGRATION_CREATE.
    IntegrationCreate(Box<IntegrationEvent>),
    /// INTEGRATION_UPDATE.
    IntegrationUpdate(Box<IntegrationEvent>),
    /// INTEGRATION_DELETE.
    IntegrationDelete(IntegrationDeleteEvent),
    /// GUILD_SOUNDBOARD_SOUND_CREATE.
    GuildSoundboardSoundCreate(Box<SoundboardSound>),
    /// GUILD_SOUNDBOARD_SOUND_UPDATE.
    GuildSoundboardSoundUpdate(Box<SoundboardSound>),
    /// GUILD_SOUNDBOARD_SOUND_DELETE.
    GuildSoundboardSoundDelete(SoundboardSoundDeleteEvent),
    /// GUILD_SOUNDBOARD_SOUNDS_UPDATE.
    GuildSoundboardSoundsUpdate(SoundboardSoundsEvent),
    /// SOUNDBOARD_SOUNDS.
    SoundboardSounds(SoundboardSoundsEvent),
    /// WEBHOOKS_UPDATE.
    WebhooksUpdate(WebhooksUpdateEvent),
    /// INVITE_CREATE.
    InviteCreate(Box<InviteEvent>),
    /// INVITE_DELETE.
    InviteDelete(Box<InviteEvent>),
    /// MESSAGE_REACTION_ADD.
    MessageReactionAdd(Box<ReactionEvent>),
    /// MESSAGE_REACTION_REMOVE.
    MessageReactionRemove(Box<ReactionEvent>),
    /// MESSAGE_REACTION_REMOVE_ALL.
    MessageReactionRemoveAll(ReactionRemoveAllEvent),
    /// MESSAGE_REACTION_REMOVE_EMOJI.
    MessageReactionRemoveEmoji(Box<ReactionRemoveEmojiEvent>),
    /// TYPING_START.
    TypingStart(TypingStartEvent),
    /// PRESENCE_UPDATE.
    PresenceUpdate(Box<PresenceUpdateEvent>),
    /// USER_UPDATE.
    UserUpdate(Box<User>),
    /// INTERACTION_CREATE.
    InteractionCreate(Box<Interaction>),
    /// VOICE_STATE_UPDATE.
    VoiceStateUpdate(Box<VoiceState>),
    /// VOICE_SERVER_UPDATE.
    VoiceServerUpdate(Box<VoiceServerUpdate>),
    /// THREAD_CREATE.
    ThreadCreate(Box<ThreadEvent>),
    /// THREAD_UPDATE.
    ThreadUpdate(Box<ThreadEvent>),
    /// THREAD_DELETE.
    ThreadDelete(Box<ThreadEvent>),
    /// THREAD_LIST_SYNC.
    ThreadListSync(Box<ThreadListSyncEvent>),
    /// THREAD_MEMBER_UPDATE.
    ThreadMemberUpdate(Box<ThreadMemberUpdateEvent>),
    /// THREAD_MEMBERS_UPDATE.
    ThreadMembersUpdate(Box<ThreadMembersUpdateEvent>),
    /// MESSAGE_POLL_VOTE_ADD.
    MessagePollVoteAdd(PollVoteEvent),
    /// MESSAGE_POLL_VOTE_REMOVE.
    MessagePollVoteRemove(PollVoteEvent),
    /// GUILD_STICKERS_UPDATE.
    GuildStickersUpdate(GuildStickersUpdateEvent),
    /// GUILD_SCHEDULED_EVENT_CREATE.
    GuildScheduledEventCreate(Box<ScheduledEvent>),
    /// GUILD_SCHEDULED_EVENT_UPDATE.
    GuildScheduledEventUpdate(Box<ScheduledEvent>),
    /// GUILD_SCHEDULED_EVENT_DELETE.
    GuildScheduledEventDelete(Box<ScheduledEvent>),
    /// GUILD_SCHEDULED_EVENT_USER_ADD.
    GuildScheduledEventUserAdd(Box<GuildScheduledEventUserEvent>),
    /// GUILD_SCHEDULED_EVENT_USER_REMOVE.
    GuildScheduledEventUserRemove(Box<GuildScheduledEventUserEvent>),
    /// STAGE_INSTANCE_CREATE.
    StageInstanceCreate(Box<StageInstance>),
    /// STAGE_INSTANCE_UPDATE.
    StageInstanceUpdate(Box<StageInstance>),
    /// STAGE_INSTANCE_DELETE.
    StageInstanceDelete(Box<StageInstance>),
    /// VOICE_CHANNEL_EFFECT_SEND.
    VoiceChannelEffectSend(Box<VoiceChannelEffectEvent>),
    /// VOICE_CHANNEL_START_TIME_UPDATE.
    VoiceChannelStartTimeUpdate(VoiceChannelStartTimeUpdateEvent),
    /// VOICE_CHANNEL_STATUS_UPDATE.
    VoiceChannelStatusUpdate(VoiceChannelStatusUpdateEvent),
    /// CHANNEL_INFO.
    ChannelInfo(ChannelInfoEvent),
    /// RATE_LIMITED.
    RateLimited(Box<RateLimitedEvent>),
    /// APPLICATION_COMMAND_PERMISSIONS_UPDATE.
    ApplicationCommandPermissionsUpdate(Box<ApplicationCommandPermissionsUpdateEvent>),
    /// AUTO_MODERATION_RULE_CREATE.
    AutoModerationRuleCreate(Box<AutoModerationEvent>),
    /// AUTO_MODERATION_RULE_UPDATE.
    AutoModerationRuleUpdate(Box<AutoModerationEvent>),
    /// AUTO_MODERATION_RULE_DELETE.
    AutoModerationRuleDelete(Box<AutoModerationEvent>),
    /// AUTO_MODERATION_ACTION_EXECUTION.
    AutoModerationActionExecution(Box<AutoModerationEvent>),
    /// GUILD_AUDIT_LOG_ENTRY_CREATE.
    GuildAuditLogEntryCreate(Box<AuditLogEntryEvent>),
    /// A dispatch this crate has no typed payload for; its name is in
    /// [`Event::name`] and its payload in [`Event::raw`].
    Unknown,
    /// A known dispatch whose payload did not decode; the payload is in
    /// [`Event::raw`].
    DecodeFailed(EventDecodeError),
}

/// One gateway dispatch as handlers and collectors receive it, shared as
/// `Arc<Event>`.
///
/// - [`kind`](Self::kind): the decoded payload.
/// - [`metadata`](Self::metadata): dispatch name, shard, sequence, and when
///   the frame arrived.
/// - [`raw`](Self::raw): the original JSON payload. Known events keep it
///   only when the client was built with `keep_raw_events(true)`; unknown
///   dispatches and payloads that failed to decode always keep it, so
///   nothing Discord sends is lost. It is kept in exactly one place, and
///   `Debug` prints only its length (payloads can carry interaction tokens).
#[derive(Clone)]
pub struct Event {
    kind: EventKind,
    metadata: EventMetadata,
    raw: Option<Box<RawValue>>,
}

impl Event {
    /// Decodes one dispatch payload. Never fails: an unknown dispatch name
    /// gives [`EventKind::Unknown`] and a payload that does not match its
    /// type gives [`EventKind::DecodeFailed`]; both keep `data` as
    /// [`raw`](Self::raw). A decoded known event keeps it only when
    /// `keep_raw` is `true`.
    pub fn decode(metadata: EventMetadata, data: Box<RawValue>, keep_raw: bool) -> Self {
        let kind = decode::decode_kind(metadata.name(), data.get())
            .unwrap_or_else(EventKind::DecodeFailed);
        let keep = keep_raw || matches!(kind, EventKind::Unknown | EventKind::DecodeFailed(_));
        Self {
            kind,
            metadata,
            raw: keep.then_some(data),
        }
    }

    /// The decoded payload.
    pub fn kind(&self) -> &EventKind {
        &self.kind
    }

    /// Takes the decoded payload, dropping the metadata and raw payload.
    pub fn into_kind(self) -> EventKind {
        self.kind
    }

    /// Dispatch name, shard, sequence, and receive time.
    pub fn metadata(&self) -> &EventMetadata {
        &self.metadata
    }

    /// The dispatch name (`t`), for example `"MESSAGE_CREATE"`.
    pub fn name(&self) -> &str {
        self.metadata.name()
    }

    /// The original JSON payload (`d`): always present for
    /// [`EventKind::Unknown`] and [`EventKind::DecodeFailed`], and for known
    /// events only when raw payloads are kept (`keep_raw_events(true)`).
    pub fn raw(&self) -> Option<&RawValue> {
        self.raw.as_deref()
    }
}

#[cfg(all(test, feature = "gateway"))]
impl Event {
    /// An event built straight from a decoded kind (no raw payload), for
    /// tests that exercise the pipeline with hand-made payloads.
    pub(crate) fn from_kind(name: &str, kind: EventKind) -> Self {
        Self {
            kind,
            metadata: EventMetadata::new(name, Instant::now()),
            raw: None,
        }
    }
}

/// Prints the kind and metadata; the raw payload only as its length.
impl fmt::Debug for Event {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Event")
            .field("kind", &self.kind)
            .field("metadata", &self.metadata)
            .field("raw", &RawLength(self.raw.as_deref()))
            .finish()
    }
}

/// `Debug` view of an optional raw payload that never shows its content.
pub(crate) struct RawLength<'a>(pub(crate) Option<&'a RawValue>);

impl fmt::Debug for RawLength<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(raw) => write!(formatter, "Some(<{} bytes>)", raw.get().len()),
            None => formatter.write_str("None"),
        }
    }
}

/// Decodes a dispatch from JSON text, keeping the raw payload, with
/// metadata for shard 0 of 1 received now. For tests and tooling; the
/// gateway uses [`Event::decode`].
///
/// Fails only when `data` is not valid JSON; a payload that does not match
/// gives [`EventKind::DecodeFailed`].
pub fn decode_event(event_name: &str, data: &str) -> Result<Event, DiscordError> {
    let data = RawValue::from_string(data.to_owned())?;
    Ok(Event::decode(
        EventMetadata::new(event_name, Instant::now()),
        data,
        true,
    ))
}

#[path = "event/decode.rs"]
mod decode;

#[cfg(test)]
#[path = "event/tests.rs"]
mod tests;
