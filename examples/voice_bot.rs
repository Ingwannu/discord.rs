#![allow(
    clippy::expect_used,
    reason = "an example stops with a message when its setup or input is wrong"
)]

#[cfg(feature = "voice")]
use discordrs::{
    AudioTrack, ChannelId, GuildId, UserId, VoiceConnectionConfig, VoiceEncryptionMode,
    VoiceManager, VoiceSpeakingFlags, VoiceTransportState,
};
#[cfg(feature = "voice")]
use discordrs::{VoiceServerUpdate, VoiceState};

#[cfg(feature = "voice")]
fn main() -> Result<(), discordrs::DiscordError> {
    let mut voice = VoiceManager::new();
    // IDs are checked when built (`?` maps `InvalidId` into `DiscordError`)
    // and are `Copy` afterwards.
    let guild_id: GuildId = "1".parse()?;
    let channel_id = ChannelId::try_from(2_u64)?;
    let user_id = UserId::try_from(3_u64)?;

    voice.connect(VoiceConnectionConfig::new(guild_id, channel_id));
    let _ = voice.update_voice_state(&VoiceState {
        guild_id: Some(guild_id),
        channel_id: Some(channel_id),
        session_id: Some("session".to_string()),
        self_mute: false,
        self_deaf: false,
        ..VoiceState::default()
    });
    let _ = voice.update_server(VoiceServerUpdate {
        guild_id,
        token: "token".to_string(),
        endpoint: Some("voice.discord.media".to_string()),
    });
    let _ = voice.configure_transport(
        guild_id,
        VoiceTransportState::udp(
            "127.0.0.1",
            5000,
            VoiceEncryptionMode::aead_aes256_gcm_rtpsize(),
            42,
        ),
    );
    voice
        .enqueue(
            guild_id,
            AudioTrack::new("intro", "memory://intro").title("Intro"),
        )
        .expect("voice manager should have a player after connect");
    voice
        .enqueue(
            guild_id,
            AudioTrack::new("loop", "memory://loop").title("Loop"),
        )
        .expect("voice manager should keep the queue available");

    let current_id = voice
        .start_next(guild_id)
        .expect("queue should produce a track")
        .id
        .clone();
    let speaking = voice
        .speaking_command(guild_id, VoiceSpeakingFlags::MICROPHONE, 0)
        .expect("voice manager should build a speaking command after transport setup")
        .payload();
    // The runtime config carries the voice channel, which DAVE end-to-end
    // encryption (required outside Stage channels) uses as its MLS group;
    // `connect_voice_runtime(config)` then joins the group on its own.
    let runtime_config = voice
        .runtime_config(guild_id, user_id)
        .expect("voice manager should derive a runtime config after session setup");
    println!(
        "playing={} speaking_op={} runtime_url={} dave_channel={:?} queue_events={}",
        current_id,
        speaking["op"],
        runtime_config.websocket_url(),
        runtime_config.channel_id,
        voice.events().len()
    );
    let _ = voice.stop(guild_id);
    let _ = voice.disconnect(guild_id);
    Ok(())
}

#[cfg(not(feature = "voice"))]
fn main() {}
