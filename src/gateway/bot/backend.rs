//! Ordered forwarding of cache writes to an external [`CacheBackend`].
//!
//! Decision log:
//! - Purpose: mirror in-memory cache writes to an external store in gateway
//!   order without stalling gateway processing, and never let the mirror
//!   silently diverge.
//! - Previous design and constraints: each write ran in its own task, so a
//!   delayed put could land after a later delete and resurrect the entry.
//!   The first ordered version dropped writes when its queue was full —
//!   including deletes and READY invalidations — so a queued create could
//!   outlive a dropped delete, and a dropped invalidation kept the previous
//!   session's entries. The old `clear_shard_cache` default kept everything
//!   for multi-shard bots, which made stale state unconditional.
//! - Alternatives considered: (1) await backend calls in the internal
//!   processor — a slow backend would stall cache, handlers, and member
//!   chunks; (2) an unbounded queue — memory grows without limit behind a
//!   slow or dead backend; (3) never drop deletes by blocking — same as (1);
//!   (4) a bounded ordered queue where any lost or failed write schedules an
//!   ordered resync (`clear_shard_cache`) of the shard that wrote it.
//! - Chosen: (4). Every write is tagged with its shard. A write that does
//!   not fit, or that the backend rejects, marks the shard for a resync that
//!   runs once every write queued before the loss has been applied. A
//!   dropped delete or READY invalidation is therefore subsumed by a later
//!   clear of the same shard instead of being lost, and writes queued after
//!   the loss can only be cleared (missing), never left stale. Health is
//!   observable through `Context::cache_backend_health`, and
//!   `clear_shard_cache` is a required trait method.
//! - Why: it keeps the gateway non-blocking and memory bounded while
//!   guaranteeing the mirror is never stale, only temporarily incomplete.
//! - Trade-offs: under sustained overload a shard's mirror is cleared and
//!   refills from later events. Writes are serialized, so throughput is
//!   bound by backend latency, and each MESSAGE_UPDATE costs one extra read.
//!   A failed resync stays pending (and the mirror unhealthy) until it is
//!   retried after the next applied write.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex as StdMutex, MutexGuard as StdMutexGuard};

use tokio::sync::mpsc;
use tokio::time::Duration;
use tracing::warn;

use super::pipeline::{stop_task, AbortOnDrop};
use super::{merge_message_update, presence_from_update, MessageUpdateFields};
use crate::cache::{CacheBackend, CacheHandle};
use crate::error::DiscordError;
use crate::event::Event;
use crate::model::Snowflake;

/// Capacity of the client-wide external cache backend write queue.
pub(super) const CACHE_BACKEND_QUEUE_CAPACITY: usize = 8_192;

/// Snapshot of the external cache backend mirror's health, from
/// [`Context::cache_backend_health`](super::Context::cache_backend_health).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CacheBackendHealth {
    /// `false` while a shard resync is pending: from the first dropped or
    /// failed write of a shard until `clear_shard_cache` succeeded for it.
    /// The mirror may be incomplete but is never stale.
    pub healthy: bool,
    /// Writes that did not fit in the queue (each scheduled a resync).
    pub dropped_writes: u64,
    /// Backend calls that returned an error (each scheduled a resync).
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

