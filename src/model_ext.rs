//! discord.js-style convenience methods on model entities.
//!
//! Every method takes a `&RestClient` explicitly, so entities remain plain
//! data and can be freely cloned, cached, and sent between tasks.
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
//! `guild_id: impl Into<Snowflake>` explicitly rather than pretending the
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
    Channel, CreateChannelInvite, CreateDmChannel, CreateGuildBan, CreateGuildChannel,
    CreateGuildRole, CreateMessage, Guild, GuildMfaLevel, Invite, Member, Message,
    MessageReference, ModifyGuild, ModifyGuildMember, ModifyGuildRole, Role, Snowflake, User,
};

/// Discord CDN base used by the synchronous URL helpers.
const CDN_BASE: &str = "https://cdn.discordapp.com";

// Channel type constants (Discord `type` field). `ChannelType` in
// `model::channel` does not include the thread variants, and `Channel.kind`
// is a plain `u8`, so the classification helpers work on raw values.
const CHANNEL_TEXT: u8 = 0;
const CHANNEL_DM: u8 = 1;
const CHANNEL_VOICE: u8 = 2;
const CHANNEL_GROUP_DM: u8 = 3;
const CHANNEL_NEWS: u8 = 5;
const CHANNEL_ANNOUNCEMENT_THREAD: u8 = 10;
const CHANNEL_PUBLIC_THREAD: u8 = 11;
const CHANNEL_PRIVATE_THREAD: u8 = 12;
const CHANNEL_STAGE_VOICE: u8 = 13;

// ---------------------------------------------------------------------------
// Pure request-body / URL builders (extracted so they are unit-testable).
// ---------------------------------------------------------------------------

/// Builds the `CreateMessage` body for replying to `message` with `content`,
/// keeping `fail_if_not_exists` at Discord's default.
fn reply_body(message: &Message, content: &str) -> CreateMessage {
    CreateMessage {
        content: Some(content.to_string()),
        message_reference: Some(MessageReference::reply(message.id.clone())),
        ..CreateMessage::default()
    }
}

/// Builds a `CreateMessage` carrying only `content`.
fn content_body(content: &str) -> CreateMessage {
    CreateMessage {
        content: Some(content.to_string()),
        ..CreateMessage::default()
    }
}

