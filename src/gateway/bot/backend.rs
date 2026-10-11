//! Ordered, epoch-fenced forwarding of cache writes to an external
//! [`CacheBackend`].
//!
//! Decision log:
//! - Purpose: mirror in-memory cache writes to an external store in gateway
//!   order without stalling gateway processing, and never let the mirror
//!   silently diverge — not under overload, not after backend errors or
//!   panics, not across a shard-count change, and not when a stopped client
//!   still has requests in flight.
//! - Previous design and constraints: an ordered queue that dropped writes
//!   under load; then per-shard resyncs whose barrier was `failed ticket +
//!   1`, so writes admitted before the failure ran after the clear and a
//!   delete quarantined during recovery left their data behind; and epochs
//!   per `(shard_id, shard_count)` with unconditional clears, so a full clear
//!   or a new shard layout did not invalidate old fences and an old client's
//!   late clear could erase newer data.
//! - Alternatives considered: (1) per-shard epochs with conditional clears —
//!   still needs cross-layout ordering rules; (2) leases/ownership tokens in
//!   the store — more protocol than a cache needs; (3) one store-wide epoch
//!   with compare-and-set clears and a single writing client per store.
//! - Chosen: (3).
//!   - Every clear (full or per shard) is conditional on the epoch this
//!     client last observed and increments it; every write carries the epoch
//!     and the store ignores older ones. Any clear invalidates all earlier
//!     epochs, which covers shard-count changes. A client's first clear is a
//!     full clear that claims the store; a later rejected clear means another
//!     client took over, and this client stops mirroring (superseded).
//!   - Every clear carries a client-generated [`CacheClearId`]. A clear whose
//!     call failed stays in doubt and is retried with the same id and
//!     expected epoch before any other store operation; the store answers a
//!     retry of its last applied clear with that clear's epoch, so a lost
//!     response cannot make the sole writer look superseded, and no epoch is
//!     adopted unless the store confirms this client's own clear.
//!   - A write that does not fit, or that the backend rejects or panics on,
//!     starts recovery for its shard: the boundary is frozen at the highest
//!     admitted ticket, every queued write of that shard below it is
//!     discarded, writes arriving during recovery are discarded, and the
//!     shard is cleared once the worker passed the boundary. Failed clears
//!     retry on a backoff timer; after the queue closes the worker keeps
//!     retrying within the shutdown bound and reports shards it could not
//!     recover.
//! - Why: one counter and one comparison give the store everything it needs
//!   to reject every stale write or clear, and discard-before-clear means no
//!   pre-recovery write can be replayed on top of the clear.
//! - Trade-offs: only one client may mirror into a store, and that must be
//!   enforced by the deployment (a second client is not detected until the
//!   first one's next clear is rejected); a recovering shard
//!   loses the writes discarded during recovery until later events refill
//!   it. Writes are serialized, so throughput is bound by backend latency,
//!   and each MESSAGE_UPDATE costs one extra read.

use std::collections::BTreeMap;
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::pin::Pin;
use std::sync::{Arc, Mutex as StdMutex, MutexGuard as StdMutexGuard};

use futures_util::FutureExt;
use tokio::sync::mpsc;
use tokio::time::{Duration, Instant};
use tracing::{error, warn};

use super::pipeline::{AbortOnDrop, ABORT_CONFIRM_GRACE};
use super::{merge_message_update, presence_from_update, MessageUpdateFields};
use crate::cache::{CacheBackend, CacheClear, CacheClearId, CacheHandle};
use crate::error::DiscordError;
use crate::event::Event;
use crate::model::{GenericId, Member, Message, Presence};

/// Boxed, `Send` future returned by [`ErasedCacheBackend`].
type BackendFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, DiscordError>> + Send + 'a>>;

/// Object-safe view of a [`CacheBackend`], stored by the client as
/// `Arc<dyn ErasedCacheBackend>`.
///
/// The public trait returns `impl Future + Send` and is therefore not object
/// safe; the same reasoning as `ErasedEventHandler` in `bot.rs` applies
/// (no async-trait, one allocation per call as before). It mirrors only the
/// methods the worker below calls, under their public names so the worker
/// reads the same as with the public trait. Each boxed future makes the
/// inner call only when first polled, so [`contained`] also catches a
/// backend that panics while creating its future, not only while running
/// it.
pub(super) trait ErasedCacheBackend: Send + Sync + 'static {
    fn clear_cache(
        &self,
        expected_epoch: u64,
        clear_id: CacheClearId,
    ) -> BackendFuture<'_, CacheClear>;
    fn clear_shard_cache(
        &self,
        expected_epoch: u64,
        clear_id: CacheClearId,
        shard_id: u32,
        shard_count: u32,
    ) -> BackendFuture<'_, CacheClear>;
    fn put_member(
        &self,
        epoch: u64,
        guild_id: GenericId,
        user_id: GenericId,
        member: Member,
    ) -> BackendFuture<'_, ()>;
    fn delete_member<'a>(
        &'a self,
        epoch: u64,
        guild_id: &'a GenericId,
        user_id: &'a GenericId,
    ) -> BackendFuture<'a, ()>;
    fn put_message(&self, epoch: u64, message: Message) -> BackendFuture<'_, ()>;
    fn get_message<'a>(
        &'a self,
        channel_id: &'a GenericId,
        message_id: &'a GenericId,
    ) -> BackendFuture<'a, Option<Arc<Message>>>;
    fn delete_message<'a>(
        &'a self,
        epoch: u64,
        channel_id: &'a GenericId,
        message_id: &'a GenericId,
    ) -> BackendFuture<'a, ()>;
    fn put_presence(
        &self,
        epoch: u64,
        guild_id: GenericId,
        user_id: GenericId,
        presence: Presence,
    ) -> BackendFuture<'_, ()>;
}

