//! Gateway session handoff between processes: a process that is being
//! replaced closes its shards so that Discord keeps their sessions, and the
//! next process RESUMEs them instead of starting new ones.
//!
//! The design, the sequence semantics, and the Decision Log are in
//! `docs/design/gateway-session-handoff.md`.

use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex, MutexGuard as StdMutexGuard};

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::watch;
use tracing::warn;

use crate::bitfield::Intents;
use crate::event::{Event, EventKind};
#[cfg(doc)]
use crate::gateway::ClientBuilder;
use crate::model::{ApplicationId, UserId};

/// Characters of a session id that `Debug` output and logs show. Enough to
/// tell sessions apart in a log; a session id is half of what RESUME needs
/// (the token is the other half), so the rest stays out of logs.
const SESSION_ID_HINT_CHARS: usize = 6;

/// A gateway session handed off by a process that stopped, for the next
/// process to RESUME (see [`GatewayHandoff`]).
///
/// It holds what RESUME needs (the session id, the last sequence number to
/// replay from, and the resume URL), the shard and intents the session was
/// opened with, and two values READY told the old process that a resumed
/// process never receives: the application id and the bot's user id.
///
/// Serialize it with serde to pass it on, typically as JSON in a file only
/// the bot's user can read (mode 0600): together with the bot token it
/// continues the session. The JSON form is an object with the fields
/// `session_id`, `sequence`, `resume_gateway_url`, `shard_id`,
/// `shard_count`, `intents`, `application_id`, and `user_id`; later
/// versions only add optional fields, so a snapshot written by one 3.x
/// process can be read by the next. `Debug` shows only the start of the
/// session id.
///
/// A session must be resumed once at most: delete the file once it has been
/// read. Resuming an old snapshot a second time replays events the first
/// resume already delivered (or fails, and the shard identifies anew).
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatewaySession {
    session_id: String,
    sequence: u64,
    #[serde(default)]
    resume_gateway_url: Option<String>,
    shard_id: u32,
    shard_count: u32,
    intents: u64,
    #[serde(default)]
    application_id: Option<ApplicationId>,
    #[serde(default)]
    user_id: Option<UserId>,
}

impl GatewaySession {
    /// Discord's session id from READY.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// The sequence number RESUME sends: the last event of this session
    /// that was handed to the old process's event handler. Discord replays
    /// every later event.
    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    /// READY's `resume_gateway_url`, without the query parameters a client
    /// adds (API version, encoding, compression), so the next process builds
    /// its own. `None` when the old process had no usable one (for example
    /// it could not be dialed); the new process then sends RESUME to its
    /// configured gateway URL.
    pub fn resume_gateway_url(&self) -> Option<&str> {
        self.resume_gateway_url.as_deref()
    }

    /// The shard as `(shard_id, shard_count)`. A process resumes the session
    /// only on the same shard of the same shard count.
    pub fn shard(&self) -> (u32, u32) {
        (self.shard_id, self.shard_count)
    }

    /// The intents of the IDENTIFY that opened the session. A resumed
    /// session keeps them, so a client configured with other intents
    /// identifies anew instead of resuming.
    pub fn intents(&self) -> Intents {
        Intents::from_bits(self.intents)
    }

    /// The application id the old process used (from READY or
    /// [`ClientBuilder::application_id`]). A client that resumes the session
    /// uses it unless its builder sets one, and only until a fresh READY
    /// (after a fallback to IDENTIFY) brings the id of the token's
    /// application.
    pub fn application_id(&self) -> Option<ApplicationId> {
        self.application_id
    }

    /// The bot's user id from the READY that opened the session, if the old
    /// process saw it.
    pub fn user_id(&self) -> Option<UserId> {
        self.user_id
    }
}

#[cfg(test)]
impl GatewaySession {
    /// A session for tests in other modules (the fields are private).
    pub(crate) fn for_test(
        session_id: &str,
        sequence: u64,
        resume_gateway_url: Option<&str>,
        shard: (u32, u32),
        intents: u64,
    ) -> Self {
        Self {
            session_id: session_id.to_owned(),
            sequence,
            resume_gateway_url: resume_gateway_url.map(str::to_owned),
            shard_id: shard.0,
            shard_count: shard.1,
            intents,
            application_id: None,
            user_id: None,
        }
    }

    pub(crate) fn with_application_id(mut self, application_id: ApplicationId) -> Self {
        self.application_id = Some(application_id);
        self
    }
}

impl fmt::Debug for GatewaySession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GatewaySession")
            .field("session_id", &SessionIdHint(&self.session_id))
            .field("sequence", &self.sequence)
            .field("resume_gateway_url", &self.resume_gateway_url)
            .field("shard", &self.shard())
            .field("intents", &self.intents)
            .field("application_id", &self.application_id)
            .field("user_id", &self.user_id)
            .finish()
    }
}

