use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::{
    Activity, AuditLogEntry, Channel, ClientStatus, Entitlement, Guild, Integration, Interaction,
    Member, Message, Presence, Role, Snowflake, SoundboardSound, StageInstance, Sticker,
    Subscription, User, VoiceServerUpdate, VoiceState,
};
use crate::types::Emoji;

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `ReadyApplication`.
pub struct ReadyApplication {
    pub id: Snowflake,
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

#[derive(Clone, Debug)]
/// Typed Discord API object for `ReadyEvent`.
pub struct ReadyEvent {
    pub data: ReadyPayload,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `GuildEvent`.
pub struct GuildEvent {
    pub guild: Guild,
    pub raw: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `GuildDeletePayload`.
pub struct GuildDeletePayload {
    pub id: Snowflake,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable: Option<bool>,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `GuildDeleteEvent`.
pub struct GuildDeleteEvent {
    pub data: GuildDeletePayload,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `ChannelEvent`.
pub struct ChannelEvent {
    pub channel: Channel,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `MemberEvent`.
pub struct MemberEvent {
    pub guild_id: Snowflake,
    pub member: Member,
    pub raw: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `MemberRemovePayload`.
pub struct MemberRemovePayload {
    pub guild_id: Snowflake,
    pub user: User,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `MemberRemoveEvent`.
pub struct MemberRemoveEvent {
    pub data: MemberRemovePayload,
    pub raw: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `GuildMembersChunkPayload`.
pub struct GuildMembersChunkPayload {
    pub guild_id: Snowflake,
    #[serde(default)]
    pub members: Vec<Member>,
    pub chunk_index: u64,
    pub chunk_count: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub not_found: Vec<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presences: Option<Vec<Presence>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `GuildMembersChunkEvent`.
pub struct GuildMembersChunkEvent {
    pub data: GuildMembersChunkPayload,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `RoleEvent`.
pub struct RoleEvent {
    pub guild_id: Snowflake,
    pub role: Role,
    pub raw: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `RoleDeletePayload`.
pub struct RoleDeletePayload {
    pub guild_id: Snowflake,
    pub role_id: Snowflake,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `RoleDeleteEvent`.
pub struct RoleDeleteEvent {
    pub data: RoleDeletePayload,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `MessageEvent`.
pub struct MessageEvent {
    pub message: Message,
    pub raw: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `MessageDeletePayload`.
pub struct MessageDeletePayload {
    pub id: Snowflake,
    pub channel_id: Snowflake,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<Snowflake>,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `MessageDeleteEvent`.
pub struct MessageDeleteEvent {
    pub data: MessageDeletePayload,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `InteractionEvent`.
pub struct InteractionEvent {
    pub interaction: Interaction,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `VoiceStateEvent`.
pub struct VoiceStateEvent {
    pub state: VoiceState,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `VoiceServerEvent`.
pub struct VoiceServerEvent {
    pub data: VoiceServerUpdate,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `ResumedEvent`.
pub struct ResumedEvent {
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `BulkMessageDeleteEvent`.
pub struct BulkMessageDeleteEvent {
    pub ids: Vec<Snowflake>,
    pub channel_id: Snowflake,
    pub guild_id: Option<Snowflake>,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `ChannelPinsUpdateEvent`.
pub struct ChannelPinsUpdateEvent {
    pub channel_id: Snowflake,
    pub guild_id: Option<Snowflake>,
    pub last_pin_timestamp: Option<String>,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `GuildBanEvent`.
pub struct GuildBanEvent {
    pub guild_id: Snowflake,
    pub user: User,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `GuildEmojisUpdateEvent`.
pub struct GuildEmojisUpdateEvent {
    pub guild_id: Snowflake,
    pub emojis: Vec<Emoji>,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `EntitlementEvent`.
pub struct EntitlementEvent {
    pub entitlement: Entitlement,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `SubscriptionEvent`.
pub struct SubscriptionEvent {
    pub subscription: Subscription,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `IntegrationEvent`.
pub struct IntegrationEvent {
    pub integration: Integration,
    pub guild_id: Option<Snowflake>,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `IntegrationDeleteEvent`.
pub struct IntegrationDeleteEvent {
    pub id: Option<Snowflake>,
    pub guild_id: Option<Snowflake>,
    pub application_id: Option<Snowflake>,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `PollVoteEvent`.
pub struct PollVoteEvent {
    pub user_id: Option<Snowflake>,
    pub channel_id: Option<Snowflake>,
    pub message_id: Option<Snowflake>,
    pub guild_id: Option<Snowflake>,
    pub answer_id: Option<u64>,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `SoundboardSoundEvent`.
pub struct SoundboardSoundEvent {
    pub sound: SoundboardSound,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `SoundboardSoundDeleteEvent`.
pub struct SoundboardSoundDeleteEvent {
    pub sound_id: Snowflake,
    pub guild_id: Snowflake,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `SoundboardSoundsEvent`.
pub struct SoundboardSoundsEvent {
    pub guild_id: Snowflake,
    pub soundboard_sounds: Vec<SoundboardSound>,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `InviteEvent`.
pub struct InviteEvent {
    pub guild_id: Option<Snowflake>,
    pub channel_id: Option<Snowflake>,
    pub code: Option<String>,
    pub raw: Value,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `ReactionEvent`.
pub struct ReactionEvent {
    pub user_id: Option<Snowflake>,
    pub channel_id: Option<Snowflake>,
    pub message_id: Option<Snowflake>,
    pub guild_id: Option<Snowflake>,
    pub member: Option<Member>,
    pub message_author_id: Option<Snowflake>,
    pub emoji: Option<Emoji>,
    pub burst: Option<bool>,
    pub burst_colors: Vec<String>,
    pub reaction_type: Option<u64>,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `ReactionRemoveAllEvent`.
pub struct ReactionRemoveAllEvent {
    pub channel_id: Option<Snowflake>,
    pub message_id: Option<Snowflake>,
    pub guild_id: Option<Snowflake>,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `TypingStartEvent`.
pub struct TypingStartEvent {
    pub channel_id: Option<Snowflake>,
    pub guild_id: Option<Snowflake>,
    pub user_id: Option<Snowflake>,
    pub timestamp: Option<u64>,
    pub raw: Value,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `PresenceUpdateEvent`.
pub struct PresenceUpdateEvent {
    pub user: Option<PresenceUpdateUser>,
    pub user_id: Option<Snowflake>,
    pub guild_id: Option<Snowflake>,
    pub status: Option<String>,
    pub activities: Vec<Activity>,
    pub client_status: Option<ClientStatus>,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `PresenceUpdateUser`.
pub struct PresenceUpdateUser {
    pub id: Snowflake,
    pub username: Option<String>,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `UserUpdateEvent`.
pub struct UserUpdateEvent {
    pub user: User,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `WebhooksUpdateEvent`.
pub struct WebhooksUpdateEvent {
    pub guild_id: Option<Snowflake>,
    pub channel_id: Option<Snowflake>,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `GuildIntegrationsUpdateEvent`.
pub struct GuildIntegrationsUpdateEvent {
    pub guild_id: Option<Snowflake>,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `ThreadEvent`.
pub struct ThreadEvent {
    pub thread: Channel,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `ThreadMemberUpdateEvent`.
pub struct ThreadMemberUpdateEvent {
    pub guild_id: Option<Snowflake>,
    pub thread_id: Option<Snowflake>,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `ThreadMembersUpdateEvent`.
pub struct ThreadMembersUpdateEvent {
    pub guild_id: Option<Snowflake>,
    pub thread_id: Option<Snowflake>,
    pub added_members: Option<Vec<serde_json::Value>>,
    pub removed_member_ids: Option<Vec<Snowflake>>,
    pub member_count: Option<u64>,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `ThreadListSyncEvent`.
pub struct ThreadListSyncEvent {
    pub guild_id: Option<Snowflake>,
    pub threads: Vec<Channel>,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `ReactionRemoveEmojiEvent`.
pub struct ReactionRemoveEmojiEvent {
    pub channel_id: Option<Snowflake>,
    pub message_id: Option<Snowflake>,
    pub guild_id: Option<Snowflake>,
    pub emoji: Option<Emoji>,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `GuildStickersUpdateEvent`.
pub struct GuildStickersUpdateEvent {
    pub guild_id: Option<Snowflake>,
    pub stickers: Vec<Sticker>,
    pub raw: Value,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `ScheduledEvent`.
pub struct ScheduledEvent {
    pub id: Option<Snowflake>,
    pub guild_id: Option<Snowflake>,
    pub channel_id: Option<Snowflake>,
    pub creator_id: Option<Snowflake>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub scheduled_start_time: Option<String>,
    pub scheduled_end_time: Option<String>,
    pub privacy_level: Option<u64>,
    pub status: Option<u64>,
    pub entity_type: Option<u64>,
    pub entity_id: Option<Snowflake>,
    pub entity_metadata: Option<Value>,
    pub user_count: Option<u64>,
    pub image: Option<String>,
    pub raw: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `GuildScheduledEventUserEvent`.
pub struct GuildScheduledEventUserEvent {
    pub guild_scheduled_event_id: Snowflake,
    pub user_id: Snowflake,
    pub guild_id: Snowflake,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member: Option<Member>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<User>,
    #[serde(skip)]
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `StageInstanceEvent`.
pub struct StageInstanceEvent {
    pub stage_instance: StageInstance,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `ApplicationCommandPermissionsUpdateEvent`.
pub struct ApplicationCommandPermissionsUpdateEvent {
    pub id: Option<Snowflake>,
    pub application_id: Option<Snowflake>,
    pub guild_id: Option<Snowflake>,
    pub permissions: Vec<Value>,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `VoiceChannelEffectEvent`.
pub struct VoiceChannelEffectEvent {
    pub channel_id: Option<Snowflake>,
    pub guild_id: Option<Snowflake>,
    pub user_id: Option<Snowflake>,
    pub emoji: Option<Emoji>,
    pub animation_type: Option<u64>,
    pub animation_id: Option<u64>,
    pub sound_id: Option<Snowflake>,
    pub sound_volume: Option<f64>,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `VoiceChannelStartTimeUpdateEvent`.
pub struct VoiceChannelStartTimeUpdateEvent {
    pub channel_id: Option<Snowflake>,
    pub guild_id: Option<Snowflake>,
    pub voice_channel_start_time: Option<String>,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `VoiceChannelStatusUpdateEvent`.
pub struct VoiceChannelStatusUpdateEvent {
    pub channel_id: Option<Snowflake>,
    pub guild_id: Option<Snowflake>,
    pub status: Option<String>,
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
    pub opcode: Option<u64>,
    pub retry_after: Option<f64>,
    pub meta: Option<Value>,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `AutoModerationEvent`.
pub struct AutoModerationEvent {
    pub id: Option<Snowflake>,
    pub guild_id: Option<Snowflake>,
    pub name: Option<String>,
    pub creator_id: Option<Snowflake>,
    pub event_type: Option<u64>,
    pub trigger_type: Option<u64>,
    pub trigger_metadata: Option<Value>,
    pub actions: Vec<Value>,
    pub enabled: Option<bool>,
    pub exempt_roles: Vec<Snowflake>,
    pub exempt_channels: Vec<Snowflake>,
    pub action: Option<Value>,
    pub rule_id: Option<Snowflake>,
    pub rule_trigger_type: Option<u64>,
    pub user_id: Option<Snowflake>,
    pub channel_id: Option<Snowflake>,
    pub message_id: Option<Snowflake>,
    pub alert_system_message_id: Option<Snowflake>,
    pub content: Option<String>,
    pub matched_keyword: Option<String>,
    pub matched_content: Option<String>,
    pub raw: Value,
}

#[derive(Clone, Debug)]
/// Typed Discord API object for `AuditLogEntryEvent`.
pub struct AuditLogEntryEvent {
    pub guild_id: Option<Snowflake>,
    pub entry: Option<AuditLogEntry>,
    pub id: Option<Snowflake>,
    pub user_id: Option<Snowflake>,
    pub target_id: Option<Snowflake>,
    pub action_type: Option<u64>,
    pub changes: Option<Vec<Value>>,
    pub options: Option<Value>,
    pub reason: Option<String>,
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

#[path = "event/decode.rs"]
mod decode;
pub use decode::decode_event;

#[cfg(test)]
#[path = "event/tests.rs"]
mod tests;