/// Builds the JSON body for `POST .../messages/{id}/threads`. The crate has
/// no typed request struct for this route (`create_thread_from_message`
/// takes `&Value`), so the body is assembled here in one audited place.
fn start_thread_body(name: &str) -> serde_json::Value {
    serde_json::json!({ "name": name })
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
fn cdn_asset_url(kind: &str, id: &Snowflake, hash: &str, size: Option<u16>) -> String {
    let ext = if hash.starts_with("a_") { "gif" } else { "png" };
    match size {
        Some(size) => format!("{CDN_BASE}/{kind}/{id}/{hash}.{ext}?size={size}"),
        None => format!("{CDN_BASE}/{kind}/{id}/{hash}.{ext}"),
    }
}

/// Index of the default embed avatar for `user`: `(id >> 22) % 6`, falling
/// back to `0` when the snowflake is not numeric.
fn default_avatar_index(user: &User) -> u64 {
    user.id.as_u64().map(|id| (id >> 22) % 6).unwrap_or(0)
}

/// Returns the ID of the [`User`] carried by a [`Member`], or a
/// [`DiscordError::Model`] when the payload omitted the user object.
fn member_user_id(member: &Member) -> Result<Snowflake, DiscordError> {
    member
        .user
        .as_ref()
        .map(|user| user.id.clone())
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
        http.create_message(self.channel_id.clone(), &reply_body(self, content.as_ref()))
            .await
    }

    /// Edits this message's content and returns the updated message.
    pub async fn edit(
        &self,
        http: &RestClient,
        content: impl AsRef<str>,
    ) -> Result<Message, DiscordError> {
        http.update_message(
            self.channel_id.clone(),
            self.id.clone(),
            &content_body(content.as_ref()),
        )
        .await
    }

    /// Edits this message with a full [`CreateMessage`] body (embeds,
    /// components, flags, ...) and returns the updated message.
    pub async fn edit_with(
        &self,
        http: &RestClient,
        body: &CreateMessage,
    ) -> Result<Message, DiscordError> {
        http.update_message(self.channel_id.clone(), self.id.clone(), body)
            .await
    }

    /// Deletes this message.
    pub async fn delete(&self, http: &RestClient) -> Result<(), DiscordError> {
        http.delete_message(self.channel_id.clone(), self.id.clone())
            .await
    }

    /// Adds a reaction from the current user; `emoji` is a unicode emoji or
    /// `name:id` for custom emoji, in the format `RestClient::add_reaction`
    /// expects.
    pub async fn react(&self, http: &RestClient, emoji: &str) -> Result<(), DiscordError> {
        http.add_reaction(self.channel_id.clone(), self.id.clone(), emoji)
            .await
    }

    /// Removes the current user's reaction with `emoji`.
    pub async fn unreact(&self, http: &RestClient, emoji: &str) -> Result<(), DiscordError> {
        http.remove_reaction(self.channel_id.clone(), self.id.clone(), emoji)
            .await
    }

    /// Pins this message in its channel.
    pub async fn pin(&self, http: &RestClient) -> Result<(), DiscordError> {
        http.pin_message(self.channel_id.clone(), self.id.clone())
            .await
    }

    /// Unpins this message from its channel.
    pub async fn unpin(&self, http: &RestClient) -> Result<(), DiscordError> {
        http.unpin_message(self.channel_id.clone(), self.id.clone())
            .await
    }

    /// Crossposts this message to channels following its announcement
    /// channel.
    pub async fn crosspost(&self, http: &RestClient) -> Result<Message, DiscordError> {
        http.crosspost_message(self.channel_id.clone(), self.id.clone())
            .await
    }

    /// Forwards this message into another channel, like discord.js's
    /// `message.forward(channel)`.
    pub async fn forward_to(
        &self,
        http: &RestClient,
        channel_id: impl Into<Snowflake>,
    ) -> Result<Message, DiscordError> {
        http.forward_message(self.channel_id.clone(), self.id.clone(), channel_id)
            .await
    }

    /// Starts a thread from this message with the given name.
    pub async fn start_thread(
        &self,
        http: &RestClient,
        name: impl AsRef<str>,
    ) -> Result<Channel, DiscordError> {
        http.create_thread_from_message(
            self.channel_id.clone(),
            self.id.clone(),
            &start_thread_body(name.as_ref()),
        )
        .await
    }

    /// Returns the `https://discord.com/channels/...` jump link for this
    /// message, using `@me` for direct messages.
    pub fn link(&self) -> String {
        let guild = self
            .guild_id
            .as_ref()
            .map(Snowflake::as_str)
            .unwrap_or("@me");
        format!(
            "https://discord.com/channels/{guild}/{}/{}",
            self.channel_id, self.id
        )
    }
}

// ---------------------------------------------------------------------------
// Member
// ---------------------------------------------------------------------------

impl Member {
    /// Kicks this member from `guild_id`. For an audit-log reason use
    /// [`Member::kick_with_reason`] or call via `http.with_reason(...)`.
    pub async fn kick(
        &self,
        http: &RestClient,
        guild_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        http.remove_guild_member(guild_id, member_user_id(self)?)
            .await
    }

