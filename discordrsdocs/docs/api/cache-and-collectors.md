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

Since 3.0 (phase 4) `CacheHandle::apply_guild_create` takes the gateway's `GuildCreate` payload and stores its guild, members, and presences as the same `Arc`s the event holds instead of copying them; build one from a REST `Guild` with `GuildCreate::from(guild)`. `apply_guild_update(&Guild)` applies GUILD_UPDATE. An unavailable GUILD_CREATE (`GuildCreatePayload::Unavailable`) is never stored as a guild: the gateway runtime calls `remove_guild` for it, as for GUILD_DELETE with `"unavailable": true`.

### Shared Entries and Copy-on-Write Updates (3.0)

The cache keeps messages, members, presences, and guilds as `Arc`s and shares them with the events that delivered them:

- `upsert_message`, `upsert_member`, `upsert_presence`, and `upsert_guild` take `impl Into<Arc<T>>`: pass the `Arc` an event holds (`EventKind::MessageCreate(message)` → `upsert_message(Arc::clone(message))`) to share it, or a plain value, which is wrapped as before.
- `apply_message_update(&MessageUpdate)` merges a MESSAGE_UPDATE into the cached message under the write lock and returns the merged `Arc<Message>`; a message that is not cached stays uncached, because an update alone is never stored. The merge is copy-on-write: in place when the cache holds the only reference, otherwise into a new snapshot, so an earlier event, collector, or handler that still holds the old `Arc<Message>` keeps seeing the message as it was.
- `apply_partial_user(&PartialUser)` applies MESSAGE_UPDATE's `author` to the cached user under the write lock: sent fields replace, `null` clears, omitted fields keep their cached value, whether or not the message itself is cached. A user that is not cached is stored only when the author is complete (`PartialUser::to_user`, which needs `username`); an id alone never creates one. The gateway runtime calls it for every MESSAGE_UPDATE with an author.
- Cached objects are never mutated behind a shared `Arc`; there is no interior mutability on them.

### READY Invalidation per Shard (3.0)

All shards of a process share one cache, so a READY (a new session, not a RESUME) only drops what that shard's new session replaces — `CacheHandle::clear_shard(shard_id, shard_count)`:

- single-shard bots (`shard_count <= 1`): everything, as before (`CacheHandle::clear()`)
- otherwise: every guild-scoped entry whose guild maps to the shard by `(guild_id >> 22) % shard_count` — the guild, its channels and threads, members, roles, presences, voice states, emojis, stickers, soundboard sounds, scheduled events, stage instances, and messages of those guilds
- shard 0 also drops DM channels and their messages, because Discord delivers DMs only to shard 0
- users are kept: they are shared by every shard a user has guilds on, and `max_users` bounds what is left behind
- a gateway MESSAGE_CREATE without `guild_id` (a DM) also records its DM channel, so shard 0's READY removes it; messages inserted only through REST without a guild in an uncached channel are kept, since their owner cannot be determined, and age out through TTL and size caps
- `VoiceManager` state is not reset by READY; voice connections are managed through `ctx.voice()`

Before 3.0 every READY cleared the whole cache, so a later shard's READY wiped the guilds of shards that were already running.

The cache is updated by each shard's internal event processor, which runs independently of event handlers (see the Event Pipeline section of [Gateway](gateway.md)). A handler may therefore read state that already includes events after the one it is handling.

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

### External Cache Backend

Since `2.2.0`, `ClientBuilder::cache_backend(...)` registers an external `CacheBackend` (Redis, Valkey, ...) that receives the member, message, and presence writes the in-memory cache applies for gateway events. Since 3.0 the mirror is ordered and never silently stale:

- **Ordered writes.** All shards of a client share one worker that applies writes one at a time in gateway order. (Before, each write ran in its own task, so a delayed put could land after a later delete and resurrect the entry.)
- **Epoch contract (normative).** External backends must implement exactly this:
  1. The store keeps **one** epoch for all of its entries, starting at 0.
  2. `clear_cache(expected_epoch, clear_id)` and `clear_shard_cache(expected_epoch, clear_id, shard_id, shard_count)` are conditional: if the store's epoch equals `expected_epoch`, delete the entries in scope, increment the epoch, record `clear_id` with the new epoch as the last applied clear, and return `CacheClear::Applied(new_epoch)`.
  3. Otherwise, if `clear_id` equals the last applied clear's id, the call is a retry of a clear that already committed (its response was lost): change nothing and return `CacheClear::Applied` with the epoch that clear produced. In every other case change nothing and return `CacheClear::Rejected(current_epoch)`. Compare, delete, increment, and recording the id are atomic (one Lua script or `MULTI`/`EXEC` in Redis).
  4. Every `put_*` / `delete_*` carries the epoch it was issued under; a write whose epoch is lower than the store's current epoch is ignored and reported as `Ok(())`, atomically with respect to a concurrent clear.
  5. Any clear — full or per shard — therefore invalidates every earlier epoch, including those of another shard layout.

  `clear_shard_cache` has no default: it must delete the members, presences, and messages of the shard's guilds (and DM messages on shard 0), following the ownership rules above. `CacheHandle` implements the contract.
