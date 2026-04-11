#[cfg(feature = "gateway")]
use async_trait::async_trait;

use crate::error::DiscordError;
use crate::model::Snowflake;

/// Trait for cache-aware entity managers that can read from in-memory state and fall back to HTTP.
///
/// On gateway builds this parallels discord.js's `CachedManager` pattern.
#[cfg(feature = "gateway")]
#[async_trait]
pub trait CachedManager<T: Clone + Send + Sync + 'static>: Send + Sync {
    /// Fetch from cache first, falling back to HTTP API.
    async fn get(&self, id: impl Into<Snowflake> + Send) -> Result<T, DiscordError>;

    /// Get from cache only (no HTTP fallback).
    async fn cached(&self, id: impl Into<Snowflake> + Send) -> Option<T>;

    /// Check if the entity exists in cache.
    async fn contains(&self, id: impl Into<Snowflake> + Send) -> bool;

    /// List all cached entries.
    async fn list_cached(&self) -> Vec<T>;
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
