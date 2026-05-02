use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::DiscordError;
use crate::model::{
    Activity, AuditLogEntry, Channel, ClientStatus, Entitlement, Guild, Integration, Interaction,
    Member, Message, Presence, Role, Snowflake, SoundboardSound, StageInstance, Sticker,
    Subscription, User, VoiceServerUpdate, VoiceState,
};
use crate::parsers::parse_interaction;
use crate::types::Emoji;

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `ReadyApplication`.
pub struct ReadyApplication {
    /// Discord API payload field `id`.
    pub id: Snowflake,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `ReadyPayload`.
pub struct ReadyPayload {
    /// Discord API payload field `user`.
    pub user: User,
    /// Discord API payload field `session_id`.
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `application`.
    pub application: Option<ReadyApplication>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `resume_gateway_url`.
    pub resume_gateway_url: Option<String>,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `ReadyEvent`.
pub struct ReadyEvent {
    /// Discord API payload field `data`.
    pub data: ReadyPayload,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `GuildEvent`.
pub struct GuildEvent {
    /// Discord API payload field `guild`.
    pub guild: Guild,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `GuildDeletePayload`.
pub struct GuildDeletePayload {
    /// Discord API payload field `id`.
    pub id: Snowflake,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `unavailable`.
    pub unavailable: Option<bool>,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `GuildDeleteEvent`.
pub struct GuildDeleteEvent {
    /// Discord API payload field `data`.
    pub data: GuildDeletePayload,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `ChannelEvent`.
pub struct ChannelEvent {
    /// Discord API payload field `channel`.
    pub channel: Channel,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `MemberEvent`.
pub struct MemberEvent {
    /// Discord API payload field `guild_id`.
    pub guild_id: Snowflake,
    /// Discord API payload field `member`.
    pub member: Member,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `MemberRemovePayload`.
pub struct MemberRemovePayload {
    /// Discord API payload field `guild_id`.
    pub guild_id: Snowflake,
    /// Discord API payload field `user`.
    pub user: User,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `MemberRemoveEvent`.
pub struct MemberRemoveEvent {
    /// Discord API payload field `data`.
    pub data: MemberRemovePayload,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `GuildMembersChunkPayload`.
pub struct GuildMembersChunkPayload {
    /// Discord API payload field `guild_id`.
    pub guild_id: Snowflake,
    #[serde(default)]
    /// Discord API payload field `members`.
    pub members: Vec<Member>,
    /// Discord API payload field `chunk_index`.
    pub chunk_index: u64,
    /// Discord API payload field `chunk_count`.
    pub chunk_count: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    /// Discord API payload field `not_found`.
    pub not_found: Vec<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `presences`.
    pub presences: Option<Vec<Presence>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `nonce`.
    pub nonce: Option<String>,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `GuildMembersChunkEvent`.
pub struct GuildMembersChunkEvent {
    /// Discord API payload field `data`.
    pub data: GuildMembersChunkPayload,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `RoleEvent`.
pub struct RoleEvent {
    /// Discord API payload field `guild_id`.
    pub guild_id: Snowflake,
    /// Discord API payload field `role`.
    pub role: Role,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `RoleDeletePayload`.
pub struct RoleDeletePayload {
    /// Discord API payload field `guild_id`.
    pub guild_id: Snowflake,
    /// Discord API payload field `role_id`.
    pub role_id: Snowflake,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `RoleDeleteEvent`.
pub struct RoleDeleteEvent {
    /// Discord API payload field `data`.
    pub data: RoleDeletePayload,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `MessageEvent`.
pub struct MessageEvent {
    /// Discord API payload field `message`.
    pub message: Message,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `MessageDeletePayload`.
pub struct MessageDeletePayload {
    /// Discord API payload field `id`.
    pub id: Snowflake,
    /// Discord API payload field `channel_id`.
    pub channel_id: Snowflake,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `guild_id`.
    pub guild_id: Option<Snowflake>,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `MessageDeleteEvent`.
pub struct MessageDeleteEvent {
    /// Discord API payload field `data`.
    pub data: MessageDeletePayload,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `InteractionEvent`.
pub struct InteractionEvent {
    /// Discord API payload field `interaction`.
    pub interaction: Interaction,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `VoiceStateEvent`.
pub struct VoiceStateEvent {
    /// Discord API payload field `state`.
    pub state: VoiceState,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `VoiceServerEvent`.
pub struct VoiceServerEvent {
    /// Discord API payload field `data`.
    pub data: VoiceServerUpdate,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `ResumedEvent`.
pub struct ResumedEvent {
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `BulkMessageDeleteEvent`.
pub struct BulkMessageDeleteEvent {
    /// Discord API payload field `ids`.
    pub ids: Vec<Snowflake>,
    /// Discord API payload field `channel_id`.
    pub channel_id: Snowflake,
    /// Discord API payload field `guild_id`.
    pub guild_id: Option<Snowflake>,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `ChannelPinsUpdateEvent`.
pub struct ChannelPinsUpdateEvent {
    /// Discord API payload field `channel_id`.
    pub channel_id: Snowflake,
    /// Discord API payload field `guild_id`.
    pub guild_id: Option<Snowflake>,
    /// Discord API payload field `last_pin_timestamp`.
    pub last_pin_timestamp: Option<String>,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `GuildBanEvent`.
pub struct GuildBanEvent {
    /// Discord API payload field `guild_id`.
    pub guild_id: Snowflake,
    /// Discord API payload field `user`.
    pub user: User,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `GuildEmojisUpdateEvent`.
pub struct GuildEmojisUpdateEvent {
    /// Discord API payload field `guild_id`.
    pub guild_id: Snowflake,
    /// Discord API payload field `emojis`.
    pub emojis: Vec<Emoji>,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `EntitlementEvent`.
pub struct EntitlementEvent {
    /// Discord API payload field `entitlement`.
    pub entitlement: Entitlement,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `SubscriptionEvent`.
pub struct SubscriptionEvent {
    /// Discord API payload field `subscription`.
    pub subscription: Subscription,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `IntegrationEvent`.
pub struct IntegrationEvent {
    /// Discord API payload field `integration`.
    pub integration: Integration,
    /// Discord API payload field `guild_id`.
    pub guild_id: Option<Snowflake>,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `IntegrationDeleteEvent`.
pub struct IntegrationDeleteEvent {
    /// Discord API payload field `id`.
    pub id: Option<Snowflake>,
    /// Discord API payload field `guild_id`.
    pub guild_id: Option<Snowflake>,
    /// Discord API payload field `application_id`.
    pub application_id: Option<Snowflake>,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `PollVoteEvent`.
pub struct PollVoteEvent {
    /// Discord API payload field `user_id`.
    pub user_id: Option<Snowflake>,
    /// Discord API payload field `channel_id`.
    pub channel_id: Option<Snowflake>,
    /// Discord API payload field `message_id`.
    pub message_id: Option<Snowflake>,
    /// Discord API payload field `guild_id`.
    pub guild_id: Option<Snowflake>,
    /// Discord API payload field `answer_id`.
    pub answer_id: Option<u64>,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `SoundboardSoundEvent`.
pub struct SoundboardSoundEvent {
    /// Discord API payload field `sound`.
    pub sound: SoundboardSound,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `SoundboardSoundDeleteEvent`.
pub struct SoundboardSoundDeleteEvent {
    /// Discord API payload field `sound_id`.
    pub sound_id: Snowflake,
    /// Discord API payload field `guild_id`.
    pub guild_id: Snowflake,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `SoundboardSoundsEvent`.
pub struct SoundboardSoundsEvent {
    /// Discord API payload field `guild_id`.
    pub guild_id: Snowflake,
    /// Discord API payload field `soundboard_sounds`.
    pub soundboard_sounds: Vec<SoundboardSound>,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `InviteEvent`.
pub struct InviteEvent {
    /// Discord API payload field `guild_id`.
    pub guild_id: Option<Snowflake>,
    /// Discord API payload field `channel_id`.
    pub channel_id: Option<Snowflake>,
    /// Discord API payload field `code`.
    pub code: Option<String>,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `ReactionEvent`.
pub struct ReactionEvent {
    /// Discord API payload field `user_id`.
    pub user_id: Option<Snowflake>,
    /// Discord API payload field `channel_id`.
    pub channel_id: Option<Snowflake>,
    /// Discord API payload field `message_id`.
    pub message_id: Option<Snowflake>,
    /// Discord API payload field `guild_id`.
    pub guild_id: Option<Snowflake>,
    /// Discord API payload field `member`.
    pub member: Option<Member>,
    /// Discord API payload field `message_author_id`.
    pub message_author_id: Option<Snowflake>,
    /// Discord API payload field `emoji`.
    pub emoji: Option<Emoji>,
    /// Discord API payload field `burst`.
    pub burst: Option<bool>,
    /// Discord API payload field `burst_colors`.
    pub burst_colors: Vec<String>,
    /// Discord API payload field `reaction_type`.
    pub reaction_type: Option<u64>,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `ReactionRemoveAllEvent`.
pub struct ReactionRemoveAllEvent {
    /// Discord API payload field `channel_id`.
    pub channel_id: Option<Snowflake>,
    /// Discord API payload field `message_id`.
    pub message_id: Option<Snowflake>,
    /// Discord API payload field `guild_id`.
    pub guild_id: Option<Snowflake>,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `TypingStartEvent`.
pub struct TypingStartEvent {
    /// Discord API payload field `channel_id`.
    pub channel_id: Option<Snowflake>,
    /// Discord API payload field `guild_id`.
    pub guild_id: Option<Snowflake>,
    /// Discord API payload field `user_id`.
    pub user_id: Option<Snowflake>,
    /// Discord API payload field `timestamp`.
    pub timestamp: Option<u64>,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `PresenceUpdateEvent`.
pub struct PresenceUpdateEvent {
    /// Discord API payload field `user`.
    pub user: Option<PresenceUpdateUser>,
    /// Discord API payload field `user_id`.
    pub user_id: Option<Snowflake>,
    /// Discord API payload field `guild_id`.
    pub guild_id: Option<Snowflake>,
    /// Discord API payload field `status`.
    pub status: Option<String>,
    /// Discord API payload field `activities`.
    pub activities: Vec<Activity>,
    /// Discord API payload field `client_status`.
    pub client_status: Option<ClientStatus>,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `PresenceUpdateUser`.
pub struct PresenceUpdateUser {
    /// Discord API payload field `id`.
    pub id: Snowflake,
    /// Discord API payload field `username`.
    pub username: Option<String>,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `UserUpdateEvent`.
pub struct UserUpdateEvent {
    /// Discord API payload field `user`.
    pub user: User,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `WebhooksUpdateEvent`.
pub struct WebhooksUpdateEvent {
    /// Discord API payload field `guild_id`.
    pub guild_id: Option<Snowflake>,
    /// Discord API payload field `channel_id`.
    pub channel_id: Option<Snowflake>,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `GuildIntegrationsUpdateEvent`.
pub struct GuildIntegrationsUpdateEvent {
    /// Discord API payload field `guild_id`.
    pub guild_id: Option<Snowflake>,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `ThreadEvent`.
pub struct ThreadEvent {
    /// Discord API payload field `thread`.
    pub thread: Channel,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `ThreadMemberUpdateEvent`.
pub struct ThreadMemberUpdateEvent {
    /// Discord API payload field `guild_id`.
    pub guild_id: Option<Snowflake>,
    /// Discord API payload field `thread_id`.
    pub thread_id: Option<Snowflake>,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `ThreadMembersUpdateEvent`.
pub struct ThreadMembersUpdateEvent {
    /// Discord API payload field `guild_id`.
    pub guild_id: Option<Snowflake>,
    /// Discord API payload field `thread_id`.
    pub thread_id: Option<Snowflake>,
    /// Discord API payload field `added_members`.
    pub added_members: Option<Vec<serde_json::Value>>,
    /// Discord API payload field `removed_member_ids`.
    pub removed_member_ids: Option<Vec<Snowflake>>,
    /// Discord API payload field `member_count`.
    pub member_count: Option<u64>,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `ThreadListSyncEvent`.
pub struct ThreadListSyncEvent {
    /// Discord API payload field `guild_id`.
    pub guild_id: Option<Snowflake>,
    /// Discord API payload field `threads`.
    pub threads: Vec<Channel>,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `ReactionRemoveEmojiEvent`.
pub struct ReactionRemoveEmojiEvent {
    /// Discord API payload field `channel_id`.
    pub channel_id: Option<Snowflake>,
    /// Discord API payload field `message_id`.
    pub message_id: Option<Snowflake>,
    /// Discord API payload field `guild_id`.
    pub guild_id: Option<Snowflake>,
    /// Discord API payload field `emoji`.
    pub emoji: Option<Emoji>,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `GuildStickersUpdateEvent`.
pub struct GuildStickersUpdateEvent {
    /// Discord API payload field `guild_id`.
    pub guild_id: Option<Snowflake>,
    /// Discord API payload field `stickers`.
    pub stickers: Vec<Sticker>,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `ScheduledEvent`.
pub struct ScheduledEvent {
    /// Discord API payload field `id`.
    pub id: Option<Snowflake>,
    /// Discord API payload field `guild_id`.
    pub guild_id: Option<Snowflake>,
    /// Discord API payload field `channel_id`.
    pub channel_id: Option<Snowflake>,
    /// Discord API payload field `creator_id`.
    pub creator_id: Option<Snowflake>,
    /// Discord API payload field `name`.
    pub name: Option<String>,
    /// Discord API payload field `description`.
    pub description: Option<String>,
    /// Discord API payload field `scheduled_start_time`.
    pub scheduled_start_time: Option<String>,
    /// Discord API payload field `scheduled_end_time`.
    pub scheduled_end_time: Option<String>,
    /// Discord API payload field `privacy_level`.
    pub privacy_level: Option<u64>,
    /// Discord API payload field `status`.
    pub status: Option<u64>,
    /// Discord API payload field `entity_type`.
    pub entity_type: Option<u64>,
    /// Discord API payload field `entity_id`.
    pub entity_id: Option<Snowflake>,
    /// Discord API payload field `entity_metadata`.
    pub entity_metadata: Option<Value>,
    /// Discord API payload field `user_count`.
    pub user_count: Option<u64>,
    /// Discord API payload field `image`.
    pub image: Option<String>,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `GuildScheduledEventUserEvent`.
pub struct GuildScheduledEventUserEvent {
    /// Discord API payload field `guild_scheduled_event_id`.
    pub guild_scheduled_event_id: Snowflake,
    /// Discord API payload field `user_id`.
    pub user_id: Snowflake,
    /// Discord API payload field `guild_id`.
    pub guild_id: Snowflake,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `member`.
    pub member: Option<Member>,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Discord API payload field `user`.
    pub user: Option<User>,
    #[serde(skip)]
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `StageInstanceEvent`.
pub struct StageInstanceEvent {
    /// Discord API payload field `stage_instance`.
    pub stage_instance: StageInstance,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `ApplicationCommandPermissionsUpdateEvent`.
pub struct ApplicationCommandPermissionsUpdateEvent {
    /// Discord API payload field `id`.
    pub id: Option<Snowflake>,
    /// Discord API payload field `application_id`.
    pub application_id: Option<Snowflake>,
    /// Discord API payload field `guild_id`.
    pub guild_id: Option<Snowflake>,
    /// Discord API payload field `permissions`.
    pub permissions: Vec<Value>,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `VoiceChannelEffectEvent`.
pub struct VoiceChannelEffectEvent {
    /// Discord API payload field `channel_id`.
    pub channel_id: Option<Snowflake>,
    /// Discord API payload field `guild_id`.
    pub guild_id: Option<Snowflake>,
    /// Discord API payload field `user_id`.
    pub user_id: Option<Snowflake>,
    /// Discord API payload field `emoji`.
    pub emoji: Option<Emoji>,
    /// Discord API payload field `animation_type`.
    pub animation_type: Option<u64>,
    /// Discord API payload field `animation_id`.
    pub animation_id: Option<u64>,
    /// Discord API payload field `sound_id`.
    pub sound_id: Option<Snowflake>,
    /// Discord API payload field `sound_volume`.
    pub sound_volume: Option<f64>,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `VoiceChannelStartTimeUpdateEvent`.
pub struct VoiceChannelStartTimeUpdateEvent {
    /// Discord API payload field `channel_id`.
    pub channel_id: Option<Snowflake>,
    /// Discord API payload field `guild_id`.
    pub guild_id: Option<Snowflake>,
    /// Discord API payload field `voice_channel_start_time`.
    pub voice_channel_start_time: Option<String>,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `VoiceChannelStatusUpdateEvent`.
pub struct VoiceChannelStatusUpdateEvent {
    /// Discord API payload field `channel_id`.
    pub channel_id: Option<Snowflake>,
    /// Discord API payload field `guild_id`.
    pub guild_id: Option<Snowflake>,
    /// Discord API payload field `status`.
    pub status: Option<String>,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `ChannelInfoChannel`.
pub struct ChannelInfoChannel {
    /// Channel ID whose metadata was returned.
    pub id: Snowflake,
    /// Optional channel status value.
    pub status: Option<String>,
    /// Optional voice-channel start time value.
    pub voice_start_time: Option<String>,
    /// Raw channel-info object for forward-compatible access.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `ChannelInfoEvent`.
pub struct ChannelInfoEvent {
    /// Guild ID for the channel-info dispatch.
    pub guild_id: Snowflake,
    /// Channel metadata objects included by Discord.
    pub channels: Vec<ChannelInfoChannel>,
    /// Raw event payload for forward-compatible access.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `RateLimitedEvent`.
pub struct RateLimitedEvent {
    /// Discord API payload field `opcode`.
    pub opcode: Option<u64>,
    /// Discord API payload field `retry_after`.
    pub retry_after: Option<f64>,
    /// Discord API payload field `meta`.
    pub meta: Option<Value>,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `AutoModerationEvent`.
pub struct AutoModerationEvent {
    /// Discord API payload field `id`.
    pub id: Option<Snowflake>,
    /// Discord API payload field `guild_id`.
    pub guild_id: Option<Snowflake>,
    /// Discord API payload field `name`.
    pub name: Option<String>,
    /// Discord API payload field `creator_id`.
    pub creator_id: Option<Snowflake>,
    /// Discord API payload field `event_type`.
    pub event_type: Option<u64>,
    /// Discord API payload field `trigger_type`.
    pub trigger_type: Option<u64>,
    /// Discord API payload field `trigger_metadata`.
    pub trigger_metadata: Option<Value>,
    /// Discord API payload field `actions`.
    pub actions: Vec<Value>,
    /// Discord API payload field `enabled`.
    pub enabled: Option<bool>,
    /// Discord API payload field `exempt_roles`.
    pub exempt_roles: Vec<Snowflake>,
    /// Discord API payload field `exempt_channels`.
    pub exempt_channels: Vec<Snowflake>,
    /// Discord API payload field `action`.
    pub action: Option<Value>,
    /// Discord API payload field `rule_id`.
    pub rule_id: Option<Snowflake>,
    /// Discord API payload field `rule_trigger_type`.
    pub rule_trigger_type: Option<u64>,
    /// Discord API payload field `user_id`.
    pub user_id: Option<Snowflake>,
    /// Discord API payload field `channel_id`.
    pub channel_id: Option<Snowflake>,
    /// Discord API payload field `message_id`.
    pub message_id: Option<Snowflake>,
    /// Discord API payload field `alert_system_message_id`.
    pub alert_system_message_id: Option<Snowflake>,
    /// Discord API payload field `content`.
    pub content: Option<String>,
    /// Discord API payload field `matched_keyword`.
    pub matched_keyword: Option<String>,
    /// Discord API payload field `matched_content`.
    pub matched_content: Option<String>,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `AuditLogEntryEvent`.
pub struct AuditLogEntryEvent {
    /// Discord API payload field `guild_id`.
    pub guild_id: Option<Snowflake>,
    /// Discord API payload field `entry`.
    pub entry: Option<AuditLogEntry>,
    /// Discord API payload field `id`.
    pub id: Option<Snowflake>,
    /// Discord API payload field `user_id`.
    pub user_id: Option<Snowflake>,
    /// Discord API payload field `target_id`.
    pub target_id: Option<Snowflake>,
    /// Discord API payload field `action_type`.
    pub action_type: Option<u64>,
    /// Discord API payload field `changes`.
    pub changes: Option<Vec<Value>>,
    /// Discord API payload field `options`.
    pub options: Option<Value>,
    /// Discord API payload field `reason`.
    pub reason: Option<String>,
    /// Discord API payload field `raw`.
    pub raw: Value,
}

#[derive(Clone, Debug)]
#[non_exhaustive]
/// Typed Discord API enum for `Event`.
pub enum Event {
    /// Discord API enum variant `Ready`.
    Ready(ReadyEvent),
    /// Discord API enum variant `GuildCreate`.
    GuildCreate(GuildEvent),
    /// Discord API enum variant `GuildUpdate`.
    GuildUpdate(GuildEvent),
    /// Discord API enum variant `GuildDelete`.
    GuildDelete(GuildDeleteEvent),
    /// Discord API enum variant `ChannelCreate`.
    ChannelCreate(ChannelEvent),
    /// Discord API enum variant `ChannelUpdate`.
    ChannelUpdate(ChannelEvent),
    /// Discord API enum variant `ChannelDelete`.
    ChannelDelete(ChannelEvent),
    /// Discord API enum variant `MemberAdd`.
    MemberAdd(MemberEvent),
    /// Discord API enum variant `MemberUpdate`.
    MemberUpdate(MemberEvent),
    /// Discord API enum variant `MemberRemove`.
    MemberRemove(MemberRemoveEvent),
    /// Discord API enum variant `GuildMembersChunk`.
    GuildMembersChunk(GuildMembersChunkEvent),
    /// Discord API enum variant `RoleCreate`.
    RoleCreate(RoleEvent),
    /// Discord API enum variant `RoleUpdate`.
    RoleUpdate(RoleEvent),
    /// Discord API enum variant `RoleDelete`.
    RoleDelete(RoleDeleteEvent),
    /// Discord API enum variant `MessageCreate`.
    MessageCreate(MessageEvent),
    /// Discord API enum variant `MessageUpdate`.
    MessageUpdate(MessageEvent),
    /// Discord API enum variant `MessageDelete`.
    MessageDelete(MessageDeleteEvent),
    /// Discord API enum variant `MessageDeleteBulk`.
    MessageDeleteBulk(BulkMessageDeleteEvent),
    /// Discord API enum variant `ChannelPinsUpdate`.
    ChannelPinsUpdate(ChannelPinsUpdateEvent),
    /// Discord API enum variant `GuildBanAdd`.
    GuildBanAdd(GuildBanEvent),
    /// Discord API enum variant `GuildBanRemove`.
    GuildBanRemove(GuildBanEvent),
    /// Discord API enum variant `GuildEmojisUpdate`.
    GuildEmojisUpdate(GuildEmojisUpdateEvent),
    /// Discord API enum variant `GuildIntegrationsUpdate`.
    GuildIntegrationsUpdate(GuildIntegrationsUpdateEvent),
    /// Discord API enum variant `EntitlementCreate`.
    EntitlementCreate(EntitlementEvent),
    /// Discord API enum variant `EntitlementUpdate`.
    EntitlementUpdate(EntitlementEvent),
    /// Discord API enum variant `EntitlementDelete`.
    EntitlementDelete(EntitlementEvent),
    /// Discord API enum variant `SubscriptionCreate`.
    SubscriptionCreate(SubscriptionEvent),
    /// Discord API enum variant `SubscriptionUpdate`.
    SubscriptionUpdate(SubscriptionEvent),
    /// Discord API enum variant `SubscriptionDelete`.
    SubscriptionDelete(SubscriptionEvent),
    /// Discord API enum variant `IntegrationCreate`.
    IntegrationCreate(IntegrationEvent),
    /// Discord API enum variant `IntegrationUpdate`.
    IntegrationUpdate(IntegrationEvent),
    /// Discord API enum variant `IntegrationDelete`.
    IntegrationDelete(IntegrationDeleteEvent),
    /// Discord API enum variant `GuildSoundboardSoundCreate`.
    GuildSoundboardSoundCreate(SoundboardSoundEvent),
    /// Discord API enum variant `GuildSoundboardSoundUpdate`.
    GuildSoundboardSoundUpdate(SoundboardSoundEvent),
    /// Discord API enum variant `GuildSoundboardSoundDelete`.
    GuildSoundboardSoundDelete(SoundboardSoundDeleteEvent),
    /// Discord API enum variant `GuildSoundboardSoundsUpdate`.
    GuildSoundboardSoundsUpdate(SoundboardSoundsEvent),
    /// Discord API enum variant `SoundboardSounds`.
    SoundboardSounds(SoundboardSoundsEvent),
    /// Discord API enum variant `WebhooksUpdate`.
    WebhooksUpdate(WebhooksUpdateEvent),
    /// Discord API enum variant `InviteCreate`.
    InviteCreate(InviteEvent),
    /// Discord API enum variant `InviteDelete`.
    InviteDelete(InviteEvent),
    /// Discord API enum variant `MessageReactionAdd`.
    MessageReactionAdd(ReactionEvent),
    /// Discord API enum variant `MessageReactionRemove`.
    MessageReactionRemove(ReactionEvent),
    /// Discord API enum variant `MessageReactionRemoveAll`.
    MessageReactionRemoveAll(ReactionRemoveAllEvent),
    /// Discord API enum variant `TypingStart`.
    TypingStart(TypingStartEvent),
    /// Discord API enum variant `PresenceUpdate`.
    PresenceUpdate(PresenceUpdateEvent),
    /// Discord API enum variant `UserUpdate`.
    UserUpdate(UserUpdateEvent),
    /// Discord API enum variant `InteractionCreate`.
    InteractionCreate(InteractionEvent),
    /// Discord API enum variant `VoiceStateUpdate`.
    VoiceStateUpdate(VoiceStateEvent),
    /// Discord API enum variant `VoiceServerUpdate`.
    VoiceServerUpdate(VoiceServerEvent),
    /// Discord API enum variant `Resumed`.
    Resumed(ResumedEvent),
    /// Discord API enum variant `ThreadCreate`.
    ThreadCreate(ThreadEvent),
    /// Discord API enum variant `ThreadUpdate`.
    ThreadUpdate(ThreadEvent),
    /// Discord API enum variant `ThreadDelete`.
    ThreadDelete(ThreadEvent),
    /// Discord API enum variant `ThreadListSync`.
    ThreadListSync(ThreadListSyncEvent),
    /// Discord API enum variant `ThreadMemberUpdate`.
    ThreadMemberUpdate(ThreadMemberUpdateEvent),
    /// Discord API enum variant `ThreadMembersUpdate`.
    ThreadMembersUpdate(ThreadMembersUpdateEvent),
    /// Discord API enum variant `MessageReactionRemoveEmoji`.
    MessageReactionRemoveEmoji(ReactionRemoveEmojiEvent),
    /// Discord API enum variant `MessagePollVoteAdd`.
    MessagePollVoteAdd(PollVoteEvent),
    /// Discord API enum variant `MessagePollVoteRemove`.
    MessagePollVoteRemove(PollVoteEvent),
    /// Discord API enum variant `GuildStickersUpdate`.
    GuildStickersUpdate(GuildStickersUpdateEvent),
    /// Discord API enum variant `GuildScheduledEventCreate`.
    GuildScheduledEventCreate(ScheduledEvent),
    /// Discord API enum variant `GuildScheduledEventUpdate`.
    GuildScheduledEventUpdate(ScheduledEvent),
    /// Discord API enum variant `GuildScheduledEventDelete`.
    GuildScheduledEventDelete(ScheduledEvent),
    /// Discord API enum variant `GuildScheduledEventUserAdd`.
    GuildScheduledEventUserAdd(GuildScheduledEventUserEvent),
    /// Discord API enum variant `GuildScheduledEventUserRemove`.
    GuildScheduledEventUserRemove(GuildScheduledEventUserEvent),
    /// Discord API enum variant `StageInstanceCreate`.
    StageInstanceCreate(StageInstanceEvent),
    /// Discord API enum variant `StageInstanceUpdate`.
    StageInstanceUpdate(StageInstanceEvent),
    /// Discord API enum variant `StageInstanceDelete`.
    StageInstanceDelete(StageInstanceEvent),
    /// Discord API enum variant `VoiceChannelEffectSend`.
    VoiceChannelEffectSend(VoiceChannelEffectEvent),
    /// Discord API enum variant `VoiceChannelStartTimeUpdate`.
    VoiceChannelStartTimeUpdate(VoiceChannelStartTimeUpdateEvent),
    /// Discord API enum variant `VoiceChannelStatusUpdate`.
    VoiceChannelStatusUpdate(VoiceChannelStatusUpdateEvent),
    /// Discord API enum variant `ChannelInfo`.
    ChannelInfo(ChannelInfoEvent),
    /// Discord API enum variant `RateLimited`.
    RateLimited(RateLimitedEvent),
    /// Discord API enum variant `ApplicationCommandPermissionsUpdate`.
    ApplicationCommandPermissionsUpdate(ApplicationCommandPermissionsUpdateEvent),
    /// Discord API enum variant `AutoModerationRuleCreate`.
    AutoModerationRuleCreate(AutoModerationEvent),
    /// Discord API enum variant `AutoModerationRuleUpdate`.
    AutoModerationRuleUpdate(AutoModerationEvent),
    /// Discord API enum variant `AutoModerationRuleDelete`.
    AutoModerationRuleDelete(AutoModerationEvent),
    /// Discord API enum variant `AutoModerationActionExecution`.
    AutoModerationActionExecution(AutoModerationEvent),
    /// Discord API enum variant `GuildAuditLogEntryCreate`.
    GuildAuditLogEntryCreate(AuditLogEntryEvent),
    /// Discord API enum variant `Unknown`.
    Unknown {
        /// Raw Discord dispatch type.
        kind: String,
        /// Raw dispatch payload for unsupported event types.
        raw: Value,
    },
}

impl Event {
    /// Runs the `kind` operation.
    pub fn kind(&self) -> &str {
        match self {
            Event::Ready(_) => "READY",
            Event::GuildCreate(_) => "GUILD_CREATE",
            Event::GuildUpdate(_) => "GUILD_UPDATE",
            Event::GuildDelete(_) => "GUILD_DELETE",
            Event::ChannelCreate(_) => "CHANNEL_CREATE",
            Event::ChannelUpdate(_) => "CHANNEL_UPDATE",
            Event::ChannelDelete(_) => "CHANNEL_DELETE",
            Event::MemberAdd(_) => "GUILD_MEMBER_ADD",
            Event::MemberUpdate(_) => "GUILD_MEMBER_UPDATE",
            Event::MemberRemove(_) => "GUILD_MEMBER_REMOVE",
            Event::GuildMembersChunk(_) => "GUILD_MEMBERS_CHUNK",
            Event::RoleCreate(_) => "GUILD_ROLE_CREATE",
            Event::RoleUpdate(_) => "GUILD_ROLE_UPDATE",
            Event::RoleDelete(_) => "GUILD_ROLE_DELETE",
            Event::MessageCreate(_) => "MESSAGE_CREATE",
            Event::MessageUpdate(_) => "MESSAGE_UPDATE",
            Event::MessageDelete(_) => "MESSAGE_DELETE",
            Event::MessageDeleteBulk(_) => "MESSAGE_DELETE_BULK",
            Event::ChannelPinsUpdate(_) => "CHANNEL_PINS_UPDATE",
            Event::GuildBanAdd(_) => "GUILD_BAN_ADD",
            Event::GuildBanRemove(_) => "GUILD_BAN_REMOVE",
            Event::GuildEmojisUpdate(_) => "GUILD_EMOJIS_UPDATE",
            Event::GuildIntegrationsUpdate(_) => "GUILD_INTEGRATIONS_UPDATE",
            Event::EntitlementCreate(_) => "ENTITLEMENT_CREATE",
            Event::EntitlementUpdate(_) => "ENTITLEMENT_UPDATE",
            Event::EntitlementDelete(_) => "ENTITLEMENT_DELETE",
            Event::SubscriptionCreate(_) => "SUBSCRIPTION_CREATE",
            Event::SubscriptionUpdate(_) => "SUBSCRIPTION_UPDATE",
            Event::SubscriptionDelete(_) => "SUBSCRIPTION_DELETE",
            Event::IntegrationCreate(_) => "INTEGRATION_CREATE",
            Event::IntegrationUpdate(_) => "INTEGRATION_UPDATE",
            Event::IntegrationDelete(_) => "INTEGRATION_DELETE",
            Event::GuildSoundboardSoundCreate(_) => "GUILD_SOUNDBOARD_SOUND_CREATE",
            Event::GuildSoundboardSoundUpdate(_) => "GUILD_SOUNDBOARD_SOUND_UPDATE",
            Event::GuildSoundboardSoundDelete(_) => "GUILD_SOUNDBOARD_SOUND_DELETE",
            Event::GuildSoundboardSoundsUpdate(_) => "GUILD_SOUNDBOARD_SOUNDS_UPDATE",
            Event::SoundboardSounds(_) => "SOUNDBOARD_SOUNDS",
            Event::WebhooksUpdate(_) => "WEBHOOKS_UPDATE",
            Event::InviteCreate(_) => "INVITE_CREATE",
            Event::InviteDelete(_) => "INVITE_DELETE",
            Event::MessageReactionAdd(_) => "MESSAGE_REACTION_ADD",
            Event::MessageReactionRemove(_) => "MESSAGE_REACTION_REMOVE",
            Event::MessageReactionRemoveAll(_) => "MESSAGE_REACTION_REMOVE_ALL",
            Event::TypingStart(_) => "TYPING_START",
            Event::PresenceUpdate(_) => "PRESENCE_UPDATE",
            Event::UserUpdate(_) => "USER_UPDATE",
            Event::InteractionCreate(_) => "INTERACTION_CREATE",
            Event::VoiceStateUpdate(_) => "VOICE_STATE_UPDATE",
            Event::VoiceServerUpdate(_) => "VOICE_SERVER_UPDATE",
            Event::Resumed(_) => "RESUMED",
            Event::ThreadCreate(_) => "THREAD_CREATE",
            Event::ThreadUpdate(_) => "THREAD_UPDATE",
            Event::ThreadDelete(_) => "THREAD_DELETE",
            Event::ThreadListSync(_) => "THREAD_LIST_SYNC",
            Event::ThreadMemberUpdate(_) => "THREAD_MEMBER_UPDATE",
            Event::ThreadMembersUpdate(_) => "THREAD_MEMBERS_UPDATE",
            Event::MessageReactionRemoveEmoji(_) => "MESSAGE_REACTION_REMOVE_EMOJI",
            Event::MessagePollVoteAdd(_) => "MESSAGE_POLL_VOTE_ADD",
            Event::MessagePollVoteRemove(_) => "MESSAGE_POLL_VOTE_REMOVE",
            Event::GuildStickersUpdate(_) => "GUILD_STICKERS_UPDATE",
            Event::GuildScheduledEventCreate(_) => "GUILD_SCHEDULED_EVENT_CREATE",
            Event::GuildScheduledEventUpdate(_) => "GUILD_SCHEDULED_EVENT_UPDATE",
            Event::GuildScheduledEventDelete(_) => "GUILD_SCHEDULED_EVENT_DELETE",
            Event::GuildScheduledEventUserAdd(_) => "GUILD_SCHEDULED_EVENT_USER_ADD",
            Event::GuildScheduledEventUserRemove(_) => "GUILD_SCHEDULED_EVENT_USER_REMOVE",
            Event::StageInstanceCreate(_) => "STAGE_INSTANCE_CREATE",
            Event::StageInstanceUpdate(_) => "STAGE_INSTANCE_UPDATE",
            Event::StageInstanceDelete(_) => "STAGE_INSTANCE_DELETE",
            Event::VoiceChannelEffectSend(_) => "VOICE_CHANNEL_EFFECT_SEND",
            Event::VoiceChannelStartTimeUpdate(_) => "VOICE_CHANNEL_START_TIME_UPDATE",
            Event::VoiceChannelStatusUpdate(_) => "VOICE_CHANNEL_STATUS_UPDATE",
            Event::ChannelInfo(_) => "CHANNEL_INFO",
            Event::RateLimited(_) => "RATE_LIMITED",
            Event::ApplicationCommandPermissionsUpdate(_) => {
                "APPLICATION_COMMAND_PERMISSIONS_UPDATE"
            }
            Event::AutoModerationRuleCreate(_) => "AUTO_MODERATION_RULE_CREATE",
            Event::AutoModerationRuleUpdate(_) => "AUTO_MODERATION_RULE_UPDATE",
            Event::AutoModerationRuleDelete(_) => "AUTO_MODERATION_RULE_DELETE",
            Event::AutoModerationActionExecution(_) => "AUTO_MODERATION_ACTION_EXECUTION",
            Event::GuildAuditLogEntryCreate(_) => "GUILD_AUDIT_LOG_ENTRY_CREATE",
            Event::Unknown { kind, .. } => kind.as_str(),
        }
    }

