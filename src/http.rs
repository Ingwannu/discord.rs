use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use reqwest::{header::HeaderMap, Client, Method, StatusCode};
use serde::de::DeserializeOwned;
use serde_json::Value;
use tracing::{debug, warn};

// This file keeps only the transport core: client construction, the request
// loop, rate-limit and replay plumbing, and helpers every endpoint shares.
// Endpoint methods live in per-domain `impl RestClient` blocks under `http/`
// that mirror the `model/` domain files, so one domain owner edits one model
// file and one HTTP file (3.0 series 0, see
// docs/design/3.0-types-events-requests.md section 5). Shared helpers stay
// private here; child modules reach them through `super::` without any
// visibility widening.
mod application;
mod application_identity;
mod attachment;
mod audit_log;
mod auto_moderation;
mod body;
mod channel;
mod client_config;
mod command;
mod emoji;
mod gateway;
mod guild;
mod guild_aux;
mod interaction;
mod invite;
mod lobby;
mod media_resources;
mod member;
mod messages;
mod moderation;
mod monetization;
mod oauth;
mod paths;
mod poll;
mod rate_limit;
mod role;
mod routes;
mod scheduled_events;
#[cfg(test)]
mod tests;
mod threads;
mod voice;
mod webhook;

use crate::error::DiscordError;
use crate::model::{
    AllowedMentions, ApplicationId, CreateMessage, InteractionCallbackResponse,
    InteractionResponseType,
};
use crate::types::invalid_data_error;
pub use attachment::{FileAttachment, FileUpload};
use body::{
    build_multipart_form, build_named_file_form, build_sticker_form, clone_json_body,
    decode_json_body, json_body, multipart_body, optional_json_body, parse_body_bytes,
    parse_body_value, serialize_body, EncodedBody, RequestBody, NO_BODY,
};
pub use client_config::{RateLimitCallback, RateLimitInfo, RestClientBuilder};
use paths::{
    archived_threads_query, bool_query, configured_application_id, entitlement_query,
    joined_archived_threads_query, request_uses_bot_authorization, route_token, scrub_token,
    subscription_query, thread_member_query, RequestRoute,
};
use rate_limit::RateLimitState;

pub(crate) use paths::redact_path_tokens;
#[cfg(test)]
use rate_limit::RATE_LIMIT_BUCKET_RETENTION;
pub(crate) use routes::{RouteAuth, RouteId};

const API_BASE: &str = "https://discord.com/api/v10";
const DEFAULT_USER_AGENT: &str = concat!("DiscordBot (discordrs, ", env!("CARGO_PKG_VERSION"), ")");
const MAX_RATE_LIMIT_RETRIES: usize = 5;
const MAX_SERVER_ERROR_RETRIES: usize = 3;
const ROUTE_GATE_GC_THRESHOLD: usize = 512;

/// The `Authorization` header a request is sent with.
#[derive(Clone, Copy)]
enum RequestAuthorization<'a> {
    /// The bot token, unless the path is a tokenized webhook or interaction
    /// route ([`request_uses_bot_authorization`]). Only raw requests, whose
    /// route the client does not know, decide from the path.
    Auto,
    Bot,
    Bearer(&'a str),
    None,
}

impl RequestAuthorization<'_> {
    /// The header for a manifest route sent without a caller-supplied token.
    /// A bearer route reached this way sends no header rather than the bot
    /// token; the endpoint must use the bearer shims.
    fn for_route(route: RouteId) -> Self {
        match route.auth() {
            RouteAuth::Bot => Self::Bot,
            RouteAuth::Bearer | RouteAuth::Token | RouteAuth::None | RouteAuth::Client => {
                Self::None
            }
        }
    }
}

/// What [`RestClient::execute`] sends to: a manifest route, or a raw request
/// whose method and authorization the caller chose.
#[derive(Clone, Copy)]
enum Target<'a> {
    Route(RouteId),
    BearerRoute(RouteId, &'a str),
    Raw(&'a Method, RequestAuthorization<'a>),
}

impl<'a> Target<'a> {
    fn route(self) -> Option<RouteId> {
        match self {
            Self::Route(route) | Self::BearerRoute(route, _) => Some(route),
            Self::Raw(..) => None,
        }
    }

    fn method(self) -> Method {
        match self {
            Self::Route(route) | Self::BearerRoute(route, _) => route.method(),
            Self::Raw(method, _) => method.clone(),
        }
    }

