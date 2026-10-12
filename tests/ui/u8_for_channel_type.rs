// Discriminants are open newtypes, not integers: a raw `u8` (or an integer
// literal) is not accepted where a `ChannelType` or `ButtonStyle` is
// expected. Wrap unknown values explicitly with `ChannelType::new`.
use discordrs::permissions::ChannelClass;
use discordrs::CustomButtonBuilder;

fn main() {
    let raw: u8 = 0;
    let _class = ChannelClass::from_channel_type(raw);
    let _button = CustomButtonBuilder::new("go").style(1);
}
