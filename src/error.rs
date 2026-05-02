use std::fmt;

/// Wrapper around `reqwest::Error` that is `Clone`-able.
/// Stores the Display representation since reqwest errors are not Clone.
#[derive(Clone, Debug)]
pub struct HttpError {
    message: String,
}

impl HttpError {
    /// Creates or returns `new` data.
    pub fn new(err: &reqwest::Error) -> Self {
        Self {
            message: err.to_string(),
        }
    }

    /// Runs the `message` operation.
    pub fn message(&self) -> &str {
        &self.message
    }
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
}

impl DiscordError {
    /// Creates or returns `api` data.
    pub fn api(status: u16, code: Option<u64>, message: impl Into<String>) -> Self {
        Self::Api {
            status,
            code,
            message: message.into(),
        }
    }

    /// Creates or returns `model` data.
    pub fn model(message: impl Into<String>) -> Self {
        Self::Model {
            message: message.into(),
        }
    }

    /// Creates or returns `rate_limit` data.
    pub fn rate_limit(route: impl Into<String>, retry_after: f64) -> Self {
        Self::RateLimit {
            route: route.into(),
            retry_after,
        }
    }

    /// Creates or returns `gateway` data.
    pub fn gateway(message: impl Into<String>) -> Self {
        Self::Gateway {
            message: message.into(),
        }
    }

    /// Creates or returns `voice` data.
    pub fn voice(message: impl Into<String>) -> Self {
        Self::Voice {
            message: message.into(),
        }
    }

    /// Creates or returns `cache` data.
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

#[cfg(feature = "interactions")]
impl From<hex::FromHexError> for DiscordError {
    fn from(err: hex::FromHexError) -> Self {
        Self::Model {
            message: err.to_string(),
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

/// Backward-compatible type alias. Deprecated in favor of [`DiscordError`].
#[deprecated(since = "0.4.0", note = "Use DiscordError instead")]
pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

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