    fn authorization(self) -> RequestAuthorization<'a> {
        match self {
            Self::Route(route) => RequestAuthorization::for_route(route),
            Self::BearerRoute(_, token) => RequestAuthorization::Bearer(token),
            Self::Raw(_, authorization) => authorization,
        }
    }
}

/// Typed Discord REST client with shared per-route and global rate-limit
/// state. Cloning is cheap: clones share the same rate-limit bookkeeping,
/// connection pool, and application id.
#[derive(Clone)]
pub struct RestClient {
    client: Client,
    token: String,
    /// The application ID, or `0` while it is not known yet (an `Option`
    /// cannot be stored atomically; `0` is never a valid snowflake). Read
    /// and written through [`Self::application_id`] and
    /// [`Self::set_application_id`] only.
    application_id: Arc<AtomicU64>,
    rate_limits: Arc<RateLimitState>,
    route_gates: Arc<RateLimitRouteGates>,
    audit_log_reason: Option<String>,
    base_url: String,
    user_agent: String,
    default_allowed_mentions: Option<AllowedMentions>,
    rate_limit_callback: Option<RateLimitCallback>,
    /// Opt-in to replaying POST/PATCH after an ambiguous failure; see
    /// [`RestClientBuilder::retry_non_idempotent_requests`].
    retry_non_idempotent: bool,
    /// True when `client` was built by this crate with redirects disabled,
    /// which is what makes a connect failure proof that nothing was sent.
    managed_transport: bool,
}

/// Type alias for `DiscordHttpClient`.
pub type DiscordHttpClient = RestClient;

#[derive(Default)]
struct RateLimitRouteGates {
    // One gate per route or bucket, shared by every client clone. The std
    // lock only guards the map; the tokio gate is held for a whole request.
    routes: StdMutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl RateLimitRouteGates {
    fn gate_for(&self, route_key: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut routes = self
            .routes
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if routes.len() >= ROUTE_GATE_GC_THRESHOLD {
            // Gates whose only strong reference is the map itself have no
            // in-flight request; dropping them bounds the map without ever
            // discarding a gate that is currently serializing requests.
            routes.retain(|key, gate| key == route_key || Arc::strong_count(gate) > 1);
        }
        Arc::clone(
            routes
                .entry(route_key.to_string())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(()))),
        )
    }
}

/// The stored form of an optional application ID: the raw snowflake, or `0`
/// for "not set" (see `RestClient::application_id`).
pub(crate) fn application_id_raw(application_id: Option<ApplicationId>) -> u64 {
    application_id.map_or(0, ApplicationId::get)
}

impl RestClient {
    /// Creates a Discord REST client with bounded connect and request timeouts.
    ///
    /// `application_id` is needed by application-scoped routes (commands,
    /// interaction follow-ups, application emojis, entitlements, ...). Pass
    /// `None` when it is not known yet; those routes then fail with an error
    /// until [`Self::set_application_id`] is called (the gateway client does
    /// this from `READY`).
    pub fn new(token: impl Into<String>, application_id: Option<ApplicationId>) -> Self {
        Self {
            client: client_config::default_http_client(),
            token: token.into(),
            application_id: Arc::new(AtomicU64::new(application_id_raw(application_id))),
            rate_limits: Arc::new(RateLimitState::default()),
            route_gates: Arc::new(RateLimitRouteGates::default()),
            audit_log_reason: None,
            base_url: API_BASE.to_string(),
            user_agent: DEFAULT_USER_AGENT.to_string(),
            default_allowed_mentions: None,
            rate_limit_callback: None,
            retry_non_idempotent: false,
            managed_transport: true,
        }
    }

    /// Returns a [`RestClientBuilder`] for a client with a custom API base,
    /// timeouts, user agent, proxy, rate-limit callback, or default
    /// allowed-mentions — the equivalent of discord.js's `RESTOptions`.
    pub fn builder(
        token: impl Into<String>,
        application_id: Option<ApplicationId>,
    ) -> RestClientBuilder {
        RestClientBuilder::new(token, application_id)
    }

    #[cfg(test)]
    fn new_with_base_url(
        token: impl Into<String>,
        application_id: u64,
        base_url: impl Into<String>,
    ) -> Self {
        let mut client = Self::new(token, ApplicationId::try_new(application_id).ok());
        client.base_url = base_url.into();
        client
    }

    /// Returns a client that sends `X-Audit-Log-Reason` with every mutating
    /// request it makes, so the reason appears in the guild audit log.
    ///
    /// The returned client shares rate-limit state and the connection pool
    /// with `self`; creating one is cheap and intended per call site:
    ///
    /// ```ignore
    /// client.with_reason("spam").remove_guild_member(guild_id, user_id).await?;
    /// ```
    pub fn with_reason(&self, reason: impl Into<String>) -> Self {
        let mut scoped = self.clone();
        scoped.audit_log_reason = Some(reason.into());
        scoped
    }

