# Sharding

Sharding is provided behind the `sharding` feature on top of the `gateway` runtime.

## Current Status

- the base runtime tracks shard info in `Context`
- `start_shards(count)` spawns shards in this process and waits for them (`spawn_shards(count)` + `ShardSupervisor::wait()`)
- `spawn_shards(count)` returns a `ShardSupervisor` for status inspection and shard control; since 3.0 it returns as soon as the shard tasks exist, while later identify waves are still queued, so the supervisor owns and supervises every shard from the start
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

### Waiting and Failures (3.0)

`ShardSupervisor::wait()` (and therefore `start_shards` / `start_auto_shards`) watches every shard at once, including while later identify waves are still queued:

- The first shard that fails — for example with `DiscordError::Gateway` for a non-reconnectable close code such as 4004 (authentication failed) or 4011 (sharding required), or an event queue overflow — raises the failure *before* it drains its own handlers. The boot schedule stops at once (shards not released yet never connect, even if nobody is awaiting the supervisor), and a running `wait()` immediately sends `Shutdown` to the other shards, waits up to the handler shutdown timeout plus two seconds (never past the caller's `wait_for_shutdown` deadline), then aborts and joins whatever is still running and returns the failing shard's error. Before 3.0 shards were awaited in order, so a later shard's failure stayed hidden while an earlier shard kept running, failures during the boot waves were not acted on until every wave had launched, and an early error dropped the other handles without stopping them.
- A shard that stops cleanly (`Ok`) does not affect the others; `wait()` returns `Ok(())` once all have stopped and the external cache backend worker has flushed.
- `wait_for_shutdown(timeout)` aborts and joins every shard still running when the deadline passes. Aborted tasks are joined for up to one second; a task that cannot confirm cancellation (for example one blocking its thread) is named in the returned error instead of being claimed stopped. A `wait*` future owns the shards from the moment it is created: dropping it — even before its first poll — aborts the running shards and the boot schedule (without joining them). Dropping the `ShardSupervisor` itself without waiting leaves them running.
- `shutdown()` stops the boot schedule and sends `Shutdown` to every shard. `shutdown_and_wait()` always runs the bounded wait-and-abort cleanup, even when sending `Shutdown` failed for a shard whose channel was already closed, and returns that send error afterwards.
- Each shard stops promptly after its gateway exits: queued events are delivered and running handlers may finish for up to `ClientBuilder::handler_shutdown_timeout` (default 5 seconds, counted from the gateway's exit, not from the shutdown request), then they are aborted and joined.
- `ShardingManager::broadcast` tries every shard even if one shard's command channel is already closed, and the per-shard command bridge waits for queue space instead of giving up, so `Shutdown` reaches every running shard.

### Shared State Across In-Process Shards

All shards started by one `ClientBuilder` share the cache, the `TypeMap`, the collector hub, and the external cache backend worker (owned by the supervisor and drained when `wait` finishes). A READY only invalidates the cache entries of the shard that received it (see [Cache and Collectors](cache-and-collectors.md)), and guild-scoped gateway commands from any shard's `Context` (`fetch_members`, `request_guild_members`, `update_voice_state`, `join_voice`, `leave_voice`) are sent on the shard that owns the guild.
