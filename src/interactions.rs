use async_trait::async_trait;
use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use ed25519_dalek::{Signature, VerifyingKey};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::warn;

use crate::builders::ModalBuilder;
use crate::error::DiscordError;
use crate::model::{
    ApplicationCommandOptionChoice, Interaction, InteractionContextData, InteractionResponseState,
    Snowflake,
};
use crate::parsers::{
    parse_interaction, parse_interaction_context, parse_raw_interaction, value_to_u8,
    InteractionContext, RawInteraction,
};
use crate::response::AckKind;
use crate::types::invalid_data_error;

/// Typed Discord API enum for `InteractionResponse`.
pub enum InteractionResponse {
    /// Discord API enum variant `Pong`.
    Pong,
    /// Discord API enum variant `ChannelMessage`.
    ChannelMessage(Value),
    /// Discord API enum variant `DeferredMessage`.
    DeferredMessage,
    /// Discord API enum variant `DeferredUpdate`.
    DeferredUpdate,
    /// Discord API enum variant `AutocompleteResult`.
    AutocompleteResult(Vec<ApplicationCommandOptionChoice>),
    /// Discord API enum variant `Modal`.
    Modal(ModalBuilder),
    /// Discord API enum variant `UpdateMessage`.
    UpdateMessage(Value),
    /// Discord API enum variant `LaunchActivity`.
    LaunchActivity,
    /// Discord API enum variant `Raw`.
    Raw(Value),
}

impl InteractionResponse {
    /// Acknowledgement Discord records when it accepts this response, or
    /// `None` for `Pong` and raw payloads without a numeric `type`.
    fn ack_kind(&self) -> Option<AckKind> {
        match self {
            Self::Pong => None,
            Self::DeferredMessage | Self::DeferredUpdate => Some(AckKind::Deferred),
            Self::ChannelMessage(_)
            | Self::AutocompleteResult(_)
            | Self::Modal(_)
            | Self::UpdateMessage(_)
            | Self::LaunchActivity => Some(AckKind::Replied),
            Self::Raw(payload) => payload
                .get("type")
                .and_then(value_to_u8)
                .and_then(AckKind::for_callback_type),
        }
    }
}

/// Trait for `InteractionHandler` behavior.
#[async_trait]
pub trait InteractionHandler: Send + Sync {
    /// Handles one verified raw Discord interaction.
    async fn handle(
        &self,
        ctx: InteractionContext,
        interaction: RawInteraction,
    ) -> InteractionResponse;
}

/// Trait for `TypedInteractionHandler` behavior.
///
/// The `ctx` and `interaction` passed to the handler share one response
/// state with each other and with every clone made from them. When the
/// handler returns an initial response, the endpoint records it in that
/// state before answering Discord, so a clone kept by a spawned task sees the
/// interaction as acknowledged and can call `follow_up` or `edit_reply` over
/// REST afterwards.
#[async_trait]
pub trait TypedInteractionHandler: Send + Sync {
    /// Handles one verified typed Discord interaction.
    async fn handle_typed(
        &self,
        ctx: InteractionContextData,
        interaction: Interaction,
    ) -> InteractionResponse;
}

#[derive(Clone)]
struct InteractionsState<H> {
    verifying_key: VerifyingKey,
    handler: H,
}

const SIGNATURE_TIMESTAMP_TOLERANCE_SECS: i64 = 60 * 5;
const INTERACTION_BODY_LIMIT_BYTES: usize = 64 * 1024;

fn decode_verifying_key(public_key: &str) -> Result<VerifyingKey, DiscordError> {
    let public_key_bytes = hex::decode(public_key)?;
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
    let signature_bytes = hex::decode(signature_hex)?;
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

fn interaction_response_payload(response: InteractionResponse) -> Value {
    match response {
        InteractionResponse::Pong => serde_json::json!({ "type": 1 }),
        InteractionResponse::ChannelMessage(data) => serde_json::json!({
            "type": 4,
            "data": data,
        }),
        InteractionResponse::DeferredMessage => serde_json::json!({ "type": 5 }),
        InteractionResponse::DeferredUpdate => serde_json::json!({ "type": 6 }),
        InteractionResponse::AutocompleteResult(choices) => serde_json::json!({
            "type": 8,
            "data": { "choices": choices },
        }),
        InteractionResponse::Modal(modal) => serde_json::json!({
            "type": 9,
            "data": modal.build(),
        }),
        InteractionResponse::UpdateMessage(data) => serde_json::json!({
            "type": 7,
            "data": data,
        }),
        InteractionResponse::LaunchActivity => serde_json::json!({ "type": 12 }),
        InteractionResponse::Raw(data) => data,
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

    let payload: Value = match serde_json::from_slice(body.as_ref()) {
        Ok(payload) => payload,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": error.to_string() })),
            )
                .into_response();
        }
    };

    let interaction_type = match payload.get("type").and_then(value_to_u8) {
        Some(interaction_type) => interaction_type,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "missing or invalid interaction.type" })),
            )
                .into_response();
        }
    };

    if interaction_type == 1 {
        return (StatusCode::OK, Json(serde_json::json!({ "type": 1 }))).into_response();
    }

    let context = match parse_interaction_context(&payload) {
        Ok(context) => context,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": error.to_string() })),
            )
                .into_response();
        }
    };

    let interaction = match parse_raw_interaction(&payload) {
        Ok(interaction) => interaction,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": error.to_string() })),
            )
                .into_response();
        }
    };

    let response = state.handler.handle(context, interaction).await;
    let response_payload = interaction_response_payload(response);
    (StatusCode::OK, Json(response_payload)).into_response()
}

