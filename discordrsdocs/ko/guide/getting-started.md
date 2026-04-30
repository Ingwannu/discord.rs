# ���� ����

## �غ�

- Rust stable toolchain
- Discord ���ø����̼ǰ� �� ��ū
- ���� ����: Interaction Endpoint ��忡�� ����� ���� HTTP ��������Ʈ

## ������ �߰�

```toml
[dependencies]
discordrs = { version = "1.2.2", features = ["gateway"] }
```

�ʿ��� ��Ÿ�ӿ� ���� ����� �߰��մϴ�.

```toml
# REST/���/Ÿ�� �𵨸� ����� ��
discordrs = "1.2.2"

# Voice receive�� Opus PCM decode
discordrs = { version = "1.2.2", features = ["voice"] }

# ������ DAVE/MLS hook
discordrs = { version = "1.2.2", features = ["voice", "dave"] }
```

## �ּ� Typed Gateway Bot

```rust
use async_trait::async_trait;
use discordrs::{gateway_intents, Client, Context, Event, EventHandler};

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn handle_event(&self, _ctx: Context, event: Event) {
        if let Event::Ready(ready) = event {
            println!("READY as {}", ready.data.user.username);
        }
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

## ȯ�� ����

```bash
export DISCORD_TOKEN="your-bot-token"
```

## ����

```bash
cargo run
```

## ���� �ܰ�

- [��� ���̵�](usage-guide.md)�� �̵�
- [��Ű��ó](architecture.md) �б�
- [Ŀ�ǵ� API](../api/commands.md) ���캸��
- Poll, Subscription, Soundboard, Thread, Forum, Integration, Voice receive ���� Ȯ�� ǥ���� ���� Ÿ�� API�� Ȯ���� �� raw JSON�� ������ �������� ����ϱ�
