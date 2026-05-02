# Interactions Endpoint API

Use this mode when Discord sends interaction callbacks to your HTTP server.

## Feature Flag

```toml
[dependencies]
discordrs = { version = "2.0.1", features = ["interactions"] }
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
5. Return interaction response JSON

## When to Use

- Slash-command-first bots
- serverless or HTTP-native infrastructure
- apps where websocket Gateway runtime is not preferred

## `AppFramework`

`AppFramework` implements `TypedInteractionHandler`, so it can be passed directly to `try_typed_interactions_endpoint(...)`. It routes:

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

