# 인터랙션 엔드포인트 API

HTTP `/interactions` 콜백 처리 모드입니다.

## 기능

- Ed25519 서명 검증
- Axum 라우팅
- 타입 파싱 + 구조화 응답

## 사용 시점

- Slash Command 중심 앱
- 서버리스/HTTP 네이티브 인프라
- Gateway 없이 인터랙션 처리 시

## 처리 흐름

1. `/interactions` HTTP 요청 수신
2. 서명 헤더와 본문 검증
3. 인터랙션 타입·컨텍스트로 파싱
4. 핸들러 실행
5. 반환된 최초 응답을 인터랙션의 공유 응답 상태에 기록(타입드 엔드포인트)
6. 인터랙션 응답 JSON 반환

## 응답 상태 (3.0)

타입드 엔드포인트(`try_typed_interactions_endpoint`, `typed_interactions_endpoint`)는 핸들러에 하나의 `InteractionResponseState`를 공유하는 컨텍스트와 인터랙션을 넘깁니다([HTTP 및 헬퍼](./http-and-helpers.md) 문서의 "공유 응답 상태" 참고). 핸들러가 최초 응답(`Pong`이나 `type`이 1인 `Raw` 페이로드를 제외한 모든 응답)을 반환하면 엔드포인트가 Discord에 답하기 전에 이를 기록합니다. 지연 응답 타입(5, 6)은 `Deferred`, 나머지는 `Replied`가 됩니다.

따라서 spawn한 작업이 보관한 복제본도 인터랙션이 확인(acknowledged)된 것으로 보고 REST로 나머지 작업을 마칠 수 있습니다.

```rust
use discordrs::prelude::*;
use discordrs::{AppContext, AppFramework, InteractionResponse};

let app = AppFramework::builder()
    .command("report", move |ctx: AppContext| {
        let rest = rest.clone();
        async move {
            tokio::spawn(async move {
                let report = build_report().await;
                let _ = ctx.edit_reply(&rest, report).await;
            });
            InteractionResponse::DeferredMessage
        }
    })
    .build();
```

참고:

- 인라인 응답은 Discord가 보낸 요청에 대한 응답 본문이므로, 핸들러가 반환하는 시점에 전달된 것으로 봅니다. Discord가 그 본문을 읽기 직전에 보낸 follow-up은 Discord가 거부할 수 있습니다.
- 핸들러가 이미 REST로 응답했거나(또는 응답이 아직 전송 중이거나) 그 뒤에 또 최초 응답을 반환하면, Discord의 요청에는 답해야 하므로 엔드포인트는 페이로드를 그대로 반환하지만 상태는 REST 응답이 정한 대로 두고 경고 로그를 남깁니다. Discord는 최초 응답을 하나만 받습니다. 인라인으로 반환하거나 REST로 보내거나 둘 중 하나만 사용합니다.
- raw 엔드포인트(`InteractionHandler`, `InteractionContext`)에는 공유 응답 상태가 없습니다. ID와 토큰만 받는 헬퍼는 상태를 공유하지 않습니다.
- 인터랙션 컨텍스트의 `Debug` 출력(타입드·raw, raw 페이로드 포함)은 인터랙션 토큰 대신 `[redacted]`를 표시합니다.

## `AppFramework`

`AppFramework`는 `TypedInteractionHandler`를 구현하고 복제 비용이 작은 `Clone`이므로(복제본은 라우트와 쿨다운 상태를 공유, 3.0) `try_typed_interactions_endpoint(...)`에 바로 넘길 수 있습니다. 라우트 핸들러가 받는 `AppContext`는 `InteractionResponder`를 구현합니다.
