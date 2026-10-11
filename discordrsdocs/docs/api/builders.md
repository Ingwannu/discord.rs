# Builders API

The builders module gives a fluent API for Components V2 and modal payloads.

## Key Submodules

- `components.rs`: low-level message components (`ButtonBuilder`, `SelectMenuBuilder`, `ActionRowBuilder`)
- `container.rs`: high-level container layouts + helper factories
- `media.rs`: sections, thumbnails, media galleries
- `modal.rs`: text input, radio, checkbox, file upload, labels

## Common Pattern

```rust
use discordrs::{ActionRowBuilder, ButtonBuilder, ButtonStyle, ComponentsV2Message};

let btn = ButtonBuilder::new()
    .label("Open")
    .style(ButtonStyle::PRIMARY)
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
use discordrs::{create_container, ButtonConfig, ButtonStyle};

let buttons = vec![
    ButtonConfig::new("ticket_open", "Open Ticket").style(ButtonStyle::PRIMARY),
    ButtonConfig::new("ticket_status", "Check Status").style(ButtonStyle::SECONDARY),
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

Modal-only components follow Discord's rules: `RadioGroupBuilder`, `CheckboxGroupBuilder`, and `CheckboxBuilder` have no `disabled` setter (a disabled component in a modal is an error), and a checkbox cannot be required, so `CheckboxBuilder` has no `required` either; for a box the user must tick, use a one-option `CheckboxGroupBuilder` with `required(true)`. `CheckboxBuilder::default_checked(bool)` sets the initial state (sent as `default`; the deprecated `checked(bool)` does the same).

Since 3.0 these three builders also have `validate()` / `try_build()` like the others, checking Discord's documented limits: `custom_id` 1-100 characters, radio groups 2-10 options, checkbox groups 1-10 options, option label/value/description at most 100 characters, checkbox group `min_values` 0-10 (0 only with `required(false)`), `max_values` 1-10, and `min_values` not above `max_values`. `build()` stays unchecked, and `LabelBuilder` / `ModalBuilder` do not re-check a component they wrap, so call `validate()` (or use `try_build()`) before handing one to them.

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

## Component Embed (link previews) (3.0)

Since 2026-10-05 a website can replace its Discord link preview with a Components V2 layout by serving a `discord:component-embed` JSON document ([Discord docs](https://docs.discord.com/developers/link-previews/component-embeds)). No bot or app is involved: Discord's crawler reads the document from the page. `discordrs::request::ComponentEmbed` builds it from a restricted model, separate from these message builders and from the receiving side (`Embed::components`):

- `ComponentEmbed { component: PreviewContainer }`. The container holds `PreviewComponent`s: text displays, sections (with a link button or thumbnail accessory), media galleries, separators, and action rows of link buttons.
- `PreviewLinkButton` is the only button. It emits only `id`, `type`, `url`, `style` (always 5), `label`, `emoji`, and `disabled`. Nested containers, select menus, files, custom-ID buttons, and raw JSON cannot be expressed; `PreviewActionRow::add_button(ButtonBuilder)` does not compile.

```rust
use discordrs::request::{
    ComponentEmbed, PreviewActionRow, PreviewLinkButton, PreviewMediaGallery, PreviewMediaItem,
    PreviewSection, PreviewThumbnail,
};

let embed = ComponentEmbed::builder()
    .accent_color(0x1ABC9C)
    .add_component(
        PreviewSection::new(PreviewThumbnail::new("https://example.com/boss.png"))
            .add_text("# New version out!\nThe dungeon is deceptively tougher."),
    )
    .add_component(
        PreviewMediaGallery::new().add_item(PreviewMediaItem::new("https://example.com/1.png")),
    )
    .add_component(
        PreviewActionRow::new()
            .add_button(PreviewLinkButton::new("https://example.com/store").label("Store")),
    )
    .try_build()?;

// Inline: server-render into <head>. Discord does not run scripts, so a tag
// added by client-side code is never seen.
let script = embed.to_script_tag()?;

