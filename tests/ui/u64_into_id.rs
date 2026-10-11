// Raw integers never become IDs implicitly; construction goes through
// try_new / TryFrom / FromStr, which reject zero.
use discordrs::GuildId;

fn needs_guild(_guild_id: GuildId) {}

fn main() {
    needs_guild(81384788765712384);
    let _from: GuildId = GuildId::from(81384788765712384_u64);
    let _into: GuildId = 81384788765712384_u64.into();
}
