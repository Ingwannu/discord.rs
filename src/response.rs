use std::fmt;
use std::future::Future;
use std::sync::Arc;

use tokio::sync::watch;

pub use crate::builders::MessageBuilder;

use crate::builders::ModalBuilder;
use crate::error::DiscordError;
use crate::http::{FileAttachment, RestClient};
use crate::model::{
    ApplicationCommandOptionChoice, AutocompleteInteraction, ChatInputCommandInteraction,
    ComponentInteraction, InteractionCallbackResult, InteractionContextData,
    InteractionResponseType, Message, MessageContextMenuInteraction, MessageId,
    ModalSubmitInteraction, PrimaryEntryPointInteraction, UserContextMenuInteraction,
};
use crate::request::{
    CreateFollowup, EditInteractionResponse, InteractionMessage, InteractionResponse, Modal,
    UpdateMessage,
};

/// Discord message flag marking a response as ephemeral, as raw bits for
/// JSON bodies.
pub(crate) const EPHEMERAL_FLAG: u64 = crate::bitfield::message_flags::EPHEMERAL.bits();

/// Discord JSON error code for a second initial response: "Interaction has
/// already been acknowledged".
const ALREADY_ACKNOWLEDGED_CODE: u64 = 40060;

/// The 2.x builder for interaction callbacks; it now produces the typed
/// [`InteractionResponse`].
#[deprecated(
    since = "3.0.0",
    note = "use the InteractionResponse constructors (reply, update, defer, modal) or variants"
)]
#[derive(Clone, Debug)]
pub struct InteractionResponseBuilder {
    inner: InteractionResponse,
}

#[expect(deprecated, reason = "the deprecated builder's own methods")]
impl InteractionResponseBuilder {
    /// A `CHANNEL_MESSAGE_WITH_SOURCE` callback. Never fails (the builder
    /// converts losslessly); the `Result` is kept from 2.x.
    pub fn channel_message(message: MessageBuilder) -> Result<Self, DiscordError> {
        Ok(Self {
            inner: InteractionResponse::ChannelMessage(message.into()),
        })
    }

    /// A `DEFERRED_CHANNEL_MESSAGE_WITH_SOURCE` callback.
    pub fn deferred_channel_message(ephemeral: bool) -> Self {
        Self {
            inner: InteractionResponse::DeferredChannelMessage { ephemeral },
        }
    }

    /// An `UPDATE_MESSAGE` callback. Fails when the builder set `tts` or a
    /// poll, which a message update cannot carry.
    pub fn update_message(message: MessageBuilder) -> Result<Self, DiscordError> {
        Ok(Self {
            inner: InteractionResponse::UpdateMessage(UpdateMessage::try_from(message)?),
        })
    }

    /// A `MODAL` callback. Never fails; the `Result` is kept from 2.x.
    pub fn modal(modal: ModalBuilder) -> Result<Self, DiscordError> {
        Ok(Self {
            inner: InteractionResponse::Modal(modal.build()),
        })
    }

    /// Returns the callback.
    pub fn build(self) -> InteractionResponse {
        self.inner
    }
}

/// The 2.x name of the reply payload; the responder now takes the request
/// type of each endpoint: [`InteractionMessage`] for initial replies and
/// message updates, [`CreateFollowup`] for follow-ups, and
/// [`EditInteractionResponse`] for edits. All three convert from `&str` and
/// `String`.
#[deprecated(
    since = "3.0.0",
    note = "use request::InteractionMessage (reply, update_message), request::CreateFollowup \
            (follow_up), or request::EditInteractionResponse (edit_reply, edit_follow_up)"
)]
pub type InteractionReplyData = InteractionMessage;

/// The 2.x name of an autocomplete choice; it is now the same type as a
/// command option choice, with a typed value (string, integer, or number).
#[deprecated(since = "3.0.0", note = "use ApplicationCommandOptionChoice")]
pub type AutocompleteChoice = ApplicationCommandOptionChoice;

// ---------------------------------------------------------------------------
// Initial-response state machine
// ---------------------------------------------------------------------------
//
// [Decision Log]
// - Purpose: Discord accepts exactly one initial response per interaction
//   and rejects follow-ups and `@original` edits until one exists. Every
//   clone of an interaction (handler, collector, spawned task) must agree on
//   whether that response exists, across both transports: the REST callback
//   (gateway bots, or HTTP bots that answer Discord with 202) and the inline
//   HTTP response (interactions endpoint).
// - Existing implementation and constraints (2.2.0): one `AtomicU8` ladder
//   (unacknowledged / deferred / replied). The slot jumped to replied or
//   deferred *before* the HTTP call finished, so another clone could send a
//   follow-up before Discord had accepted the initial response. Any error
//   rolled the slot back, including a lost response after Discord had
//   accepted the callback; a dropped future left the slot claimed forever;
//   the HTTP endpoint never recorded its inline response. Since phase 1 the
//   REST client never replays an interaction callback after an ambiguous
//   failure, so the outcome of such a failure is genuinely unknown here.
// - Alternatives considered: (1) keep the ladder and only fix rollback;
//   (2) a global registry of interaction ids; (3) a per-interaction phase
//   with an explicit in-flight phase, an "outcome unknown" phase, a drop
//   guard, and a watch channel so dependent calls can wait. For uncertain
//   retries: (a) only allow retrying with the same callback kind, (b) track
//   every kind that may have landed and admit "acknowledged, kind unknown".
// - Chosen: (3) with (b). `Unacknowledged → InFlight → Deferred | Replied`,
//   with `InFlight → Unacknowledged` when Discord provably did not apply the
//   callback (4xx, 429, local validation, connect failure) and
//   `InFlight → Uncertain` when it may have (timeout, 5xx, failed body read,
//   dropped future). `Uncertain` remembers every kind that may have landed.
//   A retry refused with 40060 proves *an* earlier attempt landed, not which
//   one, so it settles to that kind only when exactly one kind was possible
//   and to "acknowledged, kind unknown" otherwise. A successful `@original`
//   operation is proof that the response exists; when it arrives while a
//   retry is in flight it is kept in the in-flight phase and applied on top
//   of the retry's outcome, so the stronger evidence always wins. The inline
//   HTTP response and the REST callback compete for the same slot with one
//   compare-and-set, so exactly one transport owns the initial response.
//   Follow-ups and `@original` operations started while a response is in
//   flight wait for its outcome instead of failing.
// - Why: a registry would need eviction and cannot see stateless id/token
//   calls either. Waiting (rather than failing) makes the natural
//   `join!(defer, follow_up)` / spawned-follow-up patterns correct without
//   retry loops. `Uncertain` is safe to leave by retrying the initial
//   response: if an earlier attempt landed, Discord answers 40060 "already
//   acknowledged", so a retry can never produce a second response. Tracking
//   the possible kinds keeps any retry kind allowed (restricting retries to
//   one kind would block "defer timed out, now reply") without ever
//   reporting a kind Discord may not hold.
// - Trade-offs and impact: `is_acknowledged()` is false while in flight or
//   uncertain (it reports only what Discord has confirmed), and
//   `is_deferred()`/`is_replied()` are both false when the kind is unknown.
//   Waiting is not bounded by any timeout: it ends when the in-flight future
//   completes or is dropped. A task that keeps a partially polled
//   initial-response future alive (neither polling nor dropping it) and then
//   awaits a follow-up waits forever. Contexts rebuilt with serde start
//   `Unacknowledged` because the state is process-local; use the
//   `RestClient` follow-up methods for them. "Connect failure or final 4xx
//   means not delivered" holds for the redirect-free clients `RestClient`
//   builds; a client passed to `RestClientBuilder::use_client` may follow a
//   redirect after delivery, and `retry_non_idempotent_requests(true)` may
//   replay a callback and report only the last attempt's error. `RestClient`
//   does not expose either setting here, so such clients can see
//   `Unacknowledged` for a delivered response; the next initial response
//   then gets 40060 and settles the state.

/// What an accepted initial response did to an interaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AckKind {
    /// Callback types 5 and 6: Discord shows a loading state (or nothing)
    /// until the original response is edited.
    Deferred,
    /// Every other initial response (message, message update, modal,
    /// autocomplete result, activity launch), or an edit that completed a
    /// deferred response.
    Replied,
}

impl AckKind {
    /// Maps an interaction callback type to the acknowledgement Discord
    /// records when it accepts that callback.
    ///
    /// Returns `None` for `PONG` (type 1), which answers Discord's endpoint
    /// ping and never acknowledges an interaction. Unknown types count as
    /// [`AckKind::Replied`] so a new Discord callback type is still treated
    /// as "an initial response exists".
    pub(crate) fn for_callback_type(kind: InteractionResponseType) -> Option<Self> {
        match kind {
            InteractionResponseType::PONG => None,
            InteractionResponseType::DEFERRED_CHANNEL_MESSAGE_WITH_SOURCE
            | InteractionResponseType::DEFERRED_UPDATE_MESSAGE => Some(Self::Deferred),
            _ => Some(Self::Replied),
        }
    }
}

/// Set of initial-response kinds that may have reached Discord.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct PossibleKinds {
    deferred: bool,
    replied: bool,
}

impl PossibleKinds {
    fn with(mut self, kind: AckKind) -> Self {
        match kind {
            AckKind::Deferred => self.deferred = true,
            AckKind::Replied => self.replied = true,
        }
        self
    }

    fn is_empty(self) -> bool {
        !self.deferred && !self.replied
    }

    /// The kind Discord holds when it is known to hold one of these: the
    /// single possible kind, or `None` when several (or none) are possible.
    fn only(self) -> Option<AckKind> {
        match (self.deferred, self.replied) {
            (true, false) => Some(AckKind::Deferred),
            (false, true) => Some(AckKind::Replied),
            _ => None,
        }
    }
}

/// Evidence from a successful `@original` operation. Ordered by strength:
/// an edit also proves the response is now a reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum OriginalSeen {
    /// A fetch or delete of `@original` succeeded.
    Exists,
    /// An edit of `@original` succeeded.
    Edited,
}

/// Internal phase; [`InteractionResponsePhase`] is its public projection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Unacknowledged,
    InFlight {
        /// What Discord records if this attempt is accepted.
        claimed: AckKind,
        /// Kinds of earlier attempts whose outcome was lost.
        prior: PossibleKinds,
        /// Strongest `@original` success seen while this attempt was in
        /// flight; applied on top of the attempt's outcome.
        original: Option<OriginalSeen>,
    },
    /// Discord holds an initial response; `None` when its kind is unknown.
    Acknowledged(Option<AckKind>),
    /// Every kind that may have landed (never empty).
    Uncertain(PossibleKinds),
}

impl Phase {
    /// Applies proof that `@original` exists to a settled phase.
    fn with_original(self, seen: OriginalSeen) -> Self {
        match (self, seen) {
            (_, OriginalSeen::Edited) => Self::Acknowledged(Some(AckKind::Replied)),
            (Self::Acknowledged(kind), OriginalSeen::Exists) => Self::Acknowledged(kind),
            (Self::Uncertain(possible), OriginalSeen::Exists) => {
                Self::Acknowledged(possible.only())
            }
            // Acknowledged outside this state (e.g. by a stateless helper).
            (Self::Unacknowledged, OriginalSeen::Exists) => Self::Acknowledged(None),
            (in_flight @ Self::InFlight { .. }, OriginalSeen::Exists) => in_flight,
        }
    }
}

/// Where the initial response of one interaction stands.
///
/// Read it with [`InteractionResponseState::phase`] or
/// [`InteractionResponder::response_phase`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum InteractionResponsePhase {
    /// No initial response has been sent, or Discord rejected the last one.
    Unacknowledged,
    /// An initial response is being sent. Follow-ups and `@original`
    /// operations wait for its outcome; another initial response fails.
    ///
    /// The wait has no timeout of its own: it ends when the future sending
    /// the initial response completes or is dropped. If a task keeps a
    /// partially polled initial-response future alive without polling it
    /// (for example a `select!` branch that lost) and then awaits a
    /// follow-up, that follow-up waits forever. Drop such futures or keep
    /// polling them.
    InFlight,
    /// Discord accepted a deferred response (callback type 5 or 6).
    Deferred,
    /// Discord accepted a reply, message update, modal, autocomplete result,
    /// or activity launch, or a deferred response was edited.
    Replied,
    /// Discord holds an initial response, but which kind is unknown: a retry
    /// was refused with error 40060 after attempts of different kinds had
    /// lost their outcome, or the response was sent outside this state (for
    /// example by a stateless helper). `is_acknowledged()` is true;
    /// `is_deferred()` and `is_replied()` are false.
    Acknowledged,
    /// An initial response may have reached Discord, but its outcome was
    /// lost (timeout, 5xx, failed response read, or the sending future was
    /// dropped). Retrying the initial response is safe: Discord answers a
    /// duplicate with error 40060, which settles the phase to acknowledged.
    /// Follow-ups and `@original` operations are passed to Discord, which
    /// rejects them if no response exists; a successful `@original`
    /// operation settles the phase.
    Uncertain,
}

impl From<Phase> for InteractionResponsePhase {
    fn from(phase: Phase) -> Self {
        match phase {
            Phase::Unacknowledged => Self::Unacknowledged,
            Phase::InFlight { .. } => Self::InFlight,
            Phase::Acknowledged(Some(AckKind::Deferred)) => Self::Deferred,
            Phase::Acknowledged(Some(AckKind::Replied)) => Self::Replied,
            Phase::Acknowledged(None) => Self::Acknowledged,
            Phase::Uncertain(_) => Self::Uncertain,
        }
    }
}

