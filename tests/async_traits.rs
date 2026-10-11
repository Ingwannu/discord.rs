//! The public async traits as user code implements them: from outside the
//! crate, with plain `async fn` and no attribute macro (3.0 dropped
//! async-trait).
//!
//! Each impl below is what an application writes. The file stops compiling
//! if a trait signature drifts from something `async fn` can implement, and
//! the tests run the returned futures on spawned Tokio tasks, which pins the
//! `Send` contract the runtime relies on. The negative side (a non-`Send`
//! future is rejected at the impl) is in `tests/ui/`
//! (`tests/compile_fail.rs`).

#![allow(
    clippy::expect_used,
    clippy::panic,
    reason = "a failed expectation fails the test"
)]

use std::future::Future;

/// Compile-time check that a future can be moved into `tokio::spawn`. Used
/// in every feature configuration: the `cached_manager` module is not gated.
fn spawnable<F: Future + Send + 'static>(future: F) -> F {
    future
}

#[cfg(feature = "gateway")]
mod event_handler {
    use std::sync::{Arc, Mutex};

    use discordrs::{
        decode_event, gateway_intents, Client, Context, DiscordHttpClient, Event, EventHandler,
        Message, ReadyPayload, TypeMap,
    };
    use serde_json::{json, Value};
    use tokio::sync::RwLock;

    use super::spawnable;

    #[derive(Default)]
    struct Recorder {
        hits: Mutex<Vec<String>>,
    }

    impl Recorder {
        fn record(&self, hit: String) {
            self.hits
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(hit);
        }

        fn hits(&self) -> Vec<String> {
            self.hits
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone()
        }
    }

    /// Overrides a few typed hooks and relies on the defaults for the rest,
    /// including the default `handle_event` routing.
    impl EventHandler for Recorder {
        async fn ready(&self, _ctx: Context, ready: ReadyPayload) {
            tokio::task::yield_now().await;
            self.record(format!("ready:{}", ready.session_id));
        }

        async fn message_create(&self, _ctx: Context, message: Message) {
            tokio::task::yield_now().await;
            self.record(format!("message:{}", message.content));
        }

        async fn raw_event(&self, _ctx: Context, event_name: String, _data: Value) {
            self.record(format!("raw:{event_name}"));
        }
    }

    /// Uses every default.
    struct Silent;

    impl EventHandler for Silent {}

    /// Replaces the routing and still reaches a typed hook through `self`.
    struct Router {
        inner: Recorder,
    }

    impl EventHandler for Router {
        async fn handle_event(&self, ctx: Context, event: Event) {
            match event {
                Event::Ready(ready) => self.ready(ctx, ready.data).await,
                _ => self.inner.record("routed:other".to_string()),
            }
        }

        async fn ready(&self, _ctx: Context, ready: ReadyPayload) {
            self.inner
                .record(format!("router-ready:{}", ready.session_id));
        }
    }

    fn context() -> Context {
        Context::new(
            Arc::new(DiscordHttpClient::new("token", None)),
            Arc::new(RwLock::new(TypeMap::new())),
        )
    }

    fn ready_event() -> Event {
        decode_event(
            "READY",
            json!({
                "user": { "id": "1", "username": "bot" },
                "session_id": "session-1",
            }),
        )
        .expect("READY decodes")
    }

    fn message_event() -> Event {
        decode_event(
            "MESSAGE_CREATE",
            json!({
                "id": "10",
                "channel_id": "20",
                "author": { "id": "2", "username": "user" },
                "content": "hello",
                "timestamp": "2026-01-01T00:00:00.000000+00:00",
            }),
        )
        .expect("MESSAGE_CREATE decodes")
    }

    #[tokio::test]
    async fn default_routing_reaches_async_fn_hooks_on_spawned_tasks() {
        let handler = Arc::new(Recorder::default());
        for event in [
            ready_event(),
            message_event(),
            decode_event("SOMETHING_NEW", json!({})).expect("unknown events decode"),
        ] {
            let task_handler = Arc::clone(&handler);
            tokio::spawn(spawnable(async move {
                task_handler.handle_event(context(), event).await;
            }))
            .await
            .expect("handler task completes");
        }
        assert_eq!(
            handler.hits(),
            vec!["ready:session-1", "message:hello", "raw:SOMETHING_NEW"]
        );
    }

