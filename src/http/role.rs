//! Guild role endpoints: create, modify, reorder, delete, and member counts.

use std::collections::HashMap;

use reqwest::Method;
use serde_json::Value;

use crate::error::DiscordError;
use crate::model::{
    CreateGuildRole, GuildId, ModifyGuildRole, ModifyGuildRolePosition, Role, RoleId,
};

use super::RestClient;

impl RestClient {
    pub async fn create_role(&self, guild_id: GuildId, body: &Value) -> Result<Role, DiscordError> {
        self.create_role_typed(guild_id, body).await
    }

    pub async fn create_role_typed<B>(
        &self,
        guild_id: GuildId,
        body: &B,
    ) -> Result<Role, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(
            Method::POST,
            &format!("/guilds/{}/roles", guild_id),
            Some(body),
        )
        .await
    }

    pub async fn update_role(
        &self,
        guild_id: GuildId,
        role_id: RoleId,
        body: &Value,
    ) -> Result<Role, DiscordError> {
        self.update_role_typed(guild_id, role_id, body).await
    }

    pub async fn update_role_typed<B>(
        &self,
        guild_id: GuildId,
        role_id: RoleId,
        body: &B,
    ) -> Result<Role, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.request_typed(
            Method::PATCH,
            &format!("/guilds/{}/roles/{}", guild_id, role_id),
            Some(body),
        )
        .await
    }

    pub async fn delete_role(
        &self,
        guild_id: GuildId,
        role_id: RoleId,
    ) -> Result<(), DiscordError> {
        self.request_no_content(
            Method::DELETE,
            &format!("/guilds/{}/roles/{}", guild_id, role_id),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn list_roles(&self, guild_id: GuildId) -> Result<Vec<Role>, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/roles", guild_id),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn modify_guild_role_positions(
        &self,
        guild_id: GuildId,
        body: &Value,
    ) -> Result<Vec<Role>, DiscordError> {
        self.request_typed(
            Method::PATCH,
            &format!("/guilds/{}/roles", guild_id),
            Some(body),
        )
        .await
    }

    pub async fn modify_guild_role_positions_typed(
        &self,
        guild_id: GuildId,
        positions: &[ModifyGuildRolePosition],
    ) -> Result<Vec<Role>, DiscordError> {
        self.request_typed(
            Method::PATCH,
            &format!("/guilds/{}/roles", guild_id),
            Some(positions),
        )
        .await
    }

    pub async fn create_guild_role(
        &self,
        guild_id: GuildId,
        body: &CreateGuildRole,
    ) -> Result<Role, DiscordError> {
        self.create_role_typed(guild_id, body).await
    }

    pub async fn modify_guild_role(
        &self,
        guild_id: GuildId,
        role_id: RoleId,
        body: &ModifyGuildRole,
    ) -> Result<Role, DiscordError> {
        self.update_role_typed(guild_id, role_id, body).await
    }

    pub async fn get_guild_role(
        &self,
        guild_id: GuildId,
        role_id: RoleId,
    ) -> Result<Role, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/roles/{}", guild_id, role_id),
            Option::<&Value>::None,
        )
        .await
    }

    /// Return the number of guild members assigned to each non-`@everyone` role.
    pub async fn get_guild_role_member_counts(
        &self,
        guild_id: GuildId,
    ) -> Result<HashMap<RoleId, u64>, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!("/guilds/{}/roles/member-counts", guild_id),
            Option::<&Value>::None,
        )
        .await
    }
}
