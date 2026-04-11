#[cfg(all(feature = "gateway", feature = "collectors"))]
use async_trait::async_trait;
#[cfg(all(feature = "gateway", feature = "collectors"))]
use discordrs::{gateway_intents, Client, Context, Event, EventHandler};
#[cfg(all(feature = "gateway", feature = "collectors"))]
use std::time::Duration;

#[cfg(all(feature = "gateway", feature = "collectors"))]
struct Handler;

#[cfg(all(feature = "gateway", feature = "collectors"))]
#[async_trait]
impl EventHandler for Handler {
    async fn handle_event(&self, ctx: Context, event: Event) {
        if matches!(event, Event::Ready(_)) {
            let mut collector = ctx
                .collectors()
                .message_collector()
                .filter(|message| !message.content.is_empty())
                .max_items(1)
                .timeout(Duration::from_secs(30));
            tokio::spawn(async move {
                if let Some(message) = collector.next().await {
                    println!("collected message {} {}", message.id, message.content);
                }
            });
        }
    }
}

#[cfg(all(feature = "gateway", feature = "collectors"))]
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

#[cfg(not(all(feature = "gateway", feature = "collectors")))]
fn main() {}
