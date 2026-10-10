//! Multi-process sharding: one binary that is both the shard manager
//! (parent) and the shard runner (child), like discord.js's ShardingManager.
//!
//! Run with:
//! ```sh
//! DISCORD_TOKEN=... cargo run --example process_sharding_bot --features sharding
//! ```
//!
//! The parent fetches the recommended shard count and `max_concurrency` from
//! `/gateway/bot`, re-executes itself once per shard with `DISCORDRS_SHARD_*`
//! env vars set, respawns crashed children, grants IDENTIFY permits per
//! concurrency bucket, and periodically broadcasts a stats request.
//!
//! In child mode stdout carries the IPC frames. Stray prints no longer
//! break the protocol (the parent logs them), but this example sends all of
//! its output to stderr, which the parent passes through. If you install a
//! tracing subscriber, point it at stderr too, e.g.
//! `tracing_subscriber::fmt().with_writer(std::io::stderr)`. Never read
//! stdin in a child: the IPC reader owns it.

#[cfg(feature = "sharding")]
use std::collections::HashMap;
#[cfg(feature = "sharding")]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(feature = "sharding")]
use std::sync::{Arc, Mutex, PoisonError};

#[cfg(feature = "sharding")]
use async_trait::async_trait;
#[cfg(feature = "sharding")]
use discordrs::sharding::process::{
    IdentifyPermit, ProcessShardManager, ProcessShardManagerConfig, ShardChildProcess,
};
#[cfg(feature = "sharding")]
use discordrs::{gateway_intents, Client, Context, DiscordError, Event, EventHandler};
#[cfg(feature = "sharding")]
use serde_json::json;

/// IDENTIFY permits held by shards that have not seen READY yet.
#[cfg(feature = "sharding")]
type HeldPermits = Arc<Mutex<HashMap<u32, IdentifyPermit>>>;

/// Gives a shard's IDENTIFY slot back to the parent. The parent spaces the
/// next grant in the same `shard_id % max_concurrency` bucket from this
/// moment, so release only after IDENTIFY has gone out.
#[cfg(feature = "sharding")]
fn release_identify(permits: &HeldPermits, shard_id: u32) {
    let permit = permits
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(&shard_id);
    // Dropping the permit sends `identify_release`; do it outside the lock.
    drop(permit);
}

#[cfg(feature = "sharding")]
struct Handler {
    child: ShardChildProcess,
    permits: HeldPermits,
    guild_count: Arc<AtomicU64>,
}

#[cfg(feature = "sharding")]
#[async_trait]
impl EventHandler for Handler {
    async fn handle_event(&self, ctx: Context, event: Event) {
        match event {
            Event::Ready(ready) => {
                let (shard_id, _) = ctx.shard_pair();
                // READY proves the IDENTIFY was accepted, so the bucket can
                // move on to the next shard.
                release_identify(&self.permits, shard_id);
                if let Err(error) = self.child.notify_shard_ready(shard_id) {
                    eprintln!("[child] failed to report shard {shard_id} ready: {error}");
                }
                eprintln!(
                    "[child] shard {shard_id} ready as {}",
                    ready.data.user.username
                );
            }
            Event::GuildCreate(_) => {
                self.guild_count.fetch_add(1, Ordering::Relaxed);
            }
            Event::GuildDelete(_) => {
                // Rust 1.99 renamed `fetch_update` to `try_update`; keep the
                // old name until the crate's MSRV reaches 1.99.
                #[allow(deprecated)]
                let _ =
                    self.guild_count
                        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
                            count.checked_sub(1)
                        });
            }
            _ => {}
        }
    }
}

