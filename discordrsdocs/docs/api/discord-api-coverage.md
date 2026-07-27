# Discord API Coverage Audit

This page records the current coverage claim for discord.rs 2.1.0 against the official Discord API documentation.

Audit source:

- Official repository: https://github.com/discord/discord-api-docs
- Snapshot checked: `main` on 2026-05-02
- Parsed docs: `developers/resources`, `developers/interactions`, `developers/topics`, `developers/events`, `developers/monetization`, `developers/activities`, `developers/communities`, `developers/components`, and `developers/bots`

## Result

| Area | Status | Evidence |
| --- | --- | --- |
| REST route shapes | 223 / 223 mapped | Official MDX `<Route method="...">...</Route>` entries all map to a `RestClient` wrapper, helper path, or tokenized interaction/webhook helper. |
| Gateway send events | 7 / 7 mapped | Official Gateway send-event headings map to identify, resume, heartbeat, request-guild-members, request-soundboard-sounds, request-channel-info, voice-state, and presence payload helpers or commands. |
| Gateway receive events | 82 / 82 mapped | Official Gateway receive-event headings map to `Event` decoder branches or non-dispatch Gateway opcode handling for Hello, Reconnect, and Invalid Session. |
| Official object headings | 90 / 90 mapped or intentionally dynamic | Non-example official `Object` headings map to public typed structs/enums or documented `serde_json::Value` extension points for highly variable interaction/audit metadata objects. |
| Code coverage | 93.50% line coverage | `cargo llvm-cov --all-features --locked --summary-only` reported total line coverage of 93.50%. |
| Coveralls upload scope | Guarded | CI validates LCOV includes at least 30 `src` files, at least 20,000 line records, and required core files before Coveralls upload. |
| Live DAVE/MLS smoke | Verified | The ignored live Discord voice test passed against a real voice session before this audit was recorded. |
| Public docs coverage | Complete for crate public API | Rustdoc public-item coverage was previously driven to 100%; REST additions are documented through API guide and examples. |

## Boundary

`223 / 223` is a REST route-shape result, not a blanket claim that every Discord object field, gateway event edge case, Social SDK behavior, or undocumented rollout behavior has live integration coverage.

The broader API coverage claim for 2.1.0 is:

- Official REST routes are represented.
- Official Gateway send and receive event names are represented by typed payload helpers, runtime command helpers, event decoder branches, or opcode handling.
- Official non-example object headings are represented by public types or intentionally dynamic JSON fields where Discord documents flexible metadata shapes.
- Major interactions, webhook, voice, DAVE, cache, collectors, components, OAuth2, lobbies, monetization, soundboard, stickers, stage, invites, polls, subscriptions, entitlements, templates, and admin surfaces have typed wrappers or models where Discord documents them.
- Remaining risk is mainly semantic drift in Discord docs, object-field changes, and live-only behavior that cannot be proven by hermetic tests.

## Routes and Options Added in 2.1.0

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
