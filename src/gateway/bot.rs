use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
#[cfg(feature = "sharding")]
use std::sync::Mutex as StdMutex;
#[cfg(feature = "sharding")]
use tokio::sync::watch;
use tokio::sync::{mpsc, RwLock};
#[cfg(feature = "sharding")]
use tokio::time::{sleep, Duration};
use tracing::{info, warn};

use crate::cache::{
    CacheConfig, CacheHandle, ChannelManager, GuildManager, MemberManager, MessageManager,
    RoleManager,
};
#[cfg(feature = "collectors")]
use crate::collector::CollectorHub;
use crate::error::DiscordError;
use crate::event::{decode_event, Event};
use crate::http::DiscordHttpClient;
use crate::model::Interaction;
#[cfg(feature = "sharding")]
use crate::sharding::{
    ShardInfo, ShardIpcMessage, ShardRuntimeChannels, ShardRuntimeState, ShardSupervisorEvent,
    ShardingManager,
};
use crate::types::invalid_data_error;
#[cfg(feature = "voice")]
use crate::voice::{AudioTrack, VoiceConnectionConfig, VoiceConnectionState, VoiceManager};
#[cfg(feature = "voice")]
use crate::voice_runtime::{
    connect as connect_voice_runtime_impl, VoiceRuntimeConfig, VoiceRuntimeHandle,
};
use crate::ws::GatewayConnectionConfig;

#[cfg(feature = "sharding")]
use super::client::SupervisorCallback;
use super::client::{EventCallback, GatewayClient, GatewayCommand, GATEWAY_COMMAND_QUEUE_CAPACITY};
use super::messenger::ShardMessenger;
#[cfg(feature = "sharding")]
use super::supervisor::{lock_sharding_manager, ShardSupervisor};

/// Typed Discord API object for `TypeMap`.
pub struct TypeMap(HashMap<TypeId, Box<dyn Any + Send + Sync>>);

impl TypeMap {
    /// Creates a `new` value.
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn insert<T: Send + Sync + 'static>(&mut self, val: T) {
        self.0.insert(TypeId::of::<T>(), Box::new(val));
    }

    pub fn get<T: Send + Sync + 'static>(&self) -> Option<&T> {
        self.0
            .get(&TypeId::of::<T>())
            .and_then(|b| b.downcast_ref())
    }

    pub fn get_mut<T: Send + Sync + 'static>(&mut self) -> Option<&mut T> {
        self.0
            .get_mut(&TypeId::of::<T>())
            .and_then(|b| b.downcast_mut())
    }
}

impl Default for TypeMap {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
/// Typed Discord API object for `Context`.
pub struct Context {
    pub http: Arc<DiscordHttpClient>,
    pub data: Arc<RwLock<TypeMap>>,
    pub cache: CacheHandle,
    pub shard_id: u32,
    pub shard_count: u32,
    gateway_commands: Arc<RwLock<HashMap<u32, ShardMessenger>>>,
    #[cfg(feature = "voice")]
    voice: Arc<RwLock<VoiceManager>>,
    #[cfg(feature = "collectors")]
    collectors: CollectorHub,
}

impl Context {
    /// Creates a `new` value.
    pub fn new(http: Arc<DiscordHttpClient>, data: Arc<RwLock<TypeMap>>) -> Self {
        Self {
            http,
            data,
            cache: CacheHandle::new(),
            shard_id: 0,
            shard_count: 1,
            gateway_commands: Arc::new(RwLock::new(HashMap::new())),
            #[cfg(feature = "voice")]
            voice: Arc::new(RwLock::new(VoiceManager::new())),
            #[cfg(feature = "collectors")]
            collectors: CollectorHub::new(),
        }
    }

    pub fn rest(&self) -> Arc<DiscordHttpClient> {
        Arc::clone(&self.http)
    }

    pub fn shard_pair(&self) -> (u32, u32) {
        (self.shard_id, self.shard_count)
    }

    #[cfg(feature = "sharding")]
    pub fn shard_info(&self) -> ShardInfo {
        ShardInfo {
            id: self.shard_id,
            total: self.shard_count,
        }
    }

    pub async fn insert_data<T: Send + Sync + 'static>(&self, value: T) {
        self.data.write().await.insert(value);
    }

    pub async fn with_data<R>(&self, map: impl FnOnce(&TypeMap) -> Option<R>) -> Option<R> {
        let data = self.data.read().await;
        map(&data)
    }

    pub async fn get_data_cloned<T>(&self) -> Option<T>
    where
        T: Send + Sync + Clone + 'static,
    {
        let data = self.data.read().await;
        data.get::<T>().cloned()
    }

    pub fn guilds(&self) -> GuildManager {
        GuildManager::new(Arc::clone(&self.http), self.cache.clone())
    }

    pub fn channels(&self) -> ChannelManager {
        ChannelManager::new(Arc::clone(&self.http), self.cache.clone())
    }

    pub fn users(&self) -> crate::cache::UserManager {
        crate::cache::UserManager::new(Arc::clone(&self.http), self.cache.clone())
    }

    pub fn members(&self) -> MemberManager {
        MemberManager::new(Arc::clone(&self.http), self.cache.clone())
    }

    pub fn messages(&self) -> MessageManager {
        MessageManager::new(Arc::clone(&self.http), self.cache.clone())
    }

    pub fn roles(&self) -> RoleManager {
        RoleManager::new(Arc::clone(&self.http), self.cache.clone())
    }

    pub async fn shard_messenger(&self) -> Option<ShardMessenger> {
        self.gateway_commands
            .read()
            .await
            .get(&self.shard_id)
            .cloned()
    }

    pub async fn update_presence(&self, status: impl Into<String>) -> Result<(), DiscordError> {
        let messenger = self
            .shard_messenger()
            .await
            .ok_or_else(|| invalid_data_error("missing shard messenger"))?;
        messenger.update_presence(status)
    }

    pub async fn update_presence_typed(
        &self,
        presence: crate::model::UpdatePresence,
    ) -> Result<(), DiscordError> {
        let messenger = self
            .shard_messenger()
            .await
            .ok_or_else(|| invalid_data_error("missing shard messenger"))?;
        messenger.update_presence_typed(presence)
    }

    pub async fn request_guild_members(
        &self,
        request: crate::model::RequestGuildMembers,
    ) -> Result<(), DiscordError> {
        let messenger = self
            .shard_messenger()
            .await
            .ok_or_else(|| invalid_data_error("missing shard messenger"))?;
        messenger.request_guild_members(request)
    }

    /// Requests ephemeral channel metadata through the active shard.
    pub async fn request_channel_info(
        &self,
        request: crate::model::RequestChannelInfo,
    ) -> Result<(), DiscordError> {
        let messenger = self
            .shard_messenger()
            .await
            .ok_or_else(|| invalid_data_error("missing shard messenger"))?;
        messenger.request_channel_info(request)
    }

    pub async fn reconnect_shard(&self) -> Result<(), DiscordError> {
        let messenger = self
            .shard_messenger()
            .await
            .ok_or_else(|| invalid_data_error("missing shard messenger"))?;
        messenger.reconnect()
    }

