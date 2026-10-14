# Interactions Endpoint API

Use this mode when Discord sends interaction callbacks to your HTTP server.

## Feature Flag

```toml
[dependencies]
discordrs = { version = "3.0.0", features = ["interactions"] }
```

## Capabilities

- Ed25519 request signature verification
- Axum routing helpers
- 64 KiB request body cap on generated `/interactions` routes
- Typed interaction parsing
- Typed response encoding: handlers return an `EndpointReply`, inline callbacks are `request::InteractionResponse` values encoded once to the response bytes
- Optional `AppFramework` routing for commands, components, and modals

Build the router with `try_interactions_endpoint` / `try_typed_interactions_endpoint`, which return an error for an invalid public key. The non-`try_` wrappers panic on that error and are deprecated since 3.0.

## Typical Flow

1. Receive `/interactions` HTTP request
2. Verify signature headers/body
3. Parse into interaction type/context
4. Execute handler logic
5. Decide which transport owns the initial response (typed endpoint)
6. Return the interaction response JSON with `200`, or `202` with no body when the response is sent over REST

## Handler Return Type (3.0)

Handlers (`InteractionHandler::handle`, `TypedInteractionHandler::handle_typed`, `AppFramework` routes and guards) return a `request::EndpointReply`:

| `EndpointReply` | HTTP answer |
|---|---|
| `Inline(response)` (usually `InteractionResponse::...into()`) | `200 OK`, `Content-Type: application/json`, the callback as the body |
| `Accepted` | `202 Accepted` with no body: the initial response goes over REST |

`InteractionResponse` is the same type the REST callback (`RestClient::create_interaction_response`) and the responder methods send, and both transports encode it with one encoder, so the inline body and the REST body are byte for byte the same (the REST client additionally fills its default `allowed_mentions` into message callbacks). Its variants are the Discord callback types:

