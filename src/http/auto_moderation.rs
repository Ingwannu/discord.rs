use reqwest::Method;
use serde_json::Value;

use crate::error::DiscordError;
use crate::model::{AutoModerationRule, Snowflake};

use super::RestClient;

impl RestClient {
    pub async fn get_auto_moderation_rules(
        &self,
        guild_id: impl Into<Snowflake>,
    ) -> Result<Vec<serde_json::Value>, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/auto-moderation/rules", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_auto_moderation_rules_typed(
        &self,
        guild_id: impl Into<Snowflake>,
    ) -> Result<Vec<AutoModerationRule>, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/auto-moderation/rules", guild_id.into()),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_auto_moderation_rule(
        &self,
        guild_id: impl Into<Snowflake>,
        rule_id: impl Into<Snowflake>,
    ) -> Result<AutoModerationRule, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!(
                "/guilds/{}/auto-moderation/rules/{}",
                guild_id.into(),
                rule_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn create_auto_moderation_rule(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<serde_json::Value, DiscordError> {
        self.request(
            Method::POST,
            &format!("/guilds/{}/auto-moderation/rules", guild_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn create_auto_moderation_rule_typed<B>(
        &self,
        guild_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<AutoModerationRule, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(
            Method::POST,
            &format!("/guilds/{}/auto-moderation/rules", guild_id.into()),
            Some(body),
        )
        .await
    }

    pub async fn modify_auto_moderation_rule(
        &self,
        guild_id: impl Into<Snowflake>,
        rule_id: impl Into<Snowflake>,
        body: &Value,
    ) -> Result<serde_json::Value, DiscordError> {
        self.request(
            Method::PATCH,
            &format!(
                "/guilds/{}/auto-moderation/rules/{}",
                guild_id.into(),
                rule_id.into()
            ),
            Some(body),
        )
        .await
    }

    pub async fn modify_auto_moderation_rule_typed<B>(
        &self,
        guild_id: impl Into<Snowflake>,
        rule_id: impl Into<Snowflake>,
        body: &B,
    ) -> Result<AutoModerationRule, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(
            Method::PATCH,
            &format!(
                "/guilds/{}/auto-moderation/rules/{}",
                guild_id.into(),
                rule_id.into()
            ),
            Some(body),
        )
        .await
    }

    pub async fn delete_auto_moderation_rule(
        &self,
        guild_id: impl Into<Snowflake>,
        rule_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!(
                "/guilds/{}/auto-moderation/rules/{}",
                guild_id.into(),
                rule_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }
}