impl<B: CacheBackend + 'static> ErasedCacheBackend for B {
    fn clear_cache(
        &self,
        expected_epoch: u64,
        clear_id: CacheClearId,
    ) -> BackendFuture<'_, CacheClear> {
        Box::pin(async move { CacheBackend::clear_cache(self, expected_epoch, clear_id).await })
    }

    fn clear_shard_cache(
        &self,
        expected_epoch: u64,
        clear_id: CacheClearId,
        shard_id: u32,
        shard_count: u32,
    ) -> BackendFuture<'_, CacheClear> {
        Box::pin(async move {
            CacheBackend::clear_shard_cache(self, expected_epoch, clear_id, shard_id, shard_count)
                .await
        })
    }

    fn put_member(
        &self,
        epoch: u64,
        guild_id: GenericId,
        user_id: GenericId,
        member: Member,
    ) -> BackendFuture<'_, ()> {
        Box::pin(
            async move { CacheBackend::put_member(self, epoch, guild_id, user_id, member).await },
        )
    }

    fn delete_member<'a>(
        &'a self,
        epoch: u64,
        guild_id: &'a GenericId,
        user_id: &'a GenericId,
    ) -> BackendFuture<'a, ()> {
        Box::pin(async move { CacheBackend::delete_member(self, epoch, *guild_id, *user_id).await })
    }

    fn put_message(&self, epoch: u64, message: Message) -> BackendFuture<'_, ()> {
        Box::pin(async move { CacheBackend::put_message(self, epoch, message).await })
    }

    fn get_message<'a>(
        &'a self,
        channel_id: &'a GenericId,
        message_id: &'a GenericId,
    ) -> BackendFuture<'a, Option<Arc<Message>>> {
        Box::pin(async move { CacheBackend::get_message(self, *channel_id, *message_id).await })
    }

    fn delete_message<'a>(
        &'a self,
        epoch: u64,
        channel_id: &'a GenericId,
        message_id: &'a GenericId,
    ) -> BackendFuture<'a, ()> {
        Box::pin(async move {
            CacheBackend::delete_message(self, epoch, *channel_id, *message_id).await
        })
    }

    fn put_presence(
        &self,
        epoch: u64,
        guild_id: GenericId,
        user_id: GenericId,
        presence: Presence,
    ) -> BackendFuture<'_, ()> {
        Box::pin(async move {
            CacheBackend::put_presence(self, epoch, guild_id, user_id, presence).await
        })
    }
}

/// Capacity of the client-wide external cache backend write queue.
pub(super) const CACHE_BACKEND_QUEUE_CAPACITY: usize = 8_192;
/// First retry delay of a failed resync; doubles up to
/// [`RESYNC_RETRY_MAX`].
const RESYNC_RETRY_BASE: Duration = Duration::from_millis(500);
const RESYNC_RETRY_MAX: Duration = Duration::from_secs(30);
/// Attempts of the claiming full clear before concluding that another
/// client keeps clearing the store.
const CLAIM_ATTEMPTS: u32 = 3;

/// Snapshot of the external cache backend mirror's health, from
/// [`Context::cache_backend_health`](super::Context::cache_backend_health).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CacheBackendHealth {
    /// `false` while a shard is recovering, after the worker stopped, or
    /// once another client took over the store. A recovering shard's mirror
    /// may be incomplete (its writes are discarded until its clear lands),
    /// never stale.
    pub healthy: bool,
    /// Whether the backend worker is running. If it stops while the client
    /// runs (it contains panics from backend code, so only an abort can stop
    /// it), every later write is counted in `dropped_writes`.
    pub worker_running: bool,
    /// Whether another client cleared the store after this one, so this
    /// client stopped mirroring (only one client may write to a store).
    pub superseded: bool,
    /// Writes that did not fit in the queue, arrived after the worker
    /// stopped, or arrived after the client was superseded.
    pub dropped_writes: u64,
    /// Writes discarded because their shard was recovering: queued before
    /// the recovery boundary, or forwarded while the recovery ran.
    pub quarantined_writes: u64,
    /// Backend calls that returned an error or panicked (including failed
    /// recovery attempts).
    pub failed_writes: u64,
    /// Recoveries (`clear_shard_cache` after a loss) that completed.
    pub resyncs: u64,
    /// `(shard_id, shard_count)` pairs whose recovery is pending.
    pub pending_resync_shards: Vec<(u32, u32)>,
    /// The most recent backend error, if any.
    pub last_error: Option<String>,
}

type ShardPair = (u32, u32);

