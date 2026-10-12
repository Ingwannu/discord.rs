//! Guild role endpoints: create, modify, reorder, delete, and member counts.

use std::collections::HashMap;

use serde_json::Value;

use crate::error::DiscordError;
use crate::model::{
    CreateGuildRole, GuildId, ModifyGuildRole, ModifyGuildRolePosition, Role, RoleId,
};

use super::{json_body, RestClient, RouteId, NO_BODY};

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
        self.send(
            RouteId::CreateGuildRole,
            &format!("/guilds/{}/roles", guild_id),
            json_body(body),
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
        self.send(
            RouteId::ModifyGuildRole,
            &format!("/guilds/{}/roles/{}", guild_id, role_id),
            json_body(body),
        )
        .await
    }

    pub async fn delete_role(
        &self,
        guild_id: GuildId,
        role_id: RoleId,
    ) -> Result<(), DiscordError> {
        self.send_unit(
            RouteId::DeleteGuildRole,
            &format!("/guilds/{}/roles/{}", guild_id, role_id),
            NO_BODY,
        )
        .await
    }

    pub async fn list_roles(&self, guild_id: GuildId) -> Result<Vec<Role>, DiscordError> {
        self.send(
            RouteId::GetGuildRoles,
            &format!("/guilds/{}/roles", guild_id),
            NO_BODY,
        )
        .await
    }

    pub async fn modify_guild_role_positions(
        &self,
        guild_id: GuildId,
        body: &Value,
    ) -> Result<Vec<Role>, DiscordError> {
        self.send(
            RouteId::ModifyGuildRolePositions,
            &format!("/guilds/{}/roles", guild_id),
            json_body(body),
        )
        .await
    }

    pub async fn modify_guild_role_positions_typed(
        &self,
        guild_id: GuildId,
        positions: &[ModifyGuildRolePosition],
    ) -> Result<Vec<Role>, DiscordError> {
        self.send(
            RouteId::ModifyGuildRolePositions,
            &format!("/guilds/{}/roles", guild_id),
            json_body(positions),
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
        self.send(
            RouteId::GetGuildRole,
            &format!("/guilds/{}/roles/{}", guild_id, role_id),
            NO_BODY,
        )
        .await
    }

    /// Return the number of guild members assigned to each non-`@everyone` role.
    pub async fn get_guild_role_member_counts(
        &self,
        guild_id: GuildId,
    ) -> Result<HashMap<RoleId, u64>, DiscordError> {
        self.send(
            RouteId::GetGuildRoleMemberCounts,
            &format!("/guilds/{}/roles/member-counts", guild_id),
            NO_BODY,
        )
        .await
    }
}
