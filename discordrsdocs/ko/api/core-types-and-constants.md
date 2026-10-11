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
- `clean_content(text, &resolver)`: 숨은 캐시 조회 없이 `MentionResolver`(트레이트 또는 클로저, `MentionKind`)로 이름을 받습니다. 슬래시 커맨드 이름은 discord.js 문자 클래스와 똑같이 판정합니다(Unicode 17.0 기준으로 생성한 표라 Rust 컴파일러 버전과 무관). `Message::clean_content(...)`와 `Member::mention()`이 이를 호출합니다.

## 타입 ID·판별값·패치 필드 (3.0, 미배포)

아래 기본 타입은 지금 사용할 수 있습니다. 다만 기존 모델·REST 메서드·이벤트·빌더는 아직 `Snowflake`, 맨 `u8` 코드, `Option<Option<T>>`를 씁니다. ID와 판별값은 3단계 시리즈 2–3에서, 패치 필드는 4단계에서 사용처가 이 타입으로 옮겨 갑니다. 설계는 저장소의 `docs/design/3.0-types-events-requests.md`에 있습니다.

### `Id<M>` 타입 ID

- `Id<M>`은 0이 아닌 `u64` snowflake에 마커 타입을 붙인 값입니다. 그래서 `GuildId`가 필요한 자리에 `ChannelId`를 넘길 수 없습니다. 보통은 별칭을 씁니다: `GuildId`, `ChannelId`(스레드 포함), `UserId`, `RoleId`, `MessageId`, `ApplicationId`, `CommandId`, `CommandVersionId`, `InteractionId`, `WebhookId`, `EmojiId`, `AttachmentId`, `StickerId`, `StickerPackId`, `SoundboardSoundId`, `ScheduledEventId`, `StageInstanceId`, `IntegrationId`, `AuditLogEntryId`, `AutoModerationRuleId`, `ForumTagId`, `SkuId`, `EntitlementId`, `SubscriptionId`, `PromotionId`, `RoleSubscriptionListingId`, `LobbyId`, `ActivityLaunchId`, `TeamId`, `OnboardingPromptId`, `OnboardingOptionId`, 그리고 종류를 정적으로 알 수 없을 때 쓰는 `GenericId`. 마커는 `discordrs::model::marker`에 있고 `IdMarker`는 봉인되어 있습니다. `MemberId`, `PresenceId`, `ThreadId`는 없습니다. 멤버와 프레즌스는 `(GuildId, UserId)`로 찾고, 스레드는 채널입니다.
- ID는 `Copy`이고 8바이트입니다. `Option<GuildId>`도 8바이트입니다.
- 생성은 항상 명시적이고 검사를 거칩니다. `GuildId::try_new(u64)`, `"123".parse::<GuildId>()`, `TryFrom<u64>`, `TryFrom<&str>`은 실패하면 `InvalidId`를 돌려줍니다(`kind()`: `Zero`, `Empty`, `InvalidDigit`, `Overflow`). 실패하지 않는 변환은 `From<NonZeroU64>` 하나입니다. `From<u64>`와 `Default`는 없습니다. 마커 사이의 변환은 `erase()`(`GenericId`로)와 `cast::<N>()`뿐입니다. `cast`는 같은 snowflake가 다른 종류의 리소스를 가리킨다는 호출자의 단언일 뿐이고(예: 길드의 `@everyone` 역할), 아무것도 검사하지 않습니다.
- `get()`, `into_nonzero()`, `timestamp()`(Unix 밀리초), `Display`(정규 십진수), `Debug`(`GuildId(123)`)를 제공합니다. 비교·정렬·해시는 숫자 값만 보므로 정렬은 숫자 순서입니다(`9 < 10`).
- 와이어 형식: 십진 문자열로 직렬화하며, JSON 객체 키(`HashMap<GuildId, _>`)로도 쓸 수 있습니다. 역직렬화는 십진 문자열과 부호 없는 정수를 받습니다. 0, 음수, 실수, `u64::MAX`를 넘는 값, 공백, 부호, 그 밖의 숫자가 아닌 문자는 거부합니다. 앞자리 0은 받아들이되 정규화합니다(`"007"`을 읽으면 7이 되고, 다시 쓰면 `"7"`).

```rust
use discordrs::{GuildId, InvalidId};

// 신뢰할 수 없는 입력(슬래시 커맨드 문자열 옵션, 설정 파일)은 경계에서 한 번 검증합니다.
// "0", " 1", "+1", "1.0" 등은 거부됩니다.
fn parse_guild(raw: &str) -> Result<GuildId, InvalidId> {
    raw.parse()
}

fn takes_guild(guild_id: GuildId) {
    // format!("{guild_id:?}") == "GuildId(81384788765712384)"
    // 이 함수에 ChannelId를 넘기면 컴파일 오류입니다.
}
```

### snowflake가 아닌 값

와이어에서는 ID처럼 보이지만 0이 아닌 snowflake가 아닌 값들입니다. 그래서 `Id<M>` 대신 전용 타입을 씁니다.

- `WidgetUserId(String)`: 익명화된 위젯 멤버 `id`입니다(Discord 예시에는 `"0"`). 불투명한 값이라 받은 그대로 보존합니다.
- `GuildInstallOwner`: `authorizing_integration_owners`의 `GUILD_INSTALL` 값입니다. `"0"`(봇 DM에서 발생한 인터랙션)이면 `BotDm`, 아니면 `Guild(GuildId)`입니다.
- `UploadId(u32)`: 같은 요청에서 새로 올리는 파일의 `files[n]` 인덱스입니다. 0을 허용하며, 저장된 첨부의 `AttachmentId`와는 다른 타입입니다.
- `ComponentId(u32)`: 컴포넌트의 숫자 `id`입니다. Discord는 0을 "지정 안 함"으로 보고 값을 새로 매깁니다.
- `PollAnswerId(u32)`: 투표의 `answer_id`입니다.

