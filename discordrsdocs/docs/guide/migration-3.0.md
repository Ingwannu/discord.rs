# Migrating from 2.x to 3.0

This guide takes a bot written against discord.rs **2.0.2** (the last release on crates.io) or **2.2.0** (the unpublished 2.x line that was on `main`) to 3.0. Where the two 2.x versions differ, the guide says so; "2.x" means both. It covers the breaking changes listed under "3.0.0" in the CHANGELOG, grouped by area; long lists (every REST rename, every discriminant type, every retyped model field) link to the complete tables on the API pages.

<!--
Every `rust` block in this guide is checked by tests/migration_guide.rs:
blocks after `<!- - snippet: NAME - ->` must equal the region NAME in that
file, which CI compiles (and, where no Discord connection is needed, runs);
`<!- - snippet: 2.x - ->` blocks show 2.x code and are not compiled. The
Korean guide (ko/guide/migration-3.0.md) shows the same code blocks.
-->

## Why 3.0

2.1.0 and 2.2.0 were never published and already broke 2.0.2's public types, so 3.0 collects every breaking change once instead of forcing a 4.0 right after. It fixes runtime bugs that 2.x could not fix compatibly (a lost REST response could post a message twice, a handler awaiting `fetch_members` deadlocked its shard in Serial mode, every READY wiped the other shards' cache, voice packets were encrypted with the wrong nonce layout) and rebuilds the types on numeric IDs, open discriminants, shared events, and typed request bodies. The effect is measured with `tests/perf_report` (deterministic allocation counts; method in `docs/design/perf-baseline.md`, final figures in the phase 4 records of `docs/design/3.0-types-events-requests.md`): decoding a 1,000-member GUILD_CREATE takes **17,744 allocations instead of 508,698** (peak heap 34.7 → 3.2 MB), a MESSAGE_CREATE **20 instead of 573**, an INTERACTION_CREATE 67 instead of 1,519; the live heap after 25 large guilds is **64 MB instead of 386 MB**; an `Event` is 128 bytes instead of 4,848; and encoding an outgoing Components V2 message takes **26 allocations instead of 313** (an interaction reply 27 instead of 406).

## Before you start

- **Rust 1.88** or newer. 3.0 declares `rust-version = "1.88"`, and CI builds and tests every feature on that toolchain. (2.x declared none.)
- **Features are unchanged**: `cache` (default), `gateway`, `collectors`, `sharding`, `zstd-stream`, `voice`, `voice-encode`, `dave`, `interactions`.

```toml
[dependencies]
discordrs = { version = "3", features = ["gateway", "collectors"] }
```

