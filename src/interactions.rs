use axum::{
    body::{Body, Bytes},
    extract::{DefaultBodyLimit, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Router,
};
use ed25519_dalek::{Signature, VerifyingKey};
use serde_json::Value;
use std::future::Future;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{error, warn};

use crate::error::DiscordError;
use crate::model::{
    AllowedMentions, Interaction, InteractionContextData, InteractionId, InteractionResponseState,
    InteractionType,
};
use crate::parsers::{
    decode_interaction, parse_interaction_context, parse_raw_interaction, value_to_u8,
    InteractionContext, RawInteraction,
};
use crate::request::{encode_interaction_response, EndpointReply, InteractionResponse};
use crate::response::{AckKind, InteractionResponsePhase};
use crate::types::invalid_data_error;

/// Handles verified raw interactions for [`interactions_endpoint`].
///
/// Implement [`handle`](Self::handle) as a plain `async fn`; no attribute
/// macro is needed. It is declared as `fn ... -> impl Future + Send` because
/// the endpoint runs inside an axum (Tokio) server, which requires `Send`
/// request futures: do not hold a non-`Send` value (`Rc`, `RefCell` borrow,
/// `std::sync::MutexGuard`) across an `.await`. A violation is a compile
/// error at the impl.
///
/// ```no_run
/// use discordrs::request::{EndpointReply, InteractionResponse};
/// use discordrs::{InteractionContext, InteractionHandler, RawInteraction};
///
/// #[derive(Clone)]
/// struct Handler;
///
/// impl InteractionHandler for Handler {
///     async fn handle(
///         &self,
///         _ctx: InteractionContext,
///         _interaction: RawInteraction,
///     ) -> EndpointReply {
///         InteractionResponse::defer(false).into()
///     }
/// }
/// ```
pub trait InteractionHandler: Send + Sync {
    /// Handles one verified raw Discord interaction.
    fn handle(
        &self,
        ctx: InteractionContext,
        interaction: RawInteraction,
    ) -> impl Future<Output = EndpointReply> + Send;
}

/// Trait for `TypedInteractionHandler` behavior.
///
/// The `ctx` and `interaction` passed to the handler share one response
/// state with each other and with every clone made from them, and exactly
/// one transport owns the initial response:
///
/// - The handler returns an initial response: the endpoint claims the state
///   for it and answers Discord with `200` and that response, so a clone
///   kept past the handler sees the interaction as acknowledged once the
///   endpoint has answered and can call `follow_up` or `edit_reply` over
///   REST. A task the handler spawns can run before that; when it is going
///   to call those, acknowledge over REST (`defer`) before spawning it and
///   return [`EndpointReply::Accepted`].
/// - The handler sent (or started sending) the initial response over REST,
///   or returns [`EndpointReply::Accepted`]: the endpoint answers
///   `202 Accepted` with no body, as Discord specifies, and any inline
///   response the handler returned is discarded with a warning.
///
/// Implement [`handle_typed`](Self::handle_typed) as a plain `async fn`; the
/// returned future must be `Send` for the same reason as with
/// [`InteractionHandler`].
pub trait TypedInteractionHandler: Send + Sync {
    /// Handles one verified typed Discord interaction.
    fn handle_typed(
        &self,
        ctx: InteractionContextData,
        interaction: Interaction,
    ) -> impl Future<Output = EndpointReply> + Send;
}

#[derive(Clone)]
struct InteractionsState<H> {
    verifying_key: VerifyingKey,
    handler: H,
    /// [`InteractionsEndpointBuilder::default_allowed_mentions`]. Behind an
    /// `Arc` because axum clones the state for every request.
    default_allowed_mentions: Option<Arc<AllowedMentions>>,
}

const SIGNATURE_TIMESTAMP_TOLERANCE_SECS: i64 = 60 * 5;

/// The request body limit of the `/interactions` route unless
/// [`InteractionsEndpointBuilder::body_limit`] sets another: 1 MiB.
///
/// A larger body is answered `413 Payload Too Large` before the signature is
/// checked or a handler runs, so Discord reports the interaction as failed.
///
/// [Decision Log] Interaction body limit
/// - Purpose: never reject a request Discord can really send, while still
///   bounding what an unauthenticated client can make the server buffer
///   (the Ed25519 signature covers the whole body, so it can only be checked
///   after the body is read).
/// - Before: 64 KiB. A channel select with 25 forum channels whose resolved
///   objects carry 4,096-character topics measured 104,741 bytes and was
///   answered 413 before the handler ran.
/// - Size of a large real request, all-ASCII text: up to 25 resolved
///   channels with 4,096-character topics (~106 KB); the message a component
///   sits on, with 2,000 characters of content, 6,000 of embeds, five
///   25-option selects with 100-character labels, values, and descriptions,
///   and the users its content mentions (~90 KB); the message it replies to
///   (another ~90 KB); and the invoking member with up to 250 roles (~6 KB):
///   about 300 KB. A modal with five channel selects can resolve 125
///   channels (~530 KB). Non-ASCII text takes up to four bytes a character,
///   so a payload built from maximal emoji-only text can exceed 1 MiB.
/// - Alternatives: keep 64 KiB (drops real interactions); axum's own
///   default of 2 MiB; no limit (lets any client make the server buffer an
///   unbounded body before verification).
/// - Chosen: 1 MiB, about ten times the largest payload observed and above
///   every all-ASCII bound above, and half of axum's default, with
///   [`InteractionsEndpointBuilder::body_limit`] for apps that expect more.
/// - Trade-offs: the limit is per request, not for all requests together.
///   The whole body is read into memory before the signature is checked,
///   and verification copies it once more behind the timestamp (Ed25519
///   signs `timestamp + body` as one message), so a request holds about
///   twice its body until verification ends, and the decoded interaction
///   after that. `n` concurrent unauthenticated requests can hold about
///   `2 × n ×` the limit; bound concurrency in front of the endpoint when
///   that matters. A reverse proxy in front should allow at least the same
///   size, or it rejects these requests before this limit does.
pub const DEFAULT_INTERACTION_BODY_LIMIT: usize = 1024 * 1024;

fn decode_verifying_key(public_key: &str) -> Result<VerifyingKey, DiscordError> {
    let public_key_bytes = crate::hex::decode(public_key)
        .map_err(|error| invalid_data_error(format!("invalid public key hex: {error}")))?;
    let public_key_bytes: [u8; 32] = public_key_bytes
        .as_slice()
        .try_into()
        .map_err(|_| invalid_data_error("public key must be 32 bytes"))?;

    VerifyingKey::from_bytes(&public_key_bytes)
        .map_err(|error| invalid_data_error(format!("invalid public key: {error}")))
}

fn current_unix_timestamp() -> Result<i64, DiscordError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| invalid_data_error("system clock is before unix epoch"))?;

    i64::try_from(duration.as_secs())
        .map_err(|_| invalid_data_error("system clock is out of range"))
}

fn validate_signature_timestamp(
    timestamp: &str,
    now_unix_timestamp: i64,
) -> Result<(), DiscordError> {
    let timestamp = timestamp
        .parse::<i64>()
        .map_err(|_| invalid_data_error("signature timestamp must be a unix timestamp"))?;
    let drift = (i128::from(timestamp) - i128::from(now_unix_timestamp)).abs();

    if drift > i128::from(SIGNATURE_TIMESTAMP_TOLERANCE_SECS) {
        return Err(invalid_data_error(
            "signature timestamp outside allowed freshness window",
        ));
    }

    Ok(())
}

fn verify_signature_with_key(
    verifying_key: &VerifyingKey,
    signature_hex: &str,
    timestamp: &str,
    body: &[u8],
) -> Result<(), DiscordError> {
    let now_unix_timestamp = current_unix_timestamp()?;
    verify_signature_with_key_at_time(
        verifying_key,
        signature_hex,
        timestamp,
        body,
        now_unix_timestamp,
    )
}

fn verify_signature_with_key_at_time(
    verifying_key: &VerifyingKey,
    signature_hex: &str,
    timestamp: &str,
    body: &[u8],
    now_unix_timestamp: i64,
) -> Result<(), DiscordError> {
    validate_signature_timestamp(timestamp, now_unix_timestamp)?;
    let signature_bytes = crate::hex::decode(signature_hex)
        .map_err(|error| invalid_data_error(format!("invalid signature hex: {error}")))?;
    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|error| invalid_data_error(format!("invalid signature bytes: {error}")))?;

    let mut signed_payload = Vec::with_capacity(timestamp.len() + body.len());
    signed_payload.extend_from_slice(timestamp.as_bytes());
    signed_payload.extend_from_slice(body);

    verifying_key
        .verify_strict(&signed_payload, &signature)
        .map_err(|_| invalid_data_error("signature verification failed"))
}

fn header_value<'a>(headers: &'a HeaderMap, name: &str) -> Result<&'a str, StatusCode> {
    headers
        .get(name)
        .ok_or(StatusCode::UNAUTHORIZED)
        .and_then(|value| value.to_str().map_err(|_| StatusCode::UNAUTHORIZED))
}

fn verify_discord_request_signature(
    verifying_key: &VerifyingKey,
    headers: &HeaderMap,
    body: &[u8],
) -> Result<(), StatusCode> {
    let now_unix_timestamp = current_unix_timestamp().map_err(|_| StatusCode::UNAUTHORIZED)?;
    verify_discord_request_signature_at_time(verifying_key, headers, body, now_unix_timestamp)
}

