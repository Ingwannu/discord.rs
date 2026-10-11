//! Memory and decode-cost report for ID-heavy gateway payloads.
//!
//! Prints `size_of` for hot models and, for a representative MESSAGE_CREATE
//! and GUILD_CREATE, how many heap allocations decoding performs, how many
//! bytes the decoded value keeps alive, and a rough time per decode. It is a
//! measurement tool, not a pass/fail check, so the report test is ignored by
//! default:
//!
//! ```text
//! cargo test --release --all-features --locked --test id_footprint -- --ignored --nocapture --test-threads=1
//! ```
//!
//! The payloads only use model fields that exist before and after the
//! phase 3 ID migration, so the same file measures both sides. The numbers
//! of the migration are recorded in docs/design/3.0-types-events-requests.md
//! §1.
//!
//! ```text
//! [Decision Log]
//! - 목적과 의도: String 기반 Snowflake를 숫자 Id로 바꾼 효과(모델 크기, 디코딩 할당 수,
//!   디코딩 결과가 붙잡는 힙 바이트, 디코딩 시간)를 같은 입력으로 전후 비교한다.
//! - 기존 구현 및 제약 조건: 벤치마크 의존성(criterion 등)이 없고, 공개 API에 측정용 항목을
//!   두지 않는다. 할당 수는 전역 할당자로만 셀 수 있다.
//! - 검토한 주요 대안: (1) criterion 벤치 추가, (2) 라이브러리 안의 cfg(test) 측정,
//!   (3) 통합 테스트 바이너리 하나에 계수 전역 할당자.
//! - 선택한 방식: (3). 통합 테스트는 바이너리마다 전역 할당자를 따로 가지므로 다른 테스트에
//!   영향이 없다.
//! - 다른 대안 대신 이 방식을 선택한 이유: (1)은 dev-dependency와 MSRV 부담이 늘고, (2)는
//!   라이브러리 테스트 전체가 계수 할당자를 거치게 된다. 할당 수와 보유 바이트는 결정적이라
//!   시간 측정보다 비교 가치가 크다.
//! - 장점, 단점 및 영향: GlobalAlloc 구현에 unsafe가 필요하다(시스템 할당자로 그대로 위임,
//!   라이브러리 코드 아님). 시간 값은 기계와 부하에 따라 흔들리므로 참고용이다.
//! ```

#![allow(
    unsafe_code,
    reason = "the counting GlobalAlloc must implement an unsafe trait; it only forwards to System"
)]
#![allow(clippy::unwrap_used, reason = "a failed expectation fails the report")]

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::mem::size_of;
use std::sync::atomic::{AtomicIsize, AtomicUsize, Ordering};
use std::time::Instant;

use serde_json::{json, Value};

struct CountingAllocator;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static LIVE_BYTES: AtomicIsize = AtomicIsize::new(0);

// SAFETY: every method forwards the caller's arguments unchanged to the
// system allocator, which upholds the GlobalAlloc contract; the counters are
// plain atomics and never touch the memory being managed.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        LIVE_BYTES.fetch_add(
            isize::try_from(layout.size()).unwrap_or(0),
            Ordering::Relaxed,
        );
        // SAFETY: same layout the caller passed to us.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE_BYTES.fetch_sub(
            isize::try_from(layout.size()).unwrap_or(0),
            Ordering::Relaxed,
        );
        // SAFETY: ptr was returned by System.alloc/realloc with this layout.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        let old = isize::try_from(layout.size()).unwrap_or(0);
        let new = isize::try_from(new_size).unwrap_or(0);
        LIVE_BYTES.fetch_add(new - old, Ordering::Relaxed);
        // SAFETY: forwarded unchanged from the caller.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

/// Allocation calls made and bytes still held after `f` returns, with the
/// returned value kept alive until both are read.
fn measure<T>(f: impl FnOnce() -> T) -> (T, usize, isize) {
    let calls_before = ALLOCATIONS.load(Ordering::SeqCst);
    let live_before = LIVE_BYTES.load(Ordering::SeqCst);
    let value = f();
    let calls = ALLOCATIONS.load(Ordering::SeqCst) - calls_before;
    let live = LIVE_BYTES.load(Ordering::SeqCst) - live_before;
    (value, calls, live)
}

fn time_per_iter(iterations: u32, mut f: impl FnMut()) -> f64 {
    for _ in 0..iterations / 10 {
        f();
    }
    let start = Instant::now();
    for _ in 0..iterations {
        f();
    }
    start.elapsed().as_secs_f64() * 1e6 / f64::from(iterations)
}

