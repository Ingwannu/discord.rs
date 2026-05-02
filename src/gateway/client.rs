use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio::time::{sleep, timeout, Duration, Instant};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{
        protocol::{frame::coding::CloseCode, CloseFrame},
        Message as WsMessage,
    },
};
use tracing::{debug, error, info, warn};

use crate::model::{RequestChannelInfo, RequestGuildMembers, Snowflake, UpdatePresence};
#[cfg(feature = "sharding")]
use crate::sharding::{ShardRuntimeState, ShardSupervisorEvent};
use crate::ws::GatewayConnectionConfig;

use super::compression::{decode_gateway_message, GatewayCompressionDecoder};
use super::outbound::{run_gateway_outbound_worker, send_gateway_outbound, GatewayOutboundMessage};

// Gateway opcodes
const OP_DISPATCH: u64 = 0;
const OP_HEARTBEAT: u64 = 1;
const OP_IDENTIFY: u64 = 2;
const OP_RESUME: u64 = 6;
const OP_RECONNECT: u64 = 7;
const OP_INVALID_SESSION: u64 = 9;
const OP_HELLO: u64 = 10;
const OP_HEARTBEAT_ACK: u64 = 11;
pub(super) const GATEWAY_COMMAND_QUEUE_CAPACITY: usize = 256;
static JITTER_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(crate) struct GatewayClient {
    token: String,
    intents: u64,
    session_id: Option<String>,
    resume_gateway_url: Option<String>,
    gateway_config: GatewayConnectionConfig,
    shard_info: Option<[u32; 2]>,
    command_rx: Option<mpsc::Receiver<GatewayCommand>>,
    deferred_commands: VecDeque<GatewayCommand>,
    #[cfg(feature = "sharding")]
    supervisor_callback: Option<SupervisorCallback>,
    sequence: Arc<AtomicU64>,
    heartbeat_ack_received: Arc<AtomicBool>,
}

// Callback type for dispatching events
pub(crate) type EventCallback = Arc<dyn Fn(String, Value) + Send + Sync>;
#[cfg(feature = "sharding")]
pub(crate) type SupervisorCallback = Arc<dyn Fn(ShardSupervisorEvent) + Send + Sync>;

#[derive(Debug)]
pub(crate) enum GatewayCommand {
    Shutdown,
    Reconnect,
    UpdatePresence(String),
    UpdatePresenceData(UpdatePresence),
    RequestGuildMembers(RequestGuildMembers),
    RequestChannelInfo(RequestChannelInfo),
    SendPayload(Value),
}

