use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex as StdMutex, MutexGuard as StdMutexGuard};

use futures_util::stream::{FuturesUnordered, StreamExt};
use tokio::task::{AbortHandle, JoinError, JoinHandle};
use tokio::time::{timeout_at, Duration, Instant};
use tracing::warn;

use super::client::voice_state_update_payload;
use crate::error::DiscordError;
use crate::sharding::{ShardIpcMessage, ShardRuntimeStatus, ShardSupervisorEvent, ShardingManager};
use crate::types::invalid_data_error;

/// How long aborted tasks get to confirm they stopped. Aborting is
/// asynchronous (a task stops at its next await point), so the supervisor
/// joins aborted tasks instead of assuming they are gone.
const ABORT_CONFIRM_GRACE: Duration = Duration::from_secs(1);

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
/// The supervisor owns the shard tasks from the moment `spawn_shards`
/// returns, which is before later identify waves have started; queued
/// shards report `ShardRuntimeState::Queued`. Dropping the supervisor
/// without waiting leaves the shards (and the boot schedule) running.
/// `wait*` methods watch every shard at once; see [`ShardSupervisor::wait`].
pub struct ShardSupervisor {
    pub(super) manager: Arc<StdMutex<ShardingManager>>,
    pub(super) tasks: Vec<(u32, JoinHandle<Result<(), DiscordError>>)>,
    /// How long the remaining shards get to stop gracefully after one shard
    /// failed, before they are aborted.
    pub(super) stop_timeout: Duration,
    /// Releases queued shards wave by wave. Aborting it makes every shard
    /// that has not been released yet stop without connecting.
    pub(super) boot_scheduler: Option<JoinHandle<()>>,
    /// Client-wide background work (the external cache backend worker),
    /// drained after every shard stopped.
    pub(super) background: Option<JoinHandle<()>>,
}

type ShardTaskResult = (u32, Result<Result<(), DiscordError>, JoinError>);
type ShardTaskFuture = Pin<Box<dyn Future<Output = ShardTaskResult> + Send>>;

/// Aborts a task when dropped. Waiting consumes the supervisor, so a
/// cancelled or abandoned wait would otherwise leave tasks running with
/// nothing able to stop them.
struct AbortOnDrop<T>(JoinHandle<T>);

impl<T> Drop for AbortOnDrop<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// The shard tasks a `wait*` call is watching.
struct ShardSet {
    running: FuturesUnordered<ShardTaskFuture>,
    aborts: HashMap<u32, AbortHandle>,
}

enum ShardNext {
    Stopped(u32, Result<(), DiscordError>),
    AllStopped,
    TimedOut,
}

impl ShardSet {
    fn new(tasks: Vec<(u32, JoinHandle<Result<(), DiscordError>>)>) -> Self {
        let mut aborts = HashMap::with_capacity(tasks.len());
        let running = tasks
            .into_iter()
            .map(|(shard_id, task)| {
                aborts.insert(shard_id, task.abort_handle());
                let mut task = AbortOnDrop(task);
                Box::pin(async move { (shard_id, (&mut task.0).await) }) as ShardTaskFuture
            })
            .collect();
        Self { running, aborts }
    }

    fn first_pending(&self) -> u32 {
        self.aborts.keys().min().copied().unwrap_or_default()
    }

    async fn next_until(&mut self, deadline: Option<Instant>) -> ShardNext {
        let next = match deadline {
            Some(deadline) => match timeout_at(deadline, self.running.next()).await {
                Ok(next) => next,
                Err(_) => return ShardNext::TimedOut,
            },
            None => self.running.next().await,
        };
        let Some((shard_id, result)) = next else {
            return ShardNext::AllStopped;
        };
        self.aborts.remove(&shard_id);
        ShardNext::Stopped(
            shard_id,
            result.unwrap_or_else(|error| Err(format!("shard task failed: {error}").into())),
        )
    }

    /// Aborts every shard still running and joins them for up to
    /// [`ABORT_CONFIRM_GRACE`]. Returns how many did not confirm.
    async fn abort_and_confirm(&mut self) -> usize {
        for abort in self.aborts.values() {
            abort.abort();
        }
        let deadline = Instant::now() + ABORT_CONFIRM_GRACE;
        while let ShardNext::Stopped(..) = self.next_until(Some(deadline)).await {}
        self.running.len()
    }
}

