use std::sync::Arc;

#[cfg(feature = "cache")]
use std::time::Instant;
#[cfg(feature = "cache")]
use tokio::sync::RwLock;

use crate::event::ScheduledEvent;
use crate::model::{
    Channel, ChannelId, EmojiId, Guild, GuildId, Member, Message, MessageId, Presence, Role,
    RoleId, ScheduledEventId, SoundboardSound, SoundboardSoundId, StageInstance, StageInstanceId,
    Sticker, StickerId, User, UserId, VoiceState,
};
use crate::types::Emoji;

#[path = "cache/config.rs"]
mod config;
pub use config::CacheConfig;

#[cfg(feature = "cache")]
#[path = "cache/backend.rs"]
mod backend;
#[cfg(feature = "cache")]
pub use backend::{CacheBackend, CacheClear, CacheClearId};

#[cfg(feature = "cache")]
#[path = "cache/store.rs"]
mod store;
#[cfg(feature = "cache")]
use store::{
    enforce_channel_limit, enforce_guild_limit, enforce_member_limit, enforce_message_limits,
    evict_channel_entries, evict_guild_entries, evict_shard_entries, insert_member_entry,
    insert_message_entry, maybe_prune_expired, ordered_overflow_keys, prune_expired, remember_key,
    remove_member_key, remove_message_key, remove_presence_key, seen_expired, CacheStore,
};

/// Discord's guild-to-shard formula, `(guild_id >> 22) % shard_count`.
///
/// Lives here rather than in `sharding` because the cache needs it without
/// the `sharding` feature: every gateway shard's READY scopes its cache
/// invalidation with it. A `shard_count` of zero is treated as one, so the
/// formula never divides by zero.
#[cfg(any(feature = "cache", feature = "gateway"))]
pub(crate) fn shard_for_guild_id(guild_id: GuildId, shard_count: u32) -> u32 {
    let shard =
        (guild_id.get() >> crate::model::SNOWFLAKE_TIMESTAMP_SHIFT) % u64::from(shard_count.max(1));
    // The remainder is below `shard_count`, so the conversion cannot fail.
    u32::try_from(shard).unwrap_or(0)
}

/// Empties every cached entity. The backend epoch (`backend_epoch`) is kept
/// on purpose: it must only grow, or a stale write could pass its check
/// again.
#[cfg(feature = "cache")]
fn clear_store(store: &mut CacheStore) {
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
    store.member_counts.clear();
    store.message_counts.clear();
    store.last_prune = None;
}

