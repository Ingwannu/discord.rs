use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use reqwest::{header::HeaderMap, StatusCode};

use super::body::{header_string, parse_body_value};
use super::paths::RequestRoute;
use super::RouteId;

pub(crate) const RATE_LIMIT_BUCKET_RETENTION: Duration = Duration::from_secs(60 * 60);
const CLEANUP_INTERVAL: Duration = Duration::from_secs(30);

/// Upper bound for any server-provided wait.
///
/// Waits come from headers and bodies the client does not control (Discord,
/// Cloudflare, or a proxy in between). The cap is far above any real Discord
/// limit, so it never shortens a legitimate wait; it only keeps
/// `Instant + Duration` and `Duration::from_secs_f64` from panicking on
/// absurd values such as `inf` or `1e300`.
pub(crate) const MAX_SERVER_WAIT: Duration = Duration::from_secs(24 * 60 * 60);

/// Clamps a server-provided wait in seconds into `0..=MAX_SERVER_WAIT`.
/// Returns `None` for NaN, which carries no usable information, so callers
/// fall through to the next source instead of guessing.
pub(crate) fn sanitize_wait_seconds(seconds: f64) -> Option<f64> {
    (!seconds.is_nan()).then(|| seconds.clamp(0.0, MAX_SERVER_WAIT.as_secs_f64()))
}

/// Converts a server-provided wait to a `Duration` without panicking.
/// `Duration::from_secs_f64` panics on negative, NaN, infinite, or
/// overflowing input, and each of those is reachable from a malformed
/// header, so every conversion of a parsed wait goes through here.
pub(crate) fn wait_duration_from_seconds(seconds: f64) -> Option<Duration> {
    sanitize_wait_seconds(seconds)
        .map(|seconds| Duration::try_from_secs_f64(seconds).unwrap_or(MAX_SERVER_WAIT))
}

fn header_seconds(headers: &HeaderMap, name: &str) -> Option<f64> {
    header_string(headers.get(name))
        .and_then(|value| f64::from_str(value.trim()).ok())
        .and_then(sanitize_wait_seconds)
}

/// Extracts the retry delay in seconds for a 429 response, preferring the
/// JSON body's fractional `retry_after`, then the `Retry-After` header
/// (integer seconds, also present on Cloudflare-level bans with non-JSON
/// bodies), then `x-ratelimit-reset-after`, then a one-second fallback.
/// The result is always finite and within `0..=MAX_SERVER_WAIT`.
pub(crate) fn retry_after_seconds(headers: &HeaderMap, body: &str) -> f64 {
    let payload = parse_body_value(body.to_string());
    payload
        .get("retry_after")
        .and_then(serde_json::Value::as_f64)
        .and_then(sanitize_wait_seconds)
        .or_else(|| header_seconds(headers, "retry-after"))
        .or_else(|| header_seconds(headers, "x-ratelimit-reset-after"))
        .unwrap_or(1.0)
}

/// Returns the key requests are blocked and serialized under.
///
/// Once Discord has reported a bucket hash for a route, the key is the hash
/// combined with the route's major-parameter scope, so routes that share a
/// real bucket wait for each other while different channels, guilds, or
/// webhooks that merely report the same hash stay independent (Discord's
/// bucket hash excludes major parameters). Before the bucket is known the
/// route key itself is used.
fn bucket_block_key(route: &RequestRoute, bucket: Option<&str>) -> String {
    match bucket {
        Some(bucket) => format!("{bucket}@{}", route.major_scope),
        None => route.key.clone(),
    }
}

/// Records `until` for `key` unless a later deadline is already recorded.
/// Responses for one bucket can be observed out of order by concurrent
/// requests; a shorter, older observation must never cut a 429 wait short.
fn extend_deadline(slot: &mut Option<Instant>, until: Instant) {
    if slot.is_none_or(|current| current < until) {
        *slot = Some(until);
    }
}

/// Whether `route` is a manifest route that Discord does not bind to the
/// bot's global rate limit.
fn is_exempt_from_global_limit(route: &RequestRoute) -> bool {
    route.id.is_some_and(RouteId::exempt_from_global_limit)
}

