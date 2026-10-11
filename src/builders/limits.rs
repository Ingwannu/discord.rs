//! Discord's documented component, modal, and embed limits, used by the
//! builders' `validate()` / `try_build()`.
//!
//! Sources: the per-component structures in
//! <https://docs.discord.com/developers/components/reference>, the modal
//! callback data in
//! <https://docs.discord.com/developers/interactions/receiving-and-responding>,
//! and "Embed Limits" in
//! <https://docs.discord.com/developers/resources/message>. As with
//! `command::limits`, local validation is a fail-fast aid, not a gate: when
//! Discord raises a limit, change it here and nowhere else.

/// `custom_id` on buttons, select menus, text inputs, modals, radio groups,
/// checkbox groups, and checkboxes: 1-100 characters. The radio group,
/// checkbox group, and checkbox structures each say "Developer-defined
/// identifier for the input; 1-100 characters".
pub(super) const MAX_CUSTOM_ID_LENGTH: usize = 100;
/// Button labels: up to 80 characters.
pub(super) const MAX_BUTTON_LABEL_LENGTH: usize = 80;
/// Components in one action row: 1-5 (five buttons, or one select menu or
/// text input).
pub(super) const MAX_ACTION_ROW_COMPONENTS: usize = 5;
/// Select menu placeholders: up to 150 characters.
pub(super) const MAX_SELECT_PLACEHOLDER_LENGTH: usize = 150;
/// String select options: 1-25.
pub(super) const MAX_SELECT_OPTIONS: usize = 25;
/// Select option labels, values, and descriptions: up to 100 characters.
/// The Radio Group Option and Checkbox Group Option structures give the same
/// limit ("max 100 characters" on `value`, `label`, and `description`), and
/// the builders model all three as `SelectOption`.
pub(super) const MAX_SELECT_OPTION_TEXT_LENGTH: usize = 100;
/// Upper bound of a select menu's `min_values` (0-25) and `max_values`
/// (1-25).
pub(super) const MAX_SELECT_VALUES: u8 = 25;
/// Components in a Components V2 message, nested ones included: up to 40.
pub(super) const MAX_COMPONENTS_V2_TOTAL: usize = 40;
/// Text display content: 1-4000 characters.
pub(super) const MAX_TEXT_DISPLAY_LENGTH: usize = 4000;
/// Media gallery items: 1-10.
pub(super) const MAX_MEDIA_GALLERY_ITEMS: usize = 10;
/// Media gallery item and thumbnail descriptions: up to 1024 characters.
pub(super) const MAX_MEDIA_DESCRIPTION_LENGTH: usize = 1024;
/// Text components in a section: 1-3.
pub(super) const MAX_SECTION_COMPONENTS: usize = 3;

/// Text input labels: 1-45 characters.
pub(super) const MAX_TEXT_INPUT_LABEL_LENGTH: usize = 45;
/// Text input placeholders: up to 100 characters.
pub(super) const MAX_TEXT_INPUT_PLACEHOLDER_LENGTH: usize = 100;
/// Text input values, and the upper bound of `min_length` (0-4000) and
/// `max_length` (1-4000).
pub(super) const MAX_TEXT_INPUT_LENGTH: u16 = 4000;
/// Modal titles: 1-45 characters.
pub(super) const MAX_MODAL_TITLE_LENGTH: usize = 45;
/// Top-level components in a modal: 1-5.
pub(super) const MAX_MODAL_COMPONENTS: usize = 5;

// Radio Group and Checkbox Group structures in components/reference
// (discord-api-docs 3714862). The two option-count limits are separate
// constants because the docs list them separately and either may change
// alone.

/// Fewest radio group options: 2. Radio Group Structure, `options`: "List of
/// options to show; min 2, max 10".
pub(super) const MIN_RADIO_GROUP_OPTIONS: usize = 2;
/// Most radio group options: 10. Radio Group Structure, `options`: "List of
/// options to show; min 2, max 10".
pub(super) const MAX_RADIO_GROUP_OPTIONS: usize = 10;
/// Checkbox group options: 1-10. Checkbox Group Structure, `options`: "List
/// of options to show; min 1, max 10".
pub(super) const MAX_CHECKBOX_GROUP_OPTIONS: usize = 10;
/// Upper bound of a checkbox group's `min_values` ("Minimum number of items
/// that must be chosen; min 0, max 10 (defaults to 1)") and `max_values`
/// ("Maximum number of items that can be chosen; min 1, max 10 (defaults to
/// the number of options)"), from the Checkbox Group Structure.
pub(super) const MAX_CHECKBOX_GROUP_VALUES: u8 = 10;

/// Embed titles: up to 256 characters.
pub(super) const MAX_EMBED_TITLE_LENGTH: usize = 256;
/// Embed descriptions: up to 4096 characters.
pub(super) const MAX_EMBED_DESCRIPTION_LENGTH: usize = 4096;
/// Fields per embed: up to 25.
pub(super) const MAX_EMBED_FIELDS: usize = 25;
/// Embed field names: 1-256 characters.
pub(super) const MAX_EMBED_FIELD_NAME_LENGTH: usize = 256;
/// Embed field values: 1-1024 characters.
pub(super) const MAX_EMBED_FIELD_VALUE_LENGTH: usize = 1024;
/// Embed footer text: up to 2048 characters.
pub(super) const MAX_EMBED_FOOTER_TEXT_LENGTH: usize = 2048;
/// Embed author names: up to 256 characters.
pub(super) const MAX_EMBED_AUTHOR_NAME_LENGTH: usize = 256;
/// Characters across an embed's title, description, field names and values,
/// footer text, and author name: up to 6000.
pub(super) const MAX_EMBED_TOTAL_LENGTH: usize = 6000;