| Variant | `type` | `data` |
|---|---|---|
| `Pong` | 1 | none (the endpoint answers PINGs itself) |
| `ChannelMessage(InteractionMessage)` / `reply(..)` | 4 | the message |
| `DeferredChannelMessage { ephemeral }` / `defer(..)` | 5 | `{"flags": 0}` or `{"flags": 64}` |
| `DeferredUpdateMessage` | 6 | none |
| `UpdateMessage(UpdateMessage)` / `update(..)` | 7 | the update (patch: omitted fields keep their value) |
| `AutocompleteResult(Vec<ApplicationCommandOptionChoice>)` / `autocomplete(..)` | 8 | `{"choices": [...]}` |
| `Modal(Modal)` / `modal(..)` | 9 | the modal (see [Builders](./builders.md#modal-helper)) |
| `PremiumRequired` (deprecated by Discord) | 10 | none |
| `LaunchActivity` | 12 | none |
| `Raw(RawInteractionResponse)` | any | JSON text, sent exactly as written |

`EndpointReply` is not a callback and has no `Serialize`: `Accepted` cannot be encoded or posted as a callback body (a compile error, pinned by `tests/ui/endpoint_reply_is_not_a_callback.rs`).

```rust
use discordrs::request::{EndpointReply, InteractionResponse};
use discordrs::{Interaction, InteractionContextData, TypedInteractionHandler};

#[derive(Clone)]
struct Handler;

impl TypedInteractionHandler for Handler {
    async fn handle_typed(&self, _ctx: InteractionContextData, interaction: Interaction) -> EndpointReply {
        match interaction {
            Interaction::ChatInputCommand(_) => InteractionResponse::reply("hello").into(),
            _ => InteractionResponse::defer(true).into(),
        }
    }
}
```

## Response State (3.0)

The typed endpoint (`try_typed_interactions_endpoint`) passes the handler a context and an interaction that share one `InteractionResponseState` (see [HTTP and Helpers](./http-and-helpers.md#shared-response-state)). Exactly one transport owns the initial response; the inline answer and the REST callback claim the same slot atomically:

| Handler did | Endpoint answers | Shared state |
|---|---|---|
| Returned an initial response, no REST response sent | `200` with that response | `Deferred` (types 5, 6) or `Replied` |
| Sent (or started sending) the initial response over REST | `202 Accepted`, no body; the returned response is discarded with a warning | whatever the REST response settles to |
| Returned `EndpointReply::Accepted` | `202 Accepted`, no body | unchanged; a REST response sent later (e.g. from a spawned task) owns it |

Discord specifies the `202` with no body for interactions received over HTTP whose response goes through the callback endpoint. Return `EndpointReply::Accepted` whenever the handler responds over REST, so the endpoint never has to discard a response.

A clone kept by a spawned task after an inline response sees the interaction as acknowledged and can finish the work over REST:

```rust
use discordrs::prelude::*;
use discordrs::{AppContext, AppFramework};

let app = AppFramework::builder()
    .command("report", move |ctx: AppContext| {
        let rest = rest.clone();
        async move {
            tokio::spawn(async move {
                let report: String = build_report().await;
                let _ = ctx.edit_reply(&rest, report).await;
            });
            InteractionResponse::defer(false)
        }
    })
    .build();
```

Notes:

- The inline response is the body of Discord's own request, so it counts as delivered when the handler returns it. A follow-up sent in the instant before Discord has read that body can still be rejected by Discord.
- With `EndpointReply::Accepted`, the REST response must be sent within Discord's 3-second window, or the interaction fails.
- The endpoint encodes the inline response before it claims the response state, as the REST path does. If it cannot be encoded (it cannot happen with the typed variants), the endpoint logs the error and answers `500` without recording an initial response.
- The raw endpoint (`InteractionHandler`, `InteractionContext`) has no shared response state; helpers that take only an id and token are stateless.
- `Debug` output of interaction contexts (typed and raw, including the raw payload) shows `[redacted]` instead of the interaction token.

## When to Use

- Slash-command-first bots
- serverless or HTTP-native infrastructure
- apps where websocket Gateway runtime is not preferred

## `AppFramework`

`AppFramework` implements `TypedInteractionHandler` and is cheaply `Clone` (clones share routes and cooldown state; 3.0), so it can be passed directly to `try_typed_interactions_endpoint(...)`. It routes:

- slash/user/message commands by command name
- autocomplete by command name, as its own route (`autocomplete(name, handler)`, `RouteKey::Autocomplete`; 3.0)
- component interactions by `custom_id`
- modal submissions by `custom_id`

```rust
use discordrs::request::InteractionResponse;
use discordrs::{AppFramework, RouteKey};

let handler = AppFramework::builder()
    .command("hello", |_ctx| async move { InteractionResponse::reply("Hello from discord.rs") })
    .modal("ticket:create", |_ctx| async move { InteractionResponse::defer(true) })
    .cooldown(RouteKey::Command("hello".to_string()), std::time::Duration::from_secs(5))
    .build();
```

A route returns anything that converts into an `EndpointReply` (an `InteractionResponse`, or `EndpointReply::Accepted` when it responds over REST). Guards return `Result<(), EndpointReply>`. Use guards for shared authorization checks and `fallback(...)` for a custom unknown-route response.

Autocomplete never shares the command's cooldown, and it is never answered with a message: a guard or cooldown rejection, a missing `autocomplete(...)` route, or a route that returns another response type is answered with an empty list of choices. See "Autocomplete" in [Application Framework](app-framework.md).

## Migrating from 2.x

| 2.x | 3.0 |
|---|---|
| `discordrs::InteractionResponse` (endpoint enum with `Value` payloads) | `request::InteractionResponse` (also re-exported at the crate root and in the prelude) |
| `InteractionResponse::Accepted` | `EndpointReply::Accepted` |
| handler / route returns `InteractionResponse` | returns `EndpointReply` (`response.into()`); framework routes may return the `InteractionResponse` directly |
| `ChannelMessage(json!({...}))` | `InteractionResponse::reply(InteractionMessage::new()...)`, or `reply("text")` |
| `UpdateMessage(json!({...}))` | `InteractionResponse::update(UpdateMessage::new()...)` |
| `DeferredMessage` | `InteractionResponse::defer(false)` (now sends `{"data":{"flags":0}}`, like the REST defer did) |
| `DeferredUpdate` | `InteractionResponse::DeferredUpdateMessage` |
| `Modal(ModalBuilder)` | `InteractionResponse::modal(builder)` (`Modal` or `ModalBuilder`) |
| `Raw(Value)` with a `type` key | `InteractionResponse::Raw(RawInteractionResponse::new(kind).data(&value)?)` |
| `command(name, ..)` also received autocomplete | `autocomplete(name, ..)` (`RouteKey::Autocomplete`); without it, no choices |

