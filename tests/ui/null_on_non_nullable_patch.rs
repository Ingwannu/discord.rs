// Patch::null() exists only for Patch<Nullable<T>>, so a non-nullable field
// such as a bool setting cannot be set to null.
use discordrs::Patch;

fn main() {
    let _inferred: Patch<bool> = Patch::null();
    let _explicit = Patch::<bool>::null();
}
