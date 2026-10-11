//! In-memory cache scenarios: GUILD_CREATE ingestion, message inserts under
//! the default limits, and MESSAGE_UPDATE merges.

use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use discordrs::model::{Guild, Message};
use discordrs::{CacheConfig, CacheHandle, ChannelId, MessageId};
use serde_json::Value;

use crate::alloc::{counting, Mark, Sample, Uncounted, LIVE_BYTES};
use crate::fixtures::{
    fixture, synthesized_guild_create, LARGE_GUILD, MESSAGE_CREATE_PLAIN, MESSAGE_UPDATE_PARTIAL,
};
use crate::inbound::merge_update_replica;
use crate::report::{Report, Scenario, DETERMINISTIC};

/// Messages inserted in the message cache scenario, spread over
/// `MESSAGE_CHANNELS` channels. The default limits (100 per channel, 10,000
/// in total) bound what stays cached.
const MESSAGE_INSERTS: u64 = 50_000;
const MESSAGE_CHANNELS: u64 = 100;
const MESSAGE_UPDATES: u64 = 10_000;

fn message_id(n: u64) -> MessageId {
    MessageId::try_from(1_500_000_000_000_000_000_u64 + n).unwrap()
}

fn message_channel_id(n: u64) -> ChannelId {
    ChannelId::try_from(1_400_000_000_000_000_000_u64 + n % MESSAGE_CHANNELS).unwrap()
}

/// Inserts message `n` (a copy of `template` with its own ID and channel)
/// and returns how long the insert took. The copy is made first, so only
/// the insert is counted and timed.
async fn insert_message(
    cache: &CacheHandle,
    template: &Message,
    n: u64,
    sample: &mut Sample,
) -> Duration {
    let mut message = template.clone();
    message.id = message_id(n);
    message.channel_id = message_channel_id(n);
    let mark = counting().then(Mark::start);
    let start = Instant::now();
    cache.upsert_message(message).await;
    let elapsed = start.elapsed();
    if let Some(mark) = mark {
        let step = mark.finish();
        sample.allocs += step.allocs;
        sample.alloc_bytes += step.alloc_bytes;
        sample.peak_bytes = sample.peak_bytes.max(mark.live + step.peak_bytes);
    }
    elapsed
}

/// The apply_cache_updates MESSAGE_UPDATE path for message `n` (cached
/// clone, payload key set, merge, upsert); returns how long it took. The
/// partial's copy is excluded.
async fn update_message(
    cache: &CacheHandle,
    partial: &Message,
    partial_raw: &Value,
    n: u64,
    sample: &mut Sample,
) -> Duration {
    let mut message = partial.clone();
    message.id = message_id(n);
    message.channel_id = message_channel_id(n);
    let mark = counting().then(Mark::start);
    let start = Instant::now();
    let cached = cache
        .message(message.channel_id, message.id)
        .await
        .unwrap_or_else(|| panic!("message {n} is not cached"));
    let fields: HashSet<String> = partial_raw
        .as_object()
        .map(|object| object.keys().cloned().collect())
        .unwrap_or_default();
    cache
        .upsert_message(merge_update_replica(cached, message, &fields))
        .await;
    let elapsed = start.elapsed();
    if let Some(mark) = mark {
        let step = mark.finish();
        sample.allocs += step.allocs;
        sample.alloc_bytes += step.alloc_bytes;
        sample.peak_bytes = sample.peak_bytes.max(mark.live + step.peak_bytes);
    }
    elapsed
}

/// The message cache's hash maps use std's per-process random hasher, so
/// whether a map with deletions resizes or rehashes in place can change
/// between runs; that moves the peak by a table size (128 KiB was seen).
const HASHED_DETERMINISTIC: &[&str] = &["allocs", "alloc_bytes", "retained_bytes"];

