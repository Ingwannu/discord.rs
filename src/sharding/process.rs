//! Multi-process shard management (discord.js `ShardingManager` equivalent).
//!
//! A [`ProcessShardManager`] spawns one child process per shard group, keeps
//! each child alive (respawning it with exponential backoff when it exits
//! unexpectedly), decides when each shard may IDENTIFY, and talks to the
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
//!   child does not corrupt the protocol. The parent logs such lines through
//!   `tracing` at INFO level (`child stdout: ...`) and carries on.
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
//! Both sides write through a dedicated writer with two queues: control
//! frames (IDENTIFY grants and releases, shutdown) always go first, while
//! data frames (broadcasts, responses, logs) have bounded queues that reject
//! new frames when full. No async task or destructor ever blocks on a pipe.
//!
//! # IDENTIFY coordination
//!
//! Discord allows one IDENTIFY per rate-limit bucket every 5 seconds, where
//! the bucket is `shard_id % max_concurrency`
//! (<https://docs.discord.com/developers/events/gateway#max-concurrency>), and
//! a daily number of session starts. The parent owns one queue per bucket
//! plus the session-start budget, and grants one permit per IDENTIFY attempt
//! over IPC.
//!
//! Children pass [`ShardChildProcess::identify_gate`] to
//! [`GatewayConnectionConfig::identify_gate`]. The gateway client then asks
//! for a permit before *every* connection that will IDENTIFY: the first
//! one, and every fresh session after INVALID_SESSION, close codes 4007/4009,
//! or a reconnect before READY. It reports `Sent` right after the IDENTIFY
//! write, or `Abandoned` if the attempt ended earlier. RESUME needs no
//! permit.
//!
//! The parent keeps the bucket until the child reports the outcome or the
//! child's exit is confirmed. The next grant comes `identify_interval` after
//! a `Sent`, or right away after an `Abandoned`. A child that holds a permit
//! longer than [`ProcessShardManagerConfig::identify_permit_timeout`] is
//! killed, and the bucket is reused only once that child is gone, so a
//! stalled child can never IDENTIFY next to the shard that got its slot.
//!
//! # Protocol reference (for custom child programs)
//!
//! | Direction | `op` | Fields | Meaning |
//! |---|---|---|---|
//! | parent → child | `shutdown` | | Stop gracefully; stdin closes right after. |
//! | parent → child | `request` | `id`, `payload` | Answer with a `response` carrying the same `id`. |
//! | parent → child | `identify_grant` | `shard_id`, `lease` | The shard may send one IDENTIFY now. |
//! | child → parent | `ready` | `shard_id` | The shard is connected. |
//! | child → parent | `response` | `id`, `payload` | Answer to a `request`. |
//! | child → parent | `log` | `message` | Surfaced through the parent's `tracing`. |
//! | child → parent | `identify_request` | `shard_id` | Queue for a permit (one outstanding per shard). |
//! | child → parent | `identify_release` | `shard_id`, `lease`, `sent` | The attempt is over; `sent: false` means IDENTIFY was never written. |
//!
//! Example frame: `@discordrs-ipc/1 {"op":"ready","shard_id":0}`. Empty
//! lines are ignored. The Rust child writes a newline before every frame
//! so that an unterminated `print!` cannot swallow the prefix.
//!
//! # Lifecycle guarantees
//!
//! [`ProcessShardManager::shutdown`] is the completion API: it resolves only
//! once every child has exited, been killed, or failed to be confirmed dead,
//! and reports the last case as an error. Children see a `shutdown` op and
//! then stdin EOF. That stops *cooperative* children (those awaiting
//! [`ShardChildProcess::shutdown_signal`]); others are killed when
//! `shutdown_timeout` expires. If the manager is dropped without
//! `shutdown()`, the monitors stop their children in the background; that
//! needs the Tokio runtime to stay alive, otherwise `kill_on_drop` is the
//! only cleanup. If the parent process dies abruptly, a child notices only
//! through stdin EOF, so only cooperative children exit then.
//!
//! [`ProcessShardManager`]: crate::sharding::process::ProcessShardManager
//! [`ProcessShardManager::shutdown`]: crate::sharding::process::ProcessShardManager::shutdown
//! [`ProcessShardManagerConfig::max_ipc_frame_bytes`]: crate::sharding::process::ProcessShardManagerConfig::max_ipc_frame_bytes
//! [`ProcessShardManagerConfig::identify_permit_timeout`]: crate::sharding::process::ProcessShardManagerConfig::identify_permit_timeout
//! [`ShardChildProcess`]: crate::sharding::process::ShardChildProcess
//! [`ShardChildProcess::from_env`]: crate::sharding::process::ShardChildProcess::from_env
//! [`ShardChildProcess::identify_gate`]: crate::sharding::process::ShardChildProcess::identify_gate
//! [`ShardChildProcess::shutdown_signal`]: crate::sharding::process::ShardChildProcess::shutdown_signal
//! [`GatewayConnectionConfig::identify_gate`]: crate::ws::GatewayConnectionConfig::identify_gate
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
// - Purpose: never let two IDENTIFYs of one `max_concurrency` bucket overlap,
//   across every child process, every respawn, and every re-IDENTIFY the
//   gateway client performs on its own, and stay within the daily
//   session-start budget.
// - Previous implementation and constraints: first child process starts were
//   staggered by 5 seconds. Then (first 3.0 draft) permits were taken once
//   before `Client::start()` and expired into a free bucket after a timeout:
//   a stalled child could still IDENTIFY next to the shard that got its
//   slot, and internal re-IDENTIFYs bypassed the parent. Only a budget of
//   `remaining == 0` at startup was respected.
// - Alternatives considered: (1) grant spacing alone, with lease IDs;
//   (2) expiring permits into the next grant; (3) parent-owned bucket leases
//   released only by an acknowledged outcome or a confirmed child exit,
//   with kill-and-reap as the timeout recovery, and the gateway asking for a
//   permit per IDENTIFY attempt through `IdentifyGate`.
// - Chosen approach: (3), plus a conservative budget: every grant takes a
//   reservation that is settled exactly once (`Abandoned` refunds it, which
//   wakes a waiting reserver). A refresh from `/gateway/bot` counts all
//   still-outstanding reservations, plus those that settled `Sent` while
//   the request was in flight, against the new window. The count may go
//   negative, so refunds never create starts Discord lacks. A failed
//   refresh keeps the budget closed and retries with backoff.
// - Why: lease IDs and expiry cannot stop a process that is merely slow;
//   only its confirmed death proves it will not IDENTIFY later. Asking per
//   attempt, from inside the gateway client, is the only way to cover the
//   client's own re-IDENTIFYs.
// - Trade-offs: a child whose gateway client is stuck for longer than the
//   permit timeout (beyond the client's own 15s connect+HELLO bound) is
//   killed and respawned, taking every shard in that process with it. A
//   refresh may count a reservation twice (once as sent, once as
//   outstanding); that start comes back at the next refresh.
//   The budget is counted per manager: several managers sharing one token
//   must split the budget themselves.
//
// [Decision Log] Monitor lifecycle
// - Purpose: shutdown must always be observable and bounded, `wait()` and
//   `shutdown()` must agree on the outcome, and failures must be visible.
// - Previous implementation and constraints: the monitor awaited stdin
//   writes inside its `select!`. `wait()` took the task handles, so a
//   concurrent `shutdown()` returned before children exited. A failed kill
//   and a panicking monitor were both reported as success.
// - Alternatives considered: write timeouts on the inline write; a `JoinSet`
//   drained by whichever caller comes first; per-child writer tasks plus a
//   shared completion state that records each monitor's terminal outcome.
// - Chosen approach: the last one. Frames go to a writer task with a
//   control-priority queue and a bounded data queue. One deadline covers
//   write + wait before the kill. A `watch` state counts live monitors and
//   collects their failures, and every waiter reads the same final state.
//   Every terminal outcome other than a requested shutdown (or a successful
//   exit with respawn disabled) is a failure: give-up after `max_respawns`,
//   failed exit with respawn disabled, spawn failure, a `child.wait()` error
//   (the child is killed and confirmed dead first), and an unconfirmed kill.
// - Why: write timeouts still stall the loop. Handle ownership makes waiters
//   race. A failure recorded on drop also covers panics and runtime shutdown.
// - Trade-offs: a child that reads stdin too slowly gets `null` broadcast
//   entries once 64 data frames are queued. Control frames are not limited
//   by count; the protocol bounds them (one grant per shard plus one
//   shutdown).

use std::collections::{HashMap, HashSet, VecDeque};
use std::future::Future;
use std::io::{self, BufRead, Write as _};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex as StdMutex, MutexGuard, OnceLock, Weak};
use std::time::Duration;

use futures_util::stream::{FuturesUnordered, StreamExt};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::{mpsc, oneshot, watch, Mutex as AsyncMutex, OwnedMutexGuard};
use tokio::task::JoinSet;
use tokio::time::{sleep_until, timeout, timeout_at, Instant};
use tracing::{debug, info, warn};

use crate::error::DiscordError;
use crate::http::RestClient;
use crate::model::SessionStartLimit;
#[cfg(doc)]
use crate::ws::GatewayConnectionConfig;
use crate::ws::{IdentifyGate, IdentifyOutcome, IdentifyPermit, IdentifyPermitFuture};

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
/// Environment variable holding the parent's broadcast timeout in
/// milliseconds. Children cancel a request handler once the parent has
/// stopped waiting for its answer.
pub const ENV_SHARD_IPC_REQUEST_TIMEOUT_MS: &str = "DISCORDRS_SHARD_IPC_REQUEST_TIMEOUT_MS";

/// Prefix that marks a line as an IPC frame. The `/1` is the protocol
/// version; lines with any other prefix are treated as ordinary output.
pub const IPC_FRAME_PREFIX: &str = "@discordrs-ipc/1 ";
/// Default for [`ProcessShardManagerConfig::max_ipc_frame_bytes`] (1 MiB).
pub const DEFAULT_MAX_IPC_FRAME_BYTES: usize = 1024 * 1024;
/// How many broadcast requests a child runs at once. Requests beyond this
/// are answered with `null` before a task is spawned for them.
pub const DEFAULT_MAX_CONCURRENT_REQUESTS: usize = 32;

/// Smallest accepted frame limit: below this even a `ready` frame with a
/// large shard id plus some payload would not fit, so smaller settings are
/// raised to this value.
const MIN_IPC_FRAME_BYTES: usize = 4096;
/// Per-child queue between [`ProcessShardManager::broadcast`] and the
/// monitor. The monitor drains it without awaiting I/O, so it only fills
/// when broadcasts arrive faster than the monitor loop can turn.
const COMMAND_QUEUE_CAPACITY: usize = 256;
/// Data frames (broadcast requests) waiting for a child's stdin writer. When
/// a child stops reading stdin this fills up, and further broadcasts fail
/// fast instead of piling up.
const IPC_WRITE_QUEUE_CAPACITY: usize = 64;
/// Data frames (responses, logs, ready) waiting for the child's stdout
/// writer thread.
const CHILD_DATA_QUEUE_CAPACITY: usize = 256;
/// Default broadcast timeout, also the child's request-handler timeout when
/// the parent does not pass one.
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
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
/// When a budget refresh is impossible, Discord's daily window is assumed.
const BUDGET_FALLBACK_WINDOW: Duration = Duration::from_secs(24 * 60 * 60);
/// Minimum pause before asking `/gateway/bot` again when it still reports
/// an exhausted budget with an already elapsed reset.
const BUDGET_RETRY_DELAY: Duration = Duration::from_secs(5);
/// Clamp for user-supplied durations so deadline arithmetic cannot overflow.
const FAR_FUTURE: Duration = Duration::from_secs(60 * 60 * 24 * 365);

fn deadline_after(duration: Duration) -> Instant {
    let now = Instant::now();
    now.checked_add(duration.min(FAR_FUTURE)).unwrap_or(now)
}

