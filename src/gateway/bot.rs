use std::any::{Any, TypeId};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex, RwLock as StdRwLock};

use serde_json::value::RawValue;
#[cfg(feature = "sharding")]
use tokio::sync::watch;
use tokio::sync::{mpsc, RwLock};
#[cfg(feature = "sharding")]
use tokio::time::sleep;
use tokio::time::Duration;
use tracing::warn;

#[cfg(feature = "cache")]
use crate::cache::CacheBackend;
use crate::cache::{
    CacheConfig, CacheHandle, ChannelManager, GuildManager, MemberManager, MessageManager,
    RoleManager,
};
#[cfg(feature = "collectors")]
use crate::collector::CollectorHub;
use crate::error::{DiscordError, IncompleteMemberFetch};
use crate::event::{Event, EventDecodeError, EventKind};
use crate::http::DiscordHttpClient;
#[cfg(feature = "voice")]
use crate::model::UserId;
use crate::model::{
    ApplicationId, ChannelId, GuildCreatePayload, GuildId, Interaction, MessageId, RoleId,
};
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
    connect as connect_voice_runtime_impl, connect_with_options as connect_voice_runtime_with_dave,
    VoiceRuntimeConfig, VoiceRuntimeHandle,
};
use crate::ws::GatewayConnectionConfig;

#[cfg(feature = "sharding")]
use super::client::GatewayCommand;
#[cfg(feature = "sharding")]
use super::client::SupervisorCallback;
use super::client::{EventCallback, GatewayClient, GATEWAY_COMMAND_QUEUE_CAPACITY};
use super::messenger::ShardMessenger;
#[cfg(feature = "sharding")]
use super::supervisor::{failure_raised, lock_sharding_manager, FleetFailure, ShardSupervisor};

#[cfg(feature = "cache")]
#[path = "bot/backend.rs"]
mod backend;
#[path = "bot/pipeline.rs"]
mod pipeline;

#[cfg(all(test, feature = "cache"))]
use backend::forward_cache_backend_updates;
#[cfg(feature = "cache")]
pub use backend::CacheBackendHealth;
#[cfg(feature = "cache")]
use backend::{
    BackendWrite, CacheBackendForwarder, CacheBackendWorker, ErasedCacheBackend, ForwarderShared,
};
#[cfg(test)]
use pipeline::{process_gateway_dispatch, BacklogWarning, OverflowLog};
use pipeline::{spawn_event_pipeline, GatewayDispatch};

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

/// Outbound command senders of the shards running in this process, keyed by
/// shard id. A std lock (never held across an await) so a drop guard can
/// unregister a shard synchronously when its runtime future ends or is
/// cancelled.
type GatewayCommandRegistry = Arc<StdRwLock<HashMap<u32, ShardMessenger>>>;

/// In-flight [`Context::fetch_members`] calls keyed by request nonce.
///
/// A std mutex for the same reason as [`GatewayCommandRegistry`]: the
/// [`MemberChunkWaiterGuard`] must remove its entry from `Drop`, which
/// cannot await. Critical sections are a map lookup and a non-blocking send.
type MemberChunkWaiters = Arc<StdMutex<HashMap<String, MemberChunkWaiter>>>;

struct MemberChunkWaiter {
    /// Shard the request went out on; its chunks can only arrive there, so
    /// the waiter is failed fast when that shard stops.
    shard_id: u32,
    /// Unbounded because the event processor must never wait on a caller;
    /// growth is bounded by the `chunk_count` Discord sends for one request.
    chunks: mpsc::UnboundedSender<crate::event::GuildMembersChunkPayload>,
}

fn read_gateway_commands(
    registry: &StdRwLock<HashMap<u32, ShardMessenger>>,
) -> std::sync::RwLockReadGuard<'_, HashMap<u32, ShardMessenger>> {
    registry
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn write_gateway_commands(
    registry: &StdRwLock<HashMap<u32, ShardMessenger>>,
) -> std::sync::RwLockWriteGuard<'_, HashMap<u32, ShardMessenger>> {
    registry
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn lock_member_chunk_waiters(
    waiters: &StdMutex<HashMap<String, MemberChunkWaiter>>,
) -> std::sync::MutexGuard<'_, HashMap<String, MemberChunkWaiter>> {
    waiters
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Unregisters a [`Context::fetch_members`] waiter however the call ends.
///
/// Removing the entry only after a normal completion leaked it whenever the
/// caller timed out early, dropped the future (`select!`, task abort), or
/// failed to send the request; the map then grew for the life of the
/// process. Tying removal to scope exit covers every path.
struct MemberChunkWaiterGuard {
    waiters: MemberChunkWaiters,
    nonce: String,
}

impl Drop for MemberChunkWaiterGuard {
    fn drop(&mut self) {
        lock_member_chunk_waiters(&self.waiters).remove(&self.nonce);
    }
}

/// Chunks received so far for one [`Context::fetch_members`] request.
#[derive(Default)]
struct MemberChunkProgress {
    expected: Option<u64>,
    seen: HashSet<u64>,
    members: Vec<Arc<crate::model::Member>>,
    /// Members the chunks reported as not decoded
    /// (`GuildMembersChunkPayload::dropped_members`).
    dropped: usize,
}

impl MemberChunkProgress {
    /// Adds a chunk; returns the members once every chunk arrived, or
    /// [`DiscordError::IncompleteMemberFetch`] with them when any member
    /// in the chunks did not decode.
    fn add(
        &mut self,
        chunk: crate::event::GuildMembersChunkPayload,
    ) -> Result<Option<Vec<Arc<crate::model::Member>>>, DiscordError> {
        let count = *self.expected.get_or_insert(chunk.chunk_count);
        if chunk.chunk_count != count {
            return Err(DiscordError::gateway(format!(
                "GUILD_MEMBERS_CHUNK chunk_count changed from {count} to {}",
                chunk.chunk_count
            )));
        }
        if count > 0 && chunk.chunk_index >= count {
            return Err(DiscordError::gateway(format!(
                "GUILD_MEMBERS_CHUNK chunk_index {} is out of range for chunk_count {count}",
                chunk.chunk_index
            )));
        }
        // A repeated index must not add its members twice.
        if self.seen.insert(chunk.chunk_index) {
            self.members.extend(chunk.members);
            self.dropped = self.dropped.saturating_add(chunk.dropped_members);
        }
        if u64::try_from(self.seen.len()).unwrap_or(u64::MAX) >= count {
            let members = std::mem::take(&mut self.members);
            // Waits for every chunk first, so the partial list is as
            // complete as the request can make it.
            if self.dropped > 0 {
                return Err(DiscordError::IncompleteMemberFetch(Box::new(
                    IncompleteMemberFetch {
                        guild_id: chunk.guild_id,
                        members,
                        dropped: self.dropped,
                    },
                )));
            }
            return Ok(Some(members));
        }
        Ok(None)
    }

    fn describe(&self) -> String {
        match self.expected {
            Some(count) => format!("received {} of {count} chunks", self.seen.len()),
            None => "no chunks received".to_string(),
        }
    }
}

