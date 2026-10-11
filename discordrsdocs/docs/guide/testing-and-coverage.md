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

`src/helpers.rs` and the `src/http/` endpoint modules often validate tokens, application ids, and message ids
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

## Compile-Fail Tests (trybuild)

`tests/compile_fail.rs` checks that misuse of the 3.0 typed primitives stays a compile error:

- passing a `ChannelId` where a `GuildId` is required;
- implicit `u64` → ID conversion;
- `GuildId::default()`, or `#[derive(Default)]` on a struct that stores an ID;
- `Patch::null()` on a non-nullable `Patch<bool>`.

Each case is a file in `tests/ui/` with an expected `.stderr` that holds rustc's diagnostic text. That text changes between Rust releases, so the snapshots are kept for one toolchain only, the pinned `RUST_TOOLCHAIN` (currently 1.99.0). The test does nothing unless `DISCORDRS_TRYBUILD=1` is set. CI sets it only in the `rust-check` job (step "Compile-fail snapshots"). The `msrv` and `stable-canary` jobs still compile the test file, and the test then skips itself.

```bash
# Run locally, as CI does:
DISCORDRS_TRYBUILD=1 cargo +1.99.0 test --locked --test compile_fail
```

Regenerate the snapshots when `RUST_TOOLCHAIN` is bumped (the step fails with a diff), or after adding a case:

```bash
DISCORDRS_TRYBUILD=1 TRYBUILD=overwrite cargo +<pinned version> test --locked --test compile_fail
git diff tests/ui/   # review: each case must still fail for the intended reason
```

Review the regenerated `.stderr` files before committing. A snapshot that now shows a different error, such as a typo in the case file or a missing import, means the case no longer tests what its comment says.

## Discord API Drift Report

`ci/api_drift.py` compares the official
[discord-api-docs](https://github.com/discord/discord-api-docs) with the crate, and
`.github/workflows/api-drift.yml` runs it every Monday at 03:17 UTC and on manual dispatch, only
in `Ingwannu/discord.rs`.

What it compares:

- **REST routes**: every `<Route method="X">path</Route>` under `developers/` (except
  `developers/change-log.mdx`, which also quotes routes that were later removed), with markdown
  links and backslashes stripped and `{params}` normalized to `{}`, against the
  `Method::X` + path pairs in `src/http.rs`, `src/http/**/*.rs` (not `tests.rs`), and
  `src/oauth2.rs`. The scanner reads `"/literal"` and `format!("/literal/{}", ..)` arguments,
  one-hop `let path = format!(..)` bindings in the same function, and reqwest
  `.post(format!("{}/oauth2/..", base))` calls. `#[cfg(test)]` code, comments, and literals passed
  to other functions (for example `Some("/slack")`) never count.
- **Gateway dispatch events**: the first table and the level-4 headings of the "Receive Events"
  section of `developers/events/gateway-events.mdx` (`Guild Members Chunk` →
  `GUILD_MEMBERS_CHUNK`), against the `"EVENT_NAME" =>` arms in `src/event/decode.rs`.
- Both directions: routes/events Discord documents that the crate lacks, and crate routes/events
  Discord no longer documents.

Run it locally (Python 3.11+, standard library only):

```bash
git clone --depth 1 https://github.com/discord/discord-api-docs.git /tmp/discord-api-docs
python3 ci/api_drift.py --docs /tmp/discord-api-docs          # markdown report on stdout
python3 ci/api_drift.py --docs /tmp/discord-api-docs --format json
python3 ci/api_drift.py --docs /tmp/discord-api-docs --fail-on-drift   # exit 1 on findings
python3 ci/api_drift.py --self-check                           # what PR CI runs; no docs needed
python3 -m unittest discover -s ci/tests -v                    # parser tests on fixture MDX/Rust
```

Exit codes: `0` report produced (drift or not), `1` findings with `--fail-on-drift`, `2` the
extraction is broken (bad allowlist, missing docs page, or counts below the sanity floors in the
allowlist settings). Code 2 fails the weekly run on purpose: a parser that silently found zero
routes would otherwise report "no drift".

### Allowlist

Routes that are not a literal at the call site, and documented mismatches, live in
`ci/api-drift-allowlist.toml`. Every entry needs a `reason` (15+ characters); entries without one
are rejected, because a reasonless allowlist silently turns drift detection off.

| Table | Use | Extra checks |
| --- | --- | --- |
| `helper_route` | documented route built by a path helper (`global_commands_path`, `webhook_message_path`, `followup_webhook_path`, `interaction_callback_path`, `identity_profile_path`, ...) | `evidence` substrings must exist in non-test `src/`; each name in `functions` must still send that method with a helper-built path |
| `ignored_route` | documented route deliberately not wrapped | reported once Discord drops it or the crate implements it |
| `crate_only_route` | crate route that is not a `<Route>` tag (OAuth2 token URLs) | reported once Discord documents it or the crate drops it |
| `non_dispatch_event` | receive-event row that is an opcode (`HELLO`, `RECONNECT`, `INVALID_SESSION`) | optional `evidence` |
| `ignored_event` / `crate_only_event` | event-name mismatches kept on purpose | same staleness checks as routes |

The `drift-tooling` CI job runs the unit tests and `--self-check` on every PR. `--self-check`
fails when the allowlist no longer describes `src/`, with a message naming the entry to edit:

- *evidence `...` no longer appears in src/*: a path helper changed its format string or was
  renamed; update `evidence`.
- *`fn x` no longer sends Method::Y with a helper-built path*: the wrapper was renamed (for
  example a `_typed` suffix removed) or now uses a literal; update `functions`, or delete the
  entry if the route is now a literal.
- *unmapped helper-built call*: a new wrapper passes a helper-built path; add its function to the
  matching `helper_route` (or a new entry), so the weekly report does not list the route as
  missing.

### Weekly issue

The workflow writes the report to the run summary and an `api-drift-report` artifact, then
`ci/drift_issue.py` maintains **one** issue labelled `api-drift`:

- It only touches issues whose body carries the `<!-- api-drift-report -->` marker, never an issue
  a person labelled by hand.
- Drift: update the oldest open managed issue (or open one); add a comment only when the findings
  fingerprint changes, so unchanged drift does not notify anyone weekly. Newer managed duplicates
  are closed.
- No drift: close the managed issue with a comment. The next drift opens a new issue.
- The workflow has `contents: read` and `issues: write` only, and drift never fails it or any PR.

Offline dry run of the issue step:
`echo '[]' > /tmp/issues.json && python3 ci/drift_issue.py report.json report.md --dry-run --issues-json /tmp/issues.json`.

The workflow YAML was checked with `actionlint` 1.7.12 (including its shellcheck pass on `run:`
scripts) and by running each step's commands locally in order with `RUNNER_TEMP` and
`GITHUB_STEP_SUMMARY` set.
