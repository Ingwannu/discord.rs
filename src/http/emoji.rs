use serde_json::Value;

use crate::error::DiscordError;
use crate::model::{EmojiId, GuildId};
use crate::types::Emoji;

use super::{configured_application_id, json_body, RestClient, RouteId, NO_BODY};

#[derive(Debug, serde::Deserialize)]
struct EmojiListResponse {
    #[serde(default)]
    items: Vec<Emoji>,
}

impl RestClient {
    pub async fn get_guild_emojis(
        &self,
        guild_id: GuildId,
    ) -> Result<Vec<serde_json::Value>, DiscordError> {
        self.send(
            RouteId::ListGuildEmojis,
            &format!("/guilds/{}/emojis", guild_id),
            NO_BODY,
        )
        .await
    }

    pub async fn get_guild_emojis_typed(
        &self,
        guild_id: GuildId,
    ) -> Result<Vec<Emoji>, DiscordError> {
        self.send(
            RouteId::ListGuildEmojis,
            &format!("/guilds/{}/emojis", guild_id),
            NO_BODY,
        )
        .await
    }

    pub async fn get_guild_emoji(
        &self,
        guild_id: GuildId,
        emoji_id: EmojiId,
    ) -> Result<serde_json::Value, DiscordError> {
        self.send(
            RouteId::GetGuildEmoji,
            &format!("/guilds/{}/emojis/{}", guild_id, emoji_id),
            NO_BODY,
        )
        .await
    }

    pub async fn get_guild_emoji_typed(
        &self,
        guild_id: GuildId,
        emoji_id: EmojiId,
    ) -> Result<Emoji, DiscordError> {
        self.send(
            RouteId::GetGuildEmoji,
            &format!("/guilds/{}/emojis/{}", guild_id, emoji_id),
            NO_BODY,
        )
        .await
    }

    pub async fn create_guild_emoji(
        &self,
        guild_id: GuildId,
        body: &Value,
    ) -> Result<serde_json::Value, DiscordError> {
        self.send(
            RouteId::CreateGuildEmoji,
            &format!("/guilds/{}/emojis", guild_id),
            json_body(body),
        )
        .await
    }

    pub async fn create_guild_emoji_typed<B>(
        &self,
        guild_id: GuildId,
        body: &B,
    ) -> Result<Emoji, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.send(
            RouteId::CreateGuildEmoji,
            &format!("/guilds/{}/emojis", guild_id),
            json_body(body),
        )
        .await
    }

    pub async fn modify_guild_emoji(
        &self,
        guild_id: GuildId,
        emoji_id: EmojiId,
        body: &Value,
    ) -> Result<serde_json::Value, DiscordError> {
        self.send(
            RouteId::ModifyGuildEmoji,
            &format!("/guilds/{}/emojis/{}", guild_id, emoji_id),
            json_body(body),
        )
        .await
    }

    pub async fn modify_guild_emoji_typed<B>(
        &self,
        guild_id: GuildId,
        emoji_id: EmojiId,
        body: &B,
    ) -> Result<Emoji, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.send(
            RouteId::ModifyGuildEmoji,
            &format!("/guilds/{}/emojis/{}", guild_id, emoji_id),
            json_body(body),
        )
        .await
    }

    pub async fn delete_guild_emoji(
        &self,
        guild_id: GuildId,
        emoji_id: EmojiId,
    ) -> Result<(), DiscordError> {
        self.send_unit(
            RouteId::DeleteGuildEmoji,
            &format!("/guilds/{}/emojis/{}", guild_id, emoji_id),
            NO_BODY,
        )
        .await
    }

    pub async fn get_application_emojis(&self) -> Result<Vec<serde_json::Value>, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        let response: Value = self
            .send(
                RouteId::ListApplicationEmojis,
                &format!("/applications/{application_id}/emojis"),
                NO_BODY,
            )
            .await?;
        Ok(response
            .get("items")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default())
    }

    pub async fn get_application_emojis_typed(&self) -> Result<Vec<Emoji>, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        let response: EmojiListResponse = self
            .send(
                RouteId::ListApplicationEmojis,
                &format!("/applications/{application_id}/emojis"),
                NO_BODY,
            )
            .await?;
        Ok(response.items)
    }

    pub async fn get_application_emoji(
        &self,
        emoji_id: EmojiId,
    ) -> Result<serde_json::Value, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.send(
            RouteId::GetApplicationEmoji,
            &format!("/applications/{application_id}/emojis/{}", emoji_id),
            NO_BODY,
        )
        .await
    }

    pub async fn get_application_emoji_typed(
        &self,
        emoji_id: EmojiId,
    ) -> Result<Emoji, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.send(
            RouteId::GetApplicationEmoji,
            &format!("/applications/{application_id}/emojis/{}", emoji_id),
            NO_BODY,
        )
        .await
    }

    pub async fn create_application_emoji(
        &self,
        body: &Value,
    ) -> Result<serde_json::Value, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.send(
            RouteId::CreateApplicationEmoji,
            &format!("/applications/{application_id}/emojis"),
            json_body(body),
        )
        .await
    }

    pub async fn create_application_emoji_typed<B>(&self, body: &B) -> Result<Emoji, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        let application_id = configured_application_id(self.application_id())?;
        self.send(
            RouteId::CreateApplicationEmoji,
            &format!("/applications/{application_id}/emojis"),
            json_body(body),
        )
        .await
    }

    pub async fn modify_application_emoji(
        &self,
        emoji_id: EmojiId,
        body: &Value,
    ) -> Result<serde_json::Value, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.send(
            RouteId::ModifyApplicationEmoji,
            &format!("/applications/{application_id}/emojis/{}", emoji_id),
            json_body(body),
        )
        .await
    }

    pub async fn modify_application_emoji_typed<B>(
        &self,
        emoji_id: EmojiId,
        body: &B,
    ) -> Result<Emoji, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        let application_id = configured_application_id(self.application_id())?;
        self.send(
            RouteId::ModifyApplicationEmoji,
            &format!("/applications/{application_id}/emojis/{}", emoji_id),
            json_body(body),
        )
        .await
    }

    pub async fn delete_application_emoji(&self, emoji_id: EmojiId) -> Result<(), DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.send_unit(
            RouteId::DeleteApplicationEmoji,
            &format!("/applications/{application_id}/emojis/{}", emoji_id),
            NO_BODY,
        )
        .await
    }
}
