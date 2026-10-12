//! discord.js-style convenience methods on model entities.
//!
//! Every method takes a `&RestClient` explicitly, so entities remain plain
//! data and can be freely cloned, cached, and sent between tasks. The
//! permission and formatting helpers near the end are pure and delegate to
//! [`crate::permissions`] and [`crate::formatters`].
//!
//! ```ignore
//! message.reply(&http, "hi").await?;
//! member.kick(&http, guild_id).await?;
//! guild.create_channel(&http, &CreateGuildChannel { name: "general".into(), ..Default::default() }).await?;
//! ```
//!
//! # Audit-log reasons
//!
//! Moderation methods honor the audit-log reason of the client they are
//! called with: pass `&http.with_reason("...")` to any method here to attach
//! an `X-Audit-Log-Reason` header. For the two most common cases
//! ([`Member::kick_with_reason`] and [`Member::ban_with_reason`]) dedicated
//! variants are provided.
//!
//! # Guild context on [`Member`]
//!
//! Discord's member payload does not include the guild ID (verified against
//! [`Member`]'s fields), so every REST method on `Member` takes
//! `guild_id: GuildId` explicitly rather than pretending the
//! entity knows which guild it came from.
//!
//! # Testing strategy
//!
//! `RestClient`'s base URL can only be redirected via a constructor that is
//! private to the `http` module, so this module cannot point a client at a
//! local mock server. The delegation layer is therefore kept one-call thin —
//! each async method forwards to exactly one existing `RestClient` method
//! (all of which are covered by `http`'s own tests) — and the unit tests
//! below focus on the pure, synchronous parts: URL/mention/tag formatting
//! and the extracted request-body builders (`reply_body`, `ban_body`,
//! `timeout_body`, `start_thread_body`, ...).

use crate::error::DiscordError;
use crate::http::RestClient;
use crate::model::{
    Channel, ChannelId, CreateChannelInvite, CreateDmChannel, CreateGuildBan, CreateGuildChannel,
    CreateGuildRole, Guild, GuildId, GuildMfaLevel, Id, IdMarker, Invite, Member, Message,
    MessageReference, MfaLevel, ModifyGuild, ModifyGuildMember, ModifyGuildRole, Role, RoleId,
    User, UserId,
};
use crate::request::{CreateMessage, EditMessage, ModifyChannel, StartThreadFromMessage};

/// Discord CDN base used by the synchronous URL helpers.
const CDN_BASE: &str = "https://cdn.discordapp.com";

// ---------------------------------------------------------------------------
// Pure request-body / URL builders (extracted so they are unit-testable).
// ---------------------------------------------------------------------------

/// Builds the `CreateMessage` body for replying to `message` with `content`,
/// keeping `fail_if_not_exists` at Discord's default.
fn reply_body(message: &Message, content: &str) -> CreateMessage {
    CreateMessage::new()
        .content(content)
        .message_reference(MessageReference::reply(message.id))
}

/// Builds a `CreateMessage` carrying only `content`.
fn content_body(content: &str) -> CreateMessage {
    CreateMessage::new().content(content)
}

/// Builds the ban request body, mapping `None` to "delete nothing".
fn ban_body(delete_message_seconds: Option<u64>) -> CreateGuildBan {
    CreateGuildBan {
        delete_message_seconds,
        ..CreateGuildBan::default()
    }
}

/// Builds the member-modify body for setting (`Some`) or clearing (`None`)
/// `communication_disabled_until`.
fn timeout_body(until: Option<String>) -> ModifyGuildMember {
    ModifyGuildMember {
        communication_disabled_until: Some(until),
        ..ModifyGuildMember::default()
    }
}

/// Formats a CDN asset URL such as `/icons/{id}/{hash}.png?size=128`,
/// switching to `.gif` for animated hashes (prefixed with `a_`).
fn cdn_asset_url<M: IdMarker>(kind: &str, id: Id<M>, hash: &str, size: Option<u16>) -> String {
    let ext = if hash.starts_with("a_") { "gif" } else { "png" };
    match size {
        Some(size) => format!("{CDN_BASE}/{kind}/{id}/{hash}.{ext}?size={size}"),
        None => format!("{CDN_BASE}/{kind}/{id}/{hash}.{ext}"),
    }
}