/// Shows the first [`SESSION_ID_HINT_CHARS`] characters of a session id and
/// how long it is, for `Debug` output and logs.
pub(crate) struct SessionIdHint<'a>(pub(crate) &'a str);

impl fmt::Display for SessionIdHint<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let shown: String = self.0.chars().take(SESSION_ID_HINT_CHARS).collect();
        write!(formatter, "{shown}… ({} chars)", self.0.chars().count())
    }
}

impl fmt::Debug for SessionIdHint<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "\"{self}\"")
    }
}

/// Ends a client's gateway connections for a process handoff and collects
/// the sessions the next process resumes.
///
/// Register a handle with [`ClientBuilder::handoff`], start the client, and
/// call [`request`](Self::request) when the process is about to be
/// replaced. Every shard of the client then
///
/// 1. stops handing events to the event handler;
/// 2. closes its connection with close code 4000, which keeps the session
///    resumable (closing with 1000 or 1001, as a normal shutdown does, makes
///    Discord end it);
/// 3. gives running handler calls up to
///    [`ClientBuilder::handler_shutdown_timeout`] to finish and aborts the
///    rest, as a normal shutdown does; and
/// 4. records its [`GatewaySession`], whose sequence number is the last
///    event the handler was given.
///
/// `start` (or `ShardSupervisor::wait`) then returns `Ok(())`, and
/// [`take_sessions`](Self::take_sessions) returns the sessions. The start
/// future must keep being polled after the request: in a `tokio::select!`,
/// pin it and await it again, as below.
///
/// The next process passes the sessions to [`ClientBuilder::resume_sessions`].
/// Its first connection per shard RESUMEs, Discord replays every event after
/// the handed-off sequence (including those the old process read but did
/// not hand to its handler), and the `resumed` handler runs. **It receives
/// no READY and starts with an empty cache**: initialisation that runs on
/// READY must also run on RESUMED, or not depend on either. When the session
/// cannot be resumed (INVALID_SESSION, close code 4007 or 4009, or a
/// session that does not match the shard count or intents), the shard logs
/// a warning and identifies anew, and READY follows as usual. A resume uses
/// none of the daily session-start budget and waits for no IDENTIFY gate.
///
/// What the application must do, and what it gets:
///
/// - Events whose handler call started before the handoff are not replayed.
///   A call aborted at the shutdown timeout loses the rest of its work, so
///   such events are delivered at most once: set the timeout long enough
///   for your handlers, and finish work your handlers spawned (tasks,
///   queues) before the process exits.
/// - Every other event, including interactions, is replayed to the next
///   process. Discord's three-second interaction deadline keeps running
///   meanwhile, so keep the gap between the request and the new process's
///   RESUME short.
/// - Interactions run in their own lane and can overtake queued regular
///   events. If the handoff leaves regular events queued behind an
///   interaction whose handler already ran, the sequence points before that
///   interaction and the next process receives it again (at least once);
///   Discord rejects a second response with error 40060.
/// - Events a running handler's collector or `fetch_members` consumed after
///   the request are replayed as well.
///
/// ```no_run
/// use std::io::Write;
///
/// use discordrs::{gateway_intents, Client, EventHandler, GatewayHandoff, GatewaySession};
///
/// struct Bot;
/// impl EventHandler for Bot {}
///
/// const SESSIONS: &str = "/var/lib/my-bot/gateway-sessions.json";
///
/// /// SIGTERM, an admin command, a new binary on disk, ...
/// async fn update_requested() {
///     std::future::pending::<()>().await
/// }
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let token = std::env::var("DISCORD_TOKEN")?;
///     // Sessions the previous process handed off. Delete the file at once:
///     // a session must not be resumed twice.
///     let sessions: Vec<GatewaySession> = match std::fs::read(SESSIONS) {
///         Ok(json) => {
///             std::fs::remove_file(SESSIONS)?;
///             serde_json::from_slice(&json)?
///         }
///         Err(_) => Vec::new(),
///     };
///
///     let handoff = GatewayHandoff::new();
///     let run = Client::builder(&token, gateway_intents::GUILDS)
///         .event_handler(Bot)
///         .resume_sessions(sessions)
///         .handoff(handoff.clone())
///         .start();
///     tokio::pin!(run);
///     tokio::select! {
///         result = &mut run => return Ok(result?),
///         _ = update_requested() => handoff.request(),
///     }
///     // Closes every shard with 4000 and lets running handlers finish.
///     run.await?;
///
///     let json = serde_json::to_vec(&handoff.take_sessions())?;
///     let mut options = std::fs::OpenOptions::new();
///     options.write(true).create_new(true);
///     #[cfg(unix)]
///     std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
///     options.open(SESSIONS)?.write_all(&json)?;
///     // Now replace the process (exec the new binary, or exit and let the
///     // supervisor start it).
///     Ok(())
/// }
/// ```
#[derive(Clone)]
pub struct GatewayHandoff {
    shared: Arc<HandoffShared>,
}

