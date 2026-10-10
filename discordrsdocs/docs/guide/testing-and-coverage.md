# Testing And Coverage

This guide records the local testing and coverage patterns that worked well while raising
`discord.rs` coverage.

## Goals

- Keep tests hermetic: no live Discord traffic.
- Prefer unit tests first, local harness tests second.
- Raise real `cargo llvm-cov` coverage, not only filtered CI coverage.

## Verification Commands

Run the full verification set after meaningful changes:

```bash
cargo test --all-features
cargo check --all-features --all-targets
cargo clippy --all-features --tests -- -D warnings
cargo fmt --all -- --check
cargo llvm-cov --all-features --summary-only --fail-under-lines 90.0
```

If `cargo llvm-cov` is not installed:

```bash
rustup component add llvm-tools-preview
cargo install cargo-llvm-cov
```

## High-Yield Test Seams

### Local HTTP Harness

`src/http.rs` exposes a test-only base URL seam via `RestClient::new_with_base_url(...)`.
Use it with a local `TcpListener` and scripted raw HTTP responses.

Best targets:

- REST wrapper methods that only differ by path, method, or body
- rate-limit state transitions
- auth-header behavior
- validation-before-network error paths

### Local WebSocket Harness

For `src/gateway/client.rs` and `src/voice_runtime.rs`, use:

- `tokio::net::TcpListener`
- `tokio_tungstenite::accept_async`
- scripted HELLO / READY / INVALID_SESSION / RECONNECT payloads

This is enough to cover identify, resume, reconnect, heartbeat, READY propagation, and voice
runtime handshake logic without using Discord.

### Validation-First Helper Tests

`src/helpers.rs` and parts of `src/http.rs` often validate tokens, application ids, and message ids
before making a request. These are cheap coverage wins and should be preferred over external mocks.

### Event Decode Tables

`src/event.rs` coverage moves fastest when you add tiny valid payloads per event variant instead of
building large shared fixtures.

### Runtime State Assertions

For `gateway/bot`, `cache`, `collector`, `sharding`, `voice`, and `voice_runtime`:

1. prefill state
2. apply one event or command
3. assert only the intended mutation
4. assert neighboring state stayed untouched

## Coverage Order That Worked Well

1. pure builders / bitfields / types / parser helpers
2. event decode tables
3. `http.rs` local server harness
4. `gateway/client.rs` local websocket harness
5. `gateway/bot.rs` stateful helper and dispatch tests
6. `voice.rs` and `voice_runtime.rs`
7. collector and sharding cleanup

## Things That Were Not Worth It

- real Discord network tests
- huge end-to-end fixtures when a minimal payload hits the same branch
- stable-toolchain coverage attributes such as `#[coverage(off)]`
- production refactors that exist only to move trivial coverage numbers

## Practical Notes

- Check `cargo llvm-cov --summary-only` often and start with the biggest files.
- CI coverage intentionally measures the full source tree without an `ignore-filename-regex`;
  lower the threshold only when the real total changes, not to preserve a badge number.
- Prefer existing `#[cfg(test)]` modules when the target is private helper code.
- Use harness tests only after direct unit tests stop being enough.
- Treat `gateway/bot.rs`, `http.rs`, and `interactions.rs` as the final expensive coverage tiers.

## Minimum Supported Rust Version (MSRV)

`Cargo.toml` declares `rust-version = "1.88"`. The value covers **every feature**
(`--all-features`), which is what Cargo's `rust-version` promises: on 1.88 the whole crate,
including `voice`, `voice-encode`, and `dave`, builds and passes its tests. Older toolchains are
refused by Cargo with "requires rustc 1.88" unless the user passes `--ignore-rust-version`.

How it was determined (2026-10-10, `main` at `f3b786c`, the committed `Cargo.lock`): first the
highest `rust-version`/edition of every dependency in the Linux build graph
(`cargo metadata` + `cargo tree --target x86_64-unknown-linux-gnu`), then real builds on each
candidate toolchain with `--ignore-rust-version`:

| Feature set | Minimum | What sets it |
| --- | --- | --- |
| default (`cache`) | 1.83 | `icu_*` 2.1 (via `url` → `idna`) declare 1.83; 1.82 is refused |
| `gateway`, `collectors`, `sharding`, `zstd-stream`, `interactions` | 1.85 | `base64ct` 1.8 (via `ed25519-dalek`) is edition 2024 |
| `voice` | 1.88 | `slice::as_chunks` in `src/voice/player.rs` (fails on 1.87) |
| `voice-encode` | 1.87 | `opus-rs` uses `is_multiple_of` |
| `dave` | 1.88 | `davey` uses let chains (fails on 1.87); `openmls` uses `is_multiple_of` |
| **all features (declared)** | **1.88** | 1.88.0 passed `check --all-targets` and the full test suites for default and all features |

CI's `msrv` job reads `rust-version` from `Cargo.toml`, installs that toolchain, and runs
`cargo check --all-targets` and `cargo test` for default and all features with `--locked`.
Because `rust-version` is set, clippy's `incompatible_msrv` lint also fails the pinned clippy jobs
(`-D warnings`) as soon as code uses a standard-library API newer than the MSRV, so the usual
signal arrives before the `msrv` job even runs.

Re-checking or raising the MSRV:

```bash
rustup toolchain install 1.87.0 --profile minimal
# Probe below the declared version without editing Cargo.toml:
cargo +1.87.0 check --all-targets --all-features --locked --ignore-rust-version
cargo +1.87.0 check --all-targets --locked --ignore-rust-version
# Confirm the declared version end to end, as CI does:
cargo +1.88 test --all-features --locked
cargo +1.88 test --locked
```

- Raising it is a deliberate change: edit `rust-version`, record the reason (the API or
  dependency that needs it) in `CHANGELOG.md`, and update the table above. Prefer an older
  equivalent when the new API is only a convenience (the `fetch_update` allowance in
  `examples/process_sharding_bot.rs` exists for that reason).
- The MSRV is verified against the committed `Cargo.lock`. If a `cargo update` pulls a
  dependency that needs a newer compiler, the `msrv` job fails on that PR: either raise the MSRV
  or keep the older version (`cargo update -p <crate> --precise <version>`;
  `CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS=fallback cargo update` makes Cargo 1.84+ prefer
  versions compatible with `rust-version`).

## CI Toolchains

The blocking `rust-check` and `coverage` jobs use the toolchain pinned in `RUST_TOOLCHAIN` at the
top of `.github/workflows/ci.yml` instead of the moving `stable` channel. On 2026-10-10, Rust 1.99
added clippy lints (`double_must_use`, `chunks_exact_to_as_chunks`) that turned every open PR red
without any code change. With a pin, new lints arrive in one deliberate PR.

- `stable-canary (non-blocking)` runs clippy on the latest stable with `continue-on-error`. When
  it goes red, bump `RUST_TOOLCHAIN` and fix the new lints in the same PR.
- A pinned toolchain is not preinstalled on runners, and `rustup set profile minimal` leaves out
  clippy and rustfmt, so the setup steps request `--component clippy,rustfmt` (or
  `llvm-tools-preview`) explicitly. Locally, `cargo +1.99.0 clippy` fails the same way if rustup
  auto-installed a bare `1.99.0`; use `rustup component add clippy --toolchain 1.99.0`.
- The `msrv` job is separate and follows `rust-version`, not the pin.