/// One write for the external cache backend, applied in queue order.
#[derive(Debug)]
pub(super) enum BackendWrite {
    ClearShard,
    PutMember {
        guild_id: GenericId,
        user_id: GenericId,
        member: Box<crate::model::Member>,
    },
    DeleteMember {
        guild_id: GenericId,
        user_id: GenericId,
    },
    PutMembersChunk {
        guild_id: GenericId,
        members: Vec<crate::model::Member>,
        presences: Vec<crate::model::Presence>,
    },
    PutMessage(Box<crate::model::Message>),
    UpdateMessage(Box<BackendMessageUpdate>),
    DeleteMessages {
        channel_id: GenericId,
        message_ids: Vec<GenericId>,
    },
    PutPresence {
        guild_id: GenericId,
        user_id: GenericId,
        presence: Box<crate::model::Presence>,
    },
    /// Test hook: acknowledges once every earlier write was applied.
    #[cfg(test)]
    Flush(tokio::sync::oneshot::Sender<()>),
}

#[derive(Debug)]
pub(super) struct BackendMessageUpdate {
    partial: crate::model::Message,
    fields: MessageUpdateFields,
    /// The in-memory cache's merged copy, used when the backend does not
    /// hold the message.
    in_memory: Option<crate::model::Message>,
}

#[derive(Debug)]
struct QueuedWrite {
    /// Queue position; `None` for test flushes, which are not real writes.
    ticket: Option<u64>,
    shard: ShardPair,
    write: BackendWrite,
}

/// A shard whose mirror lost a write and is waiting to be cleared.
#[derive(Clone, Copy, Debug)]
struct PendingResync {
    /// One past the highest ticket admitted when the loss was recorded.
    /// Queued writes of the shard below it are discarded, and the clear runs
    /// once the worker has dequeued every ticket below it; nothing moves it
    /// later, so recovery cannot be postponed.
    barrier: u64,
    /// Failed attempts so far, for the retry backoff.
    attempts: u32,
    /// Earliest time of the next attempt after a failure.
    retry_at: Option<Instant>,
}

#[derive(Default)]
struct ForwarderState {
    next_ticket: u64,
    pending_resync: BTreeMap<ShardPair, PendingResync>,
    worker_stopped: bool,
    /// Store epoch seen in the rejection that showed another client took
    /// over.
    superseded: Option<u64>,
    dropped_writes: u64,
    quarantined_writes: u64,
    failed_writes: u64,
    resyncs: u64,
    last_error: Option<String>,
    last_drop_warning: Option<std::time::Instant>,
}

impl ForwarderState {
    /// Starts recovery for `shard` unless it is already recovering. The
    /// boundary covers every write admitted so far.
    fn start_recovery(&mut self, shard: ShardPair) {
        let barrier = self.next_ticket;
        self.pending_resync.entry(shard).or_insert(PendingResync {
            barrier,
            attempts: 0,
            retry_at: None,
        });
    }

    fn unrecovered(&self) -> Vec<ShardPair> {
        self.pending_resync.keys().copied().collect()
    }
}

/// State shared by the forwarders (one per shard runtime), the worker, and
/// health readers.
#[derive(Default)]
pub(super) struct ForwarderShared {
    state: StdMutex<ForwarderState>,
}

impl ForwarderShared {
    fn state(&self) -> StdMutexGuard<'_, ForwarderState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(super) fn health(&self) -> CacheBackendHealth {
        let state = self.state();
        CacheBackendHealth {
            healthy: state.pending_resync.is_empty()
                && !state.worker_stopped
                && state.superseded.is_none(),
            worker_running: !state.worker_stopped,
            superseded: state.superseded.is_some(),
            dropped_writes: state.dropped_writes,
            quarantined_writes: state.quarantined_writes,
            failed_writes: state.failed_writes,
            resyncs: state.resyncs,
            pending_resync_shards: state.unrecovered(),
            last_error: state.last_error.clone(),
        }
    }
}

/// Client-wide, ordered forwarder of cache writes; see the module docs.
#[derive(Clone)]
pub(super) struct CacheBackendForwarder {
    writes: mpsc::Sender<QueuedWrite>,
    shared: Arc<ForwarderShared>,
}

/// Owns the backend worker task so the client can drain it with a bound
/// and never leave it writing after the client stopped.
pub(super) struct CacheBackendWorker {
    task: AbortOnDrop<Result<(), String>>,
    shared: Arc<ForwarderShared>,
}