fn verify_discord_request_signature_at_time(
    verifying_key: &VerifyingKey,
    headers: &HeaderMap,
    body: &[u8],
    now_unix_timestamp: i64,
) -> Result<(), StatusCode> {
    let signature = header_value(headers, "x-signature-ed25519")?;
    let timestamp = header_value(headers, "x-signature-timestamp")?;
    verify_signature_with_key_at_time(
        verifying_key,
        signature,
        timestamp,
        body,
        now_unix_timestamp,
    )
    .map_err(|_| StatusCode::UNAUTHORIZED)
}

/// Provides the `verify_discord_signature` helper.
pub fn verify_discord_signature(
    public_key: &str,
    signature: &str,
    timestamp: &str,
    body: &[u8],
) -> Result<(), DiscordError> {
    let verifying_key = decode_verifying_key(public_key)?;
    verify_signature_with_key(&verifying_key, signature, timestamp, body)
}

/// The body of the PING answer, `{"type":1}`: what
/// [`InteractionResponse::Pong`] encodes to (pinned by a test), kept as a
/// constant because verification pings arrive before any handler runs.
const PONG_BODY: &[u8] = br#"{"type":1}"#;

/// How the endpoint answers Discord's HTTP request.
#[derive(Debug, PartialEq)]
enum EndpointAnswer {
    /// `200 OK` with this encoded interaction response as the body.
    Inline(Vec<u8>),
    /// `202 Accepted` with no body: the initial response goes (or went)
    /// through the REST callback endpoint.
    Accepted,
    /// `500` with no body: the handler's response could not be encoded.
    /// Discord then reports the interaction as failed, which is what would
    /// happen anyway without a valid answer.
    EncodeFailed,
}

impl IntoResponse for EndpointAnswer {
    fn into_response(self) -> Response {
        match self {
            Self::Inline(body) => json_response(StatusCode::OK, body),
            Self::Accepted => StatusCode::ACCEPTED.into_response(),
            Self::EncodeFailed => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
}

impl EndpointAnswer {
    /// Encodes an inline answer with the same encoder and the same default
    /// `allowed_mentions` rule the REST callback uses, so both transports
    /// send identical bytes for one response and one policy.
    fn from_reply(reply: EndpointReply, default_mentions: Option<&AllowedMentions>) -> Self {
        match reply {
            EndpointReply::Accepted => Self::Accepted,
            EndpointReply::Inline(response) => Self::encode(&response, default_mentions),
        }
    }

    fn encode(response: &InteractionResponse, default_mentions: Option<&AllowedMentions>) -> Self {
        match encode_interaction_response(response, default_mentions) {
            Ok(body) => Self::Inline(body),
            Err(encode_error) => {
                error!(
                    error = %encode_error,
                    "could not encode the handler's interaction response; answering 500"
                );
                Self::EncodeFailed
            }
        }
    }
}

/// A response with an `application/json` body that is already encoded.
fn json_response(status: StatusCode, body: Vec<u8>) -> Response {
    let mut response = Response::new(Body::from(body));
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    response
}

/// `400 Bad Request` with `{"error": message}`.
fn bad_request(message: &str) -> Response {
    #[derive(serde::Serialize)]
    struct ErrorBody<'a> {
        error: &'a str,
    }
    match serde_json::to_vec(&ErrorBody { error: message }) {
        Ok(body) => json_response(StatusCode::BAD_REQUEST, body),
        Err(_) => StatusCode::BAD_REQUEST.into_response(),
    }
}

async fn handle_interactions_request<H>(
    State(state): State<InteractionsState<H>>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse
where
    H: InteractionHandler + Clone + Send + Sync + 'static,
{
    if let Err(status) =
        verify_discord_request_signature(&state.verifying_key, &headers, body.as_ref())
    {
        return status.into_response();
    }

    // Text, not bytes: the crate only instantiates its decoders for the
    // text reader (`from_str`).
    let payload: Value = match body_json(body.as_ref())
        .and_then(|text| serde_json::from_str(text).map_err(|error| error.to_string()))
    {
        Ok(payload) => payload,
        Err(error) => return bad_request(&error),
    };

    let interaction_type = match payload
        .get("type")
        .and_then(value_to_u8)
        .map(InteractionType::new)
    {
        Some(interaction_type) => interaction_type,
        None => return bad_request("missing or invalid interaction.type"),
    };

    if interaction_type == InteractionType::PING {
        return pong();
    }

    let context = match parse_interaction_context(&payload) {
        Ok(context) => context,
        Err(error) => return bad_request(&error.to_string()),
    };

    let interaction = match parse_raw_interaction(&payload) {
        Ok(interaction) => interaction,
        Err(error) => return bad_request(&error.to_string()),
    };

    // Raw handlers have no shared response state: they own the answer.
    let reply = state.handler.handle(context, interaction).await;
    EndpointAnswer::from_reply(reply, state.default_allowed_mentions.as_deref()).into_response()
}

/// Builds the signed `/interactions` route with settings other than the
/// defaults of [`try_interactions_endpoint`] and
/// [`try_typed_interactions_endpoint`].
///
/// ```no_run
/// # use discordrs::request::{EndpointReply, InteractionResponse};
/// # use discordrs::model::{Interaction, InteractionContextData};
/// # use discordrs::interactions::InteractionsEndpointBuilder;
/// # use discordrs::model::AllowedMentions;
/// # use discordrs::TypedInteractionHandler;
/// # #[derive(Clone)]
/// # struct Handler;
/// # impl TypedInteractionHandler for Handler {
/// #     async fn handle_typed(&self, _: InteractionContextData, _: Interaction) -> EndpointReply {
/// #         InteractionResponse::defer(false).into()
/// #     }
/// # }
/// # fn main() -> Result<(), discordrs::DiscordError> {
/// # let public_key = "0000000000000000000000000000000000000000000000000000000000000000";
/// let router = InteractionsEndpointBuilder::new(public_key)
///     .body_limit(4 * 1024 * 1024)
///     .default_allowed_mentions(AllowedMentions::default())
///     .build_typed(Handler)?;
/// # let _ = router;
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug)]
pub struct InteractionsEndpointBuilder {
    public_key: String,
    body_limit: usize,
    default_allowed_mentions: Option<AllowedMentions>,
}

impl InteractionsEndpointBuilder {
    /// Starts a builder for the application's hex-encoded Ed25519 public
    /// key. The key is checked by [`Self::build`] or [`Self::build_typed`].
    pub fn new(public_key: impl Into<String>) -> Self {
        Self {
            public_key: public_key.into(),
            body_limit: DEFAULT_INTERACTION_BODY_LIMIT,
            default_allowed_mentions: None,
        }
    }

    /// Sets the largest request body the route reads, in bytes (default
    /// [`DEFAULT_INTERACTION_BODY_LIMIT`], 1 MiB). A larger request is
    /// answered `413 Payload Too Large` before its signature is checked, so
    /// the handler never sees it and Discord reports the interaction as
    /// failed. See the default's documentation for how large real
    /// interactions get.
    ///
    /// The limit applies to each request separately. The body is read in
    /// full before the signature is checked and copied once more for the
    /// check, so a request holds about twice its body while it is verified;
    /// concurrent requests add up.
    pub fn body_limit(mut self, bytes: usize) -> Self {
        self.body_limit = bytes;
        self
    }

    /// Sets the `allowed_mentions` written into inline message responses
    /// that do not set one themselves, for raw and typed handlers (and
    /// [`AppFramework`](crate::AppFramework) routes) alike.
    ///
    /// The rule is the one the REST callback applies with
    /// [`RestClientBuilder::default_allowed_mentions`](crate::http::RestClientBuilder::default_allowed_mentions):
    /// only `CHANNEL_MESSAGE_WITH_SOURCE` and `UPDATE_MESSAGE` (types 4 and
    /// 7) carry a message and get the default; a value the response sets
    /// itself (on an update, an explicit clear too) wins; and
    /// [`InteractionResponse::Raw`] is sent exactly as written. The endpoint
    /// has no [`RestClient`](crate::RestClient), so it does not see a
    /// client's default: give both the same value to answer the same way
    /// inline and over REST.
    ///
    /// [Decision Log] Default mentions on the inline transport
    /// - Purpose: an app that sets a mention policy once (for example "never
    ///   ping `@everyone`") gets it on every message it sends, whichever
    ///   transport carries the initial response.
    /// - Before: the endpoint encoded inline responses with no default, so a
    ///   `ChannelMessage` returned from a handler pinged what its content
    ///   mentioned while the same response sent with
    ///   `RestClient::create_interaction_response` did not; nothing could
    ///   configure the endpoint, which takes no `RestClient`.
    /// - Alternatives: (1) take a `RestClient` (or its default) in the
    ///   builder; (2) have handlers set `allowed_mentions` on each response;
    ///   (3) a builder setting passed to the encoder REST already uses.
    /// - Chosen: (3). The value goes to `encode_interaction_response`, the
    ///   function the REST callback calls with the client's default, so one
    ///   response and one policy encode to the same bytes either way.
    /// - Why: (1) ties an inbound-only endpoint to an outbound client it may
    ///   not otherwise need, and a later
    ///   [`RestClient::with_default_allowed_mentions`](crate::RestClient::with_default_allowed_mentions)
    ///   scope would not follow it anyway. (2) is what the missing default
    ///   forced, and one forgotten route pings.
    /// - Trade-offs: the policy is set in two places when an app also
    ///   responds over REST. Unset (the default) keeps the 3.0.0 bytes.
    pub fn default_allowed_mentions(mut self, allowed_mentions: AllowedMentions) -> Self {
        self.default_allowed_mentions = Some(allowed_mentions);
        self
    }

