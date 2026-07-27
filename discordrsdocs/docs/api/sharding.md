# Sharding

Sharding is provided behind the `sharding` feature on top of the `gateway` runtime.

## Current Status

- the base runtime tracks shard info in `Context`
- `start_shards(count)` spawns shards and waits until every shard task finishes
- `spawn_shards(count)` returns a `ShardSupervisor` for status inspection and shard control
- since `2.1`, `spawn_shards(...)` fetches `/gateway/bot` and uses the account's real `max_concurrency` for identify wave sizing (previously hardcoded to 1), and each shard's IDENTIFY is paced to Discord's 1-per-5s-per-shard limit

## Primary Types

- `ShardConfig`, `ShardInfo`, `ShardIpcMessage`, `ShardRuntimeStatus`
- `ShardingManager`, `ShardMessenger`, `ShardSupervisor`
- `discordrs::sharding::process::{ProcessShardManager, ProcessShardManagerConfig, ShardChildProcess}` (2.2.0)

## Multi-Process Sharding (2.2.0)

`discordrs::sharding::process` is the discord.js `ShardingManager` equivalent: a parent process spawns one child process per shard group, respawns crashed children with exponential backoff (base `respawn_delay`, capped at five minutes, reset after 60s of uptime, bounded by `max_respawns`), staggers child starts for Discord's identify limits (`spawn_stagger`, default 5s), and exchanges JSON-lines IPC over child stdio. By default children re-execute the current binary, and `ShardChildProcess::from_env()` returns `Some` only in child mode, so one binary serves both roles:

```rust
use discordrs::sharding::process::{
    ProcessShardManager, ProcessShardManagerConfig, ShardChildProcess,
};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), discordrs::DiscordError> {
    let token = std::env::var("DISCORD_TOKEN")?;

    if let Some(child) = ShardChildProcess::from_env() {
        // Child mode: run child.shard_ids() with normal gateway Clients.
        // stdout is the IPC channel — log to stderr only.
        child.serve(|request| async move { json!({ "echo": request }) });
        child.notify_ready()?;
        child.shutdown_signal().await;
        return Ok(());
    }

    // Parent mode: spawn_auto fetches the recommended shard count
    // from /gateway/bot when total_shards is None.
    let manager =
        ProcessShardManager::spawn_auto(token, ProcessShardManagerConfig::default()).await?;

    // Request/response across all children — the broadcastEval /
    // fetchClientValues equivalent.
    let stats = manager.broadcast(json!({ "op": "stats" })).await?;
    for (shard_id, value) in stats {
        println!("shard {shard_id}: {value}");
    }

    manager.wait().await?;
    Ok(())
}
```

- `ProcessShardManagerConfig` fields: `total_shards`, `shard_list`, `shards_per_process`, `respawn`, `respawn_delay`, `max_respawns`, `program`/`args`/`extra_env` for custom child binaries, and `spawn_stagger`.
- `manager.children()` reports per-child `ChildStatus`, `manager.shutdown()` broadcasts a graceful shutdown op, and `manager.wait()` blocks until all children exit.
- Children observe shutdown through `shutdown_signal()`, which also resolves when the parent dies and stdin closes, so children never linger as orphans.
- `child.log(...)` sends log lines to the parent over IPC; `child.notify_ready()` reports shard readiness.
- Full example: `examples/process_sharding_bot.rs` (`cargo run --example process_sharding_bot --features sharding`).

## Behavior

- initial shard boot is queued instead of identifying every shard at once; queued shards report `ShardRuntimeState::Queued`
- identify waves are sized by the `max_concurrency` value Discord reports for the bot account
- shutdown can be awaited with `shutdown_and_wait()` or `wait_for_shutdown(timeout)`
- reconnect backoff is interruptible, so shutdown does not wait for a long sleep to finish