/// Milliseconds for a log field, saturating where `as u64` would truncate.
fn log_millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn lock_or_recover<T>(mutex: &StdMutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
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
    /// Answer to [`ChildMessage::IdentifyRequest`]: the shard may send one
    /// IDENTIFY now and must then report [`ChildMessage::IdentifyRelease`]
    /// with the same `lease`.
    IdentifyGrant {
        /// Shard whose IDENTIFY is permitted.
        shard_id: u32,
        /// Correlation id for the release.
        lease: u64,
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
    /// The IDENTIFY attempt for `lease` is over.
    IdentifyRelease {
        /// Shard whose permit is returned.
        shard_id: u32,
        /// Lease id from the grant.
        lease: u64,
        /// Whether IDENTIFY was (or may have been) written. `false` frees
        /// the bucket without waiting out the interval and refunds the
        /// session-start budget. Defaults to `true`, the safe assumption.
        #[serde(default = "identify_sent_default")]
        sent: bool,
    },
}

fn identify_sent_default() -> bool {
    true
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

/// Where the manager gets Discord's daily session-start budget from.
#[derive(Clone, Debug, Default)]
pub enum SessionStartBudget {
    /// Read `session_start_limit` from `/gateway/bot`, even when the shard
    /// count and `max_concurrency` are configured, and read it again
    /// whenever the budget runs out and its reset passes. Only
    /// [`ProcessShardManager::spawn_auto`] has a token to do this;
    /// [`ProcessShardManager::spawn`] rejects it.
    #[default]
    FromGateway,
    /// Start from these numbers (e.g. from your own `/gateway/bot` call).
    /// After `reset_after` passes, the budget is assumed to be back at
    /// `total`, minus reservations still outstanding (there is nothing to
    /// refresh from).
    Fixed(SessionStartLimit),
    /// Do not track the session-start budget. Only bucket pacing applies.
    Unchecked,
}

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
    /// IDENTIFY pacing comes from the permits children's gateway clients
    /// request through [`ShardChildProcess::identify_gate`]. Set a stagger
    /// only for custom child programs that do not speak the identify part of
    /// the protocol.
    pub spawn_stagger: Duration,
    /// Discord's `max_concurrency`: how many IDENTIFY buckets
    /// (`shard_id % max_concurrency`) may identify in parallel. `None` means
    /// [`ProcessShardManager::spawn_auto`] reads it from `/gateway/bot`, and
    /// [`ProcessShardManager::spawn`] assumes `1` (the value for bots
    /// without large bot sharding).
    pub max_concurrency: Option<u32>,
    /// Daily session-start budget source (default
    /// [`SessionStartBudget::FromGateway`]).
    pub session_start_budget: SessionStartBudget,
    /// Minimum spacing between IDENTIFYs in one bucket, measured from the
    /// previous attempt's reported send (default 5s, Discord's documented
    /// window). Lower it only in tests.
    pub identify_interval: Duration,
    /// How long a child may hold an IDENTIFY permit before the parent kills
    /// it (default 30s). The bucket is reused only after that child has
    /// exited. The gateway client gives up connection setup (connect and
    /// HELLO) after 15s while holding a permit, so this only fires for a
    /// stuck child.
    pub identify_permit_timeout: Duration,
    /// Per-child response deadline used by [`ProcessShardManager::broadcast`]
    /// (default 10s). Children cancel request handlers after the same time.
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
            session_start_budget: SessionStartBudget::FromGateway,
            identify_interval: Duration::from_secs(5),
            identify_permit_timeout: Duration::from_secs(30),
            broadcast_timeout: DEFAULT_REQUEST_TIMEOUT,
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

// ---------------------------------------------------------------------------
// IDENTIFY coordination (parent side)
// ---------------------------------------------------------------------------

/// Daily session-start budget, counted conservatively.
///
/// Every grant takes a reservation. `available` is what may still be
/// reserved: Discord's `remaining` at the last refresh, minus every
/// reservation that could still IDENTIFY against that window. That includes
/// reservations that were outstanding when the refresh happened, because
/// they can still send after it. Each reservation is settled exactly once:
/// `Sent` keeps the start consumed and `Abandoned` gives it back.
///
/// Outstanding reservations are always part of the current count, so a
/// refund is always relative to a count that includes it, even across
/// refreshes. `available` may go negative to remember that debt; without
/// that, a refund after a refresh could hand out a start Discord no longer
/// has.
///
/// A reservation that settles `Sent` while a refresh request is in flight
/// stays charged against that response. Discord may have computed
/// `remaining` before that IDENTIFY happened.
struct SessionBudget {
    state: StdMutex<BudgetState>,
    /// Serializes reservers: one of them waits for a refund or the reset
    /// (and refreshes), and the rest queue behind it in order.
    reserve_lock: AsyncMutex<()>,
    /// Bumped on every refund so a waiting reserver rechecks at once instead
    /// of sleeping until the reset.
    changed: watch::Sender<()>,
    refresher: Option<RestClient>,
    /// Base delay for retrying a failed or still-exhausted refresh.
    retry_delay: Duration,
}

#[derive(Debug, PartialEq, Eq)]
struct BudgetState {
    /// Starts that may still be reserved. Negative while outstanding
    /// reservations exceed what Discord reported at the last refresh.
    available: i64,
    total: u32,
    /// Granted reservations that are not settled yet.
    outstanding: u32,
    reset_at: Instant,
    /// Consecutive failed refreshes, for the retry backoff.
    refresh_failures: u32,
    /// Set while a `/gateway/bot` request is in flight.
    refreshing: bool,
    /// Reservations that settled `Sent` while the request was in flight.
    /// Discord may have computed `remaining` before that IDENTIFY, so these
    /// stay charged against the response even though they are no longer
    /// outstanding.
    sent_during_refresh: u32,
}

impl BudgetState {
    fn from_limit(limit: &SessionStartLimit) -> Self {
        Self {
            available: i64::from(limit.remaining),
            total: limit.total.max(limit.remaining).max(1),
            outstanding: 0,
            reset_at: deadline_after(Duration::from_millis(limit.reset_after)),
            refresh_failures: 0,
            refreshing: false,
            sent_during_refresh: 0,
        }
    }

    /// Takes Discord's fresh numbers while keeping every reservation that
    /// may not be reflected in them counted against the new window: those
    /// still outstanding, and those that settled `Sent` while the request
    /// was in flight. (Reservations abandoned meanwhile never reached
    /// Discord and need no charge; no new ones can start, because the
    /// refreshing reserver holds `reserve_lock`.)
    fn reconcile(&mut self, limit: &SessionStartLimit, retry_delay: Duration) {
        self.total = limit.total.max(limit.remaining).max(1);
        let unaccounted = i64::from(self.outstanding) + i64::from(self.sent_during_refresh);
        self.available = i64::from(limit.remaining) - unaccounted;
        self.refresh_failures = 0;
        let mut reset_in = Duration::from_millis(limit.reset_after);
        if self.available <= 0 {
            // Still exhausted: do not ask again right away.
            reset_in = reset_in.max(retry_delay);
        }
        self.reset_at = deadline_after(reset_in);
    }
}

/// One reserved session start. It is settled exactly once; dropping it
/// unsettled (the holder died) counts as sent.
struct BudgetReservation {
    budget: Arc<SessionBudget>,
    settled: bool,
}

impl BudgetReservation {
    fn settle(mut self, outcome: IdentifyOutcome) {
        self.settle_once(outcome);
    }

    fn settle_once(&mut self, outcome: IdentifyOutcome) {
        if !std::mem::replace(&mut self.settled, true) {
            self.budget.settle(outcome);
        }
    }
}

impl Drop for BudgetReservation {
    fn drop(&mut self) {
        self.settle_once(IdentifyOutcome::Sent);
    }
}

impl SessionBudget {
    fn new(state: BudgetState, refresher: Option<RestClient>) -> Self {
        Self {
            state: StdMutex::new(state),
            reserve_lock: AsyncMutex::new(()),
            changed: watch::channel(()).0,
            refresher,
            retry_delay: BUDGET_RETRY_DELAY,
        }
    }

    /// Reserves one session start, waiting for a refund or the reset when
    /// none are left. Cancel-safe: the counters only change in synchronous
    /// sections, and the reservation is returned without another await.
    async fn reserve(self: &Arc<Self>) -> BudgetReservation {
        let _serialized = self.reserve_lock.lock().await;
        let mut changes = self.changed.subscribe();
        let mut logged = false;
        loop {
            // Mark refunds seen *before* checking, so one that lands after
            // the check still wakes the wait below.
            changes.borrow_and_update();
            let reset_at = {
                let mut state = lock_or_recover(&self.state);
                if state.available > 0 {
                    state.available -= 1;
                    state.outstanding += 1;
                    return BudgetReservation {
                        budget: Arc::clone(self),
                        settled: false,
                    };
                }
                state.reset_at
            };
            if !logged {
                warn!(
                    wait_ms = log_millis(reset_at.saturating_duration_since(Instant::now())),
                    "session start budget exhausted; holding IDENTIFY grants until a refund \
                     or the reset"
                );
                logged = true;
            }
            tokio::select! {
                () = sleep_until(reset_at) => self.refresh().await,
                // A refund freed a start. The sender lives in `self`, so
                // this cannot report a closed channel while we run.
                _ = changes.changed() => {}
            }
        }
    }

    async fn refresh(&self) {
        let Some(rest) = &self.refresher else {
            // A fixed budget has nothing to ask: assume Discord restored the
            // daily total, with outstanding reservations still counted.
            let mut state = lock_or_recover(&self.state);
            state.available = i64::from(state.total) - i64::from(state.outstanding);
            state.reset_at = deadline_after(BUDGET_FALLBACK_WINDOW);
            return;
        };
        {
            let mut state = lock_or_recover(&self.state);
            state.refreshing = true;
            state.sent_during_refresh = 0;
        }
        let fetched = rest.get_gateway_bot().await;
        let mut state = lock_or_recover(&self.state);
        state.refreshing = false;
        match fetched {
            Ok(gateway_bot) => {
                state.reconcile(&gateway_bot.session_start_limit, self.retry_delay);
                info!(
                    available = state.available,
                    outstanding = state.outstanding,
                    total = state.total,
                    "session start budget refreshed"
                );
            }
            Err(error) => {
                // Without fresh numbers the budget stays closed: assuming a
                // reset could start sessions Discord would refuse.
                let delay = respawn_backoff(self.retry_delay, state.refresh_failures);
                state.refresh_failures = state.refresh_failures.saturating_add(1);
                state.reset_at = deadline_after(delay);
                warn!(
                    retry_ms = log_millis(delay),
                    "refreshing the session start limit failed: {error}; keeping IDENTIFY \
                     grants paused"
                );
            }
        }
        state.sent_during_refresh = 0;
    }

    fn settle(&self, outcome: IdentifyOutcome) {
        let refunded = {
            let mut state = lock_or_recover(&self.state);
            state.outstanding = state.outstanding.saturating_sub(1);
            if outcome == IdentifyOutcome::Sent && state.refreshing {
                state.sent_during_refresh = state.sent_during_refresh.saturating_add(1);
            }
            if outcome == IdentifyOutcome::Abandoned {
                state.available += 1;
                true
            } else {
                false
            }
        };
        if refunded {
            self.changed.send_replace(());
        }
    }
}

/// Per-bucket IDENTIFY queues and the session budget, shared by every child
/// monitor.
///
/// Each bucket is a FIFO `tokio::sync::Mutex`. Holding its guard means
/// "this bucket's IDENTIFY attempt is in progress", so waiters queue fairly,
/// and a waiter that is dropped (its child exited) leaves the queue without
/// disturbing the others.
struct IdentifyScheduler {
    buckets: Vec<Arc<AsyncMutex<BucketState>>>,
    max_concurrency: u32,
    interval: Duration,
    budget: Option<Arc<SessionBudget>>,
}

struct BucketState {
    /// Earliest moment the next permit for this bucket may be granted.
    available_at: Instant,
}

/// Ownership of a bucket, plus its session-start reservation, for one
/// granted IDENTIFY attempt. It ends with the attempt's reported outcome,
/// or, when dropped without one (the child exited or was killed while
/// holding it), as `Sent`, the safe assumption.
struct BucketLease {
    bucket: Option<OwnedMutexGuard<BucketState>>,
    interval: Duration,
    reservation: Option<BudgetReservation>,
}

impl BucketLease {
    fn finish(mut self, outcome: IdentifyOutcome) {
        self.end(outcome);
    }

    fn end(&mut self, outcome: IdentifyOutcome) {
        if let Some(reservation) = self.reservation.take() {
            reservation.settle(outcome);
        }
        let Some(mut bucket) = self.bucket.take() else {
            return;
        };
        if outcome == IdentifyOutcome::Sent {
            bucket.available_at = deadline_after(self.interval);
        }
    }
}

impl Drop for BucketLease {
    fn drop(&mut self) {
        self.end(IdentifyOutcome::Sent);
    }
}

impl IdentifyScheduler {
    fn new(
        max_concurrency: u32,
        total_shards: u32,
        interval: Duration,
        budget: Option<Arc<SessionBudget>>,
    ) -> Self {
        let max_concurrency = max_concurrency.max(1);
        // `shard_id % max_concurrency` never exceeds the shard count, so
        // allocating more buckets than shards would be wasted.
        let bucket_count =
            usize::try_from(max_concurrency.min(total_shards.max(1))).unwrap_or(usize::MAX);
        let now = Instant::now();
        let buckets = (0..bucket_count)
            .map(|_| Arc::new(AsyncMutex::new(BucketState { available_at: now })))
            .collect();
        Self {
            buckets,
            max_concurrency,
            interval,
            budget,
        }
    }

    fn bucket_of(&self, shard_id: u32) -> usize {
        // The outer modulo is a no-op for valid shard ids and keeps the
        // index in range for invalid ones; `buckets` is never empty.
        usize::try_from(shard_id % self.max_concurrency).unwrap_or(0) % self.buckets.len()
    }

    async fn acquire(&self, shard_id: u32) -> BucketLease {
        let bucket = Arc::clone(&self.buckets[self.bucket_of(shard_id)]);
        let guard = bucket.lock_owned().await;
        // Cancelling during either wait drops the guard without touching
        // `available_at` or the budget, which is correct because nothing
        // was granted.
        sleep_until(guard.available_at).await;
        let reservation = match &self.budget {
            Some(budget) => Some(budget.reserve().await),
            None => None,
        };
        BucketLease {
            bucket: Some(guard),
            interval: self.interval,
            reservation,
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
    // Written by the child's monitor task, snapshotted by `children()`.
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
    kill_reap_timeout: Duration,
    max_frame: usize,
    scheduler: Arc<IdentifyScheduler>,
    identify_permit_timeout: Duration,
}

/// Knobs that are not part of the public configuration; tests shorten them.
struct SpawnInternals {
    /// How long to wait for a killed child to be reaped. Zero skips the wait
    /// and treats the exit as unconfirmed right away.
    kill_reap_timeout: Duration,
}

impl Default for SpawnInternals {
    fn default() -> Self {
        Self {
            kill_reap_timeout: KILL_REAP_TIMEOUT,
        }
    }
}

/// Shared outcome of all monitors: how many are still running and which
/// ones ended in failure.
#[derive(Clone, Debug, Default)]
struct CompletionState {
    remaining: usize,
    failures: Vec<String>,
}

/// Records one monitor's terminal outcome when the monitor task ends for
/// any reason, including a panic or runtime shutdown, so waiters neither
/// hang on a monitor that is gone nor mistake its failure for success.
struct MonitorCompletion {
    state: Arc<watch::Sender<CompletionState>>,
    label: String,
    result: Option<Result<(), String>>,
}

impl Drop for MonitorCompletion {
    fn drop(&mut self) {
        let failure = match self.result.take() {
            Some(Ok(())) => None,
            Some(Err(error)) => Some(format!("{}: {error}", self.label)),
            None if std::thread::panicking() => Some(format!("{}: monitor panicked", self.label)),
            None => Some(format!(
                "{}: monitor was cancelled before its child was confirmed stopped \
                 (Tokio runtime shut down?)",
                self.label
            )),
        };
        if let Some(failure) = &failure {
            warn!("{failure}");
        }
        self.state.send_modify(|state| {
            state.remaining = state.remaining.saturating_sub(1);
            if let Some(failure) = failure {
                state.failures.push(failure);
            }
        });
    }
}

fn completion_result(state: &CompletionState) -> Result<(), DiscordError> {
    if state.failures.is_empty() {
        Ok(())
    } else {
        Err(DiscordError::gateway(format!(
            "{} shard child monitor(s) failed: {}",
            state.failures.len(),
            state.failures.join("; ")
        )))
    }
}

/// Multi-process shard manager: spawns one child process per shard group,
/// respawns crashed children with exponential backoff, grants IDENTIFY
/// permits per `max_concurrency` bucket within the session-start budget, and
/// provides request/response broadcasting over framed IPC — the discord.js
/// `ShardingManager` equivalent.
///
/// Children are started with `DISCORDRS_SHARD_IDS`, `DISCORDRS_SHARD_COUNT`,
/// `DISCORDRS_SHARD_IPC=1`, `DISCORDRS_SHARD_IPC_MAX_FRAME`, and
/// `DISCORDRS_SHARD_IPC_REQUEST_TIMEOUT_MS` in their environment so the same
/// binary can detect child mode via [`ShardChildProcess::from_env`].
///
/// Use [`Self::shutdown`] to stop the children and learn the outcome.
/// Dropping the manager also stops them, but only in the background and
/// only while the Tokio runtime keeps running.
pub struct ProcessShardManager {
    total_shards: u32,
    max_concurrency: u32,
    children: Vec<ChildEntry>,
    shutdown_tx: watch::Sender<bool>,
    completion: watch::Receiver<CompletionState>,
    // `broadcast(&self)` may run concurrently; request ids must be unique.
    next_request_id: AtomicU64,
    broadcast_timeout: Duration,
    max_frame: usize,
}

impl ProcessShardManager {
    /// Starts child processes for every shard group described by `config`,
    /// without contacting Discord.
    ///
    /// `config.total_shards` must be `Some`. Without a token the
    /// session-start budget cannot be read, so `config.session_start_budget`
    /// must be [`SessionStartBudget::Fixed`] or
    /// [`SessionStartBudget::Unchecked`]; use [`Self::spawn_auto`] to read
    /// the shard count, `max_concurrency`, and the budget from Discord.
    /// Without `config.max_concurrency`, permits are granted for one bucket.
    pub async fn spawn(config: ProcessShardManagerConfig) -> Result<Self, DiscordError> {
        let budget = match &config.session_start_budget {
            SessionStartBudget::FromGateway => {
                return Err(DiscordError::model(
                    "ProcessShardManager::spawn cannot read the session start limit without \
                     a token; use spawn_auto(token, ..), or set session_start_budget to \
                     SessionStartBudget::Fixed(..) or SessionStartBudget::Unchecked",
                ));
            }
            SessionStartBudget::Fixed(limit) => {
                Some(SessionBudget::new(BudgetState::from_limit(limit), None))
            }
            SessionStartBudget::Unchecked => None,
        };
        Self::start(config, budget, SpawnInternals::default()).await
    }

    /// Like [`Self::spawn`], but reads `/gateway/bot` with `token`.
    ///
    /// The response fills `config.total_shards` and `config.max_concurrency`
    /// when they are `None`, and, with the default
    /// [`SessionStartBudget::FromGateway`], seeds the session-start budget,
    /// which is read again whenever it runs out. That request is made even
    /// when the shard count and concurrency are configured, and its failure
    /// is an error, because skipping it would silently disable budget
    /// protection.
    pub async fn spawn_auto(
        token: impl Into<String>,
        config: ProcessShardManagerConfig,
    ) -> Result<Self, DiscordError> {
        let rest = RestClient::new(token, None);
        Self::spawn_auto_with(rest, config).await
    }

    async fn spawn_auto_with(
        rest: RestClient,
        mut config: ProcessShardManagerConfig,
    ) -> Result<Self, DiscordError> {
        let budget_from_gateway =
            matches!(config.session_start_budget, SessionStartBudget::FromGateway);
        let gateway_bot = if budget_from_gateway
            || config.total_shards.is_none()
            || config.max_concurrency.is_none()
        {
            match rest.get_gateway_bot().await {
                Ok(gateway_bot) => Some(gateway_bot),
                Err(error) if !budget_from_gateway && config.total_shards.is_some() => {
                    warn!(
                        "failed to fetch /gateway/bot for max_concurrency; \
                         granting one IDENTIFY at a time: {error}"
                    );
                    None
                }
                Err(error) => return Err(error),
            }
        } else {
            None
        };
        if let Some(gateway_bot) = &gateway_bot {
            let limit = &gateway_bot.session_start_limit;
            info!(
                shards = gateway_bot.shards,
                max_concurrency = limit.max_concurrency,
                remaining = limit.remaining,
                "read /gateway/bot sharding parameters"
            );
            if config.total_shards.is_none() {
                config.total_shards = Some(gateway_bot.shards.max(1));
            }
            if config.max_concurrency.is_none() {
                config.max_concurrency = Some(limit.max_concurrency.max(1));
            }
        }
        let budget = match (&config.session_start_budget, &gateway_bot) {
            (SessionStartBudget::FromGateway, Some(gateway_bot)) => Some(SessionBudget::new(
                BudgetState::from_limit(&gateway_bot.session_start_limit),
                Some(rest),
            )),
            (SessionStartBudget::Fixed(limit), _) => {
                Some(SessionBudget::new(BudgetState::from_limit(limit), None))
            }
            _ => None,
        };
        Self::start(config, budget, SpawnInternals::default()).await
    }

    async fn start(
        config: ProcessShardManagerConfig,
        budget: Option<SessionBudget>,
        internals: SpawnInternals,
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
        let shards_per_process =
            usize::try_from(config.shards_per_process.max(1)).unwrap_or(usize::MAX);
        let max_concurrency = config.max_concurrency.unwrap_or(1).max(1);
        let max_frame = config.max_ipc_frame_bytes.max(MIN_IPC_FRAME_BYTES);
        let scheduler = Arc::new(IdentifyScheduler::new(
            max_concurrency,
            total_shards,
            config.identify_interval,
            budget.map(Arc::new),
        ));

        let groups: Vec<Vec<u32>> = shard_ids
            .chunks(shards_per_process)
            .map(<[u32]>::to_vec)
            .collect();
        let (shutdown_tx, _) = watch::channel(false);
        let (completion_tx, completion) = watch::channel(CompletionState {
            remaining: groups.len(),
            failures: Vec::new(),
        });
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
                (
                    ENV_SHARD_IPC_REQUEST_TIMEOUT_MS.to_string(),
                    config.broadcast_timeout.as_millis().to_string(),
                ),
            ];
            env.extend(config.extra_env.iter().cloned());

            let label = format!("shard child {index} (shards {group:?})");
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
                kill_reap_timeout: internals.kill_reap_timeout,
                max_frame,
                scheduler: Arc::clone(&scheduler),
                identify_permit_timeout: config.identify_permit_timeout,
            };

            let shutdown_rx = shutdown_tx.subscribe();
            let completion = MonitorCompletion {
                state: Arc::clone(&completion_tx),
                label,
                result: None,
            };
            tokio::spawn(async move {
                // Move the whole guard into the task; capturing only
                // `completion.result` would drop the guard (and report the
                // monitor as finished) right here.
                let mut completion = completion;
                completion.result = Some(run_child_monitor(context, command_rx, shutdown_rx).await);
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
            completion,
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
    /// called [`ShardChildProcess::serve`] yet, is at its concurrent-request
    /// limit, or is not draining its stdin. Callers should treat `Null`
    /// entries as "shard unavailable".
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

    /// Stops all children and waits for the outcome. Each child receives the
    /// `shutdown` op and gets `shutdown_timeout` in total to exit before it
    /// is killed.
    ///
    /// Resolves once every child monitor has finished, with `Ok` only if
    /// every child ended by this request (or, with respawn disabled, by a
    /// successful exit). Otherwise the error lists each monitor's terminal
    /// outcome:
    ///
    /// - gave up after `max_respawns`;
    /// - exited unsuccessfully with respawn disabled;
    /// - could not be spawned;
    /// - `child.wait()` failed (the child was killed and its exit confirmed
    ///   first, if possible);
    /// - killed but its exit not confirmed (its IDENTIFY permits stay held
    ///   until it does exit);
    /// - the monitor panicked or was cancelled by runtime shutdown.
    ///
    /// Safe to call concurrently with [`Self::wait`] and more than once.
    pub async fn shutdown(&self) -> Result<(), DiscordError> {
        info!("process shard manager shutting down");
        self.shutdown_tx.send_replace(true);
        self.wait().await
    }

    /// Waits until every child monitor has finished, i.e. all children have
    /// exited and no further respawns will happen, and returns the same
    /// outcome as [`Self::shutdown`]. Any number of callers may wait
    /// concurrently; all of them observe the same result.
    pub async fn wait(&self) -> Result<(), DiscordError> {
        let mut completion = self.completion.clone();
        let finished = completion
            .wait_for(|state| state.remaining == 0)
            .await
            .map(|state| state.clone());
        // An error means every monitor (and with it the sender) is gone; the
        // last value is final.
        let state = finished.unwrap_or_else(|_| completion.borrow().clone());
        completion_result(&state)
    }
}

// ---------------------------------------------------------------------------
// Child monitor task
// ---------------------------------------------------------------------------

enum ChildRunOutcome {
    /// The child exited, on its own or after the monitor killed it (permit
    /// held too long, broken IPC session), and the exit is confirmed. A
    /// respawn may follow.
    Exited(ChildExit),
    /// Shutdown was requested and the child is confirmed gone.
    Stopped,
    /// The child's end could not be supervised normally: waiting for it
    /// failed, or it was killed but its exit could not be confirmed. The
    /// monitor stops and reports this instead of respawning a duplicate
    /// next to a process that may still be alive.
    Failed(String),
}

/// How a child that was not asked to stop ended, kept for the monitor's
/// terminal report.
struct ChildExit {
    success: bool,
    description: String,
}

impl ChildExit {
    fn from_status(status: std::process::ExitStatus, killed_for: Option<&str>) -> Self {
        match killed_for {
            Some(reason) => Self {
                success: false,
                description: format!("was killed by the manager ({reason})"),
            },
            None => Self {
                success: status.success(),
                description: format!("exited with {status}"),
            },
        }
    }
}

/// Resolves once shutdown has been requested or the sender is gone, which
/// also means "stop".
async fn wait_for_shutdown(shutdown_rx: &mut watch::Receiver<bool>) {
    shutdown_rx.wait_for(|requested| *requested).await.ok();
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
) -> Result<(), String> {
    if !context.start_delay.is_zero()
        && !idle_until(
            deadline_after(context.start_delay),
            &mut shutdown_rx,
            &mut command_rx,
        )
        .await
    {
        set_state(&context.status, ChildState::Stopped);
        return Ok(());
    }

    let mut backoff_attempt: u32 = 0;
    let mut respawns: u32 = 0;

    loop {
        if shutdown_requested(&shutdown_rx) {
            set_state(&context.status, ChildState::Stopped);
            return Ok(());
        }

        set_state(&context.status, ChildState::Starting);
        // Every terminal outcome reaches `wait()`/`shutdown()`: a monitor
        // that stops for any reason other than a requested shutdown (or a
        // successful exit with respawn disabled) reports how its last child
        // ended.
        let last_exit = match spawn_child_process(&context) {
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
                    ChildRunOutcome::Stopped => {
                        set_state(&context.status, ChildState::Stopped);
                        info!(
                            child = context.index,
                            shard_ids = ?context.shard_ids,
                            "shard child stopped after shutdown request"
                        );
                        return Ok(());
                    }
                    ChildRunOutcome::Failed(reason) => {
                        set_state(&context.status, ChildState::Stopped);
                        return Err(reason);
                    }
                    ChildRunOutcome::Exited(exit) => {
                        warn!(
                            child = context.index,
                            shard_ids = ?context.shard_ids,
                            uptime_secs = started.elapsed().as_secs(),
                            "shard child {}",
                            exit.description
                        );
                        if started.elapsed() >= BACKOFF_RESET_UPTIME {
                            backoff_attempt = 0;
                        }
                        exit
                    }
                }
            }
            Err(error) => {
                warn!(
                    child = context.index,
                    shard_ids = ?context.shard_ids,
                    "failed to spawn shard child: {error}"
                );
                ChildExit {
                    success: false,
                    description: format!("could not be spawned: {error}"),
                }
            }
        };

        if !context.respawn {
            set_state(&context.status, ChildState::Stopped);
            return if last_exit.success {
                Ok(())
            } else {
                Err(format!(
                    "the child {}; respawn is disabled",
                    last_exit.description
                ))
            };
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
                return Err(format!(
                    "gave up after {max} respawn(s); the last child {}",
                    last_exit.description
                ));
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
            delay_ms = log_millis(delay),
            "respawning shard child after backoff"
        );
        if !idle_until(deadline_after(delay), &mut shutdown_rx, &mut command_rx).await {
            set_state(&context.status, ChildState::Stopped);
            return Ok(());
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
/// pipe write.
///
/// Control frames (grants, shutdown) go through an unbounded queue that the
/// task always serves first. The protocol bounds them to one grant per
/// assigned shard plus one shutdown, so they can never be dropped. Data
/// frames (broadcast requests) go through a bounded queue and fail fast when
/// it is full. A failed write is reported once through the receiver
/// returned by [`Self::spawn`]: the child can no longer receive grants or
/// shutdown, so the session is over. Dropping the writer aborts the task,
/// which closes stdin even if a write is stuck.
struct ParentWriter {
    control_tx: Option<mpsc::UnboundedSender<String>>,
    data_tx: Option<mpsc::Sender<String>>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl ParentWriter {
    fn spawn(stdin: Option<ChildStdin>) -> (Self, oneshot::Receiver<String>) {
        let (control_tx, mut control_rx) = mpsc::unbounded_channel::<String>();
        let (data_tx, mut data_rx) = mpsc::channel::<String>(IPC_WRITE_QUEUE_CAPACITY);
        let (failed_tx, failed_rx) = oneshot::channel();
        let task = match stdin {
            Some(mut stdin) => Some(crate::task::spawn_erased(async move {
                loop {
                    let frame = tokio::select! {
                        biased;
                        Some(frame) = control_rx.recv() => frame,
                        Some(frame) = data_rx.recv() => frame,
                        else => break,
                    };
                    let written = async {
                        stdin.write_all(frame.as_bytes()).await?;
                        stdin.flush().await
                    }
                    .await;
                    if let Err(error) = written {
                        // Fails only if the monitor already stopped
                        // watching this child.
                        failed_tx.send(error.to_string()).ok();
                        return;
                    }
                }
                // Both queues closed by `close()`: dropping stdin sends EOF.
            })),
            None => {
                // The receiver is returned below, so this cannot fail.
                failed_tx.send("child stdin pipe missing".to_string()).ok();
                None
            }
        };
        (
            Self {
                control_tx: Some(control_tx),
                data_tx: Some(data_tx),
                task,
            },
            failed_rx,
        )
    }

    fn send_control(&self, frame: String) -> Result<(), &'static str> {
        match &self.control_tx {
            Some(tx) => tx.send(frame).map_err(|_| "child stdin writer has stopped"),
            None => Err("IPC writer is closed"),
        }
    }

    fn try_send_data(&self, frame: String) -> Result<(), &'static str> {
        let Some(tx) = &self.data_tx else {
            return Err("IPC writer is closed");
        };
        tx.try_send(frame).map_err(|error| match error {
            mpsc::error::TrySendError::Full(_) => {
                "IPC write queue is full (child is not reading stdin)"
            }
            mpsc::error::TrySendError::Closed(_) => "child stdin writer has stopped",
        })
    }

    /// Stops accepting frames. Already queued frames are still written,
    /// control frames first, then stdin is closed.
    fn close(&mut self) {
        self.control_tx = None;
        self.data_tx = None;
    }
}

impl Drop for ParentWriter {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

type IdentifyWait = Pin<Box<dyn Future<Output = (u32, BucketLease)> + Send>>;

struct HeldPermit {
    lease_id: u64,
    lease: BucketLease,
    expires_at: Instant,
}

/// IPC state of one child process instance. Dropping it (after the child's
/// exit is confirmed) fails pending broadcasts with `null`, leaves the
/// IDENTIFY queues, and returns held buckets as if their IDENTIFY was sent.
struct ChildSession<'a> {
    context: &'a MonitorContext,
    pending: HashMap<u64, oneshot::Sender<Value>>,
    ready: HashSet<u32>,
    identify_waiting: FuturesUnordered<IdentifyWait>,
    identify_queued: HashSet<u32>,
    identify_held: HashMap<u32, HeldPermit>,
    next_lease_id: u64,
    /// Set once the child is being stopped or killed: no further grants.
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
            next_lease_id: 0,
            stopping: false,
        }
    }

    fn forward_request(
        &mut self,
        writer: &ParentWriter,
        id: u64,
        frame: String,
        respond_to: oneshot::Sender<Value>,
    ) {
        if respond_to.is_closed() {
            return; // broadcaster already timed out
        }
        self.pending.retain(|_, sender| !sender.is_closed());
        match writer.try_send_data(frame) {
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
                    if sender.send(payload).is_err() {
                        debug!(
                            child = context.index,
                            request_id = id,
                            "shard child response arrived after the request timed out"
                        );
                    }
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
            ChildMessage::IdentifyRelease {
                shard_id,
                lease,
                sent,
            } => self.release_identify(shard_id, lease, sent),
        }
    }

    fn queue_identify(&mut self, shard_id: u32) {
        let index = self.context.index;
        if self.stopping {
            debug!(
                child = index,
                shard_id, "ignoring identify request while stopping"
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
        let scheduler = Arc::clone(&self.context.scheduler);
        self.identify_waiting.push(Box::pin(async move {
            let lease = scheduler.acquire(shard_id).await;
            (shard_id, lease)
        }));
    }

    /// Records the grant as held, then queues it for delivery. The bucket
    /// stays held whether or not the frame arrives: only a matching release
    /// or the child's confirmed exit gives it back. An error means the
    /// writer is gone and the child must be stopped.
    fn grant_identify(
        &mut self,
        writer: &ParentWriter,
        shard_id: u32,
        lease: BucketLease,
    ) -> Result<(), String> {
        self.identify_queued.remove(&shard_id);
        self.next_lease_id += 1;
        let lease_id = self.next_lease_id;
        self.identify_held.insert(
            shard_id,
            HeldPermit {
                lease_id,
                lease,
                expires_at: deadline_after(self.context.identify_permit_timeout),
            },
        );
        let frame = frame_line(&ParentMessage::IdentifyGrant {
            shard_id,
            lease: lease_id,
        })
        .map_err(|error| error.to_string())?;
        writer.send_control(frame).map_err(|reason| {
            format!("cannot deliver identify grant for shard {shard_id}: {reason}")
        })?;
        debug!(
            child = self.context.index,
            shard_id, lease_id, "granted IDENTIFY permit"
        );
        Ok(())
    }

    fn release_identify(&mut self, shard_id: u32, lease_id: u64, sent: bool) {
        let index = self.context.index;
        match self.identify_held.get(&shard_id) {
            Some(held) if held.lease_id == lease_id => {
                if let Some(held) = self.identify_held.remove(&shard_id) {
                    held.lease.finish(if sent {
                        IdentifyOutcome::Sent
                    } else {
                        IdentifyOutcome::Abandoned
                    });
                }
            }
            Some(held) => warn!(
                child = index,
                shard_id,
                lease_id,
                held_lease = held.lease_id,
                "identify release for a stale lease; ignoring"
            ),
            None => debug!(
                child = index,
                shard_id, "identify release without a held permit"
            ),
        }
    }

    fn next_permit_expiry(&self) -> Option<Instant> {
        if self.stopping {
            return None;
        }
        self.identify_held
            .values()
            .map(|held| held.expires_at)
            .min()
    }

    fn expired_permit(&self) -> String {
        let now = Instant::now();
        let expired = self
            .identify_held
            .iter()
            .find(|(_, held)| held.expires_at <= now)
            .map(|(&shard_id, held)| (shard_id, held.lease_id));
        match expired {
            Some((shard_id, lease_id)) => format!(
                "IDENTIFY permit {lease_id} for shard {shard_id} was not released within {:?}",
                self.context.identify_permit_timeout
            ),
            None => "an IDENTIFY permit was not released in time".to_string(),
        }
    }

    /// Stops queueing and granting. Held permits stay held until the
    /// child's exit is confirmed.
    fn stop_granting(&mut self) {
        self.stopping = true;
        self.identify_waiting = FuturesUnordered::new();
        self.identify_queued.clear();
    }

    fn take_held_leases(&mut self) -> Vec<BucketLease> {
        self.identify_held
            .drain()
            .map(|(_, held)| held.lease)
            .collect()
    }

    /// Processes stdout lines still buffered after the child exited.
    async fn drain_stdout(&mut self, reader: &mut FrameReader<ChildStdout>) {
        let drained = timeout(EXIT_DRAIN_TIMEOUT, async {
            while !reader.is_closed() {
                let event = reader.next_event().await;
                self.handle_stdout(event);
            }
        })
        .await;
        if drained.is_err() {
            // Something else (e.g. a grandchild) still holds the pipe open.
            debug!("stopped draining an exited shard child's stdout after {EXIT_DRAIN_TIMEOUT:?}");
        }
    }
}

/// Kills the child (if not already doing so) without waiting; the supervise
/// loop keeps running until `child.wait()` confirms the exit, and held
/// permits stay held until then.
fn begin_kill(
    context: &MonitorContext,
    child: &mut Child,
    session: &mut ChildSession<'_>,
    killing: &mut Option<String>,
    reason: String,
) {
    if killing.is_some() {
        return;
    }
    warn!(child = context.index, "killing shard child: {reason}");
    session.stop_granting();
    if let Err(error) = child.start_kill() {
        debug!(
            child = context.index,
            "kill failed (child already gone?): {error}"
        );
    }
    *killing = Some(reason);
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
        return match kill_and_confirm(context, child, Vec::new()).await {
            Ok(()) => ChildRunOutcome::Exited(ChildExit {
                success: false,
                description: "had no stdout pipe and was killed".to_string(),
            }),
            Err(reason) => ChildRunOutcome::Failed(reason),
        };
    };
    let (writer, mut writer_failed) = ParentWriter::spawn(child.stdin.take());
    let mut writer_watch = true;
    let mut reader = FrameReader::new(stdout, context.max_frame);
    let mut session = ChildSession::new(context);
    let mut killing: Option<String> = None;

    loop {
        let permit_expiry = session.next_permit_expiry();
        tokio::select! {
            () = wait_for_shutdown(shutdown_rx) => {
                return graceful_stop(child, writer, &mut reader, session).await;
            }
            status = child.wait() => {
                return match status {
                    Ok(status) => {
                        info!(child = context.index, %status, "shard child exited");
                        session.drain_stdout(&mut reader).await;
                        // Exit confirmed: held buckets are returned when
                        // `session` drops, counted as sent.
                        ChildRunOutcome::Exited(ChildExit::from_status(status, killing.as_deref()))
                    }
                    Err(error) => wait_failed(context, child, session, error).await,
                };
            }
            command = command_rx.recv() => match command {
                Some(ChildCommand::Request { id, frame, respond_to }) => {
                    session.forward_request(&writer, id, frame, respond_to);
                }
                None => {
                    // The manager was dropped: do not leave orphans.
                    return graceful_stop(child, writer, &mut reader, session).await;
                }
            },
            event = reader.next_event(), if !reader.is_closed() => {
                session.handle_stdout(event);
            }
            Some((shard_id, lease)) = session.identify_waiting.next(),
                if !session.identify_waiting.is_empty() => {
                if let Err(reason) = session.grant_identify(&writer, shard_id, lease) {
                    begin_kill(context, &mut child, &mut session, &mut killing, reason);
                }
            }
            () = sleep_until(permit_expiry.unwrap_or_else(Instant::now)),
                if permit_expiry.is_some() => {
                let reason = session.expired_permit();
                begin_kill(context, &mut child, &mut session, &mut killing, reason);
            }
            failure = &mut writer_failed, if writer_watch => {
                writer_watch = false;
                if let Ok(error) = failure {
                    let reason = format!(
                        "IPC write to the child failed ({error}); it can no longer \
                         receive grants or shutdown"
                    );
                    begin_kill(context, &mut child, &mut session, &mut killing, reason);
                }
            }
        }
    }
}

/// Stops a child within `shutdown_timeout`, measured from the start of
/// this call: queue the `shutdown` op ahead of any data, close stdin, keep
/// draining stdout (a child blocked on a full stdout pipe could not exit),
/// and kill the child when the deadline passes. Held IDENTIFY permits are
/// returned only once the exit is confirmed.
async fn graceful_stop(
    mut child: Child,
    mut writer: ParentWriter,
    reader: &mut FrameReader<ChildStdout>,
    mut session: ChildSession<'_>,
) -> ChildRunOutcome {
    let context = session.context;
    let deadline = deadline_after(context.shutdown_timeout);
    session.stop_granting();

    let queued = frame_line(&ParentMessage::Shutdown)
        .map_err(|error| error.to_string())
        .and_then(|frame| writer.send_control(frame).map_err(str::to_string));
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

    let confirmed = match exited {
        Ok(Ok(status)) => {
            debug!(child = context.index, %status, "shard child exited after shutdown op");
            Ok(())
        }
        Ok(Err(error)) => {
            warn!(
                child = context.index,
                "waiting for shard child failed ({error}); killing it"
            );
            // Even a confirmed kill leaves this visible: supervision of the
            // child broke.
            Err(
                match kill_and_confirm(context, child, session.take_held_leases()).await {
                    Ok(()) => format!(
                        "waiting for the child failed ({error}); it was killed and its exit \
                         confirmed"
                    ),
                    Err(reason) => format!("waiting for the child failed ({error}); {reason}"),
                },
            )
        }
        Err(_) => {
            warn!(
                child = context.index,
                timeout_ms = log_millis(context.shutdown_timeout),
                "shard child did not exit before the shutdown deadline; killing it"
            );
            kill_and_confirm(context, child, session.take_held_leases()).await
        }
    };
    // `writer` drops here and aborts a write that is still stuck.
    drop(writer);
    match confirmed {
        Ok(()) => ChildRunOutcome::Stopped,
        Err(reason) => ChildRunOutcome::Failed(reason),
    }
}

/// `child.wait()` failed, so the exit is not confirmed even if the process
/// may already be gone (`kill_on_drop` would not confirm anything either).
/// The child is killed, and its exit confirmed, before its held IDENTIFY
/// buckets are returned; if confirmation fails they stay held. Either way
/// supervision of this child broke, so the monitor stops and reports it.
async fn wait_failed(
    context: &MonitorContext,
    child: Child,
    mut session: ChildSession<'_>,
    error: io::Error,
) -> ChildRunOutcome {
    warn!(
        child = context.index,
        "waiting for shard child failed: {error}; killing it to confirm its exit"
    );
    session.stop_granting();
    let held = session.take_held_leases();
    ChildRunOutcome::Failed(match kill_and_confirm(context, child, held).await {
        Ok(()) => {
            format!("waiting for the child failed ({error}); it was killed and its exit confirmed")
        }
        Err(reason) => format!("waiting for the child failed ({error}); {reason}"),
    })
}

/// Kills the child and waits up to `kill_reap_timeout` for the OS to confirm
/// its exit. If the exit cannot be confirmed, the child and its held
/// IDENTIFY buckets move to a background task that keeps the buckets until
/// the exit is observed. Better to block a bucket than to let a live child
/// IDENTIFY next to another shard.
async fn kill_and_confirm(
    context: &MonitorContext,
    mut child: Child,
    held: Vec<BucketLease>,
) -> Result<(), String> {
    if let Err(error) = child.start_kill() {
        debug!(
            child = context.index,
            "kill failed (child already gone?): {error}"
        );
    }
    let reap_timeout = context.kill_reap_timeout;
    if !reap_timeout.is_zero() {
        match timeout(reap_timeout, child.wait()).await {
            Ok(Ok(_)) => return Ok(()),
            Ok(Err(error)) => {
                warn!(
                    child = context.index,
                    "waiting for killed shard child failed: {error}"
                );
            }
            Err(_) => {}
        }
    }
    let reason = format!(
        "shard child (pid {:?}) was killed but its exit was not confirmed within \
         {reap_timeout:?}; {} IDENTIFY permit(s) stay held until it exits",
        child.id(),
        held.len()
    );
    warn!(child = context.index, "{reason}");
    tokio::spawn(async move {
        match child.wait().await {
            Ok(_) => drop(held),
            Err(error) => {
                warn!(
                    "cannot confirm that a killed shard child exited ({error}); \
                     its IDENTIFY buckets stay blocked"
                );
                let _held = held;
                std::future::pending::<()>().await;
            }
        }
    });
    Err(reason)
}

// ---------------------------------------------------------------------------
// Child side
// ---------------------------------------------------------------------------

type FrameSink = Box<dyn FnMut(&[u8]) -> io::Result<()> + Send>;
type FrameSource = Box<dyn BufRead + Send>;
type RequestFuture = Pin<Box<dyn Future<Output = Value> + Send>>;
type RequestHandler = Arc<dyn Fn(Value) -> RequestFuture + Send + Sync>;

/// Values a child reads from its environment.
#[derive(Debug, PartialEq, Eq)]
struct ChildSettings {
    shard_ids: Vec<u32>,
    shard_count: u32,
    max_frame: usize,
    request_timeout: Duration,
    max_concurrent_requests: usize,
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
        let request_timeout = match get(ENV_SHARD_IPC_REQUEST_TIMEOUT_MS) {
            None => DEFAULT_REQUEST_TIMEOUT,
            Some(raw) => match raw.trim().parse::<u64>() {
                Ok(ms) => Duration::from_millis(ms),
                Err(_) => {
                    warn!(
                        "malformed {ENV_SHARD_IPC_REQUEST_TIMEOUT_MS} value {raw:?}; \
                         using the default"
                    );
                    DEFAULT_REQUEST_TIMEOUT
                }
            },
        };
        Some(Self {
            shard_ids,
            shard_count,
            max_frame,
            request_timeout,
            max_concurrent_requests: DEFAULT_MAX_CONCURRENT_REQUESTS,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FramePriority {
    /// IDENTIFY requests and releases: never rejected, written first. The
    /// identify slots bound them to two per assigned shard.
    Control,
    /// Responses, logs, ready notices: bounded, rejected when full.
    Data,
}

#[derive(Default)]
struct WriterQueues {
    control: VecDeque<Vec<u8>>,
    data: VecDeque<Vec<u8>>,
    closed: bool,
    failed: Option<String>,
}

/// Queues between child code and the stdout writer thread. Enqueueing
/// never blocks, so async tasks and destructors (like a dropped
/// [`IdentifyPermit`]) never wait on a full stdout pipe.
#[derive(Default)]
struct ChildWriter {
    queues: StdMutex<WriterQueues>,
    ready: Condvar,
}

impl ChildWriter {
    fn enqueue(&self, frame: Vec<u8>, priority: FramePriority) -> Result<(), DiscordError> {
        let mut queues = lock_or_recover(&self.queues);
        if let Some(error) = &queues.failed {
            return Err(DiscordError::gateway(format!(
                "IPC stdout write failed: {error}"
            )));
        }
        if queues.closed {
            return Err(DiscordError::gateway("IPC writer is closed"));
        }
        match priority {
            FramePriority::Control => queues.control.push_back(frame),
            FramePriority::Data => {
                if queues.data.len() >= CHILD_DATA_QUEUE_CAPACITY {
                    return Err(DiscordError::gateway(
                        "IPC write queue is full (the parent is not reading stdout)",
                    ));
                }
                queues.data.push_back(frame);
            }
        }
        drop(queues);
        self.ready.notify_one();
        Ok(())
    }

    /// Blocks until a frame is available (control first). `None` once the
    /// writer is closed and drained, or has failed.
    fn next_frame(&self) -> Option<Vec<u8>> {
        let mut queues = lock_or_recover(&self.queues);
        loop {
            if queues.failed.is_some() {
                return None;
            }
            if let Some(frame) = queues.control.pop_front() {
                return Some(frame);
            }
            if let Some(frame) = queues.data.pop_front() {
                return Some(frame);
            }
            if queues.closed {
                return None;
            }
            queues = self
                .ready
                .wait(queues)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }

    fn close(&self) {
        lock_or_recover(&self.queues).closed = true;
        self.ready.notify_all();
    }

    fn fail(&self, error: String) {
        {
            let mut queues = lock_or_recover(&self.queues);
            queues.failed = Some(error);
            queues.control.clear();
            queues.data.clear();
        }
        self.ready.notify_all();
    }

    fn has_failed(&self) -> bool {
        lock_or_recover(&self.queues).failed.is_some()
    }
}

/// Body of the stdout writer thread. A failed write ends the IPC session:
/// the parent can no longer hear this child, so the child is told to shut
/// down and every waiter is resolved.
fn run_child_writer(writer: Arc<ChildWriter>, mut sink: FrameSink, ipc: Weak<ChildIpc>) {
    while let Some(frame) = writer.next_frame() {
        if let Err(error) = sink(&frame) {
            warn!("IPC stdout write failed: {error}; ending the IPC session");
            writer.fail(error.to_string());
            if let Some(ipc) = ipc.upgrade() {
                ipc.close();
            }
            return;
        }
    }
}

enum IdentifySlot {
    /// Requested from the parent; the sender delivers the permit.
    Pending(oneshot::Sender<IdentifyPermit>),
    /// Granted and not yet finished.
    Held,
}

#[derive(Default)]
struct ChildIdentifyState {
    /// Set once the parent asked for shutdown or the IPC session ended; no
    /// further permits will be granted.
    closed: bool,
    slots: HashMap<u32, IdentifySlot>,
}

/// Shared child-side IPC state behind every [`ShardChildProcess`] clone and
/// every outstanding [`IdentifyPermit`].
struct ChildIpc {
    settings: ChildSettings,
    shutdown_tx: watch::Sender<bool>,
    writer: Arc<ChildWriter>,
    /// Taken by the reader thread when IPC starts.
    source: StdMutex<Option<FrameSource>>,
    /// Request queue of the dispatcher started by `serve()`; set once.
    dispatcher: OnceLock<mpsc::Sender<(u64, Value)>>,
    // Touched by `acquire_identify`, reader-thread grants, and permit drops.
    identify: StdMutex<ChildIdentifyState>,
}

impl Drop for ChildIpc {
    fn drop(&mut self) {
        // Lets the writer thread flush what is queued and exit.
        self.writer.close();
    }
}

impl ChildIpc {
    fn send(&self, message: &ChildMessage, priority: FramePriority) -> Result<(), DiscordError> {
        self.send_encoded(&encode_ipc_line(message)?, priority)
    }

    fn send_encoded(&self, frame: &str, priority: FramePriority) -> Result<(), DiscordError> {
        if frame.len() > self.settings.max_frame {
            return Err(DiscordError::model(format!(
                "IPC frame of {} bytes exceeds the parent's limit of {} bytes",
                frame.len(),
                self.settings.max_frame
            )));
        }
        // The leading newline ends any unterminated `print!` output so the
        // prefix always starts a line on the parent's side.
        let mut bytes = Vec::with_capacity(frame.len() + 2);
        bytes.push(b'\n');
        bytes.extend_from_slice(frame.as_bytes());
        bytes.push(b'\n');
        self.writer.enqueue(bytes, priority)
    }

    /// Answers a request. A response larger than the parent accepts would be
    /// discarded there and surface only as a timeout, so it is replaced by
    /// `null` here, where the reason can be logged.
    fn send_response(&self, id: u64, payload: Value) {
        let result = match encode_ipc_line(&ChildMessage::Response { id, payload }) {
            Ok(frame) if frame.len() <= self.settings.max_frame => {
                self.send_encoded(&frame, FramePriority::Data)
            }
            Ok(frame) => {
                warn!(
                    request_id = id,
                    bytes = frame.len(),
                    limit = self.settings.max_frame,
                    "broadcast response exceeds the IPC frame limit; answering null"
                );
                self.send(
                    &ChildMessage::Response {
                        id,
                        payload: Value::Null,
                    },
                    FramePriority::Data,
                )
            }
            Err(error) => Err(error),
        };
        if let Err(error) = result {
            warn!(request_id = id, "failed to queue IPC response: {error}");
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
            .name("discordrs-shard-ipc-in".to_string())
            .spawn(move || ipc.read_loop(source));
        if let Err(error) = spawned {
            warn!("failed to start the IPC reader thread: {error}; treating as shutdown");
            self.close();
        }
    }

    fn read_loop(self: Arc<Self>, mut source: FrameSource) {
        let mut limiter = LineLimiter::new(self.settings.max_frame);
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
        // stdin closed: the parent finished shutting us down or went away.
        info!("IPC stdin closed; treating as shutdown");
        self.close();
    }

    /// Handles one parent line on the reader thread. Everything here only
    /// enqueues, so the reader keeps seeing shutdown and EOF even when
    /// stdout is backed up.
    fn handle_parent_line(self: &Arc<Self>, event: LineEvent) {
        let line = match event {
            LineEvent::Line(line) => line,
            LineEvent::Oversized(bytes) => {
                warn!(
                    bytes,
                    limit = self.settings.max_frame,
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
            IpcLine::Message(ParentMessage::IdentifyGrant { shard_id, lease }) => {
                self.grant_identify(shard_id, lease);
            }
        }
    }

    /// Admission control happens here, before any task exists: the
    /// dispatcher queue holds at most `max_concurrent_requests` requests,
    /// and anything beyond is answered with `null` at once.
    fn dispatch_request(&self, id: u64, payload: Value) {
        let Some(dispatcher) = self.dispatcher.get() else {
            debug!(request_id = id, "request before serve(); answering null");
            self.send_response(id, Value::Null);
            return;
        };
        match dispatcher.try_send((id, payload)) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full((id, _))) => {
                warn!(
                    request_id = id,
                    "too many concurrent broadcast requests; answering null"
                );
                self.send_response(id, Value::Null);
            }
            Err(mpsc::error::TrySendError::Closed((id, _))) => {
                self.send_response(id, Value::Null);
            }
        }
    }

    fn grant_identify(self: &Arc<Self>, shard_id: u32, lease: u64) {
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
                let ipc = Arc::clone(self);
                let permit = IdentifyPermit::new(move |outcome| {
                    ipc.finish_identify(shard_id, lease, outcome);
                });
                // If the acquirer gave up, the permit comes back and its
                // drop reports the attempt as abandoned.
                if let Err(permit) = waiter.send(permit) {
                    drop(permit);
                }
            }
            None => {
                warn!(
                    shard_id,
                    lease, "identify grant without a pending request; returning it"
                );
                let release = ChildMessage::IdentifyRelease {
                    shard_id,
                    lease,
                    sent: false,
                };
                if let Err(error) = self.send(&release, FramePriority::Control) {
                    warn!(
                        shard_id,
                        "failed to return unexpected identify grant: {error}"
                    );
                }
            }
        }
    }

    fn finish_identify(&self, shard_id: u32, lease: u64, outcome: IdentifyOutcome) {
        // Queue the release before freeing the slot: once the slot is free a
        // new acquire may queue its request, and the parent must see the
        // release first or it would reject the request as a duplicate.
        let release = ChildMessage::IdentifyRelease {
            shard_id,
            lease,
            sent: outcome == IdentifyOutcome::Sent,
        };
        if let Err(error) = self.send(&release, FramePriority::Control) {
            warn!(shard_id, "failed to release IDENTIFY permit: {error}");
        }
        lock_or_recover(&self.identify).slots.remove(&shard_id);
    }

    /// Signals shutdown, fails acquisitions that are still waiting, and
    /// thereby stops the request dispatcher (which cancels running
    /// handlers). Held permits stay valid until finished.
    fn close(&self) {
        self.shutdown_tx.send_replace(true);
        let mut state = lock_or_recover(&self.identify);
        state.closed = true;
        state
            .slots
            .retain(|_, slot| matches!(slot, IdentifySlot::Held));
    }
}

/// Runs request handlers for `serve()`: at most `max_concurrent_requests`
/// at once (tracked in a `JoinSet`), each cancelled after the parent's
/// broadcast timeout, and all of them cancelled on shutdown.
async fn run_request_dispatcher(
    ipc: Arc<ChildIpc>,
    mut requests: mpsc::Receiver<(u64, Value)>,
    handler: RequestHandler,
) {
    let limit = ipc.settings.max_concurrent_requests.max(1);
    let request_timeout = ipc.settings.request_timeout;
    let mut shutdown_rx = ipc.shutdown_tx.subscribe();
    let mut running: JoinSet<(u64, Option<Value>)> = JoinSet::new();
    let mut request_ids: HashMap<tokio::task::Id, u64> = HashMap::new();
    loop {
        tokio::select! {
            biased;
            () = wait_for_shutdown(&mut shutdown_rx) => break,
            Some(joined) = running.join_next_with_id(), if !running.is_empty() => match joined {
                Ok((task_id, (id, Some(response)))) => {
                    request_ids.remove(&task_id);
                    ipc.send_response(id, response);
                }
                Ok((task_id, (id, None))) => {
                    request_ids.remove(&task_id);
                    debug!(request_id = id, "request handler timed out; the parent stopped waiting");
                }
                Err(error) => {
                    if let Some(id) = request_ids.remove(&error.id()) {
                        warn!(request_id = id, "request handler failed: {error}");
                        ipc.send_response(id, Value::Null);
                    }
                }
            },
            request = requests.recv() => {
                let Some((id, payload)) = request else {
                    break;
                };
                if running.len() >= limit {
                    warn!(request_id = id, "too many concurrent broadcast requests; answering null");
                    ipc.send_response(id, Value::Null);
                    continue;
                }
                let handler = Arc::clone(&handler);
                let task = running.spawn(async move {
                    (id, timeout(request_timeout, handler(payload)).await.ok())
                });
                request_ids.insert(task.id(), id);
            }
        }
    }
    // The parent no longer waits for these answers.
    running.shutdown().await;
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
/// The handle is cheap to clone.
///
/// A typical child calls [`Self::serve`], starts one gateway client per
/// assigned shard with
/// `GatewayConnectionConfig::default().identify_gate(child.identify_gate())`,
/// reports [`Self::notify_shard_ready`] from each READY, and awaits
/// [`Self::shutdown_signal`].
///
/// # stdin and stdout
///
/// IPC frames share stdout with any other output. Stray prints do not break
/// the protocol (the parent logs them), but logs belong on stderr, which the
/// parent passes through unchanged. **Never read stdin** in child mode: the
/// IPC reader owns it. Frames are written by a background thread, so a
/// process that exits right after [`Self::log`] can lose that line.
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
        let writer = Arc::new(ChildWriter::default());
        let ipc = Arc::new_cyclic(|weak: &Weak<ChildIpc>| {
            let thread_writer = Arc::clone(&writer);
            let thread_ipc = Weak::clone(weak);
            let spawned = std::thread::Builder::new()
                .name("discordrs-shard-ipc-out".to_string())
                .spawn(move || run_child_writer(thread_writer, sink, thread_ipc));
            if let Err(error) = spawned {
                writer.fail(format!("failed to start the IPC writer thread: {error}"));
            }
            ChildIpc {
                settings,
                shutdown_tx,
                writer,
                source: StdMutex::new(Some(source)),
                dispatcher: OnceLock::new(),
                identify: StdMutex::new(ChildIdentifyState::default()),
            }
        });
        if ipc.writer.has_failed() {
            ipc.close();
        }
        Self { ipc }
    }

    /// Shard ids assigned to this process.
    pub fn shard_ids(&self) -> &[u32] {
        &self.ipc.settings.shard_ids
    }

    /// Total shard count across all processes.
    pub fn shard_count(&self) -> u32 {
        self.ipc.settings.shard_count
    }

    /// Reports every assigned shard as ready. Prefer
    /// [`Self::notify_shard_ready`] from each shard's READY handler for an
    /// accurate status.
    pub fn notify_ready(&self) -> Result<(), DiscordError> {
        for &shard_id in &self.ipc.settings.shard_ids {
            self.ipc
                .send(&ChildMessage::Ready { shard_id }, FramePriority::Data)?;
        }
        Ok(())
    }

    /// Reports one assigned shard as ready. Fails for a shard that is not
    /// assigned to this process. Status only: READY does not pace IDENTIFY.
    pub fn notify_shard_ready(&self, shard_id: u32) -> Result<(), DiscordError> {
        self.ensure_assigned(shard_id)?;
        self.ipc
            .send(&ChildMessage::Ready { shard_id }, FramePriority::Data)
    }

    /// Sends a `log` line to the parent, which surfaces it via `tracing`.
    /// Fails if the encoded line exceeds the parent's frame limit or the
    /// outgoing queue is full.
    pub fn log(&self, message: impl Into<String>) -> Result<(), DiscordError> {
        self.ipc.send(
            &ChildMessage::Log {
                message: message.into(),
            },
            FramePriority::Data,
        )
    }

    /// Starts serving broadcast requests with `handler` and starts the IPC
    /// reader if it is not running yet.
    ///
    /// At most [`DEFAULT_MAX_CONCURRENT_REQUESTS`] handlers run at once;
    /// further requests are answered with `null` without spawning anything.
    /// Each handler is cancelled when the parent's broadcast timeout passes
    /// (its answer could no longer be used), and all running handlers are
    /// cancelled when the parent requests shutdown. Requests that arrive
    /// before `serve` are answered with `null`, so call it early.
    ///
    /// Fails outside a Tokio runtime or when called a second time.
    pub fn serve<F, Fut>(&self, handler: F) -> Result<(), DiscordError>
    where
        F: Fn(Value) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Value> + Send + 'static,
    {
        let runtime = tokio::runtime::Handle::try_current().map_err(|_| {
            DiscordError::model("ShardChildProcess::serve must be called inside a Tokio runtime")
        })?;
        let (requests_tx, requests_rx) =
            mpsc::channel(self.ipc.settings.max_concurrent_requests.max(1));
        self.ipc
            .dispatcher
            .set(requests_tx)
            .map_err(|_| DiscordError::model("ShardChildProcess::serve was already called"))?;
        let handler: RequestHandler =
            Arc::new(move |payload| -> RequestFuture { Box::pin(handler(payload)) });
        runtime.spawn(run_request_dispatcher(
            Arc::clone(&self.ipc),
            requests_rx,
            handler,
        ));
        self.ipc.start_reader();
        Ok(())
    }

    /// IDENTIFY gate for this child's gateway clients:
    /// `GatewayConnectionConfig::default().identify_gate(child.identify_gate())`.
    ///
    /// The gateway client then asks the parent for a permit before every
    /// connection that will IDENTIFY and reports the outcome itself, so no
    /// application code handles permits. The client must be configured with
    /// `.shard(id, count)` for one of this process's shards.
    pub fn identify_gate(&self) -> Arc<dyn IdentifyGate> {
        Arc::new(self.clone())
    }

    /// Waits for the parent to grant `shard_id` one IDENTIFY attempt. The
    /// gateway hook ([`Self::identify_gate`]) calls this; use it directly
    /// only for a custom gateway implementation.
    ///
    /// The parent grants at most one permit per bucket
    /// (`shard_id % max_concurrency`) at a time, across every child process,
    /// and keeps the bucket until the permit is finished. Call
    /// `permit.finish(IdentifyOutcome::Sent)` right after writing IDENTIFY.
    /// Dropping the permit reports `Abandoned` (never sent), which frees the
    /// bucket without the interval. A permit held past
    /// `identify_permit_timeout` gets this process killed.
    ///
    /// Cancelling this future is safe: a grant that arrives later is
    /// returned as abandoned, and a new call for the same shard takes over
    /// the request that is already queued.
    ///
    /// # Errors
    ///
    /// Fails if `shard_id` is not assigned to this process, if a permit for
    /// it is already held or being acquired, if the request cannot be
    /// queued, or if shutdown is requested (or the IPC session ends) before
    /// the grant arrives.
    pub async fn acquire_identify(&self, shard_id: u32) -> Result<IdentifyPermit, DiscordError> {
        self.ensure_assigned(shard_id)?;
        let (waiter, permit_rx) = oneshot::channel();
        let send_request = {
            let mut state = lock_or_recover(&self.ipc.identify);
            if state.closed {
                return Err(DiscordError::gateway(
                    "the IPC session has ended (shutdown requested, stdin closed, or stdout \
                     broken); no IDENTIFY permits will be granted",
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
            let request = ChildMessage::IdentifyRequest { shard_id };
            if let Err(error) = self.ipc.send(&request, FramePriority::Control) {
                lock_or_recover(&self.ipc.identify).slots.remove(&shard_id);
                return Err(error);
            }
        }
        permit_rx.await.map_err(|_| {
            DiscordError::gateway(format!(
                "IPC session ended before the IDENTIFY permit for shard {shard_id} was granted"
            ))
        })
    }

    /// Future that resolves once the parent has requested shutdown (via the
    /// `shutdown` op or by closing stdin) or the IPC session failed. Starts
    /// the IPC reader if needed. Can be called multiple times.
    ///
    /// Only a child that awaits this (or exits some other way) stops
    /// gracefully. The parent kills children that are still running when
    /// its shutdown deadline passes, but cannot do so if the parent itself
    /// died.
    pub fn shutdown_signal(&self) -> impl Future<Output = ()> + Send + 'static {
        self.ipc.start_reader();
        let mut shutdown_rx = self.ipc.shutdown_tx.subscribe();
        async move {
            // An error means the sender is gone: nothing more will arrive,
            // so unblock the caller.
            shutdown_rx.wait_for(|requested| *requested).await.ok();
        }
    }

    fn ensure_assigned(&self, shard_id: u32) -> Result<(), DiscordError> {
        if self.ipc.settings.shard_ids.contains(&shard_id) {
            Ok(())
        } else {
            Err(DiscordError::model(format!(
                "shard {shard_id} is not assigned to this child process (assigned: {:?})",
                self.ipc.settings.shard_ids
            )))
        }
    }
}

impl IdentifyGate for ShardChildProcess {
    fn acquire(&self, shard_id: u32, shard_count: u32) -> IdentifyPermitFuture<'_> {
        Box::pin(async move {
            // A mismatch usually means the gateway client was built without
            // `.shard(id, count)`; identifying with the wrong shard pair
            // would be rejected by Discord anyway.
            if shard_count != self.shard_count() {
                return Err(DiscordError::model(format!(
                    "gateway client identifies as shard {shard_id}/{shard_count}, but this \
                     child process runs shards of {}; configure the client with \
                     .shard(id, {})",
                    self.shard_count(),
                    self.shard_count()
                )));
            }
            self.acquire_identify(shard_id).await
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::GatewayBot;
    use serde_json::json;
    use std::sync::atomic::AtomicUsize;
    use tokio::time::sleep;

    // -- protocol -----------------------------------------------------------

    #[test]
    fn parent_messages_round_trip_with_expected_wire_format() {
        assert_eq!(IPC_FRAME_PREFIX, "@discordrs-ipc/1 ");
        let cases = [
            (ParentMessage::Shutdown, r#"{"op":"shutdown"}"#),
            (
                ParentMessage::Request {
                    id: 7,
                    payload: json!({"kind": "stats"}),
                },
                r#"{"op":"request","id":7,"payload":{"kind":"stats"}}"#,
            ),
            (
                ParentMessage::IdentifyGrant {
                    shard_id: 3,
                    lease: 9,
                },
                r#"{"op":"identify_grant","shard_id":3,"lease":9}"#,
            ),
        ];
        for (message, json) in cases {
            let encoded = encode_ipc_line(&message).unwrap();
            assert_eq!(encoded, format!("{IPC_FRAME_PREFIX}{json}"));
            assert_eq!(
                decode_ipc_line::<ParentMessage>(&format!("  {encoded}\n")).unwrap(),
                message
            );
        }
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
                ChildMessage::IdentifyRelease {
                    shard_id: 4,
                    lease: 2,
                    sent: false,
                },
                r#"{"op":"identify_release","shard_id":4,"lease":2,"sent":false}"#,
            ),
        ];
        for (message, json) in cases {
            let encoded = encode_ipc_line(&message).unwrap();
            assert_eq!(encoded, format!("{IPC_FRAME_PREFIX}{json}"));
            assert_eq!(decode_ipc_line::<ChildMessage>(&encoded).unwrap(), message);
        }
        // A release without `sent` is assumed to have identified.
        assert_eq!(
            decode_ipc_line::<ChildMessage>(&format!(
                r#"{IPC_FRAME_PREFIX}{{"op":"identify_release","shard_id":1,"lease":5}}"#
            ))
            .unwrap(),
            ChildMessage::IdentifyRelease {
                shard_id: 1,
                lease: 5,
                sent: true
            }
        );
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

    // -- backoff, settings -----------------------------------------------------

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
                request_timeout: DEFAULT_REQUEST_TIMEOUT,
                max_concurrent_requests: DEFAULT_MAX_CONCURRENT_REQUESTS,
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
        assert_eq!(
            with((ENV_SHARD_IPC_REQUEST_TIMEOUT_MS, "1500")).map(|s| s.request_timeout),
            Some(Duration::from_millis(1500))
        );
        assert_eq!(
            with((ENV_SHARD_IPC_REQUEST_TIMEOUT_MS, "soon")).map(|s| s.request_timeout),
            Some(DEFAULT_REQUEST_TIMEOUT)
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

    // -- monitor completion -----------------------------------------------------

    #[test]
    fn monitor_completion_records_failures_cancellations_and_panics() {
        let (tx, rx) = watch::channel(CompletionState {
            remaining: 4,
            failures: Vec::new(),
        });
        let tx = Arc::new(tx);
        let guard = |label: &str| MonitorCompletion {
            state: Arc::clone(&tx),
            label: label.to_string(),
            result: None,
        };

        let mut ok = guard("ok");
        ok.result = Some(Ok(()));
        drop(ok);
        let mut failed = guard("failed");
        failed.result = Some(Err("boom".to_string()));
        drop(failed);
        drop(guard("cancelled"));
        let panicking = guard("panicking");
        let joined = std::thread::spawn(move || {
            let _guard = panicking;
            panic!("monitor bug");
        })
        .join();
        assert!(joined.is_err());

        let state = rx.borrow().clone();
        assert_eq!(state.remaining, 0);
        assert_eq!(state.failures.len(), 3, "{:?}", state.failures);
        let error = completion_result(&state).unwrap_err().to_string();
        assert!(error.contains("failed: boom"), "{error}");
        assert!(
            error.contains("cancelled: monitor was cancelled"),
            "{error}"
        );
        assert!(error.contains("panicking: monitor panicked"), "{error}");
        assert!(completion_result(&CompletionState::default()).is_ok());
    }

    // -- IDENTIFY scheduling and session budget --------------------------------

    fn session_limit(remaining: u32, total: u32, reset_after: u64) -> SessionStartLimit {
        SessionStartLimit {
            total,
            remaining,
            reset_after,
            max_concurrency: 1,
        }
    }

    #[test]
    fn identify_scheduler_buckets_follow_shard_modulo_max_concurrency() {
        let scheduler = IdentifyScheduler::new(2, 10, Duration::ZERO, None);
        assert_eq!(scheduler.buckets.len(), 2);
        assert_eq!(scheduler.bucket_of(7), 1);
        assert_eq!(scheduler.bucket_of(8), 0);

        // More concurrency than shards: one bucket per shard.
        let scheduler = IdentifyScheduler::new(16, 4, Duration::ZERO, None);
        assert_eq!(scheduler.buckets.len(), 4);
        assert_eq!(scheduler.bucket_of(3), 3);
        assert!(scheduler.bucket_of(100) < 4);

        let scheduler = IdentifyScheduler::new(0, 0, Duration::ZERO, None);
        assert_eq!(scheduler.buckets.len(), 1);
        assert_eq!(scheduler.bucket_of(5), 0);
    }

    #[tokio::test]
    async fn identify_scheduler_holds_a_bucket_until_the_attempt_ends() {
        let interval = Duration::from_millis(150);
        let scheduler = IdentifyScheduler::new(2, 4, interval, None);
        let first = scheduler.acquire(0).await;

        // A different bucket does not wait for bucket 0.
        let other = timeout(Duration::from_secs(1), scheduler.acquire(1))
            .await
            .expect("bucket 1 must not wait for bucket 0");

        // The same bucket waits for as long as the attempt is in progress...
        let mut same = Box::pin(scheduler.acquire(2));
        assert!(timeout(Duration::from_millis(100), &mut same)
            .await
            .is_err());
        let sent_at = Instant::now();
        first.finish(IdentifyOutcome::Sent);
        // ...and then for the interval measured from the reported send.
        let second = timeout(Duration::from_secs(2), &mut same)
            .await
            .expect("granted after the send");
        assert!(sent_at.elapsed() >= interval);
        drop(other);

        // A lease dropped without an outcome (child gone) counts as sent.
        let dropped_at = Instant::now();
        drop(second);
        let _third = scheduler.acquire(0).await;
        assert!(dropped_at.elapsed() >= interval);
    }

    #[tokio::test]
    async fn abandoned_attempts_free_the_bucket_at_once_and_refund_the_budget() {
        // One session start left, and its reset is an hour away.
        let budget = Arc::new(SessionBudget::new(
            BudgetState::from_limit(&session_limit(1, 1000, 3_600_000)),
            None,
        ));
        let scheduler = IdentifyScheduler::new(1, 1, Duration::from_secs(60), Some(budget));
        let lease = scheduler.acquire(0).await;
        lease.finish(IdentifyOutcome::Abandoned);
        // Neither the 60s interval nor the exhausted budget applies: the
        // attempt never reached Discord.
        let lease = timeout(Duration::from_secs(1), scheduler.acquire(0))
            .await
            .expect("an abandoned attempt must not hold the bucket or consume the budget");
        lease.finish(IdentifyOutcome::Abandoned);
    }

    #[tokio::test]
    async fn identify_scheduler_queues_fifo_and_skips_cancelled_waiters() {
        let scheduler = IdentifyScheduler::new(1, 3, Duration::ZERO, None);
        let held = scheduler.acquire(0).await;
        let mut cancelled = Box::pin(scheduler.acquire(1));
        let mut second = Box::pin(scheduler.acquire(2));
        let mut third = Box::pin(scheduler.acquire(0));
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

    #[test]
    fn budget_state_from_limit_keeps_a_usable_total() {
        let state = BudgetState::from_limit(&session_limit(5, 0, 0));
        assert_eq!((state.available, state.total, state.outstanding), (5, 5, 0));
        let state = BudgetState::from_limit(&session_limit(0, 0, 0));
        assert_eq!((state.available, state.total), (0, 1));
    }

    fn budget_state(available: i64, total: u32, reset_in: Duration) -> BudgetState {
        BudgetState {
            available,
            total,
            outstanding: 0,
            reset_at: Instant::now() + reset_in,
            refresh_failures: 0,
            refreshing: false,
            sent_during_refresh: 0,
        }
    }

    fn budget_counts(budget: &SessionBudget) -> (i64, u32) {
        let state = lock_or_recover(&budget.state);
        (state.available, state.outstanding)
    }

    fn gateway_bot_body(remaining: u32) -> &'static str {
        match remaining {
            0 => {
                r#"{"url":"wss://gateway.discord.gg","shards":2,"session_start_limit":{"total":1000,"remaining":0,"reset_after":3600000,"max_concurrency":2}}"#
            }
            1 => {
                r#"{"url":"wss://gateway.discord.gg","shards":2,"session_start_limit":{"total":1000,"remaining":1,"reset_after":3600000,"max_concurrency":2}}"#
            }
            _ => {
                r#"{"url":"wss://gateway.discord.gg","shards":2,"session_start_limit":{"total":1000,"remaining":3,"reset_after":3600000,"max_concurrency":2}}"#
            }
        }
    }

    #[tokio::test]
    async fn fixed_budget_waits_for_the_reset_and_keeps_outstanding_reservations() {
        let budget = Arc::new(SessionBudget::new(
            BudgetState::from_limit(&session_limit(1, 3, 200)),
            None,
        ));
        let start = Instant::now();
        let first = budget.reserve().await;
        assert!(start.elapsed() < Duration::from_millis(150));
        // Exhausted: wait for the reset, then (nothing to refresh from)
        // assume the daily total is back, minus the outstanding reservation.
        let second = budget.reserve().await;
        assert!(start.elapsed() >= Duration::from_millis(200));
        assert_eq!(budget_counts(&budget), (1, 2));
        first.settle(IdentifyOutcome::Abandoned);
        assert_eq!(budget_counts(&budget), (2, 1));
        drop(second);
        assert_eq!(budget_counts(&budget), (2, 0));
    }

    /// Regression: a refund did not wake a reserver that was already waiting
    /// for the reset, so a freed start could sit unused for hours.
    #[tokio::test]
    async fn refund_wakes_a_reserver_that_is_already_waiting() {
        let budget = Arc::new(SessionBudget::new(
            BudgetState::from_limit(&session_limit(1, 1000, 3_600_000)),
            None,
        ));
        let first = budget.reserve().await;
        let mut second = Box::pin(budget.reserve());
        assert!(timeout(Duration::from_millis(100), &mut second)
            .await
            .is_err());
        first.settle(IdentifyOutcome::Abandoned);
        let second = timeout(Duration::from_secs(1), &mut second)
            .await
            .expect("the refund must wake the waiting reserver");
        second.settle(IdentifyOutcome::Sent);
        assert_eq!(budget_counts(&budget), (0, 0));
    }

    /// Regression: a refresh replaced the counter with Discord's remaining
    /// allowance and forgot reservations still outstanding in another
    /// bucket. Shard 0 (bucket 0) holds an unsent grant across the reset;
    /// shard 1 (bucket 1) waits for the budget and triggers the refresh.
    #[tokio::test]
    async fn outstanding_reservations_survive_a_refresh_across_buckets() {
        let (port, server) = gateway_bot_server(vec![
            ("200 OK", gateway_bot_body(1)),
            ("200 OK", gateway_bot_body(1)),
            ("200 OK", gateway_bot_body(0)),
        ])
        .await;
        let setup = || {
            let budget = Arc::new(SessionBudget::new(
                budget_state(1, 1000, Duration::from_millis(150)),
                Some(rest_for(port)),
            ));
            let scheduler = IdentifyScheduler::new(2, 2, Duration::ZERO, Some(Arc::clone(&budget)));
            (budget, scheduler)
        };

        // Discord says one start is left, but the outstanding grant may
        // still use it; once it does, the other bucket must keep waiting.
        let (budget, scheduler) = setup();
        let first = scheduler.acquire(0).await;
        let mut second = Box::pin(scheduler.acquire(1));
        assert!(
            timeout(Duration::from_millis(400), &mut second)
                .await
                .is_err(),
            "the refresh forgot the outstanding reservation"
        );
        assert_eq!(budget_counts(&budget), (0, 1));
        first.finish(IdentifyOutcome::Sent);
        assert!(timeout(Duration::from_millis(200), &mut second)
            .await
            .is_err());
        drop(second);

        // Same, but the outstanding attempt is abandoned: its refund is
        // valid against the refreshed count.
        let (budget, scheduler) = setup();
        let first = scheduler.acquire(0).await;
        let mut second = Box::pin(scheduler.acquire(1));
        assert!(timeout(Duration::from_millis(400), &mut second)
            .await
            .is_err());
        first.finish(IdentifyOutcome::Abandoned);
        let second = timeout(Duration::from_secs(1), &mut second)
            .await
            .expect("the refunded start must be granted");
        drop(second);
        assert_eq!(budget_counts(&budget), (0, 0));

        // Discord reports nothing left: the refresh records a debt, and the
        // abandoned attempt's refund only pays it back.
        let (budget, scheduler) = setup();
        let first = scheduler.acquire(0).await;
        let mut second = Box::pin(scheduler.acquire(1));
        assert!(timeout(Duration::from_millis(400), &mut second)
            .await
            .is_err());
        assert_eq!(budget_counts(&budget), (-1, 1));
        first.finish(IdentifyOutcome::Abandoned);
        assert!(
            timeout(Duration::from_millis(200), &mut second)
                .await
                .is_err(),
            "a refund must not create a start Discord does not have"
        );
        assert_eq!(budget_counts(&budget), (0, 0));

        assert_eq!(server.await.unwrap().len(), 3);
    }

    /// Regression: a reservation that settled `Sent` while the refresh
    /// request was in flight left `outstanding` before reconciliation, so
    /// Discord's `remaining`, computed before that IDENTIFY, was installed in
    /// full and over-allocated. The mock holds its response until the
    /// outstanding grant has settled.
    #[tokio::test]
    async fn sent_settlement_during_a_refresh_stays_charged() {
        use tokio::io::AsyncReadExt as _;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (received_tx, received_rx) = oneshot::channel::<()>();
        let (release_tx, release_rx) = oneshot::channel::<()>();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buffer = vec![0_u8; 4096];
            let _ = stream.read(&mut buffer).await.unwrap();
            let _ = received_tx.send(());
            let _ = release_rx.await;
            let body = gateway_bot_body(1);
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
                 Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await.unwrap();
        });
        let budget = Arc::new(SessionBudget::new(
            budget_state(1, 1000, Duration::from_millis(100)),
            Some(rest_for(port)),
        ));
        let scheduler = IdentifyScheduler::new(2, 2, Duration::ZERO, Some(Arc::clone(&budget)));
        let first = scheduler.acquire(0).await;
        let mut second = Box::pin(scheduler.acquire(1));

        // Drive the second acquisition until its refresh reaches Discord.
        tokio::select! {
            _ = &mut second => panic!("granted before the refresh"),
            received = received_rx => received.unwrap(),
        }
        // The outstanding grant IDENTIFYs while the response is in flight,
        // and the response (`remaining: 1`) predates that IDENTIFY.
        first.finish(IdentifyOutcome::Sent);
        release_tx.send(()).unwrap();
        assert!(
            timeout(Duration::from_millis(400), &mut second)
                .await
                .is_err(),
            "the refresh over-allocated a start already used"
        );
        assert_eq!(budget_counts(&budget), (0, 0));
        drop(second);
        server.await.unwrap();
    }

    /// Regression: a failed refresh assumed a full reset, granting starts
    /// Discord might refuse. It now stays closed until a refresh succeeds.
    #[tokio::test]
    async fn failed_refresh_keeps_the_budget_closed_until_a_refresh_succeeds() {
        let unauthorized = r#"{"message":"401: Unauthorized","code":0}"#;
        let (port, server) = gateway_bot_server(vec![
            ("401 Unauthorized", unauthorized),
            ("200 OK", gateway_bot_body(3)),
        ])
        .await;
        let mut budget = SessionBudget::new(
            budget_state(0, 5, Duration::from_millis(100)),
            Some(rest_for(port)),
        );
        budget.retry_delay = Duration::from_millis(300);
        let budget = Arc::new(budget);

        let started = Instant::now();
        let reservation = timeout(Duration::from_secs(5), budget.reserve())
            .await
            .expect("the retried refresh must open the budget");
        assert!(
            started.elapsed() >= Duration::from_millis(400),
            "granted after a failed refresh"
        );
        assert_eq!(budget_counts(&budget), (2, 1));
        assert_eq!(lock_or_recover(&budget.state).refresh_failures, 0);
        drop(reservation);
        assert_eq!(server.await.unwrap().len(), 2);
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

    /// Stand-in for a stdout pipe that can be "full": writes block until
    /// the valve opens.
    struct Valve {
        open: StdMutex<bool>,
        changed: Condvar,
    }

    impl Valve {
        fn closed() -> Arc<Self> {
            Arc::new(Self {
                open: StdMutex::new(false),
                changed: Condvar::new(),
            })
        }

        fn open(&self) {
            *lock_or_recover(&self.open) = true;
            self.changed.notify_all();
        }

        fn wait_open(&self) {
            let mut open = lock_or_recover(&self.open);
            while !*open {
                open = self
                    .changed
                    .wait(open)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
            }
        }
    }

    fn child_settings(shard_ids: &[u32]) -> ChildSettings {
        ChildSettings {
            shard_ids: shard_ids.to_vec(),
            shard_count: 8,
            max_frame: MIN_IPC_FRAME_BYTES,
            request_timeout: Duration::from_secs(60),
            max_concurrent_requests: DEFAULT_MAX_CONCURRENT_REQUESTS,
        }
    }

    struct ChildHarness {
        child: ShardChildProcess,
        to_child: Option<std::sync::mpsc::Sender<Vec<u8>>>,
        from_child: mpsc::UnboundedReceiver<Vec<u8>>,
    }

    fn child_harness_with(settings: ChildSettings, valve: Option<Arc<Valve>>) -> ChildHarness {
        let (to_child, rx) = std::sync::mpsc::channel();
        let (frame_tx, from_child) = mpsc::unbounded_channel();
        let child = ShardChildProcess::with_io(
            settings,
            Box::new(std::io::BufReader::new(ChannelSource::new(rx))),
            Box::new(move |bytes: &[u8]| {
                if let Some(valve) = &valve {
                    valve.wait_open();
                }
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

    fn child_harness(shard_ids: &[u32]) -> ChildHarness {
        child_harness_with(child_settings(shard_ids), None)
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

        fn grant(&self, shard_id: u32, lease: u64) {
            self.send(&ParentMessage::IdentifyGrant { shard_id, lease });
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

    fn release(shard_id: u32, lease: u64, sent: bool) -> ChildMessage {
        ChildMessage::IdentifyRelease {
            shard_id,
            lease,
            sent,
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
        let error = harness
            .child
            .serve(|payload| async move { payload })
            .unwrap_err();
        assert!(error.to_string().contains("already called"), "{error}");
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
        harness.grant(1, 11);
        let permit = timeout(Duration::from_secs(5), acquire)
            .await
            .unwrap()
            .unwrap()
            .unwrap();

        let error = harness.child.acquire_identify(1).await.unwrap_err();
        assert!(error.to_string().contains("already held"), "{error}");

        permit.finish(IdentifyOutcome::Sent);
        assert_eq!(harness.next_message().await, release(1, 11, true));

        // The slot is free again; this time the attempt is abandoned.
        let child = harness.child.clone();
        let again = tokio::spawn(async move { child.acquire_identify(1).await });
        assert_eq!(
            harness.next_message().await,
            ChildMessage::IdentifyRequest { shard_id: 1 }
        );
        harness.grant(1, 12);
        drop(again.await.unwrap().unwrap());
        assert_eq!(harness.next_message().await, release(1, 12, false));
    }

    #[tokio::test]
    async fn child_identify_gate_checks_the_shard_count() {
        let mut harness = child_harness(&[0]);
        let gate = harness.child.identify_gate();
        let error = gate.acquire(0, 1).await.unwrap_err();
        assert!(error.to_string().contains(".shard(id, 8)"), "{error}");

        let acquire = tokio::spawn(async move { gate.acquire(0, 8).await });
        assert_eq!(
            harness.next_message().await,
            ChildMessage::IdentifyRequest { shard_id: 0 }
        );
        harness.grant(0, 1);
        let mut permit = acquire.await.unwrap().unwrap();
        // A cancelled IDENTIFY write still counts as sent.
        permit.mark_sending();
        drop(permit);
        assert_eq!(harness.next_message().await, release(0, 1, true));
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

        harness.grant(0, 3);
        first.await.unwrap().unwrap().finish(IdentifyOutcome::Sent);
        assert_eq!(harness.next_message().await, release(0, 3, true));
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
        harness.grant(0, 4);
        assert_eq!(harness.next_message().await, release(0, 4, false));

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
        harness.grant(1, 5);
        let permit = timeout(Duration::from_secs(5), retry)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        permit.finish(IdentifyOutcome::Sent);
        assert_eq!(harness.next_message().await, release(1, 5, true));
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
        assert!(error.to_string().contains("IPC session ended"), "{error}");
        let error = harness.child.acquire_identify(0).await.unwrap_err();
        assert!(
            error.to_string().contains("IPC session has ended"),
            "{error}"
        );

        // A grant that still arrives is returned unused.
        harness.grant(0, 6);
        assert_eq!(harness.next_message().await, release(0, 6, false));
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
    async fn stdout_write_failure_ends_the_ipc_session() {
        let (_to_child, rx) = std::sync::mpsc::channel::<Vec<u8>>();
        let child = ShardChildProcess::with_io(
            child_settings(&[0]),
            Box::new(std::io::BufReader::new(ChannelSource::new(rx))),
            Box::new(|_: &[u8]| Err(io::Error::from(io::ErrorKind::BrokenPipe))),
        );
        // Queued; the writer thread then fails and ends the session.
        child.notify_ready().unwrap();
        timeout(Duration::from_secs(5), child.shutdown_signal())
            .await
            .expect("a broken stdout must end the IPC session");
        let error = child.log("after").unwrap_err();
        assert!(
            error.to_string().contains("IPC stdout write failed"),
            "{error}"
        );
        let error = child.acquire_identify(0).await.unwrap_err();
        assert!(
            error.to_string().contains("IPC session has ended"),
            "{error}"
        );
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

    /// Regression: child IPC writes used to lock stdout and write inline from
    /// async tasks, the reader thread, and `Drop`, so a parent that stopped
    /// reading blocked Tokio workers and kept the reader from seeing
    /// shutdown. Runs on the default current-thread runtime, where one
    /// blocking write would freeze everything.
    #[tokio::test]
    async fn full_stdout_pipe_blocks_neither_the_runtime_nor_the_reader() {
        let valve = Valve::closed();
        let mut harness = child_harness_with(child_settings(&[0, 1]), Some(Arc::clone(&valve)));
        harness
            .child
            .serve(|payload| async move { payload })
            .unwrap();

        // The writer thread takes this frame and blocks in the "full pipe".
        harness.child.log("first").unwrap();
        let writer = Arc::clone(&harness.child.ipc.writer);
        let taken = timeout(Duration::from_secs(5), async {
            while !lock_or_recover(&writer.queues).data.is_empty() {
                sleep(Duration::from_millis(5)).await;
            }
        })
        .await;
        assert!(taken.is_ok(), "writer thread never took the first frame");

        // Data frames are admitted up to the bound, then rejected; nothing
        // blocks.
        let accepted = (0..CHILD_DATA_QUEUE_CAPACITY + 10)
            .take_while(|index| harness.child.log(format!("log {index}")).is_ok())
            .count();
        assert_eq!(accepted, CHILD_DATA_QUEUE_CAPACITY);

        // Control frames are still admitted: one acquisition that will be
        // granted, one that is cancelled before its grant.
        let child = harness.child.clone();
        let acquire = tokio::spawn(async move { child.acquire_identify(0).await });
        sleep(Duration::from_millis(20)).await;
        assert!(
            timeout(Duration::from_millis(50), harness.child.acquire_identify(1))
                .await
                .is_err()
        );
        // A request whose response cannot be queued must not wedge anything.
        harness.send(&ParentMessage::Request {
            id: 5,
            payload: json!("x"),
        });
        // The reader still handles grants and returns the cancelled one; the
        // permit's drop only enqueues.
        harness.grant(1, 7);
        harness.grant(0, 8);
        let permit = timeout(Duration::from_secs(5), acquire)
            .await
            .expect("grant must arrive while stdout is stuck")
            .unwrap()
            .unwrap();
        let dropped = std::time::Instant::now();
        drop(permit);
        assert!(dropped.elapsed() < Duration::from_millis(100));
        // Timers keep firing on this single-threaded runtime...
        timeout(Duration::from_secs(1), sleep(Duration::from_millis(20)))
            .await
            .expect("runtime is blocked");
        // ...and the reader still sees shutdown.
        harness.send(&ParentMessage::Shutdown);
        timeout(Duration::from_secs(5), harness.child.shutdown_signal())
            .await
            .expect("reader blocked behind stdout");

        // Once the pipe drains: the frame in flight, then control frames
        // ahead of the queued data.
        valve.open();
        assert_eq!(
            harness.next_message().await,
            ChildMessage::Log {
                message: "first".to_string()
            }
        );
        assert_eq!(
            harness.next_message().await,
            ChildMessage::IdentifyRequest { shard_id: 0 }
        );
        assert_eq!(
            harness.next_message().await,
            ChildMessage::IdentifyRequest { shard_id: 1 }
        );
        assert_eq!(harness.next_message().await, release(1, 7, false));
        assert_eq!(harness.next_message().await, release(0, 8, false));
        assert_eq!(
            harness.next_message().await,
            ChildMessage::Log {
                message: "log 0".to_string()
            }
        );
    }

    /// Counts how many handler futures were dropped before finishing.
    struct DropCounter(Arc<AtomicUsize>);

    impl Drop for DropCounter {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn hanging_handler(
        started: &Arc<AtomicUsize>,
        cancelled: &Arc<AtomicUsize>,
    ) -> impl Fn(Value) -> Pin<Box<dyn Future<Output = Value> + Send>> + Send + Sync + 'static {
        let started = Arc::clone(started);
        let cancelled = Arc::clone(cancelled);
        move |payload| {
            started.fetch_add(1, Ordering::SeqCst);
            let cancelled = Arc::clone(&cancelled);
            Box::pin(async move {
                if payload == json!("hang") {
                    let _counter = DropCounter(cancelled);
                    std::future::pending::<()>().await;
                }
                payload
            })
        }
    }

    async fn wait_for_count(counter: &AtomicUsize, wanted: usize) -> bool {
        timeout(Duration::from_secs(5), async {
            while counter.load(Ordering::SeqCst) < wanted {
                sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .is_ok()
    }

    /// Regression: every request used to spawn a detached task, with no
    /// limit and no cancellation when the parent stopped waiting.
    #[tokio::test]
    async fn request_handlers_are_admitted_before_spawning_and_time_out() {
        let mut settings = child_settings(&[0]);
        settings.max_concurrent_requests = 2;
        settings.request_timeout = Duration::from_millis(200);
        let mut harness = child_harness_with(settings, None);
        let started = Arc::new(AtomicUsize::new(0));
        let cancelled = Arc::new(AtomicUsize::new(0));
        harness
            .child
            .serve(hanging_handler(&started, &cancelled))
            .unwrap();

        for id in 1..=2 {
            harness.send(&ParentMessage::Request {
                id,
                payload: json!("hang"),
            });
        }
        assert!(wait_for_count(&started, 2).await);
        // Beyond the limit: answered null without running the handler.
        for id in 3..=4 {
            harness.send(&ParentMessage::Request {
                id,
                payload: json!("hang"),
            });
            assert_eq!(
                harness.next_message().await,
                ChildMessage::Response {
                    id,
                    payload: Value::Null
                }
            );
        }
        assert_eq!(started.load(Ordering::SeqCst), 2);

        // The parent's timeout cancels the running handlers without an
        // answer, which frees their slots.
        assert!(wait_for_count(&cancelled, 2).await);
        harness.send(&ParentMessage::Request {
            id: 5,
            payload: json!("echo"),
        });
        assert_eq!(
            harness.next_message().await,
            ChildMessage::Response {
                id: 5,
                payload: json!("echo")
            }
        );
    }

    #[tokio::test]
    async fn shutdown_cancels_running_request_handlers() {
        let mut harness = child_harness(&[0]);
        let started = Arc::new(AtomicUsize::new(0));
        let cancelled = Arc::new(AtomicUsize::new(0));
        harness
            .child
            .serve(hanging_handler(&started, &cancelled))
            .unwrap();
        for id in 1..=3 {
            harness.send(&ParentMessage::Request {
                id,
                payload: json!("hang"),
            });
        }
        assert!(wait_for_count(&started, 3).await);
        // The request timeout here is 60s: only shutdown can cancel them.
        harness.send(&ParentMessage::Shutdown);
        assert!(
            wait_for_count(&cancelled, 3).await,
            "handlers kept running after shutdown"
        );
        harness.assert_quiet().await;
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
        // Same path the gateway client takes through the hook.
        let gate = child.identify_gate();
        for &shard_id in child.shard_ids() {
            let permit = gate.acquire(shard_id, child.shard_count()).await.unwrap();
            permit.finish(IdentifyOutcome::Sent);
            child.notify_shard_ready(shard_id).unwrap();
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
            session_start_budget: SessionStartBudget::Unchecked,
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
    async fn spawn_rejects_missing_total_shards_bad_shard_lists_and_unread_budgets() {
        let unchecked = || ProcessShardManagerConfig {
            session_start_budget: SessionStartBudget::Unchecked,
            ..ProcessShardManagerConfig::default()
        };
        let error = ProcessShardManager::spawn(unchecked())
            .await
            .map(|_| ())
            .unwrap_err();
        assert!(error.to_string().contains("total_shards"));

        let config = ProcessShardManagerConfig {
            total_shards: Some(2),
            shard_list: Some(vec![0, 5]),
            ..unchecked()
        };
        let error = ProcessShardManager::spawn(config)
            .await
            .map(|_| ())
            .unwrap_err();
        assert!(error.to_string().contains("out of range"));

        let config = ProcessShardManagerConfig {
            total_shards: Some(2),
            shard_list: Some(Vec::new()),
            ..unchecked()
        };
        let error = ProcessShardManager::spawn(config)
            .await
            .map(|_| ())
            .unwrap_err();
        assert!(error.to_string().contains("must not be empty"));

        // Without a token the budget cannot be read: an explicit choice is
        // required instead of silently skipping budget protection.
        let config = ProcessShardManagerConfig {
            total_shards: Some(1),
            ..ProcessShardManagerConfig::default()
        };
        let error = ProcessShardManager::spawn(config)
            .await
            .map(|_| ())
            .unwrap_err();
        assert!(error.to_string().contains("session start limit"), "{error}");
    }

    #[cfg(unix)]
    fn scripted_child_config(script: &str) -> ProcessShardManagerConfig {
        ProcessShardManagerConfig {
            total_shards: Some(1),
            program: Some(PathBuf::from("/bin/sh")),
            args: vec!["-c".to_string(), script.to_string()],
            session_start_budget: SessionStartBudget::Unchecked,
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
        // Giving up is a terminal outcome the caller must see.
        let error = timeout(Duration::from_secs(5), manager.wait())
            .await
            .expect("manager.wait timed out")
            .unwrap_err()
            .to_string();
        assert!(error.contains("gave up after 2 respawn(s)"), "{error}");
        assert!(error.contains("exited with exit status: 1"), "{error}");

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
        // A failed exit is reported...
        let mut config = scripted_child_config("exit 3");
        config.respawn = false;
        let manager = ProcessShardManager::spawn(config).await.unwrap();
        let error = timeout(Duration::from_secs(5), manager.wait())
            .await
            .expect("manager.wait timed out")
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("exited with exit status: 3; respawn is disabled"),
            "{error}"
        );
        let status = &manager.children()[0];
        assert_eq!(status.state, ChildState::Stopped);
        assert_eq!(status.respawns, 0);

        // ...a successful one is not.
        let mut config = scripted_child_config("exit 0");
        config.respawn = false;
        let manager = ProcessShardManager::spawn(config).await.unwrap();
        timeout(Duration::from_secs(5), manager.wait())
            .await
            .expect("manager.wait timed out")
            .unwrap();
    }

    #[tokio::test]
    async fn spawn_failure_is_reported() {
        let config = ProcessShardManagerConfig {
            total_shards: Some(1),
            program: Some(PathBuf::from("/nonexistent/discordrs-shard-child")),
            respawn: false,
            session_start_budget: SessionStartBudget::Unchecked,
            ..ProcessShardManagerConfig::default()
        };
        let manager = ProcessShardManager::spawn(config).await.unwrap();
        let error = timeout(Duration::from_secs(5), manager.wait())
            .await
            .expect("manager.wait timed out")
            .unwrap_err()
            .to_string();
        assert!(error.contains("could not be spawned"), "{error}");
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
        // Fill the data queue behind the stuck frame.
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

    /// Regression: a kill whose exit could not be confirmed was reported as
    /// a clean stop, and `wait()` always returned `Ok`.
    #[cfg(unix)]
    #[tokio::test]
    async fn unconfirmed_termination_is_reported_to_every_waiter() {
        let mut config = scripted_child_config("exec sleep 30");
        config.shutdown_timeout = Duration::from_millis(100);
        // A zero reap timeout treats every kill as unconfirmed.
        let manager = ProcessShardManager::start(
            config,
            None,
            SpawnInternals {
                kill_reap_timeout: Duration::ZERO,
            },
        )
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
        let error = timeout(Duration::from_secs(5), manager.shutdown())
            .await
            .expect("shutdown timed out")
            .unwrap_err()
            .to_string();
        assert!(error.contains("exit was not confirmed"), "{error}");
        let waited = timeout(Duration::from_secs(1), waiter)
            .await
            .expect("the concurrent waiter must finish too")
            .unwrap_err()
            .to_string();
        assert_eq!(waited, error);
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
echo '@discordrs-ipc/1 {"op":"identify_release","shard_id":0,"lease":9}'
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

    /// Times (ns since the epoch) a scripted child appended to `file`.
    #[cfg(target_os = "linux")]
    fn read_times(dir: &std::path::Path, file: &str) -> Vec<i128> {
        std::fs::read_to_string(dir.join(file))
            .unwrap_or_default()
            .lines()
            .map(|line| line.trim().parse().unwrap())
            .collect()
    }

    /// Child that asks for an IDENTIFY permit for its single shard, records
    /// when each grant arrived (`grant-<shard>`), and keeps the lease id in
    /// `$L`; the handling of the grant follows.
    #[cfg(target_os = "linux")]
    const IDENTIFY_CHILD_PRELUDE: &str = r#"
P='@discordrs-ipc/1 '
S=$DISCORDRS_SHARD_IDS
echo "$P{\"op\":\"identify_request\",\"shard_id\":$S}"
while read -r line; do
  case "$line" in
    *'"op":"shutdown"'*) exit 0 ;;
    *identify_grant*)
      L=$(printf '%s' "$line" | sed 's/.*"lease":\([0-9]*\).*/\1/')
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

    /// "Sends" IDENTIFY (records `sent-<shard>`), releases, and reports ready.
    #[cfg(target_os = "linux")]
    const SEND_RELEASE_AND_READY: &str = r#"
      date +%s%N >> "$SCRATCH_DIR/sent-$S"
      echo "$P{\"op\":\"identify_release\",\"shard_id\":$S,\"lease\":$L,\"sent\":true}"
      echo "$P{\"op\":\"ready\",\"shard_id\":$S}""#;

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn identify_grants_are_spaced_per_bucket_across_processes() {
        let dir = scratch_dir("buckets");
        let mut config = identify_child_config(&dir, SEND_RELEASE_AND_READY);
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
                let times = read_times(&dir, &format!("grant-{shard}"));
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
            SEND_RELEASE_AND_READY,
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

        let grants = read_times(&dir, "grant-0");
        assert_eq!(grants.len(), 2, "grants: {grants:?}");
        // The crashed child never released its permit; the parent returned
        // the bucket when the exit was confirmed and spaced the next grant.
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

    /// Regression: an expired permit used to free the bucket while the
    /// stalled holder could still IDENTIFY, so two shards of one bucket
    /// could IDENTIFY together. Each child's first incarnation stalls for 3s
    /// between its grant and its (simulated) IDENTIFY send.
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn stalled_permit_holder_is_killed_before_its_bucket_is_reused() {
        let dir = scratch_dir("stalled");
        let on_grant = [
            r#"
      if [ ! -e "$SCRATCH_DIR/stalled-$S" ]; then : > "$SCRATCH_DIR/stalled-$S"; sleep 3; fi"#,
            SEND_RELEASE_AND_READY,
        ]
        .concat();
        let mut config = identify_child_config(&dir, &on_grant);
        config.total_shards = Some(2);
        config.identify_interval = Duration::from_millis(100);
        config.identify_permit_timeout = Duration::from_millis(400);
        config.max_respawns = Some(3);
        let manager = ProcessShardManager::spawn(config).await.unwrap();
        assert!(
            wait_until(Duration::from_secs(15), || all_in_state(
                &manager,
                ChildState::Running
            ))
            .await,
            "children never reached Running: {:?}",
            manager.children()
        );
        assert!(manager.children().iter().all(|child| child.respawns == 1));

        // Every stalled holder was killed before its delayed send: each
        // shard sent exactly once, from its respawned process.
        let mut events: Vec<(i128, u32, &str)> = Vec::new();
        for shard in 0..2 {
            let sent = read_times(&dir, &format!("sent-{shard}"));
            assert_eq!(sent.len(), 1, "shard {shard} sends: {sent:?}");
            events.push((sent[0], shard, "sent"));
            let grants = read_times(&dir, &format!("grant-{shard}"));
            assert_eq!(grants.len(), 2, "shard {shard} grants: {grants:?}");
            events.extend(grants.into_iter().map(|time| (time, shard, "grant")));
        }
        events.sort();
        // Both shards share one bucket: every send must come while its own
        // shard holds the most recent grant.
        for (index, &(_, shard, kind)) in events.iter().enumerate() {
            if kind == "sent" {
                let holder = events[..index]
                    .iter()
                    .rev()
                    .find(|(_, _, kind)| *kind == "grant")
                    .map(|&(_, shard, _)| shard);
                assert_eq!(holder, Some(shard), "events: {events:?}");
            }
        }
        // A stalled grant is followed by the next one only after the kill.
        let grants: Vec<i128> = events
            .iter()
            .filter(|(_, _, kind)| *kind == "grant")
            .map(|&(time, _, _)| time)
            .collect();
        assert!(
            grants[1] - grants[0] >= 390_000_000,
            "bucket reused {}ns after a stalled grant",
            grants[1] - grants[0]
        );

        timeout(Duration::from_secs(5), manager.shutdown())
            .await
            .expect("shutdown timed out")
            .unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Regression: a grant that could not be queued behind a full writer was
    /// dropped, and the child waited for it forever. The child requests a
    /// permit while the parent's stdin writer is stuck on a large broadcast
    /// with its data queue full.
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn identify_grant_is_delivered_after_a_saturated_writer_recovers() {
        let dir = scratch_dir("saturated");
        let script = r#"
P='@discordrs-ipc/1 '
sleep 0.5
echo "$P{\"op\":\"identify_request\",\"shard_id\":0}"
sleep 1
while read -r line; do
  case "$line" in
    *'"op":"shutdown"'*) exit 0 ;;
    *identify_grant*)
      L=$(printf '%s' "$line" | sed 's/.*"lease":\([0-9]*\).*/\1/')
      echo "$P{\"op\":\"identify_release\",\"shard_id\":0,\"lease\":$L,\"sent\":true}"
      echo "$P{\"op\":\"ready\",\"shard_id\":0}" ;;
  esac
done
"#;
        let mut config = scripted_child_config(script);
        config.broadcast_timeout = Duration::from_secs(1);
        let manager = Arc::new(ProcessShardManager::spawn(config).await.unwrap());
        assert!(
            wait_until(Duration::from_secs(5), || manager.children()[0]
                .pid
                .is_some())
            .await
        );

        // Stuck write (200 KiB > pipe buffer) plus a full data queue, all
        // before the child asks for its permit.
        let mut broadcasts = vec![{
            let manager = Arc::clone(&manager);
            tokio::spawn(async move { manager.broadcast(json!("x".repeat(200 * 1024))).await })
        }];
        sleep(Duration::from_millis(100)).await;
        for _ in 0..IPC_WRITE_QUEUE_CAPACITY + 4 {
            let manager = Arc::clone(&manager);
            broadcasts.push(tokio::spawn(async move {
                manager.broadcast(json!("small")).await
            }));
        }

        assert!(
            wait_until(Duration::from_secs(10), || all_in_state(
                &manager,
                ChildState::Running
            ))
            .await,
            "the grant was lost behind the saturated writer: {:?}",
            manager.children()
        );
        for broadcast in broadcasts {
            let _ = broadcast.await;
        }
        timeout(Duration::from_secs(5), manager.shutdown())
            .await
            .expect("shutdown timed out")
            .unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// When a grant cannot be delivered at all (the child closed stdin),
    /// the session is failed: the child is killed instead of waiting.
    #[cfg(unix)]
    #[tokio::test]
    async fn undeliverable_grant_fails_the_session() {
        let script = r#"
exec 0<&-
echo '@discordrs-ipc/1 {"op":"identify_request","shard_id":0}'
exec sleep 30
"#;
        let mut config = scripted_child_config(script);
        config.respawn = false;
        let manager = ProcessShardManager::spawn(config).await.unwrap();
        let error = timeout(Duration::from_secs(5), manager.wait())
            .await
            .expect("the child must be killed once its stdin is gone")
            .unwrap_err()
            .to_string();
        assert!(error.contains("killed by the manager"), "{error}");
        assert!(error.contains("IPC write to the child failed"), "{error}");
        assert_eq!(manager.children()[0].state, ChildState::Stopped);
    }

    fn test_monitor_context(
        scheduler: &Arc<IdentifyScheduler>,
        kill_reap_timeout: Duration,
    ) -> MonitorContext {
        MonitorContext {
            index: 0,
            shard_ids: vec![0],
            status: Arc::new(StdMutex::new(ChildStatus {
                pid: None,
                shard_ids: vec![0],
                state: ChildState::Starting,
                respawns: 0,
            })),
            program: PathBuf::from("/bin/sh"),
            args: Vec::new(),
            env: Vec::new(),
            respawn: false,
            respawn_delay: Duration::ZERO,
            max_respawns: None,
            start_delay: Duration::ZERO,
            shutdown_timeout: Duration::from_secs(5),
            kill_reap_timeout,
            max_frame: MIN_IPC_FRAME_BYTES,
            scheduler: Arc::clone(scheduler),
            identify_permit_timeout: Duration::from_secs(60),
        }
    }

    /// Runs `wait_failed` for a live child that holds the only bucket, as if
    /// `child.wait()` had just returned `Err`. Returns the outcome and the
    /// child's pid.
    #[cfg(target_os = "linux")]
    async fn fail_wait_for_live_child(
        scheduler: &Arc<IdentifyScheduler>,
        kill_reap_timeout: Duration,
    ) -> (ChildRunOutcome, u32) {
        let context = test_monitor_context(scheduler, kill_reap_timeout);
        let child = Command::new("/bin/sh")
            .args(["-c", "exec sleep 30"])
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let pid = child.id().unwrap();
        let mut session = ChildSession::new(&context);
        let lease = scheduler.acquire(0).await;
        session.identify_held.insert(
            0,
            HeldPermit {
                lease_id: 1,
                lease,
                expires_at: deadline_after(Duration::from_secs(60)),
            },
        );
        let outcome = wait_failed(&context, child, session, io::Error::other("injected")).await;
        (outcome, pid)
    }

    #[cfg(target_os = "linux")]
    fn process_exists(pid: u32) -> bool {
        std::path::Path::new(&format!("/proc/{pid}")).exists()
    }

    /// Regression: a `child.wait()` error was logged and treated as a
    /// confirmed exit, so the session's buckets were returned and the
    /// monitor could respawn or report success while the child might still
    /// run. Now the child is killed and its exit confirmed first.
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn wait_error_kills_and_confirms_before_returning_buckets() {
        let scheduler = Arc::new(IdentifyScheduler::new(1, 1, Duration::ZERO, None));
        let (outcome, pid) = fail_wait_for_live_child(&scheduler, KILL_REAP_TIMEOUT).await;
        let ChildRunOutcome::Failed(reason) = outcome else {
            panic!("a wait error must end the monitor with a failure");
        };
        assert!(reason.contains("injected"), "{reason}");
        assert!(reason.contains("exit confirmed"), "{reason}");
        assert!(!process_exists(pid), "the child was not reaped");
        // The bucket came back with the confirmed exit.
        timeout(Duration::from_secs(1), scheduler.acquire(0))
            .await
            .expect("bucket still held after a confirmed exit");
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn wait_error_without_confirmation_keeps_the_bucket_until_the_exit() {
        let scheduler = Arc::new(IdentifyScheduler::new(1, 1, Duration::ZERO, None));
        // A zero reap timeout treats the kill as unconfirmed right away.
        let (outcome, pid) = fail_wait_for_live_child(&scheduler, Duration::ZERO).await;
        let ChildRunOutcome::Failed(reason) = outcome else {
            panic!("a wait error must end the monitor with a failure");
        };
        assert!(reason.contains("not confirmed"), "{reason}");
        // The background reaper returns the bucket only after the OS
        // reaped the child, so the process is gone by the time it is free.
        timeout(Duration::from_secs(5), scheduler.acquire(0))
            .await
            .expect("the bucket must come back once the exit is observed");
        assert!(!process_exists(pid), "bucket returned before the exit");
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

    // -- /gateway/bot ----------------------------------------------------------------

    /// Serves `/gateway/bot` once per scripted response.
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
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
            requests
        });
        (port, server)
    }

    fn rest_for(port: u16) -> RestClient {
        RestClient::builder("token", None)
            .api_base(format!("http://127.0.0.1:{port}"))
            .build()
            .unwrap()
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn spawn_auto_reads_gateway_bot_and_never_skips_the_budget_silently() {
        let ok = r#"{"url":"wss://gateway.discord.gg","shards":3,"session_start_limit":{"total":1000,"remaining":999,"reset_after":0,"max_concurrency":2}}"#;
        let unauthorized = r#"{"message":"401: Unauthorized","code":0}"#;
        let (port, server) = gateway_bot_server(vec![
            ("200 OK", ok),
            ("401 Unauthorized", unauthorized),
            ("401 Unauthorized", unauthorized),
            ("401 Unauthorized", unauthorized),
        ])
        .await;

        let mut config = scripted_child_config(EXIT_ON_SHUTDOWN);
        config.total_shards = None;
        config.session_start_budget = SessionStartBudget::FromGateway;
        let manager = ProcessShardManager::spawn_auto_with(rest_for(port), config.clone())
            .await
            .unwrap();
        assert_eq!(manager.shard_count(), 3);
        assert_eq!(manager.max_concurrency(), 2);
        assert_eq!(manager.children().len(), 3);
        timeout(Duration::from_secs(5), manager.shutdown())
            .await
            .expect("shutdown timed out")
            .unwrap();

        // Explicit sizing with a fixed budget: only max_concurrency is
        // missing, so a failed lookup falls back to one bucket.
        let fixed = ProcessShardManagerConfig {
            total_shards: Some(2),
            session_start_budget: SessionStartBudget::Fixed(session_limit(10, 1000, 0)),
            ..config.clone()
        };
        let manager = ProcessShardManager::spawn_auto_with(rest_for(port), fixed)
            .await
            .unwrap();
        assert_eq!((manager.shard_count(), manager.max_concurrency()), (2, 1));
        timeout(Duration::from_secs(5), manager.shutdown())
            .await
            .expect("shutdown timed out")
            .unwrap();

        // Fully explicit sizing still reads the budget, and failing to do
        // so is an error rather than an unprotected start.
        let explicit = ProcessShardManagerConfig {
            total_shards: Some(2),
            max_concurrency: Some(2),
            ..config.clone()
        };
        assert!(
            ProcessShardManager::spawn_auto_with(rest_for(port), explicit)
                .await
                .is_err()
        );
        // An unknown shard count is required either way.
        let unknown = ProcessShardManagerConfig {
            session_start_budget: SessionStartBudget::Unchecked,
            ..config
        };
        assert!(
            ProcessShardManager::spawn_auto_with(rest_for(port), unknown)
                .await
                .is_err()
        );

        let requests = server.await.unwrap();
        assert_eq!(requests.len(), 4);
        assert!(requests
            .iter()
            .all(|request| request.starts_with("GET /gateway/bot ")));
    }

    /// Regression: only `remaining == 0` at startup was respected, so with
    /// one session start left two children in different buckets both got
    /// grants. Sizing is explicit here; the budget is still read.
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn session_budget_paces_grants_across_children() {
        let low = r#"{"url":"wss://gateway.discord.gg","shards":2,"session_start_limit":{"total":1000,"remaining":1,"reset_after":400,"max_concurrency":2}}"#;
        let refreshed = r#"{"url":"wss://gateway.discord.gg","shards":2,"session_start_limit":{"total":1000,"remaining":10,"reset_after":3600000,"max_concurrency":2}}"#;
        let (port, server) = gateway_bot_server(vec![("200 OK", low), ("200 OK", refreshed)]).await;

        let dir = scratch_dir("budget");
        let mut config = identify_child_config(&dir, SEND_RELEASE_AND_READY);
        config.total_shards = Some(2);
        config.max_concurrency = Some(2);
        config.session_start_budget = SessionStartBudget::FromGateway;
        let manager = ProcessShardManager::spawn_auto_with(rest_for(port), config)
            .await
            .unwrap();
        assert!(
            wait_until(Duration::from_secs(10), || all_in_state(
                &manager,
                ChildState::Running
            ))
            .await,
            "children never reached Running: {:?}",
            manager.children()
        );
        // Different buckets, so only the budget can separate the grants.
        let first = read_times(&dir, "grant-0");
        let second = read_times(&dir, "grant-1");
        assert_eq!((first.len(), second.len()), (1, 1));
        let gap = (first[0] - second[0]).abs();
        assert!(gap >= 350_000_000, "grants only {gap}ns apart");
        assert_eq!(server.await.unwrap().len(), 2);

        timeout(Duration::from_secs(5), manager.shutdown())
            .await
            .expect("shutdown timed out")
            .unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn gateway_bot_model_is_used_for_budgets() {
        // Keeps the import honest for non-Linux targets where the
        // integration tests above are compiled out.
        let gateway_bot = GatewayBot {
            url: String::new(),
            shards: 1,
            session_start_limit: session_limit(2, 10, 0),
        };
        let state = BudgetState::from_limit(&gateway_bot.session_start_limit);
        assert_eq!(state.available, 2);
    }
}