/// Child mode: connect the assigned shards to the gateway, one IDENTIFY
/// permit at a time per bucket, and answer the parent's broadcast requests
/// with this process's guild count.
#[cfg(feature = "sharding")]
async fn run_child(child: ShardChildProcess, token: String) -> Result<(), DiscordError> {
    eprintln!(
        "[child] starting shards {:?} of {}",
        child.shard_ids(),
        child.shard_count()
    );

    let guild_count = Arc::new(AtomicU64::new(0));
    let permits: HeldPermits = Arc::new(Mutex::new(HashMap::new()));
    let shard_count = child.shard_count();

    // Serve broadcasts first: requests that arrive before `serve` are
    // answered with null.
    let shard_ids = child.shard_ids().to_vec();
    let stats_count = Arc::clone(&guild_count);
    child.serve(move |request| {
        let shard_ids = shard_ids.clone();
        let guilds = stats_count.load(Ordering::Relaxed);
        async move {
            json!({
                "request": request,
                "shard_ids": shard_ids,
                "guilds": guilds,
            })
        }
    })?;

    // One task per shard so that shards in different buckets do not queue
    // behind each other (shards_per_process may be > 1).
    for &shard_id in child.shard_ids() {
        let child = child.clone();
        let token = token.clone();
        let permits = Arc::clone(&permits);
        let guild_count = Arc::clone(&guild_count);
        tokio::spawn(async move {
            // The parent grants one IDENTIFY per bucket at a time, across
            // every child process and respawn.
            let permit = match child.acquire_identify(shard_id).await {
                Ok(permit) => permit,
                Err(error) => {
                    eprintln!("[child] no IDENTIFY permit for shard {shard_id}: {error}");
                    return;
                }
            };
            permits
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(shard_id, permit);

            let builder = Client::builder(&token, gateway_intents::GUILDS)
                .event_handler(Handler {
                    child,
                    permits: Arc::clone(&permits),
                    guild_count,
                })
                .shard(shard_id, shard_count);
            if let Err(error) = builder.start().await {
                eprintln!("[child] shard {shard_id} gateway error: {error}");
            }
            // The shard stopped (possibly before READY): never keep the
            // bucket blocked.
            release_identify(&permits, shard_id);
        });
    }

    // Exit cleanly when the parent sends the shutdown op (or dies).
    child.shutdown_signal().await;
    eprintln!("[child] shutdown requested; exiting");
    Ok(())
}

/// Parent mode: spawn one child process per shard and broadcast stats
/// requests forever (Ctrl-C to stop; children notice the closed pipe and
/// exit on their own).
#[cfg(feature = "sharding")]
async fn run_parent(token: String) -> Result<(), DiscordError> {
    // `total_shards` and `max_concurrency` stay `None`, so spawn_auto reads
    // both from /gateway/bot.
    let config = ProcessShardManagerConfig {
        respawn: true,
        ..ProcessShardManagerConfig::default()
    };
    let manager = Arc::new(ProcessShardManager::spawn_auto(&token, config).await?);
    eprintln!(
        "[parent] managing {} shard(s) across {} child process(es), max_concurrency {}",
        manager.shard_count(),
        manager.children().len(),
        manager.max_concurrency()
    );

    let broadcaster = Arc::clone(&manager);
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            for status in broadcaster.children() {
                eprintln!(
                    "[parent] child shards={:?} state={:?} pid={:?} respawns={}",
                    status.shard_ids, status.state, status.pid, status.respawns
                );
            }
            match broadcaster.broadcast(json!({"kind": "stats"})).await {
                Ok(results) => {
                    for (first_shard, value) in results {
                        eprintln!("[parent] shard {first_shard}: {value}");
                    }
                }
                Err(error) => eprintln!("[parent] broadcast failed: {error}"),
            }
        }
    });

    manager.wait().await
}

#[cfg(feature = "sharding")]
#[tokio::main]
async fn main() -> Result<(), DiscordError> {
    // The children inherit DISCORD_TOKEN from the parent's environment.
    let token = std::env::var("DISCORD_TOKEN")?;
    match ShardChildProcess::from_env() {
        Some(child) => run_child(child, token).await,
        None => run_parent(token).await,
    }
}

#[cfg(not(feature = "sharding"))]
fn main() {}