/// The earlier of `now + duration` and `limit`; `None` means no limit.
fn earliest_deadline(duration: Duration, limit: Option<Instant>) -> Option<Instant> {
    let own = Instant::now().checked_add(duration);
    match (own, limit) {
        (Some(own), Some(limit)) => Some(own.min(limit)),
        (own, None) => own,
        (None, limit) => limit,
    }
}

/// Waits for background work until `deadline`, then aborts and joins it.
async fn finish_background(
    background: Option<AbortOnDrop<()>>,
    deadline: Option<Instant>,
) -> Result<(), String> {
    let Some(mut background) = background else {
        return Ok(());
    };
    let finished = match deadline {
        Some(deadline) => timeout_at(deadline, &mut background.0).await.ok(),
        None => Some((&mut background.0).await),
    };
    match finished {
        Some(Err(error)) if error.is_panic() => {
            Err(format!("cache backend worker panicked: {error}"))
        }
        Some(_) => Ok(()),
        None => {
            background.0.abort();
            match tokio::time::timeout(ABORT_CONFIRM_GRACE, &mut background.0).await {
                Ok(_) => Err(
                    "cache backend worker did not flush its queued writes in time and was aborted"
                        .to_string(),
                ),
                Err(_) => Err(format!(
                    "cache backend worker did not flush in time and did not confirm cancellation \
                     within {ABORT_CONFIRM_GRACE:?} of being aborted"
                )),
            }
        }
    }
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

    /// Stops the boot schedule (shards not released yet stop without
    /// connecting) and sends `Shutdown` to every running shard. Every shard
    /// is attempted; the first send error is returned afterwards.
    pub fn shutdown(&self) -> Result<(), DiscordError> {
        if let Some(boot_scheduler) = &self.boot_scheduler {
            boot_scheduler.abort();
        }
        self.broadcast(ShardIpcMessage::Shutdown)
    }

    /// [`ShardSupervisor::shutdown`], then waits up to 15 seconds for every
    /// shard to stop (aborting the rest). The wait runs even when sending
    /// `Shutdown` failed for some shard, so no shard is left unowned; the
    /// send error is returned after cleanup.
    pub async fn shutdown_and_wait(self) -> Result<(), DiscordError> {
        let sent = self.shutdown();
        let waited = self.wait_for_shutdown(Self::SHUTDOWN_TIMEOUT).await;
        match (sent, waited) {
            (Ok(()), waited) => waited,
            (Err(send_error), Ok(())) => Err(send_error),
            (Err(send_error), Err(wait_error)) => Err(invalid_data_error(format!(
                "{wait_error} (sending shutdown also failed: {send_error})"
            ))),
        }
    }

    /// [`ShardSupervisor::wait`] with an overall deadline; shards still
    /// running when it passes are aborted and an error is returned.
    pub async fn wait_for_shutdown(self, timeout_duration: Duration) -> Result<(), DiscordError> {
        self.wait_with_timeout(Some(timeout_duration)).await
    }

    /// Waits until every shard has stopped.
    ///
    /// All shards are watched concurrently, including while later identify
    /// waves are still queued. The first shard that fails — for example with
    /// [`DiscordError::Gateway`] for a non-reconnectable close code such as
    /// 4004 — stops the boot schedule (queued shards never connect), sends
    /// `Shutdown` to the running shards, gives them the handler shutdown
    /// timeout plus two seconds (never past an overall deadline), then
    /// aborts and joins whatever is still running and returns that first
    /// error. A shard that stops cleanly does not affect the others. The
    /// external cache backend worker is drained last. Cancelling this future
    /// aborts every shard still running.
    pub async fn wait(self) -> Result<(), DiscordError> {
        self.wait_with_timeout(None).await
    }

    /// Decision log:
    /// - Purpose: surface a fatal error from any shard promptly and return
    ///   only once the remaining shards and background work have stopped,
    ///   or say which could not be confirmed.
    /// - Previous design: shard tasks were awaited in order, so a later
    ///   shard's failure (for example 4004) stayed hidden while an earlier
    ///   shard ran, an early error dropped the other handles without stopping
    ///   their tasks, and boot waves were released before the supervisor
    ///   existed, so failures during boot were not acted on.
    /// - Alternatives considered: (1) return immediately and abort the rest —
    ///   no close frame and in-flight handlers are cut off; (2) wait for the
    ///   rest to finish on their own — may never return; (3) stop the boot
    ///   schedule, broadcast `Shutdown`, wait a bounded time, then abort and
    ///   join.
    /// - Chosen: (3), watching all shards with `FuturesUnordered`; the stop
    ///   window is the earlier of `stop_timeout` and the caller's deadline.
    /// - Why: nothing is left running when `wait` returns, while the
    ///   graceful path (close frame, handler drain) is still preferred.
    /// - Trade-offs: the error surfaces within the stop window. Cancelling
    ///   the wait or hitting its deadline aborts the remaining shards. A task
    ///   that does not confirm cancellation within a second is reported in
    ///   the returned error instead of being claimed stopped.
    async fn wait_with_timeout(
        self,
        timeout_duration: Option<Duration>,
    ) -> Result<(), DiscordError> {
        let ShardSupervisor {
            manager,
            tasks,
            stop_timeout,
            boot_scheduler,
            background,
        } = self;
        // `checked_add` keeps an absurdly large timeout from panicking; it
        // then behaves like no timeout at all.
        let deadline = timeout_duration.and_then(|duration| Instant::now().checked_add(duration));
        let mut boot_scheduler = boot_scheduler.map(AbortOnDrop);
        let background = background.map(AbortOnDrop);
        let mut shards = ShardSet::new(tasks);

        loop {
            match shards.next_until(deadline).await {
                ShardNext::Stopped(_, Ok(())) => continue,
                ShardNext::AllStopped => {
                    drop(boot_scheduler);
                    return finish_background(
                        background,
                        earliest_deadline(stop_timeout, deadline),
                    )
                    .await
                    .map_err(DiscordError::gateway);
                }
                ShardNext::TimedOut => {
                    drop(boot_scheduler);
                    let first_pending = shards.first_pending();
                    let unconfirmed = shards.abort_and_confirm().await;
                    let background = finish_background(background, Some(Instant::now())).await;
                    let mut message = format!(
                        "timed out waiting for shard {first_pending} shutdown after {:?}",
                        timeout_duration.unwrap_or_default()
                    );
                    append_cleanup_notes(&mut message, unconfirmed, background);
                    return Err(invalid_data_error(message));
                }
                ShardNext::Stopped(shard_id, Err(error)) => {
                    warn!(
                        "shard {shard_id} stopped with an error, stopping the remaining shards: \
                         {error}"
                    );
                    // Queued shards must never connect after a failure.
                    boot_scheduler.take();
                    if let Err(error) =
                        lock_sharding_manager(&manager).broadcast(ShardIpcMessage::Shutdown)
                    {
                        warn!("failed to send shutdown to every remaining shard: {error}");
                    }
                    let stop_deadline = earliest_deadline(stop_timeout, deadline);
                    loop {
                        match shards.next_until(stop_deadline).await {
                            ShardNext::Stopped(other, Err(other_error)) => {
                                warn!("shard {other} failed while stopping: {other_error}");
                            }
                            ShardNext::Stopped(_, Ok(())) => {}
                            ShardNext::AllStopped | ShardNext::TimedOut => break,
                        }
                    }
                    let unconfirmed = if shards.running.is_empty() {
                        0
                    } else {
                        warn!(
                            "{} shard(s) did not stop within the stop window; aborting them",
                            shards.running.len()
                        );
                        shards.abort_and_confirm().await
                    };
                    let background = finish_background(background, stop_deadline).await;
                    if unconfirmed == 0 && background.is_ok() {
                        return Err(error);
                    }
                    let mut message = error.to_string();
                    append_cleanup_notes(&mut message, unconfirmed, background);
                    return Err(DiscordError::gateway(message));
                }
            }
        }
    }
}

fn append_cleanup_notes(message: &mut String, unconfirmed: usize, background: Result<(), String>) {
    if unconfirmed > 0 {
        message.push_str(&format!(
            "; {unconfirmed} shard task(s) did not confirm cancellation within \
             {ABORT_CONFIRM_GRACE:?} of being aborted"
        ));
    }
    if let Err(error) = background {
        message.push_str("; ");
        message.push_str(&error);
    }
}
