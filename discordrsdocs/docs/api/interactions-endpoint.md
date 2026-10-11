# Interactions Endpoint API

Use this mode when Discord sends interaction callbacks to your HTTP server.

## Feature Flag

```toml
[dependencies]
discordrs = { version = "2.2.0", features = ["interactions"] }
```

## Capabilities

- Ed25519 request signature verification
- Axum routing helpers
- 64 KiB request body cap on generated `/interactions` routes
- Typed interaction parsing
- Structured response encoding (`Pong`, message, deferred, modal, update)
- Optional `AppFramework` routing for commands, components, and modals

## Typical Flow

1. Receive `/interactions` HTTP request
2. Verify signature headers/body
3. Parse into interaction type/context
4. Execute handler logic
5. Decide which transport owns the initial response (typed endpoint)
6. Return the interaction response JSON with `200`, or `202` with no body when the response is sent over REST

## Response State (3.0)

The typed endpoint (`try_typed_interactions_endpoint`, `typed_interactions_endpoint`) passes the handler a context and an interaction that share one `InteractionResponseState` (see [HTTP and Helpers](./http-and-helpers.md#shared-response-state)). Exactly one transport owns the initial response; the inline answer and the REST callback claim the same slot atomically:

| Handler did | Endpoint answers | Shared state |
|---|---|---|
| Returned an initial response, no REST response sent | `200` with that response | `Deferred` (types 5, 6) or `Replied` |
| Sent (or started sending) the initial response over REST | `202 Accepted`, no body; the returned response is discarded with a warning | whatever the REST response settles to |
| Returned `InteractionResponse::Accepted` | `202 Accepted`, no body | unchanged; a REST response sent later (e.g. from a spawned task) owns it |

Discord specifies the `202` with no body for interactions received over HTTP whose response goes through the callback endpoint. Return `InteractionResponse::Accepted` whenever the handler responds over REST, so the endpoint never has to discard a response.

A clone kept by a spawned task after an inline response sees the interaction as acknowledged and can finish the work over REST:

```rust
use discordrs::prelude::*;
use discordrs::{AppContext, AppFramework, InteractionResponse};

let app = AppFramework::builder()
    .command("report", move |ctx: AppContext| {
        let rest = rest.clone();
        async move {
            tokio::spawn(async move {
                let report = build_report().await;
                let _ = ctx.edit_reply(&rest, report).await;
            });
            InteractionResponse::DeferredMessage
        }
    })
    .build();
```

Notes:

- The inline response is the body of Discord's own request, so it counts as delivered when the handler returns it. A follow-up sent in the instant before Discord has read that body can still be rejected by Discord.
- With `InteractionResponse::Accepted`, the REST response must be sent within Discord's 3-second window, or the interaction fails.
- The raw endpoint (`InteractionHandler`, `InteractionContext`) has no shared response state; helpers that take only an id and token are stateless.
- `Debug` output of interaction contexts (typed and raw, including the raw payload) shows `[redacted]` instead of the interaction token.

## When to Use

- Slash-command-first bots
- serverless or HTTP-native infrastructure
- apps where websocket Gateway runtime is not preferred

## `AppFramework`

`AppFramework` implements `TypedInteractionHandler` and is cheaply `Clone` (clones share routes and cooldown state; 3.0), so it can be passed directly to `try_typed_interactions_endpoint(...)`. It routes:

- slash/user/message commands by command name
- component interactions by `custom_id`
- modal submissions by `custom_id`

```rust
use discordrs::{AppFramework, InteractionResponse, RouteKey};

let handler = AppFramework::builder()
    .command("hello", |_ctx| async move {
        InteractionResponse::ChannelMessage(serde_json::json!({
            "content": "Hello from discord.rs"
        }))
    })
    .modal("ticket:create", |_ctx| async move {
        InteractionResponse::DeferredMessage
    })
    .cooldown(RouteKey::Command("hello".to_string()), std::time::Duration::from_secs(5))
    .build();
```

Use guards for shared authorization checks and `fallback(...)` for a custom unknown-route response.