    /// Runs the `raw` operation.
    pub fn raw(&self) -> &Value {
        match self {
            Event::Ready(event) => &event.raw,
            Event::GuildCreate(event) | Event::GuildUpdate(event) => &event.raw,
            Event::GuildDelete(event) => &event.raw,
            Event::ChannelCreate(event)
            | Event::ChannelUpdate(event)
            | Event::ChannelDelete(event) => &event.raw,
            Event::MemberAdd(event) | Event::MemberUpdate(event) => &event.raw,
            Event::MemberRemove(event) => &event.raw,
            Event::GuildMembersChunk(event) => &event.raw,
            Event::RoleCreate(event) | Event::RoleUpdate(event) => &event.raw,
            Event::RoleDelete(event) => &event.raw,
            Event::MessageCreate(event) | Event::MessageUpdate(event) => &event.raw,
            Event::MessageDelete(event) => &event.raw,
            Event::MessageDeleteBulk(event) => &event.raw,
            Event::ChannelPinsUpdate(event) => &event.raw,
            Event::GuildBanAdd(event) | Event::GuildBanRemove(event) => &event.raw,
            Event::GuildEmojisUpdate(event) => &event.raw,
            Event::GuildIntegrationsUpdate(event) => &event.raw,
            Event::EntitlementCreate(event)
            | Event::EntitlementUpdate(event)
            | Event::EntitlementDelete(event) => &event.raw,
            Event::SubscriptionCreate(event)
            | Event::SubscriptionUpdate(event)
            | Event::SubscriptionDelete(event) => &event.raw,
            Event::IntegrationCreate(event) | Event::IntegrationUpdate(event) => &event.raw,
            Event::IntegrationDelete(event) => &event.raw,
            Event::GuildSoundboardSoundCreate(event) | Event::GuildSoundboardSoundUpdate(event) => {
                &event.raw
            }
            Event::GuildSoundboardSoundDelete(event) => &event.raw,
            Event::GuildSoundboardSoundsUpdate(event) | Event::SoundboardSounds(event) => {
                &event.raw
            }
            Event::WebhooksUpdate(event) => &event.raw,
            Event::InviteCreate(event) | Event::InviteDelete(event) => &event.raw,
            Event::MessageReactionAdd(event) | Event::MessageReactionRemove(event) => &event.raw,
            Event::MessageReactionRemoveAll(event) => &event.raw,
            Event::TypingStart(event) => &event.raw,
            Event::PresenceUpdate(event) => &event.raw,
            Event::UserUpdate(event) => &event.raw,
            Event::InteractionCreate(event) => &event.raw,
            Event::VoiceStateUpdate(event) => &event.raw,
            Event::VoiceServerUpdate(event) => &event.raw,
            Event::Resumed(event) => &event.raw,
            Event::ThreadCreate(event)
            | Event::ThreadUpdate(event)
            | Event::ThreadDelete(event) => &event.raw,
            Event::ThreadListSync(event) => &event.raw,
            Event::ThreadMemberUpdate(event) => &event.raw,
            Event::ThreadMembersUpdate(event) => &event.raw,
            Event::MessageReactionRemoveEmoji(event) => &event.raw,
            Event::MessagePollVoteAdd(event) | Event::MessagePollVoteRemove(event) => &event.raw,
            Event::GuildStickersUpdate(event) => &event.raw,
            Event::GuildScheduledEventCreate(event)
            | Event::GuildScheduledEventUpdate(event)
            | Event::GuildScheduledEventDelete(event) => &event.raw,
            Event::GuildScheduledEventUserAdd(event)
            | Event::GuildScheduledEventUserRemove(event) => &event.raw,
            Event::StageInstanceCreate(event)
            | Event::StageInstanceUpdate(event)
            | Event::StageInstanceDelete(event) => &event.raw,
            Event::VoiceChannelEffectSend(event) => &event.raw,
            Event::VoiceChannelStartTimeUpdate(event) => &event.raw,
            Event::VoiceChannelStatusUpdate(event) => &event.raw,
            Event::ChannelInfo(event) => &event.raw,
            Event::RateLimited(event) => &event.raw,
            Event::ApplicationCommandPermissionsUpdate(event) => &event.raw,
            Event::AutoModerationRuleCreate(event)
            | Event::AutoModerationRuleUpdate(event)
            | Event::AutoModerationRuleDelete(event)
            | Event::AutoModerationActionExecution(event) => &event.raw,
            Event::GuildAuditLogEntryCreate(event) => &event.raw,
            Event::Unknown { raw, .. } => raw,
        }
    }
}

/// Runs the `decode_event` helper.
pub fn decode_event(event_name: &str, data: Value) -> Result<Event, DiscordError> {
    let event = match event_name {
        "READY" => Event::Ready(ReadyEvent {
            data: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "GUILD_CREATE" => Event::GuildCreate(GuildEvent {
            guild: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "GUILD_UPDATE" => Event::GuildUpdate(GuildEvent {
            guild: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "GUILD_DELETE" => Event::GuildDelete(GuildDeleteEvent {
            data: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "CHANNEL_CREATE" => Event::ChannelCreate(ChannelEvent {
            channel: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "CHANNEL_UPDATE" => Event::ChannelUpdate(ChannelEvent {
            channel: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "CHANNEL_DELETE" => Event::ChannelDelete(ChannelEvent {
            channel: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "GUILD_MEMBER_ADD" => Event::MemberAdd(MemberEvent {
            guild_id: read_required_snowflake(&data, "guild_id")?,
            member: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "GUILD_MEMBER_UPDATE" => Event::MemberUpdate(MemberEvent {
            guild_id: read_required_snowflake(&data, "guild_id")?,
            member: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "GUILD_MEMBER_REMOVE" => Event::MemberRemove(MemberRemoveEvent {
            data: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "GUILD_MEMBERS_CHUNK" => Event::GuildMembersChunk(GuildMembersChunkEvent {
            data: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "GUILD_ROLE_CREATE" => Event::RoleCreate(RoleEvent {
            guild_id: read_required_snowflake(&data, "guild_id")?,
            role: serde_json::from_value(data.get("role").cloned().unwrap_or(Value::Null))?,
            raw: data,
        }),
        "GUILD_ROLE_UPDATE" => Event::RoleUpdate(RoleEvent {
            guild_id: read_required_snowflake(&data, "guild_id")?,
            role: serde_json::from_value(data.get("role").cloned().unwrap_or(Value::Null))?,
            raw: data,
        }),
        "GUILD_ROLE_DELETE" => Event::RoleDelete(RoleDeleteEvent {
            data: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "MESSAGE_CREATE" => Event::MessageCreate(MessageEvent {
            message: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "MESSAGE_UPDATE" => Event::MessageUpdate(MessageEvent {
            message: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "MESSAGE_DELETE" => Event::MessageDelete(MessageDeleteEvent {
            data: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "MESSAGE_DELETE_BULK" => {
            let ids: Vec<Snowflake> =
                serde_json::from_value(data.get("ids").cloned().unwrap_or(Value::Null))?;
            Event::MessageDeleteBulk(BulkMessageDeleteEvent {
                channel_id: read_required_snowflake(&data, "channel_id")?,
                guild_id: data
                    .get("guild_id")
                    .and_then(|v| serde_json::from_value(v.clone()).ok()),
                ids,
                raw: data,
            })
        }
        "CHANNEL_PINS_UPDATE" => Event::ChannelPinsUpdate(ChannelPinsUpdateEvent {
            channel_id: read_required_snowflake(&data, "channel_id")?,
            guild_id: data
                .get("guild_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            last_pin_timestamp: data
                .get("last_pin_timestamp")
                .and_then(|v| v.as_str().map(String::from)),
            raw: data,
        }),
        "GUILD_BAN_ADD" => Event::GuildBanAdd(GuildBanEvent {
            guild_id: read_required_snowflake(&data, "guild_id")?,
            user: serde_json::from_value(data.get("user").cloned().unwrap_or(Value::Null))?,
            raw: data,
        }),
        "GUILD_BAN_REMOVE" => Event::GuildBanRemove(GuildBanEvent {
            guild_id: read_required_snowflake(&data, "guild_id")?,
            user: serde_json::from_value(data.get("user").cloned().unwrap_or(Value::Null))?,
            raw: data,
        }),
        "GUILD_EMOJIS_UPDATE" => Event::GuildEmojisUpdate(GuildEmojisUpdateEvent {
            guild_id: read_required_snowflake(&data, "guild_id")?,
            emojis: serde_json::from_value(data.get("emojis").cloned().unwrap_or(Value::Null))
                .unwrap_or_default(),
            raw: data,
        }),
        "GUILD_INTEGRATIONS_UPDATE" => {
            Event::GuildIntegrationsUpdate(GuildIntegrationsUpdateEvent {
                guild_id: data
                    .get("guild_id")
                    .and_then(|v| serde_json::from_value(v.clone()).ok()),
                raw: data,
            })
        }
        "WEBHOOKS_UPDATE" => Event::WebhooksUpdate(WebhooksUpdateEvent {
            guild_id: data
                .get("guild_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            channel_id: data
                .get("channel_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            raw: data,
        }),
        "INVITE_CREATE" => Event::InviteCreate(InviteEvent {
            guild_id: data
                .get("guild_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            channel_id: data
                .get("channel_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            code: data.get("code").and_then(|v| v.as_str().map(String::from)),
            raw: data,
        }),
        "INVITE_DELETE" => Event::InviteDelete(InviteEvent {
            guild_id: data
                .get("guild_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            channel_id: data
                .get("channel_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            code: data.get("code").and_then(|v| v.as_str().map(String::from)),
            raw: data,
        }),
        "MESSAGE_REACTION_ADD" => Event::MessageReactionAdd(decode_reaction_event(data)),
        "MESSAGE_REACTION_REMOVE" => Event::MessageReactionRemove(decode_reaction_event(data)),
        "MESSAGE_REACTION_REMOVE_ALL" => Event::MessageReactionRemoveAll(ReactionRemoveAllEvent {
            channel_id: data
                .get("channel_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            message_id: data
                .get("message_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            guild_id: data
                .get("guild_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            raw: data,
        }),
        "TYPING_START" => Event::TypingStart(TypingStartEvent {
            channel_id: data
                .get("channel_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            guild_id: data
                .get("guild_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            user_id: data
                .get("user_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            timestamp: data.get("timestamp").and_then(|v| v.as_u64()),
            raw: data,
        }),
        "PRESENCE_UPDATE" => Event::PresenceUpdate(decode_presence_update_event(data)),
        "USER_UPDATE" => Event::UserUpdate(UserUpdateEvent {
            user: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "INTERACTION_CREATE" => Event::InteractionCreate(InteractionEvent {
            interaction: parse_interaction(&data)?,
            raw: data,
        }),
        "VOICE_STATE_UPDATE" => Event::VoiceStateUpdate(VoiceStateEvent {
            state: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "VOICE_SERVER_UPDATE" => Event::VoiceServerUpdate(VoiceServerEvent {
            data: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "RESUMED" => Event::Resumed(ResumedEvent { raw: data }),
        "THREAD_CREATE" => Event::ThreadCreate(ThreadEvent {
            thread: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "THREAD_UPDATE" => Event::ThreadUpdate(ThreadEvent {
            thread: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "THREAD_DELETE" => Event::ThreadDelete(ThreadEvent {
            thread: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "THREAD_LIST_SYNC" => Event::ThreadListSync(ThreadListSyncEvent {
            guild_id: data
                .get("guild_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            threads: data
                .get("threads")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
            raw: data,
        }),
        "THREAD_MEMBER_UPDATE" => Event::ThreadMemberUpdate(ThreadMemberUpdateEvent {
            guild_id: data
                .get("guild_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            thread_id: data
                .get("id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            raw: data,
        }),
        "THREAD_MEMBERS_UPDATE" => Event::ThreadMembersUpdate(ThreadMembersUpdateEvent {
            guild_id: data
                .get("guild_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            thread_id: data
                .get("id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            added_members: data
                .get("added_members")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            removed_member_ids: data
                .get("removed_member_ids")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            member_count: data.get("member_count").and_then(|v| v.as_u64()),
            raw: data,
        }),
        "MESSAGE_REACTION_REMOVE_EMOJI" => {
            Event::MessageReactionRemoveEmoji(ReactionRemoveEmojiEvent {
                channel_id: data
                    .get("channel_id")
                    .and_then(|v| serde_json::from_value(v.clone()).ok()),
                message_id: data
                    .get("message_id")
                    .and_then(|v| serde_json::from_value(v.clone()).ok()),
                guild_id: data
                    .get("guild_id")
                    .and_then(|v| serde_json::from_value(v.clone()).ok()),
                emoji: data
                    .get("emoji")
                    .and_then(|v| serde_json::from_value(v.clone()).ok()),
                raw: data,
            })
        }
        "GUILD_STICKERS_UPDATE" => Event::GuildStickersUpdate(GuildStickersUpdateEvent {
            guild_id: data
                .get("guild_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            stickers: data
                .get("stickers")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
            raw: data,
        }),
        "ENTITLEMENT_CREATE" => Event::EntitlementCreate(EntitlementEvent {
            entitlement: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "ENTITLEMENT_UPDATE" => Event::EntitlementUpdate(EntitlementEvent {
            entitlement: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "ENTITLEMENT_DELETE" => Event::EntitlementDelete(EntitlementEvent {
            entitlement: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "SUBSCRIPTION_CREATE" => Event::SubscriptionCreate(SubscriptionEvent {
            subscription: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "SUBSCRIPTION_UPDATE" => Event::SubscriptionUpdate(SubscriptionEvent {
            subscription: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "SUBSCRIPTION_DELETE" => Event::SubscriptionDelete(SubscriptionEvent {
            subscription: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "INTEGRATION_CREATE" => Event::IntegrationCreate(IntegrationEvent {
            guild_id: read_optional_snowflake(&data, "guild_id"),
            integration: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "INTEGRATION_UPDATE" => Event::IntegrationUpdate(IntegrationEvent {
            guild_id: read_optional_snowflake(&data, "guild_id"),
            integration: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "INTEGRATION_DELETE" => Event::IntegrationDelete(IntegrationDeleteEvent {
            id: read_optional_snowflake(&data, "id"),
            guild_id: read_optional_snowflake(&data, "guild_id"),
            application_id: read_optional_snowflake(&data, "application_id"),
            raw: data,
        }),
        "GUILD_SOUNDBOARD_SOUND_CREATE" => {
            Event::GuildSoundboardSoundCreate(SoundboardSoundEvent {
                sound: serde_json::from_value(data.clone())?,
                raw: data,
            })
        }
        "GUILD_SOUNDBOARD_SOUND_UPDATE" => {
            Event::GuildSoundboardSoundUpdate(SoundboardSoundEvent {
                sound: serde_json::from_value(data.clone())?,
                raw: data,
            })
        }
        "GUILD_SOUNDBOARD_SOUND_DELETE" => {
            Event::GuildSoundboardSoundDelete(SoundboardSoundDeleteEvent {
                sound_id: read_required_snowflake(&data, "sound_id")?,
                guild_id: read_required_snowflake(&data, "guild_id")?,
                raw: data,
            })
        }
        "GUILD_SOUNDBOARD_SOUNDS_UPDATE" => {
            Event::GuildSoundboardSoundsUpdate(decode_soundboard_sounds_event(data)?)
        }
        "SOUNDBOARD_SOUNDS" => Event::SoundboardSounds(decode_soundboard_sounds_event(data)?),
        "GUILD_SCHEDULED_EVENT_CREATE" => {
            Event::GuildScheduledEventCreate(decode_scheduled_event(data)?)
        }
        "GUILD_SCHEDULED_EVENT_UPDATE" => {
            Event::GuildScheduledEventUpdate(decode_scheduled_event(data)?)
        }
        "GUILD_SCHEDULED_EVENT_DELETE" => {
            Event::GuildScheduledEventDelete(decode_scheduled_event(data)?)
        }
        "GUILD_SCHEDULED_EVENT_USER_ADD" => {
            Event::GuildScheduledEventUserAdd(decode_scheduled_event_user_event(data)?)
        }
        "GUILD_SCHEDULED_EVENT_USER_REMOVE" => {
            Event::GuildScheduledEventUserRemove(decode_scheduled_event_user_event(data)?)
        }
        "STAGE_INSTANCE_CREATE" => Event::StageInstanceCreate(decode_stage_instance_event(data)?),
        "STAGE_INSTANCE_UPDATE" => Event::StageInstanceUpdate(decode_stage_instance_event(data)?),
        "STAGE_INSTANCE_DELETE" => Event::StageInstanceDelete(decode_stage_instance_event(data)?),
        "VOICE_CHANNEL_EFFECT_SEND" => {
            Event::VoiceChannelEffectSend(decode_voice_channel_effect_event(data))
        }
        "VOICE_CHANNEL_START_TIME_UPDATE" => {
            Event::VoiceChannelStartTimeUpdate(decode_voice_channel_start_time_update_event(data))
        }
        "VOICE_CHANNEL_STATUS_UPDATE" => {
            Event::VoiceChannelStatusUpdate(decode_voice_channel_status_update_event(data))
        }
        "CHANNEL_INFO" => Event::ChannelInfo(decode_channel_info_event(data)?),
        "RATE_LIMITED" => Event::RateLimited(decode_rate_limited_event(data)),
        "APPLICATION_COMMAND_PERMISSIONS_UPDATE" => Event::ApplicationCommandPermissionsUpdate(
            decode_application_command_permissions_update_event(data),
        ),
        "AUTO_MODERATION_RULE_CREATE" => {
            Event::AutoModerationRuleCreate(decode_auto_moderation_event(data))
        }
        "AUTO_MODERATION_RULE_UPDATE" => {
            Event::AutoModerationRuleUpdate(decode_auto_moderation_event(data))
        }
        "AUTO_MODERATION_RULE_DELETE" => {
            Event::AutoModerationRuleDelete(decode_auto_moderation_event(data))
        }
        "AUTO_MODERATION_ACTION_EXECUTION" => {
            Event::AutoModerationActionExecution(decode_auto_moderation_event(data))
        }
        "GUILD_AUDIT_LOG_ENTRY_CREATE" => {
            Event::GuildAuditLogEntryCreate(decode_audit_log_entry_event(data))
        }
        "MESSAGE_POLL_VOTE_ADD" => Event::MessagePollVoteAdd(decode_poll_vote_event(data)),
        "MESSAGE_POLL_VOTE_REMOVE" => Event::MessagePollVoteRemove(decode_poll_vote_event(data)),
        _ => Event::Unknown {
            kind: event_name.to_string(),
            raw: data,
        },
    };

    Ok(event)
}

fn read_required_snowflake(value: &Value, field: &str) -> Result<Snowflake, DiscordError> {
    let Some(raw) = value.get(field) else {
        return Err(format!("missing field {field}").into());
    };

    serde_json::from_value(raw.clone()).map_err(Into::into)
}

fn read_optional_snowflake(value: &Value, field: &str) -> Option<Snowflake> {
    value
        .get(field)
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}

fn read_optional_string(value: &Value, field: &str) -> Option<String> {
    value.get(field).and_then(|v| v.as_str().map(String::from))
}

fn read_optional_u64(value: &Value, field: &str) -> Option<u64> {
    value.get(field).and_then(Value::as_u64)
}

fn decode_scheduled_event(data: Value) -> Result<ScheduledEvent, DiscordError> {
    Ok(ScheduledEvent {
        id: read_optional_snowflake(&data, "id"),
        guild_id: read_optional_snowflake(&data, "guild_id"),
        channel_id: read_optional_snowflake(&data, "channel_id"),
        creator_id: read_optional_snowflake(&data, "creator_id"),
        name: read_optional_string(&data, "name"),
        description: read_optional_string(&data, "description"),
        scheduled_start_time: read_optional_string(&data, "scheduled_start_time"),
        scheduled_end_time: read_optional_string(&data, "scheduled_end_time"),
        privacy_level: read_optional_u64(&data, "privacy_level"),
        status: read_optional_u64(&data, "status"),
        entity_type: read_optional_u64(&data, "entity_type"),
        entity_id: read_optional_snowflake(&data, "entity_id"),
        entity_metadata: data.get("entity_metadata").cloned(),
        user_count: read_optional_u64(&data, "user_count"),
        image: read_optional_string(&data, "image"),
        raw: data,
    })
}

fn decode_soundboard_sounds_event(data: Value) -> Result<SoundboardSoundsEvent, DiscordError> {
    Ok(SoundboardSoundsEvent {
        guild_id: read_required_snowflake(&data, "guild_id")?,
        soundboard_sounds: data
            .get("soundboard_sounds")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default(),
        raw: data,
    })
}

fn decode_poll_vote_event(data: Value) -> PollVoteEvent {
    PollVoteEvent {
        user_id: read_optional_snowflake(&data, "user_id"),
        channel_id: read_optional_snowflake(&data, "channel_id"),
        message_id: read_optional_snowflake(&data, "message_id"),
        guild_id: read_optional_snowflake(&data, "guild_id"),
        answer_id: read_optional_u64(&data, "answer_id"),
        raw: data,
    }
}

fn decode_reaction_event(data: Value) -> ReactionEvent {
    ReactionEvent {
        user_id: read_optional_snowflake(&data, "user_id"),
        channel_id: read_optional_snowflake(&data, "channel_id"),
        message_id: read_optional_snowflake(&data, "message_id"),
        guild_id: read_optional_snowflake(&data, "guild_id"),
        member: data
            .get("member")
            .and_then(|value| serde_json::from_value(value.clone()).ok()),
        message_author_id: read_optional_snowflake(&data, "message_author_id"),
        emoji: data
            .get("emoji")
            .and_then(|value| serde_json::from_value(value.clone()).ok()),
        burst: data.get("burst").and_then(Value::as_bool),
        burst_colors: data
            .get("burst_colors")
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default(),
        reaction_type: read_optional_u64(&data, "type"),
        raw: data,
    }
}

fn decode_presence_update_event(data: Value) -> PresenceUpdateEvent {
    let user = data.get("user").and_then(decode_presence_update_user);
    let user_id = user.as_ref().map(|user| user.id.clone()).or_else(|| {
        data.pointer("/user/id")
            .and_then(Value::as_str)
            .map(Snowflake::new)
    });

    PresenceUpdateEvent {
        user,
        user_id,
        guild_id: read_optional_snowflake(&data, "guild_id"),
        status: read_optional_string(&data, "status"),
        activities: data
            .get("activities")
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default(),
        client_status: data
            .get("client_status")
            .and_then(|value| serde_json::from_value(value.clone()).ok()),
        raw: data,
    }
}

fn decode_presence_update_user(value: &Value) -> Option<PresenceUpdateUser> {
    Some(PresenceUpdateUser {
        id: read_required_snowflake(value, "id").ok()?,
        username: read_optional_string(value, "username"),
        raw: value.clone(),
    })
}

fn decode_scheduled_event_user_event(
    data: Value,
) -> Result<GuildScheduledEventUserEvent, DiscordError> {
    let mut event: GuildScheduledEventUserEvent = serde_json::from_value(data.clone())?;
    event.raw = data;
    Ok(event)
}

fn decode_stage_instance_event(data: Value) -> Result<StageInstanceEvent, DiscordError> {
    Ok(StageInstanceEvent {
        stage_instance: serde_json::from_value(data.clone())?,
        raw: data,
    })
}

fn decode_application_command_permissions_update_event(
    data: Value,
) -> ApplicationCommandPermissionsUpdateEvent {
    ApplicationCommandPermissionsUpdateEvent {
        id: read_optional_snowflake(&data, "id"),
        application_id: read_optional_snowflake(&data, "application_id"),
        guild_id: read_optional_snowflake(&data, "guild_id"),
        permissions: data
            .get("permissions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        raw: data,
    }
}

fn decode_voice_channel_effect_event(data: Value) -> VoiceChannelEffectEvent {
    VoiceChannelEffectEvent {
        channel_id: read_optional_snowflake(&data, "channel_id"),
        guild_id: read_optional_snowflake(&data, "guild_id"),
        user_id: read_optional_snowflake(&data, "user_id"),
        emoji: data
            .get("emoji")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok()),
        animation_type: read_optional_u64(&data, "animation_type"),
        animation_id: read_optional_u64(&data, "animation_id"),
        sound_id: read_optional_snowflake(&data, "sound_id"),
        sound_volume: data.get("sound_volume").and_then(Value::as_f64),
        raw: data,
    }
}

fn decode_voice_channel_start_time_update_event(data: Value) -> VoiceChannelStartTimeUpdateEvent {
    VoiceChannelStartTimeUpdateEvent {
        channel_id: read_optional_snowflake(&data, "channel_id"),
        guild_id: read_optional_snowflake(&data, "guild_id"),
        voice_channel_start_time: read_optional_string(&data, "voice_channel_start_time"),
        raw: data,
    }
}

fn decode_voice_channel_status_update_event(data: Value) -> VoiceChannelStatusUpdateEvent {
    VoiceChannelStatusUpdateEvent {
        channel_id: read_optional_snowflake(&data, "channel_id"),
        guild_id: read_optional_snowflake(&data, "guild_id"),
        status: read_optional_string(&data, "status"),
        raw: data,
    }
}

fn decode_channel_info_event(data: Value) -> Result<ChannelInfoEvent, DiscordError> {
    let guild_id = read_required_snowflake(&data, "guild_id")?;
    let channels = data
        .get("channels")
        .and_then(Value::as_array)
        .map(|channels| {
            channels
                .iter()
                .map(|channel| {
                    Ok(ChannelInfoChannel {
                        id: read_required_snowflake(channel, "id")?,
                        status: read_optional_string(channel, "status"),
                        voice_start_time: read_optional_string(channel, "voice_start_time"),
                        raw: channel.clone(),
                    })
                })
                .collect::<Result<Vec<_>, DiscordError>>()
        })
        .transpose()?
        .unwrap_or_default();

    Ok(ChannelInfoEvent {
        guild_id,
        channels,
        raw: data,
    })
}

fn decode_rate_limited_event(data: Value) -> RateLimitedEvent {
    RateLimitedEvent {
        opcode: read_optional_u64(&data, "opcode"),
        retry_after: data.get("retry_after").and_then(Value::as_f64),
        meta: data.get("meta").cloned(),
        raw: data,
    }
}

fn decode_auto_moderation_event(data: Value) -> AutoModerationEvent {
    AutoModerationEvent {
        id: read_optional_snowflake(&data, "id"),
        guild_id: read_optional_snowflake(&data, "guild_id"),
        name: read_optional_string(&data, "name"),
        creator_id: read_optional_snowflake(&data, "creator_id"),
        event_type: read_optional_u64(&data, "event_type"),
        trigger_type: read_optional_u64(&data, "trigger_type"),
        trigger_metadata: data.get("trigger_metadata").cloned(),
        actions: data
            .get("actions")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default(),
        enabled: data.get("enabled").and_then(Value::as_bool),
        exempt_roles: data
            .get("exempt_roles")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default(),
        exempt_channels: data
            .get("exempt_channels")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default(),
        action: data.get("action").cloned(),
        rule_id: read_optional_snowflake(&data, "rule_id"),
        rule_trigger_type: read_optional_u64(&data, "rule_trigger_type"),
        user_id: read_optional_snowflake(&data, "user_id"),
        channel_id: read_optional_snowflake(&data, "channel_id"),
        message_id: read_optional_snowflake(&data, "message_id"),
        alert_system_message_id: read_optional_snowflake(&data, "alert_system_message_id"),
        content: read_optional_string(&data, "content"),
        matched_keyword: read_optional_string(&data, "matched_keyword"),
        matched_content: read_optional_string(&data, "matched_content"),
        raw: data,
    }
}

fn decode_audit_log_entry_event(data: Value) -> AuditLogEntryEvent {
    AuditLogEntryEvent {
        guild_id: read_optional_snowflake(&data, "guild_id"),
        entry: serde_json::from_value(data.clone()).ok(),
        id: read_optional_snowflake(&data, "id"),
        user_id: read_optional_snowflake(&data, "user_id"),
        target_id: read_optional_snowflake(&data, "target_id"),
        action_type: read_optional_u64(&data, "action_type"),
        changes: data
            .get("changes")
            .and_then(|v| serde_json::from_value(v.clone()).ok()),
        options: data.get("options").cloned(),
        reason: read_optional_string(&data, "reason"),
        raw: data,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::*;
    use crate::error::DiscordError;
    use crate::model::{
        Channel, Entitlement, Guild, Integration, IntegrationAccount, Interaction,
        InteractionContextData, Member, Message, PingInteraction, Role, Snowflake, SoundboardSound,
        StageInstance, Subscription, User, VoiceServerUpdate, VoiceState,
    };
    use crate::types::Emoji;

    fn snowflake(id: &str) -> Snowflake {
        Snowflake::new(id)
    }

    fn raw(kind: &str) -> Value {
        json!({ "kind": kind })
    }

    fn user(id: &str, username: &str) -> User {
        User {
            id: snowflake(id),
            username: username.to_string(),
            ..Default::default()
        }
    }

    fn guild(id: &str, name: &str) -> Guild {
        Guild {
            id: snowflake(id),
            name: name.to_string(),
            ..Default::default()
        }
    }

    fn channel(id: &str) -> Channel {
        Channel {
            id: snowflake(id),
            kind: 0,
            ..Default::default()
        }
    }

    fn member(id: &str, username: &str) -> Member {
        Member {
            user: Some(user(id, username)),
            ..Default::default()
        }
    }

    fn role(id: &str, name: &str) -> Role {
        Role {
            id: snowflake(id),
            name: name.to_string(),
            ..Default::default()
        }
    }

    fn message(id: &str, channel_id: &str, content: &str) -> Message {
        Message {
            id: snowflake(id),
            channel_id: snowflake(channel_id),
            content: content.to_string(),
            ..Default::default()
        }
    }

    fn interaction_context() -> InteractionContextData {
        InteractionContextData {
            id: snowflake("400"),
            application_id: snowflake("401"),
            token: "token".to_string(),
            ..Default::default()
        }
    }

    fn assert_kind_and_raw(event: Event, expected_kind: &str) {
        assert_eq!(event.kind(), expected_kind);
        assert_eq!(event.raw(), &raw(expected_kind));
    }

    #[test]
    fn decode_message_create_event_returns_typed_payload() {
        let raw = json!({
            "id": "2",
            "channel_id": "1",
            "content": "hello",
            "mentions": [],
            "attachments": []
        });
        let event = decode_event("MESSAGE_CREATE", raw.clone()).unwrap();

        match event {
            Event::MessageCreate(message) => {
                assert_eq!(message.message.content, "hello");
                assert_eq!(message.raw, raw);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn decode_event_handles_optional_field_fallbacks() {
        let emojis_update = decode_event(
            "GUILD_EMOJIS_UPDATE",
            json!({
                "guild_id": "1",
                "emojis": {}
            }),
        )
        .unwrap();
        match emojis_update {
            Event::GuildEmojisUpdate(event) => {
                assert_eq!(event.guild_id, snowflake("1"));
                assert!(event.emojis.is_empty());
                assert_eq!(event.raw, json!({"guild_id": "1", "emojis": {}}));
            }
            other => panic!("unexpected event: {other:?}"),
        }

        let webhooks_update = decode_event(
            "WEBHOOKS_UPDATE",
            json!({
                "guild_id": {},
                "channel_id": {}
            }),
        )
        .unwrap();
        match webhooks_update {
            Event::WebhooksUpdate(event) => {
                assert_eq!(event.guild_id, None);
                assert_eq!(event.channel_id, None);
            }
            other => panic!("unexpected event: {other:?}"),
        }

        let invite_create = decode_event(
            "INVITE_CREATE",
            json!({
                "guild_id": {},
                "channel_id": {},
                "code": 42
            }),
        )
        .unwrap();
        match invite_create {
            Event::InviteCreate(event) => {
                assert_eq!(event.guild_id, None);
                assert_eq!(event.channel_id, None);
                assert_eq!(event.code, None);
            }
            other => panic!("unexpected event: {other:?}"),
        }

        let invite_delete = decode_event(
            "INVITE_DELETE",
            json!({
                "guild_id": {},
                "channel_id": {},
                "code": 42
            }),
        )
        .unwrap();
        match invite_delete {
            Event::InviteDelete(event) => {
                assert_eq!(event.guild_id, None);
                assert_eq!(event.channel_id, None);
                assert_eq!(event.code, None);
            }
            other => panic!("unexpected event: {other:?}"),
        }

        let pins_update = decode_event(
            "CHANNEL_PINS_UPDATE",
            json!({
                "channel_id": "2",
                "guild_id": {},
                "last_pin_timestamp": 123
            }),
        )
        .unwrap();
        match pins_update {
            Event::ChannelPinsUpdate(event) => {
                assert_eq!(event.channel_id, snowflake("2"));
                assert_eq!(event.guild_id, None);
                assert_eq!(event.last_pin_timestamp, None);
            }
            other => panic!("unexpected event: {other:?}"),
        }

        let typing_start = decode_event(
            "TYPING_START",
            json!({
                "channel_id": {},
                "guild_id": {},
                "user_id": {},
                "timestamp": "later"
            }),
        )
        .unwrap();
        match typing_start {
            Event::TypingStart(event) => {
                assert_eq!(event.channel_id, None);
                assert_eq!(event.guild_id, None);
                assert_eq!(event.user_id, None);
                assert_eq!(event.timestamp, None);
            }
            other => panic!("unexpected event: {other:?}"),
        }

        let presence_update = decode_event(
            "PRESENCE_UPDATE",
            json!({
                "guild_id": {},
                "status": 1,
                "user": { "id": {} }
            }),
        )
        .unwrap();
        match presence_update {
            Event::PresenceUpdate(event) => {
                assert_eq!(event.user_id, None);
                assert_eq!(event.guild_id, None);
                assert_eq!(event.status, None);
            }
            other => panic!("unexpected event: {other:?}"),
        }

        let integrations_update = decode_event(
            "GUILD_INTEGRATIONS_UPDATE",
            json!({
                "guild_id": {}
            }),
        )
        .unwrap();
        match integrations_update {
            Event::GuildIntegrationsUpdate(event) => {
                assert_eq!(event.guild_id, None);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn decode_event_reads_nested_and_required_payloads() {
        let member_add = decode_event(
            "GUILD_MEMBER_ADD",
            json!({
                "guild_id": "100",
                "user": {
                    "id": "200",
                    "username": "member"
                }
            }),
        )
        .unwrap();
        match member_add {
            Event::MemberAdd(event) => {
                assert_eq!(event.guild_id, snowflake("100"));
                assert_eq!(
                    event
                        .member
                        .user
                        .as_ref()
                        .map(|user| user.username.as_str()),
                    Some("member")
                );
            }
            other => panic!("unexpected event: {other:?}"),
        }

        let role_create = decode_event(
            "GUILD_ROLE_CREATE",
            json!({
                "guild_id": "100",
                "role": {
                    "id": "300",
                    "name": "mods"
                }
            }),
        )
        .unwrap();
        match role_create {
            Event::RoleCreate(event) => {
                assert_eq!(event.guild_id, snowflake("100"));
                assert_eq!(event.role.id, snowflake("300"));
                assert_eq!(event.role.name, "mods");
            }
            other => panic!("unexpected event: {other:?}"),
        }

        let bulk_delete = decode_event(
            "MESSAGE_DELETE_BULK",
            json!({
                "ids": ["10", "11"],
                "channel_id": "12",
                "guild_id": "13"
            }),
        )
        .unwrap();
        match bulk_delete {
            Event::MessageDeleteBulk(event) => {
                assert_eq!(event.ids, vec![snowflake("10"), snowflake("11")]);
                assert_eq!(event.channel_id, snowflake("12"));
                assert_eq!(event.guild_id, Some(snowflake("13")));
                assert_eq!(
                    event.raw,
                    json!({
                        "ids": ["10", "11"],
                        "channel_id": "12",
                        "guild_id": "13"
                    })
                );
            }
            other => panic!("unexpected event: {other:?}"),
        }

        let pins_update = decode_event(
            "CHANNEL_PINS_UPDATE",
            json!({
                "channel_id": "14",
                "last_pin_timestamp": "2024-01-01T00:00:00Z"
            }),
        )
        .unwrap();
        match pins_update {
            Event::ChannelPinsUpdate(event) => {
                assert_eq!(event.channel_id, snowflake("14"));
                assert_eq!(
                    event.last_pin_timestamp.as_deref(),
                    Some("2024-01-01T00:00:00Z")
                );
            }
            other => panic!("unexpected event: {other:?}"),
        }

        let typing_start = decode_event(
            "TYPING_START",
            json!({
                "channel_id": "15",
                "guild_id": "16",
                "user_id": "17",
                "timestamp": 12345
            }),
        )
        .unwrap();
        match typing_start {
            Event::TypingStart(event) => {
                assert_eq!(event.channel_id, Some(snowflake("15")));
                assert_eq!(event.guild_id, Some(snowflake("16")));
                assert_eq!(event.user_id, Some(snowflake("17")));
                assert_eq!(event.timestamp, Some(12345));
            }
            other => panic!("unexpected event: {other:?}"),
        }

        let presence_update = decode_event(
            "PRESENCE_UPDATE",
            json!({
                "guild_id": "18",
                "status": "online",
                "user": {
                    "id": "19",
                    "username": "present"
                },
                "activities": [{
                    "name": "Building",
                    "type": 0
                }],
                "client_status": {
                    "desktop": "online",
                    "web": "idle"
                }
            }),
        )
        .unwrap();
        match presence_update {
            Event::PresenceUpdate(event) => {
                assert_eq!(event.guild_id, Some(snowflake("18")));
                assert_eq!(event.user_id, Some(snowflake("19")));
                assert_eq!(event.status.as_deref(), Some("online"));
                assert_eq!(
                    event
                        .user
                        .as_ref()
                        .and_then(|user| user.username.as_deref()),
                    Some("present")
                );
                assert_eq!(event.activities[0].name, "Building");
                assert_eq!(
                    event
                        .client_status
                        .as_ref()
                        .and_then(|status| status.desktop.as_deref()),
                    Some("online")
                );
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn decode_event_covers_additional_typed_gateway_payloads() {
        match decode_event(
            "GUILD_CREATE",
            json!({
                "id": "1",
                "name": "discordrs",
                "roles": []
            }),
        )
        .unwrap()
        {
            Event::GuildCreate(event) => {
                assert_eq!(event.guild.id, snowflake("1"));
                assert_eq!(event.guild.name, "discordrs");
            }
            other => panic!("unexpected guild event: {other:?}"),
        }

        match decode_event(
            "CHANNEL_CREATE",
            json!({
                "id": "2",
                "type": 0,
                "name": "general"
            }),
        )
        .unwrap()
        {
            Event::ChannelCreate(event) => {
                assert_eq!(event.channel.id, snowflake("2"));
                assert_eq!(event.channel.name.as_deref(), Some("general"));
            }
            other => panic!("unexpected channel event: {other:?}"),
        }

        match decode_event(
            "GUILD_MEMBER_REMOVE",
            json!({
                "guild_id": "3",
                "user": {
                    "id": "4",
                    "username": "member"
                }
            }),
        )
        .unwrap()
        {
            Event::MemberRemove(event) => {
                assert_eq!(event.data.guild_id, snowflake("3"));
                assert_eq!(event.data.user.id, snowflake("4"));
            }
            other => panic!("unexpected member removal event: {other:?}"),
        }

        match decode_event(
            "GUILD_ROLE_DELETE",
            json!({
                "guild_id": "5",
                "role_id": "6"
            }),
        )
        .unwrap()
        {
            Event::RoleDelete(event) => {
                assert_eq!(event.data.guild_id, snowflake("5"));
                assert_eq!(event.data.role_id, snowflake("6"));
            }
            other => panic!("unexpected role delete event: {other:?}"),
        }
    }

    #[test]
    fn decode_event_exposes_common_fields_for_newer_gateway_payloads() {
        match decode_event(
            "GUILD_SCHEDULED_EVENT_CREATE",
            json!({
                "id": "700",
                "guild_id": "701",
                "channel_id": "702",
                "creator_id": "703",
                "name": "Launch",
                "description": "Release stream",
                "scheduled_start_time": "2026-04-30T01:00:00Z",
                "scheduled_end_time": "2026-04-30T02:00:00Z",
                "privacy_level": 2,
                "status": 1,
                "entity_type": 2,
                "entity_id": "704",
                "entity_metadata": { "location": "voice" },
                "user_count": 42,
                "image": "cover"
            }),
        )
        .unwrap()
        {
            Event::GuildScheduledEventCreate(event) => {
                assert_eq!(event.id, Some(snowflake("700")));
                assert_eq!(event.guild_id, Some(snowflake("701")));
                assert_eq!(event.channel_id, Some(snowflake("702")));
                assert_eq!(event.creator_id, Some(snowflake("703")));
                assert_eq!(event.name.as_deref(), Some("Launch"));
                assert_eq!(event.description.as_deref(), Some("Release stream"));
                assert_eq!(
                    event.scheduled_start_time.as_deref(),
                    Some("2026-04-30T01:00:00Z")
                );
                assert_eq!(event.status, Some(1));
                assert_eq!(event.entity_type, Some(2));
                assert_eq!(event.entity_id, Some(snowflake("704")));
                assert_eq!(event.entity_metadata, Some(json!({ "location": "voice" })));
                assert_eq!(event.user_count, Some(42));
                assert_eq!(event.image.as_deref(), Some("cover"));
            }
            other => panic!("unexpected scheduled event: {other:?}"),
        }

        match decode_event(
            "AUTO_MODERATION_RULE_CREATE",
            json!({
                "id": "710",
                "guild_id": "711",
                "name": "Keyword Filter",
                "creator_id": "712",
                "event_type": 1,
                "trigger_type": 1,
                "trigger_metadata": { "keyword_filter": ["bad"] },
                "actions": [{ "type": 1 }],
                "enabled": true,
                "exempt_roles": ["713"],
                "exempt_channels": ["714"]
            }),
        )
        .unwrap()
        {
            Event::AutoModerationRuleCreate(event) => {
                assert_eq!(event.id, Some(snowflake("710")));
                assert_eq!(event.guild_id, Some(snowflake("711")));
                assert_eq!(event.name.as_deref(), Some("Keyword Filter"));
                assert_eq!(event.creator_id, Some(snowflake("712")));
                assert_eq!(event.event_type, Some(1));
                assert_eq!(event.trigger_type, Some(1));
                assert_eq!(
                    event.trigger_metadata,
                    Some(json!({ "keyword_filter": ["bad"] }))
                );
                assert_eq!(event.actions, vec![json!({ "type": 1 })]);
                assert_eq!(event.enabled, Some(true));
                assert_eq!(event.exempt_roles, vec![snowflake("713")]);
                assert_eq!(event.exempt_channels, vec![snowflake("714")]);
            }
            other => panic!("unexpected auto moderation rule event: {other:?}"),
        }

        match decode_event(
            "AUTO_MODERATION_ACTION_EXECUTION",
            json!({
                "guild_id": "720",
                "action": { "type": 2, "metadata": { "channel_id": "721" } },
                "rule_id": "722",
                "rule_trigger_type": 1,
                "user_id": "723",
                "channel_id": "724",
                "message_id": "725",
                "alert_system_message_id": "726",
                "content": "blocked text",
                "matched_keyword": "blocked",
                "matched_content": "blocked"
            }),
        )
        .unwrap()
        {
            Event::AutoModerationActionExecution(event) => {
                assert_eq!(event.guild_id, Some(snowflake("720")));
                assert_eq!(
                    event.action,
                    Some(json!({ "type": 2, "metadata": { "channel_id": "721" } }))
                );
                assert_eq!(event.rule_id, Some(snowflake("722")));
                assert_eq!(event.rule_trigger_type, Some(1));
                assert_eq!(event.user_id, Some(snowflake("723")));
                assert_eq!(event.channel_id, Some(snowflake("724")));
                assert_eq!(event.message_id, Some(snowflake("725")));
                assert_eq!(event.alert_system_message_id, Some(snowflake("726")));
                assert_eq!(event.content.as_deref(), Some("blocked text"));
                assert_eq!(event.matched_keyword.as_deref(), Some("blocked"));
                assert_eq!(event.matched_content.as_deref(), Some("blocked"));
            }
            other => panic!("unexpected auto moderation action event: {other:?}"),
        }

        match decode_event(
            "GUILD_AUDIT_LOG_ENTRY_CREATE",
            json!({
                "guild_id": "730",
                "id": "731",
                "user_id": "732",
                "target_id": "733",
                "action_type": 22,
                "changes": [{ "key": "nick", "new_value": "new" }],
                "options": { "delete_member_days": "1" },
                "reason": "cleanup"
            }),
        )
        .unwrap()
        {
            Event::GuildAuditLogEntryCreate(event) => {
                assert_eq!(event.guild_id, Some(snowflake("730")));
                assert_eq!(event.id, Some(snowflake("731")));
                assert_eq!(event.user_id, Some(snowflake("732")));
                assert_eq!(event.target_id, Some(snowflake("733")));
                assert_eq!(event.action_type, Some(22));
                assert_eq!(
                    event.changes,
                    Some(vec![json!({ "key": "nick", "new_value": "new" })])
                );
                assert_eq!(event.options, Some(json!({ "delete_member_days": "1" })));
                assert_eq!(event.reason.as_deref(), Some("cleanup"));
                assert_eq!(
                    event
                        .entry
                        .as_ref()
                        .and_then(|entry| entry.id.as_ref())
                        .map(Snowflake::as_str),
                    Some("731")
                );
            }
            other => panic!("unexpected audit log entry event: {other:?}"),
        }

        match decode_event(
            "USER_UPDATE",
            json!({
                "id": "740",
                "username": "bot"
            }),
        )
        .unwrap()
        {
            Event::UserUpdate(event) => {
                assert_eq!(event.user.id, snowflake("740"));
                assert_eq!(event.user.username, "bot");
            }
            other => panic!("unexpected user update event: {other:?}"),
        }

        let rate_limited = decode_event(
            "RATE_LIMITED",
            json!({
                "opcode": 8,
                "retry_after": 12.5,
                "meta": {
                    "guild_id": "750",
                    "nonce": "members-1"
                }
            }),
        )
        .unwrap();
        assert_eq!(rate_limited.kind(), "RATE_LIMITED");
        match rate_limited {
            Event::RateLimited(event) => {
                assert_eq!(event.opcode, Some(8));
                assert_eq!(event.retry_after, Some(12.5));
                assert_eq!(
                    event.meta,
                    Some(json!({ "guild_id": "750", "nonce": "members-1" }))
                );
            }
            other => panic!("unexpected rate limited event: {other:?}"),
        }
    }

    #[test]
    fn decode_event_covers_voice_ban_reaction_and_interaction_variants() {
        match decode_event(
            "VOICE_STATE_UPDATE",
            json!({
                "guild_id": "1",
                "channel_id": "2",
                "user_id": "3"
            }),
        )
        .unwrap()
        {
            Event::VoiceStateUpdate(event) => {
                assert_eq!(event.state.guild_id, Some(snowflake("1")));
                assert_eq!(event.state.channel_id, Some(snowflake("2")));
                assert_eq!(event.state.user_id, Some(snowflake("3")));
            }
            other => panic!("unexpected voice state event: {other:?}"),
        }

        match decode_event(
            "VOICE_SERVER_UPDATE",
            json!({
                "guild_id": "4",
                "token": "voice-token",
                "endpoint": "wss://voice.discord.test"
            }),
        )
        .unwrap()
        {
            Event::VoiceServerUpdate(event) => {
                assert_eq!(event.data.guild_id, snowflake("4"));
                assert_eq!(event.data.token, "voice-token");
                assert_eq!(
                    event.data.endpoint.as_deref(),
                    Some("wss://voice.discord.test")
                );
            }
            other => panic!("unexpected voice server event: {other:?}"),
        }

        match decode_event(
            "GUILD_BAN_ADD",
            json!({
                "guild_id": "7",
                "user": {
                    "id": "8",
                    "username": "banned"
                }
            }),
        )
        .unwrap()
        {
            Event::GuildBanAdd(event) => {
                assert_eq!(event.guild_id, snowflake("7"));
                assert_eq!(event.user.username, "banned");
            }
            other => panic!("unexpected guild ban event: {other:?}"),
        }

        match decode_event(
            "MESSAGE_REACTION_ADD",
            json!({
                "user_id": "9",
                "channel_id": "10",
                "message_id": "11",
                "guild_id": "12",
                "member": {
                    "user": {
                        "id": "9",
                        "username": "reactor"
                    }
                },
                "message_author_id": "99",
                "burst": true,
                "burst_colors": ["#ff0000", "#00ff00"],
                "type": 1,
                "emoji": {
                    "name": "?뵦"
                }
            }),
        )
        .unwrap()
        {
            Event::MessageReactionAdd(event) => {
                assert_eq!(event.user_id, Some(snowflake("9")));
                assert_eq!(event.channel_id, Some(snowflake("10")));
                assert_eq!(event.message_id, Some(snowflake("11")));
                assert_eq!(event.guild_id, Some(snowflake("12")));
                assert_eq!(
                    event
                        .member
                        .as_ref()
                        .and_then(|member| member.user.as_ref())
                        .map(|user| user.username.as_str()),
                    Some("reactor")
                );
                assert_eq!(event.message_author_id, Some(snowflake("99")));
                assert_eq!(event.burst, Some(true));
                assert_eq!(event.burst_colors, vec!["#ff0000", "#00ff00"]);
                assert_eq!(event.reaction_type, Some(1));
                assert_eq!(
                    event.emoji.and_then(|emoji| emoji.name),
                    Some("?뵦".to_string())
                );
            }
            other => panic!("unexpected reaction event: {other:?}"),
        }

        match decode_event(
            "INTERACTION_CREATE",
            json!({
                "id": "13",
                "application_id": "14",
                "token": "interaction-token",
                "type": 1
            }),
        )
        .unwrap()
        {
            Event::InteractionCreate(event) => {
                assert!(matches!(event.interaction, Interaction::Ping(_)));
            }
            other => panic!("unexpected interaction event: {other:?}"),
        }
    }

    #[test]
    fn decode_event_covers_remaining_success_variants() {
        match decode_event(
            "READY",
            json!({
                "user": {
                    "id": "50",
                    "username": "ready"
                },
                "session_id": "session-50"
            }),
        )
        .unwrap()
        {
            Event::Ready(event) => {
                assert_eq!(event.data.user.id, snowflake("50"));
                assert_eq!(event.data.session_id, "session-50");
                assert!(event.data.application.is_none());
                assert!(event.data.resume_gateway_url.is_none());
            }
            other => panic!("unexpected ready event: {other:?}"),
        }

        match decode_event(
            "GUILD_UPDATE",
            json!({
                "id": "51",
                "name": "guild-update",
                "roles": []
            }),
        )
        .unwrap()
        {
            Event::GuildUpdate(event) => {
                assert_eq!(event.guild.id, snowflake("51"));
                assert_eq!(event.guild.name, "guild-update");
            }
            other => panic!("unexpected guild update event: {other:?}"),
        }

        match decode_event(
            "GUILD_DELETE",
            json!({
                "id": "52"
            }),
        )
        .unwrap()
        {
            Event::GuildDelete(event) => {
                assert_eq!(event.data.id, snowflake("52"));
                assert_eq!(event.data.unavailable, None);
            }
            other => panic!("unexpected guild delete event: {other:?}"),
        }

        match decode_event(
            "CHANNEL_UPDATE",
            json!({
                "id": "53",
                "type": 0
            }),
        )
        .unwrap()
        {
            Event::ChannelUpdate(event) => {
                assert_eq!(event.channel.id, snowflake("53"));
                assert_eq!(event.channel.kind, 0);
            }
            other => panic!("unexpected channel update event: {other:?}"),
        }

        match decode_event(
            "CHANNEL_DELETE",
            json!({
                "id": "54",
                "type": 0
            }),
        )
        .unwrap()
        {
            Event::ChannelDelete(event) => {
                assert_eq!(event.channel.id, snowflake("54"));
                assert_eq!(event.channel.kind, 0);
            }
            other => panic!("unexpected channel delete event: {other:?}"),
        }

        match decode_event(
            "GUILD_MEMBER_UPDATE",
            json!({
                "guild_id": "55",
                "user": {
                    "id": "56",
                    "username": "member-update"
                }
            }),
        )
        .unwrap()
        {
            Event::MemberUpdate(event) => {
                assert_eq!(event.guild_id, snowflake("55"));
                assert_eq!(
                    event
                        .member
                        .user
                        .as_ref()
                        .map(|user| user.username.as_str()),
                    Some("member-update")
                );
            }
            other => panic!("unexpected member update event: {other:?}"),
        }

        match decode_event(
            "GUILD_ROLE_UPDATE",
            json!({
                "guild_id": "57",
                "role": {
                    "id": "58",
                    "name": "role-update"
                }
            }),
        )
        .unwrap()
        {
            Event::RoleUpdate(event) => {
                assert_eq!(event.guild_id, snowflake("57"));
                assert_eq!(event.role.id, snowflake("58"));
                assert_eq!(event.role.name, "role-update");
            }
            other => panic!("unexpected role update event: {other:?}"),
        }

        match decode_event(
            "MESSAGE_UPDATE",
            json!({
                "id": "59",
                "channel_id": "60",
                "content": "edited",
                "mentions": [],
                "attachments": []
            }),
        )
        .unwrap()
        {
            Event::MessageUpdate(event) => {
                assert_eq!(event.message.id, snowflake("59"));
                assert_eq!(event.message.channel_id, snowflake("60"));
                assert_eq!(event.message.content, "edited");
            }
            other => panic!("unexpected message update event: {other:?}"),
        }

        match decode_event(
            "MESSAGE_DELETE",
            json!({
                "id": "61",
                "channel_id": "62"
            }),
        )
        .unwrap()
        {
            Event::MessageDelete(event) => {
                assert_eq!(event.data.id, snowflake("61"));
                assert_eq!(event.data.channel_id, snowflake("62"));
                assert_eq!(event.data.guild_id, None);
            }
            other => panic!("unexpected message delete event: {other:?}"),
        }

        match decode_event(
            "GUILD_BAN_REMOVE",
            json!({
                "guild_id": "63",
                "user": {
                    "id": "64",
                    "username": "ban-remove"
                }
            }),
        )
        .unwrap()
        {
            Event::GuildBanRemove(event) => {
                assert_eq!(event.guild_id, snowflake("63"));
                assert_eq!(event.user.id, snowflake("64"));
                assert_eq!(event.user.username, "ban-remove");
            }
            other => panic!("unexpected guild ban remove event: {other:?}"),
        }

        match decode_event(
            "GUILD_EMOJIS_UPDATE",
            json!({
                "guild_id": "65"
            }),
        )
        .unwrap()
        {
            Event::GuildEmojisUpdate(event) => {
                assert_eq!(event.guild_id, snowflake("65"));
                assert!(event.emojis.is_empty());
            }
            other => panic!("unexpected guild emojis update event: {other:?}"),
        }

        match decode_event(
            "GUILD_INTEGRATIONS_UPDATE",
            json!({
                "guild_id": "66"
            }),
        )
        .unwrap()
        {
            Event::GuildIntegrationsUpdate(event) => {
                assert_eq!(event.guild_id, Some(snowflake("66")));
            }
            other => panic!("unexpected integrations update event: {other:?}"),
        }

        match decode_event(
            "WEBHOOKS_UPDATE",
            json!({
                "guild_id": "67",
                "channel_id": "68"
            }),
        )
        .unwrap()
        {
            Event::WebhooksUpdate(event) => {
                assert_eq!(event.guild_id, Some(snowflake("67")));
                assert_eq!(event.channel_id, Some(snowflake("68")));
            }
            other => panic!("unexpected webhooks update event: {other:?}"),
        }

        match decode_event(
            "INVITE_DELETE",
            json!({
                "guild_id": "69",
                "channel_id": "70",
                "code": "invite-code"
            }),
        )
        .unwrap()
        {
            Event::InviteDelete(event) => {
                assert_eq!(event.guild_id, Some(snowflake("69")));
                assert_eq!(event.channel_id, Some(snowflake("70")));
                assert_eq!(event.code.as_deref(), Some("invite-code"));
            }
            other => panic!("unexpected invite delete event: {other:?}"),
        }

        match decode_event(
            "MESSAGE_REACTION_REMOVE",
            json!({
                "user_id": "71",
                "channel_id": "72",
                "message_id": "73",
                "guild_id": "74",
                "emoji": {
                    "name": "x"
                }
            }),
        )
        .unwrap()
        {
            Event::MessageReactionRemove(event) => {
                assert_eq!(event.user_id, Some(snowflake("71")));
                assert_eq!(event.channel_id, Some(snowflake("72")));
                assert_eq!(event.message_id, Some(snowflake("73")));
                assert_eq!(event.guild_id, Some(snowflake("74")));
                assert_eq!(
                    event.emoji.and_then(|emoji| emoji.name),
                    Some("x".to_string())
                );
            }
            other => panic!("unexpected reaction remove event: {other:?}"),
        }

        match decode_event(
            "MESSAGE_REACTION_REMOVE_ALL",
            json!({
                "channel_id": "75",
                "message_id": "76",
                "guild_id": "77"
            }),
        )
        .unwrap()
        {
            Event::MessageReactionRemoveAll(event) => {
                assert_eq!(event.channel_id, Some(snowflake("75")));
                assert_eq!(event.message_id, Some(snowflake("76")));
                assert_eq!(event.guild_id, Some(snowflake("77")));
            }
            other => panic!("unexpected reaction remove all event: {other:?}"),
        }
    }

    #[test]
    fn decode_event_covers_success_payloads_with_present_optional_fields() {
        match decode_event(
            "READY",
            json!({
                "user": {
                    "id": "80",
                    "username": "ready-plus"
                },
                "session_id": "session-80",
                "application": {
                    "id": "81"
                },
                "resume_gateway_url": "wss://gateway.discord.test"
            }),
        )
        .unwrap()
        {
            Event::Ready(event) => {
                assert_eq!(event.data.user.id, snowflake("80"));
                assert_eq!(
                    event.data.application.map(|app| app.id),
                    Some(snowflake("81"))
                );
                assert_eq!(
                    event.data.resume_gateway_url.as_deref(),
                    Some("wss://gateway.discord.test")
                );
            }
            other => panic!("unexpected ready event: {other:?}"),
        }

        match decode_event(
            "GUILD_DELETE",
            json!({
                "id": "82",
                "unavailable": true
            }),
        )
        .unwrap()
        {
            Event::GuildDelete(event) => {
                assert_eq!(event.data.id, snowflake("82"));
                assert_eq!(event.data.unavailable, Some(true));
            }
            other => panic!("unexpected guild delete event: {other:?}"),
        }

        match decode_event(
            "MESSAGE_DELETE",
            json!({
                "id": "83",
                "channel_id": "84",
                "guild_id": "85"
            }),
        )
        .unwrap()
        {
            Event::MessageDelete(event) => {
                assert_eq!(event.data.id, snowflake("83"));
                assert_eq!(event.data.channel_id, snowflake("84"));
                assert_eq!(event.data.guild_id, Some(snowflake("85")));
            }
            other => panic!("unexpected message delete event: {other:?}"),
        }

        match decode_event(
            "CHANNEL_PINS_UPDATE",
            json!({
                "channel_id": "86",
                "guild_id": "87",
                "last_pin_timestamp": "2024-06-01T00:00:00Z"
            }),
        )
        .unwrap()
        {
            Event::ChannelPinsUpdate(event) => {
                assert_eq!(event.channel_id, snowflake("86"));
                assert_eq!(event.guild_id, Some(snowflake("87")));
                assert_eq!(
                    event.last_pin_timestamp.as_deref(),
                    Some("2024-06-01T00:00:00Z")
                );
            }
            other => panic!("unexpected channel pins update event: {other:?}"),
        }

        match decode_event(
            "GUILD_EMOJIS_UPDATE",
            json!({
                "guild_id": "88",
                "emojis": [
                    {
                        "name": "wave"
                    }
                ]
            }),
        )
        .unwrap()
        {
            Event::GuildEmojisUpdate(event) => {
                assert_eq!(event.guild_id, snowflake("88"));
                assert_eq!(event.emojis.len(), 1);
                assert_eq!(event.emojis[0].name.as_deref(), Some("wave"));
            }
            other => panic!("unexpected guild emojis update event: {other:?}"),
        }

        match decode_event(
            "INVITE_CREATE",
            json!({
                "guild_id": "89",
                "channel_id": "90",
                "code": "invite-create"
            }),
        )
        .unwrap()
        {
            Event::InviteCreate(event) => {
                assert_eq!(event.guild_id, Some(snowflake("89")));
                assert_eq!(event.channel_id, Some(snowflake("90")));
                assert_eq!(event.code.as_deref(), Some("invite-create"));
            }
            other => panic!("unexpected invite create event: {other:?}"),
        }
    }

    #[test]
    fn decode_event_covers_new_gateway_surface_variants() {
        match decode_event(
            "GUILD_MEMBERS_CHUNK",
            json!({
                "guild_id": "1",
                "members": [{
                    "user": { "id": "2", "username": "member" },
                    "roles": ["3"]
                }],
                "chunk_index": 0,
                "chunk_count": 1,
                "not_found": ["4"],
                "presences": [{
                    "user_id": "2",
                    "status": "online",
                    "activities": [{ "name": "Testing", "type": 0 }]
                }],
                "nonce": "abc"
            }),
        )
        .unwrap()
        {
            Event::GuildMembersChunk(event) => {
                assert_eq!(event.data.guild_id.as_str(), "1");
                assert_eq!(event.data.members.len(), 1);
                assert_eq!(event.data.not_found[0].as_str(), "4");
                assert_eq!(
                    event.data.presences.unwrap()[0]
                        .activities
                        .as_ref()
                        .unwrap()[0]
                        .name,
                    "Testing"
                );
                assert_eq!(event.data.nonce.as_deref(), Some("abc"));
            }
            other => panic!("unexpected guild members chunk event: {other:?}"),
        }

        match decode_event("RESUMED", json!({ "trace": [] })).unwrap() {
            Event::Resumed(event) => assert_eq!(event.raw["trace"], json!([])),
            other => panic!("unexpected resumed event: {other:?}"),
        }

        match decode_event(
            "VOICE_CHANNEL_STATUS_UPDATE",
            json!({
                "guild_id": "1",
                "channel_id": "2",
                "status": "Live"
            }),
        )
        .unwrap()
        {
            Event::VoiceChannelStatusUpdate(event) => {
                assert_eq!(event.channel_id.unwrap().as_str(), "2");
                assert_eq!(event.status.as_deref(), Some("Live"));
            }
            other => panic!("unexpected voice channel status event: {other:?}"),
        }

        let raw_channel_info = json!({
            "guild_id": "1",
            "channels": [{
                "id": "2",
                "status": "Live",
                "voice_start_time": "2026-05-01T00:00:00.000000+00:00"
            }]
        });
        let event = decode_event("CHANNEL_INFO", raw_channel_info.clone()).unwrap();
        assert_eq!(event.kind(), "CHANNEL_INFO");
        assert_eq!(event.raw(), &raw_channel_info);
        match event {
            Event::ChannelInfo(event) => {
                assert_eq!(event.guild_id.as_str(), "1");
                assert_eq!(event.channels[0].id.as_str(), "2");
                assert_eq!(event.channels[0].status.as_deref(), Some("Live"));
                assert_eq!(
                    event.channels[0].voice_start_time.as_deref(),
                    Some("2026-05-01T00:00:00.000000+00:00")
                );
            }
            other => panic!("unexpected channel info event: {other:?}"),
        }
    }

    #[test]
    fn decode_event_reports_required_field_errors_and_preserves_unknown_events() {
        let missing_guild_id = decode_event(
            "GUILD_MEMBER_ADD",
            json!({
                "user": {
                    "id": "20",
                    "username": "member"
                }
            }),
        )
        .unwrap_err();
        match missing_guild_id {
            DiscordError::Model { message } => assert_eq!(message, "missing field guild_id"),
            other => panic!("unexpected error: {other:?}"),
        }

        let invalid_guild_id = decode_event(
            "GUILD_ROLE_CREATE",
            json!({
                "guild_id": {},
                "role": {
                    "id": "21",
                    "name": "mods"
                }
            }),
        )
        .unwrap_err();
        assert!(
            matches!(invalid_guild_id, DiscordError::Json(message) if message.contains("snowflake"))
        );

        let missing_ids = decode_event(
            "MESSAGE_DELETE_BULK",
            json!({
                "channel_id": "22"
            }),
        )
        .unwrap_err();
        assert!(matches!(missing_ids, DiscordError::Json(_)));

        let raw = json!({ "x": 1 });
        let unknown = decode_event("SOMETHING_NEW", raw.clone()).unwrap();
        match unknown {
            Event::Unknown {
                kind,
                raw: event_raw,
            } => {
                assert_eq!(kind, "SOMETHING_NEW");
                assert_eq!(event_raw, raw);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn event_kind_and_raw_cover_remaining_variants() {
        let cases = vec![
            (
                "READY",
                Event::Ready(ReadyEvent {
                    data: ReadyPayload {
                        user: user("1", "ready"),
                        session_id: "session".to_string(),
                        application: Some(ReadyApplication { id: snowflake("2") }),
                        resume_gateway_url: Some("wss://gateway.discord.test".to_string()),
                    },
                    raw: raw("READY"),
                }),
            ),
            (
                "GUILD_CREATE",
                Event::GuildCreate(GuildEvent {
                    guild: guild("10", "guild-create"),
                    raw: raw("GUILD_CREATE"),
                }),
            ),
            (
                "GUILD_UPDATE",
                Event::GuildUpdate(GuildEvent {
                    guild: guild("11", "guild-update"),
                    raw: raw("GUILD_UPDATE"),
                }),
            ),
            (
                "GUILD_DELETE",
                Event::GuildDelete(GuildDeleteEvent {
                    data: GuildDeletePayload {
                        id: snowflake("12"),
                        unavailable: Some(true),
                    },
                    raw: raw("GUILD_DELETE"),
                }),
            ),
            (
                "CHANNEL_CREATE",
                Event::ChannelCreate(ChannelEvent {
                    channel: channel("13"),
                    raw: raw("CHANNEL_CREATE"),
                }),
            ),
            (
                "CHANNEL_UPDATE",
                Event::ChannelUpdate(ChannelEvent {
                    channel: channel("14"),
                    raw: raw("CHANNEL_UPDATE"),
                }),
            ),
            (
                "CHANNEL_DELETE",
                Event::ChannelDelete(ChannelEvent {
                    channel: channel("15"),
                    raw: raw("CHANNEL_DELETE"),
                }),
            ),
            (
                "GUILD_MEMBER_UPDATE",
                Event::MemberUpdate(MemberEvent {
                    guild_id: snowflake("16"),
                    member: member("17", "member-update"),
                    raw: raw("GUILD_MEMBER_UPDATE"),
                }),
            ),
            (
                "GUILD_MEMBER_REMOVE",
                Event::MemberRemove(MemberRemoveEvent {
                    data: MemberRemovePayload {
                        guild_id: snowflake("18"),
                        user: user("19", "member-remove"),
                    },
                    raw: raw("GUILD_MEMBER_REMOVE"),
                }),
            ),
            (
                "GUILD_ROLE_UPDATE",
                Event::RoleUpdate(RoleEvent {
                    guild_id: snowflake("20"),
                    role: role("21", "role-update"),
                    raw: raw("GUILD_ROLE_UPDATE"),
                }),
            ),
            (
                "GUILD_ROLE_DELETE",
                Event::RoleDelete(RoleDeleteEvent {
                    data: RoleDeletePayload {
                        guild_id: snowflake("22"),
                        role_id: snowflake("23"),
                    },
                    raw: raw("GUILD_ROLE_DELETE"),
                }),
            ),
            (
                "MESSAGE_UPDATE",
                Event::MessageUpdate(MessageEvent {
                    message: message("24", "25", "updated"),
                    raw: raw("MESSAGE_UPDATE"),
                }),
            ),
            (
                "MESSAGE_DELETE",
                Event::MessageDelete(MessageDeleteEvent {
                    data: MessageDeletePayload {
                        id: snowflake("26"),
                        channel_id: snowflake("27"),
                        guild_id: Some(snowflake("28")),
                    },
                    raw: raw("MESSAGE_DELETE"),
                }),
            ),
            (
                "GUILD_BAN_ADD",
                Event::GuildBanAdd(GuildBanEvent {
                    guild_id: snowflake("29"),
                    user: user("30", "ban-add"),
                    raw: raw("GUILD_BAN_ADD"),
                }),
            ),
            (
                "GUILD_BAN_REMOVE",
                Event::GuildBanRemove(GuildBanEvent {
                    guild_id: snowflake("31"),
                    user: user("32", "ban-remove"),
                    raw: raw("GUILD_BAN_REMOVE"),
                }),
            ),
            (
                "MESSAGE_REACTION_ADD",
                Event::MessageReactionAdd(ReactionEvent {
                    user_id: Some(snowflake("33")),
                    channel_id: Some(snowflake("34")),
                    message_id: Some(snowflake("35")),
                    guild_id: Some(snowflake("36")),
                    emoji: Some(Emoji::unicode("?뵦")),
                    raw: raw("MESSAGE_REACTION_ADD"),
                    ..ReactionEvent::default()
                }),
            ),
            (
                "MESSAGE_REACTION_REMOVE",
                Event::MessageReactionRemove(ReactionEvent {
                    user_id: Some(snowflake("37")),
                    channel_id: Some(snowflake("38")),
                    message_id: Some(snowflake("39")),
                    guild_id: Some(snowflake("40")),
                    emoji: Some(Emoji::unicode("?뵦")),
                    raw: raw("MESSAGE_REACTION_REMOVE"),
                    ..ReactionEvent::default()
                }),
            ),
            (
                "MESSAGE_REACTION_REMOVE_ALL",
                Event::MessageReactionRemoveAll(ReactionRemoveAllEvent {
                    channel_id: Some(snowflake("41")),
                    message_id: Some(snowflake("42")),
                    guild_id: Some(snowflake("43")),
                    raw: raw("MESSAGE_REACTION_REMOVE_ALL"),
                }),
            ),
            (
                "INTERACTION_CREATE",
                Event::InteractionCreate(InteractionEvent {
                    interaction: Interaction::Ping(PingInteraction {
                        context: interaction_context(),
                    }),
                    raw: raw("INTERACTION_CREATE"),
                }),
            ),
            (
                "VOICE_STATE_UPDATE",
                Event::VoiceStateUpdate(VoiceStateEvent {
                    state: VoiceState {
                        guild_id: Some(snowflake("44")),
                        channel_id: Some(snowflake("45")),
                        user_id: Some(snowflake("46")),
                        ..Default::default()
                    },
                    raw: raw("VOICE_STATE_UPDATE"),
                }),
            ),
            (
                "VOICE_SERVER_UPDATE",
                Event::VoiceServerUpdate(VoiceServerEvent {
                    data: VoiceServerUpdate {
                        guild_id: snowflake("47"),
                        token: "voice-token".to_string(),
                        endpoint: Some("wss://voice.discord.test".to_string()),
                    },
                    raw: raw("VOICE_SERVER_UPDATE"),
                }),
            ),
        ];

        for (kind, event) in cases {
            assert_kind_and_raw(event, kind);
        }
    }

    #[test]
    fn event_kind_and_raw_cover_missing_variants() {
        let cases = vec![
            (
                "GUILD_MEMBER_ADD",
                Event::MemberAdd(MemberEvent {
                    guild_id: snowflake("80"),
                    member: member("81", "member-add"),
                    raw: raw("GUILD_MEMBER_ADD"),
                }),
            ),
            (
                "GUILD_ROLE_CREATE",
                Event::RoleCreate(RoleEvent {
                    guild_id: snowflake("82"),
                    role: role("83", "role-create"),
                    raw: raw("GUILD_ROLE_CREATE"),
                }),
            ),
            (
                "MESSAGE_CREATE",
                Event::MessageCreate(MessageEvent {
                    message: message("84", "85", "created"),
                    raw: raw("MESSAGE_CREATE"),
                }),
            ),
            (
                "MESSAGE_DELETE_BULK",
                Event::MessageDeleteBulk(BulkMessageDeleteEvent {
                    ids: vec![snowflake("86"), snowflake("87")],
                    channel_id: snowflake("88"),
                    guild_id: Some(snowflake("89")),
                    raw: raw("MESSAGE_DELETE_BULK"),
                }),
            ),
            (
                "CHANNEL_PINS_UPDATE",
                Event::ChannelPinsUpdate(ChannelPinsUpdateEvent {
                    channel_id: snowflake("90"),
                    guild_id: Some(snowflake("91")),
                    last_pin_timestamp: Some("2024-07-01T00:00:00Z".to_string()),
                    raw: raw("CHANNEL_PINS_UPDATE"),
                }),
            ),
            (
                "GUILD_EMOJIS_UPDATE",
                Event::GuildEmojisUpdate(GuildEmojisUpdateEvent {
                    guild_id: snowflake("92"),
                    emojis: vec![Emoji::unicode("wave")],
                    raw: raw("GUILD_EMOJIS_UPDATE"),
                }),
            ),
            (
                "GUILD_INTEGRATIONS_UPDATE",
                Event::GuildIntegrationsUpdate(GuildIntegrationsUpdateEvent {
                    guild_id: Some(snowflake("93")),
                    raw: raw("GUILD_INTEGRATIONS_UPDATE"),
                }),
            ),
            (
                "ENTITLEMENT_CREATE",
                Event::EntitlementCreate(EntitlementEvent {
                    entitlement: Entitlement {
                        id: snowflake("930"),
                        sku_id: snowflake("931"),
                        application_id: snowflake("932"),
                        kind: 8,
                        deleted: false,
                        ..Entitlement::default()
                    },
                    raw: raw("ENTITLEMENT_CREATE"),
                }),
            ),
            (
                "SUBSCRIPTION_CREATE",
                Event::SubscriptionCreate(SubscriptionEvent {
                    subscription: Subscription {
                        id: snowflake("940"),
                        user_id: snowflake("941"),
                        current_period_start: "2026-04-01T00:00:00Z".to_string(),
                        current_period_end: "2026-05-01T00:00:00Z".to_string(),
                        status: 0,
                        ..Subscription::default()
                    },
                    raw: raw("SUBSCRIPTION_CREATE"),
                }),
            ),
            (
                "INTEGRATION_CREATE",
                Event::IntegrationCreate(IntegrationEvent {
                    guild_id: Some(snowflake("942")),
                    integration: Integration {
                        id: snowflake("943"),
                        name: "integration".to_string(),
                        kind: "discord".to_string(),
                        account: IntegrationAccount {
                            id: "account".to_string(),
                            name: "account".to_string(),
                        },
                        ..Integration::default()
                    },
                    raw: raw("INTEGRATION_CREATE"),
                }),
            ),
            (
                "INTEGRATION_DELETE",
                Event::IntegrationDelete(IntegrationDeleteEvent {
                    id: Some(snowflake("944")),
                    guild_id: Some(snowflake("945")),
                    application_id: Some(snowflake("946")),
                    raw: raw("INTEGRATION_DELETE"),
                }),
            ),
            (
                "GUILD_SOUNDBOARD_SOUND_CREATE",
                Event::GuildSoundboardSoundCreate(SoundboardSoundEvent {
                    sound: SoundboardSound {
                        name: "quack".to_string(),
                        sound_id: snowflake("933"),
                        guild_id: Some(snowflake("934")),
                        volume: 1.0,
                        available: true,
                        ..SoundboardSound::default()
                    },
                    raw: raw("GUILD_SOUNDBOARD_SOUND_CREATE"),
                }),
            ),
            (
                "GUILD_SOUNDBOARD_SOUND_DELETE",
                Event::GuildSoundboardSoundDelete(SoundboardSoundDeleteEvent {
                    sound_id: snowflake("935"),
                    guild_id: snowflake("936"),
                    raw: raw("GUILD_SOUNDBOARD_SOUND_DELETE"),
                }),
            ),
            (
                "SOUNDBOARD_SOUNDS",
                Event::SoundboardSounds(SoundboardSoundsEvent {
                    guild_id: snowflake("937"),
                    soundboard_sounds: vec![SoundboardSound {
                        name: "quack".to_string(),
                        sound_id: snowflake("938"),
                        volume: 1.0,
                        available: true,
                        ..SoundboardSound::default()
                    }],
                    raw: raw("SOUNDBOARD_SOUNDS"),
                }),
            ),
            (
                "ENTITLEMENT_UPDATE",
                Event::EntitlementUpdate(EntitlementEvent {
                    entitlement: Entitlement {
                        id: snowflake("939"),
                        sku_id: snowflake("940"),
                        application_id: snowflake("941"),
                        kind: 8,
                        deleted: false,
                        ..Entitlement::default()
                    },
                    raw: raw("ENTITLEMENT_UPDATE"),
                }),
            ),
            (
                "ENTITLEMENT_DELETE",
                Event::EntitlementDelete(EntitlementEvent {
                    entitlement: Entitlement {
                        id: snowflake("942"),
                        sku_id: snowflake("943"),
                        application_id: snowflake("944"),
                        kind: 8,
                        deleted: true,
                        ..Entitlement::default()
                    },
                    raw: raw("ENTITLEMENT_DELETE"),
                }),
            ),
            (
                "SUBSCRIPTION_UPDATE",
                Event::SubscriptionUpdate(SubscriptionEvent {
                    subscription: Subscription {
                        id: snowflake("945"),
                        user_id: snowflake("946"),
                        current_period_start: "2026-04-01T00:00:00Z".to_string(),
                        current_period_end: "2026-05-01T00:00:00Z".to_string(),
                        status: 1,
                        ..Subscription::default()
                    },
                    raw: raw("SUBSCRIPTION_UPDATE"),
                }),
            ),
            (
                "SUBSCRIPTION_DELETE",
                Event::SubscriptionDelete(SubscriptionEvent {
                    subscription: Subscription {
                        id: snowflake("947"),
                        user_id: snowflake("948"),
                        current_period_start: "2026-04-01T00:00:00Z".to_string(),
                        current_period_end: "2026-05-01T00:00:00Z".to_string(),
                        status: 2,
                        ..Subscription::default()
                    },
                    raw: raw("SUBSCRIPTION_DELETE"),
                }),
            ),
            (
                "INTEGRATION_UPDATE",
                Event::IntegrationUpdate(IntegrationEvent {
                    guild_id: Some(snowflake("949")),
                    integration: Integration {
                        id: snowflake("950"),
                        name: "updated-integration".to_string(),
                        kind: "discord".to_string(),
                        account: IntegrationAccount {
                            id: "account".to_string(),
                            name: "account".to_string(),
                        },
                        ..Integration::default()
                    },
                    raw: raw("INTEGRATION_UPDATE"),
                }),
            ),
            (
                "GUILD_SOUNDBOARD_SOUND_UPDATE",
                Event::GuildSoundboardSoundUpdate(SoundboardSoundEvent {
                    sound: SoundboardSound {
                        name: "updated".to_string(),
                        sound_id: snowflake("951"),
                        guild_id: Some(snowflake("952")),
                        volume: 1.0,
                        available: true,
                        ..SoundboardSound::default()
                    },
                    raw: raw("GUILD_SOUNDBOARD_SOUND_UPDATE"),
                }),
            ),
            (
                "GUILD_SOUNDBOARD_SOUNDS_UPDATE",
                Event::GuildSoundboardSoundsUpdate(SoundboardSoundsEvent {
                    guild_id: snowflake("953"),
                    soundboard_sounds: Vec::new(),
                    raw: raw("GUILD_SOUNDBOARD_SOUNDS_UPDATE"),
                }),
            ),
            (
                "WEBHOOKS_UPDATE",
                Event::WebhooksUpdate(WebhooksUpdateEvent {
                    guild_id: Some(snowflake("94")),
                    channel_id: Some(snowflake("95")),
                    raw: raw("WEBHOOKS_UPDATE"),
                }),
            ),
            (
                "INVITE_CREATE",
                Event::InviteCreate(InviteEvent {
                    guild_id: Some(snowflake("96")),
                    channel_id: Some(snowflake("97")),
                    code: Some("invite-create".to_string()),
                    raw: raw("INVITE_CREATE"),
                }),
            ),
            (
                "INVITE_DELETE",
                Event::InviteDelete(InviteEvent {
                    guild_id: Some(snowflake("98")),
                    channel_id: Some(snowflake("99")),
                    code: Some("invite-delete".to_string()),
                    raw: raw("INVITE_DELETE"),
                }),
            ),
            (
                "MESSAGE_POLL_VOTE_ADD",
                Event::MessagePollVoteAdd(PollVoteEvent {
                    user_id: Some(snowflake("980")),
                    channel_id: Some(snowflake("981")),
                    message_id: Some(snowflake("982")),
                    guild_id: Some(snowflake("983")),
                    answer_id: Some(1),
                    raw: raw("MESSAGE_POLL_VOTE_ADD"),
                }),
            ),
            (
                "MESSAGE_POLL_VOTE_REMOVE",
                Event::MessagePollVoteRemove(PollVoteEvent {
                    user_id: Some(snowflake("984")),
                    channel_id: Some(snowflake("985")),
                    message_id: Some(snowflake("986")),
                    guild_id: Some(snowflake("987")),
                    answer_id: Some(2),
                    raw: raw("MESSAGE_POLL_VOTE_REMOVE"),
                }),
            ),
            (
                "TYPING_START",
                Event::TypingStart(TypingStartEvent {
                    channel_id: Some(snowflake("100")),
                    guild_id: Some(snowflake("101")),
                    user_id: Some(snowflake("102")),
                    timestamp: Some(123),
                    raw: raw("TYPING_START"),
                }),
            ),
            (
                "PRESENCE_UPDATE",
                Event::PresenceUpdate(PresenceUpdateEvent {
                    user_id: Some(snowflake("103")),
                    guild_id: Some(snowflake("104")),
                    status: Some("idle".to_string()),
                    raw: raw("PRESENCE_UPDATE"),
                    ..PresenceUpdateEvent::default()
                }),
            ),
            (
                "THREAD_CREATE",
                Event::ThreadCreate(ThreadEvent {
                    thread: channel("110"),
                    raw: raw("THREAD_CREATE"),
                }),
            ),
            (
                "THREAD_UPDATE",
                Event::ThreadUpdate(ThreadEvent {
                    thread: channel("111"),
                    raw: raw("THREAD_UPDATE"),
                }),
            ),
            (
                "THREAD_DELETE",
                Event::ThreadDelete(ThreadEvent {
                    thread: channel("112"),
                    raw: raw("THREAD_DELETE"),
                }),
            ),
            (
                "THREAD_LIST_SYNC",
                Event::ThreadListSync(ThreadListSyncEvent {
                    guild_id: Some(snowflake("113")),
                    threads: vec![channel("114")],
                    raw: raw("THREAD_LIST_SYNC"),
                }),
            ),
            (
                "THREAD_MEMBER_UPDATE",
                Event::ThreadMemberUpdate(ThreadMemberUpdateEvent {
                    guild_id: Some(snowflake("115")),
                    thread_id: Some(snowflake("116")),
                    raw: raw("THREAD_MEMBER_UPDATE"),
                }),
            ),
            (
                "THREAD_MEMBERS_UPDATE",
                Event::ThreadMembersUpdate(ThreadMembersUpdateEvent {
                    guild_id: Some(snowflake("117")),
                    thread_id: Some(snowflake("118")),
                    added_members: Some(vec![json!({"id": "119"})]),
                    removed_member_ids: Some(vec![snowflake("120")]),
                    member_count: Some(2),
                    raw: raw("THREAD_MEMBERS_UPDATE"),
                }),
            ),
            (
                "MESSAGE_REACTION_REMOVE_EMOJI",
                Event::MessageReactionRemoveEmoji(ReactionRemoveEmojiEvent {
                    channel_id: Some(snowflake("121")),
                    message_id: Some(snowflake("122")),
                    guild_id: Some(snowflake("123")),
                    emoji: Some(Emoji::unicode("wave")),
                    raw: raw("MESSAGE_REACTION_REMOVE_EMOJI"),
                }),
            ),
            (
                "GUILD_STICKERS_UPDATE",
                Event::GuildStickersUpdate(GuildStickersUpdateEvent {
                    guild_id: Some(snowflake("124")),
                    stickers: Vec::new(),
                    raw: raw("GUILD_STICKERS_UPDATE"),
                }),
            ),
            (
                "GUILD_SCHEDULED_EVENT_UPDATE",
                Event::GuildScheduledEventUpdate(ScheduledEvent {
                    id: Some(snowflake("125")),
                    guild_id: Some(snowflake("126")),
                    raw: raw("GUILD_SCHEDULED_EVENT_UPDATE"),
                    ..ScheduledEvent::default()
                }),
            ),
            (
                "GUILD_SCHEDULED_EVENT_DELETE",
                Event::GuildScheduledEventDelete(ScheduledEvent {
                    id: Some(snowflake("127")),
                    guild_id: Some(snowflake("128")),
                    raw: raw("GUILD_SCHEDULED_EVENT_DELETE"),
                    ..ScheduledEvent::default()
                }),
            ),
            (
                "GUILD_SCHEDULED_EVENT_USER_ADD",
                Event::GuildScheduledEventUserAdd(GuildScheduledEventUserEvent {
                    guild_scheduled_event_id: snowflake("129"),
                    user_id: snowflake("130"),
                    guild_id: snowflake("131"),
                    member: None,
                    user: None,
                    raw: raw("GUILD_SCHEDULED_EVENT_USER_ADD"),
                }),
            ),
            (
                "GUILD_SCHEDULED_EVENT_USER_REMOVE",
                Event::GuildScheduledEventUserRemove(GuildScheduledEventUserEvent {
                    guild_scheduled_event_id: snowflake("132"),
                    user_id: snowflake("133"),
                    guild_id: snowflake("134"),
                    member: None,
                    user: None,
                    raw: raw("GUILD_SCHEDULED_EVENT_USER_REMOVE"),
                }),
            ),
            (
                "STAGE_INSTANCE_UPDATE",
                Event::StageInstanceUpdate(StageInstanceEvent {
                    stage_instance: StageInstance {
                        id: snowflake("135"),
                        guild_id: snowflake("136"),
                        channel_id: snowflake("137"),
                        topic: "updated".to_string(),
                        privacy_level: 2,
                        ..StageInstance::default()
                    },
                    raw: raw("STAGE_INSTANCE_UPDATE"),
                }),
            ),
            (
                "STAGE_INSTANCE_DELETE",
                Event::StageInstanceDelete(StageInstanceEvent {
                    stage_instance: StageInstance {
                        id: snowflake("138"),
                        guild_id: snowflake("139"),
                        channel_id: snowflake("140"),
                        topic: "deleted".to_string(),
                        privacy_level: 2,
                        ..StageInstance::default()
                    },
                    raw: raw("STAGE_INSTANCE_DELETE"),
                }),
            ),
            (
                "VOICE_CHANNEL_EFFECT_SEND",
                Event::VoiceChannelEffectSend(VoiceChannelEffectEvent {
                    channel_id: Some(snowflake("141")),
                    guild_id: Some(snowflake("142")),
                    user_id: Some(snowflake("143")),
                    emoji: Some(Emoji::unicode("wave")),
                    animation_type: Some(1),
                    animation_id: Some(2),
                    sound_id: Some(snowflake("144")),
                    sound_volume: Some(0.5),
                    raw: raw("VOICE_CHANNEL_EFFECT_SEND"),
                }),
            ),
            (
                "VOICE_CHANNEL_START_TIME_UPDATE",
                Event::VoiceChannelStartTimeUpdate(VoiceChannelStartTimeUpdateEvent {
                    channel_id: Some(snowflake("145")),
                    guild_id: Some(snowflake("146")),
                    voice_channel_start_time: Some("2026-05-02T00:00:00Z".to_string()),
                    raw: raw("VOICE_CHANNEL_START_TIME_UPDATE"),
                }),
            ),
            (
                "VOICE_CHANNEL_STATUS_UPDATE",
                Event::VoiceChannelStatusUpdate(VoiceChannelStatusUpdateEvent {
                    channel_id: Some(snowflake("147")),
                    guild_id: Some(snowflake("148")),
                    status: Some("live".to_string()),
                    raw: raw("VOICE_CHANNEL_STATUS_UPDATE"),
                }),
            ),
            (
                "CHANNEL_INFO",
                Event::ChannelInfo(ChannelInfoEvent {
                    guild_id: snowflake("149"),
                    channels: vec![ChannelInfoChannel {
                        id: snowflake("150"),
                        status: Some("live".to_string()),
                        voice_start_time: Some("2026-05-02T00:00:00Z".to_string()),
                        raw: raw("CHANNEL_INFO_CHANNEL"),
                    }],
                    raw: raw("CHANNEL_INFO"),
                }),
            ),
            (
                "RATE_LIMITED",
                Event::RateLimited(RateLimitedEvent {
                    opcode: Some(8),
                    retry_after: Some(1.5),
                    meta: Some(json!({"nonce": "members"})),
                    raw: raw("RATE_LIMITED"),
                }),
            ),
            (
                "APPLICATION_COMMAND_PERMISSIONS_UPDATE",
                Event::ApplicationCommandPermissionsUpdate(
                    ApplicationCommandPermissionsUpdateEvent {
                        id: Some(snowflake("151")),
                        application_id: Some(snowflake("152")),
                        guild_id: Some(snowflake("153")),
                        permissions: vec![json!({"id": "154", "type": 1, "permission": true})],
                        raw: raw("APPLICATION_COMMAND_PERMISSIONS_UPDATE"),
                    },
                ),
            ),
            (
                "AUTO_MODERATION_RULE_UPDATE",
                Event::AutoModerationRuleUpdate(AutoModerationEvent {
                    id: Some(snowflake("155")),
                    guild_id: Some(snowflake("156")),
                    name: Some("updated".to_string()),
                    creator_id: None,
                    event_type: None,
                    trigger_type: None,
                    trigger_metadata: None,
                    actions: Vec::new(),
                    enabled: None,
                    exempt_roles: Vec::new(),
                    exempt_channels: Vec::new(),
                    action: None,
                    rule_id: None,
                    rule_trigger_type: None,
                    user_id: None,
                    channel_id: None,
                    message_id: None,
                    alert_system_message_id: None,
                    content: None,
                    matched_keyword: None,
                    matched_content: None,
                    raw: raw("AUTO_MODERATION_RULE_UPDATE"),
                }),
            ),
            (
                "AUTO_MODERATION_RULE_DELETE",
                Event::AutoModerationRuleDelete(AutoModerationEvent {
                    id: Some(snowflake("157")),
                    guild_id: Some(snowflake("158")),
                    name: Some("deleted".to_string()),
                    creator_id: None,
                    event_type: None,
                    trigger_type: None,
                    trigger_metadata: None,
                    actions: Vec::new(),
                    enabled: None,
                    exempt_roles: Vec::new(),
                    exempt_channels: Vec::new(),
                    action: None,
                    rule_id: None,
                    rule_trigger_type: None,
                    user_id: None,
                    channel_id: None,
                    message_id: None,
                    alert_system_message_id: None,
                    content: None,
                    matched_keyword: None,
                    matched_content: None,
                    raw: raw("AUTO_MODERATION_RULE_DELETE"),
                }),
            ),
            (
                "SOMETHING_NEW",
                Event::Unknown {
                    kind: "SOMETHING_NEW".to_string(),
                    raw: raw("SOMETHING_NEW"),
                },
            ),
        ];

        for (kind, event) in cases {
            assert_kind_and_raw(event, kind);
        }
    }

    #[test]
    fn decode_event_handles_entitlement_and_soundboard_events() {
        let entitlement = decode_event(
            "ENTITLEMENT_UPDATE",
            json!({
                "id": "1",
                "sku_id": "2",
                "application_id": "3",
                "type": 8,
                "deleted": false,
                "consumed": false
            }),
        )
        .unwrap();
        match entitlement {
            Event::EntitlementUpdate(event) => {
                assert_eq!(event.entitlement.sku_id.as_str(), "2");
                assert!(!event.entitlement.deleted);
            }
            other => panic!("unexpected event: {other:?}"),
        }

        let subscription = decode_event(
            "SUBSCRIPTION_UPDATE",
            json!({
                "id": "30",
                "user_id": "31",
                "sku_ids": ["32"],
                "entitlement_ids": ["33"],
                "current_period_start": "2026-04-01T00:00:00Z",
                "current_period_end": "2026-05-01T00:00:00Z",
                "status": 1
            }),
        )
        .unwrap();
        match subscription {
            Event::SubscriptionUpdate(event) => {
                assert_eq!(event.subscription.id.as_str(), "30");
                assert_eq!(event.subscription.status, 1);
            }
            other => panic!("unexpected event: {other:?}"),
        }

        let integration = decode_event(
            "INTEGRATION_CREATE",
            json!({
                "id": "40",
                "guild_id": "41",
                "name": "integration",
                "type": "discord",
                "enabled": true,
                "account": { "id": "acc", "name": "account" }
            }),
        )
        .unwrap();
        match integration {
            Event::IntegrationCreate(event) => {
                assert_eq!(event.guild_id.unwrap().as_str(), "41");
                assert_eq!(event.integration.id.as_str(), "40");
            }
            other => panic!("unexpected event: {other:?}"),
        }

        let integration_delete = decode_event(
            "INTEGRATION_DELETE",
            json!({
                "id": "40",
                "guild_id": "41",
                "application_id": "42"
            }),
        )
        .unwrap();
        match integration_delete {
            Event::IntegrationDelete(event) => {
                assert_eq!(event.id.unwrap().as_str(), "40");
                assert_eq!(event.application_id.unwrap().as_str(), "42");
            }
            other => panic!("unexpected event: {other:?}"),
        }

        let poll_vote = decode_event(
            "MESSAGE_POLL_VOTE_REMOVE",
            json!({
                "user_id": "50",
                "channel_id": "51",
                "message_id": "52",
                "guild_id": "53",
                "answer_id": 2
            }),
        )
        .unwrap();
        match poll_vote {
            Event::MessagePollVoteRemove(event) => {
                assert_eq!(event.user_id.unwrap().as_str(), "50");
                assert_eq!(event.answer_id, Some(2));
            }
            other => panic!("unexpected event: {other:?}"),
        }

        let sound = decode_event(
            "GUILD_SOUNDBOARD_SOUND_UPDATE",
            json!({
                "name": "quack",
                "sound_id": "10",
                "volume": 1.0,
                "guild_id": "20",
                "available": true
            }),
        )
        .unwrap();
        match sound {
            Event::GuildSoundboardSoundUpdate(event) => {
                assert_eq!(event.sound.sound_id.as_str(), "10");
                assert_eq!(event.sound.guild_id.unwrap().as_str(), "20");
            }
            other => panic!("unexpected event: {other:?}"),
        }

        let delete = decode_event(
            "GUILD_SOUNDBOARD_SOUND_DELETE",
            json!({
                "sound_id": "10",
                "guild_id": "20"
            }),
        )
        .unwrap();
        match delete {
            Event::GuildSoundboardSoundDelete(event) => {
                assert_eq!(event.sound_id.as_str(), "10");
                assert_eq!(event.guild_id.as_str(), "20");
            }
            other => panic!("unexpected event: {other:?}"),
        }

        let sounds = decode_event(
            "GUILD_SOUNDBOARD_SOUNDS_UPDATE",
            json!({
                "guild_id": "20",
                "soundboard_sounds": [{
                    "name": "quack",
                    "sound_id": "10",
                    "volume": 1.0,
                    "available": true
                }]
            }),
        )
        .unwrap();
        match sounds {
            Event::GuildSoundboardSoundsUpdate(event) => {
                assert_eq!(event.guild_id.as_str(), "20");
                assert_eq!(event.soundboard_sounds.len(), 1);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
}
