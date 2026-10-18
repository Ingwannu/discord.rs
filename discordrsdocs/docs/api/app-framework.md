# Application Framework API

`AppFramework` is a small routing layer for HTTP interactions. It is available behind the `interactions` feature and implements `TypedInteractionHandler`.

## Routes

- `command(name, handler)`: slash, user, message, and entry-point command names
- `autocomplete(name, handler)`: autocomplete for the command with that name (`RouteKey::Autocomplete`; 3.0)
- `component(custom_id, handler)`: message component interactions
- `modal(custom_id, handler)`: modal submit interactions
- `fallback(handler)`: unknown routes

Each handler receives an owned `AppContext` containing the parsed `InteractionContextData`, the typed `Interaction`, and the resolved `RouteKey`. The handler's return value is the inline response the endpoint sends back to Discord.

`AppFramework` is cheaply `Clone` (clones share routes and cooldown state), which the typed endpoint constructors require.

### Autocomplete (3.0)

Autocomplete is its own route, separate from the command: in 2.x it was dispatched as `RouteKey::Command(name)`, so typing an option started the command's cooldown and later keystrokes were answered with the cooldown message. Discord accepts only `APPLICATION_COMMAND_AUTOCOMPLETE_RESULT` for autocomplete, so the framework never answers one with anything else: when a guard or cooldown rejects it, no `autocomplete(...)` route matches (the command's handler is not called), or the route returns another response type, the answer is an empty list of choices. `EndpointReply::Accepted` (the route answered over REST) is kept.

```rust
use discordrs::model::ApplicationCommandOptionChoice;
use discordrs::request::InteractionResponse;
use discordrs::{AppContext, AppFramework};

let app = AppFramework::builder()
    .command("search", |_ctx| async move { InteractionResponse::reply("searching") })
    .autocomplete("search", |_ctx: AppContext| async move {
        InteractionResponse::autocomplete([ApplicationCommandOptionChoice::new("rust", "rust")])
    })
    .build();
```

## Responding Later from a Route (3.0)

`AppContext` implements `InteractionResponder`, and its context shares one response state with the interaction, so a clone moved into a spawned task can call `edit_reply`, `follow_up`, or `delete_reply` over REST once the interaction is acknowledged. For work that outlives the route, acknowledge over REST first (`ctx.defer(&rest)`), then spawn the task and return `EndpointReply::Accepted`:

```rust
use discordrs::prelude::*;
use discordrs::{AppContext, AppFramework};

let app = AppFramework::builder()
    .command("export", move |ctx: AppContext| {
        let rest = rest.clone();
        async move {
            // Acknowledge over REST before the task starts: once `defer` returns,
            // Discord holds the deferral and the task may edit it at any time.
            if ctx.defer(&rest).await.is_err() {
                // The endpoint sends this only if Discord cannot have applied the
                // deferral; otherwise it answers 202.
                return InteractionResponse::reply("Could not start the export.").into();
            }
            tokio::spawn(async move {
                let file_url = run_export().await;
                let _ = ctx.edit_reply(&rest, format!("Export ready: {file_url}")).await;
            });
            EndpointReply::Accepted
        }
    })
    .build();
```

Do not spawn the task and return an inline `InteractionResponse::defer(..)` instead. The endpoint records the route's inline response only after the route has returned, and Discord gets it only after that, so a task that reaches `edit_reply` first fails locally with "interaction has not been acknowledged" or is rejected by Discord. When the work fits in Discord's 3-second window, skip the task and return the inline reply.

Send the initial response either inline (the return value) or over REST (`ctx.reply(...)`), not both. A route returns anything that converts into an `EndpointReply`: an `InteractionResponse` (answered inline with `200`), or `EndpointReply::Accepted`. A route that responds over REST should return `EndpointReply::Accepted`: the endpoint then answers Discord with `202` and no body, as Discord specifies. If a route responded over REST but returned another response anyway, the endpoint still answers `202`, discards that response, and logs a warning. `AppContext`'s `Debug` output shows `[redacted]` instead of the interaction token.

## Guards and Cooldowns

Use `guard(...)` for shared authorization or tenancy checks. A guard returns `Ok(())` to continue or `Err(reply)` (an `EndpointReply`) to answer with that reply instead of running the route.

Use `cooldown(route, duration)` for per-route, per-user throttling. Cooldowns use the Discord user from the typed interaction context when one is available. A cooldown on `RouteKey::Command(name)` does not apply to the command's autocomplete; set one on `RouteKey::Autocomplete(name)` to throttle it (a rejected autocomplete gets no choices).

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
