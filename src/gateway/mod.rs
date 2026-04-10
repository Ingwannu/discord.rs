mod bot;
mod client;

#[cfg(feature = "sharding")]
pub use bot::ShardSupervisor;
pub use bot::{
    BotClient, BotClientBuilder, Client, ClientBuilder, Context, EventHandler, ShardMessenger,
    TypeMap,
};
