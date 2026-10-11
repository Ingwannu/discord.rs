//! Per-shard event pipeline: gateway ingress → internal processor → handler
//! lanes. The design and its trade-offs are recorded on
//! [`super::start_gateway_shard`].

use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use serde_json::Value;
use tokio::sync::{mpsc, watch, Semaphore};
use tokio::task::{JoinError, JoinHandle, JoinSet};
use tokio::time::{Duration, Instant};
use tracing::{error, info, warn};

#[cfg(feature = "voice")]
use super::apply_voice_updates;
#[cfg(feature = "cache")]
use super::backend::{forward_cache_backend_updates, CacheBackendForwarder};
use super::{
    apply_cache_updates, lock_member_chunk_waiters, Context, DispatchConfig, ErasedEventHandler,
    InteractionOverloadHook, InteractionOverloadReason, MemberChunkWaiters, OverloadedInteraction,
    OVERLOAD_WARNING_INTERVAL,
};
use crate::event::{decode_event, Event};

/// How long aborted pipeline tasks get to confirm they stopped. Aborting is
/// asynchronous: a task only stops at its next await point, so shutdown
/// joins it afterwards instead of assuming it is gone.
pub(super) const ABORT_CONFIRM_GRACE: Duration = Duration::from_secs(1);

/// One gateway dispatch as handed over by the gateway read loop.
#[derive(Debug)]
pub(super) struct GatewayDispatch {
    pub(super) event_name: String,
    pub(super) data: Value,
    /// When the read loop handed the dispatch over; interaction admission
    /// deadlines count from here.
    pub(super) received_at: Instant,
}

impl GatewayDispatch {
    pub(super) fn new(event_name: String, data: Value) -> Self {
        Self {
            event_name,
            data,
            received_at: Instant::now(),
        }
    }
}

/// The first reason a shard's pipeline can no longer keep runtime state
/// consistent with the gateway stream.
///
/// The gateway client advances its sequence number before invoking the
/// event callback, so a dispatch the pipeline fails to process can never be
/// replayed by a RESUME. Any such loss (ingress overflow, a processor or
/// dispatcher that died) is raised here; the shard runtime races the
/// gateway against it and stops the shard, invalidates its cache, and fails
/// its pending member fetches instead of continuing with a silent gap.
#[derive(Clone)]
pub(super) struct PipelineFault {
    reason: Arc<watch::Sender<Option<String>>>,
    /// Cleared once the shard is being torn down on purpose, so tasks that
    /// stop during shutdown are not reported as consistency failures.
    armed: Arc<AtomicBool>,
}

impl PipelineFault {
    pub(super) fn new() -> Self {
        let (reason, _) = watch::channel(None);
        Self {
            reason: Arc::new(reason),
            armed: Arc::new(AtomicBool::new(true)),
        }
    }

    /// Records `reason` unless a fault was already raised or the pipeline is
    /// shutting down. Returns whether this call raised the fault.
    pub(super) fn raise(&self, reason: impl Into<String>) -> bool {
        if !self.armed.load(Ordering::SeqCst) {
            return false;
        }
        let reason = reason.into();
        self.reason.send_if_modified(|current| {
            if current.is_none() {
                *current = Some(reason);
                true
            } else {
                false
            }
        })
    }

    pub(super) fn reason(&self) -> Option<String> {
        self.reason.borrow().clone()
    }

    pub(super) fn is_raised(&self) -> bool {
        self.reason.borrow().is_some()
    }

    fn disarm(&self) {
        self.armed.store(false, Ordering::SeqCst);
    }

    /// Resolves with the fault reason once one is raised.
    pub(super) async fn raised(&self) -> String {
        let mut reason = self.reason.subscribe();
        loop {
            if let Some(reason) = reason.borrow_and_update().clone() {
                return reason;
            }
            if reason.changed().await.is_err() {
                // The sender lives in `self`, so this cannot happen while we
                // are polled; pend like an unraised fault.
                return std::future::pending().await;
            }
        }
    }
}

/// Raises a fault if the owning task ends without calling
/// [`FaultOnExit::complete`] — that is, if it panicked or was aborted while
/// the shard was still running.
struct FaultOnExit {
    fault: PipelineFault,
    reason: String,
    completed: bool,
}

