//! Live check of the runtime's own DAVE (end-to-end encryption) path, through
//! `connect_voice_runtime_with_options`, against a real Discord voice call:
//! join, become a member of the call's MLS group, play a tone through
//! `AudioPlayer`, and decrypt what another member says.
//!
//! Ignored by default; see `discordrsdocs/docs/api/live-dave-validation.md`.
//! It needs a prepared voice session (`examples/live_dave_capture_bot.rs`
//! prints the variables) and at least one other DAVE-capable member in the
//! channel who speaks during the receive window.

#![cfg(all(feature = "voice", feature = "voice-encode"))]
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::as_conversions,
    reason = "a manual live test: a failed expectation fails the test"
)]

use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use std::time::Duration;

use discordrs::voice::player::{
    AudioInput, AudioPlayer, AudioPlayerEvent, AudioPlayerOptions, AudioResource,
    PlayerConnectionEvent, TrackEndReason,
};
use discordrs::{
    connect_voice_runtime_with_options, VoiceConnectOptions, VoiceOpusDecoder, VoiceRuntimeConfig,
};
use tokio::time::{timeout, Instant};

/// How long to wait for the MLS group (another member must be present).
const PLAYABLE_TIMEOUT: Duration = Duration::from_secs(90);
/// Length of the tone played into the call.
const TONE: Duration = Duration::from_secs(3);
/// Default receive window; `DISCORDRS_LIVE_VOICE_RECEIVE_SECS` overrides it.
const DEFAULT_RECEIVE_SECS: u64 = 15;

struct LiveDaveConfig {
    server_id: String,
    user_id: String,
    session_id: String,
    token: String,
    endpoint: String,
    channel_id: String,
    receive: Duration,
}

impl LiveDaveConfig {
    fn from_env() -> Result<Self, String> {
        let required = [
            "DISCORDRS_LIVE_VOICE_SERVER_ID",
            "DISCORDRS_LIVE_VOICE_USER_ID",
            "DISCORDRS_LIVE_VOICE_SESSION_ID",
            "DISCORDRS_LIVE_VOICE_TOKEN",
            "DISCORDRS_LIVE_VOICE_ENDPOINT",
            "DISCORDRS_LIVE_VOICE_CHANNEL_ID",
        ];
        let missing = required
            .iter()
            .copied()
            .filter(|name| env_value(name).is_none())
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(format!(
                "set required live DAVE env vars before running this ignored test: {}",
                missing.join(", ")
            ));
        }
        let receive = match env_value("DISCORDRS_LIVE_VOICE_RECEIVE_SECS") {
            Some(seconds) => seconds
                .parse()
                .map_err(|error| format!("DISCORDRS_LIVE_VOICE_RECEIVE_SECS: {error}"))?,
            None => DEFAULT_RECEIVE_SECS,
        };

        Ok(Self {
            server_id: env_value("DISCORDRS_LIVE_VOICE_SERVER_ID").expect("validated above"),
            user_id: env_value("DISCORDRS_LIVE_VOICE_USER_ID").expect("validated above"),
            session_id: env_value("DISCORDRS_LIVE_VOICE_SESSION_ID").expect("validated above"),
            token: env_value("DISCORDRS_LIVE_VOICE_TOKEN").expect("validated above"),
            endpoint: env_value("DISCORDRS_LIVE_VOICE_ENDPOINT").expect("validated above"),
            channel_id: env_value("DISCORDRS_LIVE_VOICE_CHANNEL_ID").expect("validated above"),
            receive: Duration::from_secs(receive),
        })
    }
}

fn env_value(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
}

/// 48 kHz stereo s16le PCM of a 440 Hz tone at about -8 dBFS.
fn tone_pcm(length: Duration) -> Vec<u8> {
    let samples = (48_000.0 * length.as_secs_f64()) as usize;
    (0..samples)
        .flat_map(|index| {
            let phase = index as f64 * 440.0 / 48_000.0;
            let sample = ((phase * std::f64::consts::TAU).sin() * 13_000.0) as i16;
            let bytes = sample.to_le_bytes();
            [bytes[0], bytes[1], bytes[0], bytes[1]]
        })
        .collect()
}