/// Shared initial-response state for one interaction.
///
/// Created when an interaction is parsed and shared by every clone, so a
/// response sent through one clone is visible to all of them. Both
/// transports record into it: the REST methods on [`InteractionContextData`]
/// and [`InteractionResponder`], and the typed HTTP interactions endpoint
/// for the inline response a handler returns. Exactly one of them owns the
/// initial response. See [`InteractionResponsePhase`] for the phases.
///
/// # Delivery assumptions
///
/// A failed initial response counts as "not delivered" (and may be sent
/// again) only when it failed to connect or Discord answered 4xx/429. That
/// holds for the clients `RestClient::new` and `RestClientBuilder::build`
/// create, which never follow redirects and never replay an interaction
/// callback. It does not hold for a client passed to
/// `RestClientBuilder::use_client` that follows redirects (the request may
/// have been delivered before the redirect failed), nor with
/// `RestClientBuilder::retry_non_idempotent_requests(true)` (an earlier
/// attempt may have been applied and only the last attempt's error is
/// reported). With such clients the state can read `Unacknowledged` for a
/// delivered response; the next initial response then gets error 40060,
/// which settles it.
#[derive(Clone)]
pub struct InteractionResponseState(Arc<watch::Sender<Phase>>);

impl Default for InteractionResponseState {
    fn default() -> Self {
        Self(Arc::new(watch::channel(Phase::Unacknowledged).0))
    }
}

impl fmt::Debug for InteractionResponseState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("InteractionResponseState")
            .field(&self.phase())
            .finish()
    }
}

impl InteractionResponseState {
    /// Creates a fresh, unacknowledged response state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the current phase.
    pub fn phase(&self) -> InteractionResponsePhase {
        self.load().into()
    }

    /// Returns true when Discord accepted a deferred response that has not
    /// been edited yet.
    pub fn is_deferred(&self) -> bool {
        self.load() == Phase::Acknowledged(Some(AckKind::Deferred))
    }

    /// Returns true when Discord accepted a reply, message update, modal,
    /// autocomplete result, or activity launch, or a deferred response was
    /// edited.
    pub fn is_replied(&self) -> bool {
        self.load() == Phase::Acknowledged(Some(AckKind::Replied))
    }

    /// Returns true when Discord confirmed an initial response of any kind.
    /// False while a response is in flight or its outcome is uncertain.
    pub fn is_acknowledged(&self) -> bool {
        matches!(self.load(), Phase::Acknowledged(_))
    }

    fn load(&self) -> Phase {
        *self.0.borrow()
    }

    /// Atomically claims the initial-response slot for one REST attempt.
    ///
    /// Exactly one caller wins when several race, including against the
    /// inline HTTP response ([`Self::commit_inline`]). Claiming is allowed
    /// from `Unacknowledged` and from `Uncertain` (a retry; see the decision
    /// log).
    pub(crate) fn begin(&self, kind: AckKind) -> Result<InitialResponseClaim, DiscordError> {
        let mut claim = Err(DiscordError::model("interaction was already acknowledged"));
        self.0.send_if_modified(|phase| {
            let prior = match *phase {
                Phase::Unacknowledged => PossibleKinds::default(),
                Phase::Uncertain(possible) => possible,
                Phase::InFlight { .. } => {
                    claim = Err(DiscordError::model(
                        "interaction was already acknowledged: an initial response is in flight",
                    ));
                    return false;
                }
                Phase::Acknowledged(_) => return false,
            };
            *phase = Phase::InFlight {
                claimed: kind,
                prior,
                original: None,
            };
            claim = Ok(prior);
            true
        });
        claim.map(|prior| InitialResponseClaim {
            state: self.clone(),
            claimed: kind,
            prior,
            settled: false,
        })
    }

    /// Waits until no initial response is in flight and returns the phase.
    ///
    /// Not bounded by a timeout: it returns once the in-flight future
    /// completes or is dropped (see [`InteractionResponsePhase::InFlight`]).
    async fn settled(&self) -> Phase {
        let mut receiver = self.0.subscribe();
        let settled = receiver
            .wait_for(|phase| !matches!(phase, Phase::InFlight { .. }))
            .await
            .map(|phase| *phase);
        // `wait_for` fails only once the sender is gone, which cannot happen
        // while `self` holds it; fall back to the current value regardless.
        settled.unwrap_or_else(|_| self.load())
    }

    /// Waits out an in-flight initial response, then errors when Discord is
    /// known not to hold one. `Uncertain` passes: Discord decides.
    pub(crate) async fn require_acknowledged(&self) -> Result<(), DiscordError> {
        match self.settled().await {
            Phase::Unacknowledged => {
                Err(DiscordError::model("interaction has not been acknowledged"))
            }
            _ => Ok(()),
        }
    }

    /// Records that an `@original` operation succeeded, which proves Discord
    /// holds an initial response; an edit also turns a deferred response
    /// into a reply (as in discord.js).
    ///
    /// While a retry is in flight the evidence is stored in the in-flight
    /// phase and applied when the retry settles, so a rejected or ambiguous
    /// retry cannot discard it. Follow-up successes deliberately do not count
    /// as evidence: only `@original` is unambiguous proof of the initial
    /// response.
    pub(crate) fn confirm_original(&self, seen: OriginalSeen) {
        self.0.send_if_modified(|phase| {
            let next = match *phase {
                Phase::InFlight {
                    claimed,
                    prior,
                    original,
                } => Phase::InFlight {
                    claimed,
                    prior,
                    original: Some(original.map_or(seen, |earlier| earlier.max(seen))),
                },
                settled => settled.with_original(seen),
            };
            if next == *phase {
                return false;
            }
            *phase = next;
            true
        });
    }

    /// Claims the initial response for the inline HTTP response of the
    /// interactions endpoint. Returns false, changing nothing, when a REST
    /// response already claimed it (sent, in flight, or uncertain); the
    /// endpoint must then answer Discord with 202 and no body.
    ///
    /// Uses the same compare-and-set as [`Self::begin`], so the inline and
    /// REST transports cannot both own the initial response. The inline
    /// response has no observable delivery outcome — it is the body of
    /// Discord's own request — so it moves straight to acknowledged.
    #[cfg(any(feature = "interactions", test))]
    pub(crate) fn commit_inline(&self, kind: AckKind) -> bool {
        self.0.send_if_modified(|phase| {
            if *phase == Phase::Unacknowledged {
                *phase = Phase::Acknowledged(Some(kind));
                true
            } else {
                false
            }
        })
    }
}

/// What one initial-response attempt tells us about Discord's state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CallbackOutcome {
    /// Discord accepted the callback.
    Accepted,
    /// Discord answered 40060: some initial response already exists.
    AlreadyAcknowledged,
    /// Discord did not apply the callback.
    Rejected,
    /// Discord may or may not have applied the callback.
    Ambiguous,
}

impl CallbackOutcome {
    pub(crate) fn of<T>(result: &Result<T, DiscordError>) -> Self {
        match result {
            Ok(_) => Self::Accepted,
            Err(error) => Self::of_error(error),
        }
    }

    fn of_error(error: &DiscordError) -> Self {
        match error {
            DiscordError::Api {
                code: Some(ALREADY_ACKNOWLEDGED_CODE),
                ..
            } => Self::AlreadyAcknowledged,
            // A 4xx means Discord (or the proxy in front of it) refused the
            // request; a 5xx may follow an applied request.
            DiscordError::Api { status, .. } if (400..500).contains(status) => Self::Rejected,
            // 429s and local validation (bad token path segment, missing
            // application id) never reach the callback handler. A connect
            // failure proves nothing was delivered on the redirect-free
            // transport the crate builds (see the decision log for clients
            // supplied through `RestClientBuilder::use_client`).
            DiscordError::RateLimit { .. } | DiscordError::Model { .. } => Self::Rejected,
            DiscordError::Http(http_error) if http_error.is_connect() => Self::Rejected,
            // Timeouts, failed body reads, and anything unrecognised: the
            // conservative answer is "may have happened".
            _ => Self::Ambiguous,
        }
    }
}

/// One claimed initial-response attempt. Settles the shared state exactly
/// once: through [`Self::settle`], or — when the sending future is dropped
/// mid-flight — through `Drop`, which records the attempt as possibly
/// delivered because the request may already have reached Discord.
pub(crate) struct InitialResponseClaim {
    state: InteractionResponseState,
    claimed: AckKind,
    prior: PossibleKinds,
    settled: bool,
}

impl InitialResponseClaim {
    pub(crate) fn settle(mut self, outcome: CallbackOutcome) {
        self.settled = true;
        self.finish(outcome);
    }

    fn finish(&self, outcome: CallbackOutcome) {
        let next = match outcome {
            CallbackOutcome::Accepted => Phase::Acknowledged(Some(self.claimed)),
            // This attempt was refused because an earlier one landed — one
            // of the uncertain attempts, or a response sent outside this
            // state. 40060 does not say which, so the kind is known only if
            // a single kind was possible.
            CallbackOutcome::AlreadyAcknowledged => Phase::Acknowledged(self.prior.only()),
            CallbackOutcome::Rejected if self.prior.is_empty() => Phase::Unacknowledged,
            CallbackOutcome::Rejected => Phase::Uncertain(self.prior),
            CallbackOutcome::Ambiguous => Phase::Uncertain(self.prior.with(self.claimed)),
        };
        self.state.0.send_if_modified(|phase| {
            let Phase::InFlight { original, .. } = *phase else {
                return false;
            };
            *phase = match original {
                Some(seen) => next.with_original(seen),
                None => next,
            };
            true
        });
    }
}

impl Drop for InitialResponseClaim {
    fn drop(&mut self) {
        if !self.settled {
            self.finish(CallbackOutcome::Ambiguous);
        }
    }
}

/// discord.js-style response methods, implemented once on the shared
/// interaction context and re-exposed on each variant through
/// [`InteractionResponder`].
///
/// Initial responses claim the shared [`InteractionResponseState`] before
/// the HTTP call and settle it from the outcome; follow-ups and `@original`
/// operations wait for an in-flight initial response. See
/// [`InteractionResponsePhase`].
impl InteractionContextData {
    /// Returns the current phase of the initial response.
    pub fn response_phase(&self) -> InteractionResponsePhase {
        self.response_state.phase()
    }

    /// Sends the initial reply (type 4, `CHANNEL_MESSAGE_WITH_SOURCE`).
    ///
    /// Errors with `DiscordError::Model` when the interaction was already
    /// acknowledged (replied, deferred, or shown a modal) or another initial
    /// response is in flight.
    pub async fn reply(
        &self,
        http: &RestClient,
        data: impl Into<InteractionMessage>,
    ) -> Result<(), DiscordError> {
        let response = InteractionResponse::ChannelMessage(data.into());
        self.send_initial_response(http, &response, &[]).await
    }

    /// Sends the initial reply as an ephemeral message (flag `1 << 6`).
    pub async fn reply_ephemeral(
        &self,
        http: &RestClient,
        data: impl Into<InteractionMessage>,
    ) -> Result<(), DiscordError> {
        let response = InteractionResponse::ChannelMessage(data.into().ephemeral(true));
        self.send_initial_response(http, &response, &[]).await
    }

    /// Sends the initial reply (type 4) with file uploads. Reference each
    /// upload from the message's `attachments`
    /// ([`MessageAttachment`](crate::request::MessageAttachment)).
    pub async fn reply_with_files(
        &self,
        http: &RestClient,
        data: impl Into<InteractionMessage>,
        files: &[FileAttachment],
    ) -> Result<(), DiscordError> {
        let response = InteractionResponse::ChannelMessage(data.into());
        self.send_initial_response(http, &response, files).await
    }

    /// Sends the initial reply with `with_response=true` and returns the
    /// typed callback result, mirroring discord.js's `withResponse: true`.
    ///
    /// When Discord accepts the reply but the returned resource cannot be
    /// decoded, the error is returned and the interaction still counts as
    /// replied.
    pub async fn reply_with_result(
        &self,
        http: &RestClient,
        data: impl Into<InteractionMessage>,
    ) -> Result<InteractionCallbackResult, DiscordError> {
        let response = InteractionResponse::ChannelMessage(data.into());
        let body = http.prepare_interaction_callback(&response, &[])?;
        let claim = self.response_state.begin(AckKind::Replied)?;
        let result = http
            .post_interaction_callback_with_result(self.id, &self.token, body)
            .await;
        // The body was encoded before the claim, so a JSON error here can
        // only come from decoding a 2xx response: Discord accepted it.
        let outcome = match &result {
            Err(DiscordError::Json(_)) => CallbackOutcome::Accepted,
            other => CallbackOutcome::of(other),
        };
        claim.settle(outcome);
        result
    }

    /// Defers the reply (type 5), showing a public "thinking" state.
    pub async fn defer(&self, http: &RestClient) -> Result<(), DiscordError> {
        self.send_initial_response(http, &InteractionResponse::defer(false), &[])
            .await
    }

    /// Defers the reply (type 5) with an ephemeral "thinking" state.
    pub async fn defer_ephemeral(&self, http: &RestClient) -> Result<(), DiscordError> {
        self.send_initial_response(http, &InteractionResponse::defer(true), &[])
            .await
    }

    /// Acknowledges without any visible response (type 6). Valid only for
    /// component and modal-submit interactions.
    pub async fn defer_update(&self, http: &RestClient) -> Result<(), DiscordError> {
        self.send_initial_response(http, &InteractionResponse::DeferredUpdateMessage, &[])
            .await
    }

