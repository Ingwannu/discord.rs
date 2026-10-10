use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex as StdMutex, MutexGuard as StdMutexGuard};

use futures_util::stream::{FuturesUnordered, StreamExt};
use tokio::task::{JoinError, JoinHandle};
use tokio::time::{timeout_at, Duration, Instant};
use tracing::warn;

use super::client::voice_state_update_payload;
use crate::error::DiscordError;
use crate::sharding::{ShardIpcMessage, ShardRuntimeStatus, ShardSupervisorEvent, ShardingManager};
use crate::types::invalid_data_error;

pub(super) fn lock_sharding_manager(
    manager: &StdMutex<ShardingManager>,
) -> StdMutexGuard<'_, ShardingManager> {
    manager
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Handle to the shards started by `ClientBuilder::spawn_shards` in this
/// process: status inspection, per-shard commands, and shutdown.
///
/// Dropping the supervisor without waiting leaves the shards running.
/// `wait*` methods watch every shard at once; see
/// [`ShardSupervisor::wait`].
pub struct ShardSupervisor {
    pub(super) manager: Arc<StdMutex<ShardingManager>>,
    pub(super) tasks: Vec<(u32, JoinHandle<Result<(), DiscordError>>)>,
    /// How long the remaining shards get to stop gracefully after one shard
    /// failed, before they are aborted.
    pub(super) stop_timeout: Duration,
}

type ShardTaskResult = (u32, Result<Result<(), DiscordError>, JoinError>);
type ShardTaskFuture = Pin<Box<dyn Future<Output = ShardTaskResult> + Send>>;

/// Owns a shard task while the supervisor waits on it and aborts it when
/// dropped. Waiting consumes the supervisor, so a cancelled or abandoned
/// wait would otherwise leave shards running with nothing able to stop them.
struct AbortShardOnDrop(JoinHandle<Result<(), DiscordError>>);

impl Drop for AbortShardOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

fn watch_shard(shard_id: u32, task: JoinHandle<Result<(), DiscordError>>) -> ShardTaskFuture {
    let mut task = AbortShardOnDrop(task);
    Box::pin(async move { (shard_id, (&mut task.0).await) })
}

impl ShardSupervisor {
    const SHUTDOWN_TIMEOUT: Duration = Duration::from_millis(15_000);

    pub fn manager(&self) -> Arc<StdMutex<ShardingManager>> {
        Arc::clone(&self.manager)
    }

    pub fn statuses(&self) -> Vec<ShardRuntimeStatus> {
        lock_sharding_manager(&self.manager).statuses()
    }

    pub fn drain_events(&self) -> Result<Vec<ShardSupervisorEvent>, DiscordError> {
        lock_sharding_manager(&self.manager).drain_events()
    }

    pub fn send(&self, shard_id: u32, message: ShardIpcMessage) -> Result<(), DiscordError> {
        lock_sharding_manager(&self.manager).send(shard_id, message)
    }

    pub fn reconnect(&self, shard_id: u32) -> Result<(), DiscordError> {
        self.send(shard_id, ShardIpcMessage::Reconnect)
    }

    pub fn update_presence(
        &self,
        shard_id: u32,
        status: impl Into<String>,
    ) -> Result<(), DiscordError> {
        self.send(shard_id, ShardIpcMessage::UpdatePresence(status.into()))
    }

    pub fn update_voice_state(
        &self,
        shard_id: u32,
        guild_id: impl Into<crate::model::Snowflake>,
        channel_id: Option<crate::model::Snowflake>,
        self_mute: bool,
        self_deaf: bool,
    ) -> Result<(), DiscordError> {
        self.send(
            shard_id,
            ShardIpcMessage::SendPayload(voice_state_update_payload(
                guild_id.into(),
                channel_id,
                self_mute,
                self_deaf,
            )),
        )
    }

    pub fn join_voice(
        &self,
        shard_id: u32,
        guild_id: impl Into<crate::model::Snowflake>,
        channel_id: impl Into<crate::model::Snowflake>,
        self_mute: bool,
        self_deaf: bool,
    ) -> Result<(), DiscordError> {
        self.update_voice_state(
            shard_id,
            guild_id,
            Some(channel_id.into()),
            self_mute,
            self_deaf,
        )
    }

    pub fn leave_voice(
        &self,
        shard_id: u32,
        guild_id: impl Into<crate::model::Snowflake>,
        self_mute: bool,
        self_deaf: bool,
    ) -> Result<(), DiscordError> {
        self.update_voice_state(shard_id, guild_id, None, self_mute, self_deaf)
    }

    pub fn broadcast(&self, message: ShardIpcMessage) -> Result<(), DiscordError> {
        lock_sharding_manager(&self.manager).broadcast(message)
    }

    pub fn shutdown(&self) -> Result<(), DiscordError> {
        self.broadcast(ShardIpcMessage::Shutdown)
    }

    pub async fn shutdown_and_wait(self) -> Result<(), DiscordError> {
        self.shutdown()?;
        self.wait_for_shutdown(Self::SHUTDOWN_TIMEOUT).await
    }