impl GatewayClient {
    pub fn new(token: String, intents: u64) -> Self {
        Self {
            token,
            intents,
            session_id: None,
            resume_gateway_url: None,
            gateway_config: GatewayConnectionConfig::default(),
            shard_info: None,
            command_rx: None,
            deferred_commands: VecDeque::new(),
            #[cfg(feature = "sharding")]
            supervisor_callback: None,
            sequence: Arc::new(AtomicU64::new(0)),
            heartbeat_ack_received: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn gateway_config(mut self, gateway_config: GatewayConnectionConfig) -> Self {
        self.gateway_config = gateway_config;
        self
    }

    pub fn shard(mut self, shard_id: u32, total_shards: u32) -> Self {
        self.shard_info = Some([shard_id, total_shards.max(1)]);
        self.gateway_config = self
            .gateway_config
            .clone()
            .shard(shard_id, total_shards.max(1));
        self
    }

    pub fn control(mut self, command_rx: mpsc::Receiver<GatewayCommand>) -> Self {
        self.command_rx = Some(command_rx);
        self
    }

    #[cfg(feature = "sharding")]
    pub fn supervisor(mut self, supervisor_callback: SupervisorCallback) -> Self {
        self.supervisor_callback = Some(supervisor_callback);
        self
    }

    /// Run the gateway connection loop. Reconnects automatically.
    /// `on_event` is called for every DISPATCH event with (event_name, data).
    pub async fn run(&mut self, on_event: EventCallback) -> Result<(), crate::error::DiscordError> {
        let mut backoff = 1_u64;
        #[cfg(feature = "sharding")]
        self.publish_state(ShardRuntimeState::Starting);
        loop {
            let url = self
                .resume_gateway_url
                .clone()
                .unwrap_or_else(|| self.gateway_config.normalized_url());
            info!("Connecting to gateway: {url}");

            match self.connect_and_run(&url, on_event.clone()).await {
                Ok(action) => match action {
                    ReconnectAction::Resume => {
                        #[cfg(feature = "sharding")]
                        self.publish_state(ShardRuntimeState::Reconnecting);
                        info!("Resuming gateway session");
                        backoff = 1;
                    }
                    ReconnectAction::Reconnect => {
                        #[cfg(feature = "sharding")]
                        self.publish_state(ShardRuntimeState::Reconnecting);
                        info!("Reconnecting with fresh session");
                        self.session_id = None;
                        self.resume_gateway_url = None;
                        self.sequence.store(0, Ordering::Relaxed);
                        backoff = 1;
                    }
                    ReconnectAction::Shutdown => {
                        #[cfg(feature = "sharding")]
                        self.publish_state(ShardRuntimeState::Stopped);
                        return Ok(());
                    }
                },
                Err(e) => {
                    #[cfg(feature = "sharding")]
                    self.publish_error(e.to_string());
                    error!("Gateway connection error: {e}");
                    if self
                        .wait_for_backoff_command(Duration::from_secs(backoff.min(300)))
                        .await?
                    {
                        #[cfg(feature = "sharding")]
                        self.publish_state(ShardRuntimeState::Stopped);
                        return Ok(());
                    }
                    backoff = (backoff * 2).min(300);
                }
            }
        }
    }

    async fn connect_and_run(
        &mut self,
        url: &str,
        on_event: EventCallback,
    ) -> Result<ReconnectAction, crate::error::DiscordError> {
        let (ws_stream, _) = connect_async(url).await?;
        let (mut write, mut read) = ws_stream.split();
        let mut compression_decoder = GatewayCompressionDecoder::new(
            self.gateway_config.compression_kind(),
        )
        .map_err(|error| format!("failed to initialize gateway compression decoder: {error}"))?;

        // Wait for Hello
        let hello_text = loop {
            let hello = read.next().await.ok_or("gateway closed before Hello")??;
            match decode_gateway_message(hello, &mut compression_decoder) {
                Ok(Some(text)) => break text,
                Ok(None) => continue,
                Err(error) => {
                    return Err(format!("failed to decode gateway Hello payload: {error}").into());
                }
            }
        };
        let hello_payload: Value = serde_json::from_str(&hello_text)?;
        let hello_op = hello_payload["op"].as_u64().unwrap_or(u64::MAX);
        if hello_op != OP_HELLO {
            return Err(format!("expected Hello opcode {OP_HELLO}, got {hello_op}").into());
        }
        let heartbeat_interval_ms = hello_payload["d"]["heartbeat_interval"]
            .as_u64()
            .ok_or("missing heartbeat_interval in Hello")?;

        debug!("Received Hello, heartbeat_interval={heartbeat_interval_ms}ms");

        // Send Identify or Resume
        if let Some(ref session_id) = self.session_id {
            let seq = self.sequence.load(Ordering::Relaxed);
            let resume = resume_payload(&self.token, session_id, seq);
            write
                .send(WsMessage::Text(resume.to_string().into()))
                .await?;
            debug!("Sent Resume");
        } else {
            let identify = identify_payload(&self.token, self.intents, self.shard_info, false);
            write
                .send(WsMessage::Text(identify.to_string().into()))
                .await?;
            debug!("Sent Identify");
        }

        // Spawn heartbeat task
        let (heartbeat_tx, mut heartbeat_rx) = mpsc::channel::<String>(8);
        let sequence_clone = self.sequence.clone();
        let ack_received = self.heartbeat_ack_received.clone();
        ack_received.store(true, Ordering::Relaxed);

        let heartbeat_handle = tokio::spawn(async move {
            sleep(initial_heartbeat_delay(
                heartbeat_interval_ms,
                rand_jitter(),
            ))
            .await;

            loop {
                if !ack_received.load(Ordering::Relaxed) {
                    warn!("Heartbeat ACK not received - zombie connection");
                    let _ = heartbeat_tx.send("zombie".to_string()).await;
                    break;
                }

                ack_received.store(false, Ordering::Relaxed);
                let seq = sequence_clone.load(Ordering::Relaxed);
                let hb = serde_json::json!({
                    "op": OP_HEARTBEAT,
                    "d": if seq == 0 { Value::Null } else { Value::Number(seq.into()) }
                });
                if heartbeat_tx.send(hb.to_string()).await.is_err() {
                    break;
                }

                sleep(Duration::from_millis(heartbeat_interval_ms)).await;
            }
        });

        // Main read loop. The websocket sink lives in a worker so rate-limit
        // sleeps never block inbound frames or heartbeat ACK processing.
        let (outbound_tx, outbound_rx) = mpsc::unbounded_channel();
        let (outbound_error_tx, mut outbound_error_rx) = mpsc::unbounded_channel();
        let mut outbound_handle = tokio::spawn(async move {
            if let Err(error) = run_gateway_outbound_worker(write, outbound_rx).await {
                let _ = outbound_error_tx.send(error.to_string());
            }
        });

        let action = loop {
            tokio::select! {
                msg = read.next() => {
                    let text = match msg {
                        Some(Ok(WsMessage::Close(frame))) => {
                            #[cfg(feature = "sharding")]
                            self.publish_error(terminal_close_error(frame.clone()));
                            warn!("Gateway closed: {frame:?}");
                            if is_terminal_close_frame(frame.as_ref()) {
                                return Err(terminal_close_error(frame).into());
                            }
                            break ReconnectAction::Resume;
                        }
                        Some(Ok(message)) => {
                            match decode_gateway_message(message, &mut compression_decoder) {
                                Ok(Some(decompressed)) => decompressed,
                                Ok(None) => continue,
                                Err(e) => {
                                    warn!("Failed to decode compressed gateway payload: {e}");
                                    continue;
                                }
                            }
                        }
                        Some(Err(e)) => {
                            #[cfg(feature = "sharding")]
                            self.publish_error(e.to_string());
                            error!("Gateway read error: {e}");
                            break ReconnectAction::Resume;
                        }
                        None => {
                            #[cfg(feature = "sharding")]
                            self.publish_error("gateway stream ended".to_string());
                            warn!("Gateway stream ended");
                            break ReconnectAction::Resume;
                        }
                    };

                    let payload: Value = match serde_json::from_str(&text) {
                        Ok(v) => v,
                        Err(e) => {
                            warn!("Failed to parse gateway message: {e}");
                            continue;
                        }
                    };

                    let op = payload["op"].as_u64().unwrap_or(u64::MAX);

                    match op {
                        OP_DISPATCH => {
                            if let Some(s) = payload["s"].as_u64() {
                                self.sequence.store(s, Ordering::Relaxed);
                            }
                            let event_name = payload["t"].as_str().unwrap_or("").to_string();
                            let data = payload["d"].clone();

                            if event_name == "READY" {
                                if let Some(sid) = data["session_id"].as_str() {
                                    self.session_id = Some(sid.to_string());
                                }
                                if let Some(resume_url) = data["resume_gateway_url"].as_str() {
                                    self.resume_gateway_url = Some(
                                        self.gateway_config
                                            .clone()
                                            .with_base_url(resume_url)
                                            .normalized_url(),
                                    );
                                }
                                #[cfg(feature = "sharding")]
                                if let Some(session_id) = self.session_id.clone() {
                                    self.publish_supervisor(ShardSupervisorEvent::SessionEstablished {
                                        shard_id: self.current_shard_id(),
                                        session_id,
                                    });
                                    self.publish_state(ShardRuntimeState::Running);
                                }
                                info!("Received READY, session_id={}", self.session_id.as_deref().unwrap_or("?"));
                            }

                            on_event(event_name, data);
                        }
                        OP_HEARTBEAT => {
                            let seq = self.sequence.load(Ordering::Relaxed);
                            let hb = serde_json::json!({
                                "op": OP_HEARTBEAT,
                                "d": if seq == 0 { Value::Null } else { Value::Number(seq.into()) }
                            });
                            send_gateway_outbound(
                                &outbound_tx,
                                GatewayOutboundMessage::ImmediatePayload(hb),
                            )?;
                        }
                        OP_HEARTBEAT_ACK => {
                            self.heartbeat_ack_received.store(true, Ordering::Relaxed);
                            debug!("Heartbeat ACK received");
                        }
                        OP_RECONNECT => {
                            #[cfg(feature = "sharding")]
                            self.publish_state(ShardRuntimeState::Reconnecting);
                            info!("Received Reconnect opcode");
                            break ReconnectAction::Resume;
                        }
                        OP_INVALID_SESSION => {
                            let resumable = payload["d"].as_bool().unwrap_or(false);
                            #[cfg(feature = "sharding")]
                            self.publish_state(ShardRuntimeState::Reconnecting);
                            warn!("Invalid session, resumable={resumable}");
                            sleep(Duration::from_secs(2)).await;
                            if resumable {
                                break ReconnectAction::Resume;
                            } else {
                                break ReconnectAction::Reconnect;
                            }
                        }
                        _ => {
                            debug!("Unhandled gateway opcode: {op}");
                        }
                    }
                }
                Some(msg) = heartbeat_rx.recv() => {
                    if msg == "zombie" {
                        #[cfg(feature = "sharding")]
                        self.publish_error("heartbeat zombie connection detected".to_string());
                        warn!("Zombie connection detected, reconnecting");
                        break ReconnectAction::Resume;
                    }
                    send_gateway_outbound(
                        &outbound_tx,
                        GatewayOutboundMessage::ImmediateText(msg),
                    )?;
                    debug!("Sent heartbeat");
                }
                Some(error) = outbound_error_rx.recv() => {
                    #[cfg(feature = "sharding")]
                    self.publish_error(error.clone());
                    warn!("Gateway outbound worker failed: {error}");
                    break ReconnectAction::Resume;
                }
                command = recv_control_command(&mut self.command_rx, &mut self.deferred_commands) => {
                    match command {
                        Some(GatewayCommand::Shutdown) => {
                            let _ = outbound_tx.send(GatewayOutboundMessage::Close(Some(CloseFrame {
                                    code: CloseCode::Normal,
                                    reason: "supervisor shutdown".into(),
                                })));
                            break ReconnectAction::Shutdown;
                        }
                        Some(GatewayCommand::Reconnect) => break ReconnectAction::Resume,
                        Some(command) => send_gateway_outbound(
                            &outbound_tx,
                            GatewayOutboundMessage::Limited(command),
                        )?,
                        None => {}
                    }
                }
            }
        };

        heartbeat_handle.abort();
        if matches!(action, ReconnectAction::Shutdown) {
            drop(outbound_tx);
            if timeout(Duration::from_secs(1), &mut outbound_handle)
                .await
                .is_err()
            {
                warn!("Gateway outbound worker did not stop after shutdown close");
                outbound_handle.abort();
            }
        } else {
            outbound_handle.abort();
        }
        Ok(action)
    }
}

enum ReconnectAction {
    Resume,
    Reconnect,
    Shutdown,
}

async fn recv_control_command(
    command_rx: &mut Option<mpsc::Receiver<GatewayCommand>>,
    deferred_commands: &mut VecDeque<GatewayCommand>,
) -> Option<GatewayCommand> {
    if let Some(command) = deferred_commands.pop_front() {
        return Some(command);
    }

    match command_rx {
        Some(command_rx) => command_rx.recv().await,
        None => std::future::pending::<Option<GatewayCommand>>().await,
    }
}

pub(crate) fn voice_state_update_payload(
    guild_id: Snowflake,
    channel_id: Option<Snowflake>,
    self_mute: bool,
    self_deaf: bool,
) -> Value {
    serde_json::json!({
        "op": 4,
        "d": {
            "guild_id": guild_id,
            "channel_id": channel_id,
            "self_mute": self_mute,
            "self_deaf": self_deaf
        }
    })
}

pub(crate) fn update_presence_payload(presence: UpdatePresence) -> Value {
    serde_json::json!({
        "op": 3,
        "d": presence
    })
}

pub(crate) fn request_guild_members_payload(request: RequestGuildMembers) -> Value {
    serde_json::json!({
        "op": 8,
        "d": request
    })
}

pub(crate) fn request_soundboard_sounds_payload(
    guild_ids: Vec<Snowflake>,
    channels: Option<HashMap<Snowflake, Vec<Snowflake>>>,
) -> Value {
    serde_json::json!({
        "op": 31,
        "d": {
            "guild_ids": guild_ids,
            "channels": channels
        }
    })
}

pub(crate) fn request_channel_info_payload(request: RequestChannelInfo) -> Value {
    serde_json::json!({
        "op": 43,
        "d": request
    })
}

fn rand_jitter() -> f64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0);
    let counter = JITTER_COUNTER.fetch_add(0x9e37_79b9_7f4a_7c15, Ordering::Relaxed);
    let pid = u64::from(std::process::id());
    let mut mixed = nanos ^ counter.rotate_left(17) ^ pid.rotate_left(32);