    /// Edits the message the component was attached to (type 7). Valid only
    /// for component interactions (and modals launched from one).
    pub async fn update_message(
        &self,
        http: &RestClient,
        data: impl Into<UpdateMessage>,
    ) -> Result<(), DiscordError> {
        let response = InteractionResponse::UpdateMessage(data.into());
        self.send_initial_response(http, &response, &[]).await
    }

    /// Opens a modal (type 9): a [`Modal`] or a
    /// [`ModalBuilder`](crate::builders::ModalBuilder). Discord accepts this
    /// only as the initial response of command and component interactions.
    pub async fn show_modal(
        &self,
        http: &RestClient,
        modal: impl Into<Modal>,
    ) -> Result<(), DiscordError> {
        self.send_initial_response(http, &InteractionResponse::Modal(modal.into()), &[])
            .await
    }

    /// Sends autocomplete choices (type 8). Valid only for autocomplete
    /// interactions; errors if the interaction was already responded to.
    pub async fn respond_autocomplete(
        &self,
        http: &RestClient,
        choices: Vec<ApplicationCommandOptionChoice>,
    ) -> Result<(), DiscordError> {
        self.send_initial_response(http, &InteractionResponse::AutocompleteResult(choices), &[])
            .await
    }

    /// Sends any initial response callback through the shared response
    /// state, for callback types without a dedicated method (for example
    /// `LAUNCH_ACTIVITY`, type 12, or a [`Raw`](InteractionResponse::Raw)
    /// callback).
    ///
    /// Types 5 and 6 record a deferred response, `PONG` (type 1) is sent
    /// without touching the state, and every other type records a reply.
    pub async fn create_response(
        &self,
        http: &RestClient,
        response: &InteractionResponse,
    ) -> Result<(), DiscordError> {
        self.send_initial_response(http, response, &[]).await
    }

    /// Edits the original response (PATCH `@original`) and returns the
    /// resulting message.
    ///
    /// Waits for an in-flight initial response, then errors with
    /// `DiscordError::Model` when the interaction is not acknowledged.
    /// Editing a deferred reply promotes the state to replied, as in
    /// discord.js.
    pub async fn edit_reply(
        &self,
        http: &RestClient,
        data: impl Into<EditInteractionResponse>,
    ) -> Result<Message, DiscordError> {
        self.edit_reply_with_files(http, data, &[]).await
    }

    /// [`Self::edit_reply`] with file uploads. The edit's `attachments`
    /// must list every attachment the message keeps, the new uploads
    /// included.
    pub async fn edit_reply_with_files(
        &self,
        http: &RestClient,
        data: impl Into<EditInteractionResponse>,
        files: &[FileAttachment],
    ) -> Result<Message, DiscordError> {
        let data = data.into();
        self.response_state.require_acknowledged().await?;
        let message = if files.is_empty() {
            http.edit_original_interaction_response_with_application_id(
                self.application_id,
                &self.token,
                &data,
            )
            .await?
        } else {
            http.edit_original_interaction_response_with_application_id_and_files(
                self.application_id,
                &self.token,
                &data,
                files,
            )
            .await?
        };
        self.response_state.confirm_original(OriginalSeen::Edited);
        Ok(message)
    }

    /// Fetches the original response message (GET `@original`), waiting
    /// for an in-flight initial response first.
    pub async fn fetch_reply(&self, http: &RestClient) -> Result<Message, DiscordError> {
        self.response_state.settled().await;
        let message = http
            .get_original_interaction_response_with_application_id(self.application_id, &self.token)
            .await?;
        self.response_state.confirm_original(OriginalSeen::Exists);
        Ok(message)
    }

    /// Deletes the original response message.
    ///
    /// Waits for an in-flight initial response, then errors with
    /// `DiscordError::Model` when the interaction is not acknowledged.
    pub async fn delete_reply(&self, http: &RestClient) -> Result<(), DiscordError> {
        self.response_state.require_acknowledged().await?;
        http.delete_original_interaction_response_with_application_id(
            self.application_id,
            &self.token,
        )
        .await?;
        self.response_state.confirm_original(OriginalSeen::Exists);
        Ok(())
    }

    /// Sends a follow-up message and returns it.
    ///
    /// Waits for an in-flight initial response, then errors with
    /// `DiscordError::Model` when the interaction is not acknowledged.
    pub async fn follow_up(
        &self,
        http: &RestClient,
        data: impl Into<CreateFollowup>,
    ) -> Result<Message, DiscordError> {
        self.follow_up_with_files(http, data, &[]).await
    }

    /// Sends an ephemeral follow-up message and returns it.
    pub async fn follow_up_ephemeral(
        &self,
        http: &RestClient,
        data: impl Into<CreateFollowup>,
    ) -> Result<Message, DiscordError> {
        self.follow_up(http, data.into().ephemeral(true)).await
    }

    /// [`Self::follow_up`] with file uploads. Reference each upload from the
    /// message's `attachments`.
    pub async fn follow_up_with_files(
        &self,
        http: &RestClient,
        data: impl Into<CreateFollowup>,
        files: &[FileAttachment],
    ) -> Result<Message, DiscordError> {
        let data = data.into();
        self.response_state.require_acknowledged().await?;
        if files.is_empty() {
            http.create_followup_message_with_application_id(
                self.application_id,
                &self.token,
                &data,
            )
            .await
        } else {
            http.create_followup_message_with_application_id_and_files(
                self.application_id,
                &self.token,
                &data,
                files,
            )
            .await
        }
    }

    /// Fetches a follow-up message sent for this interaction.
    ///
    /// Waits for an in-flight initial response, then errors with
    /// `DiscordError::Model` when the interaction is not acknowledged (it
    /// cannot have follow-ups before that).
    pub async fn fetch_follow_up(
        &self,
        http: &RestClient,
        message_id: MessageId,
    ) -> Result<Message, DiscordError> {
        self.response_state.require_acknowledged().await?;
        http.get_followup_message_with_application_id(
            self.application_id,
            &self.token,
            &message_id.to_string(),
        )
        .await
    }

    /// Edits a follow-up message sent for this interaction.
    ///
    /// Waits for an in-flight initial response, then errors with
    /// `DiscordError::Model` when the interaction is not acknowledged.
    pub async fn edit_follow_up(
        &self,
        http: &RestClient,
        message_id: MessageId,
        data: impl Into<EditInteractionResponse>,
    ) -> Result<Message, DiscordError> {
        let data = data.into();
        self.response_state.require_acknowledged().await?;
        http.edit_followup_message_with_application_id(
            self.application_id,
            &self.token,
            &message_id.to_string(),
            &data,
        )
        .await
    }

    /// Deletes a follow-up message sent for this interaction.
    ///
    /// Waits for an in-flight initial response, then errors with
    /// `DiscordError::Model` when the interaction is not acknowledged.
    pub async fn delete_follow_up(
        &self,
        http: &RestClient,
        message_id: MessageId,
    ) -> Result<(), DiscordError> {
        self.response_state.require_acknowledged().await?;
        http.delete_followup_message_with_application_id(
            self.application_id,
            &self.token,
            &message_id.to_string(),
        )
        .await
    }

    /// Prepares the callback body, claims the initial-response slot, posts
    /// the callback, and settles the slot from the outcome (see
    /// [`CallbackOutcome`]). `PONG` never acknowledges an interaction, so it
    /// is posted without touching the slot.
    ///
    /// Preparing comes first: encoding and the local file checks (empty
    /// name, invalid content type) fail without claiming anything, so every
    /// error after the claim comes from sending. A local error after the
    /// claim would otherwise be classified as a possibly delivered response
    /// (`Uncertain`), and the HTTP endpoint would then discard the handler's
    /// inline fallback and answer 202.
    async fn send_initial_response(
        &self,
        http: &RestClient,
        response: &InteractionResponse,
        files: &[FileAttachment],
    ) -> Result<(), DiscordError> {
        let body = http.prepare_interaction_callback(response, files)?;
        let Some(kind) = AckKind::for_callback_type(response.kind()) else {
            return http
                .post_interaction_callback(self.id, &self.token, body)
                .await;
        };
        let claim = self.response_state.begin(kind)?;
        let result = http
            .post_interaction_callback(self.id, &self.token, body)
            .await;
        claim.settle(CallbackOutcome::of(&result));
        result
    }
}

/// discord.js-style responder methods for interaction variants that can send
/// an initial response.
///
/// Every method delegates to the shared [`InteractionContextData`], so the
/// response state is shared between clones of one interaction.
///
/// # `Send` contract
///
/// Every async method returns a `Send` future, also when called through a
/// generic `R: InteractionResponder`, so a responder call can be moved into
/// `tokio::spawn`. Implementors only provide [`Self::responder_context`];
/// an override must return a `Send` future as well.
///
/// # Waiting for an in-flight initial response
///
/// Follow-up and `@original` methods called while another clone is sending
/// the initial response wait for that response's outcome. The wait has no
/// timeout of its own; it ends when the sending future completes or is
/// dropped. Do not keep a partially polled initial-response future alive in
/// the same task while awaiting a follow-up (for example after a `select!`
/// branch lost): drop it or keep polling it, otherwise the follow-up waits
/// forever. See [`InteractionResponsePhase::InFlight`].
pub trait InteractionResponder {
    /// The shared context (id, token, response state) used for responding.
    fn responder_context(&self) -> &InteractionContextData;

    /// Returns true when Discord accepted a non-deferred initial response
    /// or a deferred response was edited.
    fn is_replied(&self) -> bool {
        self.responder_context().is_replied()
    }

    /// Returns true when the interaction was deferred and not yet replied to.
    fn is_deferred(&self) -> bool {
        self.responder_context().is_deferred()
    }

    /// Returns true when Discord confirmed an initial response of any kind.
    fn is_acknowledged(&self) -> bool {
        self.responder_context().is_acknowledged()
    }

    /// Returns the current phase of the initial response, including the
    /// in-flight and uncertain phases the boolean accessors do not show.
    fn response_phase(&self) -> InteractionResponsePhase {
        self.responder_context().response_phase()
    }

    /// Sends the initial reply (type 4). See [`InteractionContextData::reply`].
    fn reply(
        &self,
        http: &RestClient,
        data: impl Into<InteractionMessage>,
    ) -> impl Future<Output = Result<(), DiscordError>> + Send {
        self.responder_context().reply(http, data.into())
    }

    /// Sends the initial reply as an ephemeral message.
    fn reply_ephemeral(
        &self,
        http: &RestClient,
        data: impl Into<InteractionMessage>,
    ) -> impl Future<Output = Result<(), DiscordError>> + Send {
        self.responder_context().reply_ephemeral(http, data.into())
    }

    /// Sends the initial reply and returns the typed callback result.
    fn reply_with_result(
        &self,
        http: &RestClient,
        data: impl Into<InteractionMessage>,
    ) -> impl Future<Output = Result<InteractionCallbackResult, DiscordError>> + Send {
        self.responder_context()
            .reply_with_result(http, data.into())
    }

    /// Defers the reply (type 5).
    fn defer(&self, http: &RestClient) -> impl Future<Output = Result<(), DiscordError>> + Send {
        self.responder_context().defer(http)
    }

    /// Defers the reply (type 5) with an ephemeral "thinking" state.
    fn defer_ephemeral(
        &self,
        http: &RestClient,
    ) -> impl Future<Output = Result<(), DiscordError>> + Send {
        self.responder_context().defer_ephemeral(http)
    }

    /// Sends the initial reply (type 4) with file uploads. See
    /// [`InteractionContextData::reply_with_files`].
    fn reply_with_files(
        &self,
        http: &RestClient,
        data: impl Into<InteractionMessage>,
        files: &[FileAttachment],
    ) -> impl Future<Output = Result<(), DiscordError>> + Send {
        self.responder_context()
            .reply_with_files(http, data.into(), files)
    }

    /// Edits the original response and returns the resulting message.
    fn edit_reply(
        &self,
        http: &RestClient,
        data: impl Into<EditInteractionResponse>,
    ) -> impl Future<Output = Result<Message, DiscordError>> + Send {
        self.responder_context().edit_reply(http, data.into())
    }

    /// Edits the original response with file uploads. See
    /// [`InteractionContextData::edit_reply_with_files`].
    fn edit_reply_with_files(
        &self,
        http: &RestClient,
        data: impl Into<EditInteractionResponse>,
        files: &[FileAttachment],
    ) -> impl Future<Output = Result<Message, DiscordError>> + Send {
        self.responder_context()
            .edit_reply_with_files(http, data.into(), files)
    }

    /// Fetches the original response message.
    fn fetch_reply(
        &self,
        http: &RestClient,
    ) -> impl Future<Output = Result<Message, DiscordError>> + Send {
        self.responder_context().fetch_reply(http)
    }

    /// Deletes the original response message.
    fn delete_reply(
        &self,
        http: &RestClient,
    ) -> impl Future<Output = Result<(), DiscordError>> + Send {
        self.responder_context().delete_reply(http)
    }

    /// Sends a follow-up message and returns it.
    fn follow_up(
        &self,
        http: &RestClient,
        data: impl Into<CreateFollowup>,
    ) -> impl Future<Output = Result<Message, DiscordError>> + Send {
        self.responder_context().follow_up(http, data.into())
    }

    /// Sends an ephemeral follow-up message and returns it.
    fn follow_up_ephemeral(
        &self,
        http: &RestClient,
        data: impl Into<CreateFollowup>,
    ) -> impl Future<Output = Result<Message, DiscordError>> + Send {
        self.responder_context()
            .follow_up_ephemeral(http, data.into())
    }

