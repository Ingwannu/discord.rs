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

## 4.5 2.1.0 주요 추가 기능

- `RestClient::with_reason("...")`: 변경 요청에 `X-Audit-Log-Reason` 헤더를 붙여 길드 감사 로그에 사유를 남깁니다.
- `forward_message(...)`: 한 번의 호출로 메시지를 다른 채널에 전달합니다 (`MessageReference::reply/forward` 생성자 포함).
- `create_interaction_response_with_result(...)`: `with_response=true` 인터랙션 콜백으로 생성된 메시지를 즉시 반환받습니다.
- `create_guild(...)` / `delete_guild(...)` / `create_guild_from_template(...)` / `modify_guild_mfa_level(...)`: 길드 라이프사이클 라우트.
- `Context::fetch_members(...)`: 게이트웨이로 `GUILD_MEMBERS_CHUNK`를 수집합니다 (discord.js `guild.members.fetch()`에 해당).
- `ClientBuilder::presence(...)`와 `event_dispatch(EventDispatchMode::Concurrent)`: IDENTIFY 초기 presence와 동시 이벤트 디스패치.
- `gateway_intents::GUILD_MESSAGE_POLLS` / `DIRECT_MESSAGE_POLLS`: Poll 투표 이벤트 인텐트.

## 5. 참고

영문 원문 예제 전체는 [English Usage Guide](#/docs/guide/usage-guide)에서 확인할 수 있습니다.
