use serde_json::Value;

use crate::error::DiscordError;
use crate::model::{AutoModerationRule, AutoModerationRuleId, GuildId};

use super::{json_body, RestClient, RouteId, NO_BODY};

impl RestClient {
    pub async fn get_auto_moderation_rules(
        &self,
        guild_id: GuildId,
    ) -> Result<Vec<serde_json::Value>, DiscordError> {
        self.send(
            RouteId::ListAutoModerationRulesForGuild,
            &format!("/guilds/{}/auto-moderation/rules", guild_id),
            NO_BODY,
        )
        .await
    }

    pub async fn get_auto_moderation_rules_typed(
        &self,
        guild_id: GuildId,
    ) -> Result<Vec<AutoModerationRule>, DiscordError> {
        self.send(
            RouteId::ListAutoModerationRulesForGuild,
            &format!("/guilds/{}/auto-moderation/rules", guild_id),
            NO_BODY,
        )
        .await
    }

    pub async fn get_auto_moderation_rule(
        &self,
        guild_id: GuildId,
        rule_id: AutoModerationRuleId,
    ) -> Result<AutoModerationRule, DiscordError> {
        self.send(
            RouteId::GetAutoModerationRule,
            &format!("/guilds/{}/auto-moderation/rules/{}", guild_id, rule_id),
            NO_BODY,
        )
        .await
    }

    pub async fn create_auto_moderation_rule(
        &self,
        guild_id: GuildId,
        body: &Value,
    ) -> Result<serde_json::Value, DiscordError> {
        self.send(
            RouteId::CreateAutoModerationRule,
            &format!("/guilds/{}/auto-moderation/rules", guild_id),
            json_body(body),
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
        self.send(
            RouteId::CreateAutoModerationRule,
            &format!("/guilds/{}/auto-moderation/rules", guild_id),
            json_body(body),
        )
        .await
    }

    pub async fn modify_auto_moderation_rule(
        &self,
        guild_id: GuildId,
        rule_id: AutoModerationRuleId,
        body: &Value,
    ) -> Result<serde_json::Value, DiscordError> {
        self.send(
            RouteId::ModifyAutoModerationRule,
            &format!("/guilds/{}/auto-moderation/rules/{}", guild_id, rule_id),
            json_body(body),
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
        self.send(
            RouteId::ModifyAutoModerationRule,
            &format!("/guilds/{}/auto-moderation/rules/{}", guild_id, rule_id),
            json_body(body),
        )
        .await
    }

    pub async fn delete_auto_moderation_rule(
        &self,
        guild_id: GuildId,
        rule_id: AutoModerationRuleId,
    ) -> Result<(), DiscordError> {
        self.send_unit(
            RouteId::DeleteAutoModerationRule,
            &format!("/guilds/{}/auto-moderation/rules/{}", guild_id, rule_id),
            NO_BODY,
        )
        .await
    }
}
