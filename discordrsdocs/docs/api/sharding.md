# Sharding

Sharding is provided behind the `sharding` feature on top of the `gateway` runtime.

## Current Status

- the base runtime tracks shard info in `Context`
- `start_shards(count)` spawns shards and waits until every shard task finishes
- `spawn_shards(count)` returns a `ShardSupervisor` for status inspection and shard control
- since `2.1.0`, `spawn_shards(...)` fetches `/gateway/bot` and uses the account's real `max_concurrency` for identify wave sizing (previously hardcoded to 1), and each shard's IDENTIFY is paced to Discord's 1-per-5s-per-shard limit

## Primary Types

- `ShardConfig`, `ShardInfo`, `ShardIpcMessage`, `ShardRuntimeStatus`
- `ShardingManager`, `ShardMessenger`, `ShardSupervisor`

## Behavior

- initial shard boot is queued instead of identifying every shard at once; queued shards report `ShardRuntimeState::Queued`
- identify waves are sized by the `max_concurrency` value Discord reports for the bot account
- shutdown can be awaited with `shutdown_and_wait()` or `wait_for_shutdown(timeout)`
- reconnect backoff is interruptible, so shutdown does not wait for a long sleep to finish
