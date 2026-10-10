# 핵심 타입 및 상수

## `src/types.rs`

- `Error`, `invalid_data_error`
- `ButtonConfig`, `SelectOption`, `Emoji`
- `MediaGalleryItem`, `MediaInfo`
- 런타임 공유 상태용 `TypeMap`

## `src/constants.rs`

- 컴포넌트 타입 코드
- 버튼 스타일
- 텍스트 인풋 스타일
- 구분선 간격
- 게이트웨이 opcode
- 게이트웨이 인텐트 — 2.1의 Poll 인텐트 `gateway_intents::GUILD_MESSAGE_POLLS`(`1 << 24`)와 `gateway_intents::DIRECT_MESSAGE_POLLS`(`1 << 25`)는 `NON_PRIVILEGED`에 포함되며, 비트 3의 `GUILD_EXPRESSIONS` 별칭도 제공됩니다

## 3.0 신규 타입 (미배포)

- `ChannelFlags` / `channel_flags::{PINNED, REQUIRE_TAG, HIDE_MEDIA_DOWNLOAD_OPTIONS, CHANNEL_OBFUSCATED, IS_SPOILER_CHANNEL}`, `Channel::channel_flags()` / `Channel::is_obfuscated()`. 2026-11-16부터 Discord는 봇이 볼 수 없는 채널을 가려진 메타데이터로 보냅니다. 이런 채널은 `id`, `type`, `position`, `parent_id`만 믿을 수 있고, 이름이 아니라 플래그로 판별해야 합니다.
- IDENTIFY `capabilities` 필드용 `GatewayCapabilities` / `gateway_capabilities::CHANNEL_OBFUSCATION`(가리기를 미리 시험해 보는 Discord의 임시 옵션). `GatewayConnectionConfig::capabilities(...)` 또는 `ClientBuilder::gateway_capabilities(...)`로 설정합니다.
- 웹사이트 Component Embed로 만든 링크 미리보기용 `Embed::components`, `Embed::flags`, `Embed::KIND_COMPONENTS`, `Embed::is_component_embed()`.
- Application Identity Profile 모델: `ApplicationIdentity`, `ApplicationIdentities`, `ApplicationIdentityProfile`, `ApplicationIdentityProfileData`, `PrimaryProfileData`, `DynamicProfileField`(모르는 필드 타입은 그대로 보존), `ProfileMedia`, `UpdateApplicationIdentityProfile`(문서화된 한도 검사용 `validate()` 포함), `ApplicationIdentityProviderId`.
- `CurrentUserGuildsQuery::shard`, `CreateChannelInvite::target_user_ids`, `InviteTargetUserIds`.

## 2.2.0 신규 타입

- `MessageComponent` — `Message.components`를 비롯한 수신 측 페이로드가 raw `serde_json::Value` 대신 관용적(tolerant) 타입드 구조로 디코딩됩니다. 컴포넌트 트리 전체를 하나의 넓은 구조체로 다루며(`kind`가 raw 컴포넌트 `type`을 담고 알려진 모든 필드는 옵션), Discord가 새 컴포넌트 종류를 배포해도 디코딩이 실패하지 않습니다. `MessageComponent::BUTTON`, `STRING_SELECT`, `CONTAINER`, `LABEL` 등 연관 상수가 타입 코드를 제공하고, `iter()`는 컴포넌트와 모든 중첩 자식을 깊이 우선으로 순회합니다.
- `MessageSelectOption`, `MessageSelectDefaultValue`, `UnfurledMediaItem`, `MessageMediaGalleryItem` — 수신 컴포넌트의 타입드 구성 요소입니다.
- `ResolvedData` — 인터랙션 `resolved` 데이터를 `Snowflake` 키의 여섯 개 조회 맵(`users`, `members`, `roles`, `channels`, `messages`, `attachments`)으로 제공하며, ID 기반 접근자(`user(id)`, `member(id)`, `role(id)`, `channel(id)`, `message(id)`, `attachment(id)`)와 `is_empty()`를 포함합니다.
- `MessageInteractionMetadata` — 타입드 `Message::interaction_metadata`(트리거 인터랙션 id/type, 사용자, 승인 통합 소유자, 원본 응답 메시지 id, 대상 사용자/메시지, 모달용 중첩 `triggering_interaction_metadata`). 사용 중단된 `MessageInteraction`보다 이 필드를 사용합니다.
- `discordrs::response`의 `InteractionReplyData`와 `AutocompleteChoice` — 2.2.0 `InteractionResponder` 메서드의 페이로드 타입이며, `InteractionReplyData`는 `&str`, `String`, `MessageBuilder`, `CreateMessage`에서 변환됩니다.
- `discordrs::collector`의 `CollectorEndReason`/`CollectorStopHandle`, HTTP 계층의 `RateLimitInfo`/`RestClientBuilder`, `discordrs::voice::player`의 `AudioPlayerState`/`AudioPlayerEvent`/`TrackEndReason`/`NoSubscriberBehavior`.

## 2.1 신규 타입

- `Guild`가 GUILD_CREATE 컬렉션(`channels`, `threads`, `members`, `voice_states`, `presences`, `emojis`, `stickers`, `stage_instances`, `soundboard_sounds`, `guild_scheduled_events`, `joined_at`, `large`)을 모델링하고 `Guild::without_create_collections()`를 제공합니다.
- `MessageReferenceType`(`DEFAULT` / `FORWARD`)과 `MessageReference::reply(...)` / `MessageReference::forward(...)` 생성자가 추가되었습니다.
- 길드 라이프사이클 요청 본문: `CreateGuild`, `CreateGuildFromTemplate`, `GuildMfaLevel` (`discordrs::model` 경로).
- `with_response=true` 인터랙션 콜백용 `InteractionCallbackResult`, `InteractionCallbackInteraction`, `InteractionCallbackResource`.
- 예약 이벤트 쿼리 타입 `GuildScheduledEventsQuery`, `GuildScheduledEventUsersQuery`.
- 타입드 이벤트 필드 보강: `InviteEvent`(inviter, uses, max_uses, max_age, temporary, created_at, expires_at, target 타입/유저), `ThreadEvent::newly_created`, `ThreadListSyncEvent`(`channel_ids`, `members`), `ThreadMemberUpdateEvent`(`user_id`, `join_timestamp`, `flags`), `ThreadMembersUpdateEvent`의 타입드 `ThreadMember`, 자동 조정 이벤트의 타입드 `AutoModerationTriggerMetadata` / `AutoModerationAction`.
- 오류 분류 헬퍼: `HttpError::is_timeout()` / `is_connect()` / `is_body()` / `is_retryable()`, `DiscordError::is_retryable_transport()`. 게이트웨이 프로토콜 실패는 `DiscordError::Gateway`로 표면화됩니다.

하드코딩 숫자 대신 상수 사용을 권장합니다.
