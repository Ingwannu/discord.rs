# Cache and Collectors

These layers are optional. They are meant to improve runtime ergonomics without making the base crate heavy.

## Cache

The `cache` feature is enabled by default, so normal installs keep in-memory state for common lookups. `CacheHandle::new()` uses bounded defaults; builds using `default-features = false` keep the cache API available but use empty no-op storage.

Main types:

- `CacheConfig`
- `CacheHandle`
- `CacheBackend`
- `GuildManager`
- `ChannelManager`
- `MemberManager`
- `MessageManager`
- `RoleManager`

The managers prefer cache hits and fall back to `RestClient` fetches.

### GUILD_CREATE Population

Since `2.1`, GUILD_CREATE populates the channel, thread, member, user, voice-state, presence, emoji, sticker, and stage-instance caches under a single write lock; the cached `Guild` is stored with the bulk collections stripped (`Guild::without_create_collections()`) to avoid double storage. GUILD_MEMBERS_CHUNK payloads (including those collected by `Context::fetch_members(...)`) and THREAD_* events are cached too, so cache-aware managers are warm right after a guild becomes available.

### Performance Characteristics

The `2.1` release rewrote the cache hot paths:

- LRU order tracking uses an ordered-map structure (O(log n) per insert) instead of `VecDeque` scans (O(n))
- per-guild member and per-channel message caps use incremental counters instead of full-map scans
- TTL sweeps are throttled to once per 5s
- list reads filter expired entries per-entry under a read lock instead of upgrading to a write lock

`ClientBuilder::cache_config(...)` and `CacheHandle::with_config(...)` let long-running bots tune message, presence, and member storage by size and TTL. Size limits are enforced on insert, and TTL limits are purged on insert, explicit `purge_expired()`, and cache reads for the affected entity type. Use `CacheConfig::unbounded()` only when retaining all cached gateway data is intentional.

Since `2.2.0`, `CacheConfig::sweep_interval(...)` controls how often TTL sweeps may run from the hot upsert paths (default 5s):

```rust
use std::time::Duration;
use discordrs::CacheConfig;

let config = CacheConfig::default().sweep_interval(Duration::from_secs(30));
```

Use `CacheHandle::is_enabled()` when reusable code needs to detect whether it was compiled with real cache storage.

Hot member, message, and presence reads have `Arc` variants (`member_arc`, `message_arc`, `presence_arc`, and manager `cached_arc` helpers) for callers that want to avoid deep cloning cached payloads. The owned-return methods remain available and clone from the same in-memory entries.

`CacheBackend` is the async extension trait for external member/message/presence stores. `CacheHandle` implements it for the default in-memory backend, and external stores can implement the same `Arc`-returning read shape for distributed bot deployments.

Since `2.2.0`, `ClientBuilder::cache_backend(...)` registers an external `CacheBackend` (Redis, Valkey, ...) that receives the same member, message, and presence writes the in-memory cache applies for gateway events. Writes are forwarded from a spawned task per event, so a slow backend cannot stall the gateway:

```rust
use std::sync::Arc;
use discordrs::cache::CacheBackend;
use discordrs::{gateway_intents, Client};

fn with_backend(token: &str, backend: Arc<dyn CacheBackend>) {
    let _builder = Client::builder(token, gateway_intents::GUILD_MESSAGES)
        .cache_backend(backend);
}
```

```rust
use std::time::Duration;
use discordrs::{gateway_intents, CacheConfig, Client};

let client = Client::builder("bot-token", gateway_intents::GUILD_MESSAGES)
    .cache_config(
        CacheConfig::default()
            .max_messages_per_channel(100)
            .max_total_messages(10_000)
            .message_ttl(Duration::from_secs(60 * 60))
            .presence_ttl(Duration::from_secs(10 * 60))
            .max_members_per_guild(25_000),
    );
```

## Collectors

Enable the `collectors` feature when the bot needs event-driven waiting flows.

Main types:

- `CollectorHub`
- `MessageCollector`
- `InteractionCollector`
- `ComponentCollector`
- `ModalCollector`

Collectors subscribe to typed `Event` values and let handlers wait for the next matching runtime event.

### Collector Controls (2.2.0)

All four collector types share the same control surface, mirroring discord.js's `Collector`:

- `filter(...)` — uniform across message, interaction, component, and modal collectors; non-matching items do not count toward `max_items` and do not reset the idle window
- `timeout(Duration)` — overall collection window (`CollectorEndReason::Time`)
- `idle(Duration)` — separate idle window that ends collection when no matching item arrives in time (`CollectorEndReason::Idle`)
- `max_items(n)` — ends with `CollectorEndReason::Limit`
- `stop()` / `stop_with_reason(...)` — explicit stop, recording `"user"` or a custom reason (`CollectorEndReason::User(reason)`)
- `stop_handle()` — cloneable `CollectorStopHandle` for stopping from another task; an in-flight `next()` ends with `None`
- `reset_timer()` — restarts the overall timeout window
- `end_reason()` — why collection ended (`Limit` / `Time` / `Idle` / `User` / `ChannelDropped`)
- `received_count()` — how many items were collected so far

```rust
use std::time::Duration;
use discordrs::collector::CollectorEndReason;

let mut collector = ctx
    .collectors()
    .component_collector()
    .filter(|component| component.data.custom_id == "ticket_open")
    .timeout(Duration::from_secs(60))
    .idle(Duration::from_secs(15))
    .max_items(5);

let stop = collector.stop_handle();
tokio::spawn(async move { stop.stop_with_reason("shutting down") });

while let Some(component) = collector.next().await {
    println!("clicked: {}", component.data.custom_id);
}

if collector.end_reason() == Some(CollectorEndReason::Idle) {
    println!("nobody clicked for 15s");
}
```

## Typical Use

- cache for hot-path lookups
- collectors for button, modal, or follow-up message flows
- both together for stateful multi-step bots
