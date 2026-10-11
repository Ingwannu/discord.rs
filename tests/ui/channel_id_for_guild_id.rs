// A ChannelId must not be accepted where a GuildId is required, neither
// directly nor through an implicit marker conversion.
use std::num::NonZeroU64;

use discordrs::{ChannelId, GuildId};

fn needs_guild(_guild_id: GuildId) {}

fn main() {
    let channel_id = ChannelId::from_nonzero(NonZeroU64::MIN);
    needs_guild(channel_id);
    let _converted: GuildId = channel_id.into();
}