- **Dependencies.** discord.rs no longer uses `async-trait` (drop it from your `Cargo.toml` unless you use it elsewhere) or `hex` (`From<hex::FromHexError> for DiscordError` is gone). The `interactions` feature builds on **axum 0.8**: the `Router` it returns is an axum 0.8 router, so serve it with axum 0.8.
- **Order of work.** Delete every `#[async_trait]` on a discord.rs trait impl first (section 8, "No more async-trait"); a leftover attribute fails with `E0195` and hides the other errors. Then fix compile errors section by section, in the order below. Then work through the deprecation warnings: every deprecated item names its replacement and is removed in the next major release. Finally, read [Behaviour changes that still compile](#behaviour-changes-that-still-compile).

## 1. Typed IDs

**Why:** the 2.x `Snowflake` was a `String` plus a parsed number, accepted any text, and let a channel ID go where a guild ID belonged. 3.0 IDs are `Copy`, 8 bytes, never zero, checked once at the boundary, and typed per resource (`GuildId`, `ChannelId`, `UserId`, `RoleId`, `MessageId`, `ApplicationId`, …), so a mix-up is a compile error. `Message` shrank from 6,360 to 4,808 bytes.

<!-- snippet: 2.x -->
```rust
// 2.x
use discordrs::Snowflake;

let guild_id = Snowflake::from("81384788765712384");
let channel_id: Snowflake = 41771983423143937u64.into();
let channel = rest.get_channel(channel_id.clone()).await?;
let member = cache.member(&guild_id, &user_id).await;
println!("{}", user_id.as_str());
```

<!-- snippet: ids -->
```rust
use discordrs::{ChannelId, DiscordError, GuildId, RoleId, UserId};

// Untrusted text (config, a string command option) is parsed once, at the
// boundary. "0", " 1", "+1", and "1.0" are rejected.
fn parse_ids(guild: &str, channel: u64) -> Result<(GuildId, ChannelId), DiscordError> {
    let guild_id: GuildId = guild.parse()?;
    let channel_id = ChannelId::try_from(channel)?;
    Ok((guild_id, channel_id))
}

// IDs are `Copy` and 8 bytes: pass them by value, no `.clone()`.
fn describe(guild_id: GuildId, user_id: UserId) -> String {
    let everyone: RoleId = guild_id.everyone_role();
    format!(
        "user {user_id} (raw {}) in guild {guild_id}, @everyone = {everyone}",
        user_id.get()
    )
}
```

Fields whose ID kind depends on a sibling field are enums that read and write the same JSON as 2.x:

<!-- snippet: polymorphic_ids -->
```rust
use discordrs::model::{OverwriteTarget, PermissionOverwrite};
use discordrs::DiscordError;

fn overwrite_owner(json: &str) -> Result<String, DiscordError> {
    let overwrite: PermissionOverwrite = serde_json::from_str(json)?;
    Ok(match overwrite.target {
        OverwriteTarget::Role(role_id) => format!("role {role_id}"),
        OverwriteTarget::Member(user_id) => format!("member {user_id}"),
        OverwriteTarget::Unknown { kind, id } => format!("type {kind:?} for {id}"),
    })
}
```

| 2.x | 3.0 |
|---|---|
| `Snowflake` (`String` + `u64`) | the typed alias (`GuildId`, `UserId`, …); `GenericId` only where Discord leaves the kind open. `Snowflake` is a deprecated alias of `GenericId` |
| `Snowflake::new`, `From<u64>`, `From<&str>`, infallible `FromStr` | `"…".parse()?`, `GuildId::try_from(u64)?`, `GuildId::try_new(u64)?` (`InvalidId`, converts into `DiscordError` with `?`) |
| `impl Into<Snowflake>` / `&Snowflake` parameters | the concrete ID by value: `rest.get_channel(channel_id)`, `cache.member(guild_id, user_id)` |
| `as_str()`, `as_u64()` / `to_u64()`, `timestamp() -> Option` | `to_string()` / `Display`, `get()`, `timestamp() -> u64` |
| `PermissionOverwrite { id, kind }` | `target: OverwriteTarget` (`Role`, `Member`, `Unknown`); overwrite endpoints take an `OverwriteTarget` |
| `ApplicationCommandPermission { id, kind }`, `u8` `ROLE`/`USER`/`CHANNEL` | `target: CommandPermissionTarget` (`Everyone`, `AllChannels` sentinels) |
| `CreateTestEntitlement { owner_id, owner_type }` | `CreateTestEntitlement::new(sku_id, EntitlementOwner::Guild(guild_id))` |
| `MessageSelectDefaultValue` | `SelectDefaultValue` enum (deprecated alias) |
| `RestClient::new(token, 0)` (`u64` application ID) | `RestClient::new(token, None)`; `Option<ApplicationId>` everywhere |
| `Default` on `Message`, `User`, `Guild`, `Channel`, … | removed (a default would need a fake ID); build test models from JSON, use the new constructors (`CreateDmChannel::new`, `LobbyMember::new`, …) |
| `DiscordModel::id() -> &Snowflake`, `id_opt() -> Option<&Snowflake>` | `type Marker`; `id() -> Id<Self::Marker>` by value (`User::id()` is a `UserId`), `id_opt() -> Option<Id<Self::Marker>>` |
| `CachedManager<T>` taking `impl Into<Snowflake>`, synchronous without the `gateway` feature | `CachedManager<T: DiscordModel>` taking `Id<T::Marker>`, async in every build (section 8) |
| `GuildApplicationCommandPermissions::id: Snowflake` (a command, or the application for app-wide permissions) | `CommandPermissionsId::{Command(CommandId), Application(ApplicationId)}` |
| `authorizing_integration_owners: HashMap<String, Snowflake>` | `AuthorizingIntegrationOwners { guild_install: Option<GuildInstallOwner>, user_install: Option<UserId>, other }`; `"0"` (a DM with the bot) is `GuildInstallOwner::BotDm` |
| builder `id(u32)` setters, `MessageComponent::id` (2.2.0) | `id(ComponentId)` (`ComponentId::new(12)`), `MessageComponent::id: Option<ComponentId>` |

Zero, signed, empty, or non-numeric IDs are decode errors now; values that look like IDs but are not snowflakes have their own types (`WidgetUserId`, `GuildInstallOwner`, `ComponentId`, `PollAnswerId`, `UploadId`, and `CreateGuildPlaceholderId` in the deprecated create-guild body). `edit_channel_permissions` rejects a body whose `type` contradicts the `OverwriteTarget` before sending. The formatters added in 3.0 follow the same rule: `MentionResolver` methods take `UserId` / `RoleId` / `ChannelId`, and a closure resolver is `Fn(MentionTarget) -> Option<String>`. The prelude exports `GuildId`, `ChannelId`, `UserId`, `RoleId`, `MessageId`, `ApplicationId`, and `InvalidId`; import the other aliases from the crate root. Full list: "Migrating from 2.x `Snowflake`" in [Models and Constants](../api/core-types-and-constants.md).

## 2. Open discriminants

**Why:** 2.x stored integer enumerations as `u8`/`u64` (or a closed enum that rejected new values), so `channel.kind == 0` compiled and a value Discord added later either failed or vanished. In 3.0 every one is an open newtype with named constants that keeps unknown values and round-trips them unchanged. One field stays a raw number on purpose: `MessageComponent::style` (`Option<u8>`), because its meaning depends on the component type; read it with `button_style()` / `text_input_style()`.

<!-- snippet: 2.x -->
```rust
// 2.x
use discordrs::model::ChannelType;
use discordrs::{button_style, ButtonBuilder};

if channel.kind == 0 || channel.kind == ChannelType::News as u8 { /* text */ }
let button = ButtonBuilder::new().style(button_style::PRIMARY);
let activity = Activity::default(); // silently PLAYING
```

<!-- snippet: discriminants -->
```rust
use discordrs::{Channel, ChannelType, MessageType};

fn describe(channel: &Channel) -> &'static str {
    // Constants work as patterns, but the set is open: keep a `_` arm.
    match channel.kind {
        ChannelType::GUILD_TEXT | ChannelType::GUILD_ANNOUNCEMENT => "text",
        kind if kind.is_thread() => "thread",
        kind if kind.is_voice_based() => "voice",
        _ => "other",
    }
}

fn raw_numbers() {
    let kind = ChannelType::new(0); // was `0u8` or `ChannelType::Text as u8`
    assert_eq!(kind, ChannelType::GUILD_TEXT);
    assert_eq!(kind.get(), 0);
    // A value this version does not know is kept, not rejected.
    let future = MessageType::new(250);
    assert_eq!(format!("{future:?}"), "Unknown(250)");
}
```

| 2.x | 3.0 |
|---|---|
| `kind: u8` fields, `channel.kind == 0` | `ChannelType::GUILD_TEXT`; `ChannelType::new(n)` / `kind.get()` for raw numbers |
| closed `ChannelType::{Text, Dm, Voice, News, …}` (`as u8`) | `ChannelType::{GUILD_TEXT, DM, GUILD_VOICE, GUILD_ANNOUNCEMENT, …}`, including the thread types, with `is_thread()` / `is_voice_based()` / `is_text_based()` |
| `ActivityType(0)`, `x.0`, `Default` | `ActivityType::PLAYING` / `ActivityType::new(0)`, `x.get()`, no `Default` (`Activity::new(kind, name)`) |
| `constants::{component_type, button_style, separator_spacing, text_input_style}`, `command::{command_type, option_type}` | `ComponentType`, `ButtonStyle`, `SeparatorSpacing`, `TextInputStyle`, `CommandType`, `CommandOptionType` |
| `helpers::INTERACTION_RESPONSE_*`, `bitfield::interaction_callback_type` | `InteractionResponseType` (`CHANNEL_MESSAGE` → `CHANNEL_MESSAGE_WITH_SOURCE`, `DEFERRED_CHANNEL_MESSAGE` → `DEFERRED_CHANNEL_MESSAGE_WITH_SOURCE`, `AUTOCOMPLETE_RESULT` → `APPLICATION_COMMAND_AUTOCOMPLETE_RESULT`) |
| `MessageComponent::BUTTON` and the other `u8` constants | `ComponentType::BUTTON` |
| `CommandOptionBuilder::new(u8, ..)`, `ButtonBuilder::style(u8)`, `channel_types(Vec<u8>)` | the newtype (`channel_types` takes any `IntoIterator<Item = ChannelType>`) |
| `ApplicationCommand::kind: u8`, `CommandDefinition::kind: u8` | `Option<CommandType>`: `None` means `CHAT_INPUT` (the builders always set it); `ApplicationCommand::command_type()` applies that default |
| `MessageComponent::style: Option<u64>` (2.2.0), `component.style == Some(1)` | stays raw as `Option<u8>`: `component.button_style() == Some(ButtonStyle::PRIMARY)`, `text_input_style()` |
| `Presence::status: Option<String>`, `UpdatePresence::status: String` | `Option<PresenceStatus>`, `PresenceStatus` (`PresenceStatus::ONLINE`, `PresenceStatus::new(..)`; compares with `&str`) |

A missing required discriminant (component `type`, activity `type`, poll `layout_type`, …) is now a decode error instead of `0`/`PLAYING`, and a value wider than the storage fails instead of being truncated. Types whose discriminant is required lost `Default` (`Activity`, `MessageComponent`, `Poll`, `ApplicationCommandOption`, `GuildMfaLevel`, `EditChannelPermission`, …); use `Activity::new`, `MessageComponent::new`, `ApplicationCommandOption::new`, `ApplicationRoleConnectionMetadata::new`, `GuildMfaLevel::new`, or `EditChannelPermission::for_target`. Builders whose constructor takes arguments (`SelectMenuBuilder`, `TextInputBuilder`, `CommandOptionBuilder`, the command builders) lost `Default` too. Full table: "Migrating from 2.x discriminants" in [Models and Constants](../api/core-types-and-constants.md).

## 3. Patch and Nullable

**Why:** an update can omit a field (keep it), send `null` (clear it), or send a value. 2.x read partial updates into full structs, so an omitted field looked like an empty one, and request bodies spelled the three cases as `Option<Option<T>>`. 3.0 names them: `Patch<T>` is `Missing` or `Value(T)`, `Nullable<T>` is `Null` or `Value(T)`, and `NullablePatch<T> = Patch<Nullable<T>>`. An absent required field is a decode error, never a silent `null`.

<!-- snippet: 2.x -->
```rust
// 2.x
// Request: hand-written JSON, or Option<Option<T>> fields.
rest.update_channel(channel_id, &json!({ "name": "announcements", "topic": null })).await?;
// Event: MESSAGE_UPDATE arrived as a whole `Message`; omitted fields read as
// empty, so `message.content == ""` could mean "not edited" or "cleared".
async fn message_update(&self, _ctx: Context, message: Message) { /* ... */ }
```

In request bodies, the setters write the patch for you:

<!-- snippet: patch_request -->
```rust
use discordrs::request::ModifyChannel;

fn rename_and_clear_topic() -> Result<String, serde_json::Error> {
    // Omitted fields are not sent; `clear_*` sends an explicit `null`.
    let body = ModifyChannel::new().name("announcements").clear_topic();
    serde_json::to_string(&body) // {"name":"announcements","topic":null}
}
```

In events, the partial updates are typed patches too: `MessageUpdate` (MESSAGE_UPDATE), `MemberUpdate` (GUILD_MEMBER_UPDATE), and `GuildUpdate` (GUILD_UPDATE) have a `Patch` for every field that can change, and each has `apply_to`:

<!-- snippet: patch_event -->
```rust
use discordrs::model::MessageUpdate;
use discordrs::{Message, Nullable, Patch};

fn apply_edit(message: &mut Message, update: &MessageUpdate) {
    match &update.content {
        Patch::Missing => {} // the edit did not touch the content
        Patch::Value(content) => println!("new content: {content}"),
    }
    if let Patch::Value(Nullable::Null) = &update.edited_timestamp {
        println!("edited_timestamp was cleared");
    }
    // Applies every sent field; omitted ones keep their value.
    update.apply_to(message);
}
```

| 2.x | 3.0 |
|---|---|
| MESSAGE_UPDATE as `Message` | `model::MessageUpdate`; `update.apply_to(&mut message)` merges it, `author` is a `PartialUser` merged field by field |
| GUILD_MEMBER_UPDATE as `Member`, GUILD_UPDATE as `Guild` | `model::MemberUpdate` (full `user: Arc<User>`, the other fields as patches) and `model::GuildUpdate` (`roles: Patch<Vec<Arc<Role>>>`, `emojis`, `stickers`); both have `apply_to` |
| `update_channel(&json!({..}))` | `request::ModifyChannel` (setters; `clear_*` sends `null`) |
| create bodies | `Option<T>`: `None` omits the key |
| modify bodies in `discordrs::request` (`ModifyChannel`, `EditMessage`, `ModifyAutoModerationRule`, `ModifyGuildScheduledEvent`, …) | `Patch` / `NullablePatch` |
| modify bodies still in `discordrs::model` (`ModifyGuild`, `ModifyGuildMember`, …) | unchanged for now: `None` omits, `Some(None)` sends `null` |

Declaring your own patch field takes `#[serde(default, skip_serializing_if = "Patch::is_missing")]`; leaving out `default` makes an omitted key an error rather than a silent "clear".

## 4. Events and handlers

**Why:** 2.x parsed every dispatch into a `serde_json::Value`, copied it three times, and kept the raw tree on every event. 3.0 decodes each dispatch straight from the frame text, once, into an `Event` shared as `Arc<Event>` by the cache, the collectors, and your handler, and keeps the raw JSON only when you ask for it.

<!-- snippet: 2.x -->
```rust
// 2.x
use async_trait::async_trait;
use discordrs::{Context, Event, EventHandler};

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn handle_event(&self, ctx: Context, event: Event) {
        match event {
            Event::Ready(ready) => println!("ready as {}", ready.data.user.username),
            Event::MessageCreate(event) => println!("{}", event.message.content),
            _ => {}
        }
    }
}
```

`Event` is now an envelope: match on `event.kind()` (`EventKind`), and read the dispatch name, shard, sequence, and receive time from `event.metadata()`:

<!-- snippet: events_handle_event -->
```rust
use std::sync::Arc;

use discordrs::{Context, Event, EventHandler, EventKind};

struct Router;

impl EventHandler for Router {
    async fn handle_event(&self, _ctx: Context, event: Arc<Event>) {
        let (shard_id, _shard_count) = event.metadata().shard();
        match event.kind() {
            EventKind::Ready(ready) => {
                println!("shard {shard_id} ready as {}", ready.user.username);
            }
            // The `Arc<Message>` the cache and collectors share: clone the
            // `Arc` (not the message) to keep it beyond this call.
            EventKind::MessageCreate(message) => println!("{}", message.content),
            EventKind::Unknown => println!("dispatch without a type: {}", event.name()),
            EventKind::DecodeFailed(error) => eprintln!("{}: {error}", event.name()),
            _ => {}
        }
    }
}
```

The typed hooks receive the payload the way the event stores it, so no hook call copies it: an `Arc` you may keep, or a reference that lives for the call.

<!-- snippet: events_hooks -->
```rust
use std::sync::Arc;

use discordrs::model::{GuildUpdate, MemberUpdate, MessageUpdate};
use discordrs::{
    gateway_intents, Channel, Client, Context, DiscordError, EventDecodeError, EventHandler,
    GuildId, Member, Message, Nullable, Patch, PresenceUpdateEvent,
};
use serde_json::value::RawValue;

struct Hooks;

impl EventHandler for Hooks {
    async fn message_create(&self, _ctx: Context, message: Arc<Message>) {
        println!("{}: {}", message.channel_id, message.content);
    }

    // A partial message: fields Discord did not send are `Patch::Missing`.
    async fn message_update(&self, _ctx: Context, update: Arc<MessageUpdate>) {
        println!("message {} in {} edited", update.id, update.channel_id);
    }

    async fn member_add(&self, _ctx: Context, guild_id: GuildId, member: Arc<Member>) {
        // `member.user` is an `Option<Arc<User>>` shared with the cache.
        if let Some(user) = &member.user {
            println!("{} joined {guild_id}", user.username);
        }
    }

    // GUILD_MEMBER_UPDATE is a partial member: the full `user`, and the
    // other fields as patches.
    async fn member_update(&self, _ctx: Context, update: Arc<MemberUpdate>) {
        match &update.nick {
            Patch::Value(Nullable::Value(nick)) => {
                println!("{} is now {nick}", update.user_id())
            }
            Patch::Value(Nullable::Null) => println!("{} cleared their nick", update.user_id()),
            Patch::Missing => {}
        }
    }

    // GUILD_UPDATE is a partial guild too; GUILD_CREATE-only fields such as
    // `member_count` are never in it.
    async fn guild_update(&self, _ctx: Context, update: &GuildUpdate) {
        if let Patch::Value(name) = &update.name {
            println!("guild {} renamed to {name}", update.id);
        }
    }

    async fn channel_create(&self, _ctx: Context, channel: Arc<Channel>) {
        println!("new channel {}", channel.id);
    }

    // The presence is an `Arc<Presence>` shared with the cache.
    async fn presence_update(&self, _ctx: Context, event: &PresenceUpdateEvent) {
        println!("{:?} is {:?}", event.user_id(), event.status());
    }

    // Dispatches this version has no type for: the name and the JSON text.
    async fn raw_event(&self, _ctx: Context, name: &str, data: &RawValue) {
        println!("{name}: {} bytes", data.get().len());
    }

    // New: a known dispatch whose payload did not match its type.
    async fn decode_failed(
        &self,
        _ctx: Context,
        name: &str,
        error: &EventDecodeError,
        _data: &RawValue,
    ) {
        eprintln!("could not decode {name}: {error}");
    }
}

async fn run(token: &str) -> Result<(), DiscordError> {
    Client::builder(
        token,
        gateway_intents::GUILDS | gateway_intents::GUILD_MESSAGES,
    )
    .event_handler(Hooks)
    // Keep every dispatch's JSON for `event.raw()` (default off; unknown
    // and undecodable dispatches always keep theirs).
    .keep_raw_events(true)
    .start()
    .await
}
```

| 2.x | 3.0 |
|---|---|
| `handle_event(ctx, Event)`, `match event { Event::Ready(..) }` | `handle_event(ctx, Arc<Event>)`, `match event.kind() { EventKind::Ready(..) }` |
| `event.kind()` (the name string) | `event.name()`; `event.metadata()` has `shard()`, `sequence()`, `received_at()` |
| wrapper structs (`ReadyEvent { data }`, `MessageEvent { message }`, `GuildEvent`, `InteractionEvent`, …) and their `raw` fields | the variants hold the payload directly; raw JSON is `event.raw()` |
| raw `Value` on every event | `event.raw()` only with `ClientBuilder::keep_raw_events(true)`; always kept for unknown and undecodable dispatches |
| `message_create(ctx, Message)` | `message_create(ctx, Arc<Message>)` |
| `message_update(ctx, Message)` | `message_update(ctx, Arc<MessageUpdate>)` (partial) |
| `member_add(ctx, Snowflake, Member)` | `member_add(ctx, GuildId, Arc<Member>)` |
| `member_update(ctx, Snowflake, Member)` | `member_update(ctx, Arc<MemberUpdate>)`: `guild_id`, the full `user`, the other fields as patches |
| `guild_create(ctx, Guild)` | `guild_create(ctx, &GuildCreatePayload)`: `Available(GuildCreate)` or `Unavailable(UnavailableGuild)`; `GuildCreate` holds `roles: Vec<Arc<Role>>`, `channels` / `threads` as `Vec<Arc<Channel>>`, `members: Vec<Arc<Member>>`, …; `to_guild()` rebuilds a 2.x-style `Guild` |
| `guild_update(ctx, Guild)` (fields GUILD_UPDATE does not carry, such as `member_count`, read as empty) | `guild_update(ctx, &GuildUpdate)` |
| `channel_create(ctx, Channel)`, `channel_update(..)` | `channel_create(ctx, Arc<Channel>)`, likewise `channel_update` (`EventKind::ChannelCreate(Arc<Channel>)`); `channel_delete` takes `&Channel` |
| `presence_update(ctx, PresenceUpdateEvent { user: Option<PresenceUpdateUser>, user_id, status, activities, .. })` | `presence_update(ctx, &PresenceUpdateEvent { guild_id, user: Option<PartialUser>, presence: Arc<Presence> })`: read `user_id()`, `status()`, `activities()`, `client_status()`; `PresenceUpdateUser` is removed |
| `guild_members_chunk(ctx, GuildMembersChunkEvent)` | `guild_members_chunk(ctx, &GuildMembersChunkPayload)` with `members: Vec<Arc<Member>>` and `presences: Option<Vec<Arc<Presence>>>`; `ctx.fetch_members(..)` returns `Vec<Arc<Member>>` |
| `ThreadEvent::thread: Channel`, `RoleEvent::role: Role` | `Arc<Channel>`, `Arc<Role>` (the hooks still take `&ThreadEvent`, `role_create(ctx, GuildId, &Role)`) |
| other hooks take the payload by value (`ready(ctx, ReadyPayload)`, `user_update(ctx, User)`, …) | by reference (`&ReadyPayload`, `&User`, `&Interaction`, …); clone what you keep |
| `raw_event(ctx, String, Value)` for unknown **and** undecodable dispatches | `raw_event(ctx, &str, &RawValue)` for unknown ones; failures go to the new `decode_failed(ctx, &str, &EventDecodeError, &RawValue)` and `EventKind::DecodeFailed` |
| `decode_event(name, Value)` | `decode_event(name, &str)` |

For tests, `decode_event` builds an `Event` from fixture text:

<!-- snippet: events_decode -->
```rust
use discordrs::{decode_event, DiscordError, EventKind};

// Feed a handler or a match from a fixture, without a gateway connection.
fn content_of(fixture: &str) -> Result<Option<String>, DiscordError> {
    let event = decode_event("MESSAGE_CREATE", fixture)?;
    Ok(match event.kind() {
        EventKind::MessageCreate(message) => Some(message.content.clone()),
        _ => None,
    })
}
```

The runtime also changed how handlers run (no code change needed, but read it): each shard applies events to the cache, collectors, and `fetch_members` on an internal processor that never waits for handlers, so a handler can await `ctx.fetch_members(...)` or a collector in Serial mode without deadlocking, and the cache a handler reads may already include later events. Queues are bounded (`event_queue_capacity`, default 10,000); interactions have their own lane with an admission deadline (`interaction_admission_timeout`, `on_interaction_overload`). See "Event Pipeline" in [Gateway](../api/gateway.md).

## 5. Collectors

**Why:** the 2.x collector journal held a full copy of every event (1.24 MB for an empty 256-slot hub; 305 of the 386 MB live heap after 25 large guilds), and the message collector returned MESSAGE_UPDATE payloads as if they were whole messages. 3.0 collectors read the shared `Arc<Event>`s (an empty hub is about 2 KB), copy only what they return, and keep message edits apart.

<!-- snippet: 2.x -->
```rust
// 2.x
let channel_id = message.channel_id.clone();
let mut answers = ctx
    .collectors()
    .message_collector()
    .filter(move |reply: &Message| reply.channel_id == channel_id)
    .timeout(Duration::from_secs(30));
// Also yielded edits (MESSAGE_UPDATE) as `Message`s.
if let Some(answer) = answers.next().await { /* ... */ }
```

<!-- snippet: collectors -->
```rust
use std::sync::Arc;
use std::time::Duration;

use discordrs::{Context, EventHandler, EventKind, Message};

struct Quiz;

impl EventHandler for Quiz {
    async fn message_create(&self, ctx: Context, message: Arc<Message>) {
        if message.content != "!quiz" {
            return;
        }
        let channel_id = message.channel_id;
        // Starts at the event being handled: messages sent while this
        // handler call was queued are replayed, not missed.
        let mut answers = ctx
            .message_collector()
            .filter(move |reply: &Message| reply.channel_id == channel_id)
            .timeout(Duration::from_secs(30));
        // `next()` copies the matching message out; `next_event()` hands
        // over the shared `Arc<Event>` without copying.
        if let Some(event) = answers.next_event().await {
            if let EventKind::MessageCreate(answer) = event.kind() {
                println!("first answer: {}", answer.content);
            }
        }
        // Edits are partial `MessageUpdate`s with their own collector.
        let mut edits = ctx
            .message_update_collector()
            .timeout(Duration::from_secs(30));
        if let Some(update) = edits.next().await {
            println!("message {} was edited", update.id);
        }
    }
}
```

| 2.x | 3.0 |
|---|---|
| `MessageCollector` yielded MESSAGE_CREATE and MESSAGE_UPDATE | MESSAGE_CREATE only; edits go to `MessageUpdateCollector` (`ctx.message_update_collector()`) |
| `next()` only | `next()` (copied item) or `next_event()` (`Arc<Event>`, no copy) |
| `EventSubscription::recv() -> Event` | `-> Arc<Event>` |
| `ctx.collectors().*_collector()` started now | still starts now; `ctx.*_collector()` (new) starts at the handled event and replays what arrived while the handler was queued |
| `CollectorEndReason` exhaustive (2.2.0) | `#[non_exhaustive]`, new `EmojiLimit`, `UserLimit`, `MessageDelete`, `ChannelDelete`, `ThreadDelete`, `GuildDelete`; add a `_` arm |

The journal is bounded and lossy: a collector more than `capacity` events behind skips ahead and counts the gap in `lagged_events()`. When several outcomes are ready, the order is now fixed (stop, timeout, idle, then events), so an expired window never yields a late event (a 2.2.0 bug). 3.0 also adds `ReactionCollector`. See [Cache and Collectors](../api/cache-and-collectors.md).

## 6. Cache

**Why:** 2.x cloned each member, message, presence, user, channel, and role out of the event into the cache, and stored GUILD_MEMBER_UPDATE and GUILD_UPDATE payloads as if they were whole members and guilds. In 3.0 the cache stores the event's own `Arc`s instead of copies (users included: `Member::user` and `Message::author` are `Option<Arc<User>>`), merges partial updates copy-on-write, so a handler holding the old `Arc` keeps a consistent snapshot, and never creates an entry from a partial update. Caching a 1,000-member GUILD_CREATE takes 670 allocations instead of 31,291.

<!-- snippet: 2.x -->
```rust
// 2.x
cache.upsert_message(message.clone()).await;
let cached: Option<Message> = cache.message(&channel_id, &message_id).await;
cache.upsert_guild(guild).await; // GUILD_CREATE, with every member inside
```

<!-- snippet: cache -->
```rust
use std::sync::Arc;

use discordrs::model::MessageUpdate;
use discordrs::{CacheHandle, Message};

async fn track_edit(cache: &CacheHandle, message: Arc<Message>, update: &MessageUpdate) {
    // Upserts take `impl Into<Arc<T>>`: an event's `Arc` is stored, not
    // copied. A plain `Message` still works.
    cache.upsert_message(Arc::clone(&message)).await;
    // MESSAGE_UPDATE is a patch merged into the cached copy. The merge is
    // copy-on-write: `message` keeps showing the content it had.
    if let Some(merged) = cache.apply_message_update(update).await {
        println!("{:?} -> {:?}", message.content, merged.content);
    }
    // Keys are typed IDs, passed by value.
    let cached = cache.message_arc(message.channel_id, message.id).await;
    assert!(cached.is_some());
}
```

A partial update changes only what the cache already holds:

<!-- snippet: cache_partial_updates -->
```rust
use discordrs::model::MemberUpdate;
use discordrs::CacheHandle;

async fn apply_member_change(cache: &CacheHandle, update: &MemberUpdate) {
    match cache.apply_member_update(update).await {
        // Merged copy-on-write into the cached member; the update's user is
        // stored too. Earlier `Arc<Member>`s keep the old snapshot.
        Some(member) => println!("{} now has nick {:?}", update.user_id(), member.nick),
        // A partial update never creates an entry (2.x stored it as a whole
        // member): members come from GUILD_CREATE, GUILD_MEMBER_ADD,
        // GUILD_MEMBERS_CHUNK, `fetch_members`, or `upsert_member`.
        None => println!("member {} is not cached", update.user_id()),
    }
}
```

| 2.x | 3.0 |
|---|---|
| `upsert_message(Message)`, `upsert_member`, `upsert_presence`, `upsert_guild`, `upsert_user(User)`, `upsert_channel(Channel)`, `upsert_role(guild, Role)` | take `impl Into<Arc<T>>`: pass the event's `Arc`, or a value as before |
| `Member::user: Option<User>`, `Message::author: Option<User>` | `Option<Arc<User>>` (same JSON), shared with the user store; construct with `Some(Arc::new(user))`, read through the `Arc` as before |
| owned getters only (`user`, `channel`, `member`, `message`, …) | still owned copies; `user_arc`, `channel_arc`, `member_arc`, `message_arc`, `members_arc`, … return the shared `Arc` |
| GUILD_CREATE via the `Guild` with its collections | `apply_guild_create(&GuildCreate)` (from the event, or `GuildCreate::from(guild)`) |
| GUILD_UPDATE stored the decoded `Guild`, clearing the GUILD_CREATE-only fields | `apply_guild_update(&GuildUpdate) -> Option<Arc<Guild>>` merges copy-on-write; omitted fields (`member_count`, `joined_at`, `large`, …) keep their value; `GuildUpdate::from(guild)` applies a REST `Guild` |
| GUILD_MEMBER_UPDATE stored as a whole member | `apply_member_update(&MemberUpdate) -> Option<Arc<Member>>` merges copy-on-write and stores the update's user |
| an update for an uncached member or guild created it | partial updates never create entries: members and guilds arrive with GUILD_CREATE, GUILD_MEMBER_ADD, GUILD_MEMBERS_CHUNK, `fetch_members`, or an upsert |
| `guild()` returned the roles of the last GUILD_CREATE / GUILD_UPDATE | `guild()` / `guilds()` fill `roles` from the role store, so GUILD_ROLE_* events show; a full snapshot (`upsert_guild`, GUILD_CREATE, GUILD_UPDATE `roles`) replaces that guild's roles |
| `apply_members_chunk` with owned members | `apply_members_chunk(guild_id, &[Arc<Member>], Option<&[Arc<Presence>]>)` |
| MESSAGE_UPDATE merged into the cached message by checking which keys its raw JSON had | the typed `MessageUpdate` patch, merged copy-on-write by `apply_message_update(&MessageUpdate) -> Option<Arc<Message>>`; the merge covers every message field (`components`, `poll`, `call`, `message_snapshots`, …), `null` clears and `[]` empties; an uncached message stays uncached; `apply_partial_user` patches the cached author field by field |
| `&Snowflake` keys, string hashing | typed keys by value: members and presences by `(GuildId, UserId)`, messages by `(ChannelId, MessageId)`, roles by `(GuildId, RoleId)` |
| every READY cleared the whole cache | READY clears only the shard's guilds (`CacheHandle::clear_shard`) |

A partial user (a MESSAGE_UPDATE `author`, a PRESENCE_UPDATE `user`) patches the cached user and creates one only when it carries a `username`; a patch to a shared user goes into a new snapshot, so members and messages keep the user they arrived with. See "Shared Entries and Copy-on-Write Updates" in [Cache and Collectors](../api/cache-and-collectors.md).

## 7. External cache backends

**Why:** 2.x forwarded each write to the backend on its own task, so a delayed put could land after a later delete and resurrect the entry, and a failed write left the mirror silently stale. In 3.0 one worker applies writes in gateway order, every write carries an **epoch**, clears are compare-and-set, and a dropped or failed write quarantines the shard and resyncs it.

<!-- snippet: 2.x -->
```rust
// 2.x
#[async_trait]
impl CacheBackend for MyBackend {
    async fn clear_cache(&self) -> Result<(), DiscordError> { /* ... */ }
    async fn put_message(&self, message: Message) -> Result<(), DiscordError> { /* ... */ }
    async fn get_message(
        &self,
        channel_id: &Snowflake,
        message_id: &Snowflake,
    ) -> Result<Option<Arc<Message>>, DiscordError> { /* ... */ }
    // ...
}
// 2.2.0 (2.0.2 had the trait but no `cache_backend`):
let backend: Arc<dyn CacheBackend> = Arc::new(MyBackend::new());
let builder = Client::builder(&token, intents).cache_backend(backend);
```

<!-- snippet: cache_backend -->
```rust
use std::sync::Arc;

use discordrs::cache::{CacheBackend, CacheClear, CacheClearId};
use discordrs::model::Presence;
use discordrs::{
    CacheHandle, ChannelId, DiscordError, GuildId, Member, Message, MessageId, UserId,
};

/// Logs every write and stores it in an inner `CacheHandle`, which
/// implements the epoch contract. A Redis backend makes the same calls
/// atomic in a script.
struct LoggingBackend {
    inner: CacheHandle,
}

// No `#[async_trait]`: plain `async fn`, whose futures must be `Send`.
impl CacheBackend for LoggingBackend {
    async fn clear_cache(
        &self,
        expected_epoch: u64,
        clear_id: CacheClearId,
    ) -> Result<CacheClear, DiscordError> {
        println!("clear all at epoch {expected_epoch}");
        self.inner.clear_cache(expected_epoch, clear_id).await
    }

    async fn clear_shard_cache(
        &self,
        expected_epoch: u64,
        clear_id: CacheClearId,
        shard_id: u32,
        shard_count: u32,
    ) -> Result<CacheClear, DiscordError> {
        println!("clear shard {shard_id}/{shard_count} at epoch {expected_epoch}");
        self.inner
            .clear_shard_cache(expected_epoch, clear_id, shard_id, shard_count)
            .await
    }

    async fn purge_expired_entries(&self) -> Result<(), DiscordError> {
        self.inner.purge_expired_entries().await
    }

    // Every write carries the epoch it was issued under; a write older than
    // the store's epoch must be ignored and still return `Ok(())`.
    async fn put_member(
        &self,
        epoch: u64,
        guild_id: GuildId,
        user_id: UserId,
        member: Arc<Member>,
    ) -> Result<(), DiscordError> {
        self.inner
            .put_member(epoch, guild_id, user_id, member)
            .await
    }

    async fn get_member(
        &self,
        guild_id: GuildId,
        user_id: UserId,
    ) -> Result<Option<Arc<Member>>, DiscordError> {
        self.inner.get_member(guild_id, user_id).await
    }

    async fn list_members(&self, guild_id: GuildId) -> Result<Vec<Arc<Member>>, DiscordError> {
        self.inner.list_members(guild_id).await
    }

    async fn delete_member(
        &self,
        epoch: u64,
        guild_id: GuildId,
        user_id: UserId,
    ) -> Result<(), DiscordError> {
        self.inner.delete_member(epoch, guild_id, user_id).await
    }

    async fn put_message(&self, epoch: u64, message: Arc<Message>) -> Result<(), DiscordError> {
        println!("store message {} at epoch {epoch}", message.id);
        self.inner.put_message(epoch, message).await
    }

    async fn get_message(
        &self,
        channel_id: ChannelId,
        message_id: MessageId,
    ) -> Result<Option<Arc<Message>>, DiscordError> {
        self.inner.get_message(channel_id, message_id).await
    }

    async fn list_messages(
        &self,
        channel_id: ChannelId,
    ) -> Result<Vec<Arc<Message>>, DiscordError> {
        self.inner.list_messages(channel_id).await
    }

    async fn delete_message(
        &self,
        epoch: u64,
        channel_id: ChannelId,
        message_id: MessageId,
    ) -> Result<(), DiscordError> {
        self.inner
            .delete_message(epoch, channel_id, message_id)
            .await
    }

    async fn put_presence(
        &self,
        epoch: u64,
        guild_id: GuildId,
        user_id: UserId,
        presence: Arc<Presence>,
    ) -> Result<(), DiscordError> {
        self.inner
            .put_presence(epoch, guild_id, user_id, presence)
            .await
    }

    async fn get_presence(
        &self,
        guild_id: GuildId,
        user_id: UserId,
    ) -> Result<Option<Arc<Presence>>, DiscordError> {
        self.inner.get_presence(guild_id, user_id).await
    }

    async fn list_presences(
        &self,
        guild_id: GuildId,
    ) -> Result<Vec<Arc<Presence>>, DiscordError> {
        self.inner.list_presences(guild_id).await
    }

    async fn delete_presence(
        &self,
        epoch: u64,
        guild_id: GuildId,
        user_id: UserId,
    ) -> Result<(), DiscordError> {
        self.inner.delete_presence(epoch, guild_id, user_id).await
    }
}
```

| 2.x | 3.0 |
|---|---|
| `clear_cache(&self)` | `clear_cache(expected_epoch, clear_id) -> CacheClear::{Applied, Rejected}`; new required `clear_shard_cache(expected_epoch, clear_id, shard_id, shard_count)` |
| `put_member(guild, user, Member)`, `put_message(Message)`, `put_presence(.., Presence)` | `put_*(epoch, .., Arc<T>)`: the event's snapshot, shared; ignore writes older than the store's epoch |
| `delete_*(&Snowflake, ..)` | `delete_*(epoch, ..)` with typed IDs by value |
| a partial MESSAGE_UPDATE went to `put_message` as if it were a full message (2.2.0); GUILD_MEMBER_UPDATE likewise to `put_member` | merged into your stored copy (`get_message` → merge → `put_message`, `get_member` → merge → `put_member`); skipped when neither your store nor the in-memory cache holds a full copy |
| `Arc<dyn CacheBackend>` | `Arc<B>` for a concrete `B: CacheBackend` (see section 8) |

The epoch contract is normative (one epoch per store, atomic compare-delete-increment, idempotent `CacheClearId` retries) and is spelled out under "External Cache Backend" in [Cache and Collectors](../api/cache-and-collectors.md). One client may write to a store at a time; `Context::cache_backend_health()` reports the mirror's state.

## 8. No more async-trait

**Why:** `async-trait` pulled a second major version of `syn` into every build. The public async traits (`EventHandler`, `CacheBackend`, `InteractionHandler`, `TypedInteractionHandler`, `CachedManager`) now declare `fn … -> impl Future<Output = T> + Send`, which you implement with a plain `async fn`; default features build without duplicate crates.

- **Delete `#[async_trait]`** and `use async_trait::async_trait;` from every impl of these traits. A leftover attribute fails with `E0195` ("lifetime parameters or bounds on method … do not match the trait declaration").
- **Futures must be `Send`.** Do not hold an `Rc`, a `RefCell` borrow, or a `std::sync::MutexGuard` across an `.await`; the compiler reports "future cannot be sent between threads safely" at your impl.
- **`CachedManager` is async in every build.** Without the `gateway` feature it used to be a synchronous trait; it is now the same `fn … -> impl Future` trait everywhere, so add `.await` to `get`, `cached`, `contains`, and `list_cached`, and pass the model's typed ID (section 1).
- **No trait objects.** The traits are not object safe. `ClientBuilder::event_handler(handler)` is unchanged; `cache_backend` takes `Arc<B>` for a concrete backend type:

<!-- snippet: no_dyn -->
```rust
use std::sync::Arc;

use discordrs::cache::CacheBackend;
use discordrs::{gateway_intents, Client, ClientBuilder};

// 2.x took `Arc<dyn CacheBackend>`. The 3.0 traits are not object safe:
// name the concrete backend, or be generic over it.
fn with_backend<B: CacheBackend + 'static>(token: &str, backend: Arc<B>) -> ClientBuilder {
    Client::builder(token, gateway_intents::GUILD_MESSAGES).cache_backend(backend)
}
```

## 9. REST

**Why:** 2.x had two methods for many routes (`get_guild_emojis` returning `Vec<Value>` next to `get_guild_emojis_typed`), took `&Value` or any `B: Serialize` as bodies, and re-serialized JSON on every retry. 3.0 has one method per route that takes the route's request type and returns its model, and encodes each body once into bytes that every retry reuses.

<!-- snippet: 2.x -->
```rust
// 2.x
use reqwest::Method;

let channel = rest.update_channel(channel_id.clone(), &json!({ "name": "general" })).await?;
let message = rest
    .create_message(channel_id, &CreateMessage { content: Some("hello".into()), ..Default::default() })
    .await?;
let emojis = rest.get_guild_emojis_typed(guild_id.clone()).await?;
let preview: Value = rest
    .request(Method::GET, &format!("/guilds/{guild_id}/preview"), None)
    .await?;
```

<!-- snippet: rest -->
```rust
use discordrs::request::{CreateMessage, ModifyChannel};
use discordrs::{ChannelId, DiscordError, GuildId, RawRequest, RestClient};
use serde_json::Value;

async fn rest_calls(
    rest: &RestClient,
    guild_id: GuildId,
    channel_id: ChannelId,
) -> Result<(), DiscordError> {
    // One method per route: a concrete request type in, a typed model out.
    let channel = rest
        .update_channel(channel_id, &ModifyChannel::new().name("general"))
        .await?;
    let message = rest
        .create_message(channel.id, &CreateMessage::new().content("hello"))
        .await?;
    let emojis = rest.get_guild_emojis(guild_id).await?; // was `get_guild_emojis_typed`
    println!("{} in {}; {} emojis", message.id, channel.id, emojis.len());
    // A route without a method: same transport, rate limits, and retries.
    let preview: Value = rest
        .request(RawRequest::get(format!("/guilds/{guild_id}/preview")))
        .await?;
    println!("{preview}");
    Ok(())
}
```

| 2.x | 3.0 | Note |
|---|---|---|
| `update_channel(id, &Value)`, `update_channel_typed` | `update_channel(id, &request::ModifyChannel)` | `_typed` removed |
| `create_message(id, &model::CreateMessage)` | `create_message(id, &request::CreateMessage)` | the 2.x struct (JSON `components`/`embeds`) is gone; `discordrs::CreateMessage` and the prelude name the new type, `model::CreateMessage` is a deprecated alias of it (build with `new()` and setters) |
| `model::ModifyGuild`, `model::CreateGuildRole`, `model::ModifyGuildMember`, … (49 REST bodies) | `request::ModifyGuild`, … (same name) | the same structs with public fields and `Default`, moved to `discordrs::request`; they no longer implement `Deserialize`. `discordrs::X` and the prelude keep working; `model::X` is a deprecated alias |
| `create_interaction_response_typed(id, token, &InteractionCallbackResponse)` | `create_interaction_response(id, token, &request::InteractionResponse)` | removed with `InteractionCallbackResponse` (`kind` + JSON `data`); a callback the enum does not model goes through `InteractionResponse::Raw(RawInteractionResponse)` |
| `ModifyStageInstance { privacy_level }` | `ModifyStageInstance { topic, privacy_level }` | `topic` added; a literal that lists every field needs `..Default::default()` |
| `update_message`, `update_message_with_files` | `edit_message(_with_files)(.., &request::EditMessage)` | removed |
| `get_guild_emojis() -> Vec<Value>`, `get_guild_emojis_typed` | `get_guild_emojis() -> Vec<Emoji>` | `_typed` is a deprecated forward |
| `create_dm_channel_typed`, `get_guild_audit_log_typed`, `modify_guild_widget`, `*_from_request`, `*_config` | the canonical name | 35 deprecated forwards |
| `update_guild`, `update_role`, `create_role` | `modify_guild`, `modify_guild_role`, `create_guild_role` | removed |
| `execute_webhook(&Value) -> Value` | `execute_webhook(&request::ExecuteWebhook) -> Message` | |
| `modify_guild_member(..) -> ()` | `-> Member` | |
| `request(Method, &str, Option<&Value>)`, `request_typed`, `request_multipart` | `request::<T>(RawRequest)`, `request_discard(RawRequest)` | same transport, rate limits, and token redaction |
| `edit_channel_permissions(channel, overwrite_id, &Value)` | `edit_channel_permissions(channel, OverwriteTarget, &EditChannelPermission)` | `_typed` is a deprecated forward |
| `available_tags: Vec<ForumTag>` (`CreateGuildChannel`, JSON for `update_channel`) | `Vec<request::AvailableForumTag>` | `AvailableForumTag::from(&tag)` keeps a tag, `AvailableForumTag::new("name")` creates one; tags left out are deleted |
| `send_container_message`, `send_to_channel`, `send_components_v2`, `edit_message_with_container` → `Value` | → `Message` | now apply the client's default `allowed_mentions` |

41 `_typed` pairs and 5 standalone `_typed` methods were folded into the canonical names; 35 one-to-one renames remain as deprecated forwards for this major version, and 36 2.x names were removed (35 untyped variants and the legacy `create_interaction_response_typed`). The complete table is "Migrating REST Names from 2.x" in [HTTP and Helpers](../api/http-and-helpers.md). Bodies Discord leaves open stay untyped on purpose: Slack/GitHub-compatible webhooks (`&Value`) and lobby moderation metadata.

## 10. Message and component builders

**Why:** 2.x builders returned `serde_json::Value`, so a field a component kind does not support (a URL on a custom-ID button, options on a role select) was silently dropped, and a section without its required accessory failed only at Discord. 3.0 builders build the typed request models in `discordrs::request`, with one builder per kind; invalid shapes do not compile. Encoding a Components V2 message takes 26 allocations instead of 313.

<!-- snippet: 2.x -->
```rust
// 2.x
let buttons = ActionRowBuilder::new()
    .add_button(ButtonBuilder::new().custom_id("ticket:open").label("Open").style(button_style::PRIMARY))
    .add_button(ButtonBuilder::new().url("https://example.com/help").label("Help"));
let header = SectionBuilder::new() // without an accessory it still compiled
    .add_text_display(TextDisplayBuilder::new("## Support"))
    .set_thumbnail_accessory(ThumbnailBuilder::new("https://example.com/logo.png"));
let container = ContainerBuilder::new()
    .add_section(header)
    .add_action_row(buttons)
    .add_action_row(ActionRowBuilder::new().add_select_menu(
        SelectMenuBuilder::string("ticket:topic").add_option(SelectOption::new("Billing", "billing")),
    ))
    .add_action_row(ActionRowBuilder::new().add_select_menu(
        SelectMenuBuilder::role("ticket:notify").default_role("701"),
    ));
// `components: Vec<Value>`
let message = MessageBuilder::new()
    .components_v2(ComponentsV2Message::new().add_container(container))
    .build();
```

<!-- snippet: builders -->
```rust
use discordrs::request::CreateMessage;
use discordrs::{
    ActionRowBuilder, ButtonStyle, ComponentsV2Message, ContainerBuilder, CustomButtonBuilder,
    DiscordError, LinkButtonBuilder, MessageBuilder, RoleId, SectionBuilder, SelectMenuBuilder,
    SelectOption, TextDisplayBuilder, ThumbnailBuilder,
};

fn support_panel(moderators: RoleId) -> Result<CreateMessage, DiscordError> {
    // One builder per button kind: a link button has no `custom_id` setter.
    let buttons = ActionRowBuilder::new()
        .add_button(
            CustomButtonBuilder::new("ticket:open")
                .label("Open")
                .style(ButtonStyle::PRIMARY),
        )
        .add_button(LinkButtonBuilder::new("https://example.com/help").label("Help"));
    // The required accessory is part of the type: without
    // `set_*_accessory`, `add_section` does not compile.
    let header = SectionBuilder::new()
        .add_text_display(TextDisplayBuilder::new("## Support"))
        .set_thumbnail_accessory(ThumbnailBuilder::new("https://example.com/logo.png"));
    let container = ContainerBuilder::new()
        .add_section(header)
        .add_action_row(buttons)
        // A select builder is its own row, with only its kind's setters.
        .add_action_row(
            SelectMenuBuilder::string("ticket:topic")
                .add_option(SelectOption::new("Billing", "billing")),
        )
        .add_action_row(SelectMenuBuilder::role("ticket:notify").default_role(moderators));
    // `build()` returns the typed `request::CreateMessage`, not a `Value`.
    MessageBuilder::new()
        .components_v2(ComponentsV2Message::new().add_container(container))
        .try_build()
}
```

| 2.x | 3.0 |
|---|---|
| `ButtonBuilder::new().custom_id(id)` / `.url(url)` | `CustomButtonBuilder::new(id)` / `LinkButtonBuilder::new(url)`; `PremiumButtonBuilder::new(sku_id)`. `ButtonBuilder` is deprecated |
| `ActionRowBuilder::new().add_select_menu(menu)` | pass the select builder where a row goes: `add_action_row(menu)` |
| `ActionRowBuilder::add_text_input`, `add_component(Value)` | removed: text inputs belong to modals; raw JSON is `request::RawComponent::from_value(&json)?` |
| one `SelectMenuBuilder` type | `SelectMenuBuilder::{string, user, role, mentionable, channel}` return per-kind builders with only their setters |
| `default_value(SelectDefaultValue::role("701"))` | `default_role(role_id)` (`default_user`, `default_channel`, `default_mentionable`) with typed IDs |
| `SectionBuilder` checked at `validate()` | the accessory is in the type: `SectionBuilder<SectionAccessory>` is the only one `add_section` accepts |
| `X::build() -> Value` | the typed model (`Container`, `request::Embed`, `Vec<Component>`, …); `serde_json::to_value(&x)` if you need JSON |
| `MessageBuilder::build() -> model::CreateMessage` | `-> request::CreateMessage`; `try_build()` validates, `build()` never does |
| `MessageBuilder::flags(u64)` | `flags(impl Into<MessageFlags>)` (`u64` still works) |

A `MessageBuilder` converts into `InteractionMessage` and `CreateFollowup`, and fallibly into `UpdateMessage` and `EditInteractionResponse`. See "Migrating from 2.x builders" in [Builders](../api/builders.md).

## 11. Interactions

**Why:** 2.x parsed interactions into a `Value` tree first, kept command option values as `Value`, answered the HTTP endpoint with an enum of `Value` payloads, and could not parse a modal containing a checkbox. 3.0 decodes interactions straight from text, types the values, and uses one response type for the HTTP endpoint, the REST callback, and `InteractionResponder`.

<!-- snippet: 2.x -->
```rust
// 2.x
let value: serde_json::Value = serde_json::from_str(body)?;
let Interaction::ChatInputCommand(command) = parse_interaction(&value)? else { return Ok(None) };
let amount = command.data.options.iter()
    .find(|option| option.name == "amount")
    .and_then(|option| option.value.as_ref()) // Option<serde_json::Value>
    .and_then(serde_json::Value::as_i64);
```

<!-- snippet: interactions_decode -->
```rust
use discordrs::{decode_interaction, DiscordError, Interaction};

// The HTTP endpoint and the gateway decode the same way: from text, with
// no `serde_json::Value` tree.
fn amount_option(body: &str) -> Result<Option<i64>, DiscordError> {
    let Interaction::ChatInputCommand(command) = decode_interaction(body)? else {
        return Ok(None);
    };
    Ok(command
        .data
        .options
        .iter()
        .find(|option| option.name == "amount")
        // `CommandOptionValue`: `as_str`, `as_i64`, `as_f64`, `as_bool`, `as_id`.
        .and_then(|option| option.value.as_ref()?.as_i64()))
}
```

The HTTP endpoint handlers return an `EndpointReply`: `Inline` (a `request::InteractionResponse` sent as the HTTP answer) or `Accepted`. Return `EndpointReply::Accepted` only after you responded over REST (for example with `InteractionResponder`): it makes the endpoint answer `202` with no body, and if no REST response went out, Discord shows the interaction as failed. Everything else gets an inline answer.

<!-- snippet: 2.x -->
```rust
// 2.x
#[async_trait]
impl TypedInteractionHandler for Tickets {
    async fn handle_typed(&self, _ctx: InteractionContextData, interaction: Interaction) -> InteractionResponse {
        match interaction {
            Interaction::ChatInputCommand(command) if command.data.name.as_deref() == Some("ticket") => {
                let modal = ModalBuilder::new("ticket:create", "New ticket")
                    .add_text_input(TextInputBuilder::new("subject", "Subject", text_input_style::SHORT));
                InteractionResponse::Modal(modal)
            }
            Interaction::ModalSubmit(_) => InteractionResponse::ChannelMessage(json!({
                "content": "Ticket created",
                "flags": 64,
            })),
            _ => InteractionResponse::DeferredMessage,
        }
    }
}
```

<!-- snippet: interactions_endpoint -->
```rust
use discordrs::request::InteractionMessage;
use discordrs::{
    EndpointReply, Interaction, InteractionContextData, InteractionResponder,
    InteractionResponse, ModalBuilder, RestClient, TextInputBuilder, TextInputStyle,
    TypedInteractionHandler,
};

#[derive(Clone)]
struct Tickets {
    rest: RestClient,
}

fn ephemeral(text: &str) -> EndpointReply {
    InteractionResponse::reply(InteractionMessage::new().content(text).ephemeral(true)).into()
}

impl TypedInteractionHandler for Tickets {
    async fn handle_typed(
        &self,
        _ctx: InteractionContextData,
        interaction: Interaction,
    ) -> EndpointReply {
        match interaction {
            Interaction::ChatInputCommand(command)
                if command.data.name.as_deref() == Some("ticket") =>
            {
                // Text inputs are wrapped in a Label (Discord's current layout).
                let modal = ModalBuilder::new("ticket:create", "New ticket").add_text_input(
                    TextInputBuilder::new("subject", "Subject", TextInputStyle::SHORT),
                );
                InteractionResponse::modal(modal).into()
            }
            Interaction::ChatInputCommand(command)
                if command.data.name.as_deref() == Some("status") =>
            {
                // `Accepted` (202, no body) only after responding over REST.
                match command.reply_ephemeral(&self.rest, "All systems go").await {
                    Ok(()) => EndpointReply::Accepted,
                    // The endpoint sends this only if no REST response may
                    // have reached Discord; otherwise it answers 202.
                    Err(_) => ephemeral("Status is unavailable right now"),
                }
            }
            Interaction::ModalSubmit(_) => ephemeral("Ticket created"),
            // Autocomplete must be answered with choices, not a message.
            Interaction::Autocomplete(_) => InteractionResponse::autocomplete([]).into(),
            // Everything else is answered inline too: `Accepted` without a
            // REST response would leave the interaction unanswered.
            _ => ephemeral("Unknown command"),
        }
    }
}
```

`AppFramework` routes and guards follow the same type:

<!-- snippet: 2.x -->
```rust
// 2.x
let app = AppFramework::builder()
    .guard(|ctx: &AppContext| -> Result<(), InteractionResponse> { /* ... */ Ok(()) })
    .command("hello", |_ctx| async move {
        InteractionResponse::ChannelMessage(json!({ "content": "Hello from discord.rs" }))
    })
    .build();
```

<!-- snippet: framework -->
```rust
use discordrs::request::InteractionMessage;
use discordrs::{AppContext, AppFramework, EndpointReply, InteractionResponse};

fn app() -> AppFramework {
    AppFramework::builder()
        // Guards return `Result<(), EndpointReply>` (2.x: `InteractionResponse`).
        .guard(|ctx: &AppContext| {
            if ctx.context.guild_id.is_some() {
                return Ok(());
            }
            let message = InteractionMessage::new()
                .content("Use this command in a server")
                .ephemeral(true);
            Err(EndpointReply::from(InteractionResponse::reply(message)))
        })
        // Routes return anything that converts into an `EndpointReply`.
        .command("hello", |_ctx: AppContext| async move {
            InteractionResponse::reply("Hello from discord.rs")
        })
        .build()
}
```

On the gateway, answer through `InteractionResponder` (2.2.0) instead of the free helper functions of 2.0.2; most of them are deprecated:

<!-- snippet: interactions_responder -->
```rust
use discordrs::{Context, EventHandler, Interaction, InteractionResponder};

struct Commands;

impl EventHandler for Commands {
    async fn interaction_create(&self, ctx: Context, interaction: &Interaction) {
        let Interaction::ChatInputCommand(command) = interaction else {
            return;
        };
        if command.data.name.as_deref() == Some("ping") {
            // Tracks the response state: a second reply fails locally,
            // follow-ups wait until Discord accepted the first response.
            if let Err(error) = command.reply_ephemeral(ctx.rest().as_ref(), "pong").await {
                eprintln!("could not answer /ping: {error}");
            }
        }
    }
}
```

Modal submissions keep Discord's value types:

<!-- snippet: modal_values -->
```rust
use discordrs::{decode_interaction, DiscordError, Interaction};

fn ticket_form(body: &str) -> Result<Option<(String, bool)>, DiscordError> {
    let Interaction::ModalSubmit(submit) = decode_interaction(body)? else {
        return Ok(None);
    };
    let form = &submit.submission;
    // Checkboxes are booleans now (2.x failed to parse them).
    let subject = form.get_text("subject").unwrap_or_default().to_owned();
    let urgent = form.get_checkbox("urgent").unwrap_or(false);
    Ok(Some((subject, urgent)))
}
```

Command option choices are typed, and `validate()` / `try_build()` check more:

<!-- snippet: 2.x -->
```rust
// 2.x
let amount = CommandOptionBuilder::new(option_type::INTEGER, "amount", "How much")
    .choice("ten", 10)
    .choice("most", 9_007_199_254_740_991_u64); // any `impl Serialize`
let command = SlashCommandBuilder::new("pay", "Send coins").option(amount).build();
```

<!-- snippet: command_choices -->
```rust
use discordrs::{
    CommandDefinition, CommandOptionBuilder, CommandOptionType, DiscordError,
    SlashCommandBuilder,
};

fn pay_command() -> Result<CommandDefinition, DiscordError> {
    let amount = CommandOptionBuilder::new(CommandOptionType::INTEGER, "amount", "How much")
        // `impl Into<CommandOptionValue>`: strings, i8-i64, u8-u32, floats.
        .choice("ten", 10)
        // Any other `Serialize` value (here a `u64`) goes through `try_choice`.
        .try_choice("most", 9_007_199_254_740_991_u64)?;
    // `try_build()` now also checks names (lowercase, no punctuation),
    // locale keys, and each choice against the option type and +/-2^53.
    SlashCommandBuilder::new("pay", "Send coins")
        .option(amount)
        .try_build()
}
```

| 2.x | 3.0 |
|---|---|
| `parse_interaction(&Value)` | `decode_interaction(&str)`; `parse_interaction(&Value)` still works (through text) |
| `Interaction` serialized as `{"ChatInputCommand": {..}}` | Discord's wire format |
| `CommandInteractionOption::value: Option<Value>` | `Option<CommandOptionValue>` (`as_str`, `as_i64`, `as_f64`, `as_bool`, `as_id`) |
| `ApplicationCommandOptionChoice::new(name, impl Serialize)`, `CommandOptionBuilder::choice(name, impl Serialize)` | `new` / `choice` take `impl Into<CommandOptionValue>` (strings, `i8`–`i64`, `u8`–`u32`, floats); `try_new` / `try_choice` for other values; no `Default` |
| `interactions::InteractionResponse::{ChannelMessage(Value), DeferredMessage, DeferredUpdate, Modal, ..}` | `request::InteractionResponse::{reply(..), defer(ephemeral), DeferredUpdateMessage, update(..), modal(..), autocomplete(..), Raw(..)}` |
| handler returns `InteractionResponse` | returns `EndpointReply` (`response.into()`, or `EndpointReply::Accepted` only after answering over REST) |
| `AppFramework` routes, `FrameworkFuture`, and guards (`Result<(), InteractionResponse>`) | routes return `impl Into<EndpointReply>`, `FrameworkFuture` resolves to `EndpointReply`, guards return `Result<(), EndpointReply>` |
| `interactions_endpoint`, `typed_interactions_endpoint` (panic on a bad key) | `try_interactions_endpoint`, `try_typed_interactions_endpoint` (return `Result`) |
| `defer_interaction`, `followup_message`, `edit_original_response`, and the other helpers duplicated by `InteractionResponder` (2.0.2) | `InteractionResponder`: `defer`, `follow_up`, `edit_reply`, … (11 helpers are deprecated, see section 15) |
| `respond_with_message`, `respond_to_interaction` | not deprecated: stateless helpers that now share the response state (deprecation is planned once the examples move off them); prefer `reply` / `reply_ephemeral` |
| `InteractionReplyData`, `From<CreateMessage>` | `request::InteractionMessage` (deprecated alias); `TryFrom<CreateMessage>` rejects channel-only fields |
| `ModalBuilder::add_text_input` → action row | wraps the input in a Label (Discord's current layout); the action-row form is only reachable through deprecated constructors |
| 2.0.2: `Message.components: Option<Vec<Value>>` (no `MessageComponent`) | the typed `MessageComponent` tree: `kind: ComponentType`, `custom_id`, `components`, `iter()`, `value: Option<MessageComponentValue>`, `values`, `default` |
| 2.2.0: `MessageComponent::value: Option<String>`, `kind: u8` | `value: Option<MessageComponentValue>` (`String` / `Bool` / `Other`; `value_str()` / `value_bool()`), new `values` and `default`; `kind: ComponentType` |
| `V2ModalComponent::RadioGroup { value: String }` | `value: Option<String>` (`null` when nothing was picked); checkboxes read Discord's `value` key; `V2ModalSubmission::get_checkbox` |
| `CheckboxBuilder::checked(bool)` | `default_checked(bool)`; `checked` remains as a deprecated alias |
| `disabled` on the radio-group, checkbox-group, and checkbox builders, `CheckboxBuilder::required` | removed: Discord rejects disabled components in modals, and a single checkbox cannot be required (use a one-option checkbox group with `required(true)`) |
| an Entry Point command was `Interaction::Unknown` | `Interaction::PrimaryEntryPoint` |

`Interaction::Unknown::raw_data` is `Option<Box<RawValue>>` and `kind` is always the interaction type; modal submissions and component interactions carry `resolved` data.

Also changed:

- **Command validation.** `validate()` / `try_build()` on slash and entry-point command builders reject names Discord rejects (punctuation, uppercase in any script, the ASCII apostrophe; use `ʼ` U+02BC) and unknown locale keys, and check every choice against its option type and range (`INTEGER`: −2^53+1 to 2^53−1; no booleans or non-finite numbers). `build()` still never validates.
- **Response state.** `is_acknowledged()`, `is_replied()`, and `is_deferred()` report only what Discord confirmed: a response that is in flight or whose outcome is uncertain is not "acknowledged" yet (`response_phase()` shows the full state). The callback body is built and checked before the response slot is claimed, so a local error leaves the interaction unacknowledged and an inline fallback still answers.
- **Stricter decoding.** The interaction `type` must be a JSON number and `data` an object; repeated keys are rejected.
- **Other model fields that held `serde_json::Value`** are typed: `AuditLogEntry::changes` (`AuditLogChange`, whose `new_value` / `old_value` are JSON text read with `new_value_as::<T>()` / `old_value_as::<T>()`) and `options` (`AuditLogEntryOptions`), `Application::team` (`Team`), `install_params` (`ApplicationInstallParams`), `integration_types_config` (keyed by `ApplicationIntegrationType`), `GuildOnboarding::prompts` (`OnboardingPrompt`), `Guild::guild_scheduled_events` (`GuildScheduledEvent`), the `Invite` partials (`guild`, `channel`, `target_application`, `guild_scheduled_event`), `UserConnection::integrations` (`PartialIntegration`), and `Embed::provider` (`EmbedProvider`). Fields whose shape Discord leaves open are `Box<RawValue>` (JSON text, read with `serde_json::from_str(raw.get())`): `Sku::manifest_labels` and `features`, and `GuildTemplate::serialized_source_guild`. The full table is "Interaction decoding and typed JSON values" in [Models and Constants](../api/core-types-and-constants.md).

See [Interactions Endpoint](../api/interactions-endpoint.md) and [Parsers](../api/parsers.md).

## 12. Permissions and bitfields

**Why:** 2.x had two permission types (`PermissionsBitField` and `BitField<PermissionFlags>`), an `Option<String>` in `CurrentUserGuild::permissions`, misnamed bits 41–52, and wrong message flag values. 3.0 has one `Permissions` type everywhere and Discord's documented bits and wire shapes.

<!-- snippet: 2.x -->
```rust
// 2.x
use discordrs::bitfield::permissions;
use discordrs::model::PermissionsBitField;

let mut perms = PermissionsBitField(permissions::VIEW_CHANNEL.bits() | permissions::SEND_MESSAGES.bits());
perms.insert(permissions::EMBED_LINKS.bits());
perms.remove(permissions::SEND_MESSAGES.bits()); // changed `perms` in place
```

<!-- snippet: permissions -->
```rust
use discordrs::bitfield::{message_flags, permissions};
use discordrs::{MessageFlags, Permissions};

fn permission_sets() -> Result<(), serde_json::Error> {
    let mut perms: Permissions = permissions::VIEW_CHANNEL | permissions::SEND_MESSAGES;
    perms |= permissions::EMBED_LINKS; // 2.x: `perms.insert(..)`
    perms = perms - permissions::SEND_MESSAGES; // `remove` returns the new set
    assert!(perms.contains(permissions::VIEW_CHANNEL.bits()));
    // Permissions are decimal strings on the wire; the other bitfields
    // (message, channel, and user flags, intents) are numbers.
    assert_eq!(serde_json::to_string(&perms)?, r#""17408""#);
    let flags: MessageFlags = message_flags::SUPPRESS_NOTIFICATIONS;
    assert_eq!(serde_json::to_string(&flags)?, "4096"); // 2.x sent bit 10
    Ok(())
}
```

| 2.x | 3.0 |
|---|---|
| `PermissionsBitField(bits)`, `.0`, `insert`, in-place `remove` | `Permissions::from_bits(bits)`, `.bits()`, `\|=`, `set = set - flag` (`add` / `remove` / `merge` / `missing` return a new set and are `#[must_use]`) |
| `CurrentUserGuild::permissions: Option<String>` | `Option<Permissions>` |
| message, channel, and user flags, intents serialized as strings | numbers (permissions stay strings); negative input is rejected |
| custom `BitFieldFlags` sets serialized as strings | numbers by default; `const STRING_ENCODED: bool = true` keeps strings (reading accepts both) |
| `message_flags::SUPPRESS_NOTIFICATIONS` = `1 << 10`, `IS_VOICE_MESSAGE` = `1 << 12` | `1 << 12` and `1 << 13`; `HAS_SNAPSHOT` (`1 << 14`) added |
| permission names shifted by one from bit 41, `SEND_POLLS` = bit 50 | Discord's table: `SEND_POLLS` = 49, `USE_EXTERNAL_APPS` = 50, `PIN_MESSAGES` = 51, `BYPASS_SLOWMODE` = 52 |

Your own `BitFieldFlags` sets are written as numbers now. If something else reads them as strings, keep 2.x's shape:

<!-- snippet: custom_bitfield -->
```rust
use discordrs::bitfield::{BitField, BitFieldFlags};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct FeatureFlags;

impl BitFieldFlags for FeatureFlags {
    const BITS: &'static [(u64, &'static str)] = &[(1 << 0, "BETA"), (1 << 1, "STAFF")];
    // 3.0 writes custom sets as numbers (2.x: strings); this keeps strings.
    const STRING_ENCODED: bool = true;
}

fn encode_features() -> Result<String, serde_json::Error> {
    serde_json::to_string(&BitField::<FeatureFlags>::from_bits(0b11)) // "\"3\""
}
```

3.0 also computes effective permissions (`discordrs::permissions`: `permissions_for`, `Member::guild_permissions`, cached variants); see [Permissions and Formatters](../api/permissions-and-formatters.md).

## 13. Voice

**Why:** 2.x voice could not talk to real Discord voice servers: both transport modes put the RTP packet counter at the wrong end of the AEAD nonce, so Discord rejected every packet the crate sent and no received packet decrypted (the tests used the same wrong layout). 3.0 follows the layout of @discordjs/voice and Songbird, strips RTP header extensions and padding correctly, and pins both directions with known-answer vectors. There is nothing to change in your code for this: voice simply works.

The playback pipeline (`AudioPlayer`, 2.2.0 only) now delivers audio in real time and reports what it could not deliver. A custom `VoiceFrameSink` receives a `VoiceFrameTiming` instead of a bare duration:

<!-- snippet: 2.x -->
```rust
// 2.2.0
impl VoiceFrameSink for RecordingSink {
    fn send_opus_packet<'a>(&'a self, opus_frame: &'a [u8], duration: Duration) -> VoiceSinkFuture<'a> {
        Box::pin(async move { /* ... */ Ok(()) })
    }
    fn update_speaking(&self, speaking: bool) -> Result<(), DiscordError> { Ok(()) }
}
```

<!-- snippet: voice_sink -->
```rust
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use discordrs::voice::player::{VoiceFrameSink, VoiceFrameTiming, VoiceSinkFuture};
use discordrs::DiscordError;

/// A test double that records how much audio was sent and skipped.
#[derive(Default)]
struct RecordingSink {
    sent_us: AtomicU64,
    skipped_us: AtomicU64,
}

fn micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

impl VoiceFrameSink for RecordingSink {
    fn send_opus_packet<'a>(
        &'a self,
        _opus_frame: &'a [u8],
        timing: VoiceFrameTiming,
    ) -> VoiceSinkFuture<'a> {
        Box::pin(async move {
            // `gap`: media never transmitted before this frame (dropped,
            // late, or cancelled). A real sink advances its RTP timestamp
            // past it.
            self.skipped_us
                .fetch_add(micros(timing.gap), Ordering::Relaxed);
            // Transmit only when the future completes: the player may drop
            // it (a send stalled for 60 ms, stop, pause, or replacement).
            self.sent_us
                .fetch_add(micros(timing.duration), Ordering::Relaxed);
            Ok(())
        })
    }

    fn update_speaking(&self, _speaking: bool) -> Result<(), DiscordError> {
        Ok(())
    }
}
```

| 2.2.0 | 3.0 |
|---|---|
| `send_opus_packet(frame, Duration)` | `send_opus_packet(frame, VoiceFrameTiming { duration, gap })`; transmit only on completion, and a dropped future must leave the sink usable |
| `AudioPlayer::play` without `voice-encode` advanced silently | returns `DiscordError::Voice` |
| subscribing an already-subscribed sink added a second feed | replaces the earlier subscription (handed over serially) |
| `Buffering` only at start | also while rebuffering and unpausing |
| no shutdown | `AudioPlayer::shutdown().await` |

A frame must start sending within 60 ms and a send stalled for 60 ms is cancelled; a full queue drops the oldest frame; skipped media becomes an RTP timestamp gap (`VoiceRuntimeHandle::send_opus_frame_after_gap`). See [Voice](../api/voice.md).

## 14. Sharding

**Why:** the 2.x supervisor kept every published event in an unbounded queue, rolled statuses back when draining, and awaited shards one by one, so a later shard's failure went unnoticed; in the multi-process manager (2.2.0), a child that stopped reading could hide shutdown messages, stray stdout corrupted the protocol, and children identified on their own schedule. 3.0 bounds, frames, and coordinates all of it.

Supervisor events are bounded (256 per shard); dropped ones are reported with a new variant, so exhaustive matches need an arm for it:

<!-- snippet: sharding_lagged -->
```rust
use discordrs::{DiscordError, ShardSupervisorEvent, ShardingManager};

fn report(manager: &ShardingManager) -> Result<(), DiscordError> {
    for event in manager.drain_events()? {
        match event {
            ShardSupervisorEvent::StateChanged { shard_id, state } => {
                println!("shard {shard_id}: {state:?}");
            }
            // New in 3.0: at most 256 events wait per shard; older ones are
            // dropped and counted here. `statuses()` is still current.
            ShardSupervisorEvent::Lagged { shard_id, missed } => {
                eprintln!("shard {shard_id}: {missed} events dropped");
            }
            other => println!("{other:?}"),
        }
    }
    Ok(())
}
```

Multi-process sharding (2.2.0 only) grants IDENTIFY from the parent:

<!-- snippet: 2.x -->
```rust
// 2.2.0
for &shard_id in child.shard_ids() {
    let client = Client::builder(&token, gateway_intents::GUILDS)
        .event_handler(Handler)
        .shard(shard_id, child.shard_count());
    tokio::spawn(async move { client.start().await });
}
child.serve(|_request| async { json!({ "ok": true }) });
child.notify_ready()?;
```

<!-- snippet: process_child -->
```rust
use discordrs::sharding::process::ShardChildProcess;
use discordrs::{
    gateway_intents, Client, Context, DiscordError, EventHandler, GatewayConnectionConfig,
    ReadyPayload,
};
use serde_json::json;

struct Handler {
    child: ShardChildProcess,
}

impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, _ready: &ReadyPayload) {
        // Per shard, from READY (2.2.0: one `notify_ready()` for all).
        let (shard_id, _) = ctx.shard_pair();
        if let Err(error) = self.child.notify_shard_ready(shard_id) {
            eprintln!("shard {shard_id}: {error}");
        }
    }
}

async fn run_child(child: ShardChildProcess, token: &str) -> Result<(), DiscordError> {
    // `serve` returns a `Result` now; start it before the shards connect.
    child.serve(|_request| async { json!({ "ok": true }) })?;
    // Every fresh IDENTIFY waits for the parent's grant for its bucket.
    let gateway = GatewayConnectionConfig::default().identify_gate(child.identify_gate());
    for &shard_id in child.shard_ids() {
        let client = Client::builder(token, gateway_intents::GUILDS)
            .gateway_config(gateway.clone())
            .event_handler(Handler {
                child: child.clone(),
            })
            .shard(shard_id, child.shard_count());
        tokio::spawn(async move {
            if let Err(error) = client.start().await {
                eprintln!("shard {shard_id}: {error}");
            }
        });
    }
    child.shutdown_signal().await;
    Ok(())
}
```

| 2.x | 3.0 |
|---|---|
| exhaustive `match` on `ShardSupervisorEvent` | add `ShardSupervisorEvent::Lagged { shard_id, missed }` |
| `ShardRuntimeChannels::event_tx`, `ShardRuntimePublisher::event_tx` | removed: send with `publish`, which also updates the status |
| `drain_events()` failed once any shard ended | drains ended shards too; fails only when nothing is left and every shard ended |
| IPC frames were bare JSON lines (2.2.0) | prefixed `@discordrs-ipc/1 ` and size-capped: a 2.2.0 parent cannot drive a 3.0 child, so deploy both together |
| children identified when they connected (2.2.0) | `IdentifyGate`: the parent grants each IDENTIFY per `shard_id % max_concurrency` bucket and enforces the daily session-start budget; wire `child.identify_gate()` into `GatewayConnectionConfig::identify_gate` |
| `ShardChildProcess::serve(..)` returned `()` | returns `Result` |
| `ParentMessage` / `ChildMessage` exhaustive | `#[non_exhaustive]` with identify variants |
| `spawn_stagger` default > 0 | 0 (permits pace IDENTIFY) |
| `wait()` / `shutdown()` | return errors for respawn exhaustion and unsuccessful exits; one deadline covers the whole shutdown |
| `ProcessShardManager::broadcast(payload)` sent payloads of any size | fails before sending when the encoded request exceeds `max_ipc_frame_bytes` (default 1 MiB); longer child output is discarded |

Terminal close codes (4004, 4010–4014) now make `start()` return `DiscordError::Gateway` instead of reconnecting forever, and the supervisor stops the other shards on the first fatal error. For a custom multi-process layout, implement `IdentifyGate` and pass it to `ClientBuilder::identify_gate`. See [Sharding](../api/sharding.md).

## 15. Code rules and deprecations

The crate now enforces its own code rules with lints (no `unwrap`/`panic!`/`as` casts in library code, `#[must_use]` results, `unsafe` forbidden); they change nothing in your code. What you will see are deprecation warnings, each naming its replacement. Everything deprecated in 3.0 is removed in the next major release:

| Deprecated | Use |
|---|---|
| `Snowflake`, `PermissionsBitField`, `MessageSelectDefaultValue` | `GenericId` / the typed alias, `Permissions`, `SelectDefaultValue` |
| `ButtonBuilder` | `CustomButtonBuilder`, `LinkButtonBuilder`, `PremiumButtonBuilder` |
| `InteractionReplyData`, `InteractionResponseBuilder`, `AutocompleteChoice` | `request::InteractionMessage`, `request::InteractionResponse`, `ApplicationCommandOptionChoice` |
| `InteractionResponse::PremiumRequired` | a premium button |
| 11 interaction helpers (`defer_interaction`, `followup_message`, `edit_original_response`, …) | `InteractionResponder` methods |
| `interactions_endpoint`, `typed_interactions_endpoint` | `try_interactions_endpoint`, `try_typed_interactions_endpoint` |
| `CheckboxBuilder::checked` | `default_checked` |
| `ModalBuilder::add_text_input_row`, `LegacyTextInputRow::new` (text inputs in action rows) | `ModalBuilder::add_text_input`, `Label::new(label, input)` |
| `batch_edit_application_command_permissions` | `edit_application_command_permissions` (Discord disabled batch edits) |
| `model::X` for the 49 request bodies that moved, `model::CreateMessage`, `model::CreatePoll` | `request::X` |
| 35 REST renames (`*_typed`, `*_from_request`, `*_config`, …) | the canonical method |
| `create_guild`, `delete_guild`, `create_guild_from_template`, `modify_guild_mfa_level`, `Guild::delete`, `Guild::set_mfa_level` | none: Discord removed guild creation by applications (2025-07-15) |

The aliases deprecated since 0.4 are removed in 3.0: `discordrs::Error` and `types::Error` (use `DiscordError`) and `error::BoxError` (use `Box<dyn std::error::Error + Send + Sync>`).

## Behaviour changes that still compile

These need no code change but can change what your bot observes:

- **REST retries.** 5xx responses, timeouts, and lost responses are retried only for requests that are safe to repeat (GET, HEAD, OPTIONS, PUT, DELETE, and Create Message with `enforce_nonce`), so a lost response no longer posts a message twice. Opt back in with `RestClientBuilder::retry_non_idempotent_requests(true)`. The managed client no longer follows redirects, and rate-limit buckets are scoped by channel/guild/webhook, so a 429 in one channel no longer stalls the others.
- **Tokens stay out of logs**: webhook and interaction tokens are redacted from errors, rate-limit info, and `Debug` output.
- **Gateway**: terminal close codes stop the shard with an error; READY clears only its own shard's cache; an overflowing event queue stops the shard (and invalidates its cache) instead of serving a gap; an unavailable GUILD_CREATE is `GuildCreatePayload::Unavailable`.
- **Interactions**: follow-ups and `edit_reply` wait until Discord accepted the initial response, a second initial response fails locally, and `InteractionResponse::defer(false)` sends `{"data":{"flags":0}}`. The HTTP endpoint answers 202 when the response went over REST.
- **Decoding** is stricter: invalid IDs, missing required discriminants, out-of-range values, and duplicate keys are errors; REST decode errors include the position. `Webhook::kind` reads Discord's `type` key.
- **Wire values** that were wrong in 2.x are now Discord's: message flags, permission bits 41–52, checkbox `default`, and non-permission bitfields as numbers.
- **Default `allowed_mentions`** also apply to `edit_webhook_message` and the Components V2 channel helpers.
- **Cache**: partial updates (MESSAGE_UPDATE, GUILD_MEMBER_UPDATE, GUILD_UPDATE) never create entries, so a member or guild seen only through updates is not cached; cached guild roles follow GUILD_ROLE_* events.
- **Collectors** never yield an event after their window expired.
- **New preview fields**: `display_name_styles` (`DisplayNameStyles` with `DisplayNameFont` / `DisplayNameEffect`) on `User` and `Member` follows the unreleased discord-api-docs PR #8178 and may change; its fields are optional, so a changed shape never fails the user or member. Reactions gained `me_burst` and `burst_colors`.
- **Voice** sends and receives with real Discord voice servers (2.x could not).

## Checklist

- [ ] Rust 1.88+, `discordrs = "3"`, axum 0.8 if you serve the interactions endpoint; drop `async-trait` if unused.
- [ ] Remove `#[async_trait]` from every `EventHandler`, `CacheBackend`, `InteractionHandler`, `TypedInteractionHandler`, and `CachedManager` impl.
- [ ] Replace `Snowflake` with typed IDs; parse untrusted IDs with `parse()?` / `try_from(..)?`; drop `.clone()` and `&` on IDs.
- [ ] Replace raw discriminant numbers and the `u8` constant modules with the newtypes and their constants; add `_` arms.
- [ ] Match on `event.kind()` in `handle_event`; update typed hook signatures (`Arc<Message>`, `Arc<MessageUpdate>`, references); move decode-failure handling to `decode_failed`.
- [ ] Set `keep_raw_events(true)` if you read `event.raw()` for known events.
- [ ] Handle MESSAGE_UPDATE, GUILD_MEMBER_UPDATE, and GUILD_UPDATE as partial `MessageUpdate` / `MemberUpdate` / `GuildUpdate`; use `MessageUpdateCollector` for edits; read presences through `PresenceUpdateEvent::status()` and friends.
- [ ] Pass event `Arc`s to cache upserts; use `apply_guild_create`, `apply_guild_update`, `apply_member_update`, and `apply_message_update`; don't rely on updates to populate the cache. `member.user` and `message.author` are `Option<Arc<User>>`.
- [ ] Rewrite external `CacheBackend`s for the epoch contract, `Arc` snapshots, and `clear_shard_cache`; pass `Arc<YourBackend>`.
- [ ] Replace `Value` bodies with `discordrs::request` types; rename `_typed` / removed REST methods; use `RawRequest` for unwrapped routes.
- [ ] Switch to `CustomButtonBuilder` / `LinkButtonBuilder` / `PremiumButtonBuilder` and per-kind select builders; give every section an accessory.
- [ ] Decode interactions with `decode_interaction`; return `EndpointReply`; reply through `InteractionResponder`; read typed option and modal values.
- [ ] Use `Permissions`; keep the result of `add` / `remove` / `merge`.
- [ ] Custom `VoiceFrameSink`s: take `VoiceFrameTiming` and honour cancellation.
- [ ] Add `ShardSupervisorEvent::Lagged` arms; for process sharding, deploy parent and children together and wire `identify_gate()`.
- [ ] Run `cargo build` (or `cargo clippy`) and replace every deprecated call it warns about.
- [ ] Review [Behaviour changes that still compile](#behaviour-changes-that-still-compile), especially REST retries if you relied on POST replays.