/// Default avatars on the CDN, `embed/avatars/0.png` through `5.png`
/// (Discord docs: Reference › Image Formatting › CDN Endpoints, "Default
/// User Avatar").
const DEFAULT_AVATAR_COUNT: u64 = 6;

/// Index of the default embed avatar for `user`: `(id >> 22) % 6`.
fn default_avatar_index(user: &User) -> u64 {
    (user.id.get() >> crate::model::SNOWFLAKE_TIMESTAMP_SHIFT) % DEFAULT_AVATAR_COUNT
}

/// Returns the ID of the [`User`] carried by a [`Member`], or a
/// [`DiscordError::Model`] when the payload omitted the user object.
fn member_user_id(member: &Member) -> Result<UserId, DiscordError> {
    member
        .user
        .as_ref()
        .map(|user| user.id)
        .ok_or_else(|| DiscordError::model("member payload has no user object"))
}

// ---------------------------------------------------------------------------
// Message
// ---------------------------------------------------------------------------

impl Message {
    /// Sends `content` as a reply to this message in the same channel, like
    /// discord.js's `message.reply(...)`.
    pub async fn reply(
        &self,
        http: &RestClient,
        content: impl AsRef<str>,
    ) -> Result<Message, DiscordError> {
        http.create_message(self.channel_id, &reply_body(self, content.as_ref()))
            .await
    }

    /// Edits this message's content and returns the updated message.
    pub async fn edit(
        &self,
        http: &RestClient,
        content: impl AsRef<str>,
    ) -> Result<Message, DiscordError> {
        http.edit_message(
            self.channel_id,
            self.id,
            &EditMessage::new().content(content.as_ref()),
        )
        .await
    }

    /// Edits this message with a full [`EditMessage`] body (embeds,
    /// components, flags, ...) and returns the updated message.
    pub async fn edit_with(
        &self,
        http: &RestClient,
        body: &EditMessage,
    ) -> Result<Message, DiscordError> {
        http.edit_message(self.channel_id, self.id, body).await
    }

    /// Deletes this message.
    pub async fn delete(&self, http: &RestClient) -> Result<(), DiscordError> {
        http.delete_message(self.channel_id, self.id).await
    }

    /// Adds a reaction from the current user; `emoji` is a unicode emoji or
    /// `name:id` for custom emoji, in the format `RestClient::add_reaction`
    /// expects.
    pub async fn react(&self, http: &RestClient, emoji: &str) -> Result<(), DiscordError> {
        http.add_reaction(self.channel_id, self.id, emoji).await
    }

    /// Removes the current user's reaction with `emoji`.
    pub async fn unreact(&self, http: &RestClient, emoji: &str) -> Result<(), DiscordError> {
        http.remove_reaction(self.channel_id, self.id, emoji).await
    }

    /// Pins this message in its channel.
    pub async fn pin(&self, http: &RestClient) -> Result<(), DiscordError> {
        http.pin_message(self.channel_id, self.id).await
    }

    /// Unpins this message from its channel.
    pub async fn unpin(&self, http: &RestClient) -> Result<(), DiscordError> {
        http.unpin_message(self.channel_id, self.id).await
    }

    /// Crossposts this message to channels following its announcement
    /// channel.
    pub async fn crosspost(&self, http: &RestClient) -> Result<Message, DiscordError> {
        http.crosspost_message(self.channel_id, self.id).await
    }

    /// Forwards this message into another channel, like discord.js's
    /// `message.forward(channel)`.
    pub async fn forward_to(
        &self,
        http: &RestClient,
        channel_id: ChannelId,
    ) -> Result<Message, DiscordError> {
        http.forward_message(self.channel_id, self.id, channel_id)
            .await
    }

    /// Starts a thread from this message with the given name.
    pub async fn start_thread(
        &self,
        http: &RestClient,
        name: impl AsRef<str>,
    ) -> Result<Channel, DiscordError> {
        http.create_thread_from_message(
            self.channel_id,
            self.id,
            &StartThreadFromMessage::new(name.as_ref()),
        )
        .await
    }

