use std::fmt::{Debug, Display, Formatter};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use crate::error::DiscordError;

const DEFAULT_GATEWAY_URL: &str = "wss://gateway.discord.gg/";
const DEFAULT_GATEWAY_VERSION: u8 = 10;
/// Default bound on connection setup; see
/// [`GatewayConnectionConfig::setup_timeout`]. Discord sends HELLO right
/// after the WebSocket upgrade, so 15 seconds covers a slow TLS handshake
/// with room to spare while still noticing a gateway that never answers.
const DEFAULT_SETUP_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Typed Discord API enum for `GatewayEncoding`.
pub enum GatewayEncoding {
    /// Discord API enum variant `Json`.
    Json,
}

impl GatewayEncoding {
    pub fn as_str(self) -> &'static str {
        match self {
            GatewayEncoding::Json => "json",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Typed Discord API enum for `GatewayCompression`.
pub enum GatewayCompression {
    /// Discord API enum variant `ZlibStream`.
    ZlibStream,
    #[cfg(feature = "zstd-stream")]
    /// Discord API enum variant `ZstdStream`.
    ZstdStream,
}

impl GatewayCompression {
    pub fn as_str(self) -> &'static str {
        match self {
            GatewayCompression::ZlibStream => "zlib-stream",
            #[cfg(feature = "zstd-stream")]
            GatewayCompression::ZstdStream => "zstd-stream",
        }
    }
}

// [Decision Log] IDENTIFY gate hook
// - Purpose: let something outside the gateway client (the multi-process
//   shard manager, or a user's own queue) pace every fresh IDENTIFY, including
//   the ones the client sends on its own after INVALID_SESSION, close codes
//   4007/4009, or a reconnect before the first READY.
// - Previous implementation and constraints: the client only spaced IDENTIFYs
//   per shard (5s, in-process). A permit taken once before `Client::start()`
//   could not cover those internal re-IDENTIFYs, and two shards of one
//   `max_concurrency` bucket could identify together after a reconnect.
// - Alternatives considered: (1) a callback on `ClientBuilder` (bot.rs is
//   being rewritten in parallel, and in-process supervisors build clients
//   from this config anyway); (2) a channel-based protocol between the
//   client and its owner; (3) a trait object on `GatewayConnectionConfig`.
// - Chosen approach: (3). The client asks the gate for an `IdentifyPermit`
//   before dialing a connection that will IDENTIFY, and reports `Sent` right
//   after the IDENTIFY write or `Abandoned` if the attempt ended earlier.
// - Why: this config already reaches every shard, resume, and reconnect
//   path (like `capabilities`), so no constructor signatures change.
//   Acquiring before dialing rather than after HELLO means the socket never
//   idles in a queue: this client starts heartbeating only after
//   IDENTIFY/RESUME, and a queue wait can exceed `heartbeat_interval`.
// - Trade-offs: the permit also covers connect + HELLO time, which lowers a
//   bucket's throughput slightly. The config compares gates by pointer, so
//   two configs with different gate objects are unequal even when the gates
//   behave the same.

/// Outcome of one IDENTIFY attempt, reported to the [`IdentifyGate`] when
/// its [`IdentifyPermit`] ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdentifyOutcome {
    /// The IDENTIFY payload was (or may have been) written to the socket, so
    /// Discord may have counted it against the rate limit and the daily
    /// session-start budget.
    Sent,
    /// The attempt ended before the IDENTIFY was written (connect failure,
    /// close before HELLO, cancellation): Discord never saw it.
    Abandoned,
}

type FinishIdentify = Box<dyn FnOnce(IdentifyOutcome) + Send>;

/// Permission for exactly one IDENTIFY attempt, obtained from an
/// [`IdentifyGate`].
///
/// The holder calls [`Self::finish`] once the attempt is over. Dropping the
/// permit without `finish` reports [`IdentifyOutcome::Abandoned`], or
/// [`IdentifyOutcome::Sent`] after [`Self::mark_sending`]. A cancelled write
/// may still have reached the socket, so it must not count as abandoned.
pub struct IdentifyPermit {
    on_finish: Option<FinishIdentify>,
    outcome_on_drop: IdentifyOutcome,
}

