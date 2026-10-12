# HTTP and Helpers API

## `RestClient`

`RestClient` is the primary Discord REST v10 surface. It keeps shared route/global rate-limit state and also keeps `DiscordHttpClient` as a compatibility alias. Since `2.1` the client is `Clone`; clones share rate-limit state, the connection pool, and the application id.

Common operations include:

- typed message create/edit/get (`request::CreateMessage`, `request::EditMessage`)
- typed guild/channel/member/role lookups
- typed application command overwrite
- typed Auto Moderation rule reads and writes through `get_auto_moderation_rules`, `create_auto_moderation_rule`, and `modify_auto_moderation_rule`
- typed guild administration helpers for bulk bans, guild count fetches, guild ban pagination, member pagination, single-role lookup, audit logs, guild preview, prune count/result, vanity URL, incident actions, and voice regions
- typed guild channel-position updates, OAuth2 guild-member joins, role member-count reads, and public widget image downloads
- typed Stage Instance create/modify request bodies
- typed emoji helpers for guild and application emoji reads/writes
- typed scheduled-event recurrence/entity metadata, typed create/modify request helpers, and query options through `get_guild_scheduled_events_with_query(...)` (`with_user_count`) and `get_guild_scheduled_event_users_with_query(...)` (`limit`, `with_member`, `before`, `after`)
- typed current-application reads/edits and application role-connection metadata helpers
- typed application Activity Instance lookup helper
- typed Gateway URL, OAuth2 current bot application, and OAuth2 current authorization metadata helpers
- typed application command permission reads, plus OAuth2 Bearer-token permission edits
- typed current-user guild list pagination/counts, current-member modify, OAuth2 user connection and application role connection helpers, webhook-by-token lookup, invite target-user CSV helpers, guild-specific voice region helpers, and voice-state REST reads/writes
- typed lobby create/read/update/delete, member, channel-linking, and moderation metadata helpers
- typed channel invite, voice-status, and permission overwrite helpers
- typed guild message search and current channel-pin listing helpers
- legacy channel pin/unpin route helpers for Discord's `/channels/{channel.id}/pins/{message.id}` aliases
- typed sticker-pack lookup and guild sticker write helpers
- typed Webhook Resource creation/modification request bodies, query-aware execution/message routes, and Slack/GitHub-compatible execution helpers
- interaction responses and follow-up webhook helpers
- Create Group DM plus Group DM recipient add/remove helpers for OAuth2 `gdm.join` flows