    /// [`ShardSupervisor::wait`] with an overall deadline; shards still
    /// running when it passes are aborted and an error is returned.
    pub async fn wait_for_shutdown(self, timeout_duration: Duration) -> Result<(), DiscordError> {
        self.wait_with_timeout(Some(timeout_duration)).await
    }

    /// Waits until every shard has stopped.
    ///
    /// All shards are watched concurrently. The first shard that fails —
    /// for example with [`DiscordError::Gateway`] for a non-reconnectable
    /// close code such as 4004 — makes the supervisor send `Shutdown` to the
    /// remaining shards, give them the handler shutdown timeout plus two
    /// seconds to stop, abort whatever is still running, and return that
    /// first error. A shard that stops cleanly does not affect the others.
    /// Cancelling this future aborts every shard still running.
    pub async fn wait(self) -> Result<(), DiscordError> {
        self.wait_with_timeout(None).await
    }

    /// Decision log:
    /// - Purpose: surface a fatal error from any shard promptly and return
    ///   only after the remaining shards stopped.
    /// - Previous design: shard tasks were awaited in order, so a later
    ///   shard's failure (for example 4004) stayed hidden while an earlier
    ///   shard ran, and an early error dropped the other handles without
    ///   stopping their tasks.
    /// - Alternatives considered: (1) return immediately and abort the rest —
    ///   no close frame and in-flight handlers are cut off; (2) wait for the
    ///   rest to finish on their own — may never return; (3) broadcast
    ///   `Shutdown`, wait a bounded time, then abort.
    /// - Chosen: (3), watching all shards with `FuturesUnordered`.
    /// - Why: guarantees nothing is running when `wait` returns while still
    ///   preferring the graceful path (close frame, handler drain).
    /// - Trade-offs: the error surfaces within `stop_timeout`. Cancelling the
    ///   wait or hitting its timeout aborts the remaining shards (they used
    ///   to be left running detached).
    async fn wait_with_timeout(
        self,
        timeout_duration: Option<Duration>,
    ) -> Result<(), DiscordError> {
        let ShardSupervisor {
            manager,
            tasks,
            stop_timeout,
        } = self;
        // `checked_add` keeps an absurdly large timeout from panicking; it
        // then behaves like no timeout at all.
        let deadline = timeout_duration.and_then(|duration| Instant::now().checked_add(duration));
        let mut pending: Vec<u32> = tasks.iter().map(|(shard_id, _)| *shard_id).collect();
        let mut running: FuturesUnordered<ShardTaskFuture> = tasks
            .into_iter()
            .map(|(shard_id, task)| watch_shard(shard_id, task))
            .collect();

        loop {
            let next = match deadline {
                Some(deadline) => match timeout_at(deadline, running.next()).await {
                    Ok(next) => next,
                    Err(_) => {
                        let first_pending = pending.iter().min().copied().unwrap_or_default();
                        // Dropping `running` aborts every shard still running.
                        return Err(invalid_data_error(format!(
                            "timed out waiting for shard {first_pending} shutdown after {:?}",
                            timeout_duration.unwrap_or_default()
                        )));
                    }
                },
                None => running.next().await,
            };
            let Some((shard_id, result)) = next else {
                return Ok(());
            };
            pending.retain(|pending_id| *pending_id != shard_id);

            let error = match result {
                Ok(Ok(())) => continue,
                Ok(Err(error)) => error,
                Err(error) => format!("shard task failed: {error}").into(),
            };
            warn!("shard {shard_id} stopped with an error, stopping the remaining shards: {error}");
            stop_remaining_shards(&manager, running, stop_timeout).await;
            return Err(error);
        }
    }
}

/// Asks the remaining shards to shut down, waits up to `stop_timeout`, and
/// aborts any that are still running.
async fn stop_remaining_shards(
    manager: &StdMutex<ShardingManager>,
    mut running: FuturesUnordered<ShardTaskFuture>,
    stop_timeout: Duration,
) {
    if running.is_empty() {
        return;
    }
    if let Err(error) = lock_sharding_manager(manager).broadcast(ShardIpcMessage::Shutdown) {
        warn!("failed to send shutdown to every remaining shard: {error}");
    }
    let deadline = Instant::now().checked_add(stop_timeout);
    loop {
        let next = match deadline {
            Some(deadline) => match timeout_at(deadline, running.next()).await {
                Ok(next) => next,
                Err(_) => break,
            },
            None => running.next().await,
        };
        let Some((shard_id, result)) = next else {
            break;
        };
        match result {
            Ok(Ok(())) => {}
            Ok(Err(error)) => warn!("shard {shard_id} failed while stopping: {error}"),
            Err(error) => warn!("shard {shard_id} task failed while stopping: {error}"),
        }
    }
    if !running.is_empty() {
        warn!(
            "{} shard(s) did not stop within {stop_timeout:?}; aborting them",
            running.len()
        );
    }
    // Dropping `running` aborts whatever is left.
}