    #[tokio::test]
    async fn default_methods_and_overridden_routing_are_send() {
        let silent = Arc::new(Silent);
        tokio::spawn(spawnable(async move {
            silent.handle_event(context(), ready_event()).await;
            let message: Message =
                serde_json::from_value(json!({ "id": "1", "channel_id": "2", "content": "" }))
                    .expect("minimal message");
            silent.message_create(context(), message).await;
        }))
        .await
        .expect("default hooks complete");

        let router = Arc::new(Router {
            inner: Recorder::default(),
        });
        let task_router = Arc::clone(&router);
        tokio::spawn(spawnable(async move {
            task_router.handle_event(context(), ready_event()).await;
            task_router.handle_event(context(), message_event()).await;
        }))
        .await
        .expect("router completes");
        assert_eq!(
            router.inner.hits(),
            vec!["router-ready:session-1", "routed:other"]
        );
    }

    #[test]
    fn client_builder_takes_the_handler_by_value() {
        // Compiles only while `event_handler` accepts any `impl EventHandler`.
        let _builder = Client::builder("token", gateway_intents::GUILDS)
            .event_handler(Recorder::default())
            .event_handler(Silent);
    }
}

#[cfg(feature = "cache")]
mod cache_backend {
    use std::sync::Arc;

    use discordrs::cache::{CacheBackend, CacheClear, CacheClearId};
    use discordrs::model::Presence;
    use discordrs::{
        CacheHandle, ChannelId, DiscordError, GuildId, Member, Message, MessageId, User, UserId,
    };

    use super::spawnable;

    /// A backend written the way an external store would be, forwarding to
    /// the in-memory store. The reference-taking methods check that `async
    /// fn` with borrowed arguments satisfies the trait.
    #[derive(Clone, Default)]
    struct Forwarding {
        store: CacheHandle,
    }

    impl CacheBackend for Forwarding {
        async fn clear_cache(
            &self,
            expected_epoch: u64,
            clear_id: CacheClearId,
        ) -> Result<CacheClear, DiscordError> {
            self.store.clear_cache(expected_epoch, clear_id).await
        }

        async fn clear_shard_cache(
            &self,
            expected_epoch: u64,
            clear_id: CacheClearId,
            shard_id: u32,
            shard_count: u32,
        ) -> Result<CacheClear, DiscordError> {
            self.store
                .clear_shard_cache(expected_epoch, clear_id, shard_id, shard_count)
                .await
        }

        async fn purge_expired_entries(&self) -> Result<(), DiscordError> {
            self.store.purge_expired_entries().await
        }

        async fn put_member(
            &self,
            epoch: u64,
            guild_id: GuildId,
            user_id: UserId,
            member: Member,
        ) -> Result<(), DiscordError> {
            self.store
                .put_member(epoch, guild_id, user_id, member)
                .await
        }

        async fn get_member(
            &self,
            guild_id: GuildId,
            user_id: UserId,
        ) -> Result<Option<Arc<Member>>, DiscordError> {
            self.store.get_member(guild_id, user_id).await
        }

        async fn list_members(&self, guild_id: GuildId) -> Result<Vec<Arc<Member>>, DiscordError> {
            self.store.list_members(guild_id).await
        }

        async fn delete_member(
            &self,
            epoch: u64,
            guild_id: GuildId,
            user_id: UserId,
        ) -> Result<(), DiscordError> {
            self.store.delete_member(epoch, guild_id, user_id).await
        }

        async fn put_message(&self, epoch: u64, message: Message) -> Result<(), DiscordError> {
            self.store.put_message(epoch, message).await
        }

        async fn get_message(
            &self,
            channel_id: ChannelId,
            message_id: MessageId,
        ) -> Result<Option<Arc<Message>>, DiscordError> {
            self.store.get_message(channel_id, message_id).await
        }

        async fn list_messages(
            &self,
            channel_id: ChannelId,
        ) -> Result<Vec<Arc<Message>>, DiscordError> {
            self.store.list_messages(channel_id).await
        }

        async fn delete_message(
            &self,
            epoch: u64,
            channel_id: ChannelId,
            message_id: MessageId,
        ) -> Result<(), DiscordError> {
            self.store
                .delete_message(epoch, channel_id, message_id)
                .await
        }

