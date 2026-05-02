use std::sync::Arc;

#[cfg(feature = "gateway")]
use async_trait::async_trait;

use crate::error::DiscordError;
use crate::http::DiscordHttpClient;
#[cfg(feature = "gateway")]
use crate::manager::CachedManager;
use crate::model::{Channel, Guild, Member, Message, Role, Snowflake, User};

use super::CacheHandle;

#[derive(Clone)]
/// Typed Discord API object for `UserManager`.
pub struct UserManager {
    http: Arc<DiscordHttpClient>,
    cache: CacheHandle,
}

impl UserManager {
    #[cfg(feature = "gateway")]
    pub(crate) fn new(http: Arc<DiscordHttpClient>, cache: CacheHandle) -> Self {
        Self { http, cache }
    }

    pub async fn get(&self, user_id: impl Into<Snowflake>) -> Result<User, DiscordError> {
        let user_id = user_id.into();
        if let Some(user) = self.cache.user(&user_id).await {
            return Ok(user);
        }
        self.http.get_user(user_id).await
    }

    pub async fn cached(&self, user_id: impl Into<Snowflake>) -> Option<User> {
        self.cache.user(&user_id.into()).await
    }

    pub async fn contains(&self, user_id: impl Into<Snowflake>) -> bool {
        self.cache.contains_user(&user_id.into()).await
    }

    pub async fn list_cached(&self) -> Vec<User> {
        self.cache.users().await
    }
}

#[derive(Clone)]
/// Typed Discord API object for `GuildManager`.
pub struct GuildManager {
    http: Arc<DiscordHttpClient>,
    cache: CacheHandle,
}

impl GuildManager {
    #[cfg(feature = "gateway")]
    pub(crate) fn new(http: Arc<DiscordHttpClient>, cache: CacheHandle) -> Self {
        Self { http, cache }
    }

    pub async fn get(&self, guild_id: impl Into<Snowflake>) -> Result<Guild, DiscordError> {
        let guild_id = guild_id.into();
        if let Some(guild) = self.cache.guild(&guild_id).await {
            return Ok(guild);
        }
        self.http.get_guild(guild_id).await
    }

    pub async fn cached(&self, guild_id: impl Into<Snowflake>) -> Option<Guild> {
        self.cache.guild(&guild_id.into()).await
    }

    pub async fn contains(&self, guild_id: impl Into<Snowflake>) -> bool {
        self.cache.contains_guild(&guild_id.into()).await
    }

    pub async fn list_cached(&self) -> Vec<Guild> {
        self.cache.guilds().await
    }
}

#[derive(Clone)]
/// Typed Discord API object for `ChannelManager`.
pub struct ChannelManager {
    http: Arc<DiscordHttpClient>,
    cache: CacheHandle,
}

impl ChannelManager {
    #[cfg(feature = "gateway")]
    pub(crate) fn new(http: Arc<DiscordHttpClient>, cache: CacheHandle) -> Self {
        Self { http, cache }
    }

    pub async fn get(&self, channel_id: impl Into<Snowflake>) -> Result<Channel, DiscordError> {
        let channel_id = channel_id.into();
        if let Some(channel) = self.cache.channel(&channel_id).await {
            return Ok(channel);
        }
        self.http.get_channel(channel_id).await
    }

    pub async fn cached(&self, channel_id: impl Into<Snowflake>) -> Option<Channel> {
        self.cache.channel(&channel_id.into()).await
    }

    pub async fn contains(&self, channel_id: impl Into<Snowflake>) -> bool {
        self.cache.contains_channel(&channel_id.into()).await
    }

    pub async fn list_cached(&self) -> Vec<Channel> {
        self.cache.channels().await
    }
}

#[derive(Clone)]
/// Typed Discord API object for `MemberManager`.
pub struct MemberManager {
    http: Arc<DiscordHttpClient>,
    cache: CacheHandle,
}

impl MemberManager {
    #[cfg(feature = "gateway")]
    pub(crate) fn new(http: Arc<DiscordHttpClient>, cache: CacheHandle) -> Self {
        Self { http, cache }
    }

    pub async fn get(
        &self,
        guild_id: impl Into<Snowflake>,
        user_id: impl Into<Snowflake>,
    ) -> Result<Member, DiscordError> {
        let guild_id = guild_id.into();
        let user_id = user_id.into();
        if let Some(member) = self.cache.member(&guild_id, &user_id).await {
            return Ok(member);
        }
        self.http.get_member(guild_id, user_id).await
    }

    pub async fn cached(
        &self,
        guild_id: impl Into<Snowflake>,
        user_id: impl Into<Snowflake>,
    ) -> Option<Member> {
        self.cache.member(&guild_id.into(), &user_id.into()).await
    }

    pub async fn cached_arc(
        &self,
        guild_id: impl Into<Snowflake>,
        user_id: impl Into<Snowflake>,
    ) -> Option<Arc<Member>> {
        self.cache
            .member_arc(&guild_id.into(), &user_id.into())
            .await
    }

    pub async fn contains(
        &self,
        guild_id: impl Into<Snowflake>,
        user_id: impl Into<Snowflake>,
    ) -> bool {
        self.cache
            .contains_member(&guild_id.into(), &user_id.into())
            .await
    }

    pub async fn list_cached(&self, guild_id: impl Into<Snowflake>) -> Vec<Member> {
        self.cache.members(&guild_id.into()).await
    }

    pub async fn list_cached_arc(&self, guild_id: impl Into<Snowflake>) -> Vec<Arc<Member>> {
        self.cache.members_arc(&guild_id.into()).await
    }
}