    mixed ^= mixed >> 30;
    mixed = mixed.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    mixed ^= mixed >> 27;
    mixed = mixed.wrapping_mul(0x94d0_49bb_1331_11eb);
    mixed ^= mixed >> 31;

    ((mixed >> 11) as f64) / ((1_u64 << 53) as f64)
}

fn initial_heartbeat_delay(heartbeat_interval_ms: u64, jitter_factor: f64) -> Duration {
    let clamped = jitter_factor.clamp(0.0, 1.0);
    Duration::from_millis((heartbeat_interval_ms as f64 * clamped) as u64)
}

fn resume_payload(token: &str, session_id: &str, seq: u64) -> Value {
    serde_json::json!({
        "op": OP_RESUME,
        "d": {
            "token": token,
            "session_id": session_id,
            "seq": if seq == 0 { Value::Null } else { Value::Number(seq.into()) }
        }
    })
}

fn identify_payload(
    token: &str,
    intents: u64,
    shard_info: Option<[u32; 2]>,
    payload_compression: bool,
) -> Value {
    let mut identify = serde_json::json!({
        "op": OP_IDENTIFY,
        "d": {
            "token": token,
            "intents": intents,
            "properties": {
                "os": std::env::consts::OS,
                "browser": "discordrs",
                "device": "discordrs"
            }
        }
    });
    if payload_compression {
        identify["d"]["compress"] = serde_json::json!(true);
    }
    if let Some(shard_info) = shard_info {
        identify["d"]["shard"] = serde_json::json!(shard_info);
    }
    identify
}

fn is_terminal_close_frame(frame: Option<&CloseFrame>) -> bool {
    frame
        .map(|frame| is_terminal_close_code(u16::from(frame.code)))
        .unwrap_or(false)
}

fn is_terminal_close_code(code: u16) -> bool {
    matches!(code, 4004 | 4010 | 4011 | 4012 | 4013 | 4014)
}

fn terminal_close_error(frame: Option<CloseFrame>) -> String {
    match frame {
        Some(frame) => format!(
            "gateway closed with terminal close code {}: {}",
            u16::from(frame.code),
            frame.reason
        ),
        None => "gateway closed with terminal close code".to_string(),
    }
}

#[cfg(feature = "sharding")]
impl GatewayClient {
    fn current_shard_id(&self) -> u32 {
        self.shard_info.map(|pair| pair[0]).unwrap_or(0)
    }

    fn publish_state(&self, state: ShardRuntimeState) {
        self.publish_supervisor(ShardSupervisorEvent::StateChanged {
            shard_id: self.current_shard_id(),
            state,
        });
    }

