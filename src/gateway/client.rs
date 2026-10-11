use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio::time::{sleep, timeout, timeout_at, Duration, Instant};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{
        protocol::{frame::coding::CloseCode, CloseFrame},
        Message as WsMessage,
    },
};
use tracing::{debug, error, info, warn};

use crate::model::{ChannelId, GuildId, RequestChannelInfo, RequestGuildMembers, UpdatePresence};
#[cfg(feature = "sharding")]
use crate::sharding::{ShardRuntimeState, ShardSupervisorEvent};
use crate::ws::{GatewayConnectionConfig, IdentifyOutcome, IdentifyPermit};

use super::compression::{decode_gateway_message, GatewayCompressionDecoder};
use super::outbound::{run_gateway_outbound_worker, send_gateway_outbound, GatewayOutboundMessage};

// Gateway opcodes (Discord docs: Opcodes and Status Codes › Gateway Opcodes).
const OP_DISPATCH: u64 = 0;
const OP_HEARTBEAT: u64 = 1;
const OP_IDENTIFY: u64 = 2;
pub(super) const OP_PRESENCE_UPDATE: u64 = 3;
pub(super) const OP_VOICE_STATE_UPDATE: u64 = 4;
const OP_RESUME: u64 = 6;
const OP_RECONNECT: u64 = 7;
pub(super) const OP_REQUEST_GUILD_MEMBERS: u64 = 8;
const OP_INVALID_SESSION: u64 = 9;
const OP_HELLO: u64 = 10;
const OP_HEARTBEAT_ACK: u64 = 11;
/// Not in Discord's public opcode table (lazy guild subscriptions); a raw
/// payload sending it is paced like the other request opcodes.
pub(super) const OP_GUILD_SUBSCRIPTIONS: u64 = 14;
pub(super) const OP_REQUEST_SOUNDBOARD_SOUNDS: u64 = 31;
pub(super) const OP_REQUEST_CHANNEL_INFO: u64 = 43;
// Gateway close codes (Discord docs: Opcodes and Status Codes › Gateway
// Close Event Codes).
const CLOSE_AUTHENTICATION_FAILED: u16 = 4004;
const CLOSE_INVALID_SEQ: u16 = 4007;
const CLOSE_SESSION_TIMED_OUT: u16 = 4009;
const CLOSE_INVALID_SHARD: u16 = 4010;
const CLOSE_SHARDING_REQUIRED: u16 = 4011;
const CLOSE_INVALID_API_VERSION: u16 = 4012;
const CLOSE_INVALID_INTENTS: u16 = 4013;
const CLOSE_DISALLOWED_INTENTS: u16 = 4014;
/// First reconnect delay after an unhealthy session, in seconds; doubled on
/// each further failure up to [`MAX_RECONNECT_BACKOFF_SECS`].
const INITIAL_RECONNECT_BACKOFF_SECS: u64 = 1;
const MAX_RECONNECT_BACKOFF_SECS: u64 = 300;
/// Pause after INVALID_SESSION before resuming or identifying again. Discord
/// asks for a random 1–5 s; a fixed value inside that range suffices.
const INVALID_SESSION_DELAY: Duration = Duration::from_secs(2);
pub(super) const GATEWAY_COMMAND_QUEUE_CAPACITY: usize = 256;
/// Discord allows at most one IDENTIFY per shard per 5 seconds.
const IDENTIFY_MIN_INTERVAL: Duration = Duration::from_secs(5);
/// Upper bound on connection setup (TCP/TLS connect, WebSocket upgrade, and
/// HELLO) while an IDENTIFY permit is held. The permit blocks every other
/// shard in the bucket, so a hung connect or a server that never sends
/// HELLO gives it back (as abandoned) and retries through the normal
/// backoff, instead of holding the bucket until the gate's own watchdog.
const GATED_SETUP_TIMEOUT: Duration = Duration::from_secs(15);
/// Most non-presence commands kept for the next connection while the
/// client is not connected (reconnect backoff, or waiting for an IDENTIFY
/// permit, which can last hours until a daily budget reset). The backlog is
/// bounded: presence updates coalesce to the newest one, and beyond this
/// many other commands the oldest is dropped. Its caller has most likely
/// timed out by then (e.g. `fetch_members`), while newer requests may
/// still be answered.
const MAX_DEFERRED_COMMANDS: usize = 32;
/// A connection that dies faster than this is treated as unhealthy and
/// re-dialed with escalating backoff instead of immediately.
const SHORT_SESSION_THRESHOLD: Duration = Duration::from_secs(30);
/// Mixed into the heartbeat jitter so shards that start in the same instant
/// still get different first-heartbeat delays.
static JITTER_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(crate) struct GatewayClient {
    token: String,
    intents: u64,
    session_id: Option<String>,
    resume_gateway_url: Option<String>,
    gateway_config: GatewayConnectionConfig,
    shard_info: Option<[u32; 2]>,
    initial_presence: Option<UpdatePresence>,
    command_rx: Option<mpsc::Receiver<GatewayCommand>>,
    deferred_commands: VecDeque<GatewayCommand>,
    #[cfg(feature = "sharding")]
    supervisor_callback: Option<SupervisorCallback>,
    // Written by the read loop, read by the heartbeat task; kept across
    // reconnects for RESUME.
    sequence: Arc<AtomicU64>,
    // Cleared by the heartbeat task before each beat, set by the read loop
    // on ACK: still clear at the next beat means a zombie connection.
    heartbeat_ack_received: Arc<AtomicBool>,
    last_identify: Option<Instant>,
    /// Set when the server explicitly asked for a reconnect (op 7 RECONNECT
    /// or op 9 INVALID_SESSION). Those paths re-dial immediately — op 9
    /// already sleeps before disconnecting, and IDENTIFY pacing bounds the
    /// identify rate — while unsolicited short-lived disconnects back off.
    server_directed_reconnect: bool,
    /// [`GATED_SETUP_TIMEOUT`]; a field so tests can shorten it.
    gated_setup_timeout: Duration,
}

