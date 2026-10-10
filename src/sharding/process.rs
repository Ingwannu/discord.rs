//! Multi-process shard management (discord.js `ShardingManager` equivalent).
//!
//! A [`ProcessShardManager`] spawns one child process per shard group, keeps
//! each child alive (respawning it with exponential backoff when it exits
//! unexpectedly), coordinates when each shard may IDENTIFY, and talks to the
//! children over a line-based JSON IPC protocol on the children's
//! stdin/stdout pipes. The child half of the protocol is
//! [`ShardChildProcess`], which the same binary can construct via
//! [`ShardChildProcess::from_env`] so that a single executable acts as both
//! the manager (parent) and the shard runner (child), like discord.js's
//! `ShardingManager` + `process.send` pattern.
//!
//! # IPC channel: framed lines on stdin/stdout
//!
//! Every protocol message is one line that starts with [`IPC_FRAME_PREFIX`]
//! (`@discordrs-ipc/1 `) followed by a JSON object. Lines without the prefix
//! are not protocol traffic:
//!
//! - A stray `println!` (or a tracing subscriber writing to stdout) in a
//!   child no longer corrupts the protocol. The parent logs such lines
//!   through `tracing` at INFO level (`child stdout: ...`) and carries on.
//! - Lines longer than [`ProcessShardManagerConfig::max_ipc_frame_bytes`]
//!   are discarded while they stream in, so a runaway child cannot make the
//!   parent buffer unbounded memory. Invalid UTF-8 is tolerated.
//! - Child **stderr** is inherited unchanged, so it remains the best place
//!   for logs.
//!
//! Two limits remain. A child must **not read stdin** itself, because the IPC
//! reader owns it. Writes that bypass Rust's stdout lock (C libraries,
//! subprocesses that inherit fd 1) can land in the middle of a frame; that
//! one frame is then dropped as malformed and the affected broadcast entry
//! reads `null`.
//!
//! # IDENTIFY coordination
//!
//! Discord allows one IDENTIFY per rate-limit bucket every 5 seconds, where
//! the bucket is `shard_id % max_concurrency`
//! (<https://docs.discord.com/developers/events/gateway#max-concurrency>).
//! The parent owns one queue per bucket and hands out IDENTIFY permits over
//! IPC. Children ask with [`ShardChildProcess::acquire_identify`] before
//! starting a shard and release the [`IdentifyPermit`] once the shard's
//! IDENTIFY has gone out (READY is the easiest signal). The bucket's next
//! grant comes `identify_interval` after the release. Because the parent
//! coordinates across every child, initial boots and respawns share one
//! schedule. [`ProcessShardManager::spawn_auto`] reads `max_concurrency`
//! from `/gateway/bot`.
//!
//! # Protocol reference (for custom child programs)
//!
//! | Direction | `op` | Fields | Meaning |
//! |---|---|---|---|
//! | parent → child | `shutdown` | | Stop gracefully; stdin closes right after. |
//! | parent → child | `request` | `id`, `payload` | Answer with a `response` carrying the same `id`. |
//! | parent → child | `identify_grant` | `shard_id` | The shard may send IDENTIFY now. |
//! | child → parent | `ready` | `shard_id` | The shard is connected. |
//! | child → parent | `response` | `id`, `payload` | Answer to a `request`. |
//! | child → parent | `log` | `message` | Surfaced through the parent's `tracing`. |
//! | child → parent | `identify_request` | `shard_id` | Queue for an IDENTIFY permit (one outstanding per shard). |
//! | child → parent | `identify_release` | `shard_id` | IDENTIFY was sent (or abandoned); the bucket's interval starts now. |
//!
//! Example frame: `@discordrs-ipc/1 {"op":"ready","shard_id":0}`. Empty
//! lines are ignored. The Rust child writes a newline before every frame
//! so that an unterminated `print!` cannot swallow the prefix. A permit that
//! is not released within
//! [`ProcessShardManagerConfig::identify_permit_timeout`] is reclaimed, and
//! a child that exits gives back every permit it held.
//!
//! [`ProcessShardManager`]: crate::sharding::process::ProcessShardManager
//! [`ProcessShardManager::spawn_auto`]: crate::sharding::process::ProcessShardManager::spawn_auto
//! [`ProcessShardManagerConfig::max_ipc_frame_bytes`]: crate::sharding::process::ProcessShardManagerConfig::max_ipc_frame_bytes
//! [`ProcessShardManagerConfig::identify_permit_timeout`]: crate::sharding::process::ProcessShardManagerConfig::identify_permit_timeout
//! [`ShardChildProcess`]: crate::sharding::process::ShardChildProcess
//! [`ShardChildProcess::from_env`]: crate::sharding::process::ShardChildProcess::from_env
//! [`ShardChildProcess::acquire_identify`]: crate::sharding::process::ShardChildProcess::acquire_identify
//! [`IdentifyPermit`]: crate::sharding::process::IdentifyPermit
//! [`IPC_FRAME_PREFIX`]: crate::sharding::process::IPC_FRAME_PREFIX

// [Decision Log] IPC channel
// - Purpose: keep IPC working when user code prints to stdout, and bound the
//   memory a misbehaving child can make the parent allocate.
// - Previous implementation and constraints: unframed JSON lines read with
//   `BufReader::lines()`. Line length was unbounded, one stray `println!`
//   produced "unparseable" noise, and invalid UTF-8 ended the reader as if
//   the child had exited. Children may be any program, including shell
//   scripts in tests, and the crate has no `libc`/`nix` dependency and no
//   `unsafe` process plumbing.
// - Alternatives considered: (1) an extra inherited pipe fd (fd 3/4) reserved
//   for IPC; (2) a loopback or Unix-domain socket whose address is passed in
//   the environment; (3) keeping stdio but framing every protocol line with
//   a prefix and bounding line length.
// - Chosen approach: (3).
// - Why: (1) needs `pre_exec` + `dup2` (unsafe, Unix-only, new dependency)
//   and has no portable Windows equivalent. (2) adds a listening endpoint
//   that needs authentication, cleanup, and firewall handling. (3) is
//   portable, needs no unsafe code, works with any child program, and keeps
//   stdin EOF as the "parent died" signal.
// - Trade-offs: stdout stays shared. Frames can still be torn by raw fd
//   writes from foreign code, and child code must leave stdin alone. Every
//   frame pays a 17-byte prefix.
//
// [Decision Log] IDENTIFY coordination
// - Purpose: respect Discord's per-bucket IDENTIFY limit across every child
//   process, including respawns.
// - Previous implementation and constraints: child *process starts* were
//   staggered by 5 seconds. Shards inside one child, respawns, and
//   `max_concurrency` were ignored. The gateway client (`src/gateway/*`)
//   has no IDENTIFY hook this module may change, so the permit has to be
//   taken before the shard connects.
// - Alternatives considered: (1) a smarter start stagger computed from
//   `max_concurrency`; (2) a file lock or shared memory between children;
//   (3) parent-granted permits over the existing IPC channel.
// - Chosen approach: (3). Release-based leases are spaced by
//   `identify_interval`, with a timeout for children that never release.
// - Why: only the parent sees every start and respawn. A stagger cannot
//   account for respawns or slow boots. Permits need no extra OS resources
//   and match discord.js's `WaitForIdentify` worker protocol.
// - Trade-offs: children must call `acquire_identify`; children that never
//   ask are not paced, except by the optional `spawn_stagger`. Because the
//   permit is taken before connecting, a bucket's slot also covers the
//   connect time. Re-IDENTIFYs that the gateway client performs on its own
//   after a session invalidation are not routed through the parent.
//
// [Decision Log] Monitor lifecycle
// - Purpose: shutdown must always be observable and bounded, and `wait()` /
//   `shutdown()` must agree on when children are gone.
// - Previous implementation and constraints: the monitor awaited stdin
//   writes inside its `select!`, so a child that stopped reading stdin hid
//   the shutdown request. The graceful-stop write ran before its timeout
//   started. `wait()` took the task handles, so a concurrent `shutdown()`
//   returned before children exited.
// - Alternatives considered: write timeouts on the inline write; a
//   `JoinSet` drained by whichever caller comes first; a dedicated writer
//   task per child plus a shared completion counter.
// - Chosen approach: a bounded per-child writer task (frames are enqueued
//   with `try_send`, so the monitor never waits on a pipe), one deadline
//   that covers write + wait before the kill, and a `watch` counter that
//   every monitor decrements on drop and every waiter observes.
// - Why: write timeouts still stall the loop for the timeout length, and
//   handle ownership makes waiters race. A counter is shared and survives
//   monitor panics.
// - Trade-offs: a child that reads stdin too slowly sees its broadcast
//   entries answered with `null` once 64 frames are queued, instead of
//   making the broadcast wait.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::io::{self, BufRead, Write as _};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex, MutexGuard};
use std::time::Duration;

use futures_util::stream::{FuturesUnordered, StreamExt};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::{mpsc, oneshot, watch, Mutex as AsyncMutex, OwnedMutexGuard};
use tokio::time::{sleep_until, timeout, timeout_at, Instant};
use tracing::{debug, info, warn};

use crate::error::DiscordError;
use crate::http::RestClient;
use crate::model::GatewayBot;

/// Environment variable holding the comma-separated shard id list assigned to
/// a child process (e.g. `"0,1"`).
pub const ENV_SHARD_IDS: &str = "DISCORDRS_SHARD_IDS";
/// Environment variable holding the total shard count.
pub const ENV_SHARD_COUNT: &str = "DISCORDRS_SHARD_COUNT";
/// Environment variable set to `1` when the process is a shard child whose
/// stdin/stdout carry the IPC protocol.
pub const ENV_SHARD_IPC: &str = "DISCORDRS_SHARD_IPC";
/// Environment variable holding the largest IPC line, in bytes and without
/// the trailing newline, that the parent accepts. Children use it to replace
/// oversized responses with `null` instead of sending a frame the parent
/// would discard.
pub const ENV_SHARD_IPC_MAX_FRAME: &str = "DISCORDRS_SHARD_IPC_MAX_FRAME";

/// Prefix that marks a line as an IPC frame. The `/1` is the protocol
/// version; lines with any other prefix are treated as ordinary output.
pub const IPC_FRAME_PREFIX: &str = "@discordrs-ipc/1 ";
/// Default for [`ProcessShardManagerConfig::max_ipc_frame_bytes`] (1 MiB).
pub const DEFAULT_MAX_IPC_FRAME_BYTES: usize = 1024 * 1024;

/// Smallest accepted frame limit: below this even a `ready` frame with a
/// large shard id plus some payload would not fit, so smaller settings are
/// raised to this value.
const MIN_IPC_FRAME_BYTES: usize = 4096;
/// Per-child queue between [`ProcessShardManager::broadcast`] and the
/// monitor. The monitor drains it without awaiting I/O, so it only fills
/// when broadcasts arrive faster than the monitor loop can turn.
const COMMAND_QUEUE_CAPACITY: usize = 256;
/// Frames waiting for the per-child stdin writer. When a child stops reading
/// stdin this fills up, and further sends fail fast instead of piling up.
const IPC_WRITE_QUEUE_CAPACITY: usize = 64;
/// How long to wait for the OS to reap a child after `SIGKILL`; only a
/// process stuck in uninterruptible kernel sleep takes longer.
const KILL_REAP_TIMEOUT: Duration = Duration::from_secs(5);
/// After a child exits, its last stdout lines (a final response or log) may
/// still be in the pipe; read them for at most this long. A grandchild that
/// inherited stdout could otherwise keep the pipe open forever.
const EXIT_DRAIN_TIMEOUT: Duration = Duration::from_secs(1);
/// Non-protocol lines are logged truncated to this many characters.
const LOGGED_LINE_CHARS: usize = 512;
/// Backoff ceiling for respawn delays.
const MAX_RESPAWN_DELAY: Duration = Duration::from_secs(5 * 60);
/// A child that stays alive at least this long resets the backoff schedule.
const BACKOFF_RESET_UPTIME: Duration = Duration::from_secs(60);
/// Clamp for user-supplied durations so deadline arithmetic cannot overflow.
const FAR_FUTURE: Duration = Duration::from_secs(60 * 60 * 24 * 365);

fn deadline_after(duration: Duration) -> Instant {
    let now = Instant::now();
    now.checked_add(duration.min(FAR_FUTURE)).unwrap_or(now)
}

// ---------------------------------------------------------------------------
// Protocol
// ---------------------------------------------------------------------------

/// IPC message sent from the manager (parent) to a shard child, one frame
/// per stdin line.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
#[non_exhaustive]
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
    /// Answer to [`ChildMessage::IdentifyRequest`]: the shard may send
    /// IDENTIFY now. The child must reply with
    /// [`ChildMessage::IdentifyRelease`] once it has.
    IdentifyGrant {
        /// Shard whose IDENTIFY is permitted.
        shard_id: u32,
    },
}

/// IPC message sent from a shard child to the manager, one frame per stdout
/// line.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
#[non_exhaustive]
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
    /// Queue the shard for an IDENTIFY permit; answered with
    /// [`ParentMessage::IdentifyGrant`].
    IdentifyRequest {
        /// Shard that wants to IDENTIFY.
        shard_id: u32,
    },
    /// The shard's IDENTIFY has been sent (or will not be sent); the bucket's
    /// next grant is spaced from this moment.
    IdentifyRelease {
        /// Shard whose permit is returned.
        shard_id: u32,
    },
}

/// Encodes an IPC message as one frame: [`IPC_FRAME_PREFIX`] followed by the
/// JSON object, without the trailing newline.
pub fn encode_ipc_line<T: Serialize>(message: &T) -> Result<String, DiscordError> {
    let json = serde_json::to_string(message)?;
    let mut line = String::with_capacity(IPC_FRAME_PREFIX.len() + json.len());
    line.push_str(IPC_FRAME_PREFIX);
    line.push_str(&json);
    Ok(line)
}

/// Decodes one frame produced by [`encode_ipc_line`]. Surrounding whitespace
/// is ignored; a line without [`IPC_FRAME_PREFIX`] is an error because it is
/// not protocol traffic.
pub fn decode_ipc_line<T: DeserializeOwned>(line: &str) -> Result<T, DiscordError> {
    let Some(json) = line.trim().strip_prefix(IPC_FRAME_PREFIX) else {
        return Err(DiscordError::model(format!(
            "not an IPC frame: line does not start with {IPC_FRAME_PREFIX:?}"
        )));
    };
    Ok(serde_json::from_str(json)?)
}

/// Encodes a frame and appends the line terminator for the stdin writer.
fn frame_line<T: Serialize>(message: &T) -> Result<String, DiscordError> {
    let mut line = encode_ipc_line(message)?;
    line.push('\n');
    Ok(line)
}

/// Builds a `request` frame (with newline) around an already serialized
/// payload, so a broadcast serializes its payload once instead of once per
/// child. Must stay byte-compatible with `encode_ipc_line` (tested).
fn request_frame(id: u64, payload_json: &str) -> String {
    format!("{IPC_FRAME_PREFIX}{{\"op\":\"request\",\"id\":{id},\"payload\":{payload_json}}}\n")
}

// ---------------------------------------------------------------------------
// Bounded line framing
// ---------------------------------------------------------------------------

/// One newline-terminated unit read from an IPC stream.
#[derive(Debug, PartialEq, Eq)]
enum LineEvent {
    /// A complete line (without the newline) within the size limit.
    Line(Vec<u8>),
    /// A line that exceeded the limit; its bytes were discarded as they
    /// arrived. Carries the total length seen.
    Oversized(usize),
}

