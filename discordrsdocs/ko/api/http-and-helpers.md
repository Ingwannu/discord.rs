# HTTP 및 헬퍼 API

## DiscordHttpClient

- Discord REST v10 래퍼 (`RestClient`가 기본 명칭, `DiscordHttpClient`는 호환 별칭)
- `429 Too Many Requests` 자동 재시도
- 2.1부터 `RestClient`는 `Clone`이며, 복제본은 rate-limit 상태와 커넥션 풀, application id를 공유합니다
- 2.1의 전송 계층은 모든 요청에 route별 직렬화 게이트를 적용하고, `Retry-After` 헤더와 `x-ratelimit-scope`/`x-ratelimit-global` 헤더를 읽으며, 일시적 오류를 백오프(0.5s/1s/2s)로 재시도합니다
- 3.0부터 5xx 응답·타임아웃·응답 읽기 실패는 Discord가 이미 요청을 처리했을 수 있으므로, 다시 보내도 결과가 같은 요청만 재시도합니다: GET, HEAD, OPTIONS, PUT, DELETE, 그리고 `nonce`와 `enforce_nonce: true`를 함께 지정한 `POST /channels/{id}/messages`(Discord가 중복을 걸러 주는 유일한 경로)입니다. 후자는 Discord의 중복 확인 시간이 "최근 몇 분"이라서 첫 시도 후 60초 안에서만 다시 보냅니다. 그 밖의 POST·PATCH는 메시지·채널·초대가 중복 생성될 위험 대신 오류를 돌려줍니다. 429는 항상 재시도합니다. 모든 메서드를 다시 보내려면 `RestClientBuilder::retry_non_idempotent_requests(true)`를 씁니다
- 3.0부터 이 크레이트가 만든 클라이언트(`RestClient::new`, `RestClientBuilder::build`)는 리다이렉트를 따라가지 않습니다. 그래서 연결 실패는 요청이 나가지 않았다는 증거가 되고 모든 메서드를 재시도합니다. `use_client(...)`로 넣은 클라이언트는 리다이렉트를 따라갈 수 있으므로 연결 실패 후에도 POST·PATCH는 다시 보내지 않습니다
- 3.0부터 인터랙션 토큰 라우트(최초 콜백, `@original` 조회·수정·삭제, 후속 메시지 생성·조회·수정·삭제. 라우트 매니페스트의 `interaction`)는 봇의 전역 rate limit에 묶이지 않습니다. Discord가 이 라우트들에는 전역 제한을 적용하지 않기 때문에, 다른 요청이 받은 전역 429 때문에 `defer()`가 3초 기한을 넘기는 일이 없습니다. 자기 버킷의 제한은 그대로 따릅니다. `Authorization` 헤더 없이 보내는 요청(인터랙션·웹훅 토큰 라우트, 공개 라우트, 헤더 없는 원시 요청)은 Discord가 IP 주소 단위로 제한하므로 두 번째 전역 기한을 함께 씁니다. 그중 하나가 전역 429를 받으면 다른 토큰의 인터랙션 라우트를 포함해 헤더 없는 요청 모두를 막지만 봇 토큰 요청은 막지 않습니다. 일반 웹훅 토큰 라우트와 원시 요청(`RawRequest`, 클라이언트가 라우트를 모름)은 봇의 전역 제한도 여전히 기다립니다
- 3.0부터 `x-ratelimit-bucket` 해시를 채널·서버·웹훅·인터랙션 토큰 같은 major parameter와 묶어서 제한을 겁니다. Discord의 버킷 해시에는 major parameter가 빠져 있어서, 같은 해시를 보고하는 서로 다른 채널이 이제 서로를 막지 않습니다
- 3.0부터 route 키에서 웹훅·인터랙션 토큰을 해시값으로 바꾸고, 전송 오류와 API·프록시 오류 본문에서 요청 토큰을 지웁니다. REST 로그, `RateLimitInfo::route`, `DiscordError::{RateLimit, Api, Http}`에 토큰이 남지 않습니다. 버킷을 알기 전에 줄 서 있던 요청은 버킷 게이트로 옮겨 가서 같은 버킷의 요청과 동시에 나가지 않습니다
- 비정상 대기값(`inf`, `NaN`, 음수, 지나치게 큰 `Retry-After`/`x-ratelimit-reset-after`)은 panic 대신 범위로 잘라 처리합니다

## REST 표면 (3.0)

3.0은 Discord 작업 하나에 이름 하나, 시그니처 하나만 둡니다.

