//! Multi-process shard management (discord.js `ShardingManager` equivalent).
//!
//! A [`ProcessShardManager`] spawns one child process per shard group, keeps
//! each child alive (respawning it with exponential backoff when it exits
//! unexpectedly), and talks to the children over a JSON-lines IPC protocol on
//! the children's stdin/stdout pipes. The child half of the protocol is
//! [`ShardChildProcess`], which the same binary can construct via
//! [`ShardChildProcess::from_env`] so that a single executable acts as both
//! the manager (parent) and the shard runner (child) — exactly like
//! discord.js's `ShardingManager` + `process.send` pattern.
//!
//! # IMPORTANT: the child's stdout is the IPC channel
//!
//! When a process runs as a shard child, **its stdout belongs to the IPC
//! protocol**. Anything else written to stdout (a stray `println!`, a
//! tracing subscriber writing to stdout, ...) corrupts the message stream.
//! All user logging in a child process **must go to stderr** (which the
//! parent leaves untouched, so child stderr shows up in the parent's
//! terminal). Configure your tracing subscriber with a stderr writer in
//! child mode; see `examples/process_sharding_bot.rs`.
//!
//! # Protocol
//!
//! One JSON object per line:
//!
//! - parent → child: `{"op":"shutdown"}`,
//!   `{"op":"request","id":N,"payload":...}`
//! - child → parent: `{"op":"ready","shard_id":N}`,
//!   `{"op":"response","id":N,"payload":...}`,
//!   `{"op":"log","message":"..."}`

use std::collections::HashMap;
use std::future::Future;
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex, MutexGuard};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::{sleep, timeout, Instant};
use tracing::{info, warn};

use crate::error::DiscordError;
use crate::http::RestClient;

/// Environment variable holding the comma-separated shard id list assigned to
/// a child process (e.g. `"0,1"`).
pub const ENV_SHARD_IDS: &str = "DISCORDRS_SHARD_IDS";
/// Environment variable holding the total shard count.
pub const ENV_SHARD_COUNT: &str = "DISCORDRS_SHARD_COUNT";
/// Environment variable set to `1` when the process is a shard child whose
/// stdin/stdout carry the IPC protocol.
pub const ENV_SHARD_IPC: &str = "DISCORDRS_SHARD_IPC";

/// Per-child response deadline used by [`ProcessShardManager::broadcast`].
const BROADCAST_TIMEOUT: Duration = Duration::from_secs(10);
/// How long [`ProcessShardManager::shutdown`] waits for a child to exit after
/// the `shutdown` op before killing it.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(10);
/// Backoff ceiling for respawn delays.
const MAX_RESPAWN_DELAY: Duration = Duration::from_secs(5 * 60);
/// A child that stays alive at least this long resets the backoff schedule.
const BACKOFF_RESET_UPTIME: Duration = Duration::from_secs(60);

// ---------------------------------------------------------------------------
// Protocol
// ---------------------------------------------------------------------------

/// IPC message sent from the manager (parent) to a shard child, one JSON
/// object per stdin line.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ParentMessage {
    /// Ask the child to shut down gracefully.
    Shutdown,
    /// Broadcast request the child must answer with a
    /// [`ChildMessage::Response`] carrying the same `id`.
    Request {
        /// Correlation id echoed back in the response.
        id: u64,
        /// Arbitrary application payload.
        payload: Value,
    },
}

/// IPC message sent from a shard child to the manager, one JSON object per
/// stdout line.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ChildMessage {
    /// The child finished starting the given shard.
    Ready {
        /// Shard id that became ready.
        shard_id: u32,
    },
    /// Answer to a [`ParentMessage::Request`] with the same `id`.
    Response {
        /// Correlation id copied from the request.
        id: u64,
        /// Arbitrary application payload.
        payload: Value,
    },
    /// Free-form log line the parent surfaces through `tracing`.
    Log {
        /// Log text.
        message: String,
    },
}

/// Encodes an IPC message as a single JSON line (without the trailing
/// newline).
pub fn encode_ipc_line<T: Serialize>(message: &T) -> Result<String, DiscordError> {
    Ok(serde_json::to_string(message)?)
}

/// Decodes one JSON line into an IPC message. Surrounding whitespace is
/// ignored.
pub fn decode_ipc_line<T: for<'de> Deserialize<'de>>(line: &str) -> Result<T, DiscordError> {
    Ok(serde_json::from_str(line.trim())?)
}

// ---------------------------------------------------------------------------
// Backoff
// ---------------------------------------------------------------------------