impl FaultOnExit {
    fn new(fault: PipelineFault, reason: String) -> Self {
        Self {
            fault,
            reason,
            completed: false,
        }
    }

    fn complete(&mut self) {
        self.completed = true;
    }
}

impl Drop for FaultOnExit {
    fn drop(&mut self) {
        if !self.completed && self.fault.raise(self.reason.clone()) {
            error!("{}", self.reason);
        }
    }
}

/// Counts items dropped by an overflow policy and decides when to log.
///
/// Shared by synchronous callers and async stages, hence atomics and a std
/// mutex rather than task-local state.
#[derive(Default)]
pub(super) struct OverflowLog {
    dropped: AtomicU64,
    last_report: StdMutex<Option<std::time::Instant>>,
}

impl OverflowLog {
    /// Records one dropped item. Returns the running total when a warning
    /// is due: on the first drop and then at most every
    /// [`OVERLOAD_WARNING_INTERVAL`].
    pub(super) fn record(&self) -> Option<u64> {
        let total = self.dropped.fetch_add(1, Ordering::Relaxed) + 1;
        let mut last_report = self
            .last_report
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let now = std::time::Instant::now();
        if last_report.is_some_and(|last| now.duration_since(last) < OVERLOAD_WARNING_INTERVAL) {
            return None;
        }
        *last_report = Some(now);
        Some(total)
    }

    #[cfg(test)]
    pub(super) fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

/// Rate limiter for backlog warnings owned by a single task.
#[derive(Default)]
pub(super) struct BacklogWarning {
    last: Option<std::time::Instant>,
}

impl BacklogWarning {
    pub(super) fn due(&mut self) -> bool {
        let now = std::time::Instant::now();
        if self
            .last
            .is_some_and(|last| now.duration_since(last) < OVERLOAD_WARNING_INTERVAL)
        {
            return false;
        }
        self.last = Some(now);
        true
    }
}

/// The gateway-facing end of a shard's event pipeline.
pub(super) struct EventIngress {
    pub(super) shard_id: u32,
    pub(super) events: mpsc::Sender<GatewayDispatch>,
    pub(super) fault: PipelineFault,
    /// Dispatches that were not queued: the one that overflowed, and every
    /// one arriving after the fault while the shard is being stopped.
    pub(super) dropped: AtomicU64,
}

impl EventIngress {
    /// Queues a dispatch without ever waiting: this runs inside the gateway
    /// read loop, which must keep processing heartbeats. A full or closed
    /// queue raises the shard's [`PipelineFault`].
    pub(super) fn push(&self, dispatch: GatewayDispatch) {
        if self.fault.is_raised() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return;
        }
        match self.events.try_send(dispatch) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(dispatch)) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
                let reason = format!(
                    "shard {}: gateway event queue overflowed ({} events) at {}; internal event \
                     processing stalled, so the shard was stopped and its cache invalidated \
                     instead of continuing with a gap in its event stream",
                    self.shard_id,
                    self.events.max_capacity(),
                    dispatch.event_name
                );
                if self.fault.raise(reason.clone()) {
                    error!("{reason}");
                }
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                self.dropped.fetch_add(1, Ordering::Relaxed);
                let reason = format!(
                    "shard {}: gateway event processor stopped unexpectedly",
                    self.shard_id
                );
                if self.fault.raise(reason.clone()) {
                    error!("{reason}");
                }
            }
        }
    }
}

/// A spawned pipeline task that is aborted when dropped.
///
/// Cancelling the shard future (for example `start()` inside `select!`)
/// must not leave pipeline tasks running detached. The handle is taken once
/// the task has been joined, so a join interrupted by a timeout can resume
/// later without polling a finished `JoinHandle`.
pub(super) struct AbortOnDrop<T>(Option<JoinHandle<T>>);

impl<T> AbortOnDrop<T> {
    pub(super) fn new(handle: JoinHandle<T>) -> Self {
        Self(Some(handle))
    }

    /// Waits for the task; `None` once it was already joined.
    pub(super) async fn join(&mut self) -> Option<Result<T, JoinError>> {
        let handle = self.0.as_mut()?;
        let result = handle.await;
        self.0 = None;
        Some(result)
    }

