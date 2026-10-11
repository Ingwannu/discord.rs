//! Ordered, fenced forwarding of cache writes to an external
//! [`CacheBackend`].
//!
//! Decision log:
//! - Purpose: mirror in-memory cache writes to an external store in gateway
//!   order without stalling gateway processing, and never let the mirror
//!   silently diverge — not under overload, not after backend errors or
//!   panics, and not across client restarts.
//! - Previous design and constraints: a task per write let a delayed put
//!   land after a later delete. The first ordered queue dropped writes when
//!   full, including deletes and invalidations. The next version scheduled a
//!   resync per lost write, but moved the resync barrier forward on every
//!   further loss (so sustained saturation starved it), retried failed
//!   clears only after another write, ignored a dead worker, and could not
//!   stop an already submitted write from landing after a replacement
//!   client's clear.
//! - Alternatives considered: (1) await backend calls in the internal
//!   processor — a slow backend stalls cache, handlers, and member chunks;
//!   (2) an unbounded queue — memory grows behind a slow or dead backend;
//!   (3) a bounded ordered queue plus per-shard recovery, with epochs
//!   assigned by the store.
//! - Chosen: (3).
//!   - Every write is tagged with its shard. When a write does not fit or
//!     the backend rejects it (an error or a panic, which is caught), the
//!     shard gets a recovery barrier frozen at that point; from then on the
//!     shard's new writes are quarantined (not queued) until a resync —
//!     `clear_shard_cache` — has run after every write queued before the
//!     barrier. A failed resync is retried on its own backoff timer.
//!   - `clear_shard_cache` returns a store-assigned epoch; the worker stamps
//!     every later write for the shard with a [`CacheFence`] carrying it, and
//!     the store ignores writes older than its latest clear. The in-memory
//!     [`CacheHandle`] enforces this too.
//!   - The worker contains panics from backend code; if it still stops
//!     while the client runs, health reports it failed and every later write
//!     counts as dropped.
//! - Why: dropped deletes and invalidations are subsumed by a clear instead
//!   of lost, recovery cannot be postponed forever by more losses, and a
//!   late write from a previous client is rejected by the store itself.
//! - Trade-offs: under sustained overload a shard's mirror is cleared and
//!   misses the quarantined writes until it refills from later events.
//!   Writes are serialized, so throughput is bound by backend latency, and
//!   each MESSAGE_UPDATE costs one extra read. External stores must
//!   implement the fencing contract documented on [`CacheBackend`].

use std::collections::{BTreeMap, HashMap};
use std::panic::AssertUnwindSafe;
use std::sync::{Arc, Mutex as StdMutex, MutexGuard as StdMutexGuard};

use futures_util::FutureExt;
use tokio::sync::mpsc;
use tokio::time::{Duration, Instant};
use tracing::warn;

use super::pipeline::{stop_task, AbortOnDrop};
use super::{merge_message_update, presence_from_update, MessageUpdateFields};
use crate::cache::{CacheBackend, CacheFence, CacheHandle};
use crate::error::DiscordError;
use crate::event::Event;
use crate::model::Snowflake;

/// Capacity of the client-wide external cache backend write queue.
pub(super) const CACHE_BACKEND_QUEUE_CAPACITY: usize = 8_192;
/// First retry delay of a failed resync; doubles up to
/// [`RESYNC_RETRY_MAX`].
const RESYNC_RETRY_BASE: Duration = Duration::from_millis(500);
const RESYNC_RETRY_MAX: Duration = Duration::from_secs(30);