/// Result of waiting for an IDENTIFY permit.
enum PermitWait {
    /// Proceed; `Some` when a gate granted a permit for this attempt.
    Ready(Option<IdentifyPermit>),
    /// A `Shutdown` command arrived while waiting.
    Shutdown,
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
            initial_presence: None,
            command_rx: None,
            deferred_commands: VecDeque::new(),
            #[cfg(feature = "sharding")]
            supervisor_callback: None,
            sequence: Arc::new(AtomicU64::new(0)),
            heartbeat_ack_received: Arc::new(AtomicBool::new(true)),
            last_identify: None,
            server_directed_reconnect: false,
            gated_setup_timeout: GATED_SETUP_TIMEOUT,
        }
    }

    /// Sets the presence included in IDENTIFY so the bot appears with the
    /// desired status from the first moment it connects.
    pub fn initial_presence(mut self, presence: Option<UpdatePresence>) -> Self {
        self.initial_presence = presence;
        self
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
        let mut backoff = INITIAL_RECONNECT_BACKOFF_SECS;
        #[cfg(feature = "sharding")]
        self.publish_state(ShardRuntimeState::Starting);
        loop {
            let url = self
                .resume_gateway_url
                .clone()
                .unwrap_or_else(|| self.gateway_config.normalized_url());
            info!("Connecting to gateway: {url}");

            let connection_started = Instant::now();
            match self.connect_and_run(&url, Arc::clone(&on_event)).await {
                Ok(action) => {
                    match action {
                        ReconnectAction::Resume => {
                            #[cfg(feature = "sharding")]
                            self.publish_state(ShardRuntimeState::Reconnecting);
                            info!("Resuming gateway session");
                        }
                        ReconnectAction::Reconnect => {
                            #[cfg(feature = "sharding")]
                            self.publish_state(ShardRuntimeState::Reconnecting);
                            info!("Reconnecting with fresh session");
                            self.session_id = None;
                            self.resume_gateway_url = None;
                            self.sequence.store(0, Ordering::Relaxed);
                        }
                        ReconnectAction::Shutdown => {
                            #[cfg(feature = "sharding")]
                            self.publish_state(ShardRuntimeState::Stopped);
                            return Ok(());
                        }
                        ReconnectAction::Fatal(error) => {
                            // Reconnecting cannot fix bad credentials, intents,
                            // shard settings, or API version; retrying would
                            // only repeat the failure forever with backoff.
                            error!("Gateway stopped: {error}");
                            #[cfg(feature = "sharding")]
                            self.publish_state(ShardRuntimeState::Stopped);
                            return Err(error);
                        }
                    }
                    // A healthy long-lived session or a server-directed
                    // reconnect re-dials immediately so resumes don't miss
                    // events; an unsolicited connection death within seconds
                    // is re-dialed with escalating backoff to avoid a tight
                    // connect/close loop against the gateway.
                    let server_directed = std::mem::take(&mut self.server_directed_reconnect);
                    if server_directed || connection_started.elapsed() >= SHORT_SESSION_THRESHOLD {
                        backoff = INITIAL_RECONNECT_BACKOFF_SECS;
                    } else {
                        warn!(
                            "Gateway session ended after {:?}, backing off {backoff}s before reconnecting",
                            connection_started.elapsed()
                        );
                        if self
                            .wait_for_backoff_command(Duration::from_secs(backoff))
                            .await?
                        {
                            #[cfg(feature = "sharding")]
                            self.publish_state(ShardRuntimeState::Stopped);
                            return Ok(());
                        }
                        backoff = (backoff * 2).min(MAX_RECONNECT_BACKOFF_SECS);
                    }
                }
                Err(e) => {
                    #[cfg(feature = "sharding")]
                    self.publish_error(e.to_string());
                    error!("Gateway connection error: {e}");
                    if self
                        .wait_for_backoff_command(Duration::from_secs(
                            backoff.min(MAX_RECONNECT_BACKOFF_SECS),
                        ))
                        .await?
                    {
                        #[cfg(feature = "sharding")]
                        self.publish_state(ShardRuntimeState::Stopped);
                        return Ok(());
                    }
                    backoff = (backoff * 2).min(MAX_RECONNECT_BACKOFF_SECS);
                }
            }
        }
    }

    /// Asks the configured IDENTIFY gate for a permit when the next
    /// connection will IDENTIFY (no session to resume).
    ///
    /// The wait can last until a bucket frees up or the daily session budget
    /// resets, so the control channel keeps being served meanwhile:
    /// `Shutdown` ends the wait at once, other commands are deferred to the
    /// connection, and `Reconnect` is moot because this attempt is already a
    /// fresh session.
    async fn acquire_identify_permit(&mut self) -> Result<PermitWait, crate::error::DiscordError> {
        if self.session_id.is_some() {
            return Ok(PermitWait::Ready(None));
        }
        let Some(gate) = self.gateway_config.identify_gate_ref().cloned() else {
            return Ok(PermitWait::Ready(None));
        };
        let [shard_id, shard_count] = self.shard_info.unwrap_or([0, 1]);
        debug!("Waiting for IDENTIFY permit for shard {shard_id}/{shard_count}");
        // The backlog may have grown during an earlier backoff.
        self.bound_deferred_commands();
        let acquire = gate.acquire(shard_id, shard_count);
        tokio::pin!(acquire);
        let mut listening = self.command_rx.is_some();
        loop {
            tokio::select! {
                permit = &mut acquire => return permit.map(|permit| PermitWait::Ready(Some(permit))),
                command = async {
                    match self.command_rx.as_mut() {
                        Some(command_rx) => command_rx.recv().await,
                        None => None,
                    }
                }, if listening => match command {
                    Some(GatewayCommand::Shutdown) => return Ok(PermitWait::Shutdown),
                    Some(GatewayCommand::Reconnect) => {}
                    Some(command) => self.defer_command(command),
                    None => listening = false,
                },
            }
        }
    }

    /// Queues a command for the next connection and keeps the backlog
    /// within [`MAX_DEFERRED_COMMANDS`]. `Shutdown` never comes here; it
    /// ends the wait instead.
    fn defer_command(&mut self, command: GatewayCommand) {
        self.deferred_commands.push_back(command);
        self.bound_deferred_commands();
    }

    /// Applies the backlog policy to everything queued, including a backlog
    /// that grew before the policy applied: the newest presence update
    /// wins, and at most [`MAX_DEFERRED_COMMANDS`] other commands stay
    /// (oldest dropped first). Returns how many commands of each kind were
    /// dropped for the cap. Only kinds and counts are logged, because
    /// payloads can carry credentials.
    fn bound_deferred_commands(&mut self) -> BTreeMap<&'static str, usize> {
        let newest_presence = self.deferred_commands.iter().rposition(is_presence_command);
        let others = self
            .deferred_commands
            .iter()
            .filter(|command| !is_presence_command(command))
            .count();
        let mut excess = others.saturating_sub(MAX_DEFERRED_COMMANDS);
        let mut dropped: BTreeMap<&'static str, usize> = BTreeMap::new();
        let mut index = 0;
        self.deferred_commands.retain(|command| {
            let position = index;
            index += 1;
            if is_presence_command(command) {
                // Superseded presence updates are expected, not a loss.
                return Some(position) == newest_presence;
            }
            if excess > 0 {
                excess -= 1;
                *dropped.entry(gateway_command_kind(command)).or_default() += 1;
                return false;
            }
            true
        });
        if !dropped.is_empty() {
            warn!(
                dropped = dropped.values().sum::<usize>(),
                kinds = ?dropped,
                "dropped the oldest deferred gateway commands while disconnected \
                 (backlog limit {MAX_DEFERRED_COMMANDS})"
            );
        }
        dropped
    }

    async fn connect_and_run(
        &mut self,
        url: &str,
        on_event: EventCallback,
    ) -> Result<ReconnectAction, crate::error::DiscordError> {
        // A connection without a session will IDENTIFY, so take the gate's
        // permit before dialing (see `IdentifyGate`). RESUME stays outside
        // the gate. Every early return below drops the permit, which reports
        // the attempt as abandoned.
        let mut identify_permit = match self.acquire_identify_permit().await {
            Ok(PermitWait::Ready(permit)) => permit,
            Ok(PermitWait::Shutdown) => return Ok(ReconnectAction::Shutdown),
            Err(error) => return Ok(ReconnectAction::Fatal(error)),
        };
        // While a permit is held, connect + upgrade + HELLO share one
        // deadline.
        let gated_setup_timeout = self.gated_setup_timeout;
        let setup_deadline = identify_permit
            .as_ref()
            .map(|_| Instant::now() + gated_setup_timeout);
        let setup_timed_out = || {
            crate::error::DiscordError::gateway(format!(
                "gateway connection setup (connect and HELLO) took longer than \
                 {gated_setup_timeout:?} while holding an IDENTIFY permit"
            ))
        };
        let (ws_stream, _) = match setup_deadline {
            Some(deadline) => timeout_at(deadline, connect_async(url))
                .await
                .map_err(|_| setup_timed_out())??,
            None => connect_async(url).await?,
        };
        let (mut write, mut read) = ws_stream.split();
        let mut compression_decoder = GatewayCompressionDecoder::new(
            self.gateway_config.compression_kind(),
        )
        .map_err(|error| {
            crate::error::DiscordError::gateway(format!(
                "failed to initialize gateway compression decoder: {error}"
            ))
        })?;

        // Wait for Hello
        let hello_text = loop {
            let next = match setup_deadline {
                Some(deadline) => timeout_at(deadline, read.next())
                    .await
                    .map_err(|_| setup_timed_out())?,
                None => read.next().await,
            };
            let hello = next.ok_or_else(|| {
                crate::error::DiscordError::gateway("gateway closed before Hello")
            })??;
            // Discord can reject a connection before HELLO (for example 4012
            // for an invalid API version). The compression decoder ignores
            // close frames, so classify them here; otherwise a terminal code
            // would surface as a generic EOF and be retried forever.
            if let WsMessage::Close(frame) = hello {
                #[cfg(feature = "sharding")]
                self.publish_error(terminal_close_error(frame.clone()));
                warn!("Gateway closed before Hello: {frame:?}");
                return match close_frame_action(frame) {
                    action @ ReconnectAction::Fatal(_) => Ok(action),
                    _ => Err(crate::error::DiscordError::gateway(
                        "gateway closed before Hello",
                    )),
                };
            }
            match decode_gateway_message(hello, &mut compression_decoder) {
                Ok(Some(text)) => break text,
                Ok(None) => continue,
                Err(error) => {
                    return Err(crate::error::DiscordError::gateway(format!(
                        "failed to decode gateway Hello payload: {error}"
                    )));
                }
            }
        };
        let hello_payload: Value = serde_json::from_str(&hello_text)?;
        let hello_op = hello_payload["op"].as_u64().unwrap_or(u64::MAX);
        if hello_op != OP_HELLO {
            return Err(crate::error::DiscordError::gateway(format!(
                "expected Hello opcode {OP_HELLO}, got {hello_op}"
            )));
        }
        let heartbeat_interval_ms = hello_payload["d"]["heartbeat_interval"]
            .as_u64()
            .ok_or_else(|| {
                crate::error::DiscordError::gateway("missing heartbeat_interval in Hello")
            })?;

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
            // With a gate, pacing is the gate's job (it spaces the whole
            // bucket, which includes this shard); sleeping here would only
            // hold the bucket longer.
            if identify_permit.is_none() {
                if let Some(last_identify) = self.last_identify {
                    let since_last = last_identify.elapsed();
                    if since_last < IDENTIFY_MIN_INTERVAL {
                        let wait = IDENTIFY_MIN_INTERVAL - since_last;
                        debug!("Pacing IDENTIFY, waiting {wait:?} to respect the 5s/shard limit");
                        sleep(wait).await;
                    }
                }
            }
            let identify = identify_payload(
                &self.token,
                self.intents,
                self.gateway_config.capability_bits(),
                self.shard_info,
                self.initial_presence.as_ref(),
                false,
            );
            // From here the payload may reach Discord even if the write
            // fails or is cancelled, so the permit must count it as sent.
            if let Some(permit) = identify_permit.as_mut() {
                permit.mark_sending();
            }
            write
                .send(WsMessage::Text(identify.to_string().into()))
                .await?;
            if let Some(permit) = identify_permit.take() {
                permit.finish(IdentifyOutcome::Sent);
            }
            self.last_identify = Some(Instant::now());
            debug!("Sent Identify");
        }

        // Spawn heartbeat task
        let (heartbeat_tx, mut heartbeat_rx) = mpsc::channel::<String>(8);
        let sequence_clone = Arc::clone(&self.sequence);
        let ack_received = Arc::clone(&self.heartbeat_ack_received);
        ack_received.store(true, Ordering::Relaxed);

        let _heartbeat_task = AbortOnDrop::new(tokio::spawn(async move {
            sleep(initial_heartbeat_delay(
                heartbeat_interval_ms,
                rand_jitter(),
            ))
            .await;

            loop {
                if !ack_received.load(Ordering::Relaxed) {
                    warn!("Heartbeat ACK not received - zombie connection");
                    // Fails only if the read loop already ended.
                    heartbeat_tx.send("zombie".to_string()).await.ok();
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
        }));

        // Main read loop. The websocket sink lives in a worker so rate-limit
        // sleeps never block inbound frames or heartbeat ACK processing.
        let (outbound_tx, outbound_rx) = mpsc::unbounded_channel();
        let (outbound_error_tx, mut outbound_error_rx) = mpsc::unbounded_channel();
        let mut outbound_task = AbortOnDrop::new(tokio::spawn(async move {
            if let Err(error) = run_gateway_outbound_worker(write, outbound_rx).await {
                // Fails only if the read loop already ended.
                outbound_error_tx.send(error.to_string()).ok();
            }
        }));

        let action = loop {
            tokio::select! {
                msg = read.next() => {
                    let text = match msg {
                        Some(Ok(WsMessage::Close(frame))) => {
                            #[cfg(feature = "sharding")]
                            self.publish_error(terminal_close_error(frame.clone()));
                            warn!("Gateway closed: {frame:?}");
                            break close_frame_action(frame);
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
                            self.server_directed_reconnect = true;
                            break ReconnectAction::Resume;
                        }
                        OP_INVALID_SESSION => {
                            let resumable = payload["d"].as_bool().unwrap_or(false);
                            #[cfg(feature = "sharding")]
                            self.publish_state(ShardRuntimeState::Reconnecting);
                            warn!("Invalid session, resumable={resumable}");
                            sleep(INVALID_SESSION_DELAY).await;
                            self.server_directed_reconnect = true;
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
                            // Fails only if the outbound worker already
                            // ended, and then the socket is closing anyway.
                            outbound_tx.send(GatewayOutboundMessage::Close(Some(CloseFrame {
                                    code: CloseCode::Normal,
                                    reason: "supervisor shutdown".into(),
                                }))).ok();
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

        // The heartbeat task is aborted when `_heartbeat_task` drops at the
        // end of this function. Only a supervisor shutdown waits for the
        // outbound worker, so the close frame it queued is flushed. The guard
        // stays armed during the wait: if this future is cancelled or the
        // flush times out, dropping the guard still aborts the worker
        // (aborting a task that already finished is a no-op).
        if matches!(action, ReconnectAction::Shutdown) {
            drop(outbound_tx);
            if let Some(outbound_handle) = outbound_task.handle_mut() {
                if timeout(Duration::from_secs(1), outbound_handle)
                    .await
                    .is_err()
                {
                    warn!("Gateway outbound worker did not stop after shutdown close");
                }
            }
        }
        Ok(action)
    }
}

enum ReconnectAction {
    Resume,
    Reconnect,
    Shutdown,
    /// The gateway rejected the connection in a way reconnecting cannot fix.
    Fatal(crate::error::DiscordError),
}

/// Aborts a spawned per-connection task when dropped.
///
/// `connect_and_run` leaves through many paths — fatal closes, `?` on a
/// failed outbound send, normal reconnects — and every one must stop the
/// heartbeat and outbound tasks of the connection being abandoned.
/// Tying the abort to scope exit makes that unconditional instead of
/// relying on each path to remember it.
struct AbortOnDrop(Option<tokio::task::JoinHandle<()>>);

impl AbortOnDrop {
    fn new(handle: tokio::task::JoinHandle<()>) -> Self {
        Self(Some(handle))
    }

    /// Borrows the handle so the caller can await it while the guard stays
    /// armed. Never hand the handle out by value: a bare `JoinHandle`
    /// dropped by a cancelled await detaches the task instead of stopping it.
    fn handle_mut(&mut self) -> Option<&mut tokio::task::JoinHandle<()>> {
        self.0.as_mut()
    }
}

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        if let Some(handle) = self.0.take() {
            handle.abort();
        }
    }
}

/// Maps a gateway close frame to the next connection step, following
/// Discord's close-code table: terminal codes stop the shard, 4007 (invalid
/// sequence) and 4009 (session timed out) require a fresh IDENTIFY because a
/// RESUME is guaranteed to fail, and everything else resumes.
fn close_frame_action(frame: Option<CloseFrame>) -> ReconnectAction {
    let code = frame.as_ref().map(|frame| u16::from(frame.code));
    if is_terminal_close_frame(frame.as_ref()) {
        ReconnectAction::Fatal(crate::error::DiscordError::gateway(terminal_close_error(
            frame,
        )))
    } else if matches!(code, Some(CLOSE_INVALID_SEQ | CLOSE_SESSION_TIMED_OUT)) {
        ReconnectAction::Reconnect
    } else {
        ReconnectAction::Resume
    }
}

fn is_presence_command(command: &GatewayCommand) -> bool {
    matches!(
        command,
        GatewayCommand::UpdatePresence(_) | GatewayCommand::UpdatePresenceData(_)
    )
}

/// Variant name for logs; never the payload, which can carry credentials.
fn gateway_command_kind(command: &GatewayCommand) -> &'static str {
    match command {
        GatewayCommand::Shutdown => "Shutdown",
        GatewayCommand::Reconnect => "Reconnect",
        GatewayCommand::UpdatePresence(_) => "UpdatePresence",
        GatewayCommand::UpdatePresenceData(_) => "UpdatePresenceData",
        GatewayCommand::RequestGuildMembers(_) => "RequestGuildMembers",
        GatewayCommand::RequestChannelInfo(_) => "RequestChannelInfo",
        GatewayCommand::SendPayload(_) => "SendPayload",
    }
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
    guild_id: GuildId,
    channel_id: Option<ChannelId>,
    self_mute: bool,
    self_deaf: bool,
) -> Value {
    serde_json::json!({
        "op": OP_VOICE_STATE_UPDATE,
        "d": {
            "guild_id": guild_id,
            "channel_id": channel_id,
            "self_mute": self_mute,
            "self_deaf": self_deaf
        }
    })
}

pub(crate) fn update_presence_payload(presence: &UpdatePresence) -> Value {
    serde_json::json!({
        "op": OP_PRESENCE_UPDATE,
        "d": presence
    })
}

pub(crate) fn request_guild_members_payload(request: &RequestGuildMembers) -> Value {
    serde_json::json!({
        "op": OP_REQUEST_GUILD_MEMBERS,
        "d": request
    })
}

pub(crate) fn request_soundboard_sounds_payload(
    guild_ids: Vec<GuildId>,
    channels: Option<HashMap<GuildId, Vec<ChannelId>>>,
) -> Value {
    serde_json::json!({
        "op": OP_REQUEST_SOUNDBOARD_SOUNDS,
        "d": {
            "guild_ids": guild_ids,
            "channels": channels
        }
    })
}

pub(crate) fn request_channel_info_payload(request: &RequestChannelInfo) -> Value {
    serde_json::json!({
        "op": OP_REQUEST_CHANNEL_INFO,
        "d": request
    })
}

fn rand_jitter() -> f64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        // Saturating only from the year 2554 on; any value seeds the mix.
        .map(|duration| u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX))
        .unwrap_or(0);
    let counter = JITTER_COUNTER.fetch_add(0x9e37_79b9_7f4a_7c15, Ordering::Relaxed);
    let pid = u64::from(std::process::id());
    let mut mixed = nanos ^ counter.rotate_left(17) ^ pid.rotate_left(32);

    mixed ^= mixed >> 30;
    mixed = mixed.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    mixed ^= mixed >> 27;
    mixed = mixed.wrapping_mul(0x94d0_49bb_1331_11eb);
    mixed ^= mixed >> 31;

    // The top 52 bits become the mantissa of a float in [1, 2), so the
    // result is uniform over [0, 1) without an integer-to-float cast.
    f64::from_bits(ONE_F64_BITS | (mixed >> 12)) - 1.0
}

