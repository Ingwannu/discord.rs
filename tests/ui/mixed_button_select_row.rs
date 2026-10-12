// An action row holds buttons or one select menu, never both: a button row
// takes only buttons, and the 2.x row builder has no way to add a select
// menu next to its buttons (a select menu is its own row).
use discordrs::request::{ButtonRow, StringSelect};
use discordrs::{ActionRowBuilder, CustomButtonBuilder, SelectMenuBuilder};

fn main() {
    let _ = ButtonRow::new().add_button(StringSelect::new("pick"));
    let _ = ActionRowBuilder::new()
        .add_button(CustomButtonBuilder::new("go").label("Go"))
        .add_select_menu(SelectMenuBuilder::string("pick"));
    let _ = ActionRowBuilder::new().add_button(SelectMenuBuilder::string("pick"));
}