    pub(super) fn abort(&self) {
        if let Some(handle) = &self.0 {
            handle.abort();
        }
    }

    /// Hands the task over without aborting it.
    #[cfg(all(feature = "cache", feature = "sharding"))]
    pub(super) fn detach(mut self) -> Option<JoinHandle<T>> {
        self.0.take()
    }
}

impl<T> Drop for AbortOnDrop<T> {
    fn drop(&mut self) {
        self.abort();
    }
}

/// A processed event on its way to a handler call.
pub(super) struct HandlerDelivery {
    pub(super) event: Event,
    pub(super) received_at: Instant,
    /// Hub position just after this event, captured when it was published
    /// and before it was queued, so collectors created by its handler
    /// replay everything published while the handler waited.
    #[cfg(feature = "collectors")]
    pub(super) cursor: crate::collector::EventCursor,
}

impl HandlerDelivery {
    /// The context handed to this delivery's handler call.
    fn context(&self, base: &Context) -> Context {
        let mut ctx = base.clone();
        ctx.event_received_at = Some(self.received_at);
        #[cfg(feature = "collectors")]
        {
            ctx.event_cursor = Some(self.cursor);
        }
        ctx
    }
}

/// A running per-shard event pipeline.
pub(super) struct EventPipeline {
    ingress: Arc<EventIngress>,
    fault: PipelineFault,
    stop: Arc<watch::Sender<bool>>,
    processor: AbortOnDrop<()>,
    events: AbortOnDrop<()>,
    interactions: AbortOnDrop<()>,
    /// The thread running the overload hook; it exits once the processor
    /// and interaction dispatcher dropped their queue handles.
    overload_worker: Option<OverloadHookWorker>,
    #[cfg(test)]
    pub(super) handler_drops: Arc<OverflowLog>,
    /// Largest number of handler tasks a lane held at once.
    #[cfg(test)]
    pub(super) handler_high_water: Arc<std::sync::atomic::AtomicUsize>,
}

impl EventPipeline {
    pub(super) fn ingress(&self) -> Arc<EventIngress> {
        Arc::clone(&self.ingress)
    }

    pub(super) fn fault(&self) -> PipelineFault {
        self.fault.clone()
    }

    /// Drains and stops the pipeline once the gateway has stopped.
    ///
    /// Callers must drop every other [`EventIngress`] reference first (the
    /// gateway callback), so the processor sees its queue close after the
    /// queued events. The dispatchers then deliver what is left and wait
    /// for running handlers. After `timeout`, the dispatchers abort and join
    /// their handler tasks (`JoinSet::shutdown`) and the processor is
    /// aborted; everything is then joined for up to
    /// [`ABORT_CONFIRM_GRACE`]. Returns an error if a task panicked or did
    /// not confirm cancellation, so callers never claim a quiescent shard
    /// they could not verify.
    pub(super) async fn shutdown(self, timeout: Duration) -> Result<(), String> {
        let EventPipeline {
            ingress,
            fault,
            stop,
            mut processor,
            mut events,
            mut interactions,
            overload_worker,
            ..
        } = self;
        fault.disarm();
        drop(ingress);

        let drained = tokio::time::timeout(timeout, async {
            [
                processor.join().await,
                events.join().await,
                interactions.join().await,
            ]
        })
        .await;
        let results = match drained {
            Ok(results) => results,
            Err(_) => {
                warn!(
                    "event handlers still running {timeout:?} after the gateway stopped; \
                     aborting them"
                );
                stop.send_replace(true);
                processor.abort();
                let confirmed = tokio::time::timeout(ABORT_CONFIRM_GRACE, async {
                    [
                        processor.join().await,
                        events.join().await,
                        interactions.join().await,
                    ]
                })
                .await;
                match confirmed {
                    Ok(results) => results,
                    Err(_) => {
                        return Err(format!(
                            "event pipeline tasks did not confirm cancellation within \
                             {ABORT_CONFIRM_GRACE:?} after the {timeout:?} handler shutdown \
                             timeout"
                        ))
                    }
                }
            }
        };
        for result in results.into_iter().flatten() {
            if let Err(error) = result {
                if error.is_panic() {
                    return Err(format!("event pipeline task panicked: {error}"));
                }
            }
        }
        // The hook thread cannot be aborted (it runs application code); it
        // ends once the queue it drains is closed, which happened when the
        // tasks above finished.
        if let Some(overload_worker) = overload_worker {
            if tokio::time::timeout(ABORT_CONFIRM_GRACE, overload_worker.finished)
                .await
                .is_err()
            {
                return Err(format!(
                    "the interaction overload hook did not return within \
                     {ABORT_CONFIRM_GRACE:?} after the pipeline stopped"
                ));
            }
        }
        Ok(())
    }
}