#[derive(Clone)]
/// Typed Discord API object for `MessageManager`.
pub struct MessageManager {
    http: Arc<DiscordHttpClient>,
    cache: CacheHandle,
}

impl MessageManager {
    #[cfg(feature = "gateway")]
    pub(crate) fn new(http: Arc<DiscordHttpClient>, cache: CacheHandle) -> Self {
        Self { http, cache }
    }

    pub async fn get(
        &self,
        channel_id: impl Into<Snowflake>,
        message_id: impl Into<Snowflake>,
    ) -> Result<Message, DiscordError> {
        let channel_id = channel_id.into();
        let message_id = message_id.into();
        if let Some(message) = self.cache.message(&channel_id, &message_id).await {
            return Ok(message);
        }
        self.http.get_message(channel_id, message_id).await
    }

    pub async fn cached(
        &self,
        channel_id: impl Into<Snowflake>,
        message_id: impl Into<Snowflake>,
    ) -> Option<Message> {
        self.cache
            .message(&channel_id.into(), &message_id.into())
            .await
    }

    pub async fn cached_arc(
        &self,
        channel_id: impl Into<Snowflake>,
        message_id: impl Into<Snowflake>,
    ) -> Option<Arc<Message>> {
        self.cache
            .message_arc(&channel_id.into(), &message_id.into())
            .await
    }

    pub async fn contains(
        &self,
        channel_id: impl Into<Snowflake>,
        message_id: impl Into<Snowflake>,
    ) -> bool {
        self.cache
            .contains_message(&channel_id.into(), &message_id.into())
            .await
    }

    pub async fn list_cached(&self, channel_id: impl Into<Snowflake>) -> Vec<Message> {
        self.cache.messages(&channel_id.into()).await
    }

    pub async fn list_cached_arc(&self, channel_id: impl Into<Snowflake>) -> Vec<Arc<Message>> {
        self.cache.messages_arc(&channel_id.into()).await
    }
}

#[derive(Clone)]
/// Typed Discord API object for `RoleManager`.
pub struct RoleManager {
    http: Arc<DiscordHttpClient>,
    cache: CacheHandle,
}

impl RoleManager {
    #[cfg(feature = "gateway")]
    pub(crate) fn new(http: Arc<DiscordHttpClient>, cache: CacheHandle) -> Self {
        Self { http, cache }
    }

    pub async fn list(&self, guild_id: impl Into<Snowflake>) -> Result<Vec<Role>, DiscordError> {
        let guild_id = guild_id.into();
        let cached = self.cache.roles(&guild_id).await;
        if !cached.is_empty() {
            return Ok(cached);
        }
        self.http.list_roles(guild_id).await
    }

    pub async fn cached(
        &self,
        guild_id: impl Into<Snowflake>,
        role_id: impl Into<Snowflake>,
    ) -> Option<Role> {
        self.cache.role(&guild_id.into(), &role_id.into()).await
    }

    pub async fn contains(
        &self,
        guild_id: impl Into<Snowflake>,
        role_id: impl Into<Snowflake>,
    ) -> bool {
        self.cache
            .contains_role(&guild_id.into(), &role_id.into())
            .await
    }

    pub async fn list_cached(&self, guild_id: impl Into<Snowflake>) -> Vec<Role> {
        self.cache.roles(&guild_id.into()).await
    }
}

#[cfg(feature = "gateway")]
#[async_trait]
impl CachedManager<Guild> for GuildManager {
    async fn get(&self, id: impl Into<Snowflake> + Send) -> Result<Guild, DiscordError> {
        let id = id.into();
        if let Some(guild) = self.cache.guild(&id).await {
            return Ok(guild);
        }
        self.http.get_guild(id).await
    }

    async fn cached(&self, id: impl Into<Snowflake> + Send) -> Option<Guild> {
        self.cache.guild(&id.into()).await
    }

    async fn contains(&self, id: impl Into<Snowflake> + Send) -> bool {
        self.cache.contains_guild(&id.into()).await
    }

    async fn list_cached(&self) -> Vec<Guild> {
        self.cache.guilds().await
    }
}

#[cfg(feature = "gateway")]
#[async_trait]
impl CachedManager<Channel> for ChannelManager {
    async fn get(&self, id: impl Into<Snowflake> + Send) -> Result<Channel, DiscordError> {
        let id = id.into();
        if let Some(channel) = self.cache.channel(&id).await {
            return Ok(channel);
        }
        self.http.get_channel(id).await
    }

    async fn cached(&self, id: impl Into<Snowflake> + Send) -> Option<Channel> {
        self.cache.channel(&id.into()).await
    }

    async fn contains(&self, id: impl Into<Snowflake> + Send) -> bool {
        self.cache.contains_channel(&id.into()).await
    }

    async fn list_cached(&self) -> Vec<Channel> {
        self.cache.channels().await
    }
}

#[cfg(feature = "gateway")]
#[async_trait]
impl CachedManager<User> for UserManager {
    async fn get(&self, id: impl Into<Snowflake> + Send) -> Result<User, DiscordError> {
        let id = id.into();
        if let Some(user) = self.cache.user(&id).await {
            return Ok(user);
        }
        self.http.get_user(id).await
    }

    async fn cached(&self, id: impl Into<Snowflake> + Send) -> Option<User> {
        self.cache.user(&id.into()).await
    }

    async fn contains(&self, id: impl Into<Snowflake> + Send) -> bool {
        self.cache.contains_user(&id.into()).await
    }

    async fn list_cached(&self) -> Vec<User> {
        self.cache.users().await
    }
}
