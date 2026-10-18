# Discord API Coverage Audit

This page records the current coverage claim for the unreleased 3.0 line against the official Discord API documentation. The REST route-shape audit was re-run on 2026-10-10: the 2026-05-02 audit (223 routes) had fallen behind Discord, which documented 11 more method+path routes since then, plus new fields and flags. Phase 1.5 of `ROADMAP.md` added them.

Audit source:

- Official repository: https://github.com/discord/discord-api-docs
- REST re-audit snapshot: `main` at `3714862` (2026-10-09); earlier object and gateway counts below are from the 2026-05-02 snapshot
- Parsed docs: every `.md`/`.mdx` page under `developers/` except `developers/change-log.mdx` for REST routes (the 2026-10-10 re-audit set also covered `resources`, `interactions`, `topics`, `events`, `monetization`, `activities`, `communities`, `components`, and `bots`), and `developers/events/gateway-events.mdx` for Gateway receive events
- Method: `ci/api_drift.py` (see "Method" below); the `api-drift` workflow re-runs it weekly against the latest docs

## Result

| Area | Status | Evidence |
| --- | --- | --- |
| REST route shapes | 234 / 234 mapped | Every unique method + path template in the official MDX `<Route method="...">...</Route>` entries maps to a `RestClient` wrapper: 214 through `Method::*` + path literals (or a same-function `let path = format!(..)`), 20 through path helpers listed with evidence in `ci/api-drift-allowlist.toml` (global commands, interaction callbacks, webhook execute/messages, follow-ups and `@original`, Slack/GitHub webhooks, identity profiles, invite target-user bulk actions). Reproduced by `ci/api_drift.py`. |
| Gateway send events | 7 / 7 mapped | Official Gateway send-event headings map to identify, resume, heartbeat, request-guild-members, request-soundboard-sounds, request-channel-info, voice-state, and presence payload helpers or commands. |
| Gateway receive events | 82 / 82 mapped | Official Gateway receive events (the "Receive Events" table and headings) map to `Event` decoder branches (79 dispatch names in `src/event/decode.rs`) or non-dispatch Gateway opcode handling for Hello, Reconnect, and Invalid Session. Reproduced by `ci/api_drift.py`. |
| Official object headings | 90 / 90 mapped or intentionally dynamic | Non-example official `Object` headings map to public typed structs/enums or documented `serde_json::Value` extension points for highly variable interaction/audit metadata objects. |
| Code coverage | 91.2% line coverage (CI gate: 90%) | `cargo llvm-cov --all-features --locked --summary-only` on main after phase 1.5 and the phase 8 tooling (2026-10-10). The 93.50% figure recorded for 2.0 predates the 2.1/2.2 additions. |
| Coveralls upload scope | Guarded | CI validates LCOV includes at least 30 `src` files, at least 20,000 line records, and required core files before Coveralls upload. |
| Live DAVE/MLS smoke | Verified for 2.0.0 only | The ignored live harness passed against a real voice session on 2026-05-02, driving MLS by hand (transitions only, no media). The runtime-managed DAVE path added after 3.0.0 is covered by hermetic two-member MLS tests and has not been run against live Discord yet. |
| Public docs coverage | Complete for crate public API | Rustdoc public-item coverage was previously driven to 100%; REST additions are documented through API guide and examples. |

## Method

The route and receive-event rows above are produced by `ci/api_drift.py` (Python standard library; full description under "Discord API Drift Report" in the [testing guide](../guide/testing-and-coverage.md)):

1. Docs side: extract every `<Route method="X">path</Route>`, strip markdown links and backslashes, drop query strings, and normalize `{param}` names to `{}`, so `{channel.id}` and `{channel_id}` compare equal. Event names come from the "Receive Events" table and level-4 headings, converted the way the docs specify (`Channel Create` → `CHANNEL_CREATE`).
2. Crate side: scan `src/http.rs`, `src/http/**/*.rs`, and `src/oauth2.rs` (test code excluded) for `Method::X` paired with a path literal, and `src/event/decode.rs` for decoder arms. Paths built by helper functions cannot be read as literals; each one is an allowlist entry with a reason, evidence strings that must exist in `src/`, and the wrapper functions that send it. CI checks that the allowlist matches the source on every PR.
3. Compare method + path templates in both directions. Routes Discord no longer documents are reported too, not only routes the crate lacks.

The first scripted run (2026-10-10, docs `3714862`) reproduced 234 / 234 and 79 / 79 dispatch events. In the other direction it found four crate routes that Discord removed from the docs on 2025-07-28 when apps lost the ability to create guilds: `POST /guilds`, `DELETE /guilds/{guild.id}`, `POST /guilds/templates/{template.code}`, and `POST /guilds/{guild.id}/mfa` (`create_guild`, `delete_guild`, `create_guild_from_template`, `modify_guild_mfa_level`, added in 2.1). They are not allowlisted, so the weekly report keeps them visible until 3.0 decides whether to deprecate or remove them.

## Boundary

