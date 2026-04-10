#[cfg(feature = "gateway")]
use async_trait::async_trait;
#[cfg(feature = "gateway")]
use discordrs::{
    gateway_intents, respond_with_message, ChatInputCommandInteraction, Client, Context,
    CreateMessage, Event, EventHandler, Interaction,
};

#[cfg(feature = "gateway")]
struct Handler;

#[cfg(feature = "gateway")]
#[async_trait]
impl EventHandler for Handler {
    async fn handle_event(&self, ctx: Context, event: Event) {
        match event {
            Event::Ready(ready) => {
                println!("ready: {}", ready.data.user.username);
            }
            Event::InteractionCreate(event) => {
                if let Interaction::ChatInputCommand(ChatInputCommandInteraction {
                    data,
                    context,
                }) = event.interaction
                {
                    if data.name.as_deref() == Some("hello") {
                        let _ = respond_with_message(
                            ctx.rest().as_ref(),
                            &context,
                            CreateMessage {
                                content: Some("hello from the gateway runtime".to_string()),
                                ..CreateMessage::default()
                            },
                            true,
                        )
                        .await;
                    }
                }
            }
            Event::MessageCreate(message) => {
                println!("message_create: {}", message.message.content);
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