- **작업마다 정규 메서드 하나.** 2.x의 쌍(`Vec<Value>`를 돌려주는 `get_guild_emojis`와 `get_guild_emojis_typed`)과 별칭(`_from_request`, `_with_body`, `_config`, `update_guild`, `create_role`, `update_role`)은 정규 이름으로 합쳤습니다. 정규 메서드는 구체 요청 타입을 받고 타입 모델을 돌려줍니다. 1:1로 이름만 바뀐 것은 한 릴리스 동안 `#[deprecated]` 전달 메서드로 남고, 제네릭 `B: Serialize` 변형은 없어졌습니다. `_with_query`와 `_with_files` 변형은 유지합니다. 전체 표는 [아래](#2x에서-rest-이름-옮기기-30)에 있습니다.
- **구체 요청 본문.** 메시지 엔드포인트는 `discordrs::request::{CreateMessage, EditMessage, ExecuteWebhook, EditWebhookMessage}`를 받습니다. 다른 도메인의 새 본문도 `discordrs::request`에 있습니다: `ModifyChannel`(`update_channel`), `StartThreadFromMessage`, `StartThreadWithoutMessage`, `StartForumThread` + `ForumThreadMessage`(새 `create_forum_thread(_with_files)`), `CreateAutoModerationRule`/`ModifyAutoModerationRule`, 이모지 4종, 사운드보드 3종, `CreateGuildTemplate`/`ModifyGuildTemplate`, `CreateGuildScheduledEvent`/`ModifyGuildScheduledEvent`, `EditApplicationCommand`. 생성 본문은 `Option` 필드, 수정 본문은 `Patch`/`NullablePatch` 필드이고 `clear_*` setter는 `null`을 보냅니다. 아직 `discordrs::model`에 있는 2.x 본문(`ModifyGuild`, `CreateGuildRole`, `CreateWebhook` 등)은 그대로 쓰고, 나중에 `request`로 옮깁니다. 포럼·미디어 채널의 `available_tags`(`ModifyChannel`, `CreateGuildChannel`)는 `request::AvailableForumTag` 목록입니다. `AvailableForumTag::from(&tag)`는 기존 태그를 id로 유지하고, `AvailableForumTag::new("이름")`은 새 태그를 만들며, 빠진 태그는 삭제됩니다(2.x는 id를 생략할 수 없는 응답 타입 `ForumTag`를 받았습니다).
- **문서화된 예외.** `execute_slack_compatible_webhook`/`execute_github_compatible_webhook`은 Slack·GitHub 형식 본문이라 `&Value`를 받고, `update_lobby_message_moderation_metadata`는 애플리케이션이 정의하는 `HashMap<String, String>`을 받습니다.
- **본문은 한 번만 인코딩합니다.** 본문은 `serde_json::to_vec`으로 바로 바이트가 되고(`serde_json::Value` 트리 없음), 재시도를 포함한 모든 시도가 같은 바이트를 보냅니다. 객체 키는 요청 타입의 필드 순서를 따릅니다.
- **기본 `allowed_mentions`**(`RestClientBuilder::default_allowed_mentions`, `with_default_allowed_mentions`)는 본문이 지정하지 않은 모든 메시지 본문에 인코딩하면서 들어갑니다: `create_message(_with_files)`, `edit_message(_with_files)`, `execute_webhook*`, `edit_webhook_message*`, `create_forum_thread`의 첫 메시지, Components V2 채널 헬퍼(`send_container_message`, `send_to_channel`, `send_components_v2`, `edit_message_with_container`), 그리고 인터랙션 쪽의 메시지 콜백(타입 4·7, `RawInteractionResponse` 제외), follow-up, `@original`·follow-up 수정. 명시한 값, 그리고 수정 본문의 명시적 `null`이 우선합니다. 2.x는 `edit_webhook_message`와 채널 헬퍼에 적용하지 않았습니다.
- **nonce 재전송**은 JSON을 다시 읽지 않고 타입 요청(`CreateMessage::nonce_replay()`, `.enforce_nonce("...")`로 설정)에서 판정하며, Create Message에만 해당합니다.
- **음성 메시지**(`IS_VOICE_MESSAGE`)는 업로드 전에 로컬에서 확인합니다: `audio/*` 파일이 정확히 하나여야 합니다.
- `modify_guild_member`는 `()` 대신 갱신된 `Member`(Discord의 200 본문)를 돌려줍니다. `Member::edit`/`timeout`/`remove_timeout`도 같습니다.

## 원시 요청 (`RawRequest`, 3.0)

`RestClient::request::<T>(RawRequest)`는 임의의 메서드와 경로를 타입 메서드와 같은 전송 계층(route 게이트, rate limit, `with_reason`, 재시도, 오류·로그의 토큰 가리기)으로 보냅니다. `T`는 `DeserializeOwned`면 무엇이든 됩니다(타입 없는 JSON은 `serde_json::Value`). 본문은 항상 디코딩하므로 `()`는 빈 본문이나 `null`만 받습니다. 응답을 버리려면 본문을 디코딩하지 않는 `request_discard(RawRequest)`를 쓰고(Discord가 적용한 변경이 본문 모양 때문에 실패로 보고되지 않습니다), 모양과 무관하게 올바른 JSON만 요구하려면 `serde::de::IgnoredAny`를 쓰세요.

```rust
use discordrs::RawRequest;
use serde_json::{json, Value};

let channel: Value = rest.request(RawRequest::get("/channels/123")).await?;
let created: Value = rest
    .request(RawRequest::post("/channels/123/messages").json(&json!({ "content": "hi" })))
    .await?;
rest.request_discard(RawRequest::put("/channels/123/pins/456")).await?;
```

`json(&body)`는 호출할 때 한 번 인코딩하고(인코딩 오류는 아무것도 보내기 전에 `request`가 돌려줍니다), `json_bytes(bytes)`는 미리 인코딩한 JSON을, `multipart(&payload, &files)`는 `payload_json`과 `files[n]`을 보냅니다. 클라이언트가 원시 요청의 의미를 알 수 없으므로 두 가지는 타입 메서드보다 보수적입니다. POST·PATCH는 `enforce_nonce`가 있어도 모호한 실패 뒤에 다시 보내지 않고(`retry_non_idempotent_requests(true)`일 때만), `Authorization` 헤더는 `bearer(...)`나 `without_authorization()`로 고르지 않으면 경로를 따릅니다(봇 토큰, 단 `/webhooks/{id}/{token}…`과 `/interactions/…`는 헤더 없음). `Debug`는 메서드와 토큰을 가린 경로만 보여 줍니다.

2.x의 `request(method, path, Option<&Value>) -> Value`, `request_typed`, `request_multipart`, `request_typed_multipart`를 대신합니다.

## 라우트 목록과 요청 골든 (3.0)

클라이언트가 보내는 모든 라우트는 `src/http/routes.rs`의 한 행입니다: 고정 라우트 ID, 메서드, 문서 표기 경로, 인증(`bot`, `bearer`, 웹훅 토큰은 `token`, 인터랙션 토큰은 `interaction`, `none`, `client`), 쿼리 파라미터, 요청·응답 타입, 그 라우트를 보내는 메서드. 엔드포인트는 라우트 ID를 전송 계층에 넘기고, 전송 계층은 메서드와 `Authorization` 헤더를 행에서 가져옵니다. `ci/api_drift.py`가 주간 Discord drift 리포트에 같은 행을 읽고, PR마다 도는 `--self-check`가 모든 엔드포인트 메서드가 목록에 있고 자기 라우트를 보내는지 확인합니다. `src/http/goldens/rest_routes.golden`은 모든 라우트의 실제 요청(메서드, 이스케이프된 경로, 쿼리, 인증, 본문 바이트, multipart 파트)을 담고 `cargo test`가 비교합니다.

## Discord API 추가 기능 (3.0)

- **대규모 봇 샤딩:** `CurrentUserGuildsQuery { shard: Some(n), .. }`로 `shard` 쿼리 파라미터를 보냅니다. 대규모 봇 샤딩을 쓰는 앱은 2026-09-15부터 `GET /users/@me/guilds`에 이 값이 필수입니다(`0..max_concurrency`, 게이트웨이 샤드 ID와 다름).
- **초대 대상 사용자:** `CreateChannelInvite::target_user_ids`(최대 1000, CSV 업로드와 동시 사용 불가), `add_invite_target_user(code, user_id)`, `remove_invite_target_user(code, user_id)`, `bulk_add_invite_target_users(code, &ids)`, `bulk_delete_invite_target_users(code, &ids)`(최대 1000개, 로컬에서 확인).
- **게임 통계 위젯:** `update_application_identity_profile(user_id, external_id, &body)`(첫 기록은 `Ok(Some(profile))`, 이후 `204`는 `Ok(None)`. `body.data`는 저장된 통계를 통째로 바꾸고, 보내기 전에 `UpdateApplicationIdentityProfile::validate()`로 한도를 검사), `get_application_identity_profile(...)`, `get_user_application_identities(user_id)`, `get_application_identities_by_external_id(provider_type, external_id, provider_id)`, `delete_application_identity(...)`. 외부 ID는 경로 세그먼트 하나로 퍼센트 인코딩하며, `.`·`..`·빈 문자열은 거부합니다.
- **Social SDK 로비와 role connection:** `get_or_create_lobby`, 로비 메시지, 로비 채널 초대(영문 Lobby API 문서 참고), `delete_current_user_application_role_connection(bearer_token)`.

## REST 클라이언트 구성 (`RestClient::builder`, 2.2.0)

`RestClient::builder(token, application_id)`는 discord.js의 `RESTOptions`에 해당하는 `RestClientBuilder`를 반환합니다. `RestClient::new(...)`는 기존의 무설정 동작을 유지합니다. 두 곳 모두 `application_id`는 `Option<ApplicationId>`입니다. 아직 모르면 `None`을 넘기고(애플리케이션 범위 경로는 `set_application_id(...)` 전까지 오류가 납니다. 게이트웨이 클라이언트는 `READY`에서 설정합니다), `application_id()`도 같은 형태로 돌려줍니다.

```rust
use std::sync::Arc;
use std::time::Duration;

use discordrs::{AllowedMentions, RestClient};

let rest = RestClient::builder("bot-token", None)
    .api_version(10)                     // 또는 .api_base("https://discord.com/api/v10")
    .connect_timeout(Duration::from_secs(5))
    .request_timeout(Duration::from_secs(20))
    .user_agent("my-bot/1.0")
    .proxy("http://localhost:8888")
    .rate_limit_callback(Arc::new(|info| {
        eprintln!("429 발생: {} — {}초 후 재시도 (global: {})", info.route, info.retry_after, info.global);
    }))
    .default_allowed_mentions(AllowedMentions::default())
    .build()?;
```

- `use_client(reqwest::Client)`는 완전 커스텀 클라이언트를 사용하는 escape hatch로, `connect_timeout`/`request_timeout`/`proxy`보다 우선합니다.
- `rate_limit_callback(...)`은 모든 429마다 `RateLimitInfo { route, retry_after, global }`와 함께 호출됩니다 — discord.js의 `rateLimited` 이벤트에 해당합니다. `route`에는 웹훅·인터랙션 토큰이 들어가지 않습니다.
- `retry_non_idempotent_requests(true)`(3.0, 기본값 `false`)는 5xx·타임아웃·응답 유실 뒤에도 POST·PATCH를 다시 보내게 합니다. 이 클라이언트로 보내는 모든 요청이 중복되어도 괜찮을 때만 켭니다.
- `default_allowed_mentions(...)`는 페이로드가 `allowed_mentions`를 직접 지정하지 않은 경우에만 발신 메시지 페이로드에 주입됩니다. 게이트웨이 쪽에는 `ClientBuilder::default_allowed_mentions(...)`가 있습니다.

## 인터랙션 응답 API (`InteractionResponder`, 2.2.0; 응답 상태 3.0)

`discordrs::response::InteractionResponder` 트레잇은 응답 가능한 모든 인터랙션 변형(과 `AppContext`)에 discord.js 스타일 응답 메서드를 제공합니다. `discordrs::prelude`에 포함되어 있으며, 메서드가 해석되려면 트레잇을 임포트해야 합니다.

```rust
use discordrs::prelude::*; // 또는: use discordrs::response::InteractionResponder;

// command: ChatInputCommandInteraction (컨텍스트 메뉴, 컴포넌트,
// 모달 제출 인터랙션에서도 동일하게 동작합니다)
command.reply(&rest, "안녕하세요").await?;
command.reply_ephemeral(&rest, "본인에게만 보입니다").await?;

command.defer(&rest).await?;
command.edit_reply(&rest, "완료!").await?;
let followup = command.follow_up(&rest, "추가 안내").await?;
```

전체 메서드: `reply`, `reply_ephemeral`, `reply_with_result`, `reply_with_files`, `defer`, `defer_ephemeral`, `edit_reply`, `edit_reply_with_files`, `fetch_reply`, `delete_reply`, `follow_up`, `follow_up_ephemeral`, `follow_up_with_files`, `fetch_follow_up`, `edit_follow_up`, `delete_follow_up`, `show_modal`(`Modal` 또는 `ModalBuilder`), 그리고 컴포넌트/모달 제출 인터랙션의 `defer_update`/`update_message`와 자동완성 인터랙션의 `respond_autocomplete(Vec<ApplicationCommandOptionChoice>)`. 그 밖의 콜백(예: `InteractionResponse::LaunchActivity`나 `Raw` 콜백)은 `InteractionContextData::create_response(&rest, &InteractionResponse)`로 같은 상태를 거쳐 보냅니다.

트레잇의 모든 async 메서드는 제네릭 `R: InteractionResponder`를 통해 호출해도 `Send` future를 반환하므로 `tokio::spawn`에 넘길 수 있습니다.

```rust
fn spawn_follow_up<R>(interaction: R, rest: RestClient) -> tokio::task::JoinHandle<()>
where
    R: InteractionResponder + Send + 'static,
{
    tokio::spawn(async move {
        let _ = interaction.follow_up(&rest, "백그라운드 작업 완료").await;
    })
}
```

### 요청 본문 (3.0)

각 메서드는 호출하는 엔드포인트의 요청 타입을 받으며, 모두 `&str`와 `String`에서 변환됩니다.

| 메서드 | 본문 | 비고 |
|---|---|---|
| `reply`, `reply_ephemeral`, `reply_with_result`, `reply_with_files` | `request::InteractionMessage` | 콜백 타입 4의 `data`: content, embeds, components, flags(`ephemeral(true)`), allowed_mentions, attachments, poll, tts |
| `update_message` | `request::UpdateMessage` | 콜백 타입 7, 패치 의미: 생략한 필드는 유지, `clear_content()`는 `content: null` |
| `follow_up`, `follow_up_ephemeral`, `follow_up_with_files` | `request::CreateFollowup` | `POST /webhooks/{app}/{token}` |
| `edit_reply`, `edit_follow_up`, `edit_reply_with_files` | `request::EditInteractionResponse` | 패치 의미, `attachments`는 메시지에 남길 첨부 전체 |

REST 클라이언트의 기본 `allowed_mentions`(`RestClientBuilder::default_allowed_mentions`)는 이 본문들 중 `allowed_mentions`를 비워 둔 것에 채워집니다. 명시한 값(패치의 명시적 `null` 포함)이 우선하고, defer·모달·자동완성 결과에는 넣지 않습니다. 콜백 본문은 응답 슬롯을 선점하기 전에 한 번 준비하므로(인코딩, 빈 파일 이름, 전송과 같은 파서로 파일 content type 검사), 로컬 오류는 아무것도 보내지 않고 상태를 건드리지 않은 채 `DiscordError::Model`로 실패합니다.

`request::CreateMessage`는 `InteractionMessage::try_from(message)?`로 변환합니다(`CreateFollowup`, `UpdateMessage`, `EditInteractionResponse`도 같음). 채널 메시지에만 있는 필드(`message_reference`, `sticker_ids`, `nonce`, `shared_client_theme`)가 설정되어 있으면 변환이 실패합니다. Discord 인터랙션 본문에는 해당 필드가 없어서, 조용히 버리면 메시지 의미가 바뀌기 때문입니다. `CreateMessage`를 받는 헬퍼도 요청을 보내기 전에 같은 이유로 실패합니다. 2.x의 `InteractionReplyData`는 `InteractionMessage`의 deprecated 별칭입니다.

### 공유 응답 상태

파싱된 인터랙션마다 `InteractionResponseState` 하나가 생기고, 같은 인터랙션의 모든 복제본(핸들러, 컬렉터, spawn한 작업)이 공유합니다. 단계(`response_phase()`, `InteractionResponsePhase`)는 다음 중 하나입니다.

| 단계 | 의미 | 최초 응답 | `follow_up` / `edit_reply` / `delete_reply` |
|---|---|---|---|
| `Unacknowledged` | 아직 보내지 않았거나 Discord가 마지막 시도를 거부함 | 허용 | 로컬에서 실패 |
| `InFlight` | 최초 응답을 보내는 중 | 로컬에서 실패 | 결과가 나올 때까지 **대기** |
| `Deferred` / `Replied` | Discord가 응답을 수락함 | 로컬에서 실패 | 허용 |
| `Acknowledged` | Discord에 최초 응답이 있으나 종류를 알 수 없음 | 로컬에서 실패 | 허용 |
| `Uncertain` | 결과를 알 수 없음(타임아웃, 5xx, 응답 본문 읽기 실패, 보내던 future가 drop됨) | 허용(안전한 재시도) | Discord에 그대로 전달 |

- 슬롯은 Discord가 콜백을 수락한 뒤에만 `Deferred`/`Replied`가 되므로, 다른 복제본의 follow-up이 reply보다 먼저 나가는 경합이 없어졌습니다.
- 4xx, 429, 로컬 검증 실패, 연결 자체가 맺어지지 않은 경우는 `Unacknowledged`로 되돌려 다시 시도할 수 있습니다.
- 타임아웃, 5xx, 응답 본문 읽기 실패, 취소는 `Uncertain`으로 남기며, 도착했을 수 있는 응답 종류(defer 또는 reply)를 모두 기억합니다. 어떤 종류로든 최초 응답을 다시 보내도 안전합니다. 앞선 시도가 도착했다면 Discord가 40060("already acknowledged")으로 답하고, 두 번째 응답 없이 상태가 확인됨(acknowledged)으로 정리됩니다. 40060은 *어느* 시도가 도착했는지 알려 주지 않으므로, 불확실한 시도가 모두 같은 종류였을 때만 종류를 알 수 있고 아니면 `Acknowledged`(종류 미상)가 됩니다. `edit_reply`(→ `Replied`), `fetch_reply`, `delete_reply`가 성공해도 정리되며, 재시도가 전송 중일 때 성공해도 그 증거는 유지됩니다.
- `is_replied()`, `is_deferred()`, `is_acknowledged()`는 Discord가 확인한 내용만 보고하므로 `InFlight`·`Uncertain` 동안에는 false이고, `Acknowledged`에서는 `is_acknowledged()`만 true입니다.
- defer된 인터랙션은 discord.js와 같이 `edit_reply` 시 replied로 승격됩니다.
- 타입드 HTTP 인터랙션 엔드포인트도 같은 슬롯을 두고 경쟁합니다. 인라인 응답과 REST 응답 중 정확히 하나만 최초 응답을 소유합니다([인터랙션 엔드포인트](./interactions-endpoint.md) 문서의 "응답 상태" 참고).

전송 중인 응답을 기다리는 데에는 자체 타임아웃이 없습니다. 최초 응답을 보내는 future가 끝나거나 drop되어야 대기가 끝납니다. 같은 작업(task)에서 일부만 poll한 최초 응답 future를 살려 둔 채 follow-up을 기다리지 마십시오(예: `select!`에서 진 분기). 그 future를 drop하거나 계속 poll하지 않으면 follow-up은 영원히 기다립니다.

상태는 프로세스 안에서만 유지됩니다. serde로 저장소에서 다시 만든 컨텍스트는 `Unacknowledged`로 시작하므로 `RestClient` 메서드(`create_followup_message_with_application_id` 등)로 follow-up을 보냅니다.

"연결 실패나 최종 4xx는 콜백이 전달되지 않았다는 뜻"이라는 가정은 리다이렉트를 따르지 않고 콜백을 재전송하지 않는 `RestClient::new`·`RestClientBuilder::build` 클라이언트에서만 성립합니다. 리다이렉트를 따르는 클라이언트를 `RestClientBuilder::use_client`로 넘기거나 `retry_non_idempotent_requests(true)`를 켜면 이 가정이 깨집니다. 이 경우 전달된 응답에 대해 상태가 `Unacknowledged`로 보일 수 있으며, 다음 최초 응답이 40060을 받으면 정리됩니다.

### 헬퍼 함수: 상태 공유형과 상태 비공유형

`InteractionContextData`를 받는 `discordrs::helpers` 함수는 이제 위 메서드에 위임하므로 응답 상태를 공유합니다. 대응하는 응답 메서드가 있는 함수는 3.0에서 deprecated입니다: `defer_interaction`, `defer_update_interaction`, `update_interaction_message`, `respond_with_modal_typed`, `respond_with_autocomplete_choices`, `followup_message`, `get_original_response`, `edit_original_response`, `delete_original_response`, `edit_followup_response`, `delete_followup_response`. `respond_with_message`(대신 `reply`/`reply_ephemeral`), `respond_to_interaction`(`create_response`), `launch_activity`는 아직 deprecated가 아닙니다.

인터랙션 ID와 토큰만 받는 헬퍼는 **상태 비공유형(stateless)** 입니다: `respond_with_container`, `respond_component_with_container`, `respond_modal_with_container`, `respond_with_components_v2`, `respond_component_with_components_v2`, `update_component_with_container`, `respond_with_modal`, `followup_with_container`, `defer_and_followup_container`. 파싱된 인터랙션의 상태를 볼 수 없으므로 중복 응답을 막지 못하고, 인터랙션도 이 헬퍼가 보낸 응답을 알지 못합니다. raw 핸들러(`InteractionContext`)나 ID·토큰만 있는 경우에 사용하고, 한 인터랙션에서 응답 메서드와 섞어 쓰지 않습니다.

## 엔티티 편의 메서드 (`model_ext`, 2.2.0)

`discordrs::model_ext`는 타입드 모델에 discord.js 스타일 고유(inherent) 메서드를 추가합니다. 트레잇 임포트가 필요 없습니다.

```rust
// Message
let reply = message.reply(&rest, "확인했습니다!").await?;
reply.react(&rest, "✅").await?;
message.forward_to(&rest, announce_channel_id).await?;
let thread = message.start_thread(&rest, "후속 논의").await?;

// Member (길드 id를 명시적으로 전달)
member.timeout(&rest, guild_id, "2026-08-01T00:00:00+00:00").await?;
member.kick_with_reason(&rest, guild_id, "스팸").await?;

// Guild, Channel, User
let channel = guild.create_channel(&rest, &body).await?;
channel.send(&rest, "환영합니다!").await?;
user.dm(&rest, "제보 감사합니다").await?;
```

제공 범위: `message.reply/edit/edit_with/delete/react/unreact/pin/unpin/crosspost/forward_to/start_thread/link`, `member.kick/ban/timeout/remove_timeout/add_role/remove_role/edit/display_name`(및 `_with_reason` 변형), `guild.edit/delete/leave/fetch_channels/create_channel/fetch_member/fetch_roles/create_role/ban/unban/kick/set_mfa_level/icon_url/banner_url`, `channel.send/send_message/edit/delete/create_invite/mention/is_text_based/is_voice_based/is_thread`, `role.edit/delete/mention`, `user.create_dm/dm/tag/mention/avatar_url/default_avatar_url/display_avatar_url` — 애니메이션 해시와 기본 아바타를 처리하는 CDN URL 헬퍼가 포함됩니다.

## 감사 로그 사유 (`with_reason`)

`RestClient::with_reason(...)`은 자신이 보내는 모든 변경 요청에 `X-Audit-Log-Reason` 헤더(discord.js의 `encodeURIComponent`와 같은 방식으로 퍼센트 인코딩)를 붙이는 저비용 스코프 복제본을 반환합니다. 차단, 추방, 수정이 길드 감사 로그에 사유와 함께 기록됩니다.

```rust
rest.with_reason("스팸")
    .remove_guild_member(guild_id, user_id)
    .await?;
```

## 길드 라이프사이클 라우트

2.1에서 길드 생성/삭제 라우트가 타입드 헬퍼로 추가되었습니다.

```rust
use discordrs::request::{CreateGuild, CreateGuildFromTemplate};

// POST /guilds — 10개 미만의 길드에 속한 봇만 사용 가능
let guild = rest
    .create_guild(&CreateGuild {
        name: "Support HQ".to_string(),
        ..Default::default()
    })
    .await?;

// POST /guilds/templates/{code}
let from_template = rest
    .create_guild_from_template(
        "template-code",
        &CreateGuildFromTemplate {
            name: "Support HQ 2".to_string(),
            icon: None,
        },
    )
    .await?;

// POST /guilds/{id}/mfa — 갱신된 GuildMfaLevel을 반환
let level = rest.modify_guild_mfa_level(guild.id, discordrs::MfaLevel::ELEVATED).await?;

// DELETE /guilds/{id} — 봇이 길드 소유자여야 함
rest.delete_guild(from_template.id).await?;
```

## 메시지 전달 (Forward)

`MessageReference::reply(...)`와 `MessageReference::forward(...)`는 타입드 참조(`MessageReferenceType::DEFAULT` / `FORWARD`)를 생성하고, `forward_message(...)`는 한 번의 호출로 메시지를 전달합니다. discord.js의 `message.forward(channel)`에 해당합니다.

```rust
use discordrs::request::CreateMessage;

// 같은 채널에서 답장
rest.create_message(
    channel_id,
    &CreateMessage::new().content("확인했습니다!").reply_to(message_id),
)
.await?;

// 다른 채널로 전달 — Discord가 원본 메시지를 `message_snapshots`로 첨부
let forwarded = rest
    .forward_message(channel_id, message_id, announce_channel_id)
    .await?;
```

## `with_response=true` 인터랙션 콜백

`create_interaction_response_with_result(...)`는 `with_response=true`로 인터랙션 콜백을 보내고 타입드 `InteractionCallbackResult` 리소스를 반환합니다. discord.js의 `withResponse: true`에 해당하며, 생성된 메시지를 즉시 확인할 수 있습니다.

```rust
use discordrs::request::InteractionResponse;

let result = rest
    .create_interaction_response_with_result(
        interaction_id,
        interaction_token,
        &InteractionResponse::reply("완료"),
    )
    .await?;

if let Some(message) = result.resource.and_then(|resource| resource.message) {
    println!("생성된 메시지: {}", message.id);
}
```

콜백 리소스가 필요 없으면 `create_interaction_response(...)`를, 파일을 함께 올리려면 `create_interaction_response_with_files(...)`를 씁니다. 셋 모두 HTTP 엔드포인트가 돌려주는 것과 같은 `request::InteractionResponse`를 받습니다. 상태 비공유 호출이므로 파싱된 인터랙션에는 `InteractionResponder` 메서드를 권장합니다. 2.x의 `create_interaction_response_typed(...)`와 그 `InteractionCallbackResponse`(`kind` + JSON `data`)는 3.0에서 삭제했습니다. enum에 없는 콜백은 `InteractionResponse::Raw(RawInteractionResponse)`로 보냅니다. follow-up과 `@original` 엔드포인트는 `request::CreateFollowup` / `request::EditInteractionResponse`를 받습니다.

## OAuth2 토큰 폐기

`OAuth2Client::revoke_token(token, token_type_hint)`은 `POST /oauth2/token/revoke`를 호출해 사용자 grant를 무효화합니다.

## 응답 헬퍼

- `send_container_message(...)`
- `respond_with_container(...)`
- `respond_component_with_container(...)`
- `respond_modal_with_container(...)`
- `respond_with_modal(...)`

채널 헬퍼 `send_container_message`, `send_to_channel`, `send_components_v2`, `edit_message_with_container`는 `request::CreateMessage`/`EditMessage`를 `components_v2(..)`로 만들어 `create_message`/`edit_message`로 보내고 생성·수정된 `Message`를 돌려줍니다(3.0. 2.x는 원시 JSON). `edit_message_with_container`는 컴포넌트와 `IS_COMPONENTS_V2` 플래그만 설정하므로, 내용이나 임베드가 남은 메시지는 Discord가 거부합니다. 그때는 `EditMessage::clear_content()`/`clear_embeds()`와 함께 `edit_message`를 직접 호출하세요.

## 2.x에서 REST 이름 옮기기 (3.0)

"deprecated 전달"은 2.x 이름이 경고와 함께 컴파일되고 3.0 메서드를 호출한다는 뜻이고, "제거"는 없어졌다는 뜻입니다(제네릭 `B: Serialize`나 `&Value` 본문, 또는 정규 메서드가 같은 이름을 타입 시그니처로 쓰게 된 경우). 이름은 같고 시그니처만 바뀐 메서드(`update_channel(&ModifyChannel)`, `execute_webhook(&ExecuteWebhook) -> Message`, `get_guild_emojis() -> Vec<Emoji>` 등)는 영문 문서의 "Same name, new signature" 표를 보세요.

| 2.x | 3.0 | 라우트 | 2.x 이름 |
|---|---|---|---|
| `begin_guild_prune_typed` | `begin_guild_prune` | `BeginGuildPrune` | 제거 |
| `begin_guild_prune_with_body` | `begin_guild_prune` | `BeginGuildPrune` | 제거 |
| `begin_guild_prune_with_request` | `begin_guild_prune` | `BeginGuildPrune` | deprecated 전달 |
| `bulk_overwrite_global_commands_typed` | `bulk_overwrite_global_commands` | `BulkOverwriteGlobalApplicationCommands` | deprecated 전달 |
| `bulk_overwrite_guild_commands_typed` | `bulk_overwrite_guild_commands` | `BulkOverwriteGuildApplicationCommands` | deprecated 전달 |
| `create_application_emoji_typed` | `create_application_emoji` | `CreateApplicationEmoji` | 제거 |
| `create_auto_moderation_rule_typed` | `create_auto_moderation_rule` | `CreateAutoModerationRule` | 제거 |
| `create_channel_invite_typed` | `create_channel_invite` | `CreateChannelInvite` | deprecated 전달 |
| `create_dm_channel_typed` | `create_dm_channel` | `CreateDm` | deprecated 전달 |
| `create_group_dm_channel_typed` | `create_group_dm_channel` | `CreateGroupDm` | deprecated 전달 |
| `create_guild_ban_typed` | `create_guild_ban` | `CreateGuildBan` | 제거 |
| `create_guild_channel_from_request` | `create_guild_channel` | `CreateGuildChannel` | deprecated 전달 |
| `create_guild_channel_typed` | `create_guild_channel` | `CreateGuildChannel` | 제거 |
| `create_guild_emoji_typed` | `create_guild_emoji` | `CreateGuildEmoji` | 제거 |
| `create_guild_scheduled_event_typed` | `create_guild_scheduled_event` | `CreateGuildScheduledEvent` | 제거 |
| `create_guild_sticker_from_request` | `create_guild_sticker` | `CreateGuildSticker` | deprecated 전달 |
| `create_guild_sticker_typed` | `create_guild_sticker` | `CreateGuildSticker` | 제거 |
| `create_interaction_response_typed` | `create_interaction_response` | `CreateInteractionResponse` | 제거 |
| `create_role` | `create_guild_role` | `CreateGuildRole` | 제거 |
| `create_role_typed` | `create_guild_role` | `CreateGuildRole` | 제거 |
| `create_stage_instance_from_request` | `create_stage_instance` | `CreateStageInstance` | deprecated 전달 |
| `create_stage_instance_typed` | `create_stage_instance` | `CreateStageInstance` | 제거 |
| `create_webhook_from_request` | `create_webhook` | `CreateWebhook` | deprecated 전달 |
| `create_webhook_raw` | `create_webhook` | `CreateWebhook` | 제거 |
| `create_webhook_typed` | `create_webhook` | `CreateWebhook` | 제거 |
| `edit_channel_permissions_typed` | `edit_channel_permissions` | `EditChannelPermissions` | deprecated 전달 |
| `edit_current_application_from_request` | `edit_current_application` | `EditCurrentApplication` | deprecated 전달 |
| `get_application_emoji_typed` | `get_application_emoji` | `GetApplicationEmoji` | deprecated 전달 |
| `get_application_emojis_typed` | `get_application_emojis` | `ListApplicationEmojis` | deprecated 전달 |
| `get_auto_moderation_rules_typed` | `get_auto_moderation_rules` | `ListAutoModerationRulesForGuild` | deprecated 전달 |
| `get_channel_webhooks_raw` | `get_channel_webhooks` | `GetChannelWebhooks` | 제거 |
| `get_channel_webhooks_typed` | `get_channel_webhooks` | `GetChannelWebhooks` | deprecated 전달 |
| `get_current_user_guilds_typed` | `get_current_user_guilds` | `GetCurrentUserGuilds` | deprecated 전달 |
| `get_current_user_guilds_typed_with_query` | `get_current_user_guilds_with_query` | `GetCurrentUserGuilds` | deprecated 전달 |
| `get_guild_audit_log_typed` | `get_guild_audit_log` | `GetGuildAuditLog` | deprecated 전달 |
| `get_guild_emoji_typed` | `get_guild_emoji` | `GetGuildEmoji` | deprecated 전달 |
| `get_guild_emojis_typed` | `get_guild_emojis` | `ListGuildEmojis` | deprecated 전달 |
| `get_guild_preview_typed` | `get_guild_preview` | `GetGuildPreview` | deprecated 전달 |
| `get_guild_prune_count_typed` | `get_guild_prune_count` | `GetGuildPruneCount` | deprecated 전달 |
| `get_guild_widget_typed` | `get_guild_widget` | `GetGuildWidget` | deprecated 전달 |
| `get_public_archived_threads` | `list_public_archived_threads` | `ListPublicArchivedThreads` | 제거 |
| `get_voice_regions_typed` | `get_voice_regions` | `ListVoiceRegions` | deprecated 전달 |
| `modify_application_emoji_typed` | `modify_application_emoji` | `ModifyApplicationEmoji` | 제거 |
| `modify_auto_moderation_rule_typed` | `modify_auto_moderation_rule` | `ModifyAutoModerationRule` | 제거 |
| `modify_current_user_voice_state_from_request` | `modify_current_user_voice_state` | `ModifyCurrentUserVoiceState` | deprecated 전달 |
| `modify_guild_emoji_typed` | `modify_guild_emoji` | `ModifyGuildEmoji` | 제거 |
| `modify_guild_member_typed` | `modify_guild_member` | `ModifyGuildMember` | 제거 |
| `modify_guild_onboarding_config` | `modify_guild_onboarding` | `ModifyGuildOnboarding` | deprecated 전달 |
| `modify_guild_onboarding_typed` | `modify_guild_onboarding` | `ModifyGuildOnboarding` | 제거 |
| `modify_guild_role_positions_typed` | `modify_guild_role_positions` | `ModifyGuildRolePositions` | deprecated 전달 |
| `modify_guild_scheduled_event_typed` | `modify_guild_scheduled_event` | `ModifyGuildScheduledEvent` | 제거 |
| `modify_guild_sticker_from_request` | `modify_guild_sticker` | `ModifyGuildSticker` | deprecated 전달 |
| `modify_guild_sticker_typed` | `modify_guild_sticker` | `ModifyGuildSticker` | 제거 |
| `modify_guild_welcome_screen_config` | `modify_guild_welcome_screen` | `ModifyGuildWelcomeScreen` | deprecated 전달 |
| `modify_guild_welcome_screen_typed` | `modify_guild_welcome_screen` | `ModifyGuildWelcomeScreen` | 제거 |
| `modify_guild_widget` | `modify_guild_widget_settings` | `ModifyGuildWidget` | deprecated 전달 |
| `modify_guild_widget_settings_typed` | `modify_guild_widget_settings` | `ModifyGuildWidget` | 제거 |
| `modify_stage_instance_from_request` | `modify_stage_instance` | `ModifyStageInstance` | deprecated 전달 |
| `modify_stage_instance_typed` | `modify_stage_instance` | `ModifyStageInstance` | 제거 |
| `modify_user_voice_state_from_request` | `modify_user_voice_state` | `ModifyUserVoiceState` | deprecated 전달 |
| `modify_webhook_from_request` | `modify_webhook` | `ModifyWebhook` | deprecated 전달 |
| `modify_webhook_typed` | `modify_webhook` | `ModifyWebhook` | 제거 |
| `modify_webhook_with_token_from_request` | `modify_webhook_with_token` | `ModifyWebhookWithToken` | deprecated 전달 |
| `modify_webhook_with_token_typed` | `modify_webhook_with_token` | `ModifyWebhookWithToken` | 제거 |
| `update_channel_typed` | `update_channel` | `ModifyChannel` | 제거 |
| `update_guild` | `modify_guild` | `ModifyGuild` | 제거 |
| `update_guild_typed` | `modify_guild` | `ModifyGuild` | 제거 |
| `update_message` | `edit_message` | `EditMessage` | 제거 |
| `update_message_with_files` | `edit_message_with_files` | `EditMessage` | 제거 |
| `update_role` | `modify_guild_role` | `ModifyGuildRole` | 제거 |
| `update_role_typed` | `modify_guild_role` | `ModifyGuildRole` | 제거 |
