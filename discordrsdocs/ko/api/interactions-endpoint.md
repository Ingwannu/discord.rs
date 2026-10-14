# 인터랙션 엔드포인트 API

HTTP `/interactions` 콜백 처리 모드입니다.

## 기능

- Ed25519 서명 검증
- Axum 라우팅
- 타입 파싱 + 타입 응답: 핸들러는 `EndpointReply`를 반환하고, 인라인 콜백(`request::InteractionResponse`)은 한 번 인코딩한 바이트로 응답합니다

라우터는 잘못된 공개 키를 오류로 돌려주는 `try_interactions_endpoint` / `try_typed_interactions_endpoint`로 만듭니다. `try_`가 없는 래퍼는 그 오류에서 패닉하며 3.0부터 deprecated입니다.

요청 본문은 기본 1 MiB(`DEFAULT_INTERACTION_BODY_LIMIT`)까지 읽습니다. 더 큰 요청은 서명 검증과 핸들러 전에 `413 Payload Too Large`로 거부되고, Discord는 그 인터랙션을 실패로 표시합니다. 3.0 이전의 64 KiB는 실제 요청도 거부했습니다(포럼 채널 25개를 고른 채널 선택 메뉴, 각 채널 topic 4,096자: 104,741바이트). 다른 한도는 `InteractionsEndpointBuilder`로 정합니다:

```rust
use discordrs::InteractionsEndpointBuilder;

let router = InteractionsEndpointBuilder::new(public_key)
    .body_limit(4 * 1024 * 1024)
    .build_typed(handler)?; // raw 핸들러는 .build(handler)
```

1 MiB를 고른 근거(실제 요청 크기 추정)는 `DEFAULT_INTERACTION_BODY_LIMIT` 문서에 있습니다. 앞단 리버스 프록시도 같은 크기 이상을 허용해야 합니다.

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

## 핸들러 반환 타입 (3.0)

핸들러(`InteractionHandler::handle`, `TypedInteractionHandler::handle_typed`, `AppFramework` 라우트와 가드)는 `request::EndpointReply`를 반환합니다.

| `EndpointReply` | HTTP 응답 |
|---|---|
| `Inline(response)`(보통 `InteractionResponse::...into()`) | `200 OK`, `Content-Type: application/json`, 본문은 콜백 |
| `Accepted` | 본문 없는 `202 Accepted`. 최초 응답은 REST로 보냄 |

`InteractionResponse`는 REST 콜백(`RestClient::create_interaction_response`)과 응답 메서드가 보내는 것과 같은 타입이고, 두 경로가 같은 인코더 하나를 쓰므로 인라인 본문과 REST 본문은 바이트까지 같습니다(REST 클라이언트는 메시지 콜백에 기본 `allowed_mentions`를 더 채웁니다). variant는 Discord 콜백 타입과 일대일입니다.

| Variant | `type` | `data` |
|---|---|---|
| `Pong` | 1 | 없음(PING은 엔드포인트가 직접 응답) |
| `ChannelMessage(InteractionMessage)` / `reply(..)` | 4 | 메시지 |
| `DeferredChannelMessage { ephemeral }` / `defer(..)` | 5 | `{"flags": 0}` 또는 `{"flags": 64}` |
| `DeferredUpdateMessage` | 6 | 없음 |
| `UpdateMessage(UpdateMessage)` / `update(..)` | 7 | 갱신 내용(패치: 생략한 필드는 유지) |
| `AutocompleteResult(Vec<ApplicationCommandOptionChoice>)` / `autocomplete(..)` | 8 | `{"choices": [...]}` |
| `Modal(Modal)` / `modal(..)` | 9 | 모달([빌더](./builders.md) 참고) |
| `PremiumRequired`(Discord가 폐기) | 10 | 없음 |
| `LaunchActivity` | 12 | 없음 |
| `Raw(RawInteractionResponse)` | 임의 | 작성한 JSON 텍스트 그대로 |

`EndpointReply`는 콜백이 아니며 `Serialize`가 없습니다. `Accepted`를 콜백 본문으로 인코딩하거나 보낼 수 없습니다(컴파일 오류, `tests/ui/endpoint_reply_is_not_a_callback.rs`).

## 응답 상태 (3.0)

타입드 엔드포인트(`try_typed_interactions_endpoint`)는 핸들러에 하나의 `InteractionResponseState`를 공유하는 컨텍스트와 인터랙션을 넘깁니다([HTTP 및 헬퍼](./http-and-helpers.md) 문서의 "공유 응답 상태" 참고). 최초 응답은 정확히 하나의 전송 경로만 소유하며, 인라인 응답과 REST 콜백은 같은 슬롯을 원자적으로 선점합니다.

| 핸들러 동작 | 엔드포인트 응답 | 공유 상태 |
|---|---|---|
| 최초 응답을 반환했고 REST 응답은 보내지 않음 | 그 응답을 `200`으로 반환 | `Deferred`(타입 5, 6) 또는 `Replied` |
| 최초 응답을 REST로 보냈거나 보내는 중 | 본문 없는 `202 Accepted`. 반환한 응답은 경고와 함께 버림 | REST 응답의 결과에 따름 |
| `EndpointReply::Accepted` 반환 | 본문 없는 `202 Accepted` | 변화 없음. 나중에 보내는 REST 응답(예: spawn한 작업)이 소유 |