    /// Sends a follow-up message with file uploads. See
    /// [`InteractionContextData::follow_up_with_files`].
    fn follow_up_with_files(
        &self,
        http: &RestClient,
        data: impl Into<CreateFollowup>,
        files: &[FileAttachment],
    ) -> impl Future<Output = Result<Message, DiscordError>> + Send {
        self.responder_context()
            .follow_up_with_files(http, data.into(), files)
    }

    /// Fetches a follow-up message sent for this interaction.
    fn fetch_follow_up(
        &self,
        http: &RestClient,
        message_id: MessageId,
    ) -> impl Future<Output = Result<Message, DiscordError>> + Send {
        self.responder_context().fetch_follow_up(http, message_id)
    }

    /// Edits a follow-up message sent for this interaction.
    fn edit_follow_up(
        &self,
        http: &RestClient,
        message_id: MessageId,
        data: impl Into<EditInteractionResponse>,
    ) -> impl Future<Output = Result<Message, DiscordError>> + Send {
        self.responder_context()
            .edit_follow_up(http, message_id, data.into())
    }

    /// Deletes a follow-up message sent for this interaction.
    fn delete_follow_up(
        &self,
        http: &RestClient,
        message_id: MessageId,
    ) -> impl Future<Output = Result<(), DiscordError>> + Send {
        self.responder_context().delete_follow_up(http, message_id)
    }

    /// Opens a modal (type 9). Discord accepts this only for command and
    /// component interactions.
    fn show_modal(
        &self,
        http: &RestClient,
        modal: impl Into<Modal>,
    ) -> impl Future<Output = Result<(), DiscordError>> + Send {
        self.responder_context().show_modal(http, modal.into())
    }
}

impl InteractionResponder for ChatInputCommandInteraction {
    fn responder_context(&self) -> &InteractionContextData {
        &self.context
    }
}

impl InteractionResponder for UserContextMenuInteraction {
    fn responder_context(&self) -> &InteractionContextData {
        &self.context
    }
}

impl InteractionResponder for MessageContextMenuInteraction {
    fn responder_context(&self) -> &InteractionContextData {
        &self.context
    }
}

impl InteractionResponder for PrimaryEntryPointInteraction {
    fn responder_context(&self) -> &InteractionContextData {
        &self.context
    }
}

impl InteractionResponder for ComponentInteraction {
    fn responder_context(&self) -> &InteractionContextData {
        &self.context
    }
}

impl InteractionResponder for ModalSubmitInteraction {
    fn responder_context(&self) -> &InteractionContextData {
        &self.context
    }
}

impl ComponentInteraction {
    /// Acknowledges the component interaction without a visible response
    /// (type 6).
    pub async fn defer_update(&self, http: &RestClient) -> Result<(), DiscordError> {
        self.context.defer_update(http).await
    }

    /// Edits the message the component is attached to (type 7).
    pub async fn update_message(
        &self,
        http: &RestClient,
        data: impl Into<UpdateMessage>,
    ) -> Result<(), DiscordError> {
        self.context.update_message(http, data).await
    }
}

impl ModalSubmitInteraction {
    /// Acknowledges the modal submission without a visible response (type 6).
    pub async fn defer_update(&self, http: &RestClient) -> Result<(), DiscordError> {
        self.context.defer_update(http).await
    }

    /// Edits the message the modal was launched from (type 7). Valid only
    /// when the modal originated from a component interaction.
    pub async fn update_message(
        &self,
        http: &RestClient,
        data: impl Into<UpdateMessage>,
    ) -> Result<(), DiscordError> {
        self.context.update_message(http, data).await
    }
}

