// The 2.x `component_type`, `button_style`, `separator_spacing`, and
// `text_input_style` constant modules (plain `u8`s) were removed in 3.0
// series 3: the open discriminants `ComponentType`, `ButtonStyle`,
// `SeparatorSpacing`, and `TextInputStyle` in `model::discriminant` own those
// values now, and every builder setter takes them. Keeping both would leave
// two sources for the same numbers (docs/design/3.0-types-events-requests.md
// §2).

// Re-export gateway_intents from bitfield module for backward compatibility.
// The canonical definitions now live in src/bitfield.rs.
pub use crate::bitfield::gateway_intents;

/// Legacy constant ??prefer `bitfield::message_flags::IS_COMPONENTS_V2` or `MessageFlags::from_bits(1 << 15)`.
pub const MESSAGE_FLAG_IS_COMPONENTS_V2: u64 = 1 << 15;