    /// Returns the `https://discord.com/channels/...` jump link for this
    /// message, using `@me` for direct messages.
    pub fn link(&self) -> String {
        match self.guild_id {
            Some(guild_id) => format!(
                "https://discord.com/channels/{guild_id}/{}/{}",
                self.channel_id, self.id
            ),
            None => format!(
                "https://discord.com/channels/@me/{}/{}",
                self.channel_id, self.id
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// Member
// ---------------------------------------------------------------------------

impl Member {
    /// Kicks this member from `guild_id`. For an audit-log reason use
    /// [`Member::kick_with_reason`] or call via `http.with_reason(...)`.
    pub async fn kick(&self, http: &RestClient, guild_id: GuildId) -> Result<(), DiscordError> {
        http.remove_guild_member(guild_id, member_user_id(self)?)
            .await
    }

    /// Kicks this member from `guild_id` with an audit-log reason.
    pub async fn kick_with_reason(
        &self,
        http: &RestClient,
        guild_id: GuildId,
        reason: impl Into<String>,
    ) -> Result<(), DiscordError> {
        self.kick(&http.with_reason(reason), guild_id).await
    }

    /// Bans this member from `guild_id`, deleting up to
    /// `delete_message_seconds` (0..=604800) of their recent messages. For an
    /// audit-log reason use [`Member::ban_with_reason`] or call via
    /// `http.with_reason(...)`.
    pub async fn ban(
        &self,
        http: &RestClient,
        guild_id: GuildId,
        delete_message_seconds: Option<u64>,
    ) -> Result<(), DiscordError> {
        http.create_guild_ban(
            guild_id,
            member_user_id(self)?,
            &ban_body(delete_message_seconds),
        )
        .await
    }

    /// Bans this member from `guild_id` with an audit-log reason.
    pub async fn ban_with_reason(
        &self,
        http: &RestClient,
        guild_id: GuildId,
        reason: impl Into<String>,
        delete_message_seconds: Option<u64>,
    ) -> Result<(), DiscordError> {
        self.ban(&http.with_reason(reason), guild_id, delete_message_seconds)
            .await
    }

    /// Times this member out until `until`, an ISO8601 timestamp for
    /// `communication_disabled_until`, and returns the updated member. Pass a
    /// `with_reason` client for an audit-log reason.
    pub async fn timeout(
        &self,
        http: &RestClient,
        guild_id: GuildId,
        until: impl Into<String>,
    ) -> Result<Member, DiscordError> {
        http.modify_guild_member(
            guild_id,
            member_user_id(self)?,
            &timeout_body(Some(until.into())),
        )
        .await
    }

    /// Removes this member's timeout by nulling
    /// `communication_disabled_until`, and returns the updated member.
    pub async fn remove_timeout(
        &self,
        http: &RestClient,
        guild_id: GuildId,
    ) -> Result<Member, DiscordError> {
        http.modify_guild_member(guild_id, member_user_id(self)?, &timeout_body(None))
            .await
    }

    /// Adds a role to this member.
    pub async fn add_role(
        &self,
        http: &RestClient,
        guild_id: GuildId,
        role_id: RoleId,
    ) -> Result<(), DiscordError> {
        http.add_guild_member_role(guild_id, member_user_id(self)?, role_id)
            .await
    }

    /// Removes a role from this member.
    pub async fn remove_role(
        &self,
        http: &RestClient,
        guild_id: GuildId,
        role_id: RoleId,
    ) -> Result<(), DiscordError> {
        http.remove_guild_member_role(guild_id, member_user_id(self)?, role_id)
            .await
    }

    /// Applies a [`ModifyGuildMember`] body to this member (nick, roles,
    /// mute/deaf, voice channel, timeout, flags) and returns the updated
    /// member.
    pub async fn edit(
        &self,
        http: &RestClient,
        guild_id: GuildId,
        body: &ModifyGuildMember,
    ) -> Result<Member, DiscordError> {
        http.modify_guild_member(guild_id, member_user_id(self)?, body)
            .await
    }

    /// Returns the name shown for this member: guild nick, then the user's
    /// global display name, then their username; empty when the payload has
    /// no user object.
    pub fn display_name(&self) -> &str {
        if let Some(nick) = self.nick.as_deref() {
            return nick;
        }
        match &self.user {
            Some(user) => user.global_name.as_deref().unwrap_or(&user.username),
            None => "",
        }
    }
}

// ---------------------------------------------------------------------------
// Guild
// ---------------------------------------------------------------------------

impl Guild {
    /// Edits this guild's settings and returns the updated guild.
    pub async fn edit(&self, http: &RestClient, body: &ModifyGuild) -> Result<Guild, DiscordError> {
        http.modify_guild(self.id, body).await
    }

    /// Deletes this guild. Requires ownership, which bots no longer have.
    #[deprecated(
        since = "3.0.0",
        note = "Discord removed bot guild ownership in 2025; see RestClient::delete_guild"
    )]
    #[expect(deprecated, reason = "wraps the deprecated route it mirrors")]
    pub async fn delete(&self, http: &RestClient) -> Result<(), DiscordError> {
        http.delete_guild(self.id).await
    }

    /// Makes the current user leave this guild.
    pub async fn leave(&self, http: &RestClient) -> Result<(), DiscordError> {
        http.leave_guild(self.id).await
    }

    /// Fetches this guild's channels from the API.
    pub async fn fetch_channels(&self, http: &RestClient) -> Result<Vec<Channel>, DiscordError> {
        http.get_guild_channels(self.id).await
    }

    /// Creates a channel in this guild.
    pub async fn create_channel(
        &self,
        http: &RestClient,
        body: &CreateGuildChannel,
    ) -> Result<Channel, DiscordError> {
        http.create_guild_channel(self.id, body).await
    }

    /// Fetches a single member of this guild.
    pub async fn fetch_member(
        &self,
        http: &RestClient,
        user_id: UserId,
    ) -> Result<Member, DiscordError> {
        http.get_member(self.id, user_id).await
    }

    /// Fetches this guild's roles from the API.
    pub async fn fetch_roles(&self, http: &RestClient) -> Result<Vec<Role>, DiscordError> {
        http.list_roles(self.id).await
    }

    /// Creates a role in this guild.
    pub async fn create_role(
        &self,
        http: &RestClient,
        body: &CreateGuildRole,
    ) -> Result<Role, DiscordError> {
        http.create_guild_role(self.id, body).await
    }

    /// Bans a user from this guild, deleting up to `delete_message_seconds`
    /// (0..=604800) of their recent messages. Pass a `with_reason` client
    /// for an audit-log reason.
    pub async fn ban(
        &self,
        http: &RestClient,
        user_id: UserId,
        delete_message_seconds: Option<u64>,
    ) -> Result<(), DiscordError> {
        http.create_guild_ban(self.id, user_id, &ban_body(delete_message_seconds))
            .await
    }

    /// Removes a ban from this guild.
    pub async fn unban(&self, http: &RestClient, user_id: UserId) -> Result<(), DiscordError> {
        http.remove_guild_ban(self.id, user_id).await
    }

    /// Kicks a user from this guild. Pass a `with_reason` client for an
    /// audit-log reason.
    pub async fn kick(&self, http: &RestClient, user_id: UserId) -> Result<(), DiscordError> {
        http.remove_guild_member(self.id, user_id).await
    }

    /// Sets this guild's required MFA level for moderation actions and
    /// returns the updated level. Owner-only, so unusable by bots.
    #[deprecated(
        since = "3.0.0",
        note = "Discord removed bot guild ownership in 2025; see RestClient::modify_guild_mfa_level"
    )]
    #[expect(deprecated, reason = "wraps the deprecated route it mirrors")]
    pub async fn set_mfa_level(
        &self,
        http: &RestClient,
        level: MfaLevel,
    ) -> Result<GuildMfaLevel, DiscordError> {
        http.modify_guild_mfa_level(self.id, level).await
    }

