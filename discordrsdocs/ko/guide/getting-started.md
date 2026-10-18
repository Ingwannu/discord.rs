# 시작하기

## 준비

- Rust stable toolchain
- Discord 애플리케이션과 봇 토큰
- HTTP Interactions Endpoint를 사용할 경우 공개 HTTPS endpoint와 Discord Ed25519 public key

## 의존성 추가

```toml
[dependencies]
discordrs = { version = "3.0.0", features = ["gateway"] }
```

필요한 런타임 기능만 켜는 것을 권장합니다.

```toml
# REST, 빌더, 타입 모델만 사용할 때
discordrs = "3.0.0"

# Interactions Endpoint와 앱 프레임워크
discordrs = { version = "3.0.0", features = ["interactions"] }

# Voice receive와 Opus PCM decode, DAVE 종단 간 암호화 포함
# (`dave` 기능은 `voice`의 별칭)
discordrs = { version = "3.0.0", features = ["voice"] }
```

## 최소 Typed Gateway Bot

핸들러는 impl 안의 일반 `async fn`으로 씁니다(3.0부터 `#[async_trait]` 불필요). `Send` 규칙과 마이그레이션은 [게이트웨이 API](../api/gateway.md)를 참고합니다.

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
- 2.2.0 신규: `use discordrs::response::InteractionResponder;` 임포트 후 `interaction.reply(&ctx.http, "안녕하세요")`로 응답하고, `message.reply(...)`, `channel.send(...)`, `user.dm(...)`을 타입드 모델에서 바로 호출하기 — 자세한 내용은 [사용 가이드](usage-guide.md) 참고