- **One client per store — enforce it yourself.** A client's first clear is a full clear that claims the store (so entries of a previous client or shard layout cannot survive); it adopts the store's epoch from a rejection and retries. A clear whose call fails stays in doubt and is retried with the same `CacheClearId` before anything else, so a committed clear whose response was lost is recognized as this client's own and never mistaken for a takeover. A previous client's delayed write or clear is rejected by the epoch check. The claim is **not exclusive**, and two running clients sharing a store are **not detected at startup**: if a second client starts while the first runs, it claims the store, both report healthy, and the first client's writes are silently ignored until its next clear (a shard reconnecting or a recovery) is rejected; from then on it reports `superseded`, stops mirroring, and returns an error on shutdown. Deploy one store or key namespace per process, and stop an old process before its replacement starts.
- **Overload and errors: discard-then-clear recovery.** The worker queue holds 8,192 writes. A write that does not fit, or that the backend rejects or panics on (panics are caught, so backend code cannot take the worker down), starts recovery for its shard: the boundary is frozen at the last write admitted so far, every queued write of that shard below it is discarded instead of applied, writes forwarded during recovery are discarded, and the shard is cleared once the worker has passed the boundary. Nothing from before the clear can be replayed after it, and further losses never move the boundary. A failed clear is retried on its own backoff timer (0.5 s doubling to 30 s). The mirror may be temporarily incomplete and refills from later events.
- **Health.** `Context::cache_backend_health()` returns a `CacheBackendHealth` snapshot: `healthy` (no recovery pending, worker running, not superseded), `worker_running`, `superseded`, `dropped_writes`, `quarantined_writes`, `failed_writes`, `resyncs`, `pending_resync_shards`, and `last_error`.
- **Bounded shutdown.** When the client stops, the worker applies the queued writes and keeps retrying pending recoveries for up to `handler_shutdown_timeout`, then exits; if a shard is still unrecovered (or the client was superseded) `start()` / `ShardSupervisor::wait()` return an error naming it. A worker that overruns the bound is aborted and joined, so it issues no backend call after the client returned.
- **Shared snapshots (3.0).** `put_member`, `put_message`, and `put_presence` receive `Arc<Member>` / `Arc<Message>` / `Arc<Presence>`: for MESSAGE_CREATE and member add/update events this is the same allocation the event and the in-memory cache hold, so mirroring copies nothing until your backend serializes it.
- **MESSAGE_UPDATE.** Discord may send partial payloads, decoded as a typed `MessageUpdate`. The worker reads the backend's stored copy (`get_message`), merges the update into it copy-on-write (in place when your backend returns a unique `Arc`), and stores the result with `put_message` under the epoch fence; if the backend has none it stores the in-memory cache's merged `Arc`, and if neither knows the message it skips the write rather than storing a partial message as complete. If `get_message` fails and no in-memory copy exists, the shard is resynced. The merge covers every message field, including `components`, `poll`, `call`, `message_snapshots`, and `sticker_items`; an explicit `null` clears a field, an omitted one is kept, and `[]` empties a list.

```rust
use std::sync::Arc;
use discordrs::cache::CacheBackend;
use discordrs::{gateway_intents, Client};

// `CacheBackend` is not object safe since 3.0: pass `Arc<YourBackend>`.
fn with_backend<B: CacheBackend + 'static>(token: &str, backend: Arc<B>) {
    let _builder = Client::builder(token, gateway_intents::GUILD_MESSAGES)
        .cache_backend(backend);
}
```

