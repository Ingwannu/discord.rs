# 시작하기

## 준비

- Rust stable toolchain
- Discord 애플리케이션과 봇 토큰
- HTTP Interactions Endpoint를 사용할 경우 공개 HTTPS endpoint와 Discord Ed25519 public key

## 의존성 추가

```toml
[dependencies]
discordrs = { version = "2.0.0", features = ["gateway"] }
```

필요한 런타임 기능만 켜는 것을 권장합니다.

```toml
# REST, 빌더, 타입 모델만 사용할 때
discordrs = "2.0.0"

# Interactions Endpoint와 앱 프레임워크
discordrs = { version = "2.0.0", features = ["interactions"] }

# Voice receive와 Opus PCM decode
discordrs = { version = "2.0.0", features = ["voice"] }

# DAVE/MLS hook
discordrs = { version = "2.0.0", features = ["voice", "dave"] }
```

## 최소 Typed Gateway Bot

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

## 환경 변수

```bash
export DISCORD_TOKEN="your-bot-token"
```

## 실행

```bash
cargo run
```

## 다음 단계

- [사용 가이드](usage-guide.md) 읽기
- [아키텍처](architecture.md) 검토
- [명령 API](../../docs/api/commands.md) 살펴보기
- Webhook Events, Lobby, Poll, Subscription, Soundboard, Thread, Forum, Integration, Voice receive 워크플로에서는 가능한 한 raw JSON보다 타입드 API를 우선 사용하기