    pub async fn shutdown_shard(&self) -> Result<(), DiscordError> {
        let messenger = self
            .shard_messenger()
            .await
            .ok_or_else(|| invalid_data_error("missing shard messenger"))?;
        messenger.shutdown()
    }

    pub async fn update_voice_state(
        &self,
        guild_id: impl Into<crate::model::Snowflake>,
        channel_id: Option<crate::model::Snowflake>,
        self_mute: bool,
        self_deaf: bool,
    ) -> Result<(), DiscordError> {
        let messenger = self
            .shard_messenger()
            .await
            .ok_or_else(|| invalid_data_error("missing shard messenger"))?;
        messenger.update_voice_state(guild_id, channel_id, self_mute, self_deaf)
    }

    pub async fn request_soundboard_sounds(
        &self,
        guild_ids: impl IntoIterator<Item = crate::model::Snowflake>,
    ) -> Result<(), DiscordError> {
        let messenger = self
            .shard_messenger()
            .await
            .ok_or_else(|| invalid_data_error("missing shard messenger"))?;
        messenger.request_soundboard_sounds(guild_ids)
    }

    pub async fn join_voice(
        &self,
        guild_id: impl Into<crate::model::Snowflake>,
        channel_id: impl Into<crate::model::Snowflake>,
        self_mute: bool,
        self_deaf: bool,
    ) -> Result<(), DiscordError> {
        let messenger = self
            .shard_messenger()
            .await
            .ok_or_else(|| invalid_data_error("missing shard messenger"))?;
        messenger.join_voice(guild_id, channel_id, self_mute, self_deaf)
    }

    pub async fn leave_voice(
        &self,
        guild_id: impl Into<crate::model::Snowflake>,
        self_mute: bool,
        self_deaf: bool,
    ) -> Result<(), DiscordError> {
        let messenger = self
            .shard_messenger()
            .await
            .ok_or_else(|| invalid_data_error("missing shard messenger"))?;
        messenger.leave_voice(guild_id, self_mute, self_deaf)
    }

    #[cfg(feature = "voice")]
    pub fn voice(&self) -> Arc<RwLock<VoiceManager>> {
        Arc::clone(&self.voice)
    }

    #[cfg(feature = "voice")]
    pub async fn connect_voice(&self, config: VoiceConnectionConfig) -> VoiceConnectionState {
        let mut voice = self.voice.write().await;
        voice.connect(config)
    }

    #[cfg(feature = "voice")]
    pub async fn disconnect_voice(
        &self,
        guild_id: impl Into<crate::model::Snowflake>,
    ) -> Option<VoiceConnectionState> {
        let mut voice = self.voice.write().await;
        voice.disconnect(guild_id)
    }

    #[cfg(feature = "voice")]
    pub async fn enqueue_voice_track(
        &self,
        guild_id: impl Into<crate::model::Snowflake>,
        track: AudioTrack,
    ) -> Option<usize> {
        let mut voice = self.voice.write().await;
        voice.enqueue(guild_id, track)
    }

    #[cfg(feature = "voice")]
    pub async fn voice_runtime_config(
        &self,
        guild_id: impl Into<crate::model::Snowflake>,
        user_id: impl Into<crate::model::Snowflake>,
    ) -> Option<VoiceRuntimeConfig> {
        let voice = self.voice.read().await;
        voice.runtime_config(guild_id, user_id)
    }

    #[cfg(feature = "voice")]
    pub async fn connect_voice_runtime(
        &self,
        guild_id: impl Into<crate::model::Snowflake>,
        user_id: impl Into<crate::model::Snowflake>,
    ) -> Result<VoiceRuntimeHandle, DiscordError> {
        let config = self
            .voice_runtime_config(guild_id, user_id)
            .await
            .ok_or_else(|| {
                invalid_data_error("voice runtime requires endpoint, session_id, and token")
            })?;
        connect_voice_runtime_impl(config).await
    }

    #[cfg(feature = "collectors")]
    pub fn collectors(&self) -> &CollectorHub {
        &self.collectors
    }
}