    /// Returns a client that injects the given allowed-mentions into message
    /// payloads that do not set `allowed_mentions` themselves. Like
    /// [`Self::with_reason`], the returned client shares rate-limit state and
    /// the connection pool with `self`.
    pub fn with_default_allowed_mentions(&self, allowed_mentions: AllowedMentions) -> Self {
        let mut scoped = self.clone();
        scoped.default_allowed_mentions = Some(allowed_mentions);
        scoped
    }

    /// Allowed-mentions injected into outgoing message payloads that do not
    /// set `allowed_mentions` themselves, if configured.
    pub fn default_allowed_mentions(&self) -> Option<&AllowedMentions> {
        self.default_allowed_mentions.as_ref()
    }

    fn api_base(&self) -> &str {
        &self.base_url
    }

    /// The application ID used by application-scoped routes, or `None` if it
    /// has not been set.
    pub fn application_id(&self) -> Option<ApplicationId> {
        ApplicationId::try_new(self.application_id.load(Ordering::Relaxed)).ok()
    }

    /// Sets the application ID for this client and every clone of it.
    pub fn set_application_id(&self, application_id: ApplicationId) {
        self.application_id
            .store(application_id.get(), Ordering::Relaxed);
    }

    // [Decision Log] Shim plus erased transport core
    // - Purpose: keep the per-type cost of the typed REST helpers minimal,
    //   since every response type an endpoint uses instantiates one, and stop
    //   copying each response body into a `String` before parsing it.
    // - Before: each helper was a generic `async fn` that serialized with
    //   `?`, awaited three nested non-generic wrappers (`request_with_headers`
    //   -> `request_with_headers_authorized` -> the loop), then decoded with
    //   `from_utf8_lossy` -> `parse_body_value` -> `from_value::<T>`. The loop
    //   was already compiled once; the per-pair cost was the shim's own state
    //   machine with its early-return paths, about 275 LLVM IR lines (141
    //   pairs in the all-features lib test build).
    // - Alternatives: (1) keep the shape and only drop the wrapper layers;
    //   (2) return hand-written `Future` types from non-async shims, which
    //   would turn the public `async fn request_typed` family into
    //   `fn -> impl Future`, a signature change; (3) box the core future.
    // - Chosen: the generic shims do only generic work. The endpoint encodes
    //   the body into an [`EncodedBody`] (`json_body`, `multipart_body`)
    //   before the single await, and the shim hands the awaited `Result` to
    //   `decode_response`. No `?` runs inside a shim, so its state machine has
    //   one suspend point and no early returns. `execute` is the only
    //   implementation of gating, rate limits, replay, and token scrubbing,
    //   and it starts by returning a body-encoding error before anything is
    //   sent. Since 3.0 phase 5 the shims take a [`RouteId`] from the route
    //   manifest (`http/routes.rs`) instead of a `Method`: the method and the
    //   authorization come from the manifest row, so a call site cannot pair a
    //   route with the wrong method or token, and the shims are generic over
    //   the response type only (body encoding moved to the call site).
    // - Why: (2) changes public signatures and (3) adds an allocation per
    //   request; this shape keeps every signature and behaviour. Measured
    //   (2026-10-11): a shim is about 160 IR lines with its drop glue, and a
    //   crate calling 150 endpoints compiles 2.6% less IR and links a 2.6%
    //   smaller stripped release binary. Build times of this crate do not
    //   move: the lib instantiates almost no shims, and its time is mostly
    //   type checking and borrow checking (see "Build Time" in
    //   discordrsdocs/docs/guide/testing-and-coverage.md).
    // - Trade-offs: `execute` takes an already-failed body as an argument,
    //   which reads oddly but is what keeps the shims branch-free. New
    //   endpoints must add a manifest row and call these shims rather than
    //   adding another generic `async fn` that wraps the loop.

    pub async fn request(
        &self,
        method: Method,
        path: &str,
        body: Option<&Value>,
    ) -> Result<Value, DiscordError> {
        let body = Ok(body.map(clone_json_body).map(RequestBody::Json));
        self.execute(Target::Raw(&method, RequestAuthorization::Auto), path, body)
            .await
            .map(RawResponse::into_value)
    }

