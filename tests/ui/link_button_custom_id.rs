// A link button opens a URL and sends no interaction, so neither the typed
// `LinkButton` nor its builder has a `custom_id`.
use discordrs::request::LinkButton;
use discordrs::LinkButtonBuilder;

fn main() {
    let _ = LinkButtonBuilder::new("https://example.com")
        .label("Docs")
        .custom_id("docs");
    let mut button = LinkButton::new("https://example.com");
    button.custom_id = Some("docs".to_owned());
}