impl IdentifyPermit {
    /// Creates a permit whose end is reported to `on_finish` exactly once.
    pub fn new(on_finish: impl FnOnce(IdentifyOutcome) + Send + 'static) -> Self {
        Self {
            on_finish: Some(Box::new(on_finish)),
            outcome_on_drop: IdentifyOutcome::Abandoned,
        }
    }

    /// A permit that reports nothing, for gates that only delay.
    pub fn unmetered() -> Self {
        Self {
            on_finish: None,
            outcome_on_drop: IdentifyOutcome::Abandoned,
        }
    }

    /// Marks the IDENTIFY write as started: from now on, dropping the permit
    /// reports [`IdentifyOutcome::Sent`].
    pub fn mark_sending(&mut self) {
        self.outcome_on_drop = IdentifyOutcome::Sent;
    }

    /// Ends the attempt with `outcome`.
    pub fn finish(mut self, outcome: IdentifyOutcome) {
        if let Some(on_finish) = self.on_finish.take() {
            on_finish(outcome);
        }
    }
}

impl Drop for IdentifyPermit {
    fn drop(&mut self) {
        if let Some(on_finish) = self.on_finish.take() {
            on_finish(self.outcome_on_drop);
        }
    }
}

impl Debug for IdentifyPermit {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IdentifyPermit")
            .field("finished", &self.on_finish.is_none())
            .field("outcome_on_drop", &self.outcome_on_drop)
            .finish()
    }
}

/// Future returned by [`IdentifyGate::acquire`].
pub type IdentifyPermitFuture<'a> =
    Pin<Box<dyn Future<Output = Result<IdentifyPermit, DiscordError>> + Send + 'a>>;

/// Paces fresh IDENTIFYs across shards, for example per Discord
/// `max_concurrency` bucket across processes.
///
/// When a gate is set with [`GatewayConnectionConfig::identify_gate`], the
/// gateway client calls [`Self::acquire`] before dialing every connection
/// that will send IDENTIFY: the first connect, and every later fresh session
/// after INVALID_SESSION, close codes 4007/4009, or a reconnect without a
/// resumable session. RESUME attempts do not acquire.
///
/// The gate is then responsible for Discord's limits, and the client's own
/// 5-second per-shard spacing is skipped: at most one IDENTIFY per bucket
/// (`shard_id % max_concurrency`) per 5 seconds, measured from the previous
/// [`IdentifyOutcome::Sent`]. An error from `acquire` stops that shard:
/// `GatewayClient::run` returns it.
///
/// A wait may last long (until a bucket frees up or the daily session
/// budget resets). Meanwhile the client keeps serving its control channel,
/// so a shutdown ends the wait at once. Other commands are kept for the
/// connection in a bounded backlog (the same one used during reconnect
/// backoff): presence updates coalesce to the newest one, and beyond 32
/// other commands the oldest is dropped. Once granted, connection setup
/// (connect, WebSocket upgrade, and HELLO) is bounded by
/// [`GatewayConnectionConfig::setup_timeout`] (15 seconds by default); an
/// attempt that runs over is reported as [`IdentifyOutcome::Abandoned`].
pub trait IdentifyGate: Send + Sync {
    /// Waits until `shard_id` (of `shard_count`) may IDENTIFY.
    fn acquire(&self, shard_id: u32, shard_count: u32) -> IdentifyPermitFuture<'_>;
}

/// Gate handle stored on the connection config. Gates have no meaningful
/// value equality, so configs compare them by pointer identity.
#[derive(Clone)]
struct IdentifyGateHandle(Arc<dyn IdentifyGate>);

impl Debug for IdentifyGateHandle {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("IdentifyGate(..)")
    }
}

impl PartialEq for IdentifyGateHandle {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::addr_eq(Arc::as_ptr(&self.0), Arc::as_ptr(&other.0))
    }
}

impl Eq for IdentifyGateHandle {}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Typed Discord API object for `GatewayConnectionConfig`.
pub struct GatewayConnectionConfig {
    base_url: String,
    version: u8,
    encoding: GatewayEncoding,
    compression: Option<GatewayCompression>,
    shard: Option<(u32, u32)>,
    capabilities: crate::bitfield::GatewayCapabilities,
    identify_gate: Option<IdentifyGateHandle>,
    setup_timeout: Duration,
}

impl Default for GatewayConnectionConfig {
    fn default() -> Self {
        Self::new(DEFAULT_GATEWAY_URL)
    }
}

