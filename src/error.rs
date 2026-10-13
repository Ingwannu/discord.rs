use std::fmt;
use std::sync::Arc;

use crate::model::{GuildId, Member};

/// Wrapper around `reqwest::Error` that is `Clone`-able.
/// Stores the Display representation since reqwest errors are not Clone,
/// plus classification flags so callers can distinguish timeouts from
/// connection failures without string matching.
#[derive(Clone, Debug)]
pub struct HttpError {
    message: String,
    is_timeout: bool,
    is_connect: bool,
    is_body: bool,
}

impl HttpError {
    /// Creates a `new` value.
    ///
    /// reqwest's message embeds the full request URL, and webhook,
    /// interaction-callback, and follow-up URLs carry a secret token in their
    /// path. These messages end up in logs and in user-visible errors, so the
    /// URL is rewritten with its token segment redacted before it is stored.
    pub fn new(err: &reqwest::Error) -> Self {
        let mut message = err.to_string();
        if let Some(url) = err.url() {
            message = message.replace(url.as_str(), &redact_url(url));
        }
        Self {
            message,
            is_timeout: err.is_timeout(),
            is_connect: err.is_connect(),
            is_body: err.is_body() || err.is_decode(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    /// Replaces every occurrence of a known request secret (a webhook or
    /// interaction token) in the stored message with `:token`.
    pub(crate) fn scrub_secret(&mut self, secret: &str) {
        if !secret.is_empty() {
            self.message = self.message.replace(secret, ":token");
        }
    }

    /// Returns true when the request timed out.
    pub fn is_timeout(&self) -> bool {
        self.is_timeout
    }

    /// Returns true when the connection could not be established.
    pub fn is_connect(&self) -> bool {
        self.is_connect
    }

    /// Returns true when reading or decoding the response body failed.
    pub fn is_body(&self) -> bool {
        self.is_body
    }

    /// Returns true when retrying the request may succeed (timeout,
    /// connect failure, or truncated body), as opposed to errors that are
    /// deterministic such as an invalid request builder.
    pub fn is_retryable(&self) -> bool {
        self.is_timeout || self.is_connect || self.is_body
    }
}

fn redact_url(url: &url::Url) -> String {
    let origin = url.origin().ascii_serialization();
    let path = match url.query() {
        Some(query) => format!("{}?{query}", url.path()),
        None => url.path().to_string(),
    };
    format!("{origin}{}", crate::http::redact_path_tokens(&path))
}

impl fmt::Display for HttpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for HttpError {}

/// Unified error type for the discordrs framework.
///
/// Replaces the previous `Box<dyn Error>` with a matchable enum,
/// following the same pattern as discord.js's error hierarchy.
#[derive(Clone, Debug)]
pub enum DiscordError {
    /// Discord API returned an error response (4xx/5xx).
    Api {
        /// HTTP response status code.
        status: u16,
        /// Discord JSON error code, when present.
        code: Option<u64>,
        /// Discord error message or fallback response text.
        message: String,
    },
    /// An HTTP transport error occurred.
    Http(HttpError),
    /// A rate limit was encountered.
    RateLimit {
        /// Route bucket or request path that was limited.
        route: String,
        /// Retry delay in seconds.
        retry_after: f64,
    },
    /// JSON serialization or deserialization failed.
    Json(String),
    /// An I/O error occurred.
    Io(String),
    /// A model validation or data error.
    Model {
        /// Validation error message.
        message: String,
    },
    /// A gateway protocol error.
    Gateway {
        /// Gateway error message.
        message: String,
    },
    /// A voice subsystem error.
    Voice {
        /// Voice subsystem error message.
        message: String,
    },
    /// A cache operation error.
    Cache {
        /// Cache operation error message.
        message: String,
    },
    /// `Context::fetch_members` received every member chunk, but some
    /// members in them did not decode and are missing. Carries the members
    /// that did decode, so callers can choose to use a partial list.
    IncompleteMemberFetch(Box<IncompleteMemberFetch>),
}

/// The partial result of a `Context::fetch_members` call whose chunks all
/// arrived but held members that did not decode
/// ([`DiscordError::IncompleteMemberFetch`]).
///
/// A member Discord sends in a shape this version cannot read is left out
/// of its GUILD_MEMBERS_CHUNK (logged with `tracing::warn!`) instead of
/// failing the chunk; the fetch reports the loss instead of returning a
/// shorter list as if it were complete.
///
/// ```
/// use discordrs::{error::IncompleteMemberFetch, DiscordError};
///
/// fn members_or_partial(
///     result: Result<Vec<std::sync::Arc<discordrs::Member>>, DiscordError>,
/// ) -> Result<Vec<std::sync::Arc<discordrs::Member>>, DiscordError> {
///     match result {
///         Err(DiscordError::IncompleteMemberFetch(partial)) => {
///             let IncompleteMemberFetch { members, dropped, .. } = *partial;
///             eprintln!("{dropped} members could not be decoded");
///             Ok(members)
///         }
///         other => other,
///     }
/// }
/// # let _ = members_or_partial;
/// ```
// [Decision Log]
// - 목적과 의도: 요소 단위 재디코딩이 멤버를 버린 GUILD_MEMBERS_CHUNK를 `fetch_members`가 완전한
//   수집으로 돌려주지 않게 한다(astra 리뷰 P2). 버리기 전에는 청크 전체가 실패했으므로 호출자는
//   빠진 멤버를 몰래 잃지 않았다.
// - 기존 구현 및 제약 조건: `fetch_members`는 `Result<Vec<Arc<Member>>, DiscordError>`이고 모든
//   청크 인덱스가 와야 끝난다. 디코딩된 멤버는 이미 캐시에 들어간다. `DiscordError`는 `Clone`이고
//   크기가 모든 `Result`에 실린다.
// - 검토한 주요 대안: (1) 메시지만 있는 `Gateway` 오류, (2) 반환 타입을 부분 결과 구조체로 바꾸기,
//   (3) 받은 멤버와 버린 수를 담은 타입 오류 변형.
// - 선택한 방식: (3). `DiscordError::IncompleteMemberFetch(Box<IncompleteMemberFetch>)`.
// - 다른 대안 대신 이 방식을 선택한 이유: (1)은 이미 받은 멤버를 버려 다시 요청하게 만든다. (2)는 드문
//   경우 때문에 모든 호출자의 타입을 바꾼다. (3)은 `?`로 쓰는 코드에는 실패로 드러나고, 부분 목록을
//   받아들일 호출자는 match 한 번으로 꺼낸다. 상자라서 `DiscordError` 크기가 그대로다.
// - 장점, 단점 및 영향: `DiscordError`에 변형이 하나 늘어 그것을 빠짐없이 match하던 코드는 팔을
//   더해야 한다(3.0.0 미배포). 청크 하나가 아니라 모든 청크가 온 뒤에 알리므로 부분 목록이 가능한 한
//   크다.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct IncompleteMemberFetch {
    /// The guild the members were requested for.
    pub guild_id: GuildId,
    /// Every member that decoded, from all chunks. Also in the cache.
    pub members: Vec<Arc<Member>>,
    /// How many members Discord sent that did not decode.
    pub dropped: usize,
}

impl DiscordError {
    /// Creates a `api` value.
    pub fn api(status: u16, code: Option<u64>, message: impl Into<String>) -> Self {
        Self::Api {
            status,
            code,
            message: message.into(),
        }
    }

    /// Creates a `model` value.
    pub fn model(message: impl Into<String>) -> Self {
        Self::Model {
            message: message.into(),
        }
    }

    /// Creates a `rate_limit` value.
    pub fn rate_limit(route: impl Into<String>, retry_after: f64) -> Self {
        Self::RateLimit {
            route: route.into(),
            retry_after,
        }
    }

    /// Creates a `gateway` value.
    pub fn gateway(message: impl Into<String>) -> Self {
        Self::Gateway {
            message: message.into(),
        }
    }

    /// Creates a `voice` value.
    pub fn voice(message: impl Into<String>) -> Self {
        Self::Voice {
            message: message.into(),
        }
    }

    /// Creates a `cache` value.
    pub fn cache(message: impl Into<String>) -> Self {
        Self::Cache {
            message: message.into(),
        }
    }

    /// Returns the HTTP status code if this is an API error.
    pub fn status_code(&self) -> Option<u16> {
        match self {
            Self::Api { status, .. } => Some(*status),
            _ => None,
        }
    }

    /// Returns the Discord error code if this is an API error.
    pub fn discord_code(&self) -> Option<u64> {
        match self {
            Self::Api { code, .. } => *code,
            _ => None,
        }
    }

    /// Returns true when the error is a transient transport failure that a
    /// retry may resolve (timeout, connect failure, truncated body).
    pub fn is_retryable_transport(&self) -> bool {
        match self {
            Self::Http(err) => err.is_retryable(),
            _ => false,
        }
    }
}

impl fmt::Display for DiscordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Api {
                status,
                code,
                message,
            } => {
                write!(f, "Discord API error (status {status}")?;
                if let Some(code) = code {
                    write!(f, ", code {code}")?;
                }
                write!(f, "): {message}")
            }
            Self::Http(err) => write!(f, "HTTP error: {err}"),
            Self::RateLimit { route, retry_after } => write!(
                f,
                "Rate limited on route '{route}', retry after {retry_after}s"
            ),
            Self::Json(msg) => write!(f, "JSON error: {msg}"),
            Self::Io(msg) => write!(f, "I/O error: {msg}"),
            Self::Model { message } => write!(f, "Model error: {message}"),
            Self::Gateway { message } => write!(f, "Gateway error: {message}"),
            Self::Voice { message } => write!(f, "Voice error: {message}"),
            Self::Cache { message } => write!(f, "Cache error: {message}"),
            Self::IncompleteMemberFetch(partial) => write!(
                f,
                "Gateway error: {} members of guild {} did not decode ({} decoded)",
                partial.dropped,
                partial.guild_id,
                partial.members.len()
            ),
        }
    }
}

