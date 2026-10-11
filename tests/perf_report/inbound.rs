//! Inbound replica: the gateway pipeline from frame text to the `Event` a
//! handler receives, step by step, plus the typed-decode reference.

use std::collections::VecDeque;
use std::hint::black_box;
use std::sync::Arc;
use std::time::{Duration, Instant};

use discordrs::model::Presence;
use discordrs::{CacheConfig, CacheHandle, Event, EventKind, EventMetadata};
use serde_json::Value;

use crate::alloc::{Mark, Recorder, Sample, Uncounted};
use crate::fixtures::{fixture, frame, InboundCase, MESSAGE_CREATE_PLAIN};
use crate::report::{Report, Scenario, DETERMINISTIC};

/// Collector hub journal capacity (`DEFAULT_HUB_CAPACITY` in
/// src/collector.rs): the hub keeps the last 256 published events.
pub(crate) const HUB_JOURNAL_CAPACITY: usize = 256;

/// The state the gateway pipeline keeps per shard: the cache and the
/// collector hub journal, which shares the handler's `Arc<Event>`.
pub(crate) struct Pipeline {
    cache: CacheHandle,
    journal: VecDeque<Arc<Event>>,
}

impl Pipeline {
    pub(crate) fn new() -> Self {
        Self {
            cache: CacheHandle::with_config(CacheConfig::default()),
            journal: VecDeque::with_capacity(HUB_JOURNAL_CAPACITY),
        }
    }

    /// One frame from websocket text to the `Arc<Event>` a handler receives.
    pub(crate) async fn dispatch(&mut self, frame: &str, rec: &mut Recorder) -> Arc<Event> {
        // gateway/compression.rs `decode_gateway_message`:
        // `WsMessage::Text(text) => text.to_string()`.
        let text = rec.step("frame_to_string", || frame.to_string());
        // gateway/client.rs read loop: `serde_json::from_str::<Value>(&text)`.
        let payload: Value = rec.step("parse_frame", || serde_json::from_str(&text).unwrap());
        // gateway/client.rs: `payload["s"]`, `payload["t"].as_str().to_string()`,
        // `payload["d"].clone()`, then `on_event(event_name, data)`.
        let (event_name, data) = rec.step("envelope", || {
            black_box(payload["op"].as_u64());
            black_box(payload["s"].as_u64());
            (
                payload["t"].as_str().unwrap_or("").to_string(),
                payload["d"].clone(),
            )
        });
        // The read loop drops the frame before the processor task decodes.
        rec.step("drop_frame", || {
            drop(payload);
            drop(text);
        });
        // gateway/bot/pipeline.rs `GatewayDispatch::new`: the payload as
        // raw text plus its metadata, queued for the processor.
        let (metadata, raw) = rec.step("queue_payload", || {
            (
                EventMetadata::new(&event_name, tokio::time::Instant::now()),
                serde_json::value::to_raw_value(&data).unwrap(),
            )
        });
        rec.step("drop_dispatch_value", || drop(data));
        // pipeline.rs `process_gateway_dispatch`: `Event::decode` with the
        // default `keep_raw_events(false)`.
        let event = rec.step("decode_event", || Event::decode(metadata, raw, false));
        // gateway/bot.rs `apply_cache_updates`.
        let mark = rec.begin();
        apply_cache_replica(&self.cache, &event).await;
        rec.end("cache_apply", mark);
        // pipeline.rs `Arc::new(event)` and `ctx.collectors.publish(Arc::clone(&event))`
        // into the hub journal (collector.rs `CollectorHub::publish`).
        rec.step("collector_publish", || {
            let event = Arc::new(event);
            self.journal.push_back(Arc::clone(&event));
            if self.journal.len() > HUB_JOURNAL_CAPACITY {
                self.journal.pop_front();
            }
            event
        })
    }
}

/// gateway/bot.rs `apply_cache_updates`, for the event kinds measured here.
async fn apply_cache_replica(cache: &CacheHandle, event: &Event) {
    match event.kind() {
        EventKind::GuildCreate(guild) => cache.apply_guild_create(guild).await,
        EventKind::MessageCreate(message) => {
            if let Some(author) = message.author.as_ref() {
                cache.upsert_user(author.clone()).await;
            }
            // The DM channel stub is skipped: the fixtures are guild messages.
            cache.upsert_message(Arc::clone(message)).await;
        }
        EventKind::MessageUpdate(update) => {
            if let Some(author) = update.author_user() {
                cache.upsert_user(author).await;
            }
            cache.apply_message_update(update).await;
        }
        EventKind::PresenceUpdate(event) => {
            if let (Some(guild_id), Some(user_id)) = (event.guild_id, event.user_id) {
                // `presence_from_update`.
                let presence = Presence {
                    user_id: Some(user_id),
                    user: None,
                    status: event.status.clone(),
                    activities: (!event.activities.is_empty()).then(|| event.activities.clone()),
                    client_status: event.client_status.clone(),
                };
                cache.upsert_presence(guild_id, user_id, presence).await;
            }
        }
        // Chat input interactions and unknown dispatches touch no cache state.
        _ => {}
    }
}

