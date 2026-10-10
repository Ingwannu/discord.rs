# 게이트웨이 API

`gateway` 기능 플래그 활성화 시 WebSocket 런타임을 사용합니다.

## 핵심 타입

- `GatewayClient`: identify/heartbeat/resume/reconnect 처리
- `Client`: 고수준 타입드 런타임 (`BotClient`는 호환 별칭)
- `Context`: 핸들러 공유 컨텍스트(`http`, 캐시, typemap, 샤드 정보)
- `EventHandler`: 이벤트 콜백 트레잇
- `EventDispatchMode`: 직렬(기본) 또는 동시 핸들러 스케줄링

## 기본 실행

```rust
Client::builder(&token, gateway_intents::GUILDS | gateway_intents::GUILD_MESSAGES)
    .event_handler(handler)
    .start()
    .await?;
```

## 인텐트

`gateway_intents`는 문서화된 모든 인텐트 비트에 대한 상수를 제공하며, 2.1에서 Poll 인텐트가 추가되었습니다.

- `gateway_intents::GUILD_MESSAGE_POLLS`(`1 << 24`)와 `gateway_intents::DIRECT_MESSAGE_POLLS`(`1 << 25`)는 `MESSAGE_POLL_VOTE_*` 이벤트를 활성화하며, 두 인텐트 모두 `gateway_intents::NON_PRIVILEGED`에 포함됩니다.
- `gateway_intents::GUILD_EXPRESSIONS`는 비트 3의 현재 Discord 명칭입니다(기존 `GUILD_EMOJIS_AND_STICKERS` 상수도 유지됩니다).

## 초기 Presence와 디스패치 모드

`ClientBuilder::presence(...)`는 IDENTIFY 안에 초기 presence를 담아 보내므로, READY 이후에 상태를 갱신하는 대신 원하는 상태로 바로 접속합니다. `ClientBuilder::event_dispatch(...)`는 핸들러 호출 스케줄링 방식을 선택합니다.

```rust
use discordrs::{EventDispatchMode, UpdatePresence};

Client::builder(&token, intents)
    .event_handler(handler)
    .presence(UpdatePresence::online_with_activity("티켓 처리 중"))
    .event_dispatch(EventDispatchMode::Concurrent)
    .start()
    .await?;
```

- `EventDispatchMode::Serial`(기본): 샤드의 핸들러 호출을 게이트웨이 순서대로 하나씩 실행합니다.
- `EventDispatchMode::Concurrent`: 핸들러 호출을 동시 태스크로 실행하되 샤드당 최대 `max_concurrent_handlers`개(기본 256)까지만 돌립니다. 느린 핸들러 하나가 다른 핸들러를 늦추지 않습니다.

### 이벤트 파이프라인 (3.0)

샤드마다 이벤트를 두 단계로 처리합니다. 내부 처리기가 모든 이벤트를 게이트웨이 순서대로 먼저 반영합니다(캐시 갱신, 외부 캐시 백엔드 쓰기, 음성 상태, `fetch_members` 청크 연결, 컬렉터 발행). 이 단계는 핸들러를 기다리지 않습니다. 그다음 샤드별 유한 큐에서 핸들러 호출을 스케줄링합니다. 그 결과:

- 어느 모드에서든 핸들러 안에서 `ctx.fetch_members(...)`나 컬렉터를 `await`해도 샤드가 멈추지 않습니다. (3.0 이전 Serial 모드에서는 청크가 기다리는 핸들러 뒤에 쌓여 타임아웃까지 교착했습니다.)
- 핸들러가 읽는 캐시에는 지금 처리 중인 이벤트 이후의 이벤트가 이미 반영돼 있을 수 있습니다.
- 핸들러가 패닉하면 로그만 남고 이후 핸들러 호출은 계속됩니다.

두 단계 모두 `ClientBuilder::event_queue_capacity(...)`(샤드·단계당 기본 10,000개)로 제한됩니다.

| 큐 | 가득 찼을 때 |
|---|---|
| 게이트웨이 → 내부 처리기 | 이벤트를 캐시까지 포함해 통째로 버리고 오류 로그를 남깁니다. 게이트웨이 읽기 루프는 하트비트도 처리하므로 절대 기다리지 않습니다. 내부 처리 자체가 멈출 때만 생깁니다(예: 어떤 태스크가 `ctx.voice()` 쓰기 잠금을 계속 쥐고 있는 경우). |
| 내부 처리기 → 핸들러 | 핸들러에만 전달하지 않습니다. 캐시·컬렉터·`fetch_members`는 이미 이벤트를 받았습니다. 버린 개수는 주기 제한된 경고로 보고합니다. |

