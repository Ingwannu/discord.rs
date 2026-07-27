//! Multi-process sharding: one binary that is both the shard manager
//! (parent) and the shard runner (child), like discord.js's ShardingManager.
//!
//! Run with:
//! ```sh
//! DISCORD_TOKEN=... cargo run --example process_sharding_bot --features sharding
//! ```
//!
//! The parent fetches the recommended shard count from `/gateway/bot`,
//! re-executes itself once per shard with `DISCORDRS_SHARD_*` env vars set,
//! respawns crashed children, and periodically broadcasts a stats request.
//!
//! IMPORTANT: in child mode stdout is the IPC channel, so ALL example output
//! uses eprintln!/stderr. If you install a tracing subscriber, point it at
//! `std::io::stderr` (e.g. `tracing_subscriber::fmt().with_writer(std::io::stderr)`).

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
use discordrs::{gateway_intents, Client, Context, DiscordError, Event, EventHandler};
#[cfg(feature = "sharding")]
use serde_json::json;

#[cfg(feature = "sharding")]
struct Handler {
    guild_count: Arc<AtomicU64>,
}

#[cfg(feature = "sharding")]
#[async_trait]
impl EventHandler for Handler {
    async fn handle_event(&self, _ctx: Context, event: Event) {
        match event {
            Event::Ready(ready) => {
                // Child mode: stderr only! stdout belongs to the IPC protocol.
                eprintln!("[child] shard ready as {}", ready.data.user.username);
            }
            Event::GuildCreate(_) => {
                self.guild_count.fetch_add(1, Ordering::Relaxed);
            }
            Event::GuildDelete(_) => {
                let _ = self
                    .guild_count
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

    // One gateway client per assigned shard id (shards_per_process may be >1).
    for &shard_id in child.shard_ids() {
        let builder = Client::builder(&token, gateway_intents::GUILDS)
            .event_handler(Handler {
                guild_count: Arc::clone(&guild_count),
            })
            .shard(shard_id, shard_count);
        tokio::spawn(async move {
            if let Err(error) = builder.start().await {
                eprintln!("[child] shard {shard_id} gateway error: {error}");
            }
        });
    }

    // Answer parent broadcasts. A production bot would call notify_ready()
    // from the Ready event; reporting it once the clients are spawned keeps
    // this example small.
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
    });
    child.notify_ready()?;

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
    let config = ProcessShardManagerConfig {
        respawn: true,
        ..ProcessShardManagerConfig::default()
    };
    let manager = Arc::new(ProcessShardManager::spawn_auto(&token, config).await?);
    eprintln!(
        "[parent] managing {} shard(s) across {} child process(es)",
        manager.shard_count(),
        manager.children().len()
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