impl CacheBackendWorker {
    /// Waits for the worker to finish: it drains the queue once every
    /// forwarder is gone and then retries pending recoveries within its
    /// drain bound. Returns an error naming the shards left unrecovered,
    /// or saying the client was superseded. A worker still running after
    /// `timeout` (plus a second) is aborted; it then issues no further
    /// backend calls, and its in-flight writes are rejected by the store's
    /// epoch check once another client cleared the store.
    pub(super) async fn shutdown(mut self, timeout: Duration) -> Result<(), String> {
        let bound = timeout.saturating_add(ABORT_CONFIRM_GRACE);
        match tokio::time::timeout(bound, self.task.join()).await {
            Ok(Some(Ok(result))) => result,
            Ok(Some(Err(error))) if error.is_panic() => {
                Err(format!("cache backend worker panicked: {error}"))
            }
            Ok(_) => Ok(()),
            Err(_) => {
                self.task.abort();
                let confirmed = tokio::time::timeout(ABORT_CONFIRM_GRACE, self.task.join())
                    .await
                    .is_ok();
                let unrecovered = self.shared.state().unrecovered();
                let mut message =
                    format!("cache backend worker did not finish within {bound:?} and was aborted");
                if !confirmed {
                    message.push_str(&format!(
                        " (cancellation not confirmed within {ABORT_CONFIRM_GRACE:?})"
                    ));
                }
                if !unrecovered.is_empty() {
                    message.push_str(&format!(
                        "; recovery unresolved for shard(s) {unrecovered:?}"
                    ));
                }
                Err(message)
            }
        }
    }

    /// Hands the task to an owner that applies its own drop policy (the
    /// shard supervisor keeps it running when the supervisor is dropped).
    #[cfg(feature = "sharding")]
    pub(super) fn into_task(self) -> Option<tokio::task::JoinHandle<Result<(), String>>> {
        self.task.detach()
    }
}

impl CacheBackendForwarder {
    /// Spawns the worker. `drain_timeout` bounds how long it keeps retrying
    /// pending recoveries after its queue closed.
    pub(super) fn spawn(
        backend: Arc<dyn ErasedCacheBackend>,
        capacity: usize,
        drain_timeout: Duration,
    ) -> (Self, CacheBackendWorker) {
        let (writes, queued) = mpsc::channel(capacity.max(1));
        let shared = Arc::new(ForwarderShared::default());
        let task = tokio::spawn(run_cache_backend_worker(
            backend,
            queued,
            Arc::clone(&shared),
            drain_timeout,
        ));
        (
            Self {
                writes,
                shared: Arc::clone(&shared),
            },
            CacheBackendWorker {
                task: AbortOnDrop::new(task),
                shared,
            },
        )
    }

    pub(super) fn health_handle(&self) -> Arc<ForwarderShared> {
        Arc::clone(&self.shared)
    }

    /// Queues a write from shard `shard`. Never waits. A write for a
    /// recovering shard is discarded; a write that does not fit starts
    /// recovery for its shard.
    pub(super) fn forward(&self, shard: ShardPair, write: BackendWrite) {
        let mut state = self.shared.state();
        if state.superseded.is_some() || state.worker_stopped {
            state.dropped_writes += 1;
            return;
        }
        if state.pending_resync.contains_key(&shard) {
            state.quarantined_writes += 1;
            return;
        }
        let ticket = state.next_ticket;
        match self.writes.try_send(QueuedWrite {
            ticket: Some(ticket),
            shard,
            write,
        }) {
            Ok(()) => state.next_ticket += 1,
            Err(mpsc::error::TrySendError::Full(_)) => {
                state.dropped_writes += 1;
                state.start_recovery(shard);
                let now = std::time::Instant::now();
                if state
                    .last_drop_warning
                    .is_none_or(|last| now.duration_since(last) >= super::OVERLOAD_WARNING_INTERVAL)
                {
                    state.last_drop_warning = Some(now);
                    warn!(
                        "cache backend write queue is full ({} writes); {} write(s) dropped so \
                         far. Shard {}/{} is recovering: its writes are discarded until it is \
                         cleared",
                        self.writes.max_capacity(),
                        state.dropped_writes,
                        shard.0,
                        shard.1
                    );
                }
            }
            // The worker stopped: it only ends on client shutdown, but if it
            // ever ends early the write must not vanish silently.
            Err(mpsc::error::TrySendError::Closed(_)) => {
                state.dropped_writes += 1;
                state.worker_stopped = true;
            }
        }
    }

    /// Waits until every write queued before this call was applied.
    #[cfg(test)]
    pub(super) async fn flush(&self) {
        let (done_tx, done_rx) = tokio::sync::oneshot::channel();
        let flush = QueuedWrite {
            ticket: None,
            shard: (0, 1),
            write: BackendWrite::Flush(done_tx),
        };
        if self.writes.send(flush).await.is_ok() {
            let _ = done_rx.await;
        }
    }
}

/// Marks the mirror failed if the worker ends without finishing normally
/// (it was aborted, or the runtime is shutting down).
struct WorkerExitGuard {
    shared: Arc<ForwarderShared>,
    completed: bool,
}

impl Drop for WorkerExitGuard {
    fn drop(&mut self) {
        if !self.completed {
            let mut state = self.shared.state();
            state.worker_stopped = true;
            state
                .last_error
                .get_or_insert_with(|| "cache backend worker stopped".to_string());
        }
    }
}

/// Why a clear did not apply.
enum ClearFailure {
    /// The backend call failed or panicked; retry later.
    Failed(String),
    /// The store's epoch moved: another client owns the store now.
    Superseded(u64),
}

/// What a clear covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClearScope {
    Store,
    Shard(ShardPair),
}

impl ClearScope {
    fn covers(self, shard: ShardPair) -> bool {
        match self {
            ClearScope::Store => true,
            ClearScope::Shard(cleared) => cleared == shard,
        }
    }
}

