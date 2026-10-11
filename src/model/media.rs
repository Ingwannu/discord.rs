//! Stickers, sticker packs, and soundboard sounds.

use serde::{Deserialize, Serialize};

use super::{GenericId, User};

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `StickerItem`.
pub struct StickerItem {
    pub id: GenericId,
    pub name: String,
    #[serde(rename = "format_type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `Sticker`.
pub struct Sticker {
    pub id: GenericId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pack_id: Option<GenericId>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: String,
    #[serde(rename = "type")]
    pub kind: u8,
    pub format_type: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<GenericId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<User>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_value: Option<u64>,
}

/// Multipart form fields for creating a guild sticker.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct CreateGuildSticker {
    pub name: String,
    pub description: String,
    pub tags: String,
}

/// JSON request body for modifying a guild sticker.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyGuildSticker {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `StickerPack`.
pub struct StickerPack {
    pub id: GenericId,
    #[serde(default)]
    pub stickers: Vec<Sticker>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sku_id: Option<GenericId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_sticker_id: Option<GenericId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banner_asset_id: Option<GenericId>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `StickerPackList`.
pub struct StickerPackList {
    #[serde(default)]
    pub sticker_packs: Vec<StickerPack>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `SoundboardSound`.
pub struct SoundboardSound {
    pub name: String,
    pub sound_id: GenericId,
    pub volume: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji_id: Option<GenericId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<GenericId>,
    #[serde(default)]
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<User>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `SoundboardSoundList`.
pub struct SoundboardSoundList {
    #[serde(default)]
    pub items: Vec<SoundboardSound>,
}
