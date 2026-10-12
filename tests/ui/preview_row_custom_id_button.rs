// A component embed allows only link buttons, so a preview row takes a
// `PreviewLinkButton`, which has no `custom_id`. A message custom-ID button
// cannot be added (converting a message container goes through a `TryFrom`
// that rejects custom-ID buttons), and a link button cannot be given a
// custom ID.
use discordrs::request::{PreviewActionRow, PreviewLinkButton};
use discordrs::CustomButtonBuilder;

fn main() {
    let _row = PreviewActionRow::new().add_button(CustomButtonBuilder::new("vote").label("Vote"));
    let _button = PreviewLinkButton::new("https://example.com")
        .label("Vote")
        .custom_id("vote");
}
