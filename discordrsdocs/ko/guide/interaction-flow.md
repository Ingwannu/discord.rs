# 인터랙션 흐름

## 1. 컨텍스트/타입 파싱

```rust
let ctx = parse_interaction_context(payload)?;
let raw = parse_raw_interaction(payload)?;
```

## 2. 타입별 라우팅

```rust
match raw {
    RawInteraction::Command { name, .. } => { /* slash */ }
    RawInteraction::Autocomplete { name, .. } => { /* option suggestions */ }
    RawInteraction::Component { custom_id, .. } => { /* button/select */ }
    RawInteraction::ModalSubmit(submission) => { /* modal */ }
    RawInteraction::Ping => { /* endpoint pong */ }
}
```

## 3. 응답

타입드 엔드포인트에서는 핸들러가 응답을 반환하거나(`InteractionResponse::reply(..)`, `defer(..)`, `modal(..)`, `update(..)`를 `EndpointReply`로 변환), 인터랙션의 응답 메서드(`reply`, `defer`, `show_modal`, `update_message`, `follow_up`, `edit_reply`)로 REST 응답을 보내고 `EndpointReply::Accepted`를 반환합니다. [인터랙션 엔드포인트](../api/interactions-endpoint.md) 참고.

ID와 토큰만 받는 헬퍼는 raw 핸들러용으로 남아 있습니다(상태 비공유).

- `respond_with_container(...)`: Slash Command
- `respond_component_with_container(...)`: 버튼/셀렉트
- `respond_modal_with_container(...)`: 모달 제출
- `respond_with_modal(...)`: 모달 열기

## 4. Ephemeral 처리

응답 헬퍼의 `ephemeral` 플래그로 공개/비공개 응답을 분기합니다.
