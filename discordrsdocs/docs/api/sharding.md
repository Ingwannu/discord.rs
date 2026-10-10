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
- `discordrs::sharding::process::{ProcessShardManager, ProcessShardManagerConfig, ShardChildProcess, IdentifyPermit}` (multi-process; `IdentifyPermit` since 3.0)

## Multi-Process Sharding

`discordrs::sharding::process` is the discord.js `ShardingManager` equivalent. A parent process:

- spawns one child process per shard group;
- respawns crashed children with exponential backoff: base `respawn_delay`, capped at five minutes, reset after 60s of uptime, bounded by `max_respawns`;
- grants IDENTIFY permits per `max_concurrency` bucket across every child;
- exchanges framed JSON lines over child stdio.

By default children re-execute the current binary. `ShardChildProcess::from_env()` returns `Some` only in child mode, so one binary serves both roles:

```rust
use discordrs::sharding::process::{
    ProcessShardManager, ProcessShardManagerConfig, ShardChildProcess,
};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), discordrs::DiscordError> {
    let token = std::env::var("DISCORD_TOKEN")?;

    if let Some(child) = ShardChildProcess::from_env() {
        // Child mode. Serve broadcasts first; earlier requests get null.
        child.serve(|request| async move { json!({ "echo": request }) })?;
        for &shard_id in child.shard_ids() {
            // Wait for the parent's IDENTIFY grant for this shard's bucket.
            let permit = child.acquire_identify(shard_id).await?;
            // ... start a gateway Client for shard_id here, then release the
            // permit and report readiness when READY arrives:
            drop(permit);
            child.notify_shard_ready(shard_id)?;
        }
        child.shutdown_signal().await;
        return Ok(());
    }

    // Parent mode: spawn_auto reads the shard count and max_concurrency
    // from /gateway/bot when they are None.
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

The full example `examples/process_sharding_bot.rs` holds each permit until the shard's READY event (`cargo run --example process_sharding_bot --features sharding`).

### Configuration

| `ProcessShardManagerConfig` field | Default | Meaning |
|---|---|---|
| `total_shards` | `None` | Required by `spawn`; `spawn_auto` fills it from `/gateway/bot`. |
| `shard_list` | all shards | Subset of shard ids this manager runs. |
| `shards_per_process` | `1` | Shards per child process. |
| `respawn`, `respawn_delay`, `max_respawns` | `true`, 5s, unlimited | Respawn policy (exponential backoff, capped at 5 min). |
| `program`, `args`, `extra_env` | current exe | Custom child binary and its arguments and environment. |
| `max_concurrency` | `None` | IDENTIFY buckets (`shard_id % max_concurrency`). `spawn_auto` reads it from `/gateway/bot`; `spawn` assumes `1`. |
| `identify_interval` | 5s | Spacing between grants in one bucket, measured from the previous permit's release. |
| `identify_permit_timeout` | 30s | A permit that is not released by then is reclaimed. |
| `spawn_stagger` | `0` | Delay between child *process* starts. Only needed for custom children that never request permits. |
| `broadcast_timeout` | 10s | Per-child answer deadline for `broadcast`. |
| `shutdown_timeout` | 10s | Total time a child gets to exit after shutdown before it is killed. |
| `max_ipc_frame_bytes` | 1 MiB | Largest IPC line in either direction (minimum 4 KiB). |

### IDENTIFY coordination

Discord allows one IDENTIFY per bucket every 5 seconds, where the bucket is `shard_id % max_concurrency` ([Gateway: max concurrency](https://docs.discord.com/developers/events/gateway#max-concurrency)). The parent keeps one FIFO queue per bucket, shared by every child and every respawn:

- `child.acquire_identify(shard_id)` asks the parent for a permit and resolves when it is granted. Different buckets proceed in parallel.
- Drop the returned `IdentifyPermit` (or call `release()`) once the shard has sent IDENTIFY. READY is the simplest signal; also drop it if the shard fails to start. The next grant in that bucket comes `identify_interval` after the release.
- If a child exits while holding a permit, the parent reclaims it at exit time, so a crash loop cannot identify faster than the bucket allows. A permit held longer than `identify_permit_timeout` is reclaimed as well.
- When `/gateway/bot` reports `session_start_limit.remaining == 0`, `spawn_auto` delays the first grant until `reset_after`.
- Limitation: the gateway client re-IDENTIFYs on its own after an invalidated session (with its 5s-per-shard pacing). Those re-IDENTIFYs do not go through the parent.

### IPC framing

Every protocol message is one line, `@discordrs-ipc/1 ` followed by a JSON object (`IPC_FRAME_PREFIX`), sent over the child's stdin (parent → child) and stdout (child → parent).

- Lines without the prefix are ordinary output. The parent logs them at INFO (`child stdout: ...`) instead of failing, so a stray `println!` no longer corrupts IPC.
- Lines longer than `max_ipc_frame_bytes` are discarded while they stream in, so a child cannot make the parent buffer unbounded memory. Invalid UTF-8 is tolerated.
- A broadcast payload that would exceed the limit makes `broadcast` return an error before anything is sent. A child response that would exceed it is replaced with `null` by the child.
- Child stderr is inherited unchanged and remains the right place for logs. Child code must **not** read stdin, because the IPC reader owns it.
- Writes that bypass Rust's stdout lock (C libraries, subprocesses sharing fd 1) can tear one frame. That frame is dropped and its broadcast entry reads `null`.

Why stdio framing instead of an extra pipe fd or a socket: it needs no `unsafe` fd plumbing, works the same on Windows and Unix and with any child program (including shell scripts), opens no local listening endpoint, and keeps stdin EOF as the "parent died" signal.

| Direction | `op` | Fields | Meaning |
|---|---|---|---|
| parent → child | `shutdown` | | Stop gracefully; stdin closes right after. |
| parent → child | `request` | `id`, `payload` | Answer with `response` carrying the same `id`. |
| parent → child | `identify_grant` | `shard_id` | The shard may IDENTIFY now. |
| child → parent | `ready` | `shard_id` | The shard is connected. |
| child → parent | `response` | `id`, `payload` | Broadcast answer. |
| child → parent | `log` | `message` | Logged through the parent's `tracing`. |
| child → parent | `identify_request` | `shard_id` | Queue for a permit (one outstanding per shard). |
| child → parent | `identify_release` | `shard_id` | IDENTIFY sent or abandoned; the bucket interval starts. |

Children also receive `DISCORDRS_SHARD_IDS`, `DISCORDRS_SHARD_COUNT`, `DISCORDRS_SHARD_IPC=1`, and `DISCORDRS_SHARD_IPC_MAX_FRAME`.

### Lifecycle

- `manager.children()` reports per-child `ChildStatus` (`Starting` → `Ready`/`Running` → `Respawning`/`Stopped`).
- `manager.shutdown()` sends the `shutdown` op, closes stdin, keeps reading the child's stdout, and kills the child when `shutdown_timeout` (measured from the shutdown request) passes. A child that stopped reading stdin cannot delay this, and it cannot stall `broadcast` either: each child has its own bounded writer queue, and a full queue answers `null` immediately.
- `manager.wait()` and `manager.shutdown()` share one completion signal, so any number of concurrent callers all return only after every child has exited.
- Broadcast entries are `null` for children that are down, respawning, not yet serving, or not answering within `broadcast_timeout`.
- Children observe shutdown through `shutdown_signal()`, which also resolves when the parent dies and stdin closes, so children never linger as orphans. Dropping the manager also stops its children.
- `child.log(...)` sends log lines to the parent; `child.notify_shard_ready(id)` / `child.notify_ready()` report readiness.

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

All shards started by one `ClientBuilder` share the cache, the `TypeMap`, the collector hub, and the external cache backend worker (owned by the supervisor and drained when `wait` finishes; `wait` returns an error naming shards whose backend recovery could not finish). An external cache store may be mirrored by only one running client, so processes started by `ProcessShardManager` need one store or key namespace each. This must be enforced by your deployment: sharing is not detected at startup — the newest client silently takes the store over and the older one only notices (reporting `superseded` in its cache backend health) at its next clear. A READY only invalidates the cache entries of the shard that received it (see [Cache and Collectors](cache-and-collectors.md)), and guild-scoped gateway commands from any shard's `Context` (`fetch_members`, `request_guild_members`, `update_voice_state`, `join_voice`, `leave_voice`) are sent on the shard that owns the guild.
