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
//! Works in DAVE (end-to-end encrypted) channels, which is every voice
//! channel except Stage channels: it connects with
//! `Context::connect_voice_runtime_with_dave`, which takes the session,
//! token, endpoint, and channel from the voice state the context tracks for
//! the bot, so the voice runtime joins the call's MLS group and the player's
//! frames are encrypted before they leave. The player waits in `AutoPaused`
//! until the runtime has a key in use, which needs another member in the
//! channel.
//!
//! If FFmpeg produces no audio within `START_TIMEOUT` (an unreachable URL, a
//! stalled stream), the bot stops the track instead of waiting forever. This
//! works even while FFmpeg is stuck mid-read, because the player reads its
//! input on a separate task and handles `stop()` as soon as it arrives.
//! `AudioPlayer::shutdown` then waits until the connection got its silence
//! tail and every player task has finished.

#![allow(
    clippy::expect_used,
    reason = "an example stops with a message when its setup or input is wrong"
)]

#[cfg(all(feature = "gateway", feature = "voice", feature = "voice-encode"))]
mod bot {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use tokio::sync::broadcast::error::RecvError;

    use discordrs::voice::player::{
        AudioInput, AudioPlayer, AudioPlayerEvent, AudioPlayerOptions, AudioPlayerState,
        AudioResource, PlayerConnectionEvent,
    };
    use discordrs::{
        gateway_intents, ChannelId, Client, Context, DiscordError, EventHandler, GuildId,
        ReadyPayload, UserId, VoiceConnectionConfig, VoiceServerUpdate, VoiceState,
    };

    /// How long FFmpeg may take to produce the first frame before the bot
    /// gives up on the input.
    const START_TIMEOUT: Duration = Duration::from_secs(15);

    #[derive(Default)]
    struct SessionState {
        user_id: Option<UserId>,
        started: bool,
    }

    pub struct Handler {
        guild_id: GuildId,
        channel_id: ChannelId,
        input: String,
        state: Mutex<SessionState>,
    }

    impl Handler {
        pub fn new(guild_id: GuildId, channel_id: ChannelId, input: String) -> Self {
            Self {
                guild_id,
                channel_id,
                input,
                state: Mutex::new(SessionState::default()),
            }
        }

        /// Starts playback once the context tracks a complete voice session
        /// for the bot: its own VOICE_STATE_UPDATE and the
        /// VOICE_SERVER_UPDATE, which arrive in either order.
        async fn try_start_playback(&self, ctx: &Context) {
            let Some(user_id) = self
                .state
                .lock()
                .expect("session state mutex poisoned")
                .user_id
            else {
                return;
            };
            if ctx
                .voice_runtime_config(self.guild_id, user_id)
                .await
                .is_none()
            {
                return;
            }
            {
                let mut state = self.state.lock().expect("session state mutex poisoned");
                if state.started {
                    return;
                }
                state.started = true;
            }

            let ctx = ctx.clone();
            let guild_id = self.guild_id;
            let input = self.input.clone();
            tokio::spawn(async move {
                if let Err(error) = play(&ctx, guild_id, user_id, input).await {
                    eprintln!("playback failed: {error}");
                }
                std::process::exit(0);
            });
        }
    }

