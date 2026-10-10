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
//!
//! If FFmpeg produces no audio within `START_TIMEOUT` (an unreachable URL, a
//! stalled stream), the bot stops the track instead of waiting forever. This
//! works even while FFmpeg is stuck mid-read, because the player reads its
//! input on a separate task and handles `stop()` as soon as it arrives.
//! `AudioPlayer::shutdown` then waits until the connection got its silence
//! tail and every player task has finished.

#[cfg(all(feature = "gateway", feature = "voice", feature = "voice-encode"))]
mod bot {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use async_trait::async_trait;
    use discordrs::voice::player::{
        AudioInput, AudioPlayer, AudioPlayerEvent, AudioPlayerOptions, AudioResource,
    };
    use discordrs::{
        gateway_intents, Client, Context, DiscordError, EventHandler, ReadyPayload, Snowflake,
        VoiceRuntimeConfig, VoiceServerUpdate, VoiceState,
    };

    /// How long FFmpeg may take to produce the first frame before the bot
    /// gives up on the input.
    const START_TIMEOUT: Duration = Duration::from_secs(15);

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

        // A watchdog for inputs that never start. `stop()` takes effect even
        // while the player's read from FFmpeg is stalled, and dropping the
        // resource kills the FFmpeg process.
        let mut start_timeout = std::pin::pin!(tokio::time::sleep(START_TIMEOUT));
        let mut started = false;

        loop {
            tokio::select! {
                event = events.recv() => match event {
                    Ok(AudioPlayerEvent::StateChange { old, new }) => {
                        println!("player state: {old:?} -> {new:?}");
                    }
                    Ok(AudioPlayerEvent::TrackStart) => {
                        started = true;
                        println!("track started");
                    }
                    Ok(AudioPlayerEvent::TrackEnd { reason }) => {
                        println!("track ended: {reason:?}");
                        break;
                    }
                    Err(_) => break,
                },
                () = &mut start_timeout, if !started => {
                    eprintln!("no audio from {input} after {START_TIMEOUT:?}; stopping");
                    started = true;
                    player.stop()?;
                }
            }
        }

        // Waits (at most about a quarter second) for the connection's silence
        // tail and speaking-off, then for every player task to finish.
        player.shutdown().await;
        drop(subscription);
        // Shutdown released the player's reference to the connection, so it
        // can be closed now.
        match Arc::try_unwrap(handle) {
            Ok(handle) => handle.close().await,
            Err(_) => Ok(()),
        }
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
