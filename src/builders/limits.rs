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
//!
//! The typed request models in [`crate::request`] validate against the same
//! constants (crate-visible for that reason), so a builder's `try_build()` and
//! the model's `validate()` can never disagree.

/// `custom_id` on buttons, select menus, text inputs, modals, radio groups,
/// checkbox groups, and checkboxes: 1-100 characters. The radio group,
/// checkbox group, and checkbox structures each say "Developer-defined
/// identifier for the input; 1-100 characters".
pub(crate) const MAX_CUSTOM_ID_LENGTH: usize = 100;
/// Button labels: up to 80 characters.
pub(crate) const MAX_BUTTON_LABEL_LENGTH: usize = 80;
/// Components in one action row: 1-5 (five buttons, or one select menu or
/// text input).
pub(crate) const MAX_ACTION_ROW_COMPONENTS: usize = 5;
/// Select menu placeholders: up to 150 characters.
pub(crate) const MAX_SELECT_PLACEHOLDER_LENGTH: usize = 150;
/// String select options: 1-25.
pub(crate) const MAX_SELECT_OPTIONS: usize = 25;
/// Select option labels, values, and descriptions: up to 100 characters.
/// The Radio Group Option and Checkbox Group Option structures give the same
/// limit ("max 100 characters" on `value`, `label`, and `description`), and
/// the builders model all three as `SelectOption`.
pub(crate) const MAX_SELECT_OPTION_TEXT_LENGTH: usize = 100;
/// Upper bound of a select menu's `min_values` (0-25) and `max_values`
/// (1-25).
pub(crate) const MAX_SELECT_VALUES: u8 = 25;
/// What Discord uses for an omitted select menu `min_values` or
/// `max_values`: "Minimum number of items that must be chosen (defaults to
/// 1)" and "Maximum number of items that can be chosen (defaults to 1)" in
/// every select menu structure.
pub(crate) const DEFAULT_SELECT_VALUES: u8 = 1;
/// Components in a Components V2 message, nested ones included: up to 40.
pub(crate) const MAX_COMPONENTS_V2_TOTAL: usize = 40;
/// Text display content: 1-4000 characters.
pub(crate) const MAX_TEXT_DISPLAY_LENGTH: usize = 4000;
/// Media gallery items: 1-10.
pub(crate) const MAX_MEDIA_GALLERY_ITEMS: usize = 10;
/// Media gallery item and thumbnail descriptions: up to 1024 characters.
pub(crate) const MAX_MEDIA_DESCRIPTION_LENGTH: usize = 1024;
/// Text components in a section: 1-3.
pub(crate) const MAX_SECTION_COMPONENTS: usize = 3;

/// Text input labels: 1-45 characters.
pub(crate) const MAX_TEXT_INPUT_LABEL_LENGTH: usize = 45;
/// Text input placeholders: up to 100 characters.
pub(crate) const MAX_TEXT_INPUT_PLACEHOLDER_LENGTH: usize = 100;
/// Text input values, and the upper bound of `min_length` (0-4000) and
/// `max_length` (1-4000).
pub(crate) const MAX_TEXT_INPUT_LENGTH: u16 = 4000;
/// Modal titles: 1-45 characters.
pub(crate) const MAX_MODAL_TITLE_LENGTH: usize = 45;
/// Top-level components in a modal: 1-5.
pub(crate) const MAX_MODAL_COMPONENTS: usize = 5;

// Radio Group and Checkbox Group structures in components/reference
// (discord-api-docs 3714862). The two option-count limits are separate
// constants because the docs list them separately and either may change
// alone.

/// Fewest radio group options: 2. Radio Group Structure, `options`: "List of
/// options to show; min 2, max 10".
pub(crate) const MIN_RADIO_GROUP_OPTIONS: usize = 2;
/// Most radio group options: 10. Radio Group Structure, `options`: "List of
/// options to show; min 2, max 10".
pub(crate) const MAX_RADIO_GROUP_OPTIONS: usize = 10;
/// Checkbox group options: 1-10. Checkbox Group Structure, `options`: "List
/// of options to show; min 1, max 10".
pub(crate) const MAX_CHECKBOX_GROUP_OPTIONS: usize = 10;
/// Upper bound of a checkbox group's `min_values` ("Minimum number of items
/// that must be chosen; min 0, max 10 (defaults to 1)") and `max_values`
/// ("Maximum number of items that can be chosen; min 1, max 10 (defaults to
/// the number of options)"), from the Checkbox Group Structure.
pub(crate) const MAX_CHECKBOX_GROUP_VALUES: u8 = 10;

