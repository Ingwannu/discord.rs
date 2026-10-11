#![allow(
    clippy::expect_used,
    reason = "an example stops with a message when its setup or input is wrong"
)]

#[cfg(feature = "interactions")]
use axum::Router;
#[cfg(feature = "interactions")]
use discordrs::{
    try_typed_interactions_endpoint, ChatInputCommandInteraction, CreateMessage, DiscordError,
    Interaction, InteractionContextData, InteractionResponse, TypedInteractionHandler,
};

#[cfg(feature = "interactions")]
#[derive(Clone)]
struct Handler;

#[cfg(feature = "interactions")]
impl TypedInteractionHandler for Handler {
    async fn handle_typed(
        &self,
        _ctx: InteractionContextData,
        interaction: Interaction,
    ) -> InteractionResponse {
        match interaction {
            Interaction::ChatInputCommand(ChatInputCommandInteraction { data, .. })
                if data.name.as_deref() == Some("hello") =>
            {
                InteractionResponse::ChannelMessage(
                    serde_json::to_value(CreateMessage {
                        content: Some("hello from typed endpoint".to_string()),
                        ..CreateMessage::default()
                    })
                    .expect("create message should serialize"),
                )
            }
            _ => InteractionResponse::DeferredMessage,
        }
    }
}

#[cfg(feature = "interactions")]
fn build_router(public_key: &str) -> Result<Router, DiscordError> {
    try_typed_interactions_endpoint(public_key, Handler)
}

#[cfg(feature = "interactions")]
fn main() -> Result<(), DiscordError> {
    let public_key = std::env::var("DISCORD_PUBLIC_KEY").unwrap_or_default();
    let _router = build_router(&public_key)?;
    Ok(())
}

#[cfg(not(feature = "interactions"))]
fn main() {}
