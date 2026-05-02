/// Public module for `component_type` APIs.
pub mod component_type {
    /// Public constant for `ACTION_ROW`.
    pub const ACTION_ROW: u8 = 1;
    /// Public constant for `BUTTON`.
    pub const BUTTON: u8 = 2;
    /// Public constant for `STRING_SELECT`.
    pub const STRING_SELECT: u8 = 3;
    /// Public constant for `TEXT_INPUT`.
    pub const TEXT_INPUT: u8 = 4;
    /// Public constant for `USER_SELECT`.
    pub const USER_SELECT: u8 = 5;
    /// Public constant for `ROLE_SELECT`.
    pub const ROLE_SELECT: u8 = 6;
    /// Public constant for `MENTIONABLE_SELECT`.
    pub const MENTIONABLE_SELECT: u8 = 7;
    /// Public constant for `CHANNEL_SELECT`.
    pub const CHANNEL_SELECT: u8 = 8;
    /// Public constant for `SECTION`.
    pub const SECTION: u8 = 9;
    /// Public constant for `TEXT_DISPLAY`.
    pub const TEXT_DISPLAY: u8 = 10;
    /// Public constant for `THUMBNAIL`.
    pub const THUMBNAIL: u8 = 11;
    /// Public constant for `MEDIA_GALLERY`.
    pub const MEDIA_GALLERY: u8 = 12;
    /// Public constant for `FILE`.
    pub const FILE: u8 = 13;
    /// Public constant for `SEPARATOR`.
    pub const SEPARATOR: u8 = 14;
    /// Public constant for `CONTENT_INVENTORY_ENTRY`.
    pub const CONTENT_INVENTORY_ENTRY: u8 = 16;
    /// Public constant for `CONTAINER`.
    pub const CONTAINER: u8 = 17;
    /// Public constant for `LABEL`.
    pub const LABEL: u8 = 18;
    /// Public constant for `FILE_UPLOAD`.
    pub const FILE_UPLOAD: u8 = 19;
    /// Public constant for `RADIO_GROUP`.
    pub const RADIO_GROUP: u8 = 21;
    /// Public constant for `CHECKBOX_GROUP`.
    pub const CHECKBOX_GROUP: u8 = 22;
    /// Public constant for `CHECKBOX`.
    pub const CHECKBOX: u8 = 23;
}

/// Public module for `button_style` APIs.
pub mod button_style {
    /// Public constant for `PRIMARY`.
    pub const PRIMARY: u8 = 1;
    /// Public constant for `SECONDARY`.
    pub const SECONDARY: u8 = 2;
    /// Public constant for `SUCCESS`.
    pub const SUCCESS: u8 = 3;
    /// Public constant for `DANGER`.
    pub const DANGER: u8 = 4;
    /// Public constant for `LINK`.
    pub const LINK: u8 = 5;
}

/// Public module for `separator_spacing` APIs.
pub mod separator_spacing {
    /// Public constant for `SMALL`.
    pub const SMALL: u8 = 1;
    /// Public constant for `LARGE`.
    pub const LARGE: u8 = 2;
}

/// Public module for `text_input_style` APIs.
pub mod text_input_style {
    /// Public constant for `SHORT`.
    pub const SHORT: u8 = 1;
    /// Public constant for `PARAGRAPH`.
    pub const PARAGRAPH: u8 = 2;
}

// Re-export gateway_intents from bitfield module for backward compatibility.
// The canonical definitions now live in src/bitfield.rs.
pub use crate::bitfield::gateway_intents;

/// Legacy constant ??prefer `bitfield::message_flags::IS_COMPONENTS_V2` or `MessageFlags::from_bits(1 << 15)`.
pub const MESSAGE_FLAG_IS_COMPONENTS_V2: u64 = 1 << 15;
