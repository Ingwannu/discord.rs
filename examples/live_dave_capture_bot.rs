#![allow(
    clippy::expect_used,
    reason = "an example stops with a message when its setup or input is wrong"
)]

#[cfg(all(feature = "gateway", feature = "voice"))]
use std::sync::Mutex;

#[cfg(all(feature = "gateway", feature = "voice"))]
use std::io::Write;

#[cfg(all(feature = "gateway", feature = "voice"))]
use discordrs::{
    gateway_intents, ChannelId, Client, Context, EventHandler, GuildId, ReadyPayload, UserId,
    VoiceServerUpdate, VoiceState,
};

#[cfg(all(feature = "gateway", feature = "voice"))]
#[derive(Default)]
struct CaptureState {
    user_id: Option<UserId>,
    session_id: Option<String>,
    server: Option<VoiceServerUpdate>,
    printed: bool,
}

#[cfg(all(feature = "gateway", feature = "voice"))]
struct Handler {
    guild_id: GuildId,
    channel_id: ChannelId,
    state: Mutex<CaptureState>,
}

#[cfg(all(feature = "gateway", feature = "voice"))]
impl Handler {
    fn new(guild_id: GuildId, channel_id: ChannelId) -> Self {
        Self {
            guild_id,
            channel_id,
            state: Mutex::new(CaptureState::default()),
        }
    }

    fn print_if_ready(&self) {
        let mut state = self.state.lock().expect("capture state mutex poisoned");
        if state.printed {
            return;
        }

        let Some(user_id) = state.user_id else {
            return;
        };
        let Some(session_id) = state.session_id.as_deref() else {
            return;
        };
        let Some(server) = state.server.as_ref() else {
            return;
        };
        let Some(endpoint) = server.endpoint.as_deref() else {
            return;
        };

        println!();
        println!("# Live DAVE validation environment");
        println!("$env:DISCORDRS_LIVE_VOICE_SERVER_ID=\"{}\"", self.guild_id);
        println!("$env:DISCORDRS_LIVE_VOICE_USER_ID=\"{user_id}\"");
        println!("$env:DISCORDRS_LIVE_VOICE_SESSION_ID=\"{session_id}\"");
        println!("$env:DISCORDRS_LIVE_VOICE_TOKEN=\"{}\"", server.token);
        println!("$env:DISCORDRS_LIVE_VOICE_ENDPOINT=\"{endpoint}\"");
        println!(
            "$env:DISCORDRS_LIVE_VOICE_CHANNEL_ID=\"{}\"",
            self.channel_id
        );
        println!("cargo test --all-features --test live_voice_dave -- --ignored --nocapture");
        println!();

        state.printed = true;
        if let Err(error) = std::io::stdout().flush() {
            eprintln!("failed to flush the printed variables: {error}");
        }
        if matches!(
            std::env::var("DISCORDRS_CAPTURE_EXIT_AFTER_PRINT").as_deref(),
            Ok("1" | "true" | "TRUE" | "yes" | "YES")
        ) {
            std::process::exit(0);
        }
    }
}

#[cfg(all(feature = "gateway", feature = "voice"))]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, ready: &ReadyPayload) {
        {
            let mut state = self.state.lock().expect("capture state mutex poisoned");
            state.user_id = Some(ready.user.id);
        }

        println!(
            "ready as {}; joining voice channel {} in guild {}",
            ready.user.username, self.channel_id, self.guild_id
        );

        if let Err(error) = ctx
            .join_voice(self.guild_id, self.channel_id, false, true)
            .await
        {
            eprintln!("failed to send voice state update: {error}");
        }
    }

    async fn voice_state_update(&self, _ctx: Context, state: &VoiceState) {
        let Some(guild_id) = state.guild_id else {
            return;
        };
        let Some(user_id) = state.user_id else {
            return;
        };
        let Some(channel_id) = state.channel_id else {
            return;
        };
        if guild_id != self.guild_id || channel_id != self.channel_id {
            return;
        }

        let expected_user_id = {
            let capture = self.state.lock().expect("capture state mutex poisoned");
            capture.user_id
        };
        if expected_user_id != Some(user_id) {
            return;
        }

        if let Some(session_id) = state.session_id.clone() {
            let mut capture = self.state.lock().expect("capture state mutex poisoned");
            capture.session_id = Some(session_id);
            drop(capture);
            self.print_if_ready();
        }
    }

    async fn voice_server_update(&self, _ctx: Context, server: &VoiceServerUpdate) {
        if server.guild_id != self.guild_id {
            return;
        }

        let mut capture = self.state.lock().expect("capture state mutex poisoned");
        capture.server = Some(server.clone());
        drop(capture);
        self.print_if_ready();
    }
}

#[cfg(all(feature = "gateway", feature = "voice"))]
#[tokio::main]
async fn main() -> Result<(), discordrs::DiscordError> {
    let token = std::env::var("DISCORD_TOKEN")?;
    let guild_id: GuildId = std::env::var("DISCORDRS_CAPTURE_GUILD_ID")
        .map_err(|_| discordrs::DiscordError::model("set DISCORDRS_CAPTURE_GUILD_ID"))?
        .parse()?;
    let channel_id: ChannelId = std::env::var("DISCORDRS_CAPTURE_CHANNEL_ID")
        .map_err(|_| discordrs::DiscordError::model("set DISCORDRS_CAPTURE_CHANNEL_ID"))?
        .parse()?;

    Client::builder(
        &token,
        gateway_intents::GUILDS | gateway_intents::GUILD_VOICE_STATES,
    )
    .event_handler(Handler::new(guild_id, channel_id))
    .start()
    .await
}

#[cfg(not(all(feature = "gateway", feature = "voice")))]
fn main() {}