/// A clear whose outcome is unknown: the call failed (an error, a panic, a
/// lost response), so it may or may not have committed in the store.
#[derive(Clone, Copy, Debug)]
struct InDoubtClear {
    id: CacheClearId,
    expected: u64,
    scope: ClearScope,
}

/// This client's view of the store.
///
/// A clear whose call failed stays in doubt and is resolved — retried with
/// the same id and expected epoch — before any other clear or write. The
/// store answers a retry of a clear that already committed with that
/// clear's epoch (matched by id), so a lost response cannot make the sole
/// writer look superseded, and no returned epoch is ever adopted on trust:
/// only `Applied` for this client's own id moves the session forward.
struct StoreSession {
    /// Epoch of this client's last applied clear; `None` before it claimed
    /// the store.
    epoch: Option<u64>,
    /// Random per client instance, so ids of different clients never match.
    client: u64,
    next_sequence: u64,
    in_doubt: Option<InDoubtClear>,
    /// Rejections seen while claiming, before this client's first applied
    /// clear; bounded by [`CLAIM_ATTEMPTS`].
    claim_rejections: u32,
}

impl StoreSession {
    fn new() -> Self {
        use std::hash::BuildHasher;
        // `RandomState` is seeded from OS randomness per process; mixing in
        // the time and process id separates instances within a process too.
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or_default();
        let client =
            std::collections::hash_map::RandomState::new().hash_one((nanos, std::process::id()));
        Self {
            epoch: None,
            client,
            next_sequence: 0,
            in_doubt: None,
            claim_rejections: 0,
        }
    }

    fn next_id(&mut self) -> CacheClearId {
        let id = CacheClearId {
            client: self.client,
            sequence: self.next_sequence,
        };
        self.next_sequence += 1;
        id
    }

    /// Issues one clear attempt. A failed call leaves it in doubt.
    async fn attempt(
        &mut self,
        backend: &dyn ErasedCacheBackend,
        clear: InDoubtClear,
    ) -> Result<CacheClear, ClearFailure> {
        let call = match clear.scope {
            ClearScope::Store => {
                contained("clear_cache", backend.clear_cache(clear.expected, clear.id)).await
            }
            ClearScope::Shard(shard) => {
                contained(
                    "clear_shard_cache",
                    backend.clear_shard_cache(clear.expected, clear.id, shard.0, shard.1),
                )
                .await
            }
        };
        match call {
            Ok(outcome) => {
                self.in_doubt = None;
                if let CacheClear::Applied(epoch) = outcome {
                    self.epoch = Some(epoch);
                }
                Ok(outcome)
            }
            Err(error) => {
                self.in_doubt = Some(clear);
                Err(ClearFailure::Failed(error))
            }
        }
    }

    /// Settles an in-doubt clear by retrying it unchanged. Returns its scope
    /// if it turned out applied.
    async fn resolve_in_doubt(
        &mut self,
        backend: &dyn ErasedCacheBackend,
    ) -> Result<Option<ClearScope>, ClearFailure> {
        let Some(clear) = self.in_doubt else {
            return Ok(None);
        };
        match self.attempt(backend, clear).await? {
            CacheClear::Applied(_) => Ok(Some(clear.scope)),
            // Before this client's first applied clear there is no ownership
            // to lose: the rejection only reports the store's current epoch
            // (for example a claim expecting 0 against an existing store,
            // whose first rejection was lost). Continue the claim from it.
            CacheClear::Rejected(current) if self.epoch.is_none() => {
                self.note_claim_rejection(current)?;
                self.claim_with(backend, clear.id, current).await?;
                Ok(Some(ClearScope::Store))
            }
            // Not committed, and another client cleared since.
            CacheClear::Rejected(current) => Err(ClearFailure::Superseded(current)),
        }
    }

    /// Clears `shard` (or claims the store with a full clear if this client
    /// has not cleared it yet) under the compare-and-set epoch contract.
    async fn clear(
        &mut self,
        backend: &dyn ErasedCacheBackend,
        shard: ShardPair,
    ) -> Result<u64, ClearFailure> {
        if let Some(scope) = self.resolve_in_doubt(backend).await? {
            if scope.covers(shard) {
                return self
                    .epoch
                    .ok_or_else(|| ClearFailure::Failed("clear applied without an epoch".into()));
            }
        }
        let Some(expected) = self.epoch else {
            return self.claim(backend).await;
        };
        let clear = InDoubtClear {
            id: self.next_id(),
            expected,
            scope: ClearScope::Shard(shard),
        };
        match self.attempt(backend, clear).await? {
            CacheClear::Applied(epoch) => Ok(epoch),
            CacheClear::Rejected(current) => Err(ClearFailure::Superseded(current)),
        }
    }

    /// The first clear of this client: a full clear, so a previous client's
    /// entries (possibly from another shard layout) cannot survive. Adopts
    /// the store's epoch from a rejection and retries a few times. This does
    /// not make the claim exclusive: a second client starting meanwhile can
    /// claim the store after it (see the [`CacheBackend`] docs).
    async fn claim(&mut self, backend: &dyn ErasedCacheBackend) -> Result<u64, ClearFailure> {
        let id = self.next_id();
        self.claim_with(backend, id, 0).await
    }

