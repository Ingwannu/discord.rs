# Gateway API

Gateway runtime is provided behind the `gateway` feature.

## Primary Types

- `Client`: high-level typed runtime surface for Gateway bots
- `ClientBuilder`: runtime configuration + startup
- `Event`: typed Gateway event enum
- `GatewayClient`: raw websocket lifecycle management (identify, heartbeat, resume, reconnect)
- `Context`: shared runtime handles (`http`, cache, typemap, shard info)
- `EventHandler`: async trait with `handle_event(ctx, event)`
- `EventDispatchMode`: serial (default) or concurrent handler scheduling
- `RequestChannelInfo`: typed opcode 43 request for ephemeral voice-channel metadata

## Setup

```toml
[dependencies]
discordrs = { version = "2.2.0", features = ["gateway"] }
```

## Boot Pattern

```rust
Client::builder(&token, gateway_intents::GUILDS | gateway_intents::GUILD_MESSAGES)
    .event_handler(handler)
    .start()
    .await?;
```

## Intents

`gateway_intents` exposes named constants for every documented intent bit, including the poll intents added in `2.1`:

- `gateway_intents::GUILD_MESSAGE_POLLS` (`1 << 24`) and `gateway_intents::DIRECT_MESSAGE_POLLS` (`1 << 25`) enable the `MESSAGE_POLL_VOTE_*` events; both are included in `gateway_intents::NON_PRIVILEGED`.
- `gateway_intents::GUILD_EXPRESSIONS` is the current Discord name for bit 3 (the older `GUILD_EMOJIS_AND_STICKERS` constant remains).

## Initial Presence and Dispatch Mode

`ClientBuilder::presence(...)` delivers an initial presence inside IDENTIFY, so the bot connects with the desired status and activity instead of updating it after READY. `ClientBuilder::event_dispatch(...)` chooses how handler calls are scheduled:

```rust
use discordrs::{EventDispatchMode, UpdatePresence};

Client::builder(&token, intents)
    .event_handler(handler)
    .presence(UpdatePresence::online_with_activity("Handling tickets"))
    .event_dispatch(EventDispatchMode::Concurrent)
    .start()
    .await?;
```

- `EventDispatchMode::Serial` (default): handler calls on a shard run one at a time, in gateway order. INTERACTION_CREATE uses its own lane (see Interaction Lane below).
- `EventDispatchMode::Concurrent`: handler calls run as concurrent tasks, at most `max_concurrent_handlers` per shard (default 256), so one slow handler no longer delays the others.

### Event Pipeline (3.0)

Each shard processes events in two stages. An internal processor applies every event in gateway order — cache updates, external cache backend writes, voice state, `fetch_members` chunk correlation, and collector publication — without waiting for handlers. Handler calls are then scheduled from bounded per-shard lanes. Consequences:

- A handler can `await` `ctx.fetch_members(...)` or a collector in either mode without stalling its shard. (Before 3.0, Serial mode deadlocked until the fetch timed out, because the chunks queued behind the waiting handler.)
- The cache a handler reads may already include events that come after the one it is handling.
- A handler that panics is logged and does not stop later handler calls.

Both stages are bounded by `ClientBuilder::event_queue_capacity(...)` (default 10,000 events per shard and stage):

| Queue | When it is full |
|---|---|
| gateway → internal processor | The shard is **stopped** with a `DiscordError::Gateway` ("gateway event queue overflowed"), its cache entries are invalidated (`CacheHandle::clear_shard`, and `clear_shard_cache` on an external backend), and its pending `fetch_members` calls fail. The gateway client has already advanced its sequence number, so a lost dispatch can never be replayed by a RESUME; continuing would serve a cache with a silent gap. The read loop also handles heartbeats, so it never waits for space. This only happens if internal processing itself stalls, for example when a task holds the `ctx.voice()` write lock indefinitely (also across `fetch_members().await`). A processor that dies for any other reason stops the shard the same way. |
| internal processor → handlers | The event is not delivered to the handler; the cache, collectors, and `fetch_members` still saw it. A rate-limited warning reports how many deliveries were dropped. |

