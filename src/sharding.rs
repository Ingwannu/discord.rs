/// Multi-process shard management: child spawning, auto-respawn, and
/// JSON-lines IPC (the discord.js `ShardingManager` equivalent).
#[cfg(not(target_family = "wasm"))]
#[path = "sharding/process.rs"]
pub mod process;

use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex, MutexGuard, Weak};

use serde_json::Value;

use crate::error::DiscordError;
use crate::model::GuildId;
use crate::types::invalid_data_error;
use crate::ws::GatewayConnectionConfig;

/// Most supervisor events one shard keeps until they are drained.
///
/// Events are observations only: `publish` has already applied each one to
/// the shard's status, so a full queue drops its oldest event and reports
/// the drop as one [`ShardSupervisorEvent::Lagged`] instead of growing while
/// nobody drains it. 256 covers far more than one reconnect cycle
/// (Queued, Starting, Running, Reconnecting, errors, sessions) per shard.
const SHARD_EVENT_QUEUE_CAPACITY: usize = 256;

fn lock_or_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Typed Discord API object for `ShardInfo`.
pub struct ShardInfo {
    pub id: u32,
    pub total: u32,
}

impl ShardInfo {
    pub fn identify_payload(&self) -> [u32; 2] {
        [self.id, self.total]
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Typed Discord API object for `ShardConfig`.
pub struct ShardConfig {
    pub total_shards: u32,
    pub gateway: GatewayConnectionConfig,
}

impl ShardConfig {
    /// Creates a `new` value.
    pub fn new(total_shards: u32) -> Self {
        Self {
            total_shards: total_shards.max(1),
            gateway: GatewayConnectionConfig::default(),
        }
    }

    pub fn gateway(mut self, gateway: GatewayConnectionConfig) -> Self {
        self.gateway = gateway;
        self
    }

    pub fn shard_info(&self, shard_id: u32) -> Option<ShardInfo> {
        (shard_id < self.total_shards).then_some(ShardInfo {
            id: shard_id,
            total: self.total_shards,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Typed Discord API enum for `ShardIpcMessage`.
pub enum ShardIpcMessage {
    /// Discord API enum variant `Shutdown`.
    Shutdown,
    /// Discord API enum variant `Reconnect`.
    Reconnect,
    /// Discord API enum variant `UpdatePresence`.
    UpdatePresence(String),
    /// Discord API enum variant `SendPayload`.
    SendPayload(Value),
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Typed Discord API enum for `ShardRuntimeState`.
pub enum ShardRuntimeState {
    /// Discord API enum variant `Idle`.
    Idle,
    /// Discord API enum variant `Queued`.
    Queued,
    /// Discord API enum variant `Starting`.
    Starting,
    /// Discord API enum variant `Running`.
    Running,
    /// Discord API enum variant `Reconnecting`.
    Reconnecting,
    /// Discord API enum variant `Stopped`.
    Stopped,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Typed Discord API enum for `ShardSupervisorEvent`.
pub enum ShardSupervisorEvent {
    /// Discord API enum variant `StateChanged`.
    StateChanged {
        /// Shard ID whose state changed.
        shard_id: u32,
        /// New runtime state.
        state: ShardRuntimeState,
    },
    /// Discord API enum variant `SessionEstablished`.
    SessionEstablished {
        /// Shard ID that established a session.
        shard_id: u32,
        /// Discord gateway session ID.
        session_id: String,
    },
    /// Discord API enum variant `GatewayError`.
    GatewayError {
        /// Shard ID that reported the error.
        shard_id: u32,
        /// Gateway error message.
        message: String,
    },
    /// The shard's event queue was full, so its `missed` oldest events
    /// before this point were dropped. Not a shard state: the shard's
    /// status (`ShardRuntimeHandle::status`, `ShardingManager::statuses`) is
    /// still current, because every event is applied to it when published.
    Lagged {
        /// Shard ID whose events were dropped.
        shard_id: u32,
        /// How many events were dropped since the last `Lagged`.
        missed: u64,
    },
}

/// Applies `event` to `status`. Every event is applied exactly once: by
/// `publish` for the crate's own queue, or on receipt for a caller's channel
/// (`ShardingManager::attach_runtime`), whose events bypass `publish`.
fn apply_event(status: &mut ShardRuntimeStatus, event: &ShardSupervisorEvent) {
    match event {
        ShardSupervisorEvent::StateChanged { state, .. } => status.state = state.clone(),
        ShardSupervisorEvent::SessionEstablished { session_id, .. } => {
            status.session_id = Some(session_id.clone());
            status.last_error = None;
        }
        ShardSupervisorEvent::GatewayError { message, .. } => {
            status.last_error = Some(message.clone());
        }
        ShardSupervisorEvent::Lagged { .. } => {}
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Typed Discord API object for `ShardRuntimeStatus`.
pub struct ShardRuntimeStatus {
    pub info: ShardInfo,
    pub state: ShardRuntimeState,
    pub session_id: Option<String>,
    pub last_error: Option<String>,
}

impl ShardRuntimeStatus {
    fn new(info: ShardInfo) -> Self {
        Self {
            info,
            state: ShardRuntimeState::Idle,
            session_id: None,
            last_error: None,
        }
    }
}

/// A shard's supervisor events awaiting `try_recv_event`, bounded at
/// [`SHARD_EVENT_QUEUE_CAPACITY`].
struct ShardEventQueue {
    shard_id: u32,
    events: VecDeque<ShardSupervisorEvent>,
    /// Events dropped since the last `Lagged` was handed out.
    missed: u64,
}

impl ShardEventQueue {
    fn new(shard_id: u32) -> Self {
        Self {
            shard_id,
            events: VecDeque::new(),
            missed: 0,
        }
    }

    fn push(&mut self, event: ShardSupervisorEvent) {
        if self.events.len() >= SHARD_EVENT_QUEUE_CAPACITY {
            self.events.pop_front();
            self.missed = self.missed.saturating_add(1);
        }
        self.events.push_back(event);
    }

    /// The next event, preceded by one `Lagged` if events were dropped since
    /// the last call. Drops always take the oldest events, so the `Lagged`
    /// marks exactly where the gap is (like tokio's broadcast `Lagged`).
    fn pop(&mut self) -> Option<ShardSupervisorEvent> {
        if self.missed > 0 {
            return Some(ShardSupervisorEvent::Lagged {
                shard_id: self.shard_id,
                missed: std::mem::take(&mut self.missed),
            });
        }
        self.events.pop_front()
    }
}

/// The publishing side of a shard prepared by
/// [`ShardingManager::prepare_runtime`], shared by [`ShardRuntimeChannels`]
/// and every [`ShardRuntimePublisher`] clone.
#[derive(Clone)]
struct ShardEventSink {
    // Shared with the supervisor's handles, which pop. Taken before `status`.
    queue: Arc<Mutex<ShardEventQueue>>,
    // Written here, read by the supervisor's handles.
    status: Arc<Mutex<ShardRuntimeStatus>>,
    /// Held by every sink clone; once all are gone the handles report the
    /// runtime disconnected after draining, like a closed channel.
    _publishing: Arc<()>,
    /// Alive while a handle can still drain the queue; publishing fails
    /// once none is left, like a send on a closed channel.
    supervised: Weak<()>,
}

impl ShardEventSink {
    fn new(shard_id: u32, status: Arc<Mutex<ShardRuntimeStatus>>) -> (Self, ShardEventSource) {
        let queue = Arc::new(Mutex::new(ShardEventQueue::new(shard_id)));
        let publishing = Arc::new(());
        let supervising = Arc::new(());
        let source = ShardEventSource::Queue {
            queue: Arc::clone(&queue),
            publishing: Arc::downgrade(&publishing),
            _supervising: Arc::clone(&supervising),
        };
        let sink = Self {
            queue,
            status,
            _publishing: publishing,
            supervised: Arc::downgrade(&supervising),
        };
        (sink, source)
    }

    fn publish(&self, event: ShardSupervisorEvent) -> Result<(), DiscordError> {
        // Holding the queue lock while updating the status keeps publisher
        // clones on different threads from queueing events in another order
        // than they applied them.
        let mut queue = lock_or_recover(&self.queue);
        apply_event(&mut lock_or_recover(&self.status), &event);
        if self.supervised.strong_count() == 0 {
            return Err(invalid_data_error(format!(
                "failed to publish shard supervisor event for shard {}: no supervisor handle is left",
                queue.shard_id
            )));
        }
        queue.push(event);
        Ok(())
    }
}

/// What one receive attempt on a shard found.
enum ShardEventPoll {
    Event(ShardSupervisorEvent),
    /// Nothing queued; the shard can still publish.
    Empty,
    /// Nothing queued, and every publisher of the shard is gone: its
    /// runtime has ended and no event will follow.
    Finished,
}

/// Where a [`ShardRuntimeHandle`] reads supervisor events from.
#[derive(Clone)]
enum ShardEventSource {
    /// The bounded queue of a shard from `prepare_runtime`. `publish`
    /// already applied its events to the status.
    Queue {
        // Shared with the shard's sinks, which push; every handle clone pops.
        queue: Arc<Mutex<ShardEventQueue>>,
        publishing: Weak<()>,
        _supervising: Arc<()>,
    },
    /// A std channel from `attach_runtime` or `register_ipc`. Its events
    /// were sent without `publish`, so they are applied when received. A std
    /// Receiver is neither Clone nor Sync; the lock lets every handle clone
    /// drain the one channel. Always taken before `status`.
    Channel(Arc<Mutex<Receiver<ShardSupervisorEvent>>>),
}

#[derive(Clone)]
/// Typed Discord API object for `ShardRuntimeHandle`.
pub struct ShardRuntimeHandle {
    command_tx: Sender<ShardIpcMessage>,
    events: ShardEventSource,
    _event_tx_guard: Option<Sender<ShardSupervisorEvent>>,
    // Written by the shard's publisher, read by the supervisor's handles.
    status: Arc<Mutex<ShardRuntimeStatus>>,
}

impl ShardRuntimeHandle {
    pub fn info(&self) -> ShardInfo {
        lock_or_recover(&self.status).info.clone()
    }

    pub fn status(&self) -> ShardRuntimeStatus {
        lock_or_recover(&self.status).clone()
    }

    pub fn state(&self) -> ShardRuntimeState {
        self.status().state
    }

    pub fn session_id(&self) -> Option<String> {
        self.status().session_id
    }

    pub fn last_error(&self) -> Option<String> {
        self.status().last_error
    }

    pub fn command_sender(&self) -> Sender<ShardIpcMessage> {
        self.command_tx.clone()
    }

    pub fn send(&self, message: ShardIpcMessage) -> Result<(), DiscordError> {
        self.command_tx.send(message).map_err(|error| {
            invalid_data_error(format!("failed to send shard ipc message: {error}"))
        })
    }

    /// Takes the shard's next supervisor event, if any.
    ///
    /// Events are a bounded history for observers; [`Self::status`] is the
    /// current state whether or not anyone drains them. For a shard from
    /// [`ShardingManager::prepare_runtime`] at most 256 events are kept: when
    /// more are published before they are drained, the oldest are dropped
    /// and the next call returns [`ShardSupervisorEvent::Lagged`] with the
    /// number dropped. Receiving does not change the status, which `publish`
    /// already updated. Returns an error once the queue is empty and every
    /// publisher of the shard is gone (its runtime has ended); events queued
    /// before that are still returned first.
    pub fn try_recv_event(&self) -> Result<Option<ShardSupervisorEvent>, DiscordError> {
        match self.next_event() {
            ShardEventPoll::Event(event) => Ok(Some(event)),
            ShardEventPoll::Empty => Ok(None),
            ShardEventPoll::Finished => Err(invalid_data_error(format!(
                "shard {} event channel disconnected",
                self.info().id
            ))),
        }
    }

    fn next_event(&self) -> ShardEventPoll {
        match &self.events {
            ShardEventSource::Queue {
                queue, publishing, ..
            } => {
                let mut queue = lock_or_recover(queue);
                if let Some(event) = queue.pop() {
                    ShardEventPoll::Event(event)
                } else if publishing.strong_count() == 0 {
                    ShardEventPoll::Finished
                } else {
                    ShardEventPoll::Empty
                }
            }
            ShardEventSource::Channel(receiver) => match lock_or_recover(receiver).try_recv() {
                Ok(event) => {
                    apply_event(&mut lock_or_recover(&self.status), &event);
                    ShardEventPoll::Event(event)
                }
                Err(TryRecvError::Empty) => ShardEventPoll::Empty,
                Err(TryRecvError::Disconnected) => ShardEventPoll::Finished,
            },
        }
    }
}

/// Typed Discord API object for `ShardRuntimeChannels`.
///
/// Since 3.0 events are emitted only through [`Self::publish`] (or a
/// [`ShardRuntimePublisher`]); the raw unbounded `event_tx` sender is gone
/// because nothing bounded what it kept.
pub struct ShardRuntimeChannels {
    pub info: ShardInfo,
    pub command_rx: Receiver<ShardIpcMessage>,
    sink: ShardEventSink,
}

#[derive(Clone)]
/// Typed Discord API object for `ShardRuntimePublisher`.
pub struct ShardRuntimePublisher {
    pub info: ShardInfo,
    sink: ShardEventSink,
}

impl ShardRuntimePublisher {
    /// Applies `event` to the shard's status, then queues it for observers
    /// (bounded; see [`ShardRuntimeHandle::try_recv_event`]). The status is
    /// updated even when this fails, which happens only once no supervisor
    /// handle for the shard is left.
    pub fn publish(&self, event: ShardSupervisorEvent) -> Result<(), DiscordError> {
        self.sink.publish(event)
    }
}

impl ShardRuntimeChannels {
    pub fn status(&self) -> ShardRuntimeStatus {
        lock_or_recover(&self.sink.status).clone()
    }

    pub fn state(&self) -> ShardRuntimeState {
        self.status().state
    }

    pub fn set_state(&self, state: ShardRuntimeState) {
        lock_or_recover(&self.sink.status).state = state;
    }

    /// See [`ShardRuntimePublisher::publish`].
    pub fn publish(&self, event: ShardSupervisorEvent) -> Result<(), DiscordError> {
        self.sink.publish(event)
    }

    pub fn publisher(&self) -> ShardRuntimePublisher {
        ShardRuntimePublisher {
            info: self.info.clone(),
            sink: self.sink.clone(),
        }
    }

    pub fn split(self) -> (Receiver<ShardIpcMessage>, ShardRuntimePublisher) {
        let publisher = ShardRuntimePublisher {
            info: self.info,
            sink: self.sink,
        };
        (self.command_rx, publisher)
    }
}

/// Typed Discord API object for `ShardingManager`.
pub struct ShardingManager {
    config: ShardConfig,
    runtimes: HashMap<u32, ShardRuntimeHandle>,
}

impl ShardingManager {
    /// Creates a `new` value.
    pub fn new(config: ShardConfig) -> Self {
        Self {
            config,
            runtimes: HashMap::new(),
        }
    }

    pub fn shard_infos(&self) -> Vec<ShardInfo> {
        (0..self.config.total_shards)
            .map(|id| ShardInfo {
                id,
                total: self.config.total_shards,
            })
            .collect()
    }

    /// Returns the shard that owns `guild_id` (`(guild_id >> 22) %
    /// total_shards`), or `None` when no shards are configured.
    pub fn shard_for_guild(&self, guild_id: GuildId) -> Option<ShardInfo> {
        let shard_id = crate::cache::shard_for_guild_id(guild_id, self.config.total_shards);
        self.config.shard_info(shard_id)
    }

    pub fn gateway_config(&self, shard_id: u32) -> Option<GatewayConnectionConfig> {
        self.config
            .shard_info(shard_id)
            .map(|info| self.config.gateway.clone().shard(info.id, info.total))
    }

    /// Registers a shard driven through the caller's own channels.
    ///
    /// Events the caller sends on `event_rx`'s sender bypass `publish`, so
    /// they update the status when they are received, and the channel's
    /// capacity is the caller's. [`Self::prepare_runtime`] keeps the status
    /// current without draining and bounds the event history.
    pub fn attach_runtime(
        &mut self,
        shard_id: u32,
        command_tx: Sender<ShardIpcMessage>,
        event_rx: Receiver<ShardSupervisorEvent>,
    ) -> Result<ShardRuntimeHandle, DiscordError> {
        let info = self.shard_info_or_error(shard_id)?;
        let handle = ShardRuntimeHandle {
            command_tx,
            events: ShardEventSource::Channel(Arc::new(Mutex::new(event_rx))),
            _event_tx_guard: None,
            status: Arc::new(Mutex::new(ShardRuntimeStatus::new(info))),
        };

        self.runtimes.insert(shard_id, handle.clone());
        Ok(handle)
    }

    /// Registers a shard and returns the channels its runtime uses: the
    /// command receiver and the event publisher.
    ///
    /// Published events update the shard's status immediately and are kept
    /// for observers in a queue of at most 256 events (see
    /// [`ShardRuntimeHandle::try_recv_event`]), so memory stays bounded even
    /// if [`Self::drain_events`] is never called.
    pub fn prepare_runtime(&mut self, shard_id: u32) -> Result<ShardRuntimeChannels, DiscordError> {
        let info = self.shard_info_or_error(shard_id)?;
        let (command_tx, command_rx) = channel();
        let status = Arc::new(Mutex::new(ShardRuntimeStatus::new(info.clone())));
        let (sink, events) = ShardEventSink::new(shard_id, Arc::clone(&status));
        self.runtimes.insert(
            shard_id,
            ShardRuntimeHandle {
                command_tx,
                events,
                _event_tx_guard: None,
                status,
            },
        );

        Ok(ShardRuntimeChannels {
            info,
            command_rx,
            sink,
        })
    }

    pub fn handle(&self, shard_id: u32) -> Option<ShardRuntimeHandle> {
        self.runtimes.get(&shard_id).cloned()
    }

    pub fn state(&self, shard_id: u32) -> Option<ShardRuntimeState> {
        self.runtimes.get(&shard_id).map(ShardRuntimeHandle::state)
    }

    pub fn status(&self, shard_id: u32) -> Option<ShardRuntimeStatus> {
        self.runtimes.get(&shard_id).map(ShardRuntimeHandle::status)
    }

    pub fn states(&self) -> Vec<(ShardInfo, ShardRuntimeState)> {
        self.runtimes
            .values()
            .map(|handle| (handle.info(), handle.state()))
            .collect()
    }

    pub fn statuses(&self) -> Vec<ShardRuntimeStatus> {
        self.runtimes
            .values()
            .map(ShardRuntimeHandle::status)
            .collect()
    }

    pub fn runtime_count(&self) -> usize {
        self.runtimes.len()
    }

    pub fn remove_runtime(&mut self, shard_id: u32) -> Option<ShardRuntimeHandle> {
        self.runtimes.remove(&shard_id)
    }

    pub fn poll_event(&self, shard_id: u32) -> Result<Option<ShardSupervisorEvent>, DiscordError> {
        let Some(handle) = self.runtimes.get(&shard_id) else {
            return Err(invalid_data_error(format!(
                "missing shard runtime for shard {shard_id}"
            )));
        };
        handle.try_recv_event()
    }

    /// Takes every queued supervisor event of every shard, including
    /// [`ShardSupervisorEvent::Lagged`] markers where events were dropped
    /// (see [`ShardRuntimeHandle::try_recv_event`]). Statuses are not
    /// changed by draining; [`Self::statuses`] is always current.
    ///
    /// A shard whose runtime has ended (every publisher dropped) is drained
    /// like any other, so its last events, such as a final `Stopped`, are
    /// returned, and is skipped afterwards; its last status stays in
    /// [`Self::statuses`]. An ended shard never makes this fail or discard
    /// events already taken. The only error is "nothing left at all": no
    /// event to return while every registered shard has ended, so a loop
    /// that drains until an error stops after the last shard's final events.
    /// Use [`Self::poll_event`] to watch one shard end.
    pub fn drain_events(&self) -> Result<Vec<ShardSupervisorEvent>, DiscordError> {
        let mut events = Vec::new();
        let mut running_shards = 0_usize;
        for handle in self.runtimes.values() {
            loop {
                match handle.next_event() {
                    ShardEventPoll::Event(event) => events.push(event),
                    ShardEventPoll::Empty => {
                        running_shards += 1;
                        break;
                    }
                    ShardEventPoll::Finished => break,
                }
            }
        }

        if events.is_empty() && running_shards == 0 && !self.runtimes.is_empty() {
            return Err(invalid_data_error(
                "every shard's event channel disconnected",
            ));
        }
        Ok(events)
    }

    pub fn register_ipc(&mut self, shard_id: u32, sender: Sender<ShardIpcMessage>) {
        if let Some(handle) = self.runtimes.get_mut(&shard_id) {
            handle.command_tx = sender;
            return;
        }

        if let Some(info) = self.config.shard_info(shard_id) {
            let (event_tx, event_rx) = channel();
            self.runtimes.insert(
                shard_id,
                ShardRuntimeHandle {
                    command_tx: sender,
                    events: ShardEventSource::Channel(Arc::new(Mutex::new(event_rx))),
                    _event_tx_guard: Some(event_tx),
                    status: Arc::new(Mutex::new(ShardRuntimeStatus::new(info))),
                },
            );
        }
    }

    pub fn send(&self, shard_id: u32, message: ShardIpcMessage) -> Result<(), DiscordError> {
        let Some(handle) = self.runtimes.get(&shard_id) else {
            return Err(invalid_data_error(format!(
                "missing shard ipc channel for shard {shard_id}"
            )));
        };

        handle.send(message)
    }

    /// Sends `message` to every registered shard runtime.
    ///
    /// Every shard is attempted even if some sends fail; the first failure
    /// is returned afterwards. Stopping on the first error meant a single
    /// already-stopped shard (whose command channel is closed) kept a
    /// `Shutdown` from reaching all shards iterated after it.
    pub fn broadcast(&self, message: ShardIpcMessage) -> Result<(), DiscordError> {
        let mut first_error = None;
        for handle in self.runtimes.values() {
            if let Err(error) = handle.send(message.clone()) {
                first_error.get_or_insert(error);
            }
        }

        first_error.map_or(Ok(()), Err)
    }

    fn shard_info_or_error(&self, shard_id: u32) -> Result<ShardInfo, DiscordError> {
        self.config
            .shard_info(shard_id)
            .ok_or_else(|| invalid_data_error(format!("invalid shard id {shard_id}")))
    }
}

#[cfg(test)]
mod tests {
    use crate::model::test_id;
    use std::sync::mpsc::channel;

    use super::{
        ShardConfig, ShardEventSource, ShardIpcMessage, ShardRuntimeState, ShardSupervisorEvent,
        ShardingManager, SHARD_EVENT_QUEUE_CAPACITY,
    };

    #[test]
    fn shard_for_guild_uses_discord_formula() {
        let manager = ShardingManager::new(ShardConfig::new(4));
        let shard = manager.shard_for_guild(test_id(81384788765712384));

        assert_eq!(shard.unwrap().total, 4);
    }

    #[test]
    fn gateway_config_applies_shard_pair() {
        let manager = ShardingManager::new(ShardConfig::new(2));
        let config = manager.gateway_config(1).unwrap();

        assert_eq!(
            config.normalized_url(),
            "wss://gateway.discord.gg/?v=10&encoding=json&shard=1,2"
        );
    }

    #[test]
    fn prepare_runtime_exposes_command_and_event_channels() {
        let mut manager = ShardingManager::new(ShardConfig::new(2));
        let channels = manager.prepare_runtime(1).unwrap();

        manager.send(1, ShardIpcMessage::Reconnect).unwrap();
        assert_eq!(
            channels.command_rx.recv().unwrap(),
            ShardIpcMessage::Reconnect
        );

        channels
            .publish(ShardSupervisorEvent::StateChanged {
                shard_id: 1,
                state: ShardRuntimeState::Running,
            })
            .unwrap();
        channels
            .publish(ShardSupervisorEvent::SessionEstablished {
                shard_id: 1,
                session_id: "session-1".to_string(),
            })
            .unwrap();

        let event = manager.poll_event(1).unwrap().unwrap();
        assert_eq!(
            event,
            ShardSupervisorEvent::StateChanged {
                shard_id: 1,
                state: ShardRuntimeState::Running,
            }
        );
        assert_eq!(manager.state(1), Some(ShardRuntimeState::Running));
        assert_eq!(
            manager.status(1).unwrap().session_id.as_deref(),
            Some("session-1")
        );
    }

    #[test]
    fn drain_events_collects_across_shards() {
        let mut manager = ShardingManager::new(ShardConfig::new(2));
        let shard_zero = manager.prepare_runtime(0).unwrap();
        let shard_one = manager.prepare_runtime(1).unwrap();

        shard_zero
            .publish(ShardSupervisorEvent::SessionEstablished {
                shard_id: 0,
                session_id: "session-a".to_string(),
            })
            .unwrap();
        shard_one
            .publish(ShardSupervisorEvent::GatewayError {
                shard_id: 1,
                message: "boom".to_string(),
            })
            .unwrap();

        let events = manager.drain_events().unwrap();
        assert_eq!(events.len(), 2);
        assert!(manager.drain_events().unwrap().is_empty());
    }

    fn state_changed(state: ShardRuntimeState) -> ShardSupervisorEvent {
        ShardSupervisorEvent::StateChanged { shard_id: 0, state }
    }

    #[test]
    fn receiving_events_does_not_reapply_them_over_the_current_status() {
        let mut manager = ShardingManager::new(ShardConfig::new(1));
        let channels = manager.prepare_runtime(0).unwrap();

        channels
            .publish(state_changed(ShardRuntimeState::Running))
            .unwrap();
        channels
            .publish(ShardSupervisorEvent::SessionEstablished {
                shard_id: 0,
                session_id: "session-1".to_string(),
            })
            .unwrap();
        channels
            .publish(state_changed(ShardRuntimeState::Reconnecting))
            .unwrap();
        channels
            .publish(ShardSupervisorEvent::GatewayError {
                shard_id: 0,
                message: "reset".to_string(),
            })
            .unwrap();

        // Each older event that is received must leave the newer status
        // alone: re-applying them used to roll the state back to Running and
        // clear the newer error.
        for expected in 0..3 {
            assert!(manager.poll_event(0).unwrap().is_some(), "event {expected}");
            let status = manager.status(0).unwrap();
            assert_eq!(status.state, ShardRuntimeState::Reconnecting);
            assert_eq!(status.session_id.as_deref(), Some("session-1"));
            assert_eq!(status.last_error.as_deref(), Some("reset"));
        }
    }

    #[test]
    fn undrained_events_are_bounded_and_report_how_many_were_dropped() {
        let mut manager = ShardingManager::new(ShardConfig::new(1));
        let channels = manager.prepare_runtime(0).unwrap();
        let states = [ShardRuntimeState::Running, ShardRuntimeState::Reconnecting];
        let published = SHARD_EVENT_QUEUE_CAPACITY * 10;

        // Reconnect churn while nobody drains.
        for index in 0..published {
            channels
                .publish(state_changed(states[index % 2].clone()))
                .unwrap();
        }
        let ShardEventSource::Queue { queue, .. } = &manager.handle(0).unwrap().events else {
            panic!("prepare_runtime uses the bounded queue");
        };
        assert_eq!(
            queue.lock().unwrap().events.len(),
            SHARD_EVENT_QUEUE_CAPACITY
        );
        assert_eq!(manager.state(0), Some(ShardRuntimeState::Reconnecting));

        // The gap is reported first, then the newest events in order.
        let events = manager.drain_events().unwrap();
        let dropped = published - SHARD_EVENT_QUEUE_CAPACITY;
        assert_eq!(events.len(), SHARD_EVENT_QUEUE_CAPACITY + 1);
        assert_eq!(
            events[0],
            ShardSupervisorEvent::Lagged {
                shard_id: 0,
                missed: u64::try_from(dropped).unwrap(),
            }
        );
        for (offset, event) in events[1..].iter().enumerate() {
            assert_eq!(
                event,
                &state_changed(states[(dropped + offset) % 2].clone())
            );
        }
        assert_eq!(manager.state(0), Some(ShardRuntimeState::Reconnecting));

        // A later overflow reports only the events dropped since then.
        for _ in 0..SHARD_EVENT_QUEUE_CAPACITY + 2 {
            channels
                .publish(state_changed(ShardRuntimeState::Running))
                .unwrap();
        }
        assert_eq!(
            manager.poll_event(0).unwrap(),
            Some(ShardSupervisorEvent::Lagged {
                shard_id: 0,
                missed: 2,
            })
        );
        assert_eq!(
            manager.poll_event(0).unwrap(),
            Some(state_changed(ShardRuntimeState::Running))
        );
    }

    #[test]
    fn a_finished_shard_does_not_make_drain_events_lose_events() {
        let event_shard = |event: &ShardSupervisorEvent| match event {
            ShardSupervisorEvent::StateChanged { shard_id, .. }
            | ShardSupervisorEvent::SessionEstablished { shard_id, .. }
            | ShardSupervisorEvent::GatewayError { shard_id, .. }
            | ShardSupervisorEvent::Lagged { shard_id, .. } => *shard_id,
        };
        let on = |shard_id, state| ShardSupervisorEvent::StateChanged { shard_id, state };
        let mut manager = ShardingManager::new(ShardConfig::new(2));
        let shard_zero = manager.prepare_runtime(0).unwrap();
        let shard_one = manager.prepare_runtime(1).unwrap();

        shard_zero
            .publish(on(0, ShardRuntimeState::Running))
            .unwrap();
        for _ in 0..=SHARD_EVENT_QUEUE_CAPACITY {
            shard_one
                .publish(on(1, ShardRuntimeState::Running))
                .unwrap();
        }
        // Shard 0 stops and its last publisher goes away.
        shard_zero
            .publish(on(0, ShardRuntimeState::Stopped))
            .unwrap();
        drop(shard_zero);

        // Nothing is lost: shard 0's final events, shard 1's Lagged marker,
        // and shard 1's queue, whatever order the shards are visited in.
        let events = manager.drain_events().unwrap();
        assert_eq!(events.len(), 2 + 1 + SHARD_EVENT_QUEUE_CAPACITY);
        let shard_zero_events: Vec<_> = events
            .iter()
            .filter(|event| event_shard(event) == 0)
            .cloned()
            .collect();
        assert_eq!(
            shard_zero_events,
            vec![
                on(0, ShardRuntimeState::Running),
                on(0, ShardRuntimeState::Stopped)
            ]
        );
        assert!(events.contains(&ShardSupervisorEvent::Lagged {
            shard_id: 1,
            missed: 1,
        }));

        // The finished shard keeps its status, is skipped by later drains,
        // and still reports its end to per-shard polling.
        assert_eq!(manager.state(0), Some(ShardRuntimeState::Stopped));
        assert!(manager.drain_events().unwrap().is_empty());
        shard_one
            .publish(on(1, ShardRuntimeState::Reconnecting))
            .unwrap();
        assert_eq!(
            manager.drain_events().unwrap(),
            vec![on(1, ShardRuntimeState::Reconnecting)]
        );
        assert!(manager
            .poll_event(0)
            .unwrap_err()
            .to_string()
            .contains("event channel disconnected"));

        // Once the last shard has ended, its final events still come first;
        // only an empty drain with every shard gone is an error.
        shard_one
            .publish(on(1, ShardRuntimeState::Stopped))
            .unwrap();
        drop(shard_one);
        assert_eq!(
            manager.drain_events().unwrap(),
            vec![on(1, ShardRuntimeState::Stopped)]
        );
        assert!(manager
            .drain_events()
            .unwrap_err()
            .to_string()
            .contains("every shard's event channel disconnected"));
        assert_eq!(manager.statuses().len(), 2);

        // No shards at all is not "every shard ended".
        let empty = ShardingManager::new(ShardConfig::new(1));
        assert!(empty.drain_events().unwrap().is_empty());
    }

    #[test]
    fn publishing_without_a_supervisor_updates_status_and_fails() {
        let mut manager = ShardingManager::new(ShardConfig::new(1));
        let channels = manager.prepare_runtime(0).unwrap();
        drop(manager.remove_runtime(0));

        let error = channels
            .publish(state_changed(ShardRuntimeState::Stopped))
            .unwrap_err();
        assert!(error.to_string().contains("no supervisor handle is left"));
        assert_eq!(channels.state(), ShardRuntimeState::Stopped);
    }

    #[test]
    fn queued_events_are_delivered_before_a_stopped_runtime_reports_disconnect() {
        let mut manager = ShardingManager::new(ShardConfig::new(1));
        let channels = manager.prepare_runtime(0).unwrap();
        let publisher = channels.publisher();
        drop(channels);
        publisher
            .publish(state_changed(ShardRuntimeState::Stopped))
            .unwrap();
        drop(publisher);

        assert_eq!(
            manager.poll_event(0).unwrap(),
            Some(state_changed(ShardRuntimeState::Stopped))
        );
        assert!(manager
            .poll_event(0)
            .unwrap_err()
            .to_string()
            .contains("event channel disconnected"));
    }

    #[test]
    fn attach_runtime_supports_external_supervisor_channels() {
        let mut manager = ShardingManager::new(ShardConfig::new(1));
        let (command_tx, command_rx) = channel();
        let (event_tx, event_rx) = channel();

        let handle = manager.attach_runtime(0, command_tx, event_rx).unwrap();

        handle.send(ShardIpcMessage::Shutdown).unwrap();
        assert_eq!(command_rx.recv().unwrap(), ShardIpcMessage::Shutdown);

        event_tx
            .send(ShardSupervisorEvent::GatewayError {
                shard_id: 0,
                message: "gateway closed".to_string(),
            })
            .unwrap();

        assert_eq!(
            handle.try_recv_event().unwrap(),
            Some(ShardSupervisorEvent::GatewayError {
                shard_id: 0,
                message: "gateway closed".to_string(),
            })
        );
        assert_eq!(
            manager.status(0).unwrap().last_error.as_deref(),
            Some("gateway closed")
        );
        assert_eq!(manager.runtime_count(), 1);

        manager.remove_runtime(0).unwrap();
        assert_eq!(manager.runtime_count(), 0);
    }

    #[test]
    fn shard_config_clamps_total_and_reports_invalid_ids() {
        let config = ShardConfig::new(0);
        assert_eq!(config.total_shards, 1);
        assert_eq!(config.shard_info(0).unwrap().identify_payload(), [0, 1]);
        assert!(config.shard_info(1).is_none());
    }

    #[test]
    fn sharding_manager_registers_ipc_broadcasts_and_reports_missing_runtimes() {
        let mut manager = ShardingManager::new(ShardConfig::new(2));
        let (first_tx, first_rx) = channel();
        let (second_tx, second_rx) = channel();

        manager.register_ipc(0, first_tx);
        manager.register_ipc(1, second_tx);

        assert_eq!(manager.runtime_count(), 2);
        assert_eq!(manager.shard_infos().len(), 2);
        assert_eq!(manager.handle(0).unwrap().info().id, 0);
        assert!(manager.state(0).is_some());
        assert_eq!(manager.states().len(), 2);
        assert_eq!(manager.statuses().len(), 2);

        manager
            .broadcast(ShardIpcMessage::UpdatePresence("busy".to_string()))
            .unwrap();
        assert_eq!(
            first_rx.recv().unwrap(),
            ShardIpcMessage::UpdatePresence("busy".to_string())
        );
        assert_eq!(
            second_rx.recv().unwrap(),
            ShardIpcMessage::UpdatePresence("busy".to_string())
        );

        let missing_send = manager.send(9, ShardIpcMessage::Shutdown).unwrap_err();
        assert!(missing_send
            .to_string()
            .contains("missing shard ipc channel"));

        let missing_event = manager.poll_event(9).unwrap_err();
        assert!(missing_event
            .to_string()
            .contains("missing shard runtime for shard 9"));
    }

    #[test]
    fn broadcast_reaches_every_live_shard_even_if_one_channel_is_closed() {
        let mut manager = ShardingManager::new(ShardConfig::new(8));
        let mut runtimes: Vec<_> = (0..8)
            .map(|shard_id| Some(manager.prepare_runtime(shard_id).unwrap()))
            .collect();
        // Shard 0 already stopped: its command receiver is gone.
        runtimes[0] = None;

        let error = manager
            .broadcast(ShardIpcMessage::Shutdown)
            .expect_err("the closed shard is reported");
        assert!(!error.to_string().is_empty());
        for runtime in runtimes.iter().flatten() {
            assert_eq!(
                runtime.command_rx.try_recv().unwrap(),
                ShardIpcMessage::Shutdown,
                "shard {} missed the broadcast",
                runtime.info.id
            );
        }
    }

    #[test]
    fn register_ipc_keeps_empty_event_receiver_open() {
        let mut manager = ShardingManager::new(ShardConfig::new(1));
        let (command_tx, _command_rx) = channel();

        manager.register_ipc(0, command_tx);

        assert_eq!(manager.poll_event(0).unwrap(), None);
    }

    #[test]
    fn shard_runtime_channels_manage_status_and_split_publishers() {
        let mut manager = ShardingManager::new(ShardConfig::new(1));
        let channels = manager.prepare_runtime(0).unwrap();
        assert_eq!(channels.state(), ShardRuntimeState::Idle);

        channels.set_state(ShardRuntimeState::Queued);
        assert_eq!(channels.state(), ShardRuntimeState::Queued);

        let publisher = channels.publisher();
        publisher
            .publish(ShardSupervisorEvent::GatewayError {
                shard_id: 0,
                message: "boom".to_string(),
            })
            .unwrap();
        assert_eq!(
            manager.status(0).unwrap().last_error.as_deref(),
            Some("boom")
        );

        let channels = manager.prepare_runtime(0).unwrap();
        let (command_rx, publisher) = channels.split();
        drop(command_rx);
        publisher
            .publish(ShardSupervisorEvent::StateChanged {
                shard_id: 0,
                state: ShardRuntimeState::Running,
            })
            .unwrap();
        assert_eq!(publisher.info.identify_payload(), [0, 1]);
    }
}
