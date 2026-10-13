//! Stickers, sticker packs, and soundboard sounds.

use serde::{Deserialize, Serialize};

use super::{
    EmojiId, GenericId, GuildId, SkuId, SoundboardSoundId, StickerFormatType, StickerId,
    StickerPackId, StickerType, User,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `StickerItem`.
pub struct StickerItem {
    pub id: StickerId,
    pub name: String,
    /// The sticker's `format_type`.
    #[serde(rename = "format_type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<StickerFormatType>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `Sticker`.
pub struct Sticker {
    pub id: StickerId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pack_id: Option<StickerPackId>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: String,
    #[serde(rename = "type")]
    pub kind: StickerType,
    pub format_type: StickerFormatType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<GuildId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<User>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_value: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `StickerPack`.
pub struct StickerPack {
    pub id: StickerPackId,
    #[serde(default)]
    pub stickers: Vec<Sticker>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sku_id: Option<SkuId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_sticker_id: Option<StickerId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// CDN asset ID of the pack's banner image. Assets are not a resource
    /// kind with their own routes, so this stays a [`GenericId`].
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
    pub sound_id: SoundboardSoundId,
    pub volume: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji_id: Option<EmojiId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<GuildId>,
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
