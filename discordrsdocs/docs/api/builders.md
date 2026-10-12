# Builders API

The builders module gives a fluent API for message bodies, Components V2 layouts, embeds, and modals. Since 3.0 (phase 5) every message builder holds a typed request model from `discordrs::request` and `build()` returns it; nothing is assembled as `serde_json::Value`. The models serialize straight to the bytes the REST client sends.

## Key Submodules

- `message.rs`: `MessageBuilder` (builds a `request::CreateMessage`)
- `components.rs`: buttons (`CustomButtonBuilder`, `LinkButtonBuilder`, `PremiumButtonBuilder`; the 2.x `ButtonBuilder` is deprecated), `ActionRowBuilder` (a row of buttons), select menus (`SelectMenuBuilder::string/user/role/mentionable/channel` return `StringSelectBuilder`, `UserSelectBuilder`, `RoleSelectBuilder`, `MentionableSelectBuilder`, `ChannelSelectBuilder`), `ComponentsV2Message`
- `container.rs`: `ContainerBuilder`, `TextDisplayBuilder`, `SeparatorBuilder`, and the `create_container` helper
- `media.rs`: `SectionBuilder`, `ThumbnailBuilder`, `MediaGalleryBuilder`, `FileBuilder`
- `embed.rs`: `EmbedBuilder` (builds a `request::Embed`)
- `modal.rs`: modal, labels, text input, radio, checkbox, file upload (build the `request` modal models)

| Builder | `build()` / `try_build()` returns |
| --- | --- |
| `MessageBuilder` | `request::CreateMessage` (converts into `InteractionMessage`, `CreateFollowup`; `TryFrom` into `UpdateMessage`, `EditInteractionResponse`) |
| `ComponentsV2Message` | `Vec<request::Component>` |
| `ContainerBuilder` | `request::Container` |
| `ActionRowBuilder` | `request::ActionRow` (a `ButtonRow`) |
| `StringSelectBuilder` / `UserSelectBuilder` / `RoleSelectBuilder` / `MentionableSelectBuilder` / `ChannelSelectBuilder` | `StringSelect` / `UserSelect` / `RoleSelect` / `MentionableSelect` / `ChannelSelect` (each converts into `SelectMenu`, `ActionRow`, `Component`) |
| `CustomButtonBuilder` / `LinkButtonBuilder` / `PremiumButtonBuilder` | `CustomButton` / `LinkButton` / `PremiumButton` |
| `SectionBuilder<SectionAccessory>` | `request::Section` |
| `TextDisplayBuilder`, `SeparatorBuilder`, `ThumbnailBuilder`, `MediaGalleryBuilder`, `FileBuilder` | the model of the same name (`FileComponent` for files) |
| `EmbedBuilder` | `request::Embed` |
| `ModalBuilder` | `request::Modal` (inputs wrapped in Labels) |
| `TextInputBuilder` / `LabelBuilder` | `request::Label` (`TextInputBuilder` builds the Label around its text input) |
| `RadioGroupBuilder`, `CheckboxGroupBuilder`, `CheckboxBuilder`, `FileUploadBuilder` | `RadioGroup`, `CheckboxGroup`, `Checkbox`, `FileUpload` |

## Common Pattern

```rust
use discordrs::{
    ActionRowBuilder, ButtonStyle, ComponentsV2Message, ContainerBuilder, CustomButtonBuilder,
    LinkButtonBuilder, MessageBuilder, SelectMenuBuilder, SelectOption, TextDisplayBuilder,
};
use discordrs::request::CreateMessage;

let buttons = ActionRowBuilder::new()
    .add_button(CustomButtonBuilder::new("open_ticket").label("Open").style(ButtonStyle::PRIMARY))
    .add_button(LinkButtonBuilder::new("https://example.com/help").label("Help"));

let container = ContainerBuilder::new()
    .add_text_display(TextDisplayBuilder::new("## Support"))
    .add_action_row(buttons)
    // A select menu is its own row: pass the select builder where a row goes.
    .add_action_row(SelectMenuBuilder::string("topic").add_option(SelectOption::new("Billing", "billing")));

let message: CreateMessage = MessageBuilder::new()
    .components_v2(ComponentsV2Message::new().add_container(container))
    .try_build()?;
```

`components_v2(..)` sets the `IS_COMPONENTS_V2` flag. Every builder also converts into its position with `Into` (a container into a `Component`, a button builder into a `Button` or `SectionAccessory`, a select builder into a `SelectMenu` or `ActionRow`), so builders and typed models mix freely. No builder drops a value: what its model cannot hold either has no setter or fails a conversion.

### Buttons

A button is one of three kinds, each with only the fields Discord allows it:

