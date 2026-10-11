//! Performance baseline report: heap allocations, allocated bytes, retained
//! and peak live heap, and time for the gateway inbound path, the in-memory
//! cache, collector fan-out, and outbound request bodies, plus the RSS of a
//! bot process around a GUILD_CREATE storm.
//!
//! A measurement tool, not a pass/fail check, so the report test is ignored
//! by default. Run it in release; `--test-threads=1` keeps the libtest
//! harness from running the companion tests beside it (they also take a
//! lock, since all of them share the global counters):
//!
//! ```text
//! cargo test --release --all-features --locked --test perf_report -- --ignored --nocapture --test-threads=1
//! ```
//!
//! Every scenario prints one `perf-report: {json}` line, and the whole run
//! is written to `target/perf-report.json` (or to `$DISCORDRS_PERF_REPORT`).
//! `python3 ci/perf_diff.py before.json after.json` prints the change per
//! metric. Scenario definitions, metric semantics, and the recorded baseline
//! are in docs/design/perf-baseline.md.
//!
//! The inbound path is measured two ways:
//!
//! - `inbound.*` (`inbound.rs`) replays, step by step, what the gateway read
//!   loop (`src/gateway/client.rs`) and the event processor
//!   (`process_gateway_dispatch` in `src/gateway/bot/pipeline.rs`,
//!   `apply_cache_updates` in `src/gateway/bot.rs`) do with one frame. Those
//!   steps are private, so the replica rebuilds them from public API
//!   (`serde_json`, `decode_event`, `CacheHandle`, `Event::clone`); every
//!   step names the code it mirrors and must follow it when that code
//!   changes. Its allocation counts and bytes are deterministic.
//! - `e2e.*` (`e2e.rs`) drives the real `Client` in a separate bot process
//!   against a fake gateway in a third process (both this test binary,
//!   re-executed), so the counts and the RSS belong to the bot alone. It
//!   covers what the replica cannot see: websocket reads, the ingress and
//!   handler queues, task spawns, and the `Context` built for every handler
//!   call. The two agree to within the 3 allocations per event the runtime
//!   adds, which is how the replica is checked against the private code.
//!
//! Allocations are counted by a global allocator (`alloc.rs`). Counting
//! costs about 65 ns per allocation, so timed iterations run with it off
//! (`Uncounted`) after the counted passes.
//!
//! ```text
//! [Decision Log]
//! - 목적과 의도: 4단계(얕은 봉투, RawValue에서 직접 타입 디코딩, Arc<Event> 공유, 패치 인식
//!   캐시)의 효과를 같은 입력으로 전후 비교하고, 이후 회귀를 잡을 기준선을 남긴다. 사용자 목표인
//!   낮은 RAM(보유·최대 힙, RSS)과 높은 속도(할당 수, 시간)를 함께 잰다.
//! - 기존 구현 및 제약 조건: tests/id_footprint.rs가 통합 테스트 바이너리의 계수 전역 할당자로
//!   디코딩만 쟀다. 벤치 의존성이 없고 MSRV 1.88, Cargo [lints] 규칙을 따른다. 게이트웨이 단계
//!   (프레임 파싱, process_gateway_dispatch, apply_cache_updates, CollectorHub::publish)는
//!   비공개이고, 측정만을 위해 공개하지 않는다. 다른 작업자가 src/를 동시에 고친다.
//! - 검토한 주요 대안: (1) criterion/divan 벤치, (2) 라이브러리 안의 #[doc(hidden)] 측정 훅,
//!   (3) 공개 API로 단계를 그대로 재현(replica), (4) 실제 Client를 가짜 게이트웨이에 연결(e2e)하되
//!   봇과 게이트웨이를 같은 프로세스에 두거나 각각 자식 프로세스로 분리.
//! - 선택한 방식: (3)과 (4)를 함께 쓰고, e2e 봇과 가짜 게이트웨이는 각각 별도 프로세스로
//!   띄운다. 계수 할당자에 할당 바이트와 최대 live 바이트, 시간 측정용 끄기 스위치를 더했다.
//!   결과는 JSON으로 남기고 ci/perf_diff.py(표준 라이브러리만)로 비교한다.
//! - 다른 대안 대신 이 방식을 선택한 이유: (1)은 dev-dependency와 MSRV 부담이 늘고 할당을 재지
//!   못한다. (2)는 공개 표면을 늘리고 src/에서 다른 작업과 충돌한다. (3)만으로는 큐·태스크·
//!   웹소켓 비용이 빠지고 비공개 코드와 어긋나도 모르므로 (4)로 교차 확인한다. 같은 프로세스의
//!   서버는 서버 쪽 할당과 프레임 사본이 할당 수와 RSS에 섞이고, 같은 프로세스의 e2e 폭주
//!   (수백 MB 해제)는 glibc 힙을 조각내 뒤이은 replica 시간을 몇 배로 늘렸다.
//! - 장점, 단점 및 영향: replica의 할당 수·바이트·보유량은 결정적이라 작은 회귀도 보인다. 대신
//!   비공개 단계가 바뀌면 replica를 손으로 맞춰야 한다(단계마다 원본 위치를 적었다). e2e 수치는
//!   스케줄링에 따라 조금 흔들리고, 시간과 RSS는 기계와 부하에 따라 흔들린다. 자식 프로세스
//!   기동과 폭주 시나리오 때문에 한 번 실행에 약 20초가 든다. unsafe는 GlobalAlloc 구현에만
//!   있고 시스템 할당자로 그대로 넘긴다.
//! ```