The in-memory cache's size and TTL limits are set with `cache_config(...)`:

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
- `MessageCollector` (MESSAGE_CREATE)
- `MessageUpdateCollector` (MESSAGE_UPDATE, as partial `MessageUpdate`s; 3.0)
- `InteractionCollector`
- `ComponentCollector`
- `ModalCollector`
- `ReactionCollector`, with `ReactionCollection`, `CollectedReaction`, and `EmojiKey` for its results

Collectors read the same `Arc<Event>`s the handlers receive and let handlers wait for the next matching runtime event. Filters see the shared payload by reference; only an item a collector yields from `next()` is copied out, and `next_event()` returns the matching `Arc<Event>` itself without copying anything (3.0). Before 3.0 the message collector also yielded MESSAGE_UPDATE payloads as `Message`s; an update is a partial message now, so it has its own `MessageUpdateCollector`.

### Collector Controls (2.2.0)

All collector types share the same control surface, mirroring discord.js's `Collector`:

- `filter(...)` — uniform across collectors; non-matching items do not count toward `max_items` and do not reset the idle window
- `timeout(Duration)` — overall collection window (`CollectorEndReason::Time`)
- `idle(Duration)` — separate idle window that ends collection when no matching item arrives in time (`CollectorEndReason::Idle`)
- `max_items(n)` — ends with `CollectorEndReason::Limit`
- `stop()` / `stop_with_reason(...)` — explicit stop, recording `"user"` or a custom reason (`CollectorEndReason::User(reason)`)
- `stop_handle()` — cloneable `CollectorStopHandle` for stopping from another task; an in-flight `next()` ends with `None`
- `reset_timer()` — restarts the overall timeout and idle windows
- `end_reason()` — why collection ended (`Limit` / `Time` / `Idle` / `User` / `ChannelDropped`, plus the reaction-only reasons below)
- `received_count()` — how many items were collected so far
- `lagged_events()` — how many events this collector lost to journal lag (see below)
- `next()` — the next matching item, copied out of its event; `next_event()` — the next matching event as the shared `Arc<Event>` (no copy), counted toward the limits like `next()` (3.0)
- `collect()` — drives the collector to the end and returns the results; it consumes the collector, so loop over `next()` instead when you also need `end_reason()` or `lagged_events()` afterwards

When several outcomes are ready at once, the precedence is fixed since 3.0: stop, then the overall timeout, then idle, then events. A window that expired while `next()` was not being polled ends collection before any event buffered in the meantime is delivered, so a late event can neither be returned after the deadline nor restart an expired idle window.

Since 3.0, `CollectorEndReason` is `#[non_exhaustive]`, so `match` on it needs a `_` arm.

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

### Reaction Collector (3.0)

`ReactionCollector` is the equivalent of discord.js's `message.createReactionCollector()` / `awaitReactions()`. It is always bound to one message:

- `ctx.collectors().reaction_collector(message_id)` — only the message id is known; the channel and guild are learned from the first reaction event
- `ctx.collectors().reaction_collector_for_message(&message)` — takes the channel and guild from the `Message`, so deleting them ends collection right away (or set them with `.channel_id(...)` / `.guild_id(...)`)

`next()` yields each `MESSAGE_REACTION_ADD` (`ReactionEvent`) that passes the filter. `collected()` returns the running tally, and `collect()` waits for the end and returns the final `ReactionCollection`, keyed by `EmojiKey` in first-collected order:

