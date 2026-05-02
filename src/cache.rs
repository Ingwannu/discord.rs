use std::sync::Arc;

#[cfg(feature = "cache")]
use std::time::Instant;
#[cfg(feature = "cache")]
use tokio::sync::RwLock;

use crate::event::ScheduledEvent;
use crate::model::{
    Channel, Guild, Member, Message, Presence, Role, Snowflake, SoundboardSound, StageInstance,
    Sticker, User, VoiceState,
};
use crate::types::Emoji;

#[path = "cache/config.rs"]
mod config;
pub use config::CacheConfig;

#[cfg(feature = "cache")]
#[path = "cache/backend.rs"]
mod backend;
#[cfg(feature = "cache")]
pub use backend::CacheBackend;

#[cfg(feature = "cache")]
#[path = "cache/store.rs"]
mod store;
#[cfg(feature = "cache")]
use store::{
    any_seen_expired, enforce_channel_limit, enforce_guild_limit, enforce_member_limit,
    enforce_message_limits, evict_channel_entries, evict_guild_entries, ordered_overflow_keys,
    prune_expired, remember_key, remove_member_key, remove_message_key, remove_presence_key,
    seen_expired, CacheStore,
};

#[derive(Clone, Default)]
/// Typed Discord API object for `CacheHandle`.
pub struct CacheHandle {
    #[cfg(feature = "cache")]
    store: Arc<RwLock<CacheStore>>,
    config: CacheConfig,
}

impl CacheHandle {
    /// Creates a `new` value.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a `with_config` value.
    pub fn with_config(config: CacheConfig) -> Self {
        Self {
            #[cfg(feature = "cache")]
            store: Arc::new(RwLock::new(CacheStore::default())),
            config,
        }
    }

    pub fn config(&self) -> &CacheConfig {
        &self.config
    }

    pub fn is_enabled(&self) -> bool {
        cfg!(feature = "cache")
    }

    #[cfg(feature = "cache")]
    pub async fn clear(&self) {
        let mut store = self.store.write().await;
        store.guilds.clear();
        store.channels.clear();
        store.users.clear();
        store.members.clear();
        store.messages.clear();
        store.roles.clear();
        store.presences.clear();
        store.voice_states.clear();
        store.soundboard_sounds.clear();
        store.emojis.clear();
        store.stickers.clear();
        store.scheduled_events.clear();
        store.stage_instances.clear();
        store.guild_order.clear();
        store.channel_order.clear();
        store.user_order.clear();
        store.member_order.clear();
        store.message_order.clear();
        store.role_order.clear();
        store.presence_order.clear();
        store.voice_state_order.clear();
        store.soundboard_sound_order.clear();
        store.emoji_order.clear();
        store.sticker_order.clear();
        store.scheduled_event_order.clear();
        store.stage_instance_order.clear();
        store.member_seen.clear();
        store.message_seen.clear();
        store.presence_seen.clear();
    }

    #[cfg(not(feature = "cache"))]
    pub async fn clear(&self) {}

    #[cfg(feature = "cache")]
    pub async fn purge_expired(&self) {
        let mut store = self.store.write().await;
        prune_expired(&mut store, &self.config, Instant::now());
    }

    #[cfg(not(feature = "cache"))]
    pub async fn purge_expired(&self) {}

    #[cfg(feature = "cache")]
    pub async fn upsert_guild(&self, guild: Guild) {
        let mut store = self.store.write().await;
        let guild_id = guild.id.clone();
        store.guilds.insert(guild_id.clone(), guild);
        remember_key(&mut store.guild_order, guild_id);
        enforce_guild_limit(&mut store, &self.config);
    }

    #[cfg(not(feature = "cache"))]
    pub async fn upsert_guild(&self, _guild: Guild) {}

    #[cfg(feature = "cache")]
    pub async fn remove_guild(&self, guild_id: &Snowflake) {
        let mut store = self.store.write().await;
        evict_guild_entries(&mut store, guild_id);
    }

    #[cfg(not(feature = "cache"))]
    pub async fn remove_guild(&self, _guild_id: &Snowflake) {}

