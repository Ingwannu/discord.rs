# Models and Constants

## Typed Models

The typed surface is centered on:

- `Snowflake`
- `PermissionsBitField`
- `User`
- `Guild`
- `Channel`
- `Member`
- `Role`
- `Message`
- typed interaction variants

These types are designed to replace mixed `String` / `u64` IDs and raw `Value` routing in new code.

## Legacy Utility Types

`src/types.rs` still contains:

- `Error`
- `ButtonConfig`
- `Emoji`
- `SelectOption`
- `MediaGalleryItem`
- `MediaInfo`

These stay relevant for builders and compatibility helpers.

## Constants

Constant groups include:

- component type codes
- button styles
- text input styles
- separator spacing values
- gateway intents

## Advice

- Reference constants instead of hardcoded numeric magic values.
- Keep custom IDs + style values centralized in your app code.
- Prefer typed models for IDs and permission bitfields before dropping to raw JSON.
