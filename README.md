# discordrs

Standalone Discord bot framework for Rust with Components V2, Gateway WebSocket, and HTTP client

## Features

- Gateway WebSocket client with connect, heartbeat, identify, resume, reconnect, and terminal close-code handling
- HTTP REST client with automatic 429 retry and explicit application-id follow-up helpers
- Components V2 builders (`Container`, `TextDisplay`, `Section`, `MediaGallery`, `Button`, `SelectMenu`, and more)
- Modal builders with `RadioGroup`, `CheckboxGroup`, `Checkbox`, and `FileUpload`
- V2 modal submission parser with preserved `FileUpload`, `RadioGroup`, `CheckboxGroup`, and other V2 component types
- Interaction routing helpers: `parse_raw_interaction`, `parse_interaction_context`, and `try_interactions_endpoint`
- Feature-gated modules: `gateway` for bot client runtime, `interactions` for HTTP Interactions Endpoint

## Install

```toml
[dependencies]
discordrs = "0.3.1"
```

```toml
[dependencies]
# Gateway bot client
discordrs = { version = "0.3.1", features = ["gateway"] }

# HTTP Interactions Endpoint
discordrs = { version = "0.3.1", features = ["interactions"] }

# Both runtime modes
discordrs = { version = "0.3.1", features = ["gateway", "interactions"] }
```

## Documentation Site

A navigation-focused docs website (similar to the discord.js docs browsing style) is available in [`discordrsdocs/`](discordrsdocs/):

- Live URL: `http://discordrs.teamwicked.me/discordrsdocs/#/`
- Korean URL: `http://discordrs.teamwicked.me/discordrsdocs/#/ko/README`
- Entry: [`discordrsdocs/index.html`](discordrsdocs/index.html)
- Local preview:
  ```bash
  python3 -m http.server 8080 --directory discordrsdocs
  ```

## Quick Example

```rust
use discordrs::{BotClient, Context, EventHandler, gateway_intents};
use discordrs::{create_container, respond_with_container, parse_raw_interaction, parse_interaction_context, RawInteraction};
use async_trait::async_trait;
use serde_json::Value;

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _ctx: Context, ready: Value) {
        println!("Bot ready! User: {}", ready["user"]["username"]);
    }
    async fn interaction_create(&self, ctx: Context, interaction: Value) {
        let ictx = parse_interaction_context(&interaction).unwrap();
        match parse_raw_interaction(&interaction).unwrap() {
            RawInteraction::Command { name, .. } => {
                if name.as_deref() == Some("hello") {
                    let container = create_container("Hello", "Hello, World!", vec![], None);
                    let _ = respond_with_container(&ctx.http, &ictx.id, &ictx.token, container, false).await;
                }
            }
            _ => {}
        }
    }
}

#[tokio::main]
async fn main() {
    let token = std::env::var("DISCORD_TOKEN").unwrap();
    BotClient::builder(&token, gateway_intents::GUILDS | gateway_intents::GUILD_MESSAGES)
        .event_handler(Handler)
        .start()
        .await
        .unwrap();
}
```

## Components V2 Example

```rust
use discordrs::{
    button_style, create_container, send_container_message, ButtonConfig, DiscordHttpClient,
};

async fn send_support_panel(http: &DiscordHttpClient, channel_id: u64) -> Result<(), discordrs::Error> {
    let buttons = vec![
        ButtonConfig::new("ticket_open", "Open Ticket").style(button_style::PRIMARY),
        ButtonConfig::new("ticket_status", "Check Status").style(button_style::SECONDARY),
    ];

    let container = create_container(
        "Support Panel",
        "Use the buttons below to manage support requests.",
        buttons,
        None,
    );

    send_container_message(http, channel_id, container).await?;
    Ok(())
}
```

## Modal Example with RadioGroup

```rust
use discordrs::{
    CheckboxBuilder, CheckboxGroupBuilder, FileUploadBuilder, ModalBuilder, RadioGroupBuilder,
    SelectOption,
};

let modal = ModalBuilder::new("preferences_modal", "Preferences")
    .add_radio_group(
        "Theme",
        Some("Pick one"),
        RadioGroupBuilder::new("theme")
            .add_option(SelectOption::new("Light", "light"))
            .add_option(SelectOption::new("Dark", "dark"))
            .required(true),
    )
    .add_checkbox_group(
        "Notifications",
        Some("Choose any"),
        CheckboxGroupBuilder::new("notify_channels")
            .add_option(SelectOption::new("Email", "email"))
            .add_option(SelectOption::new("Push", "push"))
            .min_values(0)
            .max_values(2),
    )
    .add_checkbox(
        "Agree to Terms",
        None,
        CheckboxBuilder::new("agree_terms").required(true),
    )
    .add_file_upload(
        "Screenshot",
        Some("Attach one or more files"),
        FileUploadBuilder::new("attachments").min_values(1),
    );
```

## Interactions Endpoint Example

```rust
use async_trait::async_trait;
use axum::Router;
use discordrs::{
    create_container, InteractionContext, InteractionHandler, InteractionResponse, RawInteraction,
    try_interactions_endpoint,
};

#[derive(Clone)]
struct Handler;

#[async_trait]
impl InteractionHandler for Handler {
    async fn handle(
        &self,
        ctx: InteractionContext,
        interaction: RawInteraction,
    ) -> InteractionResponse {
        match interaction {
            RawInteraction::Command { name, .. } if name.as_deref() == Some("hello") => {
                let data = serde_json::json!({
                    "components": [create_container("Hello", "World", vec![], None).build()],
                    "flags": 1 << 15,
                });
                let _ = ctx;
                InteractionResponse::ChannelMessage(data)
            }
            _ => InteractionResponse::Raw(serde_json::json!({ "type": 5 })),
        }
    }
}

fn app(public_key: &str) -> Router {
    try_interactions_endpoint(public_key, Handler)
        .expect("invalid Discord public key")
}
```

## Feature Flags

| Feature | Description | Key deps |
|---------|-------------|----------|
| (default) | Builders, parsers, HTTP client, helpers | reqwest, serde_json |
| `gateway` | Gateway WebSocket, BotClient, EventHandler | tokio-tungstenite, flate2, async-trait |
| `interactions` | HTTP Interactions Endpoint with Ed25519 | axum, ed25519-dalek |

## Notes

- `discordrs` started as a helper around serenity workflows, but v0.3.1 is now a fully standalone framework.
- Use `try_interactions_endpoint()` when you want invalid public keys to fail at startup instead of during requests.
- Use `DiscordHttpClient::create_followup_message_with_application_id()` when you already have `InteractionContext.application_id` and the client was not initialized with an application id.
- The parser keeps V2 modal component types, including `FileUpload`, `RadioGroup`, and `CheckboxGroup`, so routing logic can keep full fidelity.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

## Developer

- ingwannu
- Contact: ingwannu@teamwicked.me, ingwannu@gmail.com