fn snowflake(n: u64) -> String {
    // Realistic 18-19 digit snowflakes (2024-era timestamps).
    (1_200_000_000_000_000_000_u64 + n * 4_194_304 + n).to_string()
}

fn user(n: u64) -> Value {
    json!({
        "id": snowflake(n),
        "username": format!("user{n}"),
        "discriminator": "0",
        "global_name": format!("User {n}"),
        "avatar": "a_0123456789abcdef0123456789abcdef",
        "bot": false
    })
}

fn member(n: u64, roles: &[u64]) -> Value {
    json!({
        "user": user(n),
        "nick": null,
        "roles": roles.iter().map(|r| snowflake(*r)).collect::<Vec<_>>(),
        "joined_at": "2024-01-01T00:00:00.000000+00:00",
        "deaf": false,
        "mute": false,
        "flags": 0
    })
}

fn message_create() -> Value {
    let guild = 1;
    json!({
        "id": snowflake(9_000),
        "channel_id": snowflake(100),
        "guild_id": snowflake(guild),
        "author": user(500),
        "member": {
            "roles": [snowflake(10), snowflake(11), snowflake(12)],
            "joined_at": "2024-01-01T00:00:00.000000+00:00",
            "deaf": false,
            "mute": false,
            "flags": 0
        },
        "content": "hello <@1200000000004194805> and <@&1200000000041943050>",
        "timestamp": "2024-06-01T12:00:00.000000+00:00",
        "edited_timestamp": null,
        "tts": false,
        "mention_everyone": false,
        "mentions": [user(501), user(502)],
        "mention_roles": [snowflake(10), snowflake(11)],
        "attachments": [{
            "id": snowflake(7_000),
            "filename": "image.png",
            "size": 12_345,
            "url": "https://cdn.discordapp.com/attachments/1/2/image.png",
            "proxy_url": "https://media.discordapp.net/attachments/1/2/image.png",
            "content_type": "image/png",
            "width": 640,
            "height": 480
        }],
        "embeds": [{
            "type": "rich",
            "title": "Embed title",
            "description": "Embed description",
            "color": 5_814_783,
            "fields": [{"name": "a", "value": "b", "inline": true}]
        }],
        "reactions": [{
            "count": 2,
            "me": false,
            "emoji": {"id": snowflake(8_000), "name": "custom"}
        }],
        "pinned": false,
        "type": 19,
        "flags": 0,
        "message_reference": {
            "type": 0,
            "message_id": snowflake(8_999),
            "channel_id": snowflake(100),
            "guild_id": snowflake(guild)
        },
        "components": [{
            "type": 1,
            "components": [{
                "type": 2,
                "style": 1,
                "label": "Click",
                "custom_id": "click"
            }]
        }],
        "sticker_items": [{"id": snowflake(6_000), "name": "sticker", "format_type": 1}]
    })
}

