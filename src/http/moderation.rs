//! Guild ban endpoints. Auto-moderation rules live in `auto_moderation.rs`.

use crate::error::DiscordError;
use crate::model::{
    Ban, BulkGuildBanRequest, BulkGuildBanResponse, CreateGuildBan, GuildBansQuery, GuildId, UserId,
};

use super::paths::guild_bans_query;
use super::{json_body, RestClient, RouteId, NO_BODY};

impl RestClient {
    pub async fn get_guild_bans(
        &self,
        guild_id: GuildId,
        limit: Option<u64>,
        before: Option<UserId>,
    ) -> Result<Vec<Ban>, DiscordError> {
        self.get_guild_bans_with_query(
            guild_id,
            &GuildBansQuery {
                limit,
                before,
                ..GuildBansQuery::default()
            },
        )
        .await
    }

    pub async fn get_guild_bans_with_query(
        &self,
        guild_id: GuildId,
        query: &GuildBansQuery,
    ) -> Result<Vec<Ban>, DiscordError> {
        let query = guild_bans_query(query);
        self.send(
            RouteId::GetGuildBans,
            &format!("/guilds/{}/bans{}", guild_id, query),
            NO_BODY,
        )
        .await
    }

    pub async fn get_guild_ban(
        &self,
        guild_id: GuildId,
        user_id: UserId,
    ) -> Result<Ban, DiscordError> {
        self.send(
            RouteId::GetGuildBan,
            &format!("/guilds/{}/bans/{}", guild_id, user_id),
            NO_BODY,
        )
        .await
    }

    /// Bans a user, optionally deleting their recent messages.
    pub async fn create_guild_ban(
        &self,
        guild_id: GuildId,
        user_id: UserId,
        body: &CreateGuildBan,
    ) -> Result<(), DiscordError> {
        self.send_unit(
            RouteId::CreateGuildBan,
            &format!("/guilds/{}/bans/{}", guild_id, user_id),
            json_body(body),
        )
        .await
    }

    pub async fn bulk_guild_ban(
        &self,
        guild_id: GuildId,
        body: &BulkGuildBanRequest,
    ) -> Result<BulkGuildBanResponse, DiscordError> {
        self.send(
            RouteId::BulkGuildBan,
            &format!("/guilds/{}/bulk-ban", guild_id),
            json_body(body),
        )
        .await
    }

    pub async fn remove_guild_ban(
        &self,
        guild_id: GuildId,
        user_id: UserId,
    ) -> Result<(), DiscordError> {
        self.send_unit(
            RouteId::RemoveGuildBan,
            &format!("/guilds/{}/bans/{}", guild_id, user_id),
            NO_BODY,
        )
        .await
    }
}
