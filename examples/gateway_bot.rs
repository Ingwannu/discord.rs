#[cfg(feature = "gateway")]
use discordrs::{
    gateway_intents, respond_with_message, Client, Context, CreateMessage, Event, EventHandler,
    EventKind, Interaction,
};
#[cfg(feature = "gateway")]
use std::sync::Arc;

#[cfg(feature = "gateway")]
struct Handler;

#[cfg(feature = "gateway")]
impl EventHandler for Handler {
    async fn handle_event(&self, ctx: Context, event: Arc<Event>) {
        match event.kind() {
            EventKind::Ready(ready) => {
                println!("ready: {}", ready.user.username);
            }
            EventKind::InteractionCreate(interaction) => {
                if let Interaction::ChatInputCommand(command) = &**interaction {
                    if command.data.name.as_deref() == Some("hello") {
                        if let Err(error) = respond_with_message(
                            ctx.rest().as_ref(),
                            &command.context,
                            CreateMessage {
                                content: Some("hello from the gateway runtime".to_string()),
                                ..CreateMessage::default()
                            },
                            true,
                        )
                        .await
                        {
                            eprintln!("failed to answer /hello: {error}");
                        }
                    }
                }
            }
            EventKind::MessageCreate(message) => {
                println!("message_create: {}", message.content);
            }
            _ => {}
        }
    }
}

#[cfg(feature = "gateway")]
#[tokio::main]
async fn main() -> Result<(), discordrs::DiscordError> {
    let token = std::env::var("DISCORD_TOKEN")?;
    Client::builder(
        &token,
        gateway_intents::GUILDS | gateway_intents::GUILD_MESSAGES,
    )
    .event_handler(Handler)
    .start()
    .await
}

#[cfg(not(feature = "gateway"))]
fn main() {}