        async fn put_presence(
            &self,
            epoch: u64,
            guild_id: GuildId,
            user_id: UserId,
            presence: Presence,
        ) -> Result<(), DiscordError> {
            self.store
                .put_presence(epoch, guild_id, user_id, presence)
                .await
        }

        async fn get_presence(
            &self,
            guild_id: GuildId,
            user_id: UserId,
        ) -> Result<Option<Arc<Presence>>, DiscordError> {
            self.store.get_presence(guild_id, user_id).await
        }

        async fn list_presences(
            &self,
            guild_id: GuildId,
        ) -> Result<Vec<Arc<Presence>>, DiscordError> {
            self.store.list_presences(guild_id).await
        }

        async fn delete_presence(
            &self,
            epoch: u64,
            guild_id: GuildId,
            user_id: UserId,
        ) -> Result<(), DiscordError> {
            self.store.delete_presence(epoch, guild_id, user_id).await
        }
    }

    /// Generic code over the trait can spawn its futures.
    async fn round_trip_member<B: CacheBackend + 'static>(backend: Arc<B>) -> Option<Arc<Member>> {
        tokio::spawn(spawnable(async move {
            let clear_id = CacheClearId {
                client: 1,
                sequence: 1,
            };
            let epoch = match backend.clear_cache(0, clear_id).await {
                Ok(CacheClear::Applied(epoch)) => epoch,
                other => panic!("first clear should apply: {other:?}"),
            };
            // Outside the crate, IDs come from checked parsing or
            // conversion; there is no infallible `From<&str>`/`From<u64>`.
            let guild_id: GuildId = "1".parse().expect("valid id");
            let user_id = UserId::try_from(2_u64).expect("valid id");
            let user: User = serde_json::from_value(serde_json::json!({
                "id": user_id,
                "username": "wumpus",
            }))
            .expect("minimal user");
            let member = Member {
                user: Some(user),
                ..Member::default()
            };
            backend
                .put_member(epoch, guild_id, user_id, member)
                .await
                .expect("put_member succeeds");
            backend
                .get_member(guild_id, user_id)
                .await
                .expect("get_member succeeds")
        }))
        .await
        .expect("backend task completes")
    }

    #[tokio::test]
    async fn async_fn_backend_futures_are_send_through_generic_code() {
        let member = round_trip_member(Arc::new(Forwarding::default())).await;
        assert_eq!(
            member.and_then(|member| member.user.as_ref().map(|user| user.username.clone())),
            Some("wumpus".to_string())
        );
    }

    #[cfg(feature = "gateway")]
    #[test]
    fn client_builder_takes_an_arc_of_a_concrete_backend() {
        let backend = Arc::new(Forwarding::default());
        let _builder = discordrs::Client::builder("token", discordrs::gateway_intents::GUILDS)
            .cache_backend(Arc::clone(&backend))
            .cache_backend(Arc::new(CacheHandle::new()));
    }
}

/// `CachedManager` is the same async trait with and without the `gateway`
/// feature, so this runs in every feature configuration.
mod cached_manager {
    use std::collections::HashMap;
    use std::sync::Arc;

    use discordrs::{CachedManager, DiscordError, User, UserId};
    use tokio::sync::RwLock;

    use super::spawnable;

    fn id(value: u64) -> UserId {
        UserId::try_new(value).expect("nonzero test id")
    }

    fn user(value: u64, username: &str) -> User {
        serde_json::from_value(serde_json::json!({ "id": id(value), "username": username }))
            .expect("minimal user")
    }

    /// A user-defined manager with its own store. The trait takes the
    /// model's own ID type (`UserId` for `User`).
    #[derive(Clone, Default)]
    struct Users {
        entries: Arc<RwLock<HashMap<UserId, User>>>,
    }

    impl CachedManager<User> for Users {
        async fn get(&self, id: UserId) -> Result<User, DiscordError> {
            self.cached(id)
                .await
                .ok_or_else(|| DiscordError::model(format!("no user {id}")))
        }

        async fn cached(&self, id: UserId) -> Option<User> {
            self.entries.read().await.get(&id).cloned()
        }

        async fn contains(&self, id: UserId) -> bool {
            self.entries.read().await.contains_key(&id)
        }

