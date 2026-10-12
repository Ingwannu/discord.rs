use crate::error::DiscordError;
use crate::model::{
    ChannelId, CreateGuildSticker, GuildId, ModifyGuildSticker, SoundboardSound, SoundboardSoundId,
    SoundboardSoundList, Sticker, StickerId, StickerPack, StickerPackId, StickerPackList,
};

use crate::request::{CreateGuildSoundboardSound, ModifyGuildSoundboardSound, SendSoundboardSound};

use super::body::sticker_body;
use super::{json_body, FileAttachment, RestClient, RouteId, NO_BODY};

impl RestClient {
    pub async fn get_sticker(&self, sticker_id: StickerId) -> Result<Sticker, DiscordError> {
        self.send(
            RouteId::GetSticker,
            &format!("/stickers/{}", sticker_id),
            NO_BODY,
        )
        .await
    }

    pub async fn list_sticker_packs(&self) -> Result<StickerPackList, DiscordError> {
        self.send(RouteId::ListStickerPacks, "/sticker-packs", NO_BODY)
            .await
    }

    pub async fn get_sticker_pack(
        &self,
        pack_id: StickerPackId,
    ) -> Result<StickerPack, DiscordError> {
        self.send(
            RouteId::GetStickerPack,
            &format!("/sticker-packs/{}", pack_id),
            NO_BODY,
        )
        .await
    }

    pub async fn get_guild_stickers(
        &self,
        guild_id: GuildId,
    ) -> Result<Vec<Sticker>, DiscordError> {
        self.send(
            RouteId::ListGuildStickers,
            &format!("/guilds/{}/stickers", guild_id),
            NO_BODY,
        )
        .await
    }

    pub async fn get_guild_sticker(
        &self,
        guild_id: GuildId,
        sticker_id: StickerId,
    ) -> Result<Sticker, DiscordError> {
        self.send(
            RouteId::GetGuildSticker,
            &format!("/guilds/{}/stickers/{}", guild_id, sticker_id),
            NO_BODY,
        )
        .await
    }

    /// Uploads a guild sticker. Discord takes this route as a form with
    /// `name`, `description`, and `tags` text fields plus the `file` part,
    /// not as `payload_json`.
    pub async fn create_guild_sticker(
        &self,
        guild_id: GuildId,
        body: &CreateGuildSticker,
        file: FileAttachment,
    ) -> Result<Sticker, DiscordError> {
        self.send(
            RouteId::CreateGuildSticker,
            &format!("/guilds/{}/stickers", guild_id),
            sticker_body(body, file),
        )
        .await
    }

    #[deprecated(since = "3.0.0", note = "use `create_guild_sticker`")]
    pub async fn create_guild_sticker_from_request(
        &self,
        guild_id: GuildId,
        body: &CreateGuildSticker,
        file: FileAttachment,
    ) -> Result<Sticker, DiscordError> {
        self.create_guild_sticker(guild_id, body, file).await
    }

    pub async fn modify_guild_sticker(
        &self,
        guild_id: GuildId,
        sticker_id: StickerId,
        body: &ModifyGuildSticker,
    ) -> Result<Sticker, DiscordError> {
        self.send(
            RouteId::ModifyGuildSticker,
            &format!("/guilds/{}/stickers/{}", guild_id, sticker_id),
            json_body(body),
        )
        .await
    }

    #[deprecated(since = "3.0.0", note = "renamed to `modify_guild_sticker`")]
    pub async fn modify_guild_sticker_from_request(
        &self,
        guild_id: GuildId,
        sticker_id: StickerId,
        body: &ModifyGuildSticker,
    ) -> Result<Sticker, DiscordError> {
        self.modify_guild_sticker(guild_id, sticker_id, body).await
    }

    pub async fn delete_guild_sticker(
        &self,
        guild_id: GuildId,
        sticker_id: StickerId,
    ) -> Result<(), DiscordError> {
        self.send_unit(
            RouteId::DeleteGuildSticker,
            &format!("/guilds/{}/stickers/{}", guild_id, sticker_id),
            NO_BODY,
        )
        .await
    }

    pub async fn send_soundboard_sound(
        &self,
        channel_id: ChannelId,
        body: &SendSoundboardSound,
    ) -> Result<(), DiscordError> {
        self.send_unit(
            RouteId::SendSoundboardSound,
            &format!("/channels/{}/send-soundboard-sound", channel_id),
            json_body(body),
        )
        .await
    }

    pub async fn list_default_soundboard_sounds(
        &self,
    ) -> Result<Vec<SoundboardSound>, DiscordError> {
        self.send(
            RouteId::ListDefaultSoundboardSounds,
            "/soundboard-default-sounds",
            NO_BODY,
        )
        .await
    }

    pub async fn list_guild_soundboard_sounds(
        &self,
        guild_id: GuildId,
    ) -> Result<SoundboardSoundList, DiscordError> {
        self.send(
            RouteId::ListGuildSoundboardSounds,
            &format!("/guilds/{}/soundboard-sounds", guild_id),
            NO_BODY,
        )
        .await
    }

    pub async fn get_guild_soundboard_sound(
        &self,
        guild_id: GuildId,
        sound_id: SoundboardSoundId,
    ) -> Result<SoundboardSound, DiscordError> {
        self.send(
            RouteId::GetGuildSoundboardSound,
            &format!("/guilds/{}/soundboard-sounds/{}", guild_id, sound_id),
            NO_BODY,
        )
        .await
    }

    pub async fn create_guild_soundboard_sound(
        &self,
        guild_id: GuildId,
        body: &CreateGuildSoundboardSound,
    ) -> Result<SoundboardSound, DiscordError> {
        self.send(
            RouteId::CreateGuildSoundboardSound,
            &format!("/guilds/{}/soundboard-sounds", guild_id),
            json_body(body),
        )
        .await
    }

    pub async fn modify_guild_soundboard_sound(
        &self,
        guild_id: GuildId,
        sound_id: SoundboardSoundId,
        body: &ModifyGuildSoundboardSound,
    ) -> Result<SoundboardSound, DiscordError> {
        self.send(
            RouteId::ModifyGuildSoundboardSound,
            &format!("/guilds/{}/soundboard-sounds/{}", guild_id, sound_id),
            json_body(body),
        )
        .await
    }

    pub async fn delete_guild_soundboard_sound(
        &self,
        guild_id: GuildId,
        sound_id: SoundboardSoundId,
    ) -> Result<(), DiscordError> {
        self.send_unit(
            RouteId::DeleteGuildSoundboardSound,
            &format!("/guilds/{}/soundboard-sounds/{}", guild_id, sound_id),
            NO_BODY,
        )
        .await
    }
}
