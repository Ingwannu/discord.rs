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
RUSTDOCFLAGS="-D warnings" cargo doc --all-features --no-deps
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

## Model Footprint Report

`tests/id_footprint.rs` measures `size_of` for hot models and, with a counting global allocator in its own test binary, the allocations, retained heap bytes, and time of decoding a representative `MESSAGE_CREATE` and `GUILD_CREATE`. It is a report, not a gate (`#[ignore]`d; a small companion test keeps its payloads decodable). Run it single-threaded so the two tests' allocations do not mix:

```text
cargo test --release --all-features --locked --test id_footprint -- --ignored --nocapture --test-threads=1
```

The payloads only use fields that exist before and after a model change, so checking out the previous commit and running the same command gives the comparison. Allocation counts and bytes are deterministic; timings are indicative only. The phase 3 series 2 step A numbers are in `docs/design/3.0-types-events-requests.md` §1 (step B left them unchanged: markers add no bytes); the series 3 numbers (discriminants as `u8` newtypes, the interned presence status) are in §2 "시리즈 3 결과".

`tests/interaction_decode_footprint.rs` is a gate, not a report: it counts the allocations of `decode_interaction` on the perf report's 3.9 KB chat-input `INTERACTION_CREATE` (`tests/fixtures/perf/interaction_create_chat_input.json`), in Discord's key order and with `data` first, and fails above a third of the pre-phase-4 baseline (652 allocations in `decode_event`). It has one test function because the counting allocator is process-wide. `--nocapture` prints the other decode paths, timings, and the interaction model sizes:

```text
cargo test --release --all-features --locked --test interaction_decode_footprint -- --nocapture
```

## Performance Report

`tests/perf_report/` measures the paths that decide RAM and speed: the gateway inbound pipeline from frame text to the `Event` a handler receives, the in-memory cache, collector fan-out, and outbound request bodies. For every scenario it reports allocations, bytes allocated, bytes retained, peak live heap, and the median time, and it records the RSS of a bot process around a storm of large `GUILD_CREATE`s. Like the footprint report it is `#[ignore]`d and needs a release build (about 26 s):

```text
cargo test --release --all-features --locked --test perf_report -- --ignored --nocapture --test-threads=1
python3 ci/perf_diff.py before.json after.json            # change per metric; ~ marks noisy metrics
# Before merging phase 4 work: no deterministic regression against the committed baseline,
# same inputs, and (when claiming a checkpoint) its gates met.
python3 ci/perf_diff.py docs/design/perf/23775d1.json target/perf-report.json --deterministic-only --strict --fail-on-regression 2
python3 ci/perf_diff.py --gates ci/perf-gates.json --checkpoint B --strict target/perf-report.json
```

