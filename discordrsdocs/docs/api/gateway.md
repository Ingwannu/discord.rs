# Gateway API

Gateway runtime is provided behind the `gateway` feature.

## Primary Types

- `Client`: high-level typed runtime surface for Gateway bots
- `ClientBuilder`: runtime configuration + startup
- `Event`: one received dispatch, shared as `Arc<Event>`: its decoded `EventKind`, `EventMetadata`, and optional raw payload (see [Events and Handlers](#events-and-handlers-30))
- `GatewayClient`: raw websocket lifecycle management (identify, heartbeat, resume, reconnect)
- `Context`: shared runtime handles (`http`, cache, typemap, shard info)
- `EventHandler`: handler trait with `handle_event(ctx, event)` and typed hooks, implemented with plain `async fn`
- `EventDispatchMode`: serial (default) or concurrent handler scheduling
- `RequestChannelInfo`: typed opcode 43 request for ephemeral voice-channel metadata

## Setup

```toml
[dependencies]
discordrs = { version = "3.0.0", features = ["gateway"] }
```

## Boot Pattern

```rust
Client::builder(&token, gateway_intents::GUILDS | gateway_intents::GUILD_MESSAGES)
    .event_handler(handler)
    .start()
    .await?;
```

## Implementing handlers without `#[async_trait]`

Since 3.0 the crate no longer depends on `async-trait`. `EventHandler`, `CacheBackend`, `InteractionHandler`, `TypedInteractionHandler`, and `CachedManager` declare their methods as `fn ... -> impl Future<Output = T> + Send` (return-position `impl Trait` in traits), so implementations are written with plain `async fn`. Delete `#[async_trait]` and the `use async_trait::async_trait;` import from your impls, and drop `async-trait` from your `Cargo.toml` if nothing else uses it.

Before (2.x):

```rust
use async_trait::async_trait;
use discordrs::{Context, EventHandler, Message};

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn message_create(&self, _ctx: Context, message: Message) {
        println!("{}", message.content);
    }
}
```

After (3.0; since phase 4 the hook receives the shared `Arc<Message>`, see [Events and Handlers](#events-and-handlers-30)):

```rust
use std::sync::Arc;

use discordrs::{Context, EventHandler, Message};

struct Handler;

impl EventHandler for Handler {
    async fn message_create(&self, _ctx: Context, message: Arc<Message>) {
        println!("{}", message.content);
    }
}
```

What else to know:

- **Futures must be `Send`.** Handlers and cache backend calls run on spawned Tokio tasks, so an `async fn` in your impl must not hold a non-`Send` value (`Rc`, a `RefCell` borrow, a `std::sync::MutexGuard`) across an `.await`. The compiler reports this at your impl ("future cannot be sent between threads safely"); use `Arc`, `tokio::sync::Mutex`, or drop the value before awaiting.
- **Leftover `#[async_trait]` attributes fail with `E0195`** ("lifetime parameters or bounds on method ... do not match the trait declaration"). That error means: remove the attribute.
- **No `dyn EventHandler` / `dyn CacheBackend`.** The traits are no longer object safe. `ClientBuilder::event_handler(handler)` still takes any handler by value. `ClientBuilder::cache_backend` now takes `Arc<B>` for a concrete backend type `B: CacheBackend` instead of `Arc<dyn CacheBackend>`; `.cache_backend(Arc::new(MyBackend::new()))` keeps compiling, while a variable typed `Arc<dyn CacheBackend>` must become `Arc<MyBackend>` (or a generic `Arc<B>`).
- Default methods are unchanged: implement only the hooks you need. Overriding `handle_event` replaces the default routing to the typed hooks.
- Internally the runtime boxes each handler call once, as async-trait did; the default `handle_event` routing now allocates once per event instead of twice.

## Events and Handlers (3.0)

Every dispatch is decoded once and reaches the cache, the collector hub, and your handler as one shared `Arc<Event>`; nothing on the way copies the payload.

```rust
use std::sync::Arc;

use discordrs::{Context, Event, EventHandler, EventKind};

struct Handler;

impl EventHandler for Handler {
    async fn handle_event(&self, _ctx: Context, event: Arc<Event>) {
        match event.kind() {
            EventKind::Ready(ready) => println!("ready as {}", ready.user.username),
            EventKind::MessageCreate(message) => println!("{}", message.content),
            EventKind::Unknown => println!("new dispatch {}", event.name()),
            _ => {}
        }
    }
}
```

- `event.kind()` is the decoded `EventKind`: one variant per dispatch, plus `Unknown` (a dispatch this crate has no type for) and `DecodeFailed(EventDecodeError)` (a known dispatch whose payload did not match its type). Payloads the cache keeps as they are — `MessageCreate(Arc<Message>)`, `MessageUpdate(Arc<MessageUpdate>)`, and `MemberAdd` / `MemberUpdate` (`MemberEvent { guild_id, member: Arc<Member> }`) — are `Arc`s shared with the cache and an external cache backend; other large payloads are boxed; small id and delete events are stored inline. `size_of::<Event>()` is at most 192 bytes (2.x: 4,848), checked by a layout test.
- `event.metadata()` (`EventMetadata`) holds the dispatch `name()`, the receiving `shard()`, the gateway `sequence()`, and `received_at()`, taken when the read loop received the frame (before any queue). `event.name()` is a shortcut for the name.
- `event.raw()` is the original `d` JSON as `Option<&RawValue>`. Known events keep it only when the client is built with `ClientBuilder::keep_raw_events(true)` (default off: raw payloads made a MESSAGE_CREATE event five times larger); unknown dispatches and decode failures always keep it, so nothing Discord sends is lost. `Debug` for `Event` prints only the raw payload's length, because payloads can carry interaction tokens.
- **GUILD_CREATE** decodes to `GuildCreatePayload`: `Available(Box<GuildCreate>)` for a guild the bot can read, or `Unavailable(UnavailableGuild)` when Discord sends the unavailable guild object (`{"id": "...", "unavailable": true}`, a guild still in an outage). An unavailable guild carries only its id and is never turned into a `Guild`; the cache drops its copy of that guild, as it does for GUILD_DELETE with `"unavailable": true`, and the guild comes back with a full GUILD_CREATE once it is available. `payload.id()` works for both, and `payload.available()` returns the `GuildCreate`. `GuildCreate` holds `guild` (`Arc<Guild>` with the GUILD_CREATE-only collections and the roles empty) plus `roles: Vec<Arc<Role>>`, `channels` and `threads` (`Vec<Arc<Channel>>`), `members: Vec<Arc<Member>>`, `presences: Vec<Arc<Presence>>`, `voice_states`, `emojis`, `stickers`, `stage_instances`, `soundboard_sounds`, and `guild_scheduled_events`. Every channel, thread, and voice state already has its `guild_id`, and every presence its `user_id`. The cache stores the same `Arc`s. `to_guild()` rebuilds the 2.x `Guild` if you need it.
- **GUILD_UPDATE** decodes to `GuildUpdate` (checkpoint C): the guild fields as `Patch<T>` / `NullablePatch<T>` (omitted keeps, `null` clears), `roles: Patch<Vec<Arc<Role>>>`, `emojis`, and `stickers`. Decoding the whole `Guild` used to clear every field GUILD_UPDATE does not carry, including the GUILD_CREATE-only `member_count`, `joined_at`, and `large`. The `guild_update` hook takes `&GuildUpdate`; `update.apply_to(&mut guild)` merges it, and `GuildUpdate::from(guild)` turns a REST `Guild` into an update that replaces every field.
- **GUILD_MEMBER_UPDATE** decodes to `MemberUpdate` (checkpoint C), a partial member like `MessageUpdate`: `guild_id`, the full `user` (`Arc<User>`, shared with the cache's user store), and every other member field as `Patch<T>` / `NullablePatch<T>`. `update.apply_to(&mut member)` merges it. The cache merges it into the member it holds (copy-on-write) and stores the user; a member it does not hold stays uncached, because the update's omitted fields are unknown (2.x stored the update as a whole member). The external cache backend merges it the same way through its ordered worker.
- **PRESENCE_UPDATE** decodes to `PresenceUpdateEvent { guild_id, user: Option<PartialUser>, presence: Arc<Presence> }` (checkpoint C): the presence is built once and shared with the cache and the external cache backend, which store that `Arc`; read it through `user_id()`, `status()`, `activities()`, and `client_status()`. The partial `user` (usually only `id`) patches the cached user when it carries more than the id.
- **Presences and activities as received.** A received `Activity` has `buttons: Option<Vec<String>>`: over the gateway Discord sends only the button labels (Discord docs, Activity Buttons: "When received over the gateway, the `buttons` field is an array of strings, which are the button labels. Bots cannot access a user's activity button URLs"); the `{label, url}` object shape is also read, as its label. Discord does not validate the field types of presences, so the gateway decodes them defensively: in PRESENCE_UPDATE an activity that does not decode is left out (the others stay) and a field of the wrong type reads as `None`; in GUILD_CREATE and GUILD_MEMBERS_CHUNK a member, presence, activity, or voice state that does not decode is left out and the rest of the guild or chunk is kept, with one `warn!` per dispatch counting what was dropped (the guild's own fields, channels, threads, and roles must still decode), where one such element used to fail the whole dispatch and leave the guild uncached.
- **MESSAGE_UPDATE** decodes to `MessageUpdate`, a partial message: every mutable field is a `Patch<T>` (fields that are `T` on `Message`) or `NullablePatch<T>` (fields that are `Option<T>`). An omitted field is `Missing` (keep), `null` clears, and `[]` replaces a list with an empty one; `author` is a `PartialUser` merged field by field. Apply an update to a message you hold with `update.apply_to(&mut message)`; an update is never turned into a full message on its own. The built-in cache patches the cached author the same way (`CacheHandle::apply_partial_user`), so a username-only edit keeps the cached avatar and `"avatar": null` clears it.

The typed hooks receive each payload the way the event stores it, so no hook call copies a payload:

| Hook | Receives |
|---|---|
| `handle_event` | `Arc<Event>` |
| `message_create` | `Arc<Message>` |
| `message_update` | `Arc<MessageUpdate>` |
| `member_add` | `GuildId`, `Arc<Member>` |
| `member_update` | `Arc<MemberUpdate>` (checkpoint C) |
| `guild_create` | `&GuildCreatePayload` (available or unavailable guild) |
| `channel_create`, `channel_update` | `Arc<Channel>` (checkpoint C) |
| `resumed` | nothing beyond `ctx` |
| `raw_event` (unknown dispatches) | `&str` name, `&RawValue` payload |
| `decode_failed` (new) | `&str` name, `&EventDecodeError`, `&RawValue` payload |
| every other hook | a reference into the event, for example `&ReadyPayload`, `&Channel`, `&Interaction`, `&ReactionEvent` |

An `Arc` argument can be kept or moved into a task as it is. A reference lives as long as the handler call; clone what you keep, or override `handle_event` and keep the `Arc<Event>`.

### Migrating from the 2.x `Event` enum

```rust
// 2.x
async fn handle_event(&self, ctx: Context, event: Event) {
    match event {
        Event::Ready(ready) => println!("{}", ready.data.user.username),
        Event::MessageCreate(event) => println!("{}", event.message.content),
        _ => {}
    }
}

// 3.0
async fn handle_event(&self, ctx: Context, event: Arc<Event>) {
    match event.kind() {
        EventKind::Ready(ready) => println!("{}", ready.user.username),
        EventKind::MessageCreate(message) => println!("{}", message.content),
        _ => {}
    }
}
```

- Match on `event.kind()` (`EventKind::...`) instead of `event`. The old `Event::kind()` string is `event.name()`.
- Wrapper structs that only held the payload are gone and the variants hold the payload directly: `ReadyEvent`, `GuildEvent`, `ChannelEvent`, `MessageEvent`, `InteractionEvent`, `VoiceStateEvent`, `VoiceServerEvent`, `UserUpdateEvent`, `EntitlementEvent`, `SubscriptionEvent`, `SoundboardSoundEvent`, `StageInstanceEvent`, `ResumedEvent`, and the `{ data, raw }` wrappers `GuildDeleteEvent`, `MemberRemoveEvent`, `GuildMembersChunkEvent`, `RoleDeleteEvent`, `MessageDeleteEvent`. No payload struct has a `raw` field any more; use `event.raw()` with `keep_raw_events(true)`.
- `GUILD_CREATE` is a `GuildCreatePayload` (an available `GuildCreate` or an `UnavailableGuild`), not a `Guild`; `MESSAGE_UPDATE` is a `MessageUpdate`, not a `Message`.
- Checkpoint C (partial updates and shared payloads): GUILD_MEMBER_UPDATE is a `MemberUpdate` and GUILD_UPDATE a `GuildUpdate` (typed patches: an omitted field keeps the cached value, `null` clears); PRESENCE_UPDATE is a `PresenceUpdateEvent` around a shared `Arc<Presence>` (read `status()`, `activities()`, ...). The cache merges these patches into what it holds and **never stores a member or guild it does not already hold** from an update alone (2.x stored the decoded update as a whole member or guild); members and guilds enter the cache through GUILD_CREATE, GUILD_MEMBER_ADD, GUILD_MEMBERS_CHUNK, and `fetch_members`. `Member::user` and `Message::author` are `Option<Arc<User>>`, CHANNEL_CREATE/UPDATE carry `Arc<Channel>`, and the `member_update`, `guild_update`, `channel_create`, and `channel_update` hooks take the new payloads.
- A payload that fails to decode is `EventKind::DecodeFailed` (and goes to the `decode_failed` hook), no longer `Unknown`.
- `decode_event(name, data: &str)` decodes JSON text into an `Event` (keeping the raw payload) for tests and tooling; the runtime uses `Event::decode(metadata, raw, keep_raw)`.

How the pipeline moves an event (phase 4): the read loop parses only a shallow envelope (`op`, `s`, `t`, and `d` as a borrowed `RawValue` — no `serde_json::Value` tree) and copies `d` once into the shard's queue; the processor decodes the typed payload straight from that text, updates the cache with the payload's own `Arc`s, publishes the `Arc<Event>` to the collector hub, and hands the same `Arc` to the handler.

## Intents

`gateway_intents` exposes named constants for every documented intent bit, including the poll intents added in `2.1`:

- `gateway_intents::GUILD_MESSAGE_POLLS` (`1 << 24`) and `gateway_intents::DIRECT_MESSAGE_POLLS` (`1 << 25`) enable the `MESSAGE_POLL_VOTE_*` events; both are included in `gateway_intents::NON_PRIVILEGED`.
- `gateway_intents::GUILD_EXPRESSIONS` is the current Discord name for bit 3 (the older `GUILD_EMOJIS_AND_STICKERS` constant remains).

## Initial Presence and Dispatch Mode

`ClientBuilder::presence(...)` delivers an initial presence inside IDENTIFY, so the bot connects with the desired status and activity instead of updating it after READY. Its activities are `BotActivity` values (`name`, `type`, `state`, `url`, the fields Discord lets bots set; `BotActivity::custom("text")` sets a custom status), not the received `Activity`. `ClientBuilder::event_dispatch(...)` chooses how handler calls are scheduled:

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

The processor publishes an event to the collector hub before queueing its handler call, so by the time a handler runs, later events may already have been published. `Context::message_collector()`, `message_update_collector()`, `component_collector()`, `interaction_collector()`, and `modal_collector()` start at the event the handler is handling (its `Context::event_cursor()`) and replay what was published since, then continue live without a gap. The hub keeps the last 256 events; older ones are skipped and counted in the collector's `lagged_events()`. `ctx.collectors().message_collector()` and friends still start "from now".

### Shutdown and Errors (3.0)

`ClientBuilder::start()` returns once the shard's gateway connection has ended plus at most `handler_shutdown_timeout` (default 5 seconds), plus up to a second to confirm cancellation and for the overload hook thread to return. The window starts when the gateway exits, not when shutdown is requested: queued events keep being delivered and running handlers may finish within it; then the remaining handler tasks are aborted **and joined**, and an external cache backend's queued writes are flushed within the same bound (its worker is joined too, so no write from this client lands after `start()` returned). If a task cannot confirm cancellation within one second of being aborted (for example a handler blocking its thread), the call returns an error saying so instead of claiming the shard stopped.

The return value is the gateway's result, so a non-reconnectable close code such as 4004 (authentication failed) or 4014 (disallowed intents), or an event queue overflow, reaches the caller as `DiscordError::Gateway`. Cancelling the `start()` future also stops the shard's processor and handler tasks. See [Sharding](sharding.md) for the multi-shard equivalent.

## Default Allowed Mentions and Cache Backend (2.2.0)

`ClientBuilder` gains two configuration hooks in `2.2.0` (since 3.0 `cache_backend` takes `Arc<B>` of a concrete backend type, see above):

```rust
use std::sync::Arc;
use discordrs::cache::CacheBackend;
use discordrs::{AllowedMentions, Client};

fn configure<B: CacheBackend + 'static>(token: &str, intents: u64, backend: Arc<B>) {
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

Since 3.0 the backend receives writes in gateway order and must implement an epoch contract: one store-wide epoch, compare-and-set clears (`clear_cache` / `clear_shard_cache` take the expected epoch and return `CacheClear::Applied` or `Rejected`), and writes that carry their epoch, so a previous client's late write or clear is rejected and a new shard layout invalidates every old writer; clears carry a `CacheClearId` so a retried clear whose response was lost is recognized as committed. Only one running client may mirror into a store — enforce this in your deployment: it is not detected at startup, and an older client sharing the store only notices (reporting `superseded`) at its next clear, after its writes were already being ignored. A dropped, failed, or panicking write discards that shard's earlier queued writes and clears the shard instead of leaving the mirror stale; `Context::cache_backend_health()` reports the mirror's state; shutdown reports shards left unrecovered; and partial MESSAGE_UPDATE payloads are merged instead of stored as complete messages. See the External Cache Backend section of [Cache and Collectors](cache-and-collectors.md).

The same allowed-mentions default exists on the REST side via `RestClient::builder().default_allowed_mentions(...)`, and `Context::default_allowed_mentions()` exposes the configured value inside handlers.

## Respond to Interactions from Gateway Events (2.2.0)

Import `discordrs::response::InteractionResponder` and reply to `EventKind::InteractionCreate` payloads directly, discord.js-style:

```rust
use discordrs::response::InteractionResponder;
use discordrs::{Context, Event, EventKind, Interaction};

async fn on_event(ctx: Context, event: &Event) -> Result<(), discordrs::DiscordError> {
    if let EventKind::InteractionCreate(interaction) = event.kind() {
        if let Interaction::ChatInputCommand(command) = &**interaction {
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

Since checkpoint C it returns `Vec<Arc<Member>>`: GUILD_MEMBERS_CHUNK decodes `members` (and `presences`, with `user_id` filled in) straight into `Arc`s that the event, the cache, the external backend, and `fetch_members` share, so collecting a large guild copies no member. Since 3.0, `fetch_members` is safe to await inside any handler (see Event Pipeline above), returns only once every chunk index `0..chunk_count` arrived (a missing chunk is a timeout error naming how many arrived, never a silently incomplete list), removes its chunk waiter when the call returns, times out, or is cancelled, and fails immediately if the shard stops. Guild-scoped gateway commands — `request_guild_members`, `fetch_members`, `update_voice_state`, `join_voice`, `leave_voice` — are sent on the shard that owns the guild (`(guild_id >> 22) % shard_count`) when that shard runs in the same process, so any shard's `Context` can use them.

If every chunk arrives but some members in them did not decode (a shape this version cannot read; the chunk keeps the members that did and logs the rest), `fetch_members` returns `Err(DiscordError::IncompleteMemberFetch(partial))` instead of a shorter list as if it were complete. `partial.members` holds every member that decoded (they are cached as usual) and `partial.dropped` how many were left out, so a caller that can work with a partial list takes `partial.members`:

```rust
use discordrs::DiscordError;

let members = match ctx.fetch_members(guild_id, None, None).await {
    Ok(members) => members,
    Err(DiscordError::IncompleteMemberFetch(partial)) => {
        eprintln!("{} members could not be decoded", partial.dropped);
        partial.members
    }
    Err(error) => return Err(error),
};
```

Each `GuildMembersChunkPayload` reports its own count in `dropped_members` (0 unless the chunk needed the element-by-element fallback; not serialized).

## Event Surface

- `EventKind` has a typed variant for every documented receive dispatch (READY, guild/channel/member/role flows, messages, reactions, interactions, voice, threads, scheduled events, soundboard, entitlements, auto moderation, `CHANNEL_INFO`, `RATE_LIMITED`, ...), plus `Unknown` and `DecodeFailed`.
- Fields that the 2.x decoder read leniently stay lenient: a malformed optional field (for example an invalid `guild_id` on TYPING_START) reads as `None` instead of failing the whole event.
- Reaction dispatches preserve Discord's current metadata fields, including `member`, `message_author_id`, `burst`, `burst_colors`, and `reaction_type`.
- Presence dispatches expose the documented partial user ID, activities, and `ClientStatus` platform status metadata.
- Override `handle_event` for one entry point, or the typed hooks (`ready`, `message_create`, `interaction_create`, ..., `raw_event`, `decode_failed`) for per-event methods.

## Gateway Control Helpers

`Context` and `ShardMessenger` expose typed control paths for common outbound Gateway commands:

- presence updates
- guild member requests
- soundboard sound requests
- voice state updates
- channel info requests through `request_channel_info(RequestChannelInfo::voice_metadata(guild_id))`

`RequestChannelInfo` serializes to Discord Gateway opcode 43 and currently exposes the documented `status` and `voice_start_time` fields. Incoming `CHANNEL_INFO` dispatches decode to `EventKind::ChannelInfo`.

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
