// A channel overwrite's target is a role or a member, told apart by type:
// a UserId cannot become a role overwrite (or a RoleId a member overwrite).
use std::num::NonZeroU64;

use discordrs::model::OverwriteTarget;
use discordrs::{RoleId, UserId};

fn main() {
    let user_id = UserId::from_nonzero(NonZeroU64::MIN);
    let role_id = RoleId::from_nonzero(NonZeroU64::MIN);
    let _role = OverwriteTarget::Role(user_id);
    let _member = OverwriteTarget::Member(role_id);
}