    /// Kicks this member from `guild_id` with an audit-log reason.
    pub async fn kick_with_reason(
        &self,
        http: &RestClient,
        guild_id: impl Into<Snowflake>,
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
        guild_id: impl Into<Snowflake>,
        delete_message_seconds: Option<u64>,
    ) -> Result<(), DiscordError> {
        http.create_guild_ban_typed(
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
        guild_id: impl Into<Snowflake>,
        reason: impl Into<String>,
        delete_message_seconds: Option<u64>,
    ) -> Result<(), DiscordError> {
        self.ban(&http.with_reason(reason), guild_id, delete_message_seconds)
            .await
    }

    /// Times this member out until `until`, an ISO8601 timestamp for
    /// `communication_disabled_until`. Pass a `with_reason` client for an
    /// audit-log reason.
    pub async fn timeout(
        &self,
        http: &RestClient,
        guild_id: impl Into<Snowflake>,
        until: impl Into<String>,
    ) -> Result<(), DiscordError> {
        http.modify_guild_member_typed(
            guild_id,
            member_user_id(self)?,
            &timeout_body(Some(until.into())),
        )
        .await
    }

    /// Removes this member's timeout by nulling
    /// `communication_disabled_until`.
    pub async fn remove_timeout(
        &self,
        http: &RestClient,
        guild_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        http.modify_guild_member_typed(guild_id, member_user_id(self)?, &timeout_body(None))
            .await
    }

    /// Adds a role to this member.
    pub async fn add_role(
        &self,
        http: &RestClient,
        guild_id: impl Into<Snowflake>,
        role_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        http.add_guild_member_role(guild_id, member_user_id(self)?, role_id)
            .await
    }

    /// Removes a role from this member.
    pub async fn remove_role(
        &self,
        http: &RestClient,
        guild_id: impl Into<Snowflake>,
        role_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        http.remove_guild_member_role(guild_id, member_user_id(self)?, role_id)
            .await
    }

    /// Applies a [`ModifyGuildMember`] body to this member (nick, roles,
    /// mute/deaf, voice channel, timeout, flags).
    pub async fn edit(
        &self,
        http: &RestClient,
        guild_id: impl Into<Snowflake>,
        body: &ModifyGuildMember,
    ) -> Result<(), DiscordError> {
        http.modify_guild_member_typed(guild_id, member_user_id(self)?, body)
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
        http.modify_guild(self.id.clone(), body).await
    }

    /// Deletes this guild. The bot must own it.
    pub async fn delete(&self, http: &RestClient) -> Result<(), DiscordError> {
        http.delete_guild(self.id.clone()).await
    }

    /// Makes the current user leave this guild.
    pub async fn leave(&self, http: &RestClient) -> Result<(), DiscordError> {
        http.leave_guild(self.id.clone()).await
    }

    /// Fetches this guild's channels from the API.
    pub async fn fetch_channels(&self, http: &RestClient) -> Result<Vec<Channel>, DiscordError> {
        http.get_guild_channels(self.id.clone()).await
    }

    /// Creates a channel in this guild.
    pub async fn create_channel(
        &self,
        http: &RestClient,
        body: &CreateGuildChannel,
    ) -> Result<Channel, DiscordError> {
        http.create_guild_channel_from_request(self.id.clone(), body)
            .await
    }

    /// Fetches a single member of this guild.
    pub async fn fetch_member(
        &self,
        http: &RestClient,
        user_id: impl Into<Snowflake>,
    ) -> Result<Member, DiscordError> {
        http.get_member(self.id.clone(), user_id).await
    }

    /// Fetches this guild's roles from the API.
    pub async fn fetch_roles(&self, http: &RestClient) -> Result<Vec<Role>, DiscordError> {
        http.list_roles(self.id.clone()).await
    }

    /// Creates a role in this guild.
    pub async fn create_role(
        &self,
        http: &RestClient,
        body: &CreateGuildRole,
    ) -> Result<Role, DiscordError> {
        http.create_role_typed(self.id.clone(), body).await
    }

    /// Bans a user from this guild, deleting up to `delete_message_seconds`
    /// (0..=604800) of their recent messages. Pass a `with_reason` client
    /// for an audit-log reason.
    pub async fn ban(
        &self,
        http: &RestClient,
        user_id: impl Into<Snowflake>,
        delete_message_seconds: Option<u64>,
    ) -> Result<(), DiscordError> {
        http.create_guild_ban_typed(self.id.clone(), user_id, &ban_body(delete_message_seconds))
            .await
    }

    /// Removes a ban from this guild.
    pub async fn unban(
        &self,
        http: &RestClient,
        user_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        http.remove_guild_ban(self.id.clone(), user_id).await
    }

    /// Kicks a user from this guild. Pass a `with_reason` client for an
    /// audit-log reason.
    pub async fn kick(
        &self,
        http: &RestClient,
        user_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        http.remove_guild_member(self.id.clone(), user_id).await
    }

    /// Sets this guild's required MFA level for moderation actions and
    /// returns the updated level.
    pub async fn set_mfa_level(
        &self,
        http: &RestClient,
        level: u64,
    ) -> Result<GuildMfaLevel, DiscordError> {
        http.modify_guild_mfa_level(self.id.clone(), level).await
    }

    /// Returns the CDN URL of this guild's icon (`.gif` when animated), or
    /// `None` when the guild has no icon.
    pub fn icon_url(&self, size: Option<u16>) -> Option<String> {
        self.icon
            .as_deref()
            .map(|hash| cdn_asset_url("icons", &self.id, hash, size))
    }

    /// Returns the CDN URL of this guild's banner (`.gif` when animated), or
    /// `None` when the guild has no banner.
    pub fn banner_url(&self, size: Option<u16>) -> Option<String> {
        self.banner
            .as_deref()
            .map(|hash| cdn_asset_url("banners", &self.id, hash, size))
    }
}

// ---------------------------------------------------------------------------
// Channel
// ---------------------------------------------------------------------------

impl Channel {
    /// Sends a plain-text message to this channel.
    pub async fn send(
        &self,
        http: &RestClient,
        content: impl AsRef<str>,
    ) -> Result<Message, DiscordError> {
        http.create_message(self.id.clone(), &content_body(content.as_ref()))
            .await
    }

