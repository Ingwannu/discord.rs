//! Collector fan-out replica: one event delivered to K message collectors,
//! one of which matches.

use std::hint::black_box;
use std::sync::Arc;
use std::time::Instant;

use discordrs::model::Message;
use discordrs::{decode_event, Event, EventKind, MessageId};

use crate::alloc::{Mark, Uncounted};
use crate::fixtures::{fixture, MESSAGE_CREATE_PLAIN};
use crate::report::{Report, Scenario, DETERMINISTIC};

/// A message collector's filter, as `MessageCollector::filter` stores it.
type MessageFilter = Box<dyn Fn(&Message) -> bool>;

pub(crate) fn run_fanout(report: &mut Report) {
    let event = Arc::new(
        decode_event("MESSAGE_CREATE", &fixture(MESSAGE_CREATE_PLAIN).to_string()).unwrap(),
    );
    let EventKind::MessageCreate(created) = event.kind() else {
        panic!("MESSAGE_CREATE fixture decoded to {:?}", event.kind());
    };
    let target = created.id;
    let other = MessageId::try_from(1_u64).unwrap();
    for k in [1_usize, 8, 64] {
        // Collector 0 waits for this message; the others for another one.
        let filters: Vec<MessageFilter> = (0..k)
            .map(|i| {
                let wanted = if i == 0 { target } else { other };
                Box::new(move |message: &Message| message.id == wanted) as MessageFilter
            })
            .collect();
        let deliver = |event: &Arc<Event>| {
            // pipeline.rs publishes the handler's `Arc` into the hub journal ...
            let journaled = Arc::clone(event);
            // One collector matches; it hands its message to the caller.
            let mut collected = None;
            for filter in &filters {
                // ... every subscription receives the same `Arc`
                // (collector.rs `EventSubscription::recv`), filters the
                // payload by reference, and copies only a match out
                // (`Extracting::item`).
                let received = Arc::clone(&journaled);
                if let EventKind::MessageCreate(message) = received.kind() {
                    if filter(message) {
                        collected = Some(Message::clone(message));
                    }
                }
            }
            (journaled, collected)
        };
        let mark = Mark::start();
        let delivered = deliver(&event);
        let sample = mark.finish();
        assert!(delivered.1.is_some());
        drop(delivered);
        let uncounted = Uncounted::new();
        let mut timings = Vec::with_capacity(2_000);
        for _ in 0..2_000 {
            let start = Instant::now();
            drop(black_box(deliver(black_box(&event))));
            timings.push(start.elapsed());
        }
        drop(uncounted);
        report.push(
            Scenario::new(&format!("fanout.message_collectors.k{k}"), "fanout")
                .set("collectors", k)
                .sample(sample)
                .timings(timings)
                .deterministic(DETERMINISTIC)
                .note("journal Arc + one Arc per collector, filters on the shared payload, one match copied; retained_bytes = the collected message"),
        );
    }
}