/// Trait for `EventHandler` behavior.
#[allow(missing_docs)]
#[async_trait]
pub trait EventHandler: Send + Sync + 'static {
    /// Handles one decoded Gateway event and dispatches to typed convenience hooks by default.
    async fn handle_event(&self, ctx: Context, event: Event) {
        match event {
            Event::Ready(event) => self.ready(ctx, event.data).await,
            Event::GuildCreate(event) => self.guild_create(ctx, event.guild).await,
            Event::GuildUpdate(event) => self.guild_update(ctx, event.guild).await,
            Event::GuildDelete(event) => self.guild_delete(ctx, event.data).await,
            Event::ChannelCreate(event) => self.channel_create(ctx, event.channel).await,
            Event::ChannelUpdate(event) => self.channel_update(ctx, event.channel).await,
            Event::ChannelDelete(event) => self.channel_delete(ctx, event.channel).await,
            Event::MemberAdd(event) => self.member_add(ctx, event.guild_id, event.member).await,
            Event::MemberUpdate(event) => {
                self.member_update(ctx, event.guild_id, event.member).await
            }
            Event::MemberRemove(event) => {
                self.member_remove(ctx, event.data.guild_id, event.data.user)
                    .await
            }
            Event::GuildMembersChunk(event) => self.guild_members_chunk(ctx, event).await,
            Event::RoleCreate(event) => self.role_create(ctx, event.guild_id, event.role).await,
            Event::RoleUpdate(event) => self.role_update(ctx, event.guild_id, event.role).await,
            Event::RoleDelete(event) => {
                self.role_delete(ctx, event.data.guild_id, event.data.role_id)
                    .await
            }
            Event::MessageCreate(event) => self.message_create(ctx, event.message).await,
            Event::MessageUpdate(event) => self.message_update(ctx, event.message).await,
            Event::MessageDelete(event) => {
                self.message_delete(ctx, event.data.channel_id, event.data.id)
                    .await
            }
            Event::MessageDeleteBulk(event) => self.message_delete_bulk(ctx, event).await,
            Event::ChannelPinsUpdate(event) => self.channel_pins_update(ctx, event).await,
            Event::GuildBanAdd(event) => self.guild_ban_add(ctx, event).await,
            Event::GuildBanRemove(event) => self.guild_ban_remove(ctx, event).await,
            Event::GuildEmojisUpdate(event) => self.guild_emojis_update(ctx, event).await,
            Event::GuildIntegrationsUpdate(event) => {
                self.guild_integrations_update(ctx, event).await
            }
            Event::EntitlementCreate(event) => self.entitlement_create(ctx, event).await,
            Event::EntitlementUpdate(event) => self.entitlement_update(ctx, event).await,
            Event::EntitlementDelete(event) => self.entitlement_delete(ctx, event).await,
            Event::SubscriptionCreate(event) => self.subscription_create(ctx, event).await,
            Event::SubscriptionUpdate(event) => self.subscription_update(ctx, event).await,
            Event::SubscriptionDelete(event) => self.subscription_delete(ctx, event).await,
            Event::IntegrationCreate(event) => self.integration_create(ctx, event).await,
            Event::IntegrationUpdate(event) => self.integration_update(ctx, event).await,
            Event::IntegrationDelete(event) => self.integration_delete(ctx, event).await,
            Event::GuildSoundboardSoundCreate(event) => {
                self.guild_soundboard_sound_create(ctx, event).await
            }
            Event::GuildSoundboardSoundUpdate(event) => {
                self.guild_soundboard_sound_update(ctx, event).await
            }
            Event::GuildSoundboardSoundDelete(event) => {
                self.guild_soundboard_sound_delete(ctx, event).await
            }
            Event::GuildSoundboardSoundsUpdate(event) => {
                self.guild_soundboard_sounds_update(ctx, event).await
            }
            Event::SoundboardSounds(event) => self.soundboard_sounds(ctx, event).await,
            Event::WebhooksUpdate(event) => self.webhooks_update(ctx, event).await,
            Event::InviteCreate(event) => self.invite_create(ctx, event).await,
            Event::InviteDelete(event) => self.invite_delete(ctx, event).await,
            Event::VoiceStateUpdate(event) => self.voice_state_update(ctx, event.state).await,
            Event::VoiceServerUpdate(event) => self.voice_server_update(ctx, event.data).await,
            Event::Resumed(event) => self.resumed(ctx, event).await,
            Event::MessageReactionAdd(event) => self.reaction_add(ctx, event).await,
            Event::MessageReactionRemove(event) => self.reaction_remove(ctx, event).await,
            Event::MessageReactionRemoveAll(event) => self.reaction_remove_all(ctx, event).await,
            Event::MessagePollVoteAdd(event) => self.poll_vote_add(ctx, event).await,
            Event::MessagePollVoteRemove(event) => self.poll_vote_remove(ctx, event).await,
            Event::TypingStart(event) => self.typing_start(ctx, event).await,
            Event::PresenceUpdate(event) => self.presence_update(ctx, event).await,
            Event::InteractionCreate(event) => {
                self.interaction_create(ctx, event.interaction).await
            }
            Event::UserUpdate(event) => self.user_update(ctx, event.user).await,
            Event::ThreadCreate(event) => self.thread_create(ctx, event).await,
            Event::ThreadUpdate(event) => self.thread_update(ctx, event).await,
            Event::ThreadDelete(event) => self.thread_delete(ctx, event).await,
            Event::ThreadListSync(event) => self.thread_list_sync(ctx, event).await,
            Event::ThreadMemberUpdate(event) => self.thread_member_update(ctx, event).await,
            Event::ThreadMembersUpdate(event) => self.thread_members_update(ctx, event).await,
            Event::MessageReactionRemoveEmoji(event) => {
                self.reaction_remove_emoji(ctx, event).await
            }
            Event::GuildStickersUpdate(event) => self.guild_stickers_update(ctx, event).await,
            Event::GuildScheduledEventCreate(event) => {
                self.guild_scheduled_event_create(ctx, event).await
            }
            Event::GuildScheduledEventUpdate(event) => {
                self.guild_scheduled_event_update(ctx, event).await
            }
            Event::GuildScheduledEventDelete(event) => {
                self.guild_scheduled_event_delete(ctx, event).await
            }
            Event::GuildScheduledEventUserAdd(event) => {
                self.guild_scheduled_event_user_add(ctx, event).await
            }
            Event::GuildScheduledEventUserRemove(event) => {
                self.guild_scheduled_event_user_remove(ctx, event).await
            }
            Event::StageInstanceCreate(event) => self.stage_instance_create(ctx, event).await,
            Event::StageInstanceUpdate(event) => self.stage_instance_update(ctx, event).await,
            Event::StageInstanceDelete(event) => self.stage_instance_delete(ctx, event).await,
            Event::VoiceChannelEffectSend(event) => {
                self.voice_channel_effect_send(ctx, event).await
            }
            Event::VoiceChannelStartTimeUpdate(event) => {
                self.voice_channel_start_time_update(ctx, event).await
            }
            Event::VoiceChannelStatusUpdate(event) => {
                self.voice_channel_status_update(ctx, event).await
            }
            Event::ChannelInfo(event) => self.channel_info(ctx, event).await,
            Event::RateLimited(event) => self.gateway_rate_limited(ctx, event).await,
            Event::ApplicationCommandPermissionsUpdate(event) => {
                self.application_command_permissions_update(ctx, event)
                    .await
            }
            Event::AutoModerationRuleCreate(event) => {
                self.auto_moderation_rule_create(ctx, event).await
            }
            Event::AutoModerationRuleUpdate(event) => {
                self.auto_moderation_rule_update(ctx, event).await
            }
            Event::AutoModerationRuleDelete(event) => {
                self.auto_moderation_rule_delete(ctx, event).await
            }
            Event::AutoModerationActionExecution(event) => {
                self.auto_moderation_action_execution(ctx, event).await
            }
            Event::GuildAuditLogEntryCreate(event) => {
                self.guild_audit_log_entry_create(ctx, event).await
            }
            Event::Unknown { kind, raw } => self.raw_event(ctx, kind, raw).await,
        }
    }

    async fn ready(&self, _ctx: Context, _ready_data: crate::event::ReadyPayload) {}
    async fn guild_create(&self, _ctx: Context, _guild: crate::model::Guild) {}
    async fn guild_update(&self, _ctx: Context, _guild: crate::model::Guild) {}
    async fn guild_delete(&self, _ctx: Context, _data: crate::event::GuildDeletePayload) {}
    async fn channel_create(&self, _ctx: Context, _channel: crate::model::Channel) {}
    async fn channel_update(&self, _ctx: Context, _channel: crate::model::Channel) {}
    async fn channel_delete(&self, _ctx: Context, _channel: crate::model::Channel) {}
    async fn member_add(
        &self,
        _ctx: Context,
        _guild_id: crate::model::Snowflake,
        _member: crate::model::Member,
    ) {
    }
    async fn member_update(
        &self,
        _ctx: Context,
        _guild_id: crate::model::Snowflake,
        _member: crate::model::Member,
    ) {
    }
    async fn member_remove(
        &self,
        _ctx: Context,
        _guild_id: crate::model::Snowflake,
        _user: crate::model::User,
    ) {
    }
    async fn guild_members_chunk(
        &self,
        _ctx: Context,
        _event: crate::event::GuildMembersChunkEvent,
    ) {
    }
    async fn role_create(
        &self,
        _ctx: Context,
        _guild_id: crate::model::Snowflake,
        _role: crate::model::Role,
    ) {
    }
    async fn role_update(
        &self,
        _ctx: Context,
        _guild_id: crate::model::Snowflake,
        _role: crate::model::Role,
    ) {
    }
    async fn role_delete(
        &self,
        _ctx: Context,
        _guild_id: crate::model::Snowflake,
        _role_id: crate::model::Snowflake,
    ) {
    }
    async fn message_create(&self, _ctx: Context, _message: crate::model::Message) {}
    async fn message_update(&self, _ctx: Context, _message: crate::model::Message) {}
    async fn message_delete(
        &self,
        _ctx: Context,
        _channel_id: crate::model::Snowflake,
        _message_id: crate::model::Snowflake,
    ) {
    }
    async fn message_delete_bulk(
        &self,
        _ctx: Context,
        _event: crate::event::BulkMessageDeleteEvent,
    ) {
    }
    async fn channel_pins_update(
        &self,
        _ctx: Context,
        _event: crate::event::ChannelPinsUpdateEvent,
    ) {
    }
    async fn guild_ban_add(&self, _ctx: Context, _event: crate::event::GuildBanEvent) {}
    async fn guild_ban_remove(&self, _ctx: Context, _event: crate::event::GuildBanEvent) {}
    async fn guild_emojis_update(
        &self,
        _ctx: Context,
        _event: crate::event::GuildEmojisUpdateEvent,
    ) {
    }
    async fn guild_integrations_update(
        &self,
        _ctx: Context,
        _event: crate::event::GuildIntegrationsUpdateEvent,
    ) {
    }
    async fn entitlement_create(&self, _ctx: Context, _event: crate::event::EntitlementEvent) {}
    async fn entitlement_update(&self, _ctx: Context, _event: crate::event::EntitlementEvent) {}
    async fn entitlement_delete(&self, _ctx: Context, _event: crate::event::EntitlementEvent) {}
    async fn subscription_create(&self, _ctx: Context, _event: crate::event::SubscriptionEvent) {}
    async fn subscription_update(&self, _ctx: Context, _event: crate::event::SubscriptionEvent) {}
    async fn subscription_delete(&self, _ctx: Context, _event: crate::event::SubscriptionEvent) {}
    async fn integration_create(&self, _ctx: Context, _event: crate::event::IntegrationEvent) {}
    async fn integration_update(&self, _ctx: Context, _event: crate::event::IntegrationEvent) {}
    async fn integration_delete(
        &self,
        _ctx: Context,
        _event: crate::event::IntegrationDeleteEvent,
    ) {
    }
    async fn guild_soundboard_sound_create(
        &self,
        _ctx: Context,
        _event: crate::event::SoundboardSoundEvent,
    ) {
    }
    async fn guild_soundboard_sound_update(
        &self,
        _ctx: Context,
        _event: crate::event::SoundboardSoundEvent,
    ) {
    }
    async fn guild_soundboard_sound_delete(
        &self,
        _ctx: Context,
        _event: crate::event::SoundboardSoundDeleteEvent,
    ) {
    }
    async fn guild_soundboard_sounds_update(
        &self,
        _ctx: Context,
        _event: crate::event::SoundboardSoundsEvent,
    ) {
    }
    async fn soundboard_sounds(&self, _ctx: Context, _event: crate::event::SoundboardSoundsEvent) {}
    async fn webhooks_update(&self, _ctx: Context, _event: crate::event::WebhooksUpdateEvent) {}
    async fn invite_create(&self, _ctx: Context, _event: crate::event::InviteEvent) {}
    async fn invite_delete(&self, _ctx: Context, _event: crate::event::InviteEvent) {}
    async fn voice_state_update(&self, _ctx: Context, _state: crate::model::VoiceState) {}
    async fn voice_server_update(&self, _ctx: Context, _data: crate::model::VoiceServerUpdate) {}
    async fn resumed(&self, _ctx: Context, _event: crate::event::ResumedEvent) {}
    async fn reaction_add(&self, _ctx: Context, _data: crate::event::ReactionEvent) {}
    async fn reaction_remove(&self, _ctx: Context, _data: crate::event::ReactionEvent) {}
    async fn reaction_remove_all(
        &self,
        _ctx: Context,
        _event: crate::event::ReactionRemoveAllEvent,
    ) {
    }
    async fn poll_vote_add(&self, _ctx: Context, _event: crate::event::PollVoteEvent) {}
    async fn poll_vote_remove(&self, _ctx: Context, _event: crate::event::PollVoteEvent) {}
    async fn typing_start(&self, _ctx: Context, _data: crate::event::TypingStartEvent) {}
    async fn presence_update(&self, _ctx: Context, _data: crate::event::PresenceUpdateEvent) {}
    async fn interaction_create(&self, _ctx: Context, _interaction: crate::model::Interaction) {}
    async fn user_update(&self, _ctx: Context, _user: crate::model::User) {}
    async fn thread_create(&self, _ctx: Context, _event: crate::event::ThreadEvent) {}
    async fn thread_update(&self, _ctx: Context, _event: crate::event::ThreadEvent) {}
    async fn thread_delete(&self, _ctx: Context, _event: crate::event::ThreadEvent) {}
    async fn thread_list_sync(&self, _ctx: Context, _event: crate::event::ThreadListSyncEvent) {}
    async fn thread_member_update(
        &self,
        _ctx: Context,
        _event: crate::event::ThreadMemberUpdateEvent,
    ) {
    }
    async fn thread_members_update(
        &self,
        _ctx: Context,
        _event: crate::event::ThreadMembersUpdateEvent,
    ) {
    }
    async fn reaction_remove_emoji(
        &self,
        _ctx: Context,
        _event: crate::event::ReactionRemoveEmojiEvent,
    ) {
    }
    async fn guild_stickers_update(
        &self,
        _ctx: Context,
        _event: crate::event::GuildStickersUpdateEvent,
    ) {
    }
    async fn guild_scheduled_event_create(
        &self,
        _ctx: Context,
        _event: crate::event::ScheduledEvent,
    ) {
    }
    async fn guild_scheduled_event_update(
        &self,
        _ctx: Context,
        _event: crate::event::ScheduledEvent,
    ) {
    }
    async fn guild_scheduled_event_delete(
        &self,
        _ctx: Context,
        _event: crate::event::ScheduledEvent,
    ) {
    }
    async fn guild_scheduled_event_user_add(
        &self,
        _ctx: Context,
        _event: crate::event::GuildScheduledEventUserEvent,
    ) {
    }
    async fn guild_scheduled_event_user_remove(
        &self,
        _ctx: Context,
        _event: crate::event::GuildScheduledEventUserEvent,
    ) {
    }
    async fn stage_instance_create(&self, _ctx: Context, _event: crate::event::StageInstanceEvent) {
    }
    async fn stage_instance_update(&self, _ctx: Context, _event: crate::event::StageInstanceEvent) {
    }
    async fn stage_instance_delete(&self, _ctx: Context, _event: crate::event::StageInstanceEvent) {
    }
    async fn voice_channel_effect_send(
        &self,
        _ctx: Context,
        _event: crate::event::VoiceChannelEffectEvent,
    ) {
    }
    async fn voice_channel_start_time_update(
        &self,
        _ctx: Context,
        _event: crate::event::VoiceChannelStartTimeUpdateEvent,
    ) {
    }
    async fn voice_channel_status_update(
        &self,
        _ctx: Context,
        _event: crate::event::VoiceChannelStatusUpdateEvent,
    ) {
    }
    /// Handles typed `CHANNEL_INFO` Gateway dispatches.
    async fn channel_info(&self, _ctx: Context, _event: crate::event::ChannelInfoEvent) {}
    async fn gateway_rate_limited(&self, _ctx: Context, _event: crate::event::RateLimitedEvent) {}
    async fn application_command_permissions_update(
        &self,
        _ctx: Context,
        _event: crate::event::ApplicationCommandPermissionsUpdateEvent,
    ) {
    }
    async fn auto_moderation_rule_create(
        &self,
        _ctx: Context,
        _event: crate::event::AutoModerationEvent,
    ) {
    }
    async fn auto_moderation_rule_update(
        &self,
        _ctx: Context,
        _event: crate::event::AutoModerationEvent,
    ) {
    }
    async fn auto_moderation_rule_delete(
        &self,
        _ctx: Context,
        _event: crate::event::AutoModerationEvent,
    ) {
    }
    async fn auto_moderation_action_execution(
        &self,
        _ctx: Context,
        _event: crate::event::AutoModerationEvent,
    ) {
    }
    async fn guild_audit_log_entry_create(
        &self,
        _ctx: Context,
        _event: crate::event::AuditLogEntryEvent,
    ) {
    }
    async fn raw_event(&self, _ctx: Context, _event_name: String, _data: Value) {}
}

