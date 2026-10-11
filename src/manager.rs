#[cfg(feature = "gateway")]
use std::future::Future;

use crate::error::DiscordError;
use crate::model::Snowflake;

/// Trait for cache-aware entity managers that can read from in-memory state and fall back to HTTP.
///
/// On gateway builds this parallels discord.js's `CachedManager` pattern.
/// The methods return `Send` futures, so generic code can `tokio::spawn`
/// them; implement them as plain `async fn` (no attribute macro) that hold
/// no non-`Send` value across an `.await`.
#[cfg(feature = "gateway")]
pub trait CachedManager<T: Clone + Send + Sync + 'static>: Send + Sync {
    /// Fetch from cache first, falling back to HTTP API.
    fn get(
        &self,
        id: impl Into<Snowflake> + Send,
    ) -> impl Future<Output = Result<T, DiscordError>> + Send;

    /// Get from cache only (no HTTP fallback).
    fn cached(&self, id: impl Into<Snowflake> + Send) -> impl Future<Output = Option<T>> + Send;

    /// Check if the entity exists in cache.
    fn contains(&self, id: impl Into<Snowflake> + Send) -> impl Future<Output = bool> + Send;

    /// List all cached entries.
    fn list_cached(&self) -> impl Future<Output = Vec<T>> + Send;
}

/// Sync trait shape exposed for non-gateway builds.
///
/// This keeps generic code compiling without the `gateway` feature, but this crate does not
/// currently provide built-in `CachedManager` implementations for the exported manager types in
/// non-gateway builds.
#[cfg(not(feature = "gateway"))]
pub trait CachedManager<T: Clone + Send + Sync + 'static>: Send + Sync {
    fn get(&self, id: impl Into<Snowflake>) -> Result<T, DiscordError>;
    fn cached(&self, id: impl Into<Snowflake>) -> Option<T>;
    fn contains(&self, id: impl Into<Snowflake>) -> bool;
    fn list_cached(&self) -> Vec<T>;
}
