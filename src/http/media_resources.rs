use reqwest::Method;
use serde_json::Value;

use crate::error::DiscordError;
use crate::model::{
    CreateGuildSticker, ModifyGuildSticker, Snowflake, SoundboardSound, SoundboardSoundList,
    Sticker, StickerPack, StickerPackList,
};

use super::{parse_body_value, serialize_body, FileAttachment, RequestBody, RestClient};

impl RestClient {
    pub async fn get_sticker(
        &self,
        sticker_id: impl Into<Snowflake>,
    ) -> Result<Sticker, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/stickers/{}", sticker_id.into()),
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
        pack_id: impl Into<Snowflake>,
    ) -> Result<StickerPack, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/sticker-packs/{}", pack_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_guild_stickers(
        &self,
        guild_id: impl Into<Snowflake>,
    ) -> Result<Vec<Sticker>, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/stickers", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_guild_sticker(
        &self,
        guild_id: impl Into<Snowflake>,
        sticker_id: impl Into<Snowflake>,
    ) -> Result<Sticker, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/stickers/{}", guild_id.into(), sticker_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn create_guild_sticker(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &Value,
        file: FileAttachment,
    ) -> Result<Sticker, DiscordError> {
        let path = format!("/guilds/{}/stickers", guild_id.into());
        let response = self
            .request_with_headers(
                Method::POST,
                &path,
                Some(RequestBody::StickerMultipart {
                    payload_json: body.clone(),
                    file,
                }),
            )
            .await?;
        serde_json::from_value(parse_body_value(response.body)).map_err(Into::into)
    }

    pub async fn create_guild_sticker_typed<B>(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &B,
        file: FileAttachment,
    ) -> Result<Sticker, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        let path = format!("/guilds/{}/stickers", guild_id.into());
        let response = self
            .request_with_headers(
                Method::POST,
                &path,
                Some(RequestBody::StickerMultipart {
                    payload_json: serialize_body(body)?,
                    file,
                }),
            )
            .await?;
        serde_json::from_value(parse_body_value(response.body)).map_err(Into::into)
    }

    pub async fn create_guild_sticker_from_request(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &CreateGuildSticker,
        file: FileAttachment,
    ) -> Result<Sticker, DiscordError> {
        self.create_guild_sticker_typed(guild_id, body, file).await
    }

    pub async fn modify_guild_sticker(
        &self,
        guild_id: impl Into<Snowflake>,
        sticker_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<Sticker, DiscordError> {
        self.request_typed(
            Method::PATCH,
            &format!("/guilds/{}/stickers/{}", guild_id.into(), sticker_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn modify_guild_sticker_typed<B>(
        &self,
        guild_id: impl Into<Snowflake>,
        sticker_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<Sticker, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(
            Method::PATCH,
            &format!("/guilds/{}/stickers/{}", guild_id.into(), sticker_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn modify_guild_sticker_from_request(
        &self,
        guild_id: impl Into<Snowflake>,
        sticker_id: impl Into<Snowflake>,
        body: &ModifyGuildSticker,
    ) -> Result<Sticker, DiscordError> {
        self.modify_guild_sticker_typed(guild_id, sticker_id, body)
            .await
    }

    pub async fn delete_guild_sticker(
        &self,
        guild_id: impl Into<Snowflake>,
        sticker_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!("/guilds/{}/stickers/{}", guild_id.into(), sticker_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn send_soundboard_sound(
        &self,
        channel_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::POST,
            &format!("/channels/{}/send-soundboard-sound", channel_id.into()),
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
        guild_id: impl Into<Snowflake>,
    ) -> Result<SoundboardSoundList, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/soundboard-sounds", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_guild_soundboard_sound(
        &self,
        guild_id: impl Into<Snowflake>,
        sound_id: impl Into<Snowflake>,
    ) -> Result<SoundboardSound, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!(
                "/guilds/{}/soundboard-sounds/{}",
                guild_id.into(),
                sound_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn create_guild_soundboard_sound(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<SoundboardSound, DiscordError> {
        self.request_typed(
            Method::POST,
            &format!("/guilds/{}/soundboard-sounds", guild_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn modify_guild_soundboard_sound(
        &self,
        guild_id: impl Into<Snowflake>,
        sound_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<SoundboardSound, DiscordError> {
        self.request_typed(
            Method::PATCH,
            &format!(
                "/guilds/{}/soundboard-sounds/{}",
                guild_id.into(),
                sound_id.into()
            ),
            Some(body),
        )
        .await
    }

    pub async fn delete_guild_soundboard_sound(
        &self,
        guild_id: impl Into<Snowflake>,
        sound_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!(
                "/guilds/{}/soundboard-sounds/{}",
                guild_id.into(),
                sound_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }
}
