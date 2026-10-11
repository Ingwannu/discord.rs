// Discord defines no default for a discriminant, so the open newtypes have
// no `Default` and a model with a required discriminant cannot derive it.
use discordrs::ChannelType;

#[derive(Default)]
struct StoredChannel {
    kind: ChannelType,
}

fn main() {
    let _kind = ChannelType::default();
    let _stored = StoredChannel::default();
}
