#[cfg(all(feature = "gateway", feature = "cache"))]
use discordrs::{gateway_intents, Client, Context, Event, EventHandler};

#[cfg(all(feature = "gateway", feature = "cache"))]
struct Handler;

#[cfg(all(feature = "gateway", feature = "cache"))]
impl EventHandler for Handler {
    async fn handle_event(&self, ctx: Context, event: Event) {
        match event {
            Event::Ready(ready) => {
                let cached_guilds = ctx.guilds().list_cached().await.len();
                println!(
                    "ready_user={} cached_guilds={cached_guilds}",
                    ready.data.user.username
                );
            }
            Event::MessageCreate(message) => {
                let fetched = ctx
                    .messages()
                    .get(message.message.channel_id, message.message.id)
                    .await;
                println!("cached_or_rest_message={}", fetched.is_ok());

                if let (Some(guild_id), Some(author)) =
                    (message.message.guild_id, message.message.author.as_ref())
                {
                    // IDs are Copy: pass them by value, no clones.
                    let cached_member = ctx.cache.member(guild_id, author.id).await.is_some();
                    println!("cached_member_lookup={cached_member}");
                }
            }
            _ => {}
        }
    }
}

#[cfg(all(feature = "gateway", feature = "cache"))]
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

#[cfg(not(all(feature = "gateway", feature = "cache")))]
fn main() {}
