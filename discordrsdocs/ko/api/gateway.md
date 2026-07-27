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

`gateway_intents`는 문서화된 모든 인텐트 비트에 대한 상수를 제공하며, 2.1.0에서 Poll 인텐트가 추가되었습니다.

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

- `EventDispatchMode::Serial`(기본): 샤드의 이벤트를 게이트웨이 순서대로 하나씩 처리합니다.
- `EventDispatchMode::Concurrent`: 각 핸들러 호출이 자체 태스크에서 실행됩니다. 캐시/컬렉터 갱신은 여전히 게이트웨이 순서대로 디스패치 전에 수행되지만, 느린 핸들러 하나가 샤드를 막지 않습니다. 디스패치 큐가 5,000개를 넘으면 경고 로그가 남습니다.

## 게이트웨이로 길드 멤버 가져오기

`Context::fetch_members(...)`는 게이트웨이로 길드 멤버를 요청하고, 대응하는 `GUILD_MEMBERS_CHUNK` 페이로드를 기다려 수집합니다. discord.js의 `guild.members.fetch()`에 해당하며, 가져온 멤버와 presence는 캐시에도 채워집니다.

```rust
// 전체 멤버 (GUILD_MEMBERS privileged 인텐트 필요)
let members = ctx.fetch_members(guild_id, None, None).await?;

// 사용자명 접두사 검색 + 개수 제한
let admins = ctx.fetch_members(guild_id, Some("admin".to_string()), Some(10)).await?;
```

기본 전체 시한은 60초이며 `Context::fetch_members_with_timeout(...)`으로 조정할 수 있습니다. raw 청크 처리가 필요하면 저수준 `ctx.request_guild_members(...)`를 그대로 사용할 수 있습니다.

## 운영 노트

- 2.1.0부터 IDENTIFY는 Discord의 샤드당 5초 1회 제한에 맞춰 페이싱되고, 30초 미만의 짧은 세션은 즉시 재접속하는 대신 점증 백오프로 재접속하므로 INVALID_SESSION 반복 시의 재접속 루프를 방지합니다.
- 게이트웨이 프로토콜 실패(누락된 Hello, 디코드 오류, 종료 close code, 명령 큐 오버플로)는 `DiscordError::Model`이 아니라 `DiscordError::Gateway`로 표면화됩니다.
