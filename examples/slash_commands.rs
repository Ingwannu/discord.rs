#![allow(
    clippy::expect_used,
    reason = "an example stops with a message when its setup or input is wrong"
)]

use discordrs::{
    ApplicationId, CommandOptionBuilder, Permissions, RestClient, SlashCommandBuilder,
    UserCommandBuilder,
};

fn command_definitions() -> Vec<discordrs::CommandDefinition> {
    let ping = SlashCommandBuilder::new("ping", "Ping the bot")
        .string_option("target", "Optional target label", false)
        .default_member_permissions(Permissions::from_bits(0))
        .build();

    let inspect = UserCommandBuilder::new("Inspect User").build();
    let ban = SlashCommandBuilder::new("ban", "Ban a member")
        .user_option("member", "Member to ban", true)
        .option(CommandOptionBuilder::string("reason", "Reason for the action").required(false))
        .build();

    vec![ping, inspect, ban]
}

#[tokio::main]
async fn main() -> Result<(), discordrs::DiscordError> {
    let token = std::env::var("DISCORD_TOKEN")?;
    // `?` turns a malformed or zero ID into a `DiscordError`.
    let application_id: ApplicationId = std::env::var("DISCORD_APPLICATION_ID")?.parse()?;

    let client = RestClient::new(token, Some(application_id));
    let commands = command_definitions();
    let registered = client.bulk_overwrite_global_commands(&commands).await?;

    println!("registered_commands={}", registered.len());
    Ok(())
}
