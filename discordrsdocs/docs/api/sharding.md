# Sharding

Sharding is provided behind the `sharding` feature on top of the `gateway` runtime.

## Current Status

- the base runtime tracks shard info in `Context`
- `start_shards(count)` spawns shards in this process and waits for them (`spawn_shards(count)` + `ShardSupervisor::wait()`)
- `spawn_shards(count)` returns a `ShardSupervisor` for status inspection and shard control; since 3.0 it returns as soon as the shard tasks exist, while later identify waves are still queued, so the supervisor owns and supervises every shard from the start
- since `2.1`, `spawn_shards(...)` fetches `/gateway/bot` and uses the account's real `max_concurrency` for identify wave sizing (previously hardcoded to 1); since 3.0 it also takes the daily session-start budget from it, and a failed request is returned as an error instead of starting without budget protection (unless `ClientBuilder::identify_gate(...)` sets a gate of your own)
- since 3.0 all shards of one client share a built-in IDENTIFY gate (bucket pacing and the session-start budget for every IDENTIFY, re-IDENTIFYs included); see [In-Process IDENTIFY Coordination](#in-process-identify-coordination-30)

## Primary Types

- `ShardConfig`, `ShardInfo`, `ShardIpcMessage`, `ShardRuntimeStatus`
- `ShardingManager`, `ShardMessenger`, `ShardSupervisor`
- `discordrs::sharding::process::{ProcessShardManager, ProcessShardManagerConfig, SessionStartBudget, ShardChildProcess}` (multi-process)
- `IdentifyGate`, `IdentifyPermit`, `IdentifyOutcome` and `GatewayConnectionConfig::identify_gate(...)` (3.0): a hook that paces every fresh IDENTIFY of a gateway client

## Multi-Process Sharding

`discordrs::sharding::process` is the discord.js `ShardingManager` equivalent. A parent process:

- spawns one child process per shard group;
- respawns crashed children with exponential backoff: base `respawn_delay`, capped at five minutes, reset after 60s of uptime, bounded by `max_respawns`;
- grants every IDENTIFY attempt per `max_concurrency` bucket and within the daily session-start budget, across all children;
- exchanges framed JSON lines over child stdio.

By default children re-execute the current binary. `ShardChildProcess::from_env()` returns `Some` only in child mode, so one binary serves both roles:

```rust
use discordrs::sharding::process::{
    ProcessShardManager, ProcessShardManagerConfig, ShardChildProcess,
};
use discordrs::{gateway_intents, Client, GatewayConnectionConfig};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), discordrs::DiscordError> {
    let token = std::env::var("DISCORD_TOKEN")?;

    if let Some(child) = ShardChildProcess::from_env() {
        // Child mode. Serve broadcasts first; earlier requests get null.
        child.serve(|request| async move { json!({ "echo": request }) })?;
        // Every fresh IDENTIFY of these clients waits for the parent's grant.
        let gateway = GatewayConnectionConfig::default().identify_gate(child.identify_gate());
        for &shard_id in child.shard_ids() {
            // Add .event_handler(...) and call child.notify_shard_ready(shard_id)
            // from its READY event.
            let builder = Client::builder(&token, gateway_intents::GUILDS)
                .gateway_config(gateway.clone())
                .shard(shard_id, child.shard_count());
            tokio::spawn(builder.start());
        }
        child.shutdown_signal().await;
        return Ok(());
    }

    // Parent mode: spawn_auto reads the shard count, max_concurrency, and the
    // session-start budget from /gateway/bot.
    let manager =
        ProcessShardManager::spawn_auto(token, ProcessShardManagerConfig::default()).await?;

    // Request/response across all children — the broadcastEval /
    // fetchClientValues equivalent.
    let stats = manager.broadcast(json!({ "op": "stats" })).await?;
    for (shard_id, value) in stats {
        println!("shard {shard_id}: {value}");
    }

    // shutdown() is the completion API: it returns once every child is gone,
    // and errors if one could not be confirmed stopped.
    manager.shutdown().await?;
    Ok(())
}
```

The full example is `examples/process_sharding_bot.rs` (`cargo run --example process_sharding_bot --features sharding`).

### Configuration

| `ProcessShardManagerConfig` field | Default | Meaning |
|---|---|---|
| `total_shards` | `None` | Required by `spawn`; `spawn_auto` fills it from `/gateway/bot`. |
| `shard_list` | all shards | Subset of shard ids this manager runs. |
| `shards_per_process` | `1` | Shards per child process. |
| `respawn`, `respawn_delay`, `max_respawns` | `true`, 5s, unlimited | Respawn policy (exponential backoff, capped at 5 min). |
| `program`, `args`, `extra_env` | current exe | Custom child binary and its arguments and environment. |
| `max_concurrency` | `None` | IDENTIFY buckets (`shard_id % max_concurrency`). `spawn_auto` reads it from `/gateway/bot`; `spawn` assumes `1`. |
| `session_start_budget` | `FromGateway` | Daily session-start budget source: `FromGateway` (needs `spawn_auto`), `Fixed(SessionStartLimit)`, or `Unchecked` (explicit opt-out). `spawn` rejects `FromGateway` because it has no token. |
| `identify_interval` | 5s | Spacing between IDENTIFYs in one bucket, measured from the previous attempt's reported send. |
| `identify_permit_timeout` | 30s | A child holding a permit longer than this is killed; the bucket is reused only after its exit is confirmed. |
| `spawn_stagger` | `0` | Delay between child *process* starts. Only needed for custom children that never request permits. |
| `broadcast_timeout` | 10s | Per-child answer deadline for `broadcast`; children cancel the handler after the same time. |
| `shutdown_timeout` | 10s | Total time a child gets to exit after shutdown before it is killed. |
| `max_ipc_frame_bytes` | 1 MiB | Largest IPC line in either direction (minimum 4 KiB). |

### IDENTIFY coordination

Discord allows one IDENTIFY per bucket every 5 seconds, where the bucket is `shard_id % max_concurrency` ([Gateway: max concurrency](https://docs.discord.com/developers/events/gateway#max-concurrency)), and a daily number of session starts. The parent keeps one FIFO queue per bucket plus the session-start budget, shared by every child and every respawn:

- **Every fresh IDENTIFY goes through the parent.** With `GatewayConnectionConfig::identify_gate(child.identify_gate())`, the gateway client asks for a permit before each connection that will IDENTIFY. That covers the first connect and every fresh session after INVALID_SESSION, close codes 4007/4009, or a reconnect before READY. RESUME needs no permit. READY handlers play no part in pacing.
- **The client reports each outcome.** It sends `Sent` right after the IDENTIFY write (also when the write fails or is cancelled midway), or `Abandoned` when the attempt ended before the write, such as a connect failure or a close before HELLO.
- **The next grant waits for that outcome.** After `Sent`, the next grant in the bucket comes `identify_interval` later. After `Abandoned`, it comes at once and the session start is refunded.
- **The permit is taken before dialing.** This client heartbeats only after IDENTIFY/RESUME, so a queue wait after HELLO could outlast `heartbeat_interval`. The permit therefore also covers connection setup. Setup (TCP/TLS connect, WebSocket upgrade, and HELLO) is bounded to 15s (since 3.2 for every connection, RESUMEs included, and a shutdown or handoff request is served during it); with a permit held, an attempt that runs over is abandoned and retried with backoff.
- **Waiting for a permit never blocks shutdown.** The client keeps serving its control channel while it waits, even until a budget reset: `Shutdown` ends the wait at once. Other commands are kept for the connection in a bounded backlog: presence updates coalesce to the newest one, and beyond 32 other commands (member/channel requests, raw payloads) the oldest is dropped with a warning naming only the command kinds and counts, because its caller has most likely timed out by then. The same bounded backlog applies during reconnect backoff, and a backlog that grew larger earlier is trimmed when the wait begins.
- **The parent keeps the bucket until it hears the outcome or the child is confirmed dead.** A child that holds a permit longer than `identify_permit_timeout` is killed. The bucket is reused only after its exit is confirmed, so a stalled child can never IDENTIFY next to the shard that got its slot. A child that dies while holding a permit counts as having sent its IDENTIFY.
- **The session budget is counted conservatively.** Every grant takes a reservation, settled once when the attempt is reported: `Abandoned` refunds it, and the refund immediately wakes a grant that is waiting for budget.
  - When the budget runs out, grants pause until a refund or the reset. At the reset, `spawn_auto` reads `/gateway/bot` again and counts every reservation still outstanding (granted in any bucket but not yet reported) against the new window. It also counts reservations reported as sent while that request was in flight, since Discord may have computed its numbers before those IDENTIFYs. The count may go negative, so a later refund cannot create a start Discord does not have.
  - A failed refresh keeps grants paused and retries with backoff; it never assumes a reset.
  - With `Fixed(..)`, there is nothing to refresh from, so after the reset the budget is assumed to be back at `total` minus outstanding reservations.
  - The count is per manager: several managers sharing one token must split the budget themselves.
- **Custom gateways** can use `IdentifyGate` directly. `child.acquire_identify(shard_id)` returns an `IdentifyPermit`: call `finish(IdentifyOutcome::Sent)` after writing IDENTIFY, and dropping it reports `Abandoned`.

`ClientBuilder::identify_gate(gate)` is a shorthand for setting the gate on `ClientBuilder::gateway_config(...)` as shown above.

### IPC framing

Every protocol message is one line, `@discordrs-ipc/1 ` followed by a JSON object (`IPC_FRAME_PREFIX`), sent over the child's stdin (parent → child) and stdout (child → parent).

- Lines without the prefix are ordinary output. The parent logs them at INFO (`child stdout: ...`) instead of failing, so a stray `println!` does not corrupt IPC.
- Lines longer than `max_ipc_frame_bytes` are discarded while they stream in, so a child cannot make the parent buffer unbounded memory. Invalid UTF-8 is tolerated.
- A broadcast payload that would exceed the limit makes `broadcast` return an error before anything is sent. A child response that would exceed it is replaced with `null` by the child.
- Both sides write through a dedicated writer. Control frames (grants, releases, shutdown) go first and are never dropped; the protocol bounds them to one grant per shard plus one shutdown. Data frames have bounded queues that reject new frames when full. No async task or destructor waits on a pipe: a parent that stops reading child stdout cannot block the child's Tokio runtime or its stdin reader. If a grant can no longer be delivered because the child's stdin is gone, the parent kills that child instead of leaving it waiting.
- Child stderr is inherited unchanged and remains the right place for logs. Child code must **not** read stdin, because the IPC reader owns it. Frames are written by a background thread, so a child that exits right after `log(...)` can lose that line.
- Writes that bypass Rust's stdout lock (C libraries, subprocesses sharing fd 1) can tear one frame. That frame is dropped and its broadcast entry reads `null`.

Why stdio framing instead of an extra pipe fd or a socket: it needs no `unsafe` fd plumbing, works the same on Windows and Unix and with any child program (including shell scripts), opens no local listening endpoint, and keeps stdin EOF as the "parent went away" signal.

| Direction | `op` | Fields | Meaning |
|---|---|---|---|
| parent → child | `shutdown` | | Stop gracefully; stdin closes right after. |
| parent → child | `request` | `id`, `payload` | Answer with `response` carrying the same `id`. |
| parent → child | `identify_grant` | `shard_id`, `lease` | The shard may send one IDENTIFY now. |
| child → parent | `ready` | `shard_id` | The shard is connected (status only). |
| child → parent | `response` | `id`, `payload` | Broadcast answer. |
| child → parent | `log` | `message` | Logged through the parent's `tracing`. |
| child → parent | `identify_request` | `shard_id` | Queue for a permit (one outstanding per shard). |
| child → parent | `identify_release` | `shard_id`, `lease`, `sent` | The attempt is over; `sent: false` means IDENTIFY was never written (default `true`). |

Children also receive `DISCORDRS_SHARD_IDS`, `DISCORDRS_SHARD_COUNT`, `DISCORDRS_SHARD_IPC=1`, `DISCORDRS_SHARD_IPC_MAX_FRAME`, and `DISCORDRS_SHARD_IPC_REQUEST_TIMEOUT_MS`.

### Broadcast handlers

`child.serve(handler)` runs at most 32 handlers at once (`DEFAULT_MAX_CONCURRENT_REQUESTS`). Requests beyond that are answered with `null` before any task is spawned. Running handlers are tracked: each is cancelled when the parent's `broadcast_timeout` passes, since its answer could no longer be used, and all of them are cancelled when shutdown arrives. `serve` can be called once and needs a Tokio runtime.

### Lifecycle

- `manager.children()` reports per-child `ChildStatus` (`Starting` → `Ready`/`Running` → `Respawning`/`Stopped`).
- `manager.shutdown().await` is the completion API. It sends the `shutdown` op ahead of any queued data, closes stdin, keeps reading the child's stdout, and kills the child when `shutdown_timeout` (measured from the shutdown request) passes. A child that stopped reading stdin cannot delay this, and it cannot stall `broadcast` either: a full data queue answers `null` immediately.
- `manager.wait()` and `manager.shutdown()` share one completion state, so any number of concurrent callers all return only after every child monitor has finished, with the same result. The result is `Ok` only if every child ended by request (or, with respawn disabled, by a successful exit). Otherwise the error lists each terminal outcome:
  - a child that gave up after `max_respawns`;
  - an unsuccessful exit with respawn disabled;
  - a spawn failure;
  - a failed `child.wait()`, after which the child is killed and its exit confirmed before its IDENTIFY buckets are returned;
  - a killed child whose exit could not be confirmed (its permits then stay held until it does exit);
  - a panicked monitor, or a runtime that shut down underneath one.
- Broadcast entries are `null` for children that are down, respawning, not yet serving, at their handler limit, or not answering within `broadcast_timeout`.
- Children observe shutdown through `shutdown_signal()`, which also resolves when stdin closes. That stops *cooperative* children, the ones awaiting the signal. A child that ignores it is killed by the parent's shutdown deadline, but if the parent process itself dies abruptly, nothing kills such a child.
- Dropping the manager without `shutdown()` stops the children in the background only while the Tokio runtime keeps running. Otherwise `kill_on_drop` is the only cleanup.
- `child.log(...)` sends log lines to the parent; `child.notify_shard_ready(id)` / `child.notify_ready()` report readiness.
- Testing note: the strongest lifecycle tests (stalled stdin, kill and reap, IDENTIFY spacing across processes) use `/bin/sh` children and run only on Unix; the cross-process timing tests run only on Linux.

## Behavior

- initial shard boot is queued instead of identifying every shard at once; queued shards report `ShardRuntimeState::Queued`
- identify waves are sized by the `max_concurrency` value Discord reports for the bot account
- shutdown can be awaited with `shutdown_and_wait()` or `wait_for_shutdown(timeout)`
- reconnect backoff is interruptible, so shutdown does not wait for a long sleep to finish
- supervisor events are a bounded history for observers (3.0). `publish` updates the shard's status at once, so `statuses()` is always current. Up to 256 events per shard then wait for `drain_events()` / `poll_event()`. When more arrive before they are drained, the oldest are dropped and the next drain starts with `ShardSupervisorEvent::Lagged { shard_id, missed }`. Draining no longer applies events a second time, so `statuses()` cannot roll back to an older state, and memory stays bounded when nothing drains. Runtimes registered with `attach_runtime` keep the caller's own channel, whose events update the status when they are received.
- a shard whose runtime has ended (every publisher dropped, for example after it published `Stopped`) does not break `drain_events()` (3.0). Its remaining events are returned with everyone else's, it is skipped afterwards, and its last status stays in `statuses()`. `drain_events()` returns an error only when there is no event to return and every registered shard has ended, so a loop that drains until an error stops after the last shard's final events. `poll_event(shard_id)` / `ShardRuntimeHandle::try_recv_event()` still report "event channel disconnected" for an ended shard once its queue is empty. Before 3.0 the first ended shard made every `drain_events()` call fail and discard the events it had already taken, from that shard and from the others.

### In-Process IDENTIFY Coordination (3.0)

Shards started by `spawn_shards` / `spawn_auto_shards` (and `start_shards` / `start_auto_shards`) share one built-in `IdentifyGate`. It uses the same per-bucket queues and session-start budget accounting as `ProcessShardManager` (see [IDENTIFY coordination](#identify-coordination)), without IPC:

- **One IDENTIFY per bucket every 5 seconds**, where the bucket is `shard_id % max_concurrency`, measured from the previous IDENTIFY in that bucket. This covers the first connect and every re-IDENTIFY after INVALID_SESSION, close codes 4007/4009, or a reconnect before READY. Shards in different buckets do not wait for each other.
- **Every IDENTIFY takes one session start** from the budget read from `/gateway/bot`. An attempt that ends before IDENTIFY is written (connect failure, close before HELLO) gives its start back.
- **An IDENTIFY the budget cannot cover waits for the reset.** At the reset, `/gateway/bot` is read again; a failed read keeps the wait going and retries with backoff. If the budget is below the shard count at startup, the shards that fit start right away, the rest wait for the reset, and a warning says how many. `Shutdown` ends a wait at once.
- **The budget is counted per client.** Several clients or processes on one token must split it themselves; `ProcessShardManager` keeps one budget for all of its children.
- **A gate of your own replaces the built-in one.** With `ClientBuilder::identify_gate(...)` (or a gate on `ClientBuilder::gateway_config(...)`), that gate is responsible for both limits.

Before 3.0 only the first boot was staggered (waves of `max_concurrency`, 5 seconds apart) and each shard spaced only its own IDENTIFYs, so shards of one bucket that lost their sessions together identified at the same moment and Discord rejected them. The budget was looked at once: `remaining == 0` delayed the whole boot, but with one start left and two shards both identified, `spawn_shards` ignored the budget, and re-IDENTIFYs were never counted. Discord resets the bot token when the budget is exceeded.

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