#[derive(Clone)]
/// Typed Discord API object for `Context`.
pub struct Context {
    pub http: Arc<DiscordHttpClient>,
    /// User data shared by every handler call; an async lock because
    /// handlers may hold it across `.await`.
    pub data: Arc<RwLock<TypeMap>>,
    pub cache: CacheHandle,
    pub shard_id: u32,
    pub shard_count: u32,
    gateway_commands: GatewayCommandRegistry,
    member_chunk_waiters: MemberChunkWaiters,
    default_allowed_mentions: Option<crate::model::AllowedMentions>,
    // The event processor applies voice updates; handlers connect and play
    // through `ctx.voice()`.
    #[cfg(feature = "voice")]
    voice: Arc<RwLock<VoiceManager>>,
    #[cfg(feature = "collectors")]
    collectors: CollectorHub,
    /// When the shard received the event this handler call is handling.
    event_received_at: Option<tokio::time::Instant>,
    /// Collector hub position just after the event this handler call is
    /// handling; see [`Context::message_collector`].
    #[cfg(feature = "collectors")]
    event_cursor: Option<crate::collector::EventCursor>,
    #[cfg(feature = "cache")]
    cache_backend_health: Option<Arc<ForwarderShared>>,
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
            gateway_commands: Arc::new(StdRwLock::new(HashMap::new())),
            member_chunk_waiters: Arc::new(StdMutex::new(HashMap::new())),
            default_allowed_mentions: None,
            #[cfg(feature = "voice")]
            voice: Arc::new(RwLock::new(VoiceManager::new())),
            #[cfg(feature = "collectors")]
            collectors: CollectorHub::new(),
            event_received_at: None,
            #[cfg(feature = "collectors")]
            event_cursor: None,
            #[cfg(feature = "cache")]
            cache_backend_health: None,
        }
    }

    /// When this shard received the event the current handler call is
    /// handling (`None` outside handler calls). Interaction handlers can use
    /// it to see how much of Discord's three-second response window is
    /// left.
    pub fn event_received_at(&self) -> Option<tokio::time::Instant> {
        self.event_received_at
    }

    /// Health of the external cache backend mirror registered with
    /// [`ClientBuilder::cache_backend`], or `None` without one.
    #[cfg(feature = "cache")]
    pub fn cache_backend_health(&self) -> Option<CacheBackendHealth> {
        self.cache_backend_health
            .as_ref()
            .map(|shared| shared.health())
    }

    /// Allowed-mentions configured via
    /// [`ClientBuilder::default_allowed_mentions`], if any. The runtime's
    /// REST client injects these into message payloads that do not set
    /// `allowed_mentions` themselves.
    pub fn default_allowed_mentions(&self) -> Option<&crate::model::AllowedMentions> {
        self.default_allowed_mentions.as_ref()
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
        read_gateway_commands(&self.gateway_commands)
            .get(&self.shard_id)
            .cloned()
    }

    /// Picks the messenger for a guild-scoped gateway command.
    ///
    /// Discord only honours member requests and voice state updates sent on
    /// the shard that owns the guild (`(guild_id >> 22) % shard_count`), but
    /// a handler's `Context` belongs to whichever shard delivered the event.
    /// Routing by guild lets any shard's context fetch members or join voice
    /// in any guild served by this process. Falls back to the context's own
    /// shard when the owner runs elsewhere (for example another process), so
    /// single-shard and externally sharded setups behave as before.
    fn messenger_for_guild(&self, guild_id: GuildId) -> Result<ShardMessenger, DiscordError> {
        let commands = read_gateway_commands(&self.gateway_commands);
        let owner = crate::cache::shard_for_guild_id(guild_id, self.shard_count);
        commands
            .get(&owner)
            .or_else(|| commands.get(&self.shard_id))
            .cloned()
            .ok_or_else(|| invalid_data_error("missing shard messenger"))
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

    /// Sends REQUEST_GUILD_MEMBERS on the shard that owns `request.guild_id`
    /// (see [`Context::fetch_members`] for awaiting the resulting chunks).
    pub async fn request_guild_members(
        &self,
        request: crate::model::RequestGuildMembers,
    ) -> Result<(), DiscordError> {
        self.messenger_for_guild(request.guild_id)?
            .request_guild_members(request)
    }

    /// Requests guild members over the gateway and awaits the resulting
    /// GUILD_MEMBERS_CHUNK payloads, returning the collected members — the
    /// equivalent of discord.js's `guild.members.fetch()`. Fetched members
    /// are also written to the cache by the event processor.
    ///
    /// Pass `query: None` and `limit: None` to fetch all members (requires
    /// the GUILD_MEMBERS privileged intent); a non-empty `query` prefix
    /// searches by username with the given limit.
    ///
    /// Safe to await inside an [`EventHandler`] in every
    /// [`EventDispatchMode`]: chunks are correlated by the shard's internal
    /// event processor, which keeps running while handlers await. The
    /// request goes out on the shard that owns the guild when that shard
    /// runs in this process.
    ///
    /// If every chunk arrived but some members in them did not decode (a
    /// shape this version cannot read; each chunk keeps the members that
    /// did and logs the rest), the call fails with
    /// [`DiscordError::IncompleteMemberFetch`], which carries the decoded
    /// members and the number dropped; take `members` from it to accept a
    /// partial list. The decoded members are cached either way.
    pub async fn fetch_members(
        &self,
        guild_id: GuildId,
        query: Option<String>,
        limit: Option<u32>,
    ) -> Result<Vec<Arc<crate::model::Member>>, DiscordError> {
        self.fetch_members_with_timeout(guild_id, query, limit, Duration::from_secs(60))
            .await
    }

    /// [`Context::fetch_members`] with an explicit overall timeout.
    ///
    /// The chunk waiter is unregistered when the call returns, times out, or
    /// is cancelled, and the call fails promptly if the shard stops.
    pub async fn fetch_members_with_timeout(
        &self,
        guild_id: GuildId,
        query: Option<String>,
        limit: Option<u32>,
        timeout: Duration,
    ) -> Result<Vec<Arc<crate::model::Member>>, DiscordError> {
        // Process-wide, so concurrent fetches get distinct chunk nonces.
        static FETCH_NONCE: AtomicU64 = AtomicU64::new(0);
        let messenger = self.messenger_for_guild(guild_id)?;
        let nonce = format!(
            "drs{}-{}",
            messenger.shard_id(),
            FETCH_NONCE.fetch_add(1, Ordering::Relaxed)
        );

        let (chunk_tx, mut chunk_rx) = mpsc::unbounded_channel();
        lock_member_chunk_waiters(&self.member_chunk_waiters).insert(
            nonce.clone(),
            MemberChunkWaiter {
                shard_id: messenger.shard_id(),
                chunks: chunk_tx,
            },
        );
        let _waiter = MemberChunkWaiterGuard {
            waiters: Arc::clone(&self.member_chunk_waiters),
            nonce: nonce.clone(),
        };

        messenger.request_guild_members(crate::model::RequestGuildMembers {
            guild_id,
            query: Some(query.unwrap_or_default()),
            limit: Some(limit.unwrap_or(0)),
            presences: None,
            user_ids: None,
            nonce: Some(nonce),
        })?;

        // Complete only once every chunk index 0..chunk_count arrived:
        // returning on the last index alone would report a list with a
        // missing middle chunk as success.
        let mut chunks = MemberChunkProgress::default();
        let collect = async {
            while let Some(chunk) = chunk_rx.recv().await {
                if let Some(members) = chunks.add(chunk)? {
                    return Ok(members);
                }
            }
            Err(DiscordError::gateway(format!(
                "shard stopped while collecting member chunks ({})",
                chunks.describe()
            )))
        };
        match tokio::time::timeout(timeout, collect).await {
            Ok(result) => result,
            Err(_) => Err(DiscordError::gateway(format!(
                "timed out waiting for GUILD_MEMBERS_CHUNK ({})",
                chunks.describe()
            ))),
        }
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

    /// Sends a voice state update on the shard that owns the guild.
    pub async fn update_voice_state(
        &self,
        guild_id: GuildId,
        channel_id: Option<ChannelId>,
        self_mute: bool,
        self_deaf: bool,
    ) -> Result<(), DiscordError> {
        self.messenger_for_guild(guild_id)?
            .update_voice_state(guild_id, channel_id, self_mute, self_deaf)
    }

    pub async fn request_soundboard_sounds(
        &self,
        guild_ids: impl IntoIterator<Item = GuildId>,
    ) -> Result<(), DiscordError> {
        let messenger = self
            .shard_messenger()
            .await
            .ok_or_else(|| invalid_data_error("missing shard messenger"))?;
        messenger.request_soundboard_sounds(guild_ids)
    }

    /// Joins a voice channel through the shard that owns the guild.
    pub async fn join_voice(
        &self,
        guild_id: GuildId,
        channel_id: ChannelId,
        self_mute: bool,
        self_deaf: bool,
    ) -> Result<(), DiscordError> {
        self.messenger_for_guild(guild_id)?
            .join_voice(guild_id, channel_id, self_mute, self_deaf)
    }

    /// Leaves voice through the shard that owns the guild.
    pub async fn leave_voice(
        &self,
        guild_id: GuildId,
        self_mute: bool,
        self_deaf: bool,
    ) -> Result<(), DiscordError> {
        self.messenger_for_guild(guild_id)?
            .leave_voice(guild_id, self_mute, self_deaf)
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
    pub async fn disconnect_voice(&self, guild_id: GuildId) -> Option<VoiceConnectionState> {
        let mut voice = self.voice.write().await;
        voice.disconnect(guild_id)
    }

    #[cfg(feature = "voice")]
    pub async fn enqueue_voice_track(&self, guild_id: GuildId, track: AudioTrack) -> Option<usize> {
        let mut voice = self.voice.write().await;
        voice.enqueue(guild_id, track)
    }

    #[cfg(feature = "voice")]
    pub async fn voice_runtime_config(
        &self,
        guild_id: GuildId,
        user_id: UserId,
    ) -> Option<VoiceRuntimeConfig> {
        let voice = self.voice.read().await;
        voice.runtime_config(guild_id, user_id)
    }

    /// Connects the voice runtime for `guild_id` from the voice state this
    /// context tracked, leaving DAVE end-to-end encryption to the caller, as
    /// in 3.0.0 (`connect_voice_runtime`).
    ///
    /// In a DAVE call (every voice channel except Stage channels) the
    /// handle's plain sends and receives refuse it; audio needs your own MLS
    /// handling. Use [`Context::connect_voice_runtime_with_dave`] to have the
    /// runtime run DAVE.
    #[cfg(feature = "voice")]
    pub async fn connect_voice_runtime(
        &self,
        guild_id: GuildId,
        user_id: UserId,
    ) -> Result<VoiceRuntimeHandle, DiscordError> {
        let config = self
            .voice_runtime_config(guild_id, user_id)
            .await
            .ok_or_else(|| {
                invalid_data_error("voice runtime requires endpoint, session_id, and token")
            })?;
        connect_voice_runtime_impl(config).await
    }

    /// Connects the voice runtime for `guild_id` from the voice state this
    /// context tracked (session ID, token, endpoint, and channel), with the
    /// runtime running DAVE end-to-end encryption itself
    /// (`connect_voice_runtime_with_options`). Discord requires DAVE in
    /// every voice channel except Stage channels, so this is the method to
    /// use for audio there: an `AudioPlayer` subscribed to the handle and its
    /// plain send and receive methods encrypt and decrypt on their own.
    ///
    /// Do not drive MLS by hand (`VoiceDaveySession`, `send_dave_*`) on the
    /// returned handle: the runtime sends its own key package.
    ///
    /// # Security
    ///
    /// Keep the `davey` crate's trace logging off (its `tracing` target at
    /// `info` or coarser): at trace level it writes DAVE media key secrets to
    /// the log. See `connect_voice_runtime_with_options`.
    ///
    /// `user_id` must be the bot's own user. The tracked state is updated by
    /// every VOICE_STATE_UPDATE of the guild, other members' included, so
    /// call this right after the bot's own VOICE_STATE_UPDATE and
    /// VOICE_SERVER_UPDATE, or build the configuration from those events and
    /// call `connect_voice_runtime_with_options` yourself, as
    /// `examples/music_bot.rs` does.
    ///
    /// [Decision Log]
    /// - Purpose: one call that gives a working voice handle in DAVE
    ///   channels, from the state the context already tracks.
    /// - Constraint: `connect_voice_runtime` existed in 3.0.0, and callers
    ///   may drive MLS themselves on its handle; giving it a runtime session
    ///   would put two sessions with different keys on one connection.
    /// - Alternatives: change `connect_voice_runtime` (breaks those callers);
    ///   a flag argument (changes its signature); a second method.
    /// - Chosen: a second method, named after what it adds.
    /// - Trade-offs: two similar methods; the docs and examples point to this
    ///   one.
    #[cfg(feature = "voice")]
    pub async fn connect_voice_runtime_with_dave(
        &self,
        guild_id: GuildId,
        user_id: UserId,
    ) -> Result<VoiceRuntimeHandle, DiscordError> {
        let (config, options) = {
            let voice = self.voice.read().await;
            (
                voice.runtime_config(guild_id, user_id),
                voice.runtime_options(guild_id),
            )
        };
        let (Some(config), Some(options)) = (config, options) else {
            return Err(invalid_data_error(
                "voice runtime requires endpoint, session_id, and token",
            ));
        };
        connect_voice_runtime_with_dave(config, options).await
    }

    /// The runtime's collector hub. Collectors created from it directly
    /// start "from now": they only see events published after they were
    /// created. Inside a handler, prefer [`Context::message_collector`] and
    /// friends, which also see events that arrived while the handler call
    /// was queued.
    #[cfg(feature = "collectors")]
    pub fn collectors(&self) -> &CollectorHub {
        &self.collectors
    }

    /// Hub position just after the event the current handler call is
    /// handling, captured when that event was published (before its handler
    /// call was queued). `None` outside handler calls.
    #[cfg(feature = "collectors")]
    pub fn event_cursor(&self) -> Option<crate::collector::EventCursor> {
        self.event_cursor
    }

    /// The hub position collectors created from this context start at.
    #[cfg(feature = "collectors")]
    fn collector_start(&self) -> crate::collector::EventCursor {
        self.event_cursor
            .unwrap_or_else(|| self.collectors.cursor())
    }

    /// Collects messages published after the event this handler call is
    /// handling — including ones published while the call waited in the
    /// queue, before this collector existed. Outside handler calls it starts
    /// from now. If more events than the hub keeps (256 by default) passed
    /// in between, the oldest are skipped and counted in
    /// `lagged_events()`.
    #[cfg(feature = "collectors")]
    pub fn message_collector(&self) -> crate::collector::MessageCollector {
        self.collectors
            .message_collector_from(self.collector_start())
    }

    /// Like [`Context::message_collector`], for message updates
    /// (partial messages; see [`MessageUpdate`](crate::model::MessageUpdate)).
    #[cfg(feature = "collectors")]
    pub fn message_update_collector(&self) -> crate::collector::MessageUpdateCollector {
        self.collectors
            .message_update_collector_from(self.collector_start())
    }

    /// Like [`Context::message_collector`], for interactions.
    #[cfg(feature = "collectors")]
    pub fn interaction_collector(&self) -> crate::collector::InteractionCollector {
        self.collectors
            .interaction_collector_from(self.collector_start())
    }

    /// Like [`Context::message_collector`], for component interactions.
    #[cfg(feature = "collectors")]
    pub fn component_collector(&self) -> crate::collector::ComponentCollector {
        self.collectors
            .component_collector_from(self.collector_start())
    }

    /// Like [`Context::message_collector`], for modal submissions.
    #[cfg(feature = "collectors")]
    pub fn modal_collector(&self) -> crate::collector::ModalCollector {
        self.collectors.modal_collector_from(self.collector_start())
    }

    /// Like [`Context::message_collector`], for reactions on one message.
    #[cfg(feature = "collectors")]
    pub fn reaction_collector(&self, message_id: MessageId) -> crate::collector::ReactionCollector {
        self.collectors
            .reaction_collector_from(self.collector_start(), message_id)
    }
}

/// Receives gateway events; register one with [`ClientBuilder::event_handler`].
///
/// Every method has a default that does nothing, so an implementation only
/// overrides the hooks it needs. [`handle_event`](Self::handle_event)
/// receives every event first, as the shared `Arc<Event>`, and by default
/// forwards it to the matching typed hook (`ready`, `message_create`, ...);
/// unknown dispatches go to [`raw_event`](Self::raw_event) and payloads that
/// failed to decode to [`decode_failed`](Self::decode_failed). Overriding
/// `handle_event` replaces that routing, so the typed hooks are then only
/// called if the override calls them.
///
/// # How hooks receive payloads
///
/// The runtime decodes each event once and shares it between the cache,
/// collectors, and this handler; no hook call copies a payload:
///
/// - Payloads the cache stores as they are arrive as an `Arc` you may keep
///   or move into a task: `message_create` (`Arc<Message>`),
///   `message_update` (`Arc<MessageUpdate>`, a partial message), and
///   `member_add` / `member_update` (`Arc<Member>`).
/// - Every other payload arrives as a reference into the event
///   (`&Channel`, `&ReactionEvent`, ...). Clone what you keep beyond the
///   call, or override `handle_event` and keep the `Arc<Event>`.
///
/// # Implementing
///
/// Write the methods as plain `async fn` — no attribute macro is needed:
///
/// ```no_run
/// use std::sync::Arc;
///
/// use discordrs::{Context, EventHandler, Message, ReadyPayload};
///
/// struct Handler;
///
/// impl EventHandler for Handler {
///     async fn ready(&self, _ctx: Context, ready: &ReadyPayload) {
///         println!("connected as {}", ready.user.username);
///     }
///
///     async fn message_create(&self, _ctx: Context, message: Arc<Message>) {
///         println!("message: {}", message.content);
///     }
/// }
/// ```
///
/// # `Send` contract
///
/// The methods are declared as `fn ... -> impl Future<Output = ()> + Send`,
/// because the runtime runs every handler call on a spawned Tokio task. An
/// `async fn` in your impl therefore must produce a `Send` future: do not
/// hold a non-`Send` value (`Rc`, `RefCell` borrow, `std::sync::MutexGuard`)
/// across an `.await`. Violations are compile errors at the impl.
///
/// The trait is not object safe (`dyn EventHandler` does not exist). The
/// runtime stores the handler type-erased and boxes each `handle_event`
/// future once; the typed hook it awaits is part of that future, so the
/// default routing allocates once per event (async-trait boxed both).
// [Decision Log]
// - 목적과 의도: 이벤트 하나를 핸들러에 넘길 때 payload를 깊게 복제하지 않는다(기준선 핫스팟
//   ④: 4,848 B 인라인 Event가 전달마다 상자 두 개로 복사됐다).
// - 기존 구현 및 제약 조건: 2.x 훅은 payload를 값으로 받았고(`message: Message`), 기본
//   handle_event가 enum을 분해해 넘겼다. 이제 이벤트는 `Arc<Event>` 하나를 캐시·컬렉터·핸들러가
//   공유하므로 값으로 넘기려면 복제해야 한다. RPITIT 훅은 인자의 수명을 반환 future에 담는다.
// - 검토한 주요 대안: (1) 모든 훅이 `&T`, (2) 모든 훅이 `Arc<T>`(payload마다 Arc), (3) 저장
//   방식대로: 이벤트가 `Arc<T>`로 가진 payload는 `Arc<T>` 복제, 나머지는 `&T`, (4) 값 유지 +
//   훅마다 clone.
// - 선택한 방식: (3).
// - 다른 대안 대신 이 방식을 선택한 이유: (2)는 공유하지 않는 payload까지 `Arc` 할당을 늘리고
//   `EventKind`가 커진다. (1)은 가장 흔한 `message_create`에서 태스크로 넘기려면 깊은 복제가
//   필요하다. (4)는 이벤트마다 깊은 복제다. (3)은 할당이 0이고, 규칙이 `EventKind`의 저장 방식
//   하나로 설명된다.
// - 장점, 단점 및 영향: 훅 시그니처가 깨진다(마이그레이션 노트). 참조 훅의 future는 이벤트를
//   빌리므로 사용자는 보관할 값만 복제한다. GUILD_CREATE 훅은 `&GuildCreatePayload`(사용 가능한
//   길드 또는 id만 있는 사용 불가 길드)를 받는다.
#[expect(
    missing_docs,
    reason = "one hook per Gateway event: the hook is named after the event, whose payload \
              type carries the docs"
)]
pub trait EventHandler: Send + Sync + 'static {
    /// Handles one decoded Gateway event and dispatches to the typed hooks
    /// by default (see the trait docs for how each hook receives its
    /// payload).
    fn handle_event(&self, ctx: Context, event: Arc<Event>) -> impl Future<Output = ()> + Send {
        async move {
            match event.kind() {
                EventKind::Ready(ready) => self.ready(ctx, ready).await,
                EventKind::Resumed => self.resumed(ctx).await,
                EventKind::GuildCreate(guild) => self.guild_create(ctx, guild).await,
                EventKind::GuildUpdate(guild) => self.guild_update(ctx, guild).await,
                EventKind::GuildDelete(data) => self.guild_delete(ctx, data).await,
                EventKind::ChannelCreate(channel) => {
                    self.channel_create(ctx, Arc::clone(channel)).await;
                }
                EventKind::ChannelUpdate(channel) => {
                    self.channel_update(ctx, Arc::clone(channel)).await;
                }
                EventKind::ChannelDelete(channel) => self.channel_delete(ctx, channel).await,
                EventKind::MemberAdd(payload) => {
                    self.member_add(ctx, payload.guild_id, Arc::clone(&payload.member))
                        .await
                }
                EventKind::MemberUpdate(update) => {
                    self.member_update(ctx, Arc::clone(update)).await
                }
                EventKind::MemberRemove(data) => {
                    self.member_remove(ctx, data.guild_id, &data.user).await
                }
                EventKind::GuildMembersChunk(chunk) => self.guild_members_chunk(ctx, chunk).await,
                EventKind::RoleCreate(payload) => {
                    self.role_create(ctx, payload.guild_id, &payload.role).await
                }
                EventKind::RoleUpdate(payload) => {
                    self.role_update(ctx, payload.guild_id, &payload.role).await
                }
                EventKind::RoleDelete(data) => {
                    self.role_delete(ctx, data.guild_id, data.role_id).await
                }
                EventKind::MessageCreate(message) => {
                    self.message_create(ctx, Arc::clone(message)).await
                }
                EventKind::MessageUpdate(update) => {
                    self.message_update(ctx, Arc::clone(update)).await
                }
                EventKind::MessageDelete(data) => {
                    self.message_delete(ctx, data.channel_id, data.id).await
                }
                EventKind::MessageDeleteBulk(payload) => {
                    self.message_delete_bulk(ctx, payload).await
                }
                EventKind::ChannelPinsUpdate(payload) => {
                    self.channel_pins_update(ctx, payload).await
                }
                EventKind::GuildBanAdd(payload) => self.guild_ban_add(ctx, payload).await,
                EventKind::GuildBanRemove(payload) => self.guild_ban_remove(ctx, payload).await,
                EventKind::GuildEmojisUpdate(payload) => {
                    self.guild_emojis_update(ctx, payload).await
                }
                EventKind::GuildIntegrationsUpdate(payload) => {
                    self.guild_integrations_update(ctx, payload).await
                }
                EventKind::EntitlementCreate(entitlement) => {
                    self.entitlement_create(ctx, entitlement).await
                }
                EventKind::EntitlementUpdate(entitlement) => {
                    self.entitlement_update(ctx, entitlement).await
                }
                EventKind::EntitlementDelete(entitlement) => {
                    self.entitlement_delete(ctx, entitlement).await
                }
                EventKind::SubscriptionCreate(subscription) => {
                    self.subscription_create(ctx, subscription).await
                }
                EventKind::SubscriptionUpdate(subscription) => {
                    self.subscription_update(ctx, subscription).await
                }
                EventKind::SubscriptionDelete(subscription) => {
                    self.subscription_delete(ctx, subscription).await
                }
                EventKind::IntegrationCreate(payload) => {
                    self.integration_create(ctx, payload).await
                }
                EventKind::IntegrationUpdate(payload) => {
                    self.integration_update(ctx, payload).await
                }
                EventKind::IntegrationDelete(payload) => {
                    self.integration_delete(ctx, payload).await
                }
                EventKind::GuildSoundboardSoundCreate(sound) => {
                    self.guild_soundboard_sound_create(ctx, sound).await
                }
                EventKind::GuildSoundboardSoundUpdate(sound) => {
                    self.guild_soundboard_sound_update(ctx, sound).await
                }
                EventKind::GuildSoundboardSoundDelete(payload) => {
                    self.guild_soundboard_sound_delete(ctx, payload).await
                }
                EventKind::GuildSoundboardSoundsUpdate(payload) => {
                    self.guild_soundboard_sounds_update(ctx, payload).await
                }
                EventKind::SoundboardSounds(payload) => self.soundboard_sounds(ctx, payload).await,
                EventKind::WebhooksUpdate(payload) => self.webhooks_update(ctx, payload).await,
                EventKind::InviteCreate(payload) => self.invite_create(ctx, payload).await,
                EventKind::InviteDelete(payload) => self.invite_delete(ctx, payload).await,
                EventKind::VoiceStateUpdate(state) => self.voice_state_update(ctx, state).await,
                EventKind::VoiceServerUpdate(data) => self.voice_server_update(ctx, data).await,
                EventKind::MessageReactionAdd(reaction) => self.reaction_add(ctx, reaction).await,
                EventKind::MessageReactionRemove(reaction) => {
                    self.reaction_remove(ctx, reaction).await
                }
                EventKind::MessageReactionRemoveAll(payload) => {
                    self.reaction_remove_all(ctx, payload).await
                }
                EventKind::MessageReactionRemoveEmoji(payload) => {
                    self.reaction_remove_emoji(ctx, payload).await
                }
                EventKind::MessagePollVoteAdd(payload) => self.poll_vote_add(ctx, payload).await,
                EventKind::MessagePollVoteRemove(payload) => {
                    self.poll_vote_remove(ctx, payload).await
                }
                EventKind::TypingStart(payload) => self.typing_start(ctx, payload).await,
                EventKind::PresenceUpdate(payload) => self.presence_update(ctx, payload).await,
                EventKind::InteractionCreate(interaction) => {
                    self.interaction_create(ctx, interaction).await
                }
                EventKind::UserUpdate(user) => self.user_update(ctx, user).await,
                EventKind::ThreadCreate(payload) => self.thread_create(ctx, payload).await,
                EventKind::ThreadUpdate(payload) => self.thread_update(ctx, payload).await,
                EventKind::ThreadDelete(payload) => self.thread_delete(ctx, payload).await,
                EventKind::ThreadListSync(payload) => self.thread_list_sync(ctx, payload).await,
                EventKind::ThreadMemberUpdate(payload) => {
                    self.thread_member_update(ctx, payload).await
                }
                EventKind::ThreadMembersUpdate(payload) => {
                    self.thread_members_update(ctx, payload).await
                }
                EventKind::GuildStickersUpdate(payload) => {
                    self.guild_stickers_update(ctx, payload).await
                }
                EventKind::GuildScheduledEventCreate(payload) => {
                    self.guild_scheduled_event_create(ctx, payload).await
                }
                EventKind::GuildScheduledEventUpdate(payload) => {
                    self.guild_scheduled_event_update(ctx, payload).await
                }
                EventKind::GuildScheduledEventDelete(payload) => {
                    self.guild_scheduled_event_delete(ctx, payload).await
                }
                EventKind::GuildScheduledEventUserAdd(payload) => {
                    self.guild_scheduled_event_user_add(ctx, payload).await
                }
                EventKind::GuildScheduledEventUserRemove(payload) => {
                    self.guild_scheduled_event_user_remove(ctx, payload).await
                }
                EventKind::StageInstanceCreate(stage_instance) => {
                    self.stage_instance_create(ctx, stage_instance).await
                }
                EventKind::StageInstanceUpdate(stage_instance) => {
                    self.stage_instance_update(ctx, stage_instance).await
                }
                EventKind::StageInstanceDelete(stage_instance) => {
                    self.stage_instance_delete(ctx, stage_instance).await
                }
                EventKind::VoiceChannelEffectSend(payload) => {
                    self.voice_channel_effect_send(ctx, payload).await
                }
                EventKind::VoiceChannelStartTimeUpdate(payload) => {
                    self.voice_channel_start_time_update(ctx, payload).await
                }
                EventKind::VoiceChannelStatusUpdate(payload) => {
                    self.voice_channel_status_update(ctx, payload).await
                }
                EventKind::ChannelInfo(payload) => self.channel_info(ctx, payload).await,
                EventKind::RateLimited(payload) => self.gateway_rate_limited(ctx, payload).await,
                EventKind::ApplicationCommandPermissionsUpdate(payload) => {
                    self.application_command_permissions_update(ctx, payload)
                        .await
                }
                EventKind::AutoModerationRuleCreate(payload) => {
                    self.auto_moderation_rule_create(ctx, payload).await
                }
                EventKind::AutoModerationRuleUpdate(payload) => {
                    self.auto_moderation_rule_update(ctx, payload).await
                }
                EventKind::AutoModerationRuleDelete(payload) => {
                    self.auto_moderation_rule_delete(ctx, payload).await
                }
                EventKind::AutoModerationActionExecution(payload) => {
                    self.auto_moderation_action_execution(ctx, payload).await
                }
                EventKind::GuildAuditLogEntryCreate(payload) => {
                    self.guild_audit_log_entry_create(ctx, payload).await
                }
                EventKind::Unknown => {
                    if let Some(data) = event.raw() {
                        self.raw_event(ctx, event.name(), data).await;
                    }
                }
                EventKind::DecodeFailed(error) => {
                    if let Some(data) = event.raw() {
                        self.decode_failed(ctx, event.name(), error, data).await;
                    }
                }
            }
        }
    }

    fn ready(
        &self,
        _ctx: Context,
        _ready: &crate::event::ReadyPayload,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn resumed(&self, _ctx: Context) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn guild_create(
        &self,
        _ctx: Context,
        _guild: &crate::model::GuildCreatePayload,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn guild_update(
        &self,
        _ctx: Context,
        _update: &crate::model::GuildUpdate,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn guild_delete(
        &self,
        _ctx: Context,
        _data: &crate::event::GuildDeletePayload,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn channel_create(
        &self,
        _ctx: Context,
        _channel: Arc<crate::model::Channel>,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn channel_update(
        &self,
        _ctx: Context,
        _channel: Arc<crate::model::Channel>,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn channel_delete(
        &self,
        _ctx: Context,
        _channel: &crate::model::Channel,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn member_add(
        &self,
        _ctx: Context,
        _guild_id: GuildId,
        _member: Arc<crate::model::Member>,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn member_update(
        &self,
        _ctx: Context,
        _update: Arc<crate::model::MemberUpdate>,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn member_remove(
        &self,
        _ctx: Context,
        _guild_id: GuildId,
        _user: &crate::model::User,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn guild_members_chunk(
        &self,
        _ctx: Context,
        _chunk: &crate::event::GuildMembersChunkPayload,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn role_create(
        &self,
        _ctx: Context,
        _guild_id: GuildId,
        _role: &crate::model::Role,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn role_update(
        &self,
        _ctx: Context,
        _guild_id: GuildId,
        _role: &crate::model::Role,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn role_delete(
        &self,
        _ctx: Context,
        _guild_id: GuildId,
        _role_id: RoleId,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn message_create(
        &self,
        _ctx: Context,
        _message: Arc<crate::model::Message>,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn message_update(
        &self,
        _ctx: Context,
        _update: Arc<crate::model::MessageUpdate>,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn message_delete(
        &self,
        _ctx: Context,
        _channel_id: ChannelId,
        _message_id: MessageId,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn message_delete_bulk(
        &self,
        _ctx: Context,
        _event: &crate::event::BulkMessageDeleteEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn channel_pins_update(
        &self,
        _ctx: Context,
        _event: &crate::event::ChannelPinsUpdateEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn guild_ban_add(
        &self,
        _ctx: Context,
        _event: &crate::event::GuildBanEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn guild_ban_remove(
        &self,
        _ctx: Context,
        _event: &crate::event::GuildBanEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn guild_emojis_update(
        &self,
        _ctx: Context,
        _event: &crate::event::GuildEmojisUpdateEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn guild_integrations_update(
        &self,
        _ctx: Context,
        _event: &crate::event::GuildIntegrationsUpdateEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn entitlement_create(
        &self,
        _ctx: Context,
        _entitlement: &crate::model::Entitlement,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn entitlement_update(
        &self,
        _ctx: Context,
        _entitlement: &crate::model::Entitlement,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn entitlement_delete(
        &self,
        _ctx: Context,
        _entitlement: &crate::model::Entitlement,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn subscription_create(
        &self,
        _ctx: Context,
        _subscription: &crate::model::Subscription,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn subscription_update(
        &self,
        _ctx: Context,
        _subscription: &crate::model::Subscription,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn subscription_delete(
        &self,
        _ctx: Context,
        _subscription: &crate::model::Subscription,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn integration_create(
        &self,
        _ctx: Context,
        _event: &crate::event::IntegrationEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn integration_update(
        &self,
        _ctx: Context,
        _event: &crate::event::IntegrationEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn integration_delete(
        &self,
        _ctx: Context,
        _event: &crate::event::IntegrationDeleteEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn guild_soundboard_sound_create(
        &self,
        _ctx: Context,
        _sound: &crate::model::SoundboardSound,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn guild_soundboard_sound_update(
        &self,
        _ctx: Context,
        _sound: &crate::model::SoundboardSound,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn guild_soundboard_sound_delete(
        &self,
        _ctx: Context,
        _event: &crate::event::SoundboardSoundDeleteEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn guild_soundboard_sounds_update(
        &self,
        _ctx: Context,
        _event: &crate::event::SoundboardSoundsEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn soundboard_sounds(
        &self,
        _ctx: Context,
        _event: &crate::event::SoundboardSoundsEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn webhooks_update(
        &self,
        _ctx: Context,
        _event: &crate::event::WebhooksUpdateEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn invite_create(
        &self,
        _ctx: Context,
        _event: &crate::event::InviteEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn invite_delete(
        &self,
        _ctx: Context,
        _event: &crate::event::InviteEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn voice_state_update(
        &self,
        _ctx: Context,
        _state: &crate::model::VoiceState,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn voice_server_update(
        &self,
        _ctx: Context,
        _data: &crate::model::VoiceServerUpdate,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn reaction_add(
        &self,
        _ctx: Context,
        _reaction: &crate::event::ReactionEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn reaction_remove(
        &self,
        _ctx: Context,
        _reaction: &crate::event::ReactionEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn reaction_remove_all(
        &self,
        _ctx: Context,
        _event: &crate::event::ReactionRemoveAllEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn reaction_remove_emoji(
        &self,
        _ctx: Context,
        _event: &crate::event::ReactionRemoveEmojiEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn poll_vote_add(
        &self,
        _ctx: Context,
        _event: &crate::event::PollVoteEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn poll_vote_remove(
        &self,
        _ctx: Context,
        _event: &crate::event::PollVoteEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn typing_start(
        &self,
        _ctx: Context,
        _event: &crate::event::TypingStartEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn presence_update(
        &self,
        _ctx: Context,
        _event: &crate::event::PresenceUpdateEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn interaction_create(
        &self,
        _ctx: Context,
        _interaction: &crate::model::Interaction,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn user_update(
        &self,
        _ctx: Context,
        _user: &crate::model::User,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn thread_create(
        &self,
        _ctx: Context,
        _event: &crate::event::ThreadEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn thread_update(
        &self,
        _ctx: Context,
        _event: &crate::event::ThreadEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn thread_delete(
        &self,
        _ctx: Context,
        _event: &crate::event::ThreadEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn thread_list_sync(
        &self,
        _ctx: Context,
        _event: &crate::event::ThreadListSyncEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn thread_member_update(
        &self,
        _ctx: Context,
        _event: &crate::event::ThreadMemberUpdateEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn thread_members_update(
        &self,
        _ctx: Context,
        _event: &crate::event::ThreadMembersUpdateEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn guild_stickers_update(
        &self,
        _ctx: Context,
        _event: &crate::event::GuildStickersUpdateEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn guild_scheduled_event_create(
        &self,
        _ctx: Context,
        _event: &crate::event::ScheduledEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn guild_scheduled_event_update(
        &self,
        _ctx: Context,
        _event: &crate::event::ScheduledEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn guild_scheduled_event_delete(
        &self,
        _ctx: Context,
        _event: &crate::event::ScheduledEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn guild_scheduled_event_user_add(
        &self,
        _ctx: Context,
        _event: &crate::event::GuildScheduledEventUserEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn guild_scheduled_event_user_remove(
        &self,
        _ctx: Context,
        _event: &crate::event::GuildScheduledEventUserEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn stage_instance_create(
        &self,
        _ctx: Context,
        _stage_instance: &crate::model::StageInstance,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn stage_instance_update(
        &self,
        _ctx: Context,
        _stage_instance: &crate::model::StageInstance,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn stage_instance_delete(
        &self,
        _ctx: Context,
        _stage_instance: &crate::model::StageInstance,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn voice_channel_effect_send(
        &self,
        _ctx: Context,
        _event: &crate::event::VoiceChannelEffectEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn voice_channel_start_time_update(
        &self,
        _ctx: Context,
        _event: &crate::event::VoiceChannelStartTimeUpdateEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn voice_channel_status_update(
        &self,
        _ctx: Context,
        _event: &crate::event::VoiceChannelStatusUpdateEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn channel_info(
        &self,
        _ctx: Context,
        _event: &crate::event::ChannelInfoEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn gateway_rate_limited(
        &self,
        _ctx: Context,
        _event: &crate::event::RateLimitedEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn application_command_permissions_update(
        &self,
        _ctx: Context,
        _event: &crate::event::ApplicationCommandPermissionsUpdateEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn auto_moderation_rule_create(
        &self,
        _ctx: Context,
        _event: &crate::event::AutoModerationEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn auto_moderation_rule_update(
        &self,
        _ctx: Context,
        _event: &crate::event::AutoModerationEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn auto_moderation_rule_delete(
        &self,
        _ctx: Context,
        _event: &crate::event::AutoModerationEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn auto_moderation_action_execution(
        &self,
        _ctx: Context,
        _event: &crate::event::AutoModerationEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    fn guild_audit_log_entry_create(
        &self,
        _ctx: Context,
        _event: &crate::event::AuditLogEntryEvent,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    /// Handles a dispatch this crate has no typed payload for
    /// ([`EventKind::Unknown`]), with its name and raw payload.
    fn raw_event(
        &self,
        _ctx: Context,
        _event_name: &str,
        _data: &RawValue,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
    /// Handles a known dispatch whose payload did not decode
    /// ([`EventKind::DecodeFailed`]). The runtime has already logged a
    /// warning; the raw payload is passed for inspection.
    fn decode_failed(
        &self,
        _ctx: Context,
        _event_name: &str,
        _error: &EventDecodeError,
        _data: &RawValue,
    ) -> impl Future<Output = ()> + Send {
        async {}
    }
}

/// Boxed, `Send` future returned by the type-erased handler.
type HandlerFuture<'a> = Pin<Box<dyn Future<Output = ()> + Send + 'a>>;

/// Object-safe view of an [`EventHandler`], which the runtime stores as
/// `Arc<dyn ErasedEventHandler>`.
///
/// ```text
/// [Decision Log]
/// - 목적과 의도: async-trait 의존성을 없애 syn v3 이중 빌드를 끊으면서, 런타임은 계속
///   핸들러 한 개를 여러 샤드·작업에서 공유한다.
/// - 기존 구현 및 제약 조건: #[async_trait]는 메서드마다 Box<dyn Future>를 돌려 trait을
///   object safe로 만들었고, 런타임이 Arc<dyn EventHandler>를 저장했다. 반환 위치
///   impl Future(RPITIT, 1.75+)를 쓰면 trait이 object safe가 아니다. MSRV는 1.88이다.
/// - 검토한 주요 대안: (1) async-trait 유지 또는 0.1.89 고정(사용자 Cargo는 최신 0.1.x를
///   고르므로 효과 없음), (2) 런타임 전체를 핸들러 타입으로 제네릭화, (3) 공개 trait은
///   RPITIT로 두고 내부에서만 박싱하는 비공개 trait + 블랭킷 impl.
/// - 선택한 방식: (3). 런타임은 handle_event 하나만 부르므로 이 trait도 메서드 하나다.
/// - 다른 대안 대신 이 방식을 선택한 이유: (2)는 ClientBuilder·샤드 감독자·파이프라인
///   전부에 타입 매개변수가 번지고 핸들러마다 런타임이 단형화되어 빌드가 오히려 느려진다.
///   (3)은 사용자 impl에서 매크로만 지우면 되고, 이벤트당 할당이 한 번이다. async-trait
///   시절의 기본 handle_event는 바깥 박스와 타입 훅 박스로 두 번 할당했다.
/// - 장점, 단점 및 영향: 공개 API에서 dyn EventHandler가 사라진다(파괴적 변경, CHANGELOG
///   기록). 사용자 future는 trait 선언의 + Send 때문에 Send여야 하며, 위반은 impl에서
///   컴파일 오류가 된다. 핸들러 하나의 future 크기는 가장 큰 타입 훅 future를 따른다.
/// ```
trait ErasedEventHandler: Send + Sync + 'static {
    fn handle_event_boxed(&self, ctx: Context, event: Arc<Event>) -> HandlerFuture<'_>;
}

impl<H: EventHandler> ErasedEventHandler for H {
    fn handle_event_boxed(&self, ctx: Context, event: Arc<Event>) -> HandlerFuture<'_> {
        Box::pin(self.handle_event(ctx, event))
    }
}

/// How gateway events are delivered to the [`EventHandler`].
///
/// In both modes each shard first runs its events through an internal
/// processor, in gateway order and independently of handlers: cache
/// updates, external [`CacheBackend`](crate::cache::CacheBackend)
/// forwarding, voice state, [`Context::fetch_members`] chunk correlation,
/// and collector publication. Handler calls are scheduled afterwards from a
/// bounded per-shard queue (see [`ClientBuilder::event_queue_capacity`]).
/// Because the processor never waits for handlers, a handler may await
/// `fetch_members` or a collector without stalling its own shard, and the
/// cache a handler reads may already include later events.
///
/// The mode applies to every event except INTERACTION_CREATE, which uses a
/// separate interaction lane with its own execution slots and admission
/// deadline (see [`ClientBuilder::max_concurrent_interactions`]), so an
/// interaction never waits behind other handler calls.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EventDispatchMode {
    /// Handler calls on a shard run one at a time, in gateway order (the
    /// default). A slow handler delays the next handler call, but not
    /// internal processing or interactions.
    #[default]
    Serial,
    /// Handler calls run as concurrent tasks, at most
    /// [`ClientBuilder::max_concurrent_handlers`] per shard, so one slow
    /// handler no longer delays the others. Calls may interleave and finish
    /// out of order.
    Concurrent,
}

/// Default per-shard capacity of each event queue (internal processing and
/// handler delivery). A shard receives up to 2,500 GUILD_CREATEs in a burst
/// after READY; the default holds that burst four times over before a slow
/// Serial handler starts losing deliveries.
const DEFAULT_EVENT_QUEUE_CAPACITY: usize = 10_000;
/// Default per-shard handler task limit in [`EventDispatchMode::Concurrent`].
const DEFAULT_MAX_CONCURRENT_HANDLERS: usize = 256;
/// Default time running handlers get to finish after a shard's gateway exits.
const DEFAULT_HANDLER_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
/// Queue depth at which the event pipeline logs a backlog warning (capped to
/// half of a smaller configured capacity).
const EVENT_BACKLOG_WARN_THRESHOLD: usize = 5_000;
/// Minimum interval between repeated backlog and overflow warnings, so an
/// overloaded shard does not also flood the log.
const OVERLOAD_WARNING_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);
/// Default per-shard capacity of the interaction queue.
const DEFAULT_INTERACTION_QUEUE_CAPACITY: usize = 256;
/// Default execution slots reserved for interaction handlers per shard.
const DEFAULT_MAX_CONCURRENT_INTERACTIONS: usize = 32;
/// Default time an interaction may wait for a slot. Discord requires the
/// initial response within three seconds of sending the interaction; this
/// leaves the handler (or the overload hook) about a second to respond.
const DEFAULT_INTERACTION_ADMISSION_TIMEOUT: Duration = Duration::from_secs(2);

/// Why an interaction was handed to the overload hook instead of the
/// [`EventHandler`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum InteractionOverloadReason {
    /// The shard's interaction queue was full when the interaction was
    /// processed.
    QueueFull,
    /// No interaction slot freed up within the admission timeout.
    AdmissionTimeout,
}

/// An interaction the runtime could not hand to the [`EventHandler`] in
/// time; see [`ClientBuilder::on_interaction_overload`].
#[non_exhaustive]
pub struct OverloadedInteraction {
    /// The context the handler call would have received.
    pub ctx: Context,
    /// The interaction, still unacknowledged by this runtime.
    pub interaction: Interaction,
    /// Why it was not admitted.
    pub reason: InteractionOverloadReason,
    /// How long ago the shard received it, so the hook can tell how much of
    /// Discord's three-second window is left.
    pub waited: Duration,
}

type InteractionOverloadHook = Arc<dyn Fn(OverloadedInteraction) + Send + Sync>;

/// Per-shard event pipeline settings collected by [`ClientBuilder`].
#[derive(Clone)]
struct DispatchConfig {
    mode: EventDispatchMode,
    queue_capacity: usize,
    max_concurrent_handlers: usize,
    shutdown_timeout: Duration,
    interaction_queue_capacity: usize,
    max_concurrent_interactions: usize,
    interaction_admission_timeout: Duration,
    interaction_overload: Option<InteractionOverloadHook>,
    /// Whether decoded known events keep their raw payload
    /// ([`ClientBuilder::keep_raw_events`]).
    keep_raw_events: bool,
}

impl Default for DispatchConfig {
    fn default() -> Self {
        Self {
            mode: EventDispatchMode::Serial,
            queue_capacity: DEFAULT_EVENT_QUEUE_CAPACITY,
            max_concurrent_handlers: DEFAULT_MAX_CONCURRENT_HANDLERS,
            shutdown_timeout: DEFAULT_HANDLER_SHUTDOWN_TIMEOUT,
            interaction_queue_capacity: DEFAULT_INTERACTION_QUEUE_CAPACITY,
            max_concurrent_interactions: DEFAULT_MAX_CONCURRENT_INTERACTIONS,
            interaction_admission_timeout: DEFAULT_INTERACTION_ADMISSION_TIMEOUT,
            interaction_overload: None,
            keep_raw_events: false,
        }
    }
}

impl DispatchConfig {
    /// Serial mode is the concurrent scheduler with one permit: the next
    /// handler task can only start once the previous one released it, which
    /// preserves gateway order while sharing the panic isolation and
    /// shutdown tracking of the concurrent path.
    fn handler_permits(&self) -> usize {
        match self.mode {
            EventDispatchMode::Serial => 1,
            EventDispatchMode::Concurrent => self.max_concurrent_handlers.max(1),
        }
    }

    fn backlog_warn_threshold(&self) -> usize {
        (self.queue_capacity / 2).clamp(1, EVENT_BACKLOG_WARN_THRESHOLD)
    }
}

/// Builder for the gateway [`Client`] runtime.
pub struct ClientBuilder {
    token: String,
    intents: u64,
    handler: Option<Arc<dyn ErasedEventHandler>>,
    data: TypeMap,
    application_id: Option<ApplicationId>,
    gateway_config: GatewayConnectionConfig,
    cache_config: CacheConfig,
    shard: Option<(u32, u32)>,
    presence: Option<crate::model::UpdatePresence>,
    dispatch: DispatchConfig,
    default_allowed_mentions: Option<crate::model::AllowedMentions>,
    #[cfg(feature = "cache")]
    cache_backend: Option<Arc<dyn ErasedCacheBackend>>,
}

impl ClientBuilder {
    /// Sets allowed-mentions injected into outgoing message payloads
    /// (`create_message`, `update_message`, `execute_webhook`, interaction
    /// responses) whenever the payload does not set `allowed_mentions`
    /// itself — the equivalent of discord.js's
    /// `ClientOptions#allowedMentions`. Also exposed on every event's
    /// [`Context::default_allowed_mentions`].
    pub fn default_allowed_mentions(
        mut self,
        allowed_mentions: crate::model::AllowedMentions,
    ) -> Self {
        self.default_allowed_mentions = Some(allowed_mentions);
        self
    }

    /// Registers an external [`CacheBackend`] (Redis, Valkey, ...) that
    /// receives the member, message, and presence writes the in-memory cache
    /// applies for gateway events.
    ///
    /// Writes go through one ordered worker per client, so a backend sees
    /// them in gateway order. The backend must implement the epoch contract
    /// documented on [`CacheBackend`]: one store-wide epoch, conditional
    /// (compare-and-set) clears identified by a [`CacheClearId`] so retries
    /// are idempotent, and writes that carry their epoch.
    ///
    /// Only one running client may mirror into a store, and **you must
    /// enforce that**: it is not detected when a client starts. A second
    /// client's first clear simply claims the store; both clients then
    /// report healthy while the first one's writes are silently ignored,
    /// until its next clear is rejected and it reports `superseded` in
    /// [`Context::cache_backend_health`], stops mirroring, and returns an
    /// error on shutdown.
    ///
    /// [`CacheClearId`]: crate::cache::CacheClearId
    ///
    /// The queue holds 8,192 writes and never stalls gateway processing. A
    /// write that does not fit, or that the backend rejects or panics on,
    /// starts recovery for its shard: queued writes of the shard admitted
    /// before that point are discarded, new ones are discarded until the
    /// shard has been cleared, and failed clears are retried on a backoff
    /// timer. A READY clears that shard only. A MESSAGE_UPDATE is merged
    /// into the backend's stored message (or the in-memory copy when the
    /// backend has none) and skipped when neither holds the full message.
    /// When the client stops, queued writes and pending recoveries are
    /// finished within [`ClientBuilder::handler_shutdown_timeout`]; shards
    /// left unrecovered are reported in the returned error.
    #[cfg(feature = "cache")]
    ///
    /// Pass the backend in an `Arc` (`.cache_backend(Arc::new(backend))`)
    /// and keep a clone to read the store yourself. Since 3.0 this takes
    /// `Arc<B>` for a concrete backend type instead of
    /// `Arc<dyn CacheBackend>`, because the trait's methods return
    /// `impl Future` and it is no longer object safe.
    pub fn cache_backend<B: CacheBackend + 'static>(mut self, backend: Arc<B>) -> Self {
        self.cache_backend = Some(backend);
        self
    }
    pub fn event_handler<H: EventHandler>(mut self, handler: H) -> Self {
        self.handler = Some(Arc::new(handler));
        self
    }

    /// Sets the presence sent inside IDENTIFY, so the bot connects with the
    /// desired status and activity instead of updating it after READY.
    pub fn presence(mut self, presence: crate::model::UpdatePresence) -> Self {
        self.presence = Some(presence);
        self
    }

    /// Chooses how handler calls are scheduled; see [`EventDispatchMode`].
    pub fn event_dispatch(mut self, dispatch_mode: EventDispatchMode) -> Self {
        self.dispatch.mode = dispatch_mode;
        self
    }

    /// Sets how many gateway events each shard buffers in each of its two
    /// queues (default 10,000, minimum 1).
    ///
    /// The first queue sits between the gateway connection and the internal
    /// processor. The connection's read loop also handles heartbeats, so it
    /// never waits for space: when this queue is full the event is dropped
    /// for everything, including the cache, and an error is logged. It only
    /// fills if internal processing itself stalls (for example a task holds
    /// the [`Context::voice`] write lock indefinitely).
    ///
    /// The second queue holds events waiting for a handler call. When
    /// handlers fall this far behind, further events are not delivered to
    /// the handler (the cache, collectors, and member-chunk waiters still
    /// see them) and a rate-limited warning reports how many were dropped.
    /// A backlog warning is logged once a queue is half full (at most every
    /// 30 seconds).
    pub fn event_queue_capacity(mut self, capacity: usize) -> Self {
        self.dispatch.queue_capacity = capacity.max(1);
        self
    }

    /// Keeps the raw JSON payload of every decoded event, readable through
    /// [`Event::raw`] (default `false`).
    ///
    /// Unknown dispatches and payloads that fail to decode always keep it.
    /// Known events otherwise drop it right after decoding, which keeps an
    /// event's memory at its typed size (raw payloads made a MESSAGE_CREATE
    /// five times larger); enable this only when you read fields the typed
    /// payloads do not expose.
    pub fn keep_raw_events(mut self, keep: bool) -> Self {
        self.dispatch.keep_raw_events = keep;
        self
    }

    /// Caps concurrently running handler tasks per shard in
    /// [`EventDispatchMode::Concurrent`] (default 256, minimum 1). Events
    /// wait in the handler queue while the cap is reached. Serial mode
    /// always runs one handler at a time.
    pub fn max_concurrent_handlers(mut self, limit: usize) -> Self {
        self.dispatch.max_concurrent_handlers = limit.max(1);
        self
    }

    /// Sets how long handler calls may keep running after a shard's gateway
    /// connection has ended (default 5 seconds). The window starts when the
    /// gateway exits, not when shutdown is requested. Queued events keep
    /// being delivered within it; when it expires, the remaining handler
    /// tasks are aborted and joined so [`ClientBuilder::start`] and
    /// `ShardSupervisor::wait` return promptly. The same bound applies to
    /// flushing an external cache backend's queued writes.
    pub fn handler_shutdown_timeout(mut self, timeout: Duration) -> Self {
        self.dispatch.shutdown_timeout = timeout;
        self
    }

    /// Sets how many INTERACTION_CREATE events each shard buffers for the
    /// interaction lane (default 256, minimum 1). An interaction that does
    /// not fit goes to the overload hook.
    pub fn interaction_queue_capacity(mut self, capacity: usize) -> Self {
        self.dispatch.interaction_queue_capacity = capacity.max(1);
        self
    }

    /// Reserves execution slots for interaction handlers per shard (default
    /// 32, minimum 1), separate from regular handler calls.
    ///
    /// Interactions are admitted in arrival order. Admitted interactions run
    /// concurrently with each other (up to this limit) and with regular
    /// handler calls, even in [`EventDispatchMode::Serial`], because Discord
    /// requires an initial response within three seconds.
    pub fn max_concurrent_interactions(mut self, limit: usize) -> Self {
        self.dispatch.max_concurrent_interactions = limit.max(1);
        self
    }

    /// Sets how long an interaction may wait for a slot, measured from when
    /// the shard received it (default 2 seconds). An interaction that cannot
    /// be admitted in time goes to the overload hook instead of the handler.
    pub fn interaction_admission_timeout(mut self, timeout: Duration) -> Self {
        self.dispatch.interaction_admission_timeout = timeout;
        self
    }

    /// Registers a hook for interactions that cannot be handed to the event
    /// handler in time (full interaction queue, or no slot within the
    /// admission timeout), so the application can still reply or defer —
    /// for example with an ephemeral "busy, try again" message. Without a
    /// hook such interactions are logged and dropped, and Discord shows the
    /// user "This interaction failed".
    ///
    /// The runtime never acknowledges interactions on its own: autocomplete,
    /// modal, and component interactions need different responses, so the
    /// hook decides. The [`Interaction`] it receives owns its shared response
    /// state, so replying or deferring through
    /// [`InteractionResponder`](crate::response::InteractionResponder) goes
    /// through the same acknowledgement state machine as a handler reply.
    ///
    /// The hook runs on a dedicated thread per shard (with the Tokio runtime
    /// entered, so it can `tokio::spawn` the REST call), never on the event
    /// pipeline: a slow or blocking hook cannot stall cache updates or other
    /// deliveries. Up to 64 overloaded interactions wait for it; beyond that
    /// they are dropped with a rate-limited warning. Panics in the hook are
    /// caught and logged.
    pub fn on_interaction_overload<F>(mut self, hook: F) -> Self
    where
        F: Fn(OverloadedInteraction) + Send + Sync + 'static,
    {
        self.dispatch.interaction_overload = Some(Arc::new(hook));
        self
    }

    /// Sets the application ID up front. Without it the REST client learns
    /// the ID from `READY`; application-scoped routes fail until then.
    pub fn application_id(mut self, id: ApplicationId) -> Self {
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

    /// Sets the IDENTIFY `capabilities` bitfield on the gateway connection
    /// config, e.g. [`gateway_capabilities::CHANNEL_OBFUSCATION`] to test
    /// Discord's channel obfuscation before it becomes mandatory.
    ///
    /// [`gateway_capabilities::CHANNEL_OBFUSCATION`]: crate::bitfield::gateway_capabilities::CHANNEL_OBFUSCATION
    pub fn gateway_capabilities(
        mut self,
        capabilities: crate::bitfield::GatewayCapabilities,
    ) -> Self {
        self.gateway_config = self.gateway_config.capabilities(capabilities);
        self
    }

    /// Routes every IDENTIFY of this client through `gate` (for example
    /// `ShardChildProcess::identify_gate()` in multi-process sharding), so a
    /// coordinator outside this process paces session starts per
    /// `shard_id % max_concurrency` bucket. RESUME is never gated.
    ///
    /// `spawn_shards` and `spawn_auto_shards` otherwise install their own
    /// gate (bucket pacing plus the session-start budget); a gate set here
    /// replaces it and becomes responsible for both.
    ///
    /// This is a shorthand for setting the gate on the gateway connection
    /// config, which already reaches every shard, reconnect, and resume path.
    pub fn identify_gate(mut self, gate: Arc<dyn crate::ws::IdentifyGate>) -> Self {
        self.gateway_config = self.gateway_config.identify_gate(gate);
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
        Arc::new(DiscordHttpClient::new(self.token, self.application_id))
    }

    /// Runs one shard (shard `(0, 1)` unless [`ClientBuilder::shard`] was
    /// set) until its gateway connection stops.
    ///
    /// Returns the gateway's result: `Ok(())` after a requested shutdown,
    /// or the error that stopped it, such as [`DiscordError::Gateway`] for a
    /// non-reconnectable close code (4004 authentication failed, 4014
    /// disallowed intents, ...). After the connection ends, queued events
    /// are still delivered and running handlers may finish for up to
    /// [`ClientBuilder::handler_shutdown_timeout`]; anything left is then
    /// aborted, so the call returns promptly.
    pub async fn start(self) -> Result<(), DiscordError> {
        #[cfg(feature = "cache")]
        let cache_backend = self.cache_backend.clone();
        let ClientBuilder {
            token,
            intents,
            handler,
            data,
            application_id,
            gateway_config,
            cache_config,
            shard,
            presence,
            dispatch,
            default_allowed_mentions,
            ..
        } = self;
        let handler = handler.ok_or("event_handler is required")?;
        let shard = shard.unwrap_or((0, 1));
        let runtime = SharedRuntime::new(
            &token,
            application_id,
            data,
            cache_config,
            default_allowed_mentions,
        );
        #[cfg(feature = "cache")]
        let (runtime, backend_worker) =
            runtime.with_cache_backend(cache_backend, dispatch.shutdown_timeout);
        #[cfg(feature = "cache")]
        let backend_flush_timeout = dispatch.shutdown_timeout;
        let result = start_gateway_shard(
            ShardLaunch {
                token,
                intents,
                handler,
                runtime,
                gateway_config,
                shard,
                presence,
                dispatch,
            },
            #[cfg(feature = "sharding")]
            ShardStartControl {
                supervisor_channels: None,
                boot_gate: None,
                failure: None,
            },
        )
        .await;
        // The shard's forwarders are gone, so the worker drains what is
        // queued and exits; it is joined (or aborted) before returning so no
        // backend write from this client can land after start() returned.
        #[cfg(feature = "cache")]
        if let Some(worker) = backend_worker {
            if let Err(error) = worker.shutdown(backend_flush_timeout).await {
                if result.is_ok() {
                    return Err(DiscordError::gateway(error));
                }
                warn!("{error}");
            }
        }
        result
    }

    /// Starts `shard_count` shards in this process and waits for them; the
    /// first shard error stops the others and is returned (see
    /// [`ShardSupervisor::wait`]).
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

    /// Like [`ClientBuilder::start_shards`] with Discord's recommended shard
    /// count from `/gateway/bot`.
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

    /// Starts `shard_count` shards in this process and returns their
    /// [`ShardSupervisor`] as soon as the shard tasks exist.
    ///
    /// `/gateway/bot` is read first, for `max_concurrency` and the daily
    /// session-start budget; if that request fails, so does this call
    /// (unless a gate set with [`Self::identify_gate`] takes over Discord's
    /// limits, in which case the boot waves assume `max_concurrency` 1). All
    /// shards then share one IDENTIFY gate: at most one IDENTIFY per
    /// `shard_id % max_concurrency` bucket every 5 seconds, each counted
    /// against the budget, including re-IDENTIFYs after INVALID_SESSION,
    /// close codes 4007/4009, or a reconnect before READY. An IDENTIFY the
    /// budget cannot cover waits for its reset (the budget is then read
    /// again); `Shutdown` still ends that wait at once.
    #[cfg(feature = "sharding")]
    pub async fn spawn_shards(self, shard_count: u32) -> Result<ShardSupervisor, DiscordError> {
        #[cfg(feature = "cache")]
        let cache_backend = self.cache_backend.clone();
        let ClientBuilder {
            token,
            intents,
            handler,
            data,
            application_id,
            gateway_config,
            cache_config,
            shard: _,
            presence,
            dispatch,
            default_allowed_mentions,
            ..
        } = self;
        let handler = handler.ok_or("event_handler is required")?;
        let total_shards = shard_count.max(1);
        // Even with a manual shard count, /gateway/bot supplies how many
        // shards may IDENTIFY concurrently and the session-start budget the
        // built-in IDENTIFY gate enforces. Starting without the budget would
        // silently drop that protection, so a failed fetch is an error,
        // unless the caller's own gate is responsible for Discord's limits.
        let metadata_http = DiscordHttpClient::new(&token, application_id);
        let session_start_limit = match metadata_http.get_gateway_bot().await {
            Ok(gateway_bot) => Some(gateway_bot.session_start_limit),
            Err(error) if gateway_config.identify_gate_ref().is_some() => {
                warn!("failed to fetch /gateway/bot for identify concurrency, using 1: {error}");
                None
            }
            Err(error) => return Err(error),
        };
        let boot_window_size = session_start_limit
            .as_ref()
            .map_or(1, |limit| limit.max_concurrency.max(1));
        let runtime = SharedRuntime::new(
            &token,
            application_id,
            data,
            cache_config,
            default_allowed_mentions,
        );
        #[cfg(feature = "cache")]
        let (runtime, backend_worker) =
            runtime.with_cache_backend(cache_backend, dispatch.shutdown_timeout);
        spawn_shard_supervisor(SpawnShardSupervisorConfig {
            token,
            intents,
            handler,
            runtime,
            gateway_config,
            total_shards,
            boot_window_size,
            boot_delay: SHARD_BOOT_DELAY,
            session_start_limit,
            budget_refresher: Some(metadata_http),
            presence,
            dispatch,
            #[cfg(feature = "cache")]
            background: backend_worker.and_then(CacheBackendWorker::into_task),
            #[cfg(not(feature = "cache"))]
            background: None,
        })
        .await
    }

    /// Like [`Self::spawn_shards`] with Discord's recommended shard count
    /// and gateway URL from `/gateway/bot`.
    #[cfg(feature = "sharding")]
    pub async fn spawn_auto_shards(self) -> Result<ShardSupervisor, DiscordError> {
        #[cfg(feature = "cache")]
        let cache_backend = self.cache_backend.clone();
        let ClientBuilder {
            token,
            intents,
            handler,
            data,
            application_id,
            gateway_config,
            cache_config,
            shard: _,
            presence,
            dispatch,
            default_allowed_mentions,
            ..
        } = self;
        let handler = handler.ok_or("event_handler is required")?;
        let metadata_http = DiscordHttpClient::new(&token, application_id);
        let gateway_bot = metadata_http.get_gateway_bot().await?;
        let auto_shard_plan = auto_shard_plan(&gateway_bot);
        let runtime = SharedRuntime::new(
            &token,
            application_id,
            data,
            cache_config,
            default_allowed_mentions,
        );
        #[cfg(feature = "cache")]
        let (runtime, backend_worker) =
            runtime.with_cache_backend(cache_backend, dispatch.shutdown_timeout);
        let gateway_config = gateway_config.with_base_url(gateway_bot.url);

        spawn_shard_supervisor(SpawnShardSupervisorConfig {
            token,
            intents,
            handler,
            runtime,
            gateway_config,
            total_shards: auto_shard_plan.total_shards,
            boot_window_size: auto_shard_plan.boot_window_size,
            boot_delay: SHARD_BOOT_DELAY,
            session_start_limit: Some(gateway_bot.session_start_limit),
            budget_refresher: Some(metadata_http),
            presence,
            dispatch,
            #[cfg(feature = "cache")]
            background: backend_worker.and_then(CacheBackendWorker::into_task),
            #[cfg(not(feature = "cache"))]
            background: None,
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
            presence: None,
            dispatch: DispatchConfig::default(),
            default_allowed_mentions: None,
            #[cfg(feature = "cache")]
            cache_backend: None,
        }
    }

    /// Creates a `rest` value.
    pub fn rest(
        token: impl Into<String>,
        application_id: Option<ApplicationId>,
    ) -> DiscordHttpClient {
        DiscordHttpClient::new(token, application_id)
    }
}

/// Type alias for `BotClient`.
pub type BotClient = Client;
/// Type alias for `BotClientBuilder`.
pub type BotClientBuilder = ClientBuilder;

#[cfg(feature = "sharding")]
const SHARD_BOOT_DELAY: Duration = Duration::from_millis(5_000);
/// Extra time the supervisor allows a stopping shard beyond the handler
/// shutdown timeout: the gateway flushes its close frame for up to a second
/// before the handler drain starts.
#[cfg(feature = "sharding")]
const SHARD_STOP_ALLOWANCE: Duration = Duration::from_secs(2);

#[derive(Clone)]
struct SharedRuntime {
    http: Arc<DiscordHttpClient>,
    data: Arc<RwLock<TypeMap>>,
    cache: CacheHandle,
    gateway_commands: GatewayCommandRegistry,
    member_chunk_waiters: MemberChunkWaiters,
    default_allowed_mentions: Option<crate::model::AllowedMentions>,
    #[cfg(feature = "cache")]
    cache_backend: Option<CacheBackendForwarder>,
    #[cfg(feature = "voice")]
    voice: Arc<RwLock<VoiceManager>>,
    #[cfg(feature = "collectors")]
    collectors: CollectorHub,
}

impl SharedRuntime {
    fn new(
        token: &str,
        application_id: Option<ApplicationId>,
        data: TypeMap,
        cache_config: CacheConfig,
        default_allowed_mentions: Option<crate::model::AllowedMentions>,
    ) -> Self {
        let mut http = DiscordHttpClient::new(token, application_id);
        if let Some(allowed_mentions) = default_allowed_mentions.clone() {
            http = http.with_default_allowed_mentions(allowed_mentions);
        }
        Self {
            http: Arc::new(http),
            data: Arc::new(RwLock::new(data)),
            cache: CacheHandle::with_config(cache_config),
            gateway_commands: Arc::new(StdRwLock::new(HashMap::new())),
            member_chunk_waiters: Arc::new(StdMutex::new(HashMap::new())),
            default_allowed_mentions,
            #[cfg(feature = "cache")]
            cache_backend: None,
            #[cfg(feature = "voice")]
            voice: Arc::new(RwLock::new(VoiceManager::new())),
            #[cfg(feature = "collectors")]
            collectors: CollectorHub::new(),
        }
    }

    /// Starts the client's single ordered backend worker and returns its
    /// owner. Must run inside a Tokio runtime (it is only called from the
    /// async start paths).
    #[cfg(feature = "cache")]
    fn with_cache_backend(
        mut self,
        cache_backend: Option<Arc<dyn ErasedCacheBackend>>,
        drain_timeout: Duration,
    ) -> (Self, Option<CacheBackendWorker>) {
        let Some(backend) = cache_backend else {
            return (self, None);
        };
        let (forwarder, worker) = CacheBackendForwarder::spawn(
            backend,
            backend::CACHE_BACKEND_QUEUE_CAPACITY,
            drain_timeout,
        );
        self.cache_backend = Some(forwarder);
        (self, Some(worker))
    }

    fn context(&self, shard: (u32, u32)) -> Context {
        let mut context = Context::new(Arc::clone(&self.http), Arc::clone(&self.data));
        context.cache = self.cache.clone();
        context.shard_id = shard.0;
        context.shard_count = shard.1;
        context.gateway_commands = Arc::clone(&self.gateway_commands);
        context.member_chunk_waiters = Arc::clone(&self.member_chunk_waiters);
        context.default_allowed_mentions = self.default_allowed_mentions.clone();
        #[cfg(feature = "voice")]
        {
            context.voice = Arc::clone(&self.voice);
        }
        #[cfg(feature = "collectors")]
        {
            context.collectors = self.collectors.clone();
        }
        #[cfg(feature = "cache")]
        {
            context.cache_backend_health = self
                .cache_backend
                .as_ref()
                .map(CacheBackendForwarder::health_handle);
        }
        context
    }
}

#[cfg(feature = "sharding")]
struct ShardStartControl {
    supervisor_channels: Option<ShardRuntimeChannels>,
    boot_gate: Option<watch::Receiver<bool>>,
    /// Raised before this shard drains its handlers after a fatal error.
    failure: Option<FleetFailure>,
}

#[cfg(feature = "sharding")]
struct SpawnShardSupervisorConfig {
    token: String,
    intents: u64,
    handler: Arc<dyn ErasedEventHandler>,
    runtime: SharedRuntime,
    gateway_config: GatewayConnectionConfig,
    total_shards: u32,
    /// Discord's `max_concurrency`: identify wave size and bucket count.
    boot_window_size: u32,
    /// Pause between identify waves, and the IDENTIFY spacing within one
    /// bucket ([`SHARD_BOOT_DELAY`] outside tests).
    boot_delay: Duration,
    /// Session-start budget for the built-in IDENTIFY gate. `None` leaves
    /// the budget unchecked; bucket pacing still applies.
    session_start_limit: Option<crate::model::SessionStartLimit>,
    /// Reads the budget again from `/gateway/bot` at its reset.
    budget_refresher: Option<DiscordHttpClient>,
    presence: Option<crate::model::UpdatePresence>,
    dispatch: DispatchConfig,
    /// The client's cache backend worker, handed to the supervisor.
    background: Option<tokio::task::JoinHandle<Result<(), String>>>,
}

#[cfg(feature = "sharding")]
struct AutoShardPlan {
    total_shards: u32,
    boot_window_size: u32,
}

/// Shard count and identify wave size from `/gateway/bot`. An exhausted or
/// short session-start budget is not handled here: the built-in IDENTIFY
/// gate holds each IDENTIFY that does not fit until the reset.
#[cfg(feature = "sharding")]
fn auto_shard_plan(gateway_bot: &crate::model::GatewayBot) -> AutoShardPlan {
    AutoShardPlan {
        total_shards: gateway_bot.shards.max(1),
        boot_window_size: gateway_bot.session_start_limit.max_concurrency.max(1),
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
        boot_delay,
        session_start_limit,
        budget_refresher,
        presence,
        dispatch,
        background,
    } = config;

    // Every shard of this client shares one IDENTIFY gate, so all of their
    // IDENTIFYs (first connects and the gateway client's own re-IDENTIFYs
    // after INVALID_SESSION, 4007/4009, or a reconnect before READY) are
    // spaced per `shard_id % max_concurrency` bucket and counted against
    // the session-start budget (see `InProcessIdentifyGate`). A gate the
    // caller configured takes over both duties.
    let gateway_config = if gateway_config.identify_gate_ref().is_some() {
        gateway_config
    } else {
        let gate: Arc<dyn crate::ws::IdentifyGate> =
            Arc::new(crate::sharding::process::InProcessIdentifyGate::new(
                boot_window_size,
                total_shards,
                boot_delay,
                session_start_limit.as_ref(),
                budget_refresher,
            ));
        gateway_config.identify_gate(gate)
    };

    let manager = Arc::new(StdMutex::new(ShardingManager::new(
        crate::sharding::ShardConfig::new(total_shards).gateway(gateway_config.clone()),
    )));
    // Everything fallible happens before the first task is spawned, so an
    // error cannot leave already-started shards without an owner.
    let mut prepared = Vec::with_capacity(usize::try_from(total_shards).unwrap_or(0));
    for shard_id in 0..total_shards {
        prepared.push((
            shard_id,
            lock_sharding_manager(&manager).prepare_runtime(shard_id)?,
        ));
    }

    // From here on nothing awaits until the supervisor is returned: the
    // supervisor owns the shard tasks immediately and supervises failures
    // while the boot scheduler releases later identify waves.
    let failures = FleetFailure::new();
    let mut tasks = Vec::with_capacity(prepared.len());
    let mut boot_gates = Vec::with_capacity(prepared.len());
    for (shard_id, supervisor_channels) in prepared {
        let (boot_tx, boot_rx) = watch::channel(false);
        let launch = ShardLaunch {
            token: token.clone(),
            intents,
            handler: Arc::clone(&handler),
            runtime: runtime.clone(),
            gateway_config: gateway_config.clone().shard(shard_id, total_shards),
            shard: (shard_id, total_shards),
            presence: presence.clone(),
            dispatch: dispatch.clone(),
        };
        tasks.push((
            shard_id,
            tokio::spawn(start_gateway_shard(
                launch,
                ShardStartControl {
                    supervisor_channels: Some(supervisor_channels),
                    boot_gate: Some(boot_rx),
                    failure: Some(failures.clone()),
                },
            )),
        ));
        boot_gates.push(boot_tx);
    }
    drop(runtime);

    let boot_scheduler = tokio::spawn(release_boot_waves(
        boot_gates,
        usize::try_from(boot_window_size.max(1)).unwrap_or(usize::MAX),
        boot_delay,
        failures.subscribe(),
    ));

    Ok(ShardSupervisor {
        manager,
        tasks,
        stop_timeout: dispatch
            .shutdown_timeout
            .saturating_add(SHARD_STOP_ALLOWANCE),
        boot_scheduler: Some(boot_scheduler),
        background,
        failures: Some(failures),
    })
}

/// Opens the boot gates wave by wave, `boot_delay` apart, so at most
/// `max_concurrency` shards IDENTIFY at once.
///
/// Owned by the [`ShardSupervisor`]: aborting this task drops the remaining
/// gates, and every shard still queued then stops without connecting. It
/// also stops by itself as soon as any shard raises a fleet failure — even
/// while that shard is still draining and nobody is awaiting the supervisor
/// — so a shard that fails in an early wave keeps later waves from
/// launching at all.
#[cfg(feature = "sharding")]
async fn release_boot_waves(
    boot_gates: Vec<watch::Sender<bool>>,
    wave_size: usize,
    boot_delay: Duration,
    failures: watch::Receiver<Option<(u32, String)>>,
) {
    let mut failures = Some(failures);
    let wave_count = boot_gates.len().div_ceil(wave_size);
    for (wave_index, wave) in boot_gates.chunks(wave_size).enumerate() {
        if failures
            .as_ref()
            .is_some_and(|failures| failures.borrow().is_some())
        {
            return;
        }
        for boot_gate in wave {
            // Fails only if that shard's task already ended; the supervisor
            // reports how it ended.
            boot_gate.send(true).ok();
        }
        if wave_index + 1 < wave_count {
            tokio::select! {
                biased;
                _ = failure_raised(&mut failures) => return,
                _ = sleep(boot_delay) => {}
            }
        }
    }
}

/// Everything one shard runtime needs, bundled so the start paths and tests
/// build it the same way.
struct ShardLaunch {
    token: String,
    intents: u64,
    handler: Arc<dyn ErasedEventHandler>,
    runtime: SharedRuntime,
    gateway_config: GatewayConnectionConfig,
    shard: (u32, u32),
    presence: Option<crate::model::UpdatePresence>,
    dispatch: DispatchConfig,
}

/// Keeps a shard's messenger registered while its runtime runs.
///
/// Dropped as soon as the gateway exits (and on cancellation of the shard
/// future): commands to the stopped shard then fail with "missing shard
/// messenger" instead of queueing into a channel nobody reads, and pending
/// [`Context::fetch_members`] calls on that shard fail immediately instead
/// of waiting out their timeout.
struct ShardRegistration {
    shard_id: u32,
    gateway_commands: GatewayCommandRegistry,
    member_chunk_waiters: MemberChunkWaiters,
}

impl Drop for ShardRegistration {
    fn drop(&mut self) {
        write_gateway_commands(&self.gateway_commands).remove(&self.shard_id);
        lock_member_chunk_waiters(&self.member_chunk_waiters)
            .retain(|_, waiter| waiter.shard_id != self.shard_id);
    }
}

/// Runs one shard: gateway connection, internal event processor, and
/// handler lanes.
///
/// Decision log:
/// - Purpose: separate handler execution from internal processing (cache,
///   member-chunk waiters, collectors, voice) so a Serial handler awaiting
///   `fetch_members` cannot deadlock its shard; bound queues and handler
///   tasks; keep runtime state consistent with the gateway stream; meet
///   Discord's interaction deadline; return from `start()` promptly and
///   verifiably once the gateway exits.
/// - Previous design and constraints: one unbounded queue ran internal
///   processing and the handler call back to back, and Concurrent mode
///   spawned an untracked task per event. The gateway read loop in
///   `client.rs` (outside this module) calls the event callback
///   synchronously, handles heartbeat ACKs on the same loop, and advances
///   the sequence number before the callback, so the callback must never
///   wait and a dispatch it loses can never be replayed by a RESUME.
/// - Alternatives considered: (1) stop reading the socket when full — the
///   heartbeat stalls and Discord drops the zombie connection; (2) RESUME
///   when full — the lost events are not replayed; (3) keep the unbounded
///   queue — a slow handler grows memory without limit; (4) drop and
///   continue — the cache, collectors, and member fetches silently diverge;
///   (5) two bounded stages where losing internal input stops the shard.
/// - Chosen: (5). Stage one (gateway → processor) uses `try_send`; a full or
///   closed queue, or a processor/dispatcher that dies, raises a
///   `PipelineFault` that is raced against the gateway: the connection is
///   dropped, the shard's cache (and backend mirror) is invalidated, its
///   pending member fetches fail, and the error is returned. Stage two only
///   drops handler deliveries; internal state already saw the event.
///   Regular handlers run under a semaphore (one permit in Serial mode);
///   INTERACTION_CREATE uses a separate lane with reserved slots and an
///   admission deadline that hands late interactions to an application
///   hook. Shutdown drains within `handler_shutdown_timeout`, then aborts
///   and joins every task and reports any that does not confirm.
/// - Why: it bounds memory without blocking heartbeats, never presents
///   state with a gap as current, and confines overload losses to handler
///   deliveries (or to the hook, for interactions).
/// - Trade-offs: a stalled processor stops the shard (a fresh IDENTIFY is
///   left to the caller, so session-start pacing stays in one place). A
///   Serial handler may read cache state that already includes later
///   events; interactions run concurrently with other handlers even in
///   Serial mode; handlers more than `event_queue_capacity` events behind
///   miss deliveries (reported with a running count).
async fn start_gateway_shard(
    launch: ShardLaunch,
    #[cfg(feature = "sharding")] shard_control: ShardStartControl,
) -> Result<(), DiscordError> {
    let ShardLaunch {
        token,
        intents,
        handler,
        runtime,
        gateway_config,
        shard,
        presence,
        dispatch,
    } = launch;

    #[cfg(feature = "sharding")]
    let fleet_failure = shard_control.failure.clone();
    #[cfg(feature = "sharding")]
    if let Some(mut boot_gate) = shard_control.boot_gate {
        if let Some(supervisor_channels) = shard_control.supervisor_channels.as_ref() {
            // Supervisor events are best effort: `publish` updates the shard
            // status first and fails only once nobody supervises the shard.
            supervisor_channels
                .publish(ShardSupervisorEvent::StateChanged {
                    shard_id: shard.0,
                    state: ShardRuntimeState::Queued,
                })
                .ok();
        }

        while !*boot_gate.borrow() {
            if boot_gate.changed().await.is_err() {
                if let Some(supervisor_channels) = shard_control.supervisor_channels.as_ref() {
                    // Best effort, as above.
                    supervisor_channels
                        .publish(ShardSupervisorEvent::StateChanged {
                            shard_id: shard.0,
                            state: ShardRuntimeState::Stopped,
                        })
                        .ok();
                }
                return Ok(());
            }
        }
    }

    let (gateway_command_tx, gateway_command_rx) = mpsc::channel(GATEWAY_COMMAND_QUEUE_CAPACITY);
    write_gateway_commands(&runtime.gateway_commands).insert(
        shard.0,
        ShardMessenger {
            shard_id: shard.0,
            command_tx: gateway_command_tx.clone(),
        },
    );
    let registration = ShardRegistration {
        shard_id: shard.0,
        gateway_commands: Arc::clone(&runtime.gateway_commands),
        member_chunk_waiters: Arc::clone(&runtime.member_chunk_waiters),
    };

    let shutdown_timeout = dispatch.shutdown_timeout;
    let pipeline = spawn_event_pipeline(
        handler,
        runtime.context(shard),
        #[cfg(feature = "cache")]
        runtime.cache_backend.clone(),
        dispatch,
    );
    let fault = pipeline.fault();
    let ingress = pipeline.ingress();
    let callback: EventCallback = Arc::new(move |dispatch: GatewayDispatch| {
        ingress.push(dispatch);
    });

    let mut gateway = GatewayClient::new(token, intents)
        .gateway_config(gateway_config)
        .initial_presence(presence)
        .control(gateway_command_rx);
    if shard.1 > 1 {
        gateway = gateway.shard(shard.0, shard.1);
    }
    #[cfg(feature = "sharding")]
    let mut fault_publisher = None;
    #[cfg(feature = "sharding")]
    if let Some(supervisor_channels) = shard_control.supervisor_channels {
        let (command_rx, publisher) = supervisor_channels.split();
        forward_shard_commands(command_rx, gateway_command_tx);
        fault_publisher = Some(publisher.clone());
        gateway = gateway.supervisor(shard_supervisor_callback(publisher));
    }
    // A pipeline fault means a dispatch was lost after the gateway already
    // advanced its sequence, so neither continuing nor RESUMEing can repair
    // the stream: stop the connection (dropping the run future closes it).
    let result = tokio::select! {
        result = gateway.run(callback) => result,
        reason = fault.raised() => {
            #[cfg(feature = "sharding")]
            if let Some(publisher) = fault_publisher.as_ref() {
                // Best effort: fails only once nobody supervises the shard.
                publisher
                    .publish(ShardSupervisorEvent::GatewayError {
                        shard_id: shard.0,
                        message: reason.clone(),
                    })
                    .ok();
                publisher
                    .publish(ShardSupervisorEvent::StateChanged {
                        shard_id: shard.0,
                        state: ShardRuntimeState::Stopped,
                    })
                    .ok();
            }
            Err(DiscordError::gateway(reason))
        }
    };
    drop(gateway);
    drop(registration);

    let result = match (result, fault.reason()) {
        (Ok(()), Some(reason)) => Err(DiscordError::gateway(reason)),
        (result, _) => result,
    };
    // Tell the supervisor and the boot scheduler now: draining handlers can
    // take up to the shutdown timeout, and later identify waves must not be
    // released meanwhile.
    #[cfg(feature = "sharding")]
    if let (Err(error), Some(failure)) = (&result, fleet_failure.as_ref()) {
        failure.raise(shard.0, error.to_string());
    }
    let pipeline_result = pipeline.shutdown(shutdown_timeout).await;
    if fault.is_raised() {
        // The shard's cached state no longer matches Discord: drop it (and
        // the backend mirror's copy) rather than serve it as current. The
        // pipeline has stopped, so nothing can re-insert stale entries.
        runtime.cache.clear_shard(shard.0, shard.1).await;
        #[cfg(feature = "cache")]
        if let Some(forwarder) = runtime.cache_backend.as_ref() {
            forwarder.forward(shard, BackendWrite::ClearShard);
        }
    }

    match pipeline_result {
        Err(error) if result.is_ok() => Err(invalid_data_error(format!(
            "gateway event pipeline did not stop cleanly: {error}"
        ))),
        Err(error) => {
            warn!("gateway event pipeline did not stop cleanly after gateway exit: {error}");
            result
        }
        Ok(()) => result,
    }
}

/// Applies an event to the in-memory cache. `shard` is the `(id, count)`
/// pair of the shard that received it; READY only invalidates that shard's
/// entries because every shard of a process shares this cache (see
/// [`CacheHandle::clear_shard`]).
///
/// Payloads the event holds as `Arc`s (messages, members, GUILD_CREATE's
/// members and presences) are stored as they are; everything else is
/// copied out of the shared event.
async fn apply_cache_updates(cache: &CacheHandle, shard: (u32, u32), event: &Event) {
    match event.kind() {
        EventKind::Ready(_) => {
            cache.clear_shard(shard.0, shard.1).await;
        }
        EventKind::GuildCreate(GuildCreatePayload::Available(guild)) => {
            cache.apply_guild_create(guild).await;
        }
        // Never a made-up guild: drop the stale copy, as GUILD_DELETE with
        // `unavailable: true` does. The guild comes back with a full
        // GUILD_CREATE once it is available.
        EventKind::GuildCreate(GuildCreatePayload::Unavailable(guild)) => {
            cache.remove_guild(guild.id).await;
        }
        EventKind::GuildUpdate(guild) => {
            cache.apply_guild_update(guild).await;
        }
        EventKind::GuildMembersChunk(chunk) => {
            cache
                .apply_members_chunk(chunk.guild_id, &chunk.members, chunk.presences.as_deref())
                .await;
        }
        EventKind::ThreadCreate(event) | EventKind::ThreadUpdate(event) => {
            cache.upsert_channel(Arc::clone(&event.thread)).await;
        }
        EventKind::ThreadDelete(event) => {
            cache.remove_channel(event.thread.id).await;
        }
        EventKind::ThreadListSync(event) => {
            for thread in &event.threads {
                let mut thread = thread.clone();
                if thread.guild_id.is_none() {
                    thread.guild_id = event.guild_id;
                }
                cache.upsert_channel(thread).await;
            }
        }
        EventKind::GuildDelete(data) => {
            cache.remove_guild(data.id).await;
        }
        EventKind::ChannelCreate(channel) | EventKind::ChannelUpdate(channel) => {
            cache.upsert_channel(Arc::clone(channel)).await;
        }
        EventKind::ChannelDelete(channel) => {
            cache.remove_channel(channel.id).await;
        }
        EventKind::MemberUpdate(update) => {
            // Stores the (complete) user and merges into the cached member;
            // a member the cache does not hold stays uncached.
            cache.apply_member_update(update).await;
        }
        EventKind::MemberAdd(event) => {
            if let Some(user) = event.member.user.as_ref() {
                cache.upsert_user(Arc::clone(user)).await;
                cache
                    .upsert_member(event.guild_id, user.id, Arc::clone(&event.member))
                    .await;
            }
        }
        EventKind::MemberRemove(data) => {
            cache.upsert_user(data.user.clone()).await;
            cache.remove_member(data.guild_id, data.user.id).await;
        }
        EventKind::RoleCreate(event) | EventKind::RoleUpdate(event) => {
            cache
                .upsert_role(event.guild_id, Arc::clone(&event.role))
                .await;
        }
        EventKind::RoleDelete(data) => {
            cache.remove_role(data.guild_id, data.role_id).await;
        }
        EventKind::MessageCreate(message) => {
            if let Some(author) = message.author.as_ref() {
                cache.upsert_user(Arc::clone(author)).await;
            }
            // Gateway messages without a guild are DMs. Recording the DM
            // channel ties the message to shard 0 (the only shard Discord
            // sends DMs to), so that shard's READY invalidates it instead of
            // leaving an orphan whose owner cannot be determined.
            if message.guild_id.is_none() {
                cache
                    .upsert_channel_if_absent(crate::model::Channel::stub(
                        message.channel_id,
                        None,
                        crate::model::ChannelType::DM,
                    ))
                    .await;
            }
            cache.upsert_message(Arc::clone(message)).await;
        }
        EventKind::MessageUpdate(update) => {
            // Patches the cached author (a patch, not a replacement: the
            // author is often partial and must not clear what it omits),
            // then merges into the cached message only; an update alone is
            // never stored as a message (its omitted fields are unknown).
            cache.apply_message_update(update).await;
        }
        EventKind::MessageDelete(data) => {
            cache.remove_message(data.channel_id, data.id).await;
        }
        EventKind::MessageDeleteBulk(event) => {
            cache
                .remove_messages_bulk(event.channel_id, &event.ids)
                .await;
        }
        EventKind::InteractionCreate(interaction) => {
            if let Interaction::Component(component) = &**interaction {
                if let Some(channel_id) = component.context.channel_id {
                    // Only a stub is known here; never overwrite a full
                    // channel object already held in the cache.
                    cache
                        .upsert_channel_if_absent(crate::model::Channel::stub(
                            channel_id,
                            component.context.guild_id,
                            // The interaction does not say which kind of
                            // channel; this placeholder is what 2.x stored.
                            crate::model::ChannelType::GUILD_TEXT,
                        ))
                        .await;
                }
            }
        }
        EventKind::VoiceStateUpdate(state) => {
            if let Some(member) = state.member.as_ref() {
                if let Some(user) = member.user.as_ref() {
                    cache.upsert_user(Arc::clone(user)).await;
                    if let Some(guild_id) = state.guild_id {
                        cache.upsert_member(guild_id, user.id, member.clone()).await;
                    }
                }
            }
            if let (Some(guild_id), Some(user_id)) = (state.guild_id, state.user_id) {
                if state.channel_id.is_some() {
                    cache
                        .upsert_voice_state(guild_id, user_id, (**state).clone())
                        .await;
                } else {
                    cache.remove_voice_state(guild_id, user_id).await;
                }
            }
        }
        EventKind::GuildEmojisUpdate(event) => {
            cache
                .replace_emojis(event.guild_id, event.emojis.clone())
                .await;
        }
        EventKind::GuildSoundboardSoundCreate(sound)
        | EventKind::GuildSoundboardSoundUpdate(sound) => {
            if let Some(guild_id) = sound.guild_id {
                cache
                    .upsert_soundboard_sound(guild_id, (**sound).clone())
                    .await;
            }
        }
        EventKind::GuildSoundboardSoundDelete(event) => {
            cache
                .remove_soundboard_sound(event.guild_id, event.sound_id)
                .await;
        }
        EventKind::GuildSoundboardSoundsUpdate(event) | EventKind::SoundboardSounds(event) => {
            cache
                .replace_soundboard_sounds(event.guild_id, event.soundboard_sounds.clone())
                .await;
        }
        EventKind::GuildStickersUpdate(event) => {
            if let Some(guild_id) = event.guild_id {
                cache
                    .replace_stickers(guild_id, event.stickers.clone())
                    .await;
            }
        }
        EventKind::GuildScheduledEventCreate(event)
        | EventKind::GuildScheduledEventUpdate(event) => {
            cache.upsert_scheduled_event((**event).clone()).await;
        }
        EventKind::GuildScheduledEventDelete(event) => {
            if let (Some(guild_id), Some(event_id)) = (event.guild_id, event.id) {
                cache.remove_scheduled_event(guild_id, event_id).await;
            }
        }
        EventKind::StageInstanceCreate(stage_instance)
        | EventKind::StageInstanceUpdate(stage_instance) => {
            cache
                .upsert_stage_instance((**stage_instance).clone())
                .await;
        }
        EventKind::StageInstanceDelete(stage_instance) => {
            cache
                .remove_stage_instance(stage_instance.guild_id, stage_instance.id)
                .await;
        }
        EventKind::PresenceUpdate(event) => {
            if let Some(user) = event.user.as_ref() {
                cache.apply_partial_user(user).await;
            }
            if let (Some(guild_id), Some(user_id)) = (event.guild_id, event.user_id()) {
                cache
                    .upsert_presence(guild_id, user_id, Arc::clone(&event.presence))
                    .await;
            }
        }
        EventKind::UserUpdate(user) => {
            cache.upsert_user((**user).clone()).await;
        }
        // Everything else carries no cached state (bans, integrations,
        // entitlements, invites, reactions, typing, polls, ...), or is not
        // a known payload at all.
        _ => {}
    }
}

#[cfg(feature = "voice")]
async fn apply_voice_updates(voice: &Arc<RwLock<VoiceManager>>, event: &Event) {
    // Only voice events need the manager; taking the write lock for every
    // dispatch would contend with user tasks holding `ctx.voice()`.
    match event.kind() {
        EventKind::VoiceStateUpdate(state) => {
            let mut voice = voice.write().await;
            voice.update_voice_state(state);
        }
        EventKind::VoiceServerUpdate(data) => {
            let mut voice = voice.write().await;
            voice.update_server((**data).clone());
        }
        _ => {}
    }
}

#[cfg(feature = "sharding")]
fn shard_supervisor_callback(
    supervisor_channels: crate::sharding::ShardRuntimePublisher,
) -> SupervisorCallback {
    Arc::new(move |event| {
        // Best effort: fails only once nobody supervises the shard.
        supervisor_channels.publish(event).ok();
    })
}

/// Bridges supervisor IPC commands (std channel) into the shard's gateway
/// command queue on a blocking thread.
///
/// Waits for queue space with `blocking_send` instead of `try_send`: a full
/// queue (a burst of presence updates, or a shard reconnecting) used to end
/// this loop for good, so a later `Shutdown` from the supervisor was never
/// delivered and the shard could not be stopped. The loop now only ends
/// when either side is gone.
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

            if gateway_command_tx.blocking_send(gateway_command).is_err() {
                break;
            }
        }
    });
}

#[cfg(test)]
#[path = "bot/tests.rs"]
mod tests;