/// Snapshot of the external cache backend mirror's health, from
/// [`Context::cache_backend_health`](super::Context::cache_backend_health).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CacheBackendHealth {
    /// `false` while a shard resync is pending or the worker stopped. A
    /// pending resync means the shard's mirror may be incomplete (its new
    /// writes are quarantined), never stale.
    pub healthy: bool,
    /// Whether the backend worker is running. If it stops while the client
    /// runs (it contains panics from backend code, so only an abort can stop
    /// it), every later write is counted in `dropped_writes`.
    pub worker_running: bool,
    /// Writes that did not fit in the queue, or arrived after the worker
    /// stopped.
    pub dropped_writes: u64,
    /// Writes not queued because their shard was waiting for a resync.
    pub quarantined_writes: u64,
    /// Backend calls that returned an error or panicked (including failed
    /// resync attempts).
    pub failed_writes: u64,
    /// Resyncs (`clear_shard_cache` after a loss) that completed.
    pub resyncs: u64,
    /// `(shard_id, shard_count)` pairs waiting for a resync.
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
        guild_id: Snowflake,
        user_id: Snowflake,
        member: Box<crate::model::Member>,
    },
    DeleteMember {
        guild_id: Snowflake,
        user_id: Snowflake,
    },
    PutMembersChunk {
        guild_id: Snowflake,
        members: Vec<crate::model::Member>,
        presences: Vec<crate::model::Presence>,
    },
    PutMessage(Box<crate::model::Message>),
    UpdateMessage(Box<BackendMessageUpdate>),
    DeleteMessages {
        channel_id: Snowflake,
        message_ids: Vec<Snowflake>,
    },
    PutPresence {
        guild_id: Snowflake,
        user_id: Snowflake,
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

/// A shard waiting for a resync.
#[derive(Clone, Copy, Debug)]
struct PendingResync {
    /// Every write with a lower ticket must be applied before the resync
    /// runs, or a stale queued put could land after the clear. Frozen when
    /// the loss is recorded: later losses never move it, so recovery cannot
    /// be postponed indefinitely.
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
    dropped_writes: u64,
    quarantined_writes: u64,
    failed_writes: u64,
    resyncs: u64,
    last_error: Option<String>,
    last_drop_warning: Option<std::time::Instant>,
}

impl ForwarderState {
    /// Records a lost write for `shard`. The first loss freezes the barrier;
    /// later losses keep it.
    fn schedule_resync(&mut self, shard: ShardPair, barrier: u64) {
        self.pending_resync.entry(shard).or_insert(PendingResync {
            barrier,
            attempts: 0,
            retry_at: None,
        });
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
            healthy: state.pending_resync.is_empty() && !state.worker_stopped,
            worker_running: !state.worker_stopped,
            dropped_writes: state.dropped_writes,
            quarantined_writes: state.quarantined_writes,
            failed_writes: state.failed_writes,
            resyncs: state.resyncs,
            pending_resync_shards: state.pending_resync.keys().copied().collect(),
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
    task: AbortOnDrop<()>,
}

impl CacheBackendWorker {
    /// Waits for the queue to drain (it closes once every forwarder clone
    /// is gone), aborting the worker after `timeout`. Once this returns the
    /// worker issues no further backend calls; writes it already submitted
    /// to a remote store are fenced by their epoch (see [`CacheFence`]).
    pub(super) async fn shutdown(mut self, timeout: Duration) -> Result<(), String> {
        stop_task(&mut self.task, timeout, "cache backend worker").await
    }

    /// Hands the task to an owner that applies its own drop policy (the
    /// shard supervisor keeps it running when the supervisor is dropped).
    #[cfg(feature = "sharding")]
    pub(super) fn into_task(self) -> Option<tokio::task::JoinHandle<()>> {
        self.task.detach()
    }
}

impl CacheBackendForwarder {
    pub(super) fn spawn(
        backend: Arc<dyn CacheBackend>,
        capacity: usize,
    ) -> (Self, CacheBackendWorker) {
        let (writes, queued) = mpsc::channel(capacity.max(1));
        let shared = Arc::new(ForwarderShared::default());
        let task = tokio::spawn(run_cache_backend_worker(
            backend,
            queued,
            Arc::clone(&shared),
        ));
        (
            Self { writes, shared },
            CacheBackendWorker {
                task: AbortOnDrop::new(task),
            },
        )
    }

    pub(super) fn health_handle(&self) -> Arc<ForwarderShared> {
        Arc::clone(&self.shared)
    }

    /// Queues a write from shard `shard`. Never waits. A write for a shard
    /// that is waiting for a resync is quarantined; a write that does not
    /// fit schedules a resync of its shard behind everything already queued.
    pub(super) fn forward(&self, shard: ShardPair, write: BackendWrite) {
        let mut state = self.shared.state();
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
                state.schedule_resync(shard, ticket);
                let now = std::time::Instant::now();
                if state
                    .last_drop_warning
                    .is_none_or(|last| now.duration_since(last) >= super::OVERLOAD_WARNING_INTERVAL)
                {
                    state.last_drop_warning = Some(now);
                    warn!(
                        "cache backend write queue is full ({} writes); {} write(s) dropped so \
                         far. Shard {}/{} is quarantined until it is resynced \
                         (clear_shard_cache) after the queued writes",
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

/// Marks the mirror failed if the worker ends without draining its queue
/// normally (it was aborted, or the runtime is shutting down).
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

/// Per-shard epochs learned from `clear_shard_cache`, owned by the worker.
type ShardEpochs = HashMap<ShardPair, u64>;

async fn run_cache_backend_worker(
    backend: Arc<dyn CacheBackend>,
    mut writes: mpsc::Receiver<QueuedWrite>,
    shared: Arc<ForwarderShared>,
) {
    let mut exit = WorkerExitGuard {
        shared: Arc::clone(&shared),
        completed: false,
    };
    let mut epochs = ShardEpochs::new();
    let mut applied: Option<u64> = None;
    loop {
        let retry_at = next_retry(&shared);
        let queued = tokio::select! {
            biased;
            queued = writes.recv() => match queued {
                Some(queued) => queued,
                None => break,
            },
            _ = sleep_until_opt(retry_at) => {
                run_due_resyncs(backend.as_ref(), &shared, &mut epochs, applied).await;
                continue;
            }
        };
        let QueuedWrite {
            ticket,
            shard,
            write,
        } = queued;
        if let Err(error) = apply_fenced_write(backend.as_ref(), &mut epochs, shard, write).await {
            warn!(
                "cache backend {error}; shard {}/{} is quarantined until it is resynced",
                shard.0, shard.1
            );
            let mut state = shared.state();
            state.failed_writes += 1;
            state.last_error = Some(error);
            // Resync right after this write.
            let barrier = ticket.map_or(0, |ticket| ticket + 1);
            state.schedule_resync(shard, barrier);
        }
        if let Some(ticket) = ticket {
            applied = Some(ticket);
        }
        run_due_resyncs(backend.as_ref(), &shared, &mut epochs, applied).await;
    }
    exit.completed = true;
}

/// The earliest retry time among pending resyncs that already failed.
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

/// Runs `clear_shard_cache` for every pending shard whose barrier has been
/// passed and whose retry time (if any) has come.
async fn run_due_resyncs(
    backend: &dyn CacheBackend,
    shared: &ForwarderShared,
    epochs: &mut ShardEpochs,
    applied: Option<u64>,
) {
    let now = Instant::now();
    let passed = applied.map_or(0, |ticket| ticket + 1);
    let due: Vec<ShardPair> = shared
        .state()
        .pending_resync
        .iter()
        .filter(|(_, pending)| {
            pending.barrier <= passed && pending.retry_at.is_none_or(|retry_at| retry_at <= now)
        })
        .map(|(shard, _)| *shard)
        .collect();
    for shard in due {
        let result = contained(
            "clear_shard_cache",
            backend.clear_shard_cache(shard.0, shard.1),
        )
        .await;
        let mut state = shared.state();
        match result {
            Ok(epoch) => {
                epochs.insert(shard, epoch);
                state.resyncs += 1;
                state.pending_resync.remove(&shard);
            }
            Err(error) => {
                state.failed_writes += 1;
                if let Some(pending) = state.pending_resync.get_mut(&shard) {
                    pending.attempts = pending.attempts.saturating_add(1);
                    let delay = RESYNC_RETRY_BASE
                        .saturating_mul(2_u32.saturating_pow(pending.attempts - 1))
                        .min(RESYNC_RETRY_MAX);
                    pending.retry_at = Some(Instant::now() + delay);
                    warn!(
                        "cache backend resync of shard {}/{} failed (attempt {}), retrying in \
                         {delay:?}: {error}",
                        shard.0, shard.1, pending.attempts
                    );
                }
                state.last_error = Some(error);
            }
        }
    }
}

/// Applies one write under the shard's current fence. A shard without an
/// epoch yet (its first write, normally READY's clear) starts a generation
/// first, so nothing is ever written unfenced.
async fn apply_fenced_write(
    backend: &dyn CacheBackend,
    epochs: &mut ShardEpochs,
    shard: ShardPair,
    write: BackendWrite,
) -> Result<(), String> {
    if matches!(write, BackendWrite::ClearShard) {
        let epoch = contained(
            "clear_shard_cache",
            backend.clear_shard_cache(shard.0, shard.1),
        )
        .await?;
        epochs.insert(shard, epoch);
        return Ok(());
    }
    #[cfg(test)]
    if let BackendWrite::Flush(done) = write {
        let _ = done.send(());
        return Ok(());
    }
    let epoch = match epochs.get(&shard) {
        Some(epoch) => *epoch,
        None => {
            let epoch = contained(
                "clear_shard_cache",
                backend.clear_shard_cache(shard.0, shard.1),
            )
            .await?;
            epochs.insert(shard, epoch);
            epoch
        }
    };
    apply_backend_write(backend, CacheFence::new(shard.0, shard.1, epoch), write).await
}

/// Applies one fenced write. Errors (including caught panics) name the
/// failed operation; any error leaves the shard's mirror possibly stale, so
/// the caller schedules a resync.
async fn apply_backend_write(
    backend: &dyn CacheBackend,
    fence: CacheFence,
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
                backend.put_member(fence, guild_id, user_id, *member),
            )
            .await
        }
        BackendWrite::DeleteMember { guild_id, user_id } => {
            contained(
                "delete_member",
                backend.delete_member(fence, &guild_id, &user_id),
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
                let Some(user_id) = member.user.as_ref().map(|user| user.id.clone()) else {
                    continue;
                };
                let result = contained(
                    "put_member",
                    backend.put_member(fence, guild_id.clone(), user_id, member),
                )
                .await;
                first_error = first_error.and(result);
            }
            for mut presence in presences {
                let user_id = presence
                    .user_id
                    .clone()
                    .or_else(|| presence.user.as_ref().map(|user| user.id.clone()));
                let Some(user_id) = user_id else {
                    continue;
                };
                presence.user_id = Some(user_id.clone());
                let result = contained(
                    "put_presence",
                    backend.put_presence(fence, guild_id.clone(), user_id, presence),
                )
                .await;
                first_error = first_error.and(result);
            }
            first_error
        }
        BackendWrite::PutMessage(message) => {
            contained("put_message", backend.put_message(fence, *message)).await
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
            contained("put_message", backend.put_message(fence, merged)).await
        }
        BackendWrite::DeleteMessages {
            channel_id,
            message_ids,
        } => {
            let mut first_error = Ok(());
            for message_id in &message_ids {
                let result = contained(
                    "delete_message",
                    backend.delete_message(fence, &channel_id, message_id),
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
                backend.put_presence(fence, guild_id, user_id, *presence),
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
                    guild_id: event.guild_id.clone(),
                    user_id: user.id.clone(),
                    member: Box::new(event.member.clone()),
                })
        }
        Event::MemberRemove(event) => Some(BackendWrite::DeleteMember {
            guild_id: event.data.guild_id.clone(),
            user_id: event.data.user.id.clone(),
        }),
        Event::GuildMembersChunk(event) => Some(BackendWrite::PutMembersChunk {
            guild_id: event.data.guild_id.clone(),
            members: event.data.members.clone(),
            presences: event.data.presences.clone().unwrap_or_default(),
        }),
        Event::MessageCreate(event) => {
            Some(BackendWrite::PutMessage(Box::new(event.message.clone())))
        }
        Event::MessageUpdate(event) => {
            let in_memory = cache
                .message(&event.message.channel_id, &event.message.id)
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
            channel_id: event.data.channel_id.clone(),
            message_ids: vec![event.data.id.clone()],
        }),
        Event::MessageDeleteBulk(event) => Some(BackendWrite::DeleteMessages {
            channel_id: event.channel_id.clone(),
            message_ids: event.ids.clone(),
        }),
        Event::PresenceUpdate(event) => match (event.guild_id.clone(), event.user_id.clone()) {
            (Some(guild_id), Some(user_id)) => Some(BackendWrite::PutPresence {
                presence: Box::new(presence_from_update(event, &user_id)),
                guild_id,
                user_id,
            }),
            _ => None,
        },
        Event::VoiceStateUpdate(event) => {
            match (event.state.guild_id.clone(), event.state.member.as_ref()) {
                (Some(guild_id), Some(member)) => {
                    member.user.as_ref().map(|user| BackendWrite::PutMember {
                        guild_id,
                        user_id: user.id.clone(),
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