#[tokio::test]
#[ignore = "requires a live Discord voice session; see docs/api/live-dave-validation.md"]
async fn live_voice_dave_plays_a_tone_and_decrypts_other_members() {
    let config = LiveDaveConfig::from_env().expect(
        "live DAVE validation was explicitly requested with --ignored but is not configured",
    );

    // The entry point that runs DAVE (the one
    // `Context::connect_voice_runtime_with_dave` uses), with the default configuration otherwise.
    let handle = Arc::new(
        connect_voice_runtime_with_options(
            VoiceRuntimeConfig::new(
                config.server_id.parse().expect("server id"),
                config.user_id.parse().expect("user id"),
                config.session_id,
                config.token,
                config.endpoint,
            ),
            VoiceConnectOptions::new(config.channel_id.parse().expect("channel id")),
        )
        .await
        .expect("failed to connect the live voice runtime"),
    );
    let status = handle.dave_status();
    println!(
        "negotiated DAVE protocol version {}",
        status.protocol_version
    );
    assert!(
        status.protocol_version > 0,
        "the call did not negotiate DAVE; use a non-Stage voice channel"
    );

    timeout(PLAYABLE_TIMEOUT, handle.wait_until_playable())
        .await
        .unwrap_or_else(|_| {
            panic!(
                "no MLS group after {PLAYABLE_TIMEOUT:?}; is another DAVE-capable member in the \
                 channel? last status: {:?}",
                handle.dave_status()
            )
        })
        .expect("the voice connection closed");
    let status = handle.dave_status();
    println!(
        "joined the MLS group: epoch {:?}, last transition {:?}, voice privacy code {:?}",
        status.epoch, status.last_transition_id, status.voice_privacy_code
    );
    assert!(status.ready, "{status:?}");

    // Send: the default AudioPlayer path. Listen in the channel: a 440 Hz
    // tone means the other clients decrypted our DAVE frames.
    let player = AudioPlayer::new(AudioPlayerOptions::default());
    let mut events = player.events();
    let mut connection_events = player.connection_events();
    let subscription = player
        .subscribe_runtime(Arc::clone(&handle))
        .expect("subscribe");
    player
        .play(AudioResource::new(AudioInput::raw_pcm(
            std::io::Cursor::new(tone_pcm(TONE)),
        )))
        .expect("play");
    let deadline = TONE + Duration::from_secs(10);
    let reason = timeout(deadline, async {
        loop {
            if let AudioPlayerEvent::TrackEnd { reason } = events.recv().await.expect("events") {
                return reason;
            }
        }
    })
    .await
    .expect("the tone did not finish");
    assert_eq!(reason, TrackEndReason::Finished);
    while let Ok(event) = connection_events.try_recv() {
        if let PlayerConnectionEvent::SendFailed { error, .. } = event {
            panic!("a send to the DAVE call failed: {error}");
        }
    }
    player.shutdown().await;
    drop(subscription);
    println!("played {TONE:?} of a 440 Hz tone; check that the channel heard it");

    // Receive: decrypt what the other members send. Speak (unmuted) now.
    println!(
        "receiving for {:?}; speak in the channel now",
        config.receive
    );
    let mut decoders: HashMap<u32, VoiceOpusDecoder> = HashMap::new();
    let mut speech_frames: HashMap<String, usize> = HashMap::new();
    let end = Instant::now() + config.receive;
    while let Some(remaining) = end.checked_duration_since(Instant::now()) {
        let Ok(packet) = timeout(remaining, handle.recv_voice_packet(4096)).await else {
            break;
        };
        let packet = packet.expect("receive failed");
        if packet.opus_frame == [0xf8, 0xff, 0xfe] {
            continue;
        }
        let decoder = decoders
            .entry(packet.rtp.ssrc)
            .or_insert_with(|| VoiceOpusDecoder::discord_default().expect("decoder"));
        decoder
            .decode_opus_frame(&packet.opus_frame, false)
            .expect("a decrypted frame is valid Opus");
        let user = packet
            .user_id
            .map_or_else(|| format!("ssrc {}", packet.rtp.ssrc), |id| id.to_string());
        *speech_frames.entry(user).or_default() += 1;
    }
    let status = handle.dave_status();
    let skipped = handle.receive_stats();
    println!(
        "decrypted speech frames per user: {speech_frames:?}; undecryptable packets: {}; \
         recoveries: {}; skipped non-voice datagrams (RTCP): {}; rejected packets: {}",
        status.undecryptable_packets,
        status.recoveries,
        skipped.non_voice_packets,
        skipped.rejected_packets
    );
    assert!(
        !speech_frames.is_empty(),
        "no decrypted speech from another member; did anyone speak during the receive window?"
    );

    match Arc::try_unwrap(handle) {
        Ok(handle) => handle.close().await.expect("failed to close voice runtime"),
        Err(_) => panic!("the voice handle is still shared"),
    }
}
