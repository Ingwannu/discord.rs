# HTTP 및 헬퍼 API

## DiscordHttpClient

- Discord REST v10 래퍼 (`RestClient`가 기본 명칭, `DiscordHttpClient`는 호환 별칭)
- `429 Too Many Requests` 자동 재시도
- 2.1부터 `RestClient`는 `Clone`이며, 복제본은 rate-limit 상태와 커넥션 풀, application id를 공유합니다
- 2.1의 전송 계층은 모든 요청에 route별 직렬화 게이트를 적용하고, `Retry-After` 헤더와 `x-ratelimit-scope`/`x-ratelimit-global` 헤더를 읽으며, 5xx 응답과 일시적 전송 오류를 백오프(0.5s/1s/2s)로 재시도합니다

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
- `rate_limit_callback(...)`은 모든 429마다 `RateLimitInfo { route, retry_after, global }`와 함께 호출됩니다 — discord.js의 `rateLimited` 이벤트에 해당합니다.
- `default_allowed_mentions(...)`는 페이로드가 `allowed_mentions`를 직접 지정하지 않은 경우에만 발신 메시지 페이로드에 주입됩니다. 게이트웨이 쪽에는 `ClientBuilder::default_allowed_mentions(...)`가 있습니다.

## 인터랙션 응답 API (`InteractionResponder`, 2.2.0)

`discordrs::response::InteractionResponder` 트레잇은 응답 가능한 모든 인터랙션 변형에 discord.js 스타일 응답 메서드를 제공합니다. 메서드가 해석되려면 트레잇을 임포트해야 합니다.

```rust
use discordrs::response::InteractionResponder;

// command: ChatInputCommandInteraction (컨텍스트 메뉴, 컴포넌트,
// 모달 제출 인터랙션에서도 동일하게 동작합니다)
command.reply(&rest, "안녕하세요").await?;
command.reply_ephemeral(&rest, "본인에게만 보입니다").await?;

command.defer(&rest).await?;
command.edit_reply(&rest, "완료!").await?;
let followup = command.follow_up(&rest, "추가 안내").await?;
```

전체 메서드: `reply`, `reply_ephemeral`, `reply_with_result`, `defer`, `defer_ephemeral`, `edit_reply`, `fetch_reply`, `delete_reply`, `follow_up`, `follow_up_ephemeral`, `show_modal(ModalBuilder)`, 그리고 컴포넌트/모달 제출 인터랙션의 `defer_update`/`update_message`와 자동완성 인터랙션의 `respond_autocomplete(Vec<AutocompleteChoice>)`. `InteractionReplyData`는 `&str`, `String`, `MessageBuilder`, `CreateMessage`에서 변환됩니다.

응답 상태는 파싱 시점에 생성되어 같은 인터랙션의 복제본 간에 원자적으로 공유됩니다. 중복 reply와 응답 확인(acknowledge) 전의 follow-up은 Discord를 거치지 않고 로컬에서 "already acknowledged" 오류로 실패하고, defer된 인터랙션은 `edit_reply` 시 replied로 승격되며, 상태 슬롯은 HTTP 호출 전에 원자적으로 선점되고 전송 실패 시 롤백됩니다. `is_replied()`, `is_deferred()`, `is_acknowledged()`로 현재 상태를 확인할 수 있습니다.

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
