#[cfg(feature = "collectors")]
use std::sync::Arc;
#[cfg(feature = "collectors")]
use std::time::Duration;

#[cfg(feature = "collectors")]
use tokio::sync::{broadcast, watch};
#[cfg(feature = "collectors")]
use tokio::time;

#[cfg(feature = "collectors")]
use crate::event::Event;
#[cfg(feature = "collectors")]
use crate::model::{ComponentInteraction, Interaction, Message, ModalSubmitInteraction};

#[cfg(feature = "collectors")]
type EventFilter<T> = Arc<dyn Fn(&T) -> bool + Send + Sync>;

#[cfg(feature = "collectors")]
/// Default reason recorded when a collector is stopped without an explicit
/// reason, mirroring discord.js's `collector.stop()` default of `"user"`.
const DEFAULT_STOP_REASON: &str = "user";

#[cfg(feature = "collectors")]
#[derive(Debug, Clone, PartialEq, Eq)]
/// Why a collector ended collection, mirroring discord.js's `Collector#endReason`.
pub enum CollectorEndReason {
    /// The configured `max_items` limit was reached (discord.js `"limit"`).
    Limit,
    /// The overall `timeout` window elapsed (discord.js `"time"`).
    Time,
    /// No matching item arrived within the `idle` window (discord.js `"idle"`).
    Idle,
    /// Collection was stopped explicitly via `stop`/`stop_with_reason`,
    /// carrying the supplied reason (`"user"` by default).
    User(String),
    /// The underlying event channel closed because the [`CollectorHub`]
    /// (and every other sender) was dropped.
    ChannelDropped,
}

#[cfg(feature = "collectors")]
#[derive(Clone)]
/// Cloneable handle that stops a collector from another task, ending any
/// in-flight `next()` call with `None`.
///
/// Obtain one with the collector's `stop_handle()` method before moving the
/// collector elsewhere (for example into a spawned task).
pub struct CollectorStopHandle {
    stop_tx: Arc<watch::Sender<Option<String>>>,
}

#[cfg(feature = "collectors")]
impl CollectorStopHandle {
    /// Stops the associated collector with the default reason `"user"`.
    pub fn stop(&self) {
        self.stop_with_reason(DEFAULT_STOP_REASON);
    }

    /// Stops the associated collector, recording `reason` as
    /// [`CollectorEndReason::User`]. The first stop reason wins; later calls
    /// are ignored.
    pub fn stop_with_reason(&self, reason: impl Into<String>) {
        request_stop(&self.stop_tx, reason.into());
    }
}

#[cfg(feature = "collectors")]
fn request_stop(stop_tx: &watch::Sender<Option<String>>, reason: String) {
    stop_tx.send_if_modified(|current| {
        if current.is_none() {
            *current = Some(reason);
            true
        } else {
            false
        }
    });
}

#[cfg(feature = "collectors")]
#[derive(Clone)]
/// Typed Discord API object for `CollectorHub`.
pub struct CollectorHub {
    sender: broadcast::Sender<Event>,
}

#[cfg(feature = "collectors")]
impl Default for CollectorHub {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "collectors")]
impl CollectorHub {
    /// Creates a `new` value.
    pub fn new() -> Self {
        Self::with_capacity(256)
    }

    /// Creates a `with_capacity` value.
    pub fn with_capacity(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity.max(1));
        Self { sender }
    }

    pub(crate) fn publish(&self, event: Event) {
        let _ = self.sender.send(event);
    }

    pub fn message_collector(&self) -> MessageCollector {
        MessageCollector::new(self.sender.subscribe())
    }

    pub fn interaction_collector(&self) -> InteractionCollector {
        InteractionCollector::new(self.sender.subscribe())
    }

    pub fn component_collector(&self) -> ComponentCollector {
        ComponentCollector::new(self.sender.subscribe())
    }

    pub fn modal_collector(&self) -> ModalCollector {
        ModalCollector::new(self.sender.subscribe())
    }
}

#[cfg(feature = "collectors")]
/// Outcome of racing the receive loop against the stop signal and timers.
enum NextOutcome<T> {
    Item(T),
    Closed,
    Stopped(String),
    TimedOut,
    Idled,
}

