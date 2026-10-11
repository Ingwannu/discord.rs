# 게이트웨이 API

`gateway` 기능 플래그 활성화 시 WebSocket 런타임을 사용합니다.

## 핵심 타입

- `GatewayClient`: identify/heartbeat/resume/reconnect 처리
- `Client`: 고수준 타입드 런타임 (`BotClient`는 호환 별칭)
- `Event`: 받은 디스패치 하나. `Arc<Event>`로 공유하며 디코딩된 `EventKind`, `EventMetadata`, 선택적 원본 payload를 담습니다([이벤트와 핸들러](#이벤트와-핸들러-30) 참고)
- `Context`: 핸들러 공유 컨텍스트(`http`, 캐시, typemap, 샤드 정보)
- `EventHandler`: 이벤트 콜백 트레잇(일반 `async fn`으로 구현)
- `EventDispatchMode`: 직렬(기본) 또는 동시 핸들러 스케줄링

## 기본 실행

```rust
Client::builder(&token, gateway_intents::GUILDS | gateway_intents::GUILD_MESSAGES)
    .event_handler(handler)
    .start()
    .await?;
```

## `#[async_trait]` 없이 핸들러 구현하기

3.0부터 크레이트는 `async-trait`에 의존하지 않습니다. `EventHandler`, `CacheBackend`, `InteractionHandler`, `TypedInteractionHandler`, `CachedManager`는 메서드를 `fn ... -> impl Future<Output = T> + Send`(트레잇 반환 위치 `impl Trait`)로 선언하므로 구현은 일반 `async fn`으로 씁니다. 기존 impl에서 `#[async_trait]`와 `use async_trait::async_trait;`를 지우고, 다른 곳에서 쓰지 않으면 `Cargo.toml`의 `async-trait`도 지웁니다.

이전(2.x):

```rust
use async_trait::async_trait;
use discordrs::{Context, EventHandler, Message};

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn message_create(&self, _ctx: Context, message: Message) {
        println!("{}", message.content);
    }
}
```

이후(3.0, 4단계부터 훅은 공유된 `Arc<Message>`를 받습니다. [이벤트와 핸들러](#이벤트와-핸들러-30) 참고):

```rust
use std::sync::Arc;

use discordrs::{Context, EventHandler, Message};

struct Handler;

impl EventHandler for Handler {
    async fn message_create(&self, _ctx: Context, message: Arc<Message>) {
        println!("{}", message.content);
    }
}
```

함께 알아 둘 점:

- **future는 `Send`여야 합니다.** 핸들러와 캐시 백엔드 호출은 생성된(spawn) Tokio 작업에서 실행되므로, impl의 `async fn`이 `.await`를 건너 `Send`가 아닌 값(`Rc`, `RefCell` 빌림, `std::sync::MutexGuard`)을 들고 있으면 안 됩니다. 컴파일러가 impl 위치에서 "future cannot be sent between threads safely"로 알려 줍니다. `Arc`, `tokio::sync::Mutex`를 쓰거나 await 전에 값을 버립니다.
- **남겨 둔 `#[async_trait]`는 `E0195`로 실패합니다**("lifetime parameters or bounds on method ... do not match the trait declaration"). 이 오류는 속성을 지우라는 뜻입니다.
- **`dyn EventHandler` / `dyn CacheBackend`는 없습니다.** 트레잇이 더 이상 object safe가 아닙니다. `ClientBuilder::event_handler(handler)`는 그대로 핸들러를 값으로 받습니다. `ClientBuilder::cache_backend`는 `Arc<dyn CacheBackend>` 대신 구체 타입 `B: CacheBackend`의 `Arc<B>`를 받습니다. `.cache_backend(Arc::new(MyBackend::new()))`는 그대로 컴파일되지만, `Arc<dyn CacheBackend>` 타입 변수는 `Arc<MyBackend>`(또는 제네릭 `Arc<B>`)로 바꿔야 합니다.
- 기본 메서드는 그대로입니다. 필요한 훅만 구현하며, `handle_event`를 재정의하면 타입 훅으로 가는 기본 라우팅을 대체합니다.
- 런타임은 내부에서 핸들러 호출마다 한 번 박싱합니다(async-trait와 같음). 기본 `handle_event` 라우팅은 이벤트당 할당이 두 번에서 한 번으로 줄었습니다.

## 이벤트와 핸들러 (3.0)

디스패치는 한 번만 디코딩되고, 캐시·컬렉터 허브·핸들러가 같은 `Arc<Event>` 하나를 공유합니다. 전달 과정에서 payload를 복사하지 않습니다.

```rust
use std::sync::Arc;

use discordrs::{Context, Event, EventHandler, EventKind};

struct Handler;

impl EventHandler for Handler {
    async fn handle_event(&self, _ctx: Context, event: Arc<Event>) {
        match event.kind() {
            EventKind::Ready(ready) => println!("{}로 준비됨", ready.user.username),
            EventKind::MessageCreate(message) => println!("{}", message.content),
            EventKind::Unknown => println!("새 디스패치 {}", event.name()),
            _ => {}
        }
    }
}
```

- `event.kind()`는 디코딩된 `EventKind`입니다. 디스패치마다 variant가 하나이고, 이 크레이트에 타입이 없는 디스패치는 `Unknown`, 타입이 있지만 payload가 맞지 않으면 `DecodeFailed(EventDecodeError)`입니다. 캐시가 그대로 보관하는 payload(`MessageCreate(Arc<Message>)`, `MessageUpdate(Arc<MessageUpdate>)`, `MemberAdd`/`MemberUpdate`의 `MemberEvent { guild_id, member: Arc<Member> }`)는 캐시·외부 캐시 백엔드와 공유하는 `Arc`이고, 나머지 큰 payload는 `Box`, 작은 ID·삭제 이벤트는 인라인입니다. `size_of::<Event>()`는 192바이트 이하입니다(2.x는 4,848바이트, 레이아웃 테스트로 고정).
- `event.metadata()`(`EventMetadata`)는 디스패치 이름 `name()`, 받은 샤드 `shard()`, 게이트웨이 시퀀스 `sequence()`, 읽기 루프가 프레임을 받은 시각 `received_at()`(큐에 들어가기 전)을 담습니다. `event.name()`은 이름의 줄임입니다.
- `event.raw()`는 원본 `d` JSON(`Option<&RawValue>`)입니다. 알려진 이벤트는 `ClientBuilder::keep_raw_events(true)`일 때만 보관합니다(기본은 끔: 원본을 보관하면 MESSAGE_CREATE 이벤트가 다섯 배 커졌습니다). 알 수 없는 디스패치와 디코드 실패는 항상 보관하므로 Discord가 보낸 내용을 잃지 않습니다. `Event`의 `Debug`는 원본의 길이만 출력합니다(인터랙션 토큰이 들어 있을 수 있음).
- **GUILD_CREATE**는 `GuildCreatePayload`로 디코딩됩니다. 봇이 읽을 수 있는 길드는 `Available(Box<GuildCreate>)`, Discord가 사용 불가 길드 객체(`{"id": "...", "unavailable": true}`, 장애가 계속되는 길드)를 보내면 `Unavailable(UnavailableGuild)`입니다. 사용 불가 길드는 id만 있고 `Guild`로 만들지 않습니다. 캐시는 `"unavailable": true`인 GUILD_DELETE와 마찬가지로 그 길드를 지우며, 길드가 다시 사용 가능해지면 완전한 GUILD_CREATE가 다시 옵니다. `payload.id()`는 두 경우 모두, `payload.available()`은 `GuildCreate`를 돌려줍니다. `GuildCreate`는 `guild`(GUILD_CREATE 전용 컬렉션을 비운 `Arc<Guild>`)와 `channels`, `threads`, `members: Vec<Arc<Member>>`, `presences: Vec<Arc<Presence>>`, `voice_states`, `emojis`, `stickers`, `stage_instances`, `soundboard_sounds`, `guild_scheduled_events`로 나뉘며, 채널·스레드·음성 상태에는 `guild_id`가, presence에는 `user_id`가 이미 채워져 있습니다. 캐시는 같은 `Arc`를 저장합니다. 2.x의 `Guild`가 필요하면 `to_guild()`로 다시 만듭니다.
- **MESSAGE_UPDATE**는 부분 메시지 `MessageUpdate`로 디코딩됩니다. 바뀔 수 있는 필드는 모두 `Patch<T>`(`Message`에서 `T`인 필드) 또는 `NullablePatch<T>`(`Option<T>`인 필드)입니다. 생략은 `Missing`(유지), `null`은 지움, `[]`는 빈 목록으로 교체이며, `author`는 필드별로 병합하는 `PartialUser`입니다. 가진 메시지에 `update.apply_to(&mut message)`로 적용합니다. 갱신만으로 완전한 메시지를 만들지 않습니다. 내장 캐시도 캐시된 작성자에 같은 방식으로 패치를 적용하므로(`CacheHandle::apply_partial_user`) 사용자 이름만 바뀐 갱신은 캐시된 아바타를 유지하고 `"avatar": null`은 아바타를 지웁니다.

타입 훅은 이벤트가 저장한 방식 그대로 payload를 받으므로 훅 호출이 payload를 복사하지 않습니다.

| 훅 | 받는 값 |
|---|---|
| `handle_event` | `Arc<Event>` |
| `message_create` | `Arc<Message>` |
| `message_update` | `Arc<MessageUpdate>` |
| `member_add`, `member_update` | `GuildId`, `Arc<Member>` |
| `guild_create` | `&GuildCreatePayload`(사용 가능 또는 사용 불가 길드) |
| `resumed` | `ctx` 외에 없음 |
| `raw_event`(알 수 없는 디스패치) | `&str` 이름, `&RawValue` payload |
| `decode_failed`(신규) | `&str` 이름, `&EventDecodeError`, `&RawValue` payload |
| 그 밖의 훅 | 이벤트 안의 참조(예: `&ReadyPayload`, `&Channel`, `&Interaction`, `&ReactionEvent`) |

`Arc` 인자는 그대로 보관하거나 태스크로 옮길 수 있습니다. 참조는 핸들러 호출 동안만 유효하므로 보관할 값은 복제하거나, `handle_event`를 재정의해 `Arc<Event>`를 보관합니다.

### 2.x `Event` enum에서 옮기기

```rust
// 2.x
async fn handle_event(&self, ctx: Context, event: Event) {
    match event {
        Event::Ready(ready) => println!("{}", ready.data.user.username),
        Event::MessageCreate(event) => println!("{}", event.message.content),
        _ => {}
    }
}

// 3.0
async fn handle_event(&self, ctx: Context, event: Arc<Event>) {
    match event.kind() {
        EventKind::Ready(ready) => println!("{}", ready.user.username),
        EventKind::MessageCreate(message) => println!("{}", message.content),
        _ => {}
    }
}
```

- `event` 대신 `event.kind()`(`EventKind::...`)로 매칭합니다. 예전 `Event::kind()` 문자열은 `event.name()`입니다.
- payload만 감싸던 래퍼 구조체는 없어지고 variant가 payload를 바로 담습니다: `ReadyEvent`, `GuildEvent`, `ChannelEvent`, `MessageEvent`, `InteractionEvent`, `VoiceStateEvent`, `VoiceServerEvent`, `UserUpdateEvent`, `EntitlementEvent`, `SubscriptionEvent`, `SoundboardSoundEvent`, `StageInstanceEvent`, `ResumedEvent`, 그리고 `{ data, raw }` 래퍼인 `GuildDeleteEvent`, `MemberRemoveEvent`, `GuildMembersChunkEvent`, `RoleDeleteEvent`, `MessageDeleteEvent`. payload 구조체에는 더 이상 `raw` 필드가 없으며, `keep_raw_events(true)`와 `event.raw()`를 씁니다.
- `GUILD_CREATE`는 `Guild`가 아니라 `GuildCreatePayload`(사용 가능한 `GuildCreate` 또는 `UnavailableGuild`), `MESSAGE_UPDATE`는 `Message`가 아니라 `MessageUpdate`입니다.
- 디코딩에 실패한 payload는 `Unknown`이 아니라 `EventKind::DecodeFailed`이며 `decode_failed` 훅으로 갑니다.
- `decode_event(name, data: &str)`는 테스트와 도구용으로 JSON 텍스트를 원본을 보관한 `Event`로 디코딩합니다. 런타임은 `Event::decode(metadata, raw, keep_raw)`를 씁니다.

파이프라인이 이벤트를 옮기는 방식(4단계): 읽기 루프는 얕은 봉투(`op`, `s`, `t`, 빌린 `RawValue`인 `d`. `serde_json::Value` 트리 없음)만 읽고 `d`를 샤드 큐로 한 번 복사합니다. 처리기는 그 텍스트에서 타입 payload를 바로 디코딩하고, payload의 `Arc`로 캐시를 갱신하고, `Arc<Event>`를 컬렉터 허브에 발행한 뒤 같은 `Arc`를 핸들러에 넘깁니다.

## 인텐트

`gateway_intents`는 문서화된 모든 인텐트 비트에 대한 상수를 제공하며, 2.1에서 Poll 인텐트가 추가되었습니다.

- `gateway_intents::GUILD_MESSAGE_POLLS`(`1 << 24`)와 `gateway_intents::DIRECT_MESSAGE_POLLS`(`1 << 25`)는 `MESSAGE_POLL_VOTE_*` 이벤트를 활성화하며, 두 인텐트 모두 `gateway_intents::NON_PRIVILEGED`에 포함됩니다.
- `gateway_intents::GUILD_EXPRESSIONS`는 비트 3의 현재 Discord 명칭입니다(기존 `GUILD_EMOJIS_AND_STICKERS` 상수도 유지됩니다).

## 초기 Presence와 디스패치 모드

`ClientBuilder::presence(...)`는 IDENTIFY 안에 초기 presence를 담아 보내므로, READY 이후에 상태를 갱신하는 대신 원하는 상태로 바로 접속합니다. `ClientBuilder::event_dispatch(...)`는 핸들러 호출 스케줄링 방식을 선택합니다.

```rust
use discordrs::{EventDispatchMode, UpdatePresence};

Client::builder(&token, intents)
    .event_handler(handler)
    .presence(UpdatePresence::online_with_activity("티켓 처리 중"))
    .event_dispatch(EventDispatchMode::Concurrent)
    .start()
    .await?;
```

- `EventDispatchMode::Serial`(기본): 샤드의 핸들러 호출을 게이트웨이 순서대로 하나씩 실행합니다. INTERACTION_CREATE는 전용 레인을 씁니다(아래 인터랙션 레인 참고).
- `EventDispatchMode::Concurrent`: 핸들러 호출을 동시 태스크로 실행하되 샤드당 최대 `max_concurrent_handlers`개(기본 256)까지만 돌립니다. 느린 핸들러 하나가 다른 핸들러를 늦추지 않습니다.

### 이벤트 파이프라인 (3.0)

샤드마다 이벤트를 두 단계로 처리합니다. 내부 처리기가 모든 이벤트를 게이트웨이 순서대로 먼저 반영합니다(캐시 갱신, 외부 캐시 백엔드 쓰기, 음성 상태, `fetch_members` 청크 연결, 컬렉터 발행). 이 단계는 핸들러를 기다리지 않습니다. 그다음 샤드별 유한 큐에서 핸들러 호출을 스케줄링합니다. 그 결과:

- 어느 모드에서든 핸들러 안에서 `ctx.fetch_members(...)`나 컬렉터를 `await`해도 샤드가 멈추지 않습니다. (3.0 이전 Serial 모드에서는 청크가 기다리는 핸들러 뒤에 쌓여 타임아웃까지 교착했습니다.)
- 핸들러가 읽는 캐시에는 지금 처리 중인 이벤트 이후의 이벤트가 이미 반영돼 있을 수 있습니다.
- 핸들러가 패닉하면 로그만 남고 이후 핸들러 호출은 계속됩니다.

두 단계 모두 `ClientBuilder::event_queue_capacity(...)`(샤드·단계당 기본 10,000개)로 제한됩니다.

| 큐 | 가득 찼을 때 |
|---|---|
| 게이트웨이 → 내부 처리기 | 샤드를 **멈추고** `DiscordError::Gateway`("gateway event queue overflowed")를 돌려주며, 그 샤드의 캐시 항목을 무효화하고(`CacheHandle::clear_shard`, 외부 백엔드에는 `clear_shard_cache`) 대기 중인 `fetch_members`를 실패시킵니다. 게이트웨이 클라이언트는 이미 시퀀스 번호를 올렸으므로 잃은 디스패치는 RESUME으로도 다시 받을 수 없고, 계속 돌리면 빈틈이 있는 캐시를 그대로 쓰게 됩니다. 읽기 루프는 하트비트도 처리하므로 자리가 날 때까지 기다리지 않습니다. 내부 처리 자체가 멈출 때만 생깁니다(예: 어떤 태스크가 `ctx.voice()` 쓰기 잠금을 계속 쥐고 있는 경우, `fetch_members().await` 동안 쥐는 경우 포함). 내부 처리기가 다른 이유로 죽어도 같은 방식으로 샤드를 멈춥니다. |
| 내부 처리기 → 핸들러 | 핸들러에만 전달하지 않습니다. 캐시·컬렉터·`fetch_members`는 이미 이벤트를 받았습니다. 버린 개수는 주기 제한된 경고로 보고합니다. |

어느 큐든 절반이 차면 백로그 경고가 남습니다(샤드당 최대 30초에 한 번).

```rust
use std::time::Duration;
use discordrs::{Client, EventDispatchMode};

Client::builder(&token, intents)
    .event_handler(handler)
    .event_dispatch(EventDispatchMode::Concurrent)
    .max_concurrent_handlers(64)
    .event_queue_capacity(20_000)
    .handler_shutdown_timeout(Duration::from_secs(10))
    .start()
    .await?;
```

### 인터랙션 레인 (3.0)

INTERACTION_CREATE는 일반 핸들러 큐를 거치지 않습니다. 샤드마다 전용 유한 큐(`interaction_queue_capacity`, 기본 256)와 예약된 실행 슬롯(`max_concurrent_interactions`, 기본 32)을 가진 인터랙션 레인이 있어, 느린 일반 핸들러 때문에 Discord의 3초 최초 응답 시한을 놓치지 않습니다.

- **순서:** 인터랙션은 도착 순서대로 들여보냅니다. 들어간 인터랙션은 서로(슬롯 수까지), 그리고 일반 핸들러 호출과 동시에 실행됩니다. `EventDispatchMode::Serial`에서도 마찬가지입니다. 끝난 핸들러 작업은 매번 새로 들여보내기 전에 회수하므로, 계속 밀린 레인도 메모리에는 슬롯 수(와 막 끝난 하나)만큼만 남습니다.
- **수락 시한:** 인터랙션은 샤드가 받은 시점부터 `interaction_admission_timeout`(기본 2초)까지만 슬롯을 기다립니다. 그때까지 들어가지 못하거나 큐에 자리가 없으면 핸들러 대신 과부하 훅으로 갑니다. 만료가 빈 슬롯보다 우선합니다. 시한이 지난 인터랙션은 같은 순간 슬롯이 비더라도 시작하지 않으며, 실행 직전에 시한을 한 번 더 확인합니다.
- **과부하 훅:** `on_interaction_overload(...)`는 `OverloadedInteraction`(`ctx`, `interaction`, `reason`, `waited`)을 받아 앱이 응답하거나 지연(defer)할 수 있게 합니다. 자동완성·모달·컴포넌트마다 맞는 응답이 다르므로 런타임은 스스로 응답하지 않습니다. `interaction`은 공유 응답 상태를 가지고 있으므로 `InteractionResponder`로 응답·지연하면 핸들러에서 응답할 때와 같은 확인(ack) 상태 머신을 거칩니다. 훅은 이벤트 파이프라인이 아니라 샤드마다 전용 스레드(Tokio 런타임 진입 상태)에서 실행되므로, 느리거나 막히는 훅이 캐시 갱신이나 전달을 멈추지 않습니다. 최대 64개까지 대기하고, 그 이상은 주기 제한된 경고와 함께 버립니다. 훅이 없으면 로그만 남고 Discord는 사용자에게 "상호작용 실패"를 보여 줍니다.
- `Context::event_received_at()`으로 이벤트가 도착한 시각을 알 수 있어, 인터랙션 핸들러가 남은 시간을 판단할 수 있습니다.

### 핸들러 안의 컬렉터 (3.0)

내부 처리기는 이벤트를 컬렉터 허브에 발행한 다음 핸들러 호출을 큐에 넣으므로, 핸들러가 실행될 즈음에는 이후 이벤트가 이미 발행됐을 수 있습니다. `Context::message_collector()`, `message_update_collector()`, `component_collector()`, `interaction_collector()`, `modal_collector()`는 핸들러가 처리 중인 이벤트 위치(`Context::event_cursor()`)에서 시작해 그 사이 발행된 이벤트를 다시 보여 주고, 빈틈 없이 실시간 이벤트로 이어집니다. 허브는 최근 256개 이벤트를 보관하며, 그보다 오래된 것은 건너뛰고 컬렉터의 `lagged_events()`에 셉니다. `ctx.collectors().message_collector()` 등은 여전히 "지금부터" 시작합니다.

### 종료와 오류 (3.0)

`ClientBuilder::start()`는 샤드의 게이트웨이 연결이 끝난 뒤 최대 `handler_shutdown_timeout`(기본 5초)에, 중단 확인과 과부하 훅 스레드 종료를 위한 최대 1초를 더한 시간 안에 돌아옵니다. 이 시간은 종료를 요청한 시점이 아니라 게이트웨이가 끝난 시점부터 셉니다. 그 사이 큐에 남은 이벤트는 계속 전달되고 실행 중인 핸들러는 마무리할 수 있으며, 그 뒤에도 남은 핸들러 작업은 중단하고 **끝날 때까지 기다립니다(join)**. 외부 캐시 백엔드에 쌓인 쓰기도 같은 시간 안에 비우고 워커를 join하므로, `start()`가 돌아온 뒤 이 클라이언트의 쓰기가 늦게 도착하지 않습니다. 중단한 작업이 1초 안에 멈췄음을 확인하지 못하면(예: 스레드를 막는 핸들러) 멈췄다고 주장하지 않고 그 사실을 오류로 돌려줍니다.

반환값은 게이트웨이의 결과이므로 4004(인증 실패), 4014(허용되지 않은 인텐트) 같은 재접속 불가 close code나 이벤트 큐 넘침은 `DiscordError::Gateway`로 호출자에게 전달됩니다. `start()` future를 취소해도 샤드의 처리기·핸들러 작업이 함께 멈춥니다. 다중 샤드는 [샤딩](../../docs/api/sharding.md) 문서를 참고합니다.

## 기본 Allowed Mentions와 캐시 백엔드 (2.2.0)

2.2.0에서 `ClientBuilder`에 두 가지 구성 옵션이 추가되었습니다(3.0부터 `cache_backend`는 구체 백엔드 타입의 `Arc<B>`를 받습니다. 위 절 참고).

```rust
use std::sync::Arc;
use discordrs::cache::CacheBackend;
use discordrs::{AllowedMentions, Client};

fn configure<B: CacheBackend + 'static>(token: &str, intents: u64, backend: Arc<B>) {
    let _builder = Client::builder(token, intents)
        // 페이로드가 allowed_mentions를 직접 지정하지 않은 경우에만 주입됩니다
        // — discord.js의 ClientOptions#allowedMentions에 해당합니다.
        .default_allowed_mentions(AllowedMentions::default())
        // 멤버/메시지/presence 캐시 쓰기를 외부 저장소(Redis, Valkey 등)로
        // 전달합니다. 클라이언트당 하나의 순차 워커가 전달하므로 느린
        // 백엔드가 게이트웨이를 지연시키지 않습니다.
        .cache_backend(backend);
}
```

3.0부터 백엔드는 쓰기를 게이트웨이 순서대로 받으며 epoch 계약을 구현해야 합니다. 저장소 전체에 epoch 하나를 두고, 정리(`clear_cache` / `clear_shard_cache`)는 기대 epoch를 받아 일치할 때만 실행하는 비교-교체(CAS)이며 `CacheClear::Applied` 또는 `Rejected`를 돌려주고, 모든 쓰기는 자기 epoch를 가집니다. 그래서 이전 클라이언트의 늦은 쓰기·정리는 거부되고, 샤드 구성이 바뀌면 이전 구성의 모든 쓰기가 무효가 됩니다. 정리에는 `CacheClearId`가 붙으므로 응답만 잃어버린 채 이미 반영된 정리를 다시 보내면 반영된 것으로 인식됩니다. 한 저장소에는 실행 중인 클라이언트 하나만 미러링할 수 있으며, 이는 배포에서 직접 보장해야 합니다. 시작 시점에는 감지되지 않고, 저장소를 같이 쓰던 이전 클라이언트는 쓰기가 이미 무시되고 있다가 다음 정리 때에야 `superseded`로 알아차립니다. 큐에 못 들어가거나 실패·패닉한 쓰기는 그 샤드의 앞선 대기 쓰기를 버리고 샤드를 정리하므로 미러가 낡은 상태로 남지 않으며, `Context::cache_backend_health()`로 상태를 보고, 종료 시 복구하지 못한 샤드를 오류로 알립니다. 부분 MESSAGE_UPDATE는 완전한 메시지로 저장하지 않고 타입 패치(`MessageUpdate`)로 병합합니다. 3.0(4단계)부터 `put_member`/`put_message`/`put_presence`는 이벤트·메모리 캐시와 같은 할당인 `Arc<T>` 스냅샷을 받습니다. 자세한 내용은 [캐시와 컬렉터](../../docs/api/cache-and-collectors.md) 문서의 External Cache Backend 절을 참고합니다.

REST 쪽에는 같은 기본값이 `RestClient::builder().default_allowed_mentions(...)`로 존재하며, 핸들러 안에서는 `Context::default_allowed_mentions()`로 구성된 값을 확인할 수 있습니다.

## 게이트웨이 이벤트에서 인터랙션에 응답하기 (2.2.0)

`discordrs::response::InteractionResponder` 트레잇을 임포트하면 `EventKind::InteractionCreate` 페이로드에 discord.js 스타일로 바로 응답할 수 있습니다.

```rust
use discordrs::response::InteractionResponder;
use discordrs::{Context, Event, EventKind, Interaction};

async fn on_event(ctx: Context, event: &Event) -> Result<(), discordrs::DiscordError> {
    if let EventKind::InteractionCreate(interaction) = event.kind() {
        if let Interaction::ChatInputCommand(command) = &**interaction {
            command.reply(&ctx.http, "안녕하세요").await?;
        }
    }
    Ok(())
}
```

전체 응답 메서드(`reply_ephemeral`, `defer`, `edit_reply`, `follow_up`, `show_modal`, `respond_autocomplete` 등)는 [HTTP 및 헬퍼 API](../api/http-and-helpers.md)를 참고합니다.

## 게이트웨이로 길드 멤버 가져오기

`Context::fetch_members(...)`는 게이트웨이로 길드 멤버를 요청하고, 대응하는 `GUILD_MEMBERS_CHUNK` 페이로드를 기다려 수집합니다. discord.js의 `guild.members.fetch()`에 해당하며, 가져온 멤버와 presence는 캐시에도 채워집니다.

```rust
// 전체 멤버 (GUILD_MEMBERS privileged 인텐트 필요)
let members = ctx.fetch_members(guild_id, None, None).await?;

// 사용자명 접두사 검색 + 개수 제한
let admins = ctx.fetch_members(guild_id, Some("admin".to_string()), Some(10)).await?;
```

기본 전체 시한은 60초이며 `Context::fetch_members_with_timeout(...)`으로 조정할 수 있습니다. raw 청크 처리가 필요하면 저수준 `ctx.request_guild_members(...)`를 그대로 사용할 수 있습니다.

3.0부터 `fetch_members`는 어떤 핸들러 안에서든 기다려도 안전하고(위 이벤트 파이프라인 참고), 청크 번호 `0..chunk_count`가 모두 도착해야만 돌아오며(빠진 청크가 있으면 받은 개수를 밝힌 타임아웃 오류이지, 조용히 모자란 목록이 아닙니다), 호출이 끝나거나 타임아웃되거나 취소되면 청크 대기 항목이 지워지고, 샤드가 멈추면 즉시 실패합니다. 길드 단위 게이트웨이 명령(`request_guild_members`, `fetch_members`, `update_voice_state`, `join_voice`, `leave_voice`)은 그 길드를 맡은 샤드(`(guild_id >> 22) % shard_count`)가 같은 프로세스에 있으면 그 샤드로 보내므로, 어느 샤드의 `Context`에서든 쓸 수 있습니다.

## 운영 노트

- 핸들러는 스레드를 막지 말고 `await`하도록 작성합니다. 무거운 작업은 백그라운드 태스크로 넘기거나 `EventDispatchMode::Concurrent`를 씁니다. Serial 모드에서 느린 핸들러는 다음 핸들러 호출(캐시 갱신은 아님)을 늦추고, `event_queue_capacity`만큼 밀리면 전달을 버리기 시작합니다.
- 3.0부터 내부 이벤트 처리와 핸들러 호출을 분리하고, 샤드별 두 큐와 동시 핸들러 수에 상한을 두며, 인터랙션에 수락 시한이 있는 전용 레인을 주고, 이벤트 흐름에 빈틈이 생길 상황이면 샤드를 멈추고 캐시를 무효화하며, READY 캐시 무효화를 받은 샤드로 한정하고, 게이트웨이가 멈추면 `start()`/`ShardSupervisor::wait()`가 곧바로 돌아옵니다(위 참고).
- 2.1부터 IDENTIFY는 Discord의 샤드당 5초 1회 제한에 맞춰 페이싱되고, 30초 미만의 짧은 세션은 즉시 재접속하는 대신 점증 백오프로 재접속하므로 INVALID_SESSION 반복 시의 재접속 루프를 방지합니다.
- 게이트웨이 프로토콜 실패(누락된 Hello, 디코드 오류, 종료 close code, 명령 큐 오버플로)는 `DiscordError::Model`이 아니라 `DiscordError::Gateway`로 표면화됩니다.
- 3.0부터 재접속해도 소용없는 close code(4004 인증 실패, 4010 잘못된 샤드, 4011 샤딩 필요, 4012 잘못된 API 버전, 4013 잘못된 인텐트, 4014 허용되지 않은 인텐트)를 받으면 `GatewayClient::run`이 백오프 재접속을 반복하지 않고 `DiscordError::Gateway`를 돌려주며 샤드를 멈춥니다. 4007(잘못된 시퀀스)과 4009(세션 만료)는 Discord가 거부할 RESUME 대신 새 세션으로 접속합니다. 연결별 하트비트·송신 작업은 어느 경로로 끝나든 정리됩니다.

## 채널 가리기 (3.0)

2026-11-16부터 Discord는 봇이 볼 수 없는 채널을 가려진 메타데이터로 보냅니다(`CHANNEL_CREATE`, `CHANNEL_UPDATE`, `GUILD_CREATE`의 채널 목록). `GET /guilds/{id}/channels`에서는 아예 빠집니다. 이름이 아니라 `Channel::is_obfuscated()`(`CHANNEL_OBFUSCATED` 플래그, `1 << 17`)로 판별합니다. 이런 채널은 `id`, `type`, `position`, `parent_id`만 믿을 수 있고, 권한 덮어쓰기는 `@everyone` 거부 하나만 들어 있는 가짜 값입니다. 봇이 다시 접근할 수 있게 되면 Discord가 전체 정보가 담긴 `CHANNEL_UPDATE`를 보내고, 캐시의 사본도 그걸로 바뀝니다.

마감 전에 시험하려면 IDENTIFY `capabilities`로 미리 켤 수 있습니다(Discord는 이 옵션이 임시라고 밝혔습니다).

```rust
use discordrs::bitfield::gateway_capabilities;

let client = Client::builder(token, intents)
    .gateway_capabilities(gateway_capabilities::CHANNEL_OBFUSCATION)
    .event_handler(handler);
```
