#[cfg(feature = "interactions")]
use async_trait::async_trait;
#[cfg(feature = "interactions")]
use axum::Router;
#[cfg(feature = "interactions")]
use discordrs::{
    typed_interactions_endpoint, ChatInputCommandInteraction, CreateMessage, Interaction,
    InteractionContextData, InteractionResponse, TypedInteractionHandler,
};

#[cfg(feature = "interactions")]
#[derive(Clone)]
struct Handler;

#[cfg(feature = "interactions")]
#[async_trait]
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
fn build_router(public_key: &str) -> Router {
    typed_interactions_endpoint(public_key, Handler)
}

#[cfg(feature = "interactions")]
fn main() {
    let public_key = std::env::var("DISCORD_PUBLIC_KEY").unwrap_or_default();
    let _router = build_router(&public_key);
}

#[cfg(not(feature = "interactions"))]
fn main() {}