    pub async fn request_multipart<B>(
        &self,
        method: Method,
        path: &str,
        body: &B,
        files: &[FileAttachment],
    ) -> Result<Value, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.execute(
            Target::Raw(&method, RequestAuthorization::Auto),
            path,
            multipart_body(body, files),
        )
        .await
        .map(RawResponse::into_value)
    }

    pub async fn request_typed<T, B>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<T, DiscordError>
    where
        T: DeserializeOwned,
        B: serde::Serialize + ?Sized,
    {
        decode_response(
            self.execute(
                Target::Raw(&method, RequestAuthorization::Auto),
                path,
                optional_json_body(body),
            )
            .await,
        )
    }

    pub async fn request_typed_multipart<T, B>(
        &self,
        method: Method,
        path: &str,
        body: &B,
        files: &[FileAttachment],
    ) -> Result<T, DiscordError>
    where
        T: DeserializeOwned,
        B: serde::Serialize + ?Sized,
    {
        decode_response(
            self.execute(
                Target::Raw(&method, RequestAuthorization::Auto),
                path,
                multipart_body(body, files),
            )
            .await,
        )
    }

    /// Sends `route` and decodes the response into `T`.
    async fn send<T: DeserializeOwned>(
        &self,
        route: RouteId,
        path: &str,
        body: EncodedBody,
    ) -> Result<T, DiscordError> {
        decode_response(self.execute(Target::Route(route), path, body).await)
    }

    /// [`Self::send`] for routes that answer `204 No Content` in some cases:
    /// `None` for 204, otherwise the decoded body.
    async fn send_optional<T: DeserializeOwned>(
        &self,
        route: RouteId,
        path: &str,
        body: EncodedBody,
    ) -> Result<Option<T>, DiscordError> {
        decode_optional_response(self.execute(Target::Route(route), path, body).await)
    }

    /// Sends `route` and discards the response body.
    async fn send_unit(
        &self,
        route: RouteId,
        path: &str,
        body: EncodedBody,
    ) -> Result<(), DiscordError> {
        self.execute(Target::Route(route), path, body)
            .await
            .map(RawResponse::discard)
    }

    /// Sends `route` without a body and returns the raw response bytes.
    async fn send_bytes(&self, route: RouteId, path: &str) -> Result<Vec<u8>, DiscordError> {
        self.execute(Target::Route(route), path, NO_BODY)
            .await
            .map(|response| response.body)
    }

    /// For endpoints that build their own [`RequestBody`] (named-file and
    /// sticker multipart forms) and decode the response themselves.
    async fn send_response(
        &self,
        route: RouteId,
        path: &str,
        body: EncodedBody,
    ) -> Result<RawResponse, DiscordError> {
        self.execute(Target::Route(route), path, body).await
    }

    /// [`Self::send`] for a `bearer` route with the caller's access token.
    async fn send_bearer<T: DeserializeOwned>(
        &self,
        route: RouteId,
        bearer_token: &str,
        path: &str,
        body: EncodedBody,
    ) -> Result<T, DiscordError> {
        decode_response(
            self.execute(Target::BearerRoute(route, bearer_token), path, body)
                .await,
        )
    }

    /// [`Self::send_unit`] for a `bearer` route with the caller's access token.
    async fn send_bearer_unit(
        &self,
        route: RouteId,
        bearer_token: &str,
        path: &str,
        body: EncodedBody,
    ) -> Result<(), DiscordError> {
        self.execute(Target::BearerRoute(route, bearer_token), path, body)
            .await
            .map(RawResponse::discard)
    }

    /// Single transport entry point: serializes same-bucket requests behind
    /// a per-bucket gate, waits out known rate-limit windows, and retries
    /// 429s plus — when [`ReplayPolicy`] allows it — 5xx responses and
    /// transient transport failures.
    ///
    /// This is the only non-trivial `async fn` in the request path and it is
    /// not generic, so it is compiled once. `body` arrives already encoded;
    /// an encoding error is returned before anything is sent. See the
    /// Decision Log above `request`.
    async fn execute(
        &self,
        target: Target<'_>,
        path: &str,
        body: EncodedBody,
    ) -> Result<RawResponse, DiscordError> {
        let body = body?;
        let method = target.method();
        let authorization = target.authorization();
        let route = RequestRoute::new(&method, path, target.route());
        let route_key = route.key.as_str();
        let token = route_token(path);
        let replay =
            ReplayPolicy::for_request(&method, &route, body.as_ref(), self.retry_non_idempotent);
        let (mut gate_key, mut _gate_permit) = self.acquire_route_gate(&route).await;
        let mut rate_limit_retries = 0;
        let mut server_error_retries = 0;
        // When the first attempt was sent, and the error of an earlier
        // attempt that Discord may already have applied. Every later send is
        // a replay of that attempt and must stay within `replay`.
        let mut first_sent_at: Option<Instant> = None;
        let mut possibly_applied: Option<DiscordError> = None;

        loop {
            // A bucket learned since the gate was taken — from this request's
            // own response or another request's — moves serialization to the
            // bucket's gate, so requests that queued on the route gate before
            // the bucket was known cannot run beside requests on the bucket.
            if self.rate_limits.block_key_for(&route) != gate_key {
                drop(_gate_permit);
                (gate_key, _gate_permit) = self.acquire_route_gate(&route).await;
            }

            while let Some(wait_duration) = self.rate_limits.wait_duration(&route) {
                if let Some(error) = &possibly_applied {
                    if !replay.allows_replay(first_sent_at, wait_duration) {
                        return Err(error.clone());
                    }
                }
                debug!("waiting for rate limit on {route_key} for {wait_duration:?}");
                sleep_for_retry_after(wait_duration.as_secs_f64()).await;
            }
            if let Some(error) = &possibly_applied {
                if !replay.allows_replay(first_sent_at, Duration::ZERO) {
                    return Err(error.clone());
                }
            }

            first_sent_at.get_or_insert_with(Instant::now);
            let response = match self
                .request_once_bytes(authorization, method.clone(), path, body.as_ref())
                .await
            {
                Ok(response) => response,
                Err(error) => {
                    let error = scrub_error_token(error, token);
                    let backoff = server_error_backoff(server_error_retries + 1);
                    // A connection that was never established cannot have
                    // delivered the request; a timeout or a lost response may
                    // follow a request Discord already applied.
                    let unsent = self.connect_failure_is_unsent(&error);
                    let retry = error.is_retryable_transport()
                        && server_error_retries < MAX_SERVER_ERROR_RETRIES
                        && (unsent
                            || replay
                                .allows_replay(first_sent_at, Duration::from_secs_f64(backoff)));
                    if !retry {
                        return Err(error);
                    }
                    server_error_retries += 1;
                    warn!(
                        "transport error on {route_key}: {error}, retrying ({server_error_retries}/{MAX_SERVER_ERROR_RETRIES})"
                    );
                    if !unsent {
                        possibly_applied.get_or_insert(error);
                    }
                    sleep_for_retry_after(backoff).await;
                    continue;
                }
            };
            let response_text = String::from_utf8_lossy(&response.body);
            self.rate_limits
                .observe(&route, &response.headers, response.status, &response_text);

            if response.status == StatusCode::TOO_MANY_REQUESTS {
                let retry_after =
                    rate_limit::retry_after_seconds(&response.headers, &response_text);
                if let Some(callback) = &self.rate_limit_callback {
                    callback(RateLimitInfo {
                        route: route_key.to_string(),
                        retry_after,
                        global: rate_limit::is_global_rate_limit(&response.headers, &response_text),
                    });
                }

                if rate_limit_retries >= MAX_RATE_LIMIT_RETRIES {
                    return Err(discord_rate_limit_error(route_key, &response_text));
                }
                // Discord did not process this attempt, but an earlier
                // ambiguous one may have been applied; waiting out the 429
                // must not carry the replay past what `replay` allows.
                if let Some(error) = &possibly_applied {
                    let wait = rate_limit::wait_duration_from_seconds(retry_after)
                        .unwrap_or(Duration::ZERO);
                    if !replay.allows_replay(first_sent_at, wait) {
                        return Err(error.clone());
                    }
                }

                rate_limit_retries += 1;
                warn!(
                    "received rate limit for {route_key}, retrying ({rate_limit_retries}/{MAX_RATE_LIMIT_RETRIES})"
                );
                sleep_for_retry_after(retry_after).await;
                continue;
            }

            if response.status.is_server_error() {
                let error = discord_api_error(response.status, &scrub_token(&response_text, token));
                let backoff = server_error_backoff(server_error_retries + 1);
                // A 5xx does not prove the request was rejected; Discord may
                // have applied it before failing.
                if server_error_retries >= MAX_SERVER_ERROR_RETRIES
                    || !replay.allows_replay(first_sent_at, Duration::from_secs_f64(backoff))
                {
                    return Err(error);
                }
                server_error_retries += 1;
                warn!(
                    "received {} for {route_key}, retrying ({server_error_retries}/{MAX_SERVER_ERROR_RETRIES})",
                    response.status
                );
                possibly_applied.get_or_insert(error);
                sleep_for_retry_after(backoff).await;
                continue;
            }

            if !response.status.is_success() {
                return Err(discord_api_error(
                    response.status,
                    &scrub_token(&response_text, token),
                ));
            }

            return Ok(response);
        }
    }

    /// Locks the serialization gate for `route`'s current block key.
    ///
    /// The key is re-checked after the lock is acquired: if Discord reported
    /// the route's bucket while this request was queued, the request moves to
    /// the bucket's gate instead of running beside requests already on it.
    async fn acquire_route_gate(
        &self,
        route: &RequestRoute,
    ) -> (String, tokio::sync::OwnedMutexGuard<()>) {
        loop {
            let key = self.rate_limits.block_key_for(route);
            let permit = self.route_gates.gate_for(&key).lock_owned().await;
            if self.rate_limits.block_key_for(route) == key {
                return (key, permit);
            }
        }
    }

    /// Returns true when `error` proves the request never reached Discord.
    ///
    /// A connect failure only proves that when the transport does not follow
    /// redirects: with redirects, the original request may have succeeded
    /// and only the connection to the redirect target failed. The managed
    /// clients built by this crate disable redirects; a client supplied
    /// through [`RestClientBuilder::use_client`] carries no such guarantee.
    fn connect_failure_is_unsent(&self, error: &DiscordError) -> bool {
        self.managed_transport && is_connect_failure(error)
    }

    async fn request_once_bytes(
        &self,
        authorization: RequestAuthorization<'_>,
        method: Method,
        path: &str,
        body: Option<&RequestBody>,
    ) -> Result<RawResponse, DiscordError> {
        let normalized_path = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{path}")
        };
        let url = format!("{}{}", self.api_base(), normalized_path);

        let mut request_builder = self
            .client
            .request(method.clone(), url)
            .header("User-Agent", &self.user_agent);

        if !matches!(
            body,
            Some(
                RequestBody::Multipart { .. }
                    | RequestBody::NamedFileMultipart { .. }
                    | RequestBody::PayloadAndNamedFileMultipart { .. }
                    | RequestBody::StickerMultipart { .. }
            )
        ) {
            request_builder = request_builder.header("Content-Type", "application/json");
        }

        if method != Method::GET {
            if let Some(reason) = &self.audit_log_reason {
                request_builder =
                    request_builder.header("X-Audit-Log-Reason", encode_audit_log_reason(reason));
            }
        }

        match authorization {
            RequestAuthorization::Bot => {
                request_builder =
                    request_builder.header("Authorization", format!("Bot {}", self.token));
            }
            RequestAuthorization::Auto if request_uses_bot_authorization(&normalized_path) => {
                request_builder =
                    request_builder.header("Authorization", format!("Bot {}", self.token));
            }
            RequestAuthorization::Bearer(token) => {
                request_builder =
                    request_builder.header("Authorization", format!("Bearer {token}"));
            }
            RequestAuthorization::Auto | RequestAuthorization::None => {}
        }

        if let Some(body) = body {
            request_builder = match body {
                RequestBody::Json(value) => request_builder.json(value),
                RequestBody::Multipart {
                    payload_json,
                    files,
                } => request_builder.multipart(build_multipart_form(payload_json, files)?),
                RequestBody::NamedFileMultipart { field_name, file } => {
                    request_builder.multipart(build_named_file_form(None, field_name, file)?)
                }
                RequestBody::PayloadAndNamedFileMultipart {
                    payload_json,
                    field_name,
                    file,
                } => request_builder.multipart(build_named_file_form(
                    Some(payload_json),
                    field_name,
                    file,
                )?),
                RequestBody::StickerMultipart { payload_json, file } => {
                    request_builder.multipart(build_sticker_form(payload_json, file)?)
                }
            };
        }

        let response = request_builder.send().await?;
        let status = response.status();
        let headers = response.headers().clone();
        let body = response.bytes().await?.to_vec();

        Ok(RawResponse {
            status,
            headers,
            body,
        })
    }
}