A backlog warning is logged when either queue is half full (at most every 30 seconds per shard).

```rust
use std::time::Duration;
use discordrs::{Client, EventDispatchMode};

Client::builder(&token, intents)
    .event_handler(handler)
    .event_dispatch(EventDispatchMode::Concurrent)
    .max_concurrent_handlers(64)
    .event_queue_capacity(20_000)
    .handler_shutdown_timeout(Duration::from_secs(10))
    .start()
    .await?;
```

### Interaction Lane (3.0)

INTERACTION_CREATE events skip the regular handler queue. Each shard has an interaction lane with its own bounded queue (`interaction_queue_capacity`, default 256) and reserved execution slots (`max_concurrent_interactions`, default 32), so a slow regular handler cannot make an interaction miss Discord's three-second initial-response window.

- **Ordering:** interactions are admitted in arrival order. Admitted interactions run concurrently with each other (up to the slot limit) and with regular handler calls — also in `EventDispatchMode::Serial`. Finished handler tasks are reaped before every admission, so a permanently backlogged lane holds at most its slot count (plus one just finished) in memory.
- **Admission deadline:** an interaction waits for a slot only until `interaction_admission_timeout` (default 2 seconds) after the shard received it. If it is not admitted by then, or does not fit in the queue, it goes to the overload hook instead of the handler. Expiry outranks a free slot: an interaction whose deadline has passed is never started, even if a slot frees up at the same moment, and the deadline is checked again right before dispatch.
- **Overload hook:** `on_interaction_overload(...)` receives an `OverloadedInteraction` (`ctx`, `interaction`, `reason`, `waited`) so the application can reply or defer. The runtime never acknowledges interactions on its own, because autocomplete, modal, and component interactions need different responses. The `interaction` owns its shared response state, so a reply or defer through `InteractionResponder` goes through the same acknowledgement state machine as a handler reply. The hook runs on a dedicated thread per shard (with the Tokio runtime entered), never on the event pipeline, so a slow or blocking hook cannot stall cache updates or deliveries; up to 64 overloaded interactions wait for it, and further ones are dropped with a rate-limited warning. Without a hook, such interactions are logged and Discord shows "This interaction failed".
- `Context::event_received_at()` tells a handler when its event arrived, so interaction handlers can see how much of the window is left.

```rust
use discordrs::response::InteractionResponder;
use discordrs::{Client, Interaction};

Client::builder(&token, intents)
    .event_handler(handler)
    .on_interaction_overload(|overloaded| {
        let ctx = overloaded.ctx;
        // Reply only where a message reply fits; autocomplete needs
        // respond_autocomplete, and some flows are better left unanswered.
        if let Interaction::ChatInputCommand(command) = overloaded.interaction {
            tokio::spawn(async move {
                let _ = command
                    .reply_ephemeral(&ctx.http, "Busy right now, please try again.")
                    .await;
            });
        }
    })
    .start()
    .await?;
```

### Collectors Inside Handlers (3.0)

The processor publishes an event to the collector hub before queueing its handler call, so by the time a handler runs, later events may already have been published. `Context::message_collector()`, `component_collector()`, `interaction_collector()`, and `modal_collector()` start at the event the handler is handling (its `Context::event_cursor()`) and replay what was published since, then continue live without a gap. The hub keeps the last 256 events; older ones are skipped and counted in the collector's `lagged_events()`. `ctx.collectors().message_collector()` and friends still start "from now".

### Shutdown and Errors (3.0)

`ClientBuilder::start()` returns once the shard's gateway connection has ended plus at most `handler_shutdown_timeout` (default 5 seconds), plus up to a second to confirm cancellation and for the overload hook thread to return. The window starts when the gateway exits, not when shutdown is requested: queued events keep being delivered and running handlers may finish within it; then the remaining handler tasks are aborted **and joined**, and an external cache backend's queued writes are flushed within the same bound (its worker is joined too, so no write from this client lands after `start()` returned). If a task cannot confirm cancellation within one second of being aborted (for example a handler blocking its thread), the call returns an error saying so instead of claiming the shard stopped.

