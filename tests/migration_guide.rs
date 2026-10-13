//! The code of the 2.x → 3.0 migration guide
//! (`discordrsdocs/docs/guide/migration-3.0.md`, Korean copy in
//! `discordrsdocs/ko/guide/migration-3.0.md`), compiled against this branch.
//!
//! Every "after" snippet in the guide is a region of this file between
//! `// snippet-start: NAME` and `// snippet-end`, inside a module gated on
//! the features it needs. In the guide, the code block follows an HTML
//! comment `<!-- snippet: NAME -->` and must equal the region (minus the
//! module's indentation); `guide_snippets_match_this_file` checks that both
//! guides contain exactly the regions of this file, so neither side can
//! drift without failing CI. "Before" blocks are marked
//! `<!-- snippet: 2.x -->`, start with a comment naming the version they
//! show (`// 2.x`, `// 2.0.2`, or `// 2.2.0`), and are not compiled,
//! because they do not build against 3.0.
//!
//! Snippets that need a live Discord connection (REST calls, a running
//! gateway client) are only compiled, like a doctest's `no_run`; the others
//! are also called by the tests next to them, which assert what the guide
//! says they do.

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation fails the test"
)]
#![allow(
    dead_code,
    reason = "no_run snippets (REST calls, gateway handlers) are compiled but never called"
)]

mod ids {
    // snippet-start: ids
    use discordrs::{ChannelId, DiscordError, GuildId, RoleId, UserId};

    // Untrusted text (config, a string command option) is parsed once, at the
    // boundary. "0", " 1", "+1", and "1.0" are rejected.
    fn parse_ids(guild: &str, channel: u64) -> Result<(GuildId, ChannelId), DiscordError> {
        let guild_id: GuildId = guild.parse()?;
        let channel_id = ChannelId::try_from(channel)?;
        Ok((guild_id, channel_id))
    }

    // IDs are `Copy` and 8 bytes: pass them by value, no `.clone()`.
    fn describe(guild_id: GuildId, user_id: UserId) -> String {
        let everyone: RoleId = guild_id.everyone_role();
        format!(
            "user {user_id} (raw {}) in guild {guild_id}, @everyone = {everyone}",
            user_id.get()
        )
    }
    // snippet-end

    #[test]
    fn parses_and_describes() {
        let (guild_id, channel_id) =
            parse_ids("81384788765712384", 41771983423143937).expect("valid ids");
        assert_eq!(channel_id.get(), 41771983423143937);
        assert!(parse_ids("0", 1).is_err());
        assert!(parse_ids("+1", 1).is_err());
        assert!(parse_ids("1", 0).is_err());
        let user_id = UserId::try_from(80351110224678912).expect("valid");
        assert_eq!(
            describe(guild_id, user_id),
            "user 80351110224678912 (raw 80351110224678912) in guild 81384788765712384, \
             @everyone = 81384788765712384"
        );
    }
}

mod polymorphic_ids {
    // snippet-start: polymorphic_ids
    use discordrs::model::{OverwriteTarget, PermissionOverwrite};
    use discordrs::DiscordError;

    fn overwrite_owner(json: &str) -> Result<String, DiscordError> {
        let overwrite: PermissionOverwrite = serde_json::from_str(json)?;
        Ok(match overwrite.target {
            OverwriteTarget::Role(role_id) => format!("role {role_id}"),
            OverwriteTarget::Member(user_id) => format!("member {user_id}"),
            OverwriteTarget::Unknown { kind, id } => format!("type {kind:?} for {id}"),
        })
    }
    // snippet-end