        async fn list_cached(&self) -> Vec<User> {
            self.entries.read().await.values().cloned().collect()
        }
    }

    fn usernames(users: Vec<User>) -> Vec<String> {
        users.into_iter().map(|user| user.username).collect()
    }

    #[tokio::test]
    async fn async_fn_manager_futures_are_send() {
        let users = Users::default();
        users.entries.write().await.insert(id(7), user(7, "seven"));
        let found = tokio::spawn(spawnable({
            let users = users.clone();
            async move {
                (
                    users.get(id(7)).await,
                    users.contains(id(8)).await,
                    users.list_cached().await,
                )
            }
        }))
        .await
        .expect("manager task completes");
        assert_eq!(
            found.0.ok().map(|user| user.username),
            Some("seven".to_string())
        );
        assert!(!found.1);
        assert_eq!(usernames(found.2), vec!["seven".to_string()]);
        assert!(users.get(id(8)).await.is_err());
    }
}

#[cfg(feature = "interactions")]
mod interaction_handlers {
    use discordrs::{
        parse_interaction, parse_interaction_context, parse_raw_interaction,
        try_interactions_endpoint, try_typed_interactions_endpoint, AppFramework, Interaction,
        InteractionContext, InteractionContextData, InteractionHandler, InteractionResponse,
        RawInteraction, TypedInteractionHandler,
    };
    use serde_json::{json, Value};

    use super::spawnable;

    /// RFC 8032 test vector 1 public key: a valid Ed25519 point.
    const PUBLIC_KEY: &str = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";

    #[derive(Clone)]
    struct Raw;

    impl InteractionHandler for Raw {
        async fn handle(
            &self,
            ctx: InteractionContext,
            _interaction: RawInteraction,
        ) -> InteractionResponse {
            tokio::task::yield_now().await;
            InteractionResponse::ChannelMessage(json!({ "content": ctx.id.to_string() }))
        }
    }

    #[derive(Clone)]
    struct Typed;

    impl TypedInteractionHandler for Typed {
        async fn handle_typed(
            &self,
            _ctx: InteractionContextData,
            interaction: Interaction,
        ) -> InteractionResponse {
            tokio::task::yield_now().await;
            match interaction {
                Interaction::ChatInputCommand(_) => InteractionResponse::DeferredMessage,
                _ => InteractionResponse::DeferredUpdate,
            }
        }
    }

    fn command_payload() -> Value {
        json!({
            "id": "100",
            "application_id": "200",
            "type": 2,
            "token": "interaction-token",
            "version": 1,
            "data": { "id": "300", "name": "ping", "type": 1 },
            "user": { "id": "400", "username": "user" },
        })
    }

    #[tokio::test]
    async fn async_fn_handlers_run_on_spawned_tasks() {
        let payload = command_payload();
        let ctx = parse_interaction_context(&payload).expect("context parses");
        let raw = parse_raw_interaction(&payload).expect("raw interaction parses");
        let response = tokio::spawn(spawnable(async move { Raw.handle(ctx, raw).await }))
            .await
            .expect("raw handler completes");
        assert!(matches!(
            response,
            InteractionResponse::ChannelMessage(body) if body["content"] == "100"
        ));

        let interaction = parse_interaction(&payload).expect("typed interaction parses");
        let context = interaction.context().clone();
        let response = tokio::spawn(spawnable(async move {
            Typed.handle_typed(context, interaction).await
        }))
        .await
        .expect("typed handler completes");
        assert!(matches!(response, InteractionResponse::DeferredMessage));
    }

    #[test]
    fn endpoints_accept_async_fn_handlers() {
        assert!(try_interactions_endpoint(PUBLIC_KEY, Raw).is_ok());
        assert!(try_typed_interactions_endpoint(PUBLIC_KEY, Typed).is_ok());
        assert!(
            try_typed_interactions_endpoint(PUBLIC_KEY, AppFramework::builder().build()).is_ok()
        );
        // Uppercase hex is accepted, malformed hex is a model error.
        assert!(try_interactions_endpoint(&PUBLIC_KEY.to_ascii_uppercase(), Raw).is_ok());
        assert!(try_interactions_endpoint(&PUBLIC_KEY[1..], Raw).is_err());
    }
}