    async fn play(
        ctx: &Context,
        guild_id: GuildId,
        user_id: UserId,
        input: String,
    ) -> Result<(), DiscordError> {
        println!("connecting the voice runtime for guild {guild_id}");
        // The runtime runs DAVE itself; `connect_voice_runtime` would leave it
        // to us, as in 3.0.0.
        let handle = Arc::new(
            ctx.connect_voice_runtime_with_dave(guild_id, user_id)
                .await?,
        );
        let dave = handle.dave_status();
        if dave.protocol_version > 0 && !dave.can_send_audio() {
            println!(
                "end-to-end encrypted call (DAVE v{}); playback starts once the MLS group forms",
                dave.protocol_version
            );
        }

        let player = AudioPlayer::new(AudioPlayerOptions::default());
        let mut connection_events = player.connection_events();
        let subscription = player.subscribe_runtime(Arc::clone(&handle))?;

        let resource = AudioResource::new(AudioInput::ffmpeg(&input)?).with_title(input.clone());
        let volume = resource.volume();
        println!("playing {input} at volume {}", volume.get());

        let mut events = player.events();
        player.play(resource)?;

        // A watchdog for inputs that never start. `stop()` takes effect even
        // while the player's read from FFmpeg is stalled, and dropping the
        // resource kills the FFmpeg process. It does not run while the
        // player waits in `AutoPaused` for the DAVE group: that is not the
        // input's fault.
        let mut start_timeout = std::pin::pin!(tokio::time::sleep(START_TIMEOUT));
        let mut started = false;
        let mut waiting_for_listeners = false;

        loop {
            tokio::select! {
                event = events.recv() => match event {
                    Ok(AudioPlayerEvent::StateChange { old, new }) => {
                        println!("player state: {old:?} -> {new:?}");
                        waiting_for_listeners = new == AudioPlayerState::AutoPaused;
                        if old == AudioPlayerState::AutoPaused {
                            start_timeout
                                .as_mut()
                                .reset(tokio::time::Instant::now() + START_TIMEOUT);
                        }
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
                // Sends that fail are reported once per stretch, not per frame.
                event = connection_events.recv() => match event {
                    Ok(PlayerConnectionEvent::SendFailed { error, .. }) => {
                        eprintln!("audio is not reaching the voice connection: {error}");
                    }
                    Ok(PlayerConnectionEvent::Recovered { failed, .. }) => {
                        println!("audio flows again after {failed} failed frames");
                    }
                    Err(RecvError::Closed) => break,
                    Ok(_) | Err(RecvError::Lagged(_)) => {}
                },
                () = &mut start_timeout, if !started && !waiting_for_listeners => {
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

    impl EventHandler for Handler {
        async fn ready(&self, ctx: Context, ready: &ReadyPayload) {
            {
                let mut state = self.state.lock().expect("session state mutex poisoned");
                state.user_id = Some(ready.user.id);
            }
            println!(
                "ready as {}; joining voice channel {} in guild {}",
                ready.user.username, self.channel_id, self.guild_id
            );
            // Track the connection before joining, so the VOICE_SERVER_UPDATE
            // is kept even if it arrives before the bot's VOICE_STATE_UPDATE.
            ctx.connect_voice(VoiceConnectionConfig::new(self.guild_id, self.channel_id))
                .await;
            if let Err(error) = ctx
                .join_voice(self.guild_id, self.channel_id, false, true)
                .await
            {
                eprintln!("failed to send voice state update: {error}");
            }
        }

        // The context has already applied these events to its tracked voice
        // state (the bot's own voice states only) when the hooks run.
        async fn voice_state_update(&self, ctx: Context, voice_state: &VoiceState) {
            if voice_state.guild_id == Some(self.guild_id) {
                self.try_start_playback(&ctx).await;
            }
        }

        async fn voice_server_update(&self, ctx: Context, server: &VoiceServerUpdate) {
            if server.guild_id == self.guild_id {
                self.try_start_playback(&ctx).await;
            }
        }
    }

    pub async fn run() -> Result<(), DiscordError> {
        let token = std::env::var("DISCORD_TOKEN")?;
        // IDs are validated once, here; a typo fails at startup instead of
        // as a 404 later. `?` turns the `InvalidId` into a `DiscordError`.
        let guild_id: GuildId = std::env::var("MUSIC_GUILD_ID")
            .map_err(|_| DiscordError::model("set MUSIC_GUILD_ID"))?
            .parse()?;
        let channel_id: ChannelId = std::env::var("MUSIC_CHANNEL_ID")
            .map_err(|_| DiscordError::model("set MUSIC_CHANNEL_ID"))?
            .parse()?;
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
