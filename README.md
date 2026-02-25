# discordrs

`discordrs` is a Rust library that provides:

- Discord Components V2 builders (`Container`, `TextDisplay`, `MediaGallery`, `Section`, `SelectMenu`, etc.)
- Modal builders and raw interaction response helpers
- Modal `Radio Group`, `Checkbox Group`, and `Checkbox` component builders
- Convenience helpers for sending/editing/followup messages with Components V2

## Install

```toml
[dependencies]
discordrs = "0.1.1"
```

## Modal Example (Radio/Checkbox)

```rust
use discordrs::{
    CheckboxBuilder, CheckboxGroupBuilder, ModalBuilder, RadioGroupBuilder, SelectOption,
};

let modal = ModalBuilder::new("preferences_modal", "Preferences")
    .add_radio_group(
        "Theme",
        Some("Pick one"),
        RadioGroupBuilder::new("theme")
            .add_option(SelectOption::new("Light", "light"))
            .add_option(SelectOption::new("Dark", "dark"))
            .required(true),
    )
    .add_checkbox_group(
        "Notifications",
        Some("Choose any"),
        CheckboxGroupBuilder::new("notify_channels")
            .add_option(SelectOption::new("Email", "email"))
            .add_option(SelectOption::new("Push", "push"))
            .min_values(0)
            .max_values(2),
    )
    .add_checkbox(
        "Agree to Terms",
        None,
        CheckboxBuilder::new("agree_terms").required(true),
    );
```

## Quick Example

```rust
use discordrs::{button_style, create_container, send_container_message, ButtonConfig};
use serenity::http::Http;
use serenity::all::ChannelId;

async fn send_panel(http: &Http, channel_id: ChannelId) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let buttons = vec![
        ButtonConfig::new("open_ticket", "티켓 열기").style(button_style::PRIMARY).emoji("🎫")
    ];

    let container = create_container(
        "지원 패널",
        "아래 버튼을 눌러 티켓을 생성하세요.",
        buttons,
        None,
    );

    send_container_message(http, channel_id, container).await?;
    Ok(())
}
```

## Slash Command Registration Scaffolding

`discordrs` now includes JSON builders for bulk command registration payloads.

```rust
use discordrs::{
    slash_command_registration_payload, CommandOptionBuilder, CommandOptionChoice,
    SlashCommandBuilder,
};

let commands = slash_command_registration_payload(vec![
    SlashCommandBuilder::new("ping", "Latency check")
        .dm_permission(false)
        .add_option(
            CommandOptionBuilder::string("target", "who to ping")
                .required(true)
                .add_choice(CommandOptionChoice::string("all", "all")),
        ),
]);

// Pass `commands` into serenity HTTP bulk overwrite endpoints.
```

## Interaction Dispatch Helper

Use `InteractionRouter` for ergonomic routing by slash command name, component `custom_id`, or prefix patterns.

```rust
use discordrs::{dispatch_interaction, InteractionRouter};

let router = InteractionRouter::new()
    .on_command("ping", "ping_handler")
    .on_component_prefix("ticket:", "ticket_component_handler")
    .on_modal_prefix("ticket_modal:", "ticket_modal_handler");

// inside event handler:
// if let Some(route) = dispatch_interaction(&router, &interaction) { ... }
```

Routing rules:
- Exact match wins first.
- If no exact match, prefix routes are checked.
- Among prefixes, the longest matching prefix wins.

## Notes

- This library uses raw Discord HTTP payloads for Components V2 because serenity does not yet model all V2 structures directly.
- For crates.io publication, ensure the package name `discordrs` is available in your account.