Discord는 HTTP로 받은 인터랙션의 응답을 콜백 엔드포인트로 보낼 때 본문 없는 `202`로 답하도록 규정합니다. 핸들러가 REST로 응답한다면 항상 `EndpointReply::Accepted`를 반환해 엔드포인트가 응답을 버릴 일이 없게 합니다.

인라인 응답 뒤에는 spawn한 작업이 보관한 복제본도 인터랙션이 확인(acknowledged)된 것으로 보고 REST로 나머지 작업을 마칠 수 있습니다.

```rust
use discordrs::prelude::*;
use discordrs::{AppContext, AppFramework};

let app = AppFramework::builder()
    .command("report", move |ctx: AppContext| {
        let rest = rest.clone();
        async move {
            tokio::spawn(async move {
                let report: String = build_report().await;
                let _ = ctx.edit_reply(&rest, report).await;
            });
            InteractionResponse::defer(false)
        }
    })
    .build();
```

참고:

- 인라인 응답은 Discord가 보낸 요청에 대한 응답 본문이므로, 핸들러가 반환하는 시점에 전달된 것으로 봅니다. Discord가 그 본문을 읽기 직전에 보낸 follow-up은 Discord가 거부할 수 있습니다.
- `EndpointReply::Accepted`를 쓰면 Discord의 3초 제한 안에 REST 응답을 보내야 하며, 그렇지 않으면 인터랙션이 실패합니다.
- 엔드포인트는 REST 경로와 마찬가지로 응답 상태를 선점하기 전에 인라인 응답을 인코딩합니다. 인코딩할 수 없으면(타입 variant로는 일어나지 않음) 오류를 기록하고 최초 응답을 기록하지 않은 채 `500`으로 답합니다.
- raw 엔드포인트(`InteractionHandler`, `InteractionContext`)에는 공유 응답 상태가 없습니다. ID와 토큰만 받는 헬퍼는 상태를 공유하지 않습니다.
- 인터랙션 컨텍스트의 `Debug` 출력(타입드·raw, raw 페이로드 포함)은 인터랙션 토큰 대신 `[redacted]`를 표시합니다.

## `AppFramework`

`AppFramework`는 `TypedInteractionHandler`를 구현하고 복제 비용이 작은 `Clone`이므로(복제본은 라우트와 쿨다운 상태를 공유, 3.0) `try_typed_interactions_endpoint(...)`에 바로 넘길 수 있습니다. 라우트 핸들러가 받는 `AppContext`는 `InteractionResponder`를 구현합니다.

라우트는 `EndpointReply`로 변환되는 값을 반환합니다(`InteractionResponse`, 또는 REST로 응답했다면 `EndpointReply::Accepted`). 가드는 `Result<(), EndpointReply>`를 반환합니다.

자동완성(autocomplete)은 명령과 다른 라우트입니다(3.0): `autocomplete(name, handler)`로 등록하고 키는 `RouteKey::Autocomplete(name)`입니다. 2.x에서는 `RouteKey::Command(name)`으로 라우팅되어, 옵션을 입력하기만 해도 명령의 쿨다운이 시작되고 이후 자동완성에는 쿨다운 메시지(자동완성에는 무효)가 가며 정작 명령은 거부됐습니다. 이제 자동완성에는 따로 설정하지 않는 한 쿨다운이 없고, 가드나 쿨다운이 거부하거나 맞는 `autocomplete(...)` 라우트가 없거나(명령 핸들러는 호출하지 않음) 라우트가 다른 응답 타입을 반환하면 빈 선택지로 답합니다. Discord는 자동완성에 `APPLICATION_COMMAND_AUTOCOMPLETE_RESULT`만 받기 때문입니다.

## 2.x에서 옮기기

| 2.x | 3.0 |
|---|---|
| `discordrs::InteractionResponse`(`Value` 페이로드의 엔드포인트 enum) | `request::InteractionResponse`(크레이트 루트와 prelude에도 재수출) |
| `InteractionResponse::Accepted` | `EndpointReply::Accepted` |
| 핸들러·라우트가 `InteractionResponse` 반환 | `EndpointReply` 반환(`response.into()`). 프레임워크 라우트는 `InteractionResponse`를 그대로 반환해도 됨 |
| `ChannelMessage(json!({...}))` | `InteractionResponse::reply(InteractionMessage::new()...)` 또는 `reply("text")` |
| `UpdateMessage(json!({...}))` | `InteractionResponse::update(UpdateMessage::new()...)` |
| `DeferredMessage` | `InteractionResponse::defer(false)`(REST defer처럼 `{"data":{"flags":0}}`을 보냄) |
| `DeferredUpdate` | `InteractionResponse::DeferredUpdateMessage` |
| `Modal(ModalBuilder)` | `InteractionResponse::modal(builder)`(`Modal` 또는 `ModalBuilder`) |
| `type` 키가 있는 `Raw(Value)` | `InteractionResponse::Raw(RawInteractionResponse::new(kind).data(&value)?)` |
| `command(name, ..)`가 자동완성도 받음 | `autocomplete(name, ..)`(`RouteKey::Autocomplete`). 없으면 빈 선택지 |
| 요청 본문 한도 64 KiB(변경 불가) | 기본 1 MiB, `InteractionsEndpointBuilder::body_limit(bytes)` |
