// Discord requires a section's accessory, and `SectionBuilder` tracks it in
// its type: a section without one cannot be built or added.
use discordrs::{ContainerBuilder, SectionBuilder, TextDisplayBuilder};

fn main() {
    let section = SectionBuilder::new().add_text_display(TextDisplayBuilder::new("text"));
    let _ = ContainerBuilder::new().add_section(section);
    let _ = SectionBuilder::new().build();
}