Every method takes a concrete request type and returns a typed model (3.0). For a route Discord added before discord.rs has a method, or for an experiment, use `RestClient::request(RawRequest)`; see [Raw requests](#raw-requests-rawrequest-30).

## REST Surface (3.0)

3.0 settles one name and one signature per Discord operation:

- **One canonical method per operation.** The 2.x pairs (`get_guild_emojis` returning `Vec<Value>` next to `get_guild_emojis_typed`) and aliases (`_from_request`, `_with_body`, `_config`, `update_guild`, `create_role`, `update_role`) are merged into the canonical name, which takes the concrete request type and returns the typed model. Renames that map 1:1 stay as `#[deprecated]` forwards for one release; generic `B: Serialize` variants are gone. `_with_query` and `_with_files` variants stay. The full table is [below](#migrating-rest-names-from-2x-30).
- **Concrete request bodies.** Message endpoints take `discordrs::request::{CreateMessage, EditMessage, ExecuteWebhook, EditWebhookMessage}`. New bodies for the other domains live in `discordrs::request` too: `ModifyChannel` (for `update_channel`), `StartThreadFromMessage`, `StartThreadWithoutMessage`, `StartForumThread` + `ForumThreadMessage` (new `create_forum_thread(_with_files)`), `CreateAutoModerationRule` / `ModifyAutoModerationRule`, `CreateGuildEmoji` / `ModifyGuildEmoji` / `CreateApplicationEmoji` / `ModifyApplicationEmoji`, `SendSoundboardSound` / `CreateGuildSoundboardSound` / `ModifyGuildSoundboardSound`, `CreateGuildTemplate` / `ModifyGuildTemplate`, `CreateGuildScheduledEvent` / `ModifyGuildScheduledEvent`, and `EditApplicationCommand`. Create bodies use `Option` fields; modify bodies use `Patch` / `NullablePatch`, where the `clear_*` setters send `null`. The 2.x bodies still in `discordrs::model` (`ModifyGuild`, `CreateGuildRole`, `CreateWebhook`, ...) keep working and move to `request` later.
- **Documented exceptions.** `execute_slack_compatible_webhook` / `execute_github_compatible_webhook` take `&Value` (Slack- and GitHub-format payloads), and `update_lobby_message_moderation_metadata` takes the application-defined `HashMap<String, String>`.
- **Bodies are encoded once.** A body is serialized straight to bytes with `serde_json::to_vec` (no `serde_json::Value` tree) and every attempt, including retries, sends the same bytes. Object keys follow the request type's field order.
- **Default `allowed_mentions`** (`RestClientBuilder::default_allowed_mentions`, `with_default_allowed_mentions`) is written while encoding every message body that leaves it unset: `create_message(_with_files)`, `edit_message(_with_files)`, `execute_webhook*`, `edit_webhook_message*`, the first message of `create_forum_thread`, the Components V2 channel helpers (`send_container_message`, `send_to_channel`, `send_components_v2`, `edit_message_with_container`), and on the interaction side the message callbacks (types 4 and 7; not `RawInteractionResponse`), follow-ups, and `@original` / follow-up edits. An explicit value, or an explicit `null` on an edit, wins. The legacy `create_interaction_response_typed` still writes it into the JSON `data` of types 4 and 7. 2.x did not apply it to `edit_webhook_message` or the channel helpers.
- **Nonce replay** comes from the typed request (`CreateMessage::nonce_replay()`, set with `.enforce_nonce("...")`), not from reading the JSON back, and only for Create Message.
- **Voice messages** (`IS_VOICE_MESSAGE`) are checked locally before upload: exactly one `audio/*` file.
- `modify_guild_member` returns the updated `Member` (Discord's 200 body) instead of `()`; `Member::edit` / `timeout` / `remove_timeout` follow.

## Raw Requests (`RawRequest`, 3.0)

`RestClient::request::<T>(RawRequest)` sends any method and path through the same transport as the typed methods: route gates, rate limits, `with_reason`, retries, and token redaction in errors and logs. `T` is any `DeserializeOwned` type (`serde_json::Value` for untyped JSON). The body is always decoded, so `()` accepts only an empty or `null` body; to ignore the response use `request_discard(RawRequest)`, which never decodes it (a mutation Discord applied is not reported as failed because its body did not match), or `serde::de::IgnoredAny` to require valid JSON of any shape.

```rust
use discordrs::RawRequest;
use serde_json::{json, Value};

let channel: Value = rest.request(RawRequest::get("/channels/123")).await?;
let created: Value = rest
    .request(RawRequest::post("/channels/123/messages").json(&json!({ "content": "hi" })))
    .await?;
let connections: Vec<Value> = rest
    .request(RawRequest::get("/users/@me/connections").bearer(&access_token))
    .await?;
rest.request_discard(RawRequest::put("/channels/123/pins/456")).await?;
```

`json(&body)` encodes once when it is called (an encoding error is returned by `request` before anything is sent), `json_bytes(bytes)` sends pre-encoded JSON, and `multipart(&payload, &files)` sends `payload_json` plus `files[n]`. Two rules are stricter than for typed methods, because the client cannot tell what a raw request does: a POST or PATCH is never replayed after an ambiguous failure, even with `enforce_nonce`, unless `retry_non_idempotent_requests(true)` is set; and the `Authorization` header follows the path (the bot token, except on `/webhooks/{id}/{token}…` and `/interactions/…`) unless `bearer(...)` or `without_authorization()` chooses. `Debug` shows only the method and the token-redacted path.

This replaces the 2.x `request(method, path, Option<&Value>) -> Value`, `request_typed`, `request_multipart`, and `request_typed_multipart`.

## Route Manifest and Request Goldens (3.0)

Every route the client sends is one row of `src/http/routes.rs`: a stable route ID, the method, the path template in the docs' spelling, the authorization (`bot`, `bearer`, `token`, `none`), the query parameters, the request and response types, and the methods that send it. Endpoints pass their route ID to the transport, which takes the method and the `Authorization` header from the row. `ci/api_drift.py` reads the same rows for the weekly Discord drift report, and its `--self-check` (every PR) checks that every endpoint method is listed and sends its route. `src/http/goldens/rest_routes.golden` records the exact request of every route (method, escaped path, query, authorization, body bytes, multipart parts); `cargo test` compares against it. See "Discord API Drift Report" in [Testing and Coverage](../guide/testing-and-coverage.md).

`1.2.1` hardens the REST layer:

- invite codes are validated before `get_invite`, `get_invite_with_options`, and `delete_invite` build bot-authorized paths
- generated query strings are percent-encoded
- request body serialization failures return `DiscordError::Json` instead of panicking
- repeated HTTP 429 responses are retried up to a bounded limit before `DiscordError::RateLimit`

`2.1` unified the transport into one retry loop:

- the per-route serialization gate now covers all requests (previously JSON requests bypassed it, allowing same-bucket 429 races), and the route-gate map is garbage-collected
- 5xx responses and transient transport errors retry with backoff (0.5s/1s/2s)
- 429 handling reads the `Retry-After` header and the `x-ratelimit-scope`/`x-ratelimit-global` headers, so Cloudflare-level bans with HTML bodies back off correctly
- `HttpError` exposes `is_timeout()`, `is_connect()`, `is_body()`, and `is_retryable()`, and `DiscordError::is_retryable_transport()` classifies retry-worthy failures

`3.0` makes retries and rate-limit identity safe:

- 5xx responses, timeouts, and failed response reads are ambiguous — Discord may already have applied the request — so they are only retried for requests that are safe to repeat: GET, HEAD, OPTIONS, PUT, DELETE, and `POST /channels/{id}/messages` with both `nonce` and `enforce_nonce: true` (the only route Discord deduplicates), the latter only within 60 seconds of the first attempt. POST and PATCH otherwise surface the error instead of risking a duplicate message, channel, or invite. 429s are always retried. `RestClientBuilder::retry_non_idempotent_requests(true)` restores replaying every method.
- The managed transport (`RestClient::new`, `RestClientBuilder::build`) does not follow redirects, so a connect failure proves nothing was sent and is retried for every method. A client supplied through `use_client(...)` may follow redirects, so POST/PATCH are not retried after connect failures there.
- A learned `x-ratelimit-bucket` hash is combined with the route's major parameters (channel, guild, webhook, or interaction token) before it blocks requests, because Discord's bucket hash excludes them; two channels that report the same hash no longer block each other. Requests are serialized per resolved bucket, and requests that queued before the bucket was known move to the bucket's gate.
- Route keys replace webhook and interaction tokens with a digest; transport errors and API/proxy error text are scrubbed of the request's token, so tokens no longer appear in REST logs, `RateLimitInfo::route`, or `DiscordError::{RateLimit, Api, Http}`.
- Malformed wait values (`inf`, `NaN`, negative, or huge `Retry-After`/`x-ratelimit-reset-after`) are clamped instead of panicking.

## Discord API Additions (3.0)

- **Large bot sharding:** `CurrentUserGuildsQuery { shard: Some(n), .. }` sends the `shard` query param that `GET /users/@me/guilds` requires since 2026-09-15 for apps using large bot sharding (values `0..max_concurrency`; not the gateway shard id).
- **Invite target users:** `CreateChannelInvite::target_user_ids` (up to 1000, mutually exclusive with a CSV upload), `add_invite_target_user(code, user_id)`, `remove_invite_target_user(code, user_id)`, `bulk_add_invite_target_users(code, &ids)`, and `bulk_delete_invite_target_users(code, &ids)` (at most 1000 IDs, checked locally).
- **Game Stats Widgets:** `update_application_identity_profile(user_id, external_id, &body)` (`Ok(Some(profile))` on the first write, `Ok(None)` on later `204`s; `body.data` replaces the stored stats; limits checked by `UpdateApplicationIdentityProfile::validate()` before sending), `get_application_identity_profile(...)`, `get_user_application_identities(user_id)`, `get_application_identities_by_external_id(provider_type, external_id, provider_id)`, and `delete_application_identity(...)`. External IDs are percent-encoded as single path segments; `.`/`..`/empty are rejected.
- **Social SDK lobbies and role connections:** see the Lobby API page for `get_or_create_lobby`, lobby messages, and lobby channel invites; `delete_current_user_application_role_connection(bearer_token)`.

## REST Client Configuration (`RestClient::builder`, 2.2.0)

`RestClient::builder(token, application_id)` returns a `RestClientBuilder` mirroring discord.js's `RESTOptions`; `RestClient::new(...)` keeps the zero-configuration behavior. `application_id` is an `Option<ApplicationId>` in both: `None` until it is known (application-scoped routes fail until `set_application_id(...)`, which the gateway client calls from `READY`), and `application_id()` returns it the same way:

```rust
use std::sync::Arc;
use std::time::Duration;

use discordrs::{AllowedMentions, RestClient};

let rest = RestClient::builder("bot-token", None)
    .api_version(10)                     // or .api_base("https://discord.com/api/v10")
    .connect_timeout(Duration::from_secs(5))
    .request_timeout(Duration::from_secs(20))
    .user_agent("my-bot/1.0")
    .proxy("http://localhost:8888")
    .rate_limit_callback(Arc::new(|info| {
        eprintln!("429 on {} — retry after {}s (global: {})", info.route, info.retry_after, info.global);
    }))
    .default_allowed_mentions(AllowedMentions::default())
    .build()?;
```

- `use_client(reqwest::Client)` supplies a fully custom client, superseding `connect_timeout`, `request_timeout`, and `proxy`.
- `rate_limit_callback(...)` fires on every 429 with `RateLimitInfo { route, retry_after, global }` — the discord.js `rateLimited` event equivalent. `route` never contains a webhook or interaction token.
- `retry_non_idempotent_requests(true)` (3.0, default `false`) lets POST and PATCH be replayed after a 5xx, timeout, or lost response. Enable it only when a duplicated mutation is acceptable for every request the client makes.
- `default_allowed_mentions(...)` is injected into outgoing message payloads (`create_message`, `edit_message`, `execute_webhook`, interaction responses) only when the payload does not set `allowed_mentions` itself. The gateway-side counterpart is `ClientBuilder::default_allowed_mentions(...)`.

## Interaction Response API (`InteractionResponder`, 2.2.0; response state 3.0)

`discordrs::response::InteractionResponder` puts the discord.js responder methods on every responding-capable interaction variant (and on `AppContext`). It is in `discordrs::prelude`; import the trait so the methods resolve:

```rust
use discordrs::prelude::*; // or: use discordrs::response::InteractionResponder;

// command: ChatInputCommandInteraction (also works on context-menu,
// component, and modal-submit interactions)
command.reply(&rest, "hi").await?;
command.reply_ephemeral(&rest, "only you can see this").await?;

command.defer(&rest).await?;
command.edit_reply(&rest, "done!").await?;
let followup = command.follow_up(&rest, "extra detail").await?;
```

The full surface: `reply`, `reply_ephemeral`, `reply_with_result`, `reply_with_files`, `defer`, `defer_ephemeral`, `edit_reply`, `edit_reply_with_files`, `fetch_reply`, `delete_reply`, `follow_up`, `follow_up_ephemeral`, `follow_up_with_files`, `edit_follow_up`, `delete_follow_up`, `show_modal` (a `Modal` or `ModalBuilder`), plus `defer_update`/`update_message` on component and modal-submit interactions and `respond_autocomplete(Vec<ApplicationCommandOptionChoice>)` on autocomplete interactions. `InteractionContextData::create_response(&rest, &InteractionResponse)` sends any other callback (for example `InteractionResponse::LaunchActivity` or a `Raw` callback) through the same state.

Every async trait method returns a `Send` future, also through a generic `R: InteractionResponder`, so a responder call can be moved into `tokio::spawn`:

```rust
fn spawn_follow_up<R>(interaction: R, rest: RestClient) -> tokio::task::JoinHandle<()>
where
    R: InteractionResponder + Send + 'static,
{
    tokio::spawn(async move {
        let _ = interaction.follow_up(&rest, "background work finished").await;
    })
}
```

### Request bodies (3.0)

Each method takes the request type of the endpoint it calls; all of them convert from `&str` and `String`:

| Method | Body | Notes |
|---|---|---|
| `reply`, `reply_ephemeral`, `reply_with_result`, `reply_with_files` | `request::InteractionMessage` | the `data` of callback type 4: content, embeds, components, flags (`ephemeral(true)`), allowed_mentions, attachments, poll, tts |
| `update_message` | `request::UpdateMessage` | callback type 7, patch semantics: an omitted field keeps its value, `clear_content()` sends `content: null` |
| `follow_up`, `follow_up_ephemeral`, `follow_up_with_files` | `request::CreateFollowup` | `POST /webhooks/{app}/{token}` |
| `edit_reply`, `edit_follow_up`, `edit_reply_with_files` | `request::EditInteractionResponse` | patch semantics, `attachments` lists every attachment the message keeps |

The REST client's default `allowed_mentions` (`RestClientBuilder::default_allowed_mentions`) fills every one of these bodies that leaves `allowed_mentions` unset; an explicit value (or an explicit `null` on a patch) wins. A deferral, modal, or autocomplete result gets nothing. The callback body is prepared once before the response slot is claimed (encoding, empty file names, and file content types checked with the transport's own parser), so a local error fails with `DiscordError::Model`, sends nothing, and leaves the state untouched.

A `request::CreateMessage` converts with `InteractionMessage::try_from(message)?` (likewise `CreateFollowup`, `UpdateMessage`, `EditInteractionResponse`). The conversion fails when the message sets a field that only channel messages accept (`message_reference`, `sticker_ids`, `nonce`, `shared_client_theme`), because Discord's interaction bodies have no such fields and silently dropping them would change the message. The helpers that take a `CreateMessage` fail the same way before sending anything. The 2.x `InteractionReplyData` is a deprecated alias of `InteractionMessage`.

### Shared response state

Each parsed interaction owns one `InteractionResponseState`, shared by every clone (handler, collector, spawned task). Its phase (`response_phase()`, `InteractionResponsePhase`) is one of:

| Phase | Meaning | Initial response | `follow_up` / `edit_reply` / `delete_reply` |
|---|---|---|---|
| `Unacknowledged` | Nothing sent, or Discord rejected the last attempt | allowed | fail locally |
| `InFlight` | An initial response is being sent | fails locally | **wait** for the outcome |
| `Deferred` / `Replied` | Discord accepted the response | fails locally | allowed |
| `Acknowledged` | Discord holds an initial response, but its kind is unknown | fails locally | allowed |
| `Uncertain` | The outcome was lost (timeout, 5xx, failed response read, or the sending future was dropped) | allowed (safe retry) | passed to Discord |

- The slot moves to `Deferred`/`Replied` only after Discord accepts the callback, so a follow-up from another clone can no longer race ahead of the reply.
- A 4xx, a 429, local validation, or a connection that was never established rolls back to `Unacknowledged`, so the reply can be retried.
- A timeout, 5xx, failed response read, or cancellation leaves `Uncertain`, which remembers every kind (deferred or reply) that may have landed. Retrying the initial response with any kind is safe: if an earlier attempt landed, Discord answers error 40060 ("already acknowledged") and the state becomes acknowledged without a second response. 40060 does not say *which* attempt landed, so the kind is known only if all uncertain attempts had the same kind; otherwise the phase is `Acknowledged` (kind unknown). A successful `edit_reply` (→ `Replied`), `fetch_reply`, or `delete_reply` also settles it, even when it completes while a retry is in flight.
- `is_replied()`, `is_deferred()`, and `is_acknowledged()` report only what Discord confirmed; they are false while `InFlight` or `Uncertain`, and only `is_acknowledged()` is true in `Acknowledged`.
- `edit_reply` on a deferred interaction promotes it to replied, as in discord.js.
- The typed HTTP interactions endpoint competes for the same slot: exactly one of the inline response and a REST response owns the initial response (see "Response State" in [Interactions Endpoint](./interactions-endpoint.md)).

Waiting for an in-flight response has no timeout of its own: it ends when the future sending the initial response completes or is dropped. Do not keep a partially polled initial-response future alive in the same task while awaiting a follow-up (for example after a `select!` branch lost); drop it or keep polling it, otherwise the follow-up waits forever.

The state is local to the process. A context rebuilt from storage with serde starts `Unacknowledged`; send follow-ups for it with the `RestClient` methods (`create_followup_message_with_application_id`, ...) instead.

"A connect failure or a final 4xx means the callback was never delivered" holds for clients from `RestClient::new` and `RestClientBuilder::build`, which never follow redirects or replay a callback. A client passed to `RestClientBuilder::use_client` that follows redirects, or `retry_non_idempotent_requests(true)`, breaks that assumption: the state can then read `Unacknowledged` for a delivered response, until the next initial response gets 40060 and settles it.

### Helper functions: state-aware and stateless

The `discordrs::helpers` functions that take an `InteractionContextData` now delegate to the methods above, so they share the response state. Those with a direct responder equivalent are deprecated in 3.0: `defer_interaction`, `defer_update_interaction`, `update_interaction_message`, `respond_with_modal_typed`, `respond_with_autocomplete_choices`, `followup_message`, `get_original_response`, `edit_original_response`, `delete_original_response`, `edit_followup_response`, and `delete_followup_response`. `respond_with_message` (use `reply`/`reply_ephemeral`), `respond_to_interaction` (`create_response`), and `launch_activity` stay without a deprecation for now.

The helpers that take only an interaction id and token are **stateless**: `respond_with_container`, `respond_component_with_container`, `respond_modal_with_container`, `respond_with_components_v2`, `respond_component_with_components_v2`, `update_component_with_container`, `respond_with_modal`, `followup_with_container`, and `defer_and_followup_container`. They cannot see a parsed interaction's state, so nothing stops a double response and the interaction does not learn what they sent. Use them with raw handlers (`InteractionContext`) or when only the id and token are available, and do not mix them with responder methods on one interaction.

## Entity Convenience Methods (`model_ext`, 2.2.0)

`discordrs::model_ext` adds discord.js-style inherent methods to the typed models — no trait import needed:

```rust
// Message
let reply = message.reply(&rest, "On it!").await?;
reply.react(&rest, "✅").await?;
message.forward_to(&rest, announce_channel_id).await?;
let thread = message.start_thread(&rest, "follow-up").await?;

// Member (guild id passed explicitly)
member.timeout(&rest, guild_id, "2026-08-01T00:00:00+00:00").await?;
member.kick_with_reason(&rest, guild_id, "spam").await?;

// Guild, Channel, User
let channel = guild.create_channel(&rest, &body).await?;
channel.send(&rest, "Welcome!").await?;
user.dm(&rest, "thanks for the report").await?;
```

Coverage: `message.reply/edit/edit_with/delete/react/unreact/pin/unpin/crosspost/forward_to/start_thread/link`, `member.kick/ban/timeout/remove_timeout/add_role/remove_role/edit/display_name` (plus `_with_reason` variants), `guild.edit/delete/leave/fetch_channels/create_channel/fetch_member/fetch_roles/create_role/ban/unban/kick/set_mfa_level/icon_url/banner_url`, `channel.send/send_message/edit/delete/create_invite/mention/is_text_based/is_voice_based/is_thread`, `role.edit/delete/mention`, and `user.create_dm/dm/tag/mention/avatar_url/default_avatar_url/display_avatar_url` — with CDN URL helpers that handle animated hashes and default avatars.

## Audit-Log Reasons

`RestClient::with_reason(...)` returns a cheap scoped clone that sends `X-Audit-Log-Reason` (percent-encoded like discord.js's `encodeURIComponent`) with every mutating request it makes, so bans, kicks, and edits show a reason in the guild audit log. The clone shares rate-limit state and the connection pool; create one per call site:

```rust
rest.with_reason("spam")
    .remove_guild_member(guild_id, user_id)
    .await?;
```

## Guild Lifecycle Routes

`2.1` added typed helpers for guild creation and deletion:

```rust
use discordrs::model::{CreateGuild, CreateGuildFromTemplate};

// POST /guilds — only usable by bots in fewer than 10 guilds.
let guild = rest
    .create_guild(&CreateGuild {
        name: "Support HQ".to_string(),
        ..Default::default()
    })
    .await?;

// POST /guilds/templates/{code}
let from_template = rest
    .create_guild_from_template(
        "template-code",
        &CreateGuildFromTemplate {
            name: "Support HQ 2".to_string(),
            icon: None,
        },
    )
    .await?;

// POST /guilds/{id}/mfa — returns the updated GuildMfaLevel.
let level = rest.modify_guild_mfa_level(guild.id, discordrs::MfaLevel::ELEVATED).await?;

// DELETE /guilds/{id} — the bot must own the guild.
rest.delete_guild(from_template.id).await?;
```

## Message Forwarding

`MessageReference::reply(...)` and `MessageReference::forward(...)` build typed references (`MessageReferenceType::DEFAULT` / `FORWARD`), and `forward_message(...)` forwards in one call — the equivalent of discord.js's `message.forward(channel)`:

```rust
use discordrs::request::CreateMessage;

// Reply in the same channel.
rest.create_message(
    channel_id,
    &CreateMessage::new().content("On it!").reply_to(message_id),
)
.await?;

// Forward into another channel; Discord attaches the source message
// as a `message_snapshots` entry.
let forwarded = rest
    .forward_message(channel_id, message_id, announce_channel_id)
    .await?;
```

## Interaction Callbacks with `with_response=true`

`create_interaction_response_with_result(...)` sends the interaction callback with `with_response=true` and returns the typed `InteractionCallbackResult` resource — mirroring discord.js's `withResponse: true` — so the created message (or activity instance) is available immediately:

```rust
use discordrs::request::InteractionResponse;

let result = rest
    .create_interaction_response_with_result(
        interaction_id,
        interaction_token,
        &InteractionResponse::reply("Done"),
    )
    .await?;

if let Some(message) = result.resource.and_then(|resource| resource.message) {
    println!("created message: {}", message.id);
}
```

`create_interaction_response(...)` is the fire-and-forget variant and `create_interaction_response_with_files(...)` uploads files with the callback; all three take the same `request::InteractionResponse` the HTTP endpoint returns. They are stateless: prefer the `InteractionResponder` methods for a parsed interaction. The 2.x `create_interaction_response_typed(...)` (an `InteractionCallbackResponse` with JSON `data`) stays until the 3.0 REST names are consolidated. The follow-up and `@original` endpoints take `request::CreateFollowup` / `request::EditInteractionResponse`.

## Application Resource Helpers

Current application management is typed through `Application`, `ModifyCurrentApplication`, `ApplicationInstallParams`, and `ApplicationIntegrationTypeConfig`:

```rust
use discordrs::{ApplicationInstallParams, ModifyCurrentApplication, Permissions};

let application = rest.get_current_application().await?;
println!("editing {}", application.name);

let updated = rest
    .edit_current_application(&ModifyCurrentApplication {
        description: Some("Support utilities".to_string()),
        install_params: Some(ApplicationInstallParams {
            scopes: vec!["bot".to_string(), "applications.commands".to_string()],
            permissions: Permissions::from_bits(2048),
        }),
        tags: Some(vec!["utility".to_string()]),
        ..ModifyCurrentApplication::default()
    })
    .await?;
```

The generic `edit_current_application(...)` helper remains available for fields Discord adds before discord.rs grows a typed model.

## Webhook Resource Helpers

Webhook management has typed request bodies for creation and modification:

```rust
use discordrs::{CreateWebhook, ModifyWebhook, ModifyWebhookWithToken};

let webhook = rest
    .create_webhook(
        channel_id,
        &CreateWebhook {
            name: "deployments".to_string(),
            avatar: None,
        },
    )
    .await?;
let webhook_id = webhook.id.clone().expect("webhook id");
let webhook_token = webhook.token.as_deref().expect("webhook token");

rest.modify_webhook(
    webhook_id,
    &ModifyWebhook {
        name: Some("ops".to_string()),
        avatar: Some(None),
        channel_id: Some(archive_channel_id),
    },
)
.await?;

rest.modify_webhook_with_token(
    webhook_id,
    webhook_token,
    &ModifyWebhookWithToken {
        name: Some("public".to_string()),
        avatar: None,
    },
)
.await?;
```

Webhook execution takes `request::ExecuteWebhook` and supports Discord's `wait`, `thread_id`, and `with_components` query parameters. `execute_webhook` sends `wait=true` and returns the `Message`; `execute_webhook_with_query` returns `Option<Message>` (`None` for Discord's 204 without `wait`). Slack-compatible and GitHub-compatible execution helpers use Discord's documented compatibility endpoints and keep an untyped `&Value` body, because those payloads follow Slack's and GitHub's formats, not Discord's:

```rust
use discordrs::WebhookExecuteQuery;

let query = WebhookExecuteQuery {
    wait: Some(false),
    thread_id: Some(thread_id),
    with_components: Some(true),
};

// `wait: false`: Discord answers 204, so the result is `None`.
let sent: Option<Message> = rest
    .execute_webhook_with_query(
        webhook_id,
        token,
        &query,
        &discordrs::request::ExecuteWebhook::new().content("deployment finished"),
    )
    .await?;

rest.execute_slack_compatible_webhook(
    webhook_id,
    token,
    &WebhookExecuteQuery {
        wait: Some(true),
        thread_id: Some(thread_id),
        with_components: None,
    },
    &serde_json::json!({ "text": "deployment finished" }),
)
.await?;
```

Token-authenticated webhook routes validate token path segments before sending a request.

Webhook message helpers expose Discord's `thread_id` and `with_components` query options when they apply:

```rust
use discordrs::WebhookMessageQuery;

let message_query = WebhookMessageQuery {
    thread_id: Some(thread_id),
    with_components: Some(true),
};

let message = rest
    .get_webhook_message_with_query(webhook_id, token, message_id, &message_query)
    .await?;

rest.edit_webhook_message_with_query(
    webhook_id,
    token,
    message.id.as_str(),
    &message_query,
    &discordrs::request::EditWebhookMessage::new().content("edited"),
)
.await?;

rest.delete_webhook_message_with_query(webhook_id, token, message.id.as_str(), &message_query)
    .await?;
```

## Application Command Permissions

Discord exposes application command permission writes as an OAuth2 flow. Reads use the configured bot token:

- `get_guild_application_command_permissions(guild_id)`
- `get_application_command_permissions(guild_id, command_id)`

Writes require an access token with Discord's `applications.commands.permissions.update` scope:

```rust
use discordrs::{ApplicationCommandPermission, EditApplicationCommandPermissions};

let body = EditApplicationCommandPermissions::new([
    ApplicationCommandPermission::role(role_id, true),
    // `@everyone` (the guild ID) and "all channels" (guild ID - 1).
    ApplicationCommandPermission::everyone(guild_id, false),
    ApplicationCommandPermission::all_channels(guild_id, true),
]);

let updated = rest
    .edit_application_command_permissions(oauth_access_token, guild_id, command_id, &body)
    .await?;
```

`edit_application_command_permissions(...)` sends `Authorization: Bearer ...` for this route instead of `Authorization: Bot ...`.

## OAuth2 Current User Helpers

Discord's user connection and linked-role endpoints require user OAuth2 scopes. These helpers take an explicit Bearer token so bot-token REST calls and user-token REST calls do not get mixed accidentally:

- `get_current_user_connections(bearer_token)`
- `get_current_user_guilds_with_query(query)`
- `get_current_user_guild_member(bearer_token, guild_id)`
- `get_current_user_application_role_connection(bearer_token)`
- `update_current_user_application_role_connection(bearer_token, body)`

`ModifyCurrentUser` is separate and uses normal bot authorization through `modify_current_user(...)`.

Current-user guild lists are available with Discord's documented pagination and count flags through `CurrentUserGuildsQuery`:

```rust
use discordrs::CurrentUserGuildsQuery;

let guilds = rest
    .get_current_user_guilds_with_query(&CurrentUserGuildsQuery {
        after: Some(last_seen_guild_id),
        limit: Some(50),
        with_counts: Some(true),
        ..Default::default()
    })
    .await?;
```

## Guild Administration Helpers

Guild fetches support Discord's optional approximate member and presence counts through `GetGuildQuery`:

```rust
use discordrs::GetGuildQuery;

let guild = rest
    .get_guild_with_query(
        guild_id,
        &GetGuildQuery {
            with_counts: Some(true),
        },
    )
    .await?;
```

Guild ban lists support Discord's `before` and `after` pagination through `GuildBansQuery`:

```rust
use discordrs::GuildBansQuery;

let bans = rest
    .get_guild_bans_with_query(
        guild_id,
        &GuildBansQuery {
            limit: Some(100),
            after: Some(last_seen_user_id),
            ..Default::default()
        },
    )
    .await?;
```

Channel position updates are typed through `ModifyGuildChannelPosition`:

```rust
use discordrs::ModifyGuildChannelPosition;

rest
    .modify_guild_channel_positions(
        guild_id,
        &[ModifyGuildChannelPosition {
            id: channel_id,
            position: Some(Some(3)),
            lock_permissions: Some(Some(true)),
            parent_id: Some(None),
        }],
    )
    .await?;
```

Use `Some(None)` for nullable Discord fields such as `parent_id`; use `None` to omit the field.

Guild role reordering is typed through `ModifyGuildRolePosition`:

```rust
use discordrs::ModifyGuildRolePosition;

let roles = rest
    .modify_guild_role_positions(
        guild_id,
        &[ModifyGuildRolePosition {
            id: role_id,
            position: Some(Some(1)),
        }],
    )
    .await?;
```

Guild Resource write bodies are typed for common administration routes:

```rust
use discordrs::{
    BeginGuildPruneRequest, CreateGuildBan, CreateGuildChannel, CreateGuildRole,
    CreateStageInstance, ModifyCurrentMember, ModifyGuild, ModifyGuildMember,
    ModifyGuildOnboarding, ModifyGuildRole, ModifyGuildWelcomeScreen, ModifyGuildWidgetSettings,
    ModifyStageInstance, RoleColors, StageInstancePrivacyLevel, WelcomeScreenChannel,
};

rest.modify_guild(
    guild_id,
    &ModifyGuild {
        name: Some("renamed".to_string()),
        description: Some(None),
        ..Default::default()
    },
)
.await?;

rest.create_guild_channel(
    guild_id,
    &CreateGuildChannel {
        name: "rules".to_string(),
        kind: Some(0),
        parent_id: Some(None),
        ..Default::default()
    },
)
.await?;

rest.create_guild_ban(
    guild_id,
    user_id,
    &CreateGuildBan {
        delete_message_seconds: Some(60),
        ..Default::default()
    },
)
.await?;

let role = rest
    .create_guild_role(
        guild_id,
        &CreateGuildRole {
            name: Some("Gradient".to_string()),
            colors: Some(RoleColors {
                primary_color: 11127295,
                secondary_color: Some(16759788),
                tertiary_color: Some(16761760),
            }),
            ..Default::default()
        },
    )
    .await?;

rest.modify_guild_role(
    guild_id,
    role.id,
    &ModifyGuildRole {
        mentionable: Some(Some(true)),
        unicode_emoji: Some(None),
        ..Default::default()
    },
)
.await?;

// Returns the updated member (3.0).
let member = rest
    .modify_guild_member(
        guild_id,
        user_id,
        &ModifyGuildMember {
            nick: Some(None),
            channel_id: Some(None),
            ..Default::default()
        },
    )
    .await?;

let current_member = rest
    .modify_current_member(
        guild_id,
        &ModifyCurrentMember {
            nick: Some(Some("bot".to_string())),
            avatar: Some(None),
            ..Default::default()
        },
    )
    .await?;

rest.modify_guild_widget_settings(
    guild_id,
    &ModifyGuildWidgetSettings {
        enabled: Some(true),
        channel_id: Some(None),
    },
)
.await?;

rest.modify_guild_welcome_screen(
    guild_id,
    &ModifyGuildWelcomeScreen {
        enabled: Some(Some(true)),
        welcome_channels: Some(Some(vec![WelcomeScreenChannel {
            channel_id,
            description: "Start here".to_string(),
            emoji_id: None,
            emoji_name: Some("wave".to_string()),
        }])),
        description: Some(None),
    },
)
.await?;

rest.modify_guild_onboarding(
    guild_id,
    &ModifyGuildOnboarding {
        default_channel_ids: Some(vec![channel_id]),
        enabled: Some(true),
        mode: Some(0),
        ..Default::default()
    },
)
.await?;

let prune = rest
    .begin_guild_prune(
        guild_id,
        &BeginGuildPruneRequest {
            days: Some(7),
            compute_prune_count: Some(false),
            include_roles: Some(vec![role.id]),
            ..Default::default()
        },
    )
    .await?;

let stage = rest
    .create_stage_instance(&CreateStageInstance {
        channel_id,
        topic: "Town hall".to_string(),
        privacy_level: Some(StageInstancePrivacyLevel::GUILD_ONLY),
        send_start_notification: Some(true),
        guild_scheduled_event_id: None,
    })
    .await?;

rest.modify_stage_instance(
    stage.channel_id,
    &ModifyStageInstance {
        privacy_level: Some(StageInstancePrivacyLevel::GUILD_ONLY),
    },
)
.await?;
```

Adding an OAuth2-authorized user to a guild is typed through `AddGuildMember`:

```rust
use discordrs::AddGuildMember;

let joined = rest
    .add_guild_member(
        guild_id,
        user_id,
        &AddGuildMember {
            access_token: oauth_access_token.to_string(),
            ..Default::default()
        },
    )
    .await?;

if let Some(member) = joined {
    println!("created member: {:?}", member.user);
}
```

`add_guild_member(...)` returns `None` when Discord responds with `204 No Content` because the user is already in the guild.

Guild member payloads preserve current profile fields such as guild `banner`, `avatar_decoration_data`, and `collectibles`. Guild member listing supports Discord's `after` cursor through `GuildMembersQuery`, and guild member search supports shared percent-encoded query building through `SearchGuildMembersQuery`:

```rust
use discordrs::{GuildMembersQuery, SearchGuildMembersQuery};

let members = rest
    .get_guild_members_with_query(
        guild_id,
        &GuildMembersQuery {
            limit: Some(100),
            after: Some(last_seen_user_id),
        },
    )
    .await?;

let matches = rest
    .search_guild_members_with_query(
        guild_id,
        &SearchGuildMembersQuery {
            query: "alice & bob".to_string(),
            limit: Some(5),
        },
    )
    .await?;
```

Role member counts are exposed as `get_guild_role_member_counts(guild_id)`, returning a map of role IDs to member counts. Discord excludes the `@everyone` role from this response.

Guild widgets are available as typed JSON through `get_guild_widget(guild_id)`. The public PNG widget image is available through `get_guild_widget_image(guild_id, style)`, where `style` is a `GuildWidgetImageStyle` value. This image route is documented by Discord as unauthenticated, so discord.rs intentionally omits the bot authorization header for it.

Audit logs are available as typed `AuditLog` payloads through `get_guild_audit_log(guild_id, query)`. Use `AuditLogQuery` for `user_id`, `action_type`, `before`, `after`, and `limit` filters:

```rust
use discordrs::AuditLogQuery;

let audit_log = rest
    .get_guild_audit_log(
        guild_id,
        &AuditLogQuery {
            after: Some(last_entry_id),
            limit: Some(50),
            ..Default::default()
        },
    )
    .await?;
```

## Channel Administration Helpers

Channel invite listing and creation are typed through `get_channel_invites(...)` and `create_channel_invite(...)`:

```rust
use discordrs::{CreateChannelInvite, RoleId};

let invites = rest.get_channel_invites(channel_id).await?;
let invite = rest
    .create_channel_invite(
        channel_id,
        &CreateChannelInvite {
            max_age: Some(600),
            unique: Some(true),
            role_ids: Some(vec!["701".parse::<RoleId>()?]),
            ..Default::default()
        },
    )
    .await?;
```

Target-user invite allowlists use Discord's CSV upload flow:

- `create_channel_invite_with_target_users_file(channel_id, body, file)`
- `get_invite_target_users_csv(code)`
- `update_invite_target_users(code, file)`
- `get_invite_target_users_job_status(code)`

Use `FileAttachment::new("target_users.csv", b"user_id\n...")` for the `target_users_file` part. `InviteTargetUsersJobStatus` exposes Discord's async processing status counters and error text.

Voice-channel status and permission overwrite updates are typed through `SetVoiceChannelStatus` and `EditChannelPermission`:

```rust
use discordrs::{EditChannelPermission, OverwriteTarget, Permissions, SetVoiceChannelStatus};

rest
    .set_voice_channel_status(
        channel_id,
        &SetVoiceChannelStatus {
            status: Some("Office hours".to_string()),
        },
    )
    .await?;

// The target says whether the overwrite ID is a role or a member; the call
// fails before sending if the body's `type` disagrees.
let target = OverwriteTarget::Role(role_id);
rest
    .edit_channel_permissions(
        channel_id,
        target,
        &EditChannelPermission {
            allow: Some(Permissions::from_bits(1024)),
            ..EditChannelPermission::for_target(target)
        },
    )
    .await?;
```

## Group DM Helpers

Group DM creation is typed through `CreateGroupDmChannel`, and recipient management is typed through `AddGroupDmRecipient`:

```rust
use std::collections::HashMap;

use discordrs::{AddGroupDmRecipient, CreateGroupDmChannel, UserId};

let group_dm = rest
    .create_group_dm_channel(&CreateGroupDmChannel {
        access_tokens: vec![gdm_join_access_token.to_string()],
        nicks: HashMap::from([(UserId::try_from(42_u64)?, "friend".to_string())]),
    })
    .await?;

rest
    .add_group_dm_recipient(
        channel_id,
        user_id,
        &AddGroupDmRecipient {
            access_token: gdm_join_access_token.to_string(),
            nick: "friend".to_string(),
        },
    )
    .await?;

rest.remove_group_dm_recipient(channel_id, user_id).await?;
```

## Sticker Resource Helpers

Sticker packs and guild sticker writes are typed through `StickerPack`, `CreateGuildSticker`, and `ModifyGuildSticker`:

```rust
use discordrs::{CreateGuildSticker, FileAttachment, ModifyGuildSticker};

let pack = rest.get_sticker_pack(sticker_pack_id).await?;
println!("pack: {}", pack.name);

let sticker = rest
    .create_guild_sticker(
        guild_id,
        &CreateGuildSticker {
            name: "wave".to_string(),
            description: "Waves hello".to_string(),
            tags: "wave,hello".to_string(),
        },
        FileAttachment::new("wave.png", Vec::<u8>::new()),
    )
    .await?;

rest.modify_guild_sticker(
    guild_id,
    sticker.id,
    &ModifyGuildSticker {
        name: Some("wave2".to_string()),
        description: Some(None),
        tags: Some("wave".to_string()),
    },
)
.await?;
```

## Message Resource Helpers

Guild message search is typed through `SearchGuildMessagesQuery` and `search_guild_messages(...)`:

```rust
use discordrs::SearchGuildMessagesQuery;

let results = rest
    .search_guild_messages(
        guild_id,
        &SearchGuildMessagesQuery {
            content: Some("release notes".to_string()),
            limit: Some(25),
            include_nsfw: Some(false),
            ..Default::default()
        },
    )
    .await?;

for group in results.messages {
    for message in group {
        println!("{}: {}", message.id, message.content);
    }
}
```

Current channel pins are typed through `ChannelPinsQuery` and `get_channel_pins(...)`:

```rust
use discordrs::ChannelPinsQuery;

let pins = rest
    .get_channel_pins(
        channel_id,
        &ChannelPinsQuery {
            limit: Some(50),
            ..Default::default()
        },
    )
    .await?;

println!("fetched {} pinned messages", pins.items.len());

rest.pin_message(channel_id, message_id).await?;
rest.unpin_message(channel_id, message_id).await?;
```

The `Message` model also preserves newer Message Resource payloads such as `message_snapshots`, `referenced_message`, `call`, `shared_client_theme`, `mention_roles`, and role subscription data. Use `MessageSnapshot`, `MessageCall`, and `SharedClientTheme` when you need typed access to those fields.

## Monetization Helpers

Monetization routes use typed `Sku`, `Subscription`, and `Entitlement` models:

- `get_skus(...)`
- `get_sku_subscriptions(...)`
- `get_sku_subscription(...)`
- `get_entitlements(...)`
- `get_entitlement(...)`
- `consume_entitlement(...)`
- `create_test_entitlement(...)`
- `delete_test_entitlement(...)`

`Sku` preserves current response metadata such as `dependent_sku_id`, `manifest_labels`, `access_type`, `features`, `release_date`, `premium`, and `show_age_gate`. `Entitlement` preserves Discord's promotion, gift-code, consumed, validity-window, guild, and subscription metadata.

## Lobby Helpers

The Lobby resource is typed through `Lobby`, `LobbyMember`, `CreateLobby`, `ModifyLobby`, `AddLobbyMember`, `LobbyMemberUpdate`, and `LinkLobbyChannel`.

Bot-authorized helpers:

- `create_lobby(...)`
- `get_lobby(...)`
- `modify_lobby(...)`
- `delete_lobby(...)`
- `add_lobby_member(...)`
- `bulk_update_lobby_members(...)`
- `remove_lobby_member(...)`
- `update_lobby_message_moderation_metadata(...)`

User Bearer-token helpers:

- `leave_lobby(bearer_token, lobby_id)`
- `link_lobby_channel(bearer_token, lobby_id, body)`

## Guild Incident Actions

Discord's guild safety incident-action route is typed through `GuildIncidentsData` and `ModifyGuildIncidentActions`.

```rust
use discordrs::ModifyGuildIncidentActions;

let data = rest
    .modify_guild_incident_actions(
        guild_id,
        &ModifyGuildIncidentActions {
            invites_disabled_until: Some("2026-05-01T12:00:00.000000+00:00".to_string()),
            dms_disabled_until: None,
        },
    )
    .await?;
```

Supplying `None` serializes `null`, which disables the matching incident action.

## Helper Functions

For Components V2 and interaction response workflows, use:

- `send_container_message(...)`
- `respond_with_container(...)`
- `respond_component_with_container(...)`
- `respond_modal_with_container(...)`
- `respond_with_modal(...)`

## Example

```rust
let c = create_container("Notice", "Done", vec![], None);
respond_with_container(http, ctx.id, &ctx.token, c, true).await?;
```

`followup_with_container` and `defer_and_followup_container` return the created `Message` (3.0; they returned the raw JSON in 2.x). So do the channel helpers `send_container_message`, `send_to_channel`, `send_components_v2`, and `edit_message_with_container`, which build a `request::CreateMessage` / `EditMessage` with `components_v2(..)` and send it through `create_message` / `edit_message`. `edit_message_with_container` sets only the components and the `IS_COMPONENTS_V2` flag, so Discord rejects it for a message that still has content or embeds; clear those with `EditMessage::clear_content()` / `clear_embeds()` and call `edit_message` instead.

## OAuth2 Backend Helpers

`OAuth2Client` is the application-backend OAuth2 surface. It builds authorization URLs and exchanges authorization codes or refresh tokens with form-encoded OAuth2 requests. It is intentionally separate from bot-token `RestClient` calls.

```rust
use discordrs::{OAuth2AuthorizationRequest, OAuth2Client, OAuth2CodeExchange, OAuth2Scope};

let oauth = OAuth2Client::new("client_id", "client_secret");
let url = oauth.authorization_url(
    OAuth2AuthorizationRequest::code(
        "https://app.example/callback",
        [OAuth2Scope::identify(), OAuth2Scope::guilds()],
    )
    .state("csrf-token"),
)?;

let token = oauth
    .exchange_code(OAuth2CodeExchange::new("code", "https://app.example/callback"))
    .await?;

// POST /oauth2/token/revoke — invalidates the user's grant.
oauth
    .revoke_token(token.access_token.clone(), Some("access_token"))
    .await?;
```

## Recommended Pattern

- Use `RestClient` or context managers for typed fetch/send flows.
- Use `OAuth2Client` for OAuth2 application backend flows.
- Use helper functions for common interaction acknowledgment paths.
- Keep Components V2 payload generation inside the builders.
- Fall back to low-level raw request helpers only when the typed surface does not yet cover the route.

## Migrating REST Names from 2.x (3.0)

"Deprecated forward" means the 2.x name still compiles with a deprecation warning and calls the 3.0 method; "removed" means it is gone (a generic `B: Serialize` or `&Value` body, or a 2.x name now used by the canonical method with a typed signature).

| 2.x | 3.0 | Route | 2.x name |
|---|---|---|---|
| `begin_guild_prune_typed` | `begin_guild_prune` | `BeginGuildPrune` | removed |
| `begin_guild_prune_with_body` | `begin_guild_prune` | `BeginGuildPrune` | removed |
| `begin_guild_prune_with_request` | `begin_guild_prune` | `BeginGuildPrune` | deprecated forward |
| `bulk_overwrite_global_commands_typed` | `bulk_overwrite_global_commands` | `BulkOverwriteGlobalApplicationCommands` | deprecated forward |
| `bulk_overwrite_guild_commands_typed` | `bulk_overwrite_guild_commands` | `BulkOverwriteGuildApplicationCommands` | deprecated forward |
| `create_application_emoji_typed` | `create_application_emoji` | `CreateApplicationEmoji` | removed |
| `create_auto_moderation_rule_typed` | `create_auto_moderation_rule` | `CreateAutoModerationRule` | removed |
| `create_channel_invite_typed` | `create_channel_invite` | `CreateChannelInvite` | deprecated forward |
| `create_dm_channel_typed` | `create_dm_channel` | `CreateDm` | deprecated forward |
| `create_group_dm_channel_typed` | `create_group_dm_channel` | `CreateGroupDm` | deprecated forward |
| `create_guild_ban_typed` | `create_guild_ban` | `CreateGuildBan` | removed |
| `create_guild_channel_from_request` | `create_guild_channel` | `CreateGuildChannel` | deprecated forward |
| `create_guild_channel_typed` | `create_guild_channel` | `CreateGuildChannel` | removed |
| `create_guild_emoji_typed` | `create_guild_emoji` | `CreateGuildEmoji` | removed |
| `create_guild_scheduled_event_typed` | `create_guild_scheduled_event` | `CreateGuildScheduledEvent` | removed |
| `create_guild_sticker_from_request` | `create_guild_sticker` | `CreateGuildSticker` | deprecated forward |
| `create_guild_sticker_typed` | `create_guild_sticker` | `CreateGuildSticker` | removed |
| `create_role` | `create_guild_role` | `CreateGuildRole` | removed |
| `create_role_typed` | `create_guild_role` | `CreateGuildRole` | removed |
| `create_stage_instance_from_request` | `create_stage_instance` | `CreateStageInstance` | deprecated forward |
| `create_stage_instance_typed` | `create_stage_instance` | `CreateStageInstance` | removed |
| `create_webhook_from_request` | `create_webhook` | `CreateWebhook` | deprecated forward |
| `create_webhook_raw` | `create_webhook` | `CreateWebhook` | removed |
| `create_webhook_typed` | `create_webhook` | `CreateWebhook` | removed |
| `edit_channel_permissions_typed` | `edit_channel_permissions` | `EditChannelPermissions` | deprecated forward |
| `edit_current_application_from_request` | `edit_current_application` | `EditCurrentApplication` | deprecated forward |
| `get_application_emoji_typed` | `get_application_emoji` | `GetApplicationEmoji` | deprecated forward |
| `get_application_emojis_typed` | `get_application_emojis` | `ListApplicationEmojis` | deprecated forward |
| `get_auto_moderation_rules_typed` | `get_auto_moderation_rules` | `ListAutoModerationRulesForGuild` | deprecated forward |
| `get_channel_webhooks_raw` | `get_channel_webhooks` | `GetChannelWebhooks` | removed |
| `get_channel_webhooks_typed` | `get_channel_webhooks` | `GetChannelWebhooks` | deprecated forward |
| `get_current_user_guilds_typed` | `get_current_user_guilds` | `GetCurrentUserGuilds` | deprecated forward |
| `get_current_user_guilds_typed_with_query` | `get_current_user_guilds_with_query` | `GetCurrentUserGuilds` | deprecated forward |
| `get_guild_audit_log_typed` | `get_guild_audit_log` | `GetGuildAuditLog` | deprecated forward |
| `get_guild_emoji_typed` | `get_guild_emoji` | `GetGuildEmoji` | deprecated forward |
| `get_guild_emojis_typed` | `get_guild_emojis` | `ListGuildEmojis` | deprecated forward |
| `get_guild_preview_typed` | `get_guild_preview` | `GetGuildPreview` | deprecated forward |
| `get_guild_prune_count_typed` | `get_guild_prune_count` | `GetGuildPruneCount` | deprecated forward |
| `get_guild_widget_typed` | `get_guild_widget` | `GetGuildWidget` | deprecated forward |
| `get_public_archived_threads` | `list_public_archived_threads` | `ListPublicArchivedThreads` | removed |
| `get_voice_regions_typed` | `get_voice_regions` | `ListVoiceRegions` | deprecated forward |
| `modify_application_emoji_typed` | `modify_application_emoji` | `ModifyApplicationEmoji` | removed |
| `modify_auto_moderation_rule_typed` | `modify_auto_moderation_rule` | `ModifyAutoModerationRule` | removed |
| `modify_current_user_voice_state_from_request` | `modify_current_user_voice_state` | `ModifyCurrentUserVoiceState` | deprecated forward |
| `modify_guild_emoji_typed` | `modify_guild_emoji` | `ModifyGuildEmoji` | removed |
| `modify_guild_member_typed` | `modify_guild_member` | `ModifyGuildMember` | removed |
| `modify_guild_onboarding_config` | `modify_guild_onboarding` | `ModifyGuildOnboarding` | deprecated forward |
| `modify_guild_onboarding_typed` | `modify_guild_onboarding` | `ModifyGuildOnboarding` | removed |
| `modify_guild_role_positions_typed` | `modify_guild_role_positions` | `ModifyGuildRolePositions` | deprecated forward |
| `modify_guild_scheduled_event_typed` | `modify_guild_scheduled_event` | `ModifyGuildScheduledEvent` | removed |
| `modify_guild_sticker_from_request` | `modify_guild_sticker` | `ModifyGuildSticker` | deprecated forward |
| `modify_guild_sticker_typed` | `modify_guild_sticker` | `ModifyGuildSticker` | removed |
| `modify_guild_welcome_screen_config` | `modify_guild_welcome_screen` | `ModifyGuildWelcomeScreen` | deprecated forward |
| `modify_guild_welcome_screen_typed` | `modify_guild_welcome_screen` | `ModifyGuildWelcomeScreen` | removed |
| `modify_guild_widget` | `modify_guild_widget_settings` | `ModifyGuildWidget` | deprecated forward |
| `modify_guild_widget_settings_typed` | `modify_guild_widget_settings` | `ModifyGuildWidget` | removed |
| `modify_stage_instance_from_request` | `modify_stage_instance` | `ModifyStageInstance` | deprecated forward |
| `modify_stage_instance_typed` | `modify_stage_instance` | `ModifyStageInstance` | removed |
| `modify_user_voice_state_from_request` | `modify_user_voice_state` | `ModifyUserVoiceState` | deprecated forward |
| `modify_webhook_from_request` | `modify_webhook` | `ModifyWebhook` | deprecated forward |
| `modify_webhook_typed` | `modify_webhook` | `ModifyWebhook` | removed |
| `modify_webhook_with_token_from_request` | `modify_webhook_with_token` | `ModifyWebhookWithToken` | deprecated forward |
| `modify_webhook_with_token_typed` | `modify_webhook_with_token` | `ModifyWebhookWithToken` | removed |
| `update_channel_typed` | `update_channel` | `ModifyChannel` | removed |
| `update_guild` | `modify_guild` | `ModifyGuild` | removed |
| `update_guild_typed` | `modify_guild` | `ModifyGuild` | removed |
| `update_message` | `edit_message` | `EditMessage` | removed |
| `update_message_with_files` | `edit_message_with_files` | `EditMessage` | removed |
| `update_role` | `modify_guild_role` | `ModifyGuildRole` | removed |
| `update_role_typed` | `modify_guild_role` | `ModifyGuildRole` | removed |

Same name, new signature (2.x → 3.0):

| Method | 2.x | 3.0 |
|---|---|---|
| `update_channel` | `&Value` | `&request::ModifyChannel` |
| `create_guild_channel` | `&Value` | `&CreateGuildChannel` |
| `edit_channel_permissions` | `&Value` | `&EditChannelPermission` |
| `create_channel_invite` | `Option<&Value>` | `&CreateChannelInvite` (`default()` sends `{}`) |
| `create_thread_from_message`, `create_thread` | `&Value` | `&request::StartThreadFromMessage`, `&request::StartThreadWithoutMessage`; forum threads use the new `create_forum_thread` |
| `create_message`, `create_message_with_files` | `&model::CreateMessage` | `&request::CreateMessage` |
| `execute_webhook`, `execute_webhook_with_files` | `&Value` → `Value` | `&request::ExecuteWebhook` → `Message` |
| `execute_webhook_with_query` | `&Value` → `Value` | `&request::ExecuteWebhook` → `Option<Message>` |
| `edit_webhook_message*` | `&model::CreateMessage` | `&request::EditWebhookMessage` |
| `get_guild_audit_log` | inline filters → `Value` | `&AuditLogQuery` → `AuditLog` |
| `get_guild_prune_count` | → `Value` | → `GuildPruneCount` |
| `begin_guild_prune` | inline query parameters → `Value` | `&BeginGuildPruneRequest` (JSON body) → `GuildPruneResult` |
| `get_guild_preview`, `get_guild_widget` | → `Value` | → `GuildPreview`, `GuildWidget` |
| `modify_guild_widget_settings`, `modify_guild_welcome_screen`, `modify_guild_onboarding` | `&Value` | `&ModifyGuildWidgetSettings`, `&ModifyGuildWelcomeScreen`, `&ModifyGuildOnboarding` |
| `create_guild_template`, `modify_guild_template` | `&Value` | `&request::CreateGuildTemplate`, `&request::ModifyGuildTemplate` |
| `modify_guild_member` | `&Value` → `()` | `&ModifyGuildMember` → `Member` |
| `modify_current_member` | `B: Serialize` | `&ModifyCurrentMember` |
| `create_guild_ban` | `&Value` | `&CreateGuildBan` |
| `modify_guild_role_positions` | `&Value` | `&[ModifyGuildRolePosition]` |
| `get_auto_moderation_rules` | → `Vec<Value>` | → `Vec<AutoModerationRule>` |
| `create_auto_moderation_rule`, `modify_auto_moderation_rule` | `&Value` → `Value` | `&request::{CreateAutoModerationRule, ModifyAutoModerationRule}` → `AutoModerationRule` |
| `get_guild_emojis`, `get_guild_emoji`, `get_application_emojis`, `get_application_emoji` | → `Value` | → `Emoji` |
| `create_guild_emoji`, `modify_guild_emoji`, `create_application_emoji`, `modify_application_emoji` | `&Value` → `Value` | `&request::*Emoji` → `Emoji` |
| `create_guild_sticker` | `&Value` | `&CreateGuildSticker` (form fields) |
| `modify_guild_sticker` | `&Value` | `&ModifyGuildSticker` |
| `send_soundboard_sound`, `create_guild_soundboard_sound`, `modify_guild_soundboard_sound` | `&Value` | `&request::{SendSoundboardSound, CreateGuildSoundboardSound, ModifyGuildSoundboardSound}` |
| `create_guild_scheduled_event`, `modify_guild_scheduled_event` | `&Value` | `&request::{CreateGuildScheduledEvent, ModifyGuildScheduledEvent}` |
| `create_stage_instance`, `modify_stage_instance` | `&Value` | `&CreateStageInstance`, `&ModifyStageInstance` |
| `modify_current_user_voice_state`, `modify_user_voice_state` | `B: Serialize` | `&ModifyCurrentUserVoiceState`, `&ModifyUserVoiceState` |
| `get_voice_regions` | → `Vec<Value>` | → `Vec<VoiceRegion>` |
| `create_webhook` | `&Value` → `Value` | `&CreateWebhook` → `Webhook` |
| helpers `send_container_message`, `send_to_channel`, `send_components_v2`, `edit_message_with_container` | → `Value` | → `Message` |
| `get_channel_webhooks` | → `Vec<Value>` | → `Vec<Webhook>` |
| `modify_webhook`, `modify_webhook_with_token` | `&Value` | `&ModifyWebhook`, `&ModifyWebhookWithToken` |
| `edit_current_application` | `B: Serialize` | `&ModifyCurrentApplication` |
| `edit_global_command`, `edit_guild_command` | `&Value` | `&request::EditApplicationCommand` |
| `batch_edit_application_command_permissions` | `B: Serialize` | `&[GuildApplicationCommandPermissions]`, deprecated (Discord disabled the route) |
| `get_current_user_guilds(_with_query)` | → `Vec<Value>` | → `Vec<CurrentUserGuild>` |
| `request` | `(Method, &str, Option<&Value>)` → `Value` | `(RawRequest)` → `T` |