// Or linked: serve `embed.to_json()?` from the same site and point at it.
let link = ComponentEmbed::link_tag("https://example.com/embeds/v2.json")?;
// <link rel="discord:component-embed" type="application/json" href="…" />
```

`link_tag(href)` writes the URL as the URL parser serializes it (ASCII only: non-ASCII percent-encoded, hosts in punycode), so the element stays valid in XHTML, and it rejects an href with control characters or surrounding spaces, which the parser would silently drop.

Keep the page's Open Graph tags: Discord falls back to them whenever it cannot show the component embed.

`build()` never validates. `try_build()`, `validate()`, `to_json()`, and `to_script_tag()` check the limits below and fail with a `ComponentEmbedError` variant that names the rule and the location (for example `component.components[2].accessory.media.url`). The limits are associated constants on `ComponentEmbed` (`MAX_PAYLOAD_BYTES`, `MAX_COMPONENTS`, ...):

| Rule | Limit | Spec section |
| --- | --- | --- |
| Document size | 3,000 bytes of the escaped JSON, escapes counted at full length | Component Embeds, "Adding the Payload" |
| Components | 40, the container included (buttons, section texts, and thumbnails count) | "Compatible Components" |
| Gallery items | 10 across the whole embed, 1-10 per gallery; thumbnails do not count | "Compatible Components" |
| Media URLs | absolute `http`/`https`, at most 2,048 characters | "Media" |
| Link buttons | label or emoji (or both); label at most 80 characters; URL at most 512 characters | "Compatible Components", Button Structure |
| Sections | 1-3 text displays and an accessory | Section Structure |
| Action rows | 1-5 link buttons | Action Row Structure |
| Media descriptions | at most 1,024 characters | Thumbnail / Media Gallery Item Structure |
| Accent colour | at most `0xFFFFFF` | Container Structure |
| Component `id`s | unique when nonzero | Anatomy of a Component |
| Container | at least one child (as `ContainerBuilder::validate`) | crate rule |

Not checked, because the spec does not require it or it cannot be known here: media file extensions (Discord decides by the fetched content), the host of a `link_tag` href (it must be on the page's host, a subdomain, or the parent domain), and text display length (bounded by the 3,000 bytes).

Escaping: `to_json()` returns compact JSON in which every `<`, `>`, `&`, U+2028, U+2029, U+FFFE, and U+FFFF inside a string is written as `\u003c`, `\u003e`, `\u0026`, `\u2028`, `\u2029`, `\ufffe`, `\uffff`; JSON already escapes the C0 controls. No `</script>` or `<!--` can end or alter the element, and the text stays well-formed inside an XHTML page (which Discord also accepts): with the C0 controls, U+FFFE and U+FFFF are the only characters XML 1.0 forbids that a Rust string can hold. JSON parsers read the escapes back as the same characters. The 3,000-byte limit is checked on exactly this string, and `to_script_tag()` puts it inside the element with no added whitespace. Other non-ASCII text stays UTF-8, so serve the page as UTF-8.

Existing message builders convert with `TryFrom`, failing with the offending path instead of dropping anything: `ComponentEmbed::try_from(container_builder)`, `PreviewContainer::try_from(&value)`, `PreviewComponent::try_from(&section_builder.build())`, and `ComponentEmbed::try_from(&document)` for a hand-written `{"component": ...}` file. Custom-ID or premium buttons, selects, text inputs, files, nested containers, raw components, and unknown keys fail. Conversion is lossless or fails: the spec's nullable fields (`accent_color`, thumbnail and gallery item `description`) are `NullablePatch` and keep an explicit `null` apart from an omitted key, and `null` anywhere else fails with `NullNotAllowed` and its path (emoji keys included, since the shared `Emoji` type cannot hold `null`). Conversion checks shape only; the limits, including separator `spacing` being 1 or 2, are checked on output.

## Practical Advice

- Keep `custom_id` values stable; they are routing keys.
- Use small helper factories to avoid repeating layout blocks.
- Prefer module-level re-exports from `discordrs` root when importing builders.