/// Returns the respawn delay for the given consecutive failure `attempt`
/// (0-based): `base * 2^attempt`, capped at five minutes.
pub fn respawn_backoff(base: Duration, attempt: u32) -> Duration {
    let factor = 2u32.checked_pow(attempt.min(31)).unwrap_or(u32::MAX);
    base.checked_mul(factor)
        .unwrap_or(MAX_RESPAWN_DELAY)
        .min(MAX_RESPAWN_DELAY)
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for [`ProcessShardManager`].
#[derive(Clone, Debug)]
pub struct ProcessShardManagerConfig {
    /// Total shard count. `None` means "ask Discord": use
    /// [`ProcessShardManager::spawn_auto`], which fetches `/gateway/bot`.
    /// [`ProcessShardManager::spawn`] requires `Some`.
    pub total_shards: Option<u32>,
    /// Subset of shard ids this manager is responsible for. `None` means all
    /// of `0..total_shards`.
    pub shard_list: Option<Vec<u32>>,
    /// How many shards each child process runs (default `1`).
    pub shards_per_process: u32,
    /// Whether children are respawned when they exit unexpectedly
    /// (default `true`).
    pub respawn: bool,
    /// Base delay before respawning a child (default 5s). Consecutive fast
    /// failures back off exponentially from this value up to five minutes;
    /// the schedule resets once a child stays alive for 60 seconds.
    pub respawn_delay: Duration,
    /// Maximum number of respawns per child before it is left stopped.
    /// `None` means unlimited.
    pub max_respawns: Option<u32>,
    /// Program to execute for each child. `None` (the default) re-executes
    /// the current binary (`std::env::current_exe()`), the discord.js
    /// single-binary pattern.
    pub program: Option<PathBuf>,
    /// Extra command-line arguments passed to every child.
    pub args: Vec<String>,
    /// Extra environment variables set on every child, in addition to the
    /// inherited environment and the `DISCORDRS_SHARD_*` variables.
    pub extra_env: Vec<(String, String)>,
    /// Delay between consecutive child starts (default 5s), a conservative
    /// stagger for Discord's identify rate limits. Set to
    /// [`Duration::ZERO`] to opt out (e.g. in tests or when children pace
    /// their own IDENTIFY calls).
    pub spawn_stagger: Duration,
}

impl Default for ProcessShardManagerConfig {
    fn default() -> Self {
        Self {
            total_shards: None,
            shard_list: None,
            shards_per_process: 1,
            respawn: true,
            respawn_delay: Duration::from_secs(5),
            max_respawns: None,
            program: None,
            args: Vec::new(),
            extra_env: Vec::new(),
            spawn_stagger: Duration::from_secs(5),
        }
    }
}

impl ProcessShardManagerConfig {
    /// Creates the default configuration.
    pub fn new() -> Self {
        Self::default()
    }
}

// ---------------------------------------------------------------------------
// Child status
// ---------------------------------------------------------------------------

/// Lifecycle state of a managed child process.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChildState {
    /// The child is being (or is about to be) spawned.
    Starting,
    /// At least one shard in the child reported ready.
    Ready,
    /// Every shard in the child reported ready.
    Running,
    /// The child exited and is waiting out the respawn backoff.
    Respawning,
    /// The child is gone and will not be respawned.
    Stopped,
}

/// Point-in-time status snapshot of one managed child process.
#[derive(Clone, Debug)]
pub struct ChildStatus {
    /// OS process id of the live child, if it is currently running.
    pub pid: Option<u32>,
    /// Shard ids assigned to this child.
    pub shard_ids: Vec<u32>,
    /// Current lifecycle state.
    pub state: ChildState,
    /// How many times this child has been respawned.
    pub respawns: u32,
}

