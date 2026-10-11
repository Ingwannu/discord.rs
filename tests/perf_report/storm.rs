//! GUILD_CREATE storm through the inbound replica, in this process: 25
//! distinct large guilds dispatched one after another into one pipeline
//! (cache + collector journal), each handler event dropped before the next
//! frame, as when handlers and queues keep up.
//!
//! Unlike `e2e.guild_create_storm` (real runtime, scheduling-dependent),
//! every number here is deterministic: no deletions happen in the cache
//! maps, so even std's randomly seeded `HashMap`s grow the same way.
//!
//! The live heap after the storm is split by what releases it:
//!
//! - `journal_bytes`: freed when 256 tiny dispatches push the guild events
//!   out of the collector journal, i.e. what only the journal kept alive. Once
//!   the cache shares payloads with events (phase 4 C), the shared part stays
//!   with the cache and is no longer counted here;
//! - `cache_bytes`: what is still live after that, relative to the start.
//!
//! The journal is filled with tiny events before the storm, so its ring has
//! already grown to its working capacity and the flush evicts exactly the
//! storm.

use std::sync::atomic::Ordering;

use serde_json::json;

use crate::alloc::{Mark, Recorder, LIVE_BYTES};
use crate::fixtures::{frame, synthesized_guild_create, FLOOR_EVENT, LARGE_GUILD, STORM_GUILDS};
use crate::inbound::{Pipeline, HUB_JOURNAL_CAPACITY};
use crate::report::{Report, Scenario, DETERMINISTIC};

/// The storm uses guild indices distinct from the e2e storm and the cache
/// scenario, though any disjoint range gives the same numbers.
const FIRST_GUILD: u64 = 200;

pub(crate) fn run_storm(rt: &tokio::runtime::Runtime, report: &mut Report) {
    let frames: Vec<String> = (FIRST_GUILD..FIRST_GUILD + STORM_GUILDS)
        .map(|index| {
            frame(
                "GUILD_CREATE",
                index,
                &synthesized_guild_create(index, &LARGE_GUILD),
            )
        })
        .collect();
    let floor = frame(FLOOR_EVENT, 1, &json!({}));

    let (storm, journal_bytes, cache_bytes) = rt.block_on(async {
        let mut pipeline = Pipeline::new();
        let mut idle = Recorder::idle();
        for _ in 0..=HUB_JOURNAL_CAPACITY {
            drop(pipeline.dispatch(&floor, &mut idle).await);
        }

        let live_start = LIVE_BYTES.load(Ordering::SeqCst);
        let mark = Mark::start();
        for text in &frames {
            // The handler's event is dropped before the next frame arrives.
            drop(pipeline.dispatch(text, &mut idle).await);
        }
        let storm = mark.finish();
        let live_after_storm = LIVE_BYTES.load(Ordering::SeqCst);

        for _ in 0..HUB_JOURNAL_CAPACITY {
            drop(pipeline.dispatch(&floor, &mut idle).await);
        }
        let live_after_flush = LIVE_BYTES.load(Ordering::SeqCst);
        drop(pipeline);
        (
            storm,
            live_after_storm - live_after_flush,
            live_after_flush - live_start,
        )
    });

    let mut deterministic = DETERMINISTIC.to_vec();
    deterministic.extend(["journal_bytes", "cache_bytes"]);
    report.push(
        Scenario::new("storm.guild_create_25", "storm")
            .batch(storm, STORM_GUILDS)
            .set("guilds", STORM_GUILDS)
            .set("members_per_guild", LARGE_GUILD.members)
            .set("journal_bytes", journal_bytes)
            .set("cache_bytes", cache_bytes)
            .deterministic(&deterministic)
            .note("replica, sequential, handler events dropped: retained_bytes = live heap after the storm (gate), split into journal_bytes and cache_bytes; allocs/alloc_bytes per guild"),
    );
}
