use std::collections::VecDeque;
use std::time::Duration;

use futures_util::{Sink, SinkExt};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio::time::{sleep, Instant};
use tokio_tungstenite::tungstenite::{
    protocol::CloseFrame, Error as WsError, Message as WsMessage,
};

use super::client::{
    request_channel_info_payload, request_guild_members_payload, update_presence_payload,
    GatewayCommand,
};
use crate::error::DiscordError;
use crate::model::UpdatePresence;

pub(super) const PRESENCE_UPDATE_LIMIT: usize = 5;
pub(super) const PRESENCE_UPDATE_WINDOW: Duration = Duration::from_secs(60);
pub(super) const GATEWAY_COMMAND_MIN_SPACING: Duration = Duration::from_millis(250);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GatewayCommandClass {
    PresenceUpdate,
    BurstSensitive,
    Other,
}

#[derive(Default)]
pub(super) struct GatewayOutboundLimiter {
    presence_updates: VecDeque<Instant>,
    next_burst_sensitive_at: Option<Instant>,
}

impl GatewayOutboundLimiter {
    pub(super) fn reserve_delay(
        &mut self,
        command: &GatewayCommand,
        now: Instant,
    ) -> Option<Duration> {
        match classify_gateway_command(command) {
            GatewayCommandClass::PresenceUpdate => self.reserve_presence_update(now),
            GatewayCommandClass::BurstSensitive => self.reserve_burst_sensitive(now),
            GatewayCommandClass::Other => None,
        }
    }

    fn reserve_presence_update(&mut self, now: Instant) -> Option<Duration> {
        while self.presence_updates.front().is_some_and(|sent_at| {
            now.saturating_duration_since(*sent_at) >= PRESENCE_UPDATE_WINDOW
        }) {
            self.presence_updates.pop_front();
        }

        if self.presence_updates.len() >= PRESENCE_UPDATE_LIMIT {
            let next_allowed = self.presence_updates[0] + PRESENCE_UPDATE_WINDOW;
            return Some(next_allowed.saturating_duration_since(now));
        }

        self.presence_updates.push_back(now);
        None
    }

    fn reserve_burst_sensitive(&mut self, now: Instant) -> Option<Duration> {
        if let Some(next_allowed) = self.next_burst_sensitive_at {
            if next_allowed > now {
                return Some(next_allowed.saturating_duration_since(now));
            }
        }

        self.next_burst_sensitive_at = Some(now + GATEWAY_COMMAND_MIN_SPACING);
        None
    }
}

pub(super) fn classify_gateway_command(command: &GatewayCommand) -> GatewayCommandClass {
    match command {
        GatewayCommand::UpdatePresence(_) | GatewayCommand::UpdatePresenceData(_) => {
            GatewayCommandClass::PresenceUpdate
        }
        GatewayCommand::RequestGuildMembers(_) | GatewayCommand::RequestChannelInfo(_) => {
            GatewayCommandClass::BurstSensitive
        }
        GatewayCommand::SendPayload(payload) => match payload.get("op").and_then(Value::as_u64) {
            Some(3) => GatewayCommandClass::PresenceUpdate,
            Some(4 | 8 | 14 | 31 | 43) => GatewayCommandClass::BurstSensitive,
            _ => GatewayCommandClass::Other,
        },
        GatewayCommand::Shutdown | GatewayCommand::Reconnect => GatewayCommandClass::Other,
    }
}

#[derive(Debug)]
pub(super) enum GatewayOutboundMessage {
    Limited(GatewayCommand),
    ImmediatePayload(Value),
    ImmediateText(String),
    Close(Option<CloseFrame>),
}

pub(super) fn send_gateway_outbound(
    outbound_tx: &mpsc::UnboundedSender<GatewayOutboundMessage>,
    message: GatewayOutboundMessage,
) -> Result<(), DiscordError> {
    outbound_tx
        .send(message)
        .map_err(|_| "gateway outbound worker stopped".into())
}

pub(super) async fn run_gateway_outbound_worker<S>(
    mut write: S,
    mut outbound_rx: mpsc::UnboundedReceiver<GatewayOutboundMessage>,
) -> Result<(), DiscordError>
where
    S: Sink<WsMessage, Error = WsError> + Unpin,
{
    let mut limiter = GatewayOutboundLimiter::default();
    let mut pending = VecDeque::new();

    loop {
        let command = match pending.pop_front() {
            Some(command) => command,
            None => match outbound_rx.recv().await {
                Some(GatewayOutboundMessage::Limited(command)) => command,
                Some(message) => {
                    if send_immediate_gateway_message(&mut write, message).await? {
                        return Ok(());
                    }
                    continue;
                }
                None => return Ok(()),
            },
        };

        while let Some(delay) = limiter.reserve_delay(&command, Instant::now()) {
            let delay = sleep(delay);
            tokio::pin!(delay);
            loop {
                tokio::select! {
                    _ = &mut delay => break,
                    message = outbound_rx.recv() => {
                        match message {
                            Some(GatewayOutboundMessage::Limited(command)) => pending.push_back(command),
                            Some(message) => {
                                if send_immediate_gateway_message(&mut write, message).await? {
                                    return Ok(());
                                }
                            }
                            None => return Ok(()),
                        }
                    }
                }
            }
        }

        send_limited_gateway_command(&mut write, command).await?;
    }
}