impl RestClient {
    /// Fills `allowed_mentions` from the configured default when the typed
    /// message body leaves it unset. Borrow-through when nothing changes.
    pub(super) fn message_with_default_allowed_mentions<'a>(
        &self,
        body: &'a CreateMessage,
    ) -> Cow<'a, CreateMessage> {
        match (&self.default_allowed_mentions, &body.allowed_mentions) {
            (Some(default), None) => {
                let mut body = body.clone();
                body.allowed_mentions = Some(default.clone());
                Cow::Owned(body)
            }
            _ => Cow::Borrowed(body),
        }
    }

    /// Inserts the configured default `allowed_mentions` into a raw JSON
    /// object payload that does not already contain the key.
    pub(super) fn json_with_default_allowed_mentions<'a>(&self, body: &'a Value) -> Cow<'a, Value> {
        let Some(default) = &self.default_allowed_mentions else {
            return Cow::Borrowed(body);
        };
        let Some(object) = body.as_object() else {
            return Cow::Borrowed(body);
        };
        if object.contains_key("allowed_mentions") {
            return Cow::Borrowed(body);
        }
        let Ok(default_value) = serde_json::to_value(default) else {
            return Cow::Borrowed(body);
        };
        let mut object = object.clone();
        object.insert("allowed_mentions".to_string(), default_value);
        Cow::Owned(Value::Object(object))
    }

    /// Applies the default `allowed_mentions` to message-bearing interaction
    /// callbacks (`CHANNEL_MESSAGE_WITH_SOURCE`, `UPDATE_MESSAGE`) whose data
    /// object leaves it unset.
    fn interaction_response_with_default_allowed_mentions<'a>(
        &self,
        body: &'a InteractionCallbackResponse,
    ) -> Cow<'a, InteractionCallbackResponse> {
        if self.default_allowed_mentions.is_none()
            || !matches!(
                body.kind,
                InteractionResponseType::CHANNEL_MESSAGE_WITH_SOURCE
                    | InteractionResponseType::UPDATE_MESSAGE
            )
        {
            return Cow::Borrowed(body);
        }
        let Some(data) = body.data.as_ref() else {
            return Cow::Borrowed(body);
        };
        match self.json_with_default_allowed_mentions(data) {
            Cow::Borrowed(_) => Cow::Borrowed(body),
            Cow::Owned(data) => {
                let mut body = body.clone();
                body.data = Some(data);
                Cow::Owned(body)
            }
        }
    }
}

