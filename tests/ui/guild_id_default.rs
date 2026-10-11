// IDs have no Default, so no sentinel ID can be created and stored models
// with a required ID cannot derive Default.
use discordrs::GuildId;

#[derive(Default)]
struct StoredGuild {
    id: GuildId,
}

fn main() {
    let _guild_id = GuildId::default();
    let _stored = StoredGuild::default();
}
