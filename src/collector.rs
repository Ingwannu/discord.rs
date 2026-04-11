#[cfg(feature = "collectors")]
use std::sync::Arc;
#[cfg(feature = "collectors")]
use std::time::Duration;

#[cfg(feature = "collectors")]
use tokio::sync::broadcast;
#[cfg(feature = "collectors")]
use tokio::time;

#[cfg(feature = "collectors")]
use crate::event::Event;
#[cfg(feature = "collectors")]
use crate::model::{ComponentInteraction, Interaction, Message, ModalSubmitInteraction};

#[cfg(feature = "collectors")]
type EventFilter<T> = Arc<dyn Fn(&T) -> bool + Send + Sync>;

#[cfg(feature = "collectors")]
#[derive(Clone)]
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
    pub fn new() -> Self {
        Self::with_capacity(256)
    }

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
pub struct MessageCollector {
    receiver: broadcast::Receiver<Event>,
    filter: Option<EventFilter<Message>>,
    timeout: Option<Duration>,
    max_items: Option<usize>,
    lagged_events: u64,
}

#[cfg(feature = "collectors")]
impl MessageCollector {
    fn new(receiver: broadcast::Receiver<Event>) -> Self {
        Self {
            receiver,
            filter: None,
            timeout: None,
            max_items: None,
            lagged_events: 0,
        }
    }

    pub fn filter<F>(mut self, filter: F) -> Self
    where
        F: Fn(&Message) -> bool + Send + Sync + 'static,
    {
        self.filter = Some(Arc::new(filter));
        self
    }

    pub fn timeout(mut self, duration: Duration) -> Self {
        self.timeout = Some(duration);
        self
    }

    pub fn max_items(mut self, max_items: usize) -> Self {
        self.max_items = Some(max_items);
        self
    }

    pub fn lagged_events(&self) -> u64 {
        self.lagged_events
    }

    pub async fn next(&mut self) -> Option<Message> {
        recv_with_timeout(self.timeout, async {
            loop {
                match self.receiver.recv().await {
                    Ok(Event::MessageCreate(event)) | Ok(Event::MessageUpdate(event)) => {
                        let passes = self
                            .filter
                            .as_ref()
                            .map(|filter| filter(&event.message))
                            .unwrap_or(true);
                        if passes {
                            return Some(event.message);
                        }
                    }
                    Ok(_) => {}
                    Err(broadcast::error::RecvError::Closed) => return None,
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        self.lagged_events = self.lagged_events.saturating_add(skipped);
                    }
                }
            }
        })
        .await
    }

    pub async fn collect(mut self) -> Vec<Message> {
        let mut messages = Vec::new();
        while let Some(message) = self.next().await {
            messages.push(message);
            if let Some(max_items) = self.max_items {
                if messages.len() >= max_items {
                    break;
                }
            }
        }
        messages
    }
}

#[cfg(feature = "collectors")]
pub struct InteractionCollector {
    receiver: broadcast::Receiver<Event>,
    filter: Option<EventFilter<Interaction>>,
    timeout: Option<Duration>,
    max_items: Option<usize>,
    lagged_events: u64,
}

#[cfg(feature = "collectors")]
impl InteractionCollector {
    fn new(receiver: broadcast::Receiver<Event>) -> Self {
        Self {
            receiver,
            filter: None,
            timeout: None,
            max_items: None,
            lagged_events: 0,
        }
    }

    pub fn filter<F>(mut self, filter: F) -> Self
    where
        F: Fn(&Interaction) -> bool + Send + Sync + 'static,
    {
        self.filter = Some(Arc::new(filter));
        self
    }

    pub fn timeout(mut self, duration: Duration) -> Self {
        self.timeout = Some(duration);
        self
    }

    pub fn max_items(mut self, max_items: usize) -> Self {
        self.max_items = Some(max_items);
        self
    }

    pub fn lagged_events(&self) -> u64 {
        self.lagged_events
    }

    pub async fn next(&mut self) -> Option<Interaction> {
        recv_with_timeout(self.timeout, async {
            loop {
                match self.receiver.recv().await {
                    Ok(Event::InteractionCreate(event)) => {
                        let passes = self
                            .filter
                            .as_ref()
                            .map(|filter| filter(&event.interaction))
                            .unwrap_or(true);
                        if passes {
                            return Some(event.interaction);
                        }
                    }
                    Ok(_) => {}
                    Err(broadcast::error::RecvError::Closed) => return None,
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        self.lagged_events = self.lagged_events.saturating_add(skipped);
                    }
                }
            }
        })
        .await
    }
}

#[cfg(feature = "collectors")]
pub struct ComponentCollector {
    inner: InteractionCollector,
}

#[cfg(feature = "collectors")]
impl ComponentCollector {
    fn new(receiver: broadcast::Receiver<Event>) -> Self {
        Self {
            inner: InteractionCollector::new(receiver),
        }
    }

    pub fn timeout(mut self, duration: Duration) -> Self {
        self.inner = self.inner.timeout(duration);
        self
    }

    pub fn lagged_events(&self) -> u64 {
        self.inner.lagged_events()
    }

    pub async fn next(&mut self) -> Option<ComponentInteraction> {
        while let Some(interaction) = self.inner.next().await {
            if let Interaction::Component(component) = interaction {
                return Some(component);
            }
        }
        None
    }
}

#[cfg(feature = "collectors")]
pub struct ModalCollector {
    inner: InteractionCollector,
}

#[cfg(feature = "collectors")]
impl ModalCollector {
    fn new(receiver: broadcast::Receiver<Event>) -> Self {
        Self {
            inner: InteractionCollector::new(receiver),
        }
    }

    pub fn timeout(mut self, duration: Duration) -> Self {
        self.inner = self.inner.timeout(duration);
        self
    }

    pub fn lagged_events(&self) -> u64 {
        self.inner.lagged_events()
    }

    pub async fn next(&mut self) -> Option<ModalSubmitInteraction> {
        while let Some(interaction) = self.inner.next().await {
            if let Interaction::ModalSubmit(modal) = interaction {
                return Some(modal);
            }
        }
        None
    }
}

#[cfg(feature = "collectors")]
async fn recv_with_timeout<T>(
    timeout_duration: Option<Duration>,
    future: impl std::future::Future<Output = Option<T>>,
) -> Option<T> {
    match timeout_duration {
        Some(duration) => time::timeout(duration, future).await.ok().flatten(),
        None => future.await,
    }
}

#[cfg(all(test, feature = "collectors"))]
mod tests {
    use std::time::Duration;

    use serde_json::json;

    use crate::event::decode_event;

    use super::CollectorHub;

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
}
