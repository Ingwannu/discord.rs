// Each select menu kind holds only its own values, so a value it cannot
// carry has no setter instead of being dropped on the way to the wire: a
// user select takes no role default, a role select no options, a string
// select no channel types or user defaults, and a channel select no role
// default.
use discordrs::model::{RoleId, UserId};
use discordrs::{SelectMenuBuilder, SelectOption};

fn selects(user: UserId, role: RoleId) {
    let _ = SelectMenuBuilder::user("u").default_role(role);
    let _ = SelectMenuBuilder::role("r").add_option(SelectOption::new("a", "a"));
    let _ = SelectMenuBuilder::string("s").channel_types([]);
    let _ = SelectMenuBuilder::string("s").default_user(user);
    let _ = SelectMenuBuilder::channel("c").default_role(role);
    let _ = SelectMenuBuilder::user("u").default_values([role]);
}

fn main() {}
