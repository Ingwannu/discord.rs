# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased (3.0.0)

2.1.0 and 2.2.0 were never published to crates.io and already change public types relative to 2.0.2, so the next release is 3.0.0. The phased plan is in `ROADMAP.md`.

### Fixed
- **REST retries no longer duplicate mutations.** 5xx responses, timeouts, and failed response reads are retried only for requests that are safe to repeat: GET, HEAD, OPTIONS, PUT, and DELETE, plus `POST /channels/{id}/messages` with `nonce` + `enforce_nonce: true` (the only route Discord deduplicates), and that one only within 60 seconds of the first attempt, because Discord's nonce check covers just "the past few minutes". Previously a POST whose response was lost was sent again, which could post the same message twice. 429s are always retried; connection failures are retried for every method only on clients built by this crate (see below).
- **No redirects on the managed transport.** Clients from `RestClient::new` and `RestClientBuilder::build` no longer follow redirects, so a connect failure proves the request was never delivered. With `RestClientBuilder::use_client(...)` the redirect policy is unknown, so POST/PATCH are not retried after connect failures.
- **Rate-limit buckets respect major parameters.** A learned `x-ratelimit-bucket` hash is combined with the route's channel/guild/webhook/interaction-token scope before it blocks requests, so a 429 in one channel no longer stalls requests to other channels that report the same hash. Requests are serialized per resolved bucket, and a request that queued before its bucket was known moves to the bucket's gate instead of running beside it. A shorter concurrent observation no longer cuts an outstanding block short.
- **Tokens stay out of REST diagnostics.** Rate-limit route keys digest webhook and interaction tokens; transport errors and API/proxy error text are scrubbed of the exact request token; `HttpError` additionally redacts tokenized URL paths. `RateLimitInfo::route`, `DiscordError::RateLimit`, `DiscordError::Api`, `DiscordError::Http`, and REST log lines no longer contain tokens. (`Debug` output of interaction contexts and OAuth2 error bodies are scheduled for phase 3; see `ROADMAP.md`.)
- **Malformed wait headers cannot panic.** `inf`, `NaN`, negative, or huge `Retry-After`/`x-ratelimit-reset-after` values are clamped instead of reaching `Duration::from_secs_f64`.
- **Permission bits 41–52 match Discord's table.** The `PermissionFlags` name table was shifted by one from bit 41 upward, `permissions::SEND_POLLS` was bit 50 (really `USE_EXTERNAL_APPS`; `SEND_POLLS` is 49), and `PIN_MESSAGES` (51) and `BYPASS_SLOWMODE` (52) were missing. The table and constants now follow developers/topics/permissions.mdx, constants exist for every bit from 41 to 52, and the retired bit 47 (`USE_CLYDE_AI`) is no longer named.
- **Terminal gateway close codes stop the shard.** 4004 and 4010–4014 make `GatewayClient::run` return `DiscordError::Gateway` instead of reconnecting forever, including closes received before HELLO; 4007 and 4009 start a fresh session instead of a doomed RESUME. Per-connection heartbeat and outbound tasks are aborted on every exit path of a connection attempt, including cancellation during the shutdown flush. (Prompt shutdown of a whole multi-shard client is phase 2 work.)

- **Interaction responses track an in-flight phase.** Follow-ups, `edit_reply`, and `delete_reply` wait until Discord accepts the initial response instead of racing ahead of it, and a second initial response fails at once. A failed initial response rolls back only when Discord provably did not apply it (4xx, 429, local validation, connect failure); timeouts, 5xx, failed response reads, and cancelled futures leave `Uncertain`, which remembers every kind that may have landed. Retrying from there is safe: Discord's 40060 settles the state (to "acknowledged, kind unknown" when attempts of different kinds were uncertain), and a successful `@original` operation settles it too, even while a retry is in flight. A dropped future no longer leaves the slot claimed forever.
- **One owner per initial response across transports.** The typed HTTP interactions endpoint and REST responses share the initial-response slot; when the response goes over REST, the endpoint answers 202 with no body as Discord specifies instead of sending a second response. Inline responses are recorded, so retained clones (spawned tasks, `AppContext`) can follow up. `AppFramework` is `Clone`, as the docs already assumed.
- **Interaction tokens stay out of `Debug` output** for `InteractionContextData`, the interaction variants, `AppContext`, and the raw `InteractionContext` (including its raw payload).

- **Handlers can await `fetch_members` and collectors without stalling their shard.** Each shard applies events (cache, backend writes, voice, member-chunk correlation, collectors) in gateway order on an internal processor that never waits for handlers; handlers run in a bounded, tracked `JoinSet` (Serial = one at a time, Concurrent = `max_concurrent_handlers`, default 256) and their panics are contained. Serial mode used to deadlock until `fetch_members` timed out.
- **Event-stream gaps fail the shard instead of corrupting state.** Per-shard queues are bounded (`event_queue_capacity`, default 10,000) and the gateway reader never blocks; if an event cannot be admitted, or the processor or a dispatcher dies, the shard stops, its cache is invalidated, pending `fetch_members` calls fail, and `start()`/the supervisor return `DiscordError::Gateway`. `fetch_members` also requires every chunk index before returning.
- **Collectors created inside handlers no longer miss events.** The collector hub keeps a bounded, sequence-numbered journal; each handler delivery carries the cursor captured before it was queued, and `Context::*_collector()` replays from it before following live events (eviction surfaces as lag). `ctx.collectors().*` still starts from now.
- **Interactions get their own lane and deadline.** INTERACTION_CREATE has a separate queue, reserved slots (`max_concurrent_interactions`), receive timestamps (`Context::event_received_at`), and an admission deadline (`interaction_admission_timeout`, 2 s); interactions that cannot be admitted in time go to `on_interaction_overload`, which runs on its own bounded thread. The runtime never acknowledges an interaction itself.
- **READY no longer wipes other shards' cache.** `CacheHandle::clear_shard` drops only guilds owned by the shard (`(guild_id >> 22) % shard_count`), plus DMs on shard 0; users are kept.
- **External cache backend mirror is ordered, fenced, and self-healing.** One worker per client applies writes in order; a dropped or failed write quarantines that shard and runs an ordered resync with backoff. The store keeps one epoch: clears are compare-and-set with an idempotent `CacheClearId`, every write carries its epoch and is ignored when stale, so a delayed write or clear from an old client cannot resurrect or erase data. `Context::cache_backend_health()` reports health; shutdown reports unrecovered shards. The mirror supports one writing client per store, which deployments must enforce. MESSAGE_UPDATE is merged into the stored message (or skipped when no full copy exists), and the in-memory merge covers every message field.
- **Prompt, complete shard shutdown.** The supervisor watches all shards concurrently, stops the others on the first fatal error (also halting pending boot waves), returns that error from `start()`/`wait()`, aborts and joins tasks past `handler_shutdown_timeout`, and protects against cancellation from the moment the future is created. `ShardingManager::broadcast` reaches every shard even if one channel is closed, and guild-scoped gateway commands go to the shard that owns the guild.