/// Typed Discord API object for `ClientBuilder`.
pub struct ClientBuilder {
    token: String,
    intents: u64,
    handler: Option<Arc<dyn EventHandler>>,
    data: TypeMap,
    application_id: Option<u64>,
    gateway_config: GatewayConnectionConfig,
    cache_config: CacheConfig,
    shard: Option<(u32, u32)>,
}

impl ClientBuilder {
    pub fn event_handler<H: EventHandler>(mut self, handler: H) -> Self {
        self.handler = Some(Arc::new(handler));
        self
    }

    pub fn application_id(mut self, id: u64) -> Self {
        self.application_id = Some(id);
        self
    }

    pub fn type_map_insert<T: Send + Sync + 'static>(mut self, val: T) -> Self {
        self.data.insert(val);
        self
    }

    pub fn gateway_config(mut self, gateway_config: GatewayConnectionConfig) -> Self {
        self.gateway_config = gateway_config;
        self
    }

    pub fn cache_config(mut self, cache_config: CacheConfig) -> Self {
        self.cache_config = cache_config;
        self
    }

    pub fn shard(mut self, shard_id: u32, shard_count: u32) -> Self {
        self.shard = Some((shard_id, shard_count.max(1)));
        self
    }

    /// Returns just the REST client without starting a gateway connection.
    pub fn rest_only(self) -> Arc<DiscordHttpClient> {
        let application_id = self.application_id.unwrap_or(0);
        Arc::new(DiscordHttpClient::new(self.token, application_id))
    }

    pub async fn start(self) -> Result<(), DiscordError> {
        let ClientBuilder {
            token,
            intents,
            handler,
            data,
            application_id,
            gateway_config,
            cache_config,
            shard,
        } = self;
        let handler = handler.ok_or("event_handler is required")?;
        let application_id = application_id.unwrap_or(0);
        let shard = shard.unwrap_or((0, 1));
        let runtime = SharedRuntime::new(&token, application_id, data, cache_config);
        #[cfg(feature = "sharding")]
        {
            start_gateway_shard(
                token,
                intents,
                handler,
                runtime,
                gateway_config,
                shard,
                ShardStartControl {
                    supervisor_channels: None,
                    boot_gate: None,
                },
            )
            .await
        }
        #[cfg(not(feature = "sharding"))]
        {
            start_gateway_shard(token, intents, handler, runtime, gateway_config, shard).await
        }
    }

    pub async fn start_shards(self, shard_count: u32) -> Result<(), DiscordError> {
        #[cfg(feature = "sharding")]
        {
            self.spawn_shards(shard_count).await?.wait().await
        }

        #[cfg(not(feature = "sharding"))]
        {
            let _ = shard_count;
            Err("sharding feature is required to start multiple shards".into())
        }
    }

    pub async fn start_auto_shards(self) -> Result<(), DiscordError> {
        #[cfg(feature = "sharding")]
        {
            self.spawn_auto_shards().await?.wait().await
        }

        #[cfg(not(feature = "sharding"))]
        {
            Err("sharding feature is required to auto-start shards".into())
        }
    }

    #[cfg(feature = "sharding")]
    pub async fn spawn_shards(self, shard_count: u32) -> Result<ShardSupervisor, DiscordError> {
        let ClientBuilder {
            token,
            intents,
            handler,
            data,
            application_id,
            gateway_config,
            cache_config,
            shard: _,
        } = self;
        let handler = handler.ok_or("event_handler is required")?;
        let application_id = application_id.unwrap_or(0);
        let total_shards = shard_count.max(1);
        let runtime = SharedRuntime::new(&token, application_id, data, cache_config);
        spawn_shard_supervisor(SpawnShardSupervisorConfig {
            token,
            intents,
            handler,
            runtime,
            gateway_config,
            total_shards,
            boot_window_size: 1,
            initial_delay: None,
        })
        .await
    }

    #[cfg(feature = "sharding")]
    pub async fn spawn_auto_shards(self) -> Result<ShardSupervisor, DiscordError> {
        let ClientBuilder {
            token,
            intents,
            handler,
            data,
            application_id,
            gateway_config,
            cache_config,
            shard: _,
        } = self;
        let handler = handler.ok_or("event_handler is required")?;
        let application_id = application_id.unwrap_or(0);
        let metadata_http = DiscordHttpClient::new(&token, application_id);
        let gateway_bot = metadata_http.get_gateway_bot().await?;
        let auto_shard_plan = auto_shard_plan(&gateway_bot);
        let runtime = SharedRuntime::new(&token, application_id, data, cache_config);
        let gateway_config = gateway_config.with_base_url(gateway_bot.url);

        spawn_shard_supervisor(SpawnShardSupervisorConfig {
            token,
            intents,
            handler,
            runtime,
            gateway_config,
            total_shards: auto_shard_plan.total_shards,
            boot_window_size: auto_shard_plan.boot_window_size,
            initial_delay: auto_shard_plan.initial_delay,
        })
        .await
    }
}

