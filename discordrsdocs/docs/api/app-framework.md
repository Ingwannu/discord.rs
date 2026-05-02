# Application Framework API

`AppFramework` is a small routing layer for HTTP interactions. It is available behind the `interactions` feature and implements `TypedInteractionHandler`.

## Routes

- `command(name, handler)`: slash, user, message, and autocomplete command names
- `component(custom_id, handler)`: message component interactions
- `modal(custom_id, handler)`: modal submit interactions
- `fallback(handler)`: unknown routes

Each handler receives an owned `AppContext` containing the parsed `InteractionContextData`, the typed `Interaction`, and the resolved `RouteKey`.

## Guards and Cooldowns

Use `guard(...)` for shared authorization or tenancy checks. Returning `false` sends an ephemeral forbidden response before route execution.

Use `cooldown(route, duration)` for per-route, per-user throttling. Cooldowns use the Discord user from the typed interaction context when one is available.

```rust
use discordrs::{AppFramework, InteractionResponse, RouteKey};

let app = AppFramework::builder()
    .guard(|ctx| ctx.user_id().is_some())
    .command("ping", |_ctx| async move {
        InteractionResponse::ChannelMessage(serde_json::json!({
            "content": "pong"
        }))
    })
    .cooldown(RouteKey::Command("ping".to_string()), std::time::Duration::from_secs(3))
    .build();
```

## When to Stay Lower-Level

Use a custom `TypedInteractionHandler` when the app needs dynamic route registration, external middleware, streaming side effects, or direct access to raw interaction payloads before routing.
