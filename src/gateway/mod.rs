mod bot;
mod client;
mod compression;
mod handoff;
mod messenger;
mod outbound;
#[cfg(feature = "sharding")]
mod supervisor;

#[cfg(feature = "cache")]
pub use bot::CacheBackendHealth;
pub use bot::{
    BotClient, BotClientBuilder, Client, ClientBuilder, Context, EventDispatchMode, EventHandler,
    InteractionOverloadReason, OverloadedInteraction, TypeMap,
};
pub use handoff::{GatewayHandoff, GatewaySession};
pub use messenger::ShardMessenger;
#[cfg(feature = "sharding")]
pub use supervisor::ShardSupervisor;