/// Per-shard settings of the interaction lane.
#[derive(Clone)]
struct InteractionLane {
    max_concurrent: usize,
    admission_timeout: Duration,
    overload: Option<OverloadHookQueue>,
}

pub(super) fn spawn_event_pipeline(
    handler: Arc<dyn ErasedEventHandler>,
    ctx: Context,
    #[cfg(feature = "cache")] cache_backend: Option<CacheBackendForwarder>,
    dispatch: DispatchConfig,
) -> EventPipeline {
    let fault = PipelineFault::new();
    let (stop, _) = watch::channel(false);
    let stop = Arc::new(stop);
    let (ingress_tx, ingress_rx) = mpsc::channel(dispatch.queue_capacity.max(1));
    let (events_tx, events_rx) = mpsc::channel(dispatch.queue_capacity.max(1));
    let (interactions_tx, interactions_rx) =
        mpsc::channel(dispatch.interaction_queue_capacity.max(1));
    let handler_drops = Arc::new(OverflowLog::default());
    let (overload, overload_worker) =
        spawn_overload_hook(dispatch.interaction_overload.clone(), ctx.shard_id);
    let lane = InteractionLane {
        max_concurrent: dispatch.max_concurrent_interactions.max(1),
        admission_timeout: dispatch.interaction_admission_timeout,
        overload,
    };
    #[cfg(test)]
    let handler_high_water = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let ingress = Arc::new(EventIngress {
        shard_id: ctx.shard_id,
        events: ingress_tx,
        fault: fault.clone(),
        dropped: AtomicU64::new(0),
    });
    let processor = tokio::spawn(run_event_processor(
        ctx.clone(),
        #[cfg(feature = "cache")]
        cache_backend,
        ingress_rx,
        ProcessorOutputs {
            events: events_tx,
            interactions: interactions_tx,
            handler_drops: Arc::clone(&handler_drops),
            overload: lane.overload.clone(),
        },
        dispatch.backlog_warn_threshold(),
        fault.clone(),
    ));
    let events = tokio::spawn(run_handler_dispatcher(
        Arc::clone(&handler),
        ctx.clone(),
        events_rx,
        dispatch.handler_permits(),
        stop.subscribe(),
        fault.clone(),
        #[cfg(test)]
        Arc::clone(&handler_high_water),
    ));
    let interactions = tokio::spawn(run_interaction_dispatcher(
        handler,
        ctx,
        interactions_rx,
        lane,
        stop.subscribe(),
        fault.clone(),
        #[cfg(test)]
        Arc::clone(&handler_high_water),
    ));
    #[cfg(not(test))]
    drop(handler_drops);
    EventPipeline {
        ingress,
        fault,
        stop,
        processor: AbortOnDrop::new(processor),
        events: AbortOnDrop::new(events),
        interactions: AbortOnDrop::new(interactions),
        overload_worker,
        #[cfg(test)]
        handler_drops,
        #[cfg(test)]
        handler_high_water,
    }
}

struct ProcessorOutputs {
    events: mpsc::Sender<HandlerDelivery>,
    interactions: mpsc::Sender<HandlerDelivery>,
    handler_drops: Arc<OverflowLog>,
    overload: Option<OverloadHookQueue>,
}