#![cfg(all(feature = "gateway", feature = "cache", feature = "collectors"))]
#![allow(
    unsafe_code,
    reason = "the counting GlobalAlloc must implement an unsafe trait; it only forwards to System"
)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation fails the report"
)]
#![allow(
    clippy::as_conversions,
    reason = "report arithmetic: per-operation averages divide counters as f64"
)]

mod alloc;
mod cache;
mod collectors;
mod e2e;
mod fanout;
mod fixtures;
mod inbound;
mod outbound;
mod report;
mod storm;

use std::sync::Mutex;
use std::time::Instant;

use discordrs::model::{Guild, Message, Presence};
use discordrs::{decode_event, CollectorHub, Context, Event, MessageBuilder};
use serde_json::{json, Value};

use crate::alloc::Mark;
use crate::cache::run_cache;
use crate::e2e::run_e2e_in_child;
use crate::fanout::run_fanout;
use crate::fixtures::{
    fixture, inbound_cases, synthesized_guild_create, InboundCase, FLOOR_EVENT, LARGE_GUILD,
    MESSAGE_CREATE_COMPONENTS_V2, MESSAGE_CREATE_PLAIN, UNKNOWN_EVENT_NAME,
};
use crate::inbound::{run_inbound, run_typed_reference};
use crate::outbound::{run_outbound, status_card};
use crate::report::{current_thread_runtime, meta, Report, Scenario};
use crate::storm::run_storm;

/// Serializes the tests of this binary: they share the global counters.
static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[test]
#[ignore = "measurement report; run with --release --ignored --nocapture (see module docs)"]
fn perf_report() {
    let _serial = serial();
    let started = Instant::now();
    let mut report = Report::default();

    // Heap of an empty collector hub, whatever its journal stores: today
    // 256 inline `Event`s of reserved ring, after phase 4 B 256 pointers.
    // Measured instead of computed so it needs no knowledge of the private
    // element type.
    let mark = Mark::start();
    let hub = CollectorHub::new();
    let empty_hub = mark.finish();
    drop(hub);

    // Inline sizes: queue slots, journal entries, and handler futures hold
    // these by value. When phase 4 adds `EventKind`, report
    // `size_of::<EventKind>()` here as `event_kind` (gated in
    // ci/perf-gates.json).
    report.push(
        Scenario::new("layout.size_of", "layout")
            .set("event", std::mem::size_of::<Event>())
            .set("message", std::mem::size_of::<Message>())
            .set("guild", std::mem::size_of::<Guild>())
            .set("member", std::mem::size_of::<discordrs::model::Member>())
            .set("user", std::mem::size_of::<discordrs::model::User>())
            .set("channel", std::mem::size_of::<discordrs::model::Channel>())
            .set("presence", std::mem::size_of::<Presence>())
            .set("context", std::mem::size_of::<Context>())
            .set("value", std::mem::size_of::<Value>())
            .set("empty_collector_hub_bytes", empty_hub.retained_bytes)
            .deterministic(&[
                "event",
                "message",
                "guild",
                "member",
                "user",
                "channel",
                "presence",
                "context",
                "value",
                "empty_collector_hub_bytes",
            ]),
    );

    // End to end in a separate process (see `run_e2e`), before the replica
    // scenarios so the two never compete for CPU.
    run_e2e_in_child(&mut report);

    // Small scenarios first: the large ones free tens of MB of small
    // chunks, and glibc's fragmented bins then slow every later malloc.
    let rt = current_thread_runtime();
    let (large, small): (Vec<InboundCase>, Vec<InboundCase>) = inbound_cases()
        .into_iter()
        .partition(|case| case.event == "GUILD_CREATE");
    for case in &small {
        run_inbound(&rt, case, &mut report);
    }
    run_typed_reference::<Message>(
        "message_create.plain",
        &fixture(MESSAGE_CREATE_PLAIN),
        2_000,
        &mut report,
    );
    run_typed_reference::<Message>(
        "message_create.components_v2",
        &fixture(MESSAGE_CREATE_COMPONENTS_V2),
        2_000,
        &mut report,
    );
    run_fanout(&mut report);
    run_outbound(&mut report);
    for case in &large {
        run_inbound(&rt, case, &mut report);
    }
    run_typed_reference::<Guild>(
        "guild_create.large",
        &synthesized_guild_create(0, &LARGE_GUILD),
        15,
        &mut report,
    );
    run_cache(&rt, &mut report);
    // Last: at about 400 MB live it leaves the largest heap behind.
    run_storm(&rt, &mut report);

    let path = report.write(meta());
    println!(
        "perf report written to {} in {:.1} s",
        path.display(),
        started.elapsed().as_secs_f64()
    );
}