- `CustomButtonBuilder::new(custom_id)`: primary (default), secondary, success, or danger; sends an interaction.
- `LinkButtonBuilder::new(url)`: style 5, opens the URL; there is no `custom_id` setter.
- `PremiumButtonBuilder::new(sku_id)`: style 6, opens the SKU's purchase flow; there is no label, emoji, URL, or `custom_id`.

A link button with a `custom_id` or a premium button with a label does not compile (`tests/ui/link_button_custom_id.rs`, `premium_button_label_url.rs`).

### Rows and select menus

A row holds 1-5 buttons or exactly one select menu. `ActionRowBuilder` builds a row of buttons; a select menu row is the `SelectMenuBuilder` itself, accepted wherever a row is (`add_action_row(impl Into<ActionRow>)`). A row mixing buttons and a select menu does not compile (`tests/ui/mixed_button_select_row.rs`).

Auto-populated selects support Discord's `default_values`, `id`, and modal `required` fields:

```rust
use discordrs::{ComponentId, ComponentsV2Message, RoleId, SelectMenuBuilder};

// Default values carry typed IDs: a mentionable select takes users and
// roles, a user select only users, a channel select only channels.
let moderators: RoleId = "701".parse()?;
let message = ComponentsV2Message::new().add_action_row(
    SelectMenuBuilder::mentionable("notify_targets")
        .id(ComponentId::new(12))
        .default_role(moderators)
        .min_values(0)
        .max_values(2),
);
```

`SelectMenuBuilder::string(..)`, `::user(..)`, `::role(..)`, `::mentionable(..)`, and `::channel(..)` return one builder per kind, and each has only the setters its kind takes: `add_option` / `add_options` on string selects, `default_user` on user and mentionable selects, `default_role` on role and mentionable selects, `default_mentionable` on mentionable selects, `default_channel` and `channel_types` on channel selects, and `default_values(ids)` with the kind's own id type. A value the kind cannot carry does not compile (`tests/ui/select_menu_wrong_kind_fields.rs`), so nothing is dropped between the builder and the wire. `required` is modal-only (Discord ignores it in messages); `disabled` is message-only (a modal rejects it).

### Sections

Discord requires a section's accessory, and `SectionBuilder` tracks it in its type: `SectionBuilder::new()` is a `SectionBuilder<()>`, and `set_thumbnail_accessory` / `set_button_accessory` make it a `SectionBuilder<SectionAccessory>`, the only state with `build()` and the only one `add_section` accepts. Text can be added before or after the accessory.

### Raw components

Anything this version cannot express goes through `request::RawComponent` (`from_json`, `from_value`), accepted by every `add_component`. It is written verbatim (it holds a `serde_json::value::RawValue`, not a `Value` tree), counts as one component in validation, and is not otherwise checked.

## Message Bodies (`discordrs::request`)

Each endpoint family has its own body type, holding only the fields that endpoint accepts:

| Type | Endpoint | Semantics |
| --- | --- | --- |
| `CreateMessage` | `POST /channels/{id}/messages` | create; reply/forward, stickers, nonce, `shared_client_theme` |
| `EditMessage` | `PATCH /channels/{id}/messages/{id}` | patch; no nonce, stickers, reply, tts, poll (`clear_stickers()` / `clear_poll()` only) |
| `InteractionMessage` | callback type 4 `data` | create; ephemeral flag, poll |
| `UpdateMessage` | callback type 7 `data` | patch on the component's message |
| `CreateFollowup` | `POST /webhooks/{app}/{token}` | create; ephemeral flag, poll |
| `ExecuteWebhook` | `POST /webhooks/{id}/{token}` | create; username, avatar, thread name/tags |
| `EditWebhookMessage` (= `EditInteractionResponse`) | `PATCH .../messages/{id or @original}` | patch; attachments listed explicitly, poll |

Create bodies use `Option` (`None` omits the key; `Some(vec![])` writes `[]`). Patch bodies use `NullablePatch`: omitted keeps the current value, `null` clears it, a value replaces it. Query parameters (`wait`, `thread_id`, `with_components`) are in `WebhookExecuteQuery` / `WebhookMessageQuery`. Each body also has setters (`content`, `add_embed`, `components_v2`, `ephemeral`, ...), `From<&str>`, and conversions that fail instead of dropping a field (`InteractionMessage::try_from(create_message)` fails on a reply reference, stickers, or a nonce).

Attachments are `MessageAttachment::Existing(ExistingAttachment { id: AttachmentId, .. })` (keep a stored attachment) or `MessageAttachment::Upload(AttachmentUpload { id: UploadId, .. })` (describe `files[n]`); the two id types cannot be swapped. `request::Embed` and `request::CreatePoll` are the outgoing embed and poll (a poll question is text only).

## Validation