The return value is the gateway's result, so a non-reconnectable close code such as 4004 (authentication failed) or 4014 (disallowed intents), or an event queue overflow, reaches the caller as `DiscordError::Gateway`. Cancelling the `start()` future also stops the shard's processor and handler tasks. See [Sharding](sharding.md) for the multi-shard equivalent.

## Default Allowed Mentions and Cache Backend (2.2.0)

`ClientBuilder` gains two configuration hooks in `2.2.0`:

```rust
use std::sync::Arc;
use discordrs::cache::CacheBackend;
use discordrs::{AllowedMentions, Client};

fn configure(token: &str, intents: u64, backend: Arc<dyn CacheBackend>) {
    let _builder = Client::builder(token, intents)
        // Injected into outgoing message payloads whenever the payload does
        // not set allowed_mentions itself — discord.js ClientOptions#allowedMentions.
        .default_allowed_mentions(AllowedMentions::default())
        // Forwards member/message/presence cache writes to an external store
        // (Redis, Valkey, ...) through one ordered worker per client, so a
        // slow backend cannot stall the gateway.
        .cache_backend(backend);
}
```

Since 3.0 the backend receives writes in gateway order and must implement an epoch contract: one store-wide epoch, compare-and-set clears (`clear_cache` / `clear_shard_cache` take the expected epoch and return `CacheClear::Applied` or `Rejected`), and writes that carry their epoch, so a previous client's late write or clear is rejected and a new shard layout invalidates every old writer. Only one running client may mirror into a store. A dropped, failed, or panicking write discards that shard's earlier queued writes and clears the shard instead of leaving the mirror stale; `Context::cache_backend_health()` reports the mirror's state; shutdown reports shards left unrecovered; and partial MESSAGE_UPDATE payloads are merged instead of stored as complete messages. See the External Cache Backend section of [Cache and Collectors](cache-and-collectors.md).

The same allowed-mentions default exists on the REST side via `RestClient::builder().default_allowed_mentions(...)`, and `Context::default_allowed_mentions()` exposes the configured value inside handlers.

## Respond to Interactions from Gateway Events (2.2.0)

Import `discordrs::response::InteractionResponder` and reply to `Event::InteractionCreate` payloads directly, discord.js-style:

```rust
use discordrs::response::InteractionResponder;
use discordrs::{Context, Event, Interaction};

async fn on_event(ctx: Context, event: Event) -> Result<(), discordrs::DiscordError> {
    if let Event::InteractionCreate(event) = event {
        if let Interaction::ChatInputCommand(command) = event.interaction {
            command.reply(&ctx.http, "hi").await?;
        }
    }
    Ok(())
}
```

See [HTTP and Helpers](http-and-helpers.md) for the full responder surface (`reply_ephemeral`, `defer`, `edit_reply`, `follow_up`, `show_modal`, `respond_autocomplete`, ...).

## Fetch Guild Members over the Gateway

`Context::fetch_members(...)` requests guild members over the gateway and awaits the correlated `GUILD_MEMBERS_CHUNK` payloads — the discord.js `guild.members.fetch()` equivalent. Fetched members and presences also fill the cache.

```rust
// All members (requires the GUILD_MEMBERS privileged intent).
let members = ctx.fetch_members(guild_id, None, None).await?;

// Username prefix search with a limit.
let admins = ctx.fetch_members(guild_id, Some("admin".to_string()), Some(10)).await?;
```

`Context::fetch_members_with_timeout(...)` overrides the default 60-second overall deadline, and the lower-level `ctx.request_guild_members(...)` remains available for raw chunk handling.

Since 3.0, `fetch_members` is safe to await inside any handler (see Event Pipeline above), returns only once every chunk index `0..chunk_count` arrived (a missing chunk is a timeout error naming how many arrived, never a silently incomplete list), removes its chunk waiter when the call returns, times out, or is cancelled, and fails immediately if the shard stops. Guild-scoped gateway commands — `request_guild_members`, `fetch_members`, `update_voice_state`, `join_voice`, `leave_voice` — are sent on the shard that owns the guild (`(guild_id >> 22) % shard_count`) when that shard runs in the same process, so any shard's `Context` can use them.

