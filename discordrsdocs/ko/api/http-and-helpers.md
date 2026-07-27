# HTTP 및 헬퍼 API

## DiscordHttpClient

- Discord REST v10 래퍼 (`RestClient`가 기본 명칭, `DiscordHttpClient`는 호환 별칭)
- `429 Too Many Requests` 자동 재시도
- 2.1.0부터 `RestClient`는 `Clone`이며, 복제본은 rate-limit 상태와 커넥션 풀, application id를 공유합니다
- 2.1.0의 전송 계층은 모든 요청에 route별 직렬화 게이트를 적용하고, `Retry-After` 헤더와 `x-ratelimit-scope`/`x-ratelimit-global` 헤더를 읽으며, 5xx 응답과 일시적 전송 오류를 백오프(0.5s/1s/2s)로 재시도합니다

## 감사 로그 사유 (`with_reason`)

`RestClient::with_reason(...)`은 자신이 보내는 모든 변경 요청에 `X-Audit-Log-Reason` 헤더(discord.js의 `encodeURIComponent`와 같은 방식으로 퍼센트 인코딩)를 붙이는 저비용 스코프 복제본을 반환합니다. 차단, 추방, 수정이 길드 감사 로그에 사유와 함께 기록됩니다.

```rust
rest.with_reason("스팸")
    .remove_guild_member(guild_id, user_id)
    .await?;
```

## 길드 라이프사이클 라우트

2.1.0에서 길드 생성/삭제 라우트가 타입드 헬퍼로 추가되었습니다.

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
