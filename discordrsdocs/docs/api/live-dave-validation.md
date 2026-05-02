# Live DAVE Validation

`tests/live_voice_dave.rs` is the release gate for claiming live DAVE/MLS interoperability. It is ignored by default because it requires a real Discord voice session and secret voice credentials.

Do not treat a normal `cargo test --all-features` run as live DAVE validation. The ignored test intentionally fails fast when required values are missing.

## Required Inputs

Collect these values from a bot process that joins a prepared voice channel:

| Variable | Source |
| --- | --- |
| `DISCORDRS_LIVE_VOICE_SERVER_ID` | Guild ID used in the Gateway voice state update |
| `DISCORDRS_LIVE_VOICE_USER_ID` | Bot user ID |
| `DISCORDRS_LIVE_VOICE_SESSION_ID` | `session_id` from the bot user's `VOICE_STATE_UPDATE` |
| `DISCORDRS_LIVE_VOICE_TOKEN` | `token` from `VOICE_SERVER_UPDATE` |
| `DISCORDRS_LIVE_VOICE_ENDPOINT` | `endpoint` from `VOICE_SERVER_UPDATE`; pass the host or `wss://...` URL |
| `DISCORDRS_LIVE_VOICE_CHANNEL_ID` | Voice channel ID used to create the DAVE session |

The `session_id`, `token`, and `endpoint` are short-lived. Capture them immediately after the bot joins the target voice channel and run the test in the same shell session.

The voice channel should contain at least one other current DAVE-capable participant while the test runs. A bot-only channel can complete transport DAVE negotiation (`dave_protocol_version` plus external sender) without producing the MLS proposal, commit, or welcome payloads required for full transition validation. Peer SSRC/user mappings are useful diagnostics, but the release gate is the MLS transition payload flow itself.

## Capture Pattern

In a Gateway bot, call `Context::join_voice(...)` or send the equivalent voice state update. Then log the bot user's `VOICE_STATE_UPDATE` and the matching `VOICE_SERVER_UPDATE` for the same guild.

Required pair:

1. `VOICE_STATE_UPDATE` for the bot user supplies `session_id`.
2. `VOICE_SERVER_UPDATE` for the same guild supplies `token` and `endpoint`.

If either event is stale, belongs to another guild, or was captured before the final voice channel join, discard the values and rejoin the voice channel.

The repository includes `examples/live_dave_capture_bot.rs` for this capture step. It joins the configured voice channel and prints the PowerShell environment block once it sees the matching bot voice state and voice server update:

```powershell
$env:DISCORD_TOKEN="bot-token"
$env:DISCORDRS_CAPTURE_GUILD_ID="123456789012345678"
$env:DISCORDRS_CAPTURE_CHANNEL_ID="345678901234567890"
cargo run --all-features --example live_dave_capture_bot
```

Leave the capture bot running while the live test runs. The printed voice credentials identify the active voice session, and stopping the Gateway-side bot can invalidate the session before the voice runtime test has finished. For one-shot scripts that only need the env block, set `DISCORDRS_CAPTURE_EXIT_AFTER_PRINT=1`.

Treat the printed `DISCORDRS_LIVE_VOICE_TOKEN` as a secret and use it immediately; Discord voice credentials are short-lived.

## Run

```powershell
$env:DISCORDRS_LIVE_VOICE_SERVER_ID="123456789012345678"
$env:DISCORDRS_LIVE_VOICE_USER_ID="234567890123456789"
$env:DISCORDRS_LIVE_VOICE_SESSION_ID="voice-session-id"
$env:DISCORDRS_LIVE_VOICE_TOKEN="voice-server-token"
$env:DISCORDRS_LIVE_VOICE_ENDPOINT="voice-region.discord.media:443"
$env:DISCORDRS_LIVE_VOICE_CHANNEL_ID="345678901234567890"

cargo test --all-features --test live_voice_dave -- --ignored --nocapture
```

## Expected Result

A successful run connects the voice runtime, observes DAVE external sender state, sends a DAVE MLS key package, processes live proposals or commit/welcome payloads, and sends either:

- `DAVE_PROTOCOL_TRANSITION_READY` when the transition is accepted, or
- `DAVE_MLS_INVALID_COMMIT_WELCOME` when Discord sends an MLS commit/welcome payload the local session rejects.

Record the command output with the release notes before changing future DAVE support claims.

## 2.0.0 Validation Record

The 2.0.0 release validation ran this harness against a real Discord voice session on 2026-05-02. The run completed DAVE transport negotiation, received the external sender package, sent an MLS key package, processed live MLS proposal/commit/welcome payloads, and completed the transition-ready flow.

## Failure Modes

- Missing env vars: the test fails before network I/O. This is expected and prevents false green validation.
- Timeout waiting for DAVE state: the voice session did not enter a DAVE transition during the wait window. If the last state already has `dave_protocol_version` and an external sender but no epoch, proposal, commit, or welcome payload, the transport-level DAVE negotiation reached Discord but the call did not start an MLS group transition. Keep another current DAVE-capable Discord client or bot in the same voice channel and rerun.
- Websocket or UDP setup failure: refresh the voice `session_id`, `token`, and `endpoint`, then rerun.
- Commit/welcome rejection: keep the output and DAVE payload stage. It is still useful compatibility evidence, but it is not a successful stable-validation result unless the release decision explicitly accepts that behavior.