impl AutocompleteInteraction {
    /// Sends autocomplete choices (type 8); errors if this interaction was
    /// already responded to.
    pub async fn respond_autocomplete(
        &self,
        http: &RestClient,
        choices: Vec<ApplicationCommandOptionChoice>,
    ) -> Result<(), DiscordError> {
        self.context.respond_autocomplete(http, choices).await
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::model::{
        test_id, AllowedMentions, ChatInputCommandInteraction, ComponentInteraction,
        InteractionContextData, InteractionResponseType, InteractionType, ModalSubmitInteraction,
    };
    use crate::request::{CreatePoll, Embed, InteractionMessage, UpdateMessage};
    use std::collections::HashMap;
    use std::time::Duration;

    use serde_json::json;

    use crate::builders::{ComponentsV2Message, ModalBuilder, TextInputBuilder};
    use crate::constants::MESSAGE_FLAG_IS_COMPONENTS_V2;
    use crate::error::DiscordError;
    use crate::http::{FileAttachment, RestClient};

    #[expect(deprecated, reason = "covers the deprecated builder")]
    use super::InteractionResponseBuilder;
    use super::{
        AckKind, CallbackOutcome, InteractionResponder, InteractionResponsePhase,
        InteractionResponseState, MessageBuilder, OriginalSeen, Phase, PossibleKinds,
    };
    use crate::model::ApplicationCommandOptionChoice;
    use crate::request::InteractionResponse;

    /// The callback as Discord receives it.
    fn wire(response: &InteractionResponse) -> serde_json::Value {
        serde_json::to_value(response).unwrap()
    }

    const MESSAGE_JSON: &str = r#"{"id":"30","channel_id":"40","content":"ok"}"#;
    const ALREADY_ACKNOWLEDGED_JSON: &str =
        r#"{"code":40060,"message":"Interaction has already been acknowledged."}"#;

    fn sample_context() -> InteractionContextData {
        InteractionContextData {
            id: test_id(1),
            application_id: test_id(2),
            token: "token".to_string(),
            ..InteractionContextData::fixture()
        }
    }

    /// Context whose token fails path validation, so every REST call errors
    /// locally after the response-state checks and nothing reaches the
    /// network.
    fn offline_context() -> InteractionContextData {
        InteractionContextData {
            token: "bad/token".to_string(),
            ..sample_context()
        }
    }

    /// Client that is never allowed to reach the network; every test using it
    /// asserts an error is returned before any HTTP request is attempted.
    fn offline_client() -> RestClient {
        RestClient::new("test-token", Some(test_id(2)))
    }

    fn mock_client(server: &mock::Server, timeout: Duration) -> RestClient {
        RestClient::builder("bot-token", Some(test_id(2)))
            .api_base(server.base_url.clone())
            .request_timeout(timeout)
            .build()
            .unwrap()
    }

    fn assert_model_error(error: &DiscordError, expected: &str) {
        assert!(
            matches!(error, DiscordError::Model { message } if message.contains(expected)),
            "expected model error containing `{expected}`, got {error:?}"
        );
    }

    /// Scripted stand-in for Discord's HTTP API: serves one scripted reply
    /// per request (each on its own connection) and reports every request as
    /// it arrives, so tests can see exactly when the client sent something.
    /// Shared with the helpers tests (`crate::helpers`), which drive the
    /// same response paths through their id-and-token wrappers.
    pub(crate) mod mock {
        use std::collections::VecDeque;
        use std::sync::{Arc, Mutex};

        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::{TcpListener, TcpStream};
        use tokio::sync::{mpsc, oneshot};
        use tokio::task::JoinHandle;

        pub(crate) enum Reply {
            /// Respond at once with this status and JSON body.
            Now(u16, &'static str),
            /// Respond once the test releases the reply.
            Held(oneshot::Receiver<(u16, &'static str)>),
            /// Announce a 200 body longer than what is sent, then close.
            Truncated,
            /// Keep the connection open without ever responding.
            Silent,
        }

        pub(crate) struct Server {
            pub(crate) base_url: String,
            pub(crate) arrivals: mpsc::UnboundedReceiver<(String, Vec<u8>)>,
            accept_task: JoinHandle<()>,
        }

        impl Server {
            /// Waits for the next request and returns `"METHOD /path"`.
            pub(crate) async fn next_request(&mut self) -> String {
                self.next_request_with_body().await.0
            }

            /// Waits for the next request and returns `"METHOD /path"` and
            /// its body bytes.
            pub(crate) async fn next_request_with_body(&mut self) -> (String, Vec<u8>) {
                self.arrivals.recv().await.expect("mock server running")
            }

            pub(crate) fn has_pending_request(&mut self) -> bool {
                !self.arrivals.is_empty()
            }
        }

        impl Drop for Server {
            fn drop(&mut self) {
                self.accept_task.abort();
            }
        }

        pub(crate) async fn spawn(script: Vec<Reply>) -> Server {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let base_url = format!("http://{}", listener.local_addr().unwrap());
            let script = Arc::new(Mutex::new(VecDeque::from(script)));
            let (arrival_tx, arrivals) = mpsc::unbounded_channel();
            let accept_task = tokio::spawn(async move {
                while let Ok((stream, _)) = listener.accept().await {
                    tokio::spawn(serve(stream, Arc::clone(&script), arrival_tx.clone()));
                }
            });
            Server {
                base_url,
                arrivals,
                accept_task,
            }
        }

        async fn serve(
            mut stream: TcpStream,
            script: Arc<Mutex<VecDeque<Reply>>>,
            arrivals: mpsc::UnboundedSender<(String, Vec<u8>)>,
        ) {
            let Some(request) = read_request(&mut stream).await else {
                return;
            };
            let reply = script.lock().unwrap().pop_front();
            let _ = arrivals.send(request);
            let (status, body) = match reply {
                Some(Reply::Now(status, body)) => (status, body),
                Some(Reply::Held(release)) => match release.await {
                    Ok(reply) => reply,
                    Err(_) => return,
                },
                Some(Reply::Truncated) => {
                    let _ = stream
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
                              Content-Length: 64\r\nConnection: close\r\n\r\n{\"id\"",
                        )
                        .await;
                    return;
                }
                Some(Reply::Silent) => {
                    std::future::pending::<()>().await;
                    return;
                }
                None => (500, r#"{"message":"unscripted request"}"#),
            };
            let head = if status == 204 {
                "HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n".to_string()
            } else {
                format!(
                    "HTTP/1.1 {status} Mock\r\nContent-Type: application/json\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
            };
            let _ = stream.write_all(head.as_bytes()).await;
            let _ = stream.write_all(body.as_bytes()).await;
            let _ = stream.shutdown().await;
        }

        async fn read_request(stream: &mut TcpStream) -> Option<(String, Vec<u8>)> {
            let mut buffer = Vec::new();
            let mut chunk = [0_u8; 4096];
            loop {
                if let Some(end) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&buffer[..end]).into_owned();
                    let length = head
                        .lines()
                        .filter_map(|line| line.split_once(':'))
                        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
                        .unwrap_or(0);
                    while buffer.len() < end + 4 + length {
                        let read = stream.read(&mut chunk).await.ok()?;
                        if read == 0 {
                            return None;
                        }
                        buffer.extend_from_slice(&chunk[..read]);
                    }
                    let mut request_line = head.lines().next()?.split_whitespace();
                    let line = format!("{} {}", request_line.next()?, request_line.next()?);
                    return Some((line, buffer[end + 4..end + 4 + length].to_vec()));
                }
                let read = stream.read(&mut chunk).await.ok()?;
                if read == 0 {
                    return None;
                }
                buffer.extend_from_slice(&chunk[..read]);
            }
        }
    }

    /// Yields until a task is parked waiting on `state` (it subscribed to the
    /// watch channel), without relying on wall-clock timing.
    async fn wait_for_waiter(state: &InteractionResponseState) {
        while state.0.receiver_count() == 0 {
            tokio::task::yield_now().await;
        }
    }

    // --- payload builders ---------------------------------------------------

    #[test]
    fn message_builder_sets_ephemeral_components_v2_flags() {
        let message = MessageBuilder::new()
            .components_v2(ComponentsV2Message::new())
            .ephemeral(true)
            .build();

        assert_eq!(
            message.flags.map(|flags| flags.bits()),
            Some((1 << 15) | (1 << 6))
        );
    }

    #[test]
    fn message_builder_serializes_components_and_preserves_existing_flags() {
        let message = MessageBuilder::new()
            .content("hello")
            .add_component(
                crate::request::RawComponent::from_value(&json!({"type": 1, "components": []}))
                    .unwrap(),
            )
            .flags(1 << 3)
            .ephemeral(true)
            .build();

        let value = serde_json::to_value(message).unwrap();
        assert_eq!(value["content"], json!("hello"));
        assert_eq!(value["components"][0]["type"], json!(1));
        assert_eq!(value["flags"], json!((1 << 3) | (1 << 6)));
    }

    #[test]
    #[expect(deprecated, reason = "covers the deprecated builder")]
    fn interaction_response_builder_wraps_message_payload() {
        let response =
            InteractionResponseBuilder::channel_message(MessageBuilder::new().content("hello"))
                .unwrap()
                .build();

        assert_eq!(
            response.kind(),
            InteractionResponseType::CHANNEL_MESSAGE_WITH_SOURCE
        );
        assert_eq!(wire(&response)["data"]["content"], "hello");
    }

    #[test]
    #[expect(deprecated, reason = "covers the deprecated builder")]
    fn interaction_response_builder_serializes_deferred_channel_message_flags() {
        let public_response = InteractionResponseBuilder::deferred_channel_message(false).build();
        let ephemeral_response = InteractionResponseBuilder::deferred_channel_message(true).build();

        assert_eq!(
            public_response.kind(),
            InteractionResponseType::DEFERRED_CHANNEL_MESSAGE_WITH_SOURCE
        );
        assert_eq!(wire(&public_response)["data"]["flags"], json!(0));
        assert_eq!(
            ephemeral_response.kind(),
            InteractionResponseType::DEFERRED_CHANNEL_MESSAGE_WITH_SOURCE
        );
        assert_eq!(wire(&ephemeral_response)["data"]["flags"], json!(1 << 6));
    }

    #[test]
    #[expect(deprecated, reason = "covers the deprecated builder")]
    fn interaction_response_builder_wraps_update_message_payload() {
        let response = InteractionResponseBuilder::update_message(
            MessageBuilder::new()
                .content("updated")
                .components_v2(ComponentsV2Message::new()),
        )
        .unwrap()
        .build();

        let data = wire(&response)["data"].take();
        assert_eq!(response.kind(), InteractionResponseType::UPDATE_MESSAGE);
        assert_eq!(data["content"], json!("updated"));
        assert_eq!(data["components"], json!([]));
        assert_eq!(data["flags"], json!(MESSAGE_FLAG_IS_COMPONENTS_V2));
    }

    #[test]
    #[expect(deprecated, reason = "covers the deprecated builder")]
    fn interaction_response_builder_wraps_modal_payload() {
        let response = InteractionResponseBuilder::modal(ModalBuilder::new("feedback", "Feedback"))
            .unwrap()
            .build();

        let data = wire(&response)["data"].take();
        assert_eq!(response.kind(), InteractionResponseType::MODAL);
        assert_eq!(data["custom_id"], json!("feedback"));
        assert_eq!(data["title"], json!("Feedback"));
        assert_eq!(data["components"], json!([]));
    }

    /// A `MessageBuilder` converts into the callback message without
    /// losing a field.
    #[test]
    fn message_builder_fields_reach_the_callback_message() {
        let message = InteractionMessage::from(
            MessageBuilder::new()
                .content("built")
                .components_v2(
                    ComponentsV2Message::new()
                        .add_text_display(crate::builders::TextDisplayBuilder::new("one")),
                )
                .ephemeral(true),
        );
        let data = wire(&InteractionResponse::reply(message))["data"].take();
        assert_eq!(data["content"], json!("built"));
        assert_eq!(
            data["components"],
            json!([{ "type": 10, "content": "one" }])
        );
        assert_eq!(
            data["flags"],
            json!(MESSAGE_FLAG_IS_COMPONENTS_V2 | (1 << 6))
        );
    }

    /// Regression: 2.2.0 copied five fields and silently dropped `poll`. The
    /// callback writes the whole message, `allowed_mentions` and `poll`
    /// included.
    #[test]
    fn callback_messages_carry_every_interaction_field() {
        let message = InteractionMessage::new()
            .content("vote")
            .tts(false)
            .add_embed(Embed::new())
            .allowed_mentions(AllowedMentions::default())
            .poll(CreatePoll::new("Lunch?").add_answer("Pizza"))
            .ephemeral(true);
        let expected = serde_json::to_value(&message).unwrap();
        let callback = InteractionResponse::reply(message);
        assert_eq!(wire(&callback)["data"], expected);
        assert_eq!(
            expected["poll"]["question"]["text"],
            json!("Lunch?"),
            "poll must reach the callback body"
        );
        assert_eq!(expected["allowed_mentions"], json!({}));
    }

    #[test]
    fn message_callback_wraps_reply_and_update_payloads() {
        let reply = InteractionResponse::reply(InteractionMessage::from("hi").ephemeral(true));
        assert_eq!(
            reply.kind(),
            InteractionResponseType::CHANNEL_MESSAGE_WITH_SOURCE
        );
        let data = wire(&reply)["data"].take();
        assert_eq!(data["content"], json!("hi"));
        assert_eq!(data["flags"], json!(1 << 6));

        // `Some(vec![])` is written: UPDATE_MESSAGE reads it as "remove all".
        let update = InteractionResponse::update(UpdateMessage::new().components(Vec::new()));
        assert_eq!(update.kind(), InteractionResponseType::UPDATE_MESSAGE);
        assert_eq!(wire(&update)["data"], json!({ "components": [] }));
    }

    #[test]
    fn deferred_and_modal_callbacks_use_expected_kinds() {
        let public = InteractionResponse::defer(false);
        assert_eq!(
            public.kind(),
            InteractionResponseType::DEFERRED_CHANNEL_MESSAGE_WITH_SOURCE
        );
        assert_eq!(wire(&public)["data"]["flags"], json!(0));

        let ephemeral = InteractionResponse::defer(true);
        assert_eq!(wire(&ephemeral)["data"]["flags"], json!(1 << 6));

        let update = InteractionResponse::DeferredUpdateMessage;
        assert_eq!(
            update.kind(),
            InteractionResponseType::DEFERRED_UPDATE_MESSAGE
        );
        assert!(wire(&update).get("data").is_none());

        let modal = InteractionResponse::modal(ModalBuilder::new("feedback", "Feedback"));
        assert_eq!(modal.kind(), InteractionResponseType::MODAL);
        assert_eq!(wire(&modal)["data"]["custom_id"], json!("feedback"));
    }

    #[test]
    fn autocomplete_callback_serializes_choices_and_skips_missing_localizations() {
        let mut localizations = HashMap::new();
        localizations.insert("de".to_string(), "Abrechnung".to_string());

        let mut localized = ApplicationCommandOptionChoice::new("priority", 2);
        localized.name_localizations = Some(localizations);
        let response = InteractionResponse::autocomplete([
            ApplicationCommandOptionChoice::new("billing", "billing-value"),
            localized,
        ]);

        assert_eq!(
            response.kind(),
            InteractionResponseType::APPLICATION_COMMAND_AUTOCOMPLETE_RESULT
        );
        let data = wire(&response)["data"].take();
        let choices = data["choices"].as_array().unwrap();
        assert_eq!(choices.len(), 2);
        assert_eq!(choices[0]["name"], json!("billing"));
        assert_eq!(choices[0]["value"], json!("billing-value"));
        assert!(choices[0].get("name_localizations").is_none());
        assert_eq!(choices[1]["value"], json!(2));
        assert_eq!(choices[1]["name_localizations"]["de"], json!("Abrechnung"));
    }

    /// The inline HTTP answer and the REST callback compete for one slot.
    /// Whatever the interleaving, exactly one of them owns the initial
    /// response.
    #[test]
    fn inline_and_rest_claims_race_for_one_slot() {
        for _ in 0..64 {
            let state = InteractionResponseState::new();
            let (inline, rest): (Vec<bool>, Vec<bool>) = std::thread::scope(|scope| {
                let inline: Vec<_> = (0..4)
                    .map(|_| {
                        let state = &state;
                        scope.spawn(move || state.commit_inline(AckKind::Replied))
                    })
                    .collect();
                let rest: Vec<_> = (0..4)
                    .map(|_| {
                        let state = &state;
                        scope.spawn(move || {
                            state
                                .begin(AckKind::Deferred)
                                .map(|claim| claim.settle(CallbackOutcome::Accepted))
                                .is_ok()
                        })
                    })
                    .collect();
                (
                    inline.into_iter().map(|h| h.join().unwrap()).collect(),
                    rest.into_iter().map(|h| h.join().unwrap()).collect(),
                )
            });
            let winners = inline.iter().chain(rest.iter()).filter(|won| **won).count();
            assert_eq!(winners, 1, "inline {inline:?}, rest {rest:?}");
            assert!(state.is_acknowledged());
            if inline.contains(&true) {
                assert!(state.is_replied());
            } else {
                assert!(state.is_deferred());
            }
        }
    }

    /// The REST callback sends exactly the bytes the HTTP endpoint would
    /// answer with (one encoder), and a client without default mentions adds
    /// nothing.
    #[tokio::test]
    async fn rest_callbacks_send_the_endpoint_bytes() {
        let mut server = mock::spawn(vec![
            mock::Reply::Now(204, ""),
            mock::Reply::Now(204, ""),
            mock::Reply::Now(204, ""),
        ])
        .await;
        let http = mock_client(&server, Duration::from_secs(10));

        let reply = InteractionMessage::from("hi").ephemeral(true);
        let cases = [
            InteractionResponse::reply(reply.clone()),
            InteractionResponse::defer(true),
            InteractionResponse::modal(
                ModalBuilder::new("m", "M").add_text_input(TextInputBuilder::short("t", "T")),
            ),
        ];
        sample_context().reply(&http, reply).await.unwrap();
        sample_context().defer_ephemeral(&http).await.unwrap();
        sample_context()
            .show_modal(
                &http,
                ModalBuilder::new("m", "M").add_text_input(TextInputBuilder::short("t", "T")),
            )
            .await
            .unwrap();
        for expected in &cases {
            let (line, body) = server.next_request_with_body().await;
            assert_eq!(line, "POST /interactions/1/token/callback");
            assert_eq!(
                body,
                crate::request::encode_interaction_response(expected, None).unwrap()
            );
        }
    }

    /// The client's default `allowed_mentions` fills message callbacks,
    /// follow-ups, and edits that leave it unset; an explicit value wins and
    /// callbacks without a message get nothing.
    #[tokio::test]
    async fn default_allowed_mentions_fill_unset_message_bodies() {
        let mut server = mock::spawn(vec![
            mock::Reply::Now(204, ""),
            mock::Reply::Now(204, ""),
            mock::Reply::Now(204, ""),
            mock::Reply::Now(200, MESSAGE_JSON),
            mock::Reply::Now(200, MESSAGE_JSON),
        ])
        .await;
        let http = RestClient::builder("bot-token", Some(test_id(2)))
            .api_base(server.base_url.clone())
            .default_allowed_mentions(AllowedMentions {
                replied_user: Some(false),
                ..AllowedMentions::default()
            })
            .build()
            .unwrap();
        let explicit = AllowedMentions {
            parse: vec!["users".to_string()],
            ..AllowedMentions::default()
        };

        let context = sample_context();
        context.reply(&http, "hi").await.unwrap();
        sample_context()
            .reply(
                &http,
                InteractionMessage::from("hi").allowed_mentions(explicit.clone()),
            )
            .await
            .unwrap();
        sample_context().defer(&http).await.unwrap();
        context.follow_up(&http, "later").await.unwrap();
        context.edit_reply(&http, "edited").await.unwrap();

        let mut bodies = Vec::new();
        for _ in 0..5 {
            let (_, body) = server.next_request_with_body().await;
            bodies.push(serde_json::from_slice::<serde_json::Value>(&body).unwrap());
        }
        let default = json!({ "replied_user": false });
        assert_eq!(bodies[0]["data"]["allowed_mentions"], default);
        assert_eq!(
            bodies[1]["data"]["allowed_mentions"],
            json!({ "parse": ["users"] })
        );
        assert_eq!(bodies[2], json!({ "type": 5, "data": { "flags": 0 } }));
        assert_eq!(bodies[3]["allowed_mentions"], default);
        assert_eq!(bodies[4]["allowed_mentions"], default);
    }

    /// A reply with files is still exactly one initial response: it claims
    /// the slot, goes out as multipart with the encoded callback as
    /// `payload_json`, and settles to replied.
    #[tokio::test]
    async fn reply_with_files_is_one_initial_response() {
        let mut server = mock::spawn(vec![mock::Reply::Now(204, "")]).await;
        let http = mock_client(&server, Duration::from_secs(10));
        let context = sample_context();
        let files = [FileAttachment::new("report.txt", b"ok".to_vec())];

        context
            .reply_with_files(&http, "see attached", &files)
            .await
            .unwrap();
        assert!(context.is_replied());
        let (line, body) = server.next_request_with_body().await;
        assert_eq!(line, "POST /interactions/1/token/callback");
        let body = String::from_utf8_lossy(&body);
        assert!(body.contains("name=\"payload_json\""), "{body}");
        assert!(
            body.contains(r#"{"type":4,"data":{"content":"see attached"}}"#),
            "{body}"
        );
        assert!(body.contains("filename=\"report.txt\""), "{body}");

        // A second initial response with files fails locally.
        let error = context
            .reply_with_files(&http, "again", &files)
            .await
            .unwrap_err();
        assert_model_error(&error, "already acknowledged");
        assert!(!server.has_pending_request());
    }

    /// The `_with_files` responder methods, through the trait on a variant:
    /// the reply claims the slot, the follow-up and the `@original` edit go
    /// out as multipart once it is acknowledged, and the edit confirms the
    /// original response.
    #[tokio::test]
    async fn responder_file_methods_send_multipart_through_the_trait() {
        let mut server = mock::spawn(vec![
            mock::Reply::Now(204, ""),
            mock::Reply::Now(200, MESSAGE_JSON),
            mock::Reply::Now(200, MESSAGE_JSON),
        ])
        .await;
        let http = mock_client(&server, Duration::from_secs(10));
        let component = ComponentInteraction {
            context: sample_context(),
            ..ComponentInteraction::fixture()
        };
        let files = [FileAttachment::new("a.txt", b"a".to_vec())];

        component
            .reply_with_files(&http, "with a file", &files)
            .await
            .unwrap();
        component
            .follow_up_with_files(&http, "another", &files)
            .await
            .unwrap();
        component
            .edit_reply_with_files(&http, "edited", &files)
            .await
            .unwrap();
        assert!(component.is_replied());

        for expected in [
            "POST /interactions/1/token/callback",
            "POST /webhooks/2/token",
            "PATCH /webhooks/2/token/messages/@original",
        ] {
            let (line, body) = server.next_request_with_body().await;
            assert_eq!(line, expected);
            let body = String::from_utf8_lossy(&body);
            assert!(body.contains("name=\"payload_json\""), "{body}");
            assert!(body.contains("filename=\"a.txt\""), "{body}");
        }

        // Every responding variant reaches the same shared context.
        let modal = ModalSubmitInteraction {
            context: offline_context(),
            submission: crate::parsers::V2ModalSubmission {
                custom_id: "m".to_string(),
                components: Vec::new(),
            },
            resolved: None,
        };
        assert_eq!(modal.responder_context().token, "bad/token");
        let entry_point = crate::model::PrimaryEntryPointInteraction {
            context: offline_context(),
            data: crate::model::CommandInteractionData::default(),
        };
        assert_eq!(entry_point.responder_context().token, "bad/token");
    }

    /// Regression (astra review): an invalid file content type failed while
    /// the transport assembled the form, after the slot was claimed, and the
    /// error read as a possibly delivered response (`Uncertain`). It now
    /// fails before the claim: nothing is sent, the slot stays free, and the
    /// interaction can still be answered.
    #[tokio::test]
    async fn invalid_file_content_type_fails_before_the_slot_is_claimed() {
        let mut server = mock::spawn(vec![mock::Reply::Now(204, "")]).await;
        let http = mock_client(&server, Duration::from_secs(10));
        let context = sample_context();
        let files = [FileAttachment::new("a.txt", b"a".to_vec()).with_content_type("not a mime")];

        let error = context
            .reply_with_files(&http, "with a file", &files)
            .await
            .unwrap_err();
        assert_model_error(&error, "invalid content type");
        assert_eq!(
            context.response_phase(),
            InteractionResponsePhase::Unacknowledged
        );
        assert!(!server.has_pending_request(), "nothing may be sent");

        // The same holds for the stateless REST method and an empty file
        // name.
        let error = http
            .create_interaction_response_with_files(
                context.id,
                &context.token,
                &InteractionResponse::reply("x"),
                &[FileAttachment::new(" ", b"a".to_vec())],
            )
            .await
            .unwrap_err();
        assert_model_error(&error, "filename");
        assert!(!server.has_pending_request());

        // The slot is still free for the real response.
        context.defer(&http).await.unwrap();
        assert!(context.is_deferred());
        assert_eq!(
            server.next_request().await,
            "POST /interactions/1/token/callback"
        );
    }

    /// Everything a `MessageBuilder` carries reaches the wire on every typed
    /// response path, `tts` and `attachments` (kept and uploaded) included
    /// (astra review: the 2.x reply adapter dropped them). The reply and
    /// follow-up take the builder as is; the update and `@original` edit
    /// convert with `TryFrom`, which refuses `tts` locally because those
    /// bodies cannot carry it.
    #[tokio::test]
    async fn message_builder_tts_and_attachments_reach_every_response_path() {
        use crate::request::{
            AttachmentUpload, EditInteractionResponse, MessageAttachment, UpdateMessage,
        };

        let mut server = mock::spawn(vec![
            mock::Reply::Now(204, ""),
            mock::Reply::Now(200, MESSAGE_JSON),
            mock::Reply::Now(200, MESSAGE_JSON),
            mock::Reply::Now(204, ""),
        ])
        .await;
        let http = mock_client(&server, Duration::from_secs(10));
        let files = [FileAttachment::new("report.txt", b"ok".to_vec())];
        let builder = || {
            MessageBuilder::new()
                .content("see attached")
                .add_attachment(MessageAttachment::keep(test_id(77)))
                .add_attachment(
                    AttachmentUpload::new(crate::model::UploadId::new(0)).filename("report.txt"),
                )
        };
        let attachments = json!([{ "id": "77" }, { "id": 0, "filename": "report.txt" }]);

        let context = sample_context();
        context
            .reply_with_files(&http, builder().tts(true), &files)
            .await
            .unwrap();
        context
            .follow_up_with_files(&http, builder().tts(true), &files)
            .await
            .unwrap();
        context
            .edit_reply_with_files(
                &http,
                EditInteractionResponse::try_from(builder()).unwrap(),
                &files,
            )
            .await
            .unwrap();
        let component = ComponentInteraction {
            context: sample_context(),
            ..ComponentInteraction::fixture()
        };
        component
            .update_message(&http, UpdateMessage::try_from(builder()).unwrap())
            .await
            .unwrap();

        // `tts` cannot go on an update or an edit: refused before sending.
        assert!(UpdateMessage::try_from(builder().tts(true)).is_err());
        assert!(EditInteractionResponse::try_from(builder().tts(true)).is_err());

        let payload = |body: &[u8]| -> serde_json::Value {
            let text = String::from_utf8_lossy(body);
            let start = text.find('{').expect("payload_json part");
            let end = text[start..]
                .find("\r\n--")
                .map_or(text.len(), |offset| start + offset);
            serde_json::from_str(&text[start..end]).expect("payload_json is JSON")
        };
        let (line, body) = server.next_request_with_body().await;
        assert_eq!(line, "POST /interactions/1/token/callback");
        let reply = payload(&body);
        assert_eq!(reply["data"]["tts"], json!(true));
        assert_eq!(reply["data"]["attachments"], attachments);
        let (line, body) = server.next_request_with_body().await;
        assert_eq!(line, "POST /webhooks/2/token");
        let follow_up = payload(&body);
        assert_eq!(follow_up["tts"], json!(true));
        assert_eq!(follow_up["attachments"], attachments);
        let (line, body) = server.next_request_with_body().await;
        assert_eq!(line, "PATCH /webhooks/2/token/messages/@original");
        assert_eq!(payload(&body)["attachments"], attachments);
        let (line, body) = server.next_request_with_body().await;
        assert_eq!(line, "POST /interactions/1/token/callback");
        let update: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(update["type"], json!(7));
        assert_eq!(update["data"]["attachments"], attachments);
        assert!(update["data"].get("tts").is_none());
    }

    // --- state machine ------------------------------------------------------

    #[test]
    fn ack_kind_maps_callback_types() {
        assert_eq!(
            AckKind::for_callback_type(InteractionResponseType::PONG),
            None
        );
        for deferred in [
            InteractionResponseType::DEFERRED_CHANNEL_MESSAGE_WITH_SOURCE,
            InteractionResponseType::DEFERRED_UPDATE_MESSAGE,
        ] {
            assert_eq!(
                AckKind::for_callback_type(deferred),
                Some(AckKind::Deferred)
            );
        }
        for replied in [
            InteractionResponseType::CHANNEL_MESSAGE_WITH_SOURCE,
            InteractionResponseType::UPDATE_MESSAGE,
            InteractionResponseType::APPLICATION_COMMAND_AUTOCOMPLETE_RESULT,
            InteractionResponseType::MODAL,
            InteractionResponseType::LAUNCH_ACTIVITY,
            InteractionResponseType::new(200),
        ] {
            assert_eq!(AckKind::for_callback_type(replied), Some(AckKind::Replied));
        }
    }

    #[test]
    fn claim_settles_each_outcome_from_a_fresh_state() {
        let cases = [
            (
                AckKind::Deferred,
                CallbackOutcome::Accepted,
                InteractionResponsePhase::Deferred,
            ),
            (
                AckKind::Replied,
                CallbackOutcome::Accepted,
                InteractionResponsePhase::Replied,
            ),
            (
                AckKind::Replied,
                CallbackOutcome::Rejected,
                InteractionResponsePhase::Unacknowledged,
            ),
            (
                AckKind::Deferred,
                CallbackOutcome::Ambiguous,
                InteractionResponsePhase::Uncertain,
            ),
            // Acknowledged elsewhere: the kind is unknown.
            (
                AckKind::Deferred,
                CallbackOutcome::AlreadyAcknowledged,
                InteractionResponsePhase::Acknowledged,
            ),
        ];
        for (kind, outcome, expected) in cases {
            let state = InteractionResponseState::new();
            let claim = state.begin(kind).unwrap();
            assert_eq!(state.phase(), InteractionResponsePhase::InFlight);
            assert!(!state.is_acknowledged());
            claim.settle(outcome);
            assert_eq!(state.phase(), expected, "{kind:?} + {outcome:?}");
        }
    }

    #[test]
    fn retry_from_uncertain_remembers_the_earlier_attempt() {
        // Earlier deferral lost its outcome; a retried reply is refused with
        // 40060, which proves the deferral landed.
        let state = InteractionResponseState::new();
        state
            .begin(AckKind::Deferred)
            .unwrap()
            .settle(CallbackOutcome::Ambiguous);
        state
            .begin(AckKind::Replied)
            .unwrap()
            .settle(CallbackOutcome::AlreadyAcknowledged);
        assert!(state.is_deferred());

        // A retry Discord rejects outright leaves the earlier attempt open.
        let state = InteractionResponseState::new();
        state
            .begin(AckKind::Deferred)
            .unwrap()
            .settle(CallbackOutcome::Ambiguous);
        state
            .begin(AckKind::Replied)
            .unwrap()
            .settle(CallbackOutcome::Rejected);
        assert_eq!(
            state.load(),
            Phase::Uncertain(PossibleKinds::default().with(AckKind::Deferred))
        );

        // A retry that succeeds settles to its own kind.
        state
            .begin(AckKind::Replied)
            .unwrap()
            .settle(CallbackOutcome::Accepted);
        assert!(state.is_replied());
    }

    #[test]
    fn second_claim_fails_while_in_flight_and_after_acknowledgement() {
        let state = InteractionResponseState::new();
        let claim = state.begin(AckKind::Replied).unwrap();
        assert_model_error(
            &state.begin(AckKind::Deferred).map(drop).unwrap_err(),
            "in flight",
        );
        claim.settle(CallbackOutcome::Accepted);
        assert_model_error(
            &state.begin(AckKind::Deferred).map(drop).unwrap_err(),
            "already acknowledged",
        );
        // The refused claims must not clobber the original state.
        assert!(state.is_replied());
    }

    /// Regression: a future dropped between claiming and settling used to
    /// leave the slot claimed forever (2.2.0 reported it as replied).
    #[test]
    fn dropped_claim_records_an_uncertain_outcome() {
        let state = InteractionResponseState::new();
        drop(state.begin(AckKind::Deferred).unwrap());
        assert_eq!(state.phase(), InteractionResponsePhase::Uncertain);
        assert!(!state.is_deferred());
        assert!(!state.is_acknowledged());
        // Retrying is allowed from the uncertain phase.
        assert!(state.begin(AckKind::Deferred).is_ok());
    }

    #[test]
    fn concurrent_claims_only_let_one_caller_win() {
        let state = InteractionResponseState::new();
        let winners: Vec<_> = std::thread::scope(|scope| {
            (0..8)
                .map(|_| {
                    let state = &state;
                    scope.spawn(move || state.begin(AckKind::Replied).ok())
                })
                .collect::<Vec<_>>()
                .into_iter()
                .filter_map(|handle| handle.join().unwrap())
                .collect()
        });

        assert_eq!(winners.len(), 1);
        assert_eq!(state.phase(), InteractionResponsePhase::InFlight);
        for claim in winners {
            claim.settle(CallbackOutcome::Accepted);
        }
        assert!(state.is_replied());
    }

    #[tokio::test]
    async fn waiters_park_until_the_in_flight_response_is_accepted() {
        let state = InteractionResponseState::new();
        let claim = state.begin(AckKind::Deferred).unwrap();
        let waiter = tokio::spawn({
            let state = state.clone();
            async move { state.require_acknowledged().await }
        });
        wait_for_waiter(&state).await;
        tokio::task::yield_now().await;
        assert!(!waiter.is_finished());

        claim.settle(CallbackOutcome::Accepted);
        waiter.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn waiters_fail_when_the_in_flight_response_is_rejected() {
        let state = InteractionResponseState::new();
        let claim = state.begin(AckKind::Replied).unwrap();
        let waiter = tokio::spawn({
            let state = state.clone();
            async move { state.require_acknowledged().await }
        });
        wait_for_waiter(&state).await;

        claim.settle(CallbackOutcome::Rejected);
        assert_model_error(
            &waiter.await.unwrap().unwrap_err(),
            "has not been acknowledged",
        );
    }

    #[tokio::test]
    async fn waiters_are_released_when_the_claiming_future_is_dropped() {
        let state = InteractionResponseState::new();
        let claim = state.begin(AckKind::Replied).unwrap();
        let waiter = tokio::spawn({
            let state = state.clone();
            async move { state.require_acknowledged().await }
        });
        wait_for_waiter(&state).await;

        drop(claim);
        // Uncertain passes the local check; Discord decides.
        waiter.await.unwrap().unwrap();
        assert_eq!(state.phase(), InteractionResponsePhase::Uncertain);
    }

    /// Regression (review P2): a deferral lands but its response is lost, a
    /// retried reply also loses its response, and a third attempt gets 40060.
    /// 40060 proves *an* attempt landed, not which: the state must not claim
    /// "replied" while Discord may hold the deferral.
    #[test]
    fn already_acknowledged_after_mixed_uncertain_attempts_keeps_the_kind_unknown() {
        let state = InteractionResponseState::new();
        state
            .begin(AckKind::Deferred)
            .unwrap()
            .settle(CallbackOutcome::Ambiguous);
        state
            .begin(AckKind::Replied)
            .unwrap()
            .settle(CallbackOutcome::Ambiguous);
        assert_eq!(state.phase(), InteractionResponsePhase::Uncertain);
        // A dropped retry also widens, never narrows, the possible kinds.
        drop(state.begin(AckKind::Replied).unwrap());
        state
            .begin(AckKind::Replied)
            .unwrap()
            .settle(CallbackOutcome::AlreadyAcknowledged);

        assert_eq!(state.phase(), InteractionResponsePhase::Acknowledged);
        assert!(state.is_acknowledged());
        assert!(!state.is_deferred());
        assert!(!state.is_replied());
        assert_eq!(
            format!("{state:?}"),
            "InteractionResponseState(Acknowledged)"
        );
        // Further initial responses are refused; an edit settles the kind.
        assert!(state.begin(AckKind::Replied).is_err());
        state.confirm_original(OriginalSeen::Exists);
        assert_eq!(state.phase(), InteractionResponsePhase::Acknowledged);
        state.confirm_original(OriginalSeen::Edited);
        assert!(state.is_replied());
    }

    #[test]
    fn original_operations_confirm_and_promote_the_state() {
        // Proof of an original response outside this state: kind unknown.
        let state = InteractionResponseState::new();
        state.confirm_original(OriginalSeen::Exists);
        assert_eq!(state.phase(), InteractionResponsePhase::Acknowledged);

        let state = InteractionResponseState::new();
        state
            .begin(AckKind::Deferred)
            .unwrap()
            .settle(CallbackOutcome::Accepted);
        state.confirm_original(OriginalSeen::Exists);
        assert!(state.is_deferred());
        state.confirm_original(OriginalSeen::Edited);
        assert!(state.is_replied());

        let state = InteractionResponseState::new();
        state
            .begin(AckKind::Deferred)
            .unwrap()
            .settle(CallbackOutcome::Ambiguous);
        state.confirm_original(OriginalSeen::Exists);
        assert!(state.is_deferred());

        let state = InteractionResponseState::new();
        state
            .begin(AckKind::Deferred)
            .unwrap()
            .settle(CallbackOutcome::Ambiguous);
        state.confirm_original(OriginalSeen::Edited);
        assert!(state.is_replied());
    }

    /// Evidence recorded while a retry is in flight survives any outcome of
    /// that retry, and the stronger evidence wins.
    #[test]
    fn original_evidence_during_a_retry_survives_its_settlement() {
        let uncertain_deferral = || {
            let state = InteractionResponseState::new();
            state
                .begin(AckKind::Deferred)
                .unwrap()
                .settle(CallbackOutcome::Ambiguous);
            state
        };
        for (outcome, seen, expected) in [
            (
                CallbackOutcome::Rejected,
                OriginalSeen::Edited,
                InteractionResponsePhase::Replied,
            ),
            (
                CallbackOutcome::Ambiguous,
                OriginalSeen::Edited,
                InteractionResponsePhase::Replied,
            ),
            (
                CallbackOutcome::Rejected,
                OriginalSeen::Exists,
                InteractionResponsePhase::Deferred,
            ),
            (
                CallbackOutcome::AlreadyAcknowledged,
                OriginalSeen::Exists,
                InteractionResponsePhase::Deferred,
            ),
            (
                CallbackOutcome::Accepted,
                OriginalSeen::Exists,
                InteractionResponsePhase::Replied,
            ),
        ] {
            let state = uncertain_deferral();
            let claim = state.begin(AckKind::Replied).unwrap();
            state.confirm_original(seen);
            assert_eq!(state.phase(), InteractionResponsePhase::InFlight);
            claim.settle(outcome);
            assert_eq!(state.phase(), expected, "{outcome:?} after {seen:?}");
        }

        // The stronger evidence is kept when both arrive.
        let state = uncertain_deferral();
        let claim = state.begin(AckKind::Replied).unwrap();
        state.confirm_original(OriginalSeen::Edited);
        state.confirm_original(OriginalSeen::Exists);
        drop(claim);
        assert!(state.is_replied());

        // Proof of a response someone else sent, seen during a fresh attempt
        // that Discord then rejects.
        let state = InteractionResponseState::new();
        let claim = state.begin(AckKind::Replied).unwrap();
        state.confirm_original(OriginalSeen::Exists);
        claim.settle(CallbackOutcome::Rejected);
        assert_eq!(state.phase(), InteractionResponsePhase::Acknowledged);
    }

    #[test]
    fn inline_commit_only_claims_an_unacknowledged_state() {
        let state = InteractionResponseState::new();
        assert!(state.commit_inline(AckKind::Deferred));
        assert!(state.is_deferred());
        assert!(!state.commit_inline(AckKind::Replied));
        assert!(state.is_deferred());

        let state = InteractionResponseState::new();
        let _claim = state.begin(AckKind::Replied).unwrap();
        assert!(!state.commit_inline(AckKind::Replied));
        assert_eq!(state.phase(), InteractionResponsePhase::InFlight);
    }

    #[test]
    fn response_state_is_shared_between_clones_and_debug_shows_the_phase() {
        let context = sample_context();
        let clone = context.clone();
        assert_eq!(
            format!("{:?}", context.response_state),
            "InteractionResponseState(Unacknowledged)"
        );

        assert!(clone.response_state.commit_inline(AckKind::Replied));
        assert!(context.is_replied());
        assert!(context.is_acknowledged());
        assert_eq!(context.response_phase(), InteractionResponsePhase::Replied);
        assert_eq!(
            format!("{:?}", context.response_state),
            "InteractionResponseState(Replied)"
        );
    }

    #[tokio::test]
    async fn callback_outcome_classifies_errors() {
        let api = |status, code| Err::<(), _>(DiscordError::api(status, code, "x"));
        assert_eq!(
            CallbackOutcome::of(&Ok::<(), DiscordError>(())),
            CallbackOutcome::Accepted
        );
        assert_eq!(
            CallbackOutcome::of(&api(400, Some(40060))),
            CallbackOutcome::AlreadyAcknowledged
        );
        assert_eq!(
            CallbackOutcome::of(&api(400, Some(50035))),
            CallbackOutcome::Rejected
        );
        assert_eq!(
            CallbackOutcome::of(&api(404, Some(10062))),
            CallbackOutcome::Rejected
        );
        assert_eq!(
            CallbackOutcome::of(&api(503, None)),
            CallbackOutcome::Ambiguous
        );
        assert_eq!(
            CallbackOutcome::of(&Err::<(), _>(DiscordError::rate_limit("route", 1.0))),
            CallbackOutcome::Rejected
        );
        assert_eq!(
            CallbackOutcome::of(&Err::<(), _>(DiscordError::model("bad token"))),
            CallbackOutcome::Rejected
        );
        assert_eq!(
            CallbackOutcome::of(&Err::<(), _>(DiscordError::Json("eof".to_string()))),
            CallbackOutcome::Ambiguous
        );

        // A real refused connection: nothing was delivered.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let connect_error = reqwest::Client::new()
            .post(format!("http://{address}/"))
            .send()
            .await
            .unwrap_err();
        let connect_error = DiscordError::from(connect_error);
        assert!(matches!(&connect_error, DiscordError::Http(error) if error.is_connect()));
        assert_eq!(
            CallbackOutcome::of(&Err::<(), _>(connect_error)),
            CallbackOutcome::Rejected
        );
    }

    // --- REST responses against a scripted server ---------------------------

    /// Regression for the 2.2.0 race: the slot read "replied" as soon as the
    /// HTTP call started, so a clone could send a follow-up before Discord
    /// had accepted the reply. The follow-up must now wait.
    #[tokio::test]
    async fn follow_up_waits_for_the_in_flight_reply() {
        let (release, held) = tokio::sync::oneshot::channel();
        let mut server = mock::spawn(vec![
            mock::Reply::Held(held),
            mock::Reply::Now(200, MESSAGE_JSON),
        ])
        .await;
        let http = mock_client(&server, Duration::from_secs(10));
        let context = sample_context();

        let reply = tokio::spawn({
            let (context, http) = (context.clone(), http.clone());
            async move { context.reply(&http, "hi").await }
        });
        assert_eq!(
            server.next_request().await,
            "POST /interactions/1/token/callback"
        );
        assert_eq!(context.response_phase(), InteractionResponsePhase::InFlight);
        assert!(!context.is_replied());

        let follow_up = tokio::spawn({
            let (context, http) = (context.clone(), http.clone());
            async move { context.follow_up(&http, "after").await }
        });
        wait_for_waiter(&context.response_state).await;
        assert!(
            !server.has_pending_request(),
            "follow-up reached Discord before the reply was accepted"
        );

        release.send((204, "")).unwrap();
        reply.await.unwrap().unwrap();
        assert_eq!(server.next_request().await, "POST /webhooks/2/token");
        assert_eq!(follow_up.await.unwrap().unwrap().content, "ok");
        assert!(context.is_replied());
    }

    #[tokio::test]
    async fn follow_up_fails_locally_when_the_in_flight_reply_is_rejected() {
        let (release, held) = tokio::sync::oneshot::channel();
        let mut server = mock::spawn(vec![mock::Reply::Held(held)]).await;
        let http = mock_client(&server, Duration::from_secs(10));
        let context = sample_context();

        let reply = tokio::spawn({
            let (context, http) = (context.clone(), http.clone());
            async move { context.reply(&http, "hi").await }
        });
        server.next_request().await;

        // A second initial response fails at once instead of waiting.
        assert_model_error(&context.defer(&http).await.unwrap_err(), "in flight");

        let follow_up = tokio::spawn({
            let (context, http) = (context.clone(), http.clone());
            async move { context.follow_up(&http, "after").await }
        });
        wait_for_waiter(&context.response_state).await;

        release
            .send((400, r#"{"code":50035,"message":"Invalid Form Body"}"#))
            .unwrap();
        let error = reply.await.unwrap().unwrap_err();
        assert_eq!(error.discord_code(), Some(50035));
        assert_model_error(
            &follow_up.await.unwrap().unwrap_err(),
            "has not been acknowledged",
        );
        assert_eq!(
            context.response_phase(),
            InteractionResponsePhase::Unacknowledged
        );
        assert!(!server.has_pending_request());
    }

    /// Regression (review P3): an `@original` edit starts from `Uncertain`,
    /// another clone starts an initial-response retry, the edit succeeds
    /// while the retry is in flight, then the retry is rejected. The edit's
    /// proof that the original exists must not be lost.
    #[tokio::test]
    async fn edit_success_during_a_rejected_retry_keeps_the_proof() {
        let (release_edit, held_edit) = tokio::sync::oneshot::channel();
        let (release_retry, held_retry) = tokio::sync::oneshot::channel();
        let mut server = mock::spawn(vec![
            mock::Reply::Held(held_edit),
            mock::Reply::Held(held_retry),
        ])
        .await;
        let http = mock_client(&server, Duration::from_secs(10));
        let context = sample_context();
        context
            .response_state
            .begin(AckKind::Deferred)
            .unwrap()
            .settle(CallbackOutcome::Ambiguous);

        let edit = tokio::spawn({
            let (context, http) = (context.clone(), http.clone());
            async move { context.edit_reply(&http, "edited").await }
        });
        assert_eq!(
            server.next_request().await,
            "PATCH /webhooks/2/token/messages/@original"
        );
        let retry = tokio::spawn({
            let (context, http) = (context.clone(), http.clone());
            async move { context.reply(&http, "retry").await }
        });
        assert_eq!(
            server.next_request().await,
            "POST /interactions/1/token/callback"
        );
        assert_eq!(context.response_phase(), InteractionResponsePhase::InFlight);

        release_edit.send((200, MESSAGE_JSON)).unwrap();
        edit.await.unwrap().unwrap();
        assert_eq!(context.response_phase(), InteractionResponsePhase::InFlight);

        release_retry
            .send((400, r#"{"code":50035,"message":"Invalid Form Body"}"#))
            .unwrap();
        assert!(retry.await.unwrap().is_err());
        assert!(
            context.is_replied(),
            "the successful edit proves a (now edited) original response"
        );
    }

    /// Regression: cancelling a deferral mid-flight left 2.2.0's state
    /// claimed forever. Now it is uncertain, and a retry settles it through
    /// Discord's 40060 answer without producing a second response.
    #[tokio::test]
    async fn cancelled_defer_is_uncertain_and_a_retry_settles_it() {
        let (_release, held) = tokio::sync::oneshot::channel();
        let mut server = mock::spawn(vec![
            mock::Reply::Held(held),
            mock::Reply::Now(400, ALREADY_ACKNOWLEDGED_JSON),
            mock::Reply::Now(200, MESSAGE_JSON),
        ])
        .await;
        let http = mock_client(&server, Duration::from_secs(10));
        let context = sample_context();

        let defer = tokio::spawn({
            let (context, http) = (context.clone(), http.clone());
            async move { context.defer(&http).await }
        });
        server.next_request().await;
        assert_eq!(context.response_phase(), InteractionResponsePhase::InFlight);

        defer.abort();
        assert!(defer.await.unwrap_err().is_cancelled());
        assert_eq!(
            context.response_phase(),
            InteractionResponsePhase::Uncertain
        );
        assert!(!context.is_deferred());

        let error = context.reply(&http, "retry").await.unwrap_err();
        assert_eq!(error.discord_code(), Some(40060));
        assert!(
            context.is_deferred(),
            "40060 proves the cancelled deferral landed"
        );

        context.follow_up(&http, "after").await.unwrap();
        server.next_request().await;
        server.next_request().await;
    }

    /// Regression: 2.2.0 rolled back on every error, so a reply whose
    /// response was lost (timeout) looked unsent and the next reply hit 40060
    /// while the state still claimed nothing had happened.
    #[tokio::test]
    async fn timed_out_defer_is_uncertain_and_an_original_edit_settles_it() {
        let mut server = mock::spawn(vec![
            mock::Reply::Silent,
            mock::Reply::Now(200, MESSAGE_JSON),
            mock::Reply::Now(200, MESSAGE_JSON),
        ])
        .await;
        let http = mock_client(&server, Duration::from_millis(200));
        let context = sample_context();

        let error = context.defer(&http).await.unwrap_err();
        assert!(matches!(&error, DiscordError::Http(error) if error.is_timeout()));
        assert_eq!(
            context.response_phase(),
            InteractionResponsePhase::Uncertain
        );

        // Follow-ups are passed to Discord, but do not settle the phase.
        context.follow_up(&http, "maybe").await.unwrap();
        assert_eq!(
            context.response_phase(),
            InteractionResponsePhase::Uncertain
        );

        // A successful `@original` edit proves the response exists.
        context.edit_reply(&http, "done").await.unwrap();
        assert!(context.is_replied());
        for _ in 0..3 {
            server.next_request().await;
        }
    }

    #[tokio::test]
    async fn lost_or_failed_response_bodies_are_uncertain() {
        let server = mock::spawn(vec![
            mock::Reply::Truncated,
            mock::Reply::Now(500, r#"{"message":"boom"}"#),
        ])
        .await;
        let http = mock_client(&server, Duration::from_secs(10));

        let truncated = sample_context();
        let error = truncated.reply(&http, "hi").await.unwrap_err();
        assert!(
            matches!(&error, DiscordError::Http(error) if error.is_body()),
            "{error:?}"
        );
        assert_eq!(
            truncated.response_phase(),
            InteractionResponsePhase::Uncertain
        );

        let server_error = sample_context();
        let error = server_error.reply(&http, "hi").await.unwrap_err();
        assert_eq!(error.status_code(), Some(500));
        assert_eq!(
            server_error.response_phase(),
            InteractionResponsePhase::Uncertain
        );
    }

    #[tokio::test]
    async fn rejected_reply_rolls_back_so_a_retry_can_succeed() {
        let server = mock::spawn(vec![
            mock::Reply::Now(400, r#"{"code":50035,"message":"Invalid Form Body"}"#),
            mock::Reply::Now(204, ""),
        ])
        .await;
        let http = mock_client(&server, Duration::from_secs(10));
        let context = sample_context();

        assert!(context.reply(&http, "bad").await.is_err());
        assert_eq!(
            context.response_phase(),
            InteractionResponsePhase::Unacknowledged
        );
        context.reply(&http, "good").await.unwrap();
        assert!(context.is_replied());
    }

    #[tokio::test]
    async fn already_acknowledged_answer_marks_a_fresh_state_acknowledged() {
        let server = mock::spawn(vec![mock::Reply::Now(400, ALREADY_ACKNOWLEDGED_JSON)]).await;
        let http = mock_client(&server, Duration::from_secs(10));
        let context = sample_context();

        let error = context.defer(&http).await.unwrap_err();
        assert_eq!(error.discord_code(), Some(40060));
        assert!(context.is_acknowledged());
    }

    #[tokio::test]
    async fn reply_with_result_counts_an_undecodable_success_as_replied() {
        let server = mock::spawn(vec![
            mock::Reply::Now(200, r#"{"unexpected":true}"#),
            mock::Reply::Now(
                200,
                r#"{"interaction":{"id":"1","type":2},"resource":{"type":4}}"#,
            ),
        ])
        .await;
        let http = mock_client(&server, Duration::from_secs(10));

        let undecodable = sample_context();
        let error = undecodable
            .reply_with_result(&http, "hi")
            .await
            .unwrap_err();
        assert!(matches!(error, DiscordError::Json(_)));
        assert!(undecodable.is_replied());

        let decoded = sample_context();
        let result = decoded.reply_with_result(&http, "hi").await.unwrap();
        assert_eq!(
            result.interaction.kind,
            InteractionType::APPLICATION_COMMAND
        );
        assert!(decoded.is_replied());
    }

    #[tokio::test]
    async fn create_response_records_the_callback_kind() {
        let mut server = mock::spawn(vec![
            mock::Reply::Now(204, ""),
            mock::Reply::Now(204, ""),
            mock::Reply::Now(204, ""),
        ])
        .await;
        let http = mock_client(&server, Duration::from_secs(10));

        let pong = sample_context();
        pong.create_response(&http, &InteractionResponse::Pong)
            .await
            .unwrap();
        assert_eq!(
            pong.response_phase(),
            InteractionResponsePhase::Unacknowledged
        );

        let deferred = sample_context();
        deferred
            .create_response(&http, &InteractionResponse::DeferredUpdateMessage)
            .await
            .unwrap();
        assert!(deferred.is_deferred());

        let activity = sample_context();
        activity
            .create_response(&http, &InteractionResponse::LaunchActivity)
            .await
            .unwrap();
        assert!(activity.is_replied());
        for _ in 0..3 {
            assert_eq!(
                server.next_request().await,
                "POST /interactions/1/token/callback"
            );
        }
    }

    #[tokio::test]
    async fn original_and_follow_up_operations_reach_discord_once_acknowledged() {
        let mut server = mock::spawn(vec![
            mock::Reply::Now(204, ""),
            mock::Reply::Now(200, MESSAGE_JSON),
            mock::Reply::Now(200, MESSAGE_JSON),
            mock::Reply::Now(200, MESSAGE_JSON),
            mock::Reply::Now(204, ""),
            mock::Reply::Now(204, ""),
        ])
        .await;
        let http = mock_client(&server, Duration::from_secs(10));
        let component = ComponentInteraction {
            context: sample_context(),
            ..ComponentInteraction::fixture()
        };

        component.defer_update(&http).await.unwrap();
        assert!(component.is_deferred());
        component.fetch_reply(&http).await.unwrap();
        assert!(component.is_deferred());
        component.fetch_follow_up(&http, test_id(30)).await.unwrap();
        component
            .edit_follow_up(&http, test_id(30), "edited")
            .await
            .unwrap();
        component
            .delete_follow_up(&http, test_id(30))
            .await
            .unwrap();
        component.delete_reply(&http).await.unwrap();

        let mut requests = Vec::new();
        for _ in 0..6 {
            requests.push(server.next_request().await);
        }
        assert_eq!(
            requests,
            [
                "POST /interactions/1/token/callback",
                "GET /webhooks/2/token/messages/@original",
                "GET /webhooks/2/token/messages/30",
                "PATCH /webhooks/2/token/messages/30",
                "DELETE /webhooks/2/token/messages/30",
                "DELETE /webhooks/2/token/messages/@original",
            ]
        );
    }

    // --- local checks and the responder trait -------------------------------

    #[tokio::test]
    async fn initial_responses_error_after_acknowledgement_without_touching_http() {
        let http = offline_client();
        let context = sample_context();
        assert!(context.response_state.commit_inline(AckKind::Deferred));

        for error in [
            context.reply(&http, "hi").await.unwrap_err(),
            context.reply_ephemeral(&http, "hi").await.unwrap_err(),
            context.reply_with_result(&http, "hi").await.unwrap_err(),
            context.defer(&http).await.unwrap_err(),
            context.defer_ephemeral(&http).await.unwrap_err(),
            context.defer_update(&http).await.unwrap_err(),
            context.update_message(&http, "hi").await.unwrap_err(),
            context
                .show_modal(&http, ModalBuilder::new("m", "M"))
                .await
                .unwrap_err(),
            context
                .respond_autocomplete(&http, vec![ApplicationCommandOptionChoice::new("a", "b")])
                .await
                .unwrap_err(),
            context
                .create_response(&http, &InteractionResponse::DeferredUpdateMessage)
                .await
                .unwrap_err(),
        ] {
            assert_model_error(&error, "already acknowledged");
        }

        // The double acknowledgement must not clobber the original state.
        assert!(context.is_deferred());
    }

    #[tokio::test]
    async fn reply_dependent_methods_require_acknowledgement() {
        let http = offline_client();
        let context = sample_context();

        for error in [
            context.edit_reply(&http, "edited").await.unwrap_err(),
            context
                .follow_up(&http, "extra")
                .await
                .map(drop)
                .unwrap_err(),
            context
                .follow_up_ephemeral(&http, "extra")
                .await
                .map(drop)
                .unwrap_err(),
            context.delete_reply(&http).await.unwrap_err(),
            context
                .fetch_follow_up(&http, test_id(30))
                .await
                .map(drop)
                .unwrap_err(),
            context
                .edit_follow_up(&http, test_id(30), "edited")
                .await
                .map(drop)
                .unwrap_err(),
            context
                .delete_follow_up(&http, test_id(30))
                .await
                .unwrap_err(),
        ] {
            assert_model_error(&error, "has not been acknowledged");
        }

        assert!(!context.is_acknowledged());
    }

    #[tokio::test]
    async fn locally_rejected_initial_responses_roll_back() {
        let http = offline_client();
        let context = offline_context();

        for error in [
            context.reply(&http, "hi").await.unwrap_err(),
            context.reply_with_result(&http, "hi").await.unwrap_err(),
            context.defer(&http).await.unwrap_err(),
            context
                .create_response(&http, &InteractionResponse::Pong)
                .await
                .unwrap_err(),
        ] {
            assert_model_error(&error, "interaction_token");
            assert_eq!(
                context.response_phase(),
                InteractionResponsePhase::Unacknowledged
            );
        }
    }

    /// Compile-time check of the `Send` contract: `tokio::spawn` accepts
    /// these generic calls only because every responder future is `Send`.
    /// With 2.2.0's plain `async fn` in the trait this did not compile.
    fn spawn_responder_calls<R>(
        responder: R,
        http: RestClient,
    ) -> tokio::task::JoinHandle<Vec<DiscordError>>
    where
        R: InteractionResponder + Send + 'static,
    {
        tokio::spawn(async move {
            let mut errors = vec![
                responder.reply(&http, "a").await.unwrap_err(),
                responder.reply_ephemeral(&http, "a").await.unwrap_err(),
                responder.reply_with_result(&http, "a").await.unwrap_err(),
                responder.defer(&http).await.unwrap_err(),
                responder.defer_ephemeral(&http).await.unwrap_err(),
                responder
                    .show_modal(&http, ModalBuilder::new("m", "M"))
                    .await
                    .unwrap_err(),
                responder.fetch_reply(&http).await.unwrap_err(),
            ];
            responder
                .responder_context()
                .response_state
                .commit_inline(AckKind::Replied);
            errors.extend([
                responder.edit_reply(&http, "a").await.unwrap_err(),
                responder.delete_reply(&http).await.unwrap_err(),
                responder.follow_up(&http, "a").await.unwrap_err(),
                responder.follow_up_ephemeral(&http, "a").await.unwrap_err(),
                responder
                    .fetch_follow_up(&http, test_id(30))
                    .await
                    .unwrap_err(),
                responder
                    .edit_follow_up(&http, test_id(30), "a")
                    .await
                    .unwrap_err(),
                responder
                    .delete_follow_up(&http, test_id(30))
                    .await
                    .unwrap_err(),
            ]);
            errors
        })
    }

    #[tokio::test]
    async fn responder_futures_are_send_for_generic_callers() {
        let interaction = ChatInputCommandInteraction {
            context: offline_context(),
            ..ChatInputCommandInteraction::fixture()
        };
        let state = interaction.context.response_state.clone();

        let errors = spawn_responder_calls(interaction, offline_client())
            .await
            .unwrap();
        assert_eq!(errors.len(), 14);
        for error in &errors {
            assert_model_error(error, "interaction_token");
        }
        assert_eq!(state.phase(), InteractionResponsePhase::Replied);
    }

    #[tokio::test]
    async fn responder_trait_delegates_to_the_shared_context() {
        let http = offline_client();
        let interaction = ChatInputCommandInteraction {
            context: sample_context(),
            ..ChatInputCommandInteraction::fixture()
        };

        assert!(!interaction.is_acknowledged());
        assert_eq!(
            interaction.response_phase(),
            InteractionResponsePhase::Unacknowledged
        );
        let error = interaction.follow_up(&http, "early").await.unwrap_err();
        assert_model_error(&error, "has not been acknowledged");

        interaction
            .context
            .response_state
            .commit_inline(AckKind::Replied);
        assert!(interaction.is_replied());
        assert!(!interaction.is_deferred());
        let error = interaction.reply(&http, "again").await.unwrap_err();
        assert_model_error(&error, "already acknowledged");

        let component = ComponentInteraction {
            context: sample_context(),
            ..ComponentInteraction::fixture()
        };
        component
            .context
            .response_state
            .commit_inline(AckKind::Replied);
        assert_model_error(
            &component.defer_update(&http).await.unwrap_err(),
            "already acknowledged",
        );
        assert_model_error(
            &component.update_message(&http, "new").await.unwrap_err(),
            "already acknowledged",
        );

        let modal = ModalSubmitInteraction {
            context: offline_context(),
            submission: crate::parsers::V2ModalSubmission {
                custom_id: "m".to_string(),
                components: Vec::new(),
            },
            resolved: None,
        };
        assert_model_error(
            &modal.defer_update(&http).await.unwrap_err(),
            "interaction_token",
        );
        assert_model_error(
            &modal.update_message(&http, "new").await.unwrap_err(),
            "interaction_token",
        );

        let autocomplete = crate::model::AutocompleteInteraction {
            context: offline_context(),
            ..crate::model::AutocompleteInteraction::fixture()
        };
        assert_model_error(
            &autocomplete
                .respond_autocomplete(&http, vec![ApplicationCommandOptionChoice::new("a", "b")])
                .await
                .unwrap_err(),
            "interaction_token",
        );

        for responder in [
            &crate::model::UserContextMenuInteraction::fixture() as &dyn ContextOnly,
            &crate::model::MessageContextMenuInteraction::fixture(),
        ] {
            assert!(!responder.acknowledged());
        }
    }

    /// Object-safe view used to exercise `responder_context` on the
    /// context-menu variants without duplicating the test body.
    trait ContextOnly {
        fn acknowledged(&self) -> bool;
    }

    impl<R: InteractionResponder> ContextOnly for R {
        fn acknowledged(&self) -> bool {
            self.responder_context().is_acknowledged()
        }
    }
}