#[derive(Default)]
struct ForwarderState {
    next_ticket: u64,
    /// Shards whose mirror lost or failed a write, keyed to the ticket
    /// before which every queued write must be applied first: the resync
    /// must follow the writes that preceded the loss, or a stale put could
    /// land after the clear.
    pending_resync: BTreeMap<ShardPair, u64>,
    dropped_writes: u64,
    failed_writes: u64,
    resyncs: u64,
    last_error: Option<String>,
    last_drop_warning: Option<std::time::Instant>,
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
            healthy: state.pending_resync.is_empty(),
            dropped_writes: state.dropped_writes,
            failed_writes: state.failed_writes,
            resyncs: state.resyncs,
            pending_resync_shards: state.pending_resync.keys().copied().collect(),
            last_error: state.last_error.clone(),
        }
    }

    fn schedule_resync(state: &mut ForwarderState, shard: ShardPair, after_ticket: u64) {
        let entry = state.pending_resync.entry(shard).or_insert(after_ticket);
        *entry = (*entry).max(after_ticket);
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
    /// worker no longer issues backend calls, so a restarted client cannot
    /// interleave with late writes from this one.
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

    /// Queues a write from shard `shard`. Never waits: when the queue is
    /// full the write is not queued and the shard is scheduled for a resync
    /// behind everything already queued.
    pub(super) fn forward(&self, shard: ShardPair, write: BackendWrite) {
        let mut state = self.shared.state();
        let ticket = state.next_ticket;
        match self.writes.try_send(QueuedWrite {
            ticket: Some(ticket),
            shard,
            write,
        }) {
            Ok(()) => state.next_ticket += 1,
            Err(mpsc::error::TrySendError::Full(_)) => {
                state.dropped_writes += 1;
                ForwarderShared::schedule_resync(&mut state, shard, ticket);
                let now = std::time::Instant::now();
                if state
                    .last_drop_warning
                    .is_none_or(|last| now.duration_since(last) >= super::OVERLOAD_WARNING_INTERVAL)
                {
                    state.last_drop_warning = Some(now);
                    warn!(
                        "cache backend write queue is full ({} writes); {} write(s) dropped so \
                         far. Shard {}/{} will be resynced (clear_shard_cache) once the queued \
                         writes are applied",
                        self.writes.max_capacity(),
                        state.dropped_writes,
                        shard.0,
                        shard.1
                    );
                }
            }
            // The worker stopped (client shutdown); nothing left to mirror.
            Err(mpsc::error::TrySendError::Closed(_)) => {}
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

async fn run_cache_backend_worker(
    backend: Arc<dyn CacheBackend>,
    mut writes: mpsc::Receiver<QueuedWrite>,
    shared: Arc<ForwarderShared>,
) {
    while let Some(queued) = writes.recv().await {
        let QueuedWrite {
            ticket,
            shard,
            write,
        } = queued;
        if let Err(error) = apply_backend_write(backend.as_ref(), shard, write).await {
            warn!(
                "cache backend {error}; shard {}/{} will be resynced",
                shard.0, shard.1
            );
            let mut state = shared.state();
            state.failed_writes += 1;
            state.last_error = Some(error);
            // Resync right after this write.
            let after = ticket.map_or(0, |ticket| ticket + 1);
            ForwarderShared::schedule_resync(&mut state, shard, after);
        }
        if let Some(ticket) = ticket {
            run_due_resyncs(backend.as_ref(), &shared, ticket).await;
        }
    }
}

/// Runs `clear_shard_cache` for every pending shard whose preceding writes
/// (all tickets before its threshold) have now been applied.
async fn run_due_resyncs(backend: &dyn CacheBackend, shared: &ForwarderShared, applied: u64) {
    let due: Vec<(ShardPair, u64)> = shared
        .state()
        .pending_resync
        .iter()
        .filter(|(_, after)| **after <= applied + 1)
        .map(|(shard, after)| (*shard, *after))
        .collect();
    for (shard, after) in due {
        let result = backend.clear_shard_cache(shard.0, shard.1).await;
        let mut state = shared.state();
        match result {
            Ok(()) => {
                state.resyncs += 1;
                // A newer loss for the same shard keeps it pending.
                if state.pending_resync.get(&shard) == Some(&after) {
                    state.pending_resync.remove(&shard);
                }
            }
            Err(error) => {
                warn!(
                    "cache backend resync (clear_shard_cache) of shard {}/{} failed, retrying \
                     after the next write: {error}",
                    shard.0, shard.1
                );
                state.failed_writes += 1;
                state.last_error = Some(format!("clear_shard_cache failed: {error}"));
            }
        }
    }
}

/// Applies one write. Errors name the failed operation; any error leaves
/// the shard's mirror possibly stale, so the caller schedules a resync.
async fn apply_backend_write(
    backend: &dyn CacheBackend,
    shard: ShardPair,
    write: BackendWrite,
) -> Result<(), String> {
    fn named(operation: &str, result: Result<(), DiscordError>) -> Result<(), String> {
        result.map_err(|error| format!("{operation} failed: {error}"))
    }

    match write {
        BackendWrite::ClearShard => named(
            "clear_shard_cache",
            backend.clear_shard_cache(shard.0, shard.1).await,
        ),
        BackendWrite::PutMember {
            guild_id,
            user_id,
            member,
        } => named(
            "put_member",
            backend.put_member(guild_id, user_id, *member).await,
        ),
        BackendWrite::DeleteMember { guild_id, user_id } => named(
            "delete_member",
            backend.delete_member(&guild_id, &user_id).await,
        ),
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
                let result = named(
                    "put_member",
                    backend.put_member(guild_id.clone(), user_id, member).await,
                );
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
                let result = named(
                    "put_presence",
                    backend
                        .put_presence(guild_id.clone(), user_id, presence)
                        .await,
                );
                first_error = first_error.and(result);
            }
            first_error
        }
        BackendWrite::PutMessage(message) => {
            named("put_message", backend.put_message(*message).await)
        }
        BackendWrite::UpdateMessage(update) => {
            let BackendMessageUpdate {
                partial,
                fields,
                in_memory,
            } = *update;
            let stored = backend
                .get_message(&partial.channel_id, &partial.id)
                .await
                .map_err(|error| format!("get_message failed: {error}"));
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
            named("put_message", backend.put_message(merged).await)
        }
        BackendWrite::DeleteMessages {
            channel_id,
            message_ids,
        } => {
            let mut first_error = Ok(());
            for message_id in &message_ids {
                let result = named(
                    "delete_message",
                    backend.delete_message(&channel_id, message_id).await,
                );
                first_error = first_error.and(result);
            }
            first_error
        }
        BackendWrite::PutPresence {
            guild_id,
            user_id,
            presence,
        } => named(
            "put_presence",
            backend.put_presence(guild_id, user_id, *presence).await,
        ),
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
