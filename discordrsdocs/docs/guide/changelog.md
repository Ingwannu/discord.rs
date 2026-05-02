# Changelog

## Unreleased

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
- Added an ignored live DAVE MLS transition harness (`tests/live_voice_dave.rs`) so production interoperability can be verified against a real Discord voice session without running live network tests by default.
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
- Changed unsupported gateway `compress=zlib-stream` configuration to be stripped from normalized URLs so the runtime no longer advertised a mode it could not process.
- Changed interaction request verification to reject stale or future timestamps outside a five-minute freshness window.
- Hardened token-authenticated callback/webhook HTTP paths by rejecting unsafe path segments and omitting bot `Authorization` headers from `/interactions/...` and `/webhooks/...` requests.
- Fixed gateway Identify/Resume payloads to use the raw Discord token instead of an HTTP-style `Bot ` prefix.
- Fixed REST error typing so Discord API failures surface as `DiscordError::Api` / `DiscordError::RateLimit` instead of collapsing into model errors.
- Fixed typed command and autocomplete interactions to preserve nested option `value` / `focused` input data through `CommandInteractionOption`.
- Changed voice state handling to clear stale runtime/session state on disconnect and endpoint loss.
- Added README / USAGE migration notes for the tightened public API surface and canonical replacement paths.

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