/// Provides the `try_interactions_endpoint` helper.
pub fn try_interactions_endpoint<H>(public_key: &str, handler: H) -> Result<Router, DiscordError>
where
    H: InteractionHandler + Clone + Send + Sync + 'static,
{
    let verifying_key = decode_verifying_key(public_key)?;

    Ok(Router::new()
        .route("/interactions", post(handle_interactions_request::<H>))
        .layer(DefaultBodyLimit::max(INTERACTION_BODY_LIMIT_BYTES))
        .with_state(InteractionsState {
            verifying_key,
            handler,
        }))
}

/// Provides the `interactions_endpoint` helper.
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

    let payload: Value = match serde_json::from_slice(body.as_ref()) {
        Ok(payload) => payload,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": error.to_string() })),
            )
                .into_response();
        }
    };

    let interaction_type = match payload.get("type").and_then(value_to_u8) {
        Some(interaction_type) => interaction_type,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "missing or invalid interaction.type" })),
            )
                .into_response();
        }
    };

    if interaction_type == 1 {
        return (StatusCode::OK, Json(serde_json::json!({ "type": 1 }))).into_response();
    }

    let interaction = match parse_interaction(&payload) {
        Ok(interaction) => interaction,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": error.to_string() })),
            )
                .into_response();
        }
    };
    let context: InteractionContextData = interaction.context().clone();
    let interaction_id = context.id.clone();
    let response_state = context.response_state.clone();

    let response = state.handler.handle_typed(context, interaction).await;
    record_inline_response(&response_state, &interaction_id, &response);
    let response_payload = interaction_response_payload(response);
    (StatusCode::OK, Json(response_payload)).into_response()
}

/// Records the handler's inline response in the shared response state, the
/// same state the REST response methods use (see the decision log in
/// `response.rs`).
///
/// The inline response is the body of Discord's own request, so there is no
/// delivery outcome to wait for: the state moves straight to acknowledged.
/// A follow-up sent by a retained clone in the moment between this call and
/// Discord reading the body can still be rejected by Discord.
///
/// When the handler already responded over REST (or such a response is in
/// flight), the payload is still returned unchanged — Discord's request must
/// be answered — but the state keeps describing the REST response and a
/// warning is logged, because Discord accepts only one initial response.
fn record_inline_response(
    state: &InteractionResponseState,
    interaction_id: &Snowflake,
    response: &InteractionResponse,
) {
    let Some(kind) = response.ack_kind() else {
        return;
    };
    if !state.commit_inline(kind) {
        warn!(
            interaction_id = %interaction_id,
            phase = ?state.phase(),
            "interaction handler returned an inline initial response after responding over REST; \
             Discord accepts only one initial response"
        );
    }
}

/// Provides the `try_typed_interactions_endpoint` helper.
pub fn try_typed_interactions_endpoint<H>(
    public_key: &str,
    handler: H,
) -> Result<Router, DiscordError>
where
    H: TypedInteractionHandler + Clone + Send + Sync + 'static,
{
    let verifying_key = decode_verifying_key(public_key)?;

    Ok(Router::new()
        .route(
            "/interactions",
            post(handle_typed_interactions_request::<H>),
        )
        .layer(DefaultBodyLimit::max(INTERACTION_BODY_LIMIT_BYTES))
        .with_state(InteractionsState {
            verifying_key,
            handler,
        }))
}