pub(crate) fn run_cache(rt: &tokio::runtime::Runtime, report: &mut Report) {
    rt.block_on(async {
        // GUILD_CREATE x10 into a fresh cache: 10,000 members, users, and
        // presences, 2,000 channels, 3,000 roles. The typed guilds are
        // decoded before measuring; apply_guild_create clones what it keeps.
        let guilds: Vec<Guild> = (100..110)
            .map(|index| {
                serde_json::from_value(synthesized_guild_create(index, &LARGE_GUILD)).unwrap()
            })
            .collect();
        let ops = u64::try_from(guilds.len()).unwrap();
        let cache = CacheHandle::with_config(CacheConfig::default());
        let mark = Mark::start();
        for guild in &guilds {
            cache.apply_guild_create(guild).await;
        }
        let sample = mark.finish();
        drop(cache);
        let timings = {
            let _uncounted = Uncounted::new();
            let cache = CacheHandle::with_config(CacheConfig::default());
            let mut timings = Vec::with_capacity(guilds.len());
            for guild in &guilds {
                let start = Instant::now();
                cache.apply_guild_create(guild).await;
                timings.push(start.elapsed());
            }
            timings
        };
        report.push(
            Scenario::new("cache.guild_create.10x1000_members", "cache")
                .batch(sample, ops)
                .set("members", ops * LARGE_GUILD.members)
                .timings(timings)
                .deterministic(DETERMINISTIC)
                .note("retained_bytes: heap the cache holds for 10 large guilds"),
        );
        drop(guilds);

        // MESSAGE_CREATE x50,000 into a fresh cache with default limits,
        // then MESSAGE_UPDATE x10,000 against the 10,000 messages it keeps.
        let template: Message = serde_json::from_value(fixture(MESSAGE_CREATE_PLAIN)).unwrap();
        let partial_raw = fixture(MESSAGE_UPDATE_PARTIAL);
        let partial: Message = serde_json::from_value(partial_raw.clone()).unwrap();
        let first_cached = MESSAGE_INSERTS - MESSAGE_UPDATES;

        let cache = CacheHandle::with_config(CacheConfig::default());
        let live_before = LIVE_BYTES.load(Ordering::SeqCst);
        let mut insert = Sample {
            peak_bytes: live_before,
            ..Sample::default()
        };
        for n in 0..MESSAGE_INSERTS {
            insert_message(&cache, &template, n, &mut insert).await;
        }
        insert.retained_bytes = LIVE_BYTES.load(Ordering::SeqCst) - live_before;
        insert.peak_bytes -= live_before;
        let channel_messages = cache.messages_arc(message_channel_id(0)).await.len();

        let live_before = LIVE_BYTES.load(Ordering::SeqCst);
        let mut update = Sample {
            peak_bytes: live_before,
            ..Sample::default()
        };
        for n in first_cached..MESSAGE_INSERTS {
            update_message(&cache, &partial, &partial_raw, n, &mut update).await;
        }
        update.retained_bytes = LIVE_BYTES.load(Ordering::SeqCst) - live_before;
        update.peak_bytes -= live_before;
        drop(cache);

        let (insert_timings, update_timings) = {
            let _uncounted = Uncounted::new();
            let mut ignored = Sample::default();
            let cache = CacheHandle::with_config(CacheConfig::default());
            let mut insert_timings = Vec::with_capacity(usize::try_from(MESSAGE_INSERTS).unwrap());
            for n in 0..MESSAGE_INSERTS {
                insert_timings.push(insert_message(&cache, &template, n, &mut ignored).await);
            }
            let mut update_timings = Vec::with_capacity(usize::try_from(MESSAGE_UPDATES).unwrap());
            for n in first_cached..MESSAGE_INSERTS {
                update_timings
                    .push(update_message(&cache, &partial, &partial_raw, n, &mut ignored).await);
            }
            (insert_timings, update_timings)
        };
        report.push(
            Scenario::new("cache.message_insert.50k", "cache")
                .batch(insert, MESSAGE_INSERTS)
                .set(
                    "cached_messages",
                    u64::try_from(channel_messages).unwrap() * MESSAGE_CHANNELS,
                )
                .timings(insert_timings)
                .deterministic(HASHED_DETERMINISTIC)
                .note("per insert: upsert_message only (the input copy is excluded); retained_bytes: cache after 50k inserts under the default 100/channel, 10k total limits"),
        );
        report.push(
            Scenario::new("cache.message_update_merge.10k", "cache")
                .batch(update, MESSAGE_UPDATES)
                .timings(update_timings)
                .deterministic(HASHED_DETERMINISTIC)
                .note("per update: cache.message() clone + key set + merge + upsert_message (the partial's copy is excluded)"),
        );
    });
}