/// Internal stage: applies every event to runtime state in gateway order
/// and hands it to a handler lane without waiting for handlers.
async fn run_event_processor(
    ctx: Context,
    #[cfg(feature = "cache")] cache_backend: Option<CacheBackendForwarder>,
    mut ingress: mpsc::Receiver<GatewayDispatch>,
    outputs: ProcessorOutputs,
    warn_at: usize,
    fault: PipelineFault,
) {
    let mut exit = FaultOnExit::new(
        fault.clone(),
        format!(
            "shard {}: gateway event processor stopped unexpectedly",
            ctx.shard_id
        ),
    );
    let mut ingress_backlog = BacklogWarning::default();
    let mut handler_backlog = BacklogWarning::default();
    while let Some(next) = ingress.recv().await {
        let backlog = ingress.len();
        if backlog >= warn_at && ingress_backlog.due() {
            warn!(
                "shard {}: gateway event backlog is {backlog} events; internal event processing \
                 (cache, voice, collectors) is not keeping up and the shard stops if the queue \
                 fills",
                ctx.shard_id
            );
        }

        let delivery = process_gateway_dispatch(
            &ctx,
            #[cfg(feature = "cache")]
            cache_backend.as_ref(),
            next,
        )
        .await;

        if matches!(delivery.event, Event::InteractionCreate(_)) {
            match outputs.interactions.try_send(delivery) {
                Ok(()) => {}
                Err(mpsc::error::TrySendError::Full(delivery)) => report_overloaded_interaction(
                    outputs.overload.as_ref(),
                    &ctx,
                    delivery,
                    InteractionOverloadReason::QueueFull,
                ),
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    fault.raise(format!(
                        "shard {}: interaction dispatcher stopped unexpectedly",
                        ctx.shard_id
                    ));
                }
            }
            continue;
        }

        match outputs.events.try_send(delivery) {
            Ok(()) => {
                let queued = outputs.events.max_capacity() - outputs.events.capacity();
                if queued >= warn_at && handler_backlog.due() {
                    warn!(
                        "shard {}: event handler backlog is {queued} events; the event handler is \
                         not keeping up (consider EventDispatchMode::Concurrent or faster \
                         handlers). Deliveries beyond {} queued events are dropped",
                        ctx.shard_id,
                        outputs.events.max_capacity()
                    );
                }
            }
            Err(mpsc::error::TrySendError::Full(delivery)) => {
                if let Some(total) = outputs.handler_drops.record() {
                    warn!(
                        "shard {}: event handler queue is full ({} events); dropped {total} \
                         handler call(s) so far, latest {}. The cache, collectors, and \
                         fetch_members still received these events",
                        ctx.shard_id,
                        outputs.events.max_capacity(),
                        delivery.event.kind()
                    );
                }
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                fault.raise(format!(
                    "shard {}: event handler dispatcher stopped unexpectedly",
                    ctx.shard_id
                ));
            }
        }
    }
    exit.complete();
}

async fn stop_requested(stop: &mut watch::Receiver<bool>) {
    if stop.wait_for(|stop| *stop).await.is_err() {
        // The pipeline owns the sender; without it the task is being
        // dropped anyway.
        std::future::pending::<()>().await;
    }
}

/// Lets running handlers finish once the queue closed, or aborts and joins
/// them when shutdown escalates.
async fn finish_running(
    shard_id: u32,
    running: &mut JoinSet<()>,
    stop: &mut watch::Receiver<bool>,
) {
    loop {
        tokio::select! {
            biased;
            _ = stop_requested(stop) => {
                running.shutdown().await;
                return;
            }
            outcome = running.join_next() => match outcome {
                Some(outcome) => report_handler_outcome(shard_id, outcome),
                None => return,
            },
        }
    }
}