#[cfg(feature = "collectors")]
/// Shared implementation behind every collector type.
struct CollectorCore<T> {
    receiver: broadcast::Receiver<Event>,
    extract: fn(Event) -> Option<T>,
    filter: Option<EventFilter<T>>,
    timeout: Option<Duration>,
    idle: Option<Duration>,
    deadline: Option<time::Instant>,
    idle_deadline: Option<time::Instant>,
    max_items: Option<usize>,
    received: usize,
    lagged_events: u64,
    end_reason: Option<CollectorEndReason>,
    stop_tx: Arc<watch::Sender<Option<String>>>,
    stop_rx: watch::Receiver<Option<String>>,
}

#[cfg(feature = "collectors")]
impl<T> CollectorCore<T> {
    fn new(receiver: broadcast::Receiver<Event>, extract: fn(Event) -> Option<T>) -> Self {
        let (stop_tx, stop_rx) = watch::channel(None);
        Self {
            receiver,
            extract,
            filter: None,
            timeout: None,
            idle: None,
            deadline: None,
            idle_deadline: None,
            max_items: None,
            received: 0,
            lagged_events: 0,
            end_reason: None,
            stop_tx: Arc::new(stop_tx),
            stop_rx,
        }
    }

    fn stop(&self, reason: impl Into<String>) {
        request_stop(&self.stop_tx, reason.into());
    }

    fn stop_handle(&self) -> CollectorStopHandle {
        CollectorStopHandle {
            stop_tx: Arc::clone(&self.stop_tx),
        }
    }

    fn end_reason(&self) -> Option<CollectorEndReason> {
        if let Some(reason) = &self.end_reason {
            return Some(reason.clone());
        }
        self.stop_rx.borrow().clone().map(CollectorEndReason::User)
    }

    fn reset_timer(&mut self) {
        let now = time::Instant::now();
        if let Some(timeout) = self.timeout {
            self.deadline = Some(now + timeout);
        }
        if let Some(idle) = self.idle {
            self.idle_deadline = Some(now + idle);
        }
    }

    async fn next(&mut self) -> Option<T> {
        if self.end_reason.is_some() {
            return None;
        }
        if let Some(reason) = self.stop_rx.borrow().clone() {
            self.end_reason = Some(CollectorEndReason::User(reason));
            return None;
        }
        if let Some(max_items) = self.max_items {
            if self.received >= max_items {
                self.end_reason = Some(CollectorEndReason::Limit);
                return None;
            }
        }

        let now = time::Instant::now();
        if let Some(timeout) = self.timeout {
            self.deadline.get_or_insert(now + timeout);
        }
        if let Some(idle) = self.idle {
            self.idle_deadline.get_or_insert(now + idle);
        }
        let deadline = self.deadline;
        let idle_deadline = self.idle_deadline;

        let extract = self.extract;
        let filter = self.filter.clone();
        let receiver = &mut self.receiver;
        let lagged_events = &mut self.lagged_events;
        let stop_rx = &mut self.stop_rx;

        let outcome = tokio::select! {
            item = recv_matching(receiver, extract, filter, lagged_events) => match item {
                Some(item) => NextOutcome::Item(item),
                None => NextOutcome::Closed,
            },
            reason = wait_for_stop(stop_rx) => NextOutcome::Stopped(reason),
            _ = sleep_until_opt(deadline) => NextOutcome::TimedOut,
            _ = sleep_until_opt(idle_deadline) => NextOutcome::Idled,
        };

        match outcome {
            NextOutcome::Item(item) => {
                self.received = self.received.saturating_add(1);
                if let Some(max_items) = self.max_items {
                    if self.received >= max_items {
                        self.end_reason = Some(CollectorEndReason::Limit);
                    }
                }
                if let Some(idle) = self.idle {
                    self.idle_deadline = Some(time::Instant::now() + idle);
                }
                Some(item)
            }
            NextOutcome::Closed => {
                self.end_reason = Some(CollectorEndReason::ChannelDropped);
                None
            }
            NextOutcome::Stopped(reason) => {
                self.end_reason = Some(CollectorEndReason::User(reason));
                None
            }
            NextOutcome::TimedOut => {
                self.end_reason = Some(CollectorEndReason::Time);
                None
            }
            NextOutcome::Idled => {
                self.end_reason = Some(CollectorEndReason::Idle);
                None
            }
        }
    }

    async fn collect(&mut self) -> Vec<T> {
        let mut items = Vec::new();
        while let Some(item) = self.next().await {
            items.push(item);
        }
        items
    }
}

