use reqwest::Method;
use serde_json::Value;

use crate::error::DiscordError;
use crate::model::Snowflake;
use crate::types::Emoji;

use super::{configured_application_id, RestClient};

#[derive(Debug, serde::Deserialize)]
struct EmojiListResponse {
    #[serde(default)]
    items: Vec<Emoji>,
}

impl RestClient {
    pub async fn get_guild_emojis(
        &self,
        guild_id: impl Into<Snowflake>,
    ) -> Result<Vec<serde_json::Value>, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/emojis", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_guild_emojis_typed(
        &self,
        guild_id: impl Into<Snowflake>,
    ) -> Result<Vec<Emoji>, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/emojis", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_guild_emoji(
        &self,
        guild_id: impl Into<Snowflake>,
        emoji_id: impl Into<Snowflake>,
    ) -> Result<serde_json::Value, DiscordError> {
        self.request(
            Method::GET,
            &format!("/guilds/{}/emojis/{}", guild_id.into(), emoji_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_guild_emoji_typed(
        &self,
        guild_id: impl Into<Snowflake>,
        emoji_id: impl Into<Snowflake>,
    ) -> Result<Emoji, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/emojis/{}", guild_id.into(), emoji_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn create_guild_emoji(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<serde_json::Value, DiscordError> {
        self.request(
            Method::POST,
            &format!("/guilds/{}/emojis", guild_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn create_guild_emoji_typed<B>(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<Emoji, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(
            Method::POST,
            &format!("/guilds/{}/emojis", guild_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn modify_guild_emoji(
        &self,
        guild_id: impl Into<Snowflake>,
        emoji_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<serde_json::Value, DiscordError> {
        self.request(
            Method::PATCH,
            &format!("/guilds/{}/emojis/{}", guild_id.into(), emoji_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn modify_guild_emoji_typed<B>(
        &self,
        guild_id: impl Into<Snowflake>,
        emoji_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<Emoji, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(
            Method::PATCH,
            &format!("/guilds/{}/emojis/{}", guild_id.into(), emoji_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn delete_guild_emoji(
        &self,
        guild_id: impl Into<Snowflake>,
        emoji_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!("/guilds/{}/emojis/{}", guild_id.into(), emoji_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_application_emojis(&self) -> Result<Vec<serde_json::Value>, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        let response = self
            .request(
                Method::GET,
                &format!("/applications/{application_id}/emojis"),
                Option::<&Value>::None,
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
            .request_typed(
                Method::GET,
                &format!("/applications/{application_id}/emojis"),
                Option::<&Value>::None,
            )
            .await?;
        Ok(response.items)
    }

    pub async fn get_application_emoji(
        &self,
        emoji_id: impl Into<Snowflake>,
    ) -> Result<serde_json::Value, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request(
            Method::GET,
            &format!("/applications/{application_id}/emojis/{}", emoji_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_application_emoji_typed(
        &self,
        emoji_id: impl Into<Snowflake>,
    ) -> Result<Emoji, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request_typed(
            Method::GET,
            &format!("/applications/{application_id}/emojis/{}", emoji_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn create_application_emoji(
        &self,
        body: &Value,
    ) -> Result<serde_json::Value, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request(
            Method::POST,
            &format!("/applications/{application_id}/emojis"),
            Some(body),
        )
        .await
    }

    pub async fn create_application_emoji_typed<B>(&self, body: &B) -> Result<Emoji, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        let application_id = configured_application_id(self.application_id())?;
        self.request_typed(
            Method::POST,
            &format!("/applications/{application_id}/emojis"),
            Some(body),
        )
        .await
    }

    pub async fn modify_application_emoji(
        &self,
        emoji_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<serde_json::Value, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request(
            Method::PATCH,
            &format!("/applications/{application_id}/emojis/{}", emoji_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn modify_application_emoji_typed<B>(
        &self,
        emoji_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<Emoji, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        let application_id = configured_application_id(self.application_id())?;
        self.request_typed(
            Method::PATCH,
            &format!("/applications/{application_id}/emojis/{}", emoji_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn delete_application_emoji(
        &self,
        emoji_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request_no_content(
            Method::DELETE,
            &format!("/applications/{application_id}/emojis/{}", emoji_id.into()),
            Option::<&Value>::None,
        )
        .await
    }
}
