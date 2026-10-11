// A guild endpoint takes a GuildId; a ChannelId (the most common mix-up,
// since many payloads carry both) must not compile there.
use std::num::NonZeroU64;

use discordrs::{ChannelId, RestClient};

fn fetch(http: &RestClient) {
    let channel_id = ChannelId::from_nonzero(NonZeroU64::MIN);
    let _guild = http.get_guild(channel_id);
}

fn main() {
    fetch(&RestClient::new("token", None));
}
