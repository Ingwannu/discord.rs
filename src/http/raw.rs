//! The raw request escape hatch: any method and path, sent through the same
//! transport as the typed endpoints.

use std::borrow::Cow;

use reqwest::Method;
use serde::de::DeserializeOwned;
use serde::Serialize;

use super::body::{encode_json, EncodedBody, RequestBody, NO_BODY};
use super::{
    decode_response, validate_authorization_token, FileAttachment, RequestAuthorization,
    RestClient, Target,
};
use crate::error::DiscordError;

/// A REST request the client has no typed method for: a route Discord added
/// after this release, or an experiment.
///
/// [`RestClient::request`] sends it through the same transport as every
/// typed endpoint: per-route and global rate limits, the bucket gates, the
/// audit-log reason of [`RestClient::with_reason`], retries, and token
/// redaction in errors and logs. Two things are more conservative than for
/// typed routes, because the client cannot know what the request does:
///
/// - A POST or PATCH is never replayed after an ambiguous failure (a 5xx, a
///   timeout, a lost response), even a message create whose body carries
///   `enforce_nonce`; only
///   [`RestClientBuilder::retry_non_idempotent_requests`](super::RestClientBuilder::retry_non_idempotent_requests)
///   opts in. GET, PUT, and DELETE are retried as usual.
/// - The `Authorization` header follows the path: the bot token, except on
///   tokenized webhook and interaction routes (`/webhooks/{id}/{token}…`,
///   `/interactions/…`), which carry their secret in the path. Use
///   [`Self::bearer`] or [`Self::without_authorization`] to choose.
///
/// The body is encoded once, when it is set, and the same bytes are sent on
/// every attempt. An encoding error is returned by
/// [`RestClient::request`] before anything is sent.
///
/// ```no_run
/// # async fn demo(client: discordrs::RestClient) -> Result<(), discordrs::DiscordError> {
/// use discordrs::RawRequest;
/// use serde_json::{json, Value};
///
/// let channel: Value = client.request(RawRequest::get("/channels/123")).await?;
/// let _: Value = client
///     .request(RawRequest::post("/channels/123/messages").json(&json!({ "content": "hi" })))
///     .await?;
/// # Ok(()) }
/// ```
pub struct RawRequest<'a> {
    method: Method,
    path: Cow<'a, str>,
    body: EncodedBody,
    authorization: RawAuthorization<'a>,
}

#[derive(Clone, Copy)]
enum RawAuthorization<'a> {
    Path,
    Bearer(&'a str),
    None,
}

impl<'a> RawRequest<'a> {
    /// A request without a body. `path` is relative to the API base
    /// (`/channels/123`) and may carry a query string; percent-encode any
    /// caller-supplied segment yourself.
    pub fn new(method: Method, path: impl Into<Cow<'a, str>>) -> Self {
        Self {
            method,
            path: path.into(),
            body: NO_BODY,
            authorization: RawAuthorization::Path,
        }
    }

    /// `GET path`.
    pub fn get(path: impl Into<Cow<'a, str>>) -> Self {
        Self::new(Method::GET, path)
    }

    /// `POST path`.
    pub fn post(path: impl Into<Cow<'a, str>>) -> Self {
        Self::new(Method::POST, path)
    }

    /// `PUT path`.
    pub fn put(path: impl Into<Cow<'a, str>>) -> Self {
        Self::new(Method::PUT, path)
    }

    /// `PATCH path`.
    pub fn patch(path: impl Into<Cow<'a, str>>) -> Self {
        Self::new(Method::PATCH, path)
    }

    /// `DELETE path`.
    pub fn delete(path: impl Into<Cow<'a, str>>) -> Self {
        Self::new(Method::DELETE, path)
    }

    /// Sends `body` as JSON, encoded now with `serde_json`.
    pub fn json<B: Serialize + ?Sized>(mut self, body: &B) -> Self {
        self.body = encode_json(body).map(|bytes| Some(RequestBody::json(bytes)));
        self
    }

    /// Sends `body`, which is already encoded JSON, as-is.
    pub fn json_bytes(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = Ok(Some(RequestBody::json(body.into())));
        self
    }

    /// Sends `payload` as the `payload_json` part of a `multipart/form-data`
    /// body, with one `files[n]` part per file.
    pub fn multipart<B: Serialize + ?Sized>(
        mut self,
        payload: &B,
        files: &[FileAttachment],
    ) -> Self {
        self.body = encode_json(payload).and_then(|bytes| RequestBody::multipart(bytes, files));
        self
    }

    /// Authorizes with an OAuth2 access token (`Authorization: Bearer …`)
    /// instead of the bot token.
    pub fn bearer(mut self, access_token: &'a str) -> Self {
        self.authorization = RawAuthorization::Bearer(access_token);
        self
    }

    /// Sends no `Authorization` header (public routes).
    pub fn without_authorization(mut self) -> Self {
        self.authorization = RawAuthorization::None;
        self
    }

    /// The HTTP method.
    pub fn method(&self) -> &Method {
        &self.method
    }

    /// The path relative to the API base.
    pub fn path(&self) -> &str {
        &self.path
    }
}

impl std::fmt::Debug for RawRequest<'_> {
    /// Shows the method and the path with any route token redacted; never
    /// the body or a bearer token.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RawRequest")
            .field("method", &self.method)
            .field("path", &super::redact_path_tokens(&self.path))
            .finish_non_exhaustive()
    }
}

impl RestClient {
    /// Sends a [`RawRequest`] and decodes the response into `T`
    /// (`serde_json::Value` for untyped JSON, `()` to discard it).
    ///
    /// An empty body decodes as JSON `null` and a non-JSON body as a JSON
    /// string, as for typed endpoints. See [`RawRequest`] for how retries
    /// and authorization differ from typed routes.
    pub async fn request<T: DeserializeOwned>(
        &self,
        request: RawRequest<'_>,
    ) -> Result<T, DiscordError> {
        let RawRequest {
            method,
            path,
            body,
            authorization,
        } = request;
        let authorization = match authorization {
            RawAuthorization::Path => Ok(RequestAuthorization::Auto),
            RawAuthorization::Bearer(token) => validate_authorization_token("bearer_token", token)
                .map(RequestAuthorization::Bearer),
            RawAuthorization::None => Ok(RequestAuthorization::None),
        };
        // `match` rather than `?`: one suspend point, no early return per `T`
        // (see the shim Decision Log in http.rs).
        match authorization {
            Ok(authorization) => decode_response(
                self.execute(Target::Raw(&method, authorization), &path, body)
                    .await,
            ),
            Err(error) => Err(error),
        }
    }
}
