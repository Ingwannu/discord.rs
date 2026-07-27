//! Minimal music bot using the discord.js-style audio playback pipeline.
//!
//! Joins a voice channel, connects the voice runtime, and plays an input
//! (any URL or file FFmpeg can decode) through an `AudioPlayer`.
//!
//! Environment variables:
//! - `DISCORD_TOKEN` — bot token
//! - `MUSIC_GUILD_ID` — guild to join
//! - `MUSIC_CHANNEL_ID` — voice channel to join
//! - `MUSIC_INPUT` — URL or file path passed to FFmpeg
//!
//! Requires an `ffmpeg` binary on `PATH`.

#[cfg(all(feature = "gateway", feature = "voice", feature = "voice-encode"))]
mod bot {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use discordrs::voice::player::{
        AudioInput, AudioPlayer, AudioPlayerEvent, AudioPlayerOptions, AudioResource,
    };
    use discordrs::{
        gateway_intents, Client, Context, DiscordError, EventHandler, ReadyPayload, Snowflake,
        VoiceRuntimeConfig, VoiceServerUpdate, VoiceState,
    };

    #[derive(Default)]
    struct SessionState {
        user_id: Option<String>,
        session_id: Option<String>,
        server: Option<VoiceServerUpdate>,
        started: bool,
    }

    pub struct Handler {
        guild_id: String,
        channel_id: String,
        input: String,
        state: Mutex<SessionState>,
    }

    impl Handler {
        pub fn new(guild_id: String, channel_id: String, input: String) -> Self {
            Self {
                guild_id,
                channel_id,
                input,
                state: Mutex::new(SessionState::default()),
            }
        }

        /// Starts playback once the voice session id and server token are both
        /// known.
        fn try_start_playback(&self) {
            let config = {
                let mut state = self.state.lock().expect("session state mutex poisoned");
                if state.started {
                    return;
                }
                let (Some(user_id), Some(session_id), Some(server)) = (
                    state.user_id.as_ref(),
                    state.session_id.as_ref(),
                    state.server.as_ref(),
                ) else {
                    return;
                };
                let Some(endpoint) = server.endpoint.as_ref() else {
                    return;
                };
                let config = VoiceRuntimeConfig::new(
                    self.guild_id.as_str(),
                    user_id.as_str(),
                    session_id.as_str(),
                    server.token.as_str(),
                    endpoint.as_str(),
                );
                state.started = true;
                config
            };

            let input = self.input.clone();
            tokio::spawn(async move {
                if let Err(error) = play(config, input).await {
                    eprintln!("playback failed: {error}");
                }
                std::process::exit(0);
            });
        }
    }

    async fn play(config: VoiceRuntimeConfig, input: String) -> Result<(), DiscordError> {
        println!("connecting voice runtime to {}", config.endpoint);
        let handle = Arc::new(discordrs::connect_voice_runtime(config).await?);

        let player = AudioPlayer::new(AudioPlayerOptions::default());
        let subscription = player.subscribe_runtime(Arc::clone(&handle))?;

        let resource = AudioResource::new(AudioInput::ffmpeg(&input)?).with_title(input.clone());
        let volume = resource.volume();
        println!("playing {input} at volume {}", volume.get());

        let mut events = player.events();
        player.play(resource)?;

        while let Ok(event) = events.recv().await {
            match event {
                AudioPlayerEvent::StateChange { old, new } => {
                    println!("player state: {old:?} -> {new:?}");
                }
                AudioPlayerEvent::TrackStart => println!("track started"),
                AudioPlayerEvent::TrackEnd { reason } => {
                    println!("track ended: {reason:?}");
                    break;
                }
            }
        }

        // Let the trailing silence frames flush before tearing down.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        subscription.unsubscribe();
        Ok(())
    }

    #[async_trait]
    impl EventHandler for Handler {
        async fn ready(&self, ctx: Context, ready: ReadyPayload) {
            {
                let mut state = self.state.lock().expect("session state mutex poisoned");
                state.user_id = Some(ready.user.id.to_string());
            }
            println!(
                "ready as {}; joining voice channel {} in guild {}",
                ready.user.username, self.channel_id, self.guild_id
            );
            if let Err(error) = ctx
                .join_voice(
                    Snowflake::from(self.guild_id.as_str()),
                    Snowflake::from(self.channel_id.as_str()),
                    false,
                    true,
                )
                .await
            {
                eprintln!("failed to send voice state update: {error}");
            }
        }

        async fn voice_state_update(&self, _ctx: Context, voice_state: VoiceState) {
            let expected_user = {
                let state = self.state.lock().expect("session state mutex poisoned");
                state.user_id.clone()
            };
            if voice_state.guild_id.as_ref().map(Snowflake::as_str) != Some(&self.guild_id)
                || voice_state.user_id.as_ref().map(Snowflake::as_str) != expected_user.as_deref()
            {
                return;
            }
            if let Some(session_id) = voice_state.session_id {
                let mut state = self.state.lock().expect("session state mutex poisoned");
                state.session_id = Some(session_id);
                drop(state);
                self.try_start_playback();
            }
        }

        async fn voice_server_update(&self, _ctx: Context, server: VoiceServerUpdate) {
            if server.guild_id.as_str() != self.guild_id {
                return;
            }
            let mut state = self.state.lock().expect("session state mutex poisoned");
            state.server = Some(server);
            drop(state);
            self.try_start_playback();
        }
    }

    pub async fn run() -> Result<(), DiscordError> {
        let token = std::env::var("DISCORD_TOKEN")?;
        let guild_id = std::env::var("MUSIC_GUILD_ID")
            .map_err(|_| DiscordError::model("set MUSIC_GUILD_ID"))?;
        let channel_id = std::env::var("MUSIC_CHANNEL_ID")
            .map_err(|_| DiscordError::model("set MUSIC_CHANNEL_ID"))?;
        let input =
            std::env::var("MUSIC_INPUT").map_err(|_| DiscordError::model("set MUSIC_INPUT"))?;

        Client::builder(
            &token,
            gateway_intents::GUILDS | gateway_intents::GUILD_VOICE_STATES,
        )
        .event_handler(Handler::new(guild_id, channel_id, input))
        .start()
        .await
    }
}

#[cfg(all(feature = "gateway", feature = "voice", feature = "voice-encode"))]
#[tokio::main]
async fn main() -> Result<(), discordrs::DiscordError> {
    bot::run().await
}

#[cfg(not(all(feature = "gateway", feature = "voice", feature = "voice-encode")))]
fn main() {}