struct HandoffShared {
    requested: watch::Sender<bool>,
    // Written by each shard runtime once its handoff finished, read by
    // `take_sessions`; locked only to insert or drain.
    sessions: StdMutex<BTreeMap<u32, GatewaySession>>,
}

impl GatewayHandoff {
    /// A handle that has not been requested yet.
    pub fn new() -> Self {
        let (requested, _) = watch::channel(false);
        Self {
            shared: Arc::new(HandoffShared {
                requested,
                sessions: StdMutex::new(BTreeMap::new()),
            }),
        }
    }

    /// Asks every shard of the client to close for a handoff (see the type
    /// docs). Returns at once; the shards stop while the start future (or
    /// `ShardSupervisor::wait`) is polled. Calling it again has no effect,
    /// and it cannot be withdrawn. A shard that has no session yet (before
    /// its first READY, or waiting for an IDENTIFY permit) stops as in a
    /// normal shutdown and records no session.
    pub fn request(&self) {
        self.shared.requested.send_replace(true);
    }

    /// Whether [`request`](Self::request) was called.
    pub fn is_requested(&self) -> bool {
        *self.shared.requested.borrow()
    }

    /// The sessions recorded since the last call, ordered by shard id. Call
    /// it after the start future (or `ShardSupervisor::wait`) returned:
    /// every shard has recorded its session by then, even when that call
    /// returns an error (for example a handler that could not be
    /// cancelled).
    pub fn take_sessions(&self) -> Vec<GatewaySession> {
        std::mem::take(&mut *self.lock_sessions())
            .into_values()
            .collect()
    }

    /// Resolves once the handoff was requested.
    pub(crate) async fn requested(&self) {
        let mut requested = self.shared.requested.subscribe();
        if requested.wait_for(|requested| *requested).await.is_err() {
            // `self` holds the sender, so this cannot happen while polled.
            std::future::pending::<()>().await;
        }
    }

    pub(crate) fn record(&self, session: GatewaySession) {
        self.lock_sessions().insert(session.shard_id, session);
    }

    fn lock_sessions(&self) -> StdMutexGuard<'_, BTreeMap<u32, GatewaySession>> {
        self.shared
            .sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl Default for GatewayHandoff {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for GatewayHandoff {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GatewayHandoff")
            .field("requested", &self.is_requested())
            .field("recorded_sessions", &self.lock_sessions().len())
            .finish()
    }
}

/// Builds the snapshot of one shard after its handoff.
pub(crate) struct HandedOffShard {
    pub(crate) session_id: String,
    /// READY's resume URL without this client's query parameters.
    pub(crate) resume_gateway_url: Option<String>,
    pub(crate) shard: (u32, u32),
    pub(crate) intents: u64,
    pub(crate) application_id: Option<ApplicationId>,
}

impl HandedOffShard {
    pub(crate) fn into_session(self, point: ResumePoint) -> GatewaySession {
        GatewaySession {
            session_id: self.session_id,
            sequence: point.sequence,
            resume_gateway_url: self.resume_gateway_url,
            shard_id: self.shard.0,
            shard_count: self.shard.1,
            intents: self.intents,
            application_id: self.application_id,
            user_id: point.user_id,
        }
    }
}

/// Picks, for every shard in `shards` of a client with `shard_count` shards
/// and `intents`, the handed-off session it resumes. A session that cannot
/// be resumed by this client is dropped with a warning: its shard then
/// identifies anew.
pub(crate) fn match_sessions(
    sessions: Vec<GatewaySession>,
    shards: std::ops::Range<u32>,
    shard_count: u32,
    intents: u64,
) -> BTreeMap<u32, GatewaySession> {
    let mut matched = BTreeMap::new();
    for session in sessions {
        let (shard_id, count) = session.shard();
        let problem = if session.session_id.is_empty() {
            Some("it has no session id".to_string())
        } else if count != shard_count {
            Some(format!(
                "it belongs to a client with {count} shard(s), this one runs {shard_count}"
            ))
        } else if !shards.contains(&shard_id) {
            Some(format!("this client does not run shard {shard_id}"))
        } else if session.intents != intents {
            Some(format!(
                "it was opened with intents {}, this client uses {intents} (a resumed session \
                 keeps its intents)",
                session.intents
            ))
        } else if matched.contains_key(&shard_id) {
            Some(format!("another session for shard {shard_id} came first"))
        } else {
            None
        };
        match problem {
            Some(problem) => warn!(
                "not resuming handed-off gateway session {} of shard {shard_id}/{count}: \
                 {problem}; the shard identifies anew",
                SessionIdHint(&session.session_id)
            ),
            None => {
                matched.insert(shard_id, session);
            }
        }
    }
    matched
}

/// The handler lane a dispatch is routed to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DeliveryLane {
    Regular,
    Interactions,
}

