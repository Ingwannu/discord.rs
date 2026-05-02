use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use reqwest::{header::HeaderMap, StatusCode};

use super::body::{header_string, parse_body_value};

pub(crate) const RATE_LIMIT_BUCKET_RETENTION: Duration = Duration::from_secs(60 * 60);

#[derive(Default)]
pub(crate) struct RateLimitState {
    pub(crate) route_buckets: Mutex<HashMap<String, String>>,
    pub(crate) bucket_last_seen: Mutex<HashMap<String, Instant>>,
    pub(crate) blocked_until: Mutex<HashMap<String, Instant>>,
    pub(crate) global_blocked_until: Mutex<Option<Instant>>,
}

impl RateLimitState {
    fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
        mutex
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(crate) fn wait_duration(&self, route_key: &str) -> Option<Duration> {
        let now = Instant::now();
        self.cleanup_old_buckets(now);
        if let Some(global_until) = *Self::lock(&self.global_blocked_until) {
            if global_until > now {
                return Some(global_until.duration_since(now));
            }
        }

        let blocked_until = Self::lock(&self.blocked_until);
        let route_bucket_key = Self::lock(&self.route_buckets)
            .get(route_key)
            .cloned()
            .unwrap_or_else(|| route_key.to_string());

        blocked_until
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
        route_key: &str,
        headers: &HeaderMap,
        status: StatusCode,
        body: &str,
    ) {
        let now = Instant::now();
        self.cleanup_old_buckets(now);
        if let Some(bucket_id) = header_string(headers.get("x-ratelimit-bucket")) {
            Self::lock(&self.route_buckets).insert(route_key.to_string(), bucket_id.clone());
            Self::lock(&self.bucket_last_seen).insert(bucket_id.clone(), now);
        }

        if status == StatusCode::TOO_MANY_REQUESTS {
            let payload = parse_body_value(body.to_string());
            let retry_after = payload
                .get("retry_after")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(1.0);
            let blocked_until = now + Duration::from_secs_f64(retry_after.max(0.0));

            if payload
                .get("global")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
            {
                *Self::lock(&self.global_blocked_until) = Some(blocked_until);
            } else {
                self.block_key(route_key, headers, blocked_until);
            }
            return;
        }

        let remaining = header_string(headers.get("x-ratelimit-remaining"))
            .and_then(|value| value.parse::<u64>().ok());
        let reset_after = header_string(headers.get("x-ratelimit-reset-after"))
            .and_then(|value| f64::from_str(&value).ok())
            .map(Duration::from_secs_f64);

        if remaining == Some(0) {
            if let Some(reset_after) = reset_after {
                self.block_key(route_key, headers, now + reset_after);
            }
        }
    }

    fn block_key(&self, route_key: &str, headers: &HeaderMap, blocked_until: Instant) {
        let bucket_key = header_string(headers.get("x-ratelimit-bucket"))
            .or_else(|| Self::lock(&self.route_buckets).get(route_key).cloned())
            .unwrap_or_else(|| route_key.to_string());

        Self::lock(&self.blocked_until).insert(bucket_key.clone(), blocked_until);
        Self::lock(&self.bucket_last_seen).insert(bucket_key, Instant::now());
    }

    pub(crate) fn cleanup_old_buckets(&self, now: Instant) {
        {
            let mut global = Self::lock(&self.global_blocked_until);
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