impl GatewayConnectionConfig {
    /// Creates a `new` value.
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            version: DEFAULT_GATEWAY_VERSION,
            encoding: GatewayEncoding::Json,
            compression: None,
            shard: None,
            capabilities: crate::bitfield::GatewayCapabilities::from_bits(0),
            identify_gate: None,
            setup_timeout: DEFAULT_SETUP_TIMEOUT,
        }
    }

    /// Bounds the setup of every gateway connection built from this config:
    /// TCP/TLS connect, WebSocket upgrade, and HELLO (default 15 seconds).
    /// An attempt that runs over fails like a failed dial and is retried
    /// with the usual 1 to 300 second reconnect backoff, so a link on which
    /// setup is always slower than this never connects: raise it for such
    /// links. While an IDENTIFY permit is held (see [`IdentifyGate`]) the
    /// bound also limits how long the attempt keeps the bucket, and an
    /// attempt that runs over is reported as [`IdentifyOutcome::Abandoned`].
    /// A shutdown or handoff request ends a setup at once, whatever the
    /// bound.
    ///
    /// Up to 3.1 only setups holding an IDENTIFY permit were bounded (to 15
    /// seconds); a RESUME or an ungated IDENTIFY could wait forever for a
    /// gateway that accepted the connection but never sent HELLO.
    pub fn setup_timeout(mut self, timeout: Duration) -> Self {
        self.setup_timeout = timeout;
        self
    }

    /// The bound set with [`Self::setup_timeout`].
    pub fn setup_timeout_duration(&self) -> Duration {
        self.setup_timeout
    }

    pub fn version(mut self, version: u8) -> Self {
        self.version = version;
        self
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }

    pub fn encoding(mut self, encoding: GatewayEncoding) -> Self {
        self.encoding = encoding;
        self
    }

    /// Enable gateway transport compression.
    pub fn compression(mut self, compression: GatewayCompression) -> Self {
        self.compression = Some(compression);
        self
    }

    pub fn compression_kind(&self) -> Option<GatewayCompression> {
        self.compression
    }

    /// Sets the IDENTIFY `capabilities` bitfield (see
    /// [`gateway_capabilities`](crate::bitfield::gateway_capabilities)).
    ///
    /// Capabilities live on the connection config rather than as another
    /// runtime argument because they are part of how a connection is
    /// established, and this config already reaches every shard, resume, and
    /// reconnect path.
    pub fn capabilities(mut self, capabilities: crate::bitfield::GatewayCapabilities) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// Returns the IDENTIFY `capabilities` bitfield.
    pub fn capability_bits(&self) -> u64 {
        self.capabilities.bits()
    }

    pub fn shard(mut self, shard_id: u32, total_shards: u32) -> Self {
        self.shard = Some((shard_id, total_shards));
        self
    }

    /// Routes every fresh IDENTIFY of connections built from this config
    /// through `gate` (see [`IdentifyGate`]). Multi-process shard children
    /// pass `ShardChildProcess::identify_gate()` here.
    pub fn identify_gate(mut self, gate: Arc<dyn IdentifyGate>) -> Self {
        self.identify_gate = Some(IdentifyGateHandle(gate));
        self
    }

    /// The IDENTIFY gate set with [`Self::identify_gate`], if any.
    pub fn identify_gate_ref(&self) -> Option<&Arc<dyn IdentifyGate>> {
        self.identify_gate.as_ref().map(|handle| &handle.0)
    }

    pub fn normalized_url(&self) -> String {
        let mut normalized = if self.base_url.contains("://") {
            self.base_url.clone()
        } else {
            format!("wss://{}", self.base_url.trim_start_matches('/'))
        };

        ensure_root_path(&mut normalized);
        remove_query_param(&mut normalized, "compress");

        if !has_query_param(&normalized, "v") {
            append_query_param(&mut normalized, &format!("v={}", self.version));
        }

        if !has_query_param(&normalized, "encoding") {
            append_query_param(
                &mut normalized,
                &format!("encoding={}", self.encoding.as_str()),
            );
        }

        if let Some(compression) = self.compression {
            if !has_query_param(&normalized, "compress") {
                append_query_param(
                    &mut normalized,
                    &format!("compress={}", compression.as_str()),
                );
            }
        }

        if let Some((shard_id, total_shards)) = self.shard {
            if !has_query_param(&normalized, "shard") {
                append_query_param(&mut normalized, &format!("shard={shard_id},{total_shards}"));
            }
        }

        normalized
    }
}