    /// Returns the CDN URL of this guild's icon (`.gif` when animated), or
    /// `None` when the guild has no icon.
    pub fn icon_url(&self, size: Option<u16>) -> Option<String> {
        self.icon
            .as_deref()
            .map(|hash| cdn_asset_url("icons", self.id, hash, size))
    }

    /// Returns the CDN URL of this guild's banner (`.gif` when animated), or
    /// `None` when the guild has no banner.
    pub fn banner_url(&self, size: Option<u16>) -> Option<String> {
        self.banner
            .as_deref()
            .map(|hash| cdn_asset_url("banners", self.id, hash, size))
    }
}

// ---------------------------------------------------------------------------
// Channel
// ---------------------------------------------------------------------------

impl Channel {
    /// Returns the channel's flags as a typed bitfield (empty when absent).
    pub fn channel_flags(&self) -> crate::bitfield::ChannelFlags {
        crate::bitfield::ChannelFlags::from_bits(self.flags.unwrap_or(0))
    }

    /// Returns true when Discord obfuscated this channel because the bot
    /// cannot view it (`CHANNEL_OBFUSCATED`, gateway only).
    ///
    /// Only `id`, `type`, `position`, and `parent_id` are reliable on an
    /// obfuscated channel; its name is a placeholder and its permission
    /// overwrites are synthetic. Discord says to detect obfuscation by this
    /// flag, never by inspecting the name. Mandatory for all bots from
    /// 2026-11-16.
    pub fn is_obfuscated(&self) -> bool {
        self.channel_flags()
            .contains(crate::bitfield::channel_flags::CHANNEL_OBFUSCATED.bits())
    }