fn guild_create() -> Value {
    let guild = 1;
    let roles: Vec<Value> = (0..25)
        .map(|i| {
            json!({
                "id": snowflake(if i == 0 { guild } else { 10 + i }),
                "name": format!("role {i}"),
                "color": 0,
                "hoist": false,
                "position": i,
                "permissions": "1071698660929",
                "managed": false,
                "mentionable": false,
                "flags": 0
            })
        })
        .collect();
    let channels: Vec<Value> = (0..40)
        .map(|i| {
            json!({
                "id": snowflake(100 + i),
                "type": if i % 10 == 0 { 4 } else if i % 7 == 0 { 2 } else { 0 },
                "guild_id": snowflake(guild),
                "name": format!("channel-{i}"),
                "position": i,
                "parent_id": if i % 10 == 0 { Value::Null } else { Value::String(snowflake(100 + i / 10 * 10)) },
                "permission_overwrites": [
                    {"id": snowflake(guild), "type": 0, "allow": "0", "deny": "1024"},
                    {"id": snowflake(11), "type": 0, "allow": "1024", "deny": "0"}
                ],
                "nsfw": false,
                "last_message_id": snowflake(9_000 + i)
            })
        })
        .collect();
    let members: Vec<Value> = (0..100)
        .map(|i| member(500 + i, &[10 + i % 5, 20 + i % 3]))
        .collect();
    let presences: Vec<Value> = (0..100)
        .map(|i| {
            json!({
                "user": {"id": snowflake(500 + i)},
                "guild_id": snowflake(guild),
                "status": "online",
                "activities": [],
                "client_status": {"desktop": "online"}
            })
        })
        .collect();
    let emojis: Vec<Value> = (0..15)
        .map(|i| {
            json!({
                "id": snowflake(8_000 + i),
                "name": format!("emoji{i}"),
                "roles": [],
                "require_colons": true,
                "managed": false,
                "animated": false,
                "available": true
            })
        })
        .collect();
    let voice_states: Vec<Value> = (0..5)
        .map(|i| {
            json!({
                "channel_id": snowflake(107),
                "user_id": snowflake(500 + i),
                "session_id": format!("session{i}"),
                "deaf": false,
                "mute": false,
                "self_deaf": false,
                "self_mute": false,
                "self_video": false,
                "suppress": false,
                "request_to_speak_timestamp": null
            })
        })
        .collect();
    json!({
        "id": snowflake(guild),
        "name": "measurement guild",
        "icon": null,
        "owner_id": snowflake(500),
        "afk_timeout": 300,
        "verification_level": 1,
        "default_message_notifications": 1,
        "explicit_content_filter": 2,
        "features": ["COMMUNITY", "NEWS"],
        "mfa_level": 0,
        "system_channel_id": snowflake(101),
        "rules_channel_id": snowflake(102),
        "public_updates_channel_id": snowflake(103),
        "preferred_locale": "en-US",
        "premium_tier": 1,
        "nsfw_level": 0,
        "joined_at": "2024-01-01T00:00:00.000000+00:00",
        "large": false,
        "unavailable": false,
        "member_count": 100,
        "roles": roles,
        "channels": channels,
        "threads": [],
        "members": members,
        "presences": presences,
        "emojis": emojis,
        "stickers": [],
        "voice_states": voice_states
    })
}

fn report_decode(name: &str, event: &str, payload: &Value) {
    let text = payload.to_string();

    // Gateway path: the payload text decoded straight into the typed event
    // (`decode_event` also keeps a copy of the raw text).
    let (decoded, calls, live) = measure(|| discordrs::decode_event(event, &text).unwrap());
    drop(decoded);
    let per_decode = time_per_iter(2_000, || {
        black_box(discordrs::decode_event(event, black_box(&text)).unwrap());
    });

    println!(
        "{name}: payload {} bytes | decode_event: {} allocations, {} live bytes (includes the \
         kept raw text) | {:.1} us/decode",
        text.len(),
        calls,
        live,
        per_decode,
    );
}

fn report_typed<T: serde::de::DeserializeOwned>(name: &str, payload: &Value) {
    let text = payload.to_string();
    let (decoded, calls, live) = measure(|| serde_json::from_str::<T>(&text).unwrap());
    drop(decoded);
    let per_decode = time_per_iter(2_000, || {
        black_box(serde_json::from_str::<T>(black_box(&text)).unwrap());
    });
    println!(
        "{name}: from_str typed: {calls} allocations, {live} live bytes retained | {per_decode:.1} us/decode"
    );
}

#[test]
#[ignore = "measurement report; run with --ignored --nocapture (see module docs)"]
fn id_footprint_report() {
    use discordrs::model::{Channel, Guild, Member, Message, User};

    println!("size_of Message = {}", size_of::<Message>());
    println!("size_of Member  = {}", size_of::<Member>());
    println!("size_of Channel = {}", size_of::<Channel>());
    println!("size_of User    = {}", size_of::<User>());
    println!("size_of Guild   = {}", size_of::<Guild>());
    println!("size_of Event   = {}", size_of::<discordrs::Event>());

    let message = message_create();
    let guild = guild_create();
    report_typed::<Message>("MESSAGE_CREATE", &message);
    report_typed::<Guild>("GUILD_CREATE", &guild);
    report_decode("MESSAGE_CREATE", "MESSAGE_CREATE", &message);
    report_decode("GUILD_CREATE", "GUILD_CREATE", &guild);
}

/// The fixtures must decode, so a payload typo cannot silently turn the
/// report into a measurement of an error path.
#[test]
fn footprint_payloads_decode() {
    let message: discordrs::model::Message = serde_json::from_value(message_create()).unwrap();
    assert_eq!(message.mentions.len(), 2);
    let guild: discordrs::model::Guild = serde_json::from_value(guild_create()).unwrap();
    assert_eq!(guild.roles.len(), 25);
    assert_eq!(guild.channels.len(), 40);
    assert_eq!(guild.members.len(), 100);
}