/// Typed Discord API object for `Client`.
pub struct Client;

impl Client {
    /// Creates a `builder` value.
    pub fn builder(
        token: impl Into<String>,
        intents: impl Into<crate::bitfield::Intents>,
    ) -> ClientBuilder {
        ClientBuilder {
            token: token.into(),
            intents: intents.into().bits(),
            handler: None,
            data: TypeMap::new(),
            application_id: None,
            gateway_config: GatewayConnectionConfig::default(),
            cache_config: CacheConfig::default(),
            shard: None,
        }
    }

    /// Creates a `rest` value.
    pub fn rest(token: impl Into<String>, application_id: u64) -> DiscordHttpClient {
        DiscordHttpClient::new(token, application_id)
    }
}

/// Type alias for `BotClient`.
pub type BotClient = Client;
/// Type alias for `BotClientBuilder`.
pub type BotClientBuilder = ClientBuilder;

#[cfg(feature = "sharding")]
const SHARD_BOOT_DELAY: Duration = Duration::from_millis(5_000);

#[derive(Clone)]
struct SharedRuntime {
    http: Arc<DiscordHttpClient>,
    data: Arc<RwLock<TypeMap>>,
    cache: CacheHandle,
    gateway_commands: Arc<RwLock<HashMap<u32, ShardMessenger>>>,
    #[cfg(feature = "voice")]
    voice: Arc<RwLock<VoiceManager>>,
    #[cfg(feature = "collectors")]
    collectors: CollectorHub,
}