impl std::error::Error for DiscordError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Http(err) => Some(err),
            _ => None,
        }
    }
}

impl From<reqwest::Error> for DiscordError {
    fn from(err: reqwest::Error) -> Self {
        Self::Http(HttpError::new(&err))
    }
}

impl From<serde_json::Error> for DiscordError {
    fn from(err: serde_json::Error) -> Self {
        Self::Json(err.to_string())
    }
}

impl From<std::io::Error> for DiscordError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err.to_string())
    }
}

impl From<std::env::VarError> for DiscordError {
    fn from(err: std::env::VarError) -> Self {
        Self::Model {
            message: err.to_string(),
        }
    }
}

impl From<String> for DiscordError {
    fn from(message: String) -> Self {
        Self::Model { message }
    }
}

impl From<&str> for DiscordError {
    fn from(message: &str) -> Self {
        Self::Model {
            message: message.to_string(),
        }
    }
}

#[cfg(any(feature = "gateway", feature = "voice"))]
impl From<tokio_tungstenite::tungstenite::Error> for DiscordError {
    fn from(err: tokio_tungstenite::tungstenite::Error) -> Self {
        Self::Gateway {
            message: err.to_string(),
        }
    }
}

#[cfg(feature = "interactions")]
impl From<ed25519_dalek::SignatureError> for DiscordError {
    fn from(err: ed25519_dalek::SignatureError) -> Self {
        Self::Model {
            message: err.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::{DiscordError, HttpError};

    fn make_reqwest_error() -> reqwest::Error {
        reqwest::Client::new()
            .get("http://[::1")
            .build()
            .expect_err("invalid URL should produce a reqwest::Error")
    }

    #[test]
    fn http_error_clones_reqwest_display_message() {
        let err = make_reqwest_error();
        let original = err.to_string();
        let http_error = HttpError::new(&err);

        assert_eq!(http_error.message(), original);
        assert_eq!(http_error.to_string(), original);
        assert!(http_error.source().is_none());
    }

    #[test]
    fn api_error_helpers_cover_both_display_branches() {
        let with_code = DiscordError::api(404, Some(10003), "Unknown Channel");
        let without_code = DiscordError::api(401, None, "Unauthorized");

        assert_eq!(with_code.status_code(), Some(404));
        assert_eq!(with_code.discord_code(), Some(10003));
        assert_eq!(
            with_code.to_string(),
            "Discord API error (status 404, code 10003): Unknown Channel"
        );

        assert_eq!(without_code.status_code(), Some(401));
        assert_eq!(without_code.discord_code(), None);
        assert_eq!(
            without_code.to_string(),
            "Discord API error (status 401): Unauthorized"
        );
    }

    #[test]
    fn named_constructors_and_accessors_match_variants() {
        let model = DiscordError::model("bad model");
        let rate_limit = DiscordError::rate_limit("/channels/1/messages", 1.5);
        let gateway = DiscordError::gateway("gateway down");
        let voice = DiscordError::voice("voice down");
        let cache = DiscordError::cache("cache miss");

        assert!(matches!(
            &model,
            DiscordError::Model { message } if message == "bad model"
        ));
        assert_eq!(model.to_string(), "Model error: bad model");
        assert_eq!(model.status_code(), None);
        assert_eq!(model.discord_code(), None);

        assert!(matches!(
            &rate_limit,
            DiscordError::RateLimit {
                route,
                retry_after,
            } if route == "/channels/1/messages" && (*retry_after - 1.5).abs() < f64::EPSILON
        ));
        assert_eq!(
            rate_limit.to_string(),
            "Rate limited on route '/channels/1/messages', retry after 1.5s"
        );
        assert_eq!(rate_limit.status_code(), None);
        assert_eq!(rate_limit.discord_code(), None);

        assert!(matches!(
            &gateway,
            DiscordError::Gateway { message } if message == "gateway down"
        ));
        assert_eq!(gateway.to_string(), "Gateway error: gateway down");

        assert!(matches!(
            &voice,
            DiscordError::Voice { message } if message == "voice down"
        ));
        assert_eq!(voice.to_string(), "Voice error: voice down");

        assert!(matches!(
            &cache,
            DiscordError::Cache { message } if message == "cache miss"
        ));
        assert_eq!(cache.to_string(), "Cache error: cache miss");
    }

    #[test]
    fn conversion_impls_preserve_messages() {
        let reqwest_error = make_reqwest_error();
        let reqwest_message = reqwest_error.to_string();
        let discord_reqwest = DiscordError::from(reqwest_error);
        assert!(matches!(
            &discord_reqwest,
            DiscordError::Http(err) if err.message() == reqwest_message
        ));
        assert_eq!(
            discord_reqwest.to_string(),
            format!("HTTP error: {reqwest_message}")
        );

        let json_error = serde_json::from_str::<serde_json::Value>("{").unwrap_err();
        let json_message = json_error.to_string();
        let discord_json = DiscordError::from(json_error);
        assert!(matches!(
            &discord_json,
            DiscordError::Json(message) if message == &json_message
        ));
        assert_eq!(
            discord_json.to_string(),
            format!("JSON error: {json_message}")
        );

        let io_error = std::io::Error::other("boom");
        let io_message = io_error.to_string();
        let discord_io = DiscordError::from(io_error);
        assert!(matches!(
            &discord_io,
            DiscordError::Io(message) if message == &io_message
        ));
        assert_eq!(discord_io.to_string(), format!("I/O error: {io_message}"));

        let var_error = std::env::VarError::NotPresent;
        let var_message = var_error.to_string();
        let discord_var = DiscordError::from(var_error);
        assert!(matches!(
            &discord_var,
            DiscordError::Model { message } if message == &var_message
        ));

        let owned = DiscordError::from(String::from("owned"));
        assert!(matches!(
            &owned,
            DiscordError::Model { message } if message == "owned"
        ));

        let borrowed = DiscordError::from("borrowed");
        assert!(matches!(
            &borrowed,
            DiscordError::Model { message } if message == "borrowed"
        ));
    }

    #[test]
    fn source_is_only_exposed_for_http_errors() {
        let http = DiscordError::Http(HttpError::new(&make_reqwest_error()));
        assert_eq!(
            http.source().map(std::string::ToString::to_string),
            Some(
                http.to_string()
                    .trim_start_matches("HTTP error: ")
                    .to_string()
            )
        );

        assert!(DiscordError::api(400, None, "bad request")
            .source()
            .is_none());
        assert!(DiscordError::Json(String::from("bad json"))
            .source()
            .is_none());
        assert!(DiscordError::Io(String::from("disk fail"))
            .source()
            .is_none());
        assert!(DiscordError::model("bad model").source().is_none());
        assert!(DiscordError::rate_limit("/route", 2.0).source().is_none());
    }
}
