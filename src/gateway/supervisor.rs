use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex as StdMutex, MutexGuard as StdMutexGuard};

use futures_util::stream::{FuturesUnordered, StreamExt};
use tokio::sync::watch;
use tokio::task::{AbortHandle, JoinError, JoinHandle};
use tokio::time::{timeout_at, Duration, Instant};
use tracing::warn;

use super::client::voice_state_update_payload;
use crate::error::DiscordError;
use crate::model::GenericId;
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
    pub(super) background: Option<JoinHandle<Result<(), String>>>,
    /// Raised by the first shard that fails, before it drains its handlers,
    /// so the boot schedule and `wait*` react without waiting for the drain.
    pub(super) failures: Option<FleetFailure>,
}

/// The first fatal failure of a supervised group of shards.
///
/// A failing shard can spend up to the handler shutdown timeout draining
/// before its task completes. Raising the failure first lets the boot
/// scheduler stop releasing identify waves, and the supervisor start
/// stopping the other shards, immediately.
#[derive(Clone)]
pub(crate) struct FleetFailure(Arc<watch::Sender<Option<(u32, String)>>>);

impl FleetFailure {
    pub(crate) fn new() -> Self {
        let (failure, _) = watch::channel(None);
        Self(Arc::new(failure))
    }

    /// Records the failure of `shard_id`; the first one wins.
    pub(crate) fn raise(&self, shard_id: u32, message: String) {
        self.0.send_if_modified(|current| {
            if current.is_none() {
                *current = Some((shard_id, message));
                true
            } else {
                false
            }
        });
    }

    pub(crate) fn subscribe(&self) -> watch::Receiver<Option<(u32, String)>> {
        self.0.subscribe()
    }
}

