# HTTP 및 헬퍼 API

## DiscordHttpClient

- Discord REST v10 래퍼 (`RestClient`가 기본 명칭, `DiscordHttpClient`는 호환 별칭)
- `429 Too Many Requests` 자동 재시도
- 2.1부터 `RestClient`는 `Clone`이며, 복제본은 rate-limit 상태와 커넥션 풀, application id를 공유합니다
- 2.1의 전송 계층은 모든 요청에 route별 직렬화 게이트를 적용하고, `Retry-After` 헤더와 `x-ratelimit-scope`/`x-ratelimit-global` 헤더를 읽으며, 일시적 오류를 백오프(0.5s/1s/2s)로 재시도합니다
- 3.0부터 5xx 응답·타임아웃·응답 읽기 실패는 Discord가 이미 요청을 처리했을 수 있으므로, 다시 보내도 결과가 같은 요청만 재시도합니다: GET, HEAD, OPTIONS, PUT, DELETE, 그리고 `nonce`와 `enforce_nonce: true`를 함께 지정한 `POST /channels/{id}/messages`(Discord가 중복을 걸러 주는 유일한 경로)입니다. 후자는 Discord의 중복 확인 시간이 "최근 몇 분"이라서 첫 시도 후 60초 안에서만 다시 보냅니다. 그 밖의 POST·PATCH는 메시지·채널·초대가 중복 생성될 위험 대신 오류를 돌려줍니다. 429는 항상 재시도합니다. 모든 메서드를 다시 보내려면 `RestClientBuilder::retry_non_idempotent_requests(true)`를 씁니다
- 3.0부터 이 크레이트가 만든 클라이언트(`RestClient::new`, `RestClientBuilder::build`)는 리다이렉트를 따라가지 않습니다. 그래서 연결 실패는 요청이 나가지 않았다는 증거가 되고 모든 메서드를 재시도합니다. `use_client(...)`로 넣은 클라이언트는 리다이렉트를 따라갈 수 있으므로 연결 실패 후에도 POST·PATCH는 다시 보내지 않습니다
- 3.0부터 `x-ratelimit-bucket` 해시를 채널·서버·웹훅·인터랙션 토큰 같은 major parameter와 묶어서 제한을 겁니다. Discord의 버킷 해시에는 major parameter가 빠져 있어서, 같은 해시를 보고하는 서로 다른 채널이 이제 서로를 막지 않습니다
- 3.0부터 route 키에서 웹훅·인터랙션 토큰을 해시값으로 바꾸고, 전송 오류와 API·프록시 오류 본문에서 요청 토큰을 지웁니다. REST 로그, `RateLimitInfo::route`, `DiscordError::{RateLimit, Api, Http}`에 토큰이 남지 않습니다. 버킷을 알기 전에 줄 서 있던 요청은 버킷 게이트로 옮겨 가서 같은 버킷의 요청과 동시에 나가지 않습니다
- 비정상 대기값(`inf`, `NaN`, 음수, 지나치게 큰 `Retry-After`/`x-ratelimit-reset-after`)은 panic 대신 범위로 잘라 처리합니다

## Discord API 추가 기능 (3.0)

- **대규모 봇 샤딩:** `CurrentUserGuildsQuery { shard: Some(n), .. }`로 `shard` 쿼리 파라미터를 보냅니다. 대규모 봇 샤딩을 쓰는 앱은 2026-09-15부터 `GET /users/@me/guilds`에 이 값이 필수입니다(`0..max_concurrency`, 게이트웨이 샤드 ID와 다름).
- **초대 대상 사용자:** `CreateChannelInvite::target_user_ids`(최대 1000, CSV 업로드와 동시 사용 불가), `add_invite_target_user(code, user_id)`, `remove_invite_target_user(code, user_id)`, `bulk_add_invite_target_users(code, &ids)`, `bulk_delete_invite_target_users(code, &ids)`(최대 1000개, 로컬에서 확인).
- **게임 통계 위젯:** `update_application_identity_profile(user_id, external_id, &body)`(첫 기록은 `Ok(Some(profile))`, 이후 `204`는 `Ok(None)`. `body.data`는 저장된 통계를 통째로 바꾸고, 보내기 전에 `UpdateApplicationIdentityProfile::validate()`로 한도를 검사), `get_application_identity_profile(...)`, `get_user_application_identities(user_id)`, `get_application_identities_by_external_id(provider_type, external_id, provider_id)`, `delete_application_identity(...)`. 외부 ID는 경로 세그먼트 하나로 퍼센트 인코딩하며, `.`·`..`·빈 문자열은 거부합니다.
- **Social SDK 로비와 role connection:** `get_or_create_lobby`, 로비 메시지, 로비 채널 초대(영문 Lobby API 문서 참고), `delete_current_user_application_role_connection(bearer_token)`.

## REST 클라이언트 구성 (`RestClient::builder`, 2.2.0)

`RestClient::builder(token, application_id)`는 discord.js의 `RESTOptions`에 해당하는 `RestClientBuilder`를 반환합니다. `RestClient::new(...)`는 기존의 무설정 동작을 유지합니다.