- **Audio playback no longer blocks on stalled inputs or connections.** `AudioPlayer` reads each resource and drives each connection on separate tasks and handles commands immediately. Stop, pause, and replacement flush every connection before `TrackEnd`/`Paused` is published, so a sink honouring the `VoiceFrameSink` contract transmits no old-track audio afterwards. Every connection gets five silence frames and then speaking off when its audio stops, even after lagging; re-subscribing a sink hands the connection over serially.
- **Real-time voice delivery.** A frame must start sending within 60 ms and a send stalled for 60 ms is cancelled (audio is at most 120 ms late); a full queue drops the oldest frame; a recovering connection catches up at about twice real time. Untransmitted media becomes an RTP timestamp gap: `VoiceRuntimeHandle` advances sequence and timestamp only after a datagram is sent and never reuses a nonce (DAVE included). Underruns rebuffer, `unpause()` follows the same 60 ms rule, `AudioResource::next_pcm_frame` is cancel-safe, and panicking sinks/inputs are contained.

- **Multi-process sharding is bounded and IDENTIFY-coordinated.** Each child has its own bounded IPC writer with control-message priority on both sides, so a child that stops reading or writing no longer hides shutdown or grants; one deadline covers the whole shutdown (op → close stdin → keep reading → kill → confirmed reap); `wait()`/`shutdown()` agree and return every terminal failure (respawn exhaustion, spawn failure, unconfirmed kill, monitor panic). The parent grants every IDENTIFY — first connect, INVALID_SESSION, 4007/4009, reconnects before READY — per `shard_id % max_concurrency` bucket, keeps a bucket until the holder releases or its exit is confirmed, and enforces the daily session-start budget across refreshes. IPC frames carry an `@discordrs-ipc/1 ` prefix and a size cap, so stray stdout no longer corrupts the protocol; child request handlers are capped and cancellable. Commands queued while disconnected are kept in one bounded backlog (newest presence wins, at most 32 others, drops logged by kind only).

- Voice CLIENTS_CONNECT entries whose SSRC exceeds 32 bits are ignored instead of being truncated onto another SSRC; `EmbedBuilder` timestamps no longer wrap the year beyond 2³¹; the first heartbeat delay keeps sub-millisecond precision; a corrupt zlib byte counter is an error instead of a wrapped slice index.
- 15 broken intra-doc links are fixed, and CI fails on rustdoc warnings (all and default features).

### Added
- `RestClientBuilder::retry_non_idempotent_requests(bool)` to opt back in to replaying POST/PATCH after ambiguous failures.
- **Channel obfuscation readiness** (mandatory for all bots from 2026-11-16): `ChannelFlags` with `CHANNEL_OBFUSCATED` and the other documented channel flags, `Channel::channel_flags()` / `Channel::is_obfuscated()`, and IDENTIFY `capabilities` via `GatewayCapabilities`, `GatewayConnectionConfig::capabilities(...)`, and `ClientBuilder::gateway_capabilities(...)` for Discord's temporary early opt-in.
- `CurrentUserGuildsQuery::shard`, required since 2026-09-15 for `GET /users/@me/guilds` when an app uses large bot sharding.
- Invite target users without CSV: `CreateChannelInvite::target_user_ids`, `add_invite_target_user`, `remove_invite_target_user`, `bulk_add_invite_target_users`, and `bulk_delete_invite_target_users`.
- `file_types` filters on `FileUploadBuilder` and `CommandOptionBuilder` (`ATTACHMENT` options), validated for count and shape.
- Component Embed link previews: `Embed::components`, `Embed::flags`, and `Embed::is_component_embed()`.
- Application Identity Profile API (Game Stats Widgets): typed models, `UpdateApplicationIdentityProfile::validate()`, and five REST helpers; caller-supplied external IDs are percent-encoded as single path segments.
- Social SDK routes missing from the 2026-05-02 audit: `get_or_create_lobby` (`PUT /lobbies`), `send_lobby_message`, `get_lobby_messages`, `create_lobby_channel_invite_for_self`, `create_lobby_channel_invite_for_user`, and `delete_current_user_application_role_connection`.
- REST route coverage re-audited against discord-api-docs `3714862` (2026-10-09): 234 / 234 method + path routes mapped.
- Minimum supported Rust version declared as 1.88 (`rust-version`, all features) and verified in CI by building and testing default and all features on that toolchain.
- Weekly, report-only Discord API drift check (`ci/api_drift.py`, `api-drift` workflow): compares discord-api-docs REST routes and Gateway receive events with the crate in both directions and keeps a single `api-drift` issue up to date; it never fails CI. Helper-built routes are listed in `ci/api-drift-allowlist.toml` with a reason each, and every PR runs `--self-check` against the source.
- `ReactionCollector` via `CollectorHub::reaction_collector(message_id)` / `reaction_collector_for_message(&Message)` with discord.js `ReactionCollector` parity: filter, `max_items` (counts every accepted add like discord.js `total`) / `max_emojis` / `max_users`, time and idle windows, stop handles, `dispose(true)` for REMOVE / REMOVE_ALL / REMOVE_EMOJI, and message/channel/thread/parent-channel/guild deletion end reasons. Results are a `ReactionCollection` keyed by `EmojiKey`, with normal and burst users tracked separately.
- Slash-command validation covers mixed subcommand/option levels, required-before-optional order, unique (including localized) option names per level, choice type/autocomplete/value-length rules, min/max value and length ranges, locale keys and localized values, Discord's name character set and lowercase rule (rejecting only characters known to be invalid, so results do not depend on the compiler's Unicode version), and the 8,000-character per-command total using the longest localization of each field. Limits and the locale table are public in `discordrs::command::limits`.
- Option and choice localizations (`CommandOptionBuilder::name_localization`, `description_localization`, `localized_choice`) and `channel_types` on `CHANNEL` options, with the matching model fields.

