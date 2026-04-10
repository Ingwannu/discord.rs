# Sharding

Sharding is a planned runtime layer above the current `Client`.

## Current Status

- the base runtime already tracks shard info in `Context`
- the docs site now reserves a dedicated section for future shard orchestration APIs
- the target shape is a focused `ShardingManager` plus reusable WebSocket configuration primitives

## Intended Scope

- shard lifecycle management
- shard IPC
- shard-aware metrics and reconnect visibility

Until that lands, treat the current Gateway runtime as a single-shard core.
