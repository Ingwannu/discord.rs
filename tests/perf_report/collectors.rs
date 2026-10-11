//! Collector fan-out on the real `CollectorHub`, in the end-to-end process:
//! the per-frame cost of MESSAGE_CREATE with K message collectors of which
//! exactly one matches, and how many payload copies the extra collectors add.
//!
//! The hub can only be fed by a running `Client` (`CollectorHub::publish` is
//! crate-private), so this runs in the e2e child. The copy count is derived
//! from allocation counts, which are exact there even though the timings
//! are not:
//!
//! - one copy = what a single matching collector costs per event,
//!   `allocs(k=1) − allocs(k=0)`, whatever the event and payload types are;
//! - `copies(k) = (allocs(k) − allocs(k=0)) / copy`, so `k64_minus_k1` is the
//!   number of extra copies 63 non-matching collectors cause. Today every
//!   subscriber clones the whole event, so it is 63; the phase 4 gate is 0
//!   (reference filtering, clone only on a match).

use std::hint::black_box;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use discordrs::{CollectorHub, MessageId};

use crate::alloc::{Mark, Uncounted};
use crate::e2e::{e2e_scenario, lockstep, wait_until, ChildGateway, ProbeHandler, E2E_WAIT};
use crate::fixtures::{fixture, MESSAGE_CREATE_PLAIN};
use crate::inbound::HUB_JOURNAL_CAPACITY;
use crate::report::{Report, Scenario};

const FANOUT_FRAMES: usize = 200;
/// Collector counts measured; 0 is the copy-free reference.
const COLLECTOR_COUNTS: [usize; 4] = [0, 1, 8, 64];

/// Per-frame allocations and bytes for one collector count.
struct PerFrame {
    collectors: usize,
    allocs: f64,
    bytes: f64,
}

pub(crate) async fn run_collectors(
    gateway: &mut ChildGateway,
    handler: &ProbeHandler,
    hub: &CollectorHub,
    report: &mut Report,
) {
    // Read from the fixture rather than a decoded event, so this module does
    // not depend on the shape of `Event`.
    let target: MessageId = fixture(MESSAGE_CREATE_PLAIN)["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let other = MessageId::try_from(1_u64).unwrap();
    let mut per_frame = Vec::with_capacity(COLLECTOR_COUNTS.len());

    for k in COLLECTOR_COUNTS {
        let matched = Arc::new(AtomicUsize::new(0));
        let matched_notify = Arc::new(tokio::sync::Notify::new());
        let mut tasks = Vec::with_capacity(k);
        for i in 0..k {
            // Collector 0 waits for the frame's message; the others for
            // another one, so they read every event and collect nothing.
            let wanted = if i == 0 { target } else { other };
            let mut collector = hub
                .message_collector()
                .filter(move |message| message.id == wanted);
            let matched = Arc::clone(&matched);
            let matched_notify = Arc::clone(&matched_notify);
            tasks.push(tokio::spawn(async move {
                while let Some(message) = collector.next().await {
                    drop(black_box(message));
                    matched.fetch_add(1, Ordering::SeqCst);
                    matched_notify.notify_one();
                }
            }));
        }
        // Turn the hub journal over to this frame's events first: until all
        // 256 slots hold them, every publish evicts a smaller event from the
        // previous scenario and the counted pass would show that turnover as
        // retained heap (1.9 MB once, when this warm-up was 20 frames).
        let warm = vec!["message_create.plain".to_string(); HUB_JOURNAL_CAPACITY + 20];
        lockstep(gateway, handler, &warm).await;
        if k > 0 {
            wait_until("warm-up matches", || {
                matched.load(Ordering::SeqCst) >= warm.len()
            })
            .await;
        }
        // Let the non-matching collectors drain the warm-up.
        tokio::time::sleep(Duration::from_millis(50)).await;

        let pass = async |gateway: &mut ChildGateway| {
            let mut timings = Vec::with_capacity(FANOUT_FRAMES);
            let mark = Mark::start();
            for _ in 0..FANOUT_FRAMES {
                let target_seen = handler.seen.load(Ordering::SeqCst) + 1;
                let target_matched = matched.load(Ordering::SeqCst) + 1;
                let start = Instant::now();
                gateway.send("message_create.plain");
                handler.wait_seen(target_seen).await;
                if k > 0 {
                    let collected = async {
                        while matched.load(Ordering::SeqCst) < target_matched {
                            matched_notify.notified().await;
                        }
                    };
                    tokio::time::timeout(E2E_WAIT, collected)
                        .await
                        .expect("the matching collector did not yield the message");
                }
                timings.push(start.elapsed());
            }
            // Non-matching collectors may still be filtering the last frames.
            tokio::time::sleep(Duration::from_millis(50)).await;
            (mark.finish(), timings)
        };
        let (sample, _) = pass(gateway).await;
        let timings = {
            let _uncounted = Uncounted::new();
            pass(gateway).await.1
        };
        for task in tasks {
            task.abort();
        }
        let frames = FANOUT_FRAMES as f64;
        per_frame.push(PerFrame {
            collectors: k,
            allocs: sample.allocs as f64 / frames,
            bytes: sample.alloc_bytes as f64 / frames,
        });
        report.push(
            e2e_scenario(
                &format!("e2e.fanout.message_collectors.k{k}"),
                sample,
                FANOUT_FRAMES,
                timings,
            )
            .set("collectors", k)
            .note("MESSAGE_CREATE through the real pipeline with K hub collectors (one matches when K > 0); time: until the handler and the matching collector have it"),
        );
    }

    report.push(payload_copies(&per_frame));
}

/// The `collectors.payload_copies` scenario from the per-frame counts.
fn payload_copies(per_frame: &[PerFrame]) -> Scenario {
    let at = |k: usize| {
        per_frame
            .iter()
            .find(|row| row.collectors == k)
            .map(|row| (row.allocs, row.bytes))
            .unwrap()
    };
    let (allocs_0, bytes_0) = at(0);
    let (allocs_1, bytes_1) = at(1);
    let copy_allocs = allocs_1 - allocs_0;
    let copy_bytes = bytes_1 - bytes_0;
    // When a matching collector copies nothing that allocates, any extra
    // allocation still counts: report it as copies rather than divide by 0.
    let copies = |extra: f64, unit: f64| if unit >= 0.5 { extra / unit } else { extra };
    let round = |value: f64| (value * 100.0).round() / 100.0;
    let extra_over_k1 = |k: usize| {
        let (allocs, _) = at(k);
        round(copies(allocs - allocs_0, copy_allocs) - copies(copy_allocs, copy_allocs))
    };
    let (allocs_64, bytes_64) = at(64);
    let (allocs_8, _) = at(8);
    Scenario::new("collectors.payload_copies", "collectors")
        .set("copy_allocs", round(copy_allocs))
        .set("copy_bytes", round(copy_bytes))
        .set("copies_k8", round(copies(allocs_8 - allocs_0, copy_allocs)))
        .set("copies_k64", round(copies(allocs_64 - allocs_0, copy_allocs)))
        .set("k8_minus_k1", extra_over_k1(8))
        .set("k64_minus_k1", extra_over_k1(64))
        .set(
            "k64_minus_k1_by_bytes",
            round(copies(bytes_64 - bytes_0, copy_bytes) - copies(copy_bytes, copy_bytes)),
        )
        .deterministic(&["k8_minus_k1", "k64_minus_k1", "k64_minus_k1_by_bytes"])
        .note("one copy = allocs(k=1) - allocs(k=0) per MESSAGE_CREATE; k64_minus_k1 = extra copies caused by 63 non-matching collectors (gate: 0)")
}