/// Regular lane: runs handler calls for non-interaction events with at most
/// `permits` in flight (one in Serial mode, so calls start in gateway order
/// only after the previous one finished).
async fn run_handler_dispatcher(
    handler: Arc<dyn ErasedEventHandler>,
    ctx: Context,
    mut deliveries: mpsc::Receiver<HandlerDelivery>,
    permits: usize,
    mut stop: watch::Receiver<bool>,
    fault: PipelineFault,
    #[cfg(test)] high_water: Arc<std::sync::atomic::AtomicUsize>,
) {
    let mut exit = FaultOnExit::new(
        fault,
        format!(
            "shard {}: event handler dispatcher stopped unexpectedly",
            ctx.shard_id
        ),
    );
    let permits = Arc::new(Semaphore::new(permits.max(1)));
    let mut running = JoinSet::new();
    loop {
        // Reap finished calls before every admission. A permit is released
        // when a call finishes, so with a permanently backlogged queue the
        // receive below is always ready; reaping only inside that select
        // would never run and finished tasks would pile up in the JoinSet.
        reap_finished(ctx.shard_id, &mut running);
        // Take the permit before dequeuing, so an event waiting for a slot
        // stays in (and is bounded by) the queue.
        let permit = tokio::select! {
            biased;
            _ = stop_requested(&mut stop) => break,
            permit = Arc::clone(&permits).acquire_owned() => match permit {
                Ok(permit) => permit,
                Err(_) => break,
            },
        };
        let next = loop {
            tokio::select! {
                biased;
                _ = stop_requested(&mut stop) => break None,
                Some(outcome) = running.join_next(), if !running.is_empty() => {
                    report_handler_outcome(ctx.shard_id, outcome);
                }
                next = deliveries.recv() => break next,
            }
        };
        let Some(delivery) = next else {
            break;
        };
        let handler = Arc::clone(&handler);
        let handler_ctx = delivery.context(&ctx);
        running.spawn(async move {
            let _permit = permit;
            handler
                .handle_event_boxed(handler_ctx, delivery.event)
                .await;
        });
        #[cfg(test)]
        high_water.fetch_max(running.len(), Ordering::Relaxed);
    }
    finish_running(ctx.shard_id, &mut running, &mut stop).await;
    exit.complete();
}

/// Joins every handler task that already finished, without waiting.
fn reap_finished(shard_id: u32, running: &mut JoinSet<()>) {
    while let Some(outcome) = running.try_join_next() {
        report_handler_outcome(shard_id, outcome);
    }
}

/// Interaction lane: INTERACTION_CREATE events bypass the regular lane and
/// get their own execution slots, so a slow regular handler (or a full
/// regular queue) cannot make an interaction miss Discord's three-second
/// response window.
///
/// Interactions are admitted in arrival order. Each waits for a slot only
/// until `received_at + admission_timeout`; one that cannot be admitted by
/// then is handed to the overload hook instead of the handler. Admitted
/// interactions run concurrently with each other (up to the lane's limit)
/// and with regular handler calls, even in Serial mode.
async fn run_interaction_dispatcher(
    handler: Arc<dyn ErasedEventHandler>,
    ctx: Context,
    mut deliveries: mpsc::Receiver<HandlerDelivery>,
    lane: InteractionLane,
    mut stop: watch::Receiver<bool>,
    fault: PipelineFault,
    #[cfg(test)] high_water: Arc<std::sync::atomic::AtomicUsize>,
) {
    let mut exit = FaultOnExit::new(
        fault,
        format!(
            "shard {}: interaction dispatcher stopped unexpectedly",
            ctx.shard_id
        ),
    );
    let permits = Arc::new(Semaphore::new(lane.max_concurrent));
    let mut running = JoinSet::new();
    'deliveries: loop {
        reap_finished(ctx.shard_id, &mut running);
        let next = loop {
            tokio::select! {
                biased;
                _ = stop_requested(&mut stop) => break None,
                Some(outcome) = running.join_next(), if !running.is_empty() => {
                    report_handler_outcome(ctx.shard_id, outcome);
                }
                next = deliveries.recv() => break next,
            }
        };
        let Some(delivery) = next else {
            break;
        };
        let deadline = delivery.received_at.checked_add(lane.admission_timeout);
        let expired = |now: Instant| deadline.is_some_and(|deadline| now >= deadline);
        let mut permit = None;
        if !expired(Instant::now()) {
            // Expiry outranks a free slot: when both become ready in the same
            // poll (the dispatcher was not polled at the deadline), the
            // interaction is already late and goes to the overload hook.
            permit = loop {
                tokio::select! {
                    biased;
                    _ = stop_requested(&mut stop) => break 'deliveries,
                    _ = sleep_until_opt(deadline) => break None,
                    Some(outcome) = running.join_next(), if !running.is_empty() => {
                        report_handler_outcome(ctx.shard_id, outcome);
                    }
                    permit = Arc::clone(&permits).acquire_owned() => break permit.ok(),
                }
            };
        }
        // Re-check right before dispatch: never start a handler for an
        // interaction whose admission deadline has passed.
        if expired(Instant::now()) {
            permit = None;
        }
        match permit {
            Some(permit) => {
                let handler = Arc::clone(&handler);
                let handler_ctx = delivery.context(&ctx);
                running.spawn(async move {
                    let _permit = permit;
                    handler
                        .handle_event_boxed(handler_ctx, delivery.event)
                        .await;
                });
                #[cfg(test)]
                high_water.fetch_max(running.len(), Ordering::Relaxed);
            }
            None => report_overloaded_interaction(
                lane.overload.as_ref(),
                &ctx,
                delivery,
                InteractionOverloadReason::AdmissionTimeout,
            ),
        }
    }
    finish_running(ctx.shard_id, &mut running, &mut stop).await;
    exit.complete();
}