/// Backoff schedule for 5xx and transport retries: 0.5s, 1s, 2s.
/// How long Discord is assumed to deduplicate an enforced message nonce.
///
/// Discord documents the check as covering "the past few minutes" without a
/// number. One minute stays well inside that wording while covering the
/// 0.5s/1s/2s server-error backoff and ordinary rate-limit waits.
const NONCE_DEDUP_WINDOW: Duration = Duration::from_secs(60);

/// Whether a request may be sent again after an attempt that Discord may
/// already have applied (a 5xx, a timeout, or a lost response).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReplayPolicy {
    /// Repeating the request cannot produce a second effect: the HTTP
    /// methods RFC 9110 defines as idempotent, or the caller opted in with
    /// [`RestClientBuilder::retry_non_idempotent_requests`]. A replayed
    /// DELETE whose first attempt succeeded surfaces Discord's 404, which is
    /// the accepted cost of retrying it.
    Always,
    /// A channel message create (the manifest's `CreateMessage` route)
    /// carrying `nonce` + `enforce_nonce: true`. Discord returns the original
    /// message for a repeated nonce, but only within its deduplication
    /// window, so the replay is bounded by [`NONCE_DEDUP_WINDOW`] measured
    /// from the first attempt. A raw request never qualifies, even with the
    /// same path and body: the client does not interpret raw requests.
    WithinNonceWindow,
    /// POST and PATCH in general. Discord PATCH bodies usually set absolute
    /// values, but nothing guarantees that for every route.
    Never,
}

