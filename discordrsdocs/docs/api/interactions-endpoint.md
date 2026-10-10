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
5. Record the returned initial response in the interaction's shared response state (typed endpoint)
6. Return interaction response JSON

## Response State (3.0)

The typed endpoint (`try_typed_interactions_endpoint`, `typed_interactions_endpoint`) passes the handler a context and an interaction that share one `InteractionResponseState` (see [HTTP and Helpers](./http-and-helpers.md#shared-response-state)). When the handler returns an initial response (anything except `Pong`, or a `Raw` payload whose `type` is not 1), the endpoint records it before answering Discord: deferred types (5, 6) become `Deferred`, every other type `Replied`.

A clone kept by a spawned task therefore sees the interaction as acknowledged and can finish the work over REST:

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
- If the handler already responded over REST (or such a response is still in flight) and then returns another initial response, the endpoint still returns the payload, because Discord's request must be answered, but leaves the state as the REST response set it and logs a warning: Discord accepts only one initial response. Return the response inline *or* send it over REST, not both.
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