- **Permission computation** (`discordrs::permissions`): an effective-permission projection following Discord's documented algorithm — owner and base `ADMINISTRATOR` bypass (decided before any other data is required), `@everyone` → aggregated roles → member overwrite order, timeouts limited to `VIEW_CHANNEL | READ_MESSAGE_HISTORY`, threads using the parent's overwrites, and the documented implicit `VIEW_CHANNEL` / `SEND_MESSAGES` (`SEND_MESSAGES_IN_THREADS` in threads) / `CONNECT` rules. `ComputedPermissions::raw` carries the discord.js-equivalent overwrite-only value. Obfuscated channels return `ChannelPermissions::Obfuscated` instead of a guess. Available over explicit inputs, models (`permissions_for`, `guild_permissions`, `Channel::permissions_for`, `Member::guild_permissions`), and the cache (`cached_permissions_for`, `cached_guild_permissions`); missing or contradictory data is a typed `PermissionDataError`.
- **Formatters** (`discordrs::formatters`) with `@discordjs/formatters` parity: mentions (user/role/channel/slash command/linked role/game profile), custom and animated emoji, `<t:…>` timestamps with all nine `TimestampStyle`s, links, text styles, code blocks, quotes, headings, subtext, flat and nested lists, `escape_markdown` with `EscapeMarkdownOptions` (byte-identical to discord.js 0.6.2 on a committed golden corpus, with four documented fixes for upstream regex bugs), `clean_code_block_content` (breaks every run of three or more backticks), and `clean_content` with an explicit `MentionResolver` using discord.js's exact command-name character class (Unicode 17.0 table, identical on every supported compiler). Also `Member::mention()` and `Message::clean_content(...)`.

- `InteractionResponsePhase` / `response_phase()`, `InteractionResponse::Accepted`, `InteractionContextData::create_response`, `edit_follow_up` / `delete_follow_up`, `InteractionReplyData::poll`, `INTERACTION_RESPONSE_PONG`, and `AppContext: InteractionResponder`. `InteractionResponder`, `InteractionReplyData`, and `AutocompleteChoice` are in the prelude.

- **Typed primitives for the 3.0 type migration** (consumers migrate in the following phase 3 series): `Id<M>` with aliases (`GuildId`, `ChannelId`, `UserId`, … `GenericId`) — 8 bytes, `Copy`, numeric hashing/ordering, strict decimal-string serde that rejects zero, signs, overflow, and garbage, no `Default` and no implicit `u64`/cross-marker conversions; `WidgetUserId`, `GuildInstallOwner`, `UploadId`, `ComponentId`, and `PollAnswerId` for ID-like wire values that may legitimately be `"0"` or are not snowflakes; open discriminant newtypes (`MessageType`, `ComponentType`, `AuditLogActionType`, …) that keep unknown values; and `Patch<T>` / `Nullable<T>` / `NullablePatch<T>` that distinguish an absent key, `null`, and a value (an absent required field is an error, never a silent `null`). Compile-fail tests (trybuild) pin the type-safety guarantees in CI.

- `ClientBuilder::{event_queue_capacity, max_concurrent_handlers, max_concurrent_interactions, interaction_admission_timeout, on_interaction_overload, handler_shutdown_timeout}`, `Context::{event_received_at, cache_backend_health}`, `CacheHandle::clear_shard`, `CollectorHub::{cursor, subscribe_from, *_collector_from}`.

- `AudioPlayer::shutdown().await`, `VoiceFrameTiming`, `VoiceRuntimeHandle::send_opus_frame_after_gap` / `send_opus_frame_with_dave_after_gap`.

- `IdentifyGate` / `IdentifyPermit` / `IdentifyOutcome` with `GatewayConnectionConfig::identify_gate` and `ClientBuilder::identify_gate`; `ShardChildProcess::{identify_gate, acquire_identify, notify_shard_ready}`; `ProcessShardManagerConfig::{max_concurrency, identify_interval, identify_permit_timeout, broadcast_timeout, shutdown_timeout, max_ipc_frame_bytes, session_start_budget}`.

### Changed
- The merged 2.2.0 sources are now rustfmt-clean, so CI's `cargo fmt --check` passes.
- CI's blocking jobs build with a pinned Rust toolchain (1.99.0) instead of the moving stable channel; a non-blocking canary job reports new stable lints.
- The published crate excludes `ci/`, `.github/`, `discordrsdocs/`, and `docs/`.

- `CollectorEndReason` is `#[non_exhaustive]` and gains `EmojiLimit`, `UserLimit`, `MessageDelete`, `ChannelDelete`, `ThreadDelete`, and `GuildDelete`. Collectors no longer yield an event, or restart the idle window, after their timeout or idle window expired; when several outcomes are ready the order is stop, timeout, idle, then events (a 2.2.0 bug).
- `validate()` / `try_build()` on slash and entry-point command builders now reject names Discord rejects (punctuation, uppercase in any script, the ASCII apostrophe — use `ʼ` U+02BC) and unknown locale keys; `build()` still never validates.

- `From<CreateMessage> for InteractionReplyData` is replaced by `TryFrom`, which keeps `poll` and every other field interaction responses accept and rejects channel-only fields (`message_reference`, `sticker_ids`, `nonce`, `enforce_nonce`) instead of dropping them. `InteractionResponder` methods return `Send` futures so generic code can `tokio::spawn` them; `is_acknowledged()` / `is_replied()` / `is_deferred()` report only what Discord confirmed.

- `CacheBackend` (breaking): clears are `clear_cache(expected_epoch, clear_id)` / `clear_shard_cache(expected_epoch, clear_id, shard_id, shard_count)` returning `CacheClear::{Applied, Rejected}`, and every write takes the epoch it was issued under. In Serial mode the cache a handler reads may already include later events.

- Voice (breaking): `VoiceFrameSink::send_opus_packet` takes `VoiceFrameTiming` and has a documented cancellation contract; without `voice-encode`, `AudioPlayer::play` returns `DiscordError::Voice` instead of silently advancing; `Buffering` also covers rebuffering and unpausing; subscribing an already-subscribed sink replaces the earlier subscription.

- Process sharding (breaking): the IPC wire format requires the frame prefix (2.x parents and children cannot talk to 3.0 ones); `ParentMessage`/`ChildMessage` are `#[non_exhaustive]` with identify variants; `ShardChildProcess::serve` returns `Result`; the `spawn_stagger` default is 0 (pacing comes from permits); `broadcast` errors when a payload exceeds the frame cap; `wait()`/`shutdown()` return errors for respawn exhaustion and unsuccessful exits.

