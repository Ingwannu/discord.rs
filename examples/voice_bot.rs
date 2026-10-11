#[cfg(feature = "voice")]
use discordrs::{
    AudioTrack, GenericId, VoiceConnectionConfig, VoiceEncryptionMode, VoiceManager,
    VoiceSpeakingFlags, VoiceTransportState,
};
#[cfg(feature = "voice")]
use discordrs::{VoiceServerUpdate, VoiceState};

#[cfg(feature = "voice")]
fn main() -> Result<(), discordrs::DiscordError> {
    let mut voice = VoiceManager::new();
    // IDs are checked when built (`?` maps `InvalidId` into `DiscordError`)
    // and are `Copy` afterwards.
    let guild_id: GenericId = "1".parse()?;
    let channel_id = GenericId::try_from(2_u64)?;
    let user_id = GenericId::try_from(3_u64)?;

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
    let runtime_url = voice
        .runtime_config(guild_id, user_id)
        .expect("voice manager should derive a runtime config after session setup")
        .websocket_url();
    println!(
        "playing={} speaking_op={} runtime_url={} queue_events={}",
        current_id,
        speaking["op"],
        runtime_url,
        voice.events().len()
    );
    let _ = voice.stop(guild_id);
    let _ = voice.disconnect(guild_id);
    Ok(())
}

#[cfg(not(feature = "voice"))]
fn main() {}