    /// Sends a plain-text message to this channel.
    pub async fn send(
        &self,
        http: &RestClient,
        content: impl AsRef<str>,
    ) -> Result<Message, DiscordError> {
        http.create_message(self.id, &content_body(content.as_ref()))
            .await
    }

    /// Sends a full [`CreateMessage`] body (embeds, components, ...) to this
    /// channel.
    pub async fn send_message(
        &self,
        http: &RestClient,
        body: &CreateMessage,
    ) -> Result<Message, DiscordError> {
        http.create_message(self.id, body).await
    }

    /// Edits this channel (Modify Channel) and returns the updated channel.
    pub async fn edit(
        &self,
        http: &RestClient,
        body: &ModifyChannel,
    ) -> Result<Channel, DiscordError> {
        http.update_channel(self.id, body).await
    }

    /// Deletes this channel (or closes it, for DMs) and returns the deleted
    /// channel.
    pub async fn delete(&self, http: &RestClient) -> Result<Channel, DiscordError> {
        http.delete_channel(self.id).await
    }

    /// Creates an invite to this channel.
    pub async fn create_invite(
        &self,
        http: &RestClient,
        body: &CreateChannelInvite,
    ) -> Result<Invite, DiscordError> {
        http.create_channel_invite(self.id, body).await
    }

    /// Returns the `<#id>` mention for this channel.
    pub fn mention(&self) -> String {
        format!("<#{}>", self.id)
    }

    /// Whether this kind of channel carries a message timeline; shorthand
    /// for [`ChannelType::is_text_based`](crate::ChannelType::is_text_based)
    /// on [`Self::kind`] (discord.js `isTextBased()`).
    ///
    /// This classifies the channel type only. It does not mean the bot can
    /// send here: permissions, archived or locked threads, timeouts, and
    /// obfuscated channels can still forbid it (see
    /// [`crate::permissions`]).
    pub fn is_text_based(&self) -> bool {
        self.kind.is_text_based()
    }

    /// Whether this is a voice or stage channel; shorthand for
    /// [`ChannelType::is_voice_based`](crate::ChannelType::is_voice_based).
    pub fn is_voice_based(&self) -> bool {
        self.kind.is_voice_based()
    }

    /// Whether this is an announcement, public, or private thread; shorthand
    /// for [`ChannelType::is_thread`](crate::ChannelType::is_thread).
    pub fn is_thread(&self) -> bool {
        self.kind.is_thread()
    }
}

// ---------------------------------------------------------------------------
// Role
// ---------------------------------------------------------------------------

impl Role {
    /// Edits this role in `guild_id` and returns the updated role.
    pub async fn edit(
        &self,
        http: &RestClient,
        guild_id: GuildId,
        body: &ModifyGuildRole,
    ) -> Result<Role, DiscordError> {
        http.modify_guild_role(guild_id, self.id, body).await
    }