어느 큐든 절반이 차면 백로그 경고가 남습니다(샤드당 최대 30초에 한 번).

```rust
use std::time::Duration;
use discordrs::{Client, EventDispatchMode};

Client::builder(&token, intents)
    .event_handler(handler)
    .event_dispatch(EventDispatchMode::Concurrent)
    .max_concurrent_handlers(64)
    .event_queue_capacity(20_000)
    .handler_shutdown_timeout(Duration::from_secs(10))
    .start()
    .await?;
```

### 종료와 오류 (3.0)

`ClientBuilder::start()`는 샤드의 게이트웨이 연결이 끝나면 최대 `handler_shutdown_timeout`(기본 5초) 안에 돌아옵니다. 그 사이 큐에 남은 이벤트는 계속 전달되고 실행 중인 핸들러는 마무리할 수 있으며, 그 뒤에도 남은 작업은 중단됩니다. 반환값은 게이트웨이의 결과이므로 4004(인증 실패), 4014(허용되지 않은 인텐트) 같은 재접속 불가 close code는 `DiscordError::Gateway`로 호출자에게 전달됩니다. `start()` future를 취소해도 샤드의 처리기·핸들러 작업이 함께 멈춥니다. 다중 샤드는 [샤딩](../../docs/api/sharding.md) 문서를 참고합니다.

## 기본 Allowed Mentions와 캐시 백엔드 (2.2.0)

2.2.0에서 `ClientBuilder`에 두 가지 구성 옵션이 추가되었습니다.

```rust
use std::sync::Arc;
use discordrs::cache::CacheBackend;
use discordrs::{AllowedMentions, Client};

fn configure(token: &str, intents: u64, backend: Arc<dyn CacheBackend>) {
    let _builder = Client::builder(token, intents)
        // 페이로드가 allowed_mentions를 직접 지정하지 않은 경우에만 주입됩니다
        // — discord.js의 ClientOptions#allowedMentions에 해당합니다.
        .default_allowed_mentions(AllowedMentions::default())
        // 멤버/메시지/presence 캐시 쓰기를 외부 저장소(Redis, Valkey 등)로
        // 전달합니다. 클라이언트당 하나의 순차 워커가 전달하므로 느린
        // 백엔드가 게이트웨이를 지연시키지 않습니다.
        .cache_backend(backend);
}
```

3.0부터 백엔드는 쓰기를 게이트웨이 순서대로 받고(늦은 put이 뒤의 delete보다 나중에 적용되지 않습니다), READY는 받은 샤드에 대해서만 `CacheBackend::clear_shard_cache`를 호출하며, 부분 MESSAGE_UPDATE는 완전한 메시지로 저장하지 않고 병합합니다. 큐가 가득 찼을 때의 동작은 [캐시와 컬렉터](../../docs/api/cache-and-collectors.md) 문서의 External Cache Backend 절을 참고합니다.

REST 쪽에는 같은 기본값이 `RestClient::builder().default_allowed_mentions(...)`로 존재하며, 핸들러 안에서는 `Context::default_allowed_mentions()`로 구성된 값을 확인할 수 있습니다.

## 게이트웨이 이벤트에서 인터랙션에 응답하기 (2.2.0)

`discordrs::response::InteractionResponder` 트레잇을 임포트하면 `Event::InteractionCreate` 페이로드에 discord.js 스타일로 바로 응답할 수 있습니다.

```rust
use discordrs::response::InteractionResponder;
use discordrs::{Context, Event, Interaction};

async fn on_event(ctx: Context, event: Event) -> Result<(), discordrs::DiscordError> {
    if let Event::InteractionCreate(event) = event {
        if let Interaction::ChatInputCommand(command) = event.interaction {
            command.reply(&ctx.http, "안녕하세요").await?;
        }
    }
    Ok(())
}
```

전체 응답 메서드(`reply_ephemeral`, `defer`, `edit_reply`, `follow_up`, `show_modal`, `respond_autocomplete` 등)는 [HTTP 및 헬퍼 API](../api/http-and-helpers.md)를 참고합니다.

## 게이트웨이로 길드 멤버 가져오기

`Context::fetch_members(...)`는 게이트웨이로 길드 멤버를 요청하고, 대응하는 `GUILD_MEMBERS_CHUNK` 페이로드를 기다려 수집합니다. discord.js의 `guild.members.fetch()`에 해당하며, 가져온 멤버와 presence는 캐시에도 채워집니다.

