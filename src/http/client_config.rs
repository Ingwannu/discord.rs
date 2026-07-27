use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::Duration;

use reqwest::{Client, Proxy};
use tracing::warn;

use crate::error::DiscordError;
use crate::model::AllowedMentions;
use crate::types::invalid_data_error;

use super::rate_limit::RateLimitState;
use super::{RateLimitRouteGates, RestClient, API_BASE, DEFAULT_USER_AGENT};

const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

pub(super) fn default_http_client() -> Client {
    match Client::builder()
        .connect_timeout(DEFAULT_CONNECT_TIMEOUT)
        .timeout(DEFAULT_REQUEST_TIMEOUT)
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            warn!(
                error = %error,
                "failed to build bounded reqwest client; falling back to reqwest defaults"
            );
            Client::new()
        }
    }
}

/// Details of a 429 response observed by the REST client, passed to the
/// callback registered with [`RestClientBuilder::rate_limit_callback`] —
/// the equivalent of discord.js's `rateLimited` REST event.
#[derive(Clone, Debug, PartialEq)]
pub struct RateLimitInfo {
    /// Rate-limit route key (method + major-parameter bucket) that was hit.
    pub route: String,
    /// Seconds Discord asked the client to wait before retrying.
    pub retry_after: f64,
    /// Whether the 429 applies to the global limit rather than a route bucket.
    pub global: bool,
}

/// Callback invoked whenever the REST client receives a 429.
pub type RateLimitCallback = Arc<dyn Fn(RateLimitInfo) + Send + Sync>;

/// Builder for a configurable [`RestClient`], mirroring discord.js's
/// `RESTOptions`. Obtain one with [`RestClient::builder`].
///
/// [`RestClient::new`] keeps the previous zero-configuration behavior.
pub struct RestClientBuilder {
    token: String,
    application_id: u64,
    api_base: Option<String>,
    api_version: Option<u8>,
    connect_timeout: Option<Duration>,
    request_timeout: Option<Duration>,
    user_agent: Option<String>,
    proxy: Option<String>,
    client: Option<Client>,
    rate_limit_callback: Option<RateLimitCallback>,
    default_allowed_mentions: Option<AllowedMentions>,
}

impl RestClientBuilder {
    pub(super) fn new(token: impl Into<String>, application_id: u64) -> Self {
        Self {
            token: token.into(),
            application_id,
            api_base: None,
            api_version: None,
            connect_timeout: None,
            request_timeout: None,
            user_agent: None,
            proxy: None,
            client: None,
            rate_limit_callback: None,
            default_allowed_mentions: None,
        }
    }

    /// Overrides the full API base URL (default `https://discord.com/api/v10`).
    /// Takes precedence over [`Self::api_version`].
    pub fn api_base(mut self, api_base: impl Into<String>) -> Self {
        self.api_base = Some(api_base.into());
        self
    }

    /// Selects the Discord API version, producing
    /// `https://discord.com/api/v{n}`. Ignored when [`Self::api_base`] is set.
    pub fn api_version(mut self, version: u8) -> Self {
        self.api_version = Some(version);
        self
    }

    /// Sets the TCP connect timeout (default 10s). Ignored when
    /// [`Self::use_client`] supplies a prebuilt client.
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = Some(timeout);
        self
    }

    /// Sets the overall per-request timeout (default 30s). Ignored when
    /// [`Self::use_client`] supplies a prebuilt client.
    pub fn request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = Some(timeout);
        self
    }

    /// Overrides the `User-Agent` header sent with every request.
    pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = Some(user_agent.into());
        self
    }

    /// Routes all requests through an HTTP(S) proxy
    /// (`reqwest::Proxy::all`). Ignored when [`Self::use_client`] supplies a
    /// prebuilt client. An invalid proxy URL fails [`Self::build`].
    pub fn proxy(mut self, proxy_url: impl Into<String>) -> Self {
        self.proxy = Some(proxy_url.into());
        self
    }

    /// Escape hatch: use a fully custom `reqwest::Client`. Supersedes
    /// [`Self::connect_timeout`], [`Self::request_timeout`], and
    /// [`Self::proxy`].
    pub fn use_client(mut self, client: Client) -> Self {
        self.client = Some(client);
        self
    }

    /// Registers a callback invoked on every 429 the client receives,
    /// including those it retries internally.
    pub fn rate_limit_callback(mut self, callback: RateLimitCallback) -> Self {
        self.rate_limit_callback = Some(callback);
        self
    }

    /// Sets allowed-mentions injected into outgoing message payloads
    /// (`create_message`, `update_message`, `execute_webhook`, interaction
    /// responses) whenever the payload does not set `allowed_mentions`
    /// itself — the equivalent of discord.js's
    /// `ClientOptions#allowedMentions`.
    pub fn default_allowed_mentions(mut self, allowed_mentions: AllowedMentions) -> Self {
        self.default_allowed_mentions = Some(allowed_mentions);
        self
    }

    /// Builds the [`RestClient`]. Fails only on an invalid proxy URL or a
    /// reqwest client-construction error.
    pub fn build(self) -> Result<RestClient, DiscordError> {
        let base_url = match (self.api_base, self.api_version) {
            (Some(api_base), _) => api_base.trim_end_matches('/').to_string(),
            (None, Some(version)) => format!("https://discord.com/api/v{version}"),
            (None, None) => API_BASE.to_string(),
        };

        let client = match self.client {
            Some(client) => {
                if self.connect_timeout.is_some()
                    || self.request_timeout.is_some()
                    || self.proxy.is_some()
                {
                    warn!(
                        "RestClientBuilder::use_client supersedes connect_timeout, \
                         request_timeout, and proxy"
                    );
                }
                client
            }
            None => {
                let mut builder = Client::builder()
                    .connect_timeout(self.connect_timeout.unwrap_or(DEFAULT_CONNECT_TIMEOUT))
                    .timeout(self.request_timeout.unwrap_or(DEFAULT_REQUEST_TIMEOUT));
                if let Some(proxy_url) = &self.proxy {
                    let proxy = Proxy::all(proxy_url).map_err(|error| {
                        invalid_data_error(format!("invalid proxy url {proxy_url:?}: {error}"))
                    })?;
                    builder = builder.proxy(proxy);
                }
                builder.build().map_err(|error| {
                    invalid_data_error(format!("failed to build reqwest client: {error}"))
                })?
            }
        };

        Ok(RestClient {
            client,
            token: self.token,
            application_id: Arc::new(AtomicU64::new(self.application_id)),
            rate_limits: Arc::new(RateLimitState::default()),
            route_gates: Arc::new(RateLimitRouteGates::default()),
            audit_log_reason: None,
            base_url,
            user_agent: self
                .user_agent
                .unwrap_or_else(|| DEFAULT_USER_AGENT.to_string()),
            default_allowed_mentions: self.default_allowed_mentions,
            rate_limit_callback: self.rate_limit_callback,
        })
    }
}
