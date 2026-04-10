use discordrs::{
    CommandOptionBuilder, PermissionsBitField, RestClient, SlashCommandBuilder, UserCommandBuilder,
};

fn command_definitions() -> Vec<discordrs::CommandDefinition> {
    let ping = SlashCommandBuilder::new("ping", "Ping the bot")
        .string_option("target", "Optional target label", false)
        .default_member_permissions(PermissionsBitField(0))
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
    let application_id = std::env::var("DISCORD_APPLICATION_ID")?
        .parse()
        .expect("DISCORD_APPLICATION_ID must be a u64");

    let client = RestClient::new(token, application_id);
    let commands = command_definitions();
    let registered = client
        .bulk_overwrite_global_commands_typed(&commands)
        .await?;

    println!("registered_commands={}", registered.len());
    Ok(())
}
