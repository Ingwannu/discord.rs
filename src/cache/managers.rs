use std::sync::Arc;

use crate::error::DiscordError;
use crate::http::DiscordHttpClient;
#[cfg(feature = "gateway")]
use crate::manager::CachedManager;
use crate::model::{Channel, GenericId, Guild, Member, Message, Role, User};

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

    pub async fn get(&self, user_id: GenericId) -> Result<User, DiscordError> {
        if let Some(user) = self.cache.user(user_id).await {
            return Ok(user);
        }
        self.http.get_user(user_id).await
    }

    pub async fn cached(&self, user_id: GenericId) -> Option<User> {
        self.cache.user(user_id).await
    }

    pub async fn contains(&self, user_id: GenericId) -> bool {
        self.cache.contains_user(user_id).await
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

    pub async fn get(&self, guild_id: GenericId) -> Result<Guild, DiscordError> {
        if let Some(guild) = self.cache.guild(guild_id).await {
            return Ok(guild);
        }
        self.http.get_guild(guild_id).await
    }

    pub async fn cached(&self, guild_id: GenericId) -> Option<Guild> {
        self.cache.guild(guild_id).await
    }

    pub async fn contains(&self, guild_id: GenericId) -> bool {
        self.cache.contains_guild(guild_id).await
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

    pub async fn get(&self, channel_id: GenericId) -> Result<Channel, DiscordError> {
        if let Some(channel) = self.cache.channel(channel_id).await {
            return Ok(channel);
        }
        self.http.get_channel(channel_id).await
    }

    pub async fn cached(&self, channel_id: GenericId) -> Option<Channel> {
        self.cache.channel(channel_id).await
    }

    pub async fn contains(&self, channel_id: GenericId) -> bool {
        self.cache.contains_channel(channel_id).await
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
        guild_id: GenericId,
        user_id: GenericId,
    ) -> Result<Member, DiscordError> {
        if let Some(member) = self.cache.member(guild_id, user_id).await {
            return Ok(member);
        }
        self.http.get_member(guild_id, user_id).await
    }

    pub async fn cached(&self, guild_id: GenericId, user_id: GenericId) -> Option<Member> {
        self.cache.member(guild_id, user_id).await
    }

    pub async fn cached_arc(&self, guild_id: GenericId, user_id: GenericId) -> Option<Arc<Member>> {
        self.cache.member_arc(guild_id, user_id).await
    }

    pub async fn contains(&self, guild_id: GenericId, user_id: GenericId) -> bool {
        self.cache.contains_member(guild_id, user_id).await
    }

    pub async fn list_cached(&self, guild_id: GenericId) -> Vec<Member> {
        self.cache.members(guild_id).await
    }

    pub async fn list_cached_arc(&self, guild_id: GenericId) -> Vec<Arc<Member>> {
        self.cache.members_arc(guild_id).await
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
        channel_id: GenericId,
        message_id: GenericId,
    ) -> Result<Message, DiscordError> {
        if let Some(message) = self.cache.message(channel_id, message_id).await {
            return Ok(message);
        }
        self.http.get_message(channel_id, message_id).await
    }

    pub async fn cached(&self, channel_id: GenericId, message_id: GenericId) -> Option<Message> {
        self.cache.message(channel_id, message_id).await
    }

    pub async fn cached_arc(
        &self,
        channel_id: GenericId,
        message_id: GenericId,
    ) -> Option<Arc<Message>> {
        self.cache.message_arc(channel_id, message_id).await
    }

    pub async fn contains(&self, channel_id: GenericId, message_id: GenericId) -> bool {
        self.cache.contains_message(channel_id, message_id).await
    }

    pub async fn list_cached(&self, channel_id: GenericId) -> Vec<Message> {
        self.cache.messages(channel_id).await
    }

    pub async fn list_cached_arc(&self, channel_id: GenericId) -> Vec<Arc<Message>> {
        self.cache.messages_arc(channel_id).await
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

    pub async fn list(&self, guild_id: GenericId) -> Result<Vec<Role>, DiscordError> {
        let cached = self.cache.roles(guild_id).await;
        if !cached.is_empty() {
            return Ok(cached);
        }
        self.http.list_roles(guild_id).await
    }

    pub async fn cached(&self, guild_id: GenericId, role_id: GenericId) -> Option<Role> {
        self.cache.role(guild_id, role_id).await
    }

    pub async fn contains(&self, guild_id: GenericId, role_id: GenericId) -> bool {
        self.cache.contains_role(guild_id, role_id).await
    }

    pub async fn list_cached(&self, guild_id: GenericId) -> Vec<Role> {
        self.cache.roles(guild_id).await
    }
}

#[cfg(feature = "gateway")]
impl CachedManager<Guild> for GuildManager {
    async fn get(&self, id: GenericId) -> Result<Guild, DiscordError> {
        if let Some(guild) = self.cache.guild(id).await {
            return Ok(guild);
        }
        self.http.get_guild(id).await
    }

    async fn cached(&self, id: GenericId) -> Option<Guild> {
        self.cache.guild(id).await
    }

    async fn contains(&self, id: GenericId) -> bool {
        self.cache.contains_guild(id).await
    }

    async fn list_cached(&self) -> Vec<Guild> {
        self.cache.guilds().await
    }
}

#[cfg(feature = "gateway")]
impl CachedManager<Channel> for ChannelManager {
    async fn get(&self, id: GenericId) -> Result<Channel, DiscordError> {
        if let Some(channel) = self.cache.channel(id).await {
            return Ok(channel);
        }
        self.http.get_channel(id).await
    }

    async fn cached(&self, id: GenericId) -> Option<Channel> {
        self.cache.channel(id).await
    }

    async fn contains(&self, id: GenericId) -> bool {
        self.cache.contains_channel(id).await
    }

    async fn list_cached(&self) -> Vec<Channel> {
        self.cache.channels().await
    }
}

#[cfg(feature = "gateway")]
impl CachedManager<User> for UserManager {
    async fn get(&self, id: GenericId) -> Result<User, DiscordError> {
        if let Some(user) = self.cache.user(id).await {
            return Ok(user);
        }
        self.http.get_user(id).await
    }

    async fn cached(&self, id: GenericId) -> Option<User> {
        self.cache.user(id).await
    }

    async fn contains(&self, id: GenericId) -> bool {
        self.cache.contains_user(id).await
    }

    async fn list_cached(&self) -> Vec<User> {
        self.cache.users().await
    }
}