/// Incremental line splitter that never buffers more than `max` bytes.
///
/// It is fed whatever a `fill_buf` call returned and reports how much it
/// consumed, so the same logic serves the parent's async reader and the
/// child's blocking reader. All state lives here, never inside a future,
/// which keeps the async reader cancel-safe inside `select!`.
struct LineLimiter {
    line: Vec<u8>,
    max: usize,
    /// `Some(n)` while skipping the rest of an oversized line (`n` bytes so
    /// far).
    discarded: Option<usize>,
}

impl LineLimiter {
    fn new(max: usize) -> Self {
        Self {
            line: Vec::new(),
            max,
            discarded: None,
        }
    }

    /// Consumes bytes up to and including the first newline in `input`
    /// (returning the finished line), or all of `input` when it holds no
    /// newline.
    fn feed(&mut self, input: &[u8]) -> (usize, Option<LineEvent>) {
        match input.iter().position(|&byte| byte == b'\n') {
            Some(newline) => {
                let chunk = &input[..newline];
                let event = if let Some(discarded) = self.discarded.take() {
                    LineEvent::Oversized(discarded.saturating_add(chunk.len()))
                } else if self.line.len().saturating_add(chunk.len()) > self.max {
                    let total = self.line.len().saturating_add(chunk.len());
                    self.line = Vec::new();
                    LineEvent::Oversized(total)
                } else {
                    self.line.extend_from_slice(chunk);
                    LineEvent::Line(std::mem::take(&mut self.line))
                };
                (newline + 1, Some(event))
            }
            None => {
                if let Some(discarded) = self.discarded.as_mut() {
                    *discarded = discarded.saturating_add(input.len());
                } else if self.line.len().saturating_add(input.len()) > self.max {
                    self.discarded = Some(self.line.len().saturating_add(input.len()));
                    self.line = Vec::new();
                } else {
                    self.line.extend_from_slice(input);
                }
                (input.len(), None)
            }
        }
    }

    /// Flushes an unterminated final line at end of stream.
    fn finish(&mut self) -> Option<LineEvent> {
        if let Some(discarded) = self.discarded.take() {
            return Some(LineEvent::Oversized(discarded));
        }
        if self.line.is_empty() {
            None
        } else {
            Some(LineEvent::Line(std::mem::take(&mut self.line)))
        }
    }
}

/// How one received line relates to the protocol.
#[derive(Debug)]
enum IpcLine<T> {
    /// Blank line (the Rust child emits one before every frame).
    Empty,
    /// A well-formed frame.
    Message(T),
    /// Starts with the frame prefix but does not decode.
    Malformed(String),
    /// Ordinary output such as a stray `println!`.
    Foreign,
}

fn classify_line<T: DeserializeOwned>(line: &[u8]) -> IpcLine<T> {
    let line = line.trim_ascii_end();
    if line.is_empty() {
        return IpcLine::Empty;
    }
    match line.strip_prefix(IPC_FRAME_PREFIX.as_bytes()) {
        Some(json) => match serde_json::from_slice(json) {
            Ok(message) => IpcLine::Message(message),
            Err(error) => IpcLine::Malformed(error.to_string()),
        },
        None => IpcLine::Foreign,
    }
}

/// Lossy, length-limited rendering of a line for logs.
fn printable_line(line: &[u8]) -> String {
    let text = String::from_utf8_lossy(line.trim_ascii_end());
    if text.chars().count() <= LOGGED_LINE_CHARS {
        return text.into_owned();
    }
    let mut truncated: String = text.chars().take(LOGGED_LINE_CHARS).collect();
    truncated.push_str("...");
    truncated
}

/// Async bounded line reader over a child's stdout.
struct FrameReader<R> {
    reader: BufReader<R>,
    limiter: LineLimiter,
    closed: bool,
}

impl<R: AsyncRead + Unpin> FrameReader<R> {
    fn new(reader: R, max: usize) -> Self {
        Self {
            reader: BufReader::new(reader),
            limiter: LineLimiter::new(max),
            closed: false,
        }
    }

    fn is_closed(&self) -> bool {
        self.closed
    }

    /// Next line event; `Ok(None)` at end of stream. Cancel-safe: the only
    /// await point is `fill_buf`, and bytes are consumed synchronously after
    /// it returns.
    async fn next_event(&mut self) -> io::Result<Option<LineEvent>> {
        loop {
            let buffer = match self.reader.fill_buf().await {
                Ok(buffer) => buffer,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    self.closed = true;
                    return Err(error);
                }
            };
            if buffer.is_empty() {
                let event = self.limiter.finish();
                if event.is_none() {
                    self.closed = true;
                }
                return Ok(event);
            }
            let (consumed, event) = self.limiter.feed(buffer);
            self.reader.consume(consumed);
            if event.is_some() {
                return Ok(event);
            }
        }
    }
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
    /// Delay between consecutive child *process* starts (default
    /// [`Duration::ZERO`]).
    ///
    /// IDENTIFY pacing is done by the permits children request with
    /// [`ShardChildProcess::acquire_identify`], which also covers respawns
    /// and several shards per process. Set a stagger (e.g. 5s) only for
    /// custom child programs that do not speak the identify part of the
    /// protocol.
    pub spawn_stagger: Duration,
    /// Discord's `max_concurrency`: how many IDENTIFY buckets
    /// (`shard_id % max_concurrency`) may identify in parallel. `None` means
    /// [`ProcessShardManager::spawn_auto`] reads it from `/gateway/bot`, and
    /// [`ProcessShardManager::spawn`] assumes `1` (the value for bots
    /// without large bot sharding).
    pub max_concurrency: Option<u32>,
    /// Minimum spacing between IDENTIFY grants in one bucket, measured from
    /// the previous permit's release (default 5s, Discord's documented
    /// window). Lower it only in tests.
    pub identify_interval: Duration,
    /// How long a child may hold an IDENTIFY permit before the parent
    /// reclaims it (default 30s). It stops one stuck child from blocking
    /// every other shard in its bucket.
    pub identify_permit_timeout: Duration,
    /// Per-child response deadline used by [`ProcessShardManager::broadcast`]
    /// (default 10s).
    pub broadcast_timeout: Duration,
    /// Total time a child gets to exit after shutdown is requested (default
    /// 10s). The deadline covers sending the `shutdown` op, closing stdin,
    /// and waiting for the exit; the child is killed when it passes.
    pub shutdown_timeout: Duration,
    /// Largest IPC line accepted in either direction, in bytes without the
    /// newline (default [`DEFAULT_MAX_IPC_FRAME_BYTES`]; values below 4 KiB
    /// are raised to 4 KiB). Longer child output is discarded while it
    /// streams in, and broadcasts whose payload would exceed it fail.
    pub max_ipc_frame_bytes: usize,
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
            spawn_stagger: Duration::ZERO,
            max_concurrency: None,
            identify_interval: Duration::from_secs(5),
            identify_permit_timeout: Duration::from_secs(30),
            broadcast_timeout: Duration::from_secs(10),
            shutdown_timeout: Duration::from_secs(10),
            max_ipc_frame_bytes: DEFAULT_MAX_IPC_FRAME_BYTES,
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
// IDENTIFY coordination (parent side)
// ---------------------------------------------------------------------------

/// Per-bucket IDENTIFY queues shared by every child monitor.
///
/// Each bucket is a FIFO `tokio::sync::Mutex`. Holding its guard means
/// "this bucket is identifying right now", so waiters queue fairly, and a
/// waiter that is dropped (its child exited) leaves the queue without
/// disturbing the others.
struct IdentifyGate {
    buckets: Vec<Arc<AsyncMutex<IdentifyBucket>>>,
    max_concurrency: u32,
    interval: Duration,
}

struct IdentifyBucket {
    /// Earliest moment the next permit for this bucket may be granted.
    available_at: Instant,
}

/// A granted IDENTIFY slot. Dropping it (release, permit timeout, or the
/// child exiting) starts the bucket's `interval` from that moment. Spacing
/// from release instead of grant stays safe even when the child needed a
/// while to connect before sending IDENTIFY.
struct IdentifyLease {
    bucket: OwnedMutexGuard<IdentifyBucket>,
    interval: Duration,
}

impl Drop for IdentifyLease {
    fn drop(&mut self) {
        self.bucket.available_at = deadline_after(self.interval);
    }
}

impl IdentifyGate {
    fn new(
        max_concurrency: u32,
        total_shards: u32,
        interval: Duration,
        not_before: Instant,
    ) -> Self {
        let max_concurrency = max_concurrency.max(1);
        // `shard_id % max_concurrency` never exceeds the shard count, so
        // allocating more buckets than shards would be wasted.
        let bucket_count = max_concurrency.min(total_shards.max(1)) as usize;
        let buckets = (0..bucket_count)
            .map(|_| {
                Arc::new(AsyncMutex::new(IdentifyBucket {
                    available_at: not_before,
                }))
            })
            .collect();
        Self {
            buckets,
            max_concurrency,
            interval,
        }
    }

    fn bucket_of(&self, shard_id: u32) -> usize {
        // The outer modulo is a no-op for valid shard ids and keeps the
        // index in range for invalid ones; `buckets` is never empty.
        (shard_id % self.max_concurrency) as usize % self.buckets.len()
    }

    async fn acquire(&self, shard_id: u32) -> IdentifyLease {
        let bucket = Arc::clone(&self.buckets[self.bucket_of(shard_id)]);
        let guard = bucket.lock_owned().await;
        // Cancelling during this sleep drops the guard without touching
        // `available_at`, which is correct because nothing identified.
        sleep_until(guard.available_at).await;
        IdentifyLease {
            bucket: guard,
            interval: self.interval,
        }
    }
}

/// Values taken from `/gateway/bot` for [`ProcessShardManager::spawn_auto`].
#[derive(Debug, PartialEq, Eq)]
struct IdentifyPlan {
    shards: u32,
    max_concurrency: u32,
    /// Wait before the first grant when the daily session-start budget is
    /// exhausted (`remaining == 0`).
    initial_delay: Duration,
}