/// Bit pattern of `1.0_f64`: sign 0, biased exponent 1023, zero mantissa.
const ONE_F64_BITS: u64 = 0x3ff0_0000_0000_0000;

fn initial_heartbeat_delay(heartbeat_interval_ms: u64, jitter_factor: f64) -> Duration {
    // `mul_f64` panics on NaN and negative factors; `clamp` would pass NaN
    // through.
    if jitter_factor.is_nan() || jitter_factor <= 0.0 {
        return Duration::ZERO;
    }
    Duration::from_millis(heartbeat_interval_ms).mul_f64(jitter_factor.min(1.0))
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
    capabilities: u64,
    shard_info: Option<[u32; 2]>,
    presence: Option<&UpdatePresence>,
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
    if capabilities != 0 {
        identify["d"]["capabilities"] = serde_json::json!(capabilities);
    }
    if payload_compression {
        identify["d"]["compress"] = serde_json::json!(true);
    }
    if let Some(shard_info) = shard_info {
        identify["d"]["shard"] = serde_json::json!(shard_info);
    }
    if let Some(presence) = presence {
        identify["d"]["presence"] = serde_json::json!(presence);
    }
    identify
}

fn is_terminal_close_frame(frame: Option<&CloseFrame>) -> bool {
    frame
        .map(|frame| is_terminal_close_code(u16::from(frame.code)))
        .unwrap_or(false)
}

