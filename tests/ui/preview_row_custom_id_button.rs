// A component embed allows only link buttons, so a preview row takes a
// `PreviewLinkButton`, which has no `custom_id`. A message `ButtonBuilder`
// cannot be added (its conversion goes through a `TryFrom` that rejects
// custom-ID buttons), and a link button cannot be given a custom ID.
use discordrs::request::{PreviewActionRow, PreviewLinkButton};
use discordrs::ButtonBuilder;

fn main() {
    let _row = PreviewActionRow::new()
        .add_button(ButtonBuilder::new().label("Vote").custom_id("vote"));
    let _button = PreviewLinkButton::new("https://example.com")
        .label("Vote")
        .custom_id("vote");
}