fn lock_or_recover<T>(mutex: &StdMutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

// ---------------------------------------------------------------------------
// Manager internals
// ---------------------------------------------------------------------------

enum ChildCommand {
    Request {
        id: u64,
        payload: Value,
        respond_to: oneshot::Sender<Value>,
    },
}

struct ChildEntry {
    shard_ids: Vec<u32>,
    status: Arc<StdMutex<ChildStatus>>,
    command_tx: mpsc::UnboundedSender<ChildCommand>,
}

struct MonitorContext {
    index: usize,
    shard_ids: Vec<u32>,
    status: Arc<StdMutex<ChildStatus>>,
    program: PathBuf,
    args: Vec<String>,
    env: Vec<(String, String)>,
    respawn: bool,
    respawn_delay: Duration,
    max_respawns: Option<u32>,
    start_delay: Duration,
}

/// Multi-process shard manager: spawns one child process per shard group,
/// respawns crashed children with exponential backoff, and provides
/// request/response broadcasting over JSON-lines IPC — the discord.js
/// `ShardingManager` equivalent.
///
/// Children are started with `DISCORDRS_SHARD_IDS`, `DISCORDRS_SHARD_COUNT`,
/// and `DISCORDRS_SHARD_IPC=1` in their environment so the same binary can
/// detect child mode via [`ShardChildProcess::from_env`].
pub struct ProcessShardManager {
    total_shards: u32,
    children: Vec<ChildEntry>,
    shutdown_tx: watch::Sender<bool>,
    tasks: StdMutex<Vec<tokio::task::JoinHandle<()>>>,
    next_request_id: AtomicU64,
}

impl ProcessShardManager {
    /// Starts child processes for every shard group described by `config`.
    ///
    /// `config.total_shards` must be `Some`; use [`Self::spawn_auto`] to let
    /// Discord recommend a shard count. Child starts are staggered by
    /// `config.spawn_stagger` to stay comfortably inside identify limits.
    pub async fn spawn(config: ProcessShardManagerConfig) -> Result<Self, DiscordError> {
        let Some(total_shards) = config.total_shards else {
            return Err(DiscordError::model(
                "ProcessShardManagerConfig.total_shards is required for spawn(); \
                 use ProcessShardManager::spawn_auto to fetch it from /gateway/bot",
            ));
        };
        let total_shards = total_shards.max(1);

        let mut shard_ids: Vec<u32> = match &config.shard_list {
            Some(list) => {
                let mut list = list.clone();
                list.sort_unstable();
                list.dedup();
                list
            }
            None => (0..total_shards).collect(),
        };
        if shard_ids.is_empty() {
            return Err(DiscordError::model("shard_list must not be empty"));
        }
        if let Some(&bad) = shard_ids.iter().find(|&&id| id >= total_shards) {
            return Err(DiscordError::model(format!(
                "shard id {bad} is out of range for total_shards {total_shards}"
            )));
        }
        shard_ids.sort_unstable();

        let program = match &config.program {
            Some(path) => path.clone(),
            None => std::env::current_exe()?,
        };
        let shards_per_process = config.shards_per_process.max(1) as usize;

        let (shutdown_tx, _) = watch::channel(false);
        let mut children = Vec::new();
        let mut tasks = Vec::new();

        for (index, group) in shard_ids.chunks(shards_per_process).enumerate() {
            let group = group.to_vec();
            let status = Arc::new(StdMutex::new(ChildStatus {
                pid: None,
                shard_ids: group.clone(),
                state: ChildState::Starting,
                respawns: 0,
            }));
            let (command_tx, command_rx) = mpsc::unbounded_channel();

            let mut env = vec![
                (
                    ENV_SHARD_IDS.to_string(),
                    group
                        .iter()
                        .map(u32::to_string)
                        .collect::<Vec<_>>()
                        .join(","),
                ),
                (ENV_SHARD_COUNT.to_string(), total_shards.to_string()),
                (ENV_SHARD_IPC.to_string(), "1".to_string()),
            ];
            env.extend(config.extra_env.iter().cloned());

            let context = MonitorContext {
                index,
                shard_ids: group.clone(),
                status: Arc::clone(&status),
                program: program.clone(),
                args: config.args.clone(),
                env,
                respawn: config.respawn,
                respawn_delay: config.respawn_delay,
                max_respawns: config.max_respawns,
                start_delay: config
                    .spawn_stagger
                    .checked_mul(index as u32)
                    .unwrap_or(MAX_RESPAWN_DELAY),
            };

            let shutdown_rx = shutdown_tx.subscribe();
            tasks.push(tokio::spawn(run_child_monitor(
                context,
                command_rx,
                shutdown_rx,
            )));
            children.push(ChildEntry {
                shard_ids: group,
                status,
                command_tx,
            });
        }

        info!(
            total_shards,
            children = children.len(),
            "process shard manager spawned"
        );

        Ok(Self {
            total_shards,
            children,
            shutdown_tx,
            tasks: StdMutex::new(tasks),
            next_request_id: AtomicU64::new(1),
        })
    }

    /// Like [`Self::spawn`], but when `config.total_shards` is `None` it
    /// fetches the recommended shard count from Discord's `/gateway/bot`
    /// endpoint using `token`.
    pub async fn spawn_auto(
        token: impl Into<String>,
        mut config: ProcessShardManagerConfig,
    ) -> Result<Self, DiscordError> {
        if config.total_shards.is_none() {
            let rest = RestClient::new(token, 0);
            let gateway_bot = rest.get_gateway_bot().await?;
            info!(
                shards = gateway_bot.shards,
                "using shard count recommended by /gateway/bot"
            );
            config.total_shards = Some(gateway_bot.shards.max(1));
        }
        Self::spawn(config).await
    }

    /// Total shard count this manager was configured with.
    pub fn shard_count(&self) -> u32 {
        self.total_shards
    }

    /// Status snapshot of every managed child, in shard order.
    pub fn children(&self) -> Vec<ChildStatus> {
        self.children
            .iter()
            .map(|child| lock_or_recover(&child.status).clone())
            .collect()
    }

    /// Sends `payload` to every child as a `request` op and gathers the
    /// responses (the `broadcastEval` / `fetchClientValues` equivalent).
    ///
    /// # Contract
    ///
    /// Always resolves with exactly one `(first_shard_id, value)` entry per
    /// child, keyed by the lowest shard id in that child's group. A child
    /// that does not answer within 10 seconds — because it is down,
    /// respawning, or simply slow — contributes `Value::Null` for its entry;
    /// the call itself only fails on internal errors (currently never), so
    /// callers should treat `Null` entries as "shard unavailable".
    pub async fn broadcast(&self, payload: Value) -> Result<Vec<(u32, Value)>, DiscordError> {
        let mut pending = Vec::with_capacity(self.children.len());
        for child in &self.children {
            let first_shard = child.shard_ids.first().copied().unwrap_or(0);
            let id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
            let (respond_to, response_rx) = oneshot::channel();
            let sent = child
                .command_tx
                .send(ChildCommand::Request {
                    id,
                    payload: payload.clone(),
                    respond_to,
                })
                .is_ok();
            pending.push(async move {
                if !sent {
                    return (first_shard, Value::Null);
                }
                match timeout(BROADCAST_TIMEOUT, response_rx).await {
                    Ok(Ok(value)) => (first_shard, value),
                    Ok(Err(_)) | Err(_) => (first_shard, Value::Null),
                }
            });
        }
        Ok(futures_util::future::join_all(pending).await)
    }

    /// Gracefully stops all children: each receives the `shutdown` op, gets
    /// up to 10 seconds to exit, and is killed afterwards. Resolves once all
    /// monitor tasks have finished.
    pub async fn shutdown(&self) -> Result<(), DiscordError> {
        info!("process shard manager shutting down");
        let _ = self.shutdown_tx.send(true);
        self.wait().await
    }

    /// Waits until every child monitor task has finished (i.e. all children
    /// have exited and no further respawns will happen).
    pub async fn wait(&self) -> Result<(), DiscordError> {
        let tasks = {
            let mut guard = lock_or_recover(&self.tasks);
            std::mem::take(&mut *guard)
        };
        for task in tasks {
            if let Err(error) = task.await {
                warn!("child monitor task failed: {error}");
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Child monitor task
// ---------------------------------------------------------------------------

enum ChildRunOutcome {
    /// The child exited on its own (crash or clean exit).
    Exited,
    /// Shutdown was requested; the child was stopped gracefully.
    ShutdownRequested,
}

/// Resolves once shutdown has been requested; never resolves if the manager
/// is dropped without a shutdown (the command channel closing covers that
/// case separately).
async fn wait_for_shutdown(shutdown_rx: &mut watch::Receiver<bool>) {
    loop {
        if *shutdown_rx.borrow_and_update() {
            return;
        }
        if shutdown_rx.changed().await.is_err() {
            std::future::pending::<()>().await;
        }
    }
}

fn set_state(status: &StdMutex<ChildStatus>, state: ChildState) {
    lock_or_recover(status).state = state;
}

async fn run_child_monitor(
    context: MonitorContext,
    mut command_rx: mpsc::UnboundedReceiver<ChildCommand>,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    if !context.start_delay.is_zero() {
        tokio::select! {
            () = sleep(context.start_delay) => {}
            () = wait_for_shutdown(&mut shutdown_rx) => {
                set_state(&context.status, ChildState::Stopped);
                return;
            }
        }
    }

    let mut backoff_attempt: u32 = 0;
    let mut respawns: u32 = 0;

    loop {
        if *shutdown_rx.borrow() {
            set_state(&context.status, ChildState::Stopped);
            return;
        }

        set_state(&context.status, ChildState::Starting);
        match spawn_child_process(&context) {
            Ok(mut child) => {
                let pid = child.id();
                lock_or_recover(&context.status).pid = pid;
                info!(
                    child = context.index,
                    shard_ids = ?context.shard_ids,
                    pid,
                    "shard child started"
                );

                let started = Instant::now();
                let outcome =
                    supervise_child(&context, &mut child, &mut command_rx, &mut shutdown_rx).await;
                lock_or_recover(&context.status).pid = None;

                match outcome {
                    ChildRunOutcome::ShutdownRequested => {
                        set_state(&context.status, ChildState::Stopped);
                        info!(
                            child = context.index,
                            shard_ids = ?context.shard_ids,
                            "shard child stopped after shutdown request"
                        );
                        return;
                    }
                    ChildRunOutcome::Exited => {
                        warn!(
                            child = context.index,
                            shard_ids = ?context.shard_ids,
                            uptime_secs = started.elapsed().as_secs(),
                            "shard child exited unexpectedly"
                        );
                        if started.elapsed() >= BACKOFF_RESET_UPTIME {
                            backoff_attempt = 0;
                        }
                    }
                }
            }
            Err(error) => {
                warn!(
                    child = context.index,
                    shard_ids = ?context.shard_ids,
                    "failed to spawn shard child: {error}"
                );
            }
        }

        if !context.respawn {
            set_state(&context.status, ChildState::Stopped);
            return;
        }
        if let Some(max) = context.max_respawns {
            if respawns >= max {
                warn!(
                    child = context.index,
                    shard_ids = ?context.shard_ids,
                    max_respawns = max,
                    "shard child reached max respawns; giving up"
                );
                set_state(&context.status, ChildState::Stopped);
                return;
            }
        }

        respawns += 1;
        lock_or_recover(&context.status).respawns = respawns;
        set_state(&context.status, ChildState::Respawning);

        let delay = respawn_backoff(context.respawn_delay, backoff_attempt);
        backoff_attempt = backoff_attempt.saturating_add(1);
        info!(
            child = context.index,
            shard_ids = ?context.shard_ids,
            respawn = respawns,
            delay_ms = delay.as_millis() as u64,
            "respawning shard child after backoff"
        );
        tokio::select! {
            () = sleep(delay) => {}
            () = wait_for_shutdown(&mut shutdown_rx) => {
                set_state(&context.status, ChildState::Stopped);
                return;
            }
        }
    }
}

fn spawn_child_process(context: &MonitorContext) -> Result<Child, DiscordError> {
    let mut command = Command::new(&context.program);
    command
        .args(&context.args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .kill_on_drop(true);
    for (key, value) in &context.env {
        command.env(key, value);
    }
    Ok(command.spawn()?)
}

async fn write_child_line(
    stdin: &mut Option<ChildStdin>,
    message: &ParentMessage,
) -> Result<(), DiscordError> {
    let Some(writer) = stdin.as_mut() else {
        return Err(DiscordError::gateway("child stdin is closed"));
    };
    let mut line = encode_ipc_line(message)?;
    line.push('\n');
    writer
        .write_all(line.as_bytes())
        .await
        .map_err(|error| DiscordError::gateway(format!("child stdin write failed: {error}")))?;
    writer
        .flush()
        .await
        .map_err(|error| DiscordError::gateway(format!("child stdin flush failed: {error}")))
}

/// Runs one child until it exits or shutdown is requested, pumping IPC in
/// both directions.
async fn supervise_child(
    context: &MonitorContext,
    child: &mut Child,
    command_rx: &mut mpsc::UnboundedReceiver<ChildCommand>,
    shutdown_rx: &mut watch::Receiver<bool>,
) -> ChildRunOutcome {
    let mut stdin = child.stdin.take();
    let Some(stdout) = child.stdout.take() else {
        warn!(child = context.index, "child stdout pipe missing");
        let _ = child.wait().await;
        return ChildRunOutcome::Exited;
    };
    let mut lines = BufReader::new(stdout).lines();
    let mut pending: HashMap<u64, oneshot::Sender<Value>> = HashMap::new();
    let mut ready_shards = 0usize;

    loop {
        tokio::select! {
            line = lines.next_line() => {
                match line {
                    Ok(Some(line)) => handle_child_line(
                        context,
                        &line,
                        &mut pending,
                        &mut ready_shards,
                    ),
                    Ok(None) | Err(_) => {
                        // stdout closed: the child exited (or abandoned IPC).
                        let _ = child.wait().await;
                        return ChildRunOutcome::Exited;
                    }
                }
            }
            command = command_rx.recv() => {
                match command {
                    Some(ChildCommand::Request { id, payload, respond_to }) => {
                        if respond_to.is_closed() {
                            continue; // broadcaster already timed out
                        }
                        pending.retain(|_, sender| !sender.is_closed());
                        let message = ParentMessage::Request { id, payload };
                        match write_child_line(&mut stdin, &message).await {
                            Ok(()) => {
                                pending.insert(id, respond_to);
                            }
                            Err(error) => {
                                warn!(
                                    child = context.index,
                                    "failed to forward broadcast request: {error}"
                                );
                                // Dropping respond_to reports Null upstream.
                            }
                        }
                    }
                    None => {
                        // The manager was dropped: do not leave orphans.
                        graceful_stop(context, child, &mut stdin).await;
                        return ChildRunOutcome::ShutdownRequested;
                    }
                }
            }
            () = wait_for_shutdown(shutdown_rx) => {
                graceful_stop(context, child, &mut stdin).await;
                return ChildRunOutcome::ShutdownRequested;
            }
        }
    }
}

fn handle_child_line(
    context: &MonitorContext,
    line: &str,
    pending: &mut HashMap<u64, oneshot::Sender<Value>>,
    ready_shards: &mut usize,
) {
    if line.trim().is_empty() {
        return;
    }
    match decode_ipc_line::<ChildMessage>(line) {
        Ok(ChildMessage::Ready { shard_id }) => {
            *ready_shards += 1;
            let state = if *ready_shards >= context.shard_ids.len() {
                ChildState::Running
            } else {
                ChildState::Ready
            };
            set_state(&context.status, state);
            info!(child = context.index, shard_id, "shard reported ready");
        }
        Ok(ChildMessage::Response { id, payload }) => {
            if let Some(sender) = pending.remove(&id) {
                let _ = sender.send(payload);
            } else {
                warn!(
                    child = context.index,
                    request_id = id,
                    "response for unknown or expired request"
                );
            }
        }
        Ok(ChildMessage::Log { message }) => {
            info!(child = context.index, "child log: {message}");
        }
        Err(error) => {
            warn!(
                child = context.index,
                "unparseable child IPC line ({error}): {line}"
            );
        }
    }
}

/// Sends the `shutdown` op, waits up to [`SHUTDOWN_GRACE`] for the child to
/// exit, then kills it.
async fn graceful_stop(context: &MonitorContext, child: &mut Child, stdin: &mut Option<ChildStdin>) {
    if let Err(error) = write_child_line(stdin, &ParentMessage::Shutdown).await {
        warn!(
            child = context.index,
            "failed to send shutdown op ({error}); killing child"
        );
    }
    // Dropping our stdin handle closes the pipe, which doubles as an EOF
    // shutdown signal for children that stopped reading messages.
    *stdin = None;

    match timeout(SHUTDOWN_GRACE, child.wait()).await {
        Ok(_) => {}
        Err(_) => {
            warn!(
                child = context.index,
                "child did not exit within grace period; killing"
            );
            if let Err(error) = child.kill().await {
                warn!(child = context.index, "failed to kill child: {error}");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Child side
// ---------------------------------------------------------------------------

/// The child-process half of the IPC protocol.
///
/// Construct with [`ShardChildProcess::from_env`]: it returns `Some` only
/// when the process was launched by a [`ProcessShardManager`] (detected via
/// `DISCORDRS_SHARD_IPC=1`), so the same binary runs standalone otherwise.
///
/// # WARNING: stdout is reserved for IPC
///
/// While a [`ShardChildProcess`] is active, **stdout carries the IPC
/// protocol**. Never print to stdout in child mode — send all logging to
/// stderr (e.g. `eprintln!` or a tracing subscriber configured with
/// `std::io::stderr`). A single stray stdout line breaks broadcasts.
pub struct ShardChildProcess {
    shard_ids: Vec<u32>,
    shard_count: u32,
    shutdown_tx: watch::Sender<bool>,
}

impl ShardChildProcess {
    /// Detects child mode from the environment.
    ///
    /// Returns `Some` when `DISCORDRS_SHARD_IPC=1` and the
    /// `DISCORDRS_SHARD_IDS` / `DISCORDRS_SHARD_COUNT` variables parse
    /// cleanly; returns `None` otherwise (including on malformed values,
    /// which are logged to stderr via `tracing`), so a standalone launch of
    /// the same binary falls through to parent/normal mode.
    pub fn from_env() -> Option<Self> {
        if std::env::var(ENV_SHARD_IPC).ok().as_deref() != Some("1") {
            return None;
        }
        let ids_raw = match std::env::var(ENV_SHARD_IDS) {
            Ok(value) => value,
            Err(_) => {
                warn!("{ENV_SHARD_IPC}=1 but {ENV_SHARD_IDS} is missing; ignoring child mode");
                return None;
            }
        };
        let count_raw = match std::env::var(ENV_SHARD_COUNT) {
            Ok(value) => value,
            Err(_) => {
                warn!("{ENV_SHARD_IPC}=1 but {ENV_SHARD_COUNT} is missing; ignoring child mode");
                return None;
            }
        };
        let shard_ids = match ids_raw
            .split(',')
            .map(|part| part.trim().parse::<u32>())
            .collect::<Result<Vec<u32>, _>>()
        {
            Ok(ids) if !ids.is_empty() => ids,
            _ => {
                warn!("malformed {ENV_SHARD_IDS} value {ids_raw:?}; ignoring child mode");
                return None;
            }
        };
        let shard_count = match count_raw.trim().parse::<u32>() {
            Ok(count) if count >= 1 => count,
            _ => {
                warn!("malformed {ENV_SHARD_COUNT} value {count_raw:?}; ignoring child mode");
                return None;
            }
        };

        let (shutdown_tx, _) = watch::channel(false);
        Some(Self {
            shard_ids,
            shard_count,
            shutdown_tx,
        })
    }

    /// Shard ids assigned to this process.
    pub fn shard_ids(&self) -> &[u32] {
        &self.shard_ids
    }

    /// Total shard count across all processes.
    pub fn shard_count(&self) -> u32 {
        self.shard_count
    }

    /// Reports every assigned shard as ready by writing `ready` lines to
    /// stdout. Call this once the gateway connections are up (or as soon as
    /// serving starts, for a coarser signal).
    pub fn notify_ready(&self) -> Result<(), DiscordError> {
        for &shard_id in &self.shard_ids {
            let line = encode_ipc_line(&ChildMessage::Ready { shard_id })?;
            write_stdout_line(&line)?;
        }
        Ok(())
    }

    /// Sends a `log` line to the parent, which surfaces it via `tracing`.
    pub fn log(&self, message: impl Into<String>) -> Result<(), DiscordError> {
        let line = encode_ipc_line(&ChildMessage::Log {
            message: message.into(),
        })?;
        write_stdout_line(&line)
    }

    /// Starts serving the IPC protocol: a background task reads parent
    /// messages from stdin, answers each `request` op with the value produced
    /// by `handler`, and resolves [`Self::shutdown_signal`] when the parent
    /// sends `shutdown` (or closes stdin, which also happens when the parent
    /// dies — so children never linger as orphans).
    ///
    /// The process is **not** exited automatically; await
    /// [`Self::shutdown_signal`] and tear down gracefully.
    pub fn serve<F, Fut>(&self, handler: F) -> tokio::task::JoinHandle<()>
    where
        F: Fn(Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Value> + Send + 'static,
    {
        let shutdown_tx = self.shutdown_tx.clone();
        let (line_tx, mut line_rx) = mpsc::unbounded_channel::<String>();

        // Blocking stdin reader on a plain OS thread: tokio's stdin type
        // needs the `io-std` feature and blocking reads must not occupy an
        // async worker anyway.
        std::thread::spawn(move || {
            use std::io::BufRead as _;
            let stdin = std::io::stdin();
            for line in stdin.lock().lines() {
                match line {
                    Ok(line) => {
                        if line_tx.send(line).is_err() {
                            return;
                        }
                    }
                    Err(_) => return,
                }
            }
        });

        tokio::spawn(async move {
            while let Some(line) = line_rx.recv().await {
                if line.trim().is_empty() {
                    continue;
                }
                match decode_ipc_line::<ParentMessage>(&line) {
                    Ok(ParentMessage::Shutdown) => {
                        info!("shutdown op received from shard manager");
                        let _ = shutdown_tx.send(true);
                        return;
                    }
                    Ok(ParentMessage::Request { id, payload }) => {
                        let payload = handler(payload).await;
                        match encode_ipc_line(&ChildMessage::Response { id, payload })
                            .and_then(|line| write_stdout_line(&line))
                        {
                            Ok(()) => {}
                            Err(error) => warn!("failed to write IPC response: {error}"),
                        }
                    }
                    Err(error) => {
                        warn!("unparseable parent IPC line ({error}): {line}");
                    }
                }
            }
            // stdin closed: the parent went away. Treat it as shutdown so the
            // child does not keep running as an orphan.
            info!("IPC stdin closed; treating as shutdown");
            let _ = shutdown_tx.send(true);
        })
    }

    /// Future that resolves once the parent has requested shutdown (via the
    /// `shutdown` op or by closing stdin). Requires [`Self::serve`] to be
    /// running. Can be called multiple times.
    pub fn shutdown_signal(&self) -> impl Future<Output = ()> + Send + 'static {
        let mut shutdown_rx = self.shutdown_tx.subscribe();
        async move {
            loop {
                if *shutdown_rx.borrow_and_update() {
                    return;
                }
                if shutdown_rx.changed().await.is_err() {
                    // Sender dropped without signaling: nothing more will
                    // arrive, so unblock the caller.
                    return;
                }
            }
        }
    }
}

/// Writes one line to stdout and flushes it. Stdout is the IPC channel in
/// child mode, so this is the only place child code should touch it.
fn write_stdout_line(line: &str) -> Result<(), DiscordError> {
    let mut stdout = std::io::stdout().lock();
    stdout
        .write_all(line.as_bytes())
        .and_then(|()| stdout.write_all(b"\n"))
        .and_then(|()| stdout.flush())
        .map_err(|error| DiscordError::gateway(format!("IPC stdout write failed: {error}")))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parent_messages_round_trip_with_expected_wire_format() {
        let shutdown = ParentMessage::Shutdown;
        let encoded = encode_ipc_line(&shutdown).unwrap();
        assert_eq!(encoded, r#"{"op":"shutdown"}"#);
        assert_eq!(decode_ipc_line::<ParentMessage>(&encoded).unwrap(), shutdown);

        let request = ParentMessage::Request {
            id: 7,
            payload: json!({"kind": "stats"}),
        };
        let encoded = encode_ipc_line(&request).unwrap();
        assert_eq!(encoded, r#"{"op":"request","id":7,"payload":{"kind":"stats"}}"#);
        assert_eq!(
            decode_ipc_line::<ParentMessage>(&format!("  {encoded}\n")).unwrap(),
            request
        );
    }

    #[test]
    fn child_messages_round_trip_with_expected_wire_format() {
        let ready = ChildMessage::Ready { shard_id: 3 };
        let encoded = encode_ipc_line(&ready).unwrap();
        assert_eq!(encoded, r#"{"op":"ready","shard_id":3}"#);
        assert_eq!(decode_ipc_line::<ChildMessage>(&encoded).unwrap(), ready);

        let response = ChildMessage::Response {
            id: 7,
            payload: json!([1, 2]),
        };
        let encoded = encode_ipc_line(&response).unwrap();
        assert_eq!(encoded, r#"{"op":"response","id":7,"payload":[1,2]}"#);
        assert_eq!(decode_ipc_line::<ChildMessage>(&encoded).unwrap(), response);

        let log = ChildMessage::Log {
            message: "hello".to_string(),
        };
        let encoded = encode_ipc_line(&log).unwrap();
        assert_eq!(encoded, r#"{"op":"log","message":"hello"}"#);
        assert_eq!(decode_ipc_line::<ChildMessage>(&encoded).unwrap(), log);

        assert!(decode_ipc_line::<ChildMessage>(r#"{"op":"bogus"}"#).is_err());
    }

    #[test]
    fn respawn_backoff_doubles_and_caps_at_five_minutes() {
        let base = Duration::from_secs(5);
        assert_eq!(respawn_backoff(base, 0), Duration::from_secs(5));
        assert_eq!(respawn_backoff(base, 1), Duration::from_secs(10));
        assert_eq!(respawn_backoff(base, 2), Duration::from_secs(20));
        assert_eq!(respawn_backoff(base, 6), Duration::from_secs(300));
        assert_eq!(respawn_backoff(base, 60), Duration::from_secs(300));
        assert_eq!(respawn_backoff(Duration::ZERO, 40), Duration::ZERO);
    }

    #[test]
    fn from_env_requires_ipc_marker() {
        // Env vars are absent in the test process, so this must be None.
        assert!(ShardChildProcess::from_env().is_none());
    }

    #[cfg(unix)]
    fn scripted_child_config(script: &str) -> ProcessShardManagerConfig {
        ProcessShardManagerConfig {
            total_shards: Some(1),
            program: Some(PathBuf::from("/bin/sh")),
            args: vec!["-c".to_string(), script.to_string()],
            spawn_stagger: Duration::ZERO,
            ..ProcessShardManagerConfig::default()
        }
    }

    #[cfg(unix)]
    async fn wait_for_state(
        manager: &ProcessShardManager,
        wanted: ChildState,
        deadline: Duration,
    ) -> bool {
        let end = Instant::now() + deadline;
        while Instant::now() < end {
            if manager
                .children()
                .first()
                .is_some_and(|child| child.state == wanted)
            {
                return true;
            }
            sleep(Duration::from_millis(20)).await;
        }
        false
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn ipc_round_trip_with_scripted_child() {
        // The scripted child speaks one full protocol exchange: announce
        // ready, answer the first broadcast (request id 1 for a fresh
        // manager), then exit on shutdown.
        let script = r#"
echo '{"op":"ready","shard_id":0}'
echo '{"op":"log","message":"child booted"}'
while read line; do
  case "$line" in
    *'"op":"request"'*) echo '{"op":"response","id":1,"payload":{"pong":true}}' ;;
    *'"op":"shutdown"'*) exit 0 ;;
  esac
done
"#;
        let manager = ProcessShardManager::spawn(scripted_child_config(script))
            .await
            .unwrap();
        assert_eq!(manager.shard_count(), 1);

        assert!(
            wait_for_state(&manager, ChildState::Running, Duration::from_secs(5)).await,
            "child never reached Running: {:?}",
            manager.children()
        );
        let status = &manager.children()[0];
        assert_eq!(status.shard_ids, vec![0]);
        assert!(status.pid.is_some());

        let responses = manager.broadcast(json!({"kind": "ping"})).await.unwrap();
        assert_eq!(responses, vec![(0, json!({"pong": true}))]);

        timeout(Duration::from_secs(5), manager.shutdown())
            .await
            .expect("shutdown timed out")
            .unwrap();
        assert_eq!(manager.children()[0].state, ChildState::Stopped);
        assert_eq!(manager.children()[0].pid, None);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn crashing_child_is_respawned_until_max_respawns() {
        let mut config = scripted_child_config("exit 1");
        config.respawn_delay = Duration::from_millis(10);
        config.max_respawns = Some(2);

        let manager = ProcessShardManager::spawn(config).await.unwrap();
        timeout(Duration::from_secs(5), manager.wait())
            .await
            .expect("manager.wait timed out")
            .unwrap();

        let status = &manager.children()[0];
        assert_eq!(status.state, ChildState::Stopped);
        assert_eq!(status.respawns, 2);

        // Broadcasting against dead children reports Null entries.
        let responses = manager.broadcast(json!("stats")).await.unwrap();
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0].1, Value::Null);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn respawn_disabled_leaves_child_stopped() {
        let mut config = scripted_child_config("exit 3");
        config.respawn = false;

        let manager = ProcessShardManager::spawn(config).await.unwrap();
        timeout(Duration::from_secs(5), manager.wait())
            .await
            .expect("manager.wait timed out")
            .unwrap();

        let status = &manager.children()[0];
        assert_eq!(status.state, ChildState::Stopped);
        assert_eq!(status.respawns, 0);
    }

    #[tokio::test]
    async fn spawn_rejects_missing_total_shards_and_bad_shard_list() {
        let error = ProcessShardManager::spawn(ProcessShardManagerConfig::default())
            .await
            .map(|_| ())
            .unwrap_err();
        assert!(error.to_string().contains("total_shards"));

        let config = ProcessShardManagerConfig {
            total_shards: Some(2),
            shard_list: Some(vec![0, 5]),
            ..ProcessShardManagerConfig::default()
        };
        let error = ProcessShardManager::spawn(config)
            .await
            .map(|_| ())
            .unwrap_err();
        assert!(error.to_string().contains("out of range"));

        let config = ProcessShardManagerConfig {
            total_shards: Some(2),
            shard_list: Some(Vec::new()),
            ..ProcessShardManagerConfig::default()
        };
        let error = ProcessShardManager::spawn(config)
            .await
            .map(|_| ())
            .unwrap_err();
        assert!(error.to_string().contains("must not be empty"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn children_are_grouped_by_shards_per_process() {
        let mut config = scripted_child_config("read line; exit 0");
        config.total_shards = Some(5);
        config.shards_per_process = 2;
        config.respawn = false;

        let manager = ProcessShardManager::spawn(config).await.unwrap();
        let children = manager.children();
        let groups: Vec<Vec<u32>> = children.iter().map(|c| c.shard_ids.clone()).collect();
        assert_eq!(groups, vec![vec![0, 1], vec![2, 3], vec![4]]);

        timeout(Duration::from_secs(5), manager.shutdown())
            .await
            .expect("shutdown timed out")
            .unwrap();
    }
}