    #[cfg(feature = "cache")]
    pub async fn guild(&self, guild_id: &Snowflake) -> Option<Guild> {
        self.store.read().await.guilds.get(guild_id).cloned()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn guild(&self, _guild_id: &Snowflake) -> Option<Guild> {
        None
    }

    #[cfg(feature = "cache")]
    pub async fn guilds(&self) -> Vec<Guild> {
        self.store.read().await.guilds.values().cloned().collect()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn guilds(&self) -> Vec<Guild> {
        Vec::new()
    }

    pub async fn contains_guild(&self, guild_id: &Snowflake) -> bool {
        self.guild(guild_id).await.is_some()
    }

    #[cfg(feature = "cache")]
    pub async fn upsert_channel(&self, channel: Channel) {
        let mut store = self.store.write().await;
        let channel_id = channel.id.clone();
        store.channels.insert(channel_id.clone(), channel);
        remember_key(&mut store.channel_order, channel_id);
        enforce_channel_limit(&mut store, &self.config);
    }

    #[cfg(not(feature = "cache"))]
    pub async fn upsert_channel(&self, _channel: Channel) {}

    #[cfg(feature = "cache")]
    pub async fn remove_channel(&self, channel_id: &Snowflake) {
        let mut store = self.store.write().await;
        evict_channel_entries(&mut store, channel_id);
    }

    #[cfg(not(feature = "cache"))]
    pub async fn remove_channel(&self, _channel_id: &Snowflake) {}

    #[cfg(not(feature = "cache"))]
    pub async fn channel(&self, _channel_id: &Snowflake) -> Option<Channel> {
        None
    }

    #[cfg(feature = "cache")]
    pub async fn channel(&self, channel_id: &Snowflake) -> Option<Channel> {
        self.store.read().await.channels.get(channel_id).cloned()
    }

    #[cfg(feature = "cache")]
    pub async fn channels(&self) -> Vec<Channel> {
        self.store.read().await.channels.values().cloned().collect()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn channels(&self) -> Vec<Channel> {
        Vec::new()
    }

    pub async fn contains_channel(&self, channel_id: &Snowflake) -> bool {
        self.channel(channel_id).await.is_some()
    }

    #[cfg(feature = "cache")]
    pub async fn upsert_user(&self, user: User) {
        let mut store = self.store.write().await;
        let user_id = user.id.clone();
        store.users.insert(user_id.clone(), user);
        remember_key(&mut store.user_order, user_id);
        let len = store.users.len();
        for key in ordered_overflow_keys(&mut store.user_order, len, self.config.max_users) {
            store.users.remove(&key);
        }
    }

    #[cfg(not(feature = "cache"))]
    pub async fn upsert_user(&self, _user: User) {}

    #[cfg(feature = "cache")]
    pub async fn remove_user(&self, user_id: &Snowflake) {
        let mut store = self.store.write().await;
        store.users.remove(user_id);
        store.user_order.retain(|stored_id| stored_id != user_id);
    }

    #[cfg(not(feature = "cache"))]
    pub async fn remove_user(&self, _user_id: &Snowflake) {}

    #[cfg(feature = "cache")]
    pub async fn user(&self, user_id: &Snowflake) -> Option<User> {
        self.store.read().await.users.get(user_id).cloned()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn user(&self, _user_id: &Snowflake) -> Option<User> {
        None
    }

    #[cfg(feature = "cache")]
    pub async fn users(&self) -> Vec<User> {
        self.store.read().await.users.values().cloned().collect()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn users(&self) -> Vec<User> {
        Vec::new()
    }

    pub async fn contains_user(&self, user_id: &Snowflake) -> bool {
        self.user(user_id).await.is_some()
    }

    #[cfg(feature = "cache")]
    pub async fn upsert_member(&self, guild_id: Snowflake, user_id: Snowflake, member: Member) {
        let mut store = self.store.write().await;
        let key = (guild_id.clone(), user_id);
        store.members.insert(key.clone(), Arc::new(member));
        store.member_seen.insert(key.clone(), Instant::now());
        remember_key(&mut store.member_order, key);
        prune_expired(&mut store, &self.config, Instant::now());
        enforce_member_limit(&mut store, &self.config, &guild_id);
    }

    #[cfg(not(feature = "cache"))]
    pub async fn upsert_member(&self, _guild_id: Snowflake, _user_id: Snowflake, _member: Member) {}

    #[cfg(feature = "cache")]
    pub async fn remove_member(&self, guild_id: &Snowflake, user_id: &Snowflake) {
        let mut store = self.store.write().await;
        let key = (guild_id.clone(), user_id.clone());
        remove_member_key(&mut store, &key);
    }

    #[cfg(not(feature = "cache"))]
    pub async fn remove_member(&self, _guild_id: &Snowflake, _user_id: &Snowflake) {}

    #[cfg(not(feature = "cache"))]
    pub async fn member(&self, _guild_id: &Snowflake, _user_id: &Snowflake) -> Option<Member> {
        None
    }

    #[cfg(feature = "cache")]
    pub async fn member(&self, guild_id: &Snowflake, user_id: &Snowflake) -> Option<Member> {
        self.member_arc(guild_id, user_id)
            .await
            .map(|member| member.as_ref().clone())
    }

    #[cfg(feature = "cache")]
    pub async fn member_arc(
        &self,
        guild_id: &Snowflake,
        user_id: &Snowflake,
    ) -> Option<Arc<Member>> {
        let key = (guild_id.clone(), user_id.clone());
        let now = Instant::now();
        {
            let store = self.store.read().await;
            if !seen_expired(store.member_seen.get(&key), self.config.member_ttl, now) {
                return store.members.get(&key).cloned();
            }
        }

        let mut store = self.store.write().await;
        prune_expired(&mut store, &self.config, now);
        store.members.get(&key).cloned()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn member_arc(
        &self,
        _guild_id: &Snowflake,
        _user_id: &Snowflake,
    ) -> Option<Arc<Member>> {
        None
    }

    #[cfg(feature = "cache")]
    pub async fn members(&self, guild_id: &Snowflake) -> Vec<Member> {
        self.members_arc(guild_id)
            .await
            .into_iter()
            .map(|member| member.as_ref().clone())
            .collect()
    }

    #[cfg(feature = "cache")]
    pub async fn members_arc(&self, guild_id: &Snowflake) -> Vec<Arc<Member>> {
        let now = Instant::now();
        {
            let store = self.store.read().await;
            if !any_seen_expired(store.member_seen.values(), self.config.member_ttl, now) {
                return store
                    .members
                    .iter()
                    .filter(|((stored_guild_id, _), _)| stored_guild_id == guild_id)
                    .map(|(_, member)| member.clone())
                    .collect();
            }
        }

        let mut store = self.store.write().await;
        prune_expired(&mut store, &self.config, now);
        store
            .members
            .iter()
            .filter(|((stored_guild_id, _), _)| stored_guild_id == guild_id)
            .map(|(_, member)| member.clone())
            .collect()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn members_arc(&self, _guild_id: &Snowflake) -> Vec<Arc<Member>> {
        Vec::new()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn members(&self, _guild_id: &Snowflake) -> Vec<Member> {
        Vec::new()
    }

    pub async fn contains_member(&self, guild_id: &Snowflake, user_id: &Snowflake) -> bool {
        self.member(guild_id, user_id).await.is_some()
    }

    #[cfg(feature = "cache")]
    pub async fn upsert_message(&self, message: Message) {
        let channel_id = message.channel_id.clone();
        let message_id = message.id.clone();
        let mut store = self.store.write().await;
        let key = (channel_id.clone(), message_id);
        store.messages.insert(key.clone(), Arc::new(message));
        store.message_seen.insert(key.clone(), Instant::now());
        remember_key(&mut store.message_order, key);
        prune_expired(&mut store, &self.config, Instant::now());
        enforce_message_limits(&mut store, &self.config, &channel_id);
    }

    #[cfg(not(feature = "cache"))]
    pub async fn upsert_message(&self, _message: Message) {}

    #[cfg(feature = "cache")]
    pub async fn remove_message(&self, channel_id: &Snowflake, message_id: &Snowflake) {
        let mut store = self.store.write().await;
        let key = (channel_id.clone(), message_id.clone());
        remove_message_key(&mut store, &key);
    }

    #[cfg(not(feature = "cache"))]
    pub async fn remove_message(&self, _channel_id: &Snowflake, _message_id: &Snowflake) {}

    #[cfg(feature = "cache")]
    pub async fn remove_messages_bulk(&self, channel_id: &Snowflake, message_ids: &[Snowflake]) {
        let mut store = self.store.write().await;
        store
            .messages
            .retain(|(stored_channel_id, stored_message_id), _| {
                stored_channel_id != channel_id || !message_ids.contains(stored_message_id)
            });
        store
            .message_seen
            .retain(|(stored_channel_id, stored_message_id), _| {
                stored_channel_id != channel_id || !message_ids.contains(stored_message_id)
            });
        store
            .message_order
            .retain(|(stored_channel_id, stored_message_id)| {
                stored_channel_id != channel_id || !message_ids.contains(stored_message_id)
            });
    }

    #[cfg(not(feature = "cache"))]
    pub async fn remove_messages_bulk(&self, _channel_id: &Snowflake, _message_ids: &[Snowflake]) {}

    #[cfg(not(feature = "cache"))]
    pub async fn message(
        &self,
        _channel_id: &Snowflake,
        _message_id: &Snowflake,
    ) -> Option<Message> {
        None
    }

    #[cfg(feature = "cache")]
    pub async fn message(&self, channel_id: &Snowflake, message_id: &Snowflake) -> Option<Message> {
        self.message_arc(channel_id, message_id)
            .await
            .map(|message| message.as_ref().clone())
    }

    #[cfg(feature = "cache")]
    pub async fn message_arc(
        &self,
        channel_id: &Snowflake,
        message_id: &Snowflake,
    ) -> Option<Arc<Message>> {
        let key = (channel_id.clone(), message_id.clone());
        let now = Instant::now();
        {
            let store = self.store.read().await;
            if !seen_expired(store.message_seen.get(&key), self.config.message_ttl, now) {
                return store.messages.get(&key).cloned();
            }
        }

        let mut store = self.store.write().await;
        prune_expired(&mut store, &self.config, now);
        store.messages.get(&key).cloned()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn message_arc(
        &self,
        _channel_id: &Snowflake,
        _message_id: &Snowflake,
    ) -> Option<Arc<Message>> {
        None
    }

    #[cfg(feature = "cache")]
    pub async fn messages(&self, channel_id: &Snowflake) -> Vec<Message> {
        self.messages_arc(channel_id)
            .await
            .into_iter()
            .map(|message| message.as_ref().clone())
            .collect()
    }

    #[cfg(feature = "cache")]
    pub async fn messages_arc(&self, channel_id: &Snowflake) -> Vec<Arc<Message>> {
        let now = Instant::now();
        {
            let store = self.store.read().await;
            if !any_seen_expired(store.message_seen.values(), self.config.message_ttl, now) {
                return store
                    .messages
                    .iter()
                    .filter(|((stored_channel_id, _), _)| stored_channel_id == channel_id)
                    .map(|(_, message)| message.clone())
                    .collect();
            }
        }

        let mut store = self.store.write().await;
        prune_expired(&mut store, &self.config, now);
        store
            .messages
            .iter()
            .filter(|((stored_channel_id, _), _)| stored_channel_id == channel_id)
            .map(|(_, message)| message.clone())
            .collect()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn messages_arc(&self, _channel_id: &Snowflake) -> Vec<Arc<Message>> {
        Vec::new()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn messages(&self, _channel_id: &Snowflake) -> Vec<Message> {
        Vec::new()
    }

    pub async fn contains_message(&self, channel_id: &Snowflake, message_id: &Snowflake) -> bool {
        self.message(channel_id, message_id).await.is_some()
    }

    #[cfg(feature = "cache")]
    pub async fn upsert_role(&self, guild_id: Snowflake, role: Role) {
        let mut store = self.store.write().await;
        let key = (guild_id, role.id.clone());
        store.roles.insert(key.clone(), role);
        remember_key(&mut store.role_order, key);
        let len = store.roles.len();
        for key in ordered_overflow_keys(&mut store.role_order, len, self.config.max_roles) {
            store.roles.remove(&key);
        }
    }

    #[cfg(not(feature = "cache"))]
    pub async fn upsert_role(&self, _guild_id: Snowflake, _role: Role) {}

    #[cfg(feature = "cache")]
    pub async fn remove_role(&self, guild_id: &Snowflake, role_id: &Snowflake) {
        let mut store = self.store.write().await;
        let key = (guild_id.clone(), role_id.clone());
        store.roles.remove(&key);
        store.role_order.retain(|stored_key| stored_key != &key);
    }

    #[cfg(not(feature = "cache"))]
    pub async fn remove_role(&self, _guild_id: &Snowflake, _role_id: &Snowflake) {}

    #[cfg(not(feature = "cache"))]
    pub async fn roles(&self, _guild_id: &Snowflake) -> Vec<Role> {
        Vec::new()
    }

    #[cfg(feature = "cache")]
    pub async fn roles(&self, guild_id: &Snowflake) -> Vec<Role> {
        self.store
            .read()
            .await
            .roles
            .iter()
            .filter(|((stored_guild_id, _), _)| stored_guild_id == guild_id)
            .map(|(_, role)| role.clone())
            .collect()
    }

    #[cfg(feature = "cache")]
    pub async fn role(&self, guild_id: &Snowflake, role_id: &Snowflake) -> Option<Role> {
        self.store
            .read()
            .await
            .roles
            .get(&(guild_id.clone(), role_id.clone()))
            .cloned()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn role(&self, _guild_id: &Snowflake, _role_id: &Snowflake) -> Option<Role> {
        None
    }

    pub async fn contains_role(&self, guild_id: &Snowflake, role_id: &Snowflake) -> bool {
        self.role(guild_id, role_id).await.is_some()
    }

    #[cfg(feature = "cache")]
    pub async fn upsert_presence(
        &self,
        guild_id: Snowflake,
        user_id: Snowflake,
        presence: Presence,
    ) {
        let mut store = self.store.write().await;
        let key = (guild_id, user_id);
        store.presences.insert(key.clone(), Arc::new(presence));
        store.presence_seen.insert(key.clone(), Instant::now());
        remember_key(&mut store.presence_order, key);
        prune_expired(&mut store, &self.config, Instant::now());
        if let Some(max) = self.config.max_presences {
            while store.presences.len() > max {
                let Some(key) = store.presence_order.pop_front() else {
                    break;
                };
                store.presences.remove(&key);
                store.presence_seen.remove(&key);
            }
        }
    }

    #[cfg(not(feature = "cache"))]
    pub async fn upsert_presence(
        &self,
        _guild_id: Snowflake,
        _user_id: Snowflake,
        _presence: Presence,
    ) {
    }

    #[cfg(feature = "cache")]
    pub async fn remove_presence(&self, guild_id: &Snowflake, user_id: &Snowflake) {
        let mut store = self.store.write().await;
        let key = (guild_id.clone(), user_id.clone());
        remove_presence_key(&mut store, &key);
    }

    #[cfg(not(feature = "cache"))]
    pub async fn remove_presence(&self, _guild_id: &Snowflake, _user_id: &Snowflake) {}

    #[cfg(feature = "cache")]
    pub async fn presence(&self, guild_id: &Snowflake, user_id: &Snowflake) -> Option<Presence> {
        self.presence_arc(guild_id, user_id)
            .await
            .map(|presence| presence.as_ref().clone())
    }

    #[cfg(feature = "cache")]
    pub async fn presence_arc(
        &self,
        guild_id: &Snowflake,
        user_id: &Snowflake,
    ) -> Option<Arc<Presence>> {
        let key = (guild_id.clone(), user_id.clone());
        let now = Instant::now();
        {
            let store = self.store.read().await;
            if !seen_expired(store.presence_seen.get(&key), self.config.presence_ttl, now) {
                return store.presences.get(&key).cloned();
            }
        }

        let mut store = self.store.write().await;
        prune_expired(&mut store, &self.config, now);
        store.presences.get(&key).cloned()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn presence_arc(
        &self,
        _guild_id: &Snowflake,
        _user_id: &Snowflake,
    ) -> Option<Arc<Presence>> {
        None
    }

    #[cfg(not(feature = "cache"))]
    pub async fn presence(&self, _guild_id: &Snowflake, _user_id: &Snowflake) -> Option<Presence> {
        None
    }

    #[cfg(feature = "cache")]
    pub async fn presences(&self, guild_id: &Snowflake) -> Vec<Presence> {
        self.presences_arc(guild_id)
            .await
            .into_iter()
            .map(|presence| presence.as_ref().clone())
            .collect()
    }

    #[cfg(feature = "cache")]
    pub async fn presences_arc(&self, guild_id: &Snowflake) -> Vec<Arc<Presence>> {
        let now = Instant::now();
        {
            let store = self.store.read().await;
            if !any_seen_expired(store.presence_seen.values(), self.config.presence_ttl, now) {
                return store
                    .presences
                    .iter()
                    .filter(|((stored_guild_id, _), _)| stored_guild_id == guild_id)
                    .map(|(_, presence)| presence.clone())
                    .collect();
            }
        }

        let mut store = self.store.write().await;
        prune_expired(&mut store, &self.config, now);
        store
            .presences
            .iter()
            .filter(|((stored_guild_id, _), _)| stored_guild_id == guild_id)
            .map(|(_, presence)| presence.clone())
            .collect()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn presences_arc(&self, _guild_id: &Snowflake) -> Vec<Arc<Presence>> {
        Vec::new()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn presences(&self, _guild_id: &Snowflake) -> Vec<Presence> {
        Vec::new()
    }

    pub async fn contains_presence(&self, guild_id: &Snowflake, user_id: &Snowflake) -> bool {
        self.presence(guild_id, user_id).await.is_some()
    }

    #[cfg(feature = "cache")]
    pub async fn upsert_voice_state(
        &self,
        guild_id: Snowflake,
        user_id: Snowflake,
        voice_state: VoiceState,
    ) {
        let mut store = self.store.write().await;
        let key = (guild_id, user_id);
        store.voice_states.insert(key.clone(), voice_state);
        remember_key(&mut store.voice_state_order, key);
        let len = store.voice_states.len();
        for key in ordered_overflow_keys(
            &mut store.voice_state_order,
            len,
            self.config.max_voice_states,
        ) {
            store.voice_states.remove(&key);
        }
    }

    #[cfg(not(feature = "cache"))]
    pub async fn upsert_voice_state(
        &self,
        _guild_id: Snowflake,
        _user_id: Snowflake,
        _voice_state: VoiceState,
    ) {
    }

    #[cfg(feature = "cache")]
    pub async fn remove_voice_state(&self, guild_id: &Snowflake, user_id: &Snowflake) {
        let mut store = self.store.write().await;
        let key = (guild_id.clone(), user_id.clone());
        store.voice_states.remove(&key);
        store
            .voice_state_order
            .retain(|stored_key| stored_key != &key);
    }

    #[cfg(not(feature = "cache"))]
    pub async fn remove_voice_state(&self, _guild_id: &Snowflake, _user_id: &Snowflake) {}

    #[cfg(feature = "cache")]
    pub async fn voice_state(
        &self,
        guild_id: &Snowflake,
        user_id: &Snowflake,
    ) -> Option<VoiceState> {
        self.store
            .read()
            .await
            .voice_states
            .get(&(guild_id.clone(), user_id.clone()))
            .cloned()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn voice_state(
        &self,
        _guild_id: &Snowflake,
        _user_id: &Snowflake,
    ) -> Option<VoiceState> {
        None
    }

    #[cfg(feature = "cache")]
    pub async fn voice_states(&self, guild_id: &Snowflake) -> Vec<VoiceState> {
        self.store
            .read()
            .await
            .voice_states
            .iter()
            .filter(|((stored_guild_id, _), _)| stored_guild_id == guild_id)
            .map(|(_, voice_state)| voice_state.clone())
            .collect()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn voice_states(&self, _guild_id: &Snowflake) -> Vec<VoiceState> {
        Vec::new()
    }

    pub async fn contains_voice_state(&self, guild_id: &Snowflake, user_id: &Snowflake) -> bool {
        self.voice_state(guild_id, user_id).await.is_some()
    }

    #[cfg(feature = "cache")]
    pub async fn upsert_soundboard_sound(&self, guild_id: Snowflake, sound: SoundboardSound) {
        let mut store = self.store.write().await;
        let key = (guild_id, sound.sound_id.clone());
        store.soundboard_sounds.insert(key.clone(), sound);
        remember_key(&mut store.soundboard_sound_order, key);
        let len = store.soundboard_sounds.len();
        for key in ordered_overflow_keys(
            &mut store.soundboard_sound_order,
            len,
            self.config.max_soundboard_sounds,
        ) {
            store.soundboard_sounds.remove(&key);
        }
    }

    #[cfg(not(feature = "cache"))]
    pub async fn upsert_soundboard_sound(&self, _guild_id: Snowflake, _sound: SoundboardSound) {}

    #[cfg(feature = "cache")]
    pub async fn replace_soundboard_sounds(
        &self,
        guild_id: Snowflake,
        sounds: Vec<SoundboardSound>,
    ) {
        let mut store = self.store.write().await;
        store
            .soundboard_sounds
            .retain(|(stored_guild_id, _), _| stored_guild_id != &guild_id);
        store
            .soundboard_sound_order
            .retain(|(stored_guild_id, _)| stored_guild_id != &guild_id);
        for sound in sounds {
            let key = (guild_id.clone(), sound.sound_id.clone());
            store.soundboard_sounds.insert(key.clone(), sound);
            remember_key(&mut store.soundboard_sound_order, key);
        }
        let len = store.soundboard_sounds.len();
        for key in ordered_overflow_keys(
            &mut store.soundboard_sound_order,
            len,
            self.config.max_soundboard_sounds,
        ) {
            store.soundboard_sounds.remove(&key);
        }
    }

    #[cfg(not(feature = "cache"))]
    pub async fn replace_soundboard_sounds(
        &self,
        _guild_id: Snowflake,
        _sounds: Vec<SoundboardSound>,
    ) {
    }

    #[cfg(feature = "cache")]
    pub async fn remove_soundboard_sound(&self, guild_id: &Snowflake, sound_id: &Snowflake) {
        let mut store = self.store.write().await;
        let key = (guild_id.clone(), sound_id.clone());
        store.soundboard_sounds.remove(&key);
        store
            .soundboard_sound_order
            .retain(|stored_key| stored_key != &key);
    }

    #[cfg(not(feature = "cache"))]
    pub async fn remove_soundboard_sound(&self, _guild_id: &Snowflake, _sound_id: &Snowflake) {}

    #[cfg(feature = "cache")]
    pub async fn soundboard_sound(
        &self,
        guild_id: &Snowflake,
        sound_id: &Snowflake,
    ) -> Option<SoundboardSound> {
        self.store
            .read()
            .await
            .soundboard_sounds
            .get(&(guild_id.clone(), sound_id.clone()))
            .cloned()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn soundboard_sound(
        &self,
        _guild_id: &Snowflake,
        _sound_id: &Snowflake,
    ) -> Option<SoundboardSound> {
        None
    }

    #[cfg(feature = "cache")]
    pub async fn soundboard_sounds(&self, guild_id: &Snowflake) -> Vec<SoundboardSound> {
        self.store
            .read()
            .await
            .soundboard_sounds
            .iter()
            .filter(|((stored_guild_id, _), _)| stored_guild_id == guild_id)
            .map(|(_, sound)| sound.clone())
            .collect()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn soundboard_sounds(&self, _guild_id: &Snowflake) -> Vec<SoundboardSound> {
        Vec::new()
    }

    pub async fn contains_soundboard_sound(
        &self,
        guild_id: &Snowflake,
        sound_id: &Snowflake,
    ) -> bool {
        self.soundboard_sound(guild_id, sound_id).await.is_some()
    }

    #[cfg(feature = "cache")]
    pub async fn replace_emojis(&self, guild_id: Snowflake, emojis: Vec<Emoji>) {
        if !self.config.cache_emojis {
            return;
        }
        let mut store = self.store.write().await;
        store
            .emojis
            .retain(|(stored_guild_id, _), _| stored_guild_id != &guild_id);
        store
            .emoji_order
            .retain(|(stored_guild_id, _)| stored_guild_id != &guild_id);
        for emoji in emojis {
            if let Some(emoji_id) = emoji.id.clone() {
                let key = (guild_id.clone(), Snowflake::from(emoji_id.as_str()));
                store.emojis.insert(key.clone(), emoji);
                remember_key(&mut store.emoji_order, key);
            }
        }
        let len = store.emojis.len();
        for key in ordered_overflow_keys(&mut store.emoji_order, len, self.config.max_emojis) {
            store.emojis.remove(&key);
        }
    }

    #[cfg(not(feature = "cache"))]
    pub async fn replace_emojis(&self, _guild_id: Snowflake, _emojis: Vec<Emoji>) {}

    #[cfg(feature = "cache")]
    pub async fn emoji(&self, guild_id: &Snowflake, emoji_id: &Snowflake) -> Option<Emoji> {
        self.store
            .read()
            .await
            .emojis
            .get(&(guild_id.clone(), emoji_id.clone()))
            .cloned()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn emoji(&self, _guild_id: &Snowflake, _emoji_id: &Snowflake) -> Option<Emoji> {
        None
    }

    #[cfg(feature = "cache")]
    pub async fn emojis(&self, guild_id: &Snowflake) -> Vec<Emoji> {
        self.store
            .read()
            .await
            .emojis
            .iter()
            .filter(|((stored_guild_id, _), _)| stored_guild_id == guild_id)
            .map(|(_, emoji)| emoji.clone())
            .collect()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn emojis(&self, _guild_id: &Snowflake) -> Vec<Emoji> {
        Vec::new()
    }

    #[cfg(feature = "cache")]
    pub async fn replace_stickers(&self, guild_id: Snowflake, stickers: Vec<Sticker>) {
        if !self.config.cache_stickers {
            return;
        }
        let mut store = self.store.write().await;
        store
            .stickers
            .retain(|(stored_guild_id, _), _| stored_guild_id != &guild_id);
        store
            .sticker_order
            .retain(|(stored_guild_id, _)| stored_guild_id != &guild_id);
        for sticker in stickers {
            let key = (guild_id.clone(), sticker.id.clone());
            store.stickers.insert(key.clone(), sticker);
            remember_key(&mut store.sticker_order, key);
        }
        let len = store.stickers.len();
        for key in ordered_overflow_keys(&mut store.sticker_order, len, self.config.max_stickers) {
            store.stickers.remove(&key);
        }
    }

    #[cfg(not(feature = "cache"))]
    pub async fn replace_stickers(&self, _guild_id: Snowflake, _stickers: Vec<Sticker>) {}

    #[cfg(feature = "cache")]
    pub async fn sticker(&self, guild_id: &Snowflake, sticker_id: &Snowflake) -> Option<Sticker> {
        self.store
            .read()
            .await
            .stickers
            .get(&(guild_id.clone(), sticker_id.clone()))
            .cloned()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn sticker(&self, _guild_id: &Snowflake, _sticker_id: &Snowflake) -> Option<Sticker> {
        None
    }

    #[cfg(feature = "cache")]
    pub async fn stickers(&self, guild_id: &Snowflake) -> Vec<Sticker> {
        self.store
            .read()
            .await
            .stickers
            .iter()
            .filter(|((stored_guild_id, _), _)| stored_guild_id == guild_id)
            .map(|(_, sticker)| sticker.clone())
            .collect()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn stickers(&self, _guild_id: &Snowflake) -> Vec<Sticker> {
        Vec::new()
    }

    #[cfg(feature = "cache")]
    pub async fn upsert_scheduled_event(&self, event: ScheduledEvent) {
        if !self.config.cache_scheduled_events {
            return;
        }
        let (Some(guild_id), Some(event_id)) = (event.guild_id.clone(), event.id.clone()) else {
            return;
        };
        let mut store = self.store.write().await;
        let key = (guild_id, event_id);
        store.scheduled_events.insert(key.clone(), event);
        remember_key(&mut store.scheduled_event_order, key);
        let len = store.scheduled_events.len();
        for key in ordered_overflow_keys(
            &mut store.scheduled_event_order,
            len,
            self.config.max_scheduled_events,
        ) {
            store.scheduled_events.remove(&key);
        }
    }

    #[cfg(not(feature = "cache"))]
    pub async fn upsert_scheduled_event(&self, _event: ScheduledEvent) {}

    #[cfg(feature = "cache")]
    pub async fn remove_scheduled_event(&self, guild_id: &Snowflake, event_id: &Snowflake) {
        let mut store = self.store.write().await;
        let key = (guild_id.clone(), event_id.clone());
        store.scheduled_events.remove(&key);
        store
            .scheduled_event_order
            .retain(|stored_key| stored_key != &key);
    }

    #[cfg(not(feature = "cache"))]
    pub async fn remove_scheduled_event(&self, _guild_id: &Snowflake, _event_id: &Snowflake) {}

    #[cfg(feature = "cache")]
    pub async fn scheduled_event(
        &self,
        guild_id: &Snowflake,
        event_id: &Snowflake,
    ) -> Option<ScheduledEvent> {
        self.store
            .read()
            .await
            .scheduled_events
            .get(&(guild_id.clone(), event_id.clone()))
            .cloned()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn scheduled_event(
        &self,
        _guild_id: &Snowflake,
        _event_id: &Snowflake,
    ) -> Option<ScheduledEvent> {
        None
    }

    #[cfg(feature = "cache")]
    pub async fn scheduled_events(&self, guild_id: &Snowflake) -> Vec<ScheduledEvent> {
        self.store
            .read()
            .await
            .scheduled_events
            .iter()
            .filter(|((stored_guild_id, _), _)| stored_guild_id == guild_id)
            .map(|(_, event)| event.clone())
            .collect()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn scheduled_events(&self, _guild_id: &Snowflake) -> Vec<ScheduledEvent> {
        Vec::new()
    }

    #[cfg(feature = "cache")]
    pub async fn upsert_stage_instance(&self, stage_instance: StageInstance) {
        if !self.config.cache_stage_instances {
            return;
        }
        let mut store = self.store.write().await;
        let key = (stage_instance.guild_id.clone(), stage_instance.id.clone());
        store.stage_instances.insert(key.clone(), stage_instance);
        remember_key(&mut store.stage_instance_order, key);
        let len = store.stage_instances.len();
        for key in ordered_overflow_keys(
            &mut store.stage_instance_order,
            len,
            self.config.max_stage_instances,
        ) {
            store.stage_instances.remove(&key);
        }
    }

    #[cfg(not(feature = "cache"))]
    pub async fn upsert_stage_instance(&self, _stage_instance: StageInstance) {}

    #[cfg(feature = "cache")]
    pub async fn remove_stage_instance(&self, guild_id: &Snowflake, stage_instance_id: &Snowflake) {
        let mut store = self.store.write().await;
        let key = (guild_id.clone(), stage_instance_id.clone());
        store.stage_instances.remove(&key);
        store
            .stage_instance_order
            .retain(|stored_key| stored_key != &key);
    }

    #[cfg(not(feature = "cache"))]
    pub async fn remove_stage_instance(
        &self,
        _guild_id: &Snowflake,
        _stage_instance_id: &Snowflake,
    ) {
    }

    #[cfg(feature = "cache")]
    pub async fn stage_instance(
        &self,
        guild_id: &Snowflake,
        stage_instance_id: &Snowflake,
    ) -> Option<StageInstance> {
        self.store
            .read()
            .await
            .stage_instances
            .get(&(guild_id.clone(), stage_instance_id.clone()))
            .cloned()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn stage_instance(
        &self,
        _guild_id: &Snowflake,
        _stage_instance_id: &Snowflake,
    ) -> Option<StageInstance> {
        None
    }

    #[cfg(feature = "cache")]
    pub async fn stage_instances(&self, guild_id: &Snowflake) -> Vec<StageInstance> {
        self.store
            .read()
            .await
            .stage_instances
            .iter()
            .filter(|((stored_guild_id, _), _)| stored_guild_id == guild_id)
            .map(|(_, stage_instance)| stage_instance.clone())
            .collect()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn stage_instances(&self, _guild_id: &Snowflake) -> Vec<StageInstance> {
        Vec::new()
    }
}

#[path = "cache/managers.rs"]
mod managers;
pub use managers::{
    ChannelManager, GuildManager, MemberManager, MessageManager, RoleManager, UserManager,
};

#[cfg(all(test, feature = "cache"))]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use crate::event::ScheduledEvent;
    #[cfg(feature = "gateway")]
    use crate::manager::CachedManager;
    use crate::model::{
        Channel, Guild, Message, Presence, Role, Snowflake, SoundboardSound, StageInstance,
        Sticker, User, VoiceState,
    };
    use crate::types::Emoji;

    use super::{CacheBackend, CacheConfig, CacheHandle};
    #[cfg(feature = "gateway")]
    use super::{
        ChannelManager, GuildManager, MemberManager, MessageManager, RoleManager, UserManager,
    };
    #[cfg(feature = "gateway")]
    use crate::http::DiscordHttpClient;

    #[tokio::test]
    async fn cache_handle_tracks_create_and_delete_flows() {
        let cache = CacheHandle::new();
        let guild_id = Snowflake::from("1");
        let other_guild_id = Snowflake::from("2");
        let channel_id = Snowflake::from("10");
        let other_channel_id = Snowflake::from("20");
        let dm_channel_id = Snowflake::from("30");
        let user_id = Snowflake::from("11");
        let other_user_id = Snowflake::from("21");
        let message_id = Snowflake::from("12");
        let orphan_channel_id = Snowflake::from("13");
        let orphan_message_id = Snowflake::from("14");
        let other_message_id = Snowflake::from("22");
        let dm_message_id = Snowflake::from("31");
        let role_id = Snowflake::from("15");
        let other_role_id = Snowflake::from("23");

        cache
            .upsert_guild(Guild {
                id: guild_id.clone(),
                name: "discordrs".to_string(),
                ..Guild::default()
            })
            .await;
        cache
            .upsert_guild(Guild {
                id: other_guild_id.clone(),
                name: "other".to_string(),
                ..Guild::default()
            })
            .await;
        cache
            .upsert_role(
                guild_id.clone(),
                Role {
                    id: role_id.clone(),
                    name: "admin".to_string(),
                    ..Role::default()
                },
            )
            .await;
        cache
            .upsert_role(
                other_guild_id.clone(),
                Role {
                    id: other_role_id.clone(),
                    name: "member".to_string(),
                    ..Role::default()
                },
            )
            .await;
        cache
            .upsert_channel(Channel {
                id: channel_id.clone(),
                guild_id: Some(guild_id.clone()),
                kind: 0,
                name: Some("general".to_string()),
                ..Channel::default()
            })
            .await;
        cache
            .upsert_channel(Channel {
                id: other_channel_id.clone(),
                guild_id: Some(other_guild_id.clone()),
                kind: 0,
                name: Some("other-general".to_string()),
                ..Channel::default()
            })
            .await;
        cache
            .upsert_channel(Channel {
                id: dm_channel_id.clone(),
                kind: 1,
                name: Some("dm".to_string()),
                ..Channel::default()
            })
            .await;
        cache
            .upsert_member(
                guild_id.clone(),
                user_id.clone(),
                crate::model::Member {
                    user: Some(User {
                        id: user_id.clone(),
                        username: "discordrs".to_string(),
                        ..User::default()
                    }),
                    ..crate::model::Member::default()
                },
            )
            .await;
        cache
            .upsert_user(User {
                id: user_id.clone(),
                username: "discordrs".to_string(),
                ..User::default()
            })
            .await;
        cache
            .upsert_member(
                other_guild_id.clone(),
                other_user_id.clone(),
                crate::model::Member {
                    user: Some(User {
                        id: other_user_id.clone(),
                        username: "other".to_string(),
                        ..User::default()
                    }),
                    ..crate::model::Member::default()
                },
            )
            .await;
        cache
            .upsert_user(User {
                id: other_user_id.clone(),
                username: "other".to_string(),
                ..User::default()
            })
            .await;
        cache
            .upsert_message(Message {
                id: message_id.clone(),
                channel_id: channel_id.clone(),
                guild_id: Some(guild_id.clone()),
                content: "hello".to_string(),
                ..Message::default()
            })
            .await;
        cache
            .upsert_message(Message {
                id: orphan_message_id.clone(),
                channel_id: orphan_channel_id.clone(),
                guild_id: Some(guild_id.clone()),
                content: "orphan".to_string(),
                ..Message::default()
            })
            .await;
        cache
            .upsert_message(Message {
                id: other_message_id.clone(),
                channel_id: other_channel_id.clone(),
                guild_id: Some(other_guild_id.clone()),
                content: "other".to_string(),
                ..Message::default()
            })
            .await;
        cache
            .upsert_message(Message {
                id: dm_message_id.clone(),
                channel_id: dm_channel_id.clone(),
                content: "dm".to_string(),
                ..Message::default()
            })
            .await;
        cache
            .upsert_presence(
                guild_id.clone(),
                user_id.clone(),
                Presence {
                    user_id: Some(user_id.clone()),
                    status: Some("online".to_string()),
                    ..Presence::default()
                },
            )
            .await;
        cache
            .upsert_presence(
                other_guild_id.clone(),
                other_user_id.clone(),
                Presence {
                    user_id: Some(other_user_id.clone()),
                    status: Some("idle".to_string()),
                    ..Presence::default()
                },
            )
            .await;
        cache
            .upsert_voice_state(
                guild_id.clone(),
                user_id.clone(),
                VoiceState {
                    guild_id: Some(guild_id.clone()),
                    channel_id: Some(channel_id.clone()),
                    user_id: Some(user_id.clone()),
                    ..VoiceState::default()
                },
            )
            .await;
        cache
            .upsert_voice_state(
                other_guild_id.clone(),
                other_user_id.clone(),
                VoiceState {
                    guild_id: Some(other_guild_id.clone()),
                    channel_id: Some(other_channel_id.clone()),
                    user_id: Some(other_user_id.clone()),
                    ..VoiceState::default()
                },
            )
            .await;

        assert!(cache.guild(&guild_id).await.is_some());
        assert!(cache.channel(&channel_id).await.is_some());
        assert!(cache.user(&user_id).await.is_some());
        assert!(cache.member(&guild_id, &user_id).await.is_some());
        assert!(cache.message(&channel_id, &message_id).await.is_some());
        assert!(cache.presence(&guild_id, &user_id).await.is_some());
        assert!(cache.voice_state(&guild_id, &user_id).await.is_some());
        assert!(cache
            .message(&orphan_channel_id, &orphan_message_id)
            .await
            .is_some());
        assert_eq!(cache.roles(&guild_id).await.len(), 1);

        cache.remove_guild(&guild_id).await;
        assert!(cache.guild(&guild_id).await.is_none());
        assert!(cache.channel(&channel_id).await.is_none());
        assert!(cache.user(&user_id).await.is_some());
        assert!(cache.member(&guild_id, &user_id).await.is_none());
        assert!(cache.message(&channel_id, &message_id).await.is_none());
        assert!(cache.presence(&guild_id, &user_id).await.is_none());
        assert!(cache.voice_state(&guild_id, &user_id).await.is_none());
        assert!(cache
            .message(&orphan_channel_id, &orphan_message_id)
            .await
            .is_none());
        assert!(cache.roles(&guild_id).await.is_empty());
        assert!(cache.guild(&other_guild_id).await.is_some());
        assert!(cache.channel(&other_channel_id).await.is_some());
        assert!(cache.channel(&dm_channel_id).await.is_some());
        assert!(cache.user(&other_user_id).await.is_some());
        assert!(cache
            .member(&other_guild_id, &other_user_id)
            .await
            .is_some());
        assert!(cache
            .message(&other_channel_id, &other_message_id)
            .await
            .is_some());
        assert!(cache
            .message(&dm_channel_id, &dm_message_id)
            .await
            .is_some());
        assert!(cache.role(&other_guild_id, &other_role_id).await.is_some());
        assert!(cache
            .presence(&other_guild_id, &other_user_id)
            .await
            .is_some());
        assert!(cache
            .voice_state(&other_guild_id, &other_user_id)
            .await
            .is_some());
    }

    #[tokio::test]
    async fn cache_handle_exposes_contains_and_list_helpers() {
        let cache = CacheHandle::new();
        let guild_id = Snowflake::from("1");
        let channel_id = Snowflake::from("2");
        let message_id = Snowflake::from("3");
        let user_id = Snowflake::from("4");
        let role_id = Snowflake::from("5");

        cache
            .upsert_guild(Guild {
                id: guild_id.clone(),
                name: "discordrs".to_string(),
                ..Guild::default()
            })
            .await;
        cache
            .upsert_channel(Channel {
                id: channel_id.clone(),
                guild_id: Some(guild_id.clone()),
                kind: 0,
                name: Some("general".to_string()),
                ..Channel::default()
            })
            .await;
        cache
            .upsert_member(
                guild_id.clone(),
                user_id.clone(),
                crate::model::Member {
                    user: Some(User {
                        id: user_id.clone(),
                        username: "discordrs".to_string(),
                        ..User::default()
                    }),
                    ..crate::model::Member::default()
                },
            )
            .await;
        cache
            .upsert_user(User {
                id: user_id.clone(),
                username: "discordrs".to_string(),
                ..User::default()
            })
            .await;
        cache
            .upsert_message(Message {
                id: message_id.clone(),
                channel_id: channel_id.clone(),
                guild_id: Some(guild_id.clone()),
                content: "hello".to_string(),
                ..Message::default()
            })
            .await;
        cache
            .upsert_role(
                guild_id.clone(),
                Role {
                    id: role_id.clone(),
                    name: "admin".to_string(),
                    ..Role::default()
                },
            )
            .await;
        cache
            .upsert_presence(
                guild_id.clone(),
                user_id.clone(),
                Presence {
                    user_id: Some(user_id.clone()),
                    status: Some("online".to_string()),
                    ..Presence::default()
                },
            )
            .await;
        cache
            .upsert_voice_state(
                guild_id.clone(),
                user_id.clone(),
                VoiceState {
                    guild_id: Some(guild_id.clone()),
                    channel_id: Some(channel_id.clone()),
                    user_id: Some(user_id.clone()),
                    ..VoiceState::default()
                },
            )
            .await;

        assert!(cache.contains_guild(&guild_id).await);
        assert!(cache.contains_channel(&channel_id).await);
        assert!(cache.contains_user(&user_id).await);
        assert!(cache.contains_member(&guild_id, &user_id).await);
        assert!(cache.contains_message(&channel_id, &message_id).await);
        assert!(cache.contains_role(&guild_id, &role_id).await);
        assert!(cache.contains_presence(&guild_id, &user_id).await);
        assert!(cache.contains_voice_state(&guild_id, &user_id).await);
        assert_eq!(cache.guilds().await.len(), 1);
        assert_eq!(cache.channels().await.len(), 1);
        assert_eq!(cache.users().await.len(), 1);
        assert_eq!(cache.members(&guild_id).await.len(), 1);
        assert_eq!(cache.messages(&channel_id).await.len(), 1);
        assert_eq!(cache.roles(&guild_id).await.len(), 1);
        assert_eq!(cache.presences(&guild_id).await.len(), 1);
        assert_eq!(cache.voice_states(&guild_id).await.len(), 1);

        cache.clear().await;
        assert!(cache.guilds().await.is_empty());
        assert!(cache.channels().await.is_empty());
        assert!(cache.users().await.is_empty());
        assert!(cache.presences(&guild_id).await.is_empty());
        assert!(cache.voice_states(&guild_id).await.is_empty());
    }

    #[tokio::test]
    async fn cache_config_enforces_message_presence_and_member_size_limits() {
        let cache = CacheHandle::with_config(
            CacheConfig::unbounded()
                .max_messages_per_channel(2)
                .max_total_messages(3)
                .max_presences(2)
                .max_members_per_guild(2),
        );
        assert_eq!(cache.config().max_total_messages, Some(3));
        let guild_id = Snowflake::from("1");
        let channel_id = Snowflake::from("10");
        let other_channel_id = Snowflake::from("20");

        for id in ["100", "101", "102"] {
            cache
                .upsert_message(Message {
                    id: Snowflake::from(id),
                    channel_id: channel_id.clone(),
                    guild_id: Some(guild_id.clone()),
                    content: id.to_string(),
                    ..Message::default()
                })
                .await;
        }
        assert!(cache
            .message(&channel_id, &Snowflake::from("100"))
            .await
            .is_none());
        assert_eq!(cache.messages(&channel_id).await.len(), 2);

        for id in ["200", "201"] {
            cache
                .upsert_message(Message {
                    id: Snowflake::from(id),
                    channel_id: other_channel_id.clone(),
                    guild_id: Some(guild_id.clone()),
                    content: id.to_string(),
                    ..Message::default()
                })
                .await;
        }
        let total_messages =
            cache.messages(&channel_id).await.len() + cache.messages(&other_channel_id).await.len();
        assert_eq!(total_messages, 3);

        for id in ["300", "301", "302"] {
            let user_id = Snowflake::from(id);
            cache
                .upsert_presence(
                    guild_id.clone(),
                    user_id.clone(),
                    Presence {
                        user_id: Some(user_id),
                        status: Some("online".to_string()),
                        ..Presence::default()
                    },
                )
                .await;
        }
        assert_eq!(cache.presences(&guild_id).await.len(), 2);
        assert!(cache
            .presence(&guild_id, &Snowflake::from("300"))
            .await
            .is_none());

        for id in ["400", "401", "402"] {
            let user_id = Snowflake::from(id);
            cache
                .upsert_member(
                    guild_id.clone(),
                    user_id.clone(),
                    crate::model::Member {
                        user: Some(User {
                            id: user_id,
                            username: id.to_string(),
                            ..User::default()
                        }),
                        ..crate::model::Member::default()
                    },
                )
                .await;
        }
        assert_eq!(cache.members(&guild_id).await.len(), 2);
        assert!(cache
            .member(&guild_id, &Snowflake::from("400"))
            .await
            .is_none());
    }

    #[test]
    fn cache_config_default_is_bounded_unbounded_is_explicit() {
        let bounded = CacheConfig::default();
        assert_eq!(bounded.max_messages_per_channel, Some(100));
        assert_eq!(bounded.max_total_messages, Some(10_000));
        assert_eq!(bounded.max_presences, Some(50_000));
        assert_eq!(bounded.max_members_per_guild, Some(25_000));
        assert_eq!(bounded.max_users, Some(100_000));
        assert_eq!(bounded.message_ttl, Some(Duration::from_secs(60 * 60)));
        assert_eq!(bounded.presence_ttl, Some(Duration::from_secs(10 * 60)));
        assert_eq!(bounded.member_ttl, Some(Duration::from_secs(24 * 60 * 60)));
        assert!(bounded.cache_emojis);
        assert!(bounded.cache_stickers);
        assert!(bounded.cache_scheduled_events);
        assert!(bounded.cache_stage_instances);

        let unbounded = CacheConfig::unbounded();
        assert_eq!(unbounded.max_messages_per_channel, None);
        assert_eq!(unbounded.max_total_messages, None);
        assert_eq!(unbounded.max_presences, None);
        assert_eq!(unbounded.max_members_per_guild, None);
        assert_eq!(unbounded.max_users, None);
        assert_eq!(unbounded.message_ttl, None);
        assert_eq!(unbounded.presence_ttl, None);
        assert_eq!(unbounded.member_ttl, None);

        let disabled_metadata = CacheConfig::default()
            .cache_emojis(false)
            .cache_stickers(false)
            .cache_scheduled_events(false)
            .cache_stage_instances(false);
        assert!(!disabled_metadata.cache_emojis);
        assert!(!disabled_metadata.cache_stickers);
        assert!(!disabled_metadata.cache_scheduled_events);
        assert!(!disabled_metadata.cache_stage_instances);
    }

    #[tokio::test]
    async fn cache_config_ttl_expires_message_presence_and_member_entries() {
        let cache = CacheHandle::with_config(
            CacheConfig::unbounded()
                .message_ttl(Duration::ZERO)
                .presence_ttl(Duration::ZERO)
                .member_ttl(Duration::ZERO),
        );
        let guild_id = Snowflake::from("1");
        let channel_id = Snowflake::from("10");
        let user_id = Snowflake::from("20");
        let message_id = Snowflake::from("30");

        cache
            .upsert_message(Message {
                id: message_id.clone(),
                channel_id: channel_id.clone(),
                guild_id: Some(guild_id.clone()),
                content: "expired".to_string(),
                ..Message::default()
            })
            .await;
        cache
            .upsert_presence(
                guild_id.clone(),
                user_id.clone(),
                Presence {
                    user_id: Some(user_id.clone()),
                    status: Some("online".to_string()),
                    ..Presence::default()
                },
            )
            .await;
        cache
            .upsert_member(
                guild_id.clone(),
                user_id.clone(),
                crate::model::Member {
                    user: Some(User {
                        id: user_id.clone(),
                        username: "expired".to_string(),
                        ..User::default()
                    }),
                    ..crate::model::Member::default()
                },
            )
            .await;

        cache.purge_expired().await;

        assert!(cache.message(&channel_id, &message_id).await.is_none());
        assert!(cache.presence(&guild_id, &user_id).await.is_none());
        assert!(cache.member(&guild_id, &user_id).await.is_none());
        assert!(cache.messages(&channel_id).await.is_empty());
        assert!(cache.presences(&guild_id).await.is_empty());
        assert!(cache.members(&guild_id).await.is_empty());
    }

    #[tokio::test]
    async fn cache_read_paths_return_fresh_message_presence_and_member_entries() {
        let cache = CacheHandle::with_config(
            CacheConfig::unbounded()
                .message_ttl(Duration::from_secs(60))
                .presence_ttl(Duration::from_secs(60))
                .member_ttl(Duration::from_secs(60)),
        );
        let guild_id = Snowflake::from("1");
        let channel_id = Snowflake::from("10");
        let user_id = Snowflake::from("20");
        let message_id = Snowflake::from("30");

        cache
            .upsert_message(Message {
                id: message_id.clone(),
                channel_id: channel_id.clone(),
                guild_id: Some(guild_id.clone()),
                content: "fresh".to_string(),
                ..Message::default()
            })
            .await;
        cache
            .upsert_presence(
                guild_id.clone(),
                user_id.clone(),
                Presence {
                    user_id: Some(user_id.clone()),
                    status: Some("online".to_string()),
                    ..Presence::default()
                },
            )
            .await;
        cache
            .upsert_member(
                guild_id.clone(),
                user_id.clone(),
                crate::model::Member {
                    user: Some(User {
                        id: user_id.clone(),
                        username: "fresh".to_string(),
                        ..User::default()
                    }),
                    ..crate::model::Member::default()
                },
            )
            .await;

        assert!(cache.message(&channel_id, &message_id).await.is_some());
        assert_eq!(cache.messages(&channel_id).await.len(), 1);
        assert!(cache.presence(&guild_id, &user_id).await.is_some());
        assert_eq!(cache.presences(&guild_id).await.len(), 1);
        assert!(cache.member(&guild_id, &user_id).await.is_some());
        assert_eq!(cache.members(&guild_id).await.len(), 1);
    }

    #[tokio::test]
    async fn cache_arc_read_paths_share_hot_cached_entries_without_breaking_owned_reads() {
        let cache = CacheHandle::with_config(
            CacheConfig::unbounded()
                .message_ttl(Duration::from_secs(60))
                .presence_ttl(Duration::from_secs(60))
                .member_ttl(Duration::from_secs(60)),
        );
        let guild_id = Snowflake::from("1");
        let channel_id = Snowflake::from("10");
        let user_id = Snowflake::from("20");
        let message_id = Snowflake::from("30");

        cache
            .upsert_message(Message {
                id: message_id.clone(),
                channel_id: channel_id.clone(),
                content: "arc message".to_string(),
                ..Message::default()
            })
            .await;
        cache
            .upsert_presence(
                guild_id.clone(),
                user_id.clone(),
                Presence {
                    user_id: Some(user_id.clone()),
                    status: Some("online".to_string()),
                    ..Presence::default()
                },
            )
            .await;
        cache
            .upsert_member(
                guild_id.clone(),
                user_id.clone(),
                crate::model::Member {
                    user: Some(User {
                        id: user_id.clone(),
                        username: "arc member".to_string(),
                        ..User::default()
                    }),
                    ..crate::model::Member::default()
                },
            )
            .await;

        let message = cache.message_arc(&channel_id, &message_id).await.unwrap();
        let same_message = cache.message_arc(&channel_id, &message_id).await.unwrap();
        assert!(Arc::ptr_eq(&message, &same_message));
        assert_eq!(
            cache.messages_arc(&channel_id).await[0].content,
            "arc message"
        );
        assert_eq!(
            cache
                .message(&channel_id, &message_id)
                .await
                .unwrap()
                .content,
            "arc message"
        );

        let presence = cache.presence_arc(&guild_id, &user_id).await.unwrap();
        let same_presence = cache.presence_arc(&guild_id, &user_id).await.unwrap();
        assert!(Arc::ptr_eq(&presence, &same_presence));
        assert_eq!(
            cache.presences_arc(&guild_id).await[0].status.as_deref(),
            Some("online")
        );
        assert_eq!(
            cache
                .presence(&guild_id, &user_id)
                .await
                .unwrap()
                .status
                .as_deref(),
            Some("online")
        );

        let member = cache.member_arc(&guild_id, &user_id).await.unwrap();
        let same_member = cache.member_arc(&guild_id, &user_id).await.unwrap();
        assert!(Arc::ptr_eq(&member, &same_member));
        assert_eq!(
            cache.members_arc(&guild_id).await[0]
                .user
                .as_ref()
                .unwrap()
                .username,
            "arc member"
        );
        assert_eq!(
            cache
                .member(&guild_id, &user_id)
                .await
                .unwrap()
                .user
                .unwrap()
                .username,
            "arc member"
        );
    }

    #[tokio::test]
    async fn cache_backend_trait_delegates_hot_paths_to_cache_handle() {
        async fn exercise_backend<B: CacheBackend + ?Sized>(
            backend: &B,
            guild_id: Snowflake,
            channel_id: Snowflake,
            user_id: Snowflake,
            message_id: Snowflake,
        ) {
            backend
                .put_member(
                    guild_id.clone(),
                    user_id.clone(),
                    crate::model::Member {
                        user: Some(User {
                            id: user_id.clone(),
                            username: "backend member".to_string(),
                            ..User::default()
                        }),
                        ..crate::model::Member::default()
                    },
                )
                .await
                .unwrap();
            backend
                .put_message(Message {
                    id: message_id.clone(),
                    channel_id: channel_id.clone(),
                    content: "backend message".to_string(),
                    ..Message::default()
                })
                .await
                .unwrap();
            backend
                .put_presence(
                    guild_id.clone(),
                    user_id.clone(),
                    Presence {
                        user_id: Some(user_id.clone()),
                        status: Some("idle".to_string()),
                        ..Presence::default()
                    },
                )
                .await
                .unwrap();

            assert_eq!(
                backend
                    .get_member(&guild_id, &user_id)
                    .await
                    .unwrap()
                    .unwrap()
                    .user
                    .as_ref()
                    .unwrap()
                    .username,
                "backend member"
            );
            assert_eq!(
                backend
                    .get_message(&channel_id, &message_id)
                    .await
                    .unwrap()
                    .unwrap()
                    .content,
                "backend message"
            );
            assert_eq!(
                backend
                    .get_presence(&guild_id, &user_id)
                    .await
                    .unwrap()
                    .unwrap()
                    .status
                    .as_deref(),
                Some("idle")
            );
            assert_eq!(backend.list_members(&guild_id).await.unwrap().len(), 1);
            assert_eq!(backend.list_messages(&channel_id).await.unwrap().len(), 1);
            assert_eq!(backend.list_presences(&guild_id).await.unwrap().len(), 1);

            backend
                .delete_message(&channel_id, &message_id)
                .await
                .unwrap();
            backend.delete_presence(&guild_id, &user_id).await.unwrap();
            backend.delete_member(&guild_id, &user_id).await.unwrap();
            assert!(backend
                .get_message(&channel_id, &message_id)
                .await
                .unwrap()
                .is_none());
            assert!(backend
                .get_presence(&guild_id, &user_id)
                .await
                .unwrap()
                .is_none());
            assert!(backend
                .get_member(&guild_id, &user_id)
                .await
                .unwrap()
                .is_none());
        }

        let cache = CacheHandle::new();
        exercise_backend(
            &cache,
            Snowflake::from("1"),
            Snowflake::from("10"),
            Snowflake::from("20"),
            Snowflake::from("30"),
        )
        .await;

        CacheBackend::put_message(
            &cache,
            Message {
                id: Snowflake::from("31"),
                channel_id: Snowflake::from("10"),
                content: "clear me".to_string(),
                ..Message::default()
            },
        )
        .await
        .unwrap();
        cache.purge_expired_entries().await.unwrap();
        cache.clear_cache().await.unwrap();
        assert!(cache
            .message_arc(&Snowflake::from("10"), &Snowflake::from("31"))
            .await
            .is_none());
    }

    #[tokio::test]
    async fn cache_read_paths_lazily_prune_expired_entries() {
        let seed_cache = |suffix: &'static str| async move {
            let cache = CacheHandle::with_config(
                CacheConfig::unbounded()
                    .message_ttl(Duration::from_millis(1))
                    .presence_ttl(Duration::from_millis(1))
                    .member_ttl(Duration::from_millis(1)),
            );
            let guild_id = Snowflake::from(format!("1{suffix}"));
            let channel_id = Snowflake::from(format!("10{suffix}"));
            let user_id = Snowflake::from(format!("20{suffix}"));
            let message_id = Snowflake::from(format!("30{suffix}"));

            cache
                .upsert_message(Message {
                    id: message_id.clone(),
                    channel_id: channel_id.clone(),
                    guild_id: Some(guild_id.clone()),
                    content: "soon-expired".to_string(),
                    ..Message::default()
                })
                .await;
            cache
                .upsert_presence(
                    guild_id.clone(),
                    user_id.clone(),
                    Presence {
                        user_id: Some(user_id.clone()),
                        status: Some("online".to_string()),
                        ..Presence::default()
                    },
                )
                .await;
            cache
                .upsert_member(
                    guild_id.clone(),
                    user_id.clone(),
                    crate::model::Member {
                        user: Some(User {
                            id: user_id.clone(),
                            username: "soon-expired".to_string(),
                            ..User::default()
                        }),
                        ..crate::model::Member::default()
                    },
                )
                .await;

            (cache, guild_id, channel_id, user_id, message_id)
        };

        let (message_cache, _, channel_id, _, message_id) = seed_cache("1").await;
        let (presence_cache, presence_guild_id, _, presence_user_id, _) = seed_cache("2").await;
        let (member_cache, member_guild_id, _, member_user_id, _) = seed_cache("3").await;
        let (messages_cache, _, messages_channel_id, _, _) = seed_cache("4").await;
        let (presences_cache, presences_guild_id, _, _, _) = seed_cache("5").await;
        let (members_cache, members_guild_id, _, _, _) = seed_cache("6").await;
        tokio::time::sleep(Duration::from_millis(20)).await;

        assert!(message_cache
            .message(&channel_id, &message_id)
            .await
            .is_none());
        assert!(presence_cache
            .presence(&presence_guild_id, &presence_user_id)
            .await
            .is_none());
        assert!(member_cache
            .member(&member_guild_id, &member_user_id)
            .await
            .is_none());
        assert!(messages_cache
            .messages(&messages_channel_id)
            .await
            .is_empty());
        assert!(presences_cache
            .presences(&presences_guild_id)
            .await
            .is_empty());
        assert!(members_cache.members(&members_guild_id).await.is_empty());
    }

    #[tokio::test]
    async fn cache_config_enforces_core_and_metadata_size_limits() {
        let cache = CacheHandle::with_config(
            CacheConfig::unbounded()
                .max_guilds(1)
                .max_channels(1)
                .max_users(1)
                .max_roles(1)
                .max_voice_states(1)
                .max_soundboard_sounds(1)
                .max_emojis(1)
                .max_stickers(1)
                .max_scheduled_events(1)
                .max_stage_instances(1),
        );
        let guild_id = Snowflake::from("1");

        for id in ["1", "2"] {
            cache
                .upsert_guild(Guild {
                    id: Snowflake::from(id),
                    name: format!("guild-{id}"),
                    ..Guild::default()
                })
                .await;
        }
        assert!(cache.guild(&Snowflake::from("1")).await.is_none());
        assert!(cache.guild(&Snowflake::from("2")).await.is_some());

        for id in ["10", "11"] {
            cache
                .upsert_channel(Channel {
                    id: Snowflake::from(id),
                    guild_id: Some(guild_id.clone()),
                    kind: 0,
                    ..Channel::default()
                })
                .await;
        }
        assert!(cache.channel(&Snowflake::from("10")).await.is_none());
        assert!(cache.channel(&Snowflake::from("11")).await.is_some());

        for id in ["20", "21"] {
            cache
                .upsert_user(User {
                    id: Snowflake::from(id),
                    username: format!("user-{id}"),
                    ..User::default()
                })
                .await;
        }
        assert!(cache.user(&Snowflake::from("20")).await.is_none());
        assert!(cache.user(&Snowflake::from("21")).await.is_some());

        for id in ["30", "31"] {
            cache
                .upsert_role(
                    guild_id.clone(),
                    Role {
                        id: Snowflake::from(id),
                        name: format!("role-{id}"),
                        ..Role::default()
                    },
                )
                .await;
        }
        assert!(cache
            .role(&guild_id, &Snowflake::from("30"))
            .await
            .is_none());
        assert!(cache
            .role(&guild_id, &Snowflake::from("31"))
            .await
            .is_some());

        for id in ["40", "41"] {
            cache
                .upsert_voice_state(
                    guild_id.clone(),
                    Snowflake::from(id),
                    VoiceState {
                        user_id: Some(Snowflake::from(id)),
                        guild_id: Some(guild_id.clone()),
                        ..VoiceState::default()
                    },
                )
                .await;
        }
        assert!(cache
            .voice_state(&guild_id, &Snowflake::from("40"))
            .await
            .is_none());
        assert!(cache
            .voice_state(&guild_id, &Snowflake::from("41"))
            .await
            .is_some());

        for id in ["50", "51"] {
            cache
                .upsert_soundboard_sound(
                    guild_id.clone(),
                    SoundboardSound {
                        name: format!("sound-{id}"),
                        sound_id: Snowflake::from(id),
                        volume: 1.0,
                        ..SoundboardSound::default()
                    },
                )
                .await;
        }
        assert!(cache
            .soundboard_sound(&guild_id, &Snowflake::from("50"))
            .await
            .is_none());
        assert!(cache
            .soundboard_sound(&guild_id, &Snowflake::from("51"))
            .await
            .is_some());
        assert_eq!(cache.soundboard_sounds(&guild_id).await.len(), 1);
        assert!(
            cache
                .contains_soundboard_sound(&guild_id, &Snowflake::from("51"))
                .await
        );

        cache
            .replace_emojis(
                guild_id.clone(),
                vec![
                    Emoji::custom("first", "60", false),
                    Emoji::custom("second", "61", false),
                ],
            )
            .await;
        assert!(cache
            .emoji(&guild_id, &Snowflake::from("60"))
            .await
            .is_none());
        assert!(cache
            .emoji(&guild_id, &Snowflake::from("61"))
            .await
            .is_some());
        assert_eq!(cache.emojis(&guild_id).await.len(), 1);

        cache
            .replace_stickers(
                guild_id.clone(),
                vec![
                    Sticker {
                        id: Snowflake::from("70"),
                        name: "first".to_string(),
                        ..Sticker::default()
                    },
                    Sticker {
                        id: Snowflake::from("71"),
                        name: "second".to_string(),
                        ..Sticker::default()
                    },
                ],
            )
            .await;
        assert!(cache
            .sticker(&guild_id, &Snowflake::from("70"))
            .await
            .is_none());
        assert!(cache
            .sticker(&guild_id, &Snowflake::from("71"))
            .await
            .is_some());
        assert_eq!(cache.stickers(&guild_id).await.len(), 1);

        for id in ["80", "81"] {
            cache
                .upsert_scheduled_event(ScheduledEvent {
                    id: Some(Snowflake::from(id)),
                    guild_id: Some(guild_id.clone()),
                    name: Some(format!("event-{id}")),
                    ..ScheduledEvent::default()
                })
                .await;
        }
        assert!(cache
            .scheduled_event(&guild_id, &Snowflake::from("80"))
            .await
            .is_none());
        assert!(cache
            .scheduled_event(&guild_id, &Snowflake::from("81"))
            .await
            .is_some());
        assert_eq!(cache.scheduled_events(&guild_id).await.len(), 1);

        for id in ["90", "91"] {
            cache
                .upsert_stage_instance(StageInstance {
                    id: Snowflake::from(id),
                    guild_id: guild_id.clone(),
                    channel_id: Snowflake::from("11"),
                    topic: format!("stage-{id}"),
                    privacy_level: 2,
                    ..StageInstance::default()
                })
                .await;
        }
        assert!(cache
            .stage_instance(&guild_id, &Snowflake::from("90"))
            .await
            .is_none());
        assert!(cache
            .stage_instance(&guild_id, &Snowflake::from("91"))
            .await
            .is_some());
        assert_eq!(cache.stage_instances(&guild_id).await.len(), 1);
    }

    #[tokio::test]
    async fn cache_config_metadata_toggles_skip_optional_stores() {
        let cache = CacheHandle::with_config(
            CacheConfig::unbounded()
                .cache_emojis(false)
                .cache_stickers(false)
                .cache_scheduled_events(false)
                .cache_stage_instances(false),
        );
        let guild_id = Snowflake::from("1");

        cache
            .replace_emojis(guild_id.clone(), vec![Emoji::custom("one", "10", false)])
            .await;
        cache
            .replace_stickers(
                guild_id.clone(),
                vec![Sticker {
                    id: Snowflake::from("20"),
                    name: "sticker".to_string(),
                    ..Sticker::default()
                }],
            )
            .await;
        cache
            .upsert_scheduled_event(ScheduledEvent {
                id: Some(Snowflake::from("30")),
                guild_id: Some(guild_id.clone()),
                name: Some("event".to_string()),
                ..ScheduledEvent::default()
            })
            .await;
        cache
            .upsert_stage_instance(StageInstance {
                id: Snowflake::from("40"),
                guild_id: guild_id.clone(),
                channel_id: Snowflake::from("50"),
                topic: "stage".to_string(),
                privacy_level: 2,
                ..StageInstance::default()
            })
            .await;

        assert!(cache.emojis(&guild_id).await.is_empty());
        assert!(cache.stickers(&guild_id).await.is_empty());
        assert!(cache.scheduled_events(&guild_id).await.is_empty());
        assert!(cache.stage_instances(&guild_id).await.is_empty());
    }

    #[tokio::test]
    async fn remove_channel_cascades_messages_and_bulk_delete_only_targets_selected_ids() {
        let cache = CacheHandle::new();
        let channel_id = Snowflake::from("2");
        let other_channel_id = Snowflake::from("3");
        let first_message_id = Snowflake::from("10");
        let second_message_id = Snowflake::from("11");
        let untouched_message_id = Snowflake::from("12");

        cache
            .upsert_channel(Channel {
                id: channel_id.clone(),
                kind: 0,
                name: Some("general".to_string()),
                ..Channel::default()
            })
            .await;
        cache
            .upsert_channel(Channel {
                id: other_channel_id.clone(),
                kind: 0,
                name: Some("random".to_string()),
                ..Channel::default()
            })
            .await;
        for (message_id, stored_channel_id) in [
            (first_message_id.clone(), channel_id.clone()),
            (second_message_id.clone(), channel_id.clone()),
            (untouched_message_id.clone(), other_channel_id.clone()),
        ] {
            cache
                .upsert_message(Message {
                    id: message_id,
                    channel_id: stored_channel_id,
                    content: "hello".to_string(),
                    ..Message::default()
                })
                .await;
        }

        cache
            .remove_messages_bulk(&channel_id, std::slice::from_ref(&first_message_id))
            .await;
        assert!(cache
            .message(&channel_id, &first_message_id)
            .await
            .is_none());
        assert!(cache
            .message(&channel_id, &second_message_id)
            .await
            .is_some());
        assert!(cache
            .message(&other_channel_id, &untouched_message_id)
            .await
            .is_some());

        cache.remove_channel(&channel_id).await;
        assert!(cache.channel(&channel_id).await.is_none());
        assert!(cache
            .message(&channel_id, &second_message_id)
            .await
            .is_none());
        assert!(cache
            .message(&other_channel_id, &untouched_message_id)
            .await
            .is_some());
    }

    #[tokio::test]
    async fn cache_handle_removes_individual_entries_without_touching_other_guild_data() {
        let cache = CacheHandle::new();
        let guild_id = Snowflake::from("1");
        let other_guild_id = Snowflake::from("2");
        let channel_id = Snowflake::from("10");
        let other_channel_id = Snowflake::from("20");
        let user_id = Snowflake::from("11");
        let other_user_id = Snowflake::from("21");
        let message_id = Snowflake::from("12");
        let other_message_id = Snowflake::from("22");
        let role_id = Snowflake::from("13");
        let other_role_id = Snowflake::from("23");

        for (id, name) in [
            (guild_id.clone(), "discordrs"),
            (other_guild_id.clone(), "other"),
        ] {
            cache
                .upsert_guild(Guild {
                    id,
                    name: name.to_string(),
                    ..Guild::default()
                })
                .await;
        }

        for (id, guild, name) in [
            (channel_id.clone(), Some(guild_id.clone()), "general"),
            (
                other_channel_id.clone(),
                Some(other_guild_id.clone()),
                "other-general",
            ),
        ] {
            cache
                .upsert_channel(Channel {
                    id,
                    guild_id: guild,
                    kind: 0,
                    name: Some(name.to_string()),
                    ..Channel::default()
                })
                .await;
        }

        for (guild, user, username) in [
            (guild_id.clone(), user_id.clone(), "discordrs"),
            (other_guild_id.clone(), other_user_id.clone(), "other"),
        ] {
            cache
                .upsert_member(
                    guild,
                    user.clone(),
                    crate::model::Member {
                        user: Some(User {
                            id: user,
                            username: username.to_string(),
                            ..User::default()
                        }),
                        ..crate::model::Member::default()
                    },
                )
                .await;
        }

        for (message_id, channel_id, guild_id, content) in [
            (
                message_id.clone(),
                channel_id.clone(),
                Some(guild_id.clone()),
                "hello",
            ),
            (
                other_message_id.clone(),
                other_channel_id.clone(),
                Some(other_guild_id.clone()),
                "other",
            ),
        ] {
            cache
                .upsert_message(Message {
                    id: message_id,
                    channel_id,
                    guild_id,
                    content: content.to_string(),
                    ..Message::default()
                })
                .await;
        }

        for (guild_id, role_id, name) in [
            (guild_id.clone(), role_id.clone(), "admin"),
            (other_guild_id.clone(), other_role_id.clone(), "member"),
        ] {
            cache
                .upsert_role(
                    guild_id,
                    Role {
                        id: role_id,
                        name: name.to_string(),
                        ..Role::default()
                    },
                )
                .await;
        }

        assert_eq!(cache.members(&guild_id).await.len(), 1);
        assert_eq!(cache.messages(&channel_id).await.len(), 1);
        assert_eq!(cache.roles(&guild_id).await.len(), 1);

        cache.remove_member(&guild_id, &user_id).await;
        cache.remove_message(&channel_id, &message_id).await;
        cache.remove_role(&guild_id, &role_id).await;

        assert!(cache.member(&guild_id, &user_id).await.is_none());
        assert!(cache.message(&channel_id, &message_id).await.is_none());
        assert!(cache.role(&guild_id, &role_id).await.is_none());
        assert!(!cache.contains_member(&guild_id, &user_id).await);
        assert!(!cache.contains_message(&channel_id, &message_id).await);
        assert!(!cache.contains_role(&guild_id, &role_id).await);
        assert!(cache.members(&guild_id).await.is_empty());
        assert!(cache.messages(&channel_id).await.is_empty());
        assert!(cache.roles(&guild_id).await.is_empty());

        assert!(cache
            .member(&other_guild_id, &other_user_id)
            .await
            .is_some());
        assert!(cache
            .message(&other_channel_id, &other_message_id)
            .await
            .is_some());
        assert!(cache.role(&other_guild_id, &other_role_id).await.is_some());
    }

    #[cfg(feature = "gateway")]
    #[tokio::test]
    async fn managers_return_cached_values_without_hitting_http() {
        let cache = CacheHandle::new();
        let http = Arc::new(DiscordHttpClient::new("token", 1));
        let guild_id = Snowflake::from("100");
        let channel_id = Snowflake::from("200");
        let user_id = Snowflake::from("300");
        let message_id = Snowflake::from("400");
        let role_id = Snowflake::from("500");

        let guild = Guild {
            id: guild_id.clone(),
            name: "discordrs".to_string(),
            ..Guild::default()
        };
        let channel = Channel {
            id: channel_id.clone(),
            guild_id: Some(guild_id.clone()),
            kind: 0,
            name: Some("general".to_string()),
            ..Channel::default()
        };
        let member = crate::model::Member {
            user: Some(User {
                id: user_id.clone(),
                username: "discordrs".to_string(),
                ..User::default()
            }),
            ..crate::model::Member::default()
        };
        let user = User {
            id: user_id.clone(),
            username: "discordrs".to_string(),
            ..User::default()
        };
        let message = Message {
            id: message_id.clone(),
            channel_id: channel_id.clone(),
            guild_id: Some(guild_id.clone()),
            content: "cached".to_string(),
            ..Message::default()
        };
        let role = Role {
            id: role_id.clone(),
            name: "admin".to_string(),
            ..Role::default()
        };

        cache.upsert_guild(guild.clone()).await;
        cache.upsert_channel(channel.clone()).await;
        cache.upsert_user(user.clone()).await;
        cache
            .upsert_member(guild_id.clone(), user_id.clone(), member.clone())
            .await;
        cache.upsert_message(message.clone()).await;
        cache.upsert_role(guild_id.clone(), role.clone()).await;

        let guild_manager = GuildManager::new(Arc::clone(&http), cache.clone());
        let channel_manager = ChannelManager::new(Arc::clone(&http), cache.clone());
        let user_manager = UserManager::new(Arc::clone(&http), cache.clone());
        let member_manager = MemberManager::new(Arc::clone(&http), cache.clone());
        let message_manager = MessageManager::new(Arc::clone(&http), cache.clone());
        let role_manager = RoleManager::new(http, cache.clone());

        assert_eq!(
            guild_manager.get(guild_id.clone()).await.unwrap().name,
            "discordrs"
        );
        assert_eq!(
            channel_manager
                .get(channel_id.clone())
                .await
                .unwrap()
                .name
                .as_deref(),
            Some("general")
        );
        assert_eq!(
            member_manager
                .get(guild_id.clone(), user_id.clone())
                .await
                .unwrap()
                .user
                .as_ref()
                .map(|user| user.username.as_str()),
            Some("discordrs")
        );
        assert_eq!(
            user_manager.get(user_id.clone()).await.unwrap().username,
            "discordrs"
        );
        assert_eq!(
            message_manager
                .get(channel_id.clone(), message_id.clone())
                .await
                .unwrap()
                .content,
            "cached"
        );
        assert_eq!(role_manager.list(guild_id.clone()).await.unwrap().len(), 1);

        assert!(guild_manager.contains(guild_id.clone()).await);
        assert!(channel_manager.contains(channel_id.clone()).await);
        assert!(user_manager.contains(user_id.clone()).await);
        assert!(
            member_manager
                .contains(guild_id.clone(), user_id.clone())
                .await
        );
        assert!(
            message_manager
                .contains(channel_id.clone(), message_id.clone())
                .await
        );
        assert!(
            role_manager
                .contains(guild_id.clone(), role_id.clone())
                .await
        );

        assert_eq!(
            guild_manager.cached(guild_id.clone()).await.unwrap().id,
            guild_id
        );
        assert_eq!(
            channel_manager.cached(channel_id.clone()).await.unwrap().id,
            channel_id
        );
        assert_eq!(
            user_manager.cached(user_id.clone()).await.unwrap().id,
            user_id
        );
        assert_eq!(
            member_manager
                .cached(guild_id.clone(), user_id.clone())
                .await
                .unwrap()
                .user
                .as_ref()
                .map(|user| user.id.clone()),
            Some(user_id.clone())
        );
        let member_arc = member_manager
            .cached_arc(guild_id.clone(), user_id.clone())
            .await
            .unwrap();
        assert_eq!(
            member_arc.user.as_ref().map(|user| user.id.clone()),
            Some(user_id.clone())
        );
        assert_eq!(
            message_manager
                .cached(channel_id.clone(), message_id.clone())
                .await
                .unwrap()
                .id,
            message_id
        );
        let message_arc = message_manager
            .cached_arc(channel_id.clone(), message_id.clone())
            .await
            .unwrap();
        assert_eq!(message_arc.id, message_id);
        assert_eq!(
            role_manager
                .cached(guild_id.clone(), role_id.clone())
                .await
                .unwrap()
                .id,
            role_id
        );

        assert_eq!(guild_manager.list_cached().await.len(), 1);
        assert_eq!(channel_manager.list_cached().await.len(), 1);
        assert_eq!(user_manager.list_cached().await.len(), 1);
        assert_eq!(member_manager.list_cached(guild_id.clone()).await.len(), 1);
        assert_eq!(
            member_manager.list_cached_arc(guild_id.clone()).await.len(),
            1
        );
        assert_eq!(
            message_manager.list_cached(channel_id.clone()).await.len(),
            1
        );
        assert_eq!(
            message_manager
                .list_cached_arc(channel_id.clone())
                .await
                .len(),
            1
        );
        assert_eq!(role_manager.list_cached(guild_id.clone()).await.len(), 1);
    }

    #[cfg(feature = "gateway")]
    #[tokio::test]
    async fn cached_manager_trait_impls_delegate_to_cache_for_hits() {
        let cache = CacheHandle::new();
        let http = Arc::new(DiscordHttpClient::new("token", 1));
        let guild_id = Snowflake::from("701");
        let channel_id = Snowflake::from("702");
        let user_id = Snowflake::from("703");

        cache
            .upsert_guild(Guild {
                id: guild_id.clone(),
                name: "guild".to_string(),
                ..Guild::default()
            })
            .await;
        cache
            .upsert_channel(Channel {
                id: channel_id.clone(),
                guild_id: Some(guild_id.clone()),
                kind: 0,
                name: Some("cached-channel".to_string()),
                ..Channel::default()
            })
            .await;
        cache
            .upsert_user(User {
                id: user_id.clone(),
                username: "cached-user".to_string(),
                ..User::default()
            })
            .await;

        let guild_manager = GuildManager::new(Arc::clone(&http), cache.clone());
        let channel_manager = ChannelManager::new(Arc::clone(&http), cache.clone());
        let user_manager = UserManager::new(http, cache);

        assert_eq!(
            <GuildManager as CachedManager<Guild>>::get(&guild_manager, guild_id.clone())
                .await
                .unwrap()
                .name,
            "guild"
        );
        assert_eq!(
            <GuildManager as CachedManager<Guild>>::cached(&guild_manager, guild_id.clone())
                .await
                .unwrap()
                .id,
            guild_id
        );
        assert!(
            <GuildManager as CachedManager<Guild>>::contains(&guild_manager, guild_id.clone())
                .await
        );
        assert_eq!(
            <GuildManager as CachedManager<Guild>>::list_cached(&guild_manager)
                .await
                .len(),
            1
        );

        assert_eq!(
            <ChannelManager as CachedManager<Channel>>::get(&channel_manager, channel_id.clone())
                .await
                .unwrap()
                .name
                .as_deref(),
            Some("cached-channel")
        );
        assert_eq!(
            <ChannelManager as CachedManager<Channel>>::cached(
                &channel_manager,
                channel_id.clone()
            )
            .await
            .unwrap()
            .id,
            channel_id
        );
        assert!(
            <ChannelManager as CachedManager<Channel>>::contains(
                &channel_manager,
                channel_id.clone()
            )
            .await
        );
        assert_eq!(
            <ChannelManager as CachedManager<Channel>>::list_cached(&channel_manager)
                .await
                .len(),
            1
        );

        assert_eq!(
            <UserManager as CachedManager<User>>::get(&user_manager, user_id.clone())
                .await
                .unwrap()
                .username,
            "cached-user"
        );
        assert_eq!(
            <UserManager as CachedManager<User>>::cached(&user_manager, user_id.clone())
                .await
                .unwrap()
                .id,
            user_id
        );
        assert!(
            <UserManager as CachedManager<User>>::contains(&user_manager, user_id.clone()).await
        );
        assert_eq!(
            <UserManager as CachedManager<User>>::list_cached(&user_manager)
                .await
                .len(),
            1
        );
    }
}