    fn publish_error(&self, message: String) {
        self.publish_supervisor(ShardSupervisorEvent::GatewayError {
            shard_id: self.current_shard_id(),
            message,
        });
    }

    fn publish_supervisor(&self, event: ShardSupervisorEvent) {
        if let Some(callback) = &self.supervisor_callback {
            callback(event);
        }
    }
}

impl GatewayClient {
    async fn wait_for_backoff_command(
        &mut self,
        duration: Duration,
    ) -> Result<bool, crate::error::DiscordError> {
        let Some(command_rx) = self.command_rx.as_mut() else {
            sleep(duration).await;
            return Ok(false);
        };

        let deadline = Instant::now() + duration;
        loop {
            let now = Instant::now();
            if now >= deadline {
                return Ok(false);
            }

            match timeout(deadline - now, command_rx.recv()).await {
                Ok(Some(GatewayCommand::Shutdown)) => return Ok(true),
                Ok(Some(GatewayCommand::Reconnect)) => {
                    self.session_id = None;
                    self.resume_gateway_url = None;
                    self.sequence.store(0, Ordering::Relaxed);
                    return Ok(false);
                }
                Ok(Some(command)) => {
                    self.deferred_commands.push_back(command);
                }
                Ok(None) | Err(_) => return Ok(false),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::io::Write;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};
    use std::task::{Context, Poll};
    use std::time::Duration;

    use super::super::compression::{GatewayCompressionDecoder, GatewayZlibStream, ZLIB_SUFFIX};
    use super::super::outbound::{
        classify_gateway_command, run_gateway_outbound_worker, GatewayCommandClass,
        GatewayOutboundLimiter, GatewayOutboundMessage, GATEWAY_COMMAND_MIN_SPACING,
        PRESENCE_UPDATE_LIMIT, PRESENCE_UPDATE_WINDOW,
    };
    use super::{
        identify_payload, initial_heartbeat_delay, is_terminal_close_code, is_terminal_close_frame,
        rand_jitter, recv_control_command, request_channel_info_payload,
        request_guild_members_payload, resume_payload, terminal_close_error,
        update_presence_payload, voice_state_update_payload, EventCallback, GatewayClient,
        GatewayCommand, ReconnectAction, GATEWAY_COMMAND_QUEUE_CAPACITY,
    };
    use crate::model::{RequestChannelInfo, RequestGuildMembers, Snowflake, UpdatePresence};
    #[cfg(feature = "sharding")]
    use crate::sharding::{ShardRuntimeState, ShardSupervisorEvent};
    use crate::ws::{GatewayCompression, GatewayConnectionConfig};
    use flate2::{write::ZlibEncoder, Compression};
    use futures_util::Sink;
    use futures_util::{SinkExt, StreamExt};
    use tokio::net::TcpListener;
    use tokio::sync::mpsc;
    use tokio::time::Instant;
    use tokio_tungstenite::tungstenite::protocol::{frame::coding::CloseCode, CloseFrame};
    use tokio_tungstenite::tungstenite::Error as WsError;
    use tokio_tungstenite::{accept_async, tungstenite::Message as WsMessage};

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

    fn compress_gateway_payloads(payloads: &[&str]) -> Vec<Vec<u8>> {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        let mut emitted = 0;

        payloads
            .iter()
            .map(|payload| {
                encoder
                    .write_all(payload.as_bytes())
                    .expect("write gateway payload");
                encoder.flush().expect("flush gateway payload");
                let stream = encoder.get_ref();
                let chunk = stream[emitted..].to_vec();
                emitted = stream.len();
                chunk
            })
            .collect()
    }

    #[test]
    fn normalize_gateway_url_adds_missing_gateway_query() {
        assert_eq!(
            GatewayConnectionConfig::new("gateway.discord.gg").normalized_url(),
            "wss://gateway.discord.gg?v=10&encoding=json"
        );
    }

    #[test]
    fn normalize_gateway_url_preserves_existing_query_values() {
        assert_eq!(
            GatewayConnectionConfig::new("wss://gateway.discord.gg/?encoding=json")
                .normalized_url(),
            "wss://gateway.discord.gg/?encoding=json&v=10"
        );
    }

    #[test]
    fn terminal_close_codes_match_discord_non_reconnectable_codes() {
        for code in [4004_u16, 4010, 4011, 4012, 4013, 4014] {
            assert!(is_terminal_close_code(code));
        }

        for code in [4000_u16, 4007, 4009] {
            assert!(!is_terminal_close_code(code));
        }
    }

    #[test]
    fn voice_state_update_payload_matches_gateway_shape() {
        let payload = voice_state_update_payload(
            Snowflake::from("1"),
            Some(Snowflake::from("2")),
            false,
            true,
        );

        assert_eq!(payload["op"], serde_json::json!(4));
        assert_eq!(payload["d"]["guild_id"], serde_json::json!("1"));
        assert_eq!(payload["d"]["channel_id"], serde_json::json!("2"));
        assert_eq!(payload["d"]["self_mute"], serde_json::json!(false));
        assert_eq!(payload["d"]["self_deaf"], serde_json::json!(true));
    }

    #[test]
    fn gateway_typed_presence_and_member_request_payloads_match_gateway_shape() {
        let presence = UpdatePresence::online_with_activity("busy");
        let payload = update_presence_payload(presence);
        assert_eq!(payload["op"], serde_json::json!(3));
        assert_eq!(payload["d"]["status"], serde_json::json!("online"));
        assert_eq!(
            payload["d"]["activities"][0]["name"],
            serde_json::json!("busy")
        );

        let payload = request_guild_members_payload(RequestGuildMembers {
            guild_id: Snowflake::from("1"),
            query: Some("abc".to_string()),
            limit: Some(10),
            presences: Some(true),
            user_ids: Some(vec![Snowflake::from("2")]),
            nonce: Some("nonce".to_string()),
        });
        assert_eq!(payload["op"], serde_json::json!(8));
        assert_eq!(payload["d"]["guild_id"], serde_json::json!("1"));
        assert_eq!(payload["d"]["user_ids"], serde_json::json!(["2"]));
        assert_eq!(payload["d"]["nonce"], serde_json::json!("nonce"));

        let payload = request_channel_info_payload(RequestChannelInfo::voice_metadata("1"));
        assert_eq!(payload["op"], serde_json::json!(43));
        assert_eq!(payload["d"]["guild_id"], serde_json::json!("1"));
        assert_eq!(
            payload["d"]["fields"],
            serde_json::json!(["status", "voice_start_time"])
        );
    }

    #[test]
    fn gateway_command_classifier_routes_raw_payloads_through_limiter_buckets() {
        assert_eq!(
            classify_gateway_command(&GatewayCommand::UpdatePresence("online".into())),
            GatewayCommandClass::PresenceUpdate
        );
        assert_eq!(
            classify_gateway_command(&GatewayCommand::UpdatePresenceData(
                UpdatePresence::online_with_activity("online")
            )),
            GatewayCommandClass::PresenceUpdate
        );
        assert_eq!(
            classify_gateway_command(&GatewayCommand::RequestGuildMembers(RequestGuildMembers {
                guild_id: Snowflake::from("1"),
                query: None,
                limit: Some(0),
                presences: None,
                user_ids: None,
                nonce: None,
            })),
            GatewayCommandClass::BurstSensitive
        );
        assert_eq!(
            classify_gateway_command(&GatewayCommand::RequestChannelInfo(
                RequestChannelInfo::new("1", ["status"])
            )),
            GatewayCommandClass::BurstSensitive
        );
        assert_eq!(
            classify_gateway_command(&GatewayCommand::SendPayload(serde_json::json!({ "op": 3 }))),
            GatewayCommandClass::PresenceUpdate
        );
        assert_eq!(
            classify_gateway_command(&GatewayCommand::SendPayload(serde_json::json!({ "op": 8 }))),
            GatewayCommandClass::BurstSensitive
        );
        assert_eq!(
            classify_gateway_command(&GatewayCommand::SendPayload(
                serde_json::json!({ "op": 43 })
            )),
            GatewayCommandClass::BurstSensitive
        );
        assert_eq!(
            classify_gateway_command(&GatewayCommand::SendPayload(
                serde_json::json!({ "op": 99 })
            )),
            GatewayCommandClass::Other
        );
    }

    #[test]
    fn gateway_outbound_limiter_applies_presence_window_and_burst_spacing() {
        let mut limiter = GatewayOutboundLimiter::default();
        let now = Instant::now();
        let presence = GatewayCommand::UpdatePresence("online".into());

        for offset in 0..PRESENCE_UPDATE_LIMIT {
            assert_eq!(
                limiter.reserve_delay(&presence, now + Duration::from_secs(offset as u64)),
                None
            );
        }
        assert!(limiter
            .reserve_delay(
                &presence,
                now + PRESENCE_UPDATE_WINDOW - Duration::from_millis(1)
            )
            .is_some());
        assert_eq!(
            limiter.reserve_delay(&presence, now + PRESENCE_UPDATE_WINDOW),
            None
        );

        let mut limiter = GatewayOutboundLimiter::default();
        let request_members = GatewayCommand::SendPayload(serde_json::json!({ "op": 8 }));
        assert_eq!(limiter.reserve_delay(&request_members, now), None);
        assert_eq!(
            limiter.reserve_delay(
                &request_members,
                now + GATEWAY_COMMAND_MIN_SPACING - Duration::from_millis(1)
            ),
            Some(Duration::from_millis(1))
        );
        assert_eq!(
            limiter.reserve_delay(&request_members, now + GATEWAY_COMMAND_MIN_SPACING),
            None
        );
    }

    #[test]
    fn zlib_stream_decoder_waits_for_gateway_suffix() {
        let compressed = compress_gateway_payloads(&[r#"{"op":11,"d":null}"#])
            .pop()
            .expect("compressed payload");
        assert!(compressed.ends_with(ZLIB_SUFFIX));

        let split_at = compressed.len() - ZLIB_SUFFIX.len();
        let mut decoder = GatewayZlibStream::new();
        assert_eq!(
            decoder
                .decode(&compressed[..split_at])
                .expect("partial decode"),
            None
        );

        assert_eq!(
            decoder
                .decode(&compressed[split_at..])
                .expect("complete decode")
                .as_deref(),
            Some(r#"{"op":11,"d":null}"#)
        );
    }

    #[test]
    fn zlib_stream_decoder_keeps_state_across_payloads() {
        let payloads = compress_gateway_payloads(&[
            r#"{"op":0,"t":"READY","s":1,"d":{}}"#,
            r#"{"op":0,"t":"MESSAGE_CREATE","s":2,"d":{}}"#,
        ]);
        let mut decoder = GatewayZlibStream::new();

        assert_eq!(
            decoder
                .decode(&payloads[0])
                .expect("first stream payload")
                .as_deref(),
            Some(r#"{"op":0,"t":"READY","s":1,"d":{}}"#)
        );
        assert_eq!(
            decoder
                .decode(&payloads[1])
                .expect("second stream payload")
                .as_deref(),
            Some(r#"{"op":0,"t":"MESSAGE_CREATE","s":2,"d":{}}"#)
        );
    }

    #[cfg(feature = "zstd-stream")]
    #[test]
    fn zstd_stream_decoder_decodes_gateway_payloads() {
        let first = zstd::stream::encode_all(r#"{"op":0,"t":"READY","s":1,"d":{}}"#.as_bytes(), 0)
            .expect("zstd payload");
        let second = zstd::stream::encode_all(
            r#"{"op":0,"t":"MESSAGE_CREATE","s":2,"d":{}}"#.as_bytes(),
            0,
        )
        .expect("zstd payload");
        let mut decoder = GatewayCompressionDecoder::new(Some(GatewayCompression::ZstdStream))
            .expect("zstd decoder");

        assert_eq!(
            decoder.decode(&first).expect("first payload").as_deref(),
            Some(r#"{"op":0,"t":"READY","s":1,"d":{}}"#)
        );
        assert_eq!(
            decoder.decode(&second).expect("second payload").as_deref(),
            Some(r#"{"op":0,"t":"MESSAGE_CREATE","s":2,"d":{}}"#)
        );
    }

    #[test]
    fn initial_heartbeat_delay_uses_only_jitter_fraction() {
        assert_eq!(
            initial_heartbeat_delay(1_000, 0.0),
            Duration::from_millis(0)
        );
        assert_eq!(
            initial_heartbeat_delay(1_000, 0.25),
            Duration::from_millis(250)
        );
        assert_eq!(
            initial_heartbeat_delay(1_000, 1.5),
            Duration::from_millis(1_000)
        );
    }

    #[test]
    fn rand_jitter_stays_in_range_and_mixes_successive_calls() {
        let first = rand_jitter();
        let second = rand_jitter();

        assert!((0.0..1.0).contains(&first));
        assert!((0.0..1.0).contains(&second));
        assert_ne!(first, second);
    }

    #[test]
    fn identify_and_resume_payloads_use_raw_gateway_token() {
        let identify = identify_payload("secret-token", 513, Some([2, 4]), true);
        let resume = resume_payload("secret-token", "session", 42);

        assert_eq!(identify["d"]["token"], serde_json::json!("secret-token"));
        assert_eq!(resume["d"]["token"], serde_json::json!("secret-token"));
    }

    #[test]
    fn identify_without_shard_and_resume_without_sequence_keep_expected_shape() {
        let identify = identify_payload("secret-token", 513, None, true);
        let resume = resume_payload("secret-token", "session", 0);

        assert!(identify["d"].get("shard").is_none());
        assert_eq!(identify["d"]["intents"], serde_json::json!(513));
        assert_eq!(identify["d"]["compress"], serde_json::json!(true));
        assert_eq!(resume["d"]["session_id"], serde_json::json!("session"));
        assert!(resume["d"]["seq"].is_null());
    }

    #[test]
    fn identify_payload_omits_payload_compression_for_transport_compression() {
        let identify = identify_payload("secret-token", 513, None, false);

        assert!(identify["d"].get("compress").is_none());
    }

    #[test]
    fn shard_clamps_total_shards_and_updates_gateway_config() {
        let client = GatewayClient::new("secret-token".into(), 513).shard(2, 0);

        assert_eq!(client.shard_info, Some([2, 1]));
        assert_eq!(
            client.gateway_config.normalized_url(),
            "wss://gateway.discord.gg/?v=10&encoding=json&shard=2,1"
        );
    }

    #[test]
    fn terminal_close_helpers_cover_frame_and_none_cases() {
        let frame = CloseFrame {
            code: CloseCode::from(4004),
            reason: "bad auth".into(),
        };

        assert!(is_terminal_close_frame(Some(&frame)));
        assert_eq!(
            terminal_close_error(Some(frame.clone())),
            "gateway closed with terminal close code 4004: bad auth"
        );
        assert!(!is_terminal_close_frame(None));
        assert_eq!(
            terminal_close_error(None),
            "gateway closed with terminal close code"
        );
    }

    #[tokio::test]
    async fn recv_control_command_and_wait_for_backoff_command_handle_control_flow() {
        let (shutdown_tx, shutdown_rx) = mpsc::channel(GATEWAY_COMMAND_QUEUE_CAPACITY);
        let mut shutdown_client =
            GatewayClient::new("secret-token".into(), 513).control(shutdown_rx);
        shutdown_tx.send(GatewayCommand::Shutdown).await.unwrap();
        assert!(shutdown_client
            .wait_for_backoff_command(Duration::from_millis(10))
            .await
            .unwrap());

        let (reconnect_tx, reconnect_rx) = mpsc::channel(GATEWAY_COMMAND_QUEUE_CAPACITY);
        let mut reconnect_client =
            GatewayClient::new("secret-token".into(), 513).control(reconnect_rx);
        reconnect_client.session_id = Some("session".into());
        reconnect_client.resume_gateway_url = Some("wss://gateway.discord.gg".into());
        reconnect_client
            .sequence
            .store(42, std::sync::atomic::Ordering::Relaxed);
        reconnect_tx.send(GatewayCommand::Reconnect).await.unwrap();
        assert!(!reconnect_client
            .wait_for_backoff_command(Duration::from_millis(10))
            .await
            .unwrap());
        assert!(reconnect_client.session_id.is_none());
        assert!(reconnect_client.resume_gateway_url.is_none());
        assert_eq!(
            reconnect_client
                .sequence
                .load(std::sync::atomic::Ordering::Relaxed),
            0
        );

        let (presence_tx, presence_rx) = mpsc::channel(GATEWAY_COMMAND_QUEUE_CAPACITY);
        let mut presence_client =
            GatewayClient::new("secret-token".into(), 513).control(presence_rx);
        presence_tx
            .send(GatewayCommand::UpdatePresence("busy".into()))
            .await
            .unwrap();
        assert!(!presence_client
            .wait_for_backoff_command(Duration::from_millis(10))
            .await
            .unwrap());
        match recv_control_command(
            &mut presence_client.command_rx,
            &mut presence_client.deferred_commands,
        )
        .await
        {
            Some(GatewayCommand::UpdatePresence(status)) => assert_eq!(status, "busy"),
            other => panic!("unexpected deferred command: {other:?}"),
        }

        let mut no_control_client = GatewayClient::new("secret-token".into(), 513);
        assert!(!no_control_client
            .wait_for_backoff_command(Duration::from_millis(1))
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn recv_control_command_reads_payloads_and_handles_missing_channel() {
        let (tx, rx) = mpsc::channel(GATEWAY_COMMAND_QUEUE_CAPACITY);
        let mut command_rx = Some(rx);
        let mut deferred = VecDeque::new();

        tx.send(GatewayCommand::SendPayload(serde_json::json!({ "op": 4 })))
            .await
            .unwrap();
        match recv_control_command(&mut command_rx, &mut deferred).await {
            Some(GatewayCommand::SendPayload(payload)) => {
                assert_eq!(payload["op"], serde_json::json!(4));
            }
            other => panic!("unexpected control command: {other:?}"),
        }

        let mut none_rx = None;
        let pending = tokio::time::timeout(
            Duration::from_millis(5),
            recv_control_command(&mut none_rx, &mut deferred),
        )
        .await;
        assert!(pending.is_err());
    }

    #[tokio::test]
    async fn recv_control_command_prefers_deferred_commands() {
        let (tx, rx) = mpsc::channel(GATEWAY_COMMAND_QUEUE_CAPACITY);
        let mut command_rx = Some(rx);
        let mut deferred = VecDeque::new();

        tx.send(GatewayCommand::UpdatePresence("online".into()))
            .await
            .unwrap();
        deferred.push_back(GatewayCommand::Reconnect);

        assert!(matches!(
            recv_control_command(&mut command_rx, &mut deferred).await,
            Some(GatewayCommand::Reconnect)
        ));
        match recv_control_command(&mut command_rx, &mut deferred).await {
            Some(GatewayCommand::UpdatePresence(status)) => assert_eq!(status, "online"),
            other => panic!("unexpected command order: {other:?}"),
        }
    }

    #[tokio::test]
    async fn outbound_worker_sends_immediate_messages_while_presence_limited() {
        let (outbound_tx, outbound_rx) = mpsc::unbounded_channel();
        let (sent_tx, mut sent_rx) = mpsc::unbounded_channel();
        let worker = tokio::spawn(run_gateway_outbound_worker(
            RecordingSink(sent_tx),
            outbound_rx,
        ));

        for index in 0..=PRESENCE_UPDATE_LIMIT {
            outbound_tx
                .send(GatewayOutboundMessage::Limited(
                    GatewayCommand::UpdatePresence(format!("presence-{index}")),
                ))
                .unwrap();
        }
        outbound_tx
            .send(GatewayOutboundMessage::ImmediatePayload(
                serde_json::json!({
                    "op": 1,
                    "d": null
                }),
            ))
            .unwrap();

        let mut saw_immediate_heartbeat = false;
        for _ in 0..=PRESENCE_UPDATE_LIMIT {
            let message = tokio::time::timeout(Duration::from_millis(200), sent_rx.recv())
                .await
                .unwrap()
                .unwrap();
            let text = message.into_text().unwrap();
            let payload: serde_json::Value = serde_json::from_str(&text).unwrap();
            if payload["op"] == serde_json::json!(1) {
                saw_immediate_heartbeat = true;
                break;
            }
        }
        assert!(saw_immediate_heartbeat);

        drop(outbound_tx);
        tokio::time::timeout(Duration::from_millis(200), worker)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn connect_and_run_identifies_processes_ready_and_shuts_down() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (command_tx, command_rx) = mpsc::channel(GATEWAY_COMMAND_QUEUE_CAPACITY);
        let events = Arc::new(Mutex::new(Vec::<(String, serde_json::Value)>::new()));
        let events_for_callback = Arc::clone(&events);
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(stream).await.unwrap();

            ws.send(WsMessage::Text(
                serde_json::json!({
                    "op": 10,
                    "d": { "heartbeat_interval": 60_000 }
                })
                .to_string()
                .into(),
            ))
            .await
            .unwrap();

            let identify_payload: serde_json::Value =
                serde_json::from_str(&ws.next().await.unwrap().unwrap().into_text().unwrap())
                    .unwrap();

            ws.send(WsMessage::Text(
                serde_json::json!({
                    "op": 0,
                    "t": "READY",
                    "s": 7,
                    "d": {
                        "user": {
                            "id": "1",
                            "username": "discordrs"
                        },
                        "session_id": "session-1",
                        "resume_gateway_url": "wss://gateway.discord.gg"
                    }
                })
                .to_string()
                .into(),
            ))
            .await
            .unwrap();

            let _ = ws.next().await;

            identify_payload
        });

        let shutdown = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            command_tx.send(GatewayCommand::Shutdown).await.unwrap();
        });

        let mut client = GatewayClient::new("secret-token".into(), 513).control(command_rx);
        let on_event: EventCallback = Arc::new(move |name, data| {
            events_for_callback.lock().unwrap().push((name, data));
        });
        let action = client
            .connect_and_run(&format!("ws://{address}"), on_event)
            .await
            .unwrap();

        shutdown.await.unwrap();
        let identify = server.await.unwrap();

        assert!(matches!(
            action,
            ReconnectAction::Shutdown | ReconnectAction::Resume
        ));
        assert_eq!(identify["op"], serde_json::json!(2));
        assert_eq!(identify["d"]["token"], serde_json::json!("secret-token"));
        assert!(identify["d"].get("compress").is_none());
        assert_eq!(client.session_id.as_deref(), Some("session-1"));
        assert_eq!(
            client.resume_gateway_url.as_deref(),
            Some("wss://gateway.discord.gg?v=10&encoding=json")
        );
        assert_eq!(
            client.sequence.load(std::sync::atomic::Ordering::Relaxed),
            7
        );
        assert_eq!(events.lock().unwrap()[0].0, "READY");
    }

    #[tokio::test]
    async fn connect_and_run_decodes_compressed_hello_and_dispatch_payloads() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (command_tx, command_rx) = mpsc::channel(GATEWAY_COMMAND_QUEUE_CAPACITY);
        let events = Arc::new(Mutex::new(Vec::<(String, serde_json::Value)>::new()));
        let events_for_callback = Arc::clone(&events);
        let compressed = compress_gateway_payloads(&[
            &serde_json::json!({
                "op": 10,
                "d": { "heartbeat_interval": 60_000 }
            })
            .to_string(),
            &serde_json::json!({
                "op": 0,
                "t": "READY",
                "s": 8,
                "d": {
                    "user": {
                        "id": "1",
                        "username": "discordrs"
                    },
                    "session_id": "session-compressed",
                    "resume_gateway_url": "wss://gateway.discord.gg"
                }
            })
            .to_string(),
        ]);

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(stream).await.unwrap();

            ws.send(WsMessage::Binary(compressed[0].clone().into()))
                .await
                .unwrap();

            let identify_payload: serde_json::Value =
                serde_json::from_str(&ws.next().await.unwrap().unwrap().into_text().unwrap())
                    .unwrap();

            ws.send(WsMessage::Binary(compressed[1].clone().into()))
                .await
                .unwrap();
            let _ = ws.next().await;

            identify_payload
        });

        let shutdown = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            command_tx.send(GatewayCommand::Shutdown).await.unwrap();
        });

        let mut client = GatewayClient::new("secret-token".into(), 513)
            .gateway_config(
                GatewayConnectionConfig::new(format!("ws://{address}"))
                    .compression(GatewayCompression::ZlibStream),
            )
            .control(command_rx);
        let on_event: EventCallback = Arc::new(move |name, data| {
            events_for_callback.lock().unwrap().push((name, data));
        });
        let action = client
            .connect_and_run(&format!("ws://{address}"), on_event)
            .await
            .unwrap();

        shutdown.await.unwrap();
        let identify = server.await.unwrap();

        assert!(matches!(
            action,
            ReconnectAction::Shutdown | ReconnectAction::Resume
        ));
        assert_eq!(identify["op"], serde_json::json!(2));
        assert!(identify["d"].get("compress").is_none());
        assert_eq!(client.session_id.as_deref(), Some("session-compressed"));
        assert_eq!(
            client.sequence.load(std::sync::atomic::Ordering::Relaxed),
            8
        );
        assert_eq!(events.lock().unwrap()[0].0, "READY");
    }

    #[tokio::test]
    async fn connect_and_run_resumes_existing_session_and_handles_invalid_session() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(stream).await.unwrap();

            ws.send(WsMessage::Text(
                serde_json::json!({
                    "op": 10,
                    "d": { "heartbeat_interval": 60_000 }
                })
                .to_string()
                .into(),
            ))
            .await
            .unwrap();

            let resume_payload: serde_json::Value =
                serde_json::from_str(&ws.next().await.unwrap().unwrap().into_text().unwrap())
                    .unwrap();

            ws.send(WsMessage::Text(
                serde_json::json!({
                    "op": 9,
                    "d": false
                })
                .to_string()
                .into(),
            ))
            .await
            .unwrap();

            resume_payload
        });

        let mut client = GatewayClient::new("secret-token".into(), 513);
        client.session_id = Some("session-2".into());
        client
            .sequence
            .store(42, std::sync::atomic::Ordering::Relaxed);
        let action = client
            .connect_and_run(&format!("ws://{address}"), Arc::new(|_, _| {}))
            .await
            .unwrap();

        let resume = server.await.unwrap();

        assert!(matches!(action, ReconnectAction::Reconnect));
        assert_eq!(resume["op"], serde_json::json!(6));
        assert_eq!(resume["d"]["token"], serde_json::json!("secret-token"));
        assert_eq!(resume["d"]["session_id"], serde_json::json!("session-2"));
        assert_eq!(resume["d"]["seq"], serde_json::json!(42));
    }

