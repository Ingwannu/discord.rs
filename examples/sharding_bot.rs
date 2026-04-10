#[cfg(feature = "sharding")]
use discordrs::{GatewayConnectionConfig, ShardConfig, ShardingManager, Snowflake};

#[cfg(feature = "sharding")]
fn main() {
    let mut manager =
        ShardingManager::new(ShardConfig::new(4).gateway(GatewayConnectionConfig::default()));
    for shard in manager.shard_infos() {
        let gateway = manager
            .gateway_config(shard.id)
            .expect("shard should produce a gateway config");
        println!("shard={} gateway={}", shard.id, gateway.normalized_url());
    }

    let runtime = manager
        .prepare_runtime(0)
        .expect("first shard runtime should be available");
    runtime
        .publish(discordrs::ShardSupervisorEvent::StateChanged {
            shard_id: 0,
            state: discordrs::ShardRuntimeState::Running,
        })
        .expect("runtime should accept supervisor events");
    println!("runtime_statuses={}", manager.statuses().len());

    let shard = manager
        .shard_for_guild(&Snowflake::from("81384788765712384"))
        .expect("guild id should map to a shard");
    println!("guild_shard={}/{}", shard.id, shard.total);
}

#[cfg(not(feature = "sharding"))]
fn main() {}
