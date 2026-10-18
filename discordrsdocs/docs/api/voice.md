# Voice

Voice is an optional runtime layer. It stays feature-gated so core Gateway, REST, and interaction code do not pay for voice dependencies.

## Enable

```toml
[dependencies]
discordrs = { version = "3.0.0", features = ["voice"] }

# PCM -> Opus encode/playback helpers
discordrs = { version = "3.0.0", features = ["voice", "voice-encode"] }
```

`voice` includes DAVE end-to-end encryption (`davey`/OpenMLS), which Discord requires in every voice channel except Stage channels. The `dave` feature still exists as an alias of `voice`, so `features = ["voice", "dave"]` keeps building. See [DAVE](#dave-end-to-end-encryption) for why it is no longer optional.

## Surfaces

- `VoiceManager`: tracks gateway voice state/server updates and local queue state.
- `connect_voice_runtime_with_options(config, VoiceConnectOptions::new(channel_id))`: connects the voice websocket, performs UDP discovery, selects protocol, waits for session description, and then runs the call's DAVE session (MLS group) itself. In a gateway bot, `Context::connect_voice_runtime_with_dave(guild_id, user_id)` uses it with the tracked voice state (options from `VoiceManager::runtime_options(guild_id)`): call `ctx.connect_voice(VoiceConnectionConfig::new(guild_id, channel_id))` and `ctx.join_voice(..)`, and once the bot's VOICE_STATE_UPDATE and the VOICE_SERVER_UPDATE arrived (in either order; `ctx.voice_runtime_config(..)` is then `Some`), connect. The context tracks only the bot's own voice state (the user from READY), so other members joining, moving, or leaving do not change it; `examples/music_bot.rs` shows the flow.
- `connect_voice_runtime(config)` and `Context::connect_voice_runtime`: the same handshake with 3.0.0's DAVE behaviour: it advertises DAVE but leaves the session to you (plain sends and receives refuse a DAVE call; use the `*_with_dave` methods and the raw commands).
- `recv_raw_udp_packet(...)`: receives raw UDP packets with parsed RTP metadata.
- `recv_voice_packet(...)`: returns the next Opus frame with transport encryption and, in a DAVE call, the speaker's DAVE encryption removed.
- `send_opus_frame(...)`: sends an already-encoded Opus frame (DAVE-encrypted in a DAVE call) as an encrypted RTP-size packet with managed sequence, timestamp, and nonce state. The sequence number and timestamp advance only once the datagram is sent, so a failed or cancelled send leaves no hole; every attempt uses a fresh nonce.
- `send_opus_frame_after_gap(...)`: the same for a frame that follows media that was never transmitted (for example frames a player dropped while the connection lagged); the RTP timestamp moves across the gap while the sequence stays contiguous. `send_opus_frame_with_dave_after_gap(...)` does the same with your own DAVE encryptor.
- `dave_status()`, `subscribe_dave_status()`, `is_playable()`, `wait_until_playable()`, `dave_verification_code(user_id)`: the DAVE session's state (`VoiceDaveStatus`), see [DAVE](#dave-end-to-end-encryption).
- `play_opus_frames(...)`: sends a small iterator of encoded Opus frames with simple duration pacing and speaking on/off commands.
- `PcmFrame`, `AudioSource`, `AudioMixer`, and `VoiceOpusEncoder`: behind `voice-encode`, encode 48 kHz stereo 20 ms PCM frames with `opus-rs` Audio mode before calling the existing Opus playback path.
- `VoiceOpusDecoder`: decodes Opus frames to interleaved `i16` PCM, using 48 kHz stereo by default for Discord voice.
- `VoiceDaveFrameDecryptor`, `VoiceDaveFrameEncryptor`, `send_opus_frame_with_dave(...)`, `recv_voice_packet_with_dave(...)`: low-level hooks for bringing your own DAVE encryption instead of the runtime's.
- `VoiceDaveySession`: a wrapper over a `davey` / OpenMLS session for such custom setups. `VoiceDaveyDecryptor` remains as a compatibility alias.
- `get_current_user_voice_state(...)`, `get_user_voice_state(...)`, `modify_current_user_voice_state(...)`, and `modify_user_voice_state(...)`: typed Voice Resource REST helpers for stage voice-state reads and moderation updates.
- `discordrs::voice::player::{AudioPlayer, AudioResource, AudioInput, NoSubscriberBehavior}`: the `2.2.0` discord.js-style playback pipeline (see below).

## Audio Playback Pipeline

`discordrs::voice::player` mirrors `@discordjs/voice`: `AudioInput` produces PCM (FFmpeg process spawn, raw PCM readers, or files), `AudioResource` chunks it into 20ms frames with live volume control and silence padding, and `AudioPlayer` runs the `Idle`/`Buffering`/`Playing`/`Paused`/`AutoPaused` state machine with watch-based state and broadcast events. Opus encoding sits behind `voice-encode`; without it `AudioPlayer::play` returns an error (see [Without `voice-encode`](#without-voice-encode)).

```rust
use std::sync::Arc;

use discordrs::voice::player::{
    AudioInput, AudioPlayer, AudioPlayerEvent, AudioPlayerOptions, AudioResource,
};
use discordrs::{connect_voice_runtime_with_options, VoiceConnectOptions, VoiceRuntimeConfig};

async fn play(
    config: VoiceRuntimeConfig,
    options: VoiceConnectOptions,
    input: &str,
) -> Result<(), discordrs::DiscordError> {
    // `options` carries the voice channel, so the runtime runs DAVE itself.
    let handle = Arc::new(connect_voice_runtime_with_options(config, options).await?);

    let player = AudioPlayer::new(AudioPlayerOptions::default());
    let subscription = player.subscribe_runtime(Arc::clone(&handle))?;

    // Anything FFmpeg can decode (needs `ffmpeg` on PATH).
    let resource = AudioResource::new(AudioInput::ffmpeg(input)?).with_volume(0.8);

    let mut events = player.events();
    player.play(resource)?;

    while let Ok(event) = events.recv().await {
        match event {
            AudioPlayerEvent::TrackStart => println!("track started"),
            AudioPlayerEvent::TrackEnd { reason } => {
                println!("track ended: {reason:?}");
                break;
            }
            AudioPlayerEvent::StateChange { old, new } => {
                println!("player state: {old:?} -> {new:?}");
            }
        }
    }

    // Waits for the silence tail, speaking off, and every player task.
    player.shutdown().await;
    drop(subscription);
    match Arc::try_unwrap(handle) {
        Ok(handle) => handle.close().await,
        Err(_) => Ok(()),
    }
}
```

- `player.play/pause/unpause/stop` drive the state machine; `player.state()` and `player.state_watch()` observe it. A command returning `Ok` means it was queued for the player's control task, which applies it as soon as it arrives; watch the state or events for the outcome.
- `TrackEnd { reason }` carries a `TrackEndReason` (`Finished`, `Stopped`, `Error(...)`). A track that ends before producing any audio emits `TrackEnd` without `TrackStart`.
- `NoSubscriberBehavior::{Pause, Play, Stop}` (in `AudioPlayerOptions`) chooses what happens while no connection is subscribed; the default `Pause` matches discord.js.
- `AudioResource::volume()` returns a live `VolumeControl`. The volume is applied when the player encodes a frame, so a change affects the next frame encoded; up to three frames already queued for a lagging connection keep their old volume. `AudioInput::file(...)`, `raw_pcm(...)`, `ffmpeg_with_binary(...)`, and `ffmpeg_with_args(...)` cover non-URL sources.
- Subscriptions are drop-safe: dropping the `PlayerSubscription` (or calling `unsubscribe()`) detaches the connection (see below).
- `AudioResource::next_pcm_frame()` is cancel safe: dropping the future mid-frame (for example as a losing `tokio::select!` branch) keeps the PCM already read, and the next call completes the same frame.
- A connection counts as subscribed only while it is *playable* (`VoiceFrameSink::is_playable`, as `AudioPlayer.playable` in discord.js). A `VoiceRuntimeHandle` in a DAVE call becomes playable once it holds the MLS group's keys; until then the player applies `NoSubscriberBehavior` (by default it waits in `AutoPaused` and then plays the track from its first frame). Alone in a DAVE channel, that is when someone joins. With `NoSubscriberBehavior::Stop`, wait for `handle.wait_until_playable()` before calling `play`.
- `player.connection_events()` reports `PlayerConnectionEvent::SendFailed { subscription, error }` when sends to a connection start failing and `Recovered { subscription, failed }` when they work again: once per stretch, not per frame (also logged). `PlayerSubscription::id()` matches `subscription`. Frames that were not sent still become an RTP timestamp gap.
- Full gateway-integrated example: `examples/music_bot.rs` (`cargo run --example music_bot --features gateway,voice,voice-encode`). It also shows a start watchdog that calls `stop()` when FFmpeg produces no audio in time (paused while the player waits for the DAVE group), connection events, and a clean `shutdown()`.

### Real-Time Delivery, Stalls, and Shutdown (3.0)

A player is one control task, plus one reader task per loaded resource and one task per subscribed connection. The control task never waits on audio input or on a connection, so a stalled FFmpeg stream or a slow voice connection cannot hold up commands or other connections.

**Sources.** The reader decodes at most 5 frames (100 ms) ahead, plus the frame it is reading; while (re)buffering the player additionally holds the 3 frames it is waiting for. When the buffer is full the reader waits, which backpressures FFmpeg through its stdout pipe (for example while paused).

| Situation | Behavior |
|---|---|
| Track start | The player stays `Buffering` until 3 frames (60 ms) are buffered or the input has ended, then emits `Playing` and `TrackStart`. |
| Source slower than real time (underrun) | A tick without a frame sends nothing. After 3 such ticks in a row, the player goes back to `Buffering` and every connection gets its silence tail and speaking off. It resumes (`Playing`, no second `TrackStart`) once 3 frames are buffered again or the input ends. Audio is delayed, never skipped. |
| `stop()` or `play()` replacing a track | The input's reader is cancelled and the resource dropped shortly after (an FFmpeg process is killed). Before `TrackEnd` is published, the audio queued for every connection is discarded and any send of it in progress is superseded: the connection's task polls a send only after re-checking, under a lock the player holds while flushing, so a superseded send is never polled again. With a sink that keeps the contract below (an unfinished send has transmitted nothing), no audio of the old track is transmitted after `TrackEnd`; only a send that had already completed went out, before it. |
| `pause()` | Flushes like `stop()` before the `Paused` state is published, and each connection gets its silence tail. The resource's own read-ahead is kept. |
| `unpause()` | Same rule as a start: back to `Playing` right away if 3 frames are buffered (or the input has ended), otherwise `Buffering` until they are. Playback continues from the next unread frame. |
| End of track (finished or failed) | Audio already queued for a connection still plays out (within its deadline), then the silence tail. |

**Connections.** Each connection has a 3-frame queue and its own task:

| Rule | Behavior |
|---|---|
| Deadlines | A queued frame must start sending within 60 ms of being produced or it expires; a send that takes longer than 60 ms counts as stalled and is cancelled. No frame reaches a connection more than 120 ms after it was produced. |
| Full queue | The *oldest* frame is dropped. A `subscribed connection is not keeping up` warning is logged once per episode, and `subscribed connection caught up` (with the number dropped) when it recovers. Other connections are unaffected. |
| Pacing | After a send completes, the next one starts no sooner than 10 ms later, so a connection recovering from a stall catches up at about twice real time instead of bursting its backlog. |
| RTP timeline | Frames that are not transmitted (dropped, expired, failed, cancelled, or underrun ticks) are reported to the next transmitted frame as `VoiceFrameTiming::gap`. `VoiceRuntimeHandle` moves the RTP timestamp across the gap (RFC 7587 timestamps count 48 kHz sampling time) while the sequence number stays contiguous. The first silence frame of a tail carries the gap since the last transmitted audio; once silence is on the wire, the next audio starts a new segment without a gap. If no silence went out (superseded before it was sent), the next audio's gap still counts from the last transmitted audio frame. |
| Silence tail | When a connection's audio stops (stop, pause, end of track, underrun, unsubscribe, shutdown), it gets five Opus silence frames at 20 ms intervals, then speaking off, as Discord recommends. The tail is state owned by the connection's task, not queued audio, so a lagging connection still gets it after it recovers. New audio supersedes a tail that has not been sent yet. A silence frame whose send fails is retried 20 ms later and not counted; after five failures in a row the tail is given up and only speaking off is sent. |
| Speaking | Coalesced: the connection eventually sees the latest state (`true` before the first frame of a stretch of audio, `false` after its tail). It is not a log of every transition. |

**Detaching and shutting down.**

| Situation | Behavior |
|---|---|
| Unsubscribe | Queued audio is discarded. The connection's task sends the silence tail and speaking off within a 200 ms grace period; if the connection is still stalled then, the task cancels the pending send, clears speaking anyway, and exits. |
| Subscribing a sink again | Sinks are identified by their `Arc`. Subscribing a sink that is already subscribed replaces the earlier subscription (dropping the earlier `PlayerSubscription` then has no effect). A sink that is still closing, whether replaced or unsubscribed, is handed over only once its tail and speaking off are done (at most 200 ms), so two tasks never send to one connection and a retiring task can never clear the new subscription's speaking flag. Subscribing one connection to two different players is not coordinated; don't. |
| `AudioPlayer::shutdown().await` | Stops the track (`TrackEnd { reason: Stopped }`), gives every connection the same 200 ms grace, cancels whatever is still stalled, and returns once the control task, every reader, and every connection task have finished: within about 240 ms even if the input and all connections are stalled. A task blocked in synchronous code cannot be cancelled; it is logged and abandoned after that bound. Afterwards every command fails. |
| Every handle and subscription dropped | The same cleanup runs in the background, but nothing reports when it is done. Use `shutdown()` when completion matters. |
| A `VoiceFrameSink` panics | That connection is detached (logged as an error); playback continues for the others. |
| An `AudioInput` reader panics | The track ends with `TrackEndReason::Error("... audio source reader stopped unexpectedly")`; the player keeps working. |

### Custom `VoiceFrameSink` implementations

`send_opus_packet(opus_frame, timing)` receives a `VoiceFrameTiming { duration, gap }`. An RTP sender should move its timestamp by `gap` before the frame. The player relies on this contract:

- Calls for one subscription never overlap; each subscription has its own task, and a re-subscribed sink is handed over only after the previous task finished.
- **Transmit only on completion.** A send must transmit nothing before the poll in which its future completes, and must return `Ok` only if the frame was transmitted. The player treats an error or a dropped future as "not transmitted" and includes the frame's media time in the next frame's `gap`, which is exact only if a dropped send really sent nothing. `VoiceRuntimeHandle` meets this: its datagram goes out in the poll that completes the send, its sequence number and timestamp advance only after that, and each attempt uses a fresh encryption nonce.
- The send future may be dropped before it completes: after 60 ms (stalled), when playback is stopped, paused, or replaced, when new audio supersedes a pending silence frame, and when the close grace period ends. A dropped send must leave the sink usable.
- Polls must not block. The player polls the send while holding the subscription's internal lock; that is what guarantees a superseded send is never polled again.
- `update_speaking(false)` is also called from a destructor when a subscription's task is cancelled, so it must not block or panic.
- `is_playable()` (default `true`) is called by the player's control task on every 20 ms tick, so it must be cheap and must not block. A sink that is not playable gets no audio, only its silence tail and speaking off.

### Without `voice-encode`

Building with `voice` but without `voice-encode` keeps the whole player API available, so code compiles the same either way. However, `AudioPlayer::play` returns `DiscordError::Voice` ("`AudioPlayer::play` requires the `voice-encode` feature ...") and drops the resource instead of accepting it. Before 3.0 the player silently advanced through the track while transmitting nothing.

## Example

```rust
use std::time::Duration;

use discordrs::{
    connect_voice_runtime_with_options, VoiceConnectOptions, VoiceOpusDecoder, VoiceRuntimeConfig,
};

async fn receive_voice() -> Result<(), discordrs::DiscordError> {
    let handle = connect_voice_runtime_with_options(
        VoiceRuntimeConfig::new(
            "guild_id".parse()?,
            "bot_user_id".parse()?,
            "voice_session_id",
            "voice_token",
            "wss://voice.discord.media/?v=8",
        ),
        // The voice channel: DAVE uses it as the MLS group.
        VoiceConnectOptions::new("voice_channel_id".parse()?),
    )
    .await?;

    // In a DAVE call: once this client has a media key in use.
    handle.wait_until_playable().await?;
    handle
        .send_opus_frame(&[0xf8, 0xff, 0xfe], Duration::from_millis(20))
        .await?;

    let mut decoder = VoiceOpusDecoder::discord_default()?;
    let decoded = handle.recv_decoded_voice_packet(&mut decoder, 2048).await?;
    println!(
        "SSRC {} produced {} samples/channel",
        decoded.packet.rtp.ssrc,
        decoded.samples_per_channel
    );

    handle.close().await
}
```

## Voice State REST

```rust
use discordrs::{ChannelId, GuildId, ModifyCurrentUserVoiceState, ModifyUserVoiceState, UserId};

let guild_id: GuildId = "123".parse()?;
let channel_id: ChannelId = "456".parse()?;
let user_id: UserId = "789".parse()?;

let state = rest.get_current_user_voice_state(guild_id).await?;
println!("current voice session: {:?}", state.session_id);

rest.modify_current_user_voice_state(
    guild_id,
    &ModifyCurrentUserVoiceState {
        channel_id: Some(channel_id),
        suppress: Some(false),
        request_to_speak_timestamp: Some(None),
    },
)
.await?;

rest.modify_user_voice_state(
    guild_id,
    user_id,
    &ModifyUserVoiceState {
        channel_id: Some(channel_id),
        suppress: Some(true),
    },
)
.await?;
```

## Transport Encryption

`VoiceRuntimeHandle` supports the two transport modes Discord's voice gateway offers, `aead_aes256_gcm_rtpsize` (preferred when offered) and `aead_xchacha20_poly1305_rtpsize` (always offered). Both follow the same packet layout, which matches @discordjs/voice and Songbird:

- **Nonce.** Every packet ends with a 32-bit counter. The AEAD nonce is that counter's four big-endian bytes followed by zeros (12 bytes for AES-256-GCM, 24 for XChaCha20-Poly1305). Discord's docs only say the counter is "appended to payload"; the "4 least significant bytes" wording on the same page describes DAVE's frame nonce, which `davey` handles, not transport encryption.
- **Cleartext and AAD.** The fixed RTP header, the CSRCs and, when the X bit is set, only the 4-byte extension preamble are sent in the clear and authenticated as additional data. The extension body is encrypted together with the payload. After decrypting, the runtime strips the extension body, so `VoiceReceivedPacket::opus_frame` holds only the Opus (or DAVE) frame; `VoiceRtpHeader::header_len` is the media offset after decryption, not where the ciphertext starts.
- **Padding.** When the P bit is set, the payload ends with RTP padding whose last octet counts the padding octets, itself included (RFC 3550 §5.1). The padding is encrypted too, so the runtime removes it after decrypting, before Opus decoding or DAVE frame parsing. A count of zero or one longer than the payload (it would reach into the extension body) is an error; a count equal to the payload length is a padding-only packet with an empty payload.

Before 3.0 the counter went into the last four nonce bytes and received extension bodies were treated as AAD, so packets sent to or received from real Discord voice servers failed authentication; padding was also left on the media payload. Known-answer vectors from `ci/voice_transport_kat.py`, generated outside the crate and cross-checked against Node's AES-256-GCM and libsodium, now pin both directions.

## DAVE (End-to-End Encryption)

Since 2026-03-01 Discord requires [DAVE](https://daveprotocol.com/), its MLS-based end-to-end encryption, in every voice channel except Stage channels, and closes connections from clients that do not support it with code 4017 ([Discord: Indicating DAVE Protocol Support](https://docs.discord.com/developers/topics/voice-connections#indicating-dave-protocol-support)). A runtime connected with `connect_voice_runtime_with_options(config, VoiceConnectOptions::new(channel_id))` (`Context::connect_voice_runtime_with_dave` does this) runs DAVE itself, the way @discordjs/voice and Songbird do. The channel is needed because DAVE uses it as the MLS group and the voice gateway does not say which channel a session is for. Plain `connect_voice_runtime(config)` and `Context::connect_voice_runtime` keep 3.0.0's behaviour, described at the end of this section.

> **Security: keep `davey` trace logging off.** davey, the DAVE library the runtime uses for MLS and decryption, writes media key secrets (the MLS-exported sender secret of every member, ratchet keys) to `tracing` at **trace** level, in `davey::session`, `davey::cryptor::hash_ratchet`, and `davey::cryptor::mlspp_crypto`. A log with those keys can decrypt captured call audio. Keep the `davey` target at `info` or coarser, for example `RUST_LOG=info,davey=info` (with an otherwise trace-level filter, add `davey=info`). This has been so since 3.0.0's `dave` feature (`VoiceDaveySession`), and davey 0.1.5, the latest release, still does it. **The runtime mitigates it; it does not fix it:** it runs its own davey calls under a `tracing` filter that drops davey's trace-level events (key material and all) before they reach your subscriber, and it logs a warning when it starts a DAVE session while any of those modules is at trace level. Not covered: davey calls made outside the runtime (`VoiceDaveySession`, or davey directly), and davey's events that `tracing` passes to the `log` crate when no `tracing` subscriber is installed (that fallback is also invisible to the warning). The fix belongs in davey; an upstream issue is drafted, not filed.

- **Joining.** IDENTIFY advertises `max_dave_protocol_version` (1 by default, capped at 1). When `SESSION_DESCRIPTION` negotiates DAVE, the runtime sends an MLS key package, applies the voice gateway's external sender, commits proposals for members who join (checked against `CLIENTS_CONNECT`), processes commits and welcomes, reports `transition_ready`, and switches keys on `execute_transition`. Messages that arrive during the handshake (`CLIENTS_CONNECT`, the external sender) are kept and handled once the session starts.
- **Sending.** `send_opus_frame(...)`, `send_opus_frame_after_gap(...)`, `play_opus_frames(...)`, and an `AudioPlayer` subscribed with `subscribe_runtime` encrypt each Opus frame with DAVE, then with transport encryption. The Opus silence frame goes out unencrypted, as the protocol allows. Keys change only when a transition executes, as the protocol requires: after processing a commit the runtime keeps sending with the previous epoch's key, which members that have not processed it yet still hold (the runtime keeps its own sending key for this, derived the way davey derives it; davey runs MLS and decryption). Until this client has a key in use, these sends return a `DiscordError::Voice` explaining why, and transmit nothing (no sequence number, timestamp, or nonce is used up) rather than leaking audio unencrypted into an end-to-end encrypted call; `is_playable()` is `false` meanwhile, so an `AudioPlayer` waits in `AutoPaused`. A client alone in a DAVE channel has no group yet; it forms when someone joins (this client then commits them).
- **Receiving.** `recv_voice_packet(...)` and `recv_decoded_voice_packet(...)` decrypt each speaker's frames with that user's key (the SSRC → user mapping comes from `SPEAKING`). A few frames always fail around MLS transitions; such packets, and DAVE frames from an SSRC with no known user yet, are skipped rather than returned as errors (which would end receive loops that use `?`) and counted in `VoiceDaveStatus::undecryptable_packets`. The Opus silence frame (`F8 FF FE`), which Discord's SFU sends unencrypted for muted users, is returned as is.
- **Transitions.** A downgrade to version 0 (a client without DAVE joined) accepts unencrypted frames as soon as it is prepared and sends them after it executes; an upgrade (`prepare_epoch` with epoch 1) creates a new group and keeps sending unencrypted frames until the upgrade executes. An invalid commit or welcome is reported with `dave_mls_invalid_commit_welcome` and the runtime rejoins with a new key package; so does a stream of more than 36 undecryptable frames outside any transition.
- **A 4017 close** (Discord requiring DAVE) during the handshake becomes an error that says what the connection advertised and, for `connect_voice_runtime`, points to `connect_voice_runtime_with_options`.
- **No resume.** The voice runtime has no RESUME loop: when the voice websocket closes, the handle's gateway task ends (`is_playable()` turns `false` and `wait_until_playable()` fails). Connecting again is a fresh IDENTIFY, and with it a fresh DAVE session that rejoins the MLS group with a new key package.

```rust
use discordrs::{VoiceDaveStatus, VoiceRuntimeHandle};

fn describe(handle: &VoiceRuntimeHandle) {
    let dave: VoiceDaveStatus = handle.dave_status();
    if dave.protocol_version == 0 {
        println!("transport encryption only (Stage channel or downgraded call)");
    } else if dave.can_send_audio() {
        println!(
            "end-to-end encrypted, epoch {:?}, privacy code {:?}",
            dave.epoch, dave.voice_privacy_code
        );
    } else {
        println!("waiting for the MLS group; last error: {:?}", dave.last_error);
    }
}
```

`subscribe_dave_status()` returns a watch receiver for the same status, and `dave_verification_code(user_id)` the pairwise code Discord clients show when verifying a member.

**`connect_voice_runtime` (3.0.0 behaviour).** It advertises `max_dave_protocol_version` as configured but runs no DAVE session: in a DAVE call the plain sends and receives return errors, `dave_status()` only reports the negotiated version, `is_playable()` is always `true` (so an `AudioPlayer` behaves as in 3.0.0), and DAVE is up to you: `VoiceDaveySession`, the raw `send_dave_*` commands, the `VoiceRuntimeState::dave` (`VoiceDaveState`) mirror of the raw DAVE messages, and `send_opus_frame_with_dave(...)` / `recv_voice_packet_with_dave(...)` with a `VoiceDaveFrameEncryptor` / `VoiceDaveFrameDecryptor`. Those `*_with_dave` methods also work on a `connect_voice_runtime_with_options` runtime for custom frame encryption, but do not drive MLS by hand there: the runtime sends its own key package, and the voice gateway would see two.

**Upgrading from 3.0.0.** 3.0.0's runtime advertised DAVE without running it, so in DAVE channels `AudioPlayer` played nothing (the player only logged a warning per frame). Nothing breaks: code that calls `connect_voice_runtime(config)` behaves as before. To get working audio in DAVE channels, connect with `connect_voice_runtime_with_options(config, VoiceConnectOptions::new(channel_id))`; in a gateway bot, switch from `Context::connect_voice_runtime` to `Context::connect_voice_runtime_with_dave`.

**Limits.** davey (0.1.x) does not check the user list of a welcome against the call's members yet (a TODO upstream); commits and proposals are checked. The runtime's frame counter allows 2^32 - 1 frames per MLS epoch (about 2.7 years of continuous audio); after that, sends fail until the next epoch rather than reusing a nonce.

**Verification status.** Hermetic tests run two independent `davey` sessions and a test-only voice gateway (an OpenMLS external sender) through joining by welcome, committing as the only member, audio both ways (including `AudioPlayer` with real Opus), downgrade and upgrade, invalid commit/welcome recovery, and the 4017 close. The runtime-managed path has **not** been run against live Discord yet; the ignored live harness is the gate:

```powershell
$env:DISCORDRS_LIVE_VOICE_SERVER_ID="..."
$env:DISCORDRS_LIVE_VOICE_USER_ID="..."
$env:DISCORDRS_LIVE_VOICE_SESSION_ID="..."
$env:DISCORDRS_LIVE_VOICE_TOKEN="..."
$env:DISCORDRS_LIVE_VOICE_ENDPOINT="..."
$env:DISCORDRS_LIVE_VOICE_CHANNEL_ID="..."
cargo test --all-features --test live_voice_dave -- --ignored --nocapture
```

For the full capture procedure and release evidence checklist, see [Live DAVE Validation](live-dave-validation.md).