impl ReplayPolicy {
    fn for_request(
        method: &Method,
        route: &RequestRoute,
        body: Option<&RequestBody>,
        retry_non_idempotent: bool,
    ) -> Self {
        if retry_non_idempotent
            || matches!(
                *method,
                Method::GET | Method::HEAD | Method::OPTIONS | Method::PUT | Method::DELETE
            )
        {
            Self::Always
        } else if route.id.is_some_and(RouteId::replays_with_nonce)
            && body.is_some_and(RequestBody::enforces_nonce)
        {
            Self::WithinNonceWindow
        } else {
            Self::Never
        }
    }

    /// Returns true when a replay that starts after `upcoming_wait` is
    /// still allowed.
    fn allows_replay(self, first_sent_at: Option<Instant>, upcoming_wait: Duration) -> bool {
        match self {
            Self::Always => true,
            Self::WithinNonceWindow => first_sent_at.is_none_or(|sent| {
                sent.elapsed().saturating_add(upcoming_wait) < NONCE_DEDUP_WINDOW
            }),
            Self::Never => false,
        }
    }
}

/// Removes the route token from an error produced while sending a request.
fn scrub_error_token(error: DiscordError, token: Option<&str>) -> DiscordError {
    match (error, token) {
        (DiscordError::Http(mut http_error), Some(token)) => {
            http_error.scrub_secret(token);
            DiscordError::Http(http_error)
        }
        (error, _) => error,
    }
}

