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

### 권한 계산 (`discordrs::permissions`)

실제로 쓸 수 있는 권한(effective permissions)을 계산합니다. discord.js `channel.permissionsFor(member)`는 덮어쓰기 계산까지만 하며, 그 값은 `ComputedPermissions::raw`로 함께 제공합니다. Discord 문서의 알고리즘을 그대로 따릅니다: 소유자와 `ADMINISTRATOR`는 모든 권한, 기본 권한은 `@everyone`과 멤버 역할의 합, 덮어쓰기는 @everyone → 역할 전체 합산 → 멤버 순서(각각 deny 후 allow), 타임아웃 멤버는 `VIEW_CHANNEL`과 `READ_MESSAGE_HISTORY`만 유지, 스레드는 부모 채널의 덮어쓰기를 사용합니다. 문서화된 암묵 규칙도 적용합니다: `VIEW_CHANNEL`이 없으면 채널 권한 전체 제거(길드 전역 권한은 유지), `SEND_MESSAGES`가 없으면 `MENTION_EVERYONE`·`SEND_TTS_MESSAGES`·`ATTACH_FILES`·`EMBED_LINKS` 제거(스레드에서는 `SEND_MESSAGES_IN_THREADS` 기준이며 `SEND_MESSAGES`는 상속되지 않음), 음성·스테이지 채널에서 `CONNECT`가 없으면 `MANAGE_CHANNELS` 제거. 자세한 내용은 영어 문서 [Permissions and Formatters](#/docs/api/permissions-and-formatters)를 참고하세요.

- 모델 기반: `permissions_for(guild, member, channel, parent, now)`, `guild_permissions(guild, member, now)`. `Channel::permissions_for(...)`와 `Member::guild_permissions(...)`는 현재 시각을 씁니다.
- 캐시 기반: `cached_permissions_for(...)`, `cached_guild_permissions(...)`는 `CacheHandle`의 공개 읽기 API만 쓰고, 빠진 데이터를 추측하지 않고 정확히 알려 줍니다.
- 순수 코어: 명시적 입력용 `MemberPermissionInput` + `ChannelPermissionInput`(`ChannelClass`).
- 결과: `ChannelPermissions::{Known, Obfuscated}`와 `ComputedPermissions`(`permissions`, `raw`, `source: PermissionSource`, `timeout_applied`, `has(...)`, `missing(...)`). 소유자·관리자는 그 사실이 확인되는 즉시 답하므로, 답을 바꿀 수 없는 데이터(다른 역할, 덮어쓰기, 스레드 부모, 타임아웃)가 없어도 실패하지 않습니다. 모순된 길드·채널 정보(DM, 다른 길드의 채널, 스레드를 가질 수 없는 부모 등)는 항상 오류입니다. 2026-11-16부터 오는 가려진 채널(합성된 @everyone VIEW_CHANNEL deny 하나)은 잘못된 답 대신 `Obfuscated`를 돌려줍니다. 소유자·관리자는 덮어쓰기와 무관하므로 그대로 `Known`입니다.
- `PermissionDataError`는 빠지거나 모순된 객체를 이름으로 알려 줍니다(`MissingRole`, `MissingParentChannel`, `MissingPermissionOverwrites`, `GuildMismatch`, `InvalidTimestamp` 등). `DiscordError::Model`로 변환됩니다.
- 소유자·관리자는 `Permissions::all()`을 받고, 권한 비트 이름은 `bitfield::permissions`를 씁니다. 비공개 스레드 멤버십(또는 `MANAGE_THREADS`)과 보관·잠금 상태는 권한 비트가 아니므로 별도로 확인해야 합니다.

### 포매터 (`discordrs::formatters`)

`@discordjs/formatters`와 같은 출력을 냅니다. 포매터는 `impl Display`를 받고 돌려주므로 중첩해도 쓸 때만 할당하고, 텍스트 변환은 바뀐 것이 없으면 입력을 빌린 `Cow<str>`를 돌려줍니다.

- 멘션·링크: `user_mention`, `channel_mention`, `role_mention`, `linked_role_mention`, `game_profile_mention`, `slash_command_mention(...)`, `custom_emoji`, `animated_emoji`, `GuildNavigation`, `hyperlink`, `hyperlink_with_title`, `hide_link_embed`, `channel_link`, `message_link`(및 `dm_` 변형).
- 타임스탬프: `timestamp(secs).style(TimestampStyle::RelativeTime)`, `Timestamp::from(SystemTime)`. `TimestampStyle`은 문서화된 9가지 스타일(`t T d D f F s S R`)을 모두 지원합니다.
- 마크다운: `bold`, `italic`, `underline`, `strikethrough`, `spoiler`, `inline_code`, `code_block`, `code_block_with_language`, `quote`, `block_quote`, `heading`(`HeadingLevel`), `subtext`, `unordered_list`, `ordered_list`, `ListItem`을 쓰는 중첩 목록.
- `escape_markdown(text, EscapeMarkdownOptions)`와 개별 이스케이퍼. 기본값은 배포된 discord.js v14(@discordjs/formatters 0.6.2)와 같고, 커밋된 골든 코퍼스에서 출력이 일치합니다. discord.js 버그 네 가지(같은 줄 `<:` 뒤 `_` 미처리, 자동 링크 안 `_` 이스케이프, 줄당 첫 마스킹 링크만 처리, 백틱 네 개 이상에서 닫는 펜스가 남는 `cleanCodeBlockContent`)는 따르지 않습니다. 공백 판정은 원래 정규식처럼 ECMAScript `\s` 기준입니다. 모든 백틱 연속을 끊는 `clean_code_block_content`도 제공합니다.
- `clean_content(text, &resolver)`: 숨은 캐시 조회 없이 `MentionResolver`(트레이트 또는 클로저, `MentionKind`)로 이름을 받습니다. `Message::clean_content(...)`와 `Member::mention()`이 이를 호출합니다.

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