    /// Deletes this role from `guild_id`.
    pub async fn delete(&self, http: &RestClient, guild_id: GuildId) -> Result<(), DiscordError> {
        http.delete_role(guild_id, self.id).await
    }

    /// Returns the `<@&id>` mention for this role.
    pub fn mention(&self) -> String {
        format!("<@&{}>", self.id)
    }
}

// ---------------------------------------------------------------------------
// User
// ---------------------------------------------------------------------------

impl User {
    /// Opens (or reuses) the DM channel with this user.
    pub async fn create_dm(&self, http: &RestClient) -> Result<Channel, DiscordError> {
        http.create_dm_channel(&CreateDmChannel {
            recipient_id: self.id,
        })
        .await
    }

    /// Sends a direct message to this user, opening the DM channel first.
    pub async fn dm(
        &self,
        http: &RestClient,
        content: impl AsRef<str>,
    ) -> Result<Message, DiscordError> {
        let channel = self.create_dm(http).await?;
        http.create_message(channel.id, &content_body(content.as_ref()))
            .await
    }

    /// Returns the user's tag: `name#1234` for legacy users, the plain
    /// username for users migrated to unique usernames (discriminator `0`).
    pub fn tag(&self) -> String {
        match self.discriminator.as_deref() {
            Some(discriminator) if !discriminator.is_empty() && discriminator != "0" => {
                format!("{}#{discriminator}", self.username)
            }
            _ => self.username.clone(),
        }
    }

    /// Returns the `<@id>` mention for this user.
    pub fn mention(&self) -> String {
        format!("<@{}>", self.id)
    }

    /// Returns the CDN URL of this user's avatar (`.gif` when animated), or
    /// `None` when they have no custom avatar.
    pub fn avatar_url(&self, size: Option<u16>) -> Option<String> {
        self.avatar
            .as_deref()
            .map(|hash| cdn_asset_url("avatars", self.id, hash, size))
    }

    /// Returns the user's default embed avatar URL,
    /// `embed/avatars/{(id >> 22) % 6}.png`. The CDN ignores `size` for
    /// default avatars, so none is appended.
    pub fn default_avatar_url(&self) -> String {
        format!(
            "{CDN_BASE}/embed/avatars/{}.png",
            default_avatar_index(self)
        )
    }

    /// Returns the avatar shown for this user: their custom avatar, or the
    /// default embed avatar when they have none.
    pub fn display_avatar_url(&self, size: Option<u16>) -> String {
        self.avatar_url(size)
            .unwrap_or_else(|| self.default_avatar_url())
    }
}

// ---------------------------------------------------------------------------
// Permissions and formatting (thin delegates; tested in `permissions` and
// `formatters`)
// ---------------------------------------------------------------------------

impl Channel {
    /// Effective permissions of `member` in this channel. discord.js
    /// `channel.permissionsFor(member)` stops at the overwrite calculation,
    /// which is kept in `ComputedPermissions::raw`. `parent` is required when
    /// this is a thread (unless the member is owner or administrator). The
    /// timeout is checked against the current time; use
    /// [`crate::permissions::permissions_for`] to pass one explicitly.
    pub fn permissions_for(
        &self,
        guild: &Guild,
        member: &Member,
        parent: Option<&Channel>,
    ) -> Result<crate::permissions::ChannelPermissions, crate::permissions::PermissionDataError>
    {
        crate::permissions::permissions_for(
            guild,
            member,
            self,
            parent,
            std::time::SystemTime::now(),
        )
    }
}

impl Member {
    /// Guild-level permissions of this member: discord.js
    /// `member.permissions` in `ComputedPermissions::raw`, and with an active
    /// timeout (checked against the current time) applied in `permissions`.
    pub fn guild_permissions(
        &self,
        guild: &Guild,
    ) -> Result<crate::permissions::ComputedPermissions, crate::permissions::PermissionDataError>
    {
        crate::permissions::guild_permissions(guild, self, std::time::SystemTime::now())
    }

    /// Returns the `<@id>` mention for this member, or `None` when the
    /// payload has no user object.
    pub fn mention(&self) -> Option<String> {
        self.user
            .as_ref()
            .map(|user| crate::formatters::user_mention(&user.id).to_string())
    }
}