```rust
// 전체 멤버 (GUILD_MEMBERS privileged 인텐트 필요)
let members = ctx.fetch_members(guild_id, None, None).await?;

// 사용자명 접두사 검색 + 개수 제한
let admins = ctx.fetch_members(guild_id, Some("admin".to_string()), Some(10)).await?;
```

기본 전체 시한은 60초이며 `Context::fetch_members_with_timeout(...)`으로 조정할 수 있습니다. raw 청크 처리가 필요하면 저수준 `ctx.request_guild_members(...)`를 그대로 사용할 수 있습니다.

3.0부터 `fetch_members`는 어떤 핸들러 안에서든 기다려도 안전하고(위 이벤트 파이프라인 참고), 호출이 끝나거나 타임아웃되거나 취소되면 청크 대기 항목이 지워지며, 샤드가 멈추면 즉시 실패합니다. 길드 단위 게이트웨이 명령(`request_guild_members`, `fetch_members`, `update_voice_state`, `join_voice`, `leave_voice`)은 그 길드를 맡은 샤드(`(guild_id >> 22) % shard_count`)가 같은 프로세스에 있으면 그 샤드로 보내므로, 어느 샤드의 `Context`에서든 쓸 수 있습니다.

## 운영 노트

- 핸들러는 스레드를 막지 말고 `await`하도록 작성합니다. 무거운 작업은 백그라운드 태스크로 넘기거나 `EventDispatchMode::Concurrent`를 씁니다. Serial 모드에서 느린 핸들러는 다음 핸들러 호출(캐시 갱신은 아님)을 늦추고, `event_queue_capacity`만큼 밀리면 전달을 버리기 시작합니다.
- 3.0부터 내부 이벤트 처리와 핸들러 호출을 분리하고, 샤드별 두 큐와 동시 핸들러 수에 상한을 두며, READY 캐시 무효화를 받은 샤드로 한정하고, 게이트웨이가 멈추면 `start()`/`ShardSupervisor::wait()`가 곧바로 돌아옵니다(위 참고).
- 2.1부터 IDENTIFY는 Discord의 샤드당 5초 1회 제한에 맞춰 페이싱되고, 30초 미만의 짧은 세션은 즉시 재접속하는 대신 점증 백오프로 재접속하므로 INVALID_SESSION 반복 시의 재접속 루프를 방지합니다.
- 게이트웨이 프로토콜 실패(누락된 Hello, 디코드 오류, 종료 close code, 명령 큐 오버플로)는 `DiscordError::Model`이 아니라 `DiscordError::Gateway`로 표면화됩니다.
- 3.0부터 재접속해도 소용없는 close code(4004 인증 실패, 4010 잘못된 샤드, 4011 샤딩 필요, 4012 잘못된 API 버전, 4013 잘못된 인텐트, 4014 허용되지 않은 인텐트)를 받으면 `GatewayClient::run`이 백오프 재접속을 반복하지 않고 `DiscordError::Gateway`를 돌려주며 샤드를 멈춥니다. 4007(잘못된 시퀀스)과 4009(세션 만료)는 Discord가 거부할 RESUME 대신 새 세션으로 접속합니다. 연결별 하트비트·송신 작업은 어느 경로로 끝나든 정리됩니다.

## 채널 가리기 (3.0)

2026-11-16부터 Discord는 봇이 볼 수 없는 채널을 가려진 메타데이터로 보냅니다(`CHANNEL_CREATE`, `CHANNEL_UPDATE`, `GUILD_CREATE`의 채널 목록). `GET /guilds/{id}/channels`에서는 아예 빠집니다. 이름이 아니라 `Channel::is_obfuscated()`(`CHANNEL_OBFUSCATED` 플래그, `1 << 17`)로 판별합니다. 이런 채널은 `id`, `type`, `position`, `parent_id`만 믿을 수 있고, 권한 덮어쓰기는 `@everyone` 거부 하나만 들어 있는 가짜 값입니다. 봇이 다시 접근할 수 있게 되면 Discord가 전체 정보가 담긴 `CHANNEL_UPDATE`를 보내고, 캐시의 사본도 그걸로 바뀝니다.

마감 전에 시험하려면 IDENTIFY `capabilities`로 미리 켤 수 있습니다(Discord는 이 옵션이 임시라고 밝혔습니다).

```rust
use discordrs::bitfield::gateway_capabilities;

let client = Client::builder(token, intents)
    .gateway_capabilities(gateway_capabilities::CHANNEL_OBFUSCATION)
    .event_handler(handler);
```