impl Display for GatewayConnectionConfig {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.normalized_url())
    }
}

/// Gives a host-only URL the root path. The WebSocket handshake sends the URL's path and query as
/// the HTTP request target, so `wss://host?v=10` became `GET ?v=10`, which Discord answers with
/// 400. READY's `resume_gateway_url` has no path (`wss://gateway-us-east1-b.discord.gg`), so up to
/// 3.1.0 every RESUME after a server-requested reconnect (opcode 7) failed this way.
fn ensure_root_path(url: &mut String) {
    let authority_start = url.find("://").map_or(0, |index| index + 3);
    let authority_end = url[authority_start..]
        .find(['/', '?', '#'])
        .map_or(url.len(), |offset| authority_start + offset);
    if !url[authority_end..].starts_with('/') {
        url.insert(authority_end, '/');
    }
}

fn has_query_param(url: &str, key: &str) -> bool {
    url.split_once('?')
        .map(|(_, query)| {
            query
                .split('&')
                .filter(|segment| !segment.is_empty())
                .any(|segment| segment.split('=').next() == Some(key))
        })
        .unwrap_or(false)
}

fn append_query_param(url: &mut String, param: &str) {
    if url.contains('?') {
        url.push('&');
    } else {
        url.push('?');
    }
    url.push_str(param);
}