/// Resolves with the first raised failure; pends forever without a watch.
pub(crate) async fn failure_raised(
    failures: &mut Option<watch::Receiver<Option<(u32, String)>>>,
) -> (u32, String) {
    let Some(failures) = failures else {
        return std::future::pending().await;
    };
    loop {
        if let Some(failure) = failures.borrow_and_update().clone() {
            return failure;
        }
        if failures.changed().await.is_err() {
            return std::future::pending().await;
        }
    }
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
            .map(|(shard_id, task)| -> ShardTaskFuture {
                aborts.insert(shard_id, task.abort_handle());
                let mut task = AbortOnDrop(task);
                Box::pin(async move { (shard_id, (&mut task.0).await) })
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
    background: Option<AbortOnDrop<Result<(), String>>>,
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
        // The worker reports recovery it could not finish, or that another
        // client took over the store.
        Some(Ok(result)) => result,
        Some(Err(_)) => Ok(()),
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
        guild_id: GenericId,
        channel_id: Option<GenericId>,
        self_mute: bool,
        self_deaf: bool,
    ) -> Result<(), DiscordError> {
        self.send(
            shard_id,
            ShardIpcMessage::SendPayload(voice_state_update_payload(
                guild_id, channel_id, self_mute, self_deaf,
            )),
        )
    }

    pub fn join_voice(
        &self,
        shard_id: u32,
        guild_id: GenericId,
        channel_id: GenericId,
        self_mute: bool,
        self_deaf: bool,
    ) -> Result<(), DiscordError> {
        self.update_voice_state(shard_id, guild_id, Some(channel_id), self_mute, self_deaf)
    }

    pub fn leave_voice(
        &self,
        shard_id: u32,
        guild_id: GenericId,
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
    /// send error is returned after cleanup. Like the other `wait*`
    /// methods, the returned future owns the shards from the moment it is
    /// created: dropping it, even unpolled, aborts them.
    pub fn shutdown_and_wait(self) -> impl Future<Output = Result<(), DiscordError>> + Send {
        let supervised = SupervisedShards::new(self, Some(Self::SHUTDOWN_TIMEOUT));
        async move {
            let sent = supervised.shutdown();
            let waited = supervised.run().await;
            match (sent, waited) {
                (Ok(()), waited) => waited,
                (Err(send_error), Ok(())) => Err(send_error),
                (Err(send_error), Err(wait_error)) => Err(invalid_data_error(format!(
                    "{wait_error} (sending shutdown also failed: {send_error})"
                ))),
            }
        }
    }

    /// [`ShardSupervisor::wait`] with an overall deadline (counted from the
    /// first poll); shards still running when it passes are aborted and an
    /// error is returned.
    pub fn wait_for_shutdown(
        self,
        timeout_duration: Duration,
    ) -> impl Future<Output = Result<(), DiscordError>> + Send {
        SupervisedShards::new(self, Some(timeout_duration)).run()
    }

    /// Waits until every shard has stopped.
    ///
    /// All shards are watched concurrently, including while later identify
    /// waves are still queued. The first shard that fails — for example with
    /// [`DiscordError::Gateway`] for a non-reconnectable close code such as
    /// 4004 — raises the failure before draining its own handlers, which
    /// stops the boot schedule (queued shards never connect) and makes the
    /// supervisor send `Shutdown` to the running shards at once. They get
    /// the handler shutdown timeout plus two seconds (never past an overall
    /// deadline); whatever is still running is then aborted and joined, and
    /// the first error is returned. A shard that stops cleanly does not
    /// affect the others. The external cache backend worker is drained last.
    ///
    /// The returned future owns the shards from the moment it is created:
    /// dropping it — polled or not — aborts every shard still running and
    /// the boot schedule.
    pub fn wait(self) -> impl Future<Output = Result<(), DiscordError>> + Send {
        SupervisedShards::new(self, None).run()
    }
}

/// The shards, boot schedule, and background work of one `wait*` call.
///
/// Built eagerly when the `wait*` future is created, so every task is under
/// an abort-on-drop guard even if the future is dropped before its first
/// poll (an `async fn` would only install guards inside its body).
struct SupervisedShards {
    manager: Arc<StdMutex<ShardingManager>>,
    shards: ShardSet,
    stop_timeout: Duration,
    timeout: Option<Duration>,
    boot_scheduler: Option<AbortOnDrop<()>>,
    background: Option<AbortOnDrop<Result<(), String>>>,
    failures: Option<watch::Receiver<Option<(u32, String)>>>,
}

/// The failure that ends a wait: a shard task that completed with an error,
/// or one that raised a failure and is still draining.
enum FirstFailure {
    Completed(DiscordError),
    Raised { shard_id: u32, message: String },
}

impl SupervisedShards {
    fn new(supervisor: ShardSupervisor, timeout: Option<Duration>) -> Self {
        let ShardSupervisor {
            manager,
            tasks,
            stop_timeout,
            boot_scheduler,
            background,
            failures,
        } = supervisor;
        Self {
            manager,
            shards: ShardSet::new(tasks),
            stop_timeout,
            timeout,
            boot_scheduler: boot_scheduler.map(AbortOnDrop),
            background: background.map(AbortOnDrop),
            failures: failures.map(|failures| failures.subscribe()),
        }
    }

    fn shutdown(&self) -> Result<(), DiscordError> {
        if let Some(boot_scheduler) = &self.boot_scheduler {
            boot_scheduler.0.abort();
        }
        lock_sharding_manager(&self.manager).broadcast(ShardIpcMessage::Shutdown)
    }

    /// Decision log:
    /// - Purpose: surface a fatal error from any shard promptly and return
    ///   only once the remaining shards and background work have stopped,
    ///   or say which could not be confirmed.
    /// - Previous design: shard tasks were awaited in order, so a later
    ///   shard's failure (for example 4004) stayed hidden while an earlier
    ///   shard ran; an early error dropped the other handles without stopping
    ///   their tasks; boot waves kept launching while a failed shard drained;
    ///   and guards were installed only once the wait was first polled.
    /// - Alternatives considered: (1) return immediately and abort the rest —
    ///   no close frame and in-flight handlers are cut off; (2) wait for the
    ///   rest to finish on their own — may never return; (3) stop the boot
    ///   schedule, broadcast `Shutdown`, wait a bounded time, then abort and
    ///   join.
    /// - Chosen: (3), watching all shards with `FuturesUnordered` and the
    ///   fleet failure signal; the stop window is the earlier of
    ///   `stop_timeout` and the caller's deadline; guards are created with
    ///   the future.
    /// - Why: nothing is left running when `wait` returns, while the
    ///   graceful path (close frame, handler drain) is still preferred.
    /// - Trade-offs: the error surfaces within the stop window. Dropping the
    ///   wait or hitting its deadline aborts the remaining shards. A task
    ///   that does not confirm cancellation within a second is reported in
    ///   the returned error instead of being claimed stopped.
    async fn run(mut self) -> Result<(), DiscordError> {
        // `checked_add` keeps an absurdly large timeout from panicking; it
        // then behaves like no timeout at all.
        let deadline = self
            .timeout
            .and_then(|duration| Instant::now().checked_add(duration));
        loop {
            let next = tokio::select! {
                biased;
                (shard_id, message) = failure_raised(&mut self.failures) => {
                    return self
                        .stop_after_failure(FirstFailure::Raised { shard_id, message }, deadline)
                        .await;
                }
                next = self.shards.next_until(deadline) => next,
            };
            match next {
                ShardNext::Stopped(_, Ok(())) => continue,
                ShardNext::AllStopped => {
                    self.boot_scheduler.take();
                    return finish_background(
                        self.background.take(),
                        earliest_deadline(self.stop_timeout, deadline),
                    )
                    .await
                    .map_err(DiscordError::gateway);
                }
                ShardNext::TimedOut => {
                    self.boot_scheduler.take();
                    let first_pending = self.shards.first_pending();
                    let unconfirmed = self.shards.abort_and_confirm().await;
                    let background =
                        finish_background(self.background.take(), Some(Instant::now())).await;
                    let mut message = format!(
                        "timed out waiting for shard {first_pending} shutdown after {:?}",
                        self.timeout.unwrap_or_default()
                    );
                    append_cleanup_notes(&mut message, unconfirmed, background);
                    return Err(invalid_data_error(message));
                }
                ShardNext::Stopped(_, Err(error)) => {
                    return self
                        .stop_after_failure(FirstFailure::Completed(error), deadline)
                        .await;
                }
            }
        }
    }

    async fn stop_after_failure(
        mut self,
        first: FirstFailure,
        deadline: Option<Instant>,
    ) -> Result<(), DiscordError> {
        let (failing_shard, mut error) = match first {
            FirstFailure::Completed(error) => (None, Some(error)),
            FirstFailure::Raised { shard_id, message } => {
                warn!("shard {shard_id} failed, stopping the remaining shards: {message}");
                (Some((shard_id, message)), None)
            }
        };
        if let Some(error) = &error {
            warn!("a shard stopped with an error, stopping the remaining shards: {error}");
        }
        // Queued shards must never connect after a failure.
        self.boot_scheduler.take();
        if let Err(send_error) =
            lock_sharding_manager(&self.manager).broadcast(ShardIpcMessage::Shutdown)
        {
            warn!("failed to send shutdown to every remaining shard: {send_error}");
        }
        let stop_deadline = earliest_deadline(self.stop_timeout, deadline);
        loop {
            match self.shards.next_until(stop_deadline).await {
                ShardNext::Stopped(shard_id, Err(shard_error)) => {
                    // The shard that raised the failure reports its real
                    // error (with its variant) once it finished draining.
                    if error.is_none()
                        && failing_shard
                            .as_ref()
                            .is_some_and(|(failing, _)| *failing == shard_id)
                    {
                        error = Some(shard_error);
                    } else {
                        warn!("shard {shard_id} failed while stopping: {shard_error}");
                    }
                }
                ShardNext::Stopped(_, Ok(())) => {}
                ShardNext::AllStopped | ShardNext::TimedOut => break,
            }
        }
        let unconfirmed = if self.shards.running.is_empty() {
            0
        } else {
            warn!(
                "{} shard(s) did not stop within the stop window; aborting them",
                self.shards.running.len()
            );
            self.shards.abort_and_confirm().await
        };
        let background = finish_background(self.background.take(), stop_deadline).await;
        let error = error.unwrap_or_else(|| {
            let (shard_id, message) = failing_shard.unwrap_or_default();
            DiscordError::gateway(format!("shard {shard_id} failed: {message}"))
        });
        if unconfirmed == 0 && background.is_ok() {
            return Err(error);
        }
        let mut message = error.to_string();
        append_cleanup_notes(&mut message, unconfirmed, background);
        Err(DiscordError::gateway(message))
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