`build()` never checks limits; `try_build()` checks them and returns the same type; `validate()` checks without consuming. A builder delegates to its model's `validate()`, and a parent validates its children recursively, so builder and model limits cannot disagree. The message bodies add the message-wide rules:

| Rule | Limit | Source |
| --- | --- | --- |
| Content | 2000 characters | Create Message |
| Embeds | 10 per message, 6000 characters across all of them | Embed Limits |
| Stickers / nonce | 3 / 25 characters | Create Message |
| Flags | only those the endpoint accepts (unknown bits pass) | each endpoint's `flags` row |
| `IS_COMPONENTS_V2` | no content, embeds, poll, stickers, or shared theme | Create Message footnote |
| Legacy messages | only action rows, at most 5 | Legacy Message Component Behavior |
| Components V2 | at most 40 components, nested ones included | components/reference |
| `id` / `custom_id` | unique within the message (`id` 0 may repeat) | components/reference |
| Voice messages | one `Upload` attachment with `duration_secs` and `waveform`, nothing else; the file must be `audio/*` (`check_voice_files`) | Voice Messages |
| Select menus | `min_values` ≤ `max_values` with omitted ones read as 1; defaults within that range | select menu structures |

An edit that does not set `IS_COMPONENTS_V2` keeps the message in whatever mode it has, so only the rules both modes share are checked. The limits are named constants in `builders/limits.rs`, each citing its documentation source.

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
use discordrs::{CheckboxBuilder, ModalBuilder, RadioGroupBuilder, SelectOption, TextInputBuilder};

let modal = ModalBuilder::new("preferences_modal", "Preferences")
    // Every input goes into a Label; a text input uses its builder's label text.
    .add_text_input(TextInputBuilder::short("nick", "Nickname").description("Shown to others"))
    .add_radio_group(
        "Theme",
        Some("Pick one"),
        RadioGroupBuilder::new("theme")
            .add_option(SelectOption::new("Light", "light"))
            .add_option(SelectOption::new("Dark", "dark"))
            .required(true),
    )
    .add_text_display("You can change these later.")
    .add_checkbox("Newsletter", None, CheckboxBuilder::new("news"))
    .try_build()?; // -> request::Modal