    /// Builds the route for a raw [`InteractionHandler`]. Fails when the
    /// public key is not a hex-encoded Ed25519 public key.
    pub fn build<H>(self, handler: H) -> Result<Router, DiscordError>
    where
        H: InteractionHandler + Clone + Send + Sync + 'static,
    {
        let verifying_key = decode_verifying_key(&self.public_key)?;

        Ok(Router::new()
            .route("/interactions", post(handle_interactions_request::<H>))
            .layer(DefaultBodyLimit::max(self.body_limit))
            .with_state(InteractionsState {
                verifying_key,
                handler,
                default_allowed_mentions: self.default_allowed_mentions.map(Arc::new),
            }))
    }

    /// Builds the route for a [`TypedInteractionHandler`] (an
    /// [`AppFramework`](crate::AppFramework), for example). Fails when the
    /// public key is not a hex-encoded Ed25519 public key.
    pub fn build_typed<H>(self, handler: H) -> Result<Router, DiscordError>
    where
        H: TypedInteractionHandler + Clone + Send + Sync + 'static,
    {
        let verifying_key = decode_verifying_key(&self.public_key)?;

        Ok(Router::new()
            .route(
                "/interactions",
                post(handle_typed_interactions_request::<H>),
            )
            .layer(DefaultBodyLimit::max(self.body_limit))
            .with_state(InteractionsState {
                verifying_key,
                handler,
                default_allowed_mentions: self.default_allowed_mentions.map(Arc::new),
            }))
    }
}

/// Provides the `try_interactions_endpoint` helper.
///
/// Reads request bodies up to [`DEFAULT_INTERACTION_BODY_LIMIT`]; use
/// [`InteractionsEndpointBuilder`] for another limit.
pub fn try_interactions_endpoint<H>(public_key: &str, handler: H) -> Result<Router, DiscordError>
where
    H: InteractionHandler + Clone + Send + Sync + 'static,
{
    InteractionsEndpointBuilder::new(public_key).build(handler)
}

/// Provides the `interactions_endpoint` helper.
///
/// # Panics
///
/// Panics if `public_key` is not a valid hex-encoded Ed25519 public key.
/// [`try_interactions_endpoint`] returns that error instead.
#[deprecated(
    since = "3.0.0",
    note = "panics on an invalid public key; use try_interactions_endpoint and handle the error"
)]
#[expect(
    clippy::panic,
    reason = "the documented contract of this deprecated wrapper"
)]
pub fn interactions_endpoint<H>(public_key: &str, handler: H) -> Router
where
    H: InteractionHandler + Clone + Send + Sync + 'static,
{
    match try_interactions_endpoint(public_key, handler) {
        Ok(router) => router,
        Err(error) => panic!("invalid Discord public key for interactions endpoint: {error}"),
    }
}