impl Message {
    /// This message's content with mentions replaced by names from
    /// `resolver`, like discord.js `message.cleanContent`; see
    /// [`crate::formatters::clean_content`].
    pub fn clean_content<R>(&self, resolver: &R) -> std::borrow::Cow<'_, str>
    where
        R: crate::formatters::MentionResolver + ?Sized,
    {
        crate::formatters::clean_content(&self.content, resolver)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{test_id, MessageReferenceType};

    fn user(id: u64) -> User {
        User {
            id: test_id(id),
            username: "tester".to_string(),
            ..User::fixture()
        }
    }

    // -- Message ----------------------------------------------------------

    #[test]
    fn message_link_uses_guild_id_when_present() {
        let message = Message {
            id: test_id(3),
            channel_id: test_id(2),
            guild_id: Some(test_id(1)),
            ..Message::fixture()
        };
        assert_eq!(message.link(), "https://discord.com/channels/1/2/3");
    }

    #[test]
    fn message_link_uses_at_me_for_dms() {
        let message = Message {
            id: test_id(3),
            channel_id: test_id(2),
            ..Message::fixture()
        };
        assert_eq!(message.link(), "https://discord.com/channels/@me/2/3");
    }

    #[test]
    fn reply_body_sets_default_reply_reference() {
        let message = Message {
            id: test_id(42),
            channel_id: test_id(7),
            ..Message::fixture()
        };
        let body = reply_body(&message, "hi");
        assert_eq!(body.content.as_deref(), Some("hi"));
        let reference = body.message_reference.clone().expect("reply reference");
        assert_eq!(reference.kind, Some(MessageReferenceType::DEFAULT));
        assert_eq!(reference.message_id, Some(test_id(42)));
        // Same-channel reply: channel_id omitted, fail_if_not_exists left at
        // Discord's default.
        assert_eq!(reference.channel_id, None);
        assert_eq!(reference.fail_if_not_exists, None);
        // Nothing else sneaks into the payload.
        let json = serde_json::to_value(&body).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "content": "hi",
                "message_reference": { "type": 0, "message_id": "42" }
            })
        );
    }

    #[test]
    fn start_thread_body_contains_only_name() {
        assert_eq!(
            serde_json::to_value(StartThreadFromMessage::new("bugs")).unwrap(),
            serde_json::json!({ "name": "bugs" })
        );
    }

    // -- Member -----------------------------------------------------------

    #[test]
    fn display_name_prefers_nick_then_global_name_then_username() {
        let mut member = Member {
            user: Some(User {
                global_name: Some("Global".to_string()),
                ..user(1)
            }),
            nick: Some("Nick".to_string()),
            ..Member::default()
        };
        assert_eq!(member.display_name(), "Nick");

        member.nick = None;
        assert_eq!(member.display_name(), "Global");

        member.user.as_mut().unwrap().global_name = None;
        assert_eq!(member.display_name(), "tester");

        member.user = None;
        assert_eq!(member.display_name(), "");
    }

    #[test]
    fn member_user_id_errors_without_user() {
        let member = Member::default();
        assert!(matches!(
            member_user_id(&member),
            Err(DiscordError::Model { .. })
        ));
        let member = Member {
            user: Some(user(9)),
            ..Member::default()
        };
        assert_eq!(member_user_id(&member).unwrap(), test_id(9));
    }

    #[test]
    fn ban_body_serializes_only_seconds() {
        assert_eq!(
            serde_json::to_value(ban_body(Some(600))).unwrap(),
            serde_json::json!({ "delete_message_seconds": 600 })
        );
        assert_eq!(
            serde_json::to_value(ban_body(None)).unwrap(),
            serde_json::json!({})
        );
    }

    #[test]
    fn timeout_body_sets_and_clears_timestamp() {
        assert_eq!(
            serde_json::to_value(timeout_body(Some("2026-01-01T00:00:00Z".to_string()))).unwrap(),
            serde_json::json!({ "communication_disabled_until": "2026-01-01T00:00:00Z" })
        );
        assert_eq!(
            serde_json::to_value(timeout_body(None)).unwrap(),
            serde_json::json!({ "communication_disabled_until": null })
        );
    }

    // -- Guild CDN helpers --------------------------------------------------

    #[test]
    fn guild_icon_and_banner_urls() {
        let guild = Guild {
            id: test_id(10),
            icon: Some("abc".to_string()),
            banner: Some("a_def".to_string()),
            ..Guild::fixture()
        };
        assert_eq!(
            guild.icon_url(Some(128)).as_deref(),
            Some("https://cdn.discordapp.com/icons/10/abc.png?size=128")
        );
        assert_eq!(
            guild.icon_url(None).as_deref(),
            Some("https://cdn.discordapp.com/icons/10/abc.png")
        );
        // Animated hash switches to .gif.
        assert_eq!(
            guild.banner_url(Some(512)).as_deref(),
            Some("https://cdn.discordapp.com/banners/10/a_def.gif?size=512")
        );
        let bare = Guild {
            id: test_id(10),
            ..Guild::fixture()
        };
        assert_eq!(bare.icon_url(None), None);
        assert_eq!(bare.banner_url(None), None);
    }

    // -- Channel ------------------------------------------------------------

    #[test]
    fn channel_mention_and_type_classification() {
        // Wire numbers on purpose: the helpers must agree with Discord's
        // documented type values, not just with the constants' names.
        let channel = |kind: u8| Channel {
            id: test_id(5),
            kind: crate::ChannelType::new(kind),
            ..Channel::fixture()
        };
        assert_eq!(channel(0).mention(), "<#5>");

        for kind in [0, 1, 2, 3, 5, 10, 11, 12, 13] {
            assert!(channel(kind).is_text_based(), "type {kind} is text-based");
        }
        for kind in [4, 14, 15, 16] {
            assert!(!channel(kind).is_text_based(), "type {kind} not text-based");
        }
        assert!(channel(2).is_voice_based());
        assert!(channel(13).is_voice_based());
        assert!(!channel(0).is_voice_based());
        assert!(channel(10).is_thread());
        assert!(channel(11).is_thread());
        assert!(channel(12).is_thread());
        assert!(!channel(0).is_thread());
    }

    // -- Role ---------------------------------------------------------------

    #[test]
    fn role_mention_format() {
        let role = Role {
            id: test_id(77),
            ..Role::fixture()
        };
        assert_eq!(role.mention(), "<@&77>");
    }

    // -- User ---------------------------------------------------------------

    #[test]
    fn user_tag_handles_legacy_and_migrated_users() {
        let mut u = user(1);
        u.discriminator = Some("1234".to_string());
        assert_eq!(u.tag(), "tester#1234");

        u.discriminator = Some("0".to_string());
        assert_eq!(u.tag(), "tester");

        u.discriminator = None;
        assert_eq!(u.tag(), "tester");
    }

    #[test]
    fn user_mention_format() {
        assert_eq!(user(1).mention(), "<@1>");
    }

    #[test]
    fn user_avatar_urls_with_animation_and_size() {
        let mut u = user(1);
        assert_eq!(u.avatar_url(None), None);

        u.avatar = Some("abc".to_string());
        assert_eq!(
            u.avatar_url(Some(256)).as_deref(),
            Some("https://cdn.discordapp.com/avatars/1/abc.png?size=256")
        );
        u.avatar = Some("a_abc".to_string());
        assert_eq!(
            u.avatar_url(None).as_deref(),
            Some("https://cdn.discordapp.com/avatars/1/a_abc.gif")
        );
        assert_eq!(
            u.display_avatar_url(Some(64)),
            "https://cdn.discordapp.com/avatars/1/a_abc.gif?size=64"
        );
    }

    #[test]
    fn default_avatar_uses_snowflake_shift_modulo_six() {
        // (id >> 22) % 6: choose an id whose shifted value is 7 -> index 1.
        let id = 7u64 << 22;
        let u = user(id);
        assert_eq!(default_avatar_index(&u), 1);
        assert_eq!(
            u.default_avatar_url(),
            "https://cdn.discordapp.com/embed/avatars/1.png"
        );
        // Falls back to the default avatar when no custom avatar is set,
        // ignoring the size parameter.
        assert_eq!(
            u.display_avatar_url(Some(128)),
            "https://cdn.discordapp.com/embed/avatars/1.png"
        );
    }
}