## Event Surface

- Prefer `handle_event` for new code.
- `Event` currently exposes typed variants for `READY`, message events, interaction events, guild/channel/member/role cache flows, `CHANNEL_INFO`, `RATE_LIMITED`, and `Unknown`.
- Reaction dispatches preserve Discord's current metadata fields, including `member`, `message_author_id`, `burst`, `burst_colors`, and `reaction_type`.
- Presence dispatches expose the documented partial user ID, activities, and `ClientStatus` platform status metadata.
- Legacy `ready`, `message_create`, `interaction_create`, and `raw_event` hooks still exist for migration.

## Gateway Control Helpers

`Context` and `ShardMessenger` expose typed control paths for common outbound Gateway commands:

- presence updates
- guild member requests
- soundboard sound requests
- voice state updates
- channel info requests through `request_channel_info(RequestChannelInfo::voice_metadata(guild_id))`

`RequestChannelInfo` serializes to Discord Gateway opcode 43 and currently exposes the documented `status` and `voice_start_time` fields. Incoming `CHANNEL_INFO` dispatches decode to `Event::ChannelInfo`.

## Operational Notes

- Keep handler methods non-blocking: a handler that blocks the thread (rather than awaiting) still holds a runtime worker.
- Push heavy work to background tasks, or use `EventDispatchMode::Concurrent`; in Serial mode a slow handler delays the next handler call (not cache updates) and eventually drops deliveries once `event_queue_capacity` events are waiting.
- Use `Context.rest()` or the cache-aware managers from `Context`.
- `BotClient` still exists as a compatibility alias, but the docs prefer `Client`.
- `1.2.2` fixes Gateway compression negotiation so default connections do not request payload compression without a decoder, and explicit `zlib-stream` connections decode compressed `HELLO` frames before Identify.
- `2.1` paces IDENTIFY to Discord's 1-per-5s-per-shard limit, and short-lived sessions (< 30s) reconnect with escalating backoff instead of immediately, preventing tight reconnect/re-IDENTIFY loops on repeated INVALID_SESSION.
- `2.1` surfaces gateway protocol failures (missing Hello, decode errors, terminal close codes, command-queue overflow) as `DiscordError::Gateway` instead of `DiscordError::Model`.
- `3.0` separates internal event processing from handler calls, bounds both per-shard queues and concurrent handler tasks, gives interactions their own lane with an admission deadline, stops a shard (and invalidates its cache) when its event stream would otherwise have a gap, scopes READY cache invalidation to the receiving shard, and returns from `start()` / `ShardSupervisor::wait()` promptly once the gateway stops (see above).
- `3.0` stops a shard on Discord's non-reconnectable close codes (4004 authentication failed, 4010 invalid shard, 4011 sharding required, 4012 invalid API version, 4013 invalid intents, 4014 disallowed intents): `GatewayClient::run` returns the `DiscordError::Gateway` instead of reconnecting forever with backoff. 4007 (invalid sequence) and 4009 (session timed out) start a fresh session instead of attempting a RESUME that Discord would reject. Per-connection heartbeat and outbound tasks are stopped on every exit path.

## Channel Obfuscation (3.0)

From 2026-11-16 Discord dispatches channels a bot cannot view with obfuscated metadata (`CHANNEL_CREATE`, `CHANNEL_UPDATE`, and the channel list in `GUILD_CREATE`); `GET /guilds/{id}/channels` omits them. Detect them with `Channel::is_obfuscated()` (the `CHANNEL_OBFUSCATED` flag, `1 << 17`), never by name. Only `id`, `type`, `position`, and `parent_id` are reliable on such a channel, and its permission overwrites are a single synthetic `@everyone` deny. When the bot regains access, Discord sends a `CHANNEL_UPDATE` with the full channel, which replaces the cached copy.

To test before the deadline, opt in through IDENTIFY `capabilities` (Discord documents this opt-in as temporary):

```rust
use discordrs::bitfield::gateway_capabilities;

let client = Client::builder(token, intents)
    .gateway_capabilities(gateway_capabilities::CHANNEL_OBFUSCATION)
    .event_handler(handler);
```