impl SharedRuntime {
    fn new(token: &str, application_id: u64, data: TypeMap, cache_config: CacheConfig) -> Self {
        Self {
            http: Arc::new(DiscordHttpClient::new(token, application_id)),
            data: Arc::new(RwLock::new(data)),
            cache: CacheHandle::with_config(cache_config),
            gateway_commands: Arc::new(RwLock::new(HashMap::new())),
            #[cfg(feature = "voice")]
            voice: Arc::new(RwLock::new(VoiceManager::new())),
            #[cfg(feature = "collectors")]
            collectors: CollectorHub::new(),
        }
    }

    fn context(&self, shard: (u32, u32)) -> Context {
        let mut context = Context::new(Arc::clone(&self.http), Arc::clone(&self.data));
        context.cache = self.cache.clone();
        context.shard_id = shard.0;
        context.shard_count = shard.1;
        context.gateway_commands = Arc::clone(&self.gateway_commands);
        #[cfg(feature = "voice")]
        {
            context.voice = Arc::clone(&self.voice);
        }
        #[cfg(feature = "collectors")]
        {
            context.collectors = self.collectors.clone();
        }
        context
    }
}

#[cfg(feature = "sharding")]
struct ShardStartControl {
    supervisor_channels: Option<ShardRuntimeChannels>,
    boot_gate: Option<watch::Receiver<bool>>,
}

#[cfg(feature = "sharding")]
struct SpawnShardSupervisorConfig {
    token: String,
    intents: u64,
    handler: Arc<dyn EventHandler>,
    runtime: SharedRuntime,
    gateway_config: GatewayConnectionConfig,
    total_shards: u32,
    boot_window_size: u32,
    initial_delay: Option<Duration>,
}

#[cfg(feature = "sharding")]
struct AutoShardPlan {
    total_shards: u32,
    boot_window_size: u32,
    initial_delay: Option<Duration>,
}

#[cfg(feature = "sharding")]
fn auto_shard_plan(gateway_bot: &crate::model::GatewayBot) -> AutoShardPlan {
    AutoShardPlan {
        total_shards: gateway_bot.shards.max(1),
        boot_window_size: gateway_bot.session_start_limit.max_concurrency.max(1),
        initial_delay: if gateway_bot.session_start_limit.remaining == 0
            && gateway_bot.session_start_limit.reset_after > 0
        {
            Some(Duration::from_millis(
                gateway_bot.session_start_limit.reset_after,
            ))
        } else {
            None
        },
    }
}

#[cfg(feature = "sharding")]
async fn spawn_shard_supervisor(
    config: SpawnShardSupervisorConfig,
) -> Result<ShardSupervisor, DiscordError> {
    let SpawnShardSupervisorConfig {
        token,
        intents,
        handler,
        runtime,
        gateway_config,
        total_shards,
        boot_window_size,
        initial_delay,
    } = config;

    if let Some(initial_delay) = initial_delay {
        sleep(initial_delay).await;
    }

    let manager = Arc::new(StdMutex::new(ShardingManager::new(
        crate::sharding::ShardConfig::new(total_shards).gateway(gateway_config.clone()),
    )));
    let mut tasks = Vec::new();
    let mut queued_shards = Vec::new();

    for shard_id in 0..total_shards {
        let handler = Arc::clone(&handler);
        let runtime = runtime.clone();
        let token = token.clone();
        let gateway_config = gateway_config.clone().shard(shard_id, total_shards);
        let supervisor_channels = lock_sharding_manager(&manager).prepare_runtime(shard_id)?;
        let (boot_tx, boot_rx) = watch::channel(false);

        tasks.push((
            shard_id,
            tokio::spawn(async move {
                start_gateway_shard(
                    token,
                    intents,
                    handler,
                    runtime,
                    gateway_config,
                    (shard_id, total_shards),
                    ShardStartControl {
                        supervisor_channels: Some(supervisor_channels),
                        boot_gate: Some(boot_rx),
                    },
                )
                .await
            }),
        ));
        queued_shards.push((shard_id, boot_tx));
    }

    let wave_size = boot_window_size.max(1) as usize;
    for (wave_index, wave) in queued_shards.chunks(wave_size).enumerate() {
        for (_, boot_tx) in wave {
            let _ = boot_tx.send(true);
        }
        if wave_index + 1 < queued_shards.len().div_ceil(wave_size) {
            sleep(SHARD_BOOT_DELAY).await;
        }
    }

    Ok(ShardSupervisor { manager, tasks })
}

async fn start_gateway_shard(
    token: String,
    intents: u64,
    handler: Arc<dyn EventHandler>,
    runtime: SharedRuntime,
    gateway_config: GatewayConnectionConfig,
    shard: (u32, u32),
    #[cfg(feature = "sharding")] shard_control: ShardStartControl,
) -> Result<(), DiscordError> {
    #[cfg(feature = "sharding")]
    if let Some(mut boot_gate) = shard_control.boot_gate {
        if let Some(supervisor_channels) = shard_control.supervisor_channels.as_ref() {
            let _ = supervisor_channels.publish(ShardSupervisorEvent::StateChanged {
                shard_id: shard.0,
                state: ShardRuntimeState::Queued,
            });
        }

        while !*boot_gate.borrow() {
            if boot_gate.changed().await.is_err() {
                if let Some(supervisor_channels) = shard_control.supervisor_channels.as_ref() {
                    let _ = supervisor_channels.publish(ShardSupervisorEvent::StateChanged {
                        shard_id: shard.0,
                        state: ShardRuntimeState::Stopped,
                    });
                }
                return Ok(());
            }
        }
    }

    let ctx = runtime.context(shard);
    let http_for_app_id = Arc::clone(&runtime.http);
    let cache_for_events = runtime.cache.clone();
    let gateway_commands_for_runtime = Arc::clone(&runtime.gateway_commands);
    #[cfg(feature = "voice")]
    let voice_for_events = Arc::clone(&runtime.voice);
    #[cfg(feature = "collectors")]
    let collectors_for_events = runtime.collectors.clone();
    let (gateway_command_tx, gateway_command_rx) = mpsc::channel(GATEWAY_COMMAND_QUEUE_CAPACITY);
    let (event_tx, event_rx) = mpsc::unbounded_channel();

    runtime.gateway_commands.write().await.insert(
        shard.0,
        ShardMessenger {
            shard_id: shard.0,
            command_tx: gateway_command_tx.clone(),
        },
    );

    let event_processor = spawn_gateway_event_processor(
        Arc::clone(&handler),
        ctx.clone(),
        Arc::clone(&http_for_app_id),
        cache_for_events.clone(),
        #[cfg(feature = "voice")]
        Arc::clone(&voice_for_events),
        #[cfg(feature = "collectors")]
        collectors_for_events.clone(),
        event_rx,
    );

    let callback_tx = event_tx.clone();
    let callback: EventCallback = Arc::new(move |event_name: String, data: Value| {
        if callback_tx
            .send(GatewayDispatch { event_name, data })
            .is_err()
        {
            warn!("gateway event processor stopped before dispatch could be queued");
        }
    });

    let mut gateway = GatewayClient::new(token, intents)
        .gateway_config(gateway_config)
        .control(gateway_command_rx);
    if shard.1 > 1 {
        gateway = gateway.shard(shard.0, shard.1);
    }
    #[cfg(feature = "sharding")]
    if let Some(supervisor_channels) = shard_control.supervisor_channels {
        let (command_rx, publisher) = supervisor_channels.split();
        forward_shard_commands(command_rx, gateway_command_tx);
        gateway = gateway.supervisor(shard_supervisor_callback(publisher));
    }
    let result = gateway.run(callback.clone()).await;
    drop(callback);
    drop(event_tx);
    let event_processor_result = event_processor.await;
    gateway_commands_for_runtime.write().await.remove(&shard.0);
    if let Err(error) = event_processor_result {
        if result.is_ok() {
            return Err(invalid_data_error(format!(
                "gateway event processor task failed: {error}"
            )));
        }
        warn!("gateway event processor task failed after gateway exit: {error}");
    }
    result
}