    /// Sends a full [`CreateMessage`] body (embeds, components, ...) to this
    /// channel.
    pub async fn send_message(
        &self,
        http: &RestClient,
        body: &CreateMessage,
    ) -> Result<Message, DiscordError> {
        http.create_message(self.id.clone(), body).await
    }

    /// Edits this channel with any serializable modify-channel body (the
    /// crate has no typed struct for `PATCH /channels/{id}`) and returns the
    /// updated channel.
    pub async fn edit<B>(&self, http: &RestClient, body: &B) -> Result<Channel, DiscordError>
    where
        B: serde::Serialize + ?Sized,
    {
        http.update_channel_typed(self.id.clone(), body).await
    }

    /// Deletes this channel (or closes it, for DMs) and returns the deleted
    /// channel.
    pub async fn delete(&self, http: &RestClient) -> Result<Channel, DiscordError> {
        http.delete_channel(self.id.clone()).await
    }

    /// Creates an invite to this channel.
    pub async fn create_invite(
        &self,
        http: &RestClient,
        body: &CreateChannelInvite,
    ) -> Result<Invite, DiscordError> {
        http.create_channel_invite_typed(self.id.clone(), body)
            .await
    }

    /// Returns the `<#id>` mention for this channel.
    pub fn mention(&self) -> String {
        format!("<#{}>", self.id)
    }

    /// Whether messages can be sent in this channel (matches discord.js's
    /// `isTextBased()`: text, DM, group DM, announcement, threads, and
    /// voice/stage channels with text-in-voice).
    pub fn is_text_based(&self) -> bool {
        matches!(
            self.kind,
            CHANNEL_TEXT
                | CHANNEL_DM
                | CHANNEL_VOICE
                | CHANNEL_GROUP_DM
                | CHANNEL_NEWS
                | CHANNEL_ANNOUNCEMENT_THREAD
                | CHANNEL_PUBLIC_THREAD
                | CHANNEL_PRIVATE_THREAD
                | CHANNEL_STAGE_VOICE
        )
    }

    /// Whether this is a voice or stage channel.
    pub fn is_voice_based(&self) -> bool {
        matches!(self.kind, CHANNEL_VOICE | CHANNEL_STAGE_VOICE)
    }

