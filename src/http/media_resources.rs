use reqwest::Method;
use serde_json::Value;

use crate::error::DiscordError;
use crate::model::{
    ChannelId, CreateGuildSticker, GuildId, ModifyGuildSticker, SoundboardSound, SoundboardSoundId,
    SoundboardSoundList, Sticker, StickerId, StickerPack, StickerPackId, StickerPackList,
};

use super::{decode_response, serialize_body, FileAttachment, RequestBody, RestClient};

impl RestClient {
    pub async fn get_sticker(&self, sticker_id: StickerId) -> Result<Sticker, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/stickers/{}", sticker_id),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn list_sticker_packs(&self) -> Result<StickerPackList, DiscordError> {
        self.request_typed(Method::GET, "/sticker-packs", Option::<&Value>::None)
            .await
    }

    pub async fn get_sticker_pack(
        &self,
        pack_id: StickerPackId,
    ) -> Result<StickerPack, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/sticker-packs/{}", pack_id),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_guild_stickers(
        &self,
        guild_id: GuildId,
    ) -> Result<Vec<Sticker>, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/stickers", guild_id),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_guild_sticker(
        &self,
        guild_id: GuildId,
        sticker_id: StickerId,
    ) -> Result<Sticker, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/stickers/{}", guild_id, sticker_id),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn create_guild_sticker(
        &self,
        guild_id: GuildId,
        body: &Value,
        file: FileAttachment,
    ) -> Result<Sticker, DiscordError> {
        let path = format!("/guilds/{}/stickers", guild_id);
        decode_response(
            self.request_with_headers(
                Method::POST,
                &path,
                Some(RequestBody::StickerMultipart {
                    payload_json: body.clone(),
                    file,
                }),
            )
            .await,
        )
    }

    pub async fn create_guild_sticker_typed<B>(
        &self,
        guild_id: GuildId,
        body: &B,
        file: FileAttachment,
    ) -> Result<Sticker, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        let path = format!("/guilds/{}/stickers", guild_id);
        decode_response(
            self.request_with_headers(
                Method::POST,
                &path,
                Some(RequestBody::StickerMultipart {
                    payload_json: serialize_body(body)?,
                    file,
                }),
            )
            .await,
        )
    }

    pub async fn create_guild_sticker_from_request(
        &self,
        guild_id: GuildId,
        body: &CreateGuildSticker,
        file: FileAttachment,
    ) -> Result<Sticker, DiscordError> {
        self.create_guild_sticker_typed(guild_id, body, file).await
    }

    pub async fn modify_guild_sticker(
        &self,
        guild_id: GuildId,
        sticker_id: StickerId,
        body: &Value,
    ) -> Result<Sticker, DiscordError> {
        self.request_typed(
            Method::PATCH,
            &format!("/guilds/{}/stickers/{}", guild_id, sticker_id),
            Some(body),
        )
        .await
    }

    pub async fn modify_guild_sticker_typed<B>(
        &self,
        guild_id: GuildId,
        sticker_id: StickerId,
        body: &B,
    ) -> Result<Sticker, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(
            Method::PATCH,
            &format!("/guilds/{}/stickers/{}", guild_id, sticker_id),
            Some(body),
        )
        .await
    }

    pub async fn modify_guild_sticker_from_request(
        &self,
        guild_id: GuildId,
        sticker_id: StickerId,
        body: &ModifyGuildSticker,
    ) -> Result<Sticker, DiscordError> {
        self.modify_guild_sticker_typed(guild_id, sticker_id, body)
            .await
    }

    pub async fn delete_guild_sticker(
        &self,
        guild_id: GuildId,
        sticker_id: StickerId,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!("/guilds/{}/stickers/{}", guild_id, sticker_id),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn send_soundboard_sound(
        &self,
        channel_id: ChannelId,
        body: &Value,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::POST,
            &format!("/channels/{}/send-soundboard-sound", channel_id),
            Some(body),
        )
        .await
    }

    pub async fn list_default_soundboard_sounds(
        &self,
    ) -> Result<Vec<SoundboardSound>, DiscordError> {
        self.request_typed(
            Method::GET,
            "/soundboard-default-sounds",
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn list_guild_soundboard_sounds(
        &self,
        guild_id: GuildId,
    ) -> Result<SoundboardSoundList, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/soundboard-sounds", guild_id),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_guild_soundboard_sound(
        &self,
        guild_id: GuildId,
        sound_id: SoundboardSoundId,
    ) -> Result<SoundboardSound, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/soundboard-sounds/{}", guild_id, sound_id),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn create_guild_soundboard_sound(
        &self,
        guild_id: GuildId,
        body: &Value,
    ) -> Result<SoundboardSound, DiscordError> {
        self.request_typed(
            Method::POST,
            &format!("/guilds/{}/soundboard-sounds", guild_id),
            Some(body),
        )
        .await
    }

    pub async fn modify_guild_soundboard_sound(
        &self,
        guild_id: GuildId,
        sound_id: SoundboardSoundId,
        body: &Value,
    ) -> Result<SoundboardSound, DiscordError> {
        self.request_typed(
            Method::PATCH,
            &format!("/guilds/{}/soundboard-sounds/{}", guild_id, sound_id),
            Some(body),
        )
        .await
    }

    pub async fn delete_guild_soundboard_sound(
        &self,
        guild_id: GuildId,
        sound_id: SoundboardSoundId,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!("/guilds/{}/soundboard-sounds/{}", guild_id, sound_id),
            Option::<&Value>::None,
        )
        .await
    }
}
