# Builders API

The builders module gives a fluent API for Components V2 and modal payloads.

## Key Submodules

- `components.rs`: low-level message components (`ButtonBuilder`, `SelectMenuBuilder`, `ActionRowBuilder`)
- `container.rs`: high-level container layouts + helper factories
- `media.rs`: sections, thumbnails, media galleries
- `modal.rs`: text input, radio, checkbox, file upload, labels

## Common Pattern

```rust
use discordrs::{ActionRowBuilder, ButtonBuilder, ComponentsV2Message, button_style};

let btn = ButtonBuilder::new()
    .label("Open")
    .style(button_style::PRIMARY)
    .custom_id("open_ticket");

let row = ActionRowBuilder::new().add_button(btn);
let message = ComponentsV2Message::new().add_action_row(row);
```

Auto-populated selects support Discord's `default_values`, `id`, and modal `required` fields:

```rust
use discordrs::{ActionRowBuilder, ComponentId, RoleId, SelectDefaultValue, SelectMenuBuilder};

// Default values carry typed IDs (`SelectDefaultValue::{User, Role, Channel}`),
// so a role cannot be listed as a user; `default_user`, `default_role`,
// `default_channel`, and `default_mentionable` are shorthands.
let moderators: RoleId = "701".parse()?;
let row = ActionRowBuilder::new().add_select_menu(
    SelectMenuBuilder::mentionable("notify_targets")
        .id(ComponentId::new(12))
        .default_value(SelectDefaultValue::role(moderators))
        .min_values(0)
        .max_values(2)
        .required(false),
);
```

## Container Helper

```rust
use discordrs::{create_container, ButtonConfig, button_style};

let buttons = vec![
    ButtonConfig::new("ticket_open", "Open Ticket").style(button_style::PRIMARY),
    ButtonConfig::new("ticket_status", "Check Status").style(button_style::SECONDARY),
];

let container = create_container(
    "Support Panel",
    "Use controls below.",
    buttons,
    None,
);
```

## Modal Helper

```rust
use discordrs::{ModalBuilder, RadioGroupBuilder, SelectOption};

let modal = ModalBuilder::new("preferences_modal", "Preferences")
    .add_radio_group(
        "Theme",
        Some("Pick one"),
        RadioGroupBuilder::new("theme")
            .add_option(SelectOption::new("Light", "light"))
            .add_option(SelectOption::new("Dark", "dark"))
            .required(true),
    );
```

## Runtime Validation (2.2.0)

Every command, button, select-menu, action-row, embed, modal, container, and media builder exposes `validate()` (check without consuming) and `try_build()` (validate, then build). They enforce Discord's documented limits — codepoint-counted lengths, option/choice/field counts, action-row composition, the 6000-character embed total, and Components V2 caps — with errors that name the field, the limit, and the actual value. Plain `build()` remains non-validating:

```rust
use discordrs::{EmbedBuilder, ModalBuilder, SlashCommandBuilder};

// try_build() = validate + build.
let command = SlashCommandBuilder::new("ticket", "Create a support ticket").try_build()?;

let embed = EmbedBuilder::new()
    .title("Status")
    .description("All systems nominal")
    .try_build()?;

// validate() checks without consuming the builder.
let modal = ModalBuilder::new("feedback", "Feedback");
if let Err(error) = modal.validate() {
    eprintln!("invalid modal: {error}");
}
```

A too-long label or an overfull action row fails locally with an actionable message instead of a Discord 400 after the HTTP round trip.

## File Type Filters (3.0)

`FileUploadBuilder::file_types(...)` and `CommandOptionBuilder::file_types(...)` (on `ATTACHMENT` options) restrict uploads to preset groups (`"image"`, `"video"`, `"audio"`) or dot-prefixed extensions (`".pdf"`), up to 10 entries. `validate()` / `try_build()` check the count and the shape of each entry; group names are not checked against a fixed list because Discord says the presets may change. Discord compares only the file name, so validate file contents yourself.

```rust
use discordrs::builders::FileUploadBuilder;
use discordrs::command::CommandOptionBuilder;

let upload = FileUploadBuilder::new("evidence").file_types(["image", ".pdf"]).try_build()?;
let option = CommandOptionBuilder::attachment("file", "Upload a clip").file_types(["video"]);
```

## Practical Advice

- Keep `custom_id` values stable; they are routing keys.
- Use small helper factories to avoid repeating layout blocks.
- Prefer module-level re-exports from `discordrs` root when importing builders.