/// Provides the `typed_interactions_endpoint` helper.
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
        handle_typed_interactions_request, interaction_response_payload, record_inline_response,
        try_interactions_endpoint, try_typed_interactions_endpoint,
        verify_discord_request_signature_at_time, verify_discord_signature,
        verify_signature_with_key_at_time, InteractionHandler, InteractionResponse,
        InteractionsState, TypedInteractionHandler, SIGNATURE_TIMESTAMP_TOLERANCE_SECS,
    };
    use crate::builders::ModalBuilder;
    use crate::framework::{AppContext, AppFramework};
    use crate::http::RestClient;
    use crate::model::ApplicationCommandOptionChoice;
    use crate::model::{Interaction, InteractionContextData, InteractionResponseState, Snowflake};
    use crate::parsers::{InteractionContext, RawInteraction};
    use crate::response::{
        AckKind, CallbackOutcome, InteractionResponder, InteractionResponsePhase,
    };
    use async_trait::async_trait;
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

    #[derive(Clone)]
    struct TestHandler;

    #[async_trait]
    impl InteractionHandler for TestHandler {
        async fn handle(
            &self,
            _ctx: InteractionContext,
            _interaction: RawInteraction,
        ) -> InteractionResponse {
            InteractionResponse::Raw(Value::Null)
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

    #[async_trait]
    impl TypedInteractionHandler for TypedTestHandler {
        async fn handle_typed(
            &self,
            _ctx: InteractionContextData,
            _interaction: Interaction,
        ) -> InteractionResponse {
            InteractionResponse::Raw(Value::Null)
        }
    }

    #[derive(Clone, Default)]
    struct RecordingRawHandler {
        calls: Arc<AtomicUsize>,
        records: Arc<Mutex<Vec<(InteractionContext, RawInteraction)>>>,
        response: Value,
    }

    #[async_trait]
    impl InteractionHandler for RecordingRawHandler {
        async fn handle(
            &self,
            ctx: InteractionContext,
            interaction: RawInteraction,
        ) -> InteractionResponse {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.records.lock().unwrap().push((ctx, interaction));
            InteractionResponse::Raw(self.response.clone())
        }
    }

    #[derive(Clone, Default)]
    struct RecordingTypedHandler {
        calls: Arc<AtomicUsize>,
        records: Arc<Mutex<Vec<(InteractionContextData, Interaction)>>>,
        response: Value,
    }

    #[async_trait]
    impl TypedInteractionHandler for RecordingTypedHandler {
        async fn handle_typed(
            &self,
            ctx: InteractionContextData,
            interaction: Interaction,
        ) -> InteractionResponse {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.records.lock().unwrap().push((ctx, interaction));
            InteractionResponse::Raw(self.response.clone())
        }
    }

    #[test]
    fn try_typed_interactions_endpoint_accepts_valid_public_key() {
        let public_key = "0000000000000000000000000000000000000000000000000000000000000000";
        assert!(try_typed_interactions_endpoint(public_key, TypedTestHandler).is_ok());
    }

    #[test]
    fn interaction_response_payload_supports_deferred_update() {
        let payload = interaction_response_payload(InteractionResponse::DeferredUpdate);
        assert_eq!(payload["type"], 6);
    }

    #[test]
    fn interaction_response_payload_supports_autocomplete_choices() {
        let payload = interaction_response_payload(InteractionResponse::AutocompleteResult(vec![
            ApplicationCommandOptionChoice {
                name: "Support".to_string(),
                name_localizations: None,
                value: Value::String("support".to_string()),
            },
        ]));

        assert_eq!(payload["type"], 8);
        assert_eq!(payload["data"]["choices"][0]["name"], "Support");
        assert_eq!(payload["data"]["choices"][0]["value"], "support");
    }

    #[test]
    fn interaction_response_payload_supports_launch_activity() {
        let payload = interaction_response_payload(InteractionResponse::LaunchActivity);
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

        (verifying_key, hex::encode(signature.to_bytes()))
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
            &hex::encode(verifying_key.as_bytes()),
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
            interaction_response_payload(InteractionResponse::Pong),
            json!({ "type": 1 })
        );
        assert_eq!(
            interaction_response_payload(InteractionResponse::ChannelMessage(
                json!({ "content": "hello" })
            )),
            json!({
                "type": 4,
                "data": { "content": "hello" }
            })
        );
        assert_eq!(
            interaction_response_payload(InteractionResponse::UpdateMessage(
                json!({ "content": "updated" })
            )),
            json!({
                "type": 7,
                "data": { "content": "updated" }
            })
        );
        assert_eq!(
            interaction_response_payload(InteractionResponse::Modal(ModalBuilder::new(
                "feedback", "Feedback"
            )))["type"],
            json!(9)
        );
        assert_eq!(
            interaction_response_payload(InteractionResponse::Raw(json!({ "type": 99 }))),
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
        assert_eq!(ctx.id, "100");
        assert_eq!(ctx.application_id, "200");
        assert_eq!(ctx.user_id.as_deref(), Some("500"));
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
            .contains("missing or invalid component_data.custom_id"));
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

    #[test]
    fn inline_responses_record_the_matching_phase() {
        let cases = [
            (
                InteractionResponse::Pong,
                InteractionResponsePhase::Unacknowledged,
            ),
            (
                InteractionResponse::ChannelMessage(json!({ "content": "hi" })),
                InteractionResponsePhase::Replied,
            ),
            (
                InteractionResponse::DeferredMessage,
                InteractionResponsePhase::Deferred,
            ),
            (
                InteractionResponse::DeferredUpdate,
                InteractionResponsePhase::Deferred,
            ),
            (
                InteractionResponse::AutocompleteResult(Vec::new()),
                InteractionResponsePhase::Replied,
            ),
            (
                InteractionResponse::Modal(ModalBuilder::new("m", "M")),
                InteractionResponsePhase::Replied,
            ),
            (
                InteractionResponse::UpdateMessage(json!({ "content": "new" })),
                InteractionResponsePhase::Replied,
            ),
            (
                InteractionResponse::LaunchActivity,
                InteractionResponsePhase::Replied,
            ),
            (
                InteractionResponse::Raw(json!({ "type": 6 })),
                InteractionResponsePhase::Deferred,
            ),
            (
                InteractionResponse::Raw(json!({ "type": 4, "data": {} })),
                InteractionResponsePhase::Replied,
            ),
            (
                InteractionResponse::Raw(json!({ "type": 1 })),
                InteractionResponsePhase::Unacknowledged,
            ),
            (
                InteractionResponse::Raw(Value::Null),
                InteractionResponsePhase::Unacknowledged,
            ),
        ];
        for (response, expected) in cases {
            let state = InteractionResponseState::new();
            record_inline_response(&state, &Snowflake::from("1"), &response);
            assert_eq!(state.phase(), expected);
        }
    }

    #[test]
    fn inline_response_after_a_rest_response_keeps_the_rest_state() {
        let state = InteractionResponseState::new();
        state
            .begin(AckKind::Deferred)
            .unwrap()
            .settle(CallbackOutcome::Accepted);
        record_inline_response(
            &state,
            &Snowflake::from("1"),
            &InteractionResponse::ChannelMessage(json!({ "content": "late" })),
        );
        assert!(state.is_deferred());

        let in_flight = InteractionResponseState::new();
        let _claim = in_flight.begin(AckKind::Replied).unwrap();
        record_inline_response(
            &in_flight,
            &Snowflake::from("1"),
            &InteractionResponse::DeferredMessage,
        );
        assert_eq!(in_flight.phase(), InteractionResponsePhase::InFlight);
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
                    async { InteractionResponse::DeferredMessage }
                }
            })
            .build();
        let body = command_body("bad/token");

        let response = handle_typed_interactions_request(
            State(InteractionsState {
                verifying_key: test_signing_key().verifying_key(),
                handler: framework,
            }),
            signed_headers_for_body(&body),
            Bytes::from(body),
        )
        .await;
        assert_eq!(json_response(response).await, json!({ "type": 5 }));

        let ctx = retained.lock().unwrap().take().expect("route ran");
        assert!(ctx.is_deferred());
        assert!(ctx.interaction.is_deferred());
        let error = ctx
            .follow_up(&RestClient::new("bot-token", 200), "done")
            .await
            .unwrap_err();
        assert!(
            error.to_string().contains("interaction_token"),
            "follow-up should pass the acknowledgement check: {error}"
        );
    }

    #[derive(Clone)]
    struct RestThenInlineHandler;

    #[async_trait]
    impl TypedInteractionHandler for RestThenInlineHandler {
        async fn handle_typed(
            &self,
            ctx: InteractionContextData,
            _interaction: Interaction,
        ) -> InteractionResponse {
            // Stands in for a successful `ctx.defer(&http)` over REST.
            ctx.response_state
                .begin(AckKind::Deferred)
                .unwrap()
                .settle(CallbackOutcome::Accepted);
            InteractionResponse::ChannelMessage(json!({ "content": "inline" }))
        }
    }

    #[tokio::test]
    async fn typed_endpoint_still_answers_when_the_handler_responded_over_rest() {
        let body = command_body("token");
        let response = handle_typed_interactions_request(
            State(InteractionsState {
                verifying_key: test_signing_key().verifying_key(),
                handler: RestThenInlineHandler,
            }),
            signed_headers_for_body(&body),
            Bytes::from(body),
        )
        .await;
        assert_eq!(
            json_response(response).await,
            json!({ "type": 4, "data": { "content": "inline" } })
        );
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
}
