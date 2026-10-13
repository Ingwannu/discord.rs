# 2.x에서 3.0으로 옮기기

이 문서는 discord.rs **2.0.2**(crates.io에 배포된 마지막 버전) 또는 **2.2.0**(`main`에 있던 미배포 2.x)으로 작성한 봇을 3.0으로 옮기는 방법을 정리합니다. 두 2.x 버전이 다른 곳은 따로 적고, "2.x"는 둘 다를 뜻합니다. CHANGELOG "Unreleased (3.0.0)"에 적힌 깨지는 변경을 영역별로 다루며, 긴 목록(REST 이름 변경 전체, 판별값 타입 전체, 타입이 바뀐 모델 필드 전체)은 API 문서의 전체 표로 연결합니다. 코드 블록은 영어 가이드([Migrating from 2.x to 3.0](../../docs/guide/migration-3.0.md))와 같습니다.

<!--
이 문서의 `rust` 블록은 tests/migration_guide.rs가 검사한다. `<!- - snippet: NAME - ->`
뒤의 블록은 그 파일의 NAME 영역과 같아야 하고(CI가 컴파일하며, Discord 연결이 필요 없는
것은 실행도 한다), `<!- - snippet: 2.x - ->` 블록은 2.x 코드라 컴파일하지 않는다.
4단계 체크포인트 C 이후 다시 확인할 항목은 영어 가이드 상단 주석의 TODO 목록과 같다.
-->

## 왜 3.0인가

2.1.0과 2.2.0은 배포되지 않았지만 이미 2.0.2의 공개 타입을 깨뜨렸기 때문에, 3.0은 깨지는 변경을 한 번에 모아 곧바로 4.0을 강요하지 않도록 했습니다. 2.x를 호환되게 고칠 수 없던 런타임 버그(응답을 잃은 REST 요청이 메시지를 두 번 보낼 수 있었고, Serial 모드에서 `fetch_members`를 기다리는 핸들러가 샤드를 교착시켰고, READY마다 다른 샤드의 캐시까지 지웠고, 음성 패킷을 잘못된 nonce 배치로 암호화했습니다)를 고치고, 타입을 숫자 ID·열린 판별값·공유 이벤트·타입 요청 본문 위에 다시 세웠습니다. 효과는 `tests/perf_report`(결정적 할당 수, 방법은 `docs/design/perf-baseline.md`)로 쟀습니다: 멤버 1,000명 GUILD_CREATE 디코딩이 **할당 508,698회 → 20,921회**(최대 힙 34.7 → 3.2 MB), MESSAGE_CREATE **573 → 23회**, INTERACTION_CREATE 1,519 → 66회, 큰 길드 25개를 받은 뒤 live 힙 **386 MB → 87 MB**, `Event` 4,848 → 128바이트, Components V2 메시지 인코딩 **313 → 26회**(인터랙션 응답 406 → 27회)입니다.

## 시작하기 전에

- **Rust 1.88** 이상. 3.0은 `rust-version = "1.88"`을 선언하고, CI가 그 툴체인으로 모든 기능을 빌드·테스트합니다. (2.x는 선언하지 않았습니다.)
- **기능(feature) 이름은 그대로**입니다: `cache`(기본), `gateway`, `collectors`, `sharding`, `zstd-stream`, `voice`, `voice-encode`, `dave`, `interactions`.

```toml
[dependencies]
discordrs = { version = "3", features = ["gateway", "collectors"] }
```