#[cfg(feature = "collectors")]
/// Receives events until one matches `extract` and `filter`, tracking lag.
/// Returns `None` when the broadcast channel closes.
async fn recv_matching<T>(
    receiver: &mut broadcast::Receiver<Event>,
    extract: fn(Event) -> Option<T>,
    filter: Option<EventFilter<T>>,
    lagged_events: &mut u64,
) -> Option<T> {
    loop {
        match receiver.recv().await {
            Ok(event) => {
                if let Some(item) = extract(event) {
                    let passes = filter.as_ref().map(|filter| filter(&item)).unwrap_or(true);
                    if passes {
                        return Some(item);
                    }
                }
            }
            Err(broadcast::error::RecvError::Closed) => return None,
            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                *lagged_events = lagged_events.saturating_add(skipped);
            }
        }
    }
}

#[cfg(feature = "collectors")]
/// Resolves with the stop reason once a stop is requested.
async fn wait_for_stop(stop_rx: &mut watch::Receiver<Option<String>>) -> String {
    loop {
        if let Some(reason) = stop_rx.borrow_and_update().clone() {
            return reason;
        }
        if stop_rx.changed().await.is_err() {
            // The paired sender lives inside the collector itself, so it can
            // never be dropped while this future is polled. Pend forever so
            // the other `select!` branches keep making progress regardless.
            std::future::pending::<()>().await;
        }
    }
}