- `EmojiKey::Custom(id)` for custom emojis (keyed by id, like discord.js's `emoji.id ?? emoji.name`), `EmojiKey::Unicode(text)` for Unicode emojis. The text is compared as Discord sends it, so `"❤"` and `"❤️"` are different keys.
- `CollectedReaction` holds `users()` (normal reactions) and `burst_users()` (super reactions) separately, because one user can hold both for the same emoji; `count()` adds them, like Discord's reaction `count`.
- `ReactionCollection::total()`, `len()` (distinct emojis), `users()` / `user_count()` (distinct users) mirror discord.js's `collector.total`, `collected.size`, and `collector.users`. Like discord.js, `total()` is a counter: every accepted add counts (even re-adding a removed reaction) and, with `dispose(true)`, applied removals subtract. `reaction_count()` is the size of the tally right now; the two differ when a user removes and re-adds a reaction without `dispose(true)`.

Limits and end reasons:

| Option | Counts | End reason |
|---|---|---|
| `max_items(n)` | `total()`: accepted adds minus applied disposals (discord.js `max`) | `Limit` |
| `max_emojis(n)` | distinct emojis | `EmojiLimit` |
| `max_users(n)` | distinct users | `UserLimit` |
| `timeout` / `idle` | as for every collector | `Time` / `Idle` |
| — | the message is deleted (alone or in a bulk delete) | `MessageDelete` |
| — | its channel / thread is deleted, or the parent channel of its thread (see below) | `ChannelDelete` / `ThreadDelete` |
| — | the bot leaves or is removed from the guild (outages with `unavailable: true` do not count) | `GuildDelete` |

A limit of `0` ends collection before waiting, as `max_items(0)` does on the other collectors. discord.js treats `0` as "no limit"; here that means not calling the method.

For a message inside a thread, Discord sends no `THREAD_DELETE` when the thread disappears with its parent channel, and neither `Message` nor reaction events name the parent. The collector therefore ends on the parent's `CHANNEL_DELETE` only when it knows the parent: pass it with `.parent_channel_id(...)` (for example from the cached thread channel), or let the collector see a `THREAD_CREATE` / `THREAD_UPDATE` / `THREAD_LIST_SYNC` for the thread while it runs. Otherwise such a collector ends only by its timers or a stop.

With `dispose(true)`, `MESSAGE_REACTION_REMOVE`, `MESSAGE_REACTION_REMOVE_ALL`, and `MESSAGE_REACTION_REMOVE_EMOJI` update the tally, and disposed reactions make room again under `max_items`. Without it the tally only grows, meaning "everyone who reacted while the collector ran". This differs from discord.js in one place: discord.js always empties its collection on `MESSAGE_REACTION_REMOVE_ALL`, while here every removal kind waits for `dispose(true)`. As in discord.js, the filter also decides which removals apply, so a filter that reads fields only present on add events (`member`, `message_author_id`) blocks every removal.

```rust
use std::time::Duration;
use discordrs::EmojiKey;

// Poll: count ✅ / ❌ for 60 seconds, ignoring the bot's own reactions.
// `bot_user_id: UserId` is the bot's own id, for example saved from READY.
let tally = ctx
    .collectors()
    .reaction_collector_for_message(&poll_message)
    .filter(move |reaction| reaction.user_id != Some(bot_user_id))
    .dispose(true)
    .timeout(Duration::from_secs(60))
    .collect()
    .await;

let yes = tally.get(&EmojiKey::unicode("✅")).map_or(0, |r| r.count());
let no = tally.get(&EmojiKey::unicode("❌")).map_or(0, |r| r.count());
```

Reactions need the `GUILD_MESSAGE_REACTIONS` (or `DIRECT_MESSAGE_REACTIONS`) intent.

### Delivery, Lag, and Loss

`CollectorHub` keeps the most recent events in a sequence-numbered journal (since 3.0; it was a `tokio::sync::broadcast` channel before). The client publishes every dispatched event to it after updating the cache, and each collector reads it through its own subscription. The journal holds the handler's own `Arc<Event>`s, so a slot is one pointer (an empty 256-slot journal is 2 KiB) and publishing, replaying, and filtering never copy a payload:

- **From now, or from the handled event.** Collectors created from the hub (`ctx.collectors().message_collector()`, `reaction_collector(...)`, ...) see only events published after they were created. Inside a handler, `ctx.message_collector()`, `ctx.reaction_collector(...)`, and the other `Context` collector methods start at the event the handler is handling: events published while the handler call was queued are replayed first, then live events follow without a gap (the hub's `*_collector_from(cursor)` methods do the same for any `EventCursor`). Older history is not kept; fetch it over REST if you need it.
- **Bounded, lossy journal.** The journal keeps the last `capacity` events (256 by default, `CollectorHub::with_capacity(...)` to change it). It holds every event type, not only the ones a collector wants, and a collector reads it only while `next()` / `collect()` is being awaited. A collector that falls more than `capacity` events behind, or starts from a cursor older than the journal, skips the evicted events, adds them to `lagged_events()`, and continues from the oldest event still kept. Publishing never blocks the gateway.
- **Consequences for reactions.** A reaction collector that lagged may have missed adds (undercount), removals with `dispose(true)` (overcount), or the deletion event that should have ended it (it then runs until its timers or a stop). When exact numbers matter, drive `next()` yourself so you can check `lagged_events() == 0` at the end, or re-read the message's reactions over REST.

## Typical Use

- cache for hot-path lookups
- collectors for button, modal, reaction, or follow-up message flows
- both together for stateful multi-step bots