`234 / 234` is a REST route-shape result, not a blanket claim that every Discord object field, gateway event edge case, Social SDK behavior, or undocumented rollout behavior has live integration coverage.

The broader API coverage claim for 2.2.0 is:

- Official REST routes are represented.
- Official Gateway send and receive event names are represented by typed payload helpers, runtime command helpers, event decoder branches, or opcode handling.
- Official non-example object headings are represented by public types or intentionally dynamic JSON fields where Discord documents flexible metadata shapes.
- Major interactions, webhook, voice, DAVE, cache, collectors, components, OAuth2, lobbies, monetization, soundboard, stickers, stage, invites, polls, subscriptions, entitlements, templates, and admin surfaces have typed wrappers or models where Discord documents them.
- Remaining risk is mainly semantic drift in Discord docs, object-field changes, and live-only behavior that cannot be proven by hermetic tests.

## Routes, Fields, and Flags Added in 3.0 (Phase 1.5)

Routes Discord documented after the 2026-05-02 audit:

- `PUT` / `DELETE /invites/{invite.code}/target-users/{user.id}` via `add_invite_target_user(...)` / `remove_invite_target_user(...)`
- `POST /invites/{invite.code}/target-users/bulk-add` and `/bulk-delete` via `bulk_add_invite_target_users(...)` / `bulk_delete_invite_target_users(...)` (max 1000 IDs)
- `PATCH` / `GET /applications/{app}/users/{user}/identities/{external_id}/profile` via `update_application_identity_profile(...)` / `get_application_identity_profile(...)`
- `GET /users/{user}/application-identities/{app}` via `get_user_application_identities(...)`
- `GET /applications/{app}/application-identities/{provider_type}/{external_id}` via `get_application_identities_by_external_id(...)`
- `POST /users/{user}/application-identities/{app}/{provider_type}/{external_id}/delete` via `delete_application_identity(...)`
- `PUT /lobbies` via `get_or_create_lobby(...)`; `POST` / `GET /lobbies/{lobby.id}/messages` via `send_lobby_message(...)` / `get_lobby_messages(...)`
- `POST /lobbies/{lobby.id}/members/@me/invites` and `/members/{user.id}/invites` via `create_lobby_channel_invite_for_self(...)` / `create_lobby_channel_invite_for_user(...)`
- `DELETE /users/@me/applications/{application.id}/role-connection` via `delete_current_user_application_role_connection(...)`

Fields and flags:

- `CurrentUserGuildsQuery::shard` — required since 2026-09-15 for large bot sharding
- `CreateChannelInvite::target_user_ids`
- `file_types` on `FileUploadBuilder` and `CommandOptionBuilder` (`ATTACHMENT` options)
- `Embed::components` / `Embed::flags` and `Embed::is_component_embed()` for Component Embed link previews; `discordrs::request::ComponentEmbed` generates the website side (the `discord:component-embed` document, `<script>`/`<link>` tags, and the spec's limits)
- `ChannelFlags` (`CHANNEL_OBFUSCATED`, `IS_SPOILER_CHANNEL`, ...), `Channel::is_obfuscated()`, and IDENTIFY `capabilities` (`GatewayCapabilities`, `ClientBuilder::gateway_capabilities(...)`) for channel obfuscation, mandatory from 2026-11-16

## Routes and Options Added in 2.1

- `POST /guilds` via `create_guild(...)` with typed `CreateGuild`
- `DELETE /guilds/{guild.id}` via `delete_guild(...)`
- `POST /guilds/templates/{template.code}` via `create_guild_from_template(...)` with typed `CreateGuildFromTemplate`
- `POST /guilds/{guild.id}/mfa` via `modify_guild_mfa_level(...)` with typed `GuildMfaLevel`
- `POST /oauth2/token/revoke` via `OAuth2Client::revoke_token(...)`
- `with_response=true` interaction callback via `create_interaction_response_with_result(...)`, returning the typed `InteractionCallbackResult` resource
- Scheduled-event query parameters: `with_user_count` through `get_guild_scheduled_events_with_query(...)`, and `limit` / `with_member` / `before` / `after` through `get_guild_scheduled_event_users_with_query(...)`
- `X-Audit-Log-Reason` on all mutating requests via `RestClient::with_reason(...)` (percent-encoded like discord.js's `encodeURIComponent`)
- Message forwarding: `MessageReferenceType`, `MessageReference::forward(...)`, and `forward_message(...)`

## Latest REST Gaps Closed (2.0.x)

- `GET /gateway`
- `GET /oauth2/applications/@me`
- `GET /oauth2/@me`
- `POST /lobbies/{lobby.id}/members/bulk`
- `PUT /lobbies/{lobby.id}/messages/{message.id}/moderation-metadata`
- Legacy pin routes: `PUT` and `DELETE /channels/{channel.id}/pins/{message.id}`
- `GET /guilds/templates/{template.code}`
- `PATCH /guilds/{guild.id}/members/@me/nick`
- Message Resource object gaps: `AllowedMentions` and `ReactionCountDetails`
