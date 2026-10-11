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
5. 최초 응답을 어느 전송 경로가 소유할지 결정(타입드 엔드포인트)
6. 인터랙션 응답 JSON을 `200`으로 반환하거나, 응답을 REST로 보내는 경우 본문 없는 `202` 반환

## 응답 상태 (3.0)

타입드 엔드포인트(`try_typed_interactions_endpoint`, `typed_interactions_endpoint`)는 핸들러에 하나의 `InteractionResponseState`를 공유하는 컨텍스트와 인터랙션을 넘깁니다([HTTP 및 헬퍼](./http-and-helpers.md) 문서의 "공유 응답 상태" 참고). 최초 응답은 정확히 하나의 전송 경로만 소유하며, 인라인 응답과 REST 콜백은 같은 슬롯을 원자적으로 선점합니다.

| 핸들러 동작 | 엔드포인트 응답 | 공유 상태 |
|---|---|---|
| 최초 응답을 반환했고 REST 응답은 보내지 않음 | 그 응답을 `200`으로 반환 | `Deferred`(타입 5, 6) 또는 `Replied` |
| 최초 응답을 REST로 보냈거나 보내는 중 | 본문 없는 `202 Accepted`. 반환한 응답은 경고와 함께 버림 | REST 응답의 결과에 따름 |
| `InteractionResponse::Accepted` 반환 | 본문 없는 `202 Accepted` | 변화 없음. 나중에 보내는 REST 응답(예: spawn한 작업)이 소유 |

Discord는 HTTP로 받은 인터랙션의 응답을 콜백 엔드포인트로 보낼 때 본문 없는 `202`로 답하도록 규정합니다. 핸들러가 REST로 응답한다면 항상 `InteractionResponse::Accepted`를 반환해 엔드포인트가 응답을 버릴 일이 없게 합니다.

인라인 응답 뒤에는 spawn한 작업이 보관한 복제본도 인터랙션이 확인(acknowledged)된 것으로 보고 REST로 나머지 작업을 마칠 수 있습니다.

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
- `InteractionResponse::Accepted`를 쓰면 Discord의 3초 제한 안에 REST 응답을 보내야 하며, 그렇지 않으면 인터랙션이 실패합니다.
- raw 엔드포인트(`InteractionHandler`, `InteractionContext`)에는 공유 응답 상태가 없습니다. ID와 토큰만 받는 헬퍼는 상태를 공유하지 않습니다.
- 인터랙션 컨텍스트의 `Debug` 출력(타입드·raw, raw 페이로드 포함)은 인터랙션 토큰 대신 `[redacted]`를 표시합니다.

## `AppFramework`

`AppFramework`는 `TypedInteractionHandler`를 구현하고 복제 비용이 작은 `Clone`이므로(복제본은 라우트와 쿨다운 상태를 공유, 3.0) `try_typed_interactions_endpoint(...)`에 바로 넘길 수 있습니다. 라우트 핸들러가 받는 `AppContext`는 `InteractionResponder`를 구현합니다.