async fn handle_typed_interactions_request<H>(
    State(state): State<InteractionsState<H>>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse
where
    H: TypedInteractionHandler + Clone + Send + Sync + 'static,
{
    if let Err(status) =
        verify_discord_request_signature(&state.verifying_key, &headers, body.as_ref())
    {
        return status.into_response();
    }

    // Decoded straight from the body text: no `Value` tree, and the same
    // decoder instance as the gateway (see `parsers::decode_interaction`).
    let interaction = match body_json(body.as_ref())
        .and_then(|text| decode_interaction(text).map_err(|error| error.to_string()))
    {
        Ok(interaction) => interaction,
        Err(error) => {
            // Endpoint verification must get its PONG even if the PING
            // lacks a field the typed decoder requires (the check before
            // decoding used to read `type` alone).
            if is_ping(body.as_ref()) {
                return pong();
            }
            return bad_request(&error);
        }
    };
    if matches!(interaction, Interaction::Ping(_)) {
        return pong();
    }
    let context: InteractionContextData = interaction.context().clone();
    let interaction_id = context.id;
    let response_state = context.response_state.clone();

    let reply = state.handler.handle_typed(context, interaction).await;
    typed_endpoint_answer(
        &response_state,
        interaction_id,
        reply,
        state.default_allowed_mentions.as_deref(),
    )
    .into_response()
}

/// Answers a PING.
fn pong() -> Response {
    json_response(StatusCode::OK, PONG_BODY.to_vec())
}

/// Whether `body` is a JSON object whose `type` is PING, reading only that
/// key.
fn is_ping(body: &[u8]) -> bool {
    #[derive(serde::Deserialize)]
    struct TypeOnly {
        #[serde(rename = "type")]
        kind: InteractionType,
    }
    body_json(body)
        .ok()
        .and_then(|text| serde_json::from_str::<TypeOnly>(text).ok())
        .is_some_and(|payload| payload.kind == InteractionType::PING)
}

/// The request body as JSON text; Discord sends UTF-8.
fn body_json(body: &[u8]) -> Result<&str, String> {
    std::str::from_utf8(body).map_err(|error| format!("request body is not UTF-8: {error}"))
}

/// Decides which transport owns the initial response and answers Discord
/// accordingly (see the decision log in `response.rs`).
///
/// An inline initial response claims the shared state with the same
/// compare-and-set the REST methods use, so it is sent only when no REST
/// response was sent, is in flight, or may have landed. Otherwise REST owns
/// the response and Discord gets `202` with no body; sending the inline
/// response as well would be a second initial response, and its acceptance
/// by Discord would contradict whatever the REST attempt settles to.
///
/// The inline response is the body of Discord's own request, so there is no
/// delivery outcome to wait for: the state moves straight to acknowledged. A
/// follow-up sent by a retained clone in the moment before Discord reads the
/// body can still be rejected by Discord.
///
/// The response is encoded before the slot is claimed, as on the REST path:
/// a response that cannot be encoded answers 500 and claims nothing, so the
/// state never records an initial response Discord did not get. When REST
/// owns the response, the encoded bytes are dropped unsent.
fn typed_endpoint_answer(
    state: &InteractionResponseState,
    interaction_id: InteractionId,
    reply: EndpointReply,
    default_mentions: Option<&AllowedMentions>,
) -> EndpointAnswer {
    let EndpointReply::Inline(response) = reply else {
        return EndpointAnswer::Accepted;
    };
    let answer = EndpointAnswer::encode(&response, default_mentions);
    if !matches!(answer, EndpointAnswer::Inline(_)) {
        return answer;
    }
    let inline_owns = match AckKind::for_callback_type(response.kind()) {
        Some(kind) => state.commit_inline(kind),
        // `PONG` is not an initial response: it cannot compete for the
        // slot, so only REST ownership matters.
        None => state.phase() == InteractionResponsePhase::Unacknowledged,
    };
    if inline_owns {
        return answer;
    }
    warn!(
        interaction_id = %interaction_id,
        phase = ?state.phase(),
        "discarding the handler's inline response: the initial response is handled over REST, \
         so Discord is answered with 202 and no body (return EndpointReply::Accepted to \
         silence this)"
    );
    EndpointAnswer::Accepted
}

/// Provides the `try_typed_interactions_endpoint` helper.
///
/// Reads request bodies up to [`DEFAULT_INTERACTION_BODY_LIMIT`]; use
/// [`InteractionsEndpointBuilder`] for another limit.
pub fn try_typed_interactions_endpoint<H>(
    public_key: &str,
    handler: H,
) -> Result<Router, DiscordError>
where
    H: TypedInteractionHandler + Clone + Send + Sync + 'static,
{
    InteractionsEndpointBuilder::new(public_key).build_typed(handler)
}

/// Provides the `typed_interactions_endpoint` helper.
///
/// # Panics
///
/// Panics if `public_key` is not a valid hex-encoded Ed25519 public key.
/// [`try_typed_interactions_endpoint`] returns that error instead.
#[deprecated(
    since = "3.0.0",
    note = "panics on an invalid public key; use try_typed_interactions_endpoint and handle the \
            error"
)]
#[expect(
    clippy::panic,
    reason = "the documented contract of this deprecated wrapper"
)]
pub fn typed_interactions_endpoint<H>(public_key: &str, handler: H) -> Router
where
    H: TypedInteractionHandler + Clone + Send + Sync + 'static,
{
    match try_typed_interactions_endpoint(public_key, handler) {
        Ok(router) => router,
        Err(error) => {
            panic!("invalid Discord public key for typed interactions endpoint: {error}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        current_unix_timestamp, decode_verifying_key, handle_interactions_request,
        handle_typed_interactions_request, try_interactions_endpoint,
        try_typed_interactions_endpoint, typed_endpoint_answer,
        verify_discord_request_signature_at_time, verify_discord_signature,
        verify_signature_with_key_at_time, EndpointAnswer, InteractionHandler,
        InteractionsEndpointBuilder, InteractionsState, TypedInteractionHandler,
        DEFAULT_INTERACTION_BODY_LIMIT, PONG_BODY, SIGNATURE_TIMESTAMP_TOLERANCE_SECS,
    };
    use crate::builders::ModalBuilder;
    use crate::framework::{AppContext, AppFramework};
    use crate::http::RestClient;
    use crate::model::{
        test_id, AllowedMentions, ApplicationCommandOptionChoice, Interaction,
        InteractionContextData, InteractionResponseState, InteractionResponseType,
    };
    use crate::parsers::{InteractionContext, RawInteraction};
    use crate::request::{
        encode_interaction_response, EndpointReply, InteractionMessage, InteractionResponse,
        RawInteractionResponse, UpdateMessage,
    };
    use crate::response::{
        AckKind, CallbackOutcome, InteractionResponder, InteractionResponsePhase,
    };
    use axum::{
        body::{to_bytes, Bytes},
        extract::State,
        http::{HeaderMap, HeaderValue, StatusCode},
        response::IntoResponse,
    };
    use ed25519_dalek::{Signer, SigningKey, VerifyingKey};
    use serde_json::{json, Value};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    };
    const TEST_NOW_UNIX_TIMESTAMP: i64 = 1_700_000_000;

    fn inline_payload(response: InteractionResponse) -> Value {
        serde_json::from_slice(&encode_interaction_response(&response, None).unwrap()).unwrap()
    }

    /// A hand-written callback from its whole JSON (`{"type": N, "data": ..}`).
    fn raw(payload: &Value) -> InteractionResponse {
        let kind = u8::try_from(payload["type"].as_u64().expect("numeric type")).unwrap();
        let mut raw = RawInteractionResponse::new(InteractionResponseType::new(kind));
        if let Some(data) = payload.get("data") {
            raw = raw.data(data).unwrap();
        }
        InteractionResponse::Raw(raw)
    }

    #[derive(Clone)]
    struct TestHandler;

    impl InteractionHandler for TestHandler {
        async fn handle(
            &self,
            _ctx: InteractionContext,
            _interaction: RawInteraction,
        ) -> EndpointReply {
            EndpointReply::Accepted
        }
    }

    #[test]
    fn try_interactions_endpoint_rejects_invalid_public_key() {
        assert!(try_interactions_endpoint("bad-key", TestHandler).is_err());
    }

    #[test]
    fn try_interactions_endpoint_accepts_valid_public_key() {
        let public_key = "0000000000000000000000000000000000000000000000000000000000000000";
        assert!(try_interactions_endpoint(public_key, TestHandler).is_ok());
    }

    #[derive(Clone)]
    struct TypedTestHandler;

    impl TypedInteractionHandler for TypedTestHandler {
        async fn handle_typed(
            &self,
            _ctx: InteractionContextData,
            _interaction: Interaction,
        ) -> EndpointReply {
            EndpointReply::Accepted
        }
    }

    #[derive(Clone, Default)]
    struct RecordingRawHandler {
        calls: Arc<AtomicUsize>,
        records: Arc<Mutex<Vec<(InteractionContext, RawInteraction)>>>,
        response: Value,
    }

    impl InteractionHandler for RecordingRawHandler {
        async fn handle(
            &self,
            ctx: InteractionContext,
            interaction: RawInteraction,
        ) -> EndpointReply {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.records.lock().unwrap().push((ctx, interaction));
            raw(&self.response).into()
        }
    }

    #[derive(Clone, Default)]
    struct RecordingTypedHandler {
        calls: Arc<AtomicUsize>,
        records: Arc<Mutex<Vec<(InteractionContextData, Interaction)>>>,
        response: Value,
    }

    impl TypedInteractionHandler for RecordingTypedHandler {
        async fn handle_typed(
            &self,
            ctx: InteractionContextData,
            interaction: Interaction,
        ) -> EndpointReply {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.records.lock().unwrap().push((ctx, interaction));
            raw(&self.response).into()
        }
    }

    #[test]
    fn try_typed_interactions_endpoint_accepts_valid_public_key() {
        let public_key = "0000000000000000000000000000000000000000000000000000000000000000";
        assert!(try_typed_interactions_endpoint(public_key, TypedTestHandler).is_ok());
    }

    #[test]
    fn pong_body_is_the_encoded_pong() {
        assert_eq!(
            encode_interaction_response(&InteractionResponse::Pong, None).unwrap(),
            PONG_BODY
        );
    }

    #[test]
    fn interaction_response_payload_supports_deferred_update() {
        let payload = inline_payload(InteractionResponse::DeferredUpdateMessage);
        assert_eq!(payload["type"], 6);
    }

    #[test]
    fn interaction_response_payload_supports_autocomplete_choices() {
        let payload = inline_payload(InteractionResponse::AutocompleteResult(vec![
            ApplicationCommandOptionChoice {
                name: "Support".to_string(),
                name_localizations: None,
                value: "support".into(),
            },
        ]));

        assert_eq!(payload["type"], 8);
        assert_eq!(payload["data"]["choices"][0]["name"], "Support");
        assert_eq!(payload["data"]["choices"][0]["value"], "support");
    }

    #[test]
    fn interaction_response_payload_supports_launch_activity() {
        let payload = inline_payload(InteractionResponse::LaunchActivity);
        assert_eq!(payload["type"], 12);
    }

    fn test_signing_key() -> SigningKey {
        SigningKey::from_bytes(&[7_u8; 32])
    }

    fn sign_request(timestamp: &str, body: &[u8]) -> (VerifyingKey, String) {
        let signing_key = test_signing_key();
        let verifying_key = signing_key.verifying_key();
        let mut signed_payload = Vec::with_capacity(timestamp.len() + body.len());
        signed_payload.extend_from_slice(timestamp.as_bytes());
        signed_payload.extend_from_slice(body);
        let signature = signing_key.sign(&signed_payload);

        (verifying_key, crate::hex::encode(&signature.to_bytes()))
    }

    fn signed_headers(signature: &str, timestamp: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-signature-ed25519",
            HeaderValue::from_str(signature).expect("signature header"),
        );
        headers.insert(
            "x-signature-timestamp",
            HeaderValue::from_str(timestamp).expect("timestamp header"),
        );
        headers
    }

    fn signed_headers_for_body(body: &[u8]) -> HeaderMap {
        let timestamp = current_unix_timestamp().unwrap().to_string();
        let (_, signature) = sign_request(&timestamp, body);
        signed_headers(&signature, &timestamp)
    }

    async fn json_response(response: impl IntoResponse) -> Value {
        let response = response.into_response();
        serde_json::from_slice(
            &to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("response body"),
        )
        .expect("json body")
    }

    #[test]
    fn verify_signature_with_key_at_time_accepts_fresh_timestamp() {
        let body = br#"{"type":1}"#;
        let timestamp = TEST_NOW_UNIX_TIMESTAMP.to_string();
        let (verifying_key, signature) = sign_request(&timestamp, body);

        assert!(verify_signature_with_key_at_time(
            &verifying_key,
            &signature,
            &timestamp,
            body,
            TEST_NOW_UNIX_TIMESTAMP,
        )
        .is_ok());
    }

    #[test]
    fn verify_signature_with_key_at_time_rejects_stale_timestamp() {
        let body = br#"{"type":1}"#;
        let timestamp =
            (TEST_NOW_UNIX_TIMESTAMP - SIGNATURE_TIMESTAMP_TOLERANCE_SECS - 1).to_string();
        let (verifying_key, signature) = sign_request(&timestamp, body);

        let error = verify_signature_with_key_at_time(
            &verifying_key,
            &signature,
            &timestamp,
            body,
            TEST_NOW_UNIX_TIMESTAMP,
        )
        .expect_err("stale timestamps should be rejected");

        assert!(error.to_string().contains("freshness window"));
    }

    #[test]
    fn verify_signature_with_key_at_time_rejects_future_timestamp() {
        let body = br#"{"type":1}"#;
        let timestamp =
            (TEST_NOW_UNIX_TIMESTAMP + SIGNATURE_TIMESTAMP_TOLERANCE_SECS + 1).to_string();
        let (verifying_key, signature) = sign_request(&timestamp, body);

        let error = verify_signature_with_key_at_time(
            &verifying_key,
            &signature,
            &timestamp,
            body,
            TEST_NOW_UNIX_TIMESTAMP,
        )
        .expect_err("future timestamps should be rejected");

        assert!(error.to_string().contains("freshness window"));
    }

    #[test]
    fn verify_signature_with_key_at_time_rejects_invalid_timestamp() {
        let body = br#"{"type":1}"#;
        let timestamp = "not-a-timestamp";
        let (verifying_key, signature) = sign_request(timestamp, body);

        let error = verify_signature_with_key_at_time(
            &verifying_key,
            &signature,
            timestamp,
            body,
            TEST_NOW_UNIX_TIMESTAMP,
        )
        .expect_err("invalid timestamps should be rejected");

        assert!(error.to_string().contains("unix timestamp"));
    }

    #[test]
    fn verify_discord_request_signature_rejects_missing_headers() {
        let headers = HeaderMap::new();
        let body = br#"{"type":1}"#;
        let signing_key = test_signing_key();
        let verifying_key = signing_key.verifying_key();

        assert_eq!(
            verify_discord_request_signature_at_time(
                &verifying_key,
                &headers,
                body,
                TEST_NOW_UNIX_TIMESTAMP,
            ),
            Err(StatusCode::UNAUTHORIZED)
        );
    }

    #[test]
    fn verify_discord_request_signature_rejects_invalid_signature() {
        let body = br#"{"type":1}"#;
        let timestamp = TEST_NOW_UNIX_TIMESTAMP.to_string();
        let signing_key = test_signing_key();
        let verifying_key = signing_key.verifying_key();
        let headers = signed_headers(
            "0000000000000000000000000000000000000000000000000000000000000000",
            &timestamp,
        );

        assert_eq!(
            verify_discord_request_signature_at_time(
                &verifying_key,
                &headers,
                body,
                TEST_NOW_UNIX_TIMESTAMP,
            ),
            Err(StatusCode::UNAUTHORIZED)
        );
    }

    #[test]
    fn verify_discord_request_signature_rejects_stale_timestamp() {
        let body = br#"{"type":1}"#;
        let timestamp =
            (TEST_NOW_UNIX_TIMESTAMP - SIGNATURE_TIMESTAMP_TOLERANCE_SECS - 1).to_string();
        let (verifying_key, signature) = sign_request(&timestamp, body);
        let headers = signed_headers(&signature, &timestamp);

        assert_eq!(
            verify_discord_request_signature_at_time(
                &verifying_key,
                &headers,
                body,
                TEST_NOW_UNIX_TIMESTAMP,
            ),
            Err(StatusCode::UNAUTHORIZED)
        );
    }

    #[test]
    fn verify_discord_request_signature_rejects_future_timestamp() {
        let body = br#"{"type":1}"#;
        let timestamp =
            (TEST_NOW_UNIX_TIMESTAMP + SIGNATURE_TIMESTAMP_TOLERANCE_SECS + 1).to_string();
        let (verifying_key, signature) = sign_request(&timestamp, body);
        let headers = signed_headers(&signature, &timestamp);

        assert_eq!(
            verify_discord_request_signature_at_time(
                &verifying_key,
                &headers,
                body,
                TEST_NOW_UNIX_TIMESTAMP,
            ),
            Err(StatusCode::UNAUTHORIZED)
        );
    }

    #[test]
    fn verify_discord_signature_accepts_signed_requests() {
        let body = br#"{"type":1}"#;
        let timestamp = current_unix_timestamp().unwrap().to_string();
        let (verifying_key, signature) = sign_request(&timestamp, body);

        assert!(verify_discord_signature(
            &crate::hex::encode(verifying_key.as_bytes()),
            &signature,
            &timestamp,
            body
        )
        .is_ok());
    }

    #[test]
    fn decode_verifying_key_rejects_invalid_lengths() {
        let error = decode_verifying_key("abcd").unwrap_err();
        assert!(error.to_string().contains("32 bytes"));
    }

    #[test]
    fn decode_verifying_key_accepts_uppercase_and_rejects_malformed_hex() {
        let key_hex = crate::hex::encode(test_signing_key().verifying_key().as_bytes());
        assert!(decode_verifying_key(&key_hex.to_ascii_uppercase()).is_ok());

        let odd = decode_verifying_key(&key_hex[1..]).unwrap_err();
        assert!(matches!(odd, crate::DiscordError::Model { .. }));
        assert!(odd
            .to_string()
            .contains("invalid public key hex: odd number of hex digits"));

        let non_hex = decode_verifying_key(&format!("zz{}", &key_hex[2..])).unwrap_err();
        assert!(non_hex
            .to_string()
            .contains("invalid public key hex: invalid hex character 'z' at position 0"));
    }

    #[test]
    fn signature_verification_rejects_malformed_signature_hex() {
        let body = br#"{"type":1}"#;
        let timestamp = TEST_NOW_UNIX_TIMESTAMP.to_string();
        let (verifying_key, signature) = sign_request(&timestamp, body);

        let uppercase = signature.to_ascii_uppercase();
        assert!(verify_signature_with_key_at_time(
            &verifying_key,
            &uppercase,
            &timestamp,
            body,
            TEST_NOW_UNIX_TIMESTAMP
        )
        .is_ok());

        let odd = verify_signature_with_key_at_time(
            &verifying_key,
            &signature[1..],
            &timestamp,
            body,
            TEST_NOW_UNIX_TIMESTAMP,
        )
        .unwrap_err();
        assert!(odd.to_string().contains("invalid signature hex"));

        let non_hex = verify_signature_with_key_at_time(
            &verifying_key,
            &format!("{}gg", &signature[2..]),
            &timestamp,
            body,
            TEST_NOW_UNIX_TIMESTAMP,
        )
        .unwrap_err();
        assert!(non_hex
            .to_string()
            .contains("invalid signature hex: invalid hex character 'g'"));
    }

    #[test]
    fn verify_discord_request_signature_accepts_valid_headers() {
        let body = br#"{"type":1}"#;
        let timestamp = TEST_NOW_UNIX_TIMESTAMP.to_string();
        let (verifying_key, signature) = sign_request(&timestamp, body);
        let headers = signed_headers(&signature, &timestamp);

        assert_eq!(
            verify_discord_request_signature_at_time(
                &verifying_key,
                &headers,
                body,
                TEST_NOW_UNIX_TIMESTAMP,
            ),
            Ok(())
        );
    }

    #[test]
    fn interaction_response_payload_covers_remaining_response_variants() {
        assert_eq!(
            inline_payload(InteractionResponse::Pong),
            json!({ "type": 1 })
        );
        assert_eq!(
            inline_payload(InteractionResponse::reply("hello")),
            json!({
                "type": 4,
                "data": { "content": "hello" }
            })
        );
        assert_eq!(
            inline_payload(InteractionResponse::update("updated")),
            json!({
                "type": 7,
                "data": { "content": "updated" }
            })
        );
        assert_eq!(
            inline_payload(InteractionResponse::modal(ModalBuilder::new(
                "feedback", "Feedback"
            )))["type"],
            json!(9)
        );
        assert_eq!(
            inline_payload(raw(&json!({ "type": 99 }))),
            json!({ "type": 99 })
        );
    }

    #[tokio::test]
    async fn raw_handler_rejects_bad_signature_json_type_context_and_parse_errors() {
        let handler = RecordingRawHandler {
            response: json!({ "type": 4 }),
            ..Default::default()
        };
        let verifying_key = test_signing_key().verifying_key();

        let unauthorized = handle_interactions_request(
            State(InteractionsState {
                verifying_key,
                handler: handler.clone(),
                default_allowed_mentions: None,
            }),
            HeaderMap::new(),
            Bytes::from_static(br#"{"type":1}"#),
        )
        .await
        .into_response();
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

        let bad_json_body = br#"{"type":"#.to_vec();
        let bad_json = handle_interactions_request(
            State(InteractionsState {
                verifying_key,
                handler: handler.clone(),
                default_allowed_mentions: None,
            }),
            signed_headers_for_body(&bad_json_body),
            Bytes::from(bad_json_body),
        )
        .await;
        assert!(json_response(bad_json).await["error"].as_str().is_some());

        let bad_type_body = serde_json::to_vec(&json!({
            "id": "1",
            "application_id": "2",
            "token": "token",
            "type": "bad"
        }))
        .unwrap();
        let bad_type = handle_interactions_request(
            State(InteractionsState {
                verifying_key,
                handler: handler.clone(),
                default_allowed_mentions: None,
            }),
            signed_headers_for_body(&bad_type_body),
            Bytes::from(bad_type_body),
        )
        .await;
        assert_eq!(
            json_response(bad_type).await,
            json!({ "error": "missing or invalid interaction.type" })
        );

        let bad_context_body = serde_json::to_vec(&json!({
            "application_id": "2",
            "token": "token",
            "type": 2,
            "data": {}
        }))
        .unwrap();
        let bad_context = handle_interactions_request(
            State(InteractionsState {
                verifying_key,
                handler: handler.clone(),
                default_allowed_mentions: None,
            }),
            signed_headers_for_body(&bad_context_body),
            Bytes::from(bad_context_body),
        )
        .await;
        assert!(json_response(bad_context)
            .await
            .get("error")
            .and_then(Value::as_str)
            .unwrap()
            .contains("missing or invalid interaction.id"));

        let bad_parse_body = serde_json::to_vec(&json!({
            "id": "1",
            "application_id": "2",
            "token": "token",
            "type": 9,
            "data": {}
        }))
        .unwrap();
        let bad_parse = handle_interactions_request(
            State(InteractionsState {
                verifying_key,
                handler: handler.clone(),
                default_allowed_mentions: None,
            }),
            signed_headers_for_body(&bad_parse_body),
            Bytes::from(bad_parse_body),
        )
        .await;
        assert!(json_response(bad_parse)
            .await
            .get("error")
            .and_then(Value::as_str)
            .unwrap()
            .contains("unsupported interaction type"));

        assert_eq!(handler.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn raw_handler_short_circuits_ping_and_dispatches_commands() {
        let handler = RecordingRawHandler {
            response: json!({
                "type": 7,
                "data": { "content": "raw ok" }
            }),
            ..Default::default()
        };
        let verifying_key = test_signing_key().verifying_key();

        let ping_body = serde_json::to_vec(&json!({
            "id": "1",
            "application_id": "2",
            "token": "token",
            "type": 1
        }))
        .unwrap();
        let ping = handle_interactions_request(
            State(InteractionsState {
                verifying_key,
                handler: handler.clone(),
                default_allowed_mentions: None,
            }),
            signed_headers_for_body(&ping_body),
            Bytes::from(ping_body),
        )
        .await;
        assert_eq!(json_response(ping).await, json!({ "type": 1 }));
        assert_eq!(handler.calls.load(Ordering::SeqCst), 0);

        let command_body = serde_json::to_vec(&json!({
            "id": "100",
            "application_id": "200",
            "token": "interaction_token",
            "guild_id": "300",
            "channel_id": "400",
            "member": {
                "user": {
                    "id": "500",
                    "username": "tester"
                }
            },
            "type": 2,
            "data": {
                "id": "600",
                "name": "ticket",
                "type": 1,
                "options": []
            }
        }))
        .unwrap();
        let command = handle_interactions_request(
            State(InteractionsState {
                verifying_key,
                handler: handler.clone(),
                default_allowed_mentions: None,
            }),
            signed_headers_for_body(&command_body),
            Bytes::from(command_body),
        )
        .await;
        assert_eq!(
            json_response(command).await,
            json!({
                "type": 7,
                "data": { "content": "raw ok" }
            })
        );
        assert_eq!(handler.calls.load(Ordering::SeqCst), 1);
        let records = handler.records.lock().unwrap();
        let (ctx, interaction) = &records[0];
        assert_eq!(ctx.id, test_id(100));
        assert_eq!(ctx.application_id, test_id(200));
        assert_eq!(ctx.user_id, Some(test_id(500)));
        assert!(matches!(interaction, RawInteraction::Command { .. }));
    }

    #[tokio::test]
    async fn typed_handler_short_circuits_ping_and_reports_parse_errors() {
        let handler = RecordingTypedHandler {
            response: json!({
                "type": 4,
                "data": { "content": "typed ok" }
            }),
            ..Default::default()
        };
        let verifying_key = test_signing_key().verifying_key();

        let ping_body = serde_json::to_vec(&json!({
            "id": "10",
            "application_id": "20",
            "token": "token",
            "type": 1
        }))
        .unwrap();
        let ping = handle_typed_interactions_request(
            State(InteractionsState {
                verifying_key,
                handler: handler.clone(),
                default_allowed_mentions: None,
            }),
            signed_headers_for_body(&ping_body),
            Bytes::from(ping_body),
        )
        .await;
        assert_eq!(json_response(ping).await, json!({ "type": 1 }));
        assert_eq!(handler.calls.load(Ordering::SeqCst), 0);

        let parse_error_body = serde_json::to_vec(&json!({
            "id": "12",
            "application_id": "23",
            "token": "typed_token",
            "type": 3,
            "data": {
                "component_type": 2
            }
        }))
        .unwrap();
        let parse_error = handle_typed_interactions_request(
            State(InteractionsState {
                verifying_key,
                handler: handler.clone(),
                default_allowed_mentions: None,
            }),
            signed_headers_for_body(&parse_error_body),
            Bytes::from(parse_error_body),
        )
        .await;
        assert!(json_response(parse_error)
            .await
            .get("error")
            .and_then(Value::as_str)
            .unwrap()
            .contains("invalid interaction.data: missing field `custom_id`"));

        // A PING the typed decoder rejects (here: no id or token) still
        // gets its PONG, as before the endpoint decoded the whole body.
        let bare_ping = serde_json::to_vec(&json!({ "type": 1 })).unwrap();
        let bare_ping = handle_typed_interactions_request(
            State(InteractionsState {
                verifying_key,
                handler: handler.clone(),
                default_allowed_mentions: None,
            }),
            signed_headers_for_body(&bare_ping),
            Bytes::from(bare_ping),
        )
        .await;
        assert_eq!(json_response(bare_ping).await, json!({ "type": 1 }));
        assert_eq!(handler.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn typed_handler_dispatches_component_and_unknown_interactions() {
        let handler = RecordingTypedHandler {
            response: json!({
                "type": 4,
                "data": { "content": "typed ok" }
            }),
            ..Default::default()
        };
        let verifying_key = test_signing_key().verifying_key();

        let component_body = serde_json::to_vec(&json!({
            "id": "11",
            "application_id": "22",
            "token": "typed_token",
            "channel_id": "33",
            "user": {
                "id": "44",
                "username": "typed-user"
            },
            "type": 3,
            "data": {
                "custom_id": "approve",
                "component_type": 2,
                "values": ["yes"]
            }
        }))
        .unwrap();
        let component = handle_typed_interactions_request(
            State(InteractionsState {
                verifying_key,
                handler: handler.clone(),
                default_allowed_mentions: None,
            }),
            signed_headers_for_body(&component_body),
            Bytes::from(component_body),
        )
        .await;
        assert_eq!(
            json_response(component).await,
            json!({
                "type": 4,
                "data": { "content": "typed ok" }
            })
        );

        let unknown_body = serde_json::to_vec(&json!({
            "id": "13",
            "application_id": "24",
            "token": "typed_token",
            "guild_id": "35",
            "type": 42,
            "data": {
                "opaque": true
            }
        }))
        .unwrap();
        let unknown = handle_typed_interactions_request(
            State(InteractionsState {
                verifying_key,
                handler: handler.clone(),
                default_allowed_mentions: None,
            }),
            signed_headers_for_body(&unknown_body),
            Bytes::from(unknown_body),
        )
        .await;
        assert_eq!(
            json_response(unknown).await,
            json!({
                "type": 4,
                "data": { "content": "typed ok" }
            })
        );

        let records = handler.records.lock().unwrap();
        assert_eq!(records.len(), 2);
        assert!(matches!(records[0].1, Interaction::Component(_)));
        assert!(matches!(records[1].1, Interaction::Unknown { .. }));
    }

    fn answer(state: &InteractionResponseState, reply: impl Into<EndpointReply>) -> EndpointAnswer {
        typed_endpoint_answer(state, test_id(1), reply.into(), None)
    }

    #[test]
    fn inline_responses_claim_the_state_with_the_matching_phase() {
        let cases = [
            (
                InteractionResponse::Pong,
                InteractionResponsePhase::Unacknowledged,
            ),
            (
                InteractionResponse::reply("hi"),
                InteractionResponsePhase::Replied,
            ),
            (
                InteractionResponse::defer(false),
                InteractionResponsePhase::Deferred,
            ),
            (
                InteractionResponse::DeferredUpdateMessage,
                InteractionResponsePhase::Deferred,
            ),
            (
                InteractionResponse::AutocompleteResult(Vec::new()),
                InteractionResponsePhase::Replied,
            ),
            (
                InteractionResponse::modal(ModalBuilder::new("m", "M")),
                InteractionResponsePhase::Replied,
            ),
            (
                InteractionResponse::update("new"),
                InteractionResponsePhase::Replied,
            ),
            (
                InteractionResponse::LaunchActivity,
                InteractionResponsePhase::Replied,
            ),
            (
                raw(&json!({ "type": 6 })),
                InteractionResponsePhase::Deferred,
            ),
            (
                raw(&json!({ "type": 4, "data": {} })),
                InteractionResponsePhase::Replied,
            ),
            (
                raw(&json!({ "type": 1 })),
                InteractionResponsePhase::Unacknowledged,
            ),
        ];
        for (response, expected) in cases {
            let state = InteractionResponseState::new();
            assert!(matches!(
                answer(&state, response),
                EndpointAnswer::Inline(_)
            ));
            assert_eq!(state.phase(), expected);
        }
    }

    #[test]
    fn accepted_answers_202_and_leaves_the_slot_to_rest() {
        let state = InteractionResponseState::new();
        assert_eq!(
            answer(&state, EndpointReply::Accepted),
            EndpointAnswer::Accepted
        );
        assert_eq!(state.phase(), InteractionResponsePhase::Unacknowledged);
        // A REST response sent later (e.g. from a spawned task) owns it.
        assert!(state.begin(AckKind::Deferred).is_ok());
    }

    /// Regression: when REST already owned the initial response, the endpoint
    /// still answered 200 with the handler's payload, i.e. a second initial
    /// response.
    #[test]
    fn inline_response_yields_to_a_rest_owned_initial_response() {
        let sent = InteractionResponseState::new();
        sent.begin(AckKind::Deferred)
            .unwrap()
            .settle(CallbackOutcome::Accepted);
        assert_eq!(
            answer(&sent, InteractionResponse::reply("late")),
            EndpointAnswer::Accepted
        );
        assert!(sent.is_deferred());

        // In flight: the inline response must not be sent, so a later REST
        // rejection truthfully leaves nothing acknowledged.
        let in_flight = InteractionResponseState::new();
        let claim = in_flight.begin(AckKind::Replied).unwrap();
        assert_eq!(
            answer(&in_flight, InteractionResponse::defer(false)),
            EndpointAnswer::Accepted
        );
        claim.settle(CallbackOutcome::Rejected);
        assert_eq!(in_flight.phase(), InteractionResponsePhase::Unacknowledged);

        let uncertain = InteractionResponseState::new();
        uncertain
            .begin(AckKind::Replied)
            .unwrap()
            .settle(CallbackOutcome::Ambiguous);
        assert_eq!(
            answer(&uncertain, InteractionResponse::Pong),
            EndpointAnswer::Accepted
        );
        assert_eq!(uncertain.phase(), InteractionResponsePhase::Uncertain);
    }

    fn command_body(token: &str) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "id": "100",
            "application_id": "200",
            "token": token,
            "type": 2,
            "user": { "id": "500", "username": "tester" },
            "data": { "id": "600", "name": "slow", "type": 1 }
        }))
        .unwrap()
    }

    /// Regression: the typed endpoint answered inline without recording it,
    /// so a context retained by a spawned task still looked unacknowledged
    /// and `follow_up` failed locally. The retained `AppContext` must now see
    /// the deferral and pass the local check (the bad token then stops the
    /// request before the network).
    #[tokio::test]
    async fn typed_endpoint_marks_retained_framework_contexts_acknowledged() {
        let retained: Arc<Mutex<Option<AppContext>>> = Arc::default();
        let framework = AppFramework::builder()
            .command("slow", {
                let retained = Arc::clone(&retained);
                move |ctx: AppContext| {
                    *retained.lock().unwrap() = Some(ctx);
                    async { InteractionResponse::defer(false) }
                }
            })
            .build();
        let body = command_body("bad/token");

        let response = handle_typed_interactions_request(
            State(InteractionsState {
                verifying_key: test_signing_key().verifying_key(),
                handler: framework,
                default_allowed_mentions: None,
            }),
            signed_headers_for_body(&body),
            Bytes::from(body),
        )
        .await;
        assert_eq!(
            json_response(response).await,
            json!({ "type": 5, "data": { "flags": 0 } })
        );

        let ctx = retained.lock().unwrap().take().expect("route ran");
        assert!(ctx.is_deferred());
        assert!(ctx.interaction.is_deferred());
        let error = ctx
            .follow_up(&RestClient::new("bot-token", Some(test_id(200))), "done")
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("interaction_token"),
            "follow-up should pass the acknowledgement check: {error}"
        );
    }

    #[derive(Clone)]
    struct RestThenInlineHandler;

    impl TypedInteractionHandler for RestThenInlineHandler {
        async fn handle_typed(
            &self,
            ctx: InteractionContextData,
            _interaction: Interaction,
        ) -> EndpointReply {
            // Stands in for a successful `ctx.defer(&http)` over REST.
            ctx.response_state
                .begin(AckKind::Deferred)
                .unwrap()
                .settle(CallbackOutcome::Accepted);
            InteractionResponse::reply("inline").into()
        }
    }

    /// Discord: "If you send this request for an interaction received over
    /// HTTP, respond to the original HTTP request with a 202 and no body."
    #[tokio::test]
    async fn typed_endpoint_answers_202_without_body_when_rest_owns_the_response() {
        let body = command_body("token");
        let response = handle_typed_interactions_request(
            State(InteractionsState {
                verifying_key: test_signing_key().verifying_key(),
                handler: RestThenInlineHandler,
                default_allowed_mentions: None,
            }),
            signed_headers_for_body(&body),
            Bytes::from(body),
        )
        .await
        .into_response();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert!(body.is_empty());
    }

    /// Tries to reply over REST with a file whose content type is invalid,
    /// then falls back to an inline reply.
    #[derive(Clone)]
    struct LocalRestFailureHandler {
        http: RestClient,
    }

    impl TypedInteractionHandler for LocalRestFailureHandler {
        async fn handle_typed(
            &self,
            ctx: InteractionContextData,
            _interaction: Interaction,
        ) -> EndpointReply {
            let files = [crate::http::FileAttachment::new("a.txt", b"a".to_vec())
                .with_content_type("not a mime")];
            let error = ctx
                .reply_with_files(&self.http, "with a file", &files)
                .await
                .expect_err("invalid content type");
            assert!(
                error.to_string().contains("invalid content type"),
                "{error}"
            );
            InteractionResponse::reply("fallback").into()
        }
    }

    /// Regression (astra review): a REST reply that failed locally used to
    /// leave the slot uncertain, so the endpoint discarded the inline
    /// fallback and answered 202, and the interaction failed. Nothing is
    /// sent, the slot stays free, and the fallback is answered with 200.
    #[tokio::test]
    async fn local_rest_failure_leaves_the_inline_fallback_in_charge() {
        let mut server = crate::response::tests::mock::spawn(Vec::new()).await;
        let http = RestClient::builder("bot-token", Some(test_id(200)))
            .api_base(server.base_url.clone())
            .build()
            .unwrap();
        let body = command_body("token");
        let response = handle_typed_interactions_request(
            State(InteractionsState {
                verifying_key: test_signing_key().verifying_key(),
                handler: LocalRestFailureHandler { http },
                default_allowed_mentions: None,
            }),
            signed_headers_for_body(&body),
            Bytes::from(body),
        )
        .await
        .into_response();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            json_response(response).await,
            json!({ "type": 4, "data": { "content": "fallback" } })
        );
        assert!(!server.has_pending_request(), "nothing may reach Discord");
    }

    #[derive(Clone)]
    struct AcceptedRawHandler;

    impl InteractionHandler for AcceptedRawHandler {
        async fn handle(
            &self,
            _ctx: InteractionContext,
            _interaction: RawInteraction,
        ) -> EndpointReply {
            EndpointReply::Accepted
        }
    }

    #[tokio::test]
    async fn raw_endpoint_answers_202_without_body_for_accepted() {
        let body = command_body("token");
        let response = handle_interactions_request(
            State(InteractionsState {
                verifying_key: test_signing_key().verifying_key(),
                handler: AcceptedRawHandler,
                default_allowed_mentions: None,
            }),
            signed_headers_for_body(&body),
            Bytes::from(body),
        )
        .await
        .into_response();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert!(body.is_empty());
    }

    #[tokio::test]
    async fn typed_endpoint_records_raw_responses_for_the_handler_context() {
        let handler = RecordingTypedHandler {
            response: json!({ "type": 4, "data": { "content": "typed ok" } }),
            ..Default::default()
        };
        let body = command_body("token");
        handle_typed_interactions_request(
            State(InteractionsState {
                verifying_key: test_signing_key().verifying_key(),
                handler: handler.clone(),
                default_allowed_mentions: None,
            }),
            signed_headers_for_body(&body),
            Bytes::from(body),
        )
        .await;

        let records = handler.records.lock().unwrap();
        let (ctx, interaction) = &records[0];
        assert!(ctx.is_replied());
        assert!(interaction.is_replied());
    }

    /// A component interaction like the one the old 64 KiB cap rejected: a
    /// channel select whose 25 resolved forum channels each carry a
    /// 4,096-character topic.
    fn large_channel_select_body() -> Vec<u8> {
        let ids: Vec<String> = (0..25).map(|index| (1000 + index).to_string()).collect();
        let channels: serde_json::Map<String, Value> = ids
            .iter()
            .map(|id| {
                (
                    id.clone(),
                    json!({
                        "id": id,
                        "name": format!("forum-{id}"),
                        "type": 15,
                        "permissions": "0",
                        "topic": "t".repeat(4096)
                    }),
                )
            })
            .collect();
        serde_json::to_vec(&json!({
            "id": "11",
            "application_id": "22",
            "token": "typed_token",
            "channel_id": "33",
            "user": { "id": "44", "username": "typed-user" },
            "type": 3,
            "data": {
                "custom_id": "pick",
                "component_type": 8,
                "values": ids,
                "resolved": { "channels": channels }
            }
        }))
        .unwrap()
    }

    /// `body` padded with trailing JSON whitespace to exactly `len` bytes.
    fn padded(mut body: Vec<u8>, len: usize) -> Vec<u8> {
        assert!(body.len() <= len);
        body.resize(len, b' ');
        body
    }

    /// The public key `test_signing_key` signs for, as the constructors
    /// take it.
    fn test_public_key() -> String {
        crate::hex::encode(test_signing_key().verifying_key().as_bytes())
    }

    /// Serves `router` on a loopback port and POSTs `body` to
    /// `/interactions` with a valid signature. The caller aborts the
    /// returned server task once it has read what it needs.
    async fn send_signed(
        router: axum::Router,
        body: Vec<u8>,
    ) -> (
        reqwest::Response,
        tokio::task::JoinHandle<std::io::Result<()>>,
    ) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/interactions", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, router).await });
        let headers = signed_headers_for_body(&body);
        let mut request = reqwest::Client::new().post(url).body(body);
        for (name, value) in &headers {
            request = request.header(name.as_str(), value.to_str().unwrap());
        }
        (request.send().await.unwrap(), server)
    }

    /// [`send_signed`], returning the status only.
    ///
    /// A rejected body is sent with exactly one byte over the limit: the
    /// server then has read all of it when it answers 413, so it closes the
    /// connection cleanly and the client always reads the answer.
    async fn post_signed(router: axum::Router, body: Vec<u8>) -> StatusCode {
        let (response, server) = send_signed(router, body).await;
        server.abort();
        StatusCode::from_u16(response.status().as_u16()).unwrap()
    }

    /// The body of the endpoint's `200` answer to a signed command.
    async fn inline_answer(router: axum::Router) -> Vec<u8> {
        let (response, server) = send_signed(router, command_body("token")).await;
        assert_eq!(response.status().as_u16(), 200);
        let body = response.bytes().await.unwrap().to_vec();
        server.abort();
        body
    }

    #[tokio::test]
    async fn default_body_limit_admits_large_real_interactions() {
        let body = large_channel_select_body();
        assert!(
            (100_000..120_000).contains(&body.len()),
            "{} bytes",
            body.len()
        );
        let handler = RecordingTypedHandler {
            response: json!({ "type": 6 }),
            ..Default::default()
        };
        let router = try_typed_interactions_endpoint(&test_public_key(), handler.clone()).unwrap();
        assert_eq!(post_signed(router, body).await, StatusCode::OK);
        let resolved_channels = match &handler.records.lock().unwrap()[0].1 {
            Interaction::Component(component) => component
                .data
                .resolved
                .as_ref()
                .map(|resolved| resolved.channels.len()),
            _ => panic!("expected a component interaction"),
        };
        assert_eq!(resolved_channels, Some(25));

        let router = try_interactions_endpoint(&test_public_key(), TestHandler).unwrap();
        assert_eq!(
            post_signed(router, large_channel_select_body()).await,
            StatusCode::ACCEPTED
        );
    }

    #[tokio::test]
    async fn body_limit_rejects_one_byte_over_and_is_configurable() {
        let limit = DEFAULT_INTERACTION_BODY_LIMIT;
        let router = try_typed_interactions_endpoint(&test_public_key(), TypedTestHandler).unwrap();
        assert_eq!(
            post_signed(router.clone(), padded(command_body("token"), limit)).await,
            StatusCode::ACCEPTED
        );
        assert_eq!(
            post_signed(router, padded(command_body("token"), limit + 1)).await,
            StatusCode::PAYLOAD_TOO_LARGE
        );

        let raw = InteractionsEndpointBuilder::new(test_public_key())
            .body_limit(2048)
            .build(TestHandler)
            .unwrap();
        assert_eq!(
            post_signed(raw.clone(), padded(command_body("token"), 2048)).await,
            StatusCode::ACCEPTED
        );
        assert_eq!(
            post_signed(raw, padded(command_body("token"), 2049)).await,
            StatusCode::PAYLOAD_TOO_LARGE
        );

        assert!(InteractionsEndpointBuilder::new("bad-key")
            .build_typed(TypedTestHandler)
            .is_err());
        assert!(InteractionsEndpointBuilder::new("bad-key")
            .build(TestHandler)
            .is_err());
    }

    /// The policy the mention tests configure.
    fn quiet_mentions() -> AllowedMentions {
        AllowedMentions {
            replied_user: Some(false),
            ..AllowedMentions::default()
        }
    }

    /// A per-response policy that differs from [`quiet_mentions`].
    fn role_mentions() -> AllowedMentions {
        AllowedMentions {
            parse: vec!["roles".to_owned()],
            ..AllowedMentions::default()
        }
    }

    /// Answers every interaction inline with a clone of its response.
    #[derive(Clone)]
    struct FixedTypedHandler(InteractionResponse);

    impl TypedInteractionHandler for FixedTypedHandler {
        async fn handle_typed(
            &self,
            _ctx: InteractionContextData,
            _interaction: Interaction,
        ) -> EndpointReply {
            self.0.clone().into()
        }
    }

    /// [`FixedTypedHandler`] for the raw endpoint.
    #[derive(Clone)]
    struct FixedRawHandler(InteractionResponse);

    impl InteractionHandler for FixedRawHandler {
        async fn handle(
            &self,
            _ctx: InteractionContext,
            _interaction: RawInteraction,
        ) -> EndpointReply {
            self.0.clone().into()
        }
    }

    /// The endpoint's answer to `response`, through the typed or the raw
    /// route, with `policy` as the builder's default.
    async fn answer_with_policy(
        typed: bool,
        response: InteractionResponse,
        policy: Option<AllowedMentions>,
    ) -> Vec<u8> {
        let mut builder = InteractionsEndpointBuilder::new(test_public_key());
        if let Some(policy) = policy {
            builder = builder.default_allowed_mentions(policy);
        }
        let router = if typed {
            builder.build_typed(FixedTypedHandler(response)).unwrap()
        } else {
            builder.build(FixedRawHandler(response)).unwrap()
        };
        inline_answer(router).await
    }

    fn parse(body: &[u8]) -> Value {
        serde_json::from_slice(body).unwrap()
    }

    /// Regression: the endpoint encoded inline responses with no default
    /// `allowed_mentions`, unlike the REST callback, and could not be
    /// configured with one.
    #[tokio::test]
    async fn inline_message_responses_get_the_endpoint_default_mentions() {
        for typed in [true, false] {
            let reply = answer_with_policy(
                typed,
                InteractionResponse::reply("hi <@1>"),
                Some(quiet_mentions()),
            )
            .await;
            assert_eq!(
                parse(&reply),
                json!({
                    "type": 4,
                    "data": {
                        "content": "hi <@1>",
                        "allowed_mentions": { "replied_user": false }
                    }
                }),
                "typed: {typed}"
            );

            let update = answer_with_policy(
                typed,
                InteractionResponse::update("edited <@1>"),
                Some(quiet_mentions()),
            )
            .await;
            assert_eq!(
                parse(&update),
                json!({
                    "type": 7,
                    "data": {
                        "content": "edited <@1>",
                        "allowed_mentions": { "replied_user": false }
                    }
                }),
                "typed: {typed}"
            );
        }

        // Only message callbacks (4 and 7) carry `allowed_mentions`.
        let defer = answer_with_policy(
            true,
            InteractionResponse::defer(false),
            Some(quiet_mentions()),
        )
        .await;
        assert_eq!(parse(&defer), json!({ "type": 5, "data": { "flags": 0 } }));
        let modal = answer_with_policy(
            true,
            InteractionResponse::modal(ModalBuilder::new("m", "M")),
            Some(quiet_mentions()),
        )
        .await;
        assert!(parse(&modal)["data"].get("allowed_mentions").is_none());

        // Without a default the answer stays as 3.0.0 sent it.
        let unset = answer_with_policy(true, InteractionResponse::reply("hi"), None).await;
        assert_eq!(unset, br#"{"type":4,"data":{"content":"hi"}}"#);
    }

    #[tokio::test]
    async fn explicit_response_mentions_win_over_the_endpoint_default() {
        for typed in [true, false] {
            let reply = answer_with_policy(
                typed,
                InteractionResponse::reply(
                    InteractionMessage::new()
                        .content("explicit")
                        .allowed_mentions(role_mentions()),
                ),
                Some(quiet_mentions()),
            )
            .await;
            assert_eq!(
                parse(&reply)["data"]["allowed_mentions"],
                json!({ "parse": ["roles"] }),
                "typed: {typed}"
            );

            let update = answer_with_policy(
                typed,
                InteractionResponse::update(
                    UpdateMessage::new()
                        .content("explicit")
                        .allowed_mentions(role_mentions()),
                ),
                Some(quiet_mentions()),
            )
            .await;
            assert_eq!(
                parse(&update)["data"]["allowed_mentions"],
                json!({ "parse": ["roles"] }),
                "typed: {typed}"
            );
        }
    }

    #[tokio::test]
    async fn raw_responses_are_sent_as_written_despite_the_endpoint_default() {
        for typed in [true, false] {
            let body = answer_with_policy(
                typed,
                raw(&json!({ "type": 4, "data": { "content": "raw <@1>" } })),
                Some(quiet_mentions()),
            )
            .await;
            assert_eq!(
                body, br#"{"type":4,"data":{"content":"raw <@1>"}}"#,
                "typed: {typed}"
            );
        }
    }

    /// One response and one policy encode to the same bytes whether the
    /// endpoint answers inline or a `RestClient` sends the callback.
    #[tokio::test]
    async fn rest_and_inline_send_identical_bytes_for_one_policy() {
        use crate::response::tests::mock::{self, Reply};

        let responses = [
            InteractionResponse::reply("hi <@1>"),
            InteractionResponse::reply(
                InteractionMessage::new()
                    .content("explicit")
                    .allowed_mentions(role_mentions()),
            ),
            InteractionResponse::update("edited"),
            InteractionResponse::defer(true),
            raw(&json!({ "type": 4, "data": { "content": "raw" } })),
        ];
        for policy in [Some(quiet_mentions()), None] {
            let mut server =
                mock::spawn(responses.iter().map(|_| Reply::Now(204, "")).collect()).await;
            let mut http = RestClient::builder("bot-token", Some(test_id(200)))
                .api_base(server.base_url.clone());
            if let Some(policy) = policy.clone() {
                http = http.default_allowed_mentions(policy);
            }
            let http = http.build().unwrap();
            for response in &responses {
                http.create_interaction_response(test_id(100), "token", response)
                    .await
                    .unwrap();
                let (_, rest_body) = server.next_request_with_body().await;
                let inline_body = answer_with_policy(true, response.clone(), policy.clone()).await;
                assert_eq!(
                    String::from_utf8(inline_body).unwrap(),
                    String::from_utf8(rest_body).unwrap(),
                    "{response:?} with {policy:?}"
                );
            }
        }
    }
}