async fn sleep_until_opt(deadline: Option<Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

/// Capacity of the queue in front of the overload hook worker.
const OVERLOAD_HOOK_QUEUE_CAPACITY: usize = 64;

/// The bounded queue in front of the thread that runs the application's
/// overload hook.
///
/// The hook is application code; running it on the event processor (or the
/// interaction dispatcher) would let a slow or blocking hook stall cache
/// updates, member-chunk correlation, collectors, and deliveries — and
/// eventually overflow the shard's ingress. A dedicated OS thread (with the
/// runtime entered, so the hook can `tokio::spawn` its reply) isolates it
/// even from the runtime's worker threads; a blocking-pool task was not used
/// because a permanently running one stops paused-time test clocks from
/// advancing. When the hook falls more than [`OVERLOAD_HOOK_QUEUE_CAPACITY`]
/// interactions behind, further overloaded interactions are dropped and
/// counted in a rate-limited warning.
#[derive(Clone)]
struct OverloadHookQueue {
    jobs: mpsc::Sender<OverloadedInteraction>,
    dropped: Arc<OverflowLog>,
}

/// Completion signal of the overload hook thread, so shutdown can await it
/// with a bound (an OS thread cannot be aborted).
pub(super) struct OverloadHookWorker {
    finished: tokio::sync::oneshot::Receiver<()>,
}

/// Sends the completion signal when the hook thread exits, however it exits.
struct SignalOnExit(Option<tokio::sync::oneshot::Sender<()>>);

impl Drop for SignalOnExit {
    fn drop(&mut self) {
        if let Some(finished) = self.0.take() {
            // Nobody waiting for the hook thread is not an error.
            finished.send(()).ok();
        }
    }
}

/// Starts the overload hook thread, if a hook is configured. If the thread
/// cannot be spawned, overloaded interactions are only logged.
fn spawn_overload_hook(
    hook: Option<InteractionOverloadHook>,
    shard_id: u32,
) -> (Option<OverloadHookQueue>, Option<OverloadHookWorker>) {
    let Some(hook) = hook else {
        return (None, None);
    };
    let (jobs, mut queued) = mpsc::channel::<OverloadedInteraction>(OVERLOAD_HOOK_QUEUE_CAPACITY);
    let (finished_tx, finished) = tokio::sync::oneshot::channel();
    let Ok(runtime) = tokio::runtime::Handle::try_current() else {
        error!("shard {shard_id}: the interaction overload hook needs a Tokio runtime");
        return (None, None);
    };
    let spawned = std::thread::Builder::new()
        .name(format!("discordrs-overload-hook-{shard_id}"))
        .spawn(move || {
            let _finished = SignalOnExit(Some(finished_tx));
            let _runtime = runtime.enter();
            while let Some(overloaded) = queued.blocking_recv() {
                if std::panic::catch_unwind(AssertUnwindSafe(|| hook(overloaded))).is_err() {
                    error!("shard {shard_id}: the interaction overload hook panicked");
                }
            }
        });
    match spawned {
        Ok(_) => (
            Some(OverloadHookQueue {
                jobs,
                dropped: Arc::new(OverflowLog::default()),
            }),
            Some(OverloadHookWorker { finished }),
        ),
        Err(error) => {
            error!(
                "shard {shard_id}: could not start the interaction overload hook thread: {error}"
            );
            (None, None)
        }
    }
}

/// Hands an interaction that could not be admitted to the overload hook
/// worker, or logs it when no hook is configured. Never waits.
fn report_overloaded_interaction(
    hook: Option<&OverloadHookQueue>,
    base: &Context,
    delivery: HandlerDelivery,
    reason: InteractionOverloadReason,
) {
    let ctx = delivery.context(base);
    let waited = delivery.received_at.elapsed();
    let Event::InteractionCreate(event) = delivery.event else {
        return;
    };
    let Some(hook) = hook else {
        warn!(
            "shard {}: an interaction was not handed to the event handler ({reason:?} after \
             {waited:?}); set ClientBuilder::on_interaction_overload to respond to it",
            base.shard_id
        );
        return;
    };
    let overloaded = OverloadedInteraction {
        ctx,
        interaction: event.interaction,
        reason,
        waited,
    };
    match hook.jobs.try_send(overloaded) {
        Ok(()) => {}
        Err(mpsc::error::TrySendError::Full(_)) => {
            if let Some(total) = hook.dropped.record() {
                warn!(
                    "shard {}: the interaction overload hook is {OVERLOAD_HOOK_QUEUE_CAPACITY} \
                     interactions behind; dropped {total} overloaded interaction(s) so far",
                    base.shard_id
                );
            }
        }
        Err(mpsc::error::TrySendError::Closed(_)) => warn!(
            "shard {}: the interaction overload hook worker stopped; an overloaded \
             interaction was dropped",
            base.shard_id
        ),
    }
}

fn report_handler_outcome(shard_id: u32, outcome: Result<(), JoinError>) {
    if let Err(error) = outcome {
        if error.is_panic() {
            error!("shard {shard_id}: event handler panicked: {error}");
        }
    }
}

/// Runs the ordered internal processing for one dispatch and returns it as
/// a handler delivery.
pub(super) async fn process_gateway_dispatch(
    ctx: &Context,
    #[cfg(feature = "cache")] cache_backend: Option<&CacheBackendForwarder>,
    dispatch: GatewayDispatch,
) -> HandlerDelivery {
    let GatewayDispatch {
        event_name,
        data,
        received_at,
    } = dispatch;

    if event_name == "READY" && ctx.http.application_id() == 0 {
        if let Some(app_id) = data
            .pointer("/application/id")
            .and_then(|value| value.as_str())
            .and_then(|value| value.parse::<u64>().ok())
        {
            ctx.http.set_application_id(app_id);
            info!("Set application_id from READY: {app_id}");
        }
    }

    let event = match decode_event(&event_name, data.clone()) {
        Ok(event) => event,
        Err(error) => {
            warn!("Failed to decode event {event_name}: {error}");
            Event::Unknown {
                kind: event_name,
                raw: data,
            }
        }
    };

    apply_cache_updates(&ctx.cache, ctx.shard_pair(), &event).await;
    #[cfg(feature = "cache")]
    if let Some(cache_backend) = cache_backend {
        forward_cache_backend_updates(cache_backend, &ctx.cache, ctx.shard_pair(), &event).await;
    }
    #[cfg(feature = "voice")]
    apply_voice_updates(&ctx.voice, &event).await;
    route_member_chunk(&ctx.member_chunk_waiters, &event);
    #[cfg(feature = "collectors")]
    let cursor = ctx.collectors.publish(event.clone());
    HandlerDelivery {
        event,
        received_at,
        #[cfg(feature = "collectors")]
        cursor,
    }
}

/// Hands a GUILD_MEMBERS_CHUNK to the [`Context::fetch_members`] call
/// awaiting its nonce, if any.
fn route_member_chunk(waiters: &MemberChunkWaiters, event: &Event) {
    let Event::GuildMembersChunk(chunk) = event else {
        return;
    };
    let Some(nonce) = chunk.data.nonce.as_ref() else {
        return;
    };
    if let Some(waiter) = lock_member_chunk_waiters(waiters).get(nonce) {
        // Fails only if `fetch_members` already gave up; its guard then
        // removes the waiter.
        waiter.chunks.send(chunk.data.clone()).ok();
    }
}