/// Warm-up dispatches before the measured steady-state iteration: enough to
/// fill the hub journal, so every later publish also evicts one event.
const WARM_DISPATCHES: usize = HUB_JOURNAL_CAPACITY + 4;

pub(crate) fn run_inbound(rt: &tokio::runtime::Runtime, case: &InboundCase, report: &mut Report) {
    let text = frame(case.event, 42, &case.d);
    let original = frame("MESSAGE_CREATE", 41, &fixture(MESSAGE_CREATE_PLAIN));
    let fresh = || async {
        let mut pipeline = Pipeline::new();
        if case.event == "MESSAGE_UPDATE" {
            // The update merges into the cached original.
            drop(pipeline.dispatch(&original, &mut Recorder::idle()).await);
        }
        pipeline
    };
    rt.block_on(async {
        // Cold: fresh cache and empty journal. What stays allocated is what
        // one dispatch leaves behind (handler event, journal copy, cache).
        let mut pipeline = fresh().await;
        let mut cold = Recorder::counting();
        let event = pipeline.dispatch(&text, &mut cold).await;
        let (cold_total, cold_steps) = cold.finish();
        drop(event);

        // Steady state: the cache already holds the entities and the journal
        // is full, so every publish also evicts one event. The handler's
        // drop of the event is the last step.
        let mut pipeline = fresh().await;
        let warm_count = if case.event == "GUILD_CREATE" {
            3
        } else {
            WARM_DISPATCHES
        };
        for _ in 0..warm_count {
            drop(pipeline.dispatch(&text, &mut Recorder::idle()).await);
        }
        let mut warm = Recorder::counting();
        let event = pipeline.dispatch(&text, &mut warm).await;
        warm.step("handler_drop", || drop(event));
        let (warm_total, warm_steps) = warm.finish();

        let uncounted = Uncounted::new();
        let mut timings = Vec::with_capacity(case.iterations);
        for _ in 0..case.iterations {
            let start = Instant::now();
            let event = pipeline.dispatch(&text, &mut Recorder::idle()).await;
            drop(black_box(event));
            timings.push(start.elapsed());
        }

        // Separate pass for the per-step time breakdown, so the clock reads
        // around each step do not touch the median above.
        let mut step_totals: Vec<(&'static str, Duration)> = Vec::new();
        let passes = case.iterations.min(500);
        for _ in 0..passes {
            let mut rec = Recorder::timing();
            let event = pipeline.dispatch(&text, &mut rec).await;
            rec.step("handler_drop", || drop(black_box(event)));
            for (name, duration) in rec.durations {
                match step_totals.iter_mut().find(|(step, _)| *step == name) {
                    Some((_, total)) => *total += duration,
                    None => step_totals.push((name, duration)),
                }
            }
        }
        drop(pipeline);
        drop(uncounted);
        let passes_u32 = u32::try_from(passes).unwrap();
        let mean_ns: Vec<(&'static str, u64)> = step_totals
            .into_iter()
            .map(|(name, total)| {
                (
                    name,
                    u64::try_from((total / passes_u32).as_nanos()).unwrap_or(u64::MAX),
                )
            })
            .collect();

        // Per step: allocations from the steady-state dispatch, retained
        // bytes from the cold one (the handler drop only exists warm).
        let steps: Vec<(&'static str, Sample)> = warm_steps
            .iter()
            .map(|(name, warm)| {
                let retained_bytes = cold_steps
                    .iter()
                    .find(|(cold, _)| cold == name)
                    .map_or(warm.retained_bytes, |(_, cold)| cold.retained_bytes);
                (
                    *name,
                    Sample {
                        retained_bytes,
                        ..*warm
                    },
                )
            })
            .collect();
        report.push(
            Scenario::new(&format!("inbound.{}", case.name), "inbound")
                .set("event", case.event)
                .set("payload_bytes", text.len())
                .sample(Sample {
                    retained_bytes: cold_total.retained_bytes,
                    ..warm_total
                })
                .set("cold_allocs", cold_total.allocs)
                .set("cold_peak_bytes", cold_total.peak_bytes)
                .steps(&steps, &mean_ns)
                .timings(timings)
                .deterministic(DETERMINISTIC),
        );
    });
}

/// Typed decode straight from the `d` text: the floor phase 4's direct
/// decoding approaches (no `Value` tree, no raw copy). Public serde only.
pub(crate) fn run_typed_reference<T: serde::de::DeserializeOwned>(
    name: &str,
    d: &Value,
    iterations: usize,
    report: &mut Report,
) {
    let text = d.to_string();
    let mark = Mark::start();
    let decoded: T = serde_json::from_str(&text).unwrap();
    let sample = mark.finish();
    drop(decoded);
    let uncounted = Uncounted::new();
    let mut timings = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        let decoded: T = serde_json::from_str(black_box(&text)).unwrap();
        drop(black_box(decoded));
        timings.push(start.elapsed());
    }
    drop(uncounted);
    report.push(
        Scenario::new(&format!("ref.typed_from_str.{name}"), "reference")
            .set("payload_bytes", text.len())
            .sample(sample)
            .timings(timings)
            .deterministic(DETERMINISTIC)
            .note("serde_json::from_str::<T>(d) only: no envelope, Value tree, raw copy, cache, or publish"),
    );
}