fn is_terminal_close_code(code: u16) -> bool {
    matches!(
        code,
        CLOSE_AUTHENTICATION_FAILED
            | CLOSE_INVALID_SHARD
            | CLOSE_SHARDING_REQUIRED
            | CLOSE_INVALID_API_VERSION
            | CLOSE_INVALID_INTENTS
            | CLOSE_DISALLOWED_INTENTS
    )
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
        if self.command_rx.is_none() {
            sleep(duration).await;
            return Ok(false);
        }

        self.bound_deferred_commands();
        let deadline = Instant::now() + duration;
        loop {
            let now = Instant::now();
            if now >= deadline {
                return Ok(false);
            }

            let received = match self.command_rx.as_mut() {
                Some(command_rx) => timeout(deadline - now, command_rx.recv()).await,
                None => return Ok(false),
            };
            match received {
                Ok(Some(GatewayCommand::Shutdown)) => return Ok(true),
                Ok(Some(GatewayCommand::Reconnect)) => {
                    self.session_id = None;
                    self.resume_gateway_url = None;
                    self.sequence.store(0, Ordering::Relaxed);
                    return Ok(false);
                }
                Ok(Some(command)) => self.defer_command(command),
                Ok(None) | Err(_) => return Ok(false),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::model::test_id;
    use std::collections::{BTreeMap, VecDeque};
    use std::io::Write;
    use std::pin::Pin;
    use std::sync::{Arc, Mutex};
    use std::task::{Context, Poll};
    use std::time::Duration;

    #[cfg(feature = "zstd-stream")]
    use super::super::compression::GatewayCompressionDecoder;
    use super::super::compression::{byte_delta, GatewayZlibStream, ZLIB_SUFFIX};
    use super::super::outbound::{
        classify_gateway_command, run_gateway_outbound_worker, GatewayCommandClass,
        GatewayOutboundLimiter, GatewayOutboundMessage, GATEWAY_COMMAND_MIN_SPACING,
        PRESENCE_UPDATE_LIMIT, PRESENCE_UPDATE_WINDOW,
    };
    use super::{
        close_frame_action, identify_payload, initial_heartbeat_delay, is_terminal_close_code,
        is_terminal_close_frame, rand_jitter, recv_control_command, request_channel_info_payload,
        request_guild_members_payload, resume_payload, terminal_close_error,
        update_presence_payload, voice_state_update_payload, AbortOnDrop, EventCallback,
        GatewayClient, GatewayCommand, ReconnectAction, GATEWAY_COMMAND_QUEUE_CAPACITY,
    };
    use crate::model::{RequestChannelInfo, RequestGuildMembers, UpdatePresence};
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
        let payload = voice_state_update_payload(test_id(1), Some(test_id(2)), false, true);

        assert_eq!(payload["op"], serde_json::json!(4));
        assert_eq!(payload["d"]["guild_id"], serde_json::json!("1"));
        assert_eq!(payload["d"]["channel_id"], serde_json::json!("2"));
        assert_eq!(payload["d"]["self_mute"], serde_json::json!(false));
        assert_eq!(payload["d"]["self_deaf"], serde_json::json!(true));
    }

    #[test]
    fn gateway_typed_presence_and_member_request_payloads_match_gateway_shape() {
        let presence = UpdatePresence::online_with_activity("busy");
        let payload = update_presence_payload(&presence);
        assert_eq!(payload["op"], serde_json::json!(3));
        assert_eq!(payload["d"]["status"], serde_json::json!("online"));
        assert_eq!(
            payload["d"]["activities"][0]["name"],
            serde_json::json!("busy")
        );

        let payload = request_guild_members_payload(&RequestGuildMembers {
            guild_id: test_id(1),
            query: Some("abc".to_string()),
            limit: Some(10),
            presences: Some(true),
            user_ids: Some(vec![test_id(2)]),
            nonce: Some("nonce".to_string()),
        });
        assert_eq!(payload["op"], serde_json::json!(8));
        assert_eq!(payload["d"]["guild_id"], serde_json::json!("1"));
        assert_eq!(payload["d"]["user_ids"], serde_json::json!(["2"]));
        assert_eq!(payload["d"]["nonce"], serde_json::json!("nonce"));

        let payload = request_channel_info_payload(&RequestChannelInfo::voice_metadata(test_id(1)));
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
                guild_id: test_id(1),
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
                RequestChannelInfo::new(test_id(1), ["status"])
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
    fn zlib_byte_delta_rejects_a_counter_that_moved_backwards() {
        assert_eq!(byte_delta(10, 4), Ok(6));
        assert!(byte_delta(4, 10).is_err());
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
        assert_eq!(initial_heartbeat_delay(1_000, f64::NAN), Duration::ZERO);
        assert_eq!(initial_heartbeat_delay(1_000, -0.5), Duration::ZERO);
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
        let identify = identify_payload("secret-token", 513, 0, Some([2, 4]), None, true);
        let resume = resume_payload("secret-token", "session", 42);

        assert_eq!(identify["d"]["token"], serde_json::json!("secret-token"));
        assert_eq!(resume["d"]["token"], serde_json::json!("secret-token"));
    }

    #[test]
    fn identify_without_shard_and_resume_without_sequence_keep_expected_shape() {
        let identify = identify_payload("secret-token", 513, 0, None, None, true);
        let resume = resume_payload("secret-token", "session", 0);

        assert!(identify["d"].get("shard").is_none());
        assert_eq!(identify["d"]["intents"], serde_json::json!(513));
        assert_eq!(identify["d"]["compress"], serde_json::json!(true));
        assert_eq!(resume["d"]["session_id"], serde_json::json!("session"));
        assert!(resume["d"]["seq"].is_null());
    }

    #[test]
    fn identify_payload_omits_payload_compression_for_transport_compression() {
        let identify = identify_payload("secret-token", 513, 0, None, None, false);

        assert!(identify["d"].get("compress").is_none());
    }

    #[test]
    fn identify_payload_includes_initial_presence_when_provided() {
        let presence = crate::model::UpdatePresence::online_with_activity("hello");
        let identify = identify_payload("secret-token", 513, 0, None, Some(&presence), false);

        assert_eq!(
            identify["d"]["presence"]["status"],
            serde_json::json!("online")
        );
        assert_eq!(
            identify["d"]["presence"]["activities"][0]["name"],
            serde_json::json!("hello")
        );
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

    #[tokio::test]
    async fn run_stops_on_terminal_close_code_instead_of_reconnecting() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(stream).await.unwrap();
            ws.send(WsMessage::Text(
                serde_json::json!({ "op": 10, "d": { "heartbeat_interval": 60_000 } })
                    .to_string()
                    .into(),
            ))
            .await
            .unwrap();
            let _identify = ws.next().await;
            ws.send(WsMessage::Close(Some(CloseFrame {
                code: CloseCode::from(4004),
                reason: "Authentication failed.".into(),
            })))
            .await
            .unwrap();
            // A reconnecting client would dial again; give it the chance so
            // the assertion below can catch a regression.
            tokio::time::timeout(Duration::from_millis(300), listener.accept())
                .await
                .is_ok()
        });

        let mut client = GatewayClient::new("bad-token".into(), 513)
            .gateway_config(GatewayConnectionConfig::new(format!("ws://{address}/")));
        let error = tokio::time::timeout(Duration::from_secs(5), client.run(Arc::new(|_, _| {})))
            .await
            .expect("run must return instead of retrying a terminal close")
            .expect_err("a terminal close code is an error");

        assert!(error.to_string().contains("4004"), "{error}");
        assert!(
            !server.await.unwrap(),
            "the client must not reconnect after a terminal close code"
        );
    }

    #[tokio::test]
    async fn run_stops_on_terminal_close_received_before_hello() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(stream).await.unwrap();
            ws.send(WsMessage::Close(Some(CloseFrame {
                code: CloseCode::from(4012),
                reason: "Invalid API version.".into(),
            })))
            .await
            .unwrap();
            tokio::time::timeout(Duration::from_millis(300), listener.accept())
                .await
                .is_ok()
        });

        let mut client = GatewayClient::new("token".into(), 513)
            .gateway_config(GatewayConnectionConfig::new(format!("ws://{address}/")));
        let error = tokio::time::timeout(Duration::from_secs(5), client.run(Arc::new(|_, _| {})))
            .await
            .expect("run must return on a pre-HELLO terminal close")
            .expect_err("4012 is not reconnectable");
        assert!(error.to_string().contains("4012"), "{error}");
        assert!(
            !server.await.unwrap(),
            "no reconnect after a terminal close"
        );
    }

    #[tokio::test]
    async fn abort_guard_stops_task_when_awaiting_future_is_cancelled() {
        // Models the shutdown flush: the caller awaits the worker through
        // the guard and is cancelled mid-wait. The worker must be aborted,
        // not detached.
        struct DropFlag(Arc<std::sync::atomic::AtomicBool>);
        impl Drop for DropFlag {
            fn drop(&mut self) {
                self.0.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        }

        let dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = DropFlag(Arc::clone(&dropped));
        let mut guard = AbortOnDrop::new(tokio::spawn(async move {
            let _flag = flag;
            std::future::pending::<()>().await;
        }));

        let waiter = async move {
            if let Some(handle) = guard.handle_mut() {
                let _ = tokio::time::timeout(Duration::from_secs(60), handle).await;
            }
        };
        // Cancel the waiter before its timeout elapses.
        let _ = tokio::time::timeout(Duration::from_millis(20), waiter).await;

        for _ in 0..50 {
            if dropped.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(
            dropped.load(std::sync::atomic::Ordering::SeqCst),
            "the worker must be aborted when its waiter is cancelled"
        );
    }

    #[test]
    fn identify_payload_sends_capabilities_only_when_set() {
        let without = identify_payload("token", 513, 0, None, None, false);
        assert!(without["d"].get("capabilities").is_none());

        let capability = crate::bitfield::gateway_capabilities::CHANNEL_OBFUSCATION.bits();
        let with = identify_payload("token", 513, capability, None, None, false);
        assert_eq!(with["d"]["capabilities"], serde_json::json!(1 << 15));

        let config = GatewayConnectionConfig::default()
            .capabilities(crate::bitfield::gateway_capabilities::CHANNEL_OBFUSCATION);
        assert_eq!(config.capability_bits(), 1 << 15);
        assert_eq!(
            config
                .clone()
                .with_base_url("wss://resume.example")
                .capability_bits(),
            1 << 15,
            "resume URLs must keep the IDENTIFY capabilities"
        );
    }

    #[test]
    fn close_frame_action_follows_discord_close_code_table() {
        let frame = |code: u16| {
            Some(CloseFrame {
                code: CloseCode::from(code),
                reason: "".into(),
            })
        };
        for code in [4004_u16, 4010, 4011, 4012, 4013, 4014] {
            assert!(
                matches!(close_frame_action(frame(code)), ReconnectAction::Fatal(_)),
                "{code}"
            );
        }
        for code in [4007_u16, 4009] {
            assert!(
                matches!(close_frame_action(frame(code)), ReconnectAction::Reconnect),
                "{code} requires a fresh session"
            );
        }
        for code in [1000_u16, 1001, 4000, 4001, 4002, 4003, 4005, 4008] {
            assert!(
                matches!(close_frame_action(frame(code)), ReconnectAction::Resume),
                "{code}"
            );
        }
        assert!(matches!(close_frame_action(None), ReconnectAction::Resume));
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

    // -- IDENTIFY gate ---------------------------------------------------

    /// Gate that grants immediately (or refuses) and records every acquire
    /// and every reported outcome.
    #[derive(Default)]
    struct RecordingGate {
        refuse: bool,
        acquired: Mutex<Vec<(u32, u32)>>,
        outcomes: Arc<Mutex<Vec<crate::ws::IdentifyOutcome>>>,
    }

    impl crate::ws::IdentifyGate for RecordingGate {
        fn acquire(&self, shard_id: u32, shard_count: u32) -> crate::ws::IdentifyPermitFuture<'_> {
            self.acquired.lock().unwrap().push((shard_id, shard_count));
            let outcomes = Arc::clone(&self.outcomes);
            let refuse = self.refuse;
            Box::pin(async move {
                if refuse {
                    return Err(crate::error::DiscordError::gateway("gate closed"));
                }
                Ok(crate::ws::IdentifyPermit::new(move |outcome| {
                    outcomes.lock().unwrap().push(outcome)
                }))
            })
        }
    }

    fn gated_client(gate: &Arc<RecordingGate>) -> GatewayClient {
        let gate: Arc<dyn crate::ws::IdentifyGate> = gate.clone();
        GatewayClient::new("secret-token".into(), 513)
            .gateway_config(GatewayConnectionConfig::default().identify_gate(gate))
            .shard(1, 2)
    }

    /// Serves one connection: HELLO, then returns the first client payload
    /// and closes. `ready` adds a READY with a session id before closing.
    async fn serve_one_connection(listener: &TcpListener, ready: bool) -> serde_json::Value {
        let (stream, _) = listener.accept().await.unwrap();
        let mut ws = accept_async(stream).await.unwrap();
        ws.send(WsMessage::Text(
            serde_json::json!({"op": 10, "d": {"heartbeat_interval": 60_000}})
                .to_string()
                .into(),
        ))
        .await
        .unwrap();
        let payload: serde_json::Value =
            serde_json::from_str(&ws.next().await.unwrap().unwrap().into_text().unwrap()).unwrap();
        if ready {
            ws.send(WsMessage::Text(
                serde_json::json!({
                    "op": 0,
                    "t": "READY",
                    "s": 1,
                    "d": {
                        "user": {"id": "1", "username": "discordrs"},
                        "session_id": "session-gated",
                        "resume_gateway_url": "wss://gateway.discord.gg"
                    }
                })
                .to_string()
                .into(),
            ))
            .await
            .unwrap();
        }
        let _ = ws.close(None).await;
        payload
    }

    #[tokio::test]
    async fn gated_client_takes_one_permit_per_fresh_identify_and_none_for_resume() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let gate = Arc::new(RecordingGate::default());
        let mut client = gated_client(&gate);

        // Fresh session: one permit, reported as sent after the write.
        let server = serve_one_connection(&listener, true);
        let (sent, action) =
            tokio::join!(server, client.connect_and_run(&url, Arc::new(|_, _| {})));
        assert_eq!(sent["op"], serde_json::json!(2));
        assert!(matches!(action.unwrap(), ReconnectAction::Resume));
        assert_eq!(*gate.acquired.lock().unwrap(), vec![(1, 2)]);
        assert_eq!(
            *gate.outcomes.lock().unwrap(),
            vec![crate::ws::IdentifyOutcome::Sent]
        );

        // Resume: no permit.
        let server = serve_one_connection(&listener, false);
        let (sent, _) = tokio::join!(server, client.connect_and_run(&url, Arc::new(|_, _| {})));
        assert_eq!(sent["op"], serde_json::json!(6));
        assert_eq!(gate.acquired.lock().unwrap().len(), 1);

        // A later fresh session (e.g. after INVALID_SESSION) takes a new
        // permit, and the gate replaces the client's 5s per-shard sleep.
        client.session_id = None;
        let started = Instant::now();
        let server = serve_one_connection(&listener, false);
        let (sent, _) = tokio::join!(server, client.connect_and_run(&url, Arc::new(|_, _| {})));
        assert_eq!(sent["op"], serde_json::json!(2));
        assert!(started.elapsed() < Duration::from_secs(4));
        assert_eq!(gate.acquired.lock().unwrap().len(), 2);
        assert_eq!(gate.outcomes.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn gated_attempt_that_fails_before_identify_is_abandoned() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut ws = accept_async(stream).await.unwrap();
            let _ = ws.close(None).await;
        });
        let gate = Arc::new(RecordingGate::default());
        let mut client = gated_client(&gate);
        assert!(client
            .connect_and_run(&url, Arc::new(|_, _| {}))
            .await
            .is_err());
        server.await.unwrap();
        assert_eq!(
            *gate.outcomes.lock().unwrap(),
            vec![crate::ws::IdentifyOutcome::Abandoned]
        );
    }

    /// Gate that never grants, like one waiting for a daily budget reset.
    struct PendingGate;

    impl crate::ws::IdentifyGate for PendingGate {
        fn acquire(
            &self,
            _shard_id: u32,
            _shard_count: u32,
        ) -> crate::ws::IdentifyPermitFuture<'_> {
            Box::pin(std::future::pending())
        }
    }

    /// Regression: the client awaited the gate without serving its control
    /// channel, so `Shutdown` could wait until the next budget reset.
    #[tokio::test]
    async fn shutdown_ends_a_pending_identify_gate_wait() {
        let (command_tx, command_rx) = mpsc::channel(GATEWAY_COMMAND_QUEUE_CAPACITY);
        let gate: Arc<dyn crate::ws::IdentifyGate> = Arc::new(PendingGate);
        let mut client = GatewayClient::new("secret-token".into(), 513)
            .gateway_config(GatewayConnectionConfig::default().identify_gate(gate))
            .control(command_rx);
        command_tx
            .send(GatewayCommand::UpdatePresence("idle".into()))
            .await
            .unwrap();
        command_tx.send(GatewayCommand::Reconnect).await.unwrap();
        command_tx.send(GatewayCommand::Shutdown).await.unwrap();

        // Nothing listens on this URL: the gate is never passed, so the
        // client must not dial.
        let result = tokio::time::timeout(Duration::from_secs(5), client.run(Arc::new(|_, _| {})))
            .await
            .expect("Shutdown must end the wait for an IDENTIFY permit");
        assert!(result.is_ok());
        // Other commands are kept for the connection, not lost.
        assert!(matches!(
            client.deferred_commands.front(),
            Some(GatewayCommand::UpdatePresence(status)) if status == "idle"
        ));
        assert_eq!(client.deferred_commands.len(), 1);
    }

    /// Regression: commands that arrived during a long permit wait were
    /// drained into an unbounded backlog.
    #[tokio::test]
    async fn pending_identify_gate_bounds_and_coalesces_deferred_commands() {
        let (command_tx, command_rx) = mpsc::channel(GATEWAY_COMMAND_QUEUE_CAPACITY);
        let gate: Arc<dyn crate::ws::IdentifyGate> = Arc::new(PendingGate);
        let mut client = GatewayClient::new("secret-token".into(), 513)
            .gateway_config(GatewayConnectionConfig::default().identify_gate(gate))
            .control(command_rx);
        let extra = 8;
        command_tx
            .send(GatewayCommand::UpdatePresence("idle".into()))
            .await
            .unwrap();
        for index in 0..(super::MAX_DEFERRED_COMMANDS + extra) {
            command_tx
                .send(GatewayCommand::SendPayload(
                    serde_json::json!({ "n": index }),
                ))
                .await
                .unwrap();
            if index == 3 {
                command_tx
                    .send(GatewayCommand::UpdatePresenceData(
                        UpdatePresence::online_with_activity("busy"),
                    ))
                    .await
                    .unwrap();
            }
        }
        command_tx
            .send(GatewayCommand::UpdatePresence("dnd".into()))
            .await
            .unwrap();
        command_tx.send(GatewayCommand::Shutdown).await.unwrap();

        let result = tokio::time::timeout(Duration::from_secs(5), client.run(Arc::new(|_, _| {})))
            .await
            .expect("Shutdown must still end the wait");
        assert!(result.is_ok());

        // One presence update survives: the newest.
        let presences: Vec<&GatewayCommand> = client
            .deferred_commands
            .iter()
            .filter(|command| {
                matches!(
                    command,
                    GatewayCommand::UpdatePresence(_) | GatewayCommand::UpdatePresenceData(_)
                )
            })
            .collect();
        assert_eq!(presences.len(), 1);
        assert!(matches!(presences[0], GatewayCommand::UpdatePresence(status) if status == "dnd"));
        // Other commands are capped, keeping the newest.
        let payloads: Vec<u64> = client
            .deferred_commands
            .iter()
            .filter_map(|command| match command {
                GatewayCommand::SendPayload(payload) => payload["n"].as_u64(),
                _ => None,
            })
            .collect();
        let expected: Vec<u64> =
            (extra as u64..(super::MAX_DEFERRED_COMMANDS + extra) as u64).collect();
        assert_eq!(payloads, expected);
    }

    fn payload_numbers(client: &GatewayClient) -> Vec<u64> {
        client
            .deferred_commands
            .iter()
            .filter_map(|command| match command {
                GatewayCommand::SendPayload(payload) => payload["n"].as_u64(),
                _ => None,
            })
            .collect()
    }

    fn presence_statuses(client: &GatewayClient) -> Vec<String> {
        client
            .deferred_commands
            .iter()
            .filter_map(|command| match command {
                GatewayCommand::UpdatePresence(status) => Some(status.clone()),
                GatewayCommand::UpdatePresenceData(_) => Some("data".to_string()),
                _ => None,
            })
            .collect()
    }

    /// Regression: a backlog that was already oversized (e.g. from a long
    /// reconnect backoff) stayed oversized during the permit wait; each new
    /// command only replaced one old one.
    #[tokio::test]
    async fn oversized_backlog_is_normalized_on_entering_the_gate_wait() {
        let (command_tx, command_rx) = mpsc::channel(GATEWAY_COMMAND_QUEUE_CAPACITY);
        let gate: Arc<dyn crate::ws::IdentifyGate> = Arc::new(PendingGate);
        let mut client = GatewayClient::new("secret-token".into(), 513)
            .gateway_config(GatewayConnectionConfig::default().identify_gate(gate))
            .control(command_rx);
        for index in 0..100_u64 {
            if index % 40 == 0 {
                client
                    .deferred_commands
                    .push_back(GatewayCommand::UpdatePresence(format!("p{index}")));
            }
            client
                .deferred_commands
                .push_back(GatewayCommand::SendPayload(
                    serde_json::json!({ "n": index }),
                ));
        }
        command_tx.send(GatewayCommand::Shutdown).await.unwrap();

        let result = tokio::time::timeout(Duration::from_secs(5), client.run(Arc::new(|_, _| {})))
            .await
            .expect("Shutdown must end the wait");
        assert!(result.is_ok());
        assert_eq!(presence_statuses(&client), vec!["p80".to_string()]);
        let max = super::MAX_DEFERRED_COMMANDS as u64;
        assert_eq!(
            payload_numbers(&client),
            (100 - max..100).collect::<Vec<_>>()
        );
    }

    /// Regression: commands received during reconnect backoff were appended
    /// without any bound.
    #[tokio::test]
    async fn reconnect_backoff_keeps_the_deferred_backlog_bounded() {
        let (command_tx, command_rx) = mpsc::channel(GATEWAY_COMMAND_QUEUE_CAPACITY);
        let mut client = GatewayClient::new("secret-token".into(), 513).control(command_rx);
        for index in 0..50_u64 {
            client
                .deferred_commands
                .push_back(GatewayCommand::SendPayload(
                    serde_json::json!({ "n": index }),
                ));
        }
        for index in 50..90_u64 {
            command_tx
                .send(GatewayCommand::SendPayload(
                    serde_json::json!({ "n": index }),
                ))
                .await
                .unwrap();
        }
        command_tx
            .send(GatewayCommand::UpdatePresence("idle".into()))
            .await
            .unwrap();
        command_tx
            .send(GatewayCommand::UpdatePresence("dnd".into()))
            .await
            .unwrap();

        let shutdown = client
            .wait_for_backoff_command(Duration::from_millis(100))
            .await
            .unwrap();
        assert!(!shutdown);
        assert_eq!(presence_statuses(&client), vec!["dnd".to_string()]);
        let max = super::MAX_DEFERRED_COMMANDS as u64;
        assert_eq!(payload_numbers(&client), (90 - max..90).collect::<Vec<_>>());
    }

    #[test]
    fn dropped_command_report_names_kinds_not_payloads() {
        let mut client = GatewayClient::new("secret-token".into(), 513);
        client
            .deferred_commands
            .push_back(GatewayCommand::SendPayload(serde_json::json!({
                "token": "do-not-log-me"
            })));
        for index in 0..super::MAX_DEFERRED_COMMANDS {
            client
                .deferred_commands
                .push_back(GatewayCommand::SendPayload(
                    serde_json::json!({ "n": index }),
                ));
        }
        let dropped = client.bound_deferred_commands();
        assert_eq!(dropped, BTreeMap::from([("SendPayload", 1)]));
        assert!(!format!("{dropped:?}").contains("do-not-log-me"));
        assert_eq!(client.deferred_commands.len(), super::MAX_DEFERRED_COMMANDS);
        // Every variant has a fixed, payload-free name.
        for command in [
            GatewayCommand::Shutdown,
            GatewayCommand::Reconnect,
            GatewayCommand::UpdatePresence("x".into()),
            GatewayCommand::SendPayload(serde_json::json!({"token": "do-not-log-me"})),
        ] {
            assert!(!super::gateway_command_kind(&command).contains("do-not-log-me"));
        }
    }

    /// Regression: only the TCP/TLS connect was bounded while a permit was
    /// held; an upgraded socket that never sent HELLO kept the bucket until
    /// the parent's kill watchdog fired.
    #[tokio::test]
    async fn gated_setup_gives_up_when_hello_never_arrives() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let _ws = accept_async(stream).await.unwrap();
            // Upgraded, but silent: no HELLO until the test is done.
            let _ = release_rx.await;
        });
        let gate = Arc::new(RecordingGate::default());
        let mut client = gated_client(&gate);
        client.gated_setup_timeout = Duration::from_millis(200);

        let started = Instant::now();
        let error = tokio::time::timeout(
            Duration::from_secs(5),
            client.connect_and_run(&url, Arc::new(|_, _| {})),
        )
        .await
        .expect("a silent server must not hold the permit")
        .err()
        .expect("the attempt must fail");
        assert!(error.to_string().contains("HELLO"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(2));
        assert_eq!(
            *gate.outcomes.lock().unwrap(),
            vec![crate::ws::IdentifyOutcome::Abandoned]
        );
        let _ = release_tx.send(());
        server.await.unwrap();
    }

    #[tokio::test]
    async fn identify_gate_refusal_stops_the_shard_without_dialing() {
        let gate = Arc::new(RecordingGate {
            refuse: true,
            ..RecordingGate::default()
        });
        let mut client = gated_client(&gate);
        // Nothing listens on this address; a dial attempt would fail with a
        // connection error and back off instead of returning at once.
        let result = tokio::time::timeout(Duration::from_secs(5), client.run(Arc::new(|_, _| {})))
            .await
            .expect("a refused permit must stop the shard immediately");
        assert!(result.unwrap_err().to_string().contains("gate closed"));
        assert_eq!(*gate.acquired.lock().unwrap(), vec![(1, 2)]);
    }
}
