// Modal inputs (text input, file upload, radio and checkbox groups, checkbox,
// label) exist only in `request::modal` and implement nothing a message row
// accepts: a button row takes buttons, a select row takes one of the five
// select menus. Putting a text input or a label into a message row is a type
// error, not a Discord 400.
use discordrs::request::{ButtonRow, Checkbox, Label, SelectRow, TextInput};

fn main() {
    let _buttons = ButtonRow::new().add_button(TextInput::short("name"));
    let _select = SelectRow::new(TextInput::short("name"));
    let _label = SelectRow::new(Label::new("Agree", Checkbox::new("agree")));
}