/// Embed titles: up to 256 characters.
pub(crate) const MAX_EMBED_TITLE_LENGTH: usize = 256;
/// Embed descriptions: up to 4096 characters.
pub(crate) const MAX_EMBED_DESCRIPTION_LENGTH: usize = 4096;
/// Fields per embed: up to 25.
pub(crate) const MAX_EMBED_FIELDS: usize = 25;
/// Embed field names: 1-256 characters.
pub(crate) const MAX_EMBED_FIELD_NAME_LENGTH: usize = 256;
/// Embed field values: 1-1024 characters.
pub(crate) const MAX_EMBED_FIELD_VALUE_LENGTH: usize = 1024;
/// Embed footer text: up to 2048 characters.
pub(crate) const MAX_EMBED_FOOTER_TEXT_LENGTH: usize = 2048;
/// Embed author names: up to 256 characters.
pub(crate) const MAX_EMBED_AUTHOR_NAME_LENGTH: usize = 256;
/// Characters across an embed's title, description, field names and values,
/// footer text, and author name: up to 6000.
pub(crate) const MAX_EMBED_TOTAL_LENGTH: usize = 6000;

// Message-level limits used by `crate::request` validation. Sources are
// discord-api-docs 3714862: `resources/message.mdx` (Create Message,
// Edit Message, Embed Limits, Attachment Request Structure),
// `resources/poll.mdx`, and `components/reference.mdx`.

/// Message `content`: "Message contents (up to 2000 characters)" (Create
/// Message and Edit Message JSON params; Execute Webhook says the same).
pub(crate) const MAX_MESSAGE_CONTENT_LENGTH: usize = 2000;
/// Embeds per message: "Up to 10 `rich` embeds" (Create Message), "array of
/// up to 10 embed objects" (Execute Webhook), "Supports up to 10 embeds"
/// (interaction callback messages).
pub(crate) const MAX_EMBEDS_PER_MESSAGE: usize = 10;
/// Stickers per message: "IDs of up to 3 stickers" (Create Message
/// `sticker_ids`).
pub(crate) const MAX_STICKERS_PER_MESSAGE: usize = 3;
/// Message `nonce`: "up to 25 characters" (Create Message).
pub(crate) const MAX_NONCE_LENGTH: usize = 25;
/// Top-level components of a message without `IS_COMPONENTS_V2`: "Legacy
/// messages allow up to 5 action rows as top-level components"
/// (components/reference, Legacy Message Component Behavior).
pub(crate) const MAX_LEGACY_ACTION_ROWS: usize = 5;
/// Link button `url`: "URL for link-style buttons; max 512 characters"
/// (Button Structure).
pub(crate) const MAX_BUTTON_URL_LENGTH: usize = 512;
/// Container `accent_color`: "RGB from `0x000000` to `0xFFFFFF`" (Container
/// Structure).
pub(crate) const MAX_ACCENT_COLOR: u32 = 0xFF_FFFF;
/// Attachment `description`: "description (alt text) for the file (max 1024
/// characters)" (Attachment Request Structure).
pub(crate) const MAX_ATTACHMENT_DESCRIPTION_LENGTH: usize = 1024;
/// Poll question text: "The maximum length of `text` is 300 for the
/// question, and 55 for any answer" (Poll Media Object).
pub(crate) const MAX_POLL_QUESTION_LENGTH: usize = 300;
/// Poll answer text; see [`MAX_POLL_QUESTION_LENGTH`].
pub(crate) const MAX_POLL_ANSWER_LENGTH: usize = 55;
/// Poll answers: "Each of the answers available in the poll, up to 10"
/// (Poll Create Request Object) and "a maximum of 10 answers per poll".
pub(crate) const MAX_POLL_ANSWERS: usize = 10;
/// Poll `duration` in hours: "Number of hours the poll should be open for,
/// up to 32 days" (Poll Create Request Object): 32 * 24.
pub(crate) const MAX_POLL_DURATION_HOURS: u32 = 32 * 24;