fn remove_query_param(url: &mut String, key: &str) {
    let original = std::mem::take(url);
    let Some((base, query)) = original.split_once('?') else {
        *url = original;
        return;
    };

    let filtered_segments: Vec<&str> = query
        .split('&')
        .filter(|segment| !segment.is_empty())
        .filter(|segment| segment.split('=').next() != Some(key))
        .collect();

    *url = base.to_string();
    if !filtered_segments.is_empty() {
        url.push('?');
        url.push_str(&filtered_segments.join("&"));
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Arc, GatewayCompression, GatewayConnectionConfig, GatewayEncoding, IdentifyGate,
        IdentifyOutcome, IdentifyPermit, IdentifyPermitFuture,
    };
    use std::sync::Mutex;

    struct InstantGate;

    impl IdentifyGate for InstantGate {
        fn acquire(&self, _shard_id: u32, _shard_count: u32) -> IdentifyPermitFuture<'_> {
            Box::pin(async { Ok(IdentifyPermit::unmetered()) })
        }
    }

    fn recording_permit() -> (IdentifyPermit, Arc<Mutex<Vec<IdentifyOutcome>>>) {
        let outcomes = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&outcomes);
        let permit = IdentifyPermit::new(move |outcome| sink.lock().unwrap().push(outcome));
        (permit, outcomes)
    }

    #[test]
    fn identify_permit_reports_exactly_one_outcome() {
        let (permit, outcomes) = recording_permit();
        permit.finish(IdentifyOutcome::Sent);
        assert_eq!(*outcomes.lock().unwrap(), vec![IdentifyOutcome::Sent]);

        // Dropping without finishing means the IDENTIFY never went out...
        let (permit, outcomes) = recording_permit();
        assert!(format!("{permit:?}").contains("finished: false"));
        drop(permit);
        assert_eq!(*outcomes.lock().unwrap(), vec![IdentifyOutcome::Abandoned]);

        // ...unless the write had started, which must count as sent.
        let (mut permit, outcomes) = recording_permit();
        permit.mark_sending();
        drop(permit);
        assert_eq!(*outcomes.lock().unwrap(), vec![IdentifyOutcome::Sent]);

        IdentifyPermit::unmetered().finish(IdentifyOutcome::Sent);
    }

    #[tokio::test]
    async fn identify_gate_rides_on_the_config_and_compares_by_identity() {
        let gate: Arc<dyn IdentifyGate> = Arc::new(InstantGate);
        let config = GatewayConnectionConfig::default().identify_gate(Arc::clone(&gate));
        let sharded = config.clone().shard(1, 2);
        assert!(sharded.identify_gate_ref().is_some());
        assert_eq!(config, config.clone());
        assert_ne!(
            config,
            GatewayConnectionConfig::default().identify_gate(Arc::new(InstantGate))
        );
        assert_ne!(config, GatewayConnectionConfig::default());
        assert!(format!("{config:?}").contains("IdentifyGate(..)"));

        let permit = sharded
            .identify_gate_ref()
            .unwrap()
            .acquire(1, 2)
            .await
            .unwrap();
        permit.finish(IdentifyOutcome::Sent);
        assert!(GatewayConnectionConfig::default()
            .identify_gate_ref()
            .is_none());
    }

    #[test]
    fn normalized_url_gives_host_only_urls_the_root_path() {
        // READY's resume_gateway_url arrives without a path.
        for (base, expected) in [
            (
                "wss://gateway-us-east1-b.discord.gg",
                "wss://gateway-us-east1-b.discord.gg/?v=10&encoding=json",
            ),
            (
                "wss://gateway.discord.gg?v=10",
                "wss://gateway.discord.gg/?v=10&encoding=json",
            ),
            (
                "gateway.discord.gg",
                "wss://gateway.discord.gg/?v=10&encoding=json",
            ),
            (
                "wss://gateway.discord.gg/",
                "wss://gateway.discord.gg/?v=10&encoding=json",
            ),
            (
                "wss://gateway.discord.test/socket",
                "wss://gateway.discord.test/socket?v=10&encoding=json",
            ),
        ] {
            assert_eq!(
                GatewayConnectionConfig::new(base).normalized_url(),
                expected,
                "{base}"
            );
        }
    }

    #[test]
    fn setup_timeout_defaults_to_15_seconds_and_survives_other_settings() {
        let config = GatewayConnectionConfig::default();
        assert_eq!(
            config.setup_timeout_duration(),
            std::time::Duration::from_secs(15)
        );
        let config = config
            .setup_timeout(std::time::Duration::from_secs(40))
            .shard(1, 2)
            .with_base_url("wss://gateway-us-east1-b.discord.gg");
        assert_eq!(
            config.setup_timeout_duration(),
            std::time::Duration::from_secs(40)
        );
    }

    #[test]
    fn normalized_url_adds_default_query_values() {
        assert_eq!(
            GatewayConnectionConfig::default().normalized_url(),
            "wss://gateway.discord.gg/?v=10&encoding=json"
        );
    }

    #[test]
    fn normalized_url_includes_compression_when_configured_and_adds_shard() {
        let url = GatewayConnectionConfig::new(
            "wss://gateway.discord.gg/?encoding=json&compress=zlib-stream",
        )
        .compression(GatewayCompression::ZlibStream)
        .shard(2, 8)
        .normalized_url();

        assert_eq!(
            url,
            "wss://gateway.discord.gg/?encoding=json&v=10&compress=zlib-stream&shard=2,8"
        );
    }

    #[test]
    fn connection_config_supports_custom_base_url_version_encoding_and_display() {
        let config = GatewayConnectionConfig::new("gateway.discord.test")
            .with_base_url("/gateway.discord.test/socket")
            .version(11)
            .encoding(GatewayEncoding::Json);

        assert_eq!(GatewayEncoding::Json.as_str(), "json");
        assert_eq!(GatewayCompression::ZlibStream.as_str(), "zlib-stream");
        assert_eq!(
            config.normalized_url(),
            "wss://gateway.discord.test/socket?v=11&encoding=json"
        );
        assert_eq!(config.to_string(), config.normalized_url());
    }

    #[cfg(feature = "zstd-stream")]
    #[test]
    fn normalized_url_supports_zstd_stream_compression() {
        let url = GatewayConnectionConfig::new(
            "wss://gateway.discord.gg/?encoding=json&compress=zlib-stream",
        )
        .compression(GatewayCompression::ZstdStream)
        .normalized_url();

        assert_eq!(GatewayCompression::ZstdStream.as_str(), "zstd-stream");
        assert_eq!(
            url,
            "wss://gateway.discord.gg/?encoding=json&v=10&compress=zstd-stream"
        );
    }

    #[test]
    fn normalized_url_does_not_duplicate_existing_query_parameters() {
        let url =
            GatewayConnectionConfig::new("wss://gateway.discord.gg/?v=9&encoding=json&shard=1,4")
                .version(10)
                .shard(2, 8)
                .normalized_url();

        assert_eq!(url, "wss://gateway.discord.gg/?v=9&encoding=json&shard=1,4");
    }
}