- Each scenario prints a `perf-report: {json}` line, and the run is written to `target/perf-report.json` (`DISCORDRS_PERF_REPORT=path` overrides). `ci/perf_diff.py` (standard library only, tested in `ci/tests/test_perf_diff.py`) reads either form.
- `inbound.*` replays the private pipeline steps with public API (a copy of the read loop's shallow envelope, `Event::decode`, `CacheHandle`, `Arc<Event>`), step by step; `e2e.*` runs the real `Client` in a child process against a fake gateway in another, so its counts and RSS belong to the bot alone. In every event type the two differ by exactly the 3 allocations the handler lane adds, which is how the replica is checked against the private code. When you change `gateway/client.rs`, `gateway/bot/pipeline.rs`, `apply_cache_updates`, or collector publishing, update the matching step in `tests/perf_report/inbound.rs`.
- Allocation counts, bytes, retained, and peak are deterministic for the replica, cache, fan-out, outbound, and storm scenarios (identical across runs and across rustc 1.97.1 and 1.99.0); times, RSS, and end-to-end numbers are not. Timed iterations run with counting switched off, because counting costs about 65 ns per allocation. The phase 4 gates have their own scenarios: `storm.guild_create_25` (live heap after 25 large guilds, journal and cache shares), `collectors.payload_copies` (extra copies 63 non-matching collectors cause), and `layout.size_of` (`Event` and `EventKind` sizes, empty `CollectorHub` heap).
- `perf_diff.py` filters (`--scenario`, `--metric`) also decide what can fail; `--strict` fails on changed inputs or fixtures (`meta.fixtures_digest`) and on missing scenarios or gate metrics; `--gates ci/perf-gates.json --checkpoint B|C` checks the phase 4 caps, including the caps on the metrics that are recorded exceptions to the +2% regression rule (each with the baseline value and the reason in its `note`).
- The non-blocking `perf-report` workflow runs the report for each PR and its base on the same runner and posts the comparison and gate status as the job summary; a deterministic regression over 2% is a warning, never a failure. Why it does not gate yet, and how it could, is in `docs/design/perf-baseline.md` §10.
- The current baseline (`main` at `23775d1`, committed as `docs/design/perf/23775d1.json`), the earlier `706b7ae` one, the metric definitions, and the hot spots phase 4 targets are in `docs/design/perf-baseline.md`.

In unit tests, models without `Default` are built with `T::fixture()` (`src/model/fixtures.rs`, `#[cfg(test)]`) and IDs with `crate::model::test_id(n)`, which infers the marker from context. Integration tests and examples use the public API instead (`"1".parse()?`, `GuildId::try_from(1)?`, or JSON).

## Documented Example Payloads

`src/model/doc_examples.rs` round-trips Discord's documented example payloads (channels, guild, messages, invite, webhooks, stickers, monetization objects, user, application, commands, auto moderation, presences, activities, interactions, and every component example) through the typed models: everything the model writes must have been in the example (or be an empty default), no discriminant key may be dropped, and a second round trip must write the same JSON. Whole interactions (the documented command interactions, message component interactions, and modal submits) also go through `decode_interaction` in Discord's key order, with `data` moved first, and from a `Value`, and must agree. It also checks, per converted discriminant field, that an unknown value fitting the storage survives and one past it fails to decode.

The payloads live in `src/model/doc_examples/payloads.rs`, generated from a discord-api-docs checkout at the revision the crate targets (`3714862`). Regenerate after bumping that revision and review the diff:

```text
python3 ci/doc_examples.py --docs <discord-api-docs clone>
```

## Build Time

Measured 2026-10-11 on `main` at `a4fc664` (x86_64, `CARGO_BUILD_JOBS=8`, clean target, debug
profile, two runs each):

| Build | `discordrs` unit | frontend / codegen | wall |
| --- | --- | --- | --- |
| `cargo build` (default features) | 16.1–16.9 s | ~14 s / ~2.4 s | 28 s |
| `cargo build --all-features` | 26.9–28.1 s | ~21 s / ~6.2 s | 48–50 s |
| `cargo test --no-run --lib --all-features` | 57–59 s | | 80–84 s |

The crate's own time is mostly the compiler frontend: type checking and borrow checking of about
10,000 function bodies. A per-item profile of the all-features lib puts serde derive output first
(about 5.8 s of single-thread frontend time), then the gateway runtime (1.6 s) and the REST
transport plus endpoints (1.4 s). Codegen is the smaller part, so changes that only remove LLVM IR
move the lib build by little; they matter more for the test build and for downstream crates, which
instantiate the generic REST helpers for every endpoint they call.

Rules for new code:

- **REST: thin shims around one transport core.** `RestClient::execute` (`src/http.rs`) is the
  only request loop (route gate, rate limits, replay policy, token scrubbing) and is not generic,
  so it is compiled once. Endpoints encode their body into an `EncodedBody` (`json_body`,
  `multipart_body`, the named-file and sticker forms, or `NO_BODY`) and call a shim with their
  manifest `RouteId`: `send` (decode into `T`), `send_optional` (`None` for 204), `send_unit`,
  `send_bytes`, and `send_bearer` / `send_bearer_unit` for routes authorized with a caller's
  OAuth2 token. Bodies are encoded once with `serde_json::to_vec` (`body::encode_json`, no
  `Value` tree) and every attempt sends the same bytes. The shims are generic over the response
  type only, run no `?`, and hand the awaited `Result` to `decode_response`. New endpoints add a row to `src/http/routes.rs` and call these
  shims; they do not add another generic `async fn` around the loop. A shim cost about 160 LLVM
  IR lines per `(response, body)` type pair, down from about 275, before body encoding moved to
  the call site. For a crate that calls 150 endpoints this
  was 2.6% less IR and a 2.6% smaller stripped release binary; its debug compile time did not
  change measurably.
- **One deserializer for models (phase 4 B follow-up).** Each model's `Deserialize` impl is
  compiled once per deserializer type, so every path that reads models uses serde_json's `&str`
  reader (`serde_json::from_str`): gateway dispatches from their `d` text, REST responses
  (`body::decode_json_body` reads the body as UTF-8 text), webhook events (the `Value` is written
  back to text first), and the voice gateway (frames stay text). Rarely sent, mostly lenient
  dispatches read one borrowed field map instead of a derived wire struct each; hot dispatches keep
  derived structs. Measured on the all-features lib dev build with
  `cargo llvm-lines --lib --all-features`: 1,147,993 IR lines on main `23775d1`, 1,502,341 after
  phase 4 B, 1,371,082 after the follow-up (`085e921`, +19.4% over main). Most of the remaining
  `Value`-reader code (93.7k lines) is the interaction parser's second copy of `Message`, `Member`,
  `User`, `Channel`, and friends, which goes away when the parser decodes from text; the rest of
  the difference is the text reader's larger per-struct code and the gateway-only payloads. REST
  error messages now carry serde_json's position, and a duplicated key in an object decoded into a
  struct is an error; `decode_json_body_matches_the_previous_decoder_except_the_documented_changes`
  pins both. The per-category breakdown is in the "4단계 B 후속" record in
  `docs/design/3.0-types-events-requests.md`.
- **Erase long-lived task types.** `tokio::spawn::<F>` compiles its task harness four times per
  future type, about 9,000 IR lines. Spawn long-lived `()` tasks (one per player, track,
  connection, or child process) through `crate::task::spawn_erased`, which shares one instance.

Known hotspots, all-features lib (`cargo llvm-lines --lib --all-features`, 950k lines in total):

| Hotspot | Size | Note |
| --- | --- | --- |
| serde `Deserialize` through `Value` | ~138k lines, one set per model | inherent while models decode from `Value`; `visit_seq` (struct-from-array, never used for Discord JSON) is ~32k of it, and serde derive always emits it |
| gateway runtime spawns (`gateway/bot/pipeline.rs` ×5, `gateway/bot/backend.rs` ×1) | ~55k lines | can switch to `spawn_erased` (widen the `task` module's `cfg` to `gateway`); all but one ~9k instance would go |
| `voice_runtime::connect` task (returns `Result`) | ~9k lines | the only spawn with that output type, so erasing it alone saves nothing |
| REST endpoint `async fn` bodies | ~230 lines each with drop glue, per endpoint and argument type | public `async fn` API; instantiated downstream only for the endpoints a crate calls |
| `gateway::bot::spawn_shard_supervisor` | 0.38 s of type checking | the largest single item in the frontend profile |

How to measure:

```bash
cargo build --locked --all-features --timings   # target/cargo-timings/: unit time, frontend/codegen split
cargo install cargo-llvm-lines --locked
cargo llvm-lines --lib --all-features --locked | head -40
cargo llvm-lines --lib --profile test --all-features --locked | head -40   # where the REST shims are instantiated
# Local analysis only (nightly flags on a stable toolchain): per-query frontend time.
cargo install --git https://github.com/rust-lang/measureme --locked summarize
RUSTC_BOOTSTRAP=1 CARGO_INCREMENTAL=0 cargo rustc --lib --all-features --locked -- -Z self-profile=/tmp/prof
summarize summarize /tmp/prof/*.mm_profdata | head -40
```

Other builds share the machine in CI and locally, so compare at least two runs per side and report
the spread; differences under about 1 s on the lib unit are noise.

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
- `feature-isolation` runs `clippy --all-targets --no-default-features --features <f>` for each
  feature alone (and for no features). The other jobs build only the default and the full set,
  where a missing `cfg` or an import used by one feature can hide. It covers every target, so test
  helpers and imports must carry the same `cfg` as the tests that use them (for example the
  gateway tests' fake-gateway helpers are `cfg(any(feature = "cache", feature = "sharding"))`);
  examples are skipped when their `required-features` are off.

## Compile-Fail Tests (trybuild)

`tests/compile_fail.rs` checks that misuse of the 3.0 typed primitives, async-trait contracts, and the Component Embed model stays a compile error:

- passing a `ChannelId` where a `GuildId` is required;
- implicit `u64` → ID conversion;
- `GuildId::default()`, or `#[derive(Default)]` on a struct that stores an ID;
- an integer where an open discriminant (`ChannelType`, `ButtonStyle`) is expected, `Default` on a discriminant (or a struct storing one), and the 2.x tuple spelling `ActivityType(0)` / `.0`;
- `Patch::null()` on a non-nullable `Patch<bool>`;
- an `async fn` impl of a public async trait whose future is not `Send` (`CacheBackend`, the async trait available with default features, which this test uses);
- a custom-ID `ButtonBuilder` in a Component Embed preview row, or `custom_id` on a `PreviewLinkButton` (link buttons only).

The positive side of that contract is `tests/async_traits.rs`: it implements every public async trait from outside the crate with plain `async fn` (no attribute macro) and runs the futures on spawned Tokio tasks, so a signature that `async fn` can no longer implement fails the build.

The runtime side of the Component Embed contract is `tests/component_embed.rs`. Its fixtures in `tests/fixtures/component_embed/` are the spec's own example documents (copied verbatim from discord-api-docs `developers/link-previews/component-embeds.mdx`) plus the exact bytes this crate emits for them (`*.golden.*`). Every limit is tested at its boundary and one past it, and the 3,000-byte limit is tested with characters that grow when escaped. If an encoder change alters the output on purpose, regenerate the golden files and review the diff, as with the `.stderr` snapshots.

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
  links and backslashes stripped and `{params}` normalized to `{}`, against the rows of the route
  manifest in `src/http/routes.rs`. Each row is one Discord operation with a stable `RouteId`:
  method, path template in the docs' spelling, authorization, query parameters, request and
  response types, and the client methods that send it. The REST transport executes the same rows
  (endpoints pass their `RouteId`; the method and the `Authorization` header come from the row),
  so the report and the runtime cannot disagree about which routes exist.
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

### Route manifest

One row per line in `src/http/routes.rs`:

```text
GetGuild: GET "/guilds/{guild.id}" bot, query [with_counts], body () => Guild, api [get_guild, get_guild_with_query];
```

`auth` is `bot`, `bearer` (caller-supplied OAuth2 token), `token` (webhook or interaction token in
the path, no header), `none` (public route), or `client` (OAuth2 client credentials,
`OAuth2Client`). `api [..]` lists `RestClient` methods and `oauth2 [..]` lists `OAuth2Client`
methods. An optional `was [old => new, ..]` records 3.0 renames; the REST migration table is built
from it. A line inside the table that is not a row is an error, never a skipped route.

Under `cfg(test)` the rows also compile into a check that every request and response type exists,
and a `MANIFEST` table the route goldens use.

### Allowlist

Documented mismatches live in `ci/api-drift-allowlist.toml`. Every entry needs a `reason` (15+
characters); entries without one are rejected, because a reasonless allowlist silently turns drift
detection off.

| Table | Use | Extra checks |
| --- | --- | --- |
| `ignored_route` | documented route deliberately not wrapped | reported once Discord drops it or the crate implements it |
| `crate_only_route` | crate route that is not a `<Route>` tag (OAuth2 token URLs) | reported once Discord documents it or the crate drops it |
| `non_dispatch_event` | receive-event row that is an opcode (`HELLO`, `RECONNECT`, `INVALID_SESSION`) | optional `evidence` |
| `ignored_event` / `crate_only_event` | event-name mismatches kept on purpose | same staleness checks as routes |

The `drift-tooling` CI job runs the unit tests and `--self-check` on every PR. `--self-check`
fails when the allowlist or the manifest no longer describes `src/`, with a message naming what to
edit:

- *`Id` lists `RestClient::x`, which does not exist*: a method was renamed or removed; update the
  row's `api` list (and add a `was` entry if it is a 3.0 rename).
- *`RestClient::x` is listed under `Id` but never sends `RouteId::Id`*: the method sends another
  route, or delegates to a method that does; fix the row or the call.
- *endpoint `RestClient::x` is not in the route manifest*: a new `pub async fn` on a client; add
  it to its row (or a new row), so the weekly report does not list the route as missing.
- *`x` is recorded as removed (`was`) but still exists without #[deprecated]*: delete the old
  method or deprecate it as a forwarding shim.
- *`Method::X`: endpoints name their route with `RouteId`*: an endpoint file builds a request
  with an HTTP method instead of a manifest route.
- *evidence `...` no longer appears in src/*: an event-table entry's evidence moved; update it.

### Request goldens

`src/http/tests/goldens.rs` sends one call per manifest route (and per method a row lists) to a
local recording server and renders the request: method, escaped path and query, `Authorization`
header, content type, and body bytes, with multipart boundaries replaced by `BOUNDARY`. The output
must equal `src/http/goldens/rest_routes.golden`, and each request must match its row's method,
template shape, and authorization kind. A route without a case, or a case calling a method its row
does not list, fails the test. After an intended change, regenerate and review the diff:

```bash
DISCORDRS_UPDATE_GOLDENS=1 cargo test --all-features --lib http::tests::goldens
git diff src/http/goldens/rest_routes.golden
```

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