    /// Whether this is an announcement, public, or private thread.
    pub fn is_thread(&self) -> bool {
        matches!(
            self.kind,
            CHANNEL_ANNOUNCEMENT_THREAD | CHANNEL_PUBLIC_THREAD | CHANNEL_PRIVATE_THREAD
        )
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
        guild_id: impl Into<Snowflake>,
        body: &ModifyGuildRole,
    ) -> Result<Role, DiscordError> {
        http.modify_guild_role(guild_id, self.id.clone(), body)
            .await
    }

    /// Deletes this role from `guild_id`.
    pub async fn delete(
        &self,
        http: &RestClient,
        guild_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        http.delete_role(guild_id, self.id.clone()).await
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
        http.create_dm_channel_typed(&CreateDmChannel {
            recipient_id: self.id.clone(),
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
            .map(|hash| cdn_asset_url("avatars", &self.id, hash, size))
    }

    /// Returns the user's default embed avatar URL,
    /// `embed/avatars/{(id >> 22) % 6}.png`. The CDN ignores `size` for
    /// default avatars, so none is appended.
    pub fn default_avatar_url(&self) -> String {
        format!("{CDN_BASE}/embed/avatars/{}.png", default_avatar_index(self))
    }

    /// Returns the avatar shown for this user: their custom avatar, or the
    /// default embed avatar when they have none.
    pub fn display_avatar_url(&self, size: Option<u16>) -> String {
        self.avatar_url(size)
            .unwrap_or_else(|| self.default_avatar_url())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn user(id: u64) -> User {
        User {
            id: Snowflake::from(id),
            username: "tester".to_string(),
            ..User::default()
        }
    }

    // -- Message ----------------------------------------------------------

    #[test]
    fn message_link_uses_guild_id_when_present() {
        let message = Message {
            id: Snowflake::from(3u64),
            channel_id: Snowflake::from(2u64),
            guild_id: Some(Snowflake::from(1u64)),
            ..Message::default()
        };
        assert_eq!(message.link(), "https://discord.com/channels/1/2/3");
    }

    #[test]
    fn message_link_uses_at_me_for_dms() {
        let message = Message {
            id: Snowflake::from(3u64),
            channel_id: Snowflake::from(2u64),
            ..Message::default()
        };
        assert_eq!(message.link(), "https://discord.com/channels/@me/2/3");
    }

    #[test]
    fn reply_body_sets_default_reply_reference() {
        let message = Message {
            id: Snowflake::from(42u64),
            channel_id: Snowflake::from(7u64),
            ..Message::default()
        };
        let body = reply_body(&message, "hi");
        assert_eq!(body.content.as_deref(), Some("hi"));
        let reference = body.message_reference.clone().expect("reply reference");
        assert_eq!(reference.kind, Some(0));
        assert_eq!(reference.message_id, Some(Snowflake::from(42u64)));
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
            start_thread_body("bugs"),
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
        assert_eq!(member_user_id(&member).unwrap(), Snowflake::from(9u64));
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
            id: Snowflake::from(10u64),
            icon: Some("abc".to_string()),
            banner: Some("a_def".to_string()),
            ..Guild::default()
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
            id: Snowflake::from(10u64),
            ..Guild::default()
        };
        assert_eq!(bare.icon_url(None), None);
        assert_eq!(bare.banner_url(None), None);
    }

    // -- Channel ------------------------------------------------------------

    #[test]
    fn channel_mention_and_type_classification() {
        let channel = |kind: u8| Channel {
            id: Snowflake::from(5u64),
            kind,
            ..Channel::default()
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
            id: Snowflake::from(77u64),
            ..Role::default()
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
        // Non-numeric snowflake falls back to index 0.
        let odd = User {
            id: Snowflake::new("not-numeric"),
            username: "x".to_string(),
            ..User::default()
        };
        assert_eq!(default_avatar_index(&odd), 0);
    }
}