impl IdentifyPlan {
    fn from_gateway_bot(gateway_bot: &GatewayBot) -> Self {
        let limit = &gateway_bot.session_start_limit;
        Self {
            shards: gateway_bot.shards.max(1),
            max_concurrency: limit.max_concurrency.max(1),
            initial_delay: if limit.remaining == 0 {
                Duration::from_millis(limit.reset_after)
            } else {
                Duration::ZERO
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Manager
// ---------------------------------------------------------------------------

enum ChildCommand {
    Request {
        id: u64,
        /// Complete `request` frame including the newline.
        frame: String,
        respond_to: oneshot::Sender<Value>,
    },
}

struct ChildEntry {
    shard_ids: Vec<u32>,
    status: Arc<StdMutex<ChildStatus>>,
    command_tx: mpsc::Sender<ChildCommand>,
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
    shutdown_timeout: Duration,
    max_frame: usize,
    identify_gate: Arc<IdentifyGate>,
    identify_permit_timeout: Duration,
}

/// Decrements the live-monitor counter when a monitor task ends for any
/// reason, including a panic or runtime shutdown, so `wait()` cannot hang
/// on a monitor that is already gone.
struct MonitorCompletion(Arc<watch::Sender<usize>>);

impl Drop for MonitorCompletion {
    fn drop(&mut self) {
        if std::thread::panicking() {
            warn!("shard child monitor panicked");
        }
        self.0
            .send_modify(|remaining| *remaining = remaining.saturating_sub(1));
    }
}

/// Multi-process shard manager: spawns one child process per shard group,
/// respawns crashed children with exponential backoff, grants IDENTIFY
/// permits per `max_concurrency` bucket, and provides request/response
/// broadcasting over framed IPC — the discord.js `ShardingManager`
/// equivalent.
///
/// Children are started with `DISCORDRS_SHARD_IDS`, `DISCORDRS_SHARD_COUNT`,
/// `DISCORDRS_SHARD_IPC=1`, and `DISCORDRS_SHARD_IPC_MAX_FRAME` in their
/// environment so the same binary can detect child mode via
/// [`ShardChildProcess::from_env`].
///
/// Dropping the manager without [`Self::shutdown`] still stops the
/// children gracefully in the background; nothing waits for them, though.
pub struct ProcessShardManager {
    total_shards: u32,
    max_concurrency: u32,
    children: Vec<ChildEntry>,
    shutdown_tx: watch::Sender<bool>,
    monitors_remaining: watch::Receiver<usize>,
    next_request_id: AtomicU64,
    broadcast_timeout: Duration,
    max_frame: usize,
}

impl ProcessShardManager {
    /// Starts child processes for every shard group described by `config`.
    ///
    /// `config.total_shards` must be `Some`; use [`Self::spawn_auto`] to let
    /// Discord recommend a shard count and `max_concurrency`. Without
    /// `config.max_concurrency`, IDENTIFY permits are granted for one
    /// bucket only.
    pub async fn spawn(config: ProcessShardManagerConfig) -> Result<Self, DiscordError> {
        Self::spawn_with_identify_delay(config, Duration::ZERO).await
    }

    /// Like [`Self::spawn`], but fills `config.total_shards` and
    /// `config.max_concurrency` from Discord's `/gateway/bot` endpoint when
    /// they are `None`. When the daily session-start budget is exhausted,
    /// the first IDENTIFY grant waits for its reset.
    ///
    /// If only `max_concurrency` was missing and the request fails, a
    /// warning is logged and one bucket is assumed; if the shard count is
    /// missing, the error is returned.
    pub async fn spawn_auto(
        token: impl Into<String>,
        config: ProcessShardManagerConfig,
    ) -> Result<Self, DiscordError> {
        let rest = RestClient::new(token, 0);
        Self::spawn_auto_with(&rest, config).await
    }

    async fn spawn_auto_with(
        rest: &RestClient,
        mut config: ProcessShardManagerConfig,
    ) -> Result<Self, DiscordError> {
        let mut identify_delay = Duration::ZERO;
        if config.total_shards.is_none() || config.max_concurrency.is_none() {
            match rest.get_gateway_bot().await {
                Ok(gateway_bot) => {
                    let plan = IdentifyPlan::from_gateway_bot(&gateway_bot);
                    info!(
                        shards = plan.shards,
                        max_concurrency = plan.max_concurrency,
                        identify_delay_ms = plan.initial_delay.as_millis() as u64,
                        "using /gateway/bot sharding parameters"
                    );
                    if config.total_shards.is_none() {
                        config.total_shards = Some(plan.shards);
                    }
                    if config.max_concurrency.is_none() {
                        config.max_concurrency = Some(plan.max_concurrency);
                    }
                    identify_delay = plan.initial_delay;
                }
                Err(error) if config.total_shards.is_some() => {
                    warn!(
                        "failed to fetch /gateway/bot for max_concurrency; \
                         granting one IDENTIFY at a time: {error}"
                    );
                }
                Err(error) => return Err(error),
            }
        }
        Self::spawn_with_identify_delay(config, identify_delay).await
    }

    async fn spawn_with_identify_delay(
        config: ProcessShardManagerConfig,
        identify_delay: Duration,
    ) -> Result<Self, DiscordError> {
        let Some(total_shards) = config.total_shards else {
            return Err(DiscordError::model(
                "ProcessShardManagerConfig.total_shards is required for spawn(); \
                 use ProcessShardManager::spawn_auto to fetch it from /gateway/bot",
            ));
        };
        let total_shards = total_shards.max(1);

        let mut shard_ids: Vec<u32> = match &config.shard_list {
            Some(list) => list.clone(),
            None => (0..total_shards).collect(),
        };
        shard_ids.sort_unstable();
        shard_ids.dedup();
        if shard_ids.is_empty() {
            return Err(DiscordError::model("shard_list must not be empty"));
        }
        if let Some(&bad) = shard_ids.iter().find(|&&id| id >= total_shards) {
            return Err(DiscordError::model(format!(
                "shard id {bad} is out of range for total_shards {total_shards}"
            )));
        }

        let program = match &config.program {
            Some(path) => path.clone(),
            None => std::env::current_exe()?,
        };
        let shards_per_process = config.shards_per_process.max(1) as usize;
        let max_concurrency = config.max_concurrency.unwrap_or(1).max(1);
        let max_frame = config.max_ipc_frame_bytes.max(MIN_IPC_FRAME_BYTES);
        let identify_gate = Arc::new(IdentifyGate::new(
            max_concurrency,
            total_shards,
            config.identify_interval,
            deadline_after(identify_delay),
        ));

        let groups: Vec<Vec<u32>> = shard_ids
            .chunks(shards_per_process)
            .map(<[u32]>::to_vec)
            .collect();
        let (shutdown_tx, _) = watch::channel(false);
        let (completion_tx, monitors_remaining) = watch::channel(groups.len());
        let completion_tx = Arc::new(completion_tx);
        let mut children = Vec::with_capacity(groups.len());

        for (index, group) in groups.into_iter().enumerate() {
            let status = Arc::new(StdMutex::new(ChildStatus {
                pid: None,
                shard_ids: group.clone(),
                state: ChildState::Starting,
                respawns: 0,
            }));
            let (command_tx, command_rx) = mpsc::channel(COMMAND_QUEUE_CAPACITY);

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
                (ENV_SHARD_IPC_MAX_FRAME.to_string(), max_frame.to_string()),
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
                    .checked_mul(u32::try_from(index).unwrap_or(u32::MAX))
                    .unwrap_or(MAX_RESPAWN_DELAY),
                shutdown_timeout: config.shutdown_timeout,
                max_frame,
                identify_gate: Arc::clone(&identify_gate),
                identify_permit_timeout: config.identify_permit_timeout,
            };

            let shutdown_rx = shutdown_tx.subscribe();
            let completion = MonitorCompletion(Arc::clone(&completion_tx));
            tokio::spawn(async move {
                let _completion = completion;
                run_child_monitor(context, command_rx, shutdown_rx).await;
            });
            children.push(ChildEntry {
                shard_ids: group,
                status,
                command_tx,
            });
        }

        info!(
            total_shards,
            max_concurrency,
            children = children.len(),
            "process shard manager spawned"
        );

        Ok(Self {
            total_shards,
            max_concurrency,
            children,
            shutdown_tx,
            monitors_remaining,
            next_request_id: AtomicU64::new(1),
            broadcast_timeout: config.broadcast_timeout,
            max_frame,
        })
    }

    /// Total shard count this manager was configured with.
    pub fn shard_count(&self) -> u32 {
        self.total_shards
    }

    /// Number of IDENTIFY buckets permits are granted for
    /// (`shard_id % max_concurrency`).
    pub fn max_concurrency(&self) -> u32 {
        self.max_concurrency
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
    /// Resolves with exactly one `(first_shard_id, value)` entry per child,
    /// keyed by the lowest shard id in that child's group. A child that does
    /// not answer within `broadcast_timeout` contributes `Value::Null` for
    /// its entry, and so does a child that is down, respawning, has not
    /// called [`ShardChildProcess::serve`] yet, or is not draining its
    /// stdin. Callers should treat `Null` entries as "shard unavailable".
    ///
    /// Fails only when the encoded request would exceed
    /// `max_ipc_frame_bytes`; nothing is sent in that case.
    pub async fn broadcast(&self, payload: Value) -> Result<Vec<(u32, Value)>, DiscordError> {
        let payload_json = serde_json::to_string(&payload)?;
        // Size check against the longest possible id so that every child
        // receives the same verdict.
        let longest = request_frame(u64::MAX, &payload_json)
            .len()
            .saturating_sub(1);
        if longest > self.max_frame {
            return Err(DiscordError::model(format!(
                "broadcast payload encodes to a {longest}-byte IPC frame, above \
                 max_ipc_frame_bytes ({})",
                self.max_frame
            )));
        }

        let mut pending = Vec::with_capacity(self.children.len());
        for child in &self.children {
            let first_shard = child.shard_ids.first().copied().unwrap_or(0);
            let id = self.next_request_id.fetch_add(1, Ordering::Relaxed);
            let (respond_to, response_rx) = oneshot::channel();
            let command = ChildCommand::Request {
                id,
                frame: request_frame(id, &payload_json),
                respond_to,
            };
            let sent = match child.command_tx.try_send(command) {
                Ok(()) => true,
                Err(mpsc::error::TrySendError::Full(_)) => {
                    warn!(
                        first_shard,
                        "shard child command queue is full; answering broadcast with null"
                    );
                    false
                }
                Err(mpsc::error::TrySendError::Closed(_)) => false,
            };
            let broadcast_timeout = self.broadcast_timeout;
            pending.push(async move {
                if !sent {
                    return (first_shard, Value::Null);
                }
                match timeout(broadcast_timeout, response_rx).await {
                    Ok(Ok(value)) => (first_shard, value),
                    Ok(Err(_)) | Err(_) => (first_shard, Value::Null),
                }
            });
        }
        Ok(futures_util::future::join_all(pending).await)
    }

    /// Gracefully stops all children. Each receives the `shutdown` op and
    /// gets `shutdown_timeout` in total to exit before it is killed.
    /// Resolves once every child monitor has finished. Safe to call
    /// concurrently with [`Self::wait`] and more than once.
    pub async fn shutdown(&self) -> Result<(), DiscordError> {
        info!("process shard manager shutting down");
        self.shutdown_tx.send_replace(true);
        self.wait().await
    }

    /// Waits until every child monitor has finished, i.e. all children have
    /// exited and no further respawns will happen. Any number of callers
    /// may wait concurrently, including alongside [`Self::shutdown`]; all
    /// of them observe the same completion.
    pub async fn wait(&self) -> Result<(), DiscordError> {
        let mut remaining = self.monitors_remaining.clone();
        // An error means every monitor (and with it the sender) is gone,
        // which is completion as well.
        let _ = remaining.wait_for(|count| *count == 0).await;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Child monitor task
// ---------------------------------------------------------------------------

enum ChildRunOutcome {
    /// The child exited on its own (crash or clean exit).
    Exited,
    /// Shutdown was requested; the child was stopped.
    ShutdownRequested,
}

/// Resolves once shutdown has been requested or the manager was dropped
/// (sender gone), which also means "stop the children".
async fn wait_for_shutdown(shutdown_rx: &mut watch::Receiver<bool>) {
    let _ = shutdown_rx.wait_for(|requested| *requested).await;
}

fn shutdown_requested(shutdown_rx: &watch::Receiver<bool>) -> bool {
    *shutdown_rx.borrow() || shutdown_rx.has_changed().is_err()
}

fn set_state(status: &StdMutex<ChildStatus>, state: ChildState) {
    lock_or_recover(status).state = state;
}

/// Sleeps until `deadline` while no child process is running. Broadcast
/// commands that arrive meanwhile are dropped, which answers them with
/// `null` immediately instead of after the broadcast timeout. Returns
/// `false` when shutdown was requested or the manager is gone.
async fn idle_until(
    deadline: Instant,
    shutdown_rx: &mut watch::Receiver<bool>,
    command_rx: &mut mpsc::Receiver<ChildCommand>,
) -> bool {
    loop {
        tokio::select! {
            () = sleep_until(deadline) => return true,
            () = wait_for_shutdown(shutdown_rx) => return false,
            command = command_rx.recv() => {
                if command.is_none() {
                    return false;
                }
            }
        }
    }
}

async fn run_child_monitor(
    context: MonitorContext,
    mut command_rx: mpsc::Receiver<ChildCommand>,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    if !context.start_delay.is_zero()
        && !idle_until(
            deadline_after(context.start_delay),
            &mut shutdown_rx,
            &mut command_rx,
        )
        .await
    {
        set_state(&context.status, ChildState::Stopped);
        return;
    }

    let mut backoff_attempt: u32 = 0;
    let mut respawns: u32 = 0;

    loop {
        if shutdown_requested(&shutdown_rx) {
            set_state(&context.status, ChildState::Stopped);
            return;
        }

        set_state(&context.status, ChildState::Starting);
        match spawn_child_process(&context) {
            Ok(child) => {
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
                    supervise_child(&context, child, &mut command_rx, &mut shutdown_rx).await;
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
        if !idle_until(deadline_after(delay), &mut shutdown_rx, &mut command_rx).await {
            set_state(&context.status, ChildState::Stopped);
            return;
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

/// Owns a child's stdin in a dedicated task so the monitor never awaits a
/// pipe write. Frames are queued with `try_send`; a full queue means the
/// child stopped reading, and the caller fails that one message instead of
/// stalling. Dropping the writer aborts the task, which closes stdin even if
/// a write is stuck.
struct IpcWriter {
    tx: Option<mpsc::Sender<String>>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl IpcWriter {
    fn spawn(stdin: Option<ChildStdin>) -> Self {
        let (tx, mut rx) = mpsc::channel::<String>(IPC_WRITE_QUEUE_CAPACITY);
        let task = stdin.map(|mut stdin| {
            tokio::spawn(async move {
                while let Some(frame) = rx.recv().await {
                    let written = async {
                        stdin.write_all(frame.as_bytes()).await?;
                        stdin.flush().await
                    }
                    .await;
                    if let Err(error) = written {
                        debug!("shard child stdin closed: {error}");
                        return;
                    }
                }
                // Queue closed by `close()`: dropping stdin sends EOF.
            })
        });
        Self { tx: Some(tx), task }
    }

    fn try_send(&self, frame: String) -> Result<(), &'static str> {
        let Some(tx) = &self.tx else {
            return Err("IPC writer is closed");
        };
        tx.try_send(frame).map_err(|error| match error {
            mpsc::error::TrySendError::Full(_) => {
                "IPC write queue is full (child is not reading stdin)"
            }
            mpsc::error::TrySendError::Closed(_) => "child stdin is closed",
        })
    }

    /// Stops accepting frames. Already queued frames are still written,
    /// then stdin is closed.
    fn close(&mut self) {
        self.tx = None;
    }
}

impl Drop for IpcWriter {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

type IdentifyWait = Pin<Box<dyn Future<Output = (u32, IdentifyLease)> + Send>>;

struct HeldPermit {
    _lease: IdentifyLease,
    expires_at: Instant,
}

/// IPC state of one child process instance. Dropping it (child exit,
/// respawn, shutdown) fails pending broadcasts with `null`, leaves the
/// IDENTIFY queues, and returns held permits.
struct ChildSession<'a> {
    context: &'a MonitorContext,
    pending: HashMap<u64, oneshot::Sender<Value>>,
    ready: HashSet<u32>,
    identify_waiting: FuturesUnordered<IdentifyWait>,
    identify_queued: HashSet<u32>,
    identify_held: HashMap<u32, HeldPermit>,
    stopping: bool,
}

impl<'a> ChildSession<'a> {
    fn new(context: &'a MonitorContext) -> Self {
        Self {
            context,
            pending: HashMap::new(),
            ready: HashSet::new(),
            identify_waiting: FuturesUnordered::new(),
            identify_queued: HashSet::new(),
            identify_held: HashMap::new(),
            stopping: false,
        }
    }

    fn forward_request(
        &mut self,
        writer: &IpcWriter,
        id: u64,
        frame: String,
        respond_to: oneshot::Sender<Value>,
    ) {
        if respond_to.is_closed() {
            return; // broadcaster already timed out
        }
        self.pending.retain(|_, sender| !sender.is_closed());
        match writer.try_send(frame) {
            Ok(()) => {
                self.pending.insert(id, respond_to);
            }
            Err(reason) => {
                // Dropping `respond_to` reports Null upstream right away.
                warn!(
                    child = self.context.index,
                    "cannot forward broadcast request: {reason}"
                );
            }
        }
    }

    fn handle_stdout(&mut self, event: io::Result<Option<LineEvent>>) {
        let index = self.context.index;
        match event {
            Ok(Some(LineEvent::Line(line))) => match classify_line::<ChildMessage>(&line) {
                IpcLine::Empty => {}
                IpcLine::Message(message) => self.handle_message(message),
                IpcLine::Malformed(error) => warn!(
                    child = index,
                    "malformed IPC frame from shard child ({error}): {}",
                    printable_line(&line)
                ),
                IpcLine::Foreign => {
                    info!(child = index, "child stdout: {}", printable_line(&line));
                }
            },
            Ok(Some(LineEvent::Oversized(bytes))) => warn!(
                child = index,
                bytes,
                limit = self.context.max_frame,
                "discarded oversized line from shard child stdout"
            ),
            Ok(None) => debug!(child = index, "shard child closed stdout"),
            Err(error) => warn!(child = index, "reading shard child stdout failed: {error}"),
        }
    }

    fn handle_message(&mut self, message: ChildMessage) {
        let context = self.context;
        match message {
            ChildMessage::Ready { shard_id } => {
                if !context.shard_ids.contains(&shard_id) {
                    warn!(
                        child = context.index,
                        shard_id, "ready for a shard not assigned to this child; ignoring"
                    );
                    return;
                }
                self.ready.insert(shard_id);
                let state = if self.ready.len() >= context.shard_ids.len() {
                    ChildState::Running
                } else {
                    ChildState::Ready
                };
                set_state(&context.status, state);
                info!(child = context.index, shard_id, "shard reported ready");
            }
            ChildMessage::Response { id, payload } => match self.pending.remove(&id) {
                Some(sender) => {
                    let _ = sender.send(payload);
                }
                None => debug!(
                    child = context.index,
                    request_id = id,
                    "response for unknown or expired request"
                ),
            },
            ChildMessage::Log { message } => {
                info!(child = context.index, "child log: {message}");
            }
            ChildMessage::IdentifyRequest { shard_id } => self.queue_identify(shard_id),
            ChildMessage::IdentifyRelease { shard_id } => {
                if self.identify_held.remove(&shard_id).is_none() {
                    debug!(
                        child = context.index,
                        shard_id, "identify release without a held permit"
                    );
                }
            }
        }
    }

    fn queue_identify(&mut self, shard_id: u32) {
        let index = self.context.index;
        if self.stopping {
            debug!(
                child = index,
                shard_id, "ignoring identify request during shutdown"
            );
            return;
        }
        if !self.context.shard_ids.contains(&shard_id) {
            warn!(
                child = index,
                shard_id, "identify request for a shard not assigned to this child; ignoring"
            );
            return;
        }
        if self.identify_queued.contains(&shard_id) || self.identify_held.contains_key(&shard_id) {
            warn!(
                child = index,
                shard_id, "duplicate identify request while one is outstanding; ignoring"
            );
            return;
        }
        self.identify_queued.insert(shard_id);
        let gate = Arc::clone(&self.context.identify_gate);
        self.identify_waiting.push(Box::pin(async move {
            let lease = gate.acquire(shard_id).await;
            (shard_id, lease)
        }));
    }

    fn grant_identify(&mut self, writer: &IpcWriter, shard_id: u32, lease: IdentifyLease) {
        self.identify_queued.remove(&shard_id);
        let delivered = frame_line(&ParentMessage::IdentifyGrant { shard_id })
            .map_err(|error| error.to_string())
            .and_then(|frame| writer.try_send(frame).map_err(str::to_string));
        match delivered {
            Ok(()) => {
                debug!(
                    child = self.context.index,
                    shard_id, "granted IDENTIFY permit"
                );
                self.identify_held.insert(
                    shard_id,
                    HeldPermit {
                        _lease: lease,
                        expires_at: deadline_after(self.context.identify_permit_timeout),
                    },
                );
            }
            // `lease` drops here, so the bucket moves on after its interval.
            Err(reason) => warn!(
                child = self.context.index,
                shard_id, "could not deliver identify grant ({reason}); releasing it"
            ),
        }
    }

    fn next_permit_expiry(&self) -> Option<Instant> {
        self.identify_held
            .values()
            .map(|held| held.expires_at)
            .min()
    }

    fn expire_identify_permits(&mut self) {
        let now = Instant::now();
        let index = self.context.index;
        let permit_timeout = self.context.identify_permit_timeout;
        self.identify_held.retain(|&shard_id, held| {
            let keep = held.expires_at > now;
            if !keep {
                warn!(
                    child = index,
                    shard_id,
                    "identify permit not released within {permit_timeout:?}; \
                     assuming IDENTIFY was sent"
                );
            }
            keep
        });
    }

    /// Leaves every IDENTIFY queue and returns held permits; used when the
    /// child is being stopped.
    fn release_identify(&mut self) {
        self.stopping = true;
        self.identify_waiting = FuturesUnordered::new();
        self.identify_queued.clear();
        self.identify_held.clear();
    }

    /// Processes stdout lines still buffered after the child exited.
    async fn drain_stdout(&mut self, reader: &mut FrameReader<ChildStdout>) {
        let _ = timeout(EXIT_DRAIN_TIMEOUT, async {
            while !reader.is_closed() {
                let event = reader.next_event().await;
                self.handle_stdout(event);
            }
        })
        .await;
    }
}

/// Runs one child process until it exits or shutdown is requested, pumping
/// IPC in both directions. None of the `select!` branches awaits child I/O
/// in its handler, so shutdown, exit, and broadcasts are always observed.
async fn supervise_child(
    context: &MonitorContext,
    mut child: Child,
    command_rx: &mut mpsc::Receiver<ChildCommand>,
    shutdown_rx: &mut watch::Receiver<bool>,
) -> ChildRunOutcome {
    let Some(stdout) = child.stdout.take() else {
        warn!(
            child = context.index,
            "child stdout pipe missing; killing it"
        );
        kill_and_reap(context, &mut child).await;
        return ChildRunOutcome::Exited;
    };
    let writer = IpcWriter::spawn(child.stdin.take());
    let mut reader = FrameReader::new(stdout, context.max_frame);
    let mut session = ChildSession::new(context);

    loop {
        let permit_expiry = session.next_permit_expiry();
        tokio::select! {
            () = wait_for_shutdown(shutdown_rx) => {
                graceful_stop(&mut child, writer, &mut reader, &mut session).await;
                return ChildRunOutcome::ShutdownRequested;
            }
            status = child.wait() => {
                match status {
                    Ok(status) => info!(child = context.index, %status, "shard child exited"),
                    Err(error) => warn!(child = context.index, "waiting for shard child failed: {error}"),
                }
                session.drain_stdout(&mut reader).await;
                return ChildRunOutcome::Exited;
            }
            command = command_rx.recv() => match command {
                Some(ChildCommand::Request { id, frame, respond_to }) => {
                    session.forward_request(&writer, id, frame, respond_to);
                }
                None => {
                    // The manager was dropped: do not leave orphans.
                    graceful_stop(&mut child, writer, &mut reader, &mut session).await;
                    return ChildRunOutcome::ShutdownRequested;
                }
            },
            event = reader.next_event(), if !reader.is_closed() => {
                session.handle_stdout(event);
            }
            Some((shard_id, lease)) = session.identify_waiting.next(),
                if !session.identify_waiting.is_empty() => {
                session.grant_identify(&writer, shard_id, lease);
            }
            () = sleep_until(permit_expiry.unwrap_or_else(Instant::now)),
                if permit_expiry.is_some() => {
                session.expire_identify_permits();
            }
        }
    }
}

/// Stops a child within `shutdown_timeout`, measured from the start of
/// this call: queue the `shutdown` op, close stdin, keep draining stdout
/// (a child blocked on a full stdout pipe could not exit), and kill the
/// child when the deadline passes.
async fn graceful_stop(
    child: &mut Child,
    mut writer: IpcWriter,
    reader: &mut FrameReader<ChildStdout>,
    session: &mut ChildSession<'_>,
) {
    let context = session.context;
    let deadline = deadline_after(context.shutdown_timeout);
    session.release_identify();

    let queued = frame_line(&ParentMessage::Shutdown)
        .map_err(|error| error.to_string())
        .and_then(|frame| writer.try_send(frame).map_err(str::to_string));
    if let Err(reason) = queued {
        warn!(
            child = context.index,
            "could not queue shutdown op ({reason}); relying on stdin EOF and the deadline"
        );
    }
    writer.close();

    let exited = timeout_at(deadline, async {
        loop {
            tokio::select! {
                status = child.wait() => return status,
                event = reader.next_event(), if !reader.is_closed() => {
                    session.handle_stdout(event);
                }
            }
        }
    })
    .await;

    match exited {
        Ok(Ok(status)) => {
            debug!(child = context.index, %status, "shard child exited after shutdown op")
        }
        Ok(Err(error)) => {
            warn!(
                child = context.index,
                "waiting for shard child failed ({error}); killing it"
            );
            kill_and_reap(context, child).await;
        }
        Err(_) => {
            warn!(
                child = context.index,
                timeout_ms = context.shutdown_timeout.as_millis() as u64,
                "shard child did not exit before the shutdown deadline; killing it"
            );
            kill_and_reap(context, child).await;
        }
    }
    // `writer` drops here and aborts a write that is still stuck.
}

async fn kill_and_reap(context: &MonitorContext, child: &mut Child) {
    if let Err(error) = child.start_kill() {
        debug!(
            child = context.index,
            "kill failed (child already gone?): {error}"
        );
    }
    if timeout(KILL_REAP_TIMEOUT, child.wait()).await.is_err() {
        warn!(
            child = context.index,
            "shard child was not reaped within {KILL_REAP_TIMEOUT:?} after kill"
        );
    }
}

// ---------------------------------------------------------------------------
// Child side
// ---------------------------------------------------------------------------

type FrameSink = Box<dyn Fn(&[u8]) -> io::Result<()> + Send + Sync>;
type FrameSource = Box<dyn BufRead + Send>;
type RequestFuture = Pin<Box<dyn Future<Output = Value> + Send>>;
type RequestHandler = Arc<dyn Fn(Value) -> RequestFuture + Send + Sync>;

#[derive(Clone)]
struct ServeHandler {
    handler: RequestHandler,
    runtime: tokio::runtime::Handle,
}

enum IdentifySlot {
    /// Requested from the parent; the sender delivers the permit.
    Pending(oneshot::Sender<IdentifyPermit>),
    /// Granted and not yet released.
    Held,
}

#[derive(Default)]
struct ChildIdentifyState {
    /// Set once the parent asked for shutdown or the IPC channel closed; no
    /// further permits will be granted.
    closed: bool,
    slots: HashMap<u32, IdentifySlot>,
}

/// Values a child reads from its environment.
#[derive(Debug, PartialEq, Eq)]
struct ChildSettings {
    shard_ids: Vec<u32>,
    shard_count: u32,
    max_frame: usize,
}

impl ChildSettings {
    fn parse(get: impl Fn(&str) -> Option<String>) -> Option<Self> {
        if get(ENV_SHARD_IPC).as_deref() != Some("1") {
            return None;
        }
        let Some(ids_raw) = get(ENV_SHARD_IDS) else {
            warn!("{ENV_SHARD_IPC}=1 but {ENV_SHARD_IDS} is missing; ignoring child mode");
            return None;
        };
        let Some(count_raw) = get(ENV_SHARD_COUNT) else {
            warn!("{ENV_SHARD_IPC}=1 but {ENV_SHARD_COUNT} is missing; ignoring child mode");
            return None;
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
        let max_frame = match get(ENV_SHARD_IPC_MAX_FRAME) {
            None => DEFAULT_MAX_IPC_FRAME_BYTES,
            Some(raw) => match raw.trim().parse::<usize>() {
                Ok(limit) => limit.max(MIN_IPC_FRAME_BYTES),
                Err(_) => {
                    warn!("malformed {ENV_SHARD_IPC_MAX_FRAME} value {raw:?}; using the default");
                    DEFAULT_MAX_IPC_FRAME_BYTES
                }
            },
        };
        Some(Self {
            shard_ids,
            shard_count,
            max_frame,
        })
    }
}

/// Shared child-side IPC state behind every [`ShardChildProcess`] clone and
/// every [`IdentifyPermit`].
struct ChildIpc {
    shard_ids: Vec<u32>,
    shard_count: u32,
    max_frame: usize,
    shutdown_tx: watch::Sender<bool>,
    /// Writes one complete frame (with newlines). Production code locks
    /// stdout so frames never interleave with each other or with a
    /// concurrent `println!`.
    sink: FrameSink,
    /// Taken by the reader thread when IPC starts.
    source: StdMutex<Option<FrameSource>>,
    handler: StdMutex<Option<ServeHandler>>,
    identify: StdMutex<ChildIdentifyState>,
}

impl ChildIpc {
    fn send(&self, message: &ChildMessage) -> Result<(), DiscordError> {
        self.send_encoded(encode_ipc_line(message)?)
    }

    fn send_encoded(&self, frame: String) -> Result<(), DiscordError> {
        if frame.len() > self.max_frame {
            return Err(DiscordError::model(format!(
                "IPC frame of {} bytes exceeds the parent's limit of {} bytes",
                frame.len(),
                self.max_frame
            )));
        }
        // The leading newline ends any unterminated `print!` output so the
        // prefix always starts a line on the parent's side.
        let mut bytes = Vec::with_capacity(frame.len() + 2);
        bytes.push(b'\n');
        bytes.extend_from_slice(frame.as_bytes());
        bytes.push(b'\n');
        (self.sink)(&bytes)
            .map_err(|error| DiscordError::gateway(format!("IPC stdout write failed: {error}")))
    }

    /// Answers a request. A response larger than the parent accepts would be
    /// discarded there and surface only as a timeout, so it is replaced by
    /// `null` here, where the reason can be logged.
    fn send_response(&self, id: u64, payload: Value) {
        let result = match encode_ipc_line(&ChildMessage::Response { id, payload }) {
            Ok(frame) if frame.len() <= self.max_frame => self.send_encoded(frame),
            Ok(frame) => {
                warn!(
                    request_id = id,
                    bytes = frame.len(),
                    limit = self.max_frame,
                    "broadcast response exceeds the IPC frame limit; answering null"
                );
                self.send(&ChildMessage::Response {
                    id,
                    payload: Value::Null,
                })
            }
            Err(error) => Err(error),
        };
        if let Err(error) = result {
            warn!(request_id = id, "failed to write IPC response: {error}");
        }
    }

    /// Starts the stdin reader thread once. A plain OS thread is used
    /// because blocking stdin reads must not occupy an async worker and
    /// tokio's stdin needs the `io-std` feature.
    fn start_reader(self: &Arc<Self>) {
        let Some(source) = lock_or_recover(&self.source).take() else {
            return;
        };
        let ipc = Arc::clone(self);
        let spawned = std::thread::Builder::new()
            .name("discordrs-shard-ipc".to_string())
            .spawn(move || ipc.read_loop(source));
        if let Err(error) = spawned {
            warn!("failed to start the IPC reader thread: {error}; treating as shutdown");
            self.close();
        }
    }

    fn read_loop(self: Arc<Self>, mut source: FrameSource) {
        let mut limiter = LineLimiter::new(self.max_frame);
        loop {
            let event = match source.fill_buf() {
                Ok([]) => {
                    if let Some(event) = limiter.finish() {
                        self.handle_parent_line(event);
                    }
                    break;
                }
                Ok(buffer) => {
                    let (consumed, event) = limiter.feed(buffer);
                    source.consume(consumed);
                    event
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    warn!("IPC stdin read failed: {error}");
                    break;
                }
            };
            if let Some(event) = event {
                self.handle_parent_line(event);
            }
        }
        // stdin closed: the parent went away or finished shutting us down.
        // Treat it as shutdown so the child does not linger as an orphan.
        info!("IPC stdin closed; treating as shutdown");
        self.close();
    }

    fn handle_parent_line(self: &Arc<Self>, event: LineEvent) {
        let line = match event {
            LineEvent::Line(line) => line,
            LineEvent::Oversized(bytes) => {
                warn!(
                    bytes,
                    limit = self.max_frame,
                    "discarded oversized parent IPC line"
                );
                return;
            }
        };
        match classify_line::<ParentMessage>(&line) {
            // Nothing but the parent writes to stdin; ignore stray input.
            IpcLine::Empty | IpcLine::Foreign => {}
            IpcLine::Malformed(error) => {
                warn!(
                    "malformed parent IPC frame ({error}): {}",
                    printable_line(&line)
                );
            }
            IpcLine::Message(ParentMessage::Shutdown) => {
                info!("shutdown op received from shard manager");
                self.close();
            }
            IpcLine::Message(ParentMessage::Request { id, payload }) => {
                self.dispatch_request(id, payload);
            }
            IpcLine::Message(ParentMessage::IdentifyGrant { shard_id }) => {
                self.grant_identify(shard_id);
            }
        }
    }

    fn dispatch_request(self: &Arc<Self>, id: u64, payload: Value) {
        let serve = lock_or_recover(&self.handler).clone();
        match serve {
            None => {
                debug!(request_id = id, "request before serve(); answering null");
                self.send_response(id, Value::Null);
            }
            Some(serve) => {
                // One task per request so a slow handler never delays
                // shutdown or IDENTIFY grants that arrive behind it.
                let ipc = Arc::clone(self);
                serve.runtime.spawn(async move {
                    let response = (serve.handler)(payload).await;
                    ipc.send_response(id, response);
                });
            }
        }
    }

    fn grant_identify(self: &Arc<Self>, shard_id: u32) {
        let waiter = {
            let mut state = lock_or_recover(&self.identify);
            match state.slots.get_mut(&shard_id) {
                Some(slot) if matches!(slot, IdentifySlot::Pending(_)) => {
                    match std::mem::replace(slot, IdentifySlot::Held) {
                        IdentifySlot::Pending(waiter) => Some(waiter),
                        IdentifySlot::Held => None,
                    }
                }
                _ => None,
            }
        };
        match waiter {
            Some(waiter) => {
                let permit = IdentifyPermit {
                    ipc: Arc::clone(self),
                    shard_id,
                };
                // If the acquirer gave up, the permit comes back and its
                // drop releases the slot at once.
                if let Err(permit) = waiter.send(permit) {
                    drop(permit);
                }
            }
            None => {
                warn!(
                    shard_id,
                    "identify grant without a pending request; releasing it"
                );
                if let Err(error) = self.send(&ChildMessage::IdentifyRelease { shard_id }) {
                    warn!(
                        shard_id,
                        "failed to release unexpected identify grant: {error}"
                    );
                }
            }
        }
    }

    /// Signals shutdown and fails acquisitions that are still waiting; held
    /// permits stay valid until dropped.
    fn close(&self) {
        self.shutdown_tx.send_replace(true);
        let mut state = lock_or_recover(&self.identify);
        state.closed = true;
        state
            .slots
            .retain(|_, slot| matches!(slot, IdentifySlot::Held));
    }
}

fn write_stdout_frame(bytes: &[u8]) -> io::Result<()> {
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(bytes)?;
    stdout.flush()
}

/// The child-process half of the IPC protocol.
///
/// Construct with [`ShardChildProcess::from_env`]: it returns `Some` only
/// when the process was launched by a [`ProcessShardManager`] (detected via
/// `DISCORDRS_SHARD_IPC=1`), so the same binary runs standalone otherwise.
/// The handle is cheap to clone and can be moved into shard tasks and event
/// handlers.
///
/// A typical child calls [`Self::serve`], then for each assigned shard
/// awaits [`Self::acquire_identify`], starts the gateway client, and drops
/// the permit plus calls [`Self::notify_shard_ready`] when READY arrives.
/// Finally it awaits [`Self::shutdown_signal`].
///
/// # stdin and stdout
///
/// IPC frames share stdout with any other output. Stray prints no longer
/// break the protocol (the parent logs them), but logs belong on stderr,
/// which the parent passes through unchanged. **Never read stdin** in child
/// mode: the IPC reader owns it.
#[derive(Clone)]
pub struct ShardChildProcess {
    ipc: Arc<ChildIpc>,
}

impl ShardChildProcess {
    /// Detects child mode from the environment.
    ///
    /// Returns `Some` when `DISCORDRS_SHARD_IPC=1` and the
    /// `DISCORDRS_SHARD_IDS` / `DISCORDRS_SHARD_COUNT` variables parse
    /// cleanly. Otherwise, including on malformed values (which are logged
    /// via `tracing`), it returns `None`, so a standalone launch of the same
    /// binary falls through to parent/normal mode.
    pub fn from_env() -> Option<Self> {
        let settings = ChildSettings::parse(|key| std::env::var(key).ok())?;
        Some(Self::with_io(
            settings,
            Box::new(std::io::BufReader::new(std::io::stdin())),
            Box::new(write_stdout_frame),
        ))
    }

    fn with_io(settings: ChildSettings, source: FrameSource, sink: FrameSink) -> Self {
        let (shutdown_tx, _) = watch::channel(false);
        Self {
            ipc: Arc::new(ChildIpc {
                shard_ids: settings.shard_ids,
                shard_count: settings.shard_count,
                max_frame: settings.max_frame,
                shutdown_tx,
                sink,
                source: StdMutex::new(Some(source)),
                handler: StdMutex::new(None),
                identify: StdMutex::new(ChildIdentifyState::default()),
            }),
        }
    }

    /// Shard ids assigned to this process.
    pub fn shard_ids(&self) -> &[u32] {
        &self.ipc.shard_ids
    }

    /// Total shard count across all processes.
    pub fn shard_count(&self) -> u32 {
        self.ipc.shard_count
    }

    /// Reports every assigned shard as ready. Prefer
    /// [`Self::notify_shard_ready`] from each shard's READY handler for an
    /// accurate status.
    pub fn notify_ready(&self) -> Result<(), DiscordError> {
        for &shard_id in &self.ipc.shard_ids {
            self.ipc.send(&ChildMessage::Ready { shard_id })?;
        }
        Ok(())
    }

    /// Reports one assigned shard as ready. Fails for a shard that is not
    /// assigned to this process.
    pub fn notify_shard_ready(&self, shard_id: u32) -> Result<(), DiscordError> {
        self.ensure_assigned(shard_id)?;
        self.ipc.send(&ChildMessage::Ready { shard_id })
    }

    /// Sends a `log` line to the parent, which surfaces it via `tracing`.
    /// Fails if the encoded line exceeds the parent's frame limit.
    pub fn log(&self, message: impl Into<String>) -> Result<(), DiscordError> {
        self.ipc.send(&ChildMessage::Log {
            message: message.into(),
        })
    }

    /// Starts serving broadcast requests: each `request` op is answered with
    /// the value `handler` produces, run as its own Tokio task. Also starts
    /// the IPC reader if it is not running yet, so [`Self::shutdown_signal`]
    /// and IDENTIFY grants work.
    ///
    /// Requests that arrive before `serve` is called are answered with
    /// `null`, so call it early. Calling it again replaces the handler.
    /// The process is **not** exited automatically; await
    /// [`Self::shutdown_signal`] and tear down gracefully.
    ///
    /// Fails when called outside a Tokio runtime.
    pub fn serve<F, Fut>(&self, handler: F) -> Result<(), DiscordError>
    where
        F: Fn(Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Value> + Send + 'static,
    {
        let runtime = tokio::runtime::Handle::try_current().map_err(|_| {
            DiscordError::model("ShardChildProcess::serve must be called inside a Tokio runtime")
        })?;
        let handler: RequestHandler =
            Arc::new(move |payload| -> RequestFuture { Box::pin(handler(payload)) });
        *lock_or_recover(&self.ipc.handler) = Some(ServeHandler { handler, runtime });
        self.ipc.start_reader();
        Ok(())
    }

    /// Waits for the parent to grant `shard_id` an IDENTIFY slot.
    ///
    /// The parent grants at most one permit per bucket
    /// (`shard_id % max_concurrency`) at a time, across every child process,
    /// and spaces grants by `identify_interval` after each release. Hold the
    /// returned [`IdentifyPermit`] until the shard has sent IDENTIFY;
    /// dropping it when READY arrives (or when the shard fails to start) is
    /// the simplest correct choice. A permit that is never released is
    /// reclaimed after `identify_permit_timeout`.
    ///
    /// Cancelling this future is safe: a grant that arrives later is
    /// released automatically, and a new call for the same shard takes over
    /// the request that is already queued.
    ///
    /// # Errors
    ///
    /// Fails if `shard_id` is not assigned to this process, if a permit for
    /// it is already held or being acquired, if the request cannot be
    /// written, or if shutdown is requested (or stdin closes) before the
    /// grant arrives.
    pub async fn acquire_identify(&self, shard_id: u32) -> Result<IdentifyPermit, DiscordError> {
        self.ensure_assigned(shard_id)?;
        let (waiter, permit_rx) = oneshot::channel();
        let send_request = {
            let mut state = lock_or_recover(&self.ipc.identify);
            if state.closed {
                return Err(DiscordError::gateway(
                    "shard manager requested shutdown; no IDENTIFY permits will be granted",
                ));
            }
            let send_request = match state.slots.get(&shard_id) {
                Some(IdentifySlot::Held) => {
                    return Err(DiscordError::model(format!(
                        "an IDENTIFY permit for shard {shard_id} is already held"
                    )));
                }
                Some(IdentifySlot::Pending(existing)) if !existing.is_closed() => {
                    return Err(DiscordError::model(format!(
                        "an IDENTIFY permit for shard {shard_id} is already being acquired"
                    )));
                }
                // A cancelled acquisition left its request queued in the
                // parent: take it over instead of queueing a second one.
                Some(IdentifySlot::Pending(_)) => false,
                None => true,
            };
            state.slots.insert(shard_id, IdentifySlot::Pending(waiter));
            send_request
        };
        self.ipc.start_reader();
        if send_request {
            if let Err(error) = self.ipc.send(&ChildMessage::IdentifyRequest { shard_id }) {
                lock_or_recover(&self.ipc.identify).slots.remove(&shard_id);
                return Err(error);
            }
        }
        permit_rx.await.map_err(|_| {
            DiscordError::gateway(format!(
                "IPC closed before the IDENTIFY permit for shard {shard_id} was granted"
            ))
        })
    }

    /// Future that resolves once the parent has requested shutdown (via the
    /// `shutdown` op or by closing stdin, which also happens when the parent
    /// dies, so children never linger as orphans). Starts the IPC reader if
    /// needed. Can be called multiple times.
    pub fn shutdown_signal(&self) -> impl Future<Output = ()> + Send + 'static {
        self.ipc.start_reader();
        let mut shutdown_rx = self.ipc.shutdown_tx.subscribe();
        async move {
            // An error means the sender is gone: nothing more will arrive,
            // so unblock the caller.
            let _ = shutdown_rx.wait_for(|requested| *requested).await;
        }
    }

    fn ensure_assigned(&self, shard_id: u32) -> Result<(), DiscordError> {
        if self.ipc.shard_ids.contains(&shard_id) {
            Ok(())
        } else {
            Err(DiscordError::model(format!(
                "shard {shard_id} is not assigned to this child process (assigned: {:?})",
                self.ipc.shard_ids
            )))
        }
    }
}

/// Permission from the parent for one shard to send IDENTIFY, obtained with
/// [`ShardChildProcess::acquire_identify`].
///
/// Dropping the permit (or calling [`Self::release`]) tells the parent the
/// IDENTIFY has gone out, and the shard's bucket becomes available again
/// after `identify_interval`.
#[must_use = "dropping the permit releases the IDENTIFY slot immediately; hold it until READY"]
pub struct IdentifyPermit {
    ipc: Arc<ChildIpc>,
    shard_id: u32,
}

impl IdentifyPermit {
    /// Shard this permit was granted for.
    pub fn shard_id(&self) -> u32 {
        self.shard_id
    }

    /// Releases the permit now; equivalent to dropping it.
    pub fn release(self) {}
}

impl std::fmt::Debug for IdentifyPermit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IdentifyPermit")
            .field("shard_id", &self.shard_id)
            .finish()
    }
}

impl Drop for IdentifyPermit {
    fn drop(&mut self) {
        // Send the release before freeing the slot: once the slot is free a
        // new acquire may write its request, and the parent must see the
        // release first or it would reject the request as a duplicate.
        if let Err(error) = self.ipc.send(&ChildMessage::IdentifyRelease {
            shard_id: self.shard_id,
        }) {
            warn!(
                shard_id = self.shard_id,
                "failed to release IDENTIFY permit: {error}"
            );
        }
        lock_or_recover(&self.ipc.identify)
            .slots
            .remove(&self.shard_id);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SessionStartLimit;
    use serde_json::json;
    use tokio::time::sleep;

    // -- protocol -----------------------------------------------------------

    #[test]
    fn parent_messages_round_trip_with_expected_wire_format() {
        assert_eq!(IPC_FRAME_PREFIX, "@discordrs-ipc/1 ");

        let shutdown = ParentMessage::Shutdown;
        let encoded = encode_ipc_line(&shutdown).unwrap();
        assert_eq!(encoded, r#"@discordrs-ipc/1 {"op":"shutdown"}"#);
        assert_eq!(
            decode_ipc_line::<ParentMessage>(&encoded).unwrap(),
            shutdown
        );

        let request = ParentMessage::Request {
            id: 7,
            payload: json!({"kind": "stats"}),
        };
        let encoded = encode_ipc_line(&request).unwrap();
        assert_eq!(
            encoded,
            r#"@discordrs-ipc/1 {"op":"request","id":7,"payload":{"kind":"stats"}}"#
        );
        assert_eq!(
            decode_ipc_line::<ParentMessage>(&format!("  {encoded}\n")).unwrap(),
            request
        );

        let grant = ParentMessage::IdentifyGrant { shard_id: 3 };
        let encoded = encode_ipc_line(&grant).unwrap();
        assert_eq!(
            encoded,
            r#"@discordrs-ipc/1 {"op":"identify_grant","shard_id":3}"#
        );
        assert_eq!(decode_ipc_line::<ParentMessage>(&encoded).unwrap(), grant);
    }

    #[test]
    fn child_messages_round_trip_with_expected_wire_format() {
        let cases = [
            (
                ChildMessage::Ready { shard_id: 3 },
                r#"{"op":"ready","shard_id":3}"#,
            ),
            (
                ChildMessage::Response {
                    id: 7,
                    payload: json!([1, 2]),
                },
                r#"{"op":"response","id":7,"payload":[1,2]}"#,
            ),
            (
                ChildMessage::Log {
                    message: "hello".to_string(),
                },
                r#"{"op":"log","message":"hello"}"#,
            ),
            (
                ChildMessage::IdentifyRequest { shard_id: 4 },
                r#"{"op":"identify_request","shard_id":4}"#,
            ),
            (
                ChildMessage::IdentifyRelease { shard_id: 4 },
                r#"{"op":"identify_release","shard_id":4}"#,
            ),
        ];
        for (message, json) in cases {
            let encoded = encode_ipc_line(&message).unwrap();
            assert_eq!(encoded, format!("{IPC_FRAME_PREFIX}{json}"));
            assert_eq!(decode_ipc_line::<ChildMessage>(&encoded).unwrap(), message);
        }
        assert!(decode_ipc_line::<ChildMessage>(&format!(
            "{IPC_FRAME_PREFIX}{{\"op\":\"bogus\"}}"
        ))
        .is_err());
    }

    #[test]
    fn decode_ipc_line_requires_the_frame_prefix() {
        let error = decode_ipc_line::<ChildMessage>(r#"{"op":"ready","shard_id":0}"#).unwrap_err();
        assert!(error.to_string().contains("not an IPC frame"), "{error}");
    }

    #[test]
    fn request_frame_matches_serde_encoding() {
        let payload = json!({"kind": "stats", "values": [1, 2.5, null], "text": "a\"b\n"});
        let frame = request_frame(42, &serde_json::to_string(&payload).unwrap());
        assert!(frame.ends_with('\n'));
        let expected = encode_ipc_line(&ParentMessage::Request { id: 42, payload }).unwrap();
        assert_eq!(frame.trim_end_matches('\n'), expected);
    }

    // -- framing ------------------------------------------------------------

    #[test]
    fn line_limiter_splits_lines_across_reads() {
        let mut limiter = LineLimiter::new(64);
        assert_eq!(limiter.feed(b"hel"), (3, None));
        assert_eq!(
            limiter.feed(b"lo\nwor"),
            (3, Some(LineEvent::Line(b"hello".to_vec())))
        );
        assert_eq!(limiter.feed(b"wor"), (3, None));
        assert_eq!(limiter.finish(), Some(LineEvent::Line(b"wor".to_vec())));
        assert_eq!(limiter.finish(), None);
    }

    #[test]
    fn line_limiter_discards_oversized_lines_without_buffering_them() {
        let mut limiter = LineLimiter::new(16);
        let chunk = [b'x'; 1024];
        // 10 MiB without a newline: the limiter must never hold more than
        // its limit (the old `lines()` reader buffered all of it).
        for _ in 0..10 * 1024 {
            assert_eq!(limiter.feed(&chunk), (1024, None));
            assert!(limiter.line.len() <= 16);
        }
        assert_eq!(
            limiter.feed(b"tail\nok\n"),
            (5, Some(LineEvent::Oversized(10 * 1024 * 1024 + 4)))
        );
        assert_eq!(
            limiter.feed(b"ok\n"),
            (3, Some(LineEvent::Line(b"ok".to_vec())))
        );

        // Crossing the limit only when the newline arrives.
        let mut limiter = LineLimiter::new(4);
        assert_eq!(limiter.feed(b"abc"), (3, None));
        assert_eq!(limiter.feed(b"de\n"), (3, Some(LineEvent::Oversized(5))));
        // Exactly at the limit is fine.
        assert_eq!(
            limiter.feed(b"abcd\n"),
            (5, Some(LineEvent::Line(b"abcd".to_vec())))
        );
        // An oversized unterminated tail is reported at end of stream.
        assert_eq!(limiter.feed(b"abcdefgh"), (8, None));
        assert_eq!(limiter.finish(), Some(LineEvent::Oversized(8)));
    }

    #[test]
    fn classify_line_separates_frames_from_other_output() {
        assert!(matches!(classify_line::<ChildMessage>(b""), IpcLine::Empty));
        assert!(matches!(
            classify_line::<ChildMessage>(b" \r"),
            IpcLine::Empty
        ));
        assert!(matches!(
            classify_line::<ChildMessage>(b"hello world"),
            IpcLine::Foreign
        ));
        assert!(matches!(
            classify_line::<ChildMessage>(b"\xff\xfe binary"),
            IpcLine::Foreign
        ));
        // Unframed JSON (the 2.x wire format) is not protocol traffic.
        assert!(matches!(
            classify_line::<ChildMessage>(br#"{"op":"ready","shard_id":0}"#),
            IpcLine::Foreign
        ));
        assert!(matches!(
            classify_line::<ChildMessage>(br#"@discordrs-ipc/2 {"op":"ready","shard_id":0}"#),
            IpcLine::Foreign
        ));
        assert!(matches!(
            classify_line::<ChildMessage>(b"@discordrs-ipc/1 {broken"),
            IpcLine::Malformed(_)
        ));
        match classify_line::<ChildMessage>(b"@discordrs-ipc/1 {\"op\":\"ready\",\"shard_id\":4}\r")
        {
            IpcLine::Message(message) => assert_eq!(message, ChildMessage::Ready { shard_id: 4 }),
            other => panic!("expected a message, got {other:?}"),
        }
    }

    #[test]
    fn printable_line_is_lossy_and_truncated() {
        assert_eq!(printable_line(b"abc\r"), "abc");
        assert_eq!(printable_line(b"\xffok"), "\u{fffd}ok");
        let long = printable_line(&[b'y'; LOGGED_LINE_CHARS + 10]);
        assert_eq!(long.len(), LOGGED_LINE_CHARS + 3);
        assert!(long.ends_with("..."));
    }

    #[tokio::test]
    async fn frame_reader_yields_bounded_lines_then_eof() {
        let input: &[u8] = b"one\n\xff\xfe\nxxxxxxxxxxxxxxxxxxxxxxxx\ntail";
        let mut reader = FrameReader::new(input, 8);
        assert_eq!(
            reader.next_event().await.unwrap(),
            Some(LineEvent::Line(b"one".to_vec()))
        );
        assert_eq!(
            reader.next_event().await.unwrap(),
            Some(LineEvent::Line(vec![0xff, 0xfe]))
        );
        assert_eq!(
            reader.next_event().await.unwrap(),
            Some(LineEvent::Oversized(24))
        );
        assert!(!reader.is_closed());
        assert_eq!(
            reader.next_event().await.unwrap(),
            Some(LineEvent::Line(b"tail".to_vec()))
        );
        assert_eq!(reader.next_event().await.unwrap(), None);
        assert!(reader.is_closed());
    }

    // -- backoff, settings, plan ---------------------------------------------

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
    fn deadline_after_clamps_huge_durations() {
        let before = Instant::now();
        assert!(deadline_after(Duration::MAX) >= before + FAR_FUTURE);
        assert!(deadline_after(Duration::ZERO) >= before);
    }

    #[test]
    fn from_env_requires_ipc_marker() {
        // Env vars are absent in the test process, so this must be None.
        assert!(ShardChildProcess::from_env().is_none());
    }

    fn env_lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        move |key| map.get(key).cloned()
    }

    #[test]
    fn child_settings_parse_environment() {
        let base = [
            (ENV_SHARD_IPC, "1"),
            (ENV_SHARD_IDS, "0, 2"),
            (ENV_SHARD_COUNT, "4"),
        ];
        assert_eq!(
            ChildSettings::parse(env_lookup(&base)),
            Some(ChildSettings {
                shard_ids: vec![0, 2],
                shard_count: 4,
                max_frame: DEFAULT_MAX_IPC_FRAME_BYTES,
            })
        );

        let with = |extra: (&'static str, &'static str)| {
            let mut pairs: Vec<(&str, &str)> = base
                .iter()
                .copied()
                .filter(|(key, _)| *key != extra.0)
                .collect();
            pairs.push(extra);
            ChildSettings::parse(env_lookup(&pairs))
        };
        assert_eq!(
            with((ENV_SHARD_IPC_MAX_FRAME, "65536")).map(|s| s.max_frame),
            Some(65536)
        );
        assert_eq!(
            with((ENV_SHARD_IPC_MAX_FRAME, "10")).map(|s| s.max_frame),
            Some(MIN_IPC_FRAME_BYTES)
        );
        assert_eq!(
            with((ENV_SHARD_IPC_MAX_FRAME, "lots")).map(|s| s.max_frame),
            Some(DEFAULT_MAX_IPC_FRAME_BYTES)
        );
        assert_eq!(with((ENV_SHARD_IPC, "0")), None);
        assert_eq!(with((ENV_SHARD_IDS, "0,x")), None);
        assert_eq!(with((ENV_SHARD_IDS, "")), None);
        assert_eq!(with((ENV_SHARD_COUNT, "0")), None);

        assert_eq!(ChildSettings::parse(env_lookup(&[])), None);
        assert_eq!(
            ChildSettings::parse(env_lookup(&[(ENV_SHARD_IPC, "1"), (ENV_SHARD_COUNT, "1")])),
            None
        );
        assert_eq!(
            ChildSettings::parse(env_lookup(&[(ENV_SHARD_IPC, "1"), (ENV_SHARD_IDS, "0")])),
            None
        );
    }

    #[test]
    fn identify_plan_reads_gateway_bot_limits() {
        let mut gateway_bot = GatewayBot {
            url: String::new(),
            shards: 0,
            session_start_limit: SessionStartLimit {
                total: 1000,
                remaining: 0,
                reset_after: 1500,
                max_concurrency: 0,
            },
        };
        assert_eq!(
            IdentifyPlan::from_gateway_bot(&gateway_bot),
            IdentifyPlan {
                shards: 1,
                max_concurrency: 1,
                initial_delay: Duration::from_millis(1500),
            }
        );
        gateway_bot.shards = 12;
        gateway_bot.session_start_limit.remaining = 5;
        gateway_bot.session_start_limit.max_concurrency = 4;
        assert_eq!(
            IdentifyPlan::from_gateway_bot(&gateway_bot),
            IdentifyPlan {
                shards: 12,
                max_concurrency: 4,
                initial_delay: Duration::ZERO,
            }
        );
    }

    // -- identify gate --------------------------------------------------------

    #[test]
    fn identify_gate_buckets_follow_shard_modulo_max_concurrency() {
        let gate = IdentifyGate::new(2, 10, Duration::ZERO, Instant::now());
        assert_eq!(gate.buckets.len(), 2);
        assert_eq!(gate.bucket_of(7), 1);
        assert_eq!(gate.bucket_of(8), 0);

        // More concurrency than shards: one bucket per shard.
        let gate = IdentifyGate::new(16, 4, Duration::ZERO, Instant::now());
        assert_eq!(gate.buckets.len(), 4);
        assert_eq!(gate.bucket_of(3), 3);
        assert!(gate.bucket_of(100) < 4);

        let gate = IdentifyGate::new(0, 0, Duration::ZERO, Instant::now());
        assert_eq!(gate.buckets.len(), 1);
        assert_eq!(gate.bucket_of(5), 0);
    }

    #[tokio::test]
    async fn identify_gate_spaces_same_bucket_grants_from_release() {
        let interval = Duration::from_millis(150);
        let gate = IdentifyGate::new(2, 4, interval, Instant::now());
        let first = gate.acquire(0).await;

        // A different bucket does not wait for bucket 0.
        let other = timeout(Duration::from_secs(1), gate.acquire(1))
            .await
            .expect("bucket 1 must not wait for bucket 0");

        // The same bucket waits until the permit is released...
        let mut same = Box::pin(gate.acquire(2));
        assert!(timeout(Duration::from_millis(100), &mut same)
            .await
            .is_err());
        let released_at = Instant::now();
        drop(first);
        // ...and then for the interval measured from the release.
        let _second = timeout(Duration::from_secs(2), &mut same)
            .await
            .expect("granted after release");
        assert!(released_at.elapsed() >= interval);
        drop(other);
    }

    #[tokio::test]
    async fn identify_gate_queues_fifo_and_skips_cancelled_waiters() {
        let gate = IdentifyGate::new(1, 3, Duration::ZERO, Instant::now());
        let held = gate.acquire(0).await;
        let mut cancelled = Box::pin(gate.acquire(1));
        let mut second = Box::pin(gate.acquire(2));
        let mut third = Box::pin(gate.acquire(0));
        assert!(futures_util::poll!(&mut cancelled).is_pending());
        assert!(futures_util::poll!(&mut second).is_pending());
        assert!(futures_util::poll!(&mut third).is_pending());

        drop(cancelled);
        drop(held);
        let lease = timeout(Duration::from_secs(1), &mut second)
            .await
            .expect("the oldest live waiter is next");
        assert!(futures_util::poll!(&mut third).is_pending());
        drop(lease);
        let _third = timeout(Duration::from_secs(1), &mut third)
            .await
            .expect("third waiter follows");
    }

    #[tokio::test]
    async fn identify_gate_honours_initial_delay() {
        let start = Instant::now();
        let gate = IdentifyGate::new(1, 1, Duration::ZERO, start + Duration::from_millis(120));
        let _lease = gate.acquire(0).await;
        assert!(start.elapsed() >= Duration::from_millis(120));
    }

    // -- child side (in-process, in-memory pipes) ------------------------------

    /// Blocking reader fed through a channel, standing in for stdin.
    struct ChannelSource {
        rx: std::sync::mpsc::Receiver<Vec<u8>>,
        chunk: Vec<u8>,
        pos: usize,
    }

    impl ChannelSource {
        fn new(rx: std::sync::mpsc::Receiver<Vec<u8>>) -> Self {
            Self {
                rx,
                chunk: Vec::new(),
                pos: 0,
            }
        }
    }

    impl io::Read for ChannelSource {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            while self.pos >= self.chunk.len() {
                match self.rx.recv() {
                    Ok(chunk) => {
                        self.chunk = chunk;
                        self.pos = 0;
                    }
                    Err(_) => return Ok(0),
                }
            }
            let count = buf.len().min(self.chunk.len() - self.pos);
            buf[..count].copy_from_slice(&self.chunk[self.pos..self.pos + count]);
            self.pos += count;
            Ok(count)
        }
    }

    fn child_settings(shard_ids: &[u32], max_frame: usize) -> ChildSettings {
        ChildSettings {
            shard_ids: shard_ids.to_vec(),
            shard_count: 8,
            max_frame,
        }
    }

    struct ChildHarness {
        child: ShardChildProcess,
        to_child: Option<std::sync::mpsc::Sender<Vec<u8>>>,
        from_child: mpsc::UnboundedReceiver<Vec<u8>>,
    }

    fn child_harness(shard_ids: &[u32]) -> ChildHarness {
        let (to_child, rx) = std::sync::mpsc::channel();
        let (frame_tx, from_child) = mpsc::unbounded_channel();
        let child = ShardChildProcess::with_io(
            child_settings(shard_ids, MIN_IPC_FRAME_BYTES),
            Box::new(std::io::BufReader::new(ChannelSource::new(rx))),
            Box::new(move |bytes: &[u8]| {
                frame_tx
                    .send(bytes.to_vec())
                    .map_err(|_| io::Error::from(io::ErrorKind::BrokenPipe))
            }),
        );
        ChildHarness {
            child,
            to_child: Some(to_child),
            from_child,
        }
    }

    impl ChildHarness {
        fn send_raw(&self, bytes: &[u8]) {
            self.to_child
                .as_ref()
                .expect("stdin already closed")
                .send(bytes.to_vec())
                .unwrap();
        }

        fn send(&self, message: &ParentMessage) {
            self.send_raw(frame_line(message).unwrap().as_bytes());
        }

        fn close_stdin(&mut self) {
            self.to_child = None;
        }

        async fn next_message(&mut self) -> ChildMessage {
            let bytes = timeout(Duration::from_secs(5), self.from_child.recv())
                .await
                .expect("timed out waiting for a child frame")
                .expect("child sink closed");
            assert!(
                bytes.starts_with(b"\n") && bytes.ends_with(b"\n"),
                "frames are written as \\n<frame>\\n: {bytes:?}"
            );
            decode_ipc_line(std::str::from_utf8(&bytes).unwrap()).unwrap()
        }

        async fn assert_quiet(&mut self) {
            sleep(Duration::from_millis(50)).await;
            assert!(
                self.from_child.try_recv().is_err(),
                "unexpected child frame"
            );
        }
    }

    #[tokio::test]
    async fn child_answers_null_before_serve_and_ignores_garbage_input() {
        let mut harness = child_harness(&[0]);
        // Starting the shutdown signal starts the reader without a handler.
        let signal = harness.child.shutdown_signal();
        harness.send(&ParentMessage::Request {
            id: 1,
            payload: json!("early"),
        });
        assert_eq!(
            harness.next_message().await,
            ChildMessage::Response {
                id: 1,
                payload: Value::Null
            }
        );

        harness
            .child
            .serve(|payload| async move { json!({ "echo": payload }) })
            .unwrap();
        harness.send_raw(b"plain text from somewhere\n");
        harness.send_raw(b"@discordrs-ipc/1 {broken\n");
        harness.send_raw(&vec![b'x'; MIN_IPC_FRAME_BYTES * 3]);
        harness.send_raw(b"\n\xff\xfe\n\n");
        harness.send(&ParentMessage::Request {
            id: 2,
            payload: json!({"kind": "stats"}),
        });
        assert_eq!(
            harness.next_message().await,
            ChildMessage::Response {
                id: 2,
                payload: json!({"echo": {"kind": "stats"}})
            }
        );

        harness.send(&ParentMessage::Shutdown);
        timeout(Duration::from_secs(5), signal)
            .await
            .expect("shutdown op must resolve the signal");
    }

    #[tokio::test]
    async fn child_identify_permit_round_trip() {
        let mut harness = child_harness(&[0, 1]);
        let child = harness.child.clone();
        let acquire = tokio::spawn(async move { child.acquire_identify(1).await });
        assert_eq!(
            harness.next_message().await,
            ChildMessage::IdentifyRequest { shard_id: 1 }
        );
        harness.send(&ParentMessage::IdentifyGrant { shard_id: 1 });
        let permit = timeout(Duration::from_secs(5), acquire)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(permit.shard_id(), 1);
        assert_eq!(format!("{permit:?}"), "IdentifyPermit { shard_id: 1 }");

        let error = harness.child.acquire_identify(1).await.unwrap_err();
        assert!(error.to_string().contains("already held"), "{error}");

        permit.release();
        assert_eq!(
            harness.next_message().await,
            ChildMessage::IdentifyRelease { shard_id: 1 }
        );

        // The slot is free again after the release.
        let child = harness.child.clone();
        let again = tokio::spawn(async move { child.acquire_identify(1).await });
        assert_eq!(
            harness.next_message().await,
            ChildMessage::IdentifyRequest { shard_id: 1 }
        );
        harness.send(&ParentMessage::IdentifyGrant { shard_id: 1 });
        drop(again.await.unwrap().unwrap());
        assert_eq!(
            harness.next_message().await,
            ChildMessage::IdentifyRelease { shard_id: 1 }
        );
    }

    #[tokio::test]
    async fn child_acquire_rejects_unassigned_and_concurrent_requests() {
        let mut harness = child_harness(&[0]);
        let error = harness.child.acquire_identify(3).await.unwrap_err();
        assert!(error.to_string().contains("not assigned"), "{error}");

        let child = harness.child.clone();
        let first = tokio::spawn(async move { child.acquire_identify(0).await });
        assert_eq!(
            harness.next_message().await,
            ChildMessage::IdentifyRequest { shard_id: 0 }
        );
        let error = harness.child.acquire_identify(0).await.unwrap_err();
        assert!(
            error.to_string().contains("already being acquired"),
            "{error}"
        );

        harness.send(&ParentMessage::IdentifyGrant { shard_id: 0 });
        drop(first.await.unwrap().unwrap());
        assert_eq!(
            harness.next_message().await,
            ChildMessage::IdentifyRelease { shard_id: 0 }
        );
    }

    #[tokio::test]
    async fn cancelled_acquire_releases_late_grants_and_can_be_resumed() {
        let mut harness = child_harness(&[0, 1]);

        // Cancelled before the grant: a late grant is handed straight back.
        assert!(
            timeout(Duration::from_millis(50), harness.child.acquire_identify(0))
                .await
                .is_err()
        );
        assert_eq!(
            harness.next_message().await,
            ChildMessage::IdentifyRequest { shard_id: 0 }
        );
        harness.send(&ParentMessage::IdentifyGrant { shard_id: 0 });
        assert_eq!(
            harness.next_message().await,
            ChildMessage::IdentifyRelease { shard_id: 0 }
        );

        // Cancelled and retried: the retry reuses the queued request.
        assert!(
            timeout(Duration::from_millis(50), harness.child.acquire_identify(1))
                .await
                .is_err()
        );
        assert_eq!(
            harness.next_message().await,
            ChildMessage::IdentifyRequest { shard_id: 1 }
        );
        let child = harness.child.clone();
        let retry = tokio::spawn(async move { child.acquire_identify(1).await });
        harness.assert_quiet().await;
        harness.send(&ParentMessage::IdentifyGrant { shard_id: 1 });
        let permit = timeout(Duration::from_secs(5), retry)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        drop(permit);
        assert_eq!(
            harness.next_message().await,
            ChildMessage::IdentifyRelease { shard_id: 1 }
        );
    }

    #[tokio::test]
    async fn shutdown_op_resolves_signal_and_fails_identify_waiters() {
        let mut harness = child_harness(&[0]);
        let child = harness.child.clone();
        let waiting = tokio::spawn(async move { child.acquire_identify(0).await });
        assert_eq!(
            harness.next_message().await,
            ChildMessage::IdentifyRequest { shard_id: 0 }
        );

        harness.send(&ParentMessage::Shutdown);
        timeout(Duration::from_secs(5), harness.child.shutdown_signal())
            .await
            .expect("shutdown signal");
        let error = waiting.await.unwrap().unwrap_err();
        assert!(error.to_string().contains("IPC closed"), "{error}");
        let error = harness.child.acquire_identify(0).await.unwrap_err();
        assert!(error.to_string().contains("shutdown"), "{error}");

        // A grant that still arrives is not kept.
        harness.send(&ParentMessage::IdentifyGrant { shard_id: 0 });
        assert_eq!(
            harness.next_message().await,
            ChildMessage::IdentifyRelease { shard_id: 0 }
        );
    }

    #[tokio::test]
    async fn stdin_eof_counts_as_shutdown() {
        let mut harness = child_harness(&[0]);
        let signal = harness.child.shutdown_signal();
        // An oversized unterminated tail is discarded at EOF.
        harness.send_raw(&vec![b'z'; MIN_IPC_FRAME_BYTES + 1]);
        harness.close_stdin();
        timeout(Duration::from_secs(5), signal)
            .await
            .expect("stdin EOF must resolve the signal");
    }

    #[tokio::test]
    async fn oversized_responses_are_replaced_with_null() {
        let mut harness = child_harness(&[0]);
        harness
            .child
            .serve(|_| async { Value::String("y".repeat(MIN_IPC_FRAME_BYTES * 2)) })
            .unwrap();
        harness.send(&ParentMessage::Request {
            id: 9,
            payload: Value::Null,
        });
        assert_eq!(
            harness.next_message().await,
            ChildMessage::Response {
                id: 9,
                payload: Value::Null
            }
        );
    }

    #[tokio::test]
    async fn notify_and_log_write_frames() {
        let mut harness = child_harness(&[2, 5]);
        assert_eq!(harness.child.shard_ids(), &[2, 5]);
        assert_eq!(harness.child.shard_count(), 8);

        harness.child.notify_ready().unwrap();
        assert_eq!(
            harness.next_message().await,
            ChildMessage::Ready { shard_id: 2 }
        );
        assert_eq!(
            harness.next_message().await,
            ChildMessage::Ready { shard_id: 5 }
        );
        harness.child.notify_shard_ready(5).unwrap();
        assert_eq!(
            harness.next_message().await,
            ChildMessage::Ready { shard_id: 5 }
        );
        assert!(harness.child.notify_shard_ready(3).is_err());

        harness.child.log("hello").unwrap();
        assert_eq!(
            harness.next_message().await,
            ChildMessage::Log {
                message: "hello".to_string()
            }
        );
        let error = harness
            .child
            .log("z".repeat(MIN_IPC_FRAME_BYTES))
            .unwrap_err();
        assert!(error.to_string().contains("exceeds"), "{error}");
    }

    #[tokio::test]
    async fn stdout_write_failures_surface_as_errors() {
        let (to_child, rx) = std::sync::mpsc::channel::<Vec<u8>>();
        let child = ShardChildProcess::with_io(
            child_settings(&[0], MIN_IPC_FRAME_BYTES),
            Box::new(std::io::BufReader::new(ChannelSource::new(rx))),
            Box::new(|_: &[u8]| Err(io::Error::from(io::ErrorKind::BrokenPipe))),
        );
        let error = child.notify_ready().unwrap_err();
        assert!(
            error.to_string().contains("IPC stdout write failed"),
            "{error}"
        );
        // A failed request leaves no stale slot behind, so the next attempt
        // fails the same way instead of reporting a duplicate.
        for _ in 0..2 {
            let error = child.acquire_identify(0).await.unwrap_err();
            assert!(error.to_string().contains("write failed"), "{error}");
        }
        // Responses and unexpected grants cannot be written either; the
        // reader logs and keeps going until stdin closes.
        for message in [
            ParentMessage::Request {
                id: 1,
                payload: Value::Null,
            },
            ParentMessage::IdentifyGrant { shard_id: 0 },
        ] {
            to_child
                .send(frame_line(&message).unwrap().into_bytes())
                .unwrap();
        }
        drop(to_child);
        timeout(Duration::from_secs(5), child.shutdown_signal())
            .await
            .expect("EOF ends the session");
    }

    #[test]
    fn serve_requires_a_tokio_runtime() {
        let harness = child_harness(&[0]);
        let error = harness
            .child
            .serve(|payload| async move { payload })
            .unwrap_err();
        assert!(error.to_string().contains("Tokio runtime"), "{error}");
    }

    // -- end-to-end with the real child implementation ---------------------------

    const CHILD_ENTRY_TEST: &str = "sharding::process::tests::shard_child_entry_point";

    /// Body of the child process for `real_shard_child_process_round_trip`.
    /// In a normal test run `from_env` returns `None` and this passes
    /// immediately.
    #[tokio::test]
    async fn shard_child_entry_point() {
        let Some(child) = ShardChildProcess::from_env() else {
            return;
        };
        let shard_ids = child.shard_ids().to_vec();
        child
            .serve(move |request| {
                let shard_ids = shard_ids.clone();
                async move { json!({ "echo": request, "shards": shard_ids }) }
            })
            .unwrap();
        // User output on stdout, including an unterminated line, must not
        // disturb the protocol.
        println!("stray println from user code");
        print!("unterminated print without newline");
        for &shard_id in child.shard_ids() {
            let permit = child.acquire_identify(shard_id).await.unwrap();
            child.notify_shard_ready(shard_id).unwrap();
            drop(permit);
        }
        child.log("e2e child booted").unwrap();
        child.shutdown_signal().await;
    }

    async fn wait_until(deadline: Duration, mut condition: impl FnMut() -> bool) -> bool {
        let end = Instant::now() + deadline;
        loop {
            if condition() {
                return true;
            }
            if Instant::now() >= end {
                return false;
            }
            sleep(Duration::from_millis(20)).await;
        }
    }

    fn all_in_state(manager: &ProcessShardManager, state: ChildState) -> bool {
        let children = manager.children();
        !children.is_empty() && children.iter().all(|child| child.state == state)
    }

    #[tokio::test]
    async fn real_shard_child_process_round_trip() {
        let config = ProcessShardManagerConfig {
            total_shards: Some(2),
            shards_per_process: 2,
            max_concurrency: Some(1),
            program: Some(std::env::current_exe().unwrap()),
            args: [
                CHILD_ENTRY_TEST,
                "--exact",
                "--nocapture",
                "--test-threads=1",
            ]
            .map(String::from)
            .to_vec(),
            respawn: false,
            identify_interval: Duration::from_millis(100),
            ..ProcessShardManagerConfig::default()
        };
        let manager = ProcessShardManager::spawn(config).await.unwrap();
        assert!(
            wait_until(Duration::from_secs(30), || all_in_state(
                &manager,
                ChildState::Running
            ))
            .await,
            "child never reached Running: {:?}",
            manager.children()
        );

        let responses = manager.broadcast(json!({"kind": "ping"})).await.unwrap();
        assert_eq!(
            responses,
            vec![(0, json!({"echo": {"kind": "ping"}, "shards": [0, 1]}))]
        );

        let started = Instant::now();
        timeout(Duration::from_secs(15), manager.shutdown())
            .await
            .expect("shutdown timed out")
            .unwrap();
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the child should exit on the shutdown op instead of being killed"
        );
        assert_eq!(manager.children()[0].state, ChildState::Stopped);
    }

    // -- manager with scripted children -------------------------------------------

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
    fn scripted_child_config(script: &str) -> ProcessShardManagerConfig {
        ProcessShardManagerConfig {
            total_shards: Some(1),
            program: Some(PathBuf::from("/bin/sh")),
            args: vec!["-c".to_string(), script.to_string()],
            respawn_delay: Duration::from_millis(10),
            identify_interval: Duration::from_millis(100),
            shutdown_timeout: Duration::from_secs(5),
            ..ProcessShardManagerConfig::default()
        }
    }

    /// Child loop that exits on the shutdown op (or stdin EOF).
    #[cfg(unix)]
    const EXIT_ON_SHUTDOWN: &str = r#"
while read -r line; do
  case "$line" in
    *'"op":"shutdown"'*) exit 0 ;;
  esac
done
"#;

    #[cfg(unix)]
    #[tokio::test]
    async fn ipc_round_trip_with_scripted_child() {
        // The scripted child speaks one full protocol exchange: announce
        // ready, answer the first broadcast (request id 1 for a fresh
        // manager), then exit on shutdown.
        let script = r#"
echo '@discordrs-ipc/1 {"op":"ready","shard_id":0}'
echo '@discordrs-ipc/1 {"op":"log","message":"child booted"}'
while read -r line; do
  case "$line" in
    *'"op":"request"'*) echo '@discordrs-ipc/1 {"op":"response","id":1,"payload":{"pong":true}}' ;;
    *'"op":"shutdown"'*) exit 0 ;;
  esac
done
"#;
        let manager = ProcessShardManager::spawn(scripted_child_config(script))
            .await
            .unwrap();
        assert_eq!(manager.shard_count(), 1);
        assert_eq!(manager.max_concurrency(), 1);

        assert!(
            wait_until(Duration::from_secs(5), || all_in_state(
                &manager,
                ChildState::Running
            ))
            .await,
            "child never reached Running: {:?}",
            manager.children()
        );
        let status = &manager.children()[0];
        assert_eq!(status.shard_ids, vec![0]);
        assert!(status.pid.is_some());

        let responses = manager.broadcast(json!({"kind": "ping"})).await.unwrap();
        assert_eq!(responses, vec![(0, json!({"pong": true}))]);

        let error = manager
            .broadcast(Value::String("x".repeat(DEFAULT_MAX_IPC_FRAME_BYTES)))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("max_ipc_frame_bytes"), "{error}");

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
        config.max_respawns = Some(2);

        let manager = ProcessShardManager::spawn(config).await.unwrap();
        timeout(Duration::from_secs(5), manager.wait())
            .await
            .expect("manager.wait timed out")
            .unwrap();

        let status = &manager.children()[0];
        assert_eq!(status.state, ChildState::Stopped);
        assert_eq!(status.respawns, 2);

        // Broadcasting against dead children reports Null entries at once.
        let started = Instant::now();
        let responses = manager.broadcast(json!("stats")).await.unwrap();
        assert_eq!(responses, vec![(0, Value::Null)]);
        assert!(started.elapsed() < Duration::from_secs(1));
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

    #[cfg(unix)]
    #[tokio::test]
    async fn children_are_grouped_by_shards_per_process() {
        let mut config = scripted_child_config(EXIT_ON_SHUTDOWN);
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

    /// Regression: the monitor used to await stdin writes inside its
    /// `select!` and to write the shutdown op before its timeout started,
    /// so a child that stopped reading stdin hid both broadcasts and
    /// shutdown from the monitor forever.
    #[cfg(unix)]
    #[tokio::test]
    async fn child_that_stops_reading_stdin_blocks_neither_broadcasts_nor_shutdown() {
        // `sleep` never reads stdin and ignores EOF, so the pipe fills up.
        let mut config = scripted_child_config("exec sleep 30");
        config.shutdown_timeout = Duration::from_millis(300);
        config.broadcast_timeout = Duration::from_secs(30);
        config.max_ipc_frame_bytes = 4 * 1024 * 1024;
        let manager = Arc::new(ProcessShardManager::spawn(config).await.unwrap());
        assert!(
            wait_until(Duration::from_secs(5), || manager.children()[0]
                .pid
                .is_some())
            .await
        );

        // 1 MiB does not fit into a pipe buffer: the writer blocks on it.
        let big = {
            let manager = Arc::clone(&manager);
            tokio::spawn(async move { manager.broadcast(json!("x".repeat(1 << 20))).await })
        };
        sleep(Duration::from_millis(200)).await;
        // Fill the writer queue behind the stuck frame.
        let mut queued = Vec::new();
        for _ in 0..IPC_WRITE_QUEUE_CAPACITY {
            let manager = Arc::clone(&manager);
            queued.push(tokio::spawn(async move {
                manager.broadcast(json!("small")).await
            }));
        }
        sleep(Duration::from_millis(200)).await;

        // The monitor still turns: the next broadcast is refused at once.
        let started = Instant::now();
        let response = timeout(Duration::from_secs(5), manager.broadcast(json!("probe")))
            .await
            .expect("broadcast must not wait for the stalled child")
            .unwrap();
        assert_eq!(response, vec![(0, Value::Null)]);
        assert!(started.elapsed() < Duration::from_secs(2));

        // Shutdown is observed, waits out the deadline, then kills.
        let started = Instant::now();
        timeout(Duration::from_secs(5), manager.shutdown())
            .await
            .expect("shutdown hung on the stalled child")
            .unwrap();
        assert!(started.elapsed() >= Duration::from_millis(300));
        let status = &manager.children()[0];
        assert_eq!(status.state, ChildState::Stopped);
        assert_eq!(status.pid, None);

        // Broadcasts still waiting on the dead child resolve with null.
        let response = timeout(Duration::from_secs(5), big).await.unwrap().unwrap();
        assert_eq!(response.unwrap(), vec![(0, Value::Null)]);
        for task in queued {
            let response = timeout(Duration::from_secs(5), task)
                .await
                .unwrap()
                .unwrap();
            assert_eq!(response.unwrap(), vec![(0, Value::Null)]);
        }
    }

    /// Regression: `wait()` used to take every task handle, so a concurrent
    /// `shutdown()` found none and returned while the child was running.
    #[cfg(unix)]
    #[tokio::test]
    async fn concurrent_wait_and_shutdown_both_observe_child_exit() {
        let script = r#"
while read -r line; do
  case "$line" in
    *'"op":"shutdown"'*) sleep 0.3; exit 0 ;;
  esac
done
"#;
        let manager = ProcessShardManager::spawn(scripted_child_config(script))
            .await
            .unwrap();
        assert!(
            wait_until(Duration::from_secs(5), || manager.children()[0]
                .pid
                .is_some())
            .await
        );

        let waiter = manager.wait();
        tokio::pin!(waiter);
        assert!(futures_util::poll!(&mut waiter).is_pending());

        timeout(Duration::from_secs(5), manager.shutdown())
            .await
            .expect("shutdown timed out")
            .unwrap();
        let status = &manager.children()[0];
        assert_eq!(
            status.state,
            ChildState::Stopped,
            "shutdown returned before the child exited"
        );
        assert_eq!(status.pid, None);
        timeout(Duration::from_secs(1), waiter)
            .await
            .expect("the earlier wait() must complete too")
            .unwrap();
        timeout(Duration::from_secs(1), manager.wait())
            .await
            .expect("wait() after completion returns at once")
            .unwrap();
    }

    /// Regression: invalid UTF-8 ended the old `lines()` reader, which then
    /// treated the live child as exited and blocked in `wait()`; oversized
    /// and stray lines were buffered without limit or broke parsing.
    #[cfg(unix)]
    #[tokio::test]
    async fn garbage_oversized_and_non_utf8_stdout_do_not_break_ipc() {
        let script = r#"
echo 'plain println from user code'
printf '\377\376 not utf-8\n'
head -c 20000 /dev/zero | tr '\0' 'x'; echo
printf 'unterminated user output'
echo '@discordrs-ipc/1 {"op":"bogus"}'
echo '@discordrs-ipc/1 {not json'
echo '@discordrs-ipc/1 {"op":"ready","shard_id":7}'
echo '@discordrs-ipc/1 {"op":"identify_release","shard_id":0}'
echo '@discordrs-ipc/1 {"op":"response","id":999,"payload":null}'
echo
echo '@discordrs-ipc/1 {"op":"ready","shard_id":0}'
while read -r line; do
  case "$line" in
    *'"op":"request"'*) echo '@discordrs-ipc/1 {"op":"response","id":1,"payload":"still works"}' ;;
    *'"op":"shutdown"'*) exit 0 ;;
  esac
done
"#;
        let mut config = scripted_child_config(script);
        config.max_ipc_frame_bytes = MIN_IPC_FRAME_BYTES;
        let manager = ProcessShardManager::spawn(config).await.unwrap();
        assert!(
            wait_until(Duration::from_secs(5), || all_in_state(
                &manager,
                ChildState::Running
            ))
            .await,
            "child never reached Running: {:?}",
            manager.children()
        );
        let responses = manager.broadcast(json!("ping")).await.unwrap();
        assert_eq!(responses, vec![(0, json!("still works"))]);
        timeout(Duration::from_secs(5), manager.shutdown())
            .await
            .expect("shutdown timed out")
            .unwrap();
    }

    /// Regression: when stdout reached EOF the old monitor awaited
    /// `child.wait()` inside the read branch, so a child that closed stdout
    /// but kept running never received the shutdown op.
    #[cfg(unix)]
    #[tokio::test]
    async fn child_that_closes_stdout_still_receives_shutdown() {
        let script = [
            r#"echo '@discordrs-ipc/1 {"op":"ready","shard_id":0}'"#,
            "exec 1>&-",
            EXIT_ON_SHUTDOWN,
        ]
        .join("\n");
        let mut config = scripted_child_config(&script);
        config.shutdown_timeout = Duration::from_secs(10);
        let manager = ProcessShardManager::spawn(config).await.unwrap();
        assert!(
            wait_until(Duration::from_secs(5), || all_in_state(
                &manager,
                ChildState::Running
            ))
            .await
        );
        sleep(Duration::from_millis(100)).await;

        let started = Instant::now();
        timeout(Duration::from_secs(5), manager.shutdown())
            .await
            .expect("shutdown hung after the child closed stdout")
            .unwrap();
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(manager.children()[0].state, ChildState::Stopped);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn spawn_stagger_delays_later_children_and_shutdown_interrupts_it() {
        let script = r#"
echo '@discordrs-ipc/1 {"op":"ready","shard_id":0}'
while read -r line; do
  case "$line" in
    *'"op":"request"'*) echo '@discordrs-ipc/1 {"op":"response","id":1,"payload":"first"}' ;;
    *'"op":"shutdown"'*) exit 0 ;;
  esac
done
"#;
        let mut config = scripted_child_config(script);
        config.total_shards = Some(2);
        config.spawn_stagger = Duration::from_secs(30);
        config.broadcast_timeout = Duration::from_secs(30);
        let manager = ProcessShardManager::spawn(config).await.unwrap();
        assert!(
            wait_until(Duration::from_secs(5), || manager.children()[0].state
                == ChildState::Running)
            .await
        );
        assert_eq!(manager.children()[1].pid, None);

        // The waiting child answers null immediately instead of after the
        // broadcast timeout.
        let responses = timeout(Duration::from_secs(5), manager.broadcast(json!("ping")))
            .await
            .expect("broadcast waited for the staggered child")
            .unwrap();
        assert_eq!(responses, vec![(0, json!("first")), (1, Value::Null)]);

        timeout(Duration::from_secs(5), manager.shutdown())
            .await
            .expect("shutdown waited for the stagger delay")
            .unwrap();
        assert!(all_in_state(&manager, ChildState::Stopped));
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn dropping_the_manager_stops_children() {
        let dir = scratch_dir("drop");
        let script = r#"
while read -r line; do
  case "$line" in
    *'"op":"shutdown"'*) : > "$SCRATCH_DIR/stopped"; exit 0 ;;
  esac
done
"#;
        let mut config = scripted_child_config(script);
        config
            .extra_env
            .push(("SCRATCH_DIR".to_string(), dir.display().to_string()));
        let manager = ProcessShardManager::spawn(config).await.unwrap();
        assert!(
            wait_until(Duration::from_secs(5), || manager.children()[0]
                .pid
                .is_some())
            .await
        );
        drop(manager);
        let marker = dir.join("stopped");
        assert!(
            wait_until(Duration::from_secs(5), || marker.exists()).await,
            "child never received the shutdown op"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // -- IDENTIFY coordination across processes ------------------------------------

    #[cfg(target_os = "linux")]
    fn scratch_dir(tag: &str) -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "discordrs-process-test-{tag}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Grant times (ns since the epoch) a scripted child recorded.
    #[cfg(target_os = "linux")]
    fn read_grants(dir: &std::path::Path, shard_id: u32) -> Vec<i128> {
        std::fs::read_to_string(dir.join(format!("grant-{shard_id}")))
            .unwrap_or_default()
            .lines()
            .map(|line| line.trim().parse().unwrap())
            .collect()
    }

    /// Child that asks for an IDENTIFY permit for its single shard and
    /// records when the grant arrived; the handling of the grant follows.
    #[cfg(target_os = "linux")]
    const IDENTIFY_CHILD_PRELUDE: &str = r#"
P='@discordrs-ipc/1 '
S=$DISCORDRS_SHARD_IDS
echo "$P{\"op\":\"identify_request\",\"shard_id\":$S}"
while read -r line; do
  case "$line" in
    *'"op":"shutdown"'*) exit 0 ;;
    *identify_grant*)
      date +%s%N >> "$SCRATCH_DIR/grant-$S"
"#;

    #[cfg(target_os = "linux")]
    fn identify_child_config(dir: &std::path::Path, on_grant: &str) -> ProcessShardManagerConfig {
        let script = [IDENTIFY_CHILD_PRELUDE, on_grant, " ;;\n  esac\ndone\n"].concat();
        let mut config = scripted_child_config(&script);
        config
            .extra_env
            .push(("SCRATCH_DIR".to_string(), dir.display().to_string()));
        config
    }

    #[cfg(target_os = "linux")]
    const RELEASE_AND_READY: &str = r#"
      echo "$P{\"op\":\"identify_release\",\"shard_id\":$S}"
      echo "$P{\"op\":\"ready\",\"shard_id\":$S}""#;

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn identify_grants_are_spaced_per_bucket_across_processes() {
        let dir = scratch_dir("buckets");
        let mut config = identify_child_config(&dir, RELEASE_AND_READY);
        config.total_shards = Some(4);
        config.max_concurrency = Some(2);
        config.identify_interval = Duration::from_millis(300);
        let manager = ProcessShardManager::spawn(config).await.unwrap();
        assert_eq!(manager.max_concurrency(), 2);
        assert!(
            wait_until(Duration::from_secs(10), || all_in_state(
                &manager,
                ChildState::Running
            ))
            .await,
            "children never reached Running: {:?}",
            manager.children()
        );

        let grants: Vec<i128> = (0..4)
            .map(|shard| {
                let times = read_grants(&dir, shard);
                assert_eq!(times.len(), 1, "shard {shard} grants: {times:?}");
                times[0]
            })
            .collect();
        // Buckets are shard % 2: {0, 2} and {1, 3}. Within a bucket the
        // second grant follows the first one's release by the interval.
        for (a, b) in [(0, 2), (1, 3)] {
            let gap = (grants[a] - grants[b]).abs();
            assert!(
                gap >= 290_000_000,
                "shards {a} and {b} share a bucket but were granted {gap}ns apart"
            );
        }

        timeout(Duration::from_secs(5), manager.shutdown())
            .await
            .expect("shutdown timed out")
            .unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn respawned_child_waits_out_the_interval_after_crashing_with_a_permit() {
        let dir = scratch_dir("respawn");
        let on_grant = [
            r#"
      if [ ! -e "$SCRATCH_DIR/crashed" ]; then : > "$SCRATCH_DIR/crashed"; exit 1; fi"#,
            RELEASE_AND_READY,
        ]
        .concat();
        let mut config = identify_child_config(&dir, &on_grant);
        config.identify_interval = Duration::from_millis(300);
        config.max_respawns = Some(3);
        let manager = ProcessShardManager::spawn(config).await.unwrap();
        assert!(
            wait_until(Duration::from_secs(10), || all_in_state(
                &manager,
                ChildState::Running
            ))
            .await,
            "child never reached Running: {:?}",
            manager.children()
        );
        assert_eq!(manager.children()[0].respawns, 1);

        let grants = read_grants(&dir, 0);
        assert_eq!(grants.len(), 2, "grants: {grants:?}");
        // The crashed child never released its permit; the parent reclaimed
        // it when the process exited and spaced the respawn's grant.
        assert!(
            grants[1] - grants[0] >= 290_000_000,
            "respawned child identified {}ns after the crash",
            grants[1] - grants[0]
        );

        timeout(Duration::from_secs(5), manager.shutdown())
            .await
            .expect("shutdown timed out")
            .unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn unreleased_identify_permits_are_reclaimed_after_the_timeout() {
        let dir = scratch_dir("permit-timeout");
        // Never releases the permit.
        let on_grant = r#"
      echo "$P{\"op\":\"ready\",\"shard_id\":$S}""#;
        let mut config = identify_child_config(&dir, on_grant);
        config.total_shards = Some(2);
        config.identify_interval = Duration::from_millis(100);
        config.identify_permit_timeout = Duration::from_millis(400);
        let manager = ProcessShardManager::spawn(config).await.unwrap();
        assert!(
            wait_until(Duration::from_secs(10), || all_in_state(
                &manager,
                ChildState::Running
            ))
            .await,
            "children never reached Running: {:?}",
            manager.children()
        );

        let first = read_grants(&dir, 0);
        let second = read_grants(&dir, 1);
        assert_eq!((first.len(), second.len()), (1, 1));
        // Reclaimed at grant + 400ms, granted again 100ms later. The margin
        // absorbs the first child's delay between grant and `date`.
        let gap = (first[0] - second[0]).abs();
        assert!(gap >= 400_000_000, "grants only {gap}ns apart");

        timeout(Duration::from_secs(5), manager.shutdown())
            .await
            .expect("shutdown timed out")
            .unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Serves `/gateway/bot` once per scripted response.
    #[cfg(unix)]
    async fn gateway_bot_server(
        responses: Vec<(&'static str, &'static str)>,
    ) -> (u16, tokio::task::JoinHandle<Vec<String>>) {
        use tokio::io::AsyncReadExt as _;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let mut requests = Vec::new();
            for (status, body) in responses {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut buffer = vec![0_u8; 4096];
                let received = stream.read(&mut buffer).await.unwrap();
                requests.push(String::from_utf8_lossy(&buffer[..received]).into_owned());
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
            requests
        });
        (port, server)
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn spawn_auto_reads_shards_and_max_concurrency_from_gateway_bot() {
        let ok = r#"{"url":"wss://gateway.discord.gg","shards":3,"session_start_limit":{"total":1000,"remaining":0,"reset_after":200,"max_concurrency":2}}"#;
        let unauthorized = r#"{"message":"401: Unauthorized","code":0}"#;
        let (port, server) = gateway_bot_server(vec![
            ("200 OK", ok),
            ("401 Unauthorized", unauthorized),
            ("401 Unauthorized", unauthorized),
        ])
        .await;
        let rest = RestClient::builder("token", 0)
            .api_base(format!("http://127.0.0.1:{port}"))
            .build()
            .unwrap();

        let mut config = scripted_child_config(EXIT_ON_SHUTDOWN);
        config.total_shards = None;
        let manager = ProcessShardManager::spawn_auto_with(&rest, config.clone())
            .await
            .unwrap();
        assert_eq!(manager.shard_count(), 3);
        assert_eq!(manager.max_concurrency(), 2);
        assert_eq!(manager.children().len(), 3);
        timeout(Duration::from_secs(5), manager.shutdown())
            .await
            .expect("shutdown timed out")
            .unwrap();

        // A failed lookup is fatal only when the shard count is unknown.
        assert!(ProcessShardManager::spawn_auto_with(&rest, config.clone())
            .await
            .is_err());
        config.total_shards = Some(2);
        let manager = ProcessShardManager::spawn_auto_with(&rest, config)
            .await
            .unwrap();
        assert_eq!(manager.shard_count(), 2);
        assert_eq!(manager.max_concurrency(), 1);
        timeout(Duration::from_secs(5), manager.shutdown())
            .await
            .expect("shutdown timed out")
            .unwrap();

        let requests = server.await.unwrap();
        assert!(requests
            .iter()
            .all(|request| request.starts_with("GET /gateway/bot ")));
    }
}
