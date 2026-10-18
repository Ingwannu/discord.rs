# Live DAVE Validation

`tests/live_voice_dave.rs` is the release gate for claiming live DAVE/MLS interoperability. It is ignored by default because it requires a real Discord voice session and secret voice credentials.

It exercises the default path: `connect_voice_runtime_with_options` with the channel ID (what `Context::connect_voice_runtime_with_dave` uses), the runtime's own MLS session, a tone played through `AudioPlayer` (needs `voice-encode`), and `recv_voice_packet` decrypting what another member says.

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

Use a regular (non-Stage) voice channel, so that Discord negotiates DAVE. The channel must contain at least one other current DAVE-capable participant (an up-to-date Discord client) while the test runs: a client alone in a DAVE call has no MLS group and no media keys, and the test waits up to 90 seconds for one. That participant should listen for the tone and then **speak** (unmuted) during the receive window, which lasts 15 seconds unless `DISCORDRS_LIVE_VOICE_RECEIVE_SECS` says otherwise.

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

A successful run:

1. connects with DAVE negotiated (`negotiated DAVE protocol version 1`);
2. becomes a member of the call's MLS group (`joined the MLS group: epoch ..., voice privacy code ...`); the privacy code should match the one Discord clients show for the call;
3. plays three seconds of a 440 Hz tone with no `PlayerConnectionEvent::SendFailed`; **the participant must hear it**, which proves Discord clients decrypt the runtime's DAVE frames (the test cannot check that by itself);
4. decrypts at least one speech frame from another member (`decrypted speech frames per user: ...`), with the count of undecryptable packets and recoveries printed for the record.

Record the command output, and whether the tone was heard, with the release notes before changing DAVE support claims.

## Validation Records

- **2.0.0 (2026-05-02):** an earlier version of this harness, which drove MLS by hand from the raw `VoiceDaveState` mirror, completed DAVE transport negotiation, received the external sender package, sent an MLS key package, processed live MLS proposal/commit/welcome payloads, and completed the transition-ready flow. It sent and received no media.
- **Runtime-managed DAVE (after 3.0.0):** not run against live Discord yet.

## Failure Modes

- Missing env vars: the test fails before network I/O. This is expected and prevents false green validation.
- `no MLS group after 90s`: the runtime never received the group's keys. The printed status shows the DAVE protocol version and the last DAVE error. Without another DAVE-capable member in the channel this is expected; with one, keep the output (it is interop evidence) and check `last_error`.
- `the call did not negotiate DAVE`: the channel is a Stage channel, or the runtime advertised version 0.
- `no decrypted speech from another member`: nobody spoke during the receive window, or every frame failed to decrypt (see the printed undecryptable count).
- Websocket or UDP setup failure: refresh the voice `session_id`, `token`, and `endpoint`, then rerun.
- A rejected commit or welcome shows up as `recoveries` > 0 and a `last_error` naming the transition. Keep the output: it is useful compatibility evidence, but not a successful validation unless the release decision explicitly accepts it.
