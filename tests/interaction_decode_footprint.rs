//! Allocation gate and report for decoding INTERACTION_CREATE.
//!
//! The payload is the perf report's chat-input fixture
//! (`tests/fixtures/perf/interaction_create_chat_input.json`, 3.9 KB with
//! resolved users, members, channels, roles, and attachments). The
//! pre-phase-4 baseline (docs/design/perf-baseline.md, main 706b7ae) spent
//! 652 allocations in `decode_event` for it, mostly because the parser
//! cloned `data` and then `resolved` out of a `Value` tree.
//!
//! The test fails when the direct decoder (`decode_interaction` on the `d`
//! text) needs more than a third of that baseline. It also prints the other
//! paths and the model sizes with `--nocapture`:
//!
//! ```text
//! cargo test --release --all-features --locked --test interaction_decode_footprint -- --nocapture
//! ```
//!
//! One test function only: the counting allocator is process-wide, so a
//! second test running on another thread would add its allocations.
//!
//! ```text
//! [Decision Log]
//! - 목적과 의도: 4단계 인터랙션 직접 디코딩의 할당 목표(기준선 652의 1/3 이하)를 CI에서
//!   회귀 테스트로 고정하고, 경로별 수치를 같은 입력으로 비교한다.
//! - 기존 구현 및 제약 조건: perf 보고서(tests/perf_report)는 main에 아직 없고 release
//!   빌드로 전체 파이프라인을 재현한다. id_footprint.rs는 보고 전용(ignored)이다.
//! - 검토한 주요 대안: (1) 보고 전용 ignored 테스트, (2) 할당 수 상한을 단언하는 일반 테스트.
//! - 선택한 방식: (2). 할당 수는 같은 Cargo.lock에서 결정적이고, 상한을 목표 자체(217)로
//!   두어 serde/hashbrown 갱신의 작은 흔들림에는 깨지지 않는다.
//! - 장점, 단점 및 영향: 디코더가 Value 트리나 통째 복제로 돌아가면 바로 실패한다. 테스트
//!   바이너리 하나가 전역 할당자를 갖는다(다른 테스트와 격리).
//! ```

#![allow(
    unsafe_code,
    reason = "the counting GlobalAlloc must implement an unsafe trait; it only forwards to System"
)]
#![allow(clippy::unwrap_used, reason = "a failed expectation fails the test")]

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::mem::size_of;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use discordrs::model::{
    ChatInputCommandInteraction, CommandInteractionData, Interaction, InteractionContextData,
    ModalSubmitInteraction,
};
use discordrs::{decode_interaction, parse_interaction};
use serde_json::Value;

struct CountingAllocator;

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);

// SAFETY: every method forwards the caller's arguments unchanged to the
// system allocator, which upholds the GlobalAlloc contract; the counter is a
// plain atomic and never touches the memory being managed.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: same layout the caller passed to us.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: ptr was returned by System.alloc/realloc with this layout.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: forwarded unchanged from the caller.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

const FIXTURE: &str = include_str!("fixtures/perf/interaction_create_chat_input.json");

/// Pre-phase-4 `decode_event` allocations for [`FIXTURE`]
/// (docs/design/perf-baseline.md, `inbound.interaction_create.chat_input`).
const BASELINE_DECODE_EVENT_ALLOCS: usize = 652;

/// Allocation calls `f` makes; its result is dropped afterwards.
fn allocations<T>(f: impl FnOnce() -> T) -> usize {
    let before = ALLOCATIONS.load(Ordering::SeqCst);
    let value = f();
    let calls = ALLOCATIONS.load(Ordering::SeqCst) - before;
    drop(black_box(value));
    calls
}

fn micros_per_iter(iterations: u32, mut f: impl FnMut()) -> f64 {
    for _ in 0..iterations / 10 {
        f();
    }
    let start = Instant::now();
    for _ in 0..iterations {
        f();
    }
    start.elapsed().as_secs_f64() * 1e6 / f64::from(iterations)
}

#[test]
fn interaction_create_decode_allocations() {
    let value: Value = serde_json::from_str(FIXTURE).unwrap();

    // `decode_event` is not measured here: its signature changes with the
    // phase 4 envelope, and the perf report covers it.
    let parse_value_allocs = allocations(|| parse_interaction(&value).unwrap());
    let direct_allocs = allocations(|| decode_interaction(FIXTURE).unwrap());
    // Same payload with `data` before `type`: the decoder keeps `data` as
    // text and decodes it once the object ends.
    let data_first = data_first_payload(&value);
    let data_first_allocs = allocations(|| decode_interaction(&data_first).unwrap());

    println!("INTERACTION_CREATE chat input, {} bytes", FIXTURE.len());
    println!("  baseline decode_event (main 706b7ae): {BASELINE_DECODE_EVENT_ALLOCS} allocs");
    println!("  parse_interaction(&Value):            {parse_value_allocs} allocs");
    println!("  decode_interaction(&str):             {direct_allocs} allocs");
    println!("  decode_interaction(&str), data first: {data_first_allocs} allocs");

    let iterations = 2_000;
    let parse_value_us = micros_per_iter(iterations, || {
        black_box(parse_interaction(black_box(&value)).unwrap());
    });
    println!("  parse_interaction(&Value):            {parse_value_us:.1} us/iter");
    let direct_us = micros_per_iter(iterations, || {
        black_box(decode_interaction(black_box(FIXTURE)).unwrap());
    });
    println!("  decode_interaction(&str):             {direct_us:.1} us/iter");
    let data_first_us = micros_per_iter(iterations, || {
        black_box(decode_interaction(black_box(&data_first)).unwrap());
    });
    println!("  decode_interaction(&str), data first: {data_first_us:.1} us/iter");

    println!("sizes (64-bit):");
    println!(
        "  Interaction                  {} B",
        size_of::<Interaction>()
    );
    println!(
        "  InteractionContextData       {} B",
        size_of::<InteractionContextData>()
    );
    println!(
        "  ChatInputCommandInteraction  {} B",
        size_of::<ChatInputCommandInteraction>()
    );
    println!(
        "  CommandInteractionData       {} B",
        size_of::<CommandInteractionData>()
    );
    println!(
        "  ModalSubmitInteraction       {} B",
        size_of::<ModalSubmitInteraction>()
    );

    // The phase 4 gate: at least 3x fewer allocations than the baseline.
    assert!(
        direct_allocs * 3 <= BASELINE_DECODE_EVENT_ALLOCS,
        "decode_interaction made {direct_allocs} allocations; the gate is {}",
        BASELINE_DECODE_EVENT_ALLOCS / 3
    );
    assert!(
        data_first_allocs * 3 <= BASELINE_DECODE_EVENT_ALLOCS,
        "decode_interaction with data first made {data_first_allocs} allocations"
    );
}

/// The fixture re-encoded with `data` as the first key (a `Value` sorts
/// its keys, which also puts `data` before `type`).
fn data_first_payload(value: &Value) -> String {
    let object = value.as_object().unwrap();
    let mut text = format!("{{\"data\":{}", object["data"]);
    for (key, field) in object.iter().filter(|(key, _)| *key != "data") {
        text.push_str(&format!(",{}:{field}", Value::String(key.clone())));
    }
    text.push('}');
    text
}