#[derive(Debug)]
struct GatewayDispatch {
    event_name: String,
    data: Value,
}

fn spawn_gateway_event_processor(
    handler: Arc<dyn EventHandler>,
    ctx: Context,
    http_ref: Arc<DiscordHttpClient>,
    cache: CacheHandle,
    #[cfg(feature = "voice")] voice: Arc<RwLock<VoiceManager>>,
    #[cfg(feature = "collectors")] collectors: CollectorHub,
    mut event_rx: mpsc::UnboundedReceiver<GatewayDispatch>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(dispatch) = event_rx.recv().await {
            process_gateway_dispatch(
                &handler,
                &ctx,
                &http_ref,
                &cache,
                #[cfg(feature = "voice")]
                &voice,
                #[cfg(feature = "collectors")]
                &collectors,
                dispatch,
            )
            .await;
        }
    })
}

async fn process_gateway_dispatch(
    handler: &Arc<dyn EventHandler>,
    ctx: &Context,
    http_ref: &Arc<DiscordHttpClient>,
    cache: &CacheHandle,
    #[cfg(feature = "voice")] voice: &Arc<RwLock<VoiceManager>>,
    #[cfg(feature = "collectors")] collectors: &CollectorHub,
    dispatch: GatewayDispatch,
) {
    let GatewayDispatch { event_name, data } = dispatch;

    if event_name == "READY" && http_ref.application_id() == 0 {
        if let Some(app_id) = data
            .pointer("/application/id")
            .and_then(|value| value.as_str())
            .and_then(|value| value.parse::<u64>().ok())
        {
            http_ref.set_application_id(app_id);
            info!("Set application_id from READY: {app_id}");
        }
    }

    let event = match decode_event(&event_name, data.clone()) {
        Ok(event) => event,
        Err(error) => {
            warn!("Failed to decode event {event_name}: {error}");
            Event::Unknown {
                kind: event_name,
                raw: data,
            }
        }
    };

    apply_cache_updates(cache, &event).await;
    #[cfg(feature = "voice")]
    apply_voice_updates(voice, &event).await;
    #[cfg(feature = "collectors")]
    collectors.publish(event.clone());
    handler.handle_event(ctx.clone(), event).await;
}