```

Since 3.0 the modal builders hold the typed request models in `discordrs::request` and `build()` returns them instead of JSON: `ModalBuilder::build()` is a `Modal`, `TextInputBuilder::build()` the `Label` around the text input (Discord moved the input's label to the Label component), `LabelBuilder::build()` a `Label`, and `RadioGroupBuilder` / `CheckboxGroupBuilder` / `CheckboxBuilder` / `FileUploadBuilder` their `RadioGroup` / `CheckboxGroup` / `Checkbox` / `FileUpload`. `show_modal` and `InteractionResponse::modal` take a `Modal` or a `ModalBuilder`.

The modal model is exactly what Discord accepts:

| Type | Holds |
|---|---|
| `Modal` | `custom_id` (1-100), `title` (1-45), 1-5 `ModalComponent`s |
| `ModalComponent` | `Label`, `TextDisplay`, `Raw(RawComponent)`, and `LegacyTextInputRow` (see below) |
| `Label` | `label` (1-45), optional `description` (up to 100), one `LabelComponent` |
| `LabelComponent` | `TextInput`, `StringSelect`, `UserSelect`, `RoleSelect`, `MentionableSelect`, `ChannelSelect`, `FileUpload`, `RadioGroup`, `CheckboxGroup`, `Checkbox`, `Raw` |

Modal inputs cannot go into a message row (a compile error), and message-only components (buttons, sections, containers) have no place in a modal. A text input in an action row is the layout Discord deprecated; it is only produced by the deprecated `ModalBuilder::add_text_input_row` / `LegacyTextInputRow::new`, and it is the only place a text input carries its own `label`. `ModalBuilder::add_component` takes any `ModalComponent` (a `RawComponent::from_json(..)` for a component type this version does not model); the 2.x `add_component(Value)` and `add_action_row(ActionRowBuilder)` are gone.

Modal-only components follow Discord's rules: `RadioGroupBuilder`, `CheckboxGroupBuilder`, and `CheckboxBuilder` have no `disabled` setter (a disabled component in a modal is an error), and a checkbox cannot be required, so `CheckboxBuilder` has no `required` either; for a box the user must tick, use a one-option `CheckboxGroupBuilder` with `required(true)`. `CheckboxBuilder::default_checked(bool)` sets the initial state (sent as `default`; the deprecated `checked(bool)` does the same).

`Modal::validate()` (and `ModalBuilder::validate()` / `try_build()`) checks the whole modal against Discord's documented limits: the modal's `custom_id`, `title`, and 1-5 components; label text and description; text input `custom_id`, placeholder (100), value (4000), `min_length` 0-4000, `max_length` 1-4000; file upload `min_values` 0-10 (0 only with `required(false)`), `max_values` 1-10, and at most 10 `file_types`; radio groups 2-10 options, checkbox groups 1-10 options, option label/value/description at most 100 characters and no emoji (the group options have none); checkbox group `min_values` 0-10 (0 only with `required(false)`) and `max_values` 1-10; `min_values` not above `max_values`, where an omitted `max_values` counts as Discord's default (1 for a file upload, the number of options for a checkbox group); a select's own limits plus the modal rules (no `disabled`; `min_values: 0` only with `required: false`); text display content 1-4000; and no `custom_id` or non-zero component `id` used twice. Raw components are sent as written and not inspected. The single-component builders' `validate()` check that component alone; `build()` stays unchecked.

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
- `PreviewLinkButton` is the only button. It emits only `id`, `type`, `url`, `style` (always 5), `label`, `emoji`, and `disabled`. Nested containers, select menus, files, custom-ID buttons, and raw JSON cannot be expressed; `PreviewActionRow::add_button(CustomButtonBuilder)` does not compile.

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

Existing message builders convert with `TryFrom`, failing with the offending path instead of dropping anything: `ComponentEmbed::try_from(container_builder)` and `PreviewContainer::try_from(container)` convert the typed message container field by field; `PreviewContainer::try_from(&value)`, `PreviewComponent::try_from(&value)`, and `ComponentEmbed::try_from(&document)` read JSON (a hand-written `{"component": ...}` file, for example), and a `RawComponent` child of a container goes through the same reader. Custom-ID or premium buttons, selects, text inputs, files, nested containers, non-preview raw components, and unknown keys fail. Conversion is lossless or fails: the spec's nullable fields (`accent_color`, thumbnail and gallery item `description`) are `NullablePatch` and keep an explicit `null` apart from an omitted key, and `null` anywhere else fails with `NullNotAllowed` and its path (emoji keys included, since the shared `Emoji` type cannot hold `null`). Conversion checks shape only; the limits, including separator `spacing` being 1 or 2, are checked on output.

## Migrating from 2.x builders

| 2.x | 3.0 |
| --- | --- |
| `ButtonBuilder::new().style(..).custom_id(id)` | `CustomButtonBuilder::new(id).style(..)` |
| `ButtonBuilder::new().url(url)` | `LinkButtonBuilder::new(url)` |
| premium style on `ButtonBuilder` (no SKU) | `PremiumButtonBuilder::new(sku_id)` |
| keeping a `ButtonBuilder` | deprecated; `Button::try_from(builder)?` or `builder.build()?` (fails for a premium style or a missing `custom_id`) |
| `ActionRowBuilder::new().add_select_menu(menu)` | pass `menu` where the row goes (`add_action_row(menu)`) |
| `ActionRowBuilder::add_text_input` / `add_component(Value)` | removed; text inputs belong to modals, raw JSON is `RawComponent` |
| `add_component(json!(..))` on containers and messages | `add_component(RawComponent::from_value(&json!(..))?)` |
| `SectionBuilder::new()` without an accessory, checked at `validate()` | does not compile where a section is added; set the accessory |
| `X::build() -> serde_json::Value` | `X::build()` returns the typed model; `serde_json::to_value(&x)` if JSON is needed |
| `MessageBuilder::build() -> model::CreateMessage`, `components(Vec<Value>)` | `-> request::CreateMessage`, `components(Vec<Component>)` / `add_component(..)` |
| `MessageBuilder::flags(u64)` | `flags(impl Into<MessageFlags>)` (`u64` still works) |
| `EmbedBuilder::build() -> Value` | `-> request::Embed`; `MessageBuilder::add_embed(builder)` |
| `SelectMenuBuilder` (one type) silently dropped fields of other kinds | its constructors return per-kind builders (`StringSelectBuilder`, ...) without those setters; a wrong-kind value does not compile |
| `default_value(SelectDefaultValue::role(id))`, `default_values(vec![SelectDefaultValue::..])` | `default_role(id)` (or `default_user`, `default_channel`, `default_mentionable`), `default_values(ids)` with the kind's id type |
| `SelectMenuBuilder` as a parameter or return type | the per-kind builder, or `impl Into<SelectMenu>` |
| `message_flags::SUPPRESS_NOTIFICATIONS` = 1 << 10, `IS_VOICE_MESSAGE` = 1 << 12 | 1 << 12 and 1 << 13 (Discord's values); `HAS_SNAPSHOT` = 1 << 14 added |

## Practical Advice

- Keep `custom_id` values stable; they are routing keys.
- Use small helper factories to avoid repeating layout blocks.
- Prefer module-level re-exports from `discordrs` root when importing builders.
