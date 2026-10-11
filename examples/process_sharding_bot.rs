//! Multi-process sharding: one binary that is both the shard manager
//! (parent) and the shard runner (child), like discord.js's ShardingManager.
//!
//! Run with:
//! ```sh
//! DISCORD_TOKEN=... cargo run --example process_sharding_bot --features sharding
//! ```
//!
//! The parent reads the shard count, `max_concurrency`, and the daily
//! session-start budget from `/gateway/bot`. It re-executes itself once per
//! shard with `DISCORDRS_SHARD_*` env vars set, respawns crashed children,
//! grants every IDENTIFY attempt per concurrency bucket, and periodically
//! broadcasts a stats request.
//!
//! In child mode stdout carries the IPC frames. Stray prints do not break the
//! protocol (the parent logs them), but this example sends all of its output
//! to stderr, which the parent passes through. If you install a tracing
//! subscriber, point it at stderr too, e.g.
//! `tracing_subscriber::fmt().with_writer(std::io::stderr)`. Never read
//! stdin in a child: the IPC reader owns it.

#[cfg(feature = "sharding")]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(feature = "sharding")]
use std::sync::Arc;

#[cfg(feature = "sharding")]
use async_trait::async_trait;
#[cfg(feature = "sharding")]
use discordrs::sharding::process::{
    ProcessShardManager, ProcessShardManagerConfig, ShardChildProcess,
};
#[cfg(feature = "sharding")]
use discordrs::{
    gateway_intents, Client, Context, DiscordError, Event, EventHandler, GatewayConnectionConfig,
};
#[cfg(feature = "sharding")]
use serde_json::json;

#[cfg(feature = "sharding")]
struct Handler {
    child: ShardChildProcess,
    guild_count: Arc<AtomicU64>,
}

#[cfg(feature = "sharding")]
#[async_trait]
impl EventHandler for Handler {
    async fn handle_event(&self, ctx: Context, event: Event) {
        match event {
            Event::Ready(ready) => {
                // Status only: IDENTIFY pacing happens inside the gateway
                // client through the identify gate, not here.
                let (shard_id, _) = ctx.shard_pair();
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

/// Child mode: connect the assigned shards to the gateway and answer the
/// parent's broadcast requests with this process's guild count.
#[cfg(feature = "sharding")]
async fn run_child(child: ShardChildProcess, token: String) -> Result<(), DiscordError> {
    eprintln!(
        "[child] starting shards {:?} of {}",
        child.shard_ids(),
        child.shard_count()
    );

    let guild_count = Arc::new(AtomicU64::new(0));
    let shard_count = child.shard_count();

    // Serve broadcasts first: requests that arrive before `serve` are
    // answered with null. At most 32 handlers run at once, and each is
    // cancelled once the parent stops waiting for it.
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

    // Every fresh IDENTIFY of these clients, including reconnects after
    // INVALID_SESSION, waits for the parent's grant for its bucket. The
    // grant is shared with every other child process and respawn.
    let gateway_config = GatewayConnectionConfig::default().identify_gate(child.identify_gate());
    for &shard_id in child.shard_ids() {
        let builder = Client::builder(&token, gateway_intents::GUILDS)
            .gateway_config(gateway_config.clone())
            .event_handler(Handler {
                child: child.clone(),
                guild_count: Arc::clone(&guild_count),
            })
            .shard(shard_id, shard_count);
        tokio::spawn(async move {
            if let Err(error) = builder.start().await {
                eprintln!("[child] shard {shard_id} gateway error: {error}");
            }
        });
    }

    // Exit cleanly when the parent sends the shutdown op (or dies).
    child.shutdown_signal().await;
    eprintln!("[child] shutdown requested; exiting");
    Ok(())
}

/// Parent mode: spawn one child process per shard and broadcast stats
/// requests until the children stop (Ctrl-C ends the parent; children see
/// stdin close and exit on their own because they await `shutdown_signal`).
#[cfg(feature = "sharding")]
async fn run_parent(token: String) -> Result<(), DiscordError> {
    // `total_shards` and `max_concurrency` stay `None`, so spawn_auto reads
    // both, plus the session-start budget, from /gateway/bot.
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

    // Returns an error naming every child that stopped for good for any
    // reason other than a requested shutdown (gave up, unconfirmed kill, ...).
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