async fn apply_cache_updates(cache: &CacheHandle, event: &Event) {
    match event {
        Event::Ready(_) => {
            cache.clear().await;
        }
        Event::GuildCreate(event) | Event::GuildUpdate(event) => {
            cache.upsert_guild(event.guild.clone()).await;
            for role in &event.guild.roles {
                cache
                    .upsert_role(event.guild.id.clone(), role.clone())
                    .await;
            }
        }
        Event::GuildDelete(event) => {
            cache.remove_guild(&event.data.id).await;
        }
        Event::ChannelCreate(event) | Event::ChannelUpdate(event) => {
            cache.upsert_channel(event.channel.clone()).await;
        }
        Event::ChannelDelete(event) => {
            cache.remove_channel(&event.channel.id).await;
        }
        Event::MemberAdd(event) | Event::MemberUpdate(event) => {
            if let Some(user) = event.member.user.as_ref() {
                cache.upsert_user(user.clone()).await;
                cache
                    .upsert_member(
                        event.guild_id.clone(),
                        user.id.clone(),
                        event.member.clone(),
                    )
                    .await;
            }
        }
        Event::MemberRemove(event) => {
            cache.upsert_user(event.data.user.clone()).await;
            cache
                .remove_member(&event.data.guild_id, &event.data.user.id)
                .await;
        }
        Event::RoleCreate(event) | Event::RoleUpdate(event) => {
            cache
                .upsert_role(event.guild_id.clone(), event.role.clone())
                .await;
        }
        Event::RoleDelete(event) => {
            cache
                .remove_role(&event.data.guild_id, &event.data.role_id)
                .await;
        }
        Event::MessageCreate(event) => {
            if let Some(author) = event.message.author.as_ref() {
                cache.upsert_user(author.clone()).await;
            }
            cache.upsert_message(event.message.clone()).await;
        }
        Event::MessageUpdate(event) => {
            if let Some(author) = event.message.author.as_ref() {
                cache.upsert_user(author.clone()).await;
            }
            if let Some(cached_message) = cache
                .message(&event.message.channel_id, &event.message.id)
                .await
            {
                cache
                    .upsert_message(merge_message_update(
                        cached_message,
                        event.message.clone(),
                        &event.raw,
                    ))
                    .await;
            }
        }
        Event::MessageDelete(event) => {
            cache
                .remove_message(&event.data.channel_id, &event.data.id)
                .await;
        }
        Event::MessageDeleteBulk(event) => {
            cache
                .remove_messages_bulk(&event.channel_id, &event.ids)
                .await;
        }
        Event::InteractionCreate(event) => {
            if let Interaction::Component(component) = &event.interaction {
                if let Some(channel_id) = component.context.channel_id.clone() {
                    cache
                        .upsert_channel(crate::model::Channel {
                            id: channel_id,
                            guild_id: component.context.guild_id.clone(),
                            kind: 0,
                            ..crate::model::Channel::default()
                        })
                        .await;
                }
            }
        }
        Event::VoiceStateUpdate(event) => {
            if let Some(member) = event.state.member.as_ref() {
                if let Some(user) = member.user.as_ref() {
                    cache.upsert_user(user.clone()).await;
                    if let Some(guild_id) = event.state.guild_id.clone() {
                        cache
                            .upsert_member(guild_id, user.id.clone(), member.clone())
                            .await;
                    }
                }
            }
            if let (Some(guild_id), Some(user_id)) =
                (event.state.guild_id.as_ref(), event.state.user_id.as_ref())
            {
                if event.state.channel_id.is_some() {
                    cache
                        .upsert_voice_state(guild_id.clone(), user_id.clone(), event.state.clone())
                        .await;
                } else {
                    cache.remove_voice_state(guild_id, user_id).await;
                }
            }
        }
        Event::VoiceServerUpdate(_) => {}
        Event::ChannelPinsUpdate(_) => {}
        Event::GuildBanAdd(_) | Event::GuildBanRemove(_) => {}
        Event::GuildEmojisUpdate(event) => {
            cache
                .replace_emojis(event.guild_id.clone(), event.emojis.clone())
                .await;
        }
        Event::GuildIntegrationsUpdate(_)
        | Event::IntegrationCreate(_)
        | Event::IntegrationUpdate(_)
        | Event::IntegrationDelete(_) => {}
        Event::GuildSoundboardSoundCreate(event) | Event::GuildSoundboardSoundUpdate(event) => {
            if let Some(guild_id) = event.sound.guild_id.clone() {
                cache
                    .upsert_soundboard_sound(guild_id, event.sound.clone())
                    .await;
            }
        }
        Event::GuildSoundboardSoundDelete(event) => {
            cache
                .remove_soundboard_sound(&event.guild_id, &event.sound_id)
                .await;
        }
        Event::GuildSoundboardSoundsUpdate(event) | Event::SoundboardSounds(event) => {
            cache
                .replace_soundboard_sounds(event.guild_id.clone(), event.soundboard_sounds.clone())
                .await;
        }
        Event::GuildStickersUpdate(event) => {
            if let Some(guild_id) = event.guild_id.clone() {
                cache
                    .replace_stickers(guild_id, event.stickers.clone())
                    .await;
            }
        }
        Event::GuildScheduledEventCreate(event) | Event::GuildScheduledEventUpdate(event) => {
            cache.upsert_scheduled_event(event.clone()).await;
        }
        Event::GuildScheduledEventDelete(event) => {
            if let (Some(guild_id), Some(event_id)) = (event.guild_id.as_ref(), event.id.as_ref()) {
                cache.remove_scheduled_event(guild_id, event_id).await;
            }
        }
        Event::StageInstanceCreate(event) | Event::StageInstanceUpdate(event) => {
            cache
                .upsert_stage_instance(event.stage_instance.clone())
                .await;
        }
        Event::StageInstanceDelete(event) => {
            cache
                .remove_stage_instance(&event.stage_instance.guild_id, &event.stage_instance.id)
                .await;
        }
        Event::EntitlementCreate(_)
        | Event::EntitlementUpdate(_)
        | Event::EntitlementDelete(_)
        | Event::SubscriptionCreate(_)
        | Event::SubscriptionUpdate(_)
        | Event::SubscriptionDelete(_) => {}
        Event::WebhooksUpdate(_) => {}
        Event::InviteCreate(_) | Event::InviteDelete(_) => {}
        Event::MessageReactionAdd(_)
        | Event::MessageReactionRemove(_)
        | Event::MessagePollVoteAdd(_)
        | Event::MessagePollVoteRemove(_)
        | Event::RateLimited(_) => {}
        Event::MessageReactionRemoveAll(_) => {}
        Event::TypingStart(_) => {}
        Event::PresenceUpdate(event) => {
            if let (Some(guild_id), Some(user_id)) =
                (event.guild_id.as_ref(), event.user_id.as_ref())
            {
                let activities = if event.activities.is_empty() {
                    None
                } else {
                    Some(event.activities.clone())
                };
                cache
                    .upsert_presence(
                        guild_id.clone(),
                        user_id.clone(),
                        crate::model::Presence {
                            user_id: Some(user_id.clone()),
                            status: event.status.clone(),
                            activities,
                            client_status: event.client_status.clone(),
                        },
                    )
                    .await;
            }
        }
        Event::UserUpdate(event) => {
            cache.upsert_user(event.user.clone()).await;
        }
        Event::Unknown { .. } => {}
        _ => {}
    }
}

fn merge_message_update(
    mut cached: crate::model::Message,
    partial: crate::model::Message,
    raw: &Value,
) -> crate::model::Message {
    cached.id = partial.id.clone();
    cached.channel_id = partial.channel_id.clone();

    if raw.get("guild_id").is_some() {
        cached.guild_id = partial.guild_id;
    }
    if raw.get("author").is_some() {
        cached.author = partial.author;
    }
    if raw.get("member").is_some() {
        cached.member = partial.member;
    }
    if raw.get("content").is_some() {
        cached.content = partial.content;
    }
    if raw.get("timestamp").is_some() {
        cached.timestamp = partial.timestamp;
    }
    if raw.get("edited_timestamp").is_some() {
        cached.edited_timestamp = partial.edited_timestamp;
    }
    if raw.get("mentions").is_some() {
        cached.mentions = partial.mentions;
    }
    if raw.get("attachments").is_some() {
        cached.attachments = partial.attachments;
    }
    if raw.get("type").is_some() {
        cached.kind = partial.kind;
    }
    if raw.get("pinned").is_some() {
        cached.pinned = partial.pinned;
    }
    if raw.get("tts").is_some() {
        cached.tts = partial.tts;
    }
    if raw.get("flags").is_some() {
        cached.flags = partial.flags;
    }
    if raw.get("webhook_id").is_some() {
        cached.webhook_id = partial.webhook_id;
    }
    if raw.get("embeds").is_some() {
        cached.embeds = partial.embeds;
    }
    if raw.get("reactions").is_some() {
        cached.reactions = partial.reactions;
    }

    cached
}

#[cfg(feature = "voice")]
async fn apply_voice_updates(voice: &Arc<RwLock<VoiceManager>>, event: &Event) {
    let mut voice = voice.write().await;
    match event {
        Event::VoiceStateUpdate(event) => {
            let _ = voice.update_voice_state(&event.state);
        }
        Event::VoiceServerUpdate(event) => {
            let _ = voice.update_server(event.data.clone());
        }
        _ => {}
    }
}

#[cfg(feature = "sharding")]
fn shard_supervisor_callback(
    supervisor_channels: crate::sharding::ShardRuntimePublisher,
) -> SupervisorCallback {
    Arc::new(move |event| {
        let _ = supervisor_channels.publish(event);
    })
}

#[cfg(feature = "sharding")]
fn forward_shard_commands(
    command_rx: std::sync::mpsc::Receiver<ShardIpcMessage>,
    gateway_command_tx: mpsc::Sender<GatewayCommand>,
) {
    tokio::task::spawn_blocking(move || {
        while let Ok(command) = command_rx.recv() {
            let gateway_command = match command {
                ShardIpcMessage::Shutdown => GatewayCommand::Shutdown,
                ShardIpcMessage::Reconnect => GatewayCommand::Reconnect,
                ShardIpcMessage::UpdatePresence(status) => GatewayCommand::UpdatePresence(status),
                ShardIpcMessage::SendPayload(payload) => GatewayCommand::SendPayload(payload),
            };

            if gateway_command_tx.try_send(gateway_command).is_err() {
                break;
            }
        }
    });
}

#[cfg(test)]
#[path = "bot/tests.rs"]
mod tests;
