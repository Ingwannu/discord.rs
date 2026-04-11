#[cfg(feature = "interactions")]
use async_trait::async_trait;
#[cfg(feature = "interactions")]
use axum::Router;
#[cfg(feature = "interactions")]
use discordrs::{
    create_container, try_interactions_endpoint, CreateMessage, InteractionContext,
    InteractionHandler, InteractionResponse, RawInteraction,
};

#[cfg(feature = "interactions")]
#[derive(Clone)]
struct Handler;

#[cfg(feature = "interactions")]
#[async_trait]
impl InteractionHandler for Handler {
    async fn handle(
        &self,
        _ctx: InteractionContext,
        interaction: RawInteraction,
    ) -> InteractionResponse {
        match interaction {
            RawInteraction::Command { name, .. } if name.as_deref() == Some("hello") => {
                let _ = _ctx;
                InteractionResponse::ChannelMessage(serde_json::json!({
                    "components": [create_container("Hello", "From discordrs", vec![], None).build()],
                    "content": CreateMessage {
                        content: Some("legacy raw interaction handler".to_string()),
                        ..CreateMessage::default()
                    }.content,
                    "flags": 1 << 15,
                }))
            }
            _ => InteractionResponse::DeferredMessage,
        }
    }
}

#[cfg(feature = "interactions")]
fn build_router(public_key: &str) -> Router {
    try_interactions_endpoint(public_key, Handler).expect("invalid Discord public key")
}

#[cfg(feature = "interactions")]
fn main() {
    let public_key = std::env::var("DISCORD_PUBLIC_KEY").unwrap_or_default();
    let _router = build_router(&public_key);
}

#[cfg(not(feature = "interactions"))]
fn main() {}