    /// Issues the claiming full clear `id` expecting `expected`, adopting the
    /// store's epoch from each rejection (epoch discovery) until it applies
    /// or the claim budget is spent.
    async fn claim_with(
        &mut self,
        backend: &dyn ErasedCacheBackend,
        id: CacheClearId,
        mut expected: u64,
    ) -> Result<u64, ClearFailure> {
        loop {
            let clear = InDoubtClear {
                id,
                expected,
                scope: ClearScope::Store,
            };
            match self.attempt(backend, clear).await? {
                CacheClear::Applied(epoch) => return Ok(epoch),
                CacheClear::Rejected(current) => {
                    self.note_claim_rejection(current)?;
                    expected = current;
                }
            }
        }
    }

    /// Counts a rejection seen while claiming. Only a store whose epoch
    /// keeps moving under every attempt — another client clearing it
    /// repeatedly — exhausts the budget and supersedes this client.
    fn note_claim_rejection(&mut self, current: u64) -> Result<(), ClearFailure> {
        self.claim_rejections += 1;
        if self.claim_rejections >= CLAIM_ATTEMPTS {
            return Err(ClearFailure::Superseded(current));
        }
        Ok(())
    }

    /// The epoch to stamp writes with: settles an in-doubt clear first (a
    /// write stamped with a stale epoch would be ignored by the store) and
    /// claims the store if needed.
    async fn write_epoch(&mut self, backend: &dyn ErasedCacheBackend) -> Result<u64, ClearFailure> {
        self.resolve_in_doubt(backend).await?;
        match self.epoch {
            Some(epoch) => Ok(epoch),
            None => self.claim(backend).await,
        }
    }
}

/// Records that another client took over the store: recovery is moot and
/// this client stops writing.
fn mark_superseded(shared: &ForwarderShared, current: u64) {
    let mut state = shared.state();
    if state.superseded.is_none() {
        error!(
            "the cache backend store was cleared by another client (store epoch {current}); this \
             client stops mirroring to it — only one client may write to a store"
        );
    }
    state.superseded = Some(current);
    state.last_error = Some(format!(
        "superseded: another client cleared the store (epoch {current})"
    ));
    state.pending_resync.clear();
}

async fn run_cache_backend_worker(
    backend: Arc<dyn ErasedCacheBackend>,
    mut writes: mpsc::Receiver<QueuedWrite>,
    shared: Arc<ForwarderShared>,
    drain_timeout: Duration,
) -> Result<(), String> {
    let mut exit = WorkerExitGuard {
        shared: Arc::clone(&shared),
        completed: false,
    };
    let mut session = StoreSession::new();
    let mut dequeued: Option<u64> = None;
    loop {
        let retry_at = next_retry(&shared);
        let queued = tokio::select! {
            biased;
            queued = writes.recv() => match queued {
                Some(queued) => queued,
                None => break,
            },
            _ = sleep_until_opt(retry_at) => {
                run_due_resyncs(backend.as_ref(), &shared, &mut session, dequeued, false).await;
                continue;
            }
        };
        let QueuedWrite {
            ticket,
            shard,
            write,
        } = queued;
        if let Some(ticket) = ticket {
            dequeued = Some(ticket);
        }
        if let Some(Err(error)) = handle_write(
            backend.as_ref(),
            &shared,
            &mut session,
            ticket,
            shard,
            write,
        )
        .await
        {
            warn!(
                "cache backend {error}; shard {}/{} is recovering until it is cleared",
                shard.0, shard.1
            );
            let mut state = shared.state();
            state.failed_writes += 1;
            state.last_error = Some(error);
            state.start_recovery(shard);
        }
        run_due_resyncs(backend.as_ref(), &shared, &mut session, dequeued, false).await;
    }

    // Every admitted write has been dequeued. Keep retrying pending
    // recoveries for up to `drain_timeout` so a shutdown does not abandon a
    // shard whose clear failed moments earlier.
    let deadline = Instant::now().checked_add(drain_timeout);
    loop {
        run_due_resyncs(backend.as_ref(), &shared, &mut session, dequeued, true).await;
        let settled = {
            let state = shared.state();
            state.pending_resync.is_empty() || state.superseded.is_some()
        };
        if settled {
            break;
        }
        match next_retry(&shared) {
            Some(retry_at) if deadline.is_none_or(|deadline| retry_at <= deadline) => {
                tokio::time::sleep_until(retry_at).await;
            }
            _ => break,
        }
    }
    exit.completed = true;

    let state = shared.state();
    if let Some(current) = state.superseded {
        return Err(format!(
            "another client cleared the cache backend store (epoch {current}); this client \
             stopped mirroring to it"
        ));
    }
    let unrecovered = state.unrecovered();
    if unrecovered.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "cache backend recovery unresolved for shard(s) {unrecovered:?} when the client \
             stopped; their mirrored entries may be stale"
        ))
    }
}

