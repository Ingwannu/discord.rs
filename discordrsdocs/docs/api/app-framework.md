# Application Framework API

`AppFramework` is a small routing layer for HTTP interactions. It is available behind the `interactions` feature and implements `TypedInteractionHandler`.

## Routes

- `command(name, handler)`: slash, user, message, and autocomplete command names
- `component(custom_id, handler)`: message component interactions
- `modal(custom_id, handler)`: modal submit interactions
- `fallback(handler)`: unknown routes

Each handler receives an owned `AppContext` containing the parsed `InteractionContextData`, the typed `Interaction`, and the resolved `RouteKey`. The handler's return value is the inline response the endpoint sends back to Discord.

`AppFramework` is cheaply `Clone` (clones share routes and cooldown state), which the typed endpoint constructors require.

## Responding Later from a Route (3.0)

`AppContext` implements `InteractionResponder`, and its context shares one response state with the interaction. When the typed endpoint answers with the route's inline response, it records that response in the shared state, so a clone moved into a spawned task can call `edit_reply`, `follow_up`, or `delete_reply` over REST afterwards:

```rust
use discordrs::prelude::*;
use discordrs::{AppContext, AppFramework, InteractionResponse};

let app = AppFramework::builder()
    .command("export", move |ctx: AppContext| {
        let rest = rest.clone();
        async move {
            tokio::spawn(async move {
                let file_url = run_export().await;
                let _ = ctx.edit_reply(&rest, format!("Export ready: {file_url}")).await;
            });
            InteractionResponse::DeferredMessage
        }
    })
    .build();
```

Send the initial response either inline (the return value) or over REST (`ctx.reply(...)`), not both: Discord accepts one initial response, and the endpoint logs a warning when a route does both. `AppContext`'s `Debug` output shows `[redacted]` instead of the interaction token.

## Guards and Cooldowns

Use `guard(...)` for shared authorization or tenancy checks. A guard returns `Ok(())` to continue or `Err(response)` to answer with that response instead of running the route.

Use `cooldown(route, duration)` for per-route, per-user throttling. Cooldowns use the Discord user from the typed interaction context when one is available.

```rust
use discordrs::{AppContext, AppFramework, InteractionResponse, RouteKey};

let app = AppFramework::builder()
    .guard(|ctx: &AppContext| {
        if ctx.user_id().is_some() {
            Ok(())
        } else {
            Err(InteractionResponse::ChannelMessage(serde_json::json!({
                "content": "Unknown user.",
                "flags": 64
            })))
        }
    })
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