#[cfg(feature = "collectors")]
/// Sleeps until `deadline`, or pends forever when no deadline is configured.
async fn sleep_until_opt(deadline: Option<time::Instant>) {
    match deadline {
        Some(deadline) => time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

#[cfg(feature = "collectors")]
macro_rules! impl_collector_methods {
    ($collector:ident, $item:ty) => {
        #[cfg(feature = "collectors")]
        impl $collector {
            /// Only collects items for which `filter` returns `true`.
            /// Non-matching items are skipped: they do not count towards
            /// `max_items` and do not reset the idle window.
            pub fn filter<F>(mut self, filter: F) -> Self
            where
                F: Fn(&$item) -> bool + Send + Sync + 'static,
            {
                self.core.filter = Some(Arc::new(filter));
                self
            }

            /// Sets the overall collection window. Once `duration` has
            /// elapsed since the first `next()` call (or the last
            /// [`reset_timer`](Self::reset_timer)), collection ends with
            /// [`CollectorEndReason::Time`].
            pub fn timeout(mut self, duration: Duration) -> Self {
                self.core.timeout = Some(duration);
                self
            }

            /// Sets the idle window. If no matching item arrives for
            /// `duration`, collection ends with [`CollectorEndReason::Idle`].
            /// The window restarts every time a matching item is collected,
            /// unlike [`timeout`](Self::timeout) which is absolute.
            pub fn idle(mut self, duration: Duration) -> Self {
                self.core.idle = Some(duration);
                self
            }

            /// Stops collecting once this many items have been collected,
            /// recording [`CollectorEndReason::Limit`].
            pub fn max_items(mut self, max_items: usize) -> Self {
                self.core.max_items = Some(max_items);
                self
            }

            /// Number of events dropped because this collector lagged behind
            /// the broadcast channel.
            pub fn lagged_events(&self) -> u64 {
                self.core.lagged_events
            }

            /// Number of matching items this collector has yielded so far.
            pub fn received_count(&self) -> usize {
                self.core.received
            }

            /// Why collection ended, or `None` while the collector is still
            /// live. Mirrors discord.js's `Collector#endReason`.
            pub fn end_reason(&self) -> Option<CollectorEndReason> {
                self.core.end_reason()
            }

            /// Stops collection with the default reason `"user"`. Subsequent
            /// `next()` calls return `None` and
            /// [`end_reason`](Self::end_reason) reports
            /// [`CollectorEndReason::User`].
            pub fn stop(&self) {
                self.core.stop(DEFAULT_STOP_REASON);
            }

            /// Stops collection with a custom reason, recorded as
            /// [`CollectorEndReason::User`]. The first stop reason wins.
            pub fn stop_with_reason(&self, reason: impl Into<String>) {
                self.core.stop(reason);
            }

            /// Returns a cloneable [`CollectorStopHandle`] that can stop this
            /// collector from another task, ending an in-flight `next()`
            /// call with `None`.
            pub fn stop_handle(&self) -> CollectorStopHandle {
                self.core.stop_handle()
            }

            /// Restarts both the overall timeout and the idle window from
            /// now, where configured. Equivalent to discord.js's
            /// `Collector#resetTimer`.
            pub fn reset_timer(&mut self) {
                self.core.reset_timer();
            }

            /// Waits for the next matching item. Returns `None` once
            /// collection has ended; consult
            /// [`end_reason`](Self::end_reason) to learn why.
            pub async fn next(&mut self) -> Option<$item> {
                self.core.next().await
            }

            /// Collects items until the collector ends (limit, timeout, idle,
            /// stop, or channel close) and returns everything gathered.
            pub async fn collect(mut self) -> Vec<$item> {
                self.core.collect().await
            }
        }
    };
}

#[cfg(feature = "collectors")]
fn extract_message(event: Event) -> Option<Message> {
    match event {
        Event::MessageCreate(event) | Event::MessageUpdate(event) => Some(event.message),
        _ => None,
    }
}

#[cfg(feature = "collectors")]
fn extract_interaction(event: Event) -> Option<Interaction> {
    match event {
        Event::InteractionCreate(event) => Some(event.interaction),
        _ => None,
    }
}

#[cfg(feature = "collectors")]
fn extract_component(event: Event) -> Option<ComponentInteraction> {
    match extract_interaction(event)? {
        Interaction::Component(component) => Some(component),
        _ => None,
    }
}

#[cfg(feature = "collectors")]
fn extract_modal(event: Event) -> Option<ModalSubmitInteraction> {
    match extract_interaction(event)? {
        Interaction::ModalSubmit(modal) => Some(modal),
        _ => None,
    }
}

#[cfg(feature = "collectors")]
/// Typed Discord API object for `MessageCollector`.
pub struct MessageCollector {
    core: CollectorCore<Message>,
}

#[cfg(feature = "collectors")]
impl MessageCollector {
    fn new(receiver: broadcast::Receiver<Event>) -> Self {
        Self {
            core: CollectorCore::new(receiver, extract_message),
        }
    }
}

#[cfg(feature = "collectors")]
impl_collector_methods!(MessageCollector, Message);

#[cfg(feature = "collectors")]
/// Typed Discord API object for `InteractionCollector`.
pub struct InteractionCollector {
    core: CollectorCore<Interaction>,
}

#[cfg(feature = "collectors")]
impl InteractionCollector {
    fn new(receiver: broadcast::Receiver<Event>) -> Self {
        Self {
            core: CollectorCore::new(receiver, extract_interaction),
        }
    }
}

#[cfg(feature = "collectors")]
impl_collector_methods!(InteractionCollector, Interaction);

#[cfg(feature = "collectors")]
/// Typed Discord API object for `ComponentCollector`.
pub struct ComponentCollector {
    core: CollectorCore<ComponentInteraction>,
}

#[cfg(feature = "collectors")]
impl ComponentCollector {
    fn new(receiver: broadcast::Receiver<Event>) -> Self {
        Self {
            core: CollectorCore::new(receiver, extract_component),
        }
    }
}

#[cfg(feature = "collectors")]
impl_collector_methods!(ComponentCollector, ComponentInteraction);

#[cfg(feature = "collectors")]
/// Typed Discord API object for `ModalCollector`.
pub struct ModalCollector {
    core: CollectorCore<ModalSubmitInteraction>,
}

#[cfg(feature = "collectors")]
impl ModalCollector {
    fn new(receiver: broadcast::Receiver<Event>) -> Self {
        Self {
            core: CollectorCore::new(receiver, extract_modal),
        }
    }
}

#[cfg(feature = "collectors")]
impl_collector_methods!(ModalCollector, ModalSubmitInteraction);

#[cfg(all(test, feature = "collectors"))]
mod tests {
    use std::time::Duration;

    use serde_json::json;
    use serde_json::Value;
    use tokio::time;

    use crate::event::decode_event;
    use crate::model::Interaction;

    use super::{CollectorEndReason, CollectorHub};

    fn interaction_event(payload: Value) -> crate::event::Event {
        decode_event("INTERACTION_CREATE", payload).expect("valid interaction event")
    }

    fn ping_interaction(id: &str) -> crate::event::Event {
        interaction_event(json!({
            "id": id,
            "application_id": "2",
            "token": "token",
            "type": 1
        }))
    }

    fn component_interaction(id: &str, custom_id: &str) -> crate::event::Event {
        interaction_event(json!({
            "id": id,
            "application_id": "2",
            "token": "token",
            "type": 3,
            "data": {
                "custom_id": custom_id,
                "component_type": 2,
                "values": ["one"]
            }
        }))
    }

    fn modal_interaction(id: &str, custom_id: &str) -> crate::event::Event {
        interaction_event(json!({
            "id": id,
            "application_id": "2",
            "token": "token",
            "type": 5,
            "data": {
                "custom_id": custom_id,
                "components": []
            }
        }))
    }

    fn message_create(id: &str, content: &str) -> crate::event::Event {
        decode_event(
            "MESSAGE_CREATE",
            json!({
                "id": id,
                "channel_id": "1",
                "content": content,
                "mentions": [],
                "attachments": []
            }),
        )
        .expect("valid message event")
    }

    #[tokio::test]
    async fn message_collector_stops_after_max_items() {
        let hub = CollectorHub::new();
        let collector = hub
            .message_collector()
            .max_items(1)
            .timeout(Duration::from_secs(1));

        hub.publish(
            decode_event(
                "MESSAGE_CREATE",
                json!({
                    "id": "2",
                    "channel_id": "1",
                    "content": "hello",
                    "mentions": [],
                    "attachments": []
                }),
            )
            .unwrap(),
        );

        let messages = collector.collect().await;
        assert_eq!(messages.len(), 1);
    }

    #[tokio::test]
    async fn message_collector_reports_lagged_events() {
        let hub = CollectorHub::with_capacity(1);
        let mut collector = hub.message_collector().timeout(Duration::from_secs(1));

        for id in ["2", "3", "4"] {
            hub.publish(
                decode_event(
                    "MESSAGE_CREATE",
                    json!({
                        "id": id,
                        "channel_id": "1",
                        "content": format!("message-{id}"),
                        "mentions": [],
                        "attachments": []
                    }),
                )
                .unwrap(),
            );
        }

        let message = collector
            .next()
            .await
            .expect("collector should still yield the newest buffered message");
        assert_eq!(message.id.as_str(), "4");
        assert!(collector.lagged_events() >= 1);
    }

    #[tokio::test]
    async fn default_hub_and_message_filter_cover_default_paths() {
        let hub = CollectorHub::default();
        let mut collector = hub
            .message_collector()
            .filter(|message| message.content == "keep")
            .timeout(Duration::from_secs(1));

        hub.publish(
            decode_event(
                "MESSAGE_CREATE",
                json!({
                    "id": "2",
                    "channel_id": "1",
                    "content": "skip",
                    "mentions": [],
                    "attachments": []
                }),
            )
            .unwrap(),
        );
        hub.publish(ping_interaction("3"));
        hub.publish(
            decode_event(
                "MESSAGE_UPDATE",
                json!({
                    "id": "4",
                    "channel_id": "1",
                    "content": "keep",
                    "mentions": [],
                    "attachments": []
                }),
            )
            .unwrap(),
        );

        let message = collector.next().await.expect("matching message update");
        assert_eq!(message.id.as_str(), "4");
        assert_eq!(message.content, "keep");
    }

    #[tokio::test]
    async fn interaction_component_and_modal_collectors_yield_typed_variants() {
        let hub = CollectorHub::new();
        let mut interaction_collector = hub
            .interaction_collector()
            .filter(|interaction| matches!(interaction, Interaction::Component(_)))
            .timeout(Duration::from_secs(1));
        let mut component_collector = hub.component_collector().timeout(Duration::from_secs(1));
        let mut modal_collector = hub.modal_collector().timeout(Duration::from_secs(1));

        hub.publish(
            decode_event(
                "INTERACTION_CREATE",
                json!({
                    "id": "1",
                    "application_id": "2",
                    "token": "token",
                    "type": 1
                }),
            )
            .unwrap(),
        );
        hub.publish(
            decode_event(
                "INTERACTION_CREATE",
                json!({
                    "id": "3",
                    "application_id": "4",
                    "token": "token",
                    "type": 3,
                    "data": {
                        "custom_id": "button",
                        "component_type": 2,
                        "values": ["one"]
                    }
                }),
            )
            .unwrap(),
        );
        hub.publish(
            decode_event(
                "INTERACTION_CREATE",
                json!({
                    "id": "5",
                    "application_id": "6",
                    "token": "token",
                    "type": 5,
                    "data": {
                        "custom_id": "modal",
                        "components": []
                    }
                }),
            )
            .unwrap(),
        );

        match interaction_collector.next().await {
            Some(Interaction::Component(component)) => {
                assert_eq!(component.data.custom_id, "button");
            }
            other => panic!("unexpected filtered interaction: {other:?}"),
        }

        let component = component_collector
            .next()
            .await
            .expect("component interaction");
        assert_eq!(component.context.id.as_str(), "3");
        assert_eq!(component.context.application_id.as_str(), "4");
        assert_eq!(component.context.token, "token");
        assert_eq!(component.data.custom_id, "button");
        assert_eq!(component.data.component_type, 2);
        assert_eq!(component.data.values, vec!["one".to_string()]);

        let modal = modal_collector.next().await.expect("modal interaction");
        assert_eq!(modal.context.id.as_str(), "5");
        assert_eq!(modal.submission.custom_id, "modal");
    }

    #[tokio::test]
    async fn interaction_collector_filter_skips_non_matches_and_max_items_chain_is_usable() {
        let hub = CollectorHub::new();
        let mut collector = hub
            .interaction_collector()
            .max_items(1)
            .filter(|interaction| matches!(interaction, Interaction::Component(_)))
            .timeout(Duration::from_secs(1));

        hub.publish(ping_interaction("1"));
        hub.publish(component_interaction("2", "keep-me"));

        match collector.next().await {
            Some(Interaction::Component(component)) => {
                assert_eq!(component.context.id.as_str(), "2");
                assert_eq!(component.data.custom_id, "keep-me");
            }
            other => panic!("unexpected interaction after filter skip: {other:?}"),
        }
    }

    #[tokio::test]
    async fn interaction_collector_collect_stops_at_max_items() {
        let hub = CollectorHub::new();
        let collector = hub
            .interaction_collector()
            .max_items(2)
            .timeout(Duration::from_secs(1));

        hub.publish(component_interaction("1", "first"));
        hub.publish(component_interaction("2", "second"));
        hub.publish(component_interaction("3", "third"));

        let interactions = collector.collect().await;
        assert_eq!(interactions.len(), 2);
        assert_eq!(interactions[0].id().as_str(), "1");
        assert_eq!(interactions[1].id().as_str(), "2");
    }

    #[tokio::test]
    async fn component_and_modal_collectors_collect_and_expose_lagged_counts() {
        let component_hub = CollectorHub::new();
        let component_collector = component_hub
            .component_collector()
            .max_items(2)
            .timeout(Duration::from_secs(1));
        component_hub.publish(component_interaction("1", "first"));
        component_hub.publish(component_interaction("2", "second"));
        component_hub.publish(component_interaction("3", "third"));

        let components = component_collector.collect().await;
        assert_eq!(components.len(), 2);
        assert_eq!(components[0].data.custom_id, "first");
        assert_eq!(components[1].data.custom_id, "second");

        let modal_hub = CollectorHub::with_capacity(1);
        let mut lagged_modal_collector = modal_hub
            .modal_collector()
            .max_items(1)
            .timeout(Duration::from_secs(1));
        modal_hub.publish(component_interaction("4", "skipped"));
        modal_hub.publish(modal_interaction("5", "latest"));
        let modal = lagged_modal_collector.next().await.expect("latest modal");
        assert_eq!(modal.submission.custom_id, "latest");
        assert!(lagged_modal_collector.lagged_events() >= 1);

        let modal_collector = modal_hub
            .modal_collector()
            .max_items(1)
            .timeout(Duration::from_secs(1));
        modal_hub.publish(modal_interaction("6", "collected"));
        let modals = modal_collector.collect().await;
        assert_eq!(modals.len(), 1);
        assert_eq!(modals[0].submission.custom_id, "collected");
    }

    #[tokio::test]
    async fn component_collector_timeout_is_absolute_across_non_matching_interactions() {
        let hub = CollectorHub::new();
        let mut collector = hub.component_collector().timeout(Duration::from_millis(30));
        let publisher = hub.clone();

        tokio::spawn(async move {
            for id in 0..20 {
                publisher.publish(ping_interaction(&id.to_string()));
                time::sleep(Duration::from_millis(10)).await;
            }
        });

        let result = time::timeout(Duration::from_millis(120), collector.next())
            .await
            .expect("collector timeout should not reset for every non-component interaction");
        assert!(result.is_none());
        assert_eq!(collector.end_reason(), Some(CollectorEndReason::Time));
    }

    #[tokio::test]
    async fn interaction_collector_reports_lagged_interactions() {
        let hub = CollectorHub::with_capacity(1);
        let mut collector = hub.interaction_collector().timeout(Duration::from_secs(1));

        hub.publish(ping_interaction("1"));
        hub.publish(component_interaction("2", "older"));
        hub.publish(modal_interaction("3", "latest"));

        let interaction = collector
            .next()
            .await
            .expect("collector should yield the newest buffered interaction");
        match interaction {
            Interaction::ModalSubmit(modal) => {
                assert_eq!(modal.context.id.as_str(), "3");
                assert_eq!(modal.submission.custom_id, "latest");
            }
            other => panic!("unexpected interaction after lag: {other:?}"),
        }
        assert!(collector.lagged_events() >= 1);
    }

    #[tokio::test]
    async fn component_collector_returns_none_after_only_non_component_events() {
        let hub = CollectorHub::new();
        let mut collector = hub.component_collector().timeout(Duration::from_secs(1));

        hub.publish(ping_interaction("1"));
        hub.publish(modal_interaction("2", "not-a-component"));
        drop(hub);

        assert!(collector.next().await.is_none());
        assert_eq!(
            collector.end_reason(),
            Some(CollectorEndReason::ChannelDropped)
        );
    }

    #[tokio::test]
    async fn component_collector_times_out_when_no_component_arrives() {
        let hub = CollectorHub::new();
        let mut collector = hub.component_collector().timeout(Duration::from_millis(20));

        hub.publish(ping_interaction("1"));

        assert!(collector.next().await.is_none());
        assert_eq!(collector.lagged_events(), 0);
    }

    #[tokio::test]
    async fn modal_collector_returns_none_after_only_non_modal_events() {
        let hub = CollectorHub::new();
        let mut collector = hub.modal_collector().timeout(Duration::from_secs(1));

        hub.publish(ping_interaction("1"));
        hub.publish(component_interaction("2", "button"));
        drop(hub);

        assert!(collector.next().await.is_none());
    }

    #[tokio::test]
    async fn modal_collector_times_out_when_no_modal_arrives() {
        let hub = CollectorHub::new();
        let mut collector = hub.modal_collector().timeout(Duration::from_millis(20));

        hub.publish(component_interaction("1", "button"));

        assert!(collector.next().await.is_none());
        assert_eq!(collector.lagged_events(), 0);
    }

    #[tokio::test]
    async fn idle_timeout_ends_collection_before_slower_overall_timeout() {
        let hub = CollectorHub::new();
        let mut collector = hub
            .message_collector()
            .timeout(Duration::from_secs(5))
            .idle(Duration::from_millis(40));

        let started = time::Instant::now();
        assert!(collector.next().await.is_none());
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "idle should end collection long before the overall timeout"
        );
        assert_eq!(collector.end_reason(), Some(CollectorEndReason::Idle));
    }

    #[tokio::test]
    async fn idle_window_resets_on_each_collected_item() {
        let hub = CollectorHub::new();
        let mut collector = hub.message_collector().idle(Duration::from_millis(100));
        let publisher = hub.clone();

        tokio::spawn(async move {
            for id in 10..13 {
                time::sleep(Duration::from_millis(30)).await;
                publisher.publish(message_create(&id.to_string(), "tick"));
            }
        });

        let mut count = 0;
        while collector.next().await.is_some() {
            count += 1;
        }
        assert_eq!(count, 3);
        assert_eq!(collector.received_count(), 3);
        assert_eq!(collector.end_reason(), Some(CollectorEndReason::Idle));
    }

    #[tokio::test]
    async fn overall_timeout_records_time_end_reason() {
        let hub = CollectorHub::new();
        let mut collector = hub
            .interaction_collector()
            .timeout(Duration::from_millis(30));

        assert!(collector.next().await.is_none());
        assert_eq!(collector.end_reason(), Some(CollectorEndReason::Time));
    }

    #[tokio::test]
    async fn stop_with_reason_ends_in_flight_next_and_records_reason() {
        let hub = CollectorHub::new();
        let mut collector = hub.component_collector();
        let handle = collector.stop_handle();

        tokio::spawn(async move {
            time::sleep(Duration::from_millis(20)).await;
            handle.stop_with_reason("done");
        });

        let result = time::timeout(Duration::from_millis(500), collector.next())
            .await
            .expect("stop_with_reason should end the in-flight next()");
        assert!(result.is_none());
        assert_eq!(
            collector.end_reason(),
            Some(CollectorEndReason::User("done".to_string()))
        );
    }

    #[tokio::test]
    async fn stop_makes_subsequent_next_return_none_with_default_reason() {
        let hub = CollectorHub::new();
        let mut collector = hub.message_collector();

        hub.publish(message_create("2", "ignored after stop"));
        collector.stop();

        assert!(collector.next().await.is_none());
        assert!(collector.next().await.is_none());
        assert_eq!(collector.received_count(), 0);
        assert_eq!(
            collector.end_reason(),
            Some(CollectorEndReason::User("user".to_string()))
        );
    }

    #[tokio::test]
    async fn first_stop_reason_wins() {
        let hub = CollectorHub::new();
        let mut collector = hub.modal_collector();

        collector.stop_with_reason("first");
        collector.stop_with_reason("second");
        collector.stop();

        assert!(collector.next().await.is_none());
        assert_eq!(
            collector.end_reason(),
            Some(CollectorEndReason::User("first".to_string()))
        );
    }

    #[tokio::test]
    async fn component_collector_filter_only_passes_matching_custom_ids() {
        let hub = CollectorHub::new();
        let mut collector = hub
            .component_collector()
            .filter(|component| component.data.custom_id == "keep")
            .timeout(Duration::from_secs(1));

        hub.publish(component_interaction("1", "drop"));
        hub.publish(ping_interaction("2"));
        hub.publish(component_interaction("3", "keep"));

        let component = collector.next().await.expect("matching component");
        assert_eq!(component.context.id.as_str(), "3");
        assert_eq!(component.data.custom_id, "keep");
        assert_eq!(collector.received_count(), 1);
    }

    #[tokio::test]
    async fn modal_collector_filter_only_passes_matching_custom_ids() {
        let hub = CollectorHub::new();
        let mut collector = hub
            .modal_collector()
            .filter(|modal| modal.submission.custom_id == "wanted")
            .timeout(Duration::from_secs(1));

        hub.publish(modal_interaction("1", "other"));
        hub.publish(modal_interaction("2", "wanted"));

        let modal = collector.next().await.expect("matching modal");
        assert_eq!(modal.context.id.as_str(), "2");
        assert_eq!(modal.submission.custom_id, "wanted");
        assert_eq!(collector.received_count(), 1);
    }

    #[tokio::test]
    async fn reset_timer_extends_idle_deadline() {
        let hub = CollectorHub::new();
        let mut collector = hub.message_collector().idle(Duration::from_millis(100));

        collector.reset_timer();
        time::sleep(Duration::from_millis(60)).await;
        collector.reset_timer();

        let started = time::Instant::now();
        assert!(collector.next().await.is_none());
        let elapsed = started.elapsed();
        assert!(
            elapsed >= Duration::from_millis(80),
            "reset_timer should have restarted the idle window, elapsed {elapsed:?}"
        );
        assert_eq!(collector.end_reason(), Some(CollectorEndReason::Idle));
    }

    #[tokio::test]
    async fn max_items_records_limit_end_reason_and_ends_next() {
        let hub = CollectorHub::new();
        let mut collector = hub
            .message_collector()
            .max_items(1)
            .timeout(Duration::from_secs(1));

        hub.publish(message_create("2", "hello"));
        hub.publish(message_create("3", "beyond the limit"));

        assert!(collector.next().await.is_some());
        assert_eq!(collector.received_count(), 1);
        assert_eq!(collector.end_reason(), Some(CollectorEndReason::Limit));
        assert!(collector.next().await.is_none());
        assert_eq!(collector.received_count(), 1);
    }

    #[tokio::test]
    async fn end_reason_is_none_while_collector_is_live() {
        let hub = CollectorHub::new();
        let mut collector = hub.message_collector().timeout(Duration::from_secs(1));

        hub.publish(message_create("2", "hello"));

        assert!(collector.next().await.is_some());
        assert_eq!(collector.end_reason(), None);
        assert_eq!(collector.received_count(), 1);
    }
}