### 열린 판별값

정수 열거값은 이름 붙은 상수를 가진 투명 newtype입니다(`MessageType::REPLY`, `ComponentType::CONTAINER`, `ButtonStyle::PREMIUM` 등). 각 타입은 `new(raw)`, `get()`, `name()`을 제공합니다. `Debug`는 상수 이름을 출력하고, 이 버전이 모르는 값이면 `Unknown(n)`을 출력합니다. 직렬화는 정수 그대로입니다(`#[serde(transparent)]`). `Default`는 없습니다. 모르는 값도 그대로 왕복하며, 저장 폭보다 큰 값은 잘리지 않고 디코딩 오류가 됩니다. 저장 폭은 모두 `u8`이고 `AuditLogActionType`만 `u16`입니다.

크레이트 루트에서 바로 쓸 수 있는 타입: `OverwriteType`, `VideoQualityMode`, `ForumSortOrder`, `ForumLayout`, `ComponentType`, `ButtonStyle`, `TextInputStyle`, `SeparatorSpacing`, `MessageType`, `MessageActivityType`, `ReactionType`, `PollLayoutType`, `InteractionType`, `InteractionResponseType`, `CommandType`, `CommandOptionType`, `CommandPermissionType`, `RoleConnectionMetadataType`, `VerificationLevel`, `DefaultMessageNotificationLevel`, `ExplicitContentFilterLevel`, `MfaLevel`, `PremiumTier`, `GuildNsfwLevel`, `ScheduledEventStatus`, `ScheduledEventEntityType`, `ScheduledEventPrivacyLevel`, `StageInstancePrivacyLevel`, `AutoModerationEventType`, `AutoModerationTriggerType`, `AutoModerationActionType`, `KeywordPresetType`, `AuditLogActionType`, `InviteType`, `InviteTargetType`, `InviteTargetUsersJobStatusType`, `WebhookType`, `StickerType`, `StickerFormatType`, `SkuType`, `SkuAccessType`(문서화된 값이 없어 이름 없음), `EntitlementType`, `EntitlementOwnerType`, `SubscriptionStatus`, `UserPremiumType`, `ConnectionVisibility`.

일곱 개 이름은 아직 2.x 타입이 쓰고 있습니다. 시리즈 3에서 교체하기 전까지 새 버전은 `discordrs::model::discriminant::{ChannelType, MessageReferenceType, ApplicationIntegrationType, InteractionContextType, ApplicationCommandHandlerType, ActivityType, WebhookPayloadType}` 경로로만 쓸 수 있습니다. 새 `ChannelType`은 스레드 타입을 포함하고 `is_thread()`, `is_voice_based()`, `is_text_based()`(discord.js `isTextBased()`와 같은 집합)를 제공합니다. 이 헬퍼는 채널 종류만 분류합니다. 권한, 보관되거나 잠긴 스레드, 가려진 채널 때문에 여전히 메시지를 못 보낼 수 있으므로, 결과가 메시지를 보낼 수 있다는 뜻은 아닙니다.

### `Patch<T>`, `Nullable<T>`, `NullablePatch<T>`

- `Patch<T>`는 `Missing`(필드 생략) 또는 `Value(T)`이고, `Nullable<T>`는 `Null` 또는 `Value(T)`입니다. 생략·`null`로 지우기·값 지정이 모두 가능한 필드는 `NullablePatch<T>` = `Patch<Nullable<T>>`로 표현합니다.
- 생성자는 `Patch::missing()`, `Patch::value(v)`, 그리고 null 가능 패치에만 있는 `Patch::null()`입니다. `Patch::<bool>::null()`은 컴파일 오류입니다. `Default`는 `Missing`이며 `T: Default`를 요구하지 않습니다.
- 패치 필드는 `#[serde(default, skip_serializing_if = "Patch::is_missing")]`로 선언합니다. 어느 쪽을 빠뜨려도 데이터가 바뀌지 않고 오류가 납니다.
  - `default`가 없으면 생략된 키는 `missing field` 오류입니다. `null`로 바뀌지 않으므로 `NullablePatch`의 "생략"이 조용히 "지우기"가 되는 일은 없습니다.
  - `skip_serializing_if`가 없으면 `Missing` 직렬화가 오류가 납니다. 조용히 `null`이 되지 않습니다.
- `null`일 수 있는 필수 필드는 속성 없이 `Nullable<T>`로 씁니다. `null`은 `Null`로 읽고, 키가 없으면 `missing field` 오류입니다(생략을 `None`으로 읽는 `Option<T>`와 다릅니다).
- 값은 태그 없이 기록합니다. 키가 있으면 `T`로 읽으므로 `Patch<bool>` 필드의 `null`은 디코딩 오류이고, `false`·`0`·`""`·`[]`는 평범한 값입니다.
- Option 변환: `Patch::from(Option<T>)`는 `None`을 `Missing`으로 바꿉니다. `Nullable::from(Option<T>)`는 `None`을 `Null`로 바꾸고, `Option::from(Nullable<T>)`로 되돌릴 수 있습니다.

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