/// Returns true when a 429 applies to the global limit rather than a
/// route bucket, checking the `x-ratelimit-global` and
/// `x-ratelimit-scope: global` headers as well as the JSON body flag.
pub(crate) fn is_global_rate_limit(headers: &HeaderMap, body: &str) -> bool {
    if headers.get("x-ratelimit-global").is_some() {
        return true;
    }
    if header_string(headers.get("x-ratelimit-scope")).as_deref() == Some("global") {
        return true;
    }
    parse_body_value(body.to_string())
        .get("global")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

/// Shared by every client clone and updated by each response. One std lock
/// per map, each taken briefly from sync code and never two at once.
#[derive(Default)]
pub(crate) struct RateLimitState {
    pub(crate) route_buckets: Mutex<HashMap<String, String>>,
    pub(crate) bucket_last_seen: Mutex<HashMap<String, Instant>>,
    pub(crate) blocked_until: Mutex<HashMap<String, Instant>>,
    /// The bot's global limit: set by a global 429 on a request that carried
    /// an `Authorization` header, and waited for by every request except the
    /// interaction-token routes, which Discord does not bind to it.
    pub(crate) global_blocked_until: Mutex<Option<Instant>>,
    /// The global limit Discord applies per IP address to requests without
    /// an `Authorization` header (interaction and webhook token routes,
    /// public routes): set by a global 429 on any of them and waited for by
    /// all of them.
    pub(crate) unauthenticated_global_blocked_until: Mutex<Option<Instant>>,
    last_cleanup: Mutex<Option<Instant>>,
}

impl RateLimitState {
    fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
        mutex
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// How long a request for `route` must wait: until the global blocks
    /// that apply to it end, then until its bucket's block ends.
    ///
    /// Two global deadlines exist (see the fields). The bot's applies to
    /// every request except the interaction-token routes
    /// ([`RouteId::exempt_from_global_limit`]): without that exemption, a
    /// global 429 on any REST call delayed `defer()` and every other initial
    /// callback past Discord's three-second deadline. The unauthenticated
    /// one applies to every request sent without an `Authorization` header,
    /// interaction routes included, because a global 429 there means the IP
    /// address is limited for all of them. Webhook-token routes are not
    /// documented as exempt from the bot's limit, so they wait for both. A
    /// raw request has no route ID: it waits for the bot's deadline, and for
    /// the unauthenticated one when it sends no header.
    pub(crate) fn wait_duration(&self, route: &RequestRoute) -> Option<Duration> {
        let now = Instant::now();
        self.maybe_cleanup(now);
        let bot_global = if is_exempt_from_global_limit(route) {
            None
        } else {
            *Self::lock(&self.global_blocked_until)
        };
        let unauthenticated_global = if route.authorized {
            None
        } else {
            *Self::lock(&self.unauthenticated_global_blocked_until)
        };
        if let Some(global_until) = bot_global.max(unauthenticated_global) {
            if global_until > now {
                return Some(global_until.duration_since(now));
            }
        }

        let route_bucket_key = self.block_key_for(route);

        Self::lock(&self.blocked_until)
            .get(&route_bucket_key)
            .copied()
            .and_then(|until| {
                if until > now {
                    Some(until.duration_since(now))
                } else {
                    None
                }
            })
    }

    pub(crate) fn observe(
        &self,
        route: &RequestRoute,
        headers: &HeaderMap,
        status: StatusCode,
        body: &str,
    ) {
        let now = Instant::now();
        self.maybe_cleanup(now);
        if let Some(bucket_id) = header_string(headers.get("x-ratelimit-bucket")) {
            Self::lock(&self.route_buckets).insert(route.key.clone(), bucket_id.clone());
            Self::lock(&self.bucket_last_seen).insert(bucket_id, now);
        }

        if status == StatusCode::TOO_MANY_REQUESTS {
            let retry_after = retry_after_seconds(headers, body);
            let blocked_until =
                now + wait_duration_from_seconds(retry_after).unwrap_or(MAX_SERVER_WAIT);

            // A global 429 belongs to what Discord limited: the token a
            // request authorized with, or, for a request without an
            // `Authorization` header, the IP address. Recording an
            // unauthenticated one as the bot's would stall every bot
            // request; recording it on the route alone would let requests
            // with other interaction or webhook tokens keep running into it.
            if is_global_rate_limit(headers, body) {
                let deadline = if route.authorized {
                    &self.global_blocked_until
                } else {
                    &self.unauthenticated_global_blocked_until
                };
                extend_deadline(&mut Self::lock(deadline), blocked_until);
            } else {
                self.block_key(route, headers, blocked_until);
            }
            return;
        }

        let remaining = header_string(headers.get("x-ratelimit-remaining"))
            .and_then(|value| value.parse::<u64>().ok());
        let reset_after =
            header_seconds(headers, "x-ratelimit-reset-after").and_then(wait_duration_from_seconds);

        if remaining == Some(0) {
            if let Some(reset_after) = reset_after {
                self.block_key(route, headers, now + reset_after);
            }
        }
    }

    /// Resolves the key a request for `route` waits and serializes on;
    /// see [`bucket_block_key`].
    pub(crate) fn block_key_for(&self, route: &RequestRoute) -> String {
        let bucket = Self::lock(&self.route_buckets).get(&route.key).cloned();
        bucket_block_key(route, bucket.as_deref())
    }

    fn block_key(&self, route: &RequestRoute, headers: &HeaderMap, blocked_until: Instant) {
        let bucket = header_string(headers.get("x-ratelimit-bucket"))
            .or_else(|| Self::lock(&self.route_buckets).get(&route.key).cloned());
        if let Some(bucket) = &bucket {
            // Retention is tracked per Discord bucket hash so stale route →
            // bucket mappings are dropped together with their bucket.
            Self::lock(&self.bucket_last_seen).insert(bucket.clone(), Instant::now());
        }
        let block_key = bucket_block_key(route, bucket.as_deref());
        let mut blocked = Self::lock(&self.blocked_until);
        let mut slot = blocked.get(&block_key).copied();
        extend_deadline(&mut slot, blocked_until);
        if let Some(until) = slot {
            blocked.insert(block_key, until);
        }
    }

    /// Runs the retention sweep at most once per [`CLEANUP_INTERVAL`].
    /// Expiry of individual blocks is already checked at read time, so
    /// delaying the sweep never delays a request.
    fn maybe_cleanup(&self, now: Instant) {
        {
            let mut last_cleanup = Self::lock(&self.last_cleanup);
            if last_cleanup
                .is_some_and(|last| now.saturating_duration_since(last) < CLEANUP_INTERVAL)
            {
                return;
            }
            *last_cleanup = Some(now);
        }
        self.cleanup_old_buckets(now);
    }

    pub(crate) fn cleanup_old_buckets(&self, now: Instant) {
        for deadline in [
            &self.global_blocked_until,
            &self.unauthenticated_global_blocked_until,
        ] {
            let mut global = Self::lock(deadline);
            if global.is_some_and(|until| until <= now) {
                *global = None;
            }
        }

        {
            let mut blocked_until = Self::lock(&self.blocked_until);
            blocked_until.retain(|_, until| *until > now);
        }

        let stale_buckets = {
            let mut bucket_last_seen = Self::lock(&self.bucket_last_seen);
            let stale = bucket_last_seen
                .iter()
                .filter_map(|(bucket, last_seen)| {
                    (now.saturating_duration_since(*last_seen) >= RATE_LIMIT_BUCKET_RETENTION)
                        .then_some(bucket.clone())
                })
                .collect::<HashSet<_>>();
            bucket_last_seen.retain(|bucket, _| !stale.contains(bucket));
            stale
        };

        if !stale_buckets.is_empty() {
            Self::lock(&self.route_buckets).retain(|_, bucket| !stale_buckets.contains(bucket));
        }
    }
}
