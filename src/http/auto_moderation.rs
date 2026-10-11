use reqwest::Method;
use serde_json::Value;

use crate::error::DiscordError;
use crate::model::{AutoModerationRule, AutoModerationRuleId, GuildId};

use super::RestClient;

impl RestClient {
    pub async fn get_auto_moderation_rules(
        &self,
        guild_id: GuildId,
    ) -> Result<Vec<serde_json::Value>, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/auto-moderation/rules", guild_id),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_auto_moderation_rules_typed(
        &self,
        guild_id: GuildId,
    ) -> Result<Vec<AutoModerationRule>, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/auto-moderation/rules", guild_id),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_auto_moderation_rule(
        &self,
        guild_id: GuildId,
        rule_id: AutoModerationRuleId,
    ) -> Result<AutoModerationRule, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/auto-moderation/rules/{}", guild_id, rule_id),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn create_auto_moderation_rule(
        &self,
        guild_id: GuildId,
        body: &Value,
    ) -> Result<serde_json::Value, DiscordError> {
        self.request(
            Method::POST,
            &format!("/guilds/{}/auto-moderation/rules", guild_id),
            Some(body),
        )
        .await
    }

    pub async fn create_auto_moderation_rule_typed<B>(
        &self,
        guild_id: GuildId,
        body: &B,
    ) -> Result<AutoModerationRule, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(
            Method::POST,
            &format!("/guilds/{}/auto-moderation/rules", guild_id),
            Some(body),
        )
        .await
    }

    pub async fn modify_auto_moderation_rule(
        &self,
        guild_id: GuildId,
        rule_id: AutoModerationRuleId,
        body: &Value,
    ) -> Result<serde_json::Value, DiscordError> {
        self.request(
            Method::PATCH,
            &format!("/guilds/{}/auto-moderation/rules/{}", guild_id, rule_id),
            Some(body),
        )
        .await
    }

    pub async fn modify_auto_moderation_rule_typed<B>(
        &self,
        guild_id: GuildId,
        rule_id: AutoModerationRuleId,
        body: &B,
    ) -> Result<AutoModerationRule, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(
            Method::PATCH,
            &format!("/guilds/{}/auto-moderation/rules/{}", guild_id, rule_id),
            Some(body),
        )
        .await
    }

    pub async fn delete_auto_moderation_rule(
        &self,
        guild_id: GuildId,
        rule_id: AutoModerationRuleId,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!("/guilds/{}/auto-moderation/rules/{}", guild_id, rule_id),
            Option::<&Value>::None,
        )
        .await
    }
}
