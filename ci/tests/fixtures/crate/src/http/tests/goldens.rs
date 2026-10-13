// Fixture for the `forward_goldens` check: one case per deprecated forward.
// forward!(GetPinnedMessages, commented_out => get_pins(1)) is ignored.
fn forwards() -> Vec<(Case, Case)> {
    vec![forward!(GetPinnedMessages, pins => get_pins(1))]
}