- **No more `async-trait`** (breaking). `EventHandler`, `CacheBackend`, `InteractionHandler`, `TypedInteractionHandler`, and `CachedManager` declare `fn … -> impl Future<Output = T> + Send`; implement them with plain `async fn` and delete `#[async_trait]` (a leftover attribute fails with E0195). Futures must be `Send`, enforced by a compile-fail test. The traits are not object safe: `ClientBuilder::cache_backend` takes `Arc<B>` for a concrete `B: CacheBackend`. The default `handle_event` routing allocates once per event instead of twice. Default features now build with no duplicate crates (previously `syn` 2 and 3).
- The `hex` dependency is replaced by a private helper (`From<hex::FromHexError> for DiscordError` is removed; malformed hex stays `DiscordError::Model` and names the input), and the `interactions` feature uses axum 0.8.

- Smaller generated code: `RestClient` runs every request through one non-generic loop with thin typed shims that decode without an intermediate `String`, and long-lived internal tasks (voice player, sharding IPC writer) are spawned through one boxed path. The all-features library emits 3.75% less LLVM IR and a 150-endpoint consumer binary is 2.6% smaller; behaviour is unchanged.

- **Numeric IDs everywhere** (breaking). The String-backed `Snowflake` is replaced by `GenericId` (`Copy`, 8 bytes, never zero, strictly parsed) on every model, event, REST/cache/collector parameter, and cache key; `Snowflake` remains as a deprecated alias. ID parameters take a concrete `GenericId` by value (no implicit `u64`/`&str` conversions — use `"123".parse()?` or `GenericId::try_from(123)?`). Wire values that are not snowflakes have their own types: `GuildWidgetMember::id: WidgetUserId`, `AuthorizingIntegrationOwners` (bot-DM installs read as `GuildInstallOwner::BotDm`), `MessageComponent::id: Option<ComponentId>`, `PollAnswerId`, and `CreateGuildPlaceholderId` for the deprecated `CreateGuild` body. `Default` is removed from persisted models and request bodies with a required ID (new constructors such as `CreateDmChannel::new`, `LobbyMember::new`). **Effect:** hot models are 21–30% smaller (`Message` 6,360 → 4,808 bytes), and typed GUILD_CREATE decoding makes 42% fewer allocations and retains 31% less memory.

- **Code rules enforced by lints.** `Cargo.toml` `[lints]` denies `unwrap`/`expect`/`panic!`/`todo!`/`unimplemented!`/`dbg!`/`mem::forget`, `as` conversions, `allow` without a reason, silently discarded `#[must_use]` results and futures, std locks held across `.await`, and `unsafe` (`#![forbid(unsafe_code)]` in the library). Library code has no `as` casts, no unexplained `let _`, and no `#[allow]`; protocol literals are named constants citing their source, and every piece of shared state documents why it is shared (see "Code rules" in `discordrsdocs/docs/guide/architecture.md`). Failed gateway/voice close frames, late shard-child responses, and stdout drain timeouts are now logged at debug level, and the examples no longer discard errors.

### Deprecated
- `RestClient::create_guild`, `delete_guild`, `create_guild_from_template`, and `modify_guild_mfa_level`, plus `Guild::delete` and `Guild::set_mfa_level`: Discord removed guild creation by applications on 2025-07-15 and transferred bot-owned guilds to users, and dropped these routes from its docs. They will be removed in the next major release.
- 11 interaction helpers duplicated by `InteractionResponder` (`defer_interaction`, `defer_update_interaction`, `update_interaction_message`, `respond_with_modal_typed`, `respond_with_autocomplete_choices`, `followup_message`, `get_original_response`, `edit_original_response`, `delete_original_response`, `edit_followup_response`, `delete_followup_response`); id/token-only helpers are documented as stateless.
- `interactions_endpoint` and `typed_interactions_endpoint`, which panic on an invalid public key: use `try_interactions_endpoint` / `try_typed_interactions_endpoint`.

## 2.2.0 - 2026-07-27

High-level framework release: the discord.js-style ergonomics layer. Where 2.1.0 closed protocol gaps, 2.2.0 closes the developer-experience gap.

### Added
- **Interaction response API** — `interaction.reply(...)`, `reply_ephemeral`, `reply_with_result`, `defer`, `defer_ephemeral`, `defer_update`, `update_message`, `show_modal`, `respond_autocomplete`, `edit_reply`, `fetch_reply`, `delete_reply`, `follow_up` on every responding-capable interaction variant (`InteractionResponder` trait in `discordrs::response`). A shared atomic response state (created at parse time, shared across clones) enforces the discord.js contract: double replies and follow-ups before acknowledgement fail locally, deferred interactions promote to replied on `edit_reply`, and the state slot is claimed atomically before the HTTP call with rollback on transport failure. `InteractionReplyData` converts from `&str`, `String`, `MessageBuilder`, and `CreateMessage`.
- **Entity convenience methods** (`discordrs::model_ext`) — `message.reply/edit/delete/react/pin/crosspost/forward_to/start_thread/link`, `member.kick/ban/timeout/add_role/remove_role/edit/display_name`, `guild.edit/delete/leave/fetch_channels/create_channel/fetch_member/fetch_roles/create_role/ban/unban/kick/set_mfa_level/icon_url/banner_url`, `channel.send/send_message/edit/delete/create_invite/mention/is_text_based/is_voice_based/is_thread`, `role.edit/delete/mention`, `user.create_dm/dm/tag/mention/avatar_url/display_avatar_url` — plus CDN URL helpers with animated-hash and default-avatar handling.
- **Multi-process sharding** (`discordrs::sharding::process`) — `ProcessShardManager` spawns one child process per shard group with staggered identify-safe boot, exponential-backoff auto-respawn (reset after 60s uptime, `max_respawns` cap), JSON-lines IPC over child stdio, `broadcast(...)` request/response across all children (the `broadcastEval`/`fetchClientValues` equivalent), graceful shutdown, and `ShardChildProcess::from_env()` so one binary serves as both parent and child. Ships a `process_sharding_bot` example.
- **Audio playback pipeline** (`discordrs::voice::player`) — `AudioInput` (FFmpeg process spawn, raw PCM readers, files), `AudioResource` with 20ms frame chunking, live volume control, and silence padding, `AudioPlayer` state machine (Idle/Buffering/Playing/Paused/AutoPaused) with watch-based state and broadcast events (`TrackStart`, `TrackEnd` with reasons), Opus encoding behind `voice-encode`, five-frame silence flush and speaking-flag handling, and drop-safe subscriptions with `NoSubscriberBehavior::{Pause,Play,Stop}`. Ships a `music_bot` example.
- **Collector parity** — `stop()`/`stop_with_reason`, cloneable `CollectorStopHandle`, `end_reason()` (`Limit`/`Time`/`Idle`/`User`/`ChannelDropped`), idle timeouts distinct from the overall timeout, `reset_timer()`, uniform `filter()` on component and modal collectors, `received_count()`.
- **Builder runtime validation** — `validate()` and `try_build()` on command, button, select-menu, action-row, embed, modal, container, and media builders, enforcing Discord's documented limits (codepoint-counted lengths, option/choice/field counts, action-row composition, embed 6000-char total, Components V2 caps) with errors that name the field, the limit, and the actual value.
- **Configuration parity** — `RestClient::builder()` (API base/version override, connect/request timeouts, custom user agent, proxy, custom reqwest client, rate-limit callback with route/retry_after/global scope); default allowed mentions on both `RestClient` and `ClientBuilder`, injected only when a payload omits them; `ClientBuilder::cache_backend(...)` forwarding member/message/presence cache writes to a pluggable `CacheBackend` without stalling the gateway; configurable cache sweep interval.
- **Typed model completion** — `Message.components` and related read-side payloads move from raw `serde_json::Value` to tolerant typed structures, interaction `resolved` data and message interaction metadata become typed containers (see the API docs for exact shapes).