/// What the ledger needs to know about a dispatch, taken before the
/// delivery that carries it is moved into the lane.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RouteInfo {
    ready: bool,
    sequence: Option<u64>,
    user_id: Option<UserId>,
}

impl RouteInfo {
    pub(crate) fn of(event: &Event) -> Self {
        let user_id = match event.kind() {
            EventKind::Ready(payload) => Some(payload.user.id),
            _ => None,
        };
        Self {
            ready: event.name() == "READY",
            sequence: event.metadata().sequence(),
            user_id,
        }
    }
}

/// How [`DeliveryLedger::enqueue`] ended.
#[derive(Debug)]
pub(crate) enum Enqueued<T> {
    Queued,
    /// The shard is handing off: nothing was recorded or sent, and Discord
    /// replays the dispatch to the next process.
    Halted,
    /// The lane refused the value (see [`DeliveryLedger::enqueue`]).
    Refused(TrySendError<T>),
}

/// Where a handed-off session resumes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ResumePoint {
    pub(crate) sequence: u64,
    pub(crate) user_id: Option<UserId>,
}

/// Tracks which dispatches of a shard's current session have been handed to
/// the event handler, so a handoff can tell the next process where to
/// resume.
///
/// ```text
/// [Decision Log]
/// - 목적과 의도: handoff 스냅샷의 sequence를 "핸들러에 실제로 넘긴 마지막 이벤트"로 만들어,
///   소켓에서 읽었지만 아직 핸들러에 넘기지 않은 이벤트를 Discord가 다음 프로세스에 재전송하게 한다.
/// - 기존 구현 및 제약 조건: 읽기 루프는 콜백 전에 sequence를 올린다(재연결 RESUME용). 그 뒤에
///   처리기 → 일반/상호작용 두 레인이 있고, 레인은 각각 FIFO지만 서로 순서가 다르다
///   (상호작용이 앞지른다). 읽기 루프는 절대 기다리면 안 되고, 성능 게이트는 디스패치당 할당
///   수를 본다.
/// - 검토한 주요 대안: (1) 읽기 루프의 sequence를 그대로 쓴다 — 큐에 남은 이벤트가 사라진다.
///   (2) 레인별 "마지막으로 넘긴 sequence"의 최솟값 — 원자값만으로 되지만 다른 레인 이벤트가
///   끼어 있으면 이미 넘긴 이벤트를 다시 받는다. (3) 레인별 대기 FIFO + 처리기 진행점,
///   handoff를 설정한 클라이언트에서만.
/// - 선택한 방식: (3). 처리기가 레인에 넣기 전에 (세션 번호, sequence)를 FIFO에 적고, 디스패처가
///   핸들러 호출을 시작할 때 앞에서 뺀다. 재개점 = 현재 세션에서 아직 넘기지 않은 가장 오래된
///   이벤트 - 1(없으면 처리기가 마지막으로 보낸 sequence). 세션 번호는 처리기가 READY마다 올리고,
///   읽기 쪽이 센 READY 수와 다르거나 현재 세션의 READY를 아직 넘기지 않았으면 스냅샷을 만들지
///   않는다(다음 프로세스가 READY를 받아야 하므로 새로 IDENTIFY).
/// - 다른 대안 대신 이 방식을 선택한 이유: 레인이 순서를 지키는 한 정확하다(손실 없음, 중복은
///   상호작용이 앞지른 경우뿐). (2)는 같은 코드량으로 더 많은 중복을 낸다. 잠금은 짧고 await를
///   건너지 않으며, handoff가 없는 클라이언트는 비용이 없다(Option 분기 하나).
/// - 장점, 단점 및 영향: handoff 클라이언트는 디스패치마다 std 뮤텍스를 두 번 잡고 VecDeque가
///   큐 깊이만큼 자란다(정상 상태에선 재할당 없음). 일반 큐가 가득 차 버려진 이벤트는 기존처럼
///   손실로 친다(재전송되지 않는다).
/// - 2026-10-23 리뷰 수정(f76ee19 이후): 처리기가 기록한 뒤 `try_send` 전에 handoff가 레인을
///   닫으면 `Closed`가 "넘김"으로 세어져 재개점이 넘기지 않은 이벤트를 건너뛰었다. 이제 기록과
///   `try_send`를 같은 잠금 안에서 하고(`enqueue`), `halt`도 그 잠금을 잡으므로 정지 뒤에는 아무것도
///   기록·전송되지 않는다. `Closed`는 대기로 남긴다(레인이 닫혔다면 핸들러가 받지 못했다).
///   `try_send`는 기다리지 않으므로 잠금 안에서 불러도 await를 건너지 않는다.
/// ```
#[derive(Default)]
pub(crate) struct DeliveryLedger {
    // Locked by the shard's event processor (routing) and both handler
    // dispatchers (hand-off), each time for a few instructions and never
    // across an await.
    state: StdMutex<LedgerState>,
    /// READY dispatches the gateway read loop queued for the processor.
    ready_read: AtomicU64,
}