fn is_connect_failure(error: &DiscordError) -> bool {
    matches!(error, DiscordError::Http(http_error) if http_error.is_connect())
}

/// Delay before the first retry of a 5xx response, in seconds; each later
/// retry doubles it.
const SERVER_ERROR_BACKOFF_BASE_SECS: f64 = 0.5;
/// Doublings stop here: 0.5 s * 2^8 = 128 s.
const SERVER_ERROR_BACKOFF_MAX_DOUBLINGS: u32 = 8;

fn server_error_backoff(attempt: usize) -> f64 {
    let doublings = u32::try_from(attempt.saturating_sub(1))
        .map_or(SERVER_ERROR_BACKOFF_MAX_DOUBLINGS, |doublings| {
            doublings.min(SERVER_ERROR_BACKOFF_MAX_DOUBLINGS)
        });
    SERVER_ERROR_BACKOFF_BASE_SECS * f64::from(1u32 << doublings)
}

/// Percent-encodes an audit-log reason the way discord.js does
/// (`encodeURIComponent`): unreserved characters pass through, everything
/// else — including spaces and any non-ASCII UTF-8 byte — is `%XX`-encoded
/// so the value is always a valid HTTP header.
fn encode_audit_log_reason(reason: &str) -> String {
    paths::percent_encode_component(reason)
}

fn validate_authorization_token<'a>(name: &str, value: &'a str) -> Result<&'a str, DiscordError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(invalid_data_error(format!("{name} must not be empty")));
    }
    if value.chars().any(char::is_control) {
        return Err(invalid_data_error(format!(
            "{name} contains characters that are unsafe in an Authorization header"
        )));
    }
    Ok(value)
}

/// A successful (2xx) response as the transport received it.
struct RawResponse {
    status: StatusCode,
    headers: HeaderMap,
    body: Vec<u8>,
}

impl RawResponse {
    /// The untyped `Value` view used by [`RestClient::request`]: empty is
    /// `Null`, non-JSON text is a `Value::String`.
    fn into_value(self) -> Value {
        parse_body_bytes(&self.body)
    }

    fn discard(self) {}
}

/// Decodes the transport result into `T`; see `body::decode_json_body` for
/// how empty and non-JSON bodies are read. Taking the `Result` keeps each
/// shim a single expression. The plain `match` (rather than `?` or
/// `and_then`) adds no `Try`/`FromResidual`/closure instance per `T`
/// (measured: the decode glue went from about 110 to 70 LLVM IR lines per
/// response type).
fn decode_response<T: DeserializeOwned>(
    response: Result<RawResponse, DiscordError>,
) -> Result<T, DiscordError> {
    match response {
        Ok(response) => decode_json_body(&response.body),
        Err(error) => Err(error),
    }
}

/// `None` for `204 No Content`, otherwise the decoded body.
fn decode_optional_response<T: DeserializeOwned>(
    response: Result<RawResponse, DiscordError>,
) -> Result<Option<T>, DiscordError> {
    match response {
        Ok(response) if response.status == StatusCode::NO_CONTENT => Ok(None),
        Ok(response) => decode_json_body(&response.body).map(Some),
        Err(error) => Err(error),
    }
}

fn discord_api_error(status: StatusCode, body: &str) -> DiscordError {
    let payload = parse_body_value(body.to_string());
    let code = payload.get("code").and_then(Value::as_u64);
    let message = payload
        .get("message")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| payload.as_str().map(str::to_string))
        .unwrap_or_else(|| payload.to_string());
    DiscordError::api(status.as_u16(), code, message)
}

fn discord_rate_limit_error(route: &str, body: &str) -> DiscordError {
    let payload = parse_body_value(body.to_string());
    let retry_after = payload
        .get("retry_after")
        .and_then(Value::as_f64)
        .unwrap_or(1.0);
    DiscordError::rate_limit(route.to_string(), retry_after)
}

async fn sleep_for_retry_after(retry_after_seconds: f64) {
    let duration = rate_limit::wait_duration_from_seconds(retry_after_seconds)
        .unwrap_or(Duration::from_secs(1));
    tokio::time::sleep(duration).await;
}
