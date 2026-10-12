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
use discordrs::{AppContext, AppFramework};

let app = AppFramework::builder()
    .command("export", move |ctx: AppContext| {
        let rest = rest.clone();
        async move {
            tokio::spawn(async move {
                let file_url = run_export().await;
                let _ = ctx.edit_reply(&rest, format!("Export ready: {file_url}")).await;
            });
            InteractionResponse::defer(false)
        }
    })
    .build();
```

Send the initial response either inline (the return value) or over REST (`ctx.reply(...)`), not both. A route returns anything that converts into an `EndpointReply`: an `InteractionResponse` (answered inline with `200`), or `EndpointReply::Accepted`. A route that responds over REST should return `EndpointReply::Accepted`: the endpoint then answers Discord with `202` and no body, as Discord specifies. If a route responded over REST but returned another response anyway, the endpoint still answers `202`, discards that response, and logs a warning. `AppContext`'s `Debug` output shows `[redacted]` instead of the interaction token.

## Guards and Cooldowns

Use `guard(...)` for shared authorization or tenancy checks. A guard returns `Ok(())` to continue or `Err(reply)` (an `EndpointReply`) to answer with that reply instead of running the route.

Use `cooldown(route, duration)` for per-route, per-user throttling. Cooldowns use the Discord user from the typed interaction context when one is available.

```rust
use discordrs::request::{InteractionMessage, InteractionResponse};
use discordrs::{AppContext, AppFramework, RouteKey};

let app = AppFramework::builder()
    .guard(|ctx: &AppContext| {
        if ctx.user_id().is_some() {
            Ok(())
        } else {
            Err(InteractionResponse::reply(
                InteractionMessage::from("Unknown user.").ephemeral(true),
            )
            .into())
        }
    })
    .command("ping", |_ctx| async move { InteractionResponse::reply("pong") })
    .cooldown(RouteKey::Command("ping".to_string()), std::time::Duration::from_secs(3))
    .build();
```

## When to Stay Lower-Level

Use a custom `TypedInteractionHandler` when the app needs dynamic route registration, external middleware, streaming side effects, or direct access to raw interaction payloads before routing.