/// The fixtures must decode completely, so a typo cannot quietly turn a
/// scenario into a measurement of an error or `.ok()` fallback path.
#[test]
fn perf_fixtures_decode() {
    let _serial = serial();
    for case in inbound_cases() {
        let event = decode_event(case.event, case.d.clone()).unwrap();
        match (&event, case.name) {
            (Event::MessageCreate(event), "message_create.plain") => {
                assert_eq!(event.message.mentions.len(), 1);
                assert!(event.message.member.is_some());
            }
            (Event::MessageCreate(event), "message_create.embeds") => {
                assert_eq!(event.message.embeds.len(), 2);
                assert_eq!(event.message.embeds[0].fields.len(), 6);
                assert_eq!(event.message.components.as_ref().map(Vec::len), Some(2));
                assert!(event.message.interaction_metadata.is_some());
            }
            (Event::MessageCreate(event), "message_create.components_v2") => {
                let components = event.message.components.as_ref().unwrap();
                assert_eq!(components.len(), 2);
                assert_eq!(components[0].components.len(), 6);
            }
            (Event::GuildCreate(event), "guild_create.large") => {
                let guild = &event.guild;
                assert_eq!(guild.members.len(), 1_000);
                assert_eq!(guild.channels.len(), 200);
                assert_eq!(guild.roles.len(), 300);
                assert_eq!(guild.presences.len(), 1_000);
                assert_eq!(guild.voice_states.len(), 50);
                assert_eq!(guild.emojis.len(), 30);
                assert!(guild.presences.iter().all(|presence| presence
                    .activities
                    .as_ref()
                    .is_some_and(|activities| !activities.is_empty())));
            }
            (Event::MessageUpdate(event), "message_update.partial") => {
                assert!(event.message.edited_timestamp.is_some());
                assert!(event.message.author.is_none());
            }
            (Event::PresenceUpdate(event), "presence_update") => {
                // decode_presence_update_event falls back to empty
                // activities on a decode error.
                assert_eq!(event.activities.len(), 2);
                assert!(event.client_status.is_some());
                assert!(event.user_id.is_some() && event.guild_id.is_some());
            }
            (Event::InteractionCreate(event), "interaction_create.chat_input") => {
                let discordrs::model::Interaction::ChatInputCommand(command) = &event.interaction
                else {
                    panic!("not a chat input command: {:?}", event.interaction);
                };
                let resolved = command.data.resolved.as_ref().unwrap();
                assert_eq!(resolved.users.len(), 1);
                assert_eq!(resolved.members.len(), 1);
                assert_eq!(resolved.channels.len(), 1);
                assert_eq!(resolved.roles.len(), 1);
                assert_eq!(resolved.attachments.len(), 1);
            }
            (Event::Unknown { kind, .. }, "unknown_dispatch") => {
                assert_eq!(kind, UNKNOWN_EVENT_NAME);
            }
            (Event::Unknown { kind, .. }, "floor") => assert_eq!(kind, FLOOR_EVENT),
            (event, name) => panic!("{name} decoded to {}", event.kind()),
        }
    }
    // A storm guild decodes like guild 0 and owns disjoint IDs.
    let storm: Guild = serde_json::from_value(synthesized_guild_create(1, &LARGE_GUILD)).unwrap();
    let first: Guild = serde_json::from_value(synthesized_guild_create(0, &LARGE_GUILD)).unwrap();
    assert_ne!(storm.id, first.id);
    assert_ne!(storm.channels[1].id, first.channels[1].id);
    // The outbound card serializes to the documented component types.
    let body =
        serde_json::to_value(MessageBuilder::new().components_v2(status_card()).build()).unwrap();
    assert_eq!(body["components"][0]["type"], json!(17));
    assert_eq!(
        body["components"][0]["components"].as_array().map(Vec::len),
        Some(6)
    );
}