async fn send_immediate_gateway_message<S>(
    write: &mut S,
    message: GatewayOutboundMessage,
) -> Result<bool, DiscordError>
where
    S: Sink<WsMessage, Error = WsError> + Unpin,
{
    match message {
        GatewayOutboundMessage::Limited(command) => {
            send_limited_gateway_command(write, command).await?;
            Ok(false)
        }
        GatewayOutboundMessage::ImmediatePayload(payload) => {
            write
                .send(WsMessage::Text(payload.to_string().into()))
                .await?;
            Ok(false)
        }
        GatewayOutboundMessage::ImmediateText(text) => {
            write.send(WsMessage::Text(text.into())).await?;
            Ok(false)
        }
        GatewayOutboundMessage::Close(frame) => {
            let _ = write.send(WsMessage::Close(frame)).await;
            Ok(true)
        }
    }
}

async fn send_limited_gateway_command<S>(
    write: &mut S,
    command: GatewayCommand,
) -> Result<(), DiscordError>
where
    S: Sink<WsMessage, Error = WsError> + Unpin,
{
    let payload = match command {
        GatewayCommand::UpdatePresence(status) => {
            update_presence_payload(UpdatePresence::online_with_activity(status))
        }
        GatewayCommand::UpdatePresenceData(presence) => update_presence_payload(presence),
        GatewayCommand::RequestGuildMembers(request) => request_guild_members_payload(request),
        GatewayCommand::RequestChannelInfo(request) => request_channel_info_payload(request),
        GatewayCommand::SendPayload(payload) => payload,
        GatewayCommand::Shutdown | GatewayCommand::Reconnect => return Ok(()),
    };
    write
        .send(WsMessage::Text(payload.to_string().into()))
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::pin::Pin;
    use std::task::{Context, Poll};

    use futures_util::Sink;
    use tokio::sync::mpsc;
    use tokio_tungstenite::tungstenite::Error as WsError;

    use super::*;

    struct RecordingSink(mpsc::UnboundedSender<WsMessage>);

    impl Sink<WsMessage> for RecordingSink {
        type Error = WsError;

        fn poll_ready(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn start_send(self: Pin<&mut Self>, item: WsMessage) -> Result<(), Self::Error> {
            self.0.send(item).map_err(|_| WsError::ConnectionClosed)
        }

        fn poll_flush(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }

        fn poll_close(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Result<(), Self::Error>> {
            Poll::Ready(Ok(()))
        }
    }

    #[test]
    fn limiter_and_classifier_cover_other_and_expired_presence_paths() {
        let mut limiter = GatewayOutboundLimiter::default();
        let now = Instant::now();

        assert_eq!(
            classify_gateway_command(&GatewayCommand::Shutdown),
            GatewayCommandClass::Other
        );
        assert_eq!(limiter.reserve_delay(&GatewayCommand::Shutdown, now), None);

        for offset in 0..PRESENCE_UPDATE_LIMIT {
            assert_eq!(
                limiter.reserve_delay(
                    &GatewayCommand::UpdatePresence(format!("presence-{offset}")),
                    now + PRESENCE_UPDATE_WINDOW + Duration::from_millis(offset as u64)
                ),
                None
            );
        }

        assert_eq!(
            limiter.reserve_delay(
                &GatewayCommand::UpdatePresence("after-window".to_string()),
                now + PRESENCE_UPDATE_WINDOW * 2
            ),
            None
        );
    }

    #[test]
    fn send_gateway_outbound_reports_closed_worker() {
        let (tx, rx) = mpsc::unbounded_channel();
        drop(rx);

        let error = send_gateway_outbound(
            &tx,
            GatewayOutboundMessage::ImmediateText("hello".to_string()),
        )
        .unwrap_err();

        assert!(error
            .to_string()
            .contains("gateway outbound worker stopped"));
    }

    #[tokio::test]
    async fn outbound_worker_sends_immediate_and_limited_variants_then_closes() {
        let (outbound_tx, outbound_rx) = mpsc::unbounded_channel();
        let (sent_tx, mut sent_rx) = mpsc::unbounded_channel();

        let worker = tokio::spawn(run_gateway_outbound_worker(
            RecordingSink(sent_tx),
            outbound_rx,
        ));

        outbound_tx
            .send(GatewayOutboundMessage::ImmediateText(
                "raw text".to_string(),
            ))
            .unwrap();
        outbound_tx
            .send(GatewayOutboundMessage::Limited(
                GatewayCommand::SendPayload(serde_json::json!({ "op": 0, "d": { "kind": "raw" } })),
            ))
            .unwrap();
        outbound_tx
            .send(GatewayOutboundMessage::Limited(
                GatewayCommand::UpdatePresenceData(UpdatePresence::online_with_activity("typed")),
            ))
            .unwrap();
        outbound_tx
            .send(GatewayOutboundMessage::Close(None))
            .unwrap();

        assert_eq!(
            sent_rx.recv().await.unwrap().into_text().unwrap(),
            "raw text"
        );

        let raw_payload: Value =
            serde_json::from_str(&sent_rx.recv().await.unwrap().into_text().unwrap()).unwrap();
        assert_eq!(raw_payload["d"]["kind"], serde_json::json!("raw"));

        let presence_payload: Value =
            serde_json::from_str(&sent_rx.recv().await.unwrap().into_text().unwrap()).unwrap();
        assert_eq!(presence_payload["op"], serde_json::json!(3));
        assert_eq!(presence_payload["d"]["activities"][0]["name"], "typed");

        assert!(matches!(
            sent_rx.recv().await.unwrap(),
            WsMessage::Close(None)
        ));

        worker.await.unwrap().unwrap();
    }
}