    #[test]
    fn reads_the_target() {
        assert_eq!(
            overwrite_owner(r#"{"id": "80351110224678912", "type": 1, "deny": "2048"}"#)
                .expect("decodes"),
            "member 80351110224678912"
        );
        assert_eq!(
            overwrite_owner(r#"{"id": "41771983423143937", "type": 0}"#).expect("decodes"),
            "role 41771983423143937"
        );
    }
}

mod discriminants {
    // snippet-start: discriminants
    use discordrs::{Channel, ChannelType, MessageType};

    fn describe(channel: &Channel) -> &'static str {
        // Constants work as patterns, but the set is open: keep a `_` arm.
        match channel.kind {
            ChannelType::GUILD_TEXT | ChannelType::GUILD_ANNOUNCEMENT => "text",
            kind if kind.is_thread() => "thread",
            kind if kind.is_voice_based() => "voice",
            _ => "other",
        }
    }

    fn raw_numbers() {
        let kind = ChannelType::new(0); // was `0u8` or `ChannelType::Text as u8`
        assert_eq!(kind, ChannelType::GUILD_TEXT);
        assert_eq!(kind.get(), 0);
        // A value this version does not know is kept, not rejected.
        let future = MessageType::new(250);
        assert_eq!(format!("{future:?}"), "Unknown(250)");
    }
    // snippet-end

    #[test]
    fn classifies_channels() {
        let channel: Channel =
            serde_json::from_str(r#"{"id": "1", "type": 11}"#).expect("a public thread");
        assert_eq!(describe(&channel), "thread");
        let channel: Channel =
            serde_json::from_str(r#"{"id": "1", "type": 0}"#).expect("a text channel");
        assert_eq!(describe(&channel), "text");
        raw_numbers();
    }
}

mod patch_request {
    // snippet-start: patch_request
    use discordrs::request::ModifyChannel;

    fn rename_and_clear_topic() -> Result<String, serde_json::Error> {
        // Omitted fields are not sent; `clear_*` sends an explicit `null`.
        let body = ModifyChannel::new().name("announcements").clear_topic();
        serde_json::to_string(&body) // {"name":"announcements","topic":null}
    }
    // snippet-end

    #[test]
    fn sends_null_only_for_cleared_fields() {
        assert_eq!(
            rename_and_clear_topic().expect("encodes"),
            r#"{"name":"announcements","topic":null}"#
        );
    }
}

mod patch_event {
    // snippet-start: patch_event
    use discordrs::model::MessageUpdate;
    use discordrs::{Message, Nullable, Patch};

    fn apply_edit(message: &mut Message, update: &MessageUpdate) {
        match &update.content {
            Patch::Missing => {} // the edit did not touch the content
            Patch::Value(content) => println!("new content: {content}"),
        }
        if let Patch::Value(Nullable::Null) = &update.edited_timestamp {
            println!("edited_timestamp was cleared");
        }
        // Applies every sent field; omitted ones keep their value.
        update.apply_to(message);
    }
    // snippet-end

    #[test]
    fn omitted_fields_keep_their_value() {
        let mut message: Message = serde_json::from_value(serde_json::json!({
            "id": "2",
            "channel_id": "3",
            "author": {"id": "4", "username": "a"},
            "content": "old",
            "pinned": false,
        }))
        .expect("a message");
        let update: MessageUpdate =
            serde_json::from_str(r#"{"id": "2", "channel_id": "3", "content": "new"}"#)
                .expect("a partial update");
        apply_edit(&mut message, &update);
        assert_eq!(message.content, "new");
        assert_eq!(message.pinned, Some(false));
    }
}

mod permission_sets {
    // snippet-start: permissions
    use discordrs::bitfield::{message_flags, permissions};
    use discordrs::{MessageFlags, Permissions};

    fn permission_sets() -> Result<(), serde_json::Error> {
        let mut perms: Permissions = permissions::VIEW_CHANNEL | permissions::SEND_MESSAGES;
        perms |= permissions::EMBED_LINKS; // 2.x: `perms.insert(..)`
        perms = perms - permissions::SEND_MESSAGES; // `remove` returns the new set
        assert!(perms.contains(permissions::VIEW_CHANNEL.bits()));
        // Permissions are decimal strings on the wire; the other bitfields
        // (message, channel, and user flags, intents) are numbers.
        assert_eq!(serde_json::to_string(&perms)?, r#""17408""#);
        let flags: MessageFlags = message_flags::SUPPRESS_NOTIFICATIONS;
        assert_eq!(serde_json::to_string(&flags)?, "4096"); // 2.x sent bit 10
        Ok(())
    }
    // snippet-end

    #[test]
    fn sets_and_wire_shapes() {
        permission_sets().expect("encodes");
    }
}

mod custom_bitfield {
    // snippet-start: custom_bitfield
    use discordrs::bitfield::{BitField, BitFieldFlags};

    #[derive(Copy, Clone, Debug, PartialEq, Eq)]
    struct FeatureFlags;

    impl BitFieldFlags for FeatureFlags {
        const BITS: &'static [(u64, &'static str)] = &[(1 << 0, "BETA"), (1 << 1, "STAFF")];
        // 3.0 writes custom sets as numbers (2.x: strings); this keeps strings.
        const STRING_ENCODED: bool = true;
    }

    fn encode_features() -> Result<String, serde_json::Error> {
        serde_json::to_string(&BitField::<FeatureFlags>::from_bits(0b11)) // "\"3\""
    }
    // snippet-end

    #[test]
    fn keeps_the_string_shape() {
        assert_eq!(encode_features().expect("encodes"), r#""3""#);
    }
}

#[cfg(feature = "gateway")]
mod events {
    // snippet-start: events_handle_event
    use std::sync::Arc;

    use discordrs::{Context, Event, EventHandler, EventKind};

    struct Router;

    impl EventHandler for Router {
        async fn handle_event(&self, _ctx: Context, event: Arc<Event>) {
            let (shard_id, _shard_count) = event.metadata().shard();
            match event.kind() {
                EventKind::Ready(ready) => {
                    println!("shard {shard_id} ready as {}", ready.user.username);
                }
                // The `Arc<Message>` the cache and collectors share: clone the
                // `Arc` (not the message) to keep it beyond this call.
                EventKind::MessageCreate(message) => println!("{}", message.content),
                EventKind::Unknown => println!("dispatch without a type: {}", event.name()),
                EventKind::DecodeFailed(error) => eprintln!("{}: {error}", event.name()),
                _ => {}
            }
        }
    }
    // snippet-end

    #[test]
    fn handler_satisfies_the_builder_bounds() {
        let _client = discordrs::Client::builder("token", discordrs::gateway_intents::GUILDS)
            .event_handler(Router);
    }
}

#[cfg(feature = "gateway")]
mod event_hooks {
    // snippet-start: events_hooks
    use std::sync::Arc;

    use discordrs::model::MessageUpdate;
    use discordrs::{
        gateway_intents, Client, Context, DiscordError, EventDecodeError, EventHandler, GuildId,
        Member, Message,
    };
    use serde_json::value::RawValue;

    struct Hooks;

    impl EventHandler for Hooks {
        async fn message_create(&self, _ctx: Context, message: Arc<Message>) {
            println!("{}: {}", message.channel_id, message.content);
        }

        // A partial message: fields Discord did not send are `Patch::Missing`.
        async fn message_update(&self, _ctx: Context, update: Arc<MessageUpdate>) {
            println!("message {} in {} edited", update.id, update.channel_id);
        }

        async fn member_add(&self, _ctx: Context, guild_id: GuildId, member: Arc<Member>) {
            if let Some(user) = &member.user {
                println!("{} joined {guild_id}", user.username);
            }
        }

        // Dispatches this version has no type for: the name and the JSON text.
        async fn raw_event(&self, _ctx: Context, name: &str, data: &RawValue) {
            println!("{name}: {} bytes", data.get().len());
        }

        // New: a known dispatch whose payload did not match its type.
        async fn decode_failed(
            &self,
            _ctx: Context,
            name: &str,
            error: &EventDecodeError,
            _data: &RawValue,
        ) {
            eprintln!("could not decode {name}: {error}");
        }
    }

    async fn run(token: &str) -> Result<(), DiscordError> {
        Client::builder(
            token,
            gateway_intents::GUILDS | gateway_intents::GUILD_MESSAGES,
        )
        .event_handler(Hooks)
        // Keep every dispatch's JSON for `event.raw()` (default off; unknown
        // and undecodable dispatches always keep theirs).
        .keep_raw_events(true)
        .start()
        .await
    }
    // snippet-end

    #[test]
    fn handler_satisfies_the_builder_bounds() {
        let _client = Client::builder("token", gateway_intents::GUILDS).event_handler(Hooks);
    }
}

mod events_in_tests {
    // snippet-start: events_decode
    use discordrs::{decode_event, DiscordError, EventKind};

    // Feed a handler or a match from a fixture, without a gateway connection.
    fn content_of(fixture: &str) -> Result<Option<String>, DiscordError> {
        let event = decode_event("MESSAGE_CREATE", fixture)?;
        Ok(match event.kind() {
            EventKind::MessageCreate(message) => Some(message.content.clone()),
            _ => None,
        })
    }
    // snippet-end

    #[test]
    fn decodes_a_fixture() {
        let fixture = r#"{"id": "2", "channel_id": "3", "author": {"id": "4", "username": "a"},
            "content": "hi", "timestamp": "2026-10-13T00:00:00+00:00"}"#;
        assert_eq!(content_of(fixture).expect("decodes").as_deref(), Some("hi"));
    }
}

#[cfg(feature = "collectors")]
mod collectors {
    // snippet-start: collectors
    use std::sync::Arc;
    use std::time::Duration;

    use discordrs::{Context, EventHandler, EventKind, Message};

    struct Quiz;

    impl EventHandler for Quiz {
        async fn message_create(&self, ctx: Context, message: Arc<Message>) {
            if message.content != "!quiz" {
                return;
            }
            let channel_id = message.channel_id;
            // Starts at the event being handled: messages sent while this
            // handler call was queued are replayed, not missed.
            let mut answers = ctx
                .message_collector()
                .filter(move |reply: &Message| reply.channel_id == channel_id)
                .timeout(Duration::from_secs(30));
            // `next()` copies the matching message out; `next_event()` hands
            // over the shared `Arc<Event>` without copying.
            if let Some(event) = answers.next_event().await {
                if let EventKind::MessageCreate(answer) = event.kind() {
                    println!("first answer: {}", answer.content);
                }
            }
            // Edits are partial `MessageUpdate`s with their own collector.
            let mut edits = ctx
                .message_update_collector()
                .timeout(Duration::from_secs(30));
            if let Some(update) = edits.next().await {
                println!("message {} was edited", update.id);
            }
        }
    }
    // snippet-end

    #[test]
    fn handler_satisfies_the_builder_bounds() {
        let _client = discordrs::Client::builder("token", discordrs::gateway_intents::GUILDS)
            .event_handler(Quiz);
    }
}

#[cfg(feature = "cache")]
mod cache {
    // snippet-start: cache
    use std::sync::Arc;

    use discordrs::model::MessageUpdate;
    use discordrs::{CacheHandle, Message};

    async fn track_edit(cache: &CacheHandle, message: Arc<Message>, update: &MessageUpdate) {
        // Upserts take `impl Into<Arc<T>>`: an event's `Arc` is stored, not
        // copied. A plain `Message` still works.
        cache.upsert_message(Arc::clone(&message)).await;
        // MESSAGE_UPDATE is a patch merged into the cached copy. The merge is
        // copy-on-write: `message` keeps showing the content it had.
        if let Some(merged) = cache.apply_message_update(update).await {
            println!("{:?} -> {:?}", message.content, merged.content);
        }
        // Keys are typed IDs, passed by value.
        let cached = cache.message_arc(message.channel_id, message.id).await;
        assert!(cached.is_some());
    }
    // snippet-end

    #[tokio::test]
    async fn merges_an_edit() {
        let cache = CacheHandle::new();
        let message: Message = serde_json::from_value(serde_json::json!({
            "id": "2", "channel_id": "3", "author": {"id": "4", "username": "a"},
            "content": "old",
        }))
        .expect("a message");
        let message = Arc::new(message);
        let update: MessageUpdate =
            serde_json::from_str(r#"{"id": "2", "channel_id": "3", "content": "new"}"#)
                .expect("a partial update");
        track_edit(&cache, Arc::clone(&message), &update).await;
        assert_eq!(message.content, "old");
        let cached = cache
            .message_arc(message.channel_id, message.id)
            .await
            .expect("cached");
        assert_eq!(cached.content, "new");
    }
}

#[cfg(feature = "cache")]
mod cache_backend {
    // snippet-start: cache_backend
    use std::sync::Arc;

    use discordrs::cache::{CacheBackend, CacheClear, CacheClearId};
    use discordrs::model::Presence;
    use discordrs::{
        CacheHandle, ChannelId, DiscordError, GuildId, Member, Message, MessageId, UserId,
    };

    /// Logs every write and stores it in an inner `CacheHandle`, which
    /// implements the epoch contract. A Redis backend makes the same calls
    /// atomic in a script.
    struct LoggingBackend {
        inner: CacheHandle,
    }

    // No `#[async_trait]`: plain `async fn`, whose futures must be `Send`.
    impl CacheBackend for LoggingBackend {
        async fn clear_cache(
            &self,
            expected_epoch: u64,
            clear_id: CacheClearId,
        ) -> Result<CacheClear, DiscordError> {
            println!("clear all at epoch {expected_epoch}");
            self.inner.clear_cache(expected_epoch, clear_id).await
        }

        async fn clear_shard_cache(
            &self,
            expected_epoch: u64,
            clear_id: CacheClearId,
            shard_id: u32,
            shard_count: u32,
        ) -> Result<CacheClear, DiscordError> {
            println!("clear shard {shard_id}/{shard_count} at epoch {expected_epoch}");
            self.inner
                .clear_shard_cache(expected_epoch, clear_id, shard_id, shard_count)
                .await
        }

        async fn purge_expired_entries(&self) -> Result<(), DiscordError> {
            self.inner.purge_expired_entries().await
        }

        // Every write carries the epoch it was issued under; a write older than
        // the store's epoch must be ignored and still return `Ok(())`.
        async fn put_member(
            &self,
            epoch: u64,
            guild_id: GuildId,
            user_id: UserId,
            member: Arc<Member>,
        ) -> Result<(), DiscordError> {
            self.inner
                .put_member(epoch, guild_id, user_id, member)
                .await
        }

        async fn get_member(
            &self,
            guild_id: GuildId,
            user_id: UserId,
        ) -> Result<Option<Arc<Member>>, DiscordError> {
            self.inner.get_member(guild_id, user_id).await
        }

        async fn list_members(&self, guild_id: GuildId) -> Result<Vec<Arc<Member>>, DiscordError> {
            self.inner.list_members(guild_id).await
        }

        async fn delete_member(
            &self,
            epoch: u64,
            guild_id: GuildId,
            user_id: UserId,
        ) -> Result<(), DiscordError> {
            self.inner.delete_member(epoch, guild_id, user_id).await
        }

        async fn put_message(&self, epoch: u64, message: Arc<Message>) -> Result<(), DiscordError> {
            println!("store message {} at epoch {epoch}", message.id);
            self.inner.put_message(epoch, message).await
        }

        async fn get_message(
            &self,
            channel_id: ChannelId,
            message_id: MessageId,
        ) -> Result<Option<Arc<Message>>, DiscordError> {
            self.inner.get_message(channel_id, message_id).await
        }

        async fn list_messages(
            &self,
            channel_id: ChannelId,
        ) -> Result<Vec<Arc<Message>>, DiscordError> {
            self.inner.list_messages(channel_id).await
        }

        async fn delete_message(
            &self,
            epoch: u64,
            channel_id: ChannelId,
            message_id: MessageId,
        ) -> Result<(), DiscordError> {
            self.inner
                .delete_message(epoch, channel_id, message_id)
                .await
        }

        async fn put_presence(
            &self,
            epoch: u64,
            guild_id: GuildId,
            user_id: UserId,
            presence: Arc<Presence>,
        ) -> Result<(), DiscordError> {
            self.inner
                .put_presence(epoch, guild_id, user_id, presence)
                .await
        }

        async fn get_presence(
            &self,
            guild_id: GuildId,
            user_id: UserId,
        ) -> Result<Option<Arc<Presence>>, DiscordError> {
            self.inner.get_presence(guild_id, user_id).await
        }

        async fn list_presences(
            &self,
            guild_id: GuildId,
        ) -> Result<Vec<Arc<Presence>>, DiscordError> {
            self.inner.list_presences(guild_id).await
        }

        async fn delete_presence(
            &self,
            epoch: u64,
            guild_id: GuildId,
            user_id: UserId,
        ) -> Result<(), DiscordError> {
            self.inner.delete_presence(epoch, guild_id, user_id).await
        }
    }
    // snippet-end

    #[tokio::test]
    async fn stale_writes_are_ignored() {
        let backend = LoggingBackend {
            inner: CacheHandle::new(),
        };
        let clear_id = CacheClearId {
            client: 1,
            sequence: 1,
        };
        assert_eq!(
            backend.clear_cache(0, clear_id).await.expect("clears"),
            CacheClear::Applied(1)
        );
        let message: Message = serde_json::from_value(serde_json::json!({
            "id": "2", "channel_id": "3", "content": "hi",
        }))
        .expect("a message");
        let (channel_id, message_id) = (message.channel_id, message.id);
        backend
            .put_message(0, Arc::new(message))
            .await
            .expect("a stale write is not an error");
        assert!(backend
            .get_message(channel_id, message_id)
            .await
            .expect("reads")
            .is_none());
    }
}

#[cfg(all(feature = "gateway", feature = "cache"))]
mod no_dyn_traits {
    // snippet-start: no_dyn
    use std::sync::Arc;

    use discordrs::cache::CacheBackend;
    use discordrs::{gateway_intents, Client, ClientBuilder};

    // 2.x took `Arc<dyn CacheBackend>`. The 3.0 traits are not object safe:
    // name the concrete backend, or be generic over it.
    fn with_backend<B: CacheBackend + 'static>(token: &str, backend: Arc<B>) -> ClientBuilder {
        Client::builder(token, gateway_intents::GUILD_MESSAGES).cache_backend(backend)
    }
    // snippet-end

    #[test]
    fn takes_a_concrete_backend() {
        let _builder = with_backend("token", Arc::new(discordrs::CacheHandle::new()));
    }
}

mod rest {
    // snippet-start: rest
    use discordrs::request::{CreateMessage, ModifyChannel};
    use discordrs::{ChannelId, DiscordError, GuildId, RawRequest, RestClient};
    use serde_json::Value;

    async fn rest_calls(
        rest: &RestClient,
        guild_id: GuildId,
        channel_id: ChannelId,
    ) -> Result<(), DiscordError> {
        // One method per route: a concrete request type in, a typed model out.
        let channel = rest
            .update_channel(channel_id, &ModifyChannel::new().name("general"))
            .await?;
        let message = rest
            .create_message(channel.id, &CreateMessage::new().content("hello"))
            .await?;
        let emojis = rest.get_guild_emojis(guild_id).await?; // was `get_guild_emojis_typed`
        println!("{} in {}; {} emojis", message.id, channel.id, emojis.len());
        // A route without a method: same transport, rate limits, and retries.
        let preview: Value = rest
            .request(RawRequest::get(format!("/guilds/{guild_id}/preview")))
            .await?;
        println!("{preview}");
        Ok(())
    }
    // snippet-end
}

mod builders {
    // snippet-start: builders
    use discordrs::request::CreateMessage;
    use discordrs::{
        ActionRowBuilder, ButtonStyle, ComponentsV2Message, ContainerBuilder, CustomButtonBuilder,
        DiscordError, LinkButtonBuilder, MessageBuilder, RoleId, SectionBuilder, SelectMenuBuilder,
        SelectOption, TextDisplayBuilder, ThumbnailBuilder,
    };

    fn support_panel(moderators: RoleId) -> Result<CreateMessage, DiscordError> {
        // One builder per button kind: a link button has no `custom_id` setter.
        let buttons = ActionRowBuilder::new()
            .add_button(
                CustomButtonBuilder::new("ticket:open")
                    .label("Open")
                    .style(ButtonStyle::PRIMARY),
            )
            .add_button(LinkButtonBuilder::new("https://example.com/help").label("Help"));
        // The required accessory is part of the type: without
        // `set_*_accessory`, `add_section` does not compile.
        let header = SectionBuilder::new()
            .add_text_display(TextDisplayBuilder::new("## Support"))
            .set_thumbnail_accessory(ThumbnailBuilder::new("https://example.com/logo.png"));
        let container = ContainerBuilder::new()
            .add_section(header)
            .add_action_row(buttons)
            // A select builder is its own row, with only its kind's setters.
            .add_action_row(
                SelectMenuBuilder::string("ticket:topic")
                    .add_option(SelectOption::new("Billing", "billing")),
            )
            .add_action_row(SelectMenuBuilder::role("ticket:notify").default_role(moderators));
        // `build()` returns the typed `request::CreateMessage`, not a `Value`.
        MessageBuilder::new()
            .components_v2(ComponentsV2Message::new().add_container(container))
            .try_build()
    }
    // snippet-end

    #[test]
    fn builds_a_valid_components_v2_message() {
        let moderators = RoleId::try_from(701).expect("valid");
        let message = support_panel(moderators).expect("valid message");
        let json = serde_json::to_string(&message).expect("encodes");
        assert!(json.contains(r#""flags":32768"#), "{json}");
        assert!(
            json.contains(r#""default_values":[{"id":"701","type":"role"}]"#),
            "{json}"
        );
    }
}

mod interaction_decoding {
    // snippet-start: interactions_decode
    use discordrs::{decode_interaction, DiscordError, Interaction};

    // The HTTP endpoint and the gateway decode the same way: from text, with
    // no `serde_json::Value` tree.
    fn amount_option(body: &str) -> Result<Option<i64>, DiscordError> {
        let Interaction::ChatInputCommand(command) = decode_interaction(body)? else {
            return Ok(None);
        };
        Ok(command
            .data
            .options
            .iter()
            .find(|option| option.name == "amount")
            // `CommandOptionValue`: `as_str`, `as_i64`, `as_f64`, `as_bool`, `as_id`.
            .and_then(|option| option.value.as_ref()?.as_i64()))
    }
    // snippet-end

    #[test]
    fn reads_a_typed_option() {
        let body = r#"{"id": "1", "application_id": "2", "type": 2, "token": "t",
            "data": {"id": "3", "name": "pay", "type": 1,
                     "options": [{"name": "amount", "type": 4, "value": 25}]}}"#;
        assert_eq!(amount_option(body).expect("decodes"), Some(25));
    }
}

mod modal_values {
    // snippet-start: modal_values
    use discordrs::{decode_interaction, DiscordError, Interaction};

    fn ticket_form(body: &str) -> Result<Option<(String, bool)>, DiscordError> {
        let Interaction::ModalSubmit(submit) = decode_interaction(body)? else {
            return Ok(None);
        };
        let form = &submit.submission;
        // Checkboxes are booleans now (2.x failed to parse them).
        let subject = form.get_text("subject").unwrap_or_default().to_owned();
        let urgent = form.get_checkbox("urgent").unwrap_or(false);
        Ok(Some((subject, urgent)))
    }
    // snippet-end

    #[test]
    fn reads_label_wrapped_values() {
        let body = r#"{"id": "1", "application_id": "2", "type": 5, "token": "t",
            "data": {"custom_id": "ticket:create", "components": [
                {"type": 18, "id": 1, "component": {"type": 4, "id": 2, "custom_id": "subject", "value": "Printer"}},
                {"type": 18, "id": 3, "component": {"type": 23, "id": 4, "custom_id": "urgent", "value": true}}
            ]}}"#;
        assert_eq!(
            ticket_form(body).expect("decodes"),
            Some(("Printer".to_owned(), true))
        );
    }
}

#[cfg(feature = "interactions")]
mod interaction_endpoint {
    // snippet-start: interactions_endpoint
    use discordrs::request::InteractionMessage;
    use discordrs::{
        EndpointReply, Interaction, InteractionContextData, InteractionResponder,
        InteractionResponse, ModalBuilder, RestClient, TextInputBuilder, TextInputStyle,
        TypedInteractionHandler,
    };

    #[derive(Clone)]
    struct Tickets {
        rest: RestClient,
    }

    fn ephemeral(text: &str) -> EndpointReply {
        InteractionResponse::reply(InteractionMessage::new().content(text).ephemeral(true)).into()
    }

    impl TypedInteractionHandler for Tickets {
        async fn handle_typed(
            &self,
            _ctx: InteractionContextData,
            interaction: Interaction,
        ) -> EndpointReply {
            match interaction {
                Interaction::ChatInputCommand(command)
                    if command.data.name.as_deref() == Some("ticket") =>
                {
                    // Text inputs are wrapped in a Label (Discord's current layout).
                    let modal = ModalBuilder::new("ticket:create", "New ticket").add_text_input(
                        TextInputBuilder::new("subject", "Subject", TextInputStyle::SHORT),
                    );
                    InteractionResponse::modal(modal).into()
                }
                Interaction::ChatInputCommand(command)
                    if command.data.name.as_deref() == Some("status") =>
                {
                    // `Accepted` (202, no body) only after responding over REST.
                    match command.reply_ephemeral(&self.rest, "All systems go").await {
                        Ok(()) => EndpointReply::Accepted,
                        // The endpoint sends this only if no REST response may
                        // have reached Discord; otherwise it answers 202.
                        Err(_) => ephemeral("Status is unavailable right now"),
                    }
                }
                Interaction::ModalSubmit(_) => ephemeral("Ticket created"),
                // Autocomplete must be answered with choices, not a message.
                Interaction::Autocomplete(_) => InteractionResponse::autocomplete([]).into(),
                // Everything else is answered inline too: `Accepted` without a
                // REST response would leave the interaction unanswered.
                _ => ephemeral("Unknown command"),
            }
        }
    }
    // snippet-end

    #[test]
    fn builds_a_router() {
        // The Ed25519 base point: a valid public key.
        let key = "5866666666666666666666666666666666666666666666666666666666666666";
        let tickets = Tickets {
            rest: RestClient::new("token", None),
        };
        let _router = discordrs::try_typed_interactions_endpoint(key, tickets).expect("valid key");
    }
}

#[cfg(feature = "interactions")]
mod framework {
    // snippet-start: framework
    use discordrs::request::InteractionMessage;
    use discordrs::{AppContext, AppFramework, EndpointReply, InteractionResponse};

    fn app() -> AppFramework {
        AppFramework::builder()
            // Guards return `Result<(), EndpointReply>` (2.x: `InteractionResponse`).
            .guard(|ctx: &AppContext| {
                if ctx.context.guild_id.is_some() {
                    return Ok(());
                }
                let message = InteractionMessage::new()
                    .content("Use this command in a server")
                    .ephemeral(true);
                Err(EndpointReply::from(InteractionResponse::reply(message)))
            })
            // Routes return anything that converts into an `EndpointReply`.
            .command("hello", |_ctx: AppContext| async move {
                InteractionResponse::reply("Hello from discord.rs")
            })
            .build()
    }
    // snippet-end

    #[test]
    fn builds() {
        let _app = app();
    }
}

mod command_choices {
    // snippet-start: command_choices
    use discordrs::{
        CommandDefinition, CommandOptionBuilder, CommandOptionType, DiscordError,
        SlashCommandBuilder,
    };

    fn pay_command() -> Result<CommandDefinition, DiscordError> {
        let amount = CommandOptionBuilder::new(CommandOptionType::INTEGER, "amount", "How much")
            // `impl Into<CommandOptionValue>`: strings, i8-i64, u8-u32, floats.
            .choice("ten", 10)
            // Any other `Serialize` value (here a `u64`) goes through `try_choice`.
            .try_choice("most", 9_007_199_254_740_991_u64)?;
        // `try_build()` now also checks names (lowercase, no punctuation),
        // locale keys, and each choice against the option type and +/-2^53.
        SlashCommandBuilder::new("pay", "Send coins")
            .option(amount)
            .try_build()
    }
    // snippet-end

    #[test]
    fn validates_names_and_choices() {
        pay_command().expect("a valid command");
        assert!(SlashCommandBuilder::new("Pay", "Send coins")
            .validate()
            .is_err());
        let too_big = CommandOptionBuilder::new(CommandOptionType::INTEGER, "amount", "How much")
            .try_choice("too big", 9_007_199_254_740_992_u64)
            .expect("encodes");
        assert!(SlashCommandBuilder::new("pay", "Send coins")
            .option(too_big)
            .validate()
            .is_err());
    }
}

#[cfg(feature = "voice")]
mod voice_sink {
    // snippet-start: voice_sink
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

    use discordrs::voice::player::{VoiceFrameSink, VoiceFrameTiming, VoiceSinkFuture};
    use discordrs::DiscordError;

    /// A test double that records how much audio was sent and skipped.
    #[derive(Default)]
    struct RecordingSink {
        sent_us: AtomicU64,
        skipped_us: AtomicU64,
    }

    fn micros(duration: Duration) -> u64 {
        u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
    }

    impl VoiceFrameSink for RecordingSink {
        fn send_opus_packet<'a>(
            &'a self,
            _opus_frame: &'a [u8],
            timing: VoiceFrameTiming,
        ) -> VoiceSinkFuture<'a> {
            Box::pin(async move {
                // `gap`: media never transmitted before this frame (dropped,
                // late, or cancelled). A real sink advances its RTP timestamp
                // past it.
                self.skipped_us
                    .fetch_add(micros(timing.gap), Ordering::Relaxed);
                // Transmit only when the future completes: the player may drop
                // it (a send stalled for 60 ms, stop, pause, or replacement).
                self.sent_us
                    .fetch_add(micros(timing.duration), Ordering::Relaxed);
                Ok(())
            })
        }

        fn update_speaking(&self, _speaking: bool) -> Result<(), DiscordError> {
            Ok(())
        }
    }
    // snippet-end

    #[tokio::test]
    async fn records_timing() {
        let sink = RecordingSink::default();
        let frame = Duration::from_millis(20);
        sink.send_opus_packet(&[0xF8], VoiceFrameTiming::contiguous(frame))
            .await
            .expect("sends");
        assert_eq!(sink.sent_us.load(Ordering::Relaxed), 20_000);
        assert_eq!(sink.skipped_us.load(Ordering::Relaxed), 0);
    }
}

#[cfg(feature = "sharding")]
mod sharding_events {
    // snippet-start: sharding_lagged
    use discordrs::{DiscordError, ShardSupervisorEvent, ShardingManager};

    fn report(manager: &ShardingManager) -> Result<(), DiscordError> {
        for event in manager.drain_events()? {
            match event {
                ShardSupervisorEvent::StateChanged { shard_id, state } => {
                    println!("shard {shard_id}: {state:?}");
                }
                // New in 3.0: at most 256 events wait per shard; older ones are
                // dropped and counted here. `statuses()` is still current.
                ShardSupervisorEvent::Lagged { shard_id, missed } => {
                    eprintln!("shard {shard_id}: {missed} events dropped");
                }
                other => println!("{other:?}"),
            }
        }
        Ok(())
    }
    // snippet-end
}

#[cfg(feature = "sharding")]
mod process_child {
    // snippet-start: process_child
    use discordrs::sharding::process::ShardChildProcess;
    use discordrs::{
        gateway_intents, Client, Context, DiscordError, EventHandler, GatewayConnectionConfig,
        ReadyPayload,
    };
    use serde_json::json;

    struct Handler {
        child: ShardChildProcess,
    }

    impl EventHandler for Handler {
        async fn ready(&self, ctx: Context, _ready: &ReadyPayload) {
            // Per shard, from READY (2.2.0: one `notify_ready()` for all).
            let (shard_id, _) = ctx.shard_pair();
            if let Err(error) = self.child.notify_shard_ready(shard_id) {
                eprintln!("shard {shard_id}: {error}");
            }
        }
    }

    async fn run_child(child: ShardChildProcess, token: &str) -> Result<(), DiscordError> {
        // `serve` returns a `Result` now; start it before the shards connect.
        child.serve(|_request| async { json!({ "ok": true }) })?;
        // Every fresh IDENTIFY waits for the parent's grant for its bucket.
        let gateway = GatewayConnectionConfig::default().identify_gate(child.identify_gate());
        for &shard_id in child.shard_ids() {
            let client = Client::builder(token, gateway_intents::GUILDS)
                .gateway_config(gateway.clone())
                .event_handler(Handler {
                    child: child.clone(),
                })
                .shard(shard_id, child.shard_count());
            tokio::spawn(async move {
                if let Err(error) = client.start().await {
                    eprintln!("shard {shard_id}: {error}");
                }
            });
        }
        child.shutdown_signal().await;
        Ok(())
    }
    // snippet-end
}

#[cfg(feature = "gateway")]
mod interaction_responder {
    // snippet-start: interactions_responder
    use discordrs::{Context, EventHandler, Interaction, InteractionResponder};

    struct Commands;

    impl EventHandler for Commands {
        async fn interaction_create(&self, ctx: Context, interaction: &Interaction) {
            let Interaction::ChatInputCommand(command) = interaction else {
                return;
            };
            if command.data.name.as_deref() == Some("ping") {
                // Tracks the response state: a second reply fails locally,
                // follow-ups wait until Discord accepted the first response.
                if let Err(error) = command.reply_ephemeral(ctx.rest().as_ref(), "pong").await {
                    eprintln!("could not answer /ping: {error}");
                }
            }
        }
    }
    // snippet-end

    #[test]
    fn handler_satisfies_the_builder_bounds() {
        let _client = discordrs::Client::builder("token", discordrs::gateway_intents::GUILDS)
            .event_handler(Commands);
    }
}

/// Keeps the guides and this file in step: every `rust` block in a guide is
/// either a 2.x block (`<!-- snippet: 2.x -->`, first line `// 2.…`) or a
/// named snippet equal to the region of the same name here, and each guide
/// shows every region exactly once.
mod guide_sync {
    use std::collections::BTreeMap;
    use std::path::Path;

    const THIS_FILE: &str = include_str!("migration_guide.rs");
    const GUIDES: [&str; 2] = [
        "discordrsdocs/docs/guide/migration-3.0.md",
        "discordrsdocs/ko/guide/migration-3.0.md",
    ];
    const START: &str = "// snippet-start: ";
    const END: &str = "// snippet-end";
    const MARKER_OPEN: &str = "<!-- snippet: ";
    const MARKER_CLOSE: &str = " -->";
    const OLD_CODE: &str = "2.x";
    /// Start of the first line of a 2.x block: `// 2.x`, `// 2.0.2`, `// 2.2.0`.
    const OLD_CODE_COMMENT: &str = "// 2.";
    /// The snippet modules' indentation, removed from every region line.
    const INDENT: &str = "    ";
    /// A module-level import inside a snippet module.
    const MODULE_USE: &str = "    use ";

    /// The snippet regions of this file. Each snippet module holds exactly
    /// one region, and every module-level `use` is inside it, so a snippet
    /// compiles on its own (its test code cannot lend it an import).
    fn regions() -> BTreeMap<String, String> {
        let mut regions = BTreeMap::new();
        let mut current: Option<(String, Vec<String>)> = None;
        let mut module_regions = 0;
        for (index, line) in THIS_FILE.lines().enumerate() {
            let trimmed = line.trim_start();
            if line.starts_with("mod ") {
                module_regions = 0;
            }
            if let Some(name) = trimmed.strip_prefix(START) {
                assert!(current.is_none(), "line {}: nested snippet", index + 1);
                module_regions += 1;
                assert_eq!(
                    module_regions,
                    1,
                    "line {}: one snippet per module",
                    index + 1
                );
                current = Some((name.trim().to_owned(), Vec::new()));
            } else if trimmed == END {
                let (name, lines) = current.take().expect("snippet-end without a start");
                let previous = regions.insert(name.clone(), lines.join("\n"));
                assert!(previous.is_none(), "snippet {name} is defined twice");
            } else if let Some((_, lines)) = current.as_mut() {
                let line = if line.is_empty() {
                    line
                } else {
                    line.strip_prefix(INDENT).unwrap_or_else(|| {
                        panic!("line {}: snippet lines are indented once", index + 1)
                    })
                };
                lines.push(line.to_owned());
            } else if module_regions > 0 {
                assert!(
                    !line.starts_with(MODULE_USE),
                    "line {}: a snippet module's imports belong in its snippet",
                    index + 1
                );
            }
        }
        assert!(current.is_none(), "unterminated snippet");
        regions
    }

    /// The named `rust` blocks of a guide, checking the 2.x blocks' form.
    fn guide_snippets(path: &str, text: &str) -> BTreeMap<String, String> {
        let mut snippets = BTreeMap::new();
        let mut lines = text.lines().enumerate().peekable();
        let mut previous_text: Option<&str> = None;
        while let Some((index, line)) = lines.next() {
            let line_no = index + 1;
            if let Some(fence) = line.strip_prefix("```") {
                let lang = fence.trim();
                let mut code = Vec::new();
                for (_, line) in lines.by_ref() {
                    if line.starts_with("```") {
                        break;
                    }
                    code.push(line);
                }
                if lang == "rust" {
                    let marker = previous_text
                        .and_then(|text| text.strip_prefix(MARKER_OPEN))
                        .and_then(|text| text.strip_suffix(MARKER_CLOSE))
                        .unwrap_or_else(|| {
                            panic!("{path}:{line_no}: a rust block needs a snippet marker above it")
                        });
                    if marker == OLD_CODE {
                        assert!(
                            code.first()
                                .is_some_and(|first| first.starts_with(OLD_CODE_COMMENT)),
                            "{path}:{line_no}: a 2.x block starts with a comment naming its \
                             version (`// 2.x`, `// 2.0.2`, `// 2.2.0`)"
                        );
                    } else {
                        let previous = snippets.insert(marker.to_owned(), code.join("\n"));
                        assert!(previous.is_none(), "{path}: snippet {marker} shown twice");
                    }
                }
                previous_text = None;
            } else if !line.trim().is_empty() {
                if line.starts_with(MARKER_OPEN) {
                    let next = lines.peek().map(|(_, line)| *line);
                    assert_eq!(
                        next,
                        Some("```rust"),
                        "{path}:{line_no}: a snippet marker is directly followed by a rust block"
                    );
                }
                previous_text = Some(line);
            }
        }
        snippets
    }

    #[test]
    fn guide_snippets_match_this_file() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        if !root.join("discordrsdocs").is_dir() {
            // The published package excludes the docs site; the check runs in
            // the repository, where CI runs it.
            eprintln!("discordrsdocs/ not found; skipping the guide check");
            return;
        }
        let regions = regions();
        assert!(!regions.is_empty());
        for guide in GUIDES {
            let text = std::fs::read_to_string(root.join(guide))
                .unwrap_or_else(|error| panic!("{guide}: {error}"));
            let snippets = guide_snippets(guide, &text);
            for (name, code) in &regions {
                let shown = snippets
                    .get(name)
                    .unwrap_or_else(|| panic!("{guide}: snippet {name} is not shown"));
                assert_eq!(
                    shown, code,
                    "{guide}: snippet {name} differs from this file"
                );
            }
            for name in snippets.keys() {
                assert!(
                    regions.contains_key(name),
                    "{guide}: snippet {name} has no region in tests/migration_guide.rs"
                );
            }
        }
    }
}