    #[tokio::test]
    async fn connect_and_run_skips_malformed_messages_and_honors_reconnect_opcode() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(stream).await.unwrap();

            ws.send(WsMessage::Text(
                serde_json::json!({
                    "op": 10,
                    "d": { "heartbeat_interval": 60_000 }
                })
                .to_string()
                .into(),
            ))
            .await
            .unwrap();

            let identify_payload: serde_json::Value =
                serde_json::from_str(&ws.next().await.unwrap().unwrap().into_text().unwrap())
                    .unwrap();

            ws.send(WsMessage::Text("not-json".into())).await.unwrap();
            ws.send(WsMessage::Text(
                serde_json::json!({
                    "op": 7,
                    "d": null
                })
                .to_string()
                .into(),
            ))
            .await
            .unwrap();

            identify_payload
        });

        let mut client = GatewayClient::new("secret-token".into(), 513)
            .gateway_config(GatewayConnectionConfig::new(format!("ws://{address}")));
        let action = client
            .connect_and_run(&format!("ws://{address}"), Arc::new(|_, _| {}))
            .await
            .unwrap();

        let identify = server.await.unwrap();

        assert!(matches!(action, ReconnectAction::Resume));
        assert_eq!(identify["op"], serde_json::json!(2));
    }

    #[tokio::test]
    async fn connect_and_run_replies_to_server_heartbeat_requests() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (command_tx, command_rx) = mpsc::channel(GATEWAY_COMMAND_QUEUE_CAPACITY);
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(stream).await.unwrap();

            ws.send(WsMessage::Text(
                serde_json::json!({
                    "op": 10,
                    "d": { "heartbeat_interval": 60_000 }
                })
                .to_string()
                .into(),
            ))
            .await
            .unwrap();

            let identify_payload: serde_json::Value =
                serde_json::from_str(&ws.next().await.unwrap().unwrap().into_text().unwrap())
                    .unwrap();

            ws.send(WsMessage::Text(
                serde_json::json!({
                    "op": 1,
                    "d": null
                })
                .to_string()
                .into(),
            ))
            .await
            .unwrap();

            let heartbeat_payload: serde_json::Value =
                serde_json::from_str(&ws.next().await.unwrap().unwrap().into_text().unwrap())
                    .unwrap();
            let _ = ws.next().await;

            (identify_payload, heartbeat_payload)
        });

        let shutdown = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            command_tx.send(GatewayCommand::Shutdown).await.unwrap();
        });

        let mut client = GatewayClient::new("secret-token".into(), 513).control(command_rx);
        let action = client
            .connect_and_run(&format!("ws://{address}"), Arc::new(|_, _| {}))
            .await
            .unwrap();

        shutdown.await.unwrap();
        let (identify, heartbeat) = server.await.unwrap();

        assert!(matches!(
            action,
            ReconnectAction::Shutdown | ReconnectAction::Resume
        ));
        assert_eq!(identify["op"], serde_json::json!(2));
        assert_eq!(heartbeat["op"], serde_json::json!(1));
        assert!(heartbeat["d"].is_null());
    }

    #[tokio::test]
    async fn run_reconnects_after_invalid_session_and_then_shuts_down_cleanly() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (command_tx, command_rx) = mpsc::channel(GATEWAY_COMMAND_QUEUE_CAPACITY);
        let payloads = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
        let payloads_for_server = Arc::clone(&payloads);

        let server = tokio::spawn(async move {
            for iteration in 0..2 {
                let (stream, _) = listener.accept().await.unwrap();
                let mut ws = accept_async(stream).await.unwrap();

                ws.send(WsMessage::Text(
                    serde_json::json!({
                        "op": 10,
                        "d": { "heartbeat_interval": 60_000 }
                    })
                    .to_string()
                    .into(),
                ))
                .await
                .unwrap();

                let payload: serde_json::Value =
                    serde_json::from_str(&ws.next().await.unwrap().unwrap().into_text().unwrap())
                        .unwrap();
                payloads_for_server.lock().unwrap().push(payload);

                if iteration == 0 {
                    ws.send(WsMessage::Text(
                        serde_json::json!({
                            "op": 9,
                            "d": false
                        })
                        .to_string()
                        .into(),
                    ))
                    .await
                    .unwrap();
                } else {
                    let close = ws.next().await;
                    assert!(matches!(close, Some(Ok(WsMessage::Close(_))) | None));
                }
            }
        });

        let shutdown = tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(80)).await;
            command_tx.send(GatewayCommand::Shutdown).await.unwrap();
        });

        let mut client = GatewayClient::new("secret-token".into(), 513)
            .gateway_config(GatewayConnectionConfig::new(format!("ws://{address}/")))
            .control(command_rx);
        client.session_id = Some("session-2".into());
        client.resume_gateway_url = Some(format!("ws://{address}/"));
        client
            .sequence
            .store(42, std::sync::atomic::Ordering::Relaxed);

        client.run(Arc::new(|_, _| {})).await.unwrap();

        shutdown.await.unwrap();
        server.await.unwrap();

        let payloads = payloads.lock().unwrap();
        assert_eq!(payloads.len(), 2);
        assert_eq!(payloads[0]["op"], serde_json::json!(6));
        assert_eq!(payloads[1]["op"], serde_json::json!(2));
        assert!(client.session_id.is_none());
        assert!(client.resume_gateway_url.is_none());
        assert_eq!(
            client.sequence.load(std::sync::atomic::Ordering::Relaxed),
            0
        );
    }

    #[cfg(feature = "sharding")]
    #[test]
    fn supervisor_callback_records_state_and_error_events() {
        let seen = Arc::new(Mutex::new(Vec::<ShardSupervisorEvent>::new()));
        let seen_for_callback = Arc::clone(&seen);
        let callback = Arc::new(move |event| {
            seen_for_callback.lock().unwrap().push(event);
        });

        let client = GatewayClient::new("secret-token".into(), 513)
            .shard(3, 5)
            .supervisor(callback);

        client.publish_state(ShardRuntimeState::Running);
        client.publish_error("boom".to_string());

        let seen = seen.lock().unwrap();
        assert_eq!(
            seen[0],
            ShardSupervisorEvent::StateChanged {
                shard_id: 3,
                state: ShardRuntimeState::Running,
            }
        );
        assert_eq!(
            seen[1],
            ShardSupervisorEvent::GatewayError {
                shard_id: 3,
                message: "boom".to_string(),
            }
        );
    }
}