- **의존성.** discord.rs는 더 이상 `async-trait`(다른 곳에서 쓰지 않으면 `Cargo.toml`에서 지웁니다)와 `hex`(`From<hex::FromHexError> for DiscordError` 삭제)를 쓰지 않습니다. `interactions` 기능은 **axum 0.8** 기반이라 반환되는 `Router`도 axum 0.8 라우터입니다. axum 0.8로 서빙합니다.
- **작업 순서.** 먼저 discord.rs 트레잇 impl의 `#[async_trait]`를 모두 지웁니다(8절 "async-trait 제거"). 남은 속성은 `E0195`로 실패하며 다른 오류를 가립니다. 그다음 아래 순서대로 절마다 컴파일 오류를 고치고, 이어서 deprecation 경고를 처리합니다. 모든 deprecated 항목은 대체 항목을 알려 주며 다음 메이저 버전에서 삭제됩니다. 마지막으로 [컴파일은 되지만 동작이 바뀐 것](#컴파일은-되지만-동작이-바뀐-것)을 읽습니다.

## 1. 타입 ID

**이유:** 2.x `Snowflake`는 `String`과 파싱한 숫자를 함께 들고 아무 텍스트나 받았고, 길드 ID 자리에 채널 ID를 넣어도 컴파일됐습니다. 3.0 ID는 `Copy`, 8바이트, 0이 아님이 보장되고, 경계에서 한 번 검사하며, 리소스별 타입(`GuildId`, `ChannelId`, `UserId`, `RoleId`, `MessageId`, `ApplicationId`, …)이라 섞어 쓰면 컴파일 오류입니다. `Message`는 6,360바이트에서 4,808바이트로 줄었습니다.

<!-- snippet: 2.x -->
```rust
// 2.x
use discordrs::Snowflake;

let guild_id = Snowflake::from("81384788765712384");
let channel_id: Snowflake = 41771983423143937u64.into();
let channel = rest.get_channel(channel_id.clone()).await?;
let member = cache.member(&guild_id, &user_id).await;
println!("{}", user_id.as_str());
```

<!-- snippet: ids -->
```rust
use discordrs::{ChannelId, DiscordError, GuildId, RoleId, UserId};

// Untrusted text (config, a string command option) is parsed once, at the
// boundary. "0", " 1", "+1", and "1.0" are rejected.
fn parse_ids(guild: &str, channel: u64) -> Result<(GuildId, ChannelId), DiscordError> {
    let guild_id: GuildId = guild.parse()?;
    let channel_id = ChannelId::try_from(channel)?;
    Ok((guild_id, channel_id))
}

// IDs are `Copy` and 8 bytes: pass them by value, no `.clone()`.
fn describe(guild_id: GuildId, user_id: UserId) -> String {
    let everyone: RoleId = guild_id.everyone_role();
    format!(
        "user {user_id} (raw {}) in guild {guild_id}, @everyone = {everyone}",
        user_id.get()
    )
}
```

형제 필드에 따라 ID 종류가 달라지는 필드는 2.x와 같은 JSON을 읽고 쓰는 enum입니다:

<!-- snippet: polymorphic_ids -->
```rust
use discordrs::model::{OverwriteTarget, PermissionOverwrite};
use discordrs::DiscordError;

fn overwrite_owner(json: &str) -> Result<String, DiscordError> {
    let overwrite: PermissionOverwrite = serde_json::from_str(json)?;
    Ok(match overwrite.target {
        OverwriteTarget::Role(role_id) => format!("role {role_id}"),
        OverwriteTarget::Member(user_id) => format!("member {user_id}"),
        OverwriteTarget::Unknown { kind, id } => format!("type {kind:?} for {id}"),
    })
}
```

| 2.x | 3.0 |
|---|---|
| `Snowflake`(`String` + `u64`) | 타입 별칭(`GuildId`, `UserId`, …). Discord가 종류를 정하지 않은 곳에만 `GenericId`. `Snowflake`는 `GenericId`의 deprecated 별칭 |
| `Snowflake::new`, `From<u64>`, `From<&str>`, 실패하지 않는 `FromStr` | `"…".parse()?`, `GuildId::try_from(u64)?`, `GuildId::try_new(u64)?`(`InvalidId`, `?`로 `DiscordError`가 됨) |
| `impl Into<Snowflake>` / `&Snowflake` 인자 | 구체 ID를 값으로: `rest.get_channel(channel_id)`, `cache.member(guild_id, user_id)` |
| `as_str()`, `as_u64()` / `to_u64()`, `timestamp() -> Option` | `to_string()` / `Display`, `get()`, `timestamp() -> u64` |
| `PermissionOverwrite { id, kind }` | `target: OverwriteTarget`(`Role`, `Member`, `Unknown`). 덮어쓰기 엔드포인트는 `OverwriteTarget`을 받음 |
| `ApplicationCommandPermission { id, kind }`, `u8` `ROLE`/`USER`/`CHANNEL` | `target: CommandPermissionTarget`(`Everyone`, `AllChannels` 센티널) |
| `CreateTestEntitlement { owner_id, owner_type }` | `CreateTestEntitlement::new(sku_id, EntitlementOwner::Guild(guild_id))` |
| `MessageSelectDefaultValue` | `SelectDefaultValue` enum(deprecated 별칭) |
| `RestClient::new(token, 0)`(`u64` 애플리케이션 ID) | `RestClient::new(token, None)`. 모든 곳이 `Option<ApplicationId>` |
| `Message`, `User`, `Guild`, `Channel` 등의 `Default` | 삭제(기본값에는 가짜 ID가 필요). 테스트 모델은 JSON으로 만들고 새 생성자(`CreateDmChannel::new`, `LobbyMember::new`, …)를 씀 |
| `DiscordModel::id() -> &Snowflake`, `id_opt() -> Option<&Snowflake>` | `type Marker`. `id() -> Id<Self::Marker>`를 값으로(`User::id()`는 `UserId`), `id_opt() -> Option<Id<Self::Marker>>` |
| `impl Into<Snowflake>`를 받고 `gateway` 기능 없이는 동기였던 `CachedManager<T>` | `Id<T::Marker>`를 받는 `CachedManager<T: DiscordModel>`, 모든 빌드에서 async(8절) |
| `GuildApplicationCommandPermissions::id: Snowflake`(커맨드, 또는 앱 전체 권한이면 애플리케이션) | `CommandPermissionsId::{Command(CommandId), Application(ApplicationId)}` |
| `authorizing_integration_owners: HashMap<String, Snowflake>` | `AuthorizingIntegrationOwners { guild_install: Option<GuildInstallOwner>, user_install: Option<UserId>, other }`. `"0"`(봇과의 DM)은 `GuildInstallOwner::BotDm` |
| 빌더의 `id(u32)` setter, `MessageComponent::id`(2.2.0) | `id(ComponentId)`(`ComponentId::new(12)`), `MessageComponent::id: Option<ComponentId>` |

0, 부호, 빈 값, 숫자가 아닌 ID는 이제 디코딩 오류입니다. ID처럼 보이지만 snowflake가 아닌 값은 자기 타입이 있습니다(`WidgetUserId`, `GuildInstallOwner`, `ComponentId`, `PollAnswerId`, `UploadId`, deprecated 길드 생성 본문의 `CreateGuildPlaceholderId`). `edit_channel_permissions`는 `type`이 `OverwriteTarget`과 어긋나는 본문을 보내기 전에 거부합니다. 3.0에 추가된 포매터도 같은 규칙을 따릅니다: `MentionResolver` 메서드는 `UserId` / `RoleId` / `ChannelId`를 받고, 클로저 resolver는 `Fn(MentionTarget) -> Option<String>`입니다. prelude는 `GuildId`, `ChannelId`, `UserId`, `RoleId`, `MessageId`, `ApplicationId`, `InvalidId`를 내보내고, 나머지 별칭은 크레이트 루트에서 가져옵니다. 전체 목록: [핵심 타입 및 상수](../api/core-types-and-constants.md)의 "2.x `Snowflake`에서 옮기기".

## 2. 열린 판별값

**이유:** 2.x는 정수 열거값을 `u8`/`u64`(또는 새 값을 거부하는 닫힌 enum)로 저장해서 `channel.kind == 0`이 컴파일됐고, Discord가 나중에 추가한 값은 실패하거나 사라졌습니다. 3.0에서는 모두 이름 붙은 상수가 있는 열린 newtype이며, 모르는 값도 보관하고 그대로 다시 씁니다. 필드 하나만 일부러 원시 숫자로 남았습니다: `MessageComponent::style`(`Option<u8>`)은 의미가 컴포넌트 타입에 따라 달라서 `button_style()` / `text_input_style()`로 읽습니다.

<!-- snippet: 2.x -->
```rust
// 2.x
use discordrs::model::ChannelType;
use discordrs::{button_style, ButtonBuilder};

if channel.kind == 0 || channel.kind == ChannelType::News as u8 { /* text */ }
let button = ButtonBuilder::new().style(button_style::PRIMARY);
let activity = Activity::default(); // silently PLAYING
```

<!-- snippet: discriminants -->
```rust
use discordrs::{Channel, ChannelType, MessageType};

fn describe(channel: &Channel) -> &'static str {
    // Constants work as patterns, but the set is open: keep a `_` arm.
    match channel.kind {
        ChannelType::GUILD_TEXT | ChannelType::GUILD_ANNOUNCEMENT => "text",
        kind if kind.is_thread() => "thread",
        kind if kind.is_voice_based() => "voice",
        _ => "other",
    }
}

fn raw_numbers() {
    let kind = ChannelType::new(0); // was `0u8` or `ChannelType::Text as u8`
    assert_eq!(kind, ChannelType::GUILD_TEXT);
    assert_eq!(kind.get(), 0);
    // A value this version does not know is kept, not rejected.
    let future = MessageType::new(250);
    assert_eq!(format!("{future:?}"), "Unknown(250)");
}
```

| 2.x | 3.0 |
|---|---|
| `kind: u8` 필드, `channel.kind == 0` | `ChannelType::GUILD_TEXT`. 원시 숫자는 `ChannelType::new(n)` / `kind.get()` |
| 닫힌 `ChannelType::{Text, Dm, Voice, News, …}`(`as u8`) | `ChannelType::{GUILD_TEXT, DM, GUILD_VOICE, GUILD_ANNOUNCEMENT, …}`. 스레드 타입 포함, `is_thread()` / `is_voice_based()` / `is_text_based()` |
| `ActivityType(0)`, `x.0`, `Default` | `ActivityType::PLAYING` / `ActivityType::new(0)`, `x.get()`, `Default` 없음(`Activity::new(kind, name)`) |
| `constants::{component_type, button_style, separator_spacing, text_input_style}`, `command::{command_type, option_type}` | `ComponentType`, `ButtonStyle`, `SeparatorSpacing`, `TextInputStyle`, `CommandType`, `CommandOptionType` |
| `helpers::INTERACTION_RESPONSE_*`, `bitfield::interaction_callback_type` | `InteractionResponseType`(`CHANNEL_MESSAGE` → `CHANNEL_MESSAGE_WITH_SOURCE`, `DEFERRED_CHANNEL_MESSAGE` → `DEFERRED_CHANNEL_MESSAGE_WITH_SOURCE`, `AUTOCOMPLETE_RESULT` → `APPLICATION_COMMAND_AUTOCOMPLETE_RESULT`) |
| `MessageComponent::BUTTON` 등 `u8` 상수 | `ComponentType::BUTTON` |
| `CommandOptionBuilder::new(u8, ..)`, `ButtonBuilder::style(u8)`, `channel_types(Vec<u8>)` | newtype(`channel_types`는 `IntoIterator<Item = ChannelType>`) |
| `ApplicationCommand::kind: u8`, `CommandDefinition::kind: u8` | `Option<CommandType>`: `None`은 `CHAT_INPUT`(빌더는 항상 설정). `ApplicationCommand::command_type()`이 그 기본값을 적용 |
| `MessageComponent::style: Option<u64>`(2.2.0), `component.style == Some(1)` | `Option<u8>` 원시값으로 유지: `component.button_style() == Some(ButtonStyle::PRIMARY)`, `text_input_style()` |
| `Presence::status: Option<String>`, `UpdatePresence::status: String` | `Option<PresenceStatus>`, `PresenceStatus`(`PresenceStatus::ONLINE`, `PresenceStatus::new(..)`, `&str`과 비교 가능) |

필수 판별값(컴포넌트 `type`, activity `type`, poll `layout_type` 등)이 없으면 이제 `0`/`PLAYING`이 아니라 디코딩 오류이고, 저장 폭을 넘는 값은 잘리지 않고 실패합니다. 판별값이 필수인 타입은 `Default`를 잃었습니다(`Activity`, `MessageComponent`, `Poll`, `ApplicationCommandOption`, `GuildMfaLevel`, `EditChannelPermission`, …). `Activity::new`, `MessageComponent::new`, `ApplicationCommandOption::new`, `ApplicationRoleConnectionMetadata::new`, `GuildMfaLevel::new`, `EditChannelPermission::for_target`를 씁니다. 생성자가 인자를 받는 빌더(`SelectMenuBuilder`, `TextInputBuilder`, `CommandOptionBuilder`, 커맨드 빌더)도 `Default`를 잃었습니다. 전체 표: [핵심 타입 및 상수](../api/core-types-and-constants.md)의 "2.x 판별값에서 옮기기".

## 3. Patch와 Nullable

**이유:** 갱신은 필드를 생략(유지)하거나, `null`을 보내거나(지움), 값을 보냅니다. 2.x는 부분 갱신을 전체 구조체로 읽어 생략된 필드가 빈 필드처럼 보였고, 요청 본문은 세 경우를 `Option<Option<T>>`로 표현했습니다. 3.0은 이름을 붙입니다: `Patch<T>`는 `Missing` 또는 `Value(T)`, `Nullable<T>`는 `Null` 또는 `Value(T)`, `NullablePatch<T> = Patch<Nullable<T>>`. 필수 필드가 없으면 조용한 `null`이 아니라 디코딩 오류입니다.

<!-- snippet: 2.x -->
```rust
// 2.x
// Request: hand-written JSON, or Option<Option<T>> fields.
rest.update_channel(channel_id, &json!({ "name": "announcements", "topic": null })).await?;
// Event: MESSAGE_UPDATE arrived as a whole `Message`; omitted fields read as
// empty, so `message.content == ""` could mean "not edited" or "cleared".
async fn message_update(&self, _ctx: Context, message: Message) { /* ... */ }
```

요청 본문에서는 setter가 패치를 써 줍니다:

<!-- snippet: patch_request -->
```rust
use discordrs::request::ModifyChannel;

fn rename_and_clear_topic() -> Result<String, serde_json::Error> {
    // Omitted fields are not sent; `clear_*` sends an explicit `null`.
    let body = ModifyChannel::new().name("announcements").clear_topic();
    serde_json::to_string(&body) // {"name":"announcements","topic":null}
}
```

이벤트에서는 `MessageUpdate`(MESSAGE_UPDATE)가 바뀔 수 있는 모든 필드를 `Patch`로 가집니다:

<!-- snippet: patch_event -->
```rust
use discordrs::model::MessageUpdate;
use discordrs::{Message, Nullable, Patch};

fn apply_edit(message: &mut Message, update: &MessageUpdate) {
    match &update.content {
        Patch::Missing => {} // the edit did not touch the content
        Patch::Value(content) => println!("new content: {content}"),
    }
    if let Patch::Value(Nullable::Null) = &update.edited_timestamp {
        println!("edited_timestamp was cleared");
    }
    // Applies every sent field; omitted ones keep their value.
    update.apply_to(message);
}
```

| 2.x | 3.0 |
|---|---|
| MESSAGE_UPDATE를 `Message`로 | `model::MessageUpdate`. `update.apply_to(&mut message)`로 병합하고, `author`는 필드 단위로 병합되는 `PartialUser` |
| `update_channel(&json!({..}))` | `request::ModifyChannel`(setter, `clear_*`는 `null`을 보냄) |
| 생성 본문 | `Option<T>`: `None`은 키를 생략 |
| `discordrs::request`의 수정 본문(`ModifyChannel`, `EditMessage`, `ModifyAutoModerationRule`, `ModifyGuildScheduledEvent`, …) | `Patch` / `NullablePatch` |
| 아직 `discordrs::model`에 있는 수정 본문(`ModifyGuild`, `ModifyGuildMember`, …) | 당분간 그대로: `None`은 생략, `Some(None)`은 `null` |

직접 패치 필드를 선언할 때는 `#[serde(default, skip_serializing_if = "Patch::is_missing")]`가 필요합니다. `default`를 빠뜨리면 생략된 키가 조용한 "지움"이 아니라 오류가 됩니다.

## 4. 이벤트와 핸들러

**이유:** 2.x는 모든 디스패치를 `serde_json::Value`로 파싱해 세 번 복사했고, 모든 이벤트에 raw 트리를 붙여 두었습니다. 3.0은 각 디스패치를 프레임 텍스트에서 한 번 바로 디코딩해 `Event`로 만들고, 캐시·컬렉터·핸들러가 그것을 `Arc<Event>`로 공유하며, raw JSON은 요청할 때만 보관합니다.

<!-- snippet: 2.x -->
```rust
// 2.x
use async_trait::async_trait;
use discordrs::{Context, Event, EventHandler};

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn handle_event(&self, ctx: Context, event: Event) {
        match event {
            Event::Ready(ready) => println!("ready as {}", ready.data.user.username),
            Event::MessageCreate(event) => println!("{}", event.message.content),
            _ => {}
        }
    }
}
```

`Event`는 이제 봉투입니다. `event.kind()`(`EventKind`)로 match하고, 디스패치 이름·샤드·시퀀스·수신 시각은 `event.metadata()`에서 읽습니다:

<!-- snippet: events_handle_event -->
```rust
use std::sync::Arc;

use discordrs::{Context, Event, EventHandler, EventKind};

struct Router;

impl EventHandler for Router {
    async fn handle_event(&self, _ctx: Context, event: Arc<Event>) {
        let (shard_id, _shard_count) = event.metadata().shard();
        match event.kind() {
            EventKind::Ready(ready) => {
                println!("shard {shard_id} ready as {}", ready.user.username);
            }
            // The `Arc<Message>` the cache and collectors share: clone the
            // `Arc` (not the message) to keep it beyond this call.
            EventKind::MessageCreate(message) => println!("{}", message.content),
            EventKind::Unknown => println!("dispatch without a type: {}", event.name()),
            EventKind::DecodeFailed(error) => eprintln!("{}: {error}", event.name()),
            _ => {}
        }
    }
}
```

타입 훅은 이벤트가 저장한 방식 그대로 payload를 받으므로 훅 호출이 payload를 복사하지 않습니다. 보관해도 되는 `Arc`이거나, 호출 동안만 유효한 참조입니다.

<!-- snippet: events_hooks -->
```rust
use std::sync::Arc;

use discordrs::model::MessageUpdate;
use discordrs::{
    gateway_intents, Client, Context, DiscordError, EventDecodeError, EventHandler, GuildId,
    Member, Message,
};
use serde_json::value::RawValue;

struct Hooks;

impl EventHandler for Hooks {
    async fn message_create(&self, _ctx: Context, message: Arc<Message>) {
        println!("{}: {}", message.channel_id, message.content);
    }

    // A partial message: fields Discord did not send are `Patch::Missing`.
    async fn message_update(&self, _ctx: Context, update: Arc<MessageUpdate>) {
        println!("message {} in {} edited", update.id, update.channel_id);
    }

    async fn member_add(&self, _ctx: Context, guild_id: GuildId, member: Arc<Member>) {
        if let Some(user) = &member.user {
            println!("{} joined {guild_id}", user.username);
        }
    }

    // Dispatches this version has no type for: the name and the JSON text.
    async fn raw_event(&self, _ctx: Context, name: &str, data: &RawValue) {
        println!("{name}: {} bytes", data.get().len());
    }

    // New: a known dispatch whose payload did not match its type.
    async fn decode_failed(
        &self,
        _ctx: Context,
        name: &str,
        error: &EventDecodeError,
        _data: &RawValue,
    ) {
        eprintln!("could not decode {name}: {error}");
    }
}

async fn run(token: &str) -> Result<(), DiscordError> {
    Client::builder(
        token,
        gateway_intents::GUILDS | gateway_intents::GUILD_MESSAGES,
    )
    .event_handler(Hooks)
    // Keep every dispatch's JSON for `event.raw()` (default off; unknown
    // and undecodable dispatches always keep theirs).
    .keep_raw_events(true)
    .start()
    .await
}
```

| 2.x | 3.0 |
|---|---|
| `handle_event(ctx, Event)`, `match event { Event::Ready(..) }` | `handle_event(ctx, Arc<Event>)`, `match event.kind() { EventKind::Ready(..) }` |
| `event.kind()`(이름 문자열) | `event.name()`. `event.metadata()`에 `shard()`, `sequence()`, `received_at()` |
| 감싸는 구조체(`ReadyEvent { data }`, `MessageEvent { message }`, `GuildEvent`, `InteractionEvent`, …)와 그 `raw` 필드 | variant가 payload를 직접 가짐. raw JSON은 `event.raw()` |
| 모든 이벤트의 raw `Value` | `ClientBuilder::keep_raw_events(true)`일 때만 `event.raw()`. 알 수 없는 디스패치와 디코딩 실패는 항상 보관 |
| `message_create(ctx, Message)` | `message_create(ctx, Arc<Message>)` |
| `message_update(ctx, Message)` | `message_update(ctx, Arc<MessageUpdate>)`(부분 갱신) |
| `member_add(ctx, Snowflake, Member)`, `member_update(..)` | `member_add(ctx, GuildId, Arc<Member>)`, `member_update`도 같음 |
| `guild_create(ctx, Guild)` | `guild_create(ctx, &GuildCreatePayload)`: `Available(GuildCreate)` 또는 `Unavailable(UnavailableGuild)`. `to_guild()`로 2.x식 `Guild`를 다시 만듦 |
| 나머지 훅은 payload를 값으로(`ready(ctx, ReadyPayload)`, `channel_create(ctx, Channel)`, …) | 참조로(`&ReadyPayload`, `&Channel`, `&Interaction`, …). 보관할 것은 clone |
| 알 수 없는 디스패치**와** 디코딩 실패 모두 `raw_event(ctx, String, Value)` | 알 수 없는 것은 `raw_event(ctx, &str, &RawValue)`, 실패는 새 `decode_failed(ctx, &str, &EventDecodeError, &RawValue)`와 `EventKind::DecodeFailed` |
| `decode_event(name, Value)` | `decode_event(name, &str)` |

테스트에서는 `decode_event`로 fixture 텍스트에서 `Event`를 만듭니다:

<!-- snippet: events_decode -->
```rust
use discordrs::{decode_event, DiscordError, EventKind};

// Feed a handler or a match from a fixture, without a gateway connection.
fn content_of(fixture: &str) -> Result<Option<String>, DiscordError> {
    let event = decode_event("MESSAGE_CREATE", fixture)?;
    Ok(match event.kind() {
        EventKind::MessageCreate(message) => Some(message.content.clone()),
        _ => None,
    })
}
```

핸들러 실행 방식도 바뀌었습니다(코드 변경은 필요 없지만 알아 둘 것): 각 샤드는 캐시·컬렉터·`fetch_members` 반영을 핸들러를 기다리지 않는 내부 처리기에서 하므로, Serial 모드에서도 핸들러가 `ctx.fetch_members(...)`나 컬렉터를 기다려도 교착되지 않고, 핸들러가 읽는 캐시에는 이후 이벤트가 이미 반영되어 있을 수 있습니다. 큐는 유한하고(`event_queue_capacity`, 기본 10,000), 인터랙션은 수락 기한이 있는 별도 경로를 씁니다(`interaction_admission_timeout`, `on_interaction_overload`). [게이트웨이 API](../api/gateway.md)를 참고합니다.

## 5. 컬렉터

**이유:** 2.x 컬렉터 저널은 모든 이벤트의 전체 사본을 들고 있었고(빈 256칸 허브가 1.24 MB, 큰 길드 25개 뒤 live 힙 386 MB 중 305 MB), 메시지 컬렉터는 MESSAGE_UPDATE payload를 전체 메시지처럼 돌려줬습니다. 3.0 컬렉터는 공유 `Arc<Event>`를 읽고(빈 허브 약 2 KB), 돌려주는 것만 복사하며, 메시지 수정은 따로 다룹니다.

<!-- snippet: 2.x -->
```rust
// 2.x
let channel_id = message.channel_id.clone();
let mut answers = ctx
    .collectors()
    .message_collector()
    .filter(move |reply: &Message| reply.channel_id == channel_id)
    .timeout(Duration::from_secs(30));
// Also yielded edits (MESSAGE_UPDATE) as `Message`s.
if let Some(answer) = answers.next().await { /* ... */ }
```

<!-- snippet: collectors -->
```rust
use std::sync::Arc;
use std::time::Duration;

use discordrs::{Context, EventHandler, EventKind, Message};

struct Quiz;

impl EventHandler for Quiz {
    async fn message_create(&self, ctx: Context, message: Arc<Message>) {
        if message.content != "!quiz" {
            return;
        }
        let channel_id = message.channel_id;
        // Starts at the event being handled: messages sent while this
        // handler call was queued are replayed, not missed.
        let mut answers = ctx
            .message_collector()
            .filter(move |reply: &Message| reply.channel_id == channel_id)
            .timeout(Duration::from_secs(30));
        // `next()` copies the matching message out; `next_event()` hands
        // over the shared `Arc<Event>` without copying.
        if let Some(event) = answers.next_event().await {
            if let EventKind::MessageCreate(answer) = event.kind() {
                println!("first answer: {}", answer.content);
            }
        }
        // Edits are partial `MessageUpdate`s with their own collector.
        let mut edits = ctx
            .message_update_collector()
            .timeout(Duration::from_secs(30));
        if let Some(update) = edits.next().await {
            println!("message {} was edited", update.id);
        }
    }
}
```

| 2.x | 3.0 |
|---|---|
| `MessageCollector`가 MESSAGE_CREATE와 MESSAGE_UPDATE를 모두 냄 | MESSAGE_CREATE만. 수정은 `MessageUpdateCollector`(`ctx.message_update_collector()`) |
| `next()`만 | `next()`(복사한 항목) 또는 `next_event()`(`Arc<Event>`, 복사 없음) |
| `EventSubscription::recv() -> Event` | `-> Arc<Event>` |
| `ctx.collectors().*_collector()`는 지금부터 | 여전히 지금부터. 새 `ctx.*_collector()`는 처리 중인 이벤트부터 시작해 핸들러가 대기하는 동안 온 것을 재생 |
| `CollectorEndReason` 완전 열거(2.2.0) | `#[non_exhaustive]`, 새 `EmojiLimit`, `UserLimit`, `MessageDelete`, `ChannelDelete`, `ThreadDelete`, `GuildDelete`. `_` 팔 추가 |

저널은 유한하고 손실이 있습니다: `capacity`보다 뒤처진 컬렉터는 앞으로 건너뛰고 그 수를 `lagged_events()`에 셉니다. 여러 결과가 동시에 준비되면 순서가 고정됩니다(stop, timeout, idle, 이벤트). 그래서 만료된 창이 늦은 이벤트를 내지 않습니다(2.2.0 버그). 3.0은 `ReactionCollector`도 추가했습니다. [Cache and Collectors](../../docs/api/cache-and-collectors.md)를 참고합니다.

## 6. 캐시

**이유:** 2.x는 멤버·메시지·presence를 이벤트에서 복제해 캐시에 넣었습니다. 3.0 캐시는 사본 대신 이벤트의 `Arc`를 그대로 저장하고, 수정은 copy-on-write로 병합하므로 옛 `Arc`를 든 핸들러는 일관된 스냅샷을 계속 봅니다.

<!-- snippet: 2.x -->
```rust
// 2.x
cache.upsert_message(message.clone()).await;
let cached: Option<Message> = cache.message(&channel_id, &message_id).await;
cache.upsert_guild(guild).await; // GUILD_CREATE, with every member inside
```

<!-- snippet: cache -->
```rust
use std::sync::Arc;

use discordrs::model::MessageUpdate;
use discordrs::{CacheHandle, Message};

async fn track_edit(cache: &CacheHandle, message: Arc<Message>, update: &MessageUpdate) {
    // Upserts take `impl Into<Arc<T>>`: an event's `Arc` is stored, not
    // copied. A plain `Message` still works.
    cache.upsert_message(Arc::clone(&message)).await;
    // MESSAGE_UPDATE is a patch merged into the cached copy. The merge is
    // copy-on-write: `message` keeps showing the content it had.
    if let Some(merged) = cache.apply_message_update(update).await {
        println!("{:?} -> {:?}", message.content, merged.content);
    }
    // Keys are typed IDs, passed by value.
    let cached = cache.message_arc(message.channel_id, message.id).await;
    assert!(cached.is_some());
}
```

| 2.x | 3.0 |
|---|---|
| `upsert_message(Message)`, `upsert_member`, `upsert_presence`, `upsert_guild` | `impl Into<Arc<T>>`를 받음: 이벤트의 `Arc`를 넘기거나, 예전처럼 값을 넘김 |
| 컬렉션을 담은 `Guild`로 GUILD_CREATE 반영 | `apply_guild_create(&GuildCreate)`(이벤트에서, 또는 `GuildCreate::from(guild)`), `apply_guild_update(&Guild)` |
| raw JSON에 어떤 키가 있었는지 보고 MESSAGE_UPDATE를 캐시 메시지에 병합 | 타입 `MessageUpdate` 패치를 `apply_message_update(&MessageUpdate) -> Option<Arc<Message>>`가 copy-on-write로 병합. 모든 메시지 필드(`components`, `poll`, `call`, `message_snapshots`, …)를 다루며 `null`은 지우고 `[]`는 비움. 캐시에 없던 메시지는 계속 없음. `apply_partial_user`가 캐시된 작성자를 필드 단위로 패치 |
| `&Snowflake` 키, 문자열 해시 | 값으로 넘기는 타입 키: 멤버·presence는 `(GuildId, UserId)`, 메시지는 `(ChannelId, MessageId)`, 역할은 `(GuildId, RoleId)` |
| READY마다 캐시 전체 삭제 | READY는 그 샤드의 길드만 지움(`CacheHandle::clear_shard`) |

`_arc` getter(`message_arc`, `member_arc`, `members_arc`, …)는 공유 `Arc`를 복사 없이 돌려주고, 일반 getter는 여전히 소유한 사본을 돌려줍니다.

## 7. 외부 캐시 백엔드

**이유:** 2.x는 백엔드 쓰기를 각자 다른 태스크로 보내서, 늦은 put이 이후 delete 뒤에 도착해 항목을 되살릴 수 있었고, 실패한 쓰기는 미러를 조용히 낡게 남겼습니다. 3.0은 워커 하나가 게이트웨이 순서대로 쓰기를 적용하고, 모든 쓰기에 **epoch**를 붙이며, clear는 compare-and-set이고, 버려지거나 실패한 쓰기는 그 샤드를 격리한 뒤 다시 동기화합니다.

<!-- snippet: 2.x -->
```rust
// 2.x
#[async_trait]
impl CacheBackend for MyBackend {
    async fn clear_cache(&self) -> Result<(), DiscordError> { /* ... */ }
    async fn put_message(&self, message: Message) -> Result<(), DiscordError> { /* ... */ }
    async fn get_message(
        &self,
        channel_id: &Snowflake,
        message_id: &Snowflake,
    ) -> Result<Option<Arc<Message>>, DiscordError> { /* ... */ }
    // ...
}
// 2.2.0 (2.0.2 had the trait but no `cache_backend`):
let backend: Arc<dyn CacheBackend> = Arc::new(MyBackend::new());
let builder = Client::builder(&token, intents).cache_backend(backend);
```

<!-- snippet: cache_backend -->
```rust
use std::sync::Arc;

use discordrs::cache::{CacheBackend, CacheClear, CacheClearId};
use discordrs::model::Presence;
use discordrs::{
    CacheHandle, ChannelId, DiscordError, GuildId, Member, Message, MessageId, UserId,
};

/// Logs every write and stores it in an inner `CacheHandle`, which
/// implements the epoch contract. A Redis backend makes the same calls
/// atomic in a script.
struct LoggingBackend {
    inner: CacheHandle,
}

// No `#[async_trait]`: plain `async fn`, whose futures must be `Send`.
impl CacheBackend for LoggingBackend {
    async fn clear_cache(
        &self,
        expected_epoch: u64,
        clear_id: CacheClearId,
    ) -> Result<CacheClear, DiscordError> {
        println!("clear all at epoch {expected_epoch}");
        self.inner.clear_cache(expected_epoch, clear_id).await
    }

    async fn clear_shard_cache(
        &self,
        expected_epoch: u64,
        clear_id: CacheClearId,
        shard_id: u32,
        shard_count: u32,
    ) -> Result<CacheClear, DiscordError> {
        println!("clear shard {shard_id}/{shard_count} at epoch {expected_epoch}");
        self.inner
            .clear_shard_cache(expected_epoch, clear_id, shard_id, shard_count)
            .await
    }

    async fn purge_expired_entries(&self) -> Result<(), DiscordError> {
        self.inner.purge_expired_entries().await
    }

    // Every write carries the epoch it was issued under; a write older than
    // the store's epoch must be ignored and still return `Ok(())`.
    async fn put_member(
        &self,
        epoch: u64,
        guild_id: GuildId,
        user_id: UserId,
        member: Arc<Member>,
    ) -> Result<(), DiscordError> {
        self.inner
            .put_member(epoch, guild_id, user_id, member)
            .await
    }

    async fn get_member(
        &self,
        guild_id: GuildId,
        user_id: UserId,
    ) -> Result<Option<Arc<Member>>, DiscordError> {
        self.inner.get_member(guild_id, user_id).await
    }

    async fn list_members(&self, guild_id: GuildId) -> Result<Vec<Arc<Member>>, DiscordError> {
        self.inner.list_members(guild_id).await
    }

    async fn delete_member(
        &self,
        epoch: u64,
        guild_id: GuildId,
        user_id: UserId,
    ) -> Result<(), DiscordError> {
        self.inner.delete_member(epoch, guild_id, user_id).await
    }

    async fn put_message(&self, epoch: u64, message: Arc<Message>) -> Result<(), DiscordError> {
        println!("store message {} at epoch {epoch}", message.id);
        self.inner.put_message(epoch, message).await
    }

    async fn get_message(
        &self,
        channel_id: ChannelId,
        message_id: MessageId,
    ) -> Result<Option<Arc<Message>>, DiscordError> {
        self.inner.get_message(channel_id, message_id).await
    }

    async fn list_messages(
        &self,
        channel_id: ChannelId,
    ) -> Result<Vec<Arc<Message>>, DiscordError> {
        self.inner.list_messages(channel_id).await
    }

    async fn delete_message(
        &self,
        epoch: u64,
        channel_id: ChannelId,
        message_id: MessageId,
    ) -> Result<(), DiscordError> {
        self.inner
            .delete_message(epoch, channel_id, message_id)
            .await
    }

    async fn put_presence(
        &self,
        epoch: u64,
        guild_id: GuildId,
        user_id: UserId,
        presence: Arc<Presence>,
    ) -> Result<(), DiscordError> {
        self.inner
            .put_presence(epoch, guild_id, user_id, presence)
            .await
    }

    async fn get_presence(
        &self,
        guild_id: GuildId,
        user_id: UserId,
    ) -> Result<Option<Arc<Presence>>, DiscordError> {
        self.inner.get_presence(guild_id, user_id).await
    }

    async fn list_presences(
        &self,
        guild_id: GuildId,
    ) -> Result<Vec<Arc<Presence>>, DiscordError> {
        self.inner.list_presences(guild_id).await
    }

    async fn delete_presence(
        &self,
        epoch: u64,
        guild_id: GuildId,
        user_id: UserId,
    ) -> Result<(), DiscordError> {
        self.inner.delete_presence(epoch, guild_id, user_id).await
    }
}
```

| 2.x | 3.0 |
|---|---|
| `clear_cache(&self)` | `clear_cache(expected_epoch, clear_id) -> CacheClear::{Applied, Rejected}`. 새 필수 메서드 `clear_shard_cache(expected_epoch, clear_id, shard_id, shard_count)` |
| `put_member(guild, user, Member)`, `put_message(Message)`, `put_presence(.., Presence)` | `put_*(epoch, .., Arc<T>)`: 공유된 이벤트 스냅샷. 저장소 epoch보다 오래된 쓰기는 무시 |
| `delete_*(&Snowflake, ..)` | `delete_*(epoch, ..)`, 타입 ID를 값으로 |
| 부분 MESSAGE_UPDATE를 전체 메시지처럼 `put_message`로 보냄(2.2.0) | 저장된 사본에 병합(`get_message` → 병합 → `put_message`). 전체 사본이 없으면 건너뜀 |
| `Arc<dyn CacheBackend>` | 구체 타입 `B: CacheBackend`의 `Arc<B>`(8절 참고) |

epoch 계약은 규범입니다(저장소당 epoch 하나, 비교·삭제·증가의 원자성, 멱등한 `CacheClearId` 재시도). 자세한 내용은 [Cache and Collectors](../../docs/api/cache-and-collectors.md)의 "External Cache Backend"에 있습니다. 한 저장소에는 한 번에 클라이언트 하나만 쓸 수 있고, `Context::cache_backend_health()`가 미러 상태를 알려 줍니다.

## 8. async-trait 제거

**이유:** `async-trait`가 모든 빌드에 `syn`의 두 번째 메이저 버전을 끌어왔습니다. 공개 async 트레잇(`EventHandler`, `CacheBackend`, `InteractionHandler`, `TypedInteractionHandler`, `CachedManager`)은 이제 `fn … -> impl Future<Output = T> + Send`로 선언되어 일반 `async fn`으로 구현하며, 기본 기능 빌드에 중복 크레이트가 없습니다.

- 이 트레잇들의 모든 impl에서 **`#[async_trait]`와 `use async_trait::async_trait;`를 지웁니다.** 남은 속성은 `E0195`("lifetime parameters or bounds on method … do not match the trait declaration")로 실패합니다.
- **future는 `Send`여야 합니다.** `.await`를 넘어 `Rc`, `RefCell` 대여, `std::sync::MutexGuard`를 들고 있으면 안 됩니다. 컴파일러가 impl 위치에 "future cannot be sent between threads safely"를 보고합니다.
- **`CachedManager`는 모든 빌드에서 async입니다.** `gateway` 기능 없이는 동기 트레잇이었지만 이제 어디서나 같은 `fn … -> impl Future` 트레잇이므로 `get`, `cached`, `contains`, `list_cached`에 `.await`를 붙이고 모델의 타입 ID를 넘깁니다(1절).
- **트레잇 객체 불가.** 트레잇이 object safe하지 않습니다. `ClientBuilder::event_handler(handler)`는 그대로이고, `cache_backend`는 구체 백엔드 타입의 `Arc<B>`를 받습니다:

<!-- snippet: no_dyn -->
```rust
use std::sync::Arc;

use discordrs::cache::CacheBackend;
use discordrs::{gateway_intents, Client, ClientBuilder};

// 2.x took `Arc<dyn CacheBackend>`. The 3.0 traits are not object safe:
// name the concrete backend, or be generic over it.
fn with_backend<B: CacheBackend + 'static>(token: &str, backend: Arc<B>) -> ClientBuilder {
    Client::builder(token, gateway_intents::GUILD_MESSAGES).cache_backend(backend)
}
```

## 9. REST

**이유:** 2.x는 여러 라우트에 메서드가 두 개씩 있었고(`Vec<Value>`를 돌려주는 `get_guild_emojis`와 `get_guild_emojis_typed`), 본문으로 `&Value`나 아무 `B: Serialize`나 받았으며, 재시도마다 JSON을 다시 직렬화했습니다. 3.0은 라우트당 메서드 하나가 그 라우트의 요청 타입을 받아 모델을 돌려주고, 본문을 한 번 바이트로 인코딩해 모든 재시도에서 재사용합니다.

<!-- snippet: 2.x -->
```rust
// 2.x
use reqwest::Method;

let channel = rest.update_channel(channel_id.clone(), &json!({ "name": "general" })).await?;
let message = rest
    .create_message(channel_id, &CreateMessage { content: Some("hello".into()), ..Default::default() })
    .await?;
let emojis = rest.get_guild_emojis_typed(guild_id.clone()).await?;
let preview: Value = rest
    .request(Method::GET, &format!("/guilds/{guild_id}/preview"), None)
    .await?;
```

<!-- snippet: rest -->
```rust
use discordrs::request::{CreateMessage, ModifyChannel};
use discordrs::{ChannelId, DiscordError, GuildId, RawRequest, RestClient};
use serde_json::Value;

async fn rest_calls(
    rest: &RestClient,
    guild_id: GuildId,
    channel_id: ChannelId,
) -> Result<(), DiscordError> {
    // One method per route: a concrete request type in, a typed model out.
    let channel = rest
        .update_channel(channel_id, &ModifyChannel::new().name("general"))
        .await?;
    let message = rest
        .create_message(channel.id, &CreateMessage::new().content("hello"))
        .await?;
    let emojis = rest.get_guild_emojis(guild_id).await?; // was `get_guild_emojis_typed`
    println!("{} in {}; {} emojis", message.id, channel.id, emojis.len());
    // A route without a method: same transport, rate limits, and retries.
    let preview: Value = rest
        .request(RawRequest::get(format!("/guilds/{guild_id}/preview")))
        .await?;
    println!("{preview}");
    Ok(())
}
```

| 2.x | 3.0 | 비고 |
|---|---|---|
| `update_channel(id, &Value)`, `update_channel_typed` | `update_channel(id, &request::ModifyChannel)` | `_typed` 삭제 |
| `create_message(id, &model::CreateMessage)` | `create_message(id, &request::CreateMessage)` | 크레이트 루트의 `CreateMessage`는 아직 2.x 모델 타입: `discordrs::request::CreateMessage`를 import |
| `update_message`, `update_message_with_files` | `edit_message(_with_files)(.., &request::EditMessage)` | 삭제 |
| `get_guild_emojis() -> Vec<Value>`, `get_guild_emojis_typed` | `get_guild_emojis() -> Vec<Emoji>` | `_typed`는 deprecated 전달 함수 |
| `create_dm_channel_typed`, `get_guild_audit_log_typed`, `modify_guild_widget`, `*_from_request`, `*_config` | 정식 이름 | deprecated 전달 함수 35개 |
| `update_guild`, `update_role`, `create_role` | `modify_guild`, `modify_guild_role`, `create_guild_role` | 삭제 |
| `execute_webhook(&Value) -> Value` | `execute_webhook(&request::ExecuteWebhook) -> Message` | |
| `modify_guild_member(..) -> ()` | `-> Member` | |
| `request(Method, &str, Option<&Value>)`, `request_typed`, `request_multipart` | `request::<T>(RawRequest)`, `request_discard(RawRequest)` | 같은 전송 계층·rate limit·토큰 가리기 |
| `edit_channel_permissions(channel, overwrite_id, &Value)` | `edit_channel_permissions(channel, OverwriteTarget, &EditChannelPermission)` | `_typed`는 deprecated 전달 함수 |
| `available_tags: Vec<ForumTag>`(`CreateGuildChannel`, `update_channel`에는 JSON) | `Vec<request::AvailableForumTag>` | `AvailableForumTag::from(&tag)`는 태그를 유지, `AvailableForumTag::new("name")`은 새로 만듦. 빠진 태그는 삭제 |
| `send_container_message`, `send_to_channel`, `send_components_v2`, `edit_message_with_container` → `Value` | → `Message` | 클라이언트 기본 `allowed_mentions`도 적용 |

`_typed` 쌍 41개와 단독 `_typed` 메서드 5개를 정식 이름으로 합쳤고, 일대일 이름 변경 35개는 이 메이저 버전 동안 deprecated 전달 함수로 남으며, untyped 변형 35개는 삭제했습니다. 전체 표는 [HTTP 및 헬퍼](../api/http-and-helpers.md)의 "2.x에서 REST 이름 옮기기 (3.0)"에 있습니다. Discord가 형태를 정하지 않은 본문은 일부러 untyped로 둡니다: Slack/GitHub 호환 웹훅(`&Value`)과 로비 moderation 메타데이터.

## 10. 메시지·컴포넌트 빌더

**이유:** 2.x 빌더는 `serde_json::Value`를 돌려줘서, 컴포넌트 종류가 지원하지 않는 필드(custom ID 버튼의 URL, 역할 select의 옵션)가 조용히 사라졌고, 필수 accessory가 없는 section은 Discord에서야 실패했습니다. 3.0 빌더는 `discordrs::request`의 타입 요청 모델을 만들고 종류마다 빌더가 따로 있어, 잘못된 형태는 컴파일되지 않습니다. Components V2 메시지 인코딩은 할당 313회에서 26회로 줄었습니다.

<!-- snippet: 2.x -->
```rust
// 2.x
let buttons = ActionRowBuilder::new()
    .add_button(ButtonBuilder::new().custom_id("ticket:open").label("Open").style(button_style::PRIMARY))
    .add_button(ButtonBuilder::new().url("https://example.com/help").label("Help"));
let header = SectionBuilder::new() // without an accessory it still compiled
    .add_text_display(TextDisplayBuilder::new("## Support"))
    .set_thumbnail_accessory(ThumbnailBuilder::new("https://example.com/logo.png"));
let container = ContainerBuilder::new()
    .add_section(header)
    .add_action_row(buttons)
    .add_action_row(ActionRowBuilder::new().add_select_menu(
        SelectMenuBuilder::string("ticket:topic").add_option(SelectOption::new("Billing", "billing")),
    ))
    .add_action_row(ActionRowBuilder::new().add_select_menu(
        SelectMenuBuilder::role("ticket:notify").default_role("701"),
    ));
// `components: Vec<Value>`
let message = MessageBuilder::new()
    .components_v2(ComponentsV2Message::new().add_container(container))
    .build();
```

<!-- snippet: builders -->
```rust
use discordrs::request::CreateMessage;
use discordrs::{
    ActionRowBuilder, ButtonStyle, ComponentsV2Message, ContainerBuilder, CustomButtonBuilder,
    DiscordError, LinkButtonBuilder, MessageBuilder, RoleId, SectionBuilder, SelectMenuBuilder,
    SelectOption, TextDisplayBuilder, ThumbnailBuilder,
};

fn support_panel(moderators: RoleId) -> Result<CreateMessage, DiscordError> {
    // One builder per button kind: a link button has no `custom_id` setter.
    let buttons = ActionRowBuilder::new()
        .add_button(
            CustomButtonBuilder::new("ticket:open")
                .label("Open")
                .style(ButtonStyle::PRIMARY),
        )
        .add_button(LinkButtonBuilder::new("https://example.com/help").label("Help"));
    // The required accessory is part of the type: without
    // `set_*_accessory`, `add_section` does not compile.
    let header = SectionBuilder::new()
        .add_text_display(TextDisplayBuilder::new("## Support"))
        .set_thumbnail_accessory(ThumbnailBuilder::new("https://example.com/logo.png"));
    let container = ContainerBuilder::new()
        .add_section(header)
        .add_action_row(buttons)
        // A select builder is its own row, with only its kind's setters.
        .add_action_row(
            SelectMenuBuilder::string("ticket:topic")
                .add_option(SelectOption::new("Billing", "billing")),
        )
        .add_action_row(SelectMenuBuilder::role("ticket:notify").default_role(moderators));
    // `build()` returns the typed `request::CreateMessage`, not a `Value`.
    MessageBuilder::new()
        .components_v2(ComponentsV2Message::new().add_container(container))
        .try_build()
}
```

| 2.x | 3.0 |
|---|---|
| `ButtonBuilder::new().custom_id(id)` / `.url(url)` | `CustomButtonBuilder::new(id)` / `LinkButtonBuilder::new(url)`, `PremiumButtonBuilder::new(sku_id)`. `ButtonBuilder`는 deprecated |
| `ActionRowBuilder::new().add_select_menu(menu)` | 행 자리에 select 빌더를 그대로: `add_action_row(menu)` |
| `ActionRowBuilder::add_text_input`, `add_component(Value)` | 삭제: 텍스트 입력은 모달 전용, raw JSON은 `request::RawComponent::from_value(&json)?` |
| `SelectMenuBuilder` 타입 하나 | `SelectMenuBuilder::{string, user, role, mentionable, channel}`이 자기 setter만 가진 종류별 빌더를 돌려줌 |
| `default_value(SelectDefaultValue::role("701"))` | 타입 ID를 받는 `default_role(role_id)`(`default_user`, `default_channel`, `default_mentionable`) |
| `validate()`에서 검사하던 `SectionBuilder` | accessory가 타입에 있음: `add_section`은 `SectionBuilder<SectionAccessory>`만 받음 |
| `X::build() -> Value` | 타입 모델(`Container`, `request::Embed`, `Vec<Component>`, …). JSON이 필요하면 `serde_json::to_value(&x)` |
| `MessageBuilder::build() -> model::CreateMessage` | `-> request::CreateMessage`. `try_build()`는 검증하고 `build()`는 검증하지 않음 |
| `MessageBuilder::flags(u64)` | `flags(impl Into<MessageFlags>)`(`u64`도 동작) |

`MessageBuilder`는 `InteractionMessage`와 `CreateFollowup`으로 변환되고, `UpdateMessage`와 `EditInteractionResponse`로는 실패할 수 있는 변환을 합니다. [빌더 API](../api/builders.md)의 "2.x 빌더에서 옮기기"를 참고합니다.

## 11. 인터랙션

**이유:** 2.x는 인터랙션을 먼저 `Value` 트리로 파싱했고, 커맨드 옵션 값을 `Value`로 두었고, HTTP 엔드포인트 응답을 `Value` payload enum으로 만들었으며, 체크박스가 있는 모달을 파싱하지 못했습니다. 3.0은 텍스트에서 바로 디코딩하고, 값을 타입으로 다루며, HTTP 엔드포인트·REST 콜백·`InteractionResponder`가 같은 응답 타입 하나를 씁니다.

<!-- snippet: 2.x -->
```rust
// 2.x
let value: serde_json::Value = serde_json::from_str(body)?;
let Interaction::ChatInputCommand(command) = parse_interaction(&value)? else { return Ok(None) };
let amount = command.data.options.iter()
    .find(|option| option.name == "amount")
    .and_then(|option| option.value.as_ref()) // Option<serde_json::Value>
    .and_then(serde_json::Value::as_i64);
```

<!-- snippet: interactions_decode -->
```rust
use discordrs::{decode_interaction, DiscordError, Interaction};

// The HTTP endpoint and the gateway decode the same way: from text, with
// no `serde_json::Value` tree.
fn amount_option(body: &str) -> Result<Option<i64>, DiscordError> {
    let Interaction::ChatInputCommand(command) = decode_interaction(body)? else {
        return Ok(None);
    };
    Ok(command
        .data
        .options
        .iter()
        .find(|option| option.name == "amount")
        // `CommandOptionValue`: `as_str`, `as_i64`, `as_f64`, `as_bool`, `as_id`.
        .and_then(|option| option.value.as_ref()?.as_i64()))
}
```

HTTP 엔드포인트 핸들러는 `EndpointReply`를 돌려줍니다: `Inline`(HTTP 응답으로 보내는 `request::InteractionResponse`) 또는 `Accepted`. `EndpointReply::Accepted`는 REST로 응답한 뒤에만(예: `InteractionResponder`로) 돌려줍니다. 엔드포인트가 본문 없는 `202`로 답하게 하므로, REST 응답이 나가지 않았으면 Discord는 인터랙션을 실패로 표시합니다. 나머지는 모두 인라인으로 답합니다.

<!-- snippet: 2.x -->
```rust
// 2.x
#[async_trait]
impl TypedInteractionHandler for Tickets {
    async fn handle_typed(&self, _ctx: InteractionContextData, interaction: Interaction) -> InteractionResponse {
        match interaction {
            Interaction::ChatInputCommand(command) if command.data.name.as_deref() == Some("ticket") => {
                let modal = ModalBuilder::new("ticket:create", "New ticket")
                    .add_text_input(TextInputBuilder::new("subject", "Subject", text_input_style::SHORT));
                InteractionResponse::Modal(modal)
            }
            Interaction::ModalSubmit(_) => InteractionResponse::ChannelMessage(json!({
                "content": "Ticket created",
                "flags": 64,
            })),
            _ => InteractionResponse::DeferredMessage,
        }
    }
}
```

<!-- snippet: interactions_endpoint -->
```rust
use discordrs::request::InteractionMessage;
use discordrs::{
    EndpointReply, Interaction, InteractionContextData, InteractionResponder,
    InteractionResponse, ModalBuilder, RestClient, TextInputBuilder, TextInputStyle,
    TypedInteractionHandler,
};

#[derive(Clone)]
struct Tickets {
    rest: RestClient,
}

fn ephemeral(text: &str) -> EndpointReply {
    InteractionResponse::reply(InteractionMessage::new().content(text).ephemeral(true)).into()
}

impl TypedInteractionHandler for Tickets {
    async fn handle_typed(
        &self,
        _ctx: InteractionContextData,
        interaction: Interaction,
    ) -> EndpointReply {
        match interaction {
            Interaction::ChatInputCommand(command)
                if command.data.name.as_deref() == Some("ticket") =>
            {
                // Text inputs are wrapped in a Label (Discord's current layout).
                let modal = ModalBuilder::new("ticket:create", "New ticket").add_text_input(
                    TextInputBuilder::new("subject", "Subject", TextInputStyle::SHORT),
                );
                InteractionResponse::modal(modal).into()
            }
            Interaction::ChatInputCommand(command)
                if command.data.name.as_deref() == Some("status") =>
            {
                // `Accepted` (202, no body) only after responding over REST.
                match command.reply_ephemeral(&self.rest, "All systems go").await {
                    Ok(()) => EndpointReply::Accepted,
                    // The endpoint sends this only if no REST response may
                    // have reached Discord; otherwise it answers 202.
                    Err(_) => ephemeral("Status is unavailable right now"),
                }
            }
            Interaction::ModalSubmit(_) => ephemeral("Ticket created"),
            // Autocomplete must be answered with choices, not a message.
            Interaction::Autocomplete(_) => InteractionResponse::autocomplete([]).into(),
            // Everything else is answered inline too: `Accepted` without a
            // REST response would leave the interaction unanswered.
            _ => ephemeral("Unknown command"),
        }
    }
}
```

`AppFramework`의 라우트와 가드도 같은 타입을 따릅니다:

<!-- snippet: 2.x -->
```rust
// 2.x
let app = AppFramework::builder()
    .guard(|ctx: &AppContext| -> Result<(), InteractionResponse> { /* ... */ Ok(()) })
    .command("hello", |_ctx| async move {
        InteractionResponse::ChannelMessage(json!({ "content": "Hello from discord.rs" }))
    })
    .build();
```

<!-- snippet: framework -->
```rust
use discordrs::request::InteractionMessage;
use discordrs::{AppContext, AppFramework, EndpointReply, InteractionResponse};

fn app() -> AppFramework {
    AppFramework::builder()
        // Guards return `Result<(), EndpointReply>` (2.x: `InteractionResponse`).
        .guard(|ctx: &AppContext| {
            if ctx.context.guild_id.is_some() {
                return Ok(());
            }
            let message = InteractionMessage::new()
                .content("Use this command in a server")
                .ephemeral(true);
            Err(EndpointReply::from(InteractionResponse::reply(message)))
        })
        // Routes return anything that converts into an `EndpointReply`.
        .command("hello", |_ctx: AppContext| async move {
            InteractionResponse::reply("Hello from discord.rs")
        })
        .build()
}
```

게이트웨이에서는 2.0.2의 자유 헬퍼 함수 대신 `InteractionResponder`(2.2.0)로 응답합니다. 그 헬퍼 대부분은 deprecated입니다:

<!-- snippet: interactions_responder -->
```rust
use discordrs::{Context, EventHandler, Interaction, InteractionResponder};

struct Commands;

impl EventHandler for Commands {
    async fn interaction_create(&self, ctx: Context, interaction: &Interaction) {
        let Interaction::ChatInputCommand(command) = interaction else {
            return;
        };
        if command.data.name.as_deref() == Some("ping") {
            // Tracks the response state: a second reply fails locally,
            // follow-ups wait until Discord accepted the first response.
            if let Err(error) = command.reply_ephemeral(ctx.rest().as_ref(), "pong").await {
                eprintln!("could not answer /ping: {error}");
            }
        }
    }
}
```

모달 제출은 Discord의 값 타입을 유지합니다:

<!-- snippet: modal_values -->
```rust
use discordrs::{decode_interaction, DiscordError, Interaction};

fn ticket_form(body: &str) -> Result<Option<(String, bool)>, DiscordError> {
    let Interaction::ModalSubmit(submit) = decode_interaction(body)? else {
        return Ok(None);
    };
    let form = &submit.submission;
    // Checkboxes are booleans now (2.x failed to parse them).
    let subject = form.get_text("subject").unwrap_or_default().to_owned();
    let urgent = form.get_checkbox("urgent").unwrap_or(false);
    Ok(Some((subject, urgent)))
}
```

커맨드 옵션 choice는 타입을 가지며, `validate()` / `try_build()`가 더 많이 검사합니다:

<!-- snippet: 2.x -->
```rust
// 2.x
let amount = CommandOptionBuilder::new(option_type::INTEGER, "amount", "How much")
    .choice("ten", 10)
    .choice("most", 9_007_199_254_740_991_u64); // any `impl Serialize`
let command = SlashCommandBuilder::new("pay", "Send coins").option(amount).build();
```

<!-- snippet: command_choices -->
```rust
use discordrs::{
    CommandDefinition, CommandOptionBuilder, CommandOptionType, DiscordError,
    SlashCommandBuilder,
};

fn pay_command() -> Result<CommandDefinition, DiscordError> {
    let amount = CommandOptionBuilder::new(CommandOptionType::INTEGER, "amount", "How much")
        // `impl Into<CommandOptionValue>`: strings, i8-i64, u8-u32, floats.
        .choice("ten", 10)
        // Any other `Serialize` value (here a `u64`) goes through `try_choice`.
        .try_choice("most", 9_007_199_254_740_991_u64)?;
    // `try_build()` now also checks names (lowercase, no punctuation),
    // locale keys, and each choice against the option type and +/-2^53.
    SlashCommandBuilder::new("pay", "Send coins")
        .option(amount)
        .try_build()
}
```

| 2.x | 3.0 |
|---|---|
| `parse_interaction(&Value)` | `decode_interaction(&str)`. `parse_interaction(&Value)`도 동작(텍스트를 거침) |
| `{"ChatInputCommand": {..}}`로 직렬화되던 `Interaction` | Discord 와이어 형식 |
| `CommandInteractionOption::value: Option<Value>` | `Option<CommandOptionValue>`(`as_str`, `as_i64`, `as_f64`, `as_bool`, `as_id`) |
| `ApplicationCommandOptionChoice::new(name, impl Serialize)`, `CommandOptionBuilder::choice(name, impl Serialize)` | `new` / `choice`는 `impl Into<CommandOptionValue>`(문자열, `i8`–`i64`, `u8`–`u32`, 부동소수)를 받음. 그 밖의 값은 `try_new` / `try_choice`. `Default` 없음 |
| `interactions::InteractionResponse::{ChannelMessage(Value), DeferredMessage, DeferredUpdate, Modal, ..}` | `request::InteractionResponse::{reply(..), defer(ephemeral), DeferredUpdateMessage, update(..), modal(..), autocomplete(..), Raw(..)}` |
| 핸들러가 `InteractionResponse`를 반환 | `EndpointReply`를 반환(`response.into()`, REST로 응답한 뒤에만 `EndpointReply::Accepted`) |
| `AppFramework` 라우트, `FrameworkFuture`, 가드(`Result<(), InteractionResponse>`) | 라우트는 `impl Into<EndpointReply>`를 반환, `FrameworkFuture`는 `EndpointReply`로 끝나고, 가드는 `Result<(), EndpointReply>`를 반환 |
| `interactions_endpoint`, `typed_interactions_endpoint`(잘못된 키에 panic) | `try_interactions_endpoint`, `try_typed_interactions_endpoint`(`Result` 반환) |
| `defer_interaction`, `followup_message`, `edit_original_response` 등 `InteractionResponder`와 겹치는 헬퍼(2.0.2) | `InteractionResponder`: `defer`, `follow_up`, `edit_reply`, …(헬퍼 11개 deprecated, 15절 참고) |
| `respond_with_message`, `respond_to_interaction` | deprecated 아님: 이제 응답 상태를 공유하는 상태 없는 헬퍼(예제가 옮겨 가면 deprecated 예정). `reply` / `reply_ephemeral` 권장 |
| `InteractionReplyData`, `From<CreateMessage>` | `request::InteractionMessage`(deprecated 별칭). `TryFrom<CreateMessage>`는 채널 전용 필드를 거부 |
| `ModalBuilder::add_text_input` → action row | 입력을 Label로 감쌈(Discord의 현재 레이아웃). action row 형태는 deprecated 생성자로만 |
| 2.0.2: `Message.components: Option<Vec<Value>>`(`MessageComponent` 없음) | 타입 `MessageComponent` 트리: `kind: ComponentType`, `custom_id`, `components`, `iter()`, `value: Option<MessageComponentValue>`, `values`, `default` |
| 2.2.0: `MessageComponent::value: Option<String>`, `kind: u8` | `value: Option<MessageComponentValue>`(`String` / `Bool` / `Other`, `value_str()` / `value_bool()`), 새 `values`와 `default`. `kind: ComponentType` |
| `V2ModalComponent::RadioGroup { value: String }` | `value: Option<String>`(아무것도 고르지 않으면 `null`). 체크박스는 Discord의 `value` 키를 읽음. `V2ModalSubmission::get_checkbox` |
| `CheckboxBuilder::checked(bool)` | `default_checked(bool)`. `checked`는 deprecated 별칭으로 남음 |
| 라디오 그룹·체크박스 그룹·체크박스 빌더의 `disabled`, `CheckboxBuilder::required` | 삭제: Discord는 모달의 비활성 컴포넌트를 거부하고, 체크박스 하나는 required가 될 수 없음(옵션 하나짜리 체크박스 그룹에 `required(true)`) |
| Entry Point 커맨드가 `Interaction::Unknown` | `Interaction::PrimaryEntryPoint` |

`Interaction::Unknown::raw_data`는 `Option<Box<RawValue>>`이고 `kind`는 항상 인터랙션 타입입니다. 모달 제출과 컴포넌트 인터랙션은 `resolved` 데이터를 가집니다.

그 밖에 바뀐 것:

- **커맨드 검증.** 슬래시·엔트리 포인트 커맨드 빌더의 `validate()` / `try_build()`는 Discord가 거부하는 이름(구두점, 모든 문자 체계의 대문자, ASCII 아포스트로피. 대신 `ʼ` U+02BC)과 모르는 로케일 키를 거부하고, 모든 choice를 옵션 타입과 범위(`INTEGER`: −2^53+1 ~ 2^53−1, 불리언·유한하지 않은 수 불가)에 맞춰 검사합니다. `build()`는 여전히 검증하지 않습니다.
- **응답 상태.** `is_acknowledged()`, `is_replied()`, `is_deferred()`는 Discord가 확인한 것만 보고합니다. 전송 중이거나 결과가 불확실한 응답은 아직 "acknowledged"가 아닙니다(`response_phase()`가 전체 상태를 보여 줌). 콜백 본문은 응답 슬롯을 잡기 전에 만들고 검사하므로, 로컬 오류는 인터랙션을 미응답 상태로 남기고 인라인 대체 응답이 여전히 답할 수 있습니다.
- **엄격해진 디코딩.** 인터랙션 `type`은 JSON 숫자, `data`는 객체여야 하고, 반복된 키는 거부합니다.
- **`serde_json::Value`였던 다른 모델 필드**도 타입이 생겼습니다: `AuditLogEntry::changes`(`AuditLogChange`, `new_value` / `old_value`는 `new_value_as::<T>()` / `old_value_as::<T>()`로 읽는 JSON 텍스트)와 `options`(`AuditLogEntryOptions`), `Application::team`(`Team`), `install_params`(`ApplicationInstallParams`), `integration_types_config`(`ApplicationIntegrationType` 키), `GuildOnboarding::prompts`(`OnboardingPrompt`), `Guild::guild_scheduled_events`(`GuildScheduledEvent`), `Invite`의 부분 객체(`guild`, `channel`, `target_application`, `guild_scheduled_event`), `UserConnection::integrations`(`PartialIntegration`), `Embed::provider`(`EmbedProvider`). Discord가 형태를 정하지 않은 필드는 `Box<RawValue>`(JSON 텍스트, `serde_json::from_str(raw.get())`로 읽음)입니다: `Sku::manifest_labels`·`features`, `GuildTemplate::serialized_source_guild`. 전체 표는 [핵심 타입 및 상수](../api/core-types-and-constants.md)에 있습니다.

[인터랙션 엔드포인트](../api/interactions-endpoint.md)와 [파서](../api/parsers.md)를 참고합니다.

## 12. 권한과 비트필드

**이유:** 2.x에는 권한 타입이 두 개(`PermissionsBitField`와 `BitField<PermissionFlags>`) 있었고, `CurrentUserGuild::permissions`는 `Option<String>`이었으며, 41–52번 비트 이름이 어긋나 있었고, 메시지 플래그 값이 틀렸습니다. 3.0은 어디서나 `Permissions` 하나를 쓰고 Discord 문서의 비트와 와이어 형태를 따릅니다.

<!-- snippet: 2.x -->
```rust
// 2.x
use discordrs::bitfield::permissions;
use discordrs::model::PermissionsBitField;

let mut perms = PermissionsBitField(permissions::VIEW_CHANNEL.bits() | permissions::SEND_MESSAGES.bits());
perms.insert(permissions::EMBED_LINKS.bits());
perms.remove(permissions::SEND_MESSAGES.bits()); // changed `perms` in place
```

<!-- snippet: permissions -->
```rust
use discordrs::bitfield::{message_flags, permissions};
use discordrs::{MessageFlags, Permissions};

fn permission_sets() -> Result<(), serde_json::Error> {
    let mut perms: Permissions = permissions::VIEW_CHANNEL | permissions::SEND_MESSAGES;
    perms |= permissions::EMBED_LINKS; // 2.x: `perms.insert(..)`
    perms = perms - permissions::SEND_MESSAGES; // `remove` returns the new set
    assert!(perms.contains(permissions::VIEW_CHANNEL.bits()));
    // Permissions are decimal strings on the wire; the other bitfields
    // (message, channel, and user flags, intents) are numbers.
    assert_eq!(serde_json::to_string(&perms)?, r#""17408""#);
    let flags: MessageFlags = message_flags::SUPPRESS_NOTIFICATIONS;
    assert_eq!(serde_json::to_string(&flags)?, "4096"); // 2.x sent bit 10
    Ok(())
}
```

| 2.x | 3.0 |
|---|---|
| `PermissionsBitField(bits)`, `.0`, `insert`, 제자리 `remove` | `Permissions::from_bits(bits)`, `.bits()`, `\|=`, `set = set - flag`(`add` / `remove` / `merge` / `missing`은 새 집합을 돌려주며 `#[must_use]`) |
| `CurrentUserGuild::permissions: Option<String>` | `Option<Permissions>` |
| 메시지·채널·사용자 플래그와 intents를 문자열로 직렬화 | 숫자(권한은 계속 문자열). 음수 입력은 거부 |
| 직접 만든 `BitFieldFlags` 집합을 문자열로 직렬화 | 기본은 숫자. `const STRING_ENCODED: bool = true`면 문자열 유지(읽기는 둘 다 허용) |
| `message_flags::SUPPRESS_NOTIFICATIONS` = `1 << 10`, `IS_VOICE_MESSAGE` = `1 << 12` | `1 << 12`와 `1 << 13`. `HAS_SNAPSHOT`(`1 << 14`) 추가 |
| 41번 비트부터 이름이 하나씩 밀림, `SEND_POLLS` = 50번 | Discord 표대로: `SEND_POLLS` = 49, `USE_EXTERNAL_APPS` = 50, `PIN_MESSAGES` = 51, `BYPASS_SLOWMODE` = 52 |

직접 만든 `BitFieldFlags` 집합도 이제 숫자로 씁니다. 다른 쪽이 문자열로 읽는다면 2.x 형태를 유지합니다:

<!-- snippet: custom_bitfield -->
```rust
use discordrs::bitfield::{BitField, BitFieldFlags};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct FeatureFlags;

impl BitFieldFlags for FeatureFlags {
    const BITS: &'static [(u64, &'static str)] = &[(1 << 0, "BETA"), (1 << 1, "STAFF")];
    // 3.0 writes custom sets as numbers (2.x: strings); this keeps strings.
    const STRING_ENCODED: bool = true;
}

fn encode_features() -> Result<String, serde_json::Error> {
    serde_json::to_string(&BitField::<FeatureFlags>::from_bits(0b11)) // "\"3\""
}
```

3.0은 실효 권한도 계산합니다(`discordrs::permissions`: `permissions_for`, `Member::guild_permissions`, 캐시 기반 변형). [Permissions and Formatters](../../docs/api/permissions-and-formatters.md)를 참고합니다.

## 13. 음성

**이유:** 2.x 음성은 실제 Discord 음성 서버와 통신하지 못했습니다. 두 전송 모드 모두 RTP 패킷 카운터를 AEAD nonce의 반대쪽 끝에 넣어서, Discord는 크레이트가 보낸 모든 패킷을 거부했고 받은 패킷은 하나도 복호화되지 않았습니다(테스트도 같은 잘못된 배치를 써서 통과했습니다). 3.0은 @discordjs/voice·Songbird와 같은 배치를 따르고, RTP 헤더 확장과 패딩을 올바르게 제거하며, 양방향을 known-answer 벡터로 고정합니다. 코드에서 바꿀 것은 없습니다. 음성이 그냥 동작합니다.

재생 파이프라인(`AudioPlayer`, 2.2.0 전용)은 이제 실시간으로 전달하고 전달하지 못한 것을 알려 줍니다. 직접 구현한 `VoiceFrameSink`는 길이 대신 `VoiceFrameTiming`을 받습니다:

<!-- snippet: 2.x -->
```rust
// 2.2.0
impl VoiceFrameSink for RecordingSink {
    fn send_opus_packet<'a>(&'a self, opus_frame: &'a [u8], duration: Duration) -> VoiceSinkFuture<'a> {
        Box::pin(async move { /* ... */ Ok(()) })
    }
    fn update_speaking(&self, speaking: bool) -> Result<(), DiscordError> { Ok(()) }
}
```

<!-- snippet: voice_sink -->
```rust
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use discordrs::voice::player::{VoiceFrameSink, VoiceFrameTiming, VoiceSinkFuture};
use discordrs::DiscordError;

/// A test double that records how much audio was sent and skipped.
#[derive(Default)]
struct RecordingSink {
    sent_us: AtomicU64,
    skipped_us: AtomicU64,
}

fn micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

impl VoiceFrameSink for RecordingSink {
    fn send_opus_packet<'a>(
        &'a self,
        _opus_frame: &'a [u8],
        timing: VoiceFrameTiming,
    ) -> VoiceSinkFuture<'a> {
        Box::pin(async move {
            // `gap`: media never transmitted before this frame (dropped,
            // late, or cancelled). A real sink advances its RTP timestamp
            // past it.
            self.skipped_us
                .fetch_add(micros(timing.gap), Ordering::Relaxed);
            // Transmit only when the future completes: the player may drop
            // it (a send stalled for 60 ms, stop, pause, or replacement).
            self.sent_us
                .fetch_add(micros(timing.duration), Ordering::Relaxed);
            Ok(())
        })
    }

    fn update_speaking(&self, _speaking: bool) -> Result<(), DiscordError> {
        Ok(())
    }
}
```

| 2.2.0 | 3.0 |
|---|---|
| `send_opus_packet(frame, Duration)` | `send_opus_packet(frame, VoiceFrameTiming { duration, gap })`. 완료될 때만 전송하고, drop된 future는 sink를 쓸 수 있는 상태로 남겨야 함 |
| `voice-encode` 없이 `AudioPlayer::play`가 조용히 진행 | `DiscordError::Voice` 반환 |
| 이미 구독한 sink를 다시 구독하면 두 번째 공급이 추가됨 | 이전 구독을 대체(순서대로 넘겨받음) |
| `Buffering`은 시작할 때만 | 재버퍼링과 일시정지 해제 중에도 |
| 종료 없음 | `AudioPlayer::shutdown().await` |

프레임은 60 ms 안에 전송을 시작해야 하고, 60 ms 동안 멈춘 전송은 취소됩니다. 큐가 차면 가장 오래된 프레임을 버리고, 건너뛴 미디어는 RTP timestamp 간격이 됩니다(`VoiceRuntimeHandle::send_opus_frame_after_gap`). [Voice](../../docs/api/voice.md)를 참고합니다.

## 14. 샤딩

**이유:** 2.x 감독자는 발행된 모든 이벤트를 무한 큐에 쌓았고, drain할 때 상태를 되돌렸으며, 샤드를 하나씩 기다려 뒤 샤드의 실패를 놓쳤습니다. 멀티 프로세스 관리자(2.2.0)에서는 읽기를 멈춘 자식이 종료 메시지를 가릴 수 있었고, stdout에 찍힌 다른 출력이 프로토콜을 깨뜨렸으며, 자식이 제각각 IDENTIFY했습니다. 3.0은 이 모두를 유한하게 만들고, 프레임으로 감싸고, 조율합니다.

감독자 이벤트는 샤드당 256개로 제한되고, 버린 것은 새 variant로 알려 주므로 완전 열거 match에는 그 팔이 필요합니다:

<!-- snippet: sharding_lagged -->
```rust
use discordrs::{DiscordError, ShardSupervisorEvent, ShardingManager};

fn report(manager: &ShardingManager) -> Result<(), DiscordError> {
    for event in manager.drain_events()? {
        match event {
            ShardSupervisorEvent::StateChanged { shard_id, state } => {
                println!("shard {shard_id}: {state:?}");
            }
            // New in 3.0: at most 256 events wait per shard; older ones are
            // dropped and counted here. `statuses()` is still current.
            ShardSupervisorEvent::Lagged { shard_id, missed } => {
                eprintln!("shard {shard_id}: {missed} events dropped");
            }
            other => println!("{other:?}"),
        }
    }
    Ok(())
}
```

멀티 프로세스 샤딩(2.2.0 전용)은 부모가 IDENTIFY를 허가합니다:

<!-- snippet: 2.x -->
```rust
// 2.2.0
for &shard_id in child.shard_ids() {
    let client = Client::builder(&token, gateway_intents::GUILDS)
        .event_handler(Handler)
        .shard(shard_id, child.shard_count());
    tokio::spawn(async move { client.start().await });
}
child.serve(|_request| async { json!({ "ok": true }) });
child.notify_ready()?;
```

<!-- snippet: process_child -->
```rust
use discordrs::sharding::process::ShardChildProcess;
use discordrs::{
    gateway_intents, Client, Context, DiscordError, EventHandler, GatewayConnectionConfig,
    ReadyPayload,
};
use serde_json::json;

struct Handler {
    child: ShardChildProcess,
}

impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, _ready: &ReadyPayload) {
        // Per shard, from READY (2.2.0: one `notify_ready()` for all).
        let (shard_id, _) = ctx.shard_pair();
        if let Err(error) = self.child.notify_shard_ready(shard_id) {
            eprintln!("shard {shard_id}: {error}");
        }
    }
}

async fn run_child(child: ShardChildProcess, token: &str) -> Result<(), DiscordError> {
    // `serve` returns a `Result` now; start it before the shards connect.
    child.serve(|_request| async { json!({ "ok": true }) })?;
    // Every fresh IDENTIFY waits for the parent's grant for its bucket.
    let gateway = GatewayConnectionConfig::default().identify_gate(child.identify_gate());
    for &shard_id in child.shard_ids() {
        let client = Client::builder(token, gateway_intents::GUILDS)
            .gateway_config(gateway.clone())
            .event_handler(Handler {
                child: child.clone(),
            })
            .shard(shard_id, child.shard_count());
        tokio::spawn(async move {
            if let Err(error) = client.start().await {
                eprintln!("shard {shard_id}: {error}");
            }
        });
    }
    child.shutdown_signal().await;
    Ok(())
}
```

| 2.x | 3.0 |
|---|---|
| `ShardSupervisorEvent` 완전 열거 `match` | `ShardSupervisorEvent::Lagged { shard_id, missed }` 추가 |
| `ShardRuntimeChannels::event_tx`, `ShardRuntimePublisher::event_tx` | 삭제: 상태도 갱신하는 `publish`로 보냄 |
| 샤드 하나가 끝나면 `drain_events()`가 실패 | 끝난 샤드도 drain. 남은 것이 없고 모든 샤드가 끝났을 때만 실패 |
| IPC 프레임이 맨 JSON 줄(2.2.0) | `@discordrs-ipc/1 ` 접두사와 크기 상한: 2.2.0 부모는 3.0 자식을 다룰 수 없으므로 함께 배포 |
| 자식이 연결할 때 바로 IDENTIFY(2.2.0) | `IdentifyGate`: 부모가 `shard_id % max_concurrency` 버킷마다 IDENTIFY를 허가하고 일일 세션 시작 예산을 지킴. `child.identify_gate()`를 `GatewayConnectionConfig::identify_gate`에 연결 |
| `ShardChildProcess::serve(..)`가 `()` 반환 | `Result` 반환 |
| `ParentMessage` / `ChildMessage` 완전 열거 | `#[non_exhaustive]`, identify variant 추가 |
| `spawn_stagger` 기본값 0보다 큼 | 0(허가가 IDENTIFY 간격을 정함) |
| `wait()` / `shutdown()` | 재시작 한도 소진과 비정상 종료를 오류로 반환. 종료 전체에 기한 하나 |
| `ProcessShardManager::broadcast(payload)`가 크기와 무관하게 전송 | 인코딩한 요청이 `max_ipc_frame_bytes`(기본 1 MiB)를 넘으면 보내기 전에 실패. 그보다 긴 자식 출력은 버림 |

종료 close code(4004, 4010–4014)를 받으면 이제 끝없이 재접속하지 않고 `start()`가 `DiscordError::Gateway`를 반환하며, 감독자는 첫 치명적 오류에서 다른 샤드도 멈춥니다. 직접 만든 멀티 프로세스 구성에서는 `IdentifyGate`를 구현해 `ClientBuilder::identify_gate`에 넘깁니다. [Sharding](../../docs/api/sharding.md)을 참고합니다.

## 15. 코드 규칙과 deprecation

크레이트는 이제 자체 코드 규칙을 lint로 강제합니다(라이브러리 코드에 `unwrap`/`panic!`/`as` 변환 금지, `#[must_use]` 결과, `unsafe` 금지). 사용자 코드에는 영향이 없습니다. 보이는 것은 대체 항목을 알려 주는 deprecation 경고이며, 3.0에서 deprecated된 모든 항목은 다음 메이저 버전에서 삭제됩니다:

| Deprecated | 대체 |
|---|---|
| `Snowflake`, `PermissionsBitField`, `MessageSelectDefaultValue` | `GenericId` / 타입 별칭, `Permissions`, `SelectDefaultValue` |
| `ButtonBuilder` | `CustomButtonBuilder`, `LinkButtonBuilder`, `PremiumButtonBuilder` |
| `InteractionReplyData`, `InteractionResponseBuilder`, `AutocompleteChoice` | `request::InteractionMessage`, `request::InteractionResponse`, `ApplicationCommandOptionChoice` |
| `InteractionResponse::PremiumRequired` | 프리미엄 버튼 |
| 인터랙션 헬퍼 11개(`defer_interaction`, `followup_message`, `edit_original_response`, …) | `InteractionResponder` 메서드 |
| `interactions_endpoint`, `typed_interactions_endpoint` | `try_interactions_endpoint`, `try_typed_interactions_endpoint` |
| `CheckboxBuilder::checked` | `default_checked` |
| REST 이름 변경 35개(`*_typed`, `*_from_request`, `*_config`, …) | 정식 메서드 |
| `create_guild`, `delete_guild`, `create_guild_from_template`, `modify_guild_mfa_level`, `Guild::delete`, `Guild::set_mfa_level` | 없음: Discord가 애플리케이션의 길드 생성을 없앰(2025-07-15) |

## 컴파일은 되지만 동작이 바뀐 것

코드 변경은 필요 없지만 봇이 관찰하는 결과가 달라질 수 있습니다:

- **REST 재시도.** 5xx, 타임아웃, 응답 유실은 반복해도 안전한 요청(GET, HEAD, OPTIONS, PUT, DELETE, `enforce_nonce`를 쓴 Create Message)만 재시도하므로, 응답을 잃은 요청이 메시지를 두 번 보내지 않습니다. 예전 동작은 `RestClientBuilder::retry_non_idempotent_requests(true)`로 다시 켭니다. 관리형 클라이언트는 리다이렉트를 따르지 않고, rate-limit 버킷은 채널·길드·웹훅 단위라 한 채널의 429가 다른 채널을 막지 않습니다.
- **토큰은 로그에 남지 않습니다**: 웹훅·인터랙션 토큰은 오류, rate-limit 정보, `Debug` 출력에서 가려집니다.
- **게이트웨이**: 종료 close code는 오류와 함께 샤드를 멈추고, READY는 자기 샤드의 캐시만 지우며, 이벤트 큐가 넘치면 빈틈을 둔 채 계속하지 않고 샤드를 멈춥니다(캐시도 무효화). 사용 불가 GUILD_CREATE는 `GuildCreatePayload::Unavailable`입니다.
- **인터랙션**: follow-up과 `edit_reply`는 Discord가 첫 응답을 받아들일 때까지 기다리고, 두 번째 첫 응답은 로컬에서 실패하며, `InteractionResponse::defer(false)`는 `{"data":{"flags":0}}`을 보냅니다. 응답을 REST로 보냈으면 HTTP 엔드포인트는 202로 답합니다.
- **디코딩**이 엄격해졌습니다: 잘못된 ID, 빠진 필수 판별값, 범위를 넘는 값, 중복 키는 오류이고, REST 디코딩 오류에 위치가 붙습니다. `Webhook::kind`는 Discord의 `type` 키를 읽습니다.
- 2.x에서 틀렸던 **와이어 값**이 Discord 값으로 바뀌었습니다: 메시지 플래그, 권한 비트 41–52, 체크박스 `default`, 권한 외 비트필드의 숫자 직렬화.
- **기본 `allowed_mentions`**가 `edit_webhook_message`와 Components V2 채널 헬퍼에도 적용됩니다.
- **컬렉터**는 창이 만료된 뒤 이벤트를 내지 않습니다.
- **음성**이 실제 Discord 음성 서버와 송수신합니다(2.x는 불가능했습니다).

## 체크리스트

- [ ] Rust 1.88 이상, `discordrs = "3"`, 인터랙션 엔드포인트를 서빙하면 axum 0.8. 쓰지 않으면 `async-trait` 제거.
- [ ] 모든 `EventHandler`, `CacheBackend`, `InteractionHandler`, `TypedInteractionHandler`, `CachedManager` impl에서 `#[async_trait]` 제거.
- [ ] `Snowflake`를 타입 ID로 교체. 신뢰할 수 없는 ID는 `parse()?` / `try_from(..)?`로 파싱. ID의 `.clone()`과 `&` 제거.
- [ ] 원시 판별값 숫자와 `u8` 상수 모듈을 newtype과 그 상수로 교체. `_` 팔 추가.
- [ ] `handle_event`에서 `event.kind()`로 match. 타입 훅 시그니처 갱신(`Arc<Message>`, `Arc<MessageUpdate>`, 참조). 디코딩 실패 처리는 `decode_failed`로.
- [ ] 알려진 이벤트의 `event.raw()`를 읽는다면 `keep_raw_events(true)` 설정.
- [ ] MESSAGE_UPDATE를 부분 `MessageUpdate`로 처리. 수정은 `MessageUpdateCollector`로 수집.
- [ ] 캐시 upsert에 이벤트의 `Arc`를 넘김. `apply_message_update` / `apply_guild_create` 사용.
- [ ] 외부 `CacheBackend`를 epoch 계약, `Arc` 스냅샷, `clear_shard_cache`에 맞게 다시 작성. `Arc<YourBackend>`를 넘김.
- [ ] `Value` 본문을 `discordrs::request` 타입으로 교체. `_typed`·삭제된 REST 메서드 이름 변경. 감싸지 않은 라우트는 `RawRequest`.
- [ ] `CustomButtonBuilder` / `LinkButtonBuilder` / `PremiumButtonBuilder`와 종류별 select 빌더로 교체. 모든 section에 accessory 지정.
- [ ] `decode_interaction`으로 디코딩, `EndpointReply` 반환, `InteractionResponder`로 응답, 타입 옵션 값과 모달 값 읽기.
- [ ] `Permissions` 사용. `add` / `remove` / `merge` 결과를 버리지 않기.
- [ ] 직접 만든 `VoiceFrameSink`: `VoiceFrameTiming`을 받고 취소 규칙 지키기.
- [ ] `ShardSupervisorEvent::Lagged` 팔 추가. 프로세스 샤딩은 부모와 자식을 함께 배포하고 `identify_gate()` 연결.
- [ ] `cargo build`(또는 `cargo clippy`)를 돌려 경고에 나온 deprecated 호출을 모두 교체.
- [ ] [컴파일은 되지만 동작이 바뀐 것](#컴파일은-되지만-동작이-바뀐-것)을 검토. 특히 POST 재시도에 기대고 있었다면 REST 재시도 항목.