```rust
use std::sync::Arc;
use std::time::Duration;

use discordrs::{AllowedMentions, RestClient};

let rest = RestClient::builder("bot-token", 0)
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

전체 메서드: `reply`, `reply_ephemeral`, `reply_with_result`, `defer`, `defer_ephemeral`, `edit_reply`, `fetch_reply`, `delete_reply`, `follow_up`, `follow_up_ephemeral`, `edit_follow_up`, `delete_follow_up`, `show_modal(ModalBuilder)`, 그리고 컴포넌트/모달 제출 인터랙션의 `defer_update`/`update_message`와 자동완성 인터랙션의 `respond_autocomplete(Vec<AutocompleteChoice>)`. 전용 메서드가 없는 콜백 타입(예: `LAUNCH_ACTIVITY`)은 `InteractionContextData::create_response(&rest, &InteractionCallbackResponse)`로 같은 상태를 거쳐 보냅니다.

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

### 응답 데이터

`InteractionReplyData`는 `&str`, `String`, `MessageBuilder`, `CreateMessage`에서 변환됩니다. 인터랙션 응답이 받는 모든 필드(`content`, `embeds`, `components`, `flags`, `allowed_mentions`, `poll`)를 그대로 옮깁니다(3.0. 2.2.0은 `poll`을 버렸습니다). 채널 메시지에만 있는 `CreateMessage` 필드(`message_reference`, `sticker_ids`, `nonce`, `enforce_nonce`)는 Discord 인터랙션 본문에 해당 필드가 없으므로 옮기지 않습니다.

### 공유 응답 상태

파싱된 인터랙션마다 `InteractionResponseState` 하나가 생기고, 같은 인터랙션의 모든 복제본(핸들러, 컬렉터, spawn한 작업)이 공유합니다. 단계(`response_phase()`, `InteractionResponsePhase`)는 다음 중 하나입니다.

| 단계 | 의미 | 최초 응답 | `follow_up` / `edit_reply` / `delete_reply` |
|---|---|---|---|
| `Unacknowledged` | 아직 보내지 않았거나 Discord가 마지막 시도를 거부함 | 허용 | 로컬에서 실패 |
| `InFlight` | 최초 응답을 보내는 중 | 로컬에서 실패 | 결과가 나올 때까지 **대기** |
| `Deferred` / `Replied` | Discord가 응답을 수락함 | 로컬에서 실패 | 허용 |
| `Uncertain` | 결과를 알 수 없음(타임아웃, 5xx, 응답 본문 읽기 실패, 보내던 future가 drop됨) | 허용(안전한 재시도) | Discord에 그대로 전달 |

- 슬롯은 Discord가 콜백을 수락한 뒤에만 `Deferred`/`Replied`가 되므로, 다른 복제본의 follow-up이 reply보다 먼저 나가는 경합이 없어졌습니다.
- 4xx, 429, 로컬 검증 실패, 연결 자체가 맺어지지 않은 경우는 `Unacknowledged`로 되돌려 다시 시도할 수 있습니다.
- 타임아웃, 5xx, 응답 본문 읽기 실패, 취소는 `Uncertain`으로 남깁니다. 최초 응답을 다시 보내도 안전합니다. 앞선 시도가 이미 도착했다면 Discord가 40060("already acknowledged")으로 답하고, 두 번째 응답 없이 상태가 확인됨(acknowledged)으로 정리됩니다. `edit_reply`, `fetch_reply`, `delete_reply`가 성공해도 정리됩니다.
- `is_replied()`, `is_deferred()`, `is_acknowledged()`는 Discord가 확인한 내용만 보고하므로 `InFlight`·`Uncertain` 동안에는 false입니다.
- defer된 인터랙션은 discord.js와 같이 `edit_reply` 시 replied로 승격됩니다.
- 타입드 HTTP 인터랙션 엔드포인트도 핸들러가 반환한 인라인 응답을 같은 상태에 기록하므로, 엔드포인트가 응답한 뒤 보관해 둔 복제본으로 `follow_up`할 수 있습니다.

상태는 프로세스 안에서만 유지됩니다. serde로 저장소에서 다시 만든 컨텍스트는 `Unacknowledged`로 시작하므로 `RestClient` 메서드(`create_followup_message_with_application_id` 등)로 follow-up을 보냅니다.

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
use discordrs::model::{CreateGuild, CreateGuildFromTemplate};

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
let level = rest.modify_guild_mfa_level(guild.id.clone(), 1).await?;

// DELETE /guilds/{id} — 봇이 길드 소유자여야 함
rest.delete_guild(from_template.id).await?;
```

## 메시지 전달 (Forward)

`MessageReference::reply(...)`와 `MessageReference::forward(...)`는 타입드 참조(`MessageReferenceType::DEFAULT` / `FORWARD`)를 생성하고, `forward_message(...)`는 한 번의 호출로 메시지를 전달합니다. discord.js의 `message.forward(channel)`에 해당합니다.

```rust
use discordrs::{CreateMessage, MessageReference};

// 같은 채널에서 답장
rest.create_message(
    channel_id,
    &CreateMessage {
        content: Some("확인했습니다!".to_string()),
        message_reference: Some(MessageReference::reply(message_id)),
        ..Default::default()
    },
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
use discordrs::InteractionCallbackResponse;

let result = rest
    .create_interaction_response_with_result(
        interaction_id,
        interaction_token,
        &InteractionCallbackResponse {
            kind: 4,
            data: Some(serde_json::json!({ "content": "완료" })),
        },
    )
    .await?;

if let Some(message) = result.resource.and_then(|resource| resource.message) {
    println!("생성된 메시지: {}", message.id);
}
```

콜백 리소스가 필요 없으면 기존 `create_interaction_response_typed(...)`를 그대로 사용합니다.

## OAuth2 토큰 폐기

`OAuth2Client::revoke_token(token, token_type_hint)`은 `POST /oauth2/token/revoke`를 호출해 사용자 grant를 무효화합니다.

## 응답 헬퍼

- `send_container_message(...)`
- `respond_with_container(...)`
- `respond_component_with_container(...)`
- `respond_modal_with_container(...)`
- `respond_with_modal(...)`