/// Applies one dequeued write. Returns `None` when the write was not
/// attempted (discarded, superseded, or a test flush).
async fn handle_write(
    backend: &dyn ErasedCacheBackend,
    shared: &ForwarderShared,
    session: &mut StoreSession,
    ticket: Option<u64>,
    shard: ShardPair,
    write: BackendWrite,
) -> Option<Result<(), String>> {
    #[cfg(test)]
    if let BackendWrite::Flush(done) = write {
        let _ = done.send(());
        return None;
    }
    {
        let mut state = shared.state();
        if state.superseded.is_some() {
            state.dropped_writes += 1;
            return None;
        }
        // Admitted before the shard's recovery started: the coming clear
        // supersedes it, and applying it after the clear would leave data
        // a discarded later write (say, a delete) should have removed.
        if let (Some(pending), Some(ticket)) = (state.pending_resync.get(&shard), ticket) {
            if ticket < pending.barrier {
                state.quarantined_writes += 1;
                return None;
            }
        }
    }
    let outcome = if matches!(write, BackendWrite::ClearShard) {
        session.clear(backend, shard).await.map(|_| ())
    } else {
        match session.write_epoch(backend).await {
            Ok(epoch) => apply_backend_write(backend, epoch, write)
                .await
                .map_err(ClearFailure::Failed),
            Err(failure) => Err(failure),
        }
    };
    match outcome {
        Ok(()) => Some(Ok(())),
        Err(ClearFailure::Failed(error)) => Some(Err(error)),
        Err(ClearFailure::Superseded(current)) => {
            mark_superseded(shared, current);
            None
        }
    }
}

/// The earliest retry time among pending recoveries that already failed.
fn next_retry(shared: &ForwarderShared) -> Option<Instant> {
    shared
        .state()
        .pending_resync
        .values()
        .filter_map(|pending| pending.retry_at)
        .min()
}

async fn sleep_until_opt(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

/// Runs a backend call, turning a panic into an error so backend code can
/// never take the worker down.
async fn contained<T>(
    operation: &str,
    call: impl std::future::Future<Output = Result<T, DiscordError>>,
) -> Result<T, String> {
    match AssertUnwindSafe(call).catch_unwind().await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Err(format!("{operation} failed: {error}")),
        Err(_) => Err(format!("{operation} panicked")),
    }
}

/// Clears every recovering shard whose boundary the worker has passed
/// (`closed`: the queue is drained, so every boundary is passed) and whose
/// retry time, if any, has come.
async fn run_due_resyncs(
    backend: &dyn ErasedCacheBackend,
    shared: &ForwarderShared,
    session: &mut StoreSession,
    dequeued: Option<u64>,
    closed: bool,
) {
    let now = Instant::now();
    let passed = dequeued.map_or(0, |ticket| ticket + 1);
    let due: Vec<ShardPair> = shared
        .state()
        .pending_resync
        .iter()
        .filter(|(_, pending)| {
            (closed || pending.barrier <= passed)
                && pending.retry_at.is_none_or(|retry_at| retry_at <= now)
        })
        .map(|(shard, _)| *shard)
        .collect();
    for shard in due {
        let result = session.clear(backend, shard).await;
        match result {
            Ok(_) => {
                let mut state = shared.state();
                state.resyncs += 1;
                state.pending_resync.remove(&shard);
            }
            Err(ClearFailure::Superseded(current)) => {
                mark_superseded(shared, current);
                return;
            }
            Err(ClearFailure::Failed(error)) => {
                let mut state = shared.state();
                state.failed_writes += 1;
                if let Some(pending) = state.pending_resync.get_mut(&shard) {
                    pending.attempts = pending.attempts.saturating_add(1);
                    let delay = RESYNC_RETRY_BASE
                        .saturating_mul(2_u32.saturating_pow(pending.attempts - 1))
                        .min(RESYNC_RETRY_MAX);
                    pending.retry_at = Some(Instant::now() + delay);
                    warn!(
                        "cache backend recovery of shard {}/{} failed (attempt {}), retrying in \
                         {delay:?}: {error}",
                        shard.0, shard.1, pending.attempts
                    );
                }
                state.last_error = Some(error);
            }
        }
    }
}

