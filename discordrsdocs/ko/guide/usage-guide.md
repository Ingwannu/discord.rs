# 사용 가이드

이 문서는 `discordrs`의 실전 사용 흐름을 요약합니다.

## 1. 기능 플래그 선택

- `gateway`: WebSocket Gateway 런타임
- `interactions`: HTTP Interactions Endpoint
- 둘 다 활성화해서 하이브리드 운영 가능

## 2. 기본 패턴

1. Gateway 또는 Endpoint로 이벤트 수신
2. `parse_raw_interaction`, `parse_interaction_context`로 파싱
3. `create_container` 등 빌더로 응답 페이로드 생성
4. `respond_with_container` 계열 헬퍼로 응답

## 3. Slash Command 응답 예시

```rust
let ctx = parse_interaction_context(payload)?;
if let RawInteraction::Command { name, .. } = parse_raw_interaction(payload)? {
    if name.as_deref() == Some("hello") {
        let container = create_container("알림", "명령이 처리되었습니다.", vec![], None);
        respond_with_container(http, &ctx.id, &ctx.token, container, true).await?;
    }
}
```

## 4. Modal 제출 처리

`RawInteraction::ModalSubmit`에서 `V2ModalSubmission`을 받아 Radio/Checkbox 값을 그대로 읽을 수 있습니다.

## 4.5 2.1 주요 추가 기능

- `RestClient::with_reason("...")`: 변경 요청에 `X-Audit-Log-Reason` 헤더를 붙여 길드 감사 로그에 사유를 남깁니다.
- `forward_message(...)`: 한 번의 호출로 메시지를 다른 채널에 전달합니다 (`MessageReference::reply/forward` 생성자 포함).
- `create_interaction_response_with_result(...)`: `with_response=true` 인터랙션 콜백으로 생성된 메시지를 즉시 반환받습니다.
- `create_guild(...)` / `delete_guild(...)` / `create_guild_from_template(...)` / `modify_guild_mfa_level(...)`: 길드 라이프사이클 라우트.
- `Context::fetch_members(...)`: 게이트웨이로 `GUILD_MEMBERS_CHUNK`를 수집합니다 (discord.js `guild.members.fetch()`에 해당).
- `ClientBuilder::presence(...)`와 `event_dispatch(EventDispatchMode::Concurrent)`: IDENTIFY 초기 presence와 동시 이벤트 디스패치.
- `gateway_intents::GUILD_MESSAGE_POLLS` / `DIRECT_MESSAGE_POLLS`: Poll 투표 이벤트 인텐트.

## 4.6 2.2.0 주요 추가 기능

2.2.0은 discord.js 스타일 사용성 계층을 추가합니다.

- `discordrs::response::InteractionResponder`: `interaction.reply(...)`, `reply_ephemeral`, `defer`/`edit_reply`, `follow_up`, `show_modal(ModalBuilder)`, `respond_autocomplete` 응답 메서드. 응답 상태가 원자적으로 공유되어 중복 응답은 Discord를 거치지 않고 로컬에서 실패합니다. 메서드가 해석되려면 트레잇을 임포트해야 합니다.

```rust
use discordrs::response::InteractionResponder;
use discordrs::{Context, Event, Interaction};

async fn on_event(ctx: Context, event: Event) -> Result<(), discordrs::DiscordError> {
    match event {
        Event::InteractionCreate(event) => {
            if let Interaction::ChatInputCommand(command) = event.interaction {
                command.reply(&ctx.http, "안녕하세요").await?;
            }
        }
        Event::MessageCreate(event) => {
            let message = event.message;
            if message.content == "!ping" {
                message.reply(&ctx.http, "pong").await?;
            }
        }
        _ => {}
    }
    Ok(())
}
```

- `discordrs::model_ext` 엔티티 편의 메서드: `message.reply/react/pin/forward_to/start_thread`, `member.kick/ban/timeout/add_role`, `guild.create_channel/fetch_member/ban/icon_url`, `channel.send/mention/is_text_based`, `user.create_dm/dm/tag/display_avatar_url` 등. 고유(inherent) 메서드라 임포트가 필요 없습니다.
- `discordrs::sharding::process`: `ProcessShardManager`(spawn/spawn_auto, 지수 백오프 재spawn, 전체 자식 프로세스 대상 `broadcast(...)` — discord.js `broadcastEval`에 해당)와 `ShardChildProcess::from_env()` 단일 바이너리 부모/자식 패턴. 예제는 `examples/process_sharding_bot.rs`입니다.
- `discordrs::voice::player`: `AudioInput::ffmpeg(url)`, `AudioResource`, `AudioPlayer`와 `TrackStart`/`TrackEnd` 이벤트로 구성된 오디오 재생 파이프라인. 예제는 `examples/music_bot.rs`입니다.
- 컬렉터 강화: `stop()`/`stop_with_reason(...)`, 복제 가능한 `CollectorStopHandle`, `end_reason()`(`Limit`/`Time`/`Idle`/`User`/`ChannelDropped`), `idle(Duration)`, `reset_timer()`, 컴포넌트/모달 컬렉터의 `filter(...)`.
- 빌더 검증: 명령/컴포넌트/임베드/모달/컨테이너/미디어 빌더의 `validate()`와 `try_build()` — Discord 문서화 한도를 로컬에서 검사하고 필드/한도/실제 값을 알려주는 오류를 반환합니다.
- 구성: `RestClient::builder()`(api_base/api_version, 타임아웃, user agent, proxy, 커스텀 클라이언트, rate-limit 콜백, 기본 allowed mentions), `ClientBuilder::default_allowed_mentions(...)`/`cache_backend(...)`, `CacheConfig::sweep_interval(...)`.
- 타입드 모델 보강: `Message.components`가 `iter()`를 제공하는 `Vec<MessageComponent>`로, 인터랙션 `resolved` 데이터가 여섯 개 조회 맵의 `ResolvedData`로, 메시지 인터랙션 메타데이터가 `MessageInteractionMetadata`로 타입화되었습니다.

## 5. 참고

영문 원문 예제 전체는 [English Usage Guide](#/docs/guide/usage-guide)에서 확인할 수 있습니다.