#[derive(Clone, Default)]
/// Typed Discord API object for `CacheHandle`.
pub struct CacheHandle {
    // Shared by every clone: shard event processors write, handlers read.
    // Guards are never held across an await.
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
        clear_store(&mut store);
    }

    #[cfg(not(feature = "cache"))]
    pub async fn clear(&self) {}

    /// Drops the cached state that a new gateway session on shard
    /// `shard_id` of `shard_count` replaces. The runtime calls this on every
    /// READY (not on RESUMED, which replays missed events instead).
    ///
    /// All shards of a process share one cache, so a READY must not wipe
    /// guilds that other shards are still serving:
    ///
    /// - `shard_count <= 1`: the shard owns everything, so this is
    ///   [`CacheHandle::clear`].
    /// - otherwise: every guild-scoped entry (guild, channels, threads,
    ///   members, roles, presences, voice states, emojis, stickers,
    ///   soundboard sounds, scheduled events, stage instances, and messages
    ///   of those guilds) whose guild maps to `shard_id` by
    ///   `(guild_id >> 22) % shard_count` is removed. Shard 0 additionally
    ///   drops DM channels and their messages, because Discord delivers DMs
    ///   only to shard 0.
    /// - Users are kept: they are shared by every shard a user has guilds
    ///   on, and the `max_users` LRU cap bounds what is left behind.
    ///   Messages without a guild whose channel is not cached are kept too,
    ///   since their owner cannot be determined; TTL and size caps age them
    ///   out.
    #[cfg(feature = "cache")]
    pub async fn clear_shard(&self, shard_id: u32, shard_count: u32) {
        if shard_count <= 1 {
            self.clear().await;
            return;
        }
        let mut store = self.store.write().await;
        evict_shard_entries(&mut store, shard_id, shard_count);
    }

    #[cfg(not(feature = "cache"))]
    pub async fn clear_shard(&self, _shard_id: u32, _shard_count: u32) {}

    /// The conditional clear of the [`CacheBackend`] epoch contract: clears
    /// everything (`scope: None`) or one shard's entries if the store-wide
    /// epoch equals `expected_epoch`, then increments the epoch and records
    /// `clear_id` — all under one lock, so no fenced write can interleave.
    #[cfg(feature = "cache")]
    pub(crate) async fn clear_if_epoch(
        &self,
        expected_epoch: u64,
        clear_id: CacheClearId,
        scope: Option<(u32, u32)>,
    ) -> CacheClear {
        let mut store = self.store.write().await;
        if store.backend_epoch != expected_epoch {
            // A retry of the last applied clear (its response was lost)
            // gets that clear's result again; anything else is stale.
            return match store.backend_last_clear {
                Some((last_id, produced)) if last_id == clear_id => CacheClear::Applied(produced),
                _ => CacheClear::Rejected(store.backend_epoch),
            };
        }
        match scope {
            Some((shard_id, shard_count)) if shard_count > 1 => {
                evict_shard_entries(&mut store, shard_id, shard_count);
            }
            _ => clear_store(&mut store),
        }
        store.backend_epoch += 1;
        store.backend_last_clear = Some((clear_id, store.backend_epoch));
        CacheClear::Applied(store.backend_epoch)
    }

    /// Whether a backend write issued under `epoch` may still be applied.
    #[cfg(feature = "cache")]
    fn epoch_is_current(store: &CacheStore, epoch: u64) -> bool {
        epoch >= store.backend_epoch
    }

    #[cfg(feature = "cache")]
    pub(crate) async fn fenced_upsert_member(
        &self,
        epoch: u64,
        guild_id: GuildId,
        user_id: UserId,
        member: Member,
    ) {
        let mut store = self.store.write().await;
        if Self::epoch_is_current(&store, epoch) {
            self.upsert_member_locked(&mut store, guild_id, user_id, member);
        }
    }

    #[cfg(feature = "cache")]
    pub(crate) async fn fenced_remove_member(
        &self,
        epoch: u64,
        guild_id: GuildId,
        user_id: UserId,
    ) {
        let mut store = self.store.write().await;
        if Self::epoch_is_current(&store, epoch) {
            remove_member_key(&mut store, (guild_id, user_id));
        }
    }

    #[cfg(feature = "cache")]
    pub(crate) async fn fenced_upsert_message(&self, epoch: u64, message: Message) {
        let mut store = self.store.write().await;
        if Self::epoch_is_current(&store, epoch) {
            self.upsert_message_locked(&mut store, message);
        }
    }

    #[cfg(feature = "cache")]
    pub(crate) async fn fenced_remove_message(
        &self,
        epoch: u64,
        channel_id: ChannelId,
        message_id: MessageId,
    ) {
        let mut store = self.store.write().await;
        if Self::epoch_is_current(&store, epoch) {
            remove_message_key(&mut store, (channel_id, message_id));
        }
    }

    #[cfg(feature = "cache")]
    pub(crate) async fn fenced_upsert_presence(
        &self,
        epoch: u64,
        guild_id: GuildId,
        user_id: UserId,
        presence: Presence,
    ) {
        let mut store = self.store.write().await;
        if Self::epoch_is_current(&store, epoch) {
            self.upsert_presence_locked(&mut store, guild_id, user_id, presence);
        }
    }

    #[cfg(feature = "cache")]
    pub(crate) async fn fenced_remove_presence(
        &self,
        epoch: u64,
        guild_id: GuildId,
        user_id: UserId,
    ) {
        let mut store = self.store.write().await;
        if Self::epoch_is_current(&store, epoch) {
            remove_presence_key(&mut store, (guild_id, user_id));
        }
    }

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
        let guild_id = guild.id;
        store.guilds.insert(guild_id, guild);
        remember_key(&mut store.guild_order, guild_id);
        enforce_guild_limit(&mut store, &self.config);
    }

    #[cfg(not(feature = "cache"))]
    pub async fn upsert_guild(&self, _guild: Guild) {}

    #[cfg(feature = "cache")]
    pub async fn remove_guild(&self, guild_id: GuildId) {
        let mut store = self.store.write().await;
        evict_guild_entries(&mut store, guild_id);
    }

    #[cfg(not(feature = "cache"))]
    pub async fn remove_guild(&self, _guild_id: GuildId) {}

    #[cfg(feature = "cache")]
    pub async fn guild(&self, guild_id: GuildId) -> Option<Guild> {
        self.store.read().await.guilds.get(&guild_id).cloned()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn guild(&self, _guild_id: GuildId) -> Option<Guild> {
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

    pub async fn contains_guild(&self, guild_id: GuildId) -> bool {
        self.guild(guild_id).await.is_some()
    }

    #[cfg(feature = "cache")]
    pub async fn upsert_channel(&self, channel: Channel) {
        let mut store = self.store.write().await;
        let channel_id = channel.id;
        store.channels.insert(channel_id, channel);
        remember_key(&mut store.channel_order, channel_id);
        enforce_channel_limit(&mut store, &self.config);
    }

    #[cfg(not(feature = "cache"))]
    pub async fn upsert_channel(&self, _channel: Channel) {}

    /// Inserts a channel only when the cache does not already hold one with
    /// the same id, so partial channel objects (for example the stubs
    /// derived from component interactions) never clobber a full channel
    /// received from the gateway.
    #[cfg(feature = "cache")]
    pub async fn upsert_channel_if_absent(&self, channel: Channel) {
        let mut store = self.store.write().await;
        if store.channels.contains_key(&channel.id) {
            return;
        }
        let channel_id = channel.id;
        store.channels.insert(channel_id, channel);
        remember_key(&mut store.channel_order, channel_id);
        enforce_channel_limit(&mut store, &self.config);
    }

    #[cfg(not(feature = "cache"))]
    pub async fn upsert_channel_if_absent(&self, _channel: Channel) {}

    /// Distributes a full GUILD_CREATE payload into the per-entity stores —
    /// channels, threads, members, users, voice states, presences, emojis,
    /// stickers, and stage instances — under a single write lock, then
    /// stores the guild itself with the bulk collections stripped so the
    /// data is not held twice.
    #[cfg(feature = "cache")]
    pub async fn apply_guild_create(&self, guild: &Guild) {
        let mut store = self.store.write().await;
        let guild_id = guild.id;
        let now = Instant::now();

        store
            .guilds
            .insert(guild_id, guild.without_create_collections());
        remember_key(&mut store.guild_order, guild_id);

        for role in &guild.roles {
            let key = (guild_id, role.id);
            store.roles.insert(key, role.clone());
            remember_key(&mut store.role_order, key);
        }

        for channel in guild.channels.iter().chain(guild.threads.iter()) {
            let mut channel = channel.clone();
            if channel.guild_id.is_none() {
                channel.guild_id = Some(guild_id);
            }
            let channel_id = channel.id;
            store.channels.insert(channel_id, channel);
            remember_key(&mut store.channel_order, channel_id);
        }

        for member in &guild.members {
            let Some(user) = member.user.as_ref() else {
                continue;
            };
            let user_id = user.id;
            store.users.insert(user_id, user.clone());
            remember_key(&mut store.user_order, user_id);
            let key = (guild_id, user_id);
            insert_member_entry(&mut store, key, Arc::new(member.clone()), now);
        }

        for voice_state in &guild.voice_states {
            let Some(user_id) = voice_state.user_id else {
                continue;
            };
            let mut voice_state = voice_state.clone();
            if voice_state.guild_id.is_none() {
                voice_state.guild_id = Some(guild_id);
            }
            let key = (guild_id, user_id);
            store.voice_states.insert(key, voice_state);
            remember_key(&mut store.voice_state_order, key);
        }

        for presence in &guild.presences {
            let user_id = presence
                .user_id
                .or_else(|| presence.user.as_ref().map(|user| user.id));
            let Some(user_id) = user_id else {
                continue;
            };
            let mut presence = presence.clone();
            presence.user_id = Some(user_id);
            let key = (guild_id, user_id);
            store.presences.insert(key, Arc::new(presence));
            store.presence_seen.insert(key, now);
            remember_key(&mut store.presence_order, key);
        }

        if self.config.cache_emojis {
            for emoji in &guild.emojis {
                if let Some(emoji_id) = emoji.id {
                    let key = (guild_id, emoji_id);
                    store.emojis.insert(key, emoji.clone());
                    remember_key(&mut store.emoji_order, key);
                }
            }
        }

        if self.config.cache_stickers {
            for sticker in &guild.stickers {
                let key = (guild_id, sticker.id);
                store.stickers.insert(key, sticker.clone());
                remember_key(&mut store.sticker_order, key);
            }
        }

        if self.config.cache_stage_instances {
            for stage_instance in &guild.stage_instances {
                let key = (guild_id, stage_instance.id);
                store.stage_instances.insert(key, stage_instance.clone());
                remember_key(&mut store.stage_instance_order, key);
            }
        }

        for sound in &guild.soundboard_sounds {
            let key = (guild_id, sound.sound_id);
            store.soundboard_sounds.insert(key, sound.clone());
            remember_key(&mut store.soundboard_sound_order, key);
        }

        enforce_guild_limit(&mut store, &self.config);
        enforce_channel_limit(&mut store, &self.config);
        enforce_member_limit(&mut store, &self.config, guild_id);
        let user_len = store.users.len();
        for key in ordered_overflow_keys(&mut store.user_order, user_len, self.config.max_users) {
            store.users.remove(&key);
        }
        let role_len = store.roles.len();
        for key in ordered_overflow_keys(&mut store.role_order, role_len, self.config.max_roles) {
            store.roles.remove(&key);
        }
        if let Some(max) = self.config.max_presences {
            while store.presences.len() > max {
                let Some(key) = store.presence_order.pop_front() else {
                    break;
                };
                store.presences.remove(&key);
                store.presence_seen.remove(&key);
            }
        }
        let voice_len = store.voice_states.len();
        for key in ordered_overflow_keys(
            &mut store.voice_state_order,
            voice_len,
            self.config.max_voice_states,
        ) {
            store.voice_states.remove(&key);
        }
    }

    #[cfg(not(feature = "cache"))]
    pub async fn apply_guild_create(&self, _guild: &Guild) {}

    /// Stores every member (and its user) from a GUILD_MEMBERS_CHUNK
    /// payload, plus any presences it carries, under a single write lock —
    /// so `request_guild_members` fills the cache the same way discord.js's
    /// `guild.members.fetch()` does.
    #[cfg(feature = "cache")]
    pub async fn apply_members_chunk(
        &self,
        guild_id: GuildId,
        members: &[Member],
        presences: Option<&[Presence]>,
    ) {
        let mut store = self.store.write().await;
        let now = Instant::now();

        for member in members {
            let Some(user) = member.user.as_ref() else {
                continue;
            };
            let user_id = user.id;
            store.users.insert(user_id, user.clone());
            remember_key(&mut store.user_order, user_id);
            let key = (guild_id, user_id);
            insert_member_entry(&mut store, key, Arc::new(member.clone()), now);
        }

        for presence in presences.unwrap_or_default() {
            let user_id = presence
                .user_id
                .or_else(|| presence.user.as_ref().map(|user| user.id));
            let Some(user_id) = user_id else {
                continue;
            };
            let mut presence = presence.clone();
            presence.user_id = Some(user_id);
            let key = (guild_id, user_id);
            store.presences.insert(key, Arc::new(presence));
            store.presence_seen.insert(key, now);
            remember_key(&mut store.presence_order, key);
        }

        enforce_member_limit(&mut store, &self.config, guild_id);
        let user_len = store.users.len();
        for key in ordered_overflow_keys(&mut store.user_order, user_len, self.config.max_users) {
            store.users.remove(&key);
        }
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
    pub async fn apply_members_chunk(
        &self,
        _guild_id: GuildId,
        _members: &[Member],
        _presences: Option<&[Presence]>,
    ) {
    }

    #[cfg(feature = "cache")]
    pub async fn remove_channel(&self, channel_id: ChannelId) {
        let mut store = self.store.write().await;
        evict_channel_entries(&mut store, channel_id);
    }

    #[cfg(not(feature = "cache"))]
    pub async fn remove_channel(&self, _channel_id: ChannelId) {}

    #[cfg(not(feature = "cache"))]
    pub async fn channel(&self, _channel_id: ChannelId) -> Option<Channel> {
        None
    }

    #[cfg(feature = "cache")]
    pub async fn channel(&self, channel_id: ChannelId) -> Option<Channel> {
        self.store.read().await.channels.get(&channel_id).cloned()
    }

    #[cfg(feature = "cache")]
    pub async fn channels(&self) -> Vec<Channel> {
        self.store.read().await.channels.values().cloned().collect()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn channels(&self) -> Vec<Channel> {
        Vec::new()
    }

    pub async fn contains_channel(&self, channel_id: ChannelId) -> bool {
        self.channel(channel_id).await.is_some()
    }

    #[cfg(feature = "cache")]
    pub async fn upsert_user(&self, user: User) {
        let mut store = self.store.write().await;
        let user_id = user.id;
        store.users.insert(user_id, user);
        remember_key(&mut store.user_order, user_id);
        let len = store.users.len();
        for key in ordered_overflow_keys(&mut store.user_order, len, self.config.max_users) {
            store.users.remove(&key);
        }
    }

    #[cfg(not(feature = "cache"))]
    pub async fn upsert_user(&self, _user: User) {}

    #[cfg(feature = "cache")]
    pub async fn remove_user(&self, user_id: UserId) {
        let mut store = self.store.write().await;
        store.users.remove(&user_id);
        store.user_order.remove(&user_id);
    }

    #[cfg(not(feature = "cache"))]
    pub async fn remove_user(&self, _user_id: UserId) {}

    #[cfg(feature = "cache")]
    pub async fn user(&self, user_id: UserId) -> Option<User> {
        self.store.read().await.users.get(&user_id).cloned()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn user(&self, _user_id: UserId) -> Option<User> {
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

    pub async fn contains_user(&self, user_id: UserId) -> bool {
        self.user(user_id).await.is_some()
    }

    #[cfg(feature = "cache")]
    pub async fn upsert_member(&self, guild_id: GuildId, user_id: UserId, member: Member) {
        let mut store = self.store.write().await;
        self.upsert_member_locked(&mut store, guild_id, user_id, member);
    }

    #[cfg(feature = "cache")]
    fn upsert_member_locked(
        &self,
        store: &mut CacheStore,
        guild_id: GuildId,
        user_id: UserId,
        member: Member,
    ) {
        let now = Instant::now();
        let key = (guild_id, user_id);
        insert_member_entry(store, key, Arc::new(member), now);
        maybe_prune_expired(store, &self.config, now);
        enforce_member_limit(store, &self.config, guild_id);
    }

    #[cfg(not(feature = "cache"))]
    pub async fn upsert_member(&self, _guild_id: GuildId, _user_id: UserId, _member: Member) {}

    #[cfg(feature = "cache")]
    pub async fn remove_member(&self, guild_id: GuildId, user_id: UserId) {
        let mut store = self.store.write().await;
        let key = (guild_id, user_id);
        remove_member_key(&mut store, key);
    }

    #[cfg(not(feature = "cache"))]
    pub async fn remove_member(&self, _guild_id: GuildId, _user_id: UserId) {}

    #[cfg(not(feature = "cache"))]
    pub async fn member(&self, _guild_id: GuildId, _user_id: UserId) -> Option<Member> {
        None
    }

    #[cfg(feature = "cache")]
    pub async fn member(&self, guild_id: GuildId, user_id: UserId) -> Option<Member> {
        self.member_arc(guild_id, user_id)
            .await
            .map(|member| member.as_ref().clone())
    }

    #[cfg(feature = "cache")]
    pub async fn member_arc(&self, guild_id: GuildId, user_id: UserId) -> Option<Arc<Member>> {
        let key = (guild_id, user_id);
        let now = Instant::now();
        let store = self.store.read().await;
        if seen_expired(store.member_seen.get(&key), self.config.member_ttl, now) {
            // Expired entries are never returned; physical removal is left
            // to the throttled sweep on the write paths.
            return None;
        }
        store.members.get(&key).cloned()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn member_arc(&self, _guild_id: GuildId, _user_id: UserId) -> Option<Arc<Member>> {
        None
    }

    #[cfg(feature = "cache")]
    pub async fn members(&self, guild_id: GuildId) -> Vec<Member> {
        self.members_arc(guild_id)
            .await
            .into_iter()
            .map(|member| member.as_ref().clone())
            .collect()
    }

    #[cfg(feature = "cache")]
    pub async fn members_arc(&self, guild_id: GuildId) -> Vec<Arc<Member>> {
        let now = Instant::now();
        let store = self.store.read().await;
        store
            .members
            .iter()
            .filter(|((stored_guild_id, _), _)| *stored_guild_id == guild_id)
            .filter(|(key, _)| {
                // Filter expired entries per-key at read time; physical
                // removal is left to the throttled sweep.
                !seen_expired(store.member_seen.get(*key), self.config.member_ttl, now)
            })
            .map(|(_, member)| Arc::clone(member))
            .collect()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn members_arc(&self, _guild_id: GuildId) -> Vec<Arc<Member>> {
        Vec::new()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn members(&self, _guild_id: GuildId) -> Vec<Member> {
        Vec::new()
    }

    pub async fn contains_member(&self, guild_id: GuildId, user_id: UserId) -> bool {
        self.member(guild_id, user_id).await.is_some()
    }

    #[cfg(feature = "cache")]
    pub async fn upsert_message(&self, message: Message) {
        let mut store = self.store.write().await;
        self.upsert_message_locked(&mut store, message);
    }

    #[cfg(feature = "cache")]
    fn upsert_message_locked(&self, store: &mut CacheStore, message: Message) {
        let channel_id = message.channel_id;
        let now = Instant::now();
        let key = (channel_id, message.id);
        insert_message_entry(store, key, Arc::new(message), now);
        maybe_prune_expired(store, &self.config, now);
        enforce_message_limits(store, &self.config, channel_id);
    }

    #[cfg(not(feature = "cache"))]
    pub async fn upsert_message(&self, _message: Message) {}

    #[cfg(feature = "cache")]
    pub async fn remove_message(&self, channel_id: ChannelId, message_id: MessageId) {
        let mut store = self.store.write().await;
        let key = (channel_id, message_id);
        remove_message_key(&mut store, key);
    }

    #[cfg(not(feature = "cache"))]
    pub async fn remove_message(&self, _channel_id: ChannelId, _message_id: MessageId) {}

    #[cfg(feature = "cache")]
    pub async fn remove_messages_bulk(&self, channel_id: ChannelId, message_ids: &[MessageId]) {
        let mut store = self.store.write().await;
        for message_id in message_ids {
            let key = (channel_id, *message_id);
            remove_message_key(&mut store, key);
        }
    }

    #[cfg(not(feature = "cache"))]
    pub async fn remove_messages_bulk(&self, _channel_id: ChannelId, _message_ids: &[MessageId]) {}

    #[cfg(not(feature = "cache"))]
    pub async fn message(&self, _channel_id: ChannelId, _message_id: MessageId) -> Option<Message> {
        None
    }

    #[cfg(feature = "cache")]
    pub async fn message(&self, channel_id: ChannelId, message_id: MessageId) -> Option<Message> {
        self.message_arc(channel_id, message_id)
            .await
            .map(|message| message.as_ref().clone())
    }

    #[cfg(feature = "cache")]
    pub async fn message_arc(
        &self,
        channel_id: ChannelId,
        message_id: MessageId,
    ) -> Option<Arc<Message>> {
        let key = (channel_id, message_id);
        let now = Instant::now();
        let store = self.store.read().await;
        if seen_expired(store.message_seen.get(&key), self.config.message_ttl, now) {
            // Expired entries are never returned; physical removal is left
            // to the throttled sweep on the write paths.
            return None;
        }
        store.messages.get(&key).cloned()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn message_arc(
        &self,
        _channel_id: ChannelId,
        _message_id: MessageId,
    ) -> Option<Arc<Message>> {
        None
    }

    #[cfg(feature = "cache")]
    pub async fn messages(&self, channel_id: ChannelId) -> Vec<Message> {
        self.messages_arc(channel_id)
            .await
            .into_iter()
            .map(|message| message.as_ref().clone())
            .collect()
    }

    #[cfg(feature = "cache")]
    pub async fn messages_arc(&self, channel_id: ChannelId) -> Vec<Arc<Message>> {
        let now = Instant::now();
        let store = self.store.read().await;
        store
            .messages
            .iter()
            .filter(|((stored_channel_id, _), _)| *stored_channel_id == channel_id)
            .filter(|(key, _)| {
                // Filter expired entries per-key at read time; physical
                // removal is left to the throttled sweep.
                !seen_expired(store.message_seen.get(*key), self.config.message_ttl, now)
            })
            .map(|(_, message)| Arc::clone(message))
            .collect()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn messages_arc(&self, _channel_id: ChannelId) -> Vec<Arc<Message>> {
        Vec::new()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn messages(&self, _channel_id: ChannelId) -> Vec<Message> {
        Vec::new()
    }

    pub async fn contains_message(&self, channel_id: ChannelId, message_id: MessageId) -> bool {
        self.message(channel_id, message_id).await.is_some()
    }

    #[cfg(feature = "cache")]
    pub async fn upsert_role(&self, guild_id: GuildId, role: Role) {
        let mut store = self.store.write().await;
        let key = (guild_id, role.id);
        store.roles.insert(key, role);
        remember_key(&mut store.role_order, key);
        let len = store.roles.len();
        for key in ordered_overflow_keys(&mut store.role_order, len, self.config.max_roles) {
            store.roles.remove(&key);
        }
    }

    #[cfg(not(feature = "cache"))]
    pub async fn upsert_role(&self, _guild_id: GuildId, _role: Role) {}

    #[cfg(feature = "cache")]
    pub async fn remove_role(&self, guild_id: GuildId, role_id: RoleId) {
        let mut store = self.store.write().await;
        let key = (guild_id, role_id);
        store.roles.remove(&key);
        store.role_order.remove(&key);
    }

    #[cfg(not(feature = "cache"))]
    pub async fn remove_role(&self, _guild_id: GuildId, _role_id: RoleId) {}

    #[cfg(not(feature = "cache"))]
    pub async fn roles(&self, _guild_id: GuildId) -> Vec<Role> {
        Vec::new()
    }

    #[cfg(feature = "cache")]
    pub async fn roles(&self, guild_id: GuildId) -> Vec<Role> {
        self.store
            .read()
            .await
            .roles
            .iter()
            .filter(|((stored_guild_id, _), _)| *stored_guild_id == guild_id)
            .map(|(_, role)| role.clone())
            .collect()
    }

    #[cfg(feature = "cache")]
    pub async fn role(&self, guild_id: GuildId, role_id: RoleId) -> Option<Role> {
        self.store
            .read()
            .await
            .roles
            .get(&(guild_id, role_id))
            .cloned()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn role(&self, _guild_id: GuildId, _role_id: RoleId) -> Option<Role> {
        None
    }

    pub async fn contains_role(&self, guild_id: GuildId, role_id: RoleId) -> bool {
        self.role(guild_id, role_id).await.is_some()
    }

    #[cfg(feature = "cache")]
    pub async fn upsert_presence(&self, guild_id: GuildId, user_id: UserId, presence: Presence) {
        let mut store = self.store.write().await;
        self.upsert_presence_locked(&mut store, guild_id, user_id, presence);
    }

    #[cfg(feature = "cache")]
    fn upsert_presence_locked(
        &self,
        store: &mut CacheStore,
        guild_id: GuildId,
        user_id: UserId,
        presence: Presence,
    ) {
        let now = Instant::now();
        let key = (guild_id, user_id);
        store.presences.insert(key, Arc::new(presence));
        store.presence_seen.insert(key, now);
        remember_key(&mut store.presence_order, key);
        maybe_prune_expired(store, &self.config, now);
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
    pub async fn upsert_presence(&self, _guild_id: GuildId, _user_id: UserId, _presence: Presence) {
    }

    #[cfg(feature = "cache")]
    pub async fn remove_presence(&self, guild_id: GuildId, user_id: UserId) {
        let mut store = self.store.write().await;
        let key = (guild_id, user_id);
        remove_presence_key(&mut store, key);
    }

    #[cfg(not(feature = "cache"))]
    pub async fn remove_presence(&self, _guild_id: GuildId, _user_id: UserId) {}

    #[cfg(feature = "cache")]
    pub async fn presence(&self, guild_id: GuildId, user_id: UserId) -> Option<Presence> {
        self.presence_arc(guild_id, user_id)
            .await
            .map(|presence| presence.as_ref().clone())
    }

    #[cfg(feature = "cache")]
    pub async fn presence_arc(&self, guild_id: GuildId, user_id: UserId) -> Option<Arc<Presence>> {
        let key = (guild_id, user_id);
        let now = Instant::now();
        let store = self.store.read().await;
        if seen_expired(store.presence_seen.get(&key), self.config.presence_ttl, now) {
            // Expired entries are never returned; physical removal is left
            // to the throttled sweep on the write paths.
            return None;
        }
        store.presences.get(&key).cloned()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn presence_arc(
        &self,
        _guild_id: GuildId,
        _user_id: UserId,
    ) -> Option<Arc<Presence>> {
        None
    }

    #[cfg(not(feature = "cache"))]
    pub async fn presence(&self, _guild_id: GuildId, _user_id: UserId) -> Option<Presence> {
        None
    }

    #[cfg(feature = "cache")]
    pub async fn presences(&self, guild_id: GuildId) -> Vec<Presence> {
        self.presences_arc(guild_id)
            .await
            .into_iter()
            .map(|presence| presence.as_ref().clone())
            .collect()
    }

    #[cfg(feature = "cache")]
    pub async fn presences_arc(&self, guild_id: GuildId) -> Vec<Arc<Presence>> {
        let now = Instant::now();
        let store = self.store.read().await;
        store
            .presences
            .iter()
            .filter(|((stored_guild_id, _), _)| *stored_guild_id == guild_id)
            .filter(|(key, _)| {
                // Filter expired entries per-key at read time; physical
                // removal is left to the throttled sweep.
                !seen_expired(store.presence_seen.get(*key), self.config.presence_ttl, now)
            })
            .map(|(_, presence)| Arc::clone(presence))
            .collect()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn presences_arc(&self, _guild_id: GuildId) -> Vec<Arc<Presence>> {
        Vec::new()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn presences(&self, _guild_id: GuildId) -> Vec<Presence> {
        Vec::new()
    }

    pub async fn contains_presence(&self, guild_id: GuildId, user_id: UserId) -> bool {
        self.presence(guild_id, user_id).await.is_some()
    }

    #[cfg(feature = "cache")]
    pub async fn upsert_voice_state(
        &self,
        guild_id: GuildId,
        user_id: UserId,
        voice_state: VoiceState,
    ) {
        let mut store = self.store.write().await;
        let key = (guild_id, user_id);
        store.voice_states.insert(key, voice_state);
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
        _guild_id: GuildId,
        _user_id: UserId,
        _voice_state: VoiceState,
    ) {
    }

    #[cfg(feature = "cache")]
    pub async fn remove_voice_state(&self, guild_id: GuildId, user_id: UserId) {
        let mut store = self.store.write().await;
        let key = (guild_id, user_id);
        store.voice_states.remove(&key);
        store.voice_state_order.remove(&key);
    }

    #[cfg(not(feature = "cache"))]
    pub async fn remove_voice_state(&self, _guild_id: GuildId, _user_id: UserId) {}

    #[cfg(feature = "cache")]
    pub async fn voice_state(&self, guild_id: GuildId, user_id: UserId) -> Option<VoiceState> {
        self.store
            .read()
            .await
            .voice_states
            .get(&(guild_id, user_id))
            .cloned()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn voice_state(&self, _guild_id: GuildId, _user_id: UserId) -> Option<VoiceState> {
        None
    }

    #[cfg(feature = "cache")]
    pub async fn voice_states(&self, guild_id: GuildId) -> Vec<VoiceState> {
        self.store
            .read()
            .await
            .voice_states
            .iter()
            .filter(|((stored_guild_id, _), _)| *stored_guild_id == guild_id)
            .map(|(_, voice_state)| voice_state.clone())
            .collect()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn voice_states(&self, _guild_id: GuildId) -> Vec<VoiceState> {
        Vec::new()
    }

    pub async fn contains_voice_state(&self, guild_id: GuildId, user_id: UserId) -> bool {
        self.voice_state(guild_id, user_id).await.is_some()
    }

    #[cfg(feature = "cache")]
    pub async fn upsert_soundboard_sound(&self, guild_id: GuildId, sound: SoundboardSound) {
        let mut store = self.store.write().await;
        let key = (guild_id, sound.sound_id);
        store.soundboard_sounds.insert(key, sound);
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
    pub async fn upsert_soundboard_sound(&self, _guild_id: GuildId, _sound: SoundboardSound) {}

    #[cfg(feature = "cache")]
    pub async fn replace_soundboard_sounds(&self, guild_id: GuildId, sounds: Vec<SoundboardSound>) {
        let mut store = self.store.write().await;
        store
            .soundboard_sounds
            .retain(|(stored_guild_id, _), _| stored_guild_id != &guild_id);
        store
            .soundboard_sound_order
            .retain(|(stored_guild_id, _)| stored_guild_id != &guild_id);
        for sound in sounds {
            let key = (guild_id, sound.sound_id);
            store.soundboard_sounds.insert(key, sound);
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
        _guild_id: GuildId,
        _sounds: Vec<SoundboardSound>,
    ) {
    }

    #[cfg(feature = "cache")]
    pub async fn remove_soundboard_sound(&self, guild_id: GuildId, sound_id: SoundboardSoundId) {
        let mut store = self.store.write().await;
        let key = (guild_id, sound_id);
        store.soundboard_sounds.remove(&key);
        store.soundboard_sound_order.remove(&key);
    }

    #[cfg(not(feature = "cache"))]
    pub async fn remove_soundboard_sound(&self, _guild_id: GuildId, _sound_id: SoundboardSoundId) {}

    #[cfg(feature = "cache")]
    pub async fn soundboard_sound(
        &self,
        guild_id: GuildId,
        sound_id: SoundboardSoundId,
    ) -> Option<SoundboardSound> {
        self.store
            .read()
            .await
            .soundboard_sounds
            .get(&(guild_id, sound_id))
            .cloned()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn soundboard_sound(
        &self,
        _guild_id: GuildId,
        _sound_id: SoundboardSoundId,
    ) -> Option<SoundboardSound> {
        None
    }

    #[cfg(feature = "cache")]
    pub async fn soundboard_sounds(&self, guild_id: GuildId) -> Vec<SoundboardSound> {
        self.store
            .read()
            .await
            .soundboard_sounds
            .iter()
            .filter(|((stored_guild_id, _), _)| *stored_guild_id == guild_id)
            .map(|(_, sound)| sound.clone())
            .collect()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn soundboard_sounds(&self, _guild_id: GuildId) -> Vec<SoundboardSound> {
        Vec::new()
    }

    pub async fn contains_soundboard_sound(
        &self,
        guild_id: GuildId,
        sound_id: SoundboardSoundId,
    ) -> bool {
        self.soundboard_sound(guild_id, sound_id).await.is_some()
    }

    #[cfg(feature = "cache")]
    pub async fn replace_emojis(&self, guild_id: GuildId, emojis: Vec<Emoji>) {
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
            if let Some(emoji_id) = emoji.id {
                let key = (guild_id, emoji_id);
                store.emojis.insert(key, emoji);
                remember_key(&mut store.emoji_order, key);
            }
        }
        let len = store.emojis.len();
        for key in ordered_overflow_keys(&mut store.emoji_order, len, self.config.max_emojis) {
            store.emojis.remove(&key);
        }
    }

    #[cfg(not(feature = "cache"))]
    pub async fn replace_emojis(&self, _guild_id: GuildId, _emojis: Vec<Emoji>) {}

    #[cfg(feature = "cache")]
    pub async fn emoji(&self, guild_id: GuildId, emoji_id: EmojiId) -> Option<Emoji> {
        self.store
            .read()
            .await
            .emojis
            .get(&(guild_id, emoji_id))
            .cloned()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn emoji(&self, _guild_id: GuildId, _emoji_id: EmojiId) -> Option<Emoji> {
        None
    }

    #[cfg(feature = "cache")]
    pub async fn emojis(&self, guild_id: GuildId) -> Vec<Emoji> {
        self.store
            .read()
            .await
            .emojis
            .iter()
            .filter(|((stored_guild_id, _), _)| *stored_guild_id == guild_id)
            .map(|(_, emoji)| emoji.clone())
            .collect()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn emojis(&self, _guild_id: GuildId) -> Vec<Emoji> {
        Vec::new()
    }

    #[cfg(feature = "cache")]
    pub async fn replace_stickers(&self, guild_id: GuildId, stickers: Vec<Sticker>) {
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
            let key = (guild_id, sticker.id);
            store.stickers.insert(key, sticker);
            remember_key(&mut store.sticker_order, key);
        }
        let len = store.stickers.len();
        for key in ordered_overflow_keys(&mut store.sticker_order, len, self.config.max_stickers) {
            store.stickers.remove(&key);
        }
    }

    #[cfg(not(feature = "cache"))]
    pub async fn replace_stickers(&self, _guild_id: GuildId, _stickers: Vec<Sticker>) {}

    #[cfg(feature = "cache")]
    pub async fn sticker(&self, guild_id: GuildId, sticker_id: StickerId) -> Option<Sticker> {
        self.store
            .read()
            .await
            .stickers
            .get(&(guild_id, sticker_id))
            .cloned()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn sticker(&self, _guild_id: GuildId, _sticker_id: StickerId) -> Option<Sticker> {
        None
    }

    #[cfg(feature = "cache")]
    pub async fn stickers(&self, guild_id: GuildId) -> Vec<Sticker> {
        self.store
            .read()
            .await
            .stickers
            .iter()
            .filter(|((stored_guild_id, _), _)| *stored_guild_id == guild_id)
            .map(|(_, sticker)| sticker.clone())
            .collect()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn stickers(&self, _guild_id: GuildId) -> Vec<Sticker> {
        Vec::new()
    }

    #[cfg(feature = "cache")]
    pub async fn upsert_scheduled_event(&self, event: ScheduledEvent) {
        if !self.config.cache_scheduled_events {
            return;
        }
        let (Some(guild_id), Some(event_id)) = (event.guild_id, event.id) else {
            return;
        };
        let mut store = self.store.write().await;
        let key = (guild_id, event_id);
        store.scheduled_events.insert(key, event);
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
    pub async fn remove_scheduled_event(&self, guild_id: GuildId, event_id: ScheduledEventId) {
        let mut store = self.store.write().await;
        let key = (guild_id, event_id);
        store.scheduled_events.remove(&key);
        store.scheduled_event_order.remove(&key);
    }

    #[cfg(not(feature = "cache"))]
    pub async fn remove_scheduled_event(&self, _guild_id: GuildId, _event_id: ScheduledEventId) {}

    #[cfg(feature = "cache")]
    pub async fn scheduled_event(
        &self,
        guild_id: GuildId,
        event_id: ScheduledEventId,
    ) -> Option<ScheduledEvent> {
        self.store
            .read()
            .await
            .scheduled_events
            .get(&(guild_id, event_id))
            .cloned()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn scheduled_event(
        &self,
        _guild_id: GuildId,
        _event_id: ScheduledEventId,
    ) -> Option<ScheduledEvent> {
        None
    }

    #[cfg(feature = "cache")]
    pub async fn scheduled_events(&self, guild_id: GuildId) -> Vec<ScheduledEvent> {
        self.store
            .read()
            .await
            .scheduled_events
            .iter()
            .filter(|((stored_guild_id, _), _)| *stored_guild_id == guild_id)
            .map(|(_, event)| event.clone())
            .collect()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn scheduled_events(&self, _guild_id: GuildId) -> Vec<ScheduledEvent> {
        Vec::new()
    }

    #[cfg(feature = "cache")]
    pub async fn upsert_stage_instance(&self, stage_instance: StageInstance) {
        if !self.config.cache_stage_instances {
            return;
        }
        let mut store = self.store.write().await;
        let key = (stage_instance.guild_id, stage_instance.id);
        store.stage_instances.insert(key, stage_instance);
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
    pub async fn remove_stage_instance(
        &self,
        guild_id: GuildId,
        stage_instance_id: StageInstanceId,
    ) {
        let mut store = self.store.write().await;
        let key = (guild_id, stage_instance_id);
        store.stage_instances.remove(&key);
        store.stage_instance_order.remove(&key);
    }

    #[cfg(not(feature = "cache"))]
    pub async fn remove_stage_instance(
        &self,
        _guild_id: GuildId,
        _stage_instance_id: StageInstanceId,
    ) {
    }

    #[cfg(feature = "cache")]
    pub async fn stage_instance(
        &self,
        guild_id: GuildId,
        stage_instance_id: StageInstanceId,
    ) -> Option<StageInstance> {
        self.store
            .read()
            .await
            .stage_instances
            .get(&(guild_id, stage_instance_id))
            .cloned()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn stage_instance(
        &self,
        _guild_id: GuildId,
        _stage_instance_id: StageInstanceId,
    ) -> Option<StageInstance> {
        None
    }

    #[cfg(feature = "cache")]
    pub async fn stage_instances(&self, guild_id: GuildId) -> Vec<StageInstance> {
        self.store
            .read()
            .await
            .stage_instances
            .iter()
            .filter(|((stored_guild_id, _), _)| *stored_guild_id == guild_id)
            .map(|(_, stage_instance)| stage_instance.clone())
            .collect()
    }

    #[cfg(not(feature = "cache"))]
    pub async fn stage_instances(&self, _guild_id: GuildId) -> Vec<StageInstance> {
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
    use crate::model::{test_id, ChannelType, PresenceStatus, StageInstancePrivacyLevel};
    use crate::model::{ChannelId, GuildId, MessageId, UserId};
    use std::sync::Arc;
    use std::time::Duration;

    use crate::event::ScheduledEvent;
    #[cfg(feature = "gateway")]
    use crate::manager::CachedManager;
    use crate::model::{
        Channel, Guild, Message, Presence, Role, SoundboardSound, StageInstance, Sticker, User,
        VoiceState,
    };
    use crate::types::Emoji;

    use super::{CacheBackend, CacheClear, CacheClearId, CacheConfig, CacheHandle};
    #[cfg(feature = "gateway")]
    use super::{
        ChannelManager, GuildManager, MemberManager, MessageManager, RoleManager, UserManager,
    };
    #[cfg(feature = "gateway")]
    use crate::http::DiscordHttpClient;

    /// A clear id of a test client.
    fn clear_id(client: u64, sequence: u64) -> CacheClearId {
        CacheClearId { client, sequence }
    }

    /// Fills every guild-scoped store for `guild_id`, using IDs derived from
    /// `base` so several guilds can coexist.
    async fn populate_guild_entries(cache: &CacheHandle, guild_id: GuildId, base: u64) {
        cache
            .upsert_guild(Guild {
                id: guild_id,
                name: format!("guild-{base}"),
                ..Guild::fixture()
            })
            .await;
        cache
            .upsert_channel(Channel {
                id: test_id(base + 1),
                guild_id: Some(guild_id),
                ..Channel::fixture()
            })
            .await;
        // Messages fetched over REST carry no guild_id; the cached channel
        // ties this one to the guild.
        cache
            .upsert_message(Message {
                id: test_id(base + 2),
                channel_id: test_id(base + 1),
                ..Message::fixture()
            })
            .await;
        cache
            .upsert_user(User {
                id: test_id(base + 3),
                username: format!("user-{base}"),
                ..User::fixture()
            })
            .await;
        cache
            .upsert_member(
                guild_id,
                test_id(base + 3),
                crate::model::Member {
                    user: Some(User {
                        id: test_id(base + 3),
                        ..User::fixture()
                    }),
                    ..crate::model::Member::default()
                },
            )
            .await;
        cache
            .upsert_role(
                guild_id,
                Role {
                    id: test_id(base + 4),
                    ..Role::fixture()
                },
            )
            .await;
        cache
            .upsert_presence(
                guild_id,
                test_id(base + 3),
                Presence {
                    user_id: Some(test_id(base + 3)),
                    ..Presence::default()
                },
            )
            .await;
        cache
            .upsert_voice_state(
                guild_id,
                test_id(base + 3),
                VoiceState {
                    guild_id: Some(guild_id),
                    channel_id: Some(test_id(base + 1)),
                    user_id: Some(test_id(base + 3)),
                    ..VoiceState::default()
                },
            )
            .await;
        cache
            .upsert_soundboard_sound(
                guild_id,
                SoundboardSound {
                    sound_id: test_id(base + 5),
                    ..SoundboardSound::fixture()
                },
            )
            .await;
        cache
            .replace_emojis(
                guild_id,
                vec![Emoji {
                    id: Some(test_id(base + 6)),
                    ..Emoji::default()
                }],
            )
            .await;
        cache
            .replace_stickers(
                guild_id,
                vec![Sticker {
                    id: test_id(base + 7),
                    ..Sticker::fixture()
                }],
            )
            .await;
        cache
            .upsert_scheduled_event(ScheduledEvent {
                id: Some(test_id(base + 8)),
                guild_id: Some(guild_id),
                ..ScheduledEvent::default()
            })
            .await;
        cache
            .upsert_stage_instance(StageInstance {
                id: test_id(base + 9),
                guild_id,
                channel_id: test_id(base + 1),
                ..StageInstance::fixture()
            })
            .await;
        // A gateway message whose channel is not cached is still tied to
        // the guild by its guild_id.
        cache
            .upsert_message(Message {
                id: test_id(base + 11),
                channel_id: test_id(base + 10),
                guild_id: Some(guild_id),
                ..Message::fixture()
            })
            .await;
    }

    /// Asserts whether every entry written by [`populate_guild_entries`]
    /// (except the global user) is still cached.
    async fn assert_guild_entries(
        cache: &CacheHandle,
        guild_id: GuildId,
        base: u64,
        present: bool,
    ) {
        let found = [
            ("guild", cache.guild(guild_id).await.is_some()),
            ("channel", cache.channel(test_id(base + 1)).await.is_some()),
            (
                "channel message",
                cache
                    .message(test_id(base + 1), test_id(base + 2))
                    .await
                    .is_some(),
            ),
            (
                "member",
                cache.member(guild_id, test_id(base + 3)).await.is_some(),
            ),
            (
                "role",
                cache.role(guild_id, test_id(base + 4)).await.is_some(),
            ),
            (
                "presence",
                cache.presence(guild_id, test_id(base + 3)).await.is_some(),
            ),
            (
                "voice state",
                cache
                    .voice_state(guild_id, test_id(base + 3))
                    .await
                    .is_some(),
            ),
            (
                "soundboard sound",
                cache
                    .soundboard_sound(guild_id, test_id(base + 5))
                    .await
                    .is_some(),
            ),
            (
                "emoji",
                cache.emoji(guild_id, test_id(base + 6)).await.is_some(),
            ),
            (
                "sticker",
                cache.sticker(guild_id, test_id(base + 7)).await.is_some(),
            ),
            (
                "scheduled event",
                cache
                    .scheduled_event(guild_id, test_id(base + 8))
                    .await
                    .is_some(),
            ),
            (
                "stage instance",
                cache
                    .stage_instance(guild_id, test_id(base + 9))
                    .await
                    .is_some(),
            ),
            (
                "guild message",
                cache
                    .message(test_id(base + 10), test_id(base + 11))
                    .await
                    .is_some(),
            ),
        ];
        for (entry, is_cached) in found {
            assert_eq!(
                is_cached, present,
                "{entry} of guild {guild_id} cached={is_cached}, expected {present}"
            );
        }
    }

    #[tokio::test]
    async fn clear_shard_evicts_only_entries_owned_by_that_shard() {
        let cache = CacheHandle::with_config(CacheConfig::unbounded());
        // (guild_id >> 22) % 2 picks the shard.
        let shard0_guild = test_id(2_u64 << 22);
        let shard1_guild = test_id(1_u64 << 22);
        populate_guild_entries(&cache, shard0_guild, 100).await;
        populate_guild_entries(&cache, shard1_guild, 200).await;

        let dm_channel = test_id(900);
        cache
            .upsert_channel(Channel {
                id: dm_channel,
                kind: ChannelType::DM,
                ..Channel::fixture()
            })
            .await;
        cache
            .upsert_message(Message {
                id: test_id(901),
                channel_id: dm_channel,
                ..Message::fixture()
            })
            .await;
        // No guild_id and an uncached channel: the owner is unknown.
        let unknown_owner_channel = test_id(902);
        cache
            .upsert_message(Message {
                id: test_id(903),
                channel_id: unknown_owner_channel,
                ..Message::fixture()
            })
            .await;

        // A later shard's READY must not wipe the guilds of other shards.
        cache.clear_shard(1, 2).await;
        assert_guild_entries(&cache, shard1_guild, 200, false).await;
        assert_guild_entries(&cache, shard0_guild, 100, true).await;
        assert!(
            cache.channel(dm_channel).await.is_some(),
            "DMs belong to shard 0"
        );
        assert!(
            cache.user(test_id(203)).await.is_some(),
            "users are global and survive a shard's READY"
        );

        cache.clear_shard(0, 2).await;
        assert_guild_entries(&cache, shard0_guild, 100, false).await;
        assert!(cache.channel(dm_channel).await.is_none());
        assert!(cache.message(dm_channel, test_id(901)).await.is_none());
        assert!(cache
            .message(unknown_owner_channel, test_id(903))
            .await
            .is_some());
        assert_eq!(cache.users().await.len(), 2);

        // A single-shard bot owns everything: READY is a full clear.
        cache.clear_shard(0, 1).await;
        assert!(cache.guilds().await.is_empty());
        assert!(cache.users().await.is_empty());
        assert!(cache.channels().await.is_empty());
        assert!(cache
            .message(unknown_owner_channel, test_id(903))
            .await
            .is_none());

        // The CacheBackend entry point applies the same scoping.
        populate_guild_entries(&cache, shard0_guild, 100).await;
        populate_guild_entries(&cache, shard1_guild, 200).await;
        assert_eq!(
            cache
                .clear_shard_cache(0, clear_id(9, 0), 0, 2)
                .await
                .unwrap(),
            CacheClear::Applied(1)
        );
        assert_guild_entries(&cache, shard0_guild, 100, false).await;
        assert_guild_entries(&cache, shard1_guild, 200, true).await;
    }

    #[tokio::test]
    async fn cache_handle_backend_fences_writes_and_clears_with_one_store_epoch() {
        let cache = CacheHandle::new();
        let channel = test_id(10);
        let message = |id: u64| Message {
            id: test_id(id),
            channel_id: channel,
            ..Message::fixture()
        };
        let cached = |id: u64| {
            let cache = cache.clone();
            async move { cache.message(channel, test_id(id)).await.is_some() }
        };

        // An old single-shard client claims the store and writes.
        let old = match cache.clear_cache(0, clear_id(1, 0)).await.unwrap() {
            CacheClear::Applied(epoch) => epoch,
            other => panic!("{other:?}"),
        };
        cache.put_message(old, message(1001)).await.unwrap();
        assert!(cached(1001).await);

        // A replacement client with a different shard count takes over with
        // a full clear: the topology change invalidates every older epoch.
        let new = match cache.clear_cache(old, clear_id(2, 0)).await.unwrap() {
            CacheClear::Applied(epoch) => epoch,
            other => panic!("{other:?}"),
        };
        assert!(new > old);
        assert!(!cached(1001).await);
        cache.put_message(new, message(2001)).await.unwrap();
        let shard1_guild = test_id(1_u64 << 22);
        let user = test_id(5);
        cache
            .put_member(new, shard1_guild, user, crate::model::Member::default())
            .await
            .unwrap();

        // The old layout's writer can neither insert stale data nor delete
        // new-layout data.
        cache.put_message(old, message(1002)).await.unwrap();
        assert!(!cached(1002).await);
        cache
            .delete_message(old, channel, test_id(2001))
            .await
            .unwrap();
        assert!(cached(2001).await);
        cache.delete_member(old, shard1_guild, user).await.unwrap();
        cache
            .put_presence(old, shard1_guild, user, Presence::default())
            .await
            .unwrap();
        cache
            .delete_presence(old, shard1_guild, user)
            .await
            .unwrap();
        assert!(cache.member(shard1_guild, user).await.is_some());
        assert!(cache.presence(shard1_guild, user).await.is_none());

        // The old client's already-submitted clears are rejected: they
        // neither erase current data nor advance the epoch.
        assert_eq!(
            cache
                .clear_shard_cache(old, clear_id(1, 1), 0, 1)
                .await
                .unwrap(),
            CacheClear::Rejected(new)
        );
        // Re-sending the old client's own (applied) claim does not help it
        // either: the last applied clear is the replacement's.
        assert_eq!(
            cache.clear_cache(0, clear_id(1, 0)).await.unwrap(),
            CacheClear::Rejected(new)
        );
        assert_eq!(
            cache.clear_cache(old, clear_id(1, 2)).await.unwrap(),
            CacheClear::Rejected(new)
        );
        assert!(cached(2001).await);

        // A per-shard clear by the current owner only removes that shard's
        // entries and still advances the store-wide epoch.
        let after_shard_clear = match cache
            .clear_shard_cache(new, clear_id(2, 1), 1, 2)
            .await
            .unwrap()
        {
            CacheClear::Applied(epoch) => epoch,
            other => panic!("{other:?}"),
        };
        assert!(after_shard_clear > new);
        assert!(cache.member(shard1_guild, user).await.is_none());
        cache
            .put_presence(after_shard_clear, shard1_guild, user, Presence::default())
            .await
            .unwrap();
        assert!(cache.presence(shard1_guild, user).await.is_some());
        // A retry of that clear (its response was lost) gets the epoch it
        // produced and clears nothing again.
        assert_eq!(
            cache
                .clear_shard_cache(new, clear_id(2, 1), 1, 2)
                .await
                .unwrap(),
            CacheClear::Applied(after_shard_clear)
        );
        assert!(cache.presence(shard1_guild, user).await.is_some());
        cache
            .delete_presence(new, shard1_guild, user)
            .await
            .unwrap();
        assert!(
            cache.presence(shard1_guild, user).await.is_some(),
            "a write issued before the latest clear is ignored"
        );

        // The in-memory full clear keeps the epoch growing.
        cache.clear().await;
        assert_eq!(
            cache
                .clear_cache(after_shard_clear, clear_id(2, 2))
                .await
                .unwrap(),
            CacheClear::Applied(after_shard_clear + 1)
        );
    }

    #[test]
    fn shard_for_guild_id_follows_discords_formula() {
        assert_eq!(
            super::shard_for_guild_id(test_id(41_771_983_423_143_937), 4),
            u32::try_from((41_771_983_423_143_937_u64 >> 22) % 4).unwrap()
        );
        assert_eq!(
            super::shard_for_guild_id(test_id(1_u64 << 22), 0),
            0,
            "a zero shard count is treated as one shard"
        );
    }

    #[tokio::test]
    async fn cache_handle_tracks_create_and_delete_flows() {
        let cache = CacheHandle::new();
        let guild_id = test_id(1);
        let other_guild_id = test_id(2);
        let channel_id = test_id(10);
        let other_channel_id = test_id(20);
        let dm_channel_id = test_id(30);
        let user_id = test_id(11);
        let other_user_id = test_id(21);
        let message_id = test_id(12);
        let orphan_channel_id = test_id(13);
        let orphan_message_id = test_id(14);
        let other_message_id = test_id(22);
        let dm_message_id = test_id(31);
        let role_id = test_id(15);
        let other_role_id = test_id(23);

        cache
            .upsert_guild(Guild {
                id: guild_id,
                name: "discordrs".to_string(),
                ..Guild::fixture()
            })
            .await;
        cache
            .upsert_guild(Guild {
                id: other_guild_id,
                name: "other".to_string(),
                ..Guild::fixture()
            })
            .await;
        cache
            .upsert_role(
                guild_id,
                Role {
                    id: role_id,
                    name: "admin".to_string(),
                    ..Role::fixture()
                },
            )
            .await;
        cache
            .upsert_role(
                other_guild_id,
                Role {
                    id: other_role_id,
                    name: "member".to_string(),
                    ..Role::fixture()
                },
            )
            .await;
        cache
            .upsert_channel(Channel {
                id: channel_id,
                guild_id: Some(guild_id),
                kind: ChannelType::GUILD_TEXT,
                name: Some("general".to_string()),
                ..Channel::fixture()
            })
            .await;
        cache
            .upsert_channel(Channel {
                id: other_channel_id,
                guild_id: Some(other_guild_id),
                kind: ChannelType::GUILD_TEXT,
                name: Some("other-general".to_string()),
                ..Channel::fixture()
            })
            .await;
        cache
            .upsert_channel(Channel {
                id: dm_channel_id,
                kind: ChannelType::DM,
                name: Some("dm".to_string()),
                ..Channel::fixture()
            })
            .await;
        cache
            .upsert_member(
                guild_id,
                user_id,
                crate::model::Member {
                    user: Some(User {
                        id: user_id,
                        username: "discordrs".to_string(),
                        ..User::fixture()
                    }),
                    ..crate::model::Member::default()
                },
            )
            .await;
        cache
            .upsert_user(User {
                id: user_id,
                username: "discordrs".to_string(),
                ..User::fixture()
            })
            .await;
        cache
            .upsert_member(
                other_guild_id,
                other_user_id,
                crate::model::Member {
                    user: Some(User {
                        id: other_user_id,
                        username: "other".to_string(),
                        ..User::fixture()
                    }),
                    ..crate::model::Member::default()
                },
            )
            .await;
        cache
            .upsert_user(User {
                id: other_user_id,
                username: "other".to_string(),
                ..User::fixture()
            })
            .await;
        cache
            .upsert_message(Message {
                id: message_id,
                channel_id,
                guild_id: Some(guild_id),
                content: "hello".to_string(),
                ..Message::fixture()
            })
            .await;
        cache
            .upsert_message(Message {
                id: orphan_message_id,
                channel_id: orphan_channel_id,
                guild_id: Some(guild_id),
                content: "orphan".to_string(),
                ..Message::fixture()
            })
            .await;
        cache
            .upsert_message(Message {
                id: other_message_id,
                channel_id: other_channel_id,
                guild_id: Some(other_guild_id),
                content: "other".to_string(),
                ..Message::fixture()
            })
            .await;
        cache
            .upsert_message(Message {
                id: dm_message_id,
                channel_id: dm_channel_id,
                content: "dm".to_string(),
                ..Message::fixture()
            })
            .await;
        cache
            .upsert_presence(
                guild_id,
                user_id,
                Presence {
                    user_id: Some(user_id),
                    status: Some(PresenceStatus::ONLINE),
                    ..Presence::default()
                },
            )
            .await;
        cache
            .upsert_presence(
                other_guild_id,
                other_user_id,
                Presence {
                    user_id: Some(other_user_id),
                    status: Some(PresenceStatus::IDLE),
                    ..Presence::default()
                },
            )
            .await;
        cache
            .upsert_voice_state(
                guild_id,
                user_id,
                VoiceState {
                    guild_id: Some(guild_id),
                    channel_id: Some(channel_id),
                    user_id: Some(user_id),
                    ..VoiceState::default()
                },
            )
            .await;
        cache
            .upsert_voice_state(
                other_guild_id,
                other_user_id,
                VoiceState {
                    guild_id: Some(other_guild_id),
                    channel_id: Some(other_channel_id),
                    user_id: Some(other_user_id),
                    ..VoiceState::default()
                },
            )
            .await;

        assert!(cache.guild(guild_id).await.is_some());
        assert!(cache.channel(channel_id).await.is_some());
        assert!(cache.user(user_id).await.is_some());
        assert!(cache.member(guild_id, user_id).await.is_some());
        assert!(cache.message(channel_id, message_id).await.is_some());
        assert!(cache.presence(guild_id, user_id).await.is_some());
        assert!(cache.voice_state(guild_id, user_id).await.is_some());
        assert!(cache
            .message(orphan_channel_id, orphan_message_id)
            .await
            .is_some());
        assert_eq!(cache.roles(guild_id).await.len(), 1);

        cache.remove_guild(guild_id).await;
        assert!(cache.guild(guild_id).await.is_none());
        assert!(cache.channel(channel_id).await.is_none());
        assert!(cache.user(user_id).await.is_some());
        assert!(cache.member(guild_id, user_id).await.is_none());
        assert!(cache.message(channel_id, message_id).await.is_none());
        assert!(cache.presence(guild_id, user_id).await.is_none());
        assert!(cache.voice_state(guild_id, user_id).await.is_none());
        assert!(cache
            .message(orphan_channel_id, orphan_message_id)
            .await
            .is_none());
        assert!(cache.roles(guild_id).await.is_empty());
        assert!(cache.guild(other_guild_id).await.is_some());
        assert!(cache.channel(other_channel_id).await.is_some());
        assert!(cache.channel(dm_channel_id).await.is_some());
        assert!(cache.user(other_user_id).await.is_some());
        assert!(cache.member(other_guild_id, other_user_id).await.is_some());
        assert!(cache
            .message(other_channel_id, other_message_id)
            .await
            .is_some());
        assert!(cache.message(dm_channel_id, dm_message_id).await.is_some());
        assert!(cache.role(other_guild_id, other_role_id).await.is_some());
        assert!(cache
            .presence(other_guild_id, other_user_id)
            .await
            .is_some());
        assert!(cache
            .voice_state(other_guild_id, other_user_id)
            .await
            .is_some());
    }

    #[tokio::test]
    async fn cache_handle_exposes_contains_and_list_helpers() {
        let cache = CacheHandle::new();
        let guild_id = test_id(1);
        let channel_id = test_id(2);
        let message_id = test_id(3);
        let user_id = test_id(4);
        let role_id = test_id(5);

        cache
            .upsert_guild(Guild {
                id: guild_id,
                name: "discordrs".to_string(),
                ..Guild::fixture()
            })
            .await;
        cache
            .upsert_channel(Channel {
                id: channel_id,
                guild_id: Some(guild_id),
                kind: ChannelType::GUILD_TEXT,
                name: Some("general".to_string()),
                ..Channel::fixture()
            })
            .await;
        cache
            .upsert_member(
                guild_id,
                user_id,
                crate::model::Member {
                    user: Some(User {
                        id: user_id,
                        username: "discordrs".to_string(),
                        ..User::fixture()
                    }),
                    ..crate::model::Member::default()
                },
            )
            .await;
        cache
            .upsert_user(User {
                id: user_id,
                username: "discordrs".to_string(),
                ..User::fixture()
            })
            .await;
        cache
            .upsert_message(Message {
                id: message_id,
                channel_id,
                guild_id: Some(guild_id),
                content: "hello".to_string(),
                ..Message::fixture()
            })
            .await;
        cache
            .upsert_role(
                guild_id,
                Role {
                    id: role_id,
                    name: "admin".to_string(),
                    ..Role::fixture()
                },
            )
            .await;
        cache
            .upsert_presence(
                guild_id,
                user_id,
                Presence {
                    user_id: Some(user_id),
                    status: Some(PresenceStatus::ONLINE),
                    ..Presence::default()
                },
            )
            .await;
        cache
            .upsert_voice_state(
                guild_id,
                user_id,
                VoiceState {
                    guild_id: Some(guild_id),
                    channel_id: Some(channel_id),
                    user_id: Some(user_id),
                    ..VoiceState::default()
                },
            )
            .await;

        assert!(cache.contains_guild(guild_id).await);
        assert!(cache.contains_channel(channel_id).await);
        assert!(cache.contains_user(user_id).await);
        assert!(cache.contains_member(guild_id, user_id).await);
        assert!(cache.contains_message(channel_id, message_id).await);
        assert!(cache.contains_role(guild_id, role_id).await);
        assert!(cache.contains_presence(guild_id, user_id).await);
        assert!(cache.contains_voice_state(guild_id, user_id).await);
        assert_eq!(cache.guilds().await.len(), 1);
        assert_eq!(cache.channels().await.len(), 1);
        assert_eq!(cache.users().await.len(), 1);
        assert_eq!(cache.members(guild_id).await.len(), 1);
        assert_eq!(cache.messages(channel_id).await.len(), 1);
        assert_eq!(cache.roles(guild_id).await.len(), 1);
        assert_eq!(cache.presences(guild_id).await.len(), 1);
        assert_eq!(cache.voice_states(guild_id).await.len(), 1);

        cache.clear().await;
        assert!(cache.guilds().await.is_empty());
        assert!(cache.channels().await.is_empty());
        assert!(cache.users().await.is_empty());
        assert!(cache.presences(guild_id).await.is_empty());
        assert!(cache.voice_states(guild_id).await.is_empty());
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
        let guild_id = test_id(1);
        let channel_id = test_id(10);
        let other_channel_id = test_id(20);

        for id in [100, 101, 102] {
            cache
                .upsert_message(Message {
                    id: test_id(id),
                    channel_id,
                    guild_id: Some(guild_id),
                    content: id.to_string(),
                    ..Message::fixture()
                })
                .await;
        }
        assert!(cache.message(channel_id, test_id(100)).await.is_none());
        assert_eq!(cache.messages(channel_id).await.len(), 2);

        for id in [200, 201] {
            cache
                .upsert_message(Message {
                    id: test_id(id),
                    channel_id: other_channel_id,
                    guild_id: Some(guild_id),
                    content: id.to_string(),
                    ..Message::fixture()
                })
                .await;
        }
        let total_messages =
            cache.messages(channel_id).await.len() + cache.messages(other_channel_id).await.len();
        assert_eq!(total_messages, 3);

        for id in [300, 301, 302] {
            let user_id = test_id(id);
            cache
                .upsert_presence(
                    guild_id,
                    user_id,
                    Presence {
                        user_id: Some(user_id),
                        status: Some(PresenceStatus::ONLINE),
                        ..Presence::default()
                    },
                )
                .await;
        }
        assert_eq!(cache.presences(guild_id).await.len(), 2);
        assert!(cache.presence(guild_id, test_id(300)).await.is_none());

        for id in [400, 401, 402] {
            let user_id = test_id(id);
            cache
                .upsert_member(
                    guild_id,
                    user_id,
                    crate::model::Member {
                        user: Some(User {
                            id: user_id,
                            username: id.to_string(),
                            ..User::fixture()
                        }),
                        ..crate::model::Member::default()
                    },
                )
                .await;
        }
        assert_eq!(cache.members(guild_id).await.len(), 2);
        assert!(cache.member(guild_id, test_id(400)).await.is_none());
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
        let guild_id = test_id(1);
        let channel_id = test_id(10);
        let user_id = test_id(20);
        let message_id = test_id(30);

        cache
            .upsert_message(Message {
                id: message_id,
                channel_id,
                guild_id: Some(guild_id),
                content: "expired".to_string(),
                ..Message::fixture()
            })
            .await;
        cache
            .upsert_presence(
                guild_id,
                user_id,
                Presence {
                    user_id: Some(user_id),
                    status: Some(PresenceStatus::ONLINE),
                    ..Presence::default()
                },
            )
            .await;
        cache
            .upsert_member(
                guild_id,
                user_id,
                crate::model::Member {
                    user: Some(User {
                        id: user_id,
                        username: "expired".to_string(),
                        ..User::fixture()
                    }),
                    ..crate::model::Member::default()
                },
            )
            .await;

        cache.purge_expired().await;

        assert!(cache.message(channel_id, message_id).await.is_none());
        assert!(cache.presence(guild_id, user_id).await.is_none());
        assert!(cache.member(guild_id, user_id).await.is_none());
        assert!(cache.messages(channel_id).await.is_empty());
        assert!(cache.presences(guild_id).await.is_empty());
        assert!(cache.members(guild_id).await.is_empty());
    }

    #[tokio::test]
    async fn cache_read_paths_return_fresh_message_presence_and_member_entries() {
        let cache = CacheHandle::with_config(
            CacheConfig::unbounded()
                .message_ttl(Duration::from_secs(60))
                .presence_ttl(Duration::from_secs(60))
                .member_ttl(Duration::from_secs(60)),
        );
        let guild_id = test_id(1);
        let channel_id = test_id(10);
        let user_id = test_id(20);
        let message_id = test_id(30);

        cache
            .upsert_message(Message {
                id: message_id,
                channel_id,
                guild_id: Some(guild_id),
                content: "fresh".to_string(),
                ..Message::fixture()
            })
            .await;
        cache
            .upsert_presence(
                guild_id,
                user_id,
                Presence {
                    user_id: Some(user_id),
                    status: Some(PresenceStatus::ONLINE),
                    ..Presence::default()
                },
            )
            .await;
        cache
            .upsert_member(
                guild_id,
                user_id,
                crate::model::Member {
                    user: Some(User {
                        id: user_id,
                        username: "fresh".to_string(),
                        ..User::fixture()
                    }),
                    ..crate::model::Member::default()
                },
            )
            .await;

        assert!(cache.message(channel_id, message_id).await.is_some());
        assert_eq!(cache.messages(channel_id).await.len(), 1);
        assert!(cache.presence(guild_id, user_id).await.is_some());
        assert_eq!(cache.presences(guild_id).await.len(), 1);
        assert!(cache.member(guild_id, user_id).await.is_some());
        assert_eq!(cache.members(guild_id).await.len(), 1);
    }

    #[tokio::test]
    async fn cache_arc_read_paths_share_hot_cached_entries_without_breaking_owned_reads() {
        let cache = CacheHandle::with_config(
            CacheConfig::unbounded()
                .message_ttl(Duration::from_secs(60))
                .presence_ttl(Duration::from_secs(60))
                .member_ttl(Duration::from_secs(60)),
        );
        let guild_id = test_id(1);
        let channel_id = test_id(10);
        let user_id = test_id(20);
        let message_id = test_id(30);

        cache
            .upsert_message(Message {
                id: message_id,
                channel_id,
                content: "arc message".to_string(),
                ..Message::fixture()
            })
            .await;
        cache
            .upsert_presence(
                guild_id,
                user_id,
                Presence {
                    user_id: Some(user_id),
                    status: Some(PresenceStatus::ONLINE),
                    ..Presence::default()
                },
            )
            .await;
        cache
            .upsert_member(
                guild_id,
                user_id,
                crate::model::Member {
                    user: Some(User {
                        id: user_id,
                        username: "arc member".to_string(),
                        ..User::fixture()
                    }),
                    ..crate::model::Member::default()
                },
            )
            .await;

        let message = cache.message_arc(channel_id, message_id).await.unwrap();
        let same_message = cache.message_arc(channel_id, message_id).await.unwrap();
        assert!(Arc::ptr_eq(&message, &same_message));
        assert_eq!(
            cache.messages_arc(channel_id).await[0].content,
            "arc message"
        );
        assert_eq!(
            cache.message(channel_id, message_id).await.unwrap().content,
            "arc message"
        );

        let presence = cache.presence_arc(guild_id, user_id).await.unwrap();
        let same_presence = cache.presence_arc(guild_id, user_id).await.unwrap();
        assert!(Arc::ptr_eq(&presence, &same_presence));
        assert_eq!(
            cache.presences_arc(guild_id).await[0].status.as_deref(),
            Some("online")
        );
        assert_eq!(
            cache
                .presence(guild_id, user_id)
                .await
                .unwrap()
                .status
                .as_deref(),
            Some("online")
        );

        let member = cache.member_arc(guild_id, user_id).await.unwrap();
        let same_member = cache.member_arc(guild_id, user_id).await.unwrap();
        assert!(Arc::ptr_eq(&member, &same_member));
        assert_eq!(
            cache.members_arc(guild_id).await[0]
                .user
                .as_ref()
                .unwrap()
                .username,
            "arc member"
        );
        assert_eq!(
            cache
                .member(guild_id, user_id)
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
            epoch: u64,
            guild_id: GuildId,
            channel_id: ChannelId,
            user_id: UserId,
            message_id: MessageId,
        ) {
            backend
                .put_member(
                    epoch,
                    guild_id,
                    user_id,
                    crate::model::Member {
                        user: Some(User {
                            id: user_id,
                            username: "backend member".to_string(),
                            ..User::fixture()
                        }),
                        ..crate::model::Member::default()
                    },
                )
                .await
                .unwrap();
            backend
                .put_message(
                    epoch,
                    Message {
                        id: message_id,
                        channel_id,
                        content: "backend message".to_string(),
                        ..Message::fixture()
                    },
                )
                .await
                .unwrap();
            backend
                .put_presence(
                    epoch,
                    guild_id,
                    user_id,
                    Presence {
                        user_id: Some(user_id),
                        status: Some(PresenceStatus::IDLE),
                        ..Presence::default()
                    },
                )
                .await
                .unwrap();

            assert_eq!(
                backend
                    .get_member(guild_id, user_id)
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
                    .get_message(channel_id, message_id)
                    .await
                    .unwrap()
                    .unwrap()
                    .content,
                "backend message"
            );
            assert_eq!(
                backend
                    .get_presence(guild_id, user_id)
                    .await
                    .unwrap()
                    .unwrap()
                    .status
                    .as_deref(),
                Some("idle")
            );
            assert_eq!(backend.list_members(guild_id).await.unwrap().len(), 1);
            assert_eq!(backend.list_messages(channel_id).await.unwrap().len(), 1);
            assert_eq!(backend.list_presences(guild_id).await.unwrap().len(), 1);

            backend
                .delete_message(epoch, channel_id, message_id)
                .await
                .unwrap();
            backend
                .delete_presence(epoch, guild_id, user_id)
                .await
                .unwrap();
            backend
                .delete_member(epoch, guild_id, user_id)
                .await
                .unwrap();
            assert!(backend
                .get_message(channel_id, message_id)
                .await
                .unwrap()
                .is_none());
            assert!(backend
                .get_presence(guild_id, user_id)
                .await
                .unwrap()
                .is_none());
            assert!(backend
                .get_member(guild_id, user_id)
                .await
                .unwrap()
                .is_none());
        }

        let cache = CacheHandle::new();
        exercise_backend(&cache, 0, test_id(1), test_id(10), test_id(20), test_id(30)).await;

        CacheBackend::put_message(
            &cache,
            0,
            Message {
                id: test_id(31),
                channel_id: test_id(10),
                content: "clear me".to_string(),
                ..Message::fixture()
            },
        )
        .await
        .unwrap();
        cache.purge_expired_entries().await.unwrap();
        assert_eq!(
            cache.clear_cache(0, clear_id(9, 0)).await.unwrap(),
            CacheClear::Applied(1)
        );
        assert!(cache.message_arc(test_id(10), test_id(31)).await.is_none());
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
            let guild_id: GuildId = format!("1{suffix}").parse().unwrap();
            let channel_id: ChannelId = format!("10{suffix}").parse().unwrap();
            let user_id: UserId = format!("20{suffix}").parse().unwrap();
            let message_id: MessageId = format!("30{suffix}").parse().unwrap();

            cache
                .upsert_message(Message {
                    id: message_id,
                    channel_id,
                    guild_id: Some(guild_id),
                    content: "soon-expired".to_string(),
                    ..Message::fixture()
                })
                .await;
            cache
                .upsert_presence(
                    guild_id,
                    user_id,
                    Presence {
                        user_id: Some(user_id),
                        status: Some(PresenceStatus::ONLINE),
                        ..Presence::default()
                    },
                )
                .await;
            cache
                .upsert_member(
                    guild_id,
                    user_id,
                    crate::model::Member {
                        user: Some(User {
                            id: user_id,
                            username: "soon-expired".to_string(),
                            ..User::fixture()
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
            .message(channel_id, message_id)
            .await
            .is_none());
        assert!(presence_cache
            .presence(presence_guild_id, presence_user_id)
            .await
            .is_none());
        assert!(member_cache
            .member(member_guild_id, member_user_id)
            .await
            .is_none());
        assert!(messages_cache
            .messages(messages_channel_id)
            .await
            .is_empty());
        assert!(presences_cache
            .presences(presences_guild_id)
            .await
            .is_empty());
        assert!(members_cache.members(members_guild_id).await.is_empty());
    }

    #[tokio::test]
    async fn member_cap_counters_evict_per_guild_without_touching_other_guilds() {
        let cache = CacheHandle::with_config(CacheConfig::unbounded().max_members_per_guild(2));
        let guild_a = test_id(1);
        let guild_b = test_id(2);

        let upsert = |guild: GuildId, id: u64| {
            let cache = cache.clone();
            let user_id = test_id(id);
            async move {
                cache
                    .upsert_member(
                        guild,
                        user_id,
                        crate::model::Member {
                            user: Some(User {
                                id: user_id,
                                ..User::fixture()
                            }),
                            ..crate::model::Member::default()
                        },
                    )
                    .await;
            }
        };

        upsert(guild_a, 10).await;
        upsert(guild_a, 11).await;
        upsert(guild_b, 20).await;
        upsert(guild_b, 21).await;
        upsert(guild_b, 22).await;

        // Guild B is over its cap: its oldest member is evicted; guild A is
        // untouched even though the global member map holds more entries.
        assert!(cache.member(guild_b, test_id(20)).await.is_none());
        assert!(cache.member(guild_b, test_id(21)).await.is_some());
        assert!(cache.member(guild_b, test_id(22)).await.is_some());
        assert_eq!(cache.members(guild_a).await.len(), 2);
        assert_eq!(cache.members(guild_b).await.len(), 2);

        {
            let store = cache.store.read().await;
            assert_eq!(store.member_counts.get(&guild_a), Some(&2));
            assert_eq!(store.member_counts.get(&guild_b), Some(&2));
        }

        // Replacing an existing key must not double-count or evict, and must
        // refresh its LRU position.
        upsert(guild_a, 10).await;
        assert_eq!(cache.members(guild_a).await.len(), 2);
        {
            let store = cache.store.read().await;
            assert_eq!(store.member_counts.get(&guild_a), Some(&2));
        }

        // A new member for guild A now evicts "11" (oldest after the touch),
        // not the re-upserted "10".
        upsert(guild_a, 12).await;
        assert!(cache.member(guild_a, test_id(11)).await.is_none());
        assert!(cache.member(guild_a, test_id(10)).await.is_some());
        assert!(cache.member(guild_a, test_id(12)).await.is_some());

        // Removing a guild's entries drops its counter entirely.
        cache.remove_guild(guild_b).await;
        {
            let store = cache.store.read().await;
            assert_eq!(store.member_counts.get(&guild_b), None);
            assert_eq!(store.member_counts.get(&guild_a), Some(&2));
        }
    }

    #[tokio::test]
    async fn message_upsert_replacing_existing_key_does_not_corrupt_counts() {
        let cache = CacheHandle::with_config(CacheConfig::unbounded().max_messages_per_channel(2));
        let channel_id = test_id(10);

        let upsert = |id: u64| {
            let cache = cache.clone();
            let message_id = test_id(id);
            async move {
                cache
                    .upsert_message(Message {
                        id: message_id,
                        channel_id,
                        content: "hi".to_string(),
                        ..Message::fixture()
                    })
                    .await;
            }
        };

        upsert(100).await;
        upsert(101).await;
        // Replace the same key repeatedly: nothing may be evicted and the
        // per-channel counter must stay at 2.
        upsert(100).await;
        upsert(100).await;
        assert_eq!(cache.messages(channel_id).await.len(), 2);
        {
            let store = cache.store.read().await;
            assert_eq!(store.message_counts.get(&channel_id), Some(&2));
            assert_eq!(store.messages.len(), 2);
            assert_eq!(store.message_order.len(), 2);
        }

        // The replacement refreshed "100", so the next insert evicts "101".
        upsert(102).await;
        assert!(cache.message(channel_id, test_id(101)).await.is_none());
        assert!(cache.message(channel_id, test_id(100)).await.is_some());
        assert!(cache.message(channel_id, test_id(102)).await.is_some());
        {
            let store = cache.store.read().await;
            assert_eq!(store.message_counts.get(&channel_id), Some(&2));
        }
    }

    #[tokio::test]
    async fn upsert_paths_throttle_full_prune_but_purge_expired_sweeps_immediately() {
        let cache = CacheHandle::with_config(
            CacheConfig::unbounded().message_ttl(Duration::from_millis(50)),
        );
        let channel_id = test_id(10);
        let first_id = test_id(100);
        let second_id = test_id(101);

        cache
            .upsert_message(Message {
                id: first_id,
                channel_id,
                content: "first".to_string(),
                ..Message::fixture()
            })
            .await;
        tokio::time::sleep(Duration::from_millis(60)).await;
        cache
            .upsert_message(Message {
                id: second_id,
                channel_id,
                content: "second".to_string(),
                ..Message::fixture()
            })
            .await;

        // The second upsert happened within the throttle window, so the
        // expired first message is still physically present...
        {
            let store = cache.store.read().await;
            assert_eq!(store.messages.len(), 2);
            assert_eq!(store.message_counts.get(&channel_id), Some(&2));
        }
        // ...but never visible through the read paths.
        assert!(cache.message(channel_id, first_id).await.is_none());
        assert_eq!(cache.messages(channel_id).await.len(), 1);

        // An explicit purge runs the unthrottled sweep and reclaims it.
        cache.purge_expired().await;
        {
            let store = cache.store.read().await;
            assert_eq!(store.messages.len(), 1);
            assert_eq!(store.message_counts.get(&channel_id), Some(&1));
            assert_eq!(store.message_order.len(), 1);
            assert_eq!(store.message_seen.len(), 1);
        }
        // The fresh second message survives the sweep and stays readable.
        assert!(cache.message(channel_id, second_id).await.is_some());
        assert_eq!(cache.messages(channel_id).await.len(), 1);
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
        let guild_id = test_id(1);

        for id in [1, 2] {
            cache
                .upsert_guild(Guild {
                    id: test_id(id),
                    name: format!("guild-{id}"),
                    ..Guild::fixture()
                })
                .await;
        }
        assert!(cache.guild(test_id(1)).await.is_none());
        assert!(cache.guild(test_id(2)).await.is_some());

        for id in [10, 11] {
            cache
                .upsert_channel(Channel {
                    id: test_id(id),
                    guild_id: Some(guild_id),
                    kind: ChannelType::GUILD_TEXT,
                    ..Channel::fixture()
                })
                .await;
        }
        assert!(cache.channel(test_id(10)).await.is_none());
        assert!(cache.channel(test_id(11)).await.is_some());

        for id in [20, 21] {
            cache
                .upsert_user(User {
                    id: test_id(id),
                    username: format!("user-{id}"),
                    ..User::fixture()
                })
                .await;
        }
        assert!(cache.user(test_id(20)).await.is_none());
        assert!(cache.user(test_id(21)).await.is_some());

        for id in [30, 31] {
            cache
                .upsert_role(
                    guild_id,
                    Role {
                        id: test_id(id),
                        name: format!("role-{id}"),
                        ..Role::fixture()
                    },
                )
                .await;
        }
        assert!(cache.role(guild_id, test_id(30)).await.is_none());
        assert!(cache.role(guild_id, test_id(31)).await.is_some());

        for id in [40, 41] {
            cache
                .upsert_voice_state(
                    guild_id,
                    test_id(id),
                    VoiceState {
                        user_id: Some(test_id(id)),
                        guild_id: Some(guild_id),
                        ..VoiceState::default()
                    },
                )
                .await;
        }
        assert!(cache.voice_state(guild_id, test_id(40)).await.is_none());
        assert!(cache.voice_state(guild_id, test_id(41)).await.is_some());

        for id in [50, 51] {
            cache
                .upsert_soundboard_sound(
                    guild_id,
                    SoundboardSound {
                        name: format!("sound-{id}"),
                        sound_id: test_id(id),
                        volume: 1.0,
                        ..SoundboardSound::fixture()
                    },
                )
                .await;
        }
        assert!(cache
            .soundboard_sound(guild_id, test_id(50))
            .await
            .is_none());
        assert!(cache
            .soundboard_sound(guild_id, test_id(51))
            .await
            .is_some());
        assert_eq!(cache.soundboard_sounds(guild_id).await.len(), 1);
        assert!(cache.contains_soundboard_sound(guild_id, test_id(51)).await);

        cache
            .replace_emojis(
                guild_id,
                vec![
                    Emoji::custom("first", test_id(60), false),
                    Emoji::custom("second", test_id(61), false),
                ],
            )
            .await;
        assert!(cache.emoji(guild_id, test_id(60)).await.is_none());
        assert!(cache.emoji(guild_id, test_id(61)).await.is_some());
        assert_eq!(cache.emojis(guild_id).await.len(), 1);

        cache
            .replace_stickers(
                guild_id,
                vec![
                    Sticker {
                        id: test_id(70),
                        name: "first".to_string(),
                        ..Sticker::fixture()
                    },
                    Sticker {
                        id: test_id(71),
                        name: "second".to_string(),
                        ..Sticker::fixture()
                    },
                ],
            )
            .await;
        assert!(cache.sticker(guild_id, test_id(70)).await.is_none());
        assert!(cache.sticker(guild_id, test_id(71)).await.is_some());
        assert_eq!(cache.stickers(guild_id).await.len(), 1);

        for id in [80, 81] {
            cache
                .upsert_scheduled_event(ScheduledEvent {
                    id: Some(test_id(id)),
                    guild_id: Some(guild_id),
                    name: Some(format!("event-{id}")),
                    ..ScheduledEvent::default()
                })
                .await;
        }
        assert!(cache.scheduled_event(guild_id, test_id(80)).await.is_none());
        assert!(cache.scheduled_event(guild_id, test_id(81)).await.is_some());
        assert_eq!(cache.scheduled_events(guild_id).await.len(), 1);

        for id in [90, 91] {
            cache
                .upsert_stage_instance(StageInstance {
                    id: test_id(id),
                    guild_id,
                    channel_id: test_id(11),
                    topic: format!("stage-{id}"),
                    privacy_level: StageInstancePrivacyLevel::GUILD_ONLY,
                    ..StageInstance::fixture()
                })
                .await;
        }
        assert!(cache.stage_instance(guild_id, test_id(90)).await.is_none());
        assert!(cache.stage_instance(guild_id, test_id(91)).await.is_some());
        assert_eq!(cache.stage_instances(guild_id).await.len(), 1);
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
        let guild_id = test_id(1);

        cache
            .replace_emojis(guild_id, vec![Emoji::custom("one", test_id(10), false)])
            .await;
        cache
            .replace_stickers(
                guild_id,
                vec![Sticker {
                    id: test_id(20),
                    name: "sticker".to_string(),
                    ..Sticker::fixture()
                }],
            )
            .await;
        cache
            .upsert_scheduled_event(ScheduledEvent {
                id: Some(test_id(30)),
                guild_id: Some(guild_id),
                name: Some("event".to_string()),
                ..ScheduledEvent::default()
            })
            .await;
        cache
            .upsert_stage_instance(StageInstance {
                id: test_id(40),
                guild_id,
                channel_id: test_id(50),
                topic: "stage".to_string(),
                privacy_level: StageInstancePrivacyLevel::GUILD_ONLY,
                ..StageInstance::fixture()
            })
            .await;

        assert!(cache.emojis(guild_id).await.is_empty());
        assert!(cache.stickers(guild_id).await.is_empty());
        assert!(cache.scheduled_events(guild_id).await.is_empty());
        assert!(cache.stage_instances(guild_id).await.is_empty());
    }

    #[tokio::test]
    async fn remove_channel_cascades_messages_and_bulk_delete_only_targets_selected_ids() {
        let cache = CacheHandle::new();
        let channel_id = test_id(2);
        let other_channel_id = test_id(3);
        let first_message_id = test_id(10);
        let second_message_id = test_id(11);
        let untouched_message_id = test_id(12);

        cache
            .upsert_channel(Channel {
                id: channel_id,
                kind: ChannelType::GUILD_TEXT,
                name: Some("general".to_string()),
                ..Channel::fixture()
            })
            .await;
        cache
            .upsert_channel(Channel {
                id: other_channel_id,
                kind: ChannelType::GUILD_TEXT,
                name: Some("random".to_string()),
                ..Channel::fixture()
            })
            .await;
        for (message_id, stored_channel_id) in [
            (first_message_id, channel_id),
            (second_message_id, channel_id),
            (untouched_message_id, other_channel_id),
        ] {
            cache
                .upsert_message(Message {
                    id: message_id,
                    channel_id: stored_channel_id,
                    content: "hello".to_string(),
                    ..Message::fixture()
                })
                .await;
        }

        cache
            .remove_messages_bulk(channel_id, std::slice::from_ref(&first_message_id))
            .await;
        assert!(cache.message(channel_id, first_message_id).await.is_none());
        assert!(cache.message(channel_id, second_message_id).await.is_some());
        assert!(cache
            .message(other_channel_id, untouched_message_id)
            .await
            .is_some());

        cache.remove_channel(channel_id).await;
        assert!(cache.channel(channel_id).await.is_none());
        assert!(cache.message(channel_id, second_message_id).await.is_none());
        assert!(cache
            .message(other_channel_id, untouched_message_id)
            .await
            .is_some());
    }

    #[tokio::test]
    async fn cache_handle_removes_individual_entries_without_touching_other_guild_data() {
        let cache = CacheHandle::new();
        let guild_id = test_id(1);
        let other_guild_id = test_id(2);
        let channel_id = test_id(10);
        let other_channel_id = test_id(20);
        let user_id = test_id(11);
        let other_user_id = test_id(21);
        let message_id = test_id(12);
        let other_message_id = test_id(22);
        let role_id = test_id(13);
        let other_role_id = test_id(23);

        for (id, name) in [(guild_id, "discordrs"), (other_guild_id, "other")] {
            cache
                .upsert_guild(Guild {
                    id,
                    name: name.to_string(),
                    ..Guild::fixture()
                })
                .await;
        }

        for (id, guild, name) in [
            (channel_id, Some(guild_id), "general"),
            (other_channel_id, Some(other_guild_id), "other-general"),
        ] {
            cache
                .upsert_channel(Channel {
                    id,
                    guild_id: guild,
                    kind: ChannelType::GUILD_TEXT,
                    name: Some(name.to_string()),
                    ..Channel::fixture()
                })
                .await;
        }

        for (guild, user, username) in [
            (guild_id, user_id, "discordrs"),
            (other_guild_id, other_user_id, "other"),
        ] {
            cache
                .upsert_member(
                    guild,
                    user,
                    crate::model::Member {
                        user: Some(User {
                            id: user,
                            username: username.to_string(),
                            ..User::fixture()
                        }),
                        ..crate::model::Member::default()
                    },
                )
                .await;
        }

        for (message_id, channel_id, guild_id, content) in [
            (message_id, channel_id, Some(guild_id), "hello"),
            (
                other_message_id,
                other_channel_id,
                Some(other_guild_id),
                "other",
            ),
        ] {
            cache
                .upsert_message(Message {
                    id: message_id,
                    channel_id,
                    guild_id,
                    content: content.to_string(),
                    ..Message::fixture()
                })
                .await;
        }

        for (guild_id, role_id, name) in [
            (guild_id, role_id, "admin"),
            (other_guild_id, other_role_id, "member"),
        ] {
            cache
                .upsert_role(
                    guild_id,
                    Role {
                        id: role_id,
                        name: name.to_string(),
                        ..Role::fixture()
                    },
                )
                .await;
        }

        assert_eq!(cache.members(guild_id).await.len(), 1);
        assert_eq!(cache.messages(channel_id).await.len(), 1);
        assert_eq!(cache.roles(guild_id).await.len(), 1);

        cache.remove_member(guild_id, user_id).await;
        cache.remove_message(channel_id, message_id).await;
        cache.remove_role(guild_id, role_id).await;

        assert!(cache.member(guild_id, user_id).await.is_none());
        assert!(cache.message(channel_id, message_id).await.is_none());
        assert!(cache.role(guild_id, role_id).await.is_none());
        assert!(!cache.contains_member(guild_id, user_id).await);
        assert!(!cache.contains_message(channel_id, message_id).await);
        assert!(!cache.contains_role(guild_id, role_id).await);
        assert!(cache.members(guild_id).await.is_empty());
        assert!(cache.messages(channel_id).await.is_empty());
        assert!(cache.roles(guild_id).await.is_empty());

        assert!(cache.member(other_guild_id, other_user_id).await.is_some());
        assert!(cache
            .message(other_channel_id, other_message_id)
            .await
            .is_some());
        assert!(cache.role(other_guild_id, other_role_id).await.is_some());
    }

    #[cfg(feature = "gateway")]
    #[tokio::test]
    async fn managers_return_cached_values_without_hitting_http() {
        let cache = CacheHandle::new();
        let http = Arc::new(DiscordHttpClient::new("token", Some(test_id(1))));
        let guild_id = test_id(100);
        let channel_id = test_id(200);
        let user_id = test_id(300);
        let message_id = test_id(400);
        let role_id = test_id(500);

        let guild = Guild {
            id: guild_id,
            name: "discordrs".to_string(),
            ..Guild::fixture()
        };
        let channel = Channel {
            id: channel_id,
            guild_id: Some(guild_id),
            kind: ChannelType::GUILD_TEXT,
            name: Some("general".to_string()),
            ..Channel::fixture()
        };
        let member = crate::model::Member {
            user: Some(User {
                id: user_id,
                username: "discordrs".to_string(),
                ..User::fixture()
            }),
            ..crate::model::Member::default()
        };
        let user = User {
            id: user_id,
            username: "discordrs".to_string(),
            ..User::fixture()
        };
        let message = Message {
            id: message_id,
            channel_id,
            guild_id: Some(guild_id),
            content: "cached".to_string(),
            ..Message::fixture()
        };
        let role = Role {
            id: role_id,
            name: "admin".to_string(),
            ..Role::fixture()
        };

        cache.upsert_guild(guild.clone()).await;
        cache.upsert_channel(channel.clone()).await;
        cache.upsert_user(user.clone()).await;
        cache.upsert_member(guild_id, user_id, member.clone()).await;
        cache.upsert_message(message.clone()).await;
        cache.upsert_role(guild_id, role.clone()).await;

        let guild_manager = GuildManager::new(Arc::clone(&http), cache.clone());
        let channel_manager = ChannelManager::new(Arc::clone(&http), cache.clone());
        let user_manager = UserManager::new(Arc::clone(&http), cache.clone());
        let member_manager = MemberManager::new(Arc::clone(&http), cache.clone());
        let message_manager = MessageManager::new(Arc::clone(&http), cache.clone());
        let role_manager = RoleManager::new(http, cache.clone());

        assert_eq!(guild_manager.get(guild_id).await.unwrap().name, "discordrs");
        assert_eq!(
            channel_manager
                .get(channel_id)
                .await
                .unwrap()
                .name
                .as_deref(),
            Some("general")
        );
        assert_eq!(
            member_manager
                .get(guild_id, user_id)
                .await
                .unwrap()
                .user
                .as_ref()
                .map(|user| user.username.as_str()),
            Some("discordrs")
        );
        assert_eq!(
            user_manager.get(user_id).await.unwrap().username,
            "discordrs"
        );
        assert_eq!(
            message_manager
                .get(channel_id, message_id)
                .await
                .unwrap()
                .content,
            "cached"
        );
        assert_eq!(role_manager.list(guild_id).await.unwrap().len(), 1);

        assert!(guild_manager.contains(guild_id).await);
        assert!(channel_manager.contains(channel_id).await);
        assert!(user_manager.contains(user_id).await);
        assert!(member_manager.contains(guild_id, user_id).await);
        assert!(message_manager.contains(channel_id, message_id).await);
        assert!(role_manager.contains(guild_id, role_id).await);

        assert_eq!(guild_manager.cached(guild_id).await.unwrap().id, guild_id);
        assert_eq!(
            channel_manager.cached(channel_id).await.unwrap().id,
            channel_id
        );
        assert_eq!(user_manager.cached(user_id).await.unwrap().id, user_id);
        assert_eq!(
            member_manager
                .cached(guild_id, user_id)
                .await
                .unwrap()
                .user
                .as_ref()
                .map(|user| user.id),
            Some(user_id)
        );
        let member_arc = member_manager.cached_arc(guild_id, user_id).await.unwrap();
        assert_eq!(member_arc.user.as_ref().map(|user| user.id), Some(user_id));
        assert_eq!(
            message_manager
                .cached(channel_id, message_id)
                .await
                .unwrap()
                .id,
            message_id
        );
        let message_arc = message_manager
            .cached_arc(channel_id, message_id)
            .await
            .unwrap();
        assert_eq!(message_arc.id, message_id);
        assert_eq!(
            role_manager.cached(guild_id, role_id).await.unwrap().id,
            role_id
        );

        assert_eq!(guild_manager.list_cached().await.len(), 1);
        assert_eq!(channel_manager.list_cached().await.len(), 1);
        assert_eq!(user_manager.list_cached().await.len(), 1);
        assert_eq!(member_manager.list_cached(guild_id).await.len(), 1);
        assert_eq!(member_manager.list_cached_arc(guild_id).await.len(), 1);
        assert_eq!(message_manager.list_cached(channel_id).await.len(), 1);
        assert_eq!(message_manager.list_cached_arc(channel_id).await.len(), 1);
        assert_eq!(role_manager.list_cached(guild_id).await.len(), 1);
    }

    #[cfg(feature = "gateway")]
    #[tokio::test]
    async fn cached_manager_trait_impls_delegate_to_cache_for_hits() {
        let cache = CacheHandle::new();
        let http = Arc::new(DiscordHttpClient::new("token", Some(test_id(1))));
        let guild_id = test_id(701);
        let channel_id = test_id(702);
        let user_id = test_id(703);

        cache
            .upsert_guild(Guild {
                id: guild_id,
                name: "guild".to_string(),
                ..Guild::fixture()
            })
            .await;
        cache
            .upsert_channel(Channel {
                id: channel_id,
                guild_id: Some(guild_id),
                kind: ChannelType::GUILD_TEXT,
                name: Some("cached-channel".to_string()),
                ..Channel::fixture()
            })
            .await;
        cache
            .upsert_user(User {
                id: user_id,
                username: "cached-user".to_string(),
                ..User::fixture()
            })
            .await;

        let guild_manager = GuildManager::new(Arc::clone(&http), cache.clone());
        let channel_manager = ChannelManager::new(Arc::clone(&http), cache.clone());
        let user_manager = UserManager::new(http, cache);

        assert_eq!(
            <GuildManager as CachedManager<Guild>>::get(&guild_manager, guild_id)
                .await
                .unwrap()
                .name,
            "guild"
        );
        assert_eq!(
            <GuildManager as CachedManager<Guild>>::cached(&guild_manager, guild_id)
                .await
                .unwrap()
                .id,
            guild_id
        );
        assert!(<GuildManager as CachedManager<Guild>>::contains(&guild_manager, guild_id).await);
        assert_eq!(
            <GuildManager as CachedManager<Guild>>::list_cached(&guild_manager)
                .await
                .len(),
            1
        );

        assert_eq!(
            <ChannelManager as CachedManager<Channel>>::get(&channel_manager, channel_id)
                .await
                .unwrap()
                .name
                .as_deref(),
            Some("cached-channel")
        );
        assert_eq!(
            <ChannelManager as CachedManager<Channel>>::cached(&channel_manager, channel_id)
                .await
                .unwrap()
                .id,
            channel_id
        );
        assert!(
            <ChannelManager as CachedManager<Channel>>::contains(&channel_manager, channel_id)
                .await
        );
        assert_eq!(
            <ChannelManager as CachedManager<Channel>>::list_cached(&channel_manager)
                .await
                .len(),
            1
        );

        assert_eq!(
            <UserManager as CachedManager<User>>::get(&user_manager, user_id)
                .await
                .unwrap()
                .username,
            "cached-user"
        );
        assert_eq!(
            <UserManager as CachedManager<User>>::cached(&user_manager, user_id)
                .await
                .unwrap()
                .id,
            user_id
        );
        assert!(<UserManager as CachedManager<User>>::contains(&user_manager, user_id).await);
        assert_eq!(
            <UserManager as CachedManager<User>>::list_cached(&user_manager)
                .await
                .len(),
            1
        );
    }
}

#[cfg(all(test, feature = "cache"))]
mod sweep_interval_tests {
    use crate::model::test_id;
    use std::time::Duration;

    use super::{CacheConfig, CacheHandle};
    use crate::model::Message;

    #[test]
    fn cache_config_sweep_interval_defaults_and_overrides() {
        assert_eq!(
            CacheConfig::bounded().sweep_interval,
            Duration::from_secs(5)
        );
        assert_eq!(
            CacheConfig::unbounded().sweep_interval,
            Duration::from_secs(5)
        );
        let config = CacheConfig::default().sweep_interval(Duration::from_millis(50));
        assert_eq!(config.sweep_interval, Duration::from_millis(50));
    }

    /// Upserts one message, waits past its TTL, upserts another, and
    /// reports whether the first message entry is still physically stored
    /// (per-entry read expiry aside).
    async fn expired_entry_survives_second_upsert(sweep_interval: Duration) -> bool {
        let cache = CacheHandle::with_config(
            CacheConfig::default()
                .message_ttl(Duration::from_millis(1))
                .sweep_interval(sweep_interval),
        );
        let channel_id = test_id(10);

        cache
            .upsert_message(Message {
                id: test_id(1),
                channel_id,
                content: "first".to_string(),
                ..Message::fixture()
            })
            .await;
        tokio::time::sleep(Duration::from_millis(10)).await;
        cache
            .upsert_message(Message {
                id: test_id(2),
                channel_id,
                content: "second".to_string(),
                ..Message::fixture()
            })
            .await;

        let store = cache.store.read().await;
        store.messages.contains_key(&(channel_id, test_id(1)))
    }

    #[tokio::test]
    async fn sweep_interval_controls_ttl_sweeps_from_hot_upsert_paths() {
        // A zero interval sweeps on every upsert, evicting the expired entry.
        assert!(!expired_entry_survives_second_upsert(Duration::ZERO).await);
        // A long interval keeps the throttle closed, so the expired entry is
        // still physically present after the second upsert.
        assert!(expired_entry_survives_second_upsert(Duration::from_secs(3600)).await);
    }
}
