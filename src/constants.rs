pub mod component_type {
    pub const ACTION_ROW: u8 = 1;
    pub const BUTTON: u8 = 2;
    pub const STRING_SELECT: u8 = 3;
    pub const TEXT_INPUT: u8 = 4;
    pub const USER_SELECT: u8 = 5;
    pub const ROLE_SELECT: u8 = 6;
    pub const MENTIONABLE_SELECT: u8 = 7;
    pub const CHANNEL_SELECT: u8 = 8;
    pub const SECTION: u8 = 9;
    pub const TEXT_DISPLAY: u8 = 10;
    pub const THUMBNAIL: u8 = 11;
    pub const MEDIA_GALLERY: u8 = 12;
    pub const FILE: u8 = 13;
    pub const SEPARATOR: u8 = 14;
    pub const CONTENT_INVENTORY_ENTRY: u8 = 16;
    pub const CONTAINER: u8 = 17;
    pub const LABEL: u8 = 18;
    pub const FILE_UPLOAD: u8 = 19;
    pub const RADIO_GROUP: u8 = 21;
    pub const CHECKBOX_GROUP: u8 = 22;
    pub const CHECKBOX: u8 = 23;
}

pub mod button_style {
    pub const PRIMARY: u8 = 1;
    pub const SECONDARY: u8 = 2;
    pub const SUCCESS: u8 = 3;
    pub const DANGER: u8 = 4;
    pub const LINK: u8 = 5;
}

pub mod separator_spacing {
    pub const SMALL: u8 = 1;
    pub const LARGE: u8 = 2;
}

pub mod text_input_style {
    pub const SHORT: u8 = 1;
    pub const PARAGRAPH: u8 = 2;
}

// Re-export gateway_intents from bitfield module for backward compatibility.
// The canonical definitions now live in src/bitfield.rs.
pub use crate::bitfield::gateway_intents;

/// Legacy constant — prefer `bitfield::message_flags::IS_COMPONENTS_V2` or `MessageFlags::from_bits(1 << 15)`.
pub const MESSAGE_FLAG_IS_COMPONENTS_V2: u64 = 1 << 15;
