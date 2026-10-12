//! Emoji and soundboard request bodies (3.0 phase 5 B).
//!
//! Stickers keep their 2.x bodies in [`crate::model`] (`CreateGuildSticker`,
//! `ModifyGuildSticker`) until checkpoint 5C.

use serde::Serialize;

use super::fields::request_setters;
use crate::model::{EmojiId, GuildId, NullablePatch, Patch, RoleId, SoundboardSoundId};

/// Body of Create Guild Emoji (`POST /guilds/{id}/emojis`).
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct CreateGuildEmoji {
    pub name: String,
    /// The 128x128 image as image data (`data:image/png;base64,...`).
    pub image: String,
    /// Roles allowed to use the emoji; everyone when omitted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roles: Option<Vec<RoleId>>,
}

impl CreateGuildEmoji {
    /// An emoji named `name` from `image` data.
    pub fn new(name: impl Into<String>, image: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            image: image.into(),
            roles: None,
        }
    }

    request_setters! {
        opt roles: Vec<RoleId>;
    }
}

/// Body of Modify Guild Emoji (`PATCH /guilds/{id}/emojis/{id}`).
#[derive(Clone, Debug, Default, Serialize)]
#[non_exhaustive]
pub struct ModifyGuildEmoji {
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub name: Patch<String>,
    /// `null` lets everyone use the emoji again.
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub roles: NullablePatch<Vec<RoleId>>,
}

impl ModifyGuildEmoji {
    /// A modification that changes nothing yet.
    pub fn new() -> Self {
        Self::default()
    }

    request_setters! {
        patch name: String;
        nullable roles / clear_roles: Vec<RoleId>;
    }
}

/// Body of Create Application Emoji (`POST /applications/{id}/emojis`).
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct CreateApplicationEmoji {
    pub name: String,
    /// The 128x128 image as image data (`data:image/png;base64,...`).
    pub image: String,
}

impl CreateApplicationEmoji {
    /// An emoji named `name` from `image` data.
    pub fn new(name: impl Into<String>, image: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            image: image.into(),
        }
    }
}

/// Body of Modify Application Emoji (`PATCH /applications/{id}/emojis/{id}`):
/// only the name can change.
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct ModifyApplicationEmoji {
    pub name: String,
}

impl ModifyApplicationEmoji {
    /// Renames the emoji to `name`.
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

/// Body of Send Soundboard Sound
/// (`POST /channels/{id}/send-soundboard-sound`).
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct SendSoundboardSound {
    pub sound_id: SoundboardSoundId,
    /// Required to play a sound from another server.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_guild_id: Option<GuildId>,
}

impl SendSoundboardSound {
    /// Plays `sound_id` (a default sound or one of the channel's guild).
    pub fn new(sound_id: SoundboardSoundId) -> Self {
        Self {
            sound_id,
            source_guild_id: None,
        }
    }

    request_setters! {
        opt source_guild_id: GuildId;
    }
}

/// Body of Create Guild Soundboard Sound
/// (`POST /guilds/{id}/soundboard-sounds`).
#[derive(Clone, Debug, Serialize)]
#[non_exhaustive]
pub struct CreateGuildSoundboardSound {
    /// 2-32 characters.
    pub name: String,
    /// MP3 or OGG data as a data URI (`data:audio/ogg;base64,...`).
    pub sound: String,
    /// 0 to 1; Discord's default is 1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji_id: Option<EmojiId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji_name: Option<String>,
}

impl CreateGuildSoundboardSound {
    /// A sound named `name` from `sound` data.
    pub fn new(name: impl Into<String>, sound: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            sound: sound.into(),
            volume: None,
            emoji_id: None,
            emoji_name: None,
        }
    }

    request_setters! {
        opt volume: f64;
        opt emoji_id: EmojiId;
        opt emoji_name: String;
    }
}

/// Body of Modify Guild Soundboard Sound
/// (`PATCH /guilds/{id}/soundboard-sounds/{id}`).
#[derive(Clone, Debug, Default, Serialize)]
#[non_exhaustive]
pub struct ModifyGuildSoundboardSound {
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub name: Patch<String>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub volume: NullablePatch<f64>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub emoji_id: NullablePatch<EmojiId>,
    #[serde(skip_serializing_if = "Patch::is_missing")]
    pub emoji_name: NullablePatch<String>,
}

impl ModifyGuildSoundboardSound {
    /// A modification that changes nothing yet.
    pub fn new() -> Self {
        Self::default()
    }

    request_setters! {
        patch name: String;
        nullable volume / clear_volume: f64;
        nullable emoji_id / clear_emoji_id: EmojiId;
        nullable emoji_name / clear_emoji_name: String;
    }
}