/// Applies one write under the store epoch `epoch`. Errors (including
/// caught panics) name the failed operation; any error leaves the shard's
/// mirror possibly stale, so the caller schedules a resync.
async fn apply_backend_write(
    backend: &dyn ErasedCacheBackend,
    epoch: u64,
    write: BackendWrite,
) -> Result<(), String> {
    match write {
        BackendWrite::ClearShard => Ok(()),
        BackendWrite::PutMember {
            guild_id,
            user_id,
            member,
        } => {
            contained(
                "put_member",
                backend.put_member(epoch, guild_id, user_id, *member),
            )
            .await
        }
        BackendWrite::DeleteMember { guild_id, user_id } => {
            contained(
                "delete_member",
                backend.delete_member(epoch, &guild_id, &user_id),
            )
            .await
        }
        BackendWrite::PutMembersChunk {
            guild_id,
            members,
            presences,
        } => {
            let mut first_error = Ok(());
            for member in members {
                let Some(user_id) = member.user.as_ref().map(|user| user.id) else {
                    continue;
                };
                let result = contained(
                    "put_member",
                    backend.put_member(epoch, guild_id, user_id, member),
                )
                .await;
                first_error = first_error.and(result);
            }
            for mut presence in presences {
                let user_id = presence
                    .user_id
                    .or_else(|| presence.user.as_ref().map(|user| user.id));
                let Some(user_id) = user_id else {
                    continue;
                };
                presence.user_id = Some(user_id);
                let result = contained(
                    "put_presence",
                    backend.put_presence(epoch, guild_id, user_id, presence),
                )
                .await;
                first_error = first_error.and(result);
            }
            first_error
        }
        BackendWrite::PutMessage(message) => {
            contained("put_message", backend.put_message(epoch, *message)).await
        }
        BackendWrite::UpdateMessage(update) => {
            let BackendMessageUpdate {
                partial,
                fields,
                in_memory,
            } = *update;
            let stored = contained(
                "get_message",
                backend.get_message(&partial.channel_id, &partial.id),
            )
            .await;
            let merged = match stored {
                Ok(Some(stored)) => merge_message_update(stored.as_ref().clone(), partial, &fields),
                Ok(None) => match in_memory {
                    Some(message) => message,
                    // Neither copy is known: the payload may be partial, so
                    // storing it would claim fields the backend never saw.
                    None => return Ok(()),
                },
                // Without the stored copy the update cannot be merged; the
                // in-memory copy is complete, otherwise the stored message
                // may now be stale and the shard needs a resync.
                Err(error) => match in_memory {
                    Some(message) => {
                        warn!("cache backend {error}; storing the in-memory copy instead");
                        message
                    }
                    None => return Err(error),
                },
            };
            contained("put_message", backend.put_message(epoch, merged)).await
        }
        BackendWrite::DeleteMessages {
            channel_id,
            message_ids,
        } => {
            let mut first_error = Ok(());
            for message_id in &message_ids {
                let result = contained(
                    "delete_message",
                    backend.delete_message(epoch, &channel_id, message_id),
                )
                .await;
                first_error = first_error.and(result);
            }
            first_error
        }
        BackendWrite::PutPresence {
            guild_id,
            user_id,
            presence,
        } => {
            contained(
                "put_presence",
                backend.put_presence(epoch, guild_id, user_id, *presence),
            )
            .await
        }
        #[cfg(test)]
        BackendWrite::Flush(done) => {
            let _ = done.send(());
            Ok(())
        }
    }
}

/// Translates an event into the external backend write that mirrors what
/// `apply_cache_updates` did in memory (members, messages, presences), and
/// queues it. Runs after the in-memory update so a MESSAGE_UPDATE can carry
/// the in-memory merged copy as a fallback.
pub(super) async fn forward_cache_backend_updates(
    forwarder: &CacheBackendForwarder,
    cache: &CacheHandle,
    shard: ShardPair,
    event: &Event,
) {
    let write = match event {
        Event::Ready(_) => Some(BackendWrite::ClearShard),
        Event::MemberAdd(event) | Event::MemberUpdate(event) => {
            event
                .member
                .user
                .as_ref()
                .map(|user| BackendWrite::PutMember {
                    guild_id: event.guild_id,
                    user_id: user.id,
                    member: Box::new(event.member.clone()),
                })
        }
        Event::MemberRemove(event) => Some(BackendWrite::DeleteMember {
            guild_id: event.data.guild_id,
            user_id: event.data.user.id,
        }),
        Event::GuildMembersChunk(event) => Some(BackendWrite::PutMembersChunk {
            guild_id: event.data.guild_id,
            members: event.data.members.clone(),
            presences: event.data.presences.clone().unwrap_or_default(),
        }),
        Event::MessageCreate(event) => {
            Some(BackendWrite::PutMessage(Box::new(event.message.clone())))
        }
        Event::MessageUpdate(event) => {
            let in_memory = cache
                .message(event.message.channel_id, event.message.id)
                .await;
            Some(BackendWrite::UpdateMessage(Box::new(
                BackendMessageUpdate {
                    partial: event.message.clone(),
                    fields: MessageUpdateFields::from_raw(&event.raw),
                    in_memory,
                },
            )))
        }
        Event::MessageDelete(event) => Some(BackendWrite::DeleteMessages {
            channel_id: event.data.channel_id,
            message_ids: vec![event.data.id],
        }),
        Event::MessageDeleteBulk(event) => Some(BackendWrite::DeleteMessages {
            channel_id: event.channel_id,
            message_ids: event.ids.clone(),
        }),
        Event::PresenceUpdate(event) => match (event.guild_id, event.user_id) {
            (Some(guild_id), Some(user_id)) => Some(BackendWrite::PutPresence {
                presence: Box::new(presence_from_update(event, user_id)),
                guild_id,
                user_id,
            }),
            _ => None,
        },
        Event::VoiceStateUpdate(event) => {
            match (event.state.guild_id, event.state.member.as_ref()) {
                (Some(guild_id), Some(member)) => {
                    member.user.as_ref().map(|user| BackendWrite::PutMember {
                        guild_id,
                        user_id: user.id,
                        member: Box::new(member.clone()),
                    })
                }
                _ => None,
            }
        }
        _ => None,
    };
    if let Some(write) = write {
        forwarder.forward(shard, write);
    }
}
