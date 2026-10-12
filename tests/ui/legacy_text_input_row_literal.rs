// The action-row text input layout is deprecated by Discord. The only way to
// build one is the deprecated `LegacyTextInputRow::new` (a deprecation
// warning at the call site); the struct is `#[non_exhaustive]`, so it cannot
// be written as a literal outside the crate, and a `TextInput` has no
// `label` of its own to smuggle the old layout into a Label.
use discordrs::request::{LegacyTextInputRow, TextInput};

fn main() {
    let _row = LegacyTextInputRow {
        id: None,
        label: String::from("Name"),
        input: TextInput::short("name"),
    };
    let _labelled = TextInput::short("name").label("Name");
}
