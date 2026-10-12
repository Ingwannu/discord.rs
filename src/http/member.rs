//! Guild member endpoints: list, search, add, modify, remove, and member roles.

use serde_json::Value;

use crate::error::DiscordError;
use crate::model::{
    AddGuildMember, GuildId, GuildMembersQuery, Member, RoleId, SearchGuildMembersQuery, UserId,
};

use super::paths::{guild_members_query, search_guild_members_query};
use super::{json_body, RestClient, RouteId, NO_BODY};

impl RestClient {
    pub async fn get_guild_members(
        &self,
        guild_id: GuildId,
        limit: Option<u64>,
    ) -> Result<Vec<Member>, DiscordError> {
        self.get_guild_members_with_query(
            guild_id,
            &GuildMembersQuery {
                limit,
                ..GuildMembersQuery::default()
            },
        )
        .await
    }

    pub async fn get_guild_members_with_query(
        &self,
        guild_id: GuildId,
        query: &GuildMembersQuery,
    ) -> Result<Vec<Member>, DiscordError> {
        let query = guild_members_query(query);
        self.send(
            RouteId::ListGuildMembers,
            &format!("/guilds/{}/members{query}", guild_id),
            NO_BODY,
        )
        .await
    }

    /// Add an OAuth2-authorized user to a guild.
    ///
    /// Returns `Some(member)` when Discord creates the guild membership and
    /// `None` when Discord reports the user is already a member.
    pub async fn add_guild_member(
        &self,
        guild_id: GuildId,
        user_id: UserId,
        body: &AddGuildMember,
    ) -> Result<Option<Member>, DiscordError> {
        self.send_optional(
            RouteId::AddGuildMember,
            &format!("/guilds/{}/members/{}", guild_id, user_id),
            json_body(body),
        )
        .await
    }

    pub async fn remove_guild_member(
        &self,
        guild_id: GuildId,
        user_id: UserId,
    ) -> Result<(), DiscordError> {
        self.send_unit(
            RouteId::RemoveGuildMember,
            &format!("/guilds/{}/members/{}", guild_id, user_id),
            NO_BODY,
        )
        .await
    }

    pub async fn add_guild_member_role(
        &self,
        guild_id: GuildId,
        user_id: UserId,
        role_id: RoleId,
    ) -> Result<(), DiscordError> {
        self.send_unit(
            RouteId::AddGuildMemberRole,
            &format!("/guilds/{}/members/{}/roles/{}", guild_id, user_id, role_id),
            NO_BODY,
        )
        .await
    }

    pub async fn remove_guild_member_role(
        &self,
        guild_id: GuildId,
        user_id: UserId,
        role_id: RoleId,
    ) -> Result<(), DiscordError> {
        self.send_unit(
            RouteId::RemoveGuildMemberRole,
            &format!("/guilds/{}/members/{}/roles/{}", guild_id, user_id, role_id),
            NO_BODY,
        )
        .await
    }

    pub async fn get_member(
        &self,
        guild_id: GuildId,
        user_id: UserId,
    ) -> Result<Member, DiscordError> {
        self.send(
            RouteId::GetGuildMember,
            &format!("/guilds/{}/members/{}", guild_id, user_id),
            NO_BODY,
        )
        .await
    }

    pub async fn modify_guild_member(
        &self,
        guild_id: GuildId,
        user_id: UserId,
        body: &Value,
    ) -> Result<(), DiscordError> {
        self.modify_guild_member_typed(guild_id, user_id, body)
            .await
    }

    pub async fn modify_guild_member_typed<B>(
        &self,
        guild_id: GuildId,
        user_id: UserId,
        body: &B,
    ) -> Result<(), DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.send_unit(
            RouteId::ModifyGuildMember,
            &format!("/guilds/{}/members/{}", guild_id, user_id),
            json_body(body),
        )
        .await
    }

    pub async fn modify_current_member_nick(
        &self,
        guild_id: GuildId,
        nick: Option<&str>,
    ) -> Result<(), DiscordError> {
        let body = serde_json::json!({ "nick": nick });
        self.send_unit(
            RouteId::ModifyCurrentUserNick,
            &format!("/guilds/{}/members/@me/nick", guild_id),
            json_body(&body),
        )
        .await
    }

    pub async fn modify_current_member<B>(
        &self,
        guild_id: GuildId,
        body: &B,
    ) -> Result<Member, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        self.send(
            RouteId::ModifyCurrentMember,
            &format!("/guilds/{}/members/@me", guild_id),
            json_body(body),
        )
        .await
    }

    pub async fn search_guild_members(
        &self,
        guild_id: GuildId,
        query: &str,
        limit: Option<u64>,
    ) -> Result<Vec<Member>, DiscordError> {
        self.search_guild_members_with_query(
            guild_id,
            &SearchGuildMembersQuery {
                query: query.to_string(),
                limit,
            },
        )
        .await
    }

    pub async fn search_guild_members_with_query(
        &self,
        guild_id: GuildId,
        query: &SearchGuildMembersQuery,
    ) -> Result<Vec<Member>, DiscordError> {
        let query = search_guild_members_query(query);
        let path = format!("/guilds/{}/members/search{}", guild_id, query);
        self.send(RouteId::SearchGuildMembers, &path, NO_BODY).await
    }
}