#[derive(Default)]
struct LedgerState {
    /// Sessions the processor has seen: one per READY.
    epoch: u64,
    /// Sequence of the last dispatch of `epoch` the processor routed.
    routed: u64,
    /// Whether the READY that opened `epoch` reached the handler. Seeded
    /// `true` for a session resumed from a handoff (an earlier process
    /// handled its READY).
    ready_handed: bool,
    user_id: Option<UserId>,
    regular: VecDeque<Pending>,
    interactions: VecDeque<Pending>,
    /// Set when the shard hands off; from then on nothing is queued. Under
    /// the same lock as routing, so a halt cannot fall between recording a
    /// dispatch and queueing it.
    halted: bool,
}

/// A routed dispatch that no handler call has started for yet.
#[derive(Clone, Copy)]
struct Pending {
    epoch: u64,
    sequence: Option<u64>,
    ready: bool,
}

impl LedgerState {
    fn lane(&mut self, lane: DeliveryLane) -> &mut VecDeque<Pending> {
        match lane {
            DeliveryLane::Regular => &mut self.regular,
            DeliveryLane::Interactions => &mut self.interactions,
        }
    }
}

impl DeliveryLedger {
    fn lock(&self) -> StdMutexGuard<'_, LedgerState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Starts from a session handed off by an earlier process.
    pub(crate) fn seed(&self, sequence: u64, user_id: Option<UserId>) {
        let mut state = self.lock();
        state.routed = sequence;
        state.ready_handed = true;
        state.user_id = user_id;
    }