## 2.1.0 - 2026-07-27

Feature-parity and hardening release closing the remaining gaps against discord.js v14.27.

### Added
- `GUILD_MESSAGE_POLLS` (`1 << 24`) and `DIRECT_MESSAGE_POLLS` (`1 << 25`) gateway intents, plus a `GUILD_EXPRESSIONS` alias for bit 3; both poll intents are included in `NON_PRIVILEGED`, so `MESSAGE_POLL_VOTE_*` events can now be received with named constants.
- `RestClient::with_reason(...)` — a cheap scoped clone that sends `X-Audit-Log-Reason` (percent-encoded like discord.js's `encodeURIComponent`) with every mutating request, so bans, kicks, and edits show a reason in the guild audit log.
- `create_guild(...)`, `delete_guild(...)`, `create_guild_from_template(...)`, and `modify_guild_mfa_level(...)` REST helpers with typed `CreateGuild`, `CreateGuildFromTemplate`, and `GuildMfaLevel` bodies.
- `create_interaction_response_with_result(...)` — interaction callback with `with_response=true`, returning the typed `InteractionCallbackResult` resource (mirrors discord.js `withResponse: true`).
- `MessageReferenceType` (`DEFAULT`/`FORWARD`), `MessageReference::reply(...)`/`forward(...)` constructors, and `RestClient::forward_message(...)` for one-call message forwarding.
- `OAuth2Client::revoke_token(...)` for `POST /oauth2/token/revoke`.
- Scheduled-event query options: `get_guild_scheduled_events_with_query(...)` (`with_user_count`) and `get_guild_scheduled_event_users_with_query(...)` (`limit`, `with_member`, `before`, `after`).
- `ClientBuilder::presence(...)` — initial presence delivered inside IDENTIFY, so bots connect with the desired status instead of updating it after READY.
- `Context::fetch_members(...)` / `fetch_members_with_timeout(...)` — requests guild members over the gateway and awaits the correlated `GUILD_MEMBERS_CHUNK` payloads (the discord.js `guild.members.fetch()` equivalent); fetched members and presences also fill the cache.
- `EventDispatchMode` (`Serial` default / `Concurrent`) via `ClientBuilder::event_dispatch(...)`, letting each handler call run in its own task so one slow handler no longer stalls a shard, plus an event-backlog warning when the dispatch queue exceeds 5,000 events.
- `Guild` now models the GUILD_CREATE collections (`channels`, `threads`, `members`, `voice_states`, `presences`, `emojis`, `stickers`, `stage_instances`, `soundboard_sounds`, `guild_scheduled_events`, `joined_at`, `large`) and `Guild::without_create_collections()`.
- Typed event-field completion: `InviteEvent` (inviter, uses, max_uses, max_age, temporary, created_at, expires_at, target type/user), `ThreadEvent::newly_created`, `ThreadListSyncEvent` (`channel_ids`, `members`), `ThreadMemberUpdateEvent` (`user_id`, `join_timestamp`, `flags`), typed `ThreadMember` in `ThreadMembersUpdateEvent`, and typed `AutoModerationTriggerMetadata`/`AutoModerationAction` in auto-moderation events.
- `HttpError` now exposes `is_timeout()`/`is_connect()`/`is_body()`/`is_retryable()`, and `DiscordError::is_retryable_transport()`.

### Changed
- GUILD_CREATE now populates the channel, thread, member, user, voice-state, presence, emoji, sticker, and stage-instance caches under a single write lock; the cached `Guild` is stored with bulk collections stripped to avoid double storage. GUILD_MEMBERS_CHUNK and THREAD_* events are cached too.
- The REST transport is unified into one retry loop: the per-route serialization gate now covers **all** requests (previously JSON requests bypassed it, allowing same-bucket 429 races), 5xx responses and transient transport errors retry with backoff (0.5s/1s/2s), 429 handling reads the `Retry-After` header and `x-ratelimit-scope`/`x-ratelimit-global` headers (Cloudflare-level bans with HTML bodies now back off correctly), and the route-gate map is garbage-collected.
- The gateway now paces IDENTIFY to Discord's 1-per-5s-per-shard limit, and short-lived sessions (< 30s) reconnect with escalating backoff instead of immediately, preventing tight reconnect/re-IDENTIFY loops on repeated INVALID_SESSION.
- `ClientBuilder::spawn_shards(...)` fetches `/gateway/bot` to use the account's real `max_concurrency` for identify wave sizing (previously hardcoded to 1).
- Cache hot paths rewritten: LRU order tracking moved from `VecDeque` scans (O(n) per insert) to an ordered-map structure (O(log n)), per-guild member and per-channel message caps use incremental counters instead of full-map scans, TTL sweeps are throttled to once per 5s, and list reads filter expired entries per-entry under a read lock instead of upgrading to a write lock.
- The voice manager write lock is now taken only for voice events instead of on every dispatch.
- Gateway protocol failures (missing Hello, decode errors, terminal close codes, command-queue overflow) now surface as `DiscordError::Gateway` instead of `DiscordError::Model`.
- `RestClient` is now `Clone` (clones share rate-limit state, the connection pool, and the application id).
- Component-interaction channel stubs no longer overwrite fully-cached channels (insert-if-absent).

## 2.0.2 - 2026-05-02

- Included the post-2.0.1 DAVE runtime clippy cleanup in the published crate so Linux stable CI and local release artifacts match.
- Added coverage for short malformed DAVE commit/welcome binary payloads, preserving the ignored-state semantics while keeping line coverage above the 2.0.1 release checkpoint.
- Updated install snippets, Docsify pages, Korean quick-start docs, and API coverage notes for the 2.0.2 patch release.

## 2.0.1 - 2026-05-02

- Added bounded REST client connect/request timeouts so Discord HTTP calls do not wait indefinitely on stalled network paths.
- Made REST rate-limit mutex state recover from mutex poisoning instead of panicking on later requests.
- Reduced cache read-path write-lock contention by using read locks for member, message, and presence lookups unless TTL pruning is required.
- Added `Arc`-returning hot cache read APIs for members, messages, and presences, plus manager `cached_arc(...)` helpers, while keeping existing owned-return APIs compatible.
- Added a `CacheBackend` trait for external member/message/presence cache stores; the default `CacheHandle` implements it.
- Added route-level REST rate-limit gates so concurrent tasks for the same Discord bucket are serialized before request dispatch.
- Bounded Gateway shard command queues to avoid unbounded command accumulation during reconnects, outages, or stalled shards.
- Hardened tokenized webhook/interaction path validation with an allowlist, added Snowflake path-segment validation helpers, and capped generated Axum interaction endpoints at 64 KiB request bodies.
- Split cache managers, focused channel/thread model groups, and automoderation, emoji, monetization, scheduled event, thread, sticker, and soundboard REST routes into internal modules while preserving public re-exports.
- Deprecated raw JSON legacy helpers `get_public_archived_threads(...)` and `get_guild_audit_log(...)`; use `list_public_archived_threads(...)` and `get_guild_audit_log_typed(...)` instead.
- Documented that compatibility aliases `Error` and `BoxError` remain for the 2.x line and are candidates for removal in the next major release.

## 2.0.0 - 2026-05-02

- Added `AppFramework` for typed HTTP interaction routing across commands, components, and modals.
- Added typed Webhook Events parsing for Discord's HTTP event payloads, including app authorization, entitlement, lobby message, and game DM event families.
- Added typed application Activity Instance models and REST helper for Discord's Activity session lookup route.
- Added typed application command permission models and REST helpers, including OAuth2 Bearer-token writes for Discord's command-permission edit route.
- Added typed current-user OAuth2 connection and application role connection models and REST helpers.
- Added `CurrentUserGuildsQuery` and current-user guild pagination/count support, including `banner`, `approximate_member_count`, and `approximate_presence_count` fields.
- Added typed Lobby resource models and REST helpers for lobby CRUD, member updates, channel linking, and moderation metadata.
- Added typed guild incident-action models and a REST helper for Discord's `PUT /guilds/{guild.id}/incident-actions` safety route.
- Added typed Audit Log Resource models and `get_guild_audit_log_typed(...)`, including Discord's `after` cursor query.
- Added `GetGuildQuery` and `get_guild_with_query(...)` for Discord's optional `with_counts` guild fetch.
- Added `GuildBansQuery` and `get_guild_bans_with_query(...)` for Discord's `before`/`after` guild ban pagination.
- Added typed guild channel-position and OAuth2 guild-member join helpers for `PATCH /guilds/{guild.id}/channels` and `PUT /guilds/{guild.id}/members/{user.id}`.
- Added `ModifyGuildRolePosition` and `modify_guild_role_positions_typed(...)` for typed guild role reordering.
- Added typed Guild Resource request bodies for single-member bans, member/current-member edits, and guild role create/update routes.
- Added typed Guild Resource request bodies for modifying guilds, creating guild channels, widget settings, welcome screens, and onboarding.
- Added `BeginGuildPruneRequest` and `begin_guild_prune_with_request(...)` for Discord's current JSON-body prune route.
- Added `CreateStageInstance`, `ModifyStageInstance`, and typed Stage Instance request wrappers.
- Added `get_sticker_pack(...)`, `CreateGuildSticker`, `ModifyGuildSticker`, and typed guild sticker write wrappers.
- Added typed Voice Resource REST helpers for current-user and user voice-state reads/writes.
- Added `ApplicationInstallParams`, `ApplicationIntegrationTypeConfig`, `ModifyCurrentApplication`, and a typed current-application edit wrapper.
- Added typed Webhook Resource request bodies, query-aware webhook execution and message helpers, Slack/GitHub-compatible webhook execution helpers, and tokenized webhook path validation.
- Expanded `Sku` and `Entitlement` to preserve Discord's current monetization response metadata such as SKU feature/release fields and entitlement promotion/gift-code fields.
- Added Invite target-user CSV helpers and job-status models, plus multipart invite creation for Discord's `target_users_file` flow.
- Added `SelectDefaultValue` and select-menu builder support for Discord's current auto-populated select defaults, component IDs, and modal `required` flag.
- Added `GuildMembersQuery` and `get_guild_members_with_query(...)` for Discord's `after` cursor on guild member list pagination.
- Added `SearchGuildMembersQuery` and `search_guild_members_with_query(...)` with shared percent-encoded query building.
- Expanded `Member` for current Guild Member fields such as guild banner, avatar decoration data, and collectibles.
- Added typed guild role member-count reads for `GET /guilds/{guild.id}/roles/member-counts`.
- Added typed guild widget reads and unauthenticated PNG widget-image downloads for Discord's public widget routes.
- Added typed Create Group DM support through `CreateGroupDmChannel` and `create_group_dm_channel_typed(...)`.
- Added typed Group DM recipient add/remove helpers for Discord's `gdm.join` flow.
- Added typed guild message search and current channel-pin listing helpers for Discord's Message Resource.
- Updated `pin_message(...)` and `unpin_message(...)` to Discord's current Message Resource pin routes.
- Added legacy channel pin aliases, `GET /gateway`, OAuth2 authorization/application metadata helpers, guild-template code lookup, and corrected Lobby bulk/moderation routes to close the 2026-05-02 official REST route-shape audit at 223/223 mapped routes.
- Added typed `AllowedMentions` and `ReactionCountDetails` models for Message Resource object coverage.
- Added REST wrappers for the legacy batch application-command permissions route and single guild-command fetch route so every official route shape has a concrete client entry point.
- Expanded the `Message` model for current Message Resource fields such as forwarded snapshots, call metadata, shared client themes, mention role IDs, and referenced messages.
- Added typed Channel Resource helpers for channel invite listing/creation, voice-channel status updates, and channel permission overwrite edits.
- Added Gateway opcode 43 channel-info requests and typed `CHANNEL_INFO` event decoding.
- Added typed Gateway `RATE_LIMITED` event decoding and preserved current reaction dispatch fields such as `member`, `message_author_id`, `burst`, `burst_colors`, and reaction `type`.
- Expanded `PRESENCE_UPDATE` decoding with partial user, activity, and client-status metadata.
- Kept all-features line coverage above the 90% release gate with regression coverage for the new REST, Gateway, event, and framework surfaces.

## 1.2.2

- Fixed default Gateway startup so Identify no longer requests payload compression without a matching receive decoder, preventing compressed binary payloads from being read as UTF-8 JSON.
- Fixed explicit `zlib-stream` Gateway transport compression so compressed `HELLO` frames are decoded with the same stream decoder used for subsequent dispatch payloads.
- Added local websocket regression coverage for compressed Gateway `HELLO` and dispatch frames.
- Updated README, USAGE, Docsify docs, and the `discordrs-dev` Codex skill guidance for the `1.2.2` Gateway compression hotfix.

## 1.2.1

- Fixed invite-code path injection in `get_invite`, `get_invite_with_options`, and `delete_invite` by validating invite codes before building authenticated REST paths.
- Changed REST request body serialization to return `DiscordError::Json` instead of panicking when user-provided `Serialize` values cannot be represented as JSON.
- Percent-encoded generated REST query strings so timestamps, comma-separated lists, and future string parameters cannot corrupt query boundaries.
- Improved HTTP 429 handling by retrying repeated rate-limit responses up to a bounded limit while continuing to honor route and global rate-limit state.
- Enabled the `cache` feature by default and added `CacheHandle::is_enabled()` so cache-backed manager behavior is explicit in default installs.
- Changed `CacheConfig::default()` to use bounded retention for gateway cache data, added `ClientBuilder::cache_config(...)`, and kept `CacheConfig::unbounded()` as an explicit opt-in.
- Redacted OAuth2 credentials from `Debug` output for OAuth2 clients, code exchanges, refresh tokens, and token responses.
- Added `Snowflake::try_new` and `Snowflake::is_valid` for callers that want early validation while preserving the existing permissive constructor.
- Added fallible command-choice constructors while keeping existing command builder calls panic-free.
- Fixed the all-target Clippy warning in the gateway outbound limiter and added regression tests for invite validation, serialization errors, and repeated rate-limit retries.
- Updated README, USAGE, and Docsify docs for the `1.2.1` security and stability release.

## 1.2.0

- Added a `voice-encode` feature with optional `opus-rs` support, including `PcmFrame`, `AudioSource`, `AudioMixer`, and `VoiceOpusEncoder` for 48 kHz stereo 20 ms PCM-to-Opus playback through the existing voice runtime.
- Added outbound voice playback plumbing for encoded Opus frames, RTP sequence/timestamp/nonce management, and automatic speaking on/off behavior around playback.
- Added DAVE outbound media support with `VoiceDaveySession`, `VoiceDaveFrameEncryptor`, `send_opus_frame_with_dave(...)`, and typed DAVE MLS gateway commands for transition-ready, key-package, commit/welcome, and invalid commit/welcome flows.
- Added an ignored live DAVE MLS transition harness (`tests/live_voice_dave.rs`) and validated it against a real Discord voice session for the 2.0.0 release.
- Expanded Gateway typed coverage, EventHandler convenience hooks, request-guild-members helpers, interaction payload fields, application command fields, and richer Activity/Presence models.
- Added OAuth2 backend helpers, typed REST coverage for moderation/application/guild administration gaps, and continued the split of HTTP helper/path/body/rate-limit code away from the monolithic HTTP client.
- Added cache storage and eviction controls for guild metadata, emojis, stickers, scheduled events, stage instances, voice state, soundboard, and other long-running bot data.
- Hardened collector deadlines, shard IPC lifetime, voice heartbeat nonce generation, gateway outbound scheduling, reconnect jitter, and HTTP rate-limit bucket cleanup.
- Made coverage reporting stricter by removing broad coverage ignore shortcuts and keeping the release gate on real all-features line coverage.
- Updated README, USAGE, Docsify docs, and the `discordrs-dev` Codex skill guidance for the `1.2.0` public surface.

## 1.1.0

- Added Discord Gateway `zlib-stream` handling that keeps compressed payload state across binary frames and inflates only complete payload boundaries.
- Added multipart file upload support through `reqwest`'s multipart feature, including typed message, webhook, and interaction attachment helpers.
- Added typed webhook message CRUD helpers for token-authenticated webhook message fetch, edit, and delete flows.
- Added typed Poll models and REST/event coverage for poll payloads, vote events, and poll ending.
- Expanded Auto Moderation, Scheduled Event, Audit Log, Sticker, Stage Instance, Welcome Screen, Guild Onboarding, Guild Template, Invite, Integration, Forum, Soundboard, Subscription, SKU, and Entitlement models and REST/event coverage.
- Expanded cache coverage for emoji, stickers, voice states, presences, threads, webhooks, scheduled events, AutoMod rules, invites, integrations, soundboard sounds, and monetization entities with cache policy toggles.
- Added voice receive support for raw UDP packets, RTP parsing, AES-GCM and XChaCha RTP-size transport decrypt, and pure-Rust Opus PCM decoding.
- Added experimental `dave` feature support for DAVE/MLS frame parsing, state tracking, and a `davey`/OpenMLS-backed decryptor hook. Live Discord DAVE interoperability still requires real voice gateway transition testing.
- Changed the HTTP User-Agent version to use `env!("CARGO_PKG_VERSION")` so future package versions no longer need a hard-coded request-header update.
- Updated README, USAGE, Docsify docs, and the `discordrs-dev` Codex skill guidance for the `1.1.0` public surface.

## 1.0.0

- **BREAKING**: Removed the legacy raw `RestClient` convenience methods (`send_message`, `edit_message`, `create_dm_channel`, `create_interaction_response`, and `bulk_overwrite_global_commands`) from the public API. The typed `RestClient` surface is now the supported path, and internal JSON helpers remain crate-private.
- **BREAKING**: Builder implementation submodules are now private. Import builders from `discordrs::builders::{...}` or the crate root re-exports instead of deeper paths such as `discordrs::builders::modal::*`.
- **BREAKING**: `ApplicationCommand` no longer implements `DiscordModel`. Use `ApplicationCommand::id_opt()` and `ApplicationCommand::created_at()` for optional-ID command values.
- Changed gateway event processing to preserve ordering through a dedicated event processor instead of unbounded per-event task spawning.
- Changed unsupported gateway `compress=zlib-stream` configuration to be stripped from normalized URLs so the runtime no longer advertises a mode it cannot process.
- Changed interaction request verification to reject stale or future timestamps outside a five-minute freshness window.
- Hardened token-authenticated callback/webhook HTTP paths by rejecting unsafe path segments and omitting bot `Authorization` headers from `/interactions/...` and `/webhooks/...` requests.
- Fixed gateway Identify/Resume payloads to use the raw Discord token instead of an HTTP-style `Bot ` prefix.
- Fixed REST error typing so Discord API failures now surface as `DiscordError::Api` / `DiscordError::RateLimit` instead of collapsing into model errors.
- Fixed typed command and autocomplete interactions to preserve nested option `value` / `focused` input data through `CommandInteractionOption`.
- Changed voice state handling to clear stale runtime/session state on disconnect and endpoint loss.
- Fixed stale interaction examples in `USAGE.md` to match the current typed helper and typed endpoint APIs.
- Added README / USAGE migration notes for the tightened public API surface and canonical replacement paths.

## 0.4.0

- Fixed gateway cache consistency after the upgrade: `READY` now clears stale cache state, `MESSAGE_UPDATE` merges partial payloads into cached messages instead of wiping fields, and guild/channel/bulk-delete paths now evict dependent cached entries.
- Fixed `EventHandler` typed dispatch so `ready`, `message_create`, `interaction_create`, and the newly-added typed event hooks receive typed payloads, while `raw_event` is reserved for unknown events.
- Fixed REST command registration safety so legacy global-command overwrite helpers now reject missing `application_id`, matching the typed command paths.
- Fixed REST route-key normalization so major parameters (`applications`, `channels`, `guilds`, `webhooks`) keep distinct rate-limit buckets.
- Fixed `EmbedBuilder::timestamp_now()` to emit Discord-compatible UTC ISO 8601 timestamps instead of a placeholder string.
- Added typed `Snowflake`, `PermissionsBitField`, `User`, `Guild`, `Channel`, `Member`, `Role`, `Message`, `ApplicationCommand`, and typed interaction variants for chat-input, context-menu, autocomplete, component, and modal submit flows.
- Added typed `Event` decoding and the new `Client` / `ClientBuilder` gateway runtime surface while keeping `BotClient` and `BotClientBuilder` compatibility aliases.
- Added `Context::new(http, data)` so test code and helper crates can still construct a standalone `Context` outside the live gateway runtime.
- Added `RestClient` as the preferred HTTP-facing surface while keeping `DiscordHttpClient` as a compatibility alias.
- Added shared route/global rate-limit tracking for REST requests instead of a single retry-only 429 path.
- Added typed command builders: `SlashCommandBuilder`, `UserCommandBuilder`, `MessageCommandBuilder`, and `CommandOptionBuilder`, including common shortcut helpers for option-heavy slash commands.
- Added `MessageBuilder` and `InteractionResponseBuilder` so common message and interaction response bodies no longer need hand-written JSON.
- Added typed helper paths such as `send_message`, `respond_to_interaction`, `respond_with_message`, `followup_message`, `defer_interaction`, and `update_interaction_message`.
- Added typed interactions endpoint helpers: `TypedInteractionHandler`, `try_typed_interactions_endpoint`, and `typed_interactions_endpoint`.
- Added cache managers behind the `cache` feature and collectors behind the `collectors` feature.
- Added `prelude` re-exports for common runtime, builder, helper, and response types.
- Added `ws` gateway config abstraction and promoted it into the public surface for reusable gateway URL, shard, and encoding configuration.
- Added shard control primitives: `ShardConfig`, `ShardInfo`, `ShardIpcMessage`, `ShardRuntimeStatus`, `ShardingManager`, `ShardMessenger`, and `ShardSupervisor`.
- Added shard-local gateway control from `Context` and `ShardMessenger`, including presence updates, reconnect, shutdown, and voice state update payloads.
- Added supervisor-level shard control helpers so sharded runtimes can drive reconnect, presence, and voice state updates without bypassing the shard IPC layer.
- Added queued shard boot behavior so multi-shard startup does not identify every shard immediately, and exposed `ShardRuntimeState::Queued` to make that lifecycle visible.
- Added shutdown waiting helpers: `ShardSupervisor::shutdown_and_wait()` and `ShardSupervisor::wait_for_shutdown(...)`.
- Added interruptible reconnect backoff so shutdown does not wait for long reconnect sleeps to finish.
- Added `voice` state management plus a `voice_runtime` surface with websocket hello/identify, UDP IP discovery, select-protocol, session-description wait, speaking updates, and graceful close helpers.
- Added checked-in examples for gateway, interactions endpoint, typed interactions endpoint, slash commands, cache, collectors, sharding, and voice runtime setup.
- Added broader tests for typed models, typed events, cache and collectors, sharding state/control, gateway payload helpers, and voice runtime handshake flow.

## 0.3.1

- Added safer builder serialization for buttons and select menus so invalid Discord payload combinations are normalized before send.
- Added modal `FILE_UPLOAD` parsing support and `V2ModalSubmission::get_file_values()`.
- Added explicit follow-up webhook methods that accept `application_id` and fail early when it is missing.
- Added `try_interactions_endpoint()` for startup-time Discord public-key validation.
- Changed gateway reconnect behavior to preserve required resume query parameters and stop retrying documented terminal close codes forever.

## 0.3.0

- **BREAKING**: Complete rewrite from a serenity helper library to a standalone Discord bot framework.
- **BREAKING**: Helper functions now take `&DiscordHttpClient` with raw `&str` and `u64` IDs instead of serenity model types.
- **Added**: Gateway WebSocket client behind the `gateway` feature.
- **Added**: `BotClient`, `BotClientBuilder`, `EventHandler`, `Context`, and `TypeMap` for gateway bot runtime.
- **Added**: `DiscordHttpClient`, a reqwest-based REST client with automatic HTTP 429 retry.
- **Added**: `parse_raw_interaction()` and `parse_interaction_context()` for interaction routing.
- **Added**: `V2ModalSubmission` parser that preserves `Label`, `RadioGroup`, `CheckboxGroup`, `Checkbox`, and other V2 modal components.
- **Added**: `InteractionContext` with `id`, `token`, `application_id`, `guild_id`, `channel_id`, and `user_id`.
- **Added**: HTTP Interactions Endpoint behind the `interactions` feature, including Ed25519 request verification.
- **Removed**: All serenity dependencies.
- **Changed**: Module structure reorganized into dedicated `gateway/`, `parsers/`, and `builders/` directories.

## 0.1.3

- Added modal interaction components:
  - `RadioGroupBuilder` for single-choice selection.
  - `CheckboxGroupBuilder` for multi-choice selection.
  - `CheckboxBuilder` for yes/no style toggles.
- Updated package version to `0.1.3` in `Cargo.toml`.
