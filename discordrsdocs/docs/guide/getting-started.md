# Getting Started

## Prerequisites

- Rust stable toolchain
- A Discord application + bot token
- Optional: public endpoint if using Interaction Endpoint mode

## Add Dependency

```toml
[dependencies]
discordrs = { version = "2.2.0", features = ["gateway"] }
```

## Minimal Typed Gateway Bot

Handlers are plain `async fn` in the impl (no `#[async_trait]` since 3.0); see [Gateway API](../api/gateway.md) for the `Send` rule and migration.

```rust
use discordrs::{gateway_intents, Client, Context, EventHandler, ReadyPayload};

struct Handler;

impl EventHandler for Handler {
    async fn ready(&self, _ctx: Context, ready: &ReadyPayload) {
        println!("READY as {}", ready.user.username);
    }
}

#[tokio::main]
async fn main() -> Result<(), discordrs::DiscordError> {
    let token = std::env::var("DISCORD_TOKEN")?;

    Client::builder(&token, gateway_intents::GUILDS | gateway_intents::GUILD_MESSAGES)
        .event_handler(Handler)
        .start()
        .await?;

    Ok(())
}
```

## Environment Variable

```bash
export DISCORD_TOKEN="your-bot-token"
```

## Run

```bash
cargo run
```

## Next

- Go to [Usage Guide](usage-guide.md)
- Read [Architecture](architecture.md)
- Explore [Commands API](../api/commands.md)
- New in `2.2.0`: reply to interactions with `use discordrs::response::InteractionResponder;` and `interaction.reply(&ctx.http, "hi")`, and call `message.reply(...)`, `channel.send(...)`, `user.dm(...)` directly on the typed models — see the [Usage Guide](usage-guide.md) and [Changelog](changelog.md)