    /// Called by the read loop's ingress for every dispatch it queues.
    pub(crate) fn note_read(&self, name: &str) {
        if name == "READY" {
            self.ready_read.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// Records a dispatch and queues it for `lane` with `send` (a
    /// non-blocking `try_send`), both under the ledger's lock, so a
    /// concurrent [`Self::halt`] lands either before (nothing is recorded or
    /// sent) or after (the dispatch is queued, and the halted lane leaves it
    /// pending).
    ///
    /// A lane that is full refuses the dispatch for good: the regular lane
    /// drops the handler call as before, the interaction lane gives it to
    /// the overload hook, so it counts as delivered. A closed lane never
    /// gave it to a handler, so it stays pending and is replayed.
    pub(crate) fn enqueue<T>(
        &self,
        lane: DeliveryLane,
        dispatch: RouteInfo,
        send: impl FnOnce() -> Result<(), TrySendError<T>>,
    ) -> Enqueued<T> {
        let mut state = self.lock();
        if state.halted {
            return Enqueued::Halted;
        }
        if dispatch.ready {
            state.epoch += 1;
            state.routed = 0;
            state.ready_handed = false;
            if dispatch.user_id.is_some() {
                state.user_id = dispatch.user_id;
            }
        }
        if let Some(sequence) = dispatch.sequence {
            state.routed = sequence;
        }
        let epoch = state.epoch;
        state.lane(lane).push_back(Pending {
            epoch,
            sequence: dispatch.sequence,
            ready: dispatch.ready,
        });
        match send() {
            Ok(()) => Enqueued::Queued,
            Err(TrySendError::Full(value)) => {
                if let Some(pending) = state.lane(lane).pop_back() {
                    state.mark_delivered(pending);
                }
                Enqueued::Refused(TrySendError::Full(value))
            }
            Err(TrySendError::Closed(value)) => Enqueued::Refused(TrySendError::Closed(value)),
        }
    }

    /// The oldest dispatch queued in `lane` was given to a handler call (or,
    /// for an interaction past its admission deadline, to the overload
    /// hook).
    pub(crate) fn handed(&self, lane: DeliveryLane) {
        let mut state = self.lock();
        if let Some(pending) = state.lane(lane).pop_front() {
            state.mark_delivered(pending);
        }
    }

    /// Stops queueing for a handoff. Waits for an [`Self::enqueue`] in
    /// progress (a few instructions).
    pub(crate) fn halt(&self) {
        self.lock().halted = true;
    }

    /// Where the current session resumes, once the pipeline has stopped
    /// handing out events. `None` when the processor has not yet seen the
    /// latest READY the read loop queued, or the handler has not yet been
    /// given it: the next process must identify to receive a READY.
    pub(crate) fn resume_point(&self) -> Option<ResumePoint> {
        let state = self.lock();
        if self.ready_read.load(Ordering::SeqCst) != state.epoch || !state.ready_handed {
            return None;
        }
        let oldest_pending = state
            .regular
            .iter()
            .chain(state.interactions.iter())
            .filter(|pending| pending.epoch == state.epoch)
            .filter_map(|pending| pending.sequence)
            .min();
        let sequence = match oldest_pending {
            Some(oldest) => oldest.saturating_sub(1).min(state.routed),
            None => state.routed,
        };
        Some(ResumePoint {
            sequence,
            user_id: state.user_id,
        })
    }
}

impl LedgerState {
    fn mark_delivered(&mut self, pending: Pending) {
        if pending.ready && pending.epoch == self.epoch {
            self.ready_handed = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventMetadata;
    use crate::model::test_id;
    use serde_json::json;

    fn session(shard: (u32, u32), intents: u64) -> GatewaySession {
        GatewaySession {
            session_id: "0123456789abcdef0123456789abcdef".to_string(),
            sequence: 42,
            resume_gateway_url: Some("wss://gateway-us-east1-b.discord.gg".to_string()),
            shard_id: shard.0,
            shard_count: shard.1,
            intents,
            application_id: Some(test_id(7)),
            user_id: Some(test_id(8)),
        }
    }

    fn event(name: &str, sequence: u64) -> Event {
        let data = match name {
            "READY" => json!({
                "v": 10,
                "user": { "id": "9", "username": "bot" },
                "session_id": "session",
                "guilds": [],
                "application": { "id": "1", "flags": 0 }
            }),
            _ => json!({ "id": "5", "channel_id": "6", "content": "x" }),
        };
        Event::decode(
            EventMetadata::new(name, tokio::time::Instant::now()).with_sequence(sequence),
            serde_json::value::to_raw_value(&data).unwrap(),
            false,
        )
    }

    /// Queues `event` in a lane that accepts it; `false` once halted.
    fn route(ledger: &DeliveryLedger, lane: DeliveryLane, event: &Event) -> bool {
        let send = || -> Result<(), TrySendError<()>> { Ok(()) };
        !matches!(
            ledger.enqueue(lane, RouteInfo::of(event), send),
            Enqueued::Halted
        )
    }

    #[test]
    fn sessions_round_trip_through_json_and_tolerate_missing_optional_fields() {
        let original = session((1, 4), 513);
        let json = serde_json::to_value(&original).unwrap();
        assert_eq!(
            json,
            json!({
                "session_id": "0123456789abcdef0123456789abcdef",
                "sequence": 42,
                "resume_gateway_url": "wss://gateway-us-east1-b.discord.gg",
                "shard_id": 1,
                "shard_count": 4,
                "intents": 513,
                "application_id": "7",
                "user_id": "8"
            })
        );
        let back: GatewaySession = serde_json::from_value(json).unwrap();
        assert_eq!(back, original);
        let list = serde_json::to_string(&vec![original.clone(), session((2, 4), 513)]).unwrap();
        let list: Vec<GatewaySession> = serde_json::from_str(&list).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0], original);

        // A minimal object (or a newer one with fields this version does not
        // know) still reads.
        let minimal: GatewaySession = serde_json::from_value(json!({
            "session_id": "abc",
            "sequence": 3,
            "shard_id": 0,
            "shard_count": 1,
            "intents": 1,
            "added_later": true
        }))
        .unwrap();
        assert_eq!(minimal.resume_gateway_url(), None);
        assert_eq!(minimal.application_id(), None);
        assert_eq!(minimal.user_id(), None);
        assert_eq!(minimal.shard(), (0, 1));
        assert_eq!(minimal.intents().bits(), 1);
        assert_eq!(minimal.sequence(), 3);
        assert_eq!(minimal.session_id(), "abc");
    }

    #[test]
    fn debug_output_shows_only_the_start_of_the_session_id() {
        let debug = format!("{:?}", session((0, 1), 1));
        assert!(
            !debug.contains("0123456789abcdef0123456789abcdef"),
            "{debug}"
        );
        assert!(debug.contains("\"012345… (32 chars)\""), "{debug}");
        assert!(debug.contains("sequence: 42"), "{debug}");
        assert!(debug.contains("shard: (0, 1)"), "{debug}");
        assert_eq!(SessionIdHint("ab").to_string(), "ab… (2 chars)");

        let handoff = GatewayHandoff::new();
        handoff.record(session((0, 1), 1));
        let debug = format!("{handoff:?}");
        assert!(!debug.contains("0123456789"), "{debug}");
        assert!(debug.contains("recorded_sessions: 1"), "{debug}");
    }

    #[tokio::test]
    async fn the_handle_reports_its_request_and_hands_out_sessions_once_by_shard() {
        let handoff = GatewayHandoff::default();
        assert!(!handoff.is_requested());
        let waiter = handoff.clone();
        let waiting = tokio::spawn(async move { waiter.requested().await });
        handoff.request();
        handoff.request();
        tokio::time::timeout(std::time::Duration::from_secs(5), waiting)
            .await
            .unwrap()
            .unwrap();
        assert!(handoff.is_requested());
        // Resolves at once after the request.
        handoff.requested().await;

        handoff.record(session((2, 3), 1));
        handoff.record(session((0, 3), 1));
        let shards: Vec<_> = handoff.take_sessions().iter().map(|s| s.shard()).collect();
        assert_eq!(shards, vec![(0, 3), (2, 3)]);
        assert!(handoff.take_sessions().is_empty());
    }

    #[test]
    fn sessions_that_this_client_cannot_resume_are_dropped() {
        let mut empty = session((0, 2), 1);
        empty.session_id.clear();
        let mut duplicate = session((1, 2), 1);
        duplicate.sequence = 99;
        let matched = match_sessions(
            vec![
                session((1, 2), 1),
                empty,
                session((0, 3), 1),
                session((0, 2), 3),
                duplicate,
                session((5, 2), 1),
            ],
            0..2,
            2,
            1,
        );
        assert_eq!(matched.len(), 1);
        assert_eq!(
            matched[&1].sequence(),
            42,
            "the first session for a shard wins"
        );

        // A single-shard client only takes its own shard.
        let matched = match_sessions(vec![session((0, 1), 1), session((1, 1), 1)], 0..1, 1, 1);
        assert_eq!(matched.keys().copied().collect::<Vec<_>>(), vec![0]);
    }

    #[test]
    fn the_resume_point_is_the_last_dispatch_handed_to_the_handler() {
        let ledger = DeliveryLedger::default();
        // No session yet.
        assert_eq!(ledger.resume_point(), None);

        ledger.note_read("READY");
        assert!(route(&ledger, DeliveryLane::Regular, &event("READY", 1)));
        // READY was routed but not handed: the next process must identify.
        assert_eq!(ledger.resume_point(), None);
        ledger.handed(DeliveryLane::Regular);
        assert_eq!(
            ledger.resume_point(),
            Some(ResumePoint {
                sequence: 1,
                user_id: Some(test_id(9)),
            })
        );

        for sequence in 2..=4 {
            ledger.note_read("MESSAGE_CREATE");
            assert!(route(
                &ledger,
                DeliveryLane::Regular,
                &event("MESSAGE_CREATE", sequence)
            ));
        }
        ledger.handed(DeliveryLane::Regular);
        // 2 started; 3 and 4 wait in the lane.
        assert_eq!(ledger.resume_point().unwrap().sequence, 2);

        // An interaction overtakes the queued regular events.
        assert!(route(
            &ledger,
            DeliveryLane::Interactions,
            &event("INTERACTION_CREATE", 5)
        ));
        ledger.handed(DeliveryLane::Interactions);
        assert_eq!(ledger.resume_point().unwrap().sequence, 2);

        // A dispatch a full lane refused counts as delivered (dropped, or
        // given to the overload hook).
        let refused = ledger.enqueue(
            DeliveryLane::Interactions,
            RouteInfo::of(&event("INTERACTION_CREATE", 6)),
            || Err(TrySendError::Full(())),
        );
        assert!(matches!(refused, Enqueued::Refused(TrySendError::Full(()))));
        ledger.handed(DeliveryLane::Regular);
        ledger.handed(DeliveryLane::Regular);
        assert_eq!(ledger.resume_point().unwrap().sequence, 6);

        // After the halt nothing more is routed; what was not routed is
        // replayed.
        ledger.halt();
        ledger.note_read("MESSAGE_CREATE");
        assert!(!route(
            &ledger,
            DeliveryLane::Regular,
            &event("MESSAGE_CREATE", 7)
        ));
        assert_eq!(ledger.resume_point().unwrap().sequence, 6);
    }

    #[test]
    fn a_new_session_ignores_dispatches_left_from_the_old_one() {
        let ledger = DeliveryLedger::default();
        ledger.seed(100, Some(test_id(3)));
        assert_eq!(
            ledger.resume_point(),
            Some(ResumePoint {
                sequence: 100,
                user_id: Some(test_id(3)),
            })
        );
        assert!(route(
            &ledger,
            DeliveryLane::Regular,
            &event("MESSAGE_CREATE", 101)
        ));
        assert!(route(
            &ledger,
            DeliveryLane::Regular,
            &event("MESSAGE_CREATE", 102)
        ));
        assert_eq!(ledger.resume_point().unwrap().sequence, 100);

        // The session was replaced (a fresh IDENTIFY): READY is read but the
        // processor has not reached it yet.
        ledger.note_read("READY");
        assert_eq!(ledger.resume_point(), None);
        assert!(route(&ledger, DeliveryLane::Regular, &event("READY", 1)));
        assert!(route(
            &ledger,
            DeliveryLane::Regular,
            &event("MESSAGE_CREATE", 2)
        ));
        // 101 and 102 of the old session are still queued ahead of READY.
        ledger.handed(DeliveryLane::Regular);
        ledger.handed(DeliveryLane::Regular);
        assert_eq!(ledger.resume_point(), None, "READY has not started yet");
        ledger.handed(DeliveryLane::Regular);
        assert_eq!(
            ledger.resume_point(),
            Some(ResumePoint {
                sequence: 1,
                user_id: Some(test_id(9)),
            })
        );
        ledger.handed(DeliveryLane::Regular);
        assert_eq!(ledger.resume_point().unwrap().sequence, 2);
    }

    /// Regression (review of f76ee19): the processor recorded a dispatch,
    /// then the handoff closed the lane before `try_send`, and the `Closed`
    /// error counted it as delivered, so RESUME skipped an event no handler
    /// ever saw.
    #[test]
    fn a_dispatch_a_closed_lane_refused_stays_pending() {
        let ledger = DeliveryLedger::default();
        ledger.seed(4241, None);
        let refused = ledger.enqueue(
            DeliveryLane::Regular,
            RouteInfo::of(&event("MESSAGE_CREATE", 4242)),
            || Err(TrySendError::Closed(())),
        );
        assert!(matches!(
            refused,
            Enqueued::Refused(TrySendError::Closed(()))
        ));
        assert_eq!(
            ledger.resume_point().unwrap().sequence,
            4241,
            "4242 never reached a handler, so it is replayed"
        );

        // The same in the interaction lane.
        let ledger = DeliveryLedger::default();
        ledger.seed(4241, None);
        ledger.enqueue(
            DeliveryLane::Interactions,
            RouteInfo::of(&event("INTERACTION_CREATE", 4242)),
            || Err(TrySendError::Closed(())),
        );
        assert_eq!(ledger.resume_point().unwrap().sequence, 4241);
    }

    /// Regression (review of f76ee19): a halt between recording a dispatch
    /// and queueing it. The halt now waits for the enqueue in progress, and
    /// after it nothing is recorded or sent.
    #[test]
    fn a_halt_waits_for_an_enqueue_in_progress_and_stops_later_ones() {
        let ledger = DeliveryLedger::default();
        ledger.seed(10, None);
        let halted = std::sync::atomic::AtomicBool::new(false);
        std::thread::scope(|scope| {
            let outcome = ledger.enqueue(
                DeliveryLane::Regular,
                RouteInfo::of(&event("MESSAGE_CREATE", 11)),
                || -> Result<(), TrySendError<()>> {
                    // The handoff arrives while the dispatch is being queued.
                    let halting = scope.spawn(|| {
                        ledger.halt();
                        halted.store(true, std::sync::atomic::Ordering::SeqCst);
                    });
                    std::thread::sleep(std::time::Duration::from_millis(50));
                    assert!(
                        !halted.load(std::sync::atomic::Ordering::SeqCst) && !halting.is_finished(),
                        "the halt must wait until the dispatch is queued"
                    );
                    Ok(())
                },
            );
            assert!(matches!(outcome, Enqueued::Queued));
        });
        assert!(halted.load(std::sync::atomic::Ordering::SeqCst));
        // Queued before the halt and still waiting in the lane: pending.
        assert_eq!(ledger.resume_point().unwrap().sequence, 10);

        let outcome = ledger.enqueue(
            DeliveryLane::Regular,
            RouteInfo::of(&event("MESSAGE_CREATE", 12)),
            || -> Result<(), TrySendError<()>> { panic!("nothing is sent after the halt") },
        );
        assert!(matches!(outcome, Enqueued::Halted));
        assert_eq!(ledger.resume_point().unwrap().sequence, 10);
    }
}
