#[cfg(feature = "interactions")]
use axum::Router;
#[cfg(feature = "interactions")]
use discordrs::{
    try_interactions_endpoint, DiscordError, EndpointReply, InteractionContext, InteractionHandler,
    InteractionResponse, RawInteraction,
};

#[cfg(feature = "interactions")]
#[derive(Clone)]
struct Handler;

#[cfg(feature = "interactions")]
impl InteractionHandler for Handler {
    async fn handle(&self, _ctx: InteractionContext, interaction: RawInteraction) -> EndpointReply {
        match interaction {
            // Answered inline: the endpoint returns this callback as the
            // body of Discord's request.
            RawInteraction::Command { name, .. } if name.as_deref() == Some("hello") => {
                InteractionResponse::reply("hello from the raw interaction handler").into()
            }
            _ => InteractionResponse::defer(false).into(),
        }
    }
}

#[cfg(feature = "interactions")]
fn build_router(public_key: &str) -> Result<Router, DiscordError> {
    try_interactions_endpoint(public_key, Handler)
}

#[cfg(feature = "interactions")]
fn main() -> Result<(), DiscordError> {
    let public_key = std::env::var("DISCORD_PUBLIC_KEY").unwrap_or_default();
    let _router = build_router(&public_key)?;
    Ok(())
}

#[cfg(not(feature = "interactions"))]
fn main() {}
