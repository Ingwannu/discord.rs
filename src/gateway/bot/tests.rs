use std::sync::Arc;
#[cfg(feature = "sharding")]
use std::sync::Mutex as StdMutex;

use async_trait::async_trait;
use serde_json::{json, Value};
#[cfg(feature = "sharding")]
use tokio::sync::watch;
use tokio::sync::{mpsc, Mutex, RwLock};
#[cfg(feature = "sharding")]
use tokio::task::JoinHandle;
#[cfg(feature = "sharding")]
use tokio::time::sleep;
use tokio::time::Duration;

#[cfg(feature = "sharding")]
use super::{auto_shard_plan, ShardSupervisor};
use super::{EventHandler, ShardMessenger};
use crate::bitfield::Intents;
use crate::cache::CacheConfig;
use crate::event::{
    decode_event, BulkMessageDeleteEvent, ChannelEvent, ChannelPinsUpdateEvent, Event,
    GuildBanEvent, GuildDeleteEvent, GuildDeletePayload, GuildEmojisUpdateEvent, GuildEvent,
    GuildIntegrationsUpdateEvent, GuildStickersUpdateEvent, InviteEvent, MemberEvent,
    MemberRemoveEvent, MemberRemovePayload, MessageDeleteEvent, MessageDeletePayload, MessageEvent,
    PresenceUpdateEvent, RateLimitedEvent, ReactionEvent, ReactionRemoveAllEvent, ReadyEvent,
    ReadyPayload, RoleDeleteEvent, RoleDeletePayload, RoleEvent, ScheduledEvent,
    StageInstanceEvent, TypingStartEvent, UserUpdateEvent, VoiceServerEvent, VoiceStateEvent,
    WebhooksUpdateEvent,
};
use crate::gateway::client::{GatewayCommand, GATEWAY_COMMAND_QUEUE_CAPACITY};
use crate::http::DiscordHttpClient;
use crate::model::{
    Activity, Attachment, ComponentInteraction, ComponentInteractionData, Embed, Interaction,
    InteractionContextData, Member, Message, Reaction, RequestChannelInfo, Role, Snowflake,
    SoundboardSound, StageInstance, Sticker, User, VoiceServerUpdate, VoiceState,
};
#[cfg(feature = "sharding")]
use crate::model::{GatewayBot, SessionStartLimit};
#[cfg(feature = "sharding")]
use crate::sharding::{ShardConfig, ShardingManager};
use crate::types::Emoji;
#[cfg(feature = "voice")]
use crate::voice::{AudioTrack, VoiceConnectionConfig};

/// Shard pair for cache tests that model a single-shard bot.
const SINGLE_SHARD: (u32, u32) = (0, 1);

#[test]
fn typed_event_decoder_maps_message_create() {
    let event = decode_event(
        "MESSAGE_CREATE",
        json!({
            "id": "2",
            "channel_id": "1",
            "content": "hello",
            "mentions": [],
            "attachments": []
        }),
    )
    .unwrap();

    assert_eq!(event.kind(), "MESSAGE_CREATE");
}

#[cfg(feature = "cache")]
#[tokio::test]
async fn apply_cache_updates_clears_cache_on_ready() {
    let cache = crate::cache::CacheHandle::new();
    let guild_id = Snowflake::from("1");
    let channel_id = Snowflake::from("10");
    let message_id = Snowflake::from("11");

    cache
        .upsert_guild(crate::model::Guild {
            id: guild_id.clone(),
            name: "discordrs".to_string(),
            ..crate::model::Guild::default()
        })
        .await;
    cache
        .upsert_message(Message {
            id: message_id.clone(),
            channel_id: channel_id.clone(),
            content: "cached".to_string(),
            ..Message::default()
        })
        .await;

    super::apply_cache_updates(
        &cache,
        SINGLE_SHARD,
        &Event::Ready(ReadyEvent {
            data: ReadyPayload {
                user: User {
                    id: Snowflake::from("2"),
                    username: "bot".to_string(),
                    ..User::default()
                },
                session_id: "session".to_string(),
                application: None,
                resume_gateway_url: None,
            },
            raw: json!({ "session_id": "session" }),
        }),
    )
    .await;

    assert!(cache.guild(&guild_id).await.is_none());
    assert!(cache.message(&channel_id, &message_id).await.is_none());
}

#[cfg(feature = "cache")]
#[tokio::test]
async fn apply_cache_updates_merges_partial_message_update_without_wiping_cached_fields() {
    let cache = crate::cache::CacheHandle::new();
    let channel_id = Snowflake::from("1");
    let message_id = Snowflake::from("2");

    cache
        .upsert_message(Message {
            id: message_id.clone(),
            channel_id: channel_id.clone(),
            content: "before".to_string(),
            author: Some(User {
                id: Snowflake::from("3"),
                username: "alice".to_string(),
                ..User::default()
            }),
            attachments: vec![crate::model::Attachment {
                id: Snowflake::from("4"),
                filename: "file.txt".to_string(),
                ..crate::model::Attachment::default()
            }],
            mentions: vec![User {
                id: Snowflake::from("5"),
                username: "bob".to_string(),
                ..User::default()
            }],
            ..Message::default()
        })
        .await;

    super::apply_cache_updates(
        &cache,
        SINGLE_SHARD,
        &Event::MessageUpdate(MessageEvent {
            message: Message {
                id: message_id.clone(),
                channel_id: channel_id.clone(),
                content: "after".to_string(),
                ..Message::default()
            },
            raw: json!({
                "id": message_id.as_str(),
                "channel_id": channel_id.as_str(),
                "content": "after"
            }),
        }),
    )
    .await;

    let cached = cache.message(&channel_id, &message_id).await.unwrap();
    assert_eq!(cached.content, "after");
    assert_eq!(cached.author.unwrap().username, "alice");
    assert_eq!(cached.attachments.len(), 1);
    assert_eq!(cached.mentions.len(), 1);
}

#[cfg(feature = "cache")]
#[tokio::test]
async fn apply_cache_updates_ignores_partial_message_update_without_cached_message() {
    let cache = crate::cache::CacheHandle::new();
    let channel_id = Snowflake::from("1");
    let message_id = Snowflake::from("2");

    super::apply_cache_updates(
        &cache,
        SINGLE_SHARD,
        &Event::MessageUpdate(MessageEvent {
            message: Message {
                id: message_id.clone(),
                channel_id: channel_id.clone(),
                content: "after".to_string(),
                ..Message::default()
            },
            raw: json!({
                "id": message_id.as_str(),
                "channel_id": channel_id.as_str(),
                "content": "after"
            }),
        }),
    )
    .await;

    assert!(cache.message(&channel_id, &message_id).await.is_none());
}

#[cfg(feature = "cache")]
#[tokio::test]
async fn apply_cache_updates_evicts_bulk_deleted_messages() {
    let cache = crate::cache::CacheHandle::new();
    let channel_id = Snowflake::from("1");
    let first = Snowflake::from("2");
    let second = Snowflake::from("3");

    for message_id in [first.clone(), second.clone()] {
        cache
            .upsert_message(Message {
                id: message_id,
                channel_id: channel_id.clone(),
                content: "hello".to_string(),
                ..Message::default()
            })
            .await;
    }

    super::apply_cache_updates(
        &cache,
        SINGLE_SHARD,
        &Event::MessageDeleteBulk(BulkMessageDeleteEvent {
            ids: vec![first.clone(), second.clone()],
            channel_id: channel_id.clone(),
            guild_id: None,
            raw: json!({}),
        }),
    )
    .await;

    assert!(cache.message(&channel_id, &first).await.is_none());
    assert!(cache.message(&channel_id, &second).await.is_none());
}

#[cfg(feature = "cache")]
#[tokio::test]
async fn apply_cache_updates_tracks_user_presence_and_voice_state_events() {
    let cache = crate::cache::CacheHandle::new();
    let guild_id = Snowflake::from("1");
    let channel_id = Snowflake::from("2");
    let user_id = Snowflake::from("3");
    let message_id = Snowflake::from("4");

    super::apply_cache_updates(
        &cache,
        SINGLE_SHARD,
        &Event::UserUpdate(UserUpdateEvent {
            user: User {
                id: user_id.clone(),
                username: "updated".to_string(),
                ..User::default()
            },
            raw: json!({ "id": user_id.as_str(), "username": "updated" }),
        }),
    )
    .await;
    assert_eq!(cache.user(&user_id).await.unwrap().username, "updated");

    super::apply_cache_updates(
        &cache,
        SINGLE_SHARD,
        &Event::MessageCreate(MessageEvent {
            message: Message {
                id: message_id.clone(),
                channel_id: channel_id.clone(),
                guild_id: Some(guild_id.clone()),
                author: Some(User {
                    id: user_id.clone(),
                    username: "author".to_string(),
                    ..User::default()
                }),
                content: "hello".to_string(),
                ..Message::default()
            },
            raw: json!({
                "id": message_id.as_str(),
                "channel_id": channel_id.as_str()
            }),
        }),
    )
    .await;
    assert_eq!(cache.user(&user_id).await.unwrap().username, "author");
    assert!(cache.message(&channel_id, &message_id).await.is_some());

    super::apply_cache_updates(
        &cache,
        SINGLE_SHARD,
        &Event::PresenceUpdate(PresenceUpdateEvent {
            user_id: Some(user_id.clone()),
            guild_id: Some(guild_id.clone()),
            status: Some("online".to_string()),
            activities: vec![Activity {
                name: "testing".to_string(),
                ..Activity::default()
            }],
            raw: json!({
                "user": { "id": user_id.as_str() },
                "guild_id": guild_id.as_str(),
                "status": "online",
                "activities": [{ "name": "testing" }]
            }),
            ..PresenceUpdateEvent::default()
        }),
    )
    .await;
    let presence = cache.presence(&guild_id, &user_id).await.unwrap();
    assert_eq!(presence.status.as_deref(), Some("online"));
    assert_eq!(presence.activities.unwrap().len(), 1);

    super::apply_cache_updates(
        &cache,
        SINGLE_SHARD,
        &Event::VoiceStateUpdate(VoiceStateEvent {
            state: VoiceState {
                guild_id: Some(guild_id.clone()),
                channel_id: Some(channel_id.clone()),
                user_id: Some(user_id.clone()),
                member: Some(Member {
                    user: Some(User {
                        id: user_id.clone(),
                        username: "voice".to_string(),
                        ..User::default()
                    }),
                    ..Member::default()
                }),
                ..VoiceState::default()
            },
            raw: json!({
                "guild_id": guild_id.as_str(),
                "channel_id": channel_id.as_str(),
                "user_id": user_id.as_str()
            }),
        }),
    )
    .await;
    assert_eq!(cache.user(&user_id).await.unwrap().username, "voice");
    assert!(cache.member(&guild_id, &user_id).await.is_some());
    assert!(cache.voice_state(&guild_id, &user_id).await.is_some());

    super::apply_cache_updates(
        &cache,
        SINGLE_SHARD,
        &Event::VoiceStateUpdate(VoiceStateEvent {
            state: VoiceState {
                guild_id: Some(guild_id.clone()),
                channel_id: None,
                user_id: Some(user_id.clone()),
                ..VoiceState::default()
            },
            raw: json!({
                "guild_id": guild_id.as_str(),
                "channel_id": null,
                "user_id": user_id.as_str()
            }),
        }),
    )
    .await;
    assert!(cache.voice_state(&guild_id, &user_id).await.is_none());
}

#[cfg(feature = "cache")]
#[tokio::test]
async fn apply_cache_updates_tracks_soundboard_sound_events() {
    let cache = crate::cache::CacheHandle::new();
    let guild_id = Snowflake::from("1");
    let first_sound_id = Snowflake::from("10");
    let second_sound_id = Snowflake::from("11");

    super::apply_cache_updates(
        &cache,
        SINGLE_SHARD,
        &Event::GuildSoundboardSoundCreate(crate::event::SoundboardSoundEvent {
            sound: SoundboardSound {
                name: "one".to_string(),
                sound_id: first_sound_id.clone(),
                guild_id: Some(guild_id.clone()),
                volume: 1.0,
                available: true,
                ..SoundboardSound::default()
            },
            raw: json!({}),
        }),
    )
    .await;
    assert!(cache
        .soundboard_sound(&guild_id, &first_sound_id)
        .await
        .is_some());

    super::apply_cache_updates(
        &cache,
        SINGLE_SHARD,
        &Event::SoundboardSounds(crate::event::SoundboardSoundsEvent {
            guild_id: guild_id.clone(),
            soundboard_sounds: vec![SoundboardSound {
                name: "two".to_string(),
                sound_id: second_sound_id.clone(),
                guild_id: Some(guild_id.clone()),
                volume: 1.0,
                available: true,
                ..SoundboardSound::default()
            }],
            raw: json!({}),
        }),
    )
    .await;
    assert!(cache
        .soundboard_sound(&guild_id, &first_sound_id)
        .await
        .is_none());
    assert_eq!(
        cache
            .soundboard_sound(&guild_id, &second_sound_id)
            .await
            .unwrap()
            .name,
        "two"
    );

    super::apply_cache_updates(
        &cache,
        SINGLE_SHARD,
        &Event::GuildSoundboardSoundDelete(crate::event::SoundboardSoundDeleteEvent {
            sound_id: second_sound_id.clone(),
            guild_id: guild_id.clone(),
            raw: json!({}),
        }),
    )
    .await;
    assert!(cache
        .soundboard_sound(&guild_id, &second_sound_id)
        .await
        .is_none());
}

#[cfg(feature = "cache")]
#[tokio::test]
async fn apply_cache_updates_tracks_guild_metadata_events() {
    let cache = crate::cache::CacheHandle::new();
    let guild_id = Snowflake::from("900");
    let emoji_id = Snowflake::from("901");
    let sticker_id = Snowflake::from("902");
    let scheduled_event_id = Snowflake::from("903");
    let stage_instance_id = Snowflake::from("904");
    let stage_channel_id = Snowflake::from("905");

    super::apply_cache_updates(
        &cache,
        SINGLE_SHARD,
        &Event::GuildEmojisUpdate(GuildEmojisUpdateEvent {
            guild_id: guild_id.clone(),
            emojis: vec![Emoji {
                id: Some(emoji_id.to_string()),
                name: Some("wave".to_string()),
                ..Emoji::default()
            }],
            raw: json!({}),
        }),
    )
    .await;
    assert_eq!(
        cache
            .emoji(&guild_id, &emoji_id)
            .await
            .unwrap()
            .name
            .as_deref(),
        Some("wave")
    );

    super::apply_cache_updates(
        &cache,
        SINGLE_SHARD,
        &Event::GuildStickersUpdate(GuildStickersUpdateEvent {
            guild_id: Some(guild_id.clone()),
            stickers: vec![Sticker {
                id: sticker_id.clone(),
                name: "ship".to_string(),
                kind: 1,
                format_type: 1,
                guild_id: Some(guild_id.clone()),
                ..Sticker::default()
            }],
            raw: json!({}),
        }),
    )
    .await;
    assert_eq!(
        cache.sticker(&guild_id, &sticker_id).await.unwrap().name,
        "ship"
    );

    super::apply_cache_updates(
        &cache,
        SINGLE_SHARD,
        &Event::GuildScheduledEventCreate(ScheduledEvent {
            id: Some(scheduled_event_id.clone()),
            guild_id: Some(guild_id.clone()),
            name: Some("town hall".to_string()),
            raw: json!({}),
            ..ScheduledEvent::default()
        }),
    )
    .await;
    assert_eq!(
        cache
            .scheduled_event(&guild_id, &scheduled_event_id)
            .await
            .unwrap()
            .name
            .as_deref(),
        Some("town hall")
    );

    super::apply_cache_updates(
        &cache,
        SINGLE_SHARD,
        &Event::StageInstanceCreate(StageInstanceEvent {
            stage_instance: StageInstance {
                id: stage_instance_id.clone(),
                guild_id: guild_id.clone(),
                channel_id: stage_channel_id,
                topic: "live".to_string(),
                privacy_level: 2,
                ..StageInstance::default()
            },
            raw: json!({}),
        }),
    )
    .await;
    assert_eq!(
        cache
            .stage_instance(&guild_id, &stage_instance_id)
            .await
            .unwrap()
            .topic,
        "live"
    );

    super::apply_cache_updates(
        &cache,
        SINGLE_SHARD,
        &Event::GuildScheduledEventDelete(ScheduledEvent {
            id: Some(scheduled_event_id.clone()),
            guild_id: Some(guild_id.clone()),
            raw: json!({}),
            ..ScheduledEvent::default()
        }),
    )
    .await;
    assert!(cache
        .scheduled_event(&guild_id, &scheduled_event_id)
        .await
        .is_none());

    super::apply_cache_updates(
        &cache,
        SINGLE_SHARD,
        &Event::StageInstanceDelete(StageInstanceEvent {
            stage_instance: StageInstance {
                id: stage_instance_id.clone(),
                guild_id: guild_id.clone(),
                ..StageInstance::default()
            },
            raw: json!({}),
        }),
    )
    .await;
    assert!(cache
        .stage_instance(&guild_id, &stage_instance_id)
        .await
        .is_none());
}

#[tokio::test]
async fn event_handler_routes_typed_hooks_and_reserves_raw_for_unknowns() {
    struct RecordingHandler {
        hits: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl super::EventHandler for RecordingHandler {
        async fn ready(&self, _ctx: super::Context, ready_data: ReadyPayload) {
            self.hits
                .lock()
                .await
                .push(format!("ready:{}", ready_data.user.username));
        }

        async fn message_create(&self, _ctx: super::Context, message: Message) {
            self.hits
                .lock()
                .await
                .push(format!("message:{}", message.content));
        }

        async fn interaction_create(&self, _ctx: super::Context, interaction: Interaction) {
            let label = match interaction {
                Interaction::Ping(_) => "interaction:ping",
                _ => "interaction:other",
            };
            self.hits.lock().await.push(label.to_string());
        }

        async fn message_delete_bulk(&self, _ctx: super::Context, event: BulkMessageDeleteEvent) {
            self.hits
                .lock()
                .await
                .push(format!("bulk:{}", event.ids.len()));
        }

        async fn raw_event(&self, _ctx: super::Context, event_name: String, _data: Value) {
            self.hits.lock().await.push(format!("raw:{event_name}"));
        }
    }

    let hits = Arc::new(Mutex::new(Vec::new()));
    let handler = RecordingHandler {
        hits: Arc::clone(&hits),
    };
    let context = super::Context::new(
        Arc::new(DiscordHttpClient::new("token", 0)),
        Arc::new(RwLock::new(super::TypeMap::new())),
    );

    handler
        .handle_event(
            context.clone(),
            Event::Ready(ReadyEvent {
                data: ReadyPayload {
                    user: User {
                        id: Snowflake::from("1"),
                        username: "bot".to_string(),
                        ..User::default()
                    },
                    session_id: "session".to_string(),
                    application: None,
                    resume_gateway_url: None,
                },
                raw: json!({ "session_id": "session" }),
            }),
        )
        .await;
    handler
        .handle_event(
            context.clone(),
            Event::MessageCreate(MessageEvent {
                message: Message {
                    id: Snowflake::from("2"),
                    channel_id: Snowflake::from("3"),
                    content: "hello".to_string(),
                    ..Message::default()
                },
                raw: json!({}),
            }),
        )
        .await;
    handler
        .handle_event(
            context.clone(),
            decode_event(
                "INTERACTION_CREATE",
                json!({
                    "id": "4",
                    "application_id": "5",
                    "token": "interaction-token",
                    "type": 1
                }),
            )
            .unwrap(),
        )
        .await;
    handler
        .handle_event(
            context.clone(),
            Event::MessageDeleteBulk(BulkMessageDeleteEvent {
                ids: vec![Snowflake::from("6"), Snowflake::from("7")],
                channel_id: Snowflake::from("3"),
                guild_id: None,
                raw: json!({}),
            }),
        )
        .await;
    handler
        .handle_event(
            context,
            Event::Unknown {
                kind: "SOMETHING_NEW".to_string(),
                raw: json!({ "surprise": true }),
            },
        )
        .await;

    assert_eq!(
        *hits.lock().await,
        vec![
            "ready:bot".to_string(),
            "message:hello".to_string(),
            "interaction:ping".to_string(),
            "bulk:2".to_string(),
            "raw:SOMETHING_NEW".to_string(),
        ]
    );
}

#[tokio::test]
async fn gateway_event_processor_preserves_order_and_sets_application_id() {
    struct OrderedHandler {
        hits: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl super::EventHandler for OrderedHandler {
        async fn ready(&self, _ctx: super::Context, _ready_data: ReadyPayload) {
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            self.hits.lock().await.push("ready".to_string());
        }

        async fn message_create(&self, _ctx: super::Context, message: Message) {
            self.hits
                .lock()
                .await
                .push(format!("message:{}", message.content));
        }
    }

    let hits = Arc::new(Mutex::new(Vec::new()));
    let handler: Arc<dyn super::EventHandler> = Arc::new(OrderedHandler {
        hits: Arc::clone(&hits),
    });
    let http = Arc::new(DiscordHttpClient::new("token", 0));
    let context = super::Context::new(
        Arc::clone(&http),
        Arc::new(RwLock::new(super::TypeMap::new())),
    );
    let pipeline = super::spawn_event_pipeline(
        handler,
        context,
        #[cfg(feature = "cache")]
        None,
        super::DispatchConfig::default(),
    );
    let ingress = pipeline.ingress();

    ingress.push(super::GatewayDispatch {
        event_name: "READY".to_string(),
        data: json!({
            "user": {
                "id": "1",
                "username": "bot"
            },
            "session_id": "session",
            "application": {
                "id": "42"
            },
            "resume_gateway_url": "wss://gateway.discord.gg"
        }),
        received_at: tokio::time::Instant::now(),
    });
    ingress.push(super::GatewayDispatch {
        event_name: "MESSAGE_CREATE".to_string(),
        data: json!({
            "id": "2",
            "channel_id": "3",
            "content": "hello",
            "mentions": [],
            "attachments": []
        }),
        received_at: tokio::time::Instant::now(),
    });
    drop(ingress);
    pipeline
        .shutdown(Duration::from_secs(5))
        .await
        .expect("pipeline drains cleanly");

    assert_eq!(http.application_id(), 42);
    assert_eq!(
        *hits.lock().await,
        vec!["ready".to_string(), "message:hello".to_string()]
    );
}

#[tokio::test]
async fn type_map_and_context_data_helpers_round_trip_values() {
    let mut map = super::TypeMap::default();
    map.insert::<String>("hello".to_string());
    map.insert::<u32>(7);
    assert_eq!(map.get::<String>().map(String::as_str), Some("hello"));
    assert_eq!(map.get::<u32>().copied(), Some(7));
    *map.get_mut::<u32>().unwrap() = 9;
    assert_eq!(map.get::<u32>().copied(), Some(9));
    assert!(map.get::<bool>().is_none());

    let http = Arc::new(DiscordHttpClient::new("token", 123));
    let data = Arc::new(RwLock::new(map));
    let context = super::Context::new(Arc::clone(&http), Arc::clone(&data));
    let rest = context.rest();

    assert!(Arc::ptr_eq(&rest, &http));
    assert_eq!(
        context
            .with_data(|values| values.get::<String>().cloned())
            .await,
        Some("hello".to_string())
    );

    context.insert_data::<Vec<u8>>(vec![1, 2, 3]).await;

    assert_eq!(
        context.get_data_cloned::<Vec<u8>>().await,
        Some(vec![1, 2, 3])
    );
    assert_eq!(
        context
            .with_data(|values| values.get::<u32>().copied())
            .await,
        Some(9)
    );
}

#[tokio::test]
async fn context_forwards_shard_commands_and_reports_missing_messenger() {
    let mut context = super::Context::new(
        Arc::new(DiscordHttpClient::new("token", 0)),
        Arc::new(RwLock::new(super::TypeMap::new())),
    );
    context.shard_id = 4;

    let error = context
        .update_presence("offline")
        .await
        .expect_err("missing shard messenger should fail");
    assert!(error.to_string().contains("missing shard messenger"));

    let (command_tx, mut command_rx) = mpsc::channel(GATEWAY_COMMAND_QUEUE_CAPACITY);
    super::write_gateway_commands(&context.gateway_commands).insert(
        4,
        ShardMessenger {
            shard_id: 4,
            command_tx,
        },
    );

    assert_eq!(context.shard_messenger().await.unwrap().shard_id(), 4);

    context.update_presence("online").await.unwrap();
    context.reconnect_shard().await.unwrap();
    context.shutdown_shard().await.unwrap();
    context.join_voice("1", "2", true, false).await.unwrap();
    context.leave_voice("1", false, true).await.unwrap();
    context
        .request_soundboard_sounds(vec![Snowflake::from("3")])
        .await
        .unwrap();
    context
        .request_channel_info(RequestChannelInfo::voice_metadata("3"))
        .await
        .unwrap();

    assert!(matches!(
        command_rx.try_recv().unwrap(),
        GatewayCommand::UpdatePresence(status) if status == "online"
    ));
    assert!(matches!(
        command_rx.try_recv().unwrap(),
        GatewayCommand::Reconnect
    ));
    assert!(matches!(
        command_rx.try_recv().unwrap(),
        GatewayCommand::Shutdown
    ));

    match command_rx.try_recv().unwrap() {
        GatewayCommand::SendPayload(payload) => {
            assert_eq!(payload["d"]["guild_id"], json!("1"));
            assert_eq!(payload["d"]["channel_id"], json!("2"));
            assert_eq!(payload["d"]["self_mute"], json!(true));
            assert_eq!(payload["d"]["self_deaf"], json!(false));
        }
        other => panic!("unexpected gateway command: {other:?}"),
    }

    match command_rx.try_recv().unwrap() {
        GatewayCommand::SendPayload(payload) => {
            assert_eq!(payload["d"]["guild_id"], json!("1"));
            assert_eq!(payload["d"]["channel_id"], Value::Null);
            assert_eq!(payload["d"]["self_mute"], json!(false));
            assert_eq!(payload["d"]["self_deaf"], json!(true));
        }
        other => panic!("unexpected gateway command: {other:?}"),
    }

    match command_rx.try_recv().unwrap() {
        GatewayCommand::SendPayload(payload) => {
            assert_eq!(payload["op"], json!(31));
            assert_eq!(payload["d"]["guild_ids"], json!(["3"]));
            assert_eq!(payload["d"]["channels"], Value::Null);
        }
        other => panic!("unexpected gateway command: {other:?}"),
    }

    match command_rx.try_recv().unwrap() {
        GatewayCommand::RequestChannelInfo(request) => {
            assert_eq!(request.guild_id.as_str(), "3");
            assert_eq!(
                request.fields,
                vec!["status".to_string(), "voice_start_time".to_string()]
            );
        }
        other => panic!("unexpected gateway command: {other:?}"),
    }
}

#[test]
fn shard_messenger_reports_channel_send_failures() {
    let (command_tx, command_rx) = mpsc::channel(1);
    drop(command_rx);
    let messenger = ShardMessenger {
        shard_id: 9,
        command_tx,
    };

    let error = messenger
        .shutdown()
        .expect_err("closed gateway command channel should fail");

    assert!(error.to_string().contains("failed to send gateway command"));
}

#[test]
fn client_builder_and_rest_helpers_preserve_configuration() {
    let gateway_config = crate::ws::GatewayConnectionConfig::new("wss://gateway.example.test");
    let builder = super::Client::builder("token", Intents::from_bits(7))
        .application_id(42)
        .type_map_insert::<String>("state".to_string())
        .gateway_config(gateway_config.clone())
        .cache_config(CacheConfig::unbounded().max_total_messages(5))
        .shard(9, 0);

    assert_eq!(builder.application_id, Some(42));
    assert_eq!(builder.shard, Some((9, 1)));
    assert_eq!(
        builder.gateway_config.normalized_url(),
        gateway_config.normalized_url()
    );
    assert_eq!(builder.cache_config.max_total_messages, Some(5));
    assert_eq!(builder.cache_config.max_users, None);
    assert_eq!(
        builder.data.get::<String>().map(String::as_str),
        Some("state")
    );

    let rest_only = builder.rest_only();
    assert_eq!(rest_only.application_id(), 42);

    let rest = super::Client::rest("other", 7);
    assert_eq!(rest.application_id(), 7);
}

#[tokio::test]
async fn client_builder_start_variants_require_event_handler() {
    let start_error = super::Client::builder("token", Intents::from_bits(0))
        .start()
        .await
        .expect_err("start should fail before network without a handler");
    assert!(start_error
        .to_string()
        .contains("event_handler is required"));

    #[cfg(feature = "sharding")]
    {
        let shard_error = super::Client::builder("token", Intents::from_bits(0))
            .start_shards(0)
            .await
            .expect_err("start_shards should fail before network without a handler");
        assert!(shard_error
            .to_string()
            .contains("event_handler is required"));

        let auto_error = super::Client::builder("token", Intents::from_bits(0))
            .start_auto_shards()
            .await
            .expect_err("start_auto_shards should fail before network without a handler");
        assert!(auto_error.to_string().contains("event_handler is required"));

        let spawn_error = super::Client::builder("token", Intents::from_bits(0))
            .spawn_shards(0)
            .await
            .err()
            .expect("spawn_shards should fail before network without a handler");
        assert!(spawn_error
            .to_string()
            .contains("event_handler is required"));

        let spawn_auto_error = super::Client::builder("token", Intents::from_bits(0))
            .spawn_auto_shards()
            .await
            .err()
            .expect("spawn_auto_shards should fail before network without a handler");
        assert!(spawn_auto_error
            .to_string()
            .contains("event_handler is required"));
    }
}

#[tokio::test]
async fn shared_runtime_context_reuses_shared_state() {
    let mut data = super::TypeMap::new();
    data.insert::<String>("runtime".to_string());
    let runtime = super::SharedRuntime::new("token", 77, data, CacheConfig::default(), None);
    let (command_tx, _command_rx) = mpsc::channel(GATEWAY_COMMAND_QUEUE_CAPACITY);

    super::write_gateway_commands(&runtime.gateway_commands).insert(
        2,
        ShardMessenger {
            shard_id: 2,
            command_tx,
        },
    );

    let context = runtime.context((2, 5));

    assert_eq!(context.shard_pair(), (2, 5));
    assert_eq!(
        context.get_data_cloned::<String>().await,
        Some("runtime".to_string())
    );
    assert_eq!(context.rest().application_id(), 77);
    assert_eq!(context.shard_messenger().await.unwrap().shard_id(), 2);
    #[cfg(feature = "sharding")]
    assert_eq!(
        context.shard_info(),
        crate::sharding::ShardInfo { id: 2, total: 5 }
    );

    context.insert_data::<u32>(11).await;
    assert_eq!(runtime.data.read().await.get::<u32>().copied(), Some(11));
}

#[cfg(feature = "collectors")]
#[tokio::test]
async fn shared_runtime_context_reuses_collectors_hub() {
    let runtime = super::SharedRuntime::new(
        "token",
        0,
        super::TypeMap::new(),
        CacheConfig::default(),
        None,
    );
    let context = runtime.context((0, 1));
    let mut collector = context
        .collectors()
        .message_collector()
        .timeout(std::time::Duration::from_secs(1));

    runtime
        .collectors
        .publish(Event::MessageCreate(MessageEvent {
            message: Message {
                id: Snowflake::from("88"),
                channel_id: Snowflake::from("89"),
                content: "shared".to_string(),
                ..Message::default()
            },
            raw: json!({}),
        }));

    assert_eq!(collector.next().await.unwrap().content, "shared");
}

#[tokio::test]
async fn process_gateway_dispatch_falls_back_to_raw_events_and_keeps_existing_application_id() {
    struct DispatchHandler {
        hits: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl super::EventHandler for DispatchHandler {
        async fn ready(&self, _ctx: super::Context, ready_data: ReadyPayload) {
            self.hits
                .lock()
                .await
                .push(format!("ready:{}", ready_data.session_id));
        }

        async fn raw_event(&self, _ctx: super::Context, event_name: String, _data: Value) {
            self.hits.lock().await.push(format!("raw:{event_name}"));
        }
    }

    let hits = Arc::new(Mutex::new(Vec::new()));
    let handler: Arc<dyn super::EventHandler> = Arc::new(DispatchHandler {
        hits: Arc::clone(&hits),
    });
    let http = Arc::new(DiscordHttpClient::new("token", 7));
    let context = super::Context::new(
        Arc::clone(&http),
        Arc::new(RwLock::new(super::TypeMap::new())),
    );
    for dispatch in [
        super::GatewayDispatch {
            event_name: "READY".to_string(),
            data: json!({
                "user": {
                    "id": "1",
                    "username": "bot"
                },
                "session_id": "existing",
                "application": {
                    "id": "42"
                }
            }),
            received_at: tokio::time::Instant::now(),
        },
        super::GatewayDispatch {
            event_name: "GUILD_MEMBER_ADD".to_string(),
            data: json!({
                "member": {
                    "user": {
                        "id": "2",
                        "username": "alice"
                    }
                }
            }),
            received_at: tokio::time::Instant::now(),
        },
    ] {
        let event = super::process_gateway_dispatch(
            &context,
            #[cfg(feature = "cache")]
            None,
            dispatch,
        )
        .await;
        handler.handle_event(context.clone(), event.event).await;
    }

    assert_eq!(http.application_id(), 7);
    assert_eq!(
        *hits.lock().await,
        vec![
            "ready:existing".to_string(),
            "raw:GUILD_MEMBER_ADD".to_string()
        ]
    );
}

#[cfg(feature = "cache")]
#[tokio::test]
async fn apply_cache_updates_covers_guild_channel_member_role_and_interaction_paths() {
    let cache = crate::cache::CacheHandle::new();
    let guild_id = Snowflake::from("100");
    let channel_id = Snowflake::from("101");
    let user_id = Snowflake::from("102");
    let role_id = Snowflake::from("103");

    super::apply_cache_updates(
        &cache,
        SINGLE_SHARD,
        &Event::GuildCreate(GuildEvent {
            guild: crate::model::Guild {
                id: guild_id.clone(),
                name: "guild".to_string(),
                roles: vec![Role {
                    id: role_id.clone(),
                    name: "admin".to_string(),
                    ..Role::default()
                }],
                ..crate::model::Guild::default()
            },
            raw: json!({}),
        }),
    )
    .await;
    assert_eq!(cache.guild(&guild_id).await.unwrap().name, "guild");
    assert_eq!(cache.role(&guild_id, &role_id).await.unwrap().name, "admin");

    super::apply_cache_updates(
        &cache,
        SINGLE_SHARD,
        &Event::ChannelCreate(ChannelEvent {
            channel: crate::model::Channel {
                id: channel_id.clone(),
                guild_id: Some(guild_id.clone()),
                kind: 0,
                name: Some("general".to_string()),
                ..crate::model::Channel::default()
            },
            raw: json!({}),
        }),
    )
    .await;
    assert_eq!(
        cache.channel(&channel_id).await.unwrap().name.as_deref(),
        Some("general")
    );

    super::apply_cache_updates(
        &cache,
        SINGLE_SHARD,
        &Event::MemberAdd(MemberEvent {
            guild_id: guild_id.clone(),
            member: Member {
                user: Some(User {
                    id: user_id.clone(),
                    username: "alice".to_string(),
                    ..User::default()
                }),
                nick: Some("ally".to_string()),
                ..Member::default()
            },
            raw: json!({}),
        }),
    )
    .await;
    assert_eq!(
        cache
            .member(&guild_id, &user_id)
            .await
            .unwrap()
            .nick
            .as_deref(),
        Some("ally")
    );

    super::apply_cache_updates(
        &cache,
        SINGLE_SHARD,
        &Event::MemberUpdate(MemberEvent {
            guild_id: guild_id.clone(),
            member: Member::default(),
            raw: json!({}),
        }),
    )
    .await;
    assert_eq!(
        cache
            .member(&guild_id, &user_id)
            .await
            .unwrap()
            .nick
            .as_deref(),
        Some("ally")
    );

    super::apply_cache_updates(
        &cache,
        SINGLE_SHARD,
        &Event::RoleDelete(RoleDeleteEvent {
            data: RoleDeletePayload {
                guild_id: guild_id.clone(),
                role_id: role_id.clone(),
            },
            raw: json!({}),
        }),
    )
    .await;
    assert!(cache.role(&guild_id, &role_id).await.is_none());

    super::apply_cache_updates(
        &cache,
        SINGLE_SHARD,
        &Event::MemberRemove(MemberRemoveEvent {
            data: MemberRemovePayload {
                guild_id: guild_id.clone(),
                user: User {
                    id: user_id.clone(),
                    username: "alice".to_string(),
                    ..User::default()
                },
            },
            raw: json!({}),
        }),
    )
    .await;
    assert!(cache.member(&guild_id, &user_id).await.is_none());

    let interaction_channel_id = Snowflake::from("104");
    super::apply_cache_updates(
        &cache,
        SINGLE_SHARD,
        &Event::InteractionCreate(crate::event::InteractionEvent {
            interaction: Interaction::Component(ComponentInteraction {
                context: InteractionContextData {
                    id: Snowflake::from("105"),
                    application_id: Snowflake::from("106"),
                    token: "token".to_string(),
                    guild_id: Some(guild_id.clone()),
                    channel_id: Some(interaction_channel_id.clone()),
                    ..InteractionContextData::default()
                },
                data: ComponentInteractionData {
                    custom_id: "button".to_string(),
                    component_type: 2,
                    values: vec![],
                },
            }),
            raw: json!({}),
        }),
    )
    .await;

    let interaction_channel = cache.channel(&interaction_channel_id).await.unwrap();
    assert_eq!(interaction_channel.guild_id, Some(guild_id.clone()));

    super::apply_cache_updates(
        &cache,
        SINGLE_SHARD,
        &Event::ChannelDelete(ChannelEvent {
            channel: crate::model::Channel {
                id: channel_id.clone(),
                ..crate::model::Channel::default()
            },
            raw: json!({}),
        }),
    )
    .await;
    assert!(cache.channel(&channel_id).await.is_none());

    super::apply_cache_updates(
        &cache,
        SINGLE_SHARD,
        &Event::GuildDelete(GuildDeleteEvent {
            data: GuildDeletePayload {
                id: guild_id.clone(),
                unavailable: None,
            },
            raw: json!({}),
        }),
    )
    .await;
    assert!(cache.guild(&guild_id).await.is_none());
    assert!(cache.channel(&interaction_channel_id).await.is_none());
}

#[cfg(feature = "cache")]
#[tokio::test]
async fn apply_cache_updates_leaves_cache_unchanged_for_noop_event_variants() {
    let cache = crate::cache::CacheHandle::new();
    let guild_id = Snowflake::from("300");
    let channel_id = Snowflake::from("301");
    let message_id = Snowflake::from("302");

    cache
        .upsert_guild(crate::model::Guild {
            id: guild_id.clone(),
            name: "guild".to_string(),
            ..crate::model::Guild::default()
        })
        .await;
    cache
        .upsert_channel(crate::model::Channel {
            id: channel_id.clone(),
            guild_id: Some(guild_id.clone()),
            kind: 0,
            name: Some("general".to_string()),
            ..crate::model::Channel::default()
        })
        .await;
    cache
        .upsert_message(Message {
            id: message_id.clone(),
            channel_id: channel_id.clone(),
            guild_id: Some(guild_id.clone()),
            content: "cached".to_string(),
            ..Message::default()
        })
        .await;

    let events = vec![
        Event::VoiceStateUpdate(VoiceStateEvent {
            state: VoiceState {
                guild_id: Some(guild_id.clone()),
                channel_id: Some(channel_id.clone()),
                user_id: Some(Snowflake::from("400")),
                ..VoiceState::default()
            },
            raw: json!({}),
        }),
        Event::VoiceServerUpdate(VoiceServerEvent {
            data: VoiceServerUpdate {
                guild_id: guild_id.clone(),
                token: "voice-token".to_string(),
                endpoint: Some("voice.example.test".to_string()),
            },
            raw: json!({}),
        }),
        Event::ChannelPinsUpdate(ChannelPinsUpdateEvent {
            channel_id: channel_id.clone(),
            guild_id: Some(guild_id.clone()),
            last_pin_timestamp: Some("2024-01-01T00:00:00Z".to_string()),
            raw: json!({}),
        }),
        Event::GuildBanAdd(GuildBanEvent {
            guild_id: guild_id.clone(),
            user: User {
                id: Snowflake::from("401"),
                username: "banned".to_string(),
                ..User::default()
            },
            raw: json!({}),
        }),
        Event::GuildBanRemove(GuildBanEvent {
            guild_id: guild_id.clone(),
            user: User {
                id: Snowflake::from("402"),
                username: "unbanned".to_string(),
                ..User::default()
            },
            raw: json!({}),
        }),
        Event::GuildEmojisUpdate(GuildEmojisUpdateEvent {
            guild_id: guild_id.clone(),
            emojis: vec![],
            raw: json!({}),
        }),
        Event::GuildIntegrationsUpdate(GuildIntegrationsUpdateEvent {
            guild_id: Some(guild_id.clone()),
            raw: json!({}),
        }),
        Event::WebhooksUpdate(WebhooksUpdateEvent {
            guild_id: Some(guild_id.clone()),
            channel_id: Some(channel_id.clone()),
            raw: json!({}),
        }),
        Event::InviteCreate(InviteEvent {
            guild_id: Some(guild_id.clone()),
            channel_id: Some(channel_id.clone()),
            code: Some("invite".to_string()),
            inviter: None,
            uses: None,
            max_uses: None,
            max_age: None,
            temporary: None,
            created_at: None,
            expires_at: None,
            target_type: None,
            target_user: None,
            raw: json!({}),
        }),
        Event::InviteDelete(InviteEvent {
            guild_id: Some(guild_id.clone()),
            channel_id: Some(channel_id.clone()),
            code: Some("invite".to_string()),
            inviter: None,
            uses: None,
            max_uses: None,
            max_age: None,
            temporary: None,
            created_at: None,
            expires_at: None,
            target_type: None,
            target_user: None,
            raw: json!({}),
        }),
        Event::MessageReactionAdd(ReactionEvent {
            user_id: Some(Snowflake::from("403")),
            channel_id: Some(channel_id.clone()),
            message_id: Some(message_id.clone()),
            guild_id: Some(guild_id.clone()),
            emoji: None,
            raw: json!({}),
            ..ReactionEvent::default()
        }),
        Event::MessageReactionRemove(ReactionEvent {
            user_id: Some(Snowflake::from("404")),
            channel_id: Some(channel_id.clone()),
            message_id: Some(message_id.clone()),
            guild_id: Some(guild_id.clone()),
            emoji: None,
            raw: json!({}),
            ..ReactionEvent::default()
        }),
        Event::MessageReactionRemoveAll(ReactionRemoveAllEvent {
            channel_id: Some(channel_id.clone()),
            message_id: Some(message_id.clone()),
            guild_id: Some(guild_id.clone()),
            raw: json!({}),
        }),
        Event::TypingStart(TypingStartEvent {
            channel_id: Some(channel_id.clone()),
            guild_id: Some(guild_id.clone()),
            user_id: Some(Snowflake::from("405")),
            timestamp: Some(123),
            raw: json!({}),
        }),
        Event::PresenceUpdate(PresenceUpdateEvent {
            user_id: Some(Snowflake::from("406")),
            guild_id: Some(guild_id.clone()),
            status: Some("online".to_string()),
            raw: json!({}),
            ..PresenceUpdateEvent::default()
        }),
        Event::Unknown {
            kind: "UNKNOWN_EVENT".to_string(),
            raw: json!({}),
        },
    ];

    let context = pipeline_test_context(SINGLE_SHARD);
    for event in events {
        super::apply_cache_updates(&cache, SINGLE_SHARD, &event).await;
        // The default EventHandler hooks accept every variant as no-ops.
        NoopEventHandler.handle_event(context.clone(), event).await;
    }

    assert_eq!(cache.guild(&guild_id).await.unwrap().name, "guild");
    assert_eq!(
        cache.channel(&channel_id).await.unwrap().name.as_deref(),
        Some("general")
    );
    assert_eq!(
        cache
            .message(&channel_id, &message_id)
            .await
            .unwrap()
            .content,
        "cached"
    );
}

#[tokio::test]
async fn event_handler_routes_additional_typed_hooks() {
    struct ExtendedHandler {
        hits: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl super::EventHandler for ExtendedHandler {
        async fn member_add(&self, _ctx: super::Context, guild_id: Snowflake, member: Member) {
            self.hits.lock().await.push(format!(
                "member_add:{}:{}",
                guild_id.as_str(),
                member.user.as_ref().unwrap().id.as_str()
            ));
        }

        async fn role_delete(&self, _ctx: super::Context, guild_id: Snowflake, role_id: Snowflake) {
            self.hits.lock().await.push(format!(
                "role_delete:{}:{}",
                guild_id.as_str(),
                role_id.as_str()
            ));
        }

        async fn message_delete(
            &self,
            _ctx: super::Context,
            channel_id: Snowflake,
            message_id: Snowflake,
        ) {
            self.hits.lock().await.push(format!(
                "message_delete:{}:{}",
                channel_id.as_str(),
                message_id.as_str()
            ));
        }

        async fn channel_pins_update(&self, _ctx: super::Context, event: ChannelPinsUpdateEvent) {
            self.hits.lock().await.push(format!(
                "pins:{}:{}",
                event.channel_id.as_str(),
                event.last_pin_timestamp.unwrap()
            ));
        }

        async fn guild_ban_add(&self, _ctx: super::Context, event: GuildBanEvent) {
            self.hits.lock().await.push(format!(
                "ban_add:{}:{}",
                event.guild_id.as_str(),
                event.user.id.as_str()
            ));
        }

        async fn voice_server_update(&self, _ctx: super::Context, data: VoiceServerUpdate) {
            self.hits.lock().await.push(format!(
                "voice_server:{}:{}",
                data.guild_id.as_str(),
                data.token
            ));
        }

        async fn reaction_remove_all(
            &self,
            _ctx: super::Context,
            event: crate::event::ReactionRemoveAllEvent,
        ) {
            self.hits.lock().await.push(format!(
                "reaction_all:{}:{}",
                event.channel_id.unwrap().as_str(),
                event.message_id.unwrap().as_str()
            ));
        }

        async fn presence_update(&self, _ctx: super::Context, event: PresenceUpdateEvent) {
            self.hits.lock().await.push(format!(
                "presence:{}:{}",
                event.user_id.unwrap().as_str(),
                event.status.unwrap()
            ));
        }

        async fn gateway_rate_limited(&self, _ctx: super::Context, event: RateLimitedEvent) {
            self.hits
                .lock()
                .await
                .push(format!("rate_limited:{}", event.opcode.unwrap()));
        }
    }

    let hits = Arc::new(Mutex::new(Vec::new()));
    let handler = ExtendedHandler {
        hits: Arc::clone(&hits),
    };
    let context = super::Context::new(
        Arc::new(DiscordHttpClient::new("token", 0)),
        Arc::new(RwLock::new(super::TypeMap::new())),
    );

    handler
        .handle_event(
            context.clone(),
            Event::MemberAdd(MemberEvent {
                guild_id: Snowflake::from("1"),
                member: Member {
                    user: Some(User {
                        id: Snowflake::from("2"),
                        username: "alice".to_string(),
                        ..User::default()
                    }),
                    ..Member::default()
                },
                raw: json!({}),
            }),
        )
        .await;
    handler
        .handle_event(
            context.clone(),
            Event::RoleDelete(RoleDeleteEvent {
                data: RoleDeletePayload {
                    guild_id: Snowflake::from("3"),
                    role_id: Snowflake::from("4"),
                },
                raw: json!({}),
            }),
        )
        .await;
    handler
        .handle_event(
            context.clone(),
            Event::MessageDelete(MessageDeleteEvent {
                data: MessageDeletePayload {
                    id: Snowflake::from("5"),
                    channel_id: Snowflake::from("6"),
                    guild_id: None,
                },
                raw: json!({}),
            }),
        )
        .await;
    handler
        .handle_event(
            context.clone(),
            Event::ChannelPinsUpdate(ChannelPinsUpdateEvent {
                channel_id: Snowflake::from("7"),
                guild_id: None,
                last_pin_timestamp: Some("2024-01-01T00:00:00Z".to_string()),
                raw: json!({}),
            }),
        )
        .await;
    handler
        .handle_event(
            context.clone(),
            Event::GuildBanAdd(GuildBanEvent {
                guild_id: Snowflake::from("8"),
                user: User {
                    id: Snowflake::from("9"),
                    username: "banned".to_string(),
                    ..User::default()
                },
                raw: json!({}),
            }),
        )
        .await;
    handler
        .handle_event(
            context.clone(),
            Event::VoiceServerUpdate(VoiceServerEvent {
                data: VoiceServerUpdate {
                    guild_id: Snowflake::from("10"),
                    token: "voice-token".to_string(),
                    endpoint: Some("voice.discord.media".to_string()),
                },
                raw: json!({}),
            }),
        )
        .await;
    handler
        .handle_event(
            context.clone(),
            Event::MessageReactionRemoveAll(ReactionRemoveAllEvent {
                channel_id: Some(Snowflake::from("11")),
                message_id: Some(Snowflake::from("12")),
                guild_id: None,
                raw: json!({}),
            }),
        )
        .await;
    handler
        .handle_event(
            context.clone(),
            Event::PresenceUpdate(PresenceUpdateEvent {
                user_id: Some(Snowflake::from("13")),
                guild_id: Some(Snowflake::from("14")),
                status: Some("online".to_string()),
                raw: json!({}),
                ..PresenceUpdateEvent::default()
            }),
        )
        .await;
    handler
        .handle_event(
            context.clone(),
            Event::RateLimited(RateLimitedEvent {
                opcode: Some(8),
                retry_after: Some(1.5),
                meta: Some(json!({ "guild_id": "14" })),
                raw: json!({}),
            }),
        )
        .await;

    assert_eq!(
        *hits.lock().await,
        vec![
            "member_add:1:2".to_string(),
            "role_delete:3:4".to_string(),
            "message_delete:6:5".to_string(),
            "pins:7:2024-01-01T00:00:00Z".to_string(),
            "ban_add:8:9".to_string(),
            "voice_server:10:voice-token".to_string(),
            "reaction_all:11:12".to_string(),
            "presence:13:online".to_string(),
            "rate_limited:8".to_string(),
        ]
    );
}

#[tokio::test]
async fn event_handler_default_impl_accepts_remaining_event_variants() {
    struct NoopHandler;

    #[async_trait]
    impl super::EventHandler for NoopHandler {}

    let handler = NoopHandler;
    let context = super::Context::new(
        Arc::new(DiscordHttpClient::new("token", 0)),
        Arc::new(RwLock::new(super::TypeMap::new())),
    );

    let guild_id = Snowflake::from("500");
    let channel_id = Snowflake::from("501");
    let user_id = Snowflake::from("502");
    let role_id = Snowflake::from("503");
    let message_id = Snowflake::from("504");

    let events = vec![
        Event::GuildCreate(GuildEvent {
            guild: crate::model::Guild {
                id: guild_id.clone(),
                name: "guild".to_string(),
                ..crate::model::Guild::default()
            },
            raw: json!({}),
        }),
        Event::GuildUpdate(GuildEvent {
            guild: crate::model::Guild {
                id: guild_id.clone(),
                name: "guild-updated".to_string(),
                ..crate::model::Guild::default()
            },
            raw: json!({}),
        }),
        Event::GuildDelete(GuildDeleteEvent {
            data: GuildDeletePayload {
                id: guild_id.clone(),
                unavailable: Some(true),
            },
            raw: json!({}),
        }),
        Event::ChannelCreate(ChannelEvent {
            channel: crate::model::Channel {
                id: channel_id.clone(),
                guild_id: Some(guild_id.clone()),
                kind: 0,
                name: Some("general".to_string()),
                ..crate::model::Channel::default()
            },
            raw: json!({}),
        }),
        Event::ChannelUpdate(ChannelEvent {
            channel: crate::model::Channel {
                id: channel_id.clone(),
                guild_id: Some(guild_id.clone()),
                kind: 0,
                name: Some("general-2".to_string()),
                ..crate::model::Channel::default()
            },
            raw: json!({}),
        }),
        Event::ChannelDelete(ChannelEvent {
            channel: crate::model::Channel {
                id: channel_id.clone(),
                guild_id: Some(guild_id.clone()),
                kind: 0,
                ..crate::model::Channel::default()
            },
            raw: json!({}),
        }),
        Event::MemberUpdate(MemberEvent {
            guild_id: guild_id.clone(),
            member: Member {
                user: Some(User {
                    id: user_id.clone(),
                    username: "member".to_string(),
                    ..User::default()
                }),
                ..Member::default()
            },
            raw: json!({}),
        }),
        Event::MemberRemove(MemberRemoveEvent {
            data: MemberRemovePayload {
                guild_id: guild_id.clone(),
                user: User {
                    id: user_id.clone(),
                    username: "member".to_string(),
                    ..User::default()
                },
            },
            raw: json!({}),
        }),
        Event::RoleCreate(RoleEvent {
            guild_id: guild_id.clone(),
            role: Role {
                id: role_id.clone(),
                name: "role".to_string(),
                ..Role::default()
            },
            raw: json!({}),
        }),
        Event::RoleUpdate(RoleEvent {
            guild_id: guild_id.clone(),
            role: Role {
                id: role_id.clone(),
                name: "role-updated".to_string(),
                ..Role::default()
            },
            raw: json!({}),
        }),
        Event::MessageUpdate(MessageEvent {
            message: Message {
                id: message_id.clone(),
                channel_id: channel_id.clone(),
                content: "updated".to_string(),
                ..Message::default()
            },
            raw: json!({}),
        }),
        Event::GuildBanRemove(GuildBanEvent {
            guild_id: guild_id.clone(),
            user: User {
                id: user_id.clone(),
                username: "member".to_string(),
                ..User::default()
            },
            raw: json!({}),
        }),
        Event::GuildEmojisUpdate(GuildEmojisUpdateEvent {
            guild_id: guild_id.clone(),
            emojis: vec![],
            raw: json!({}),
        }),
        Event::GuildIntegrationsUpdate(GuildIntegrationsUpdateEvent {
            guild_id: Some(guild_id.clone()),
            raw: json!({}),
        }),
        Event::WebhooksUpdate(WebhooksUpdateEvent {
            guild_id: Some(guild_id.clone()),
            channel_id: Some(channel_id.clone()),
            raw: json!({}),
        }),
        Event::InviteCreate(InviteEvent {
            guild_id: Some(guild_id.clone()),
            channel_id: Some(channel_id.clone()),
            code: Some("invite-create".to_string()),
            inviter: None,
            uses: None,
            max_uses: None,
            max_age: None,
            temporary: None,
            created_at: None,
            expires_at: None,
            target_type: None,
            target_user: None,
            raw: json!({}),
        }),
        Event::InviteDelete(InviteEvent {
            guild_id: Some(guild_id.clone()),
            channel_id: Some(channel_id.clone()),
            code: Some("invite-delete".to_string()),
            inviter: None,
            uses: None,
            max_uses: None,
            max_age: None,
            temporary: None,
            created_at: None,
            expires_at: None,
            target_type: None,
            target_user: None,
            raw: json!({}),
        }),
        Event::VoiceStateUpdate(VoiceStateEvent {
            state: VoiceState {
                guild_id: Some(guild_id.clone()),
                channel_id: Some(channel_id.clone()),
                user_id: Some(user_id.clone()),
                ..VoiceState::default()
            },
            raw: json!({}),
        }),
        Event::MessageReactionAdd(ReactionEvent {
            user_id: Some(user_id.clone()),
            channel_id: Some(channel_id.clone()),
            message_id: Some(message_id.clone()),
            guild_id: Some(guild_id.clone()),
            emoji: None,
            raw: json!({}),
            ..ReactionEvent::default()
        }),
        Event::MessageReactionRemove(ReactionEvent {
            user_id: Some(user_id),
            channel_id: Some(channel_id.clone()),
            message_id: Some(message_id),
            guild_id: Some(guild_id),
            emoji: None,
            raw: json!({}),
            ..ReactionEvent::default()
        }),
        Event::TypingStart(TypingStartEvent {
            channel_id: Some(channel_id),
            guild_id: Some(Snowflake::from("500")),
            user_id: Some(Snowflake::from("502")),
            timestamp: Some(321),
            raw: json!({}),
        }),
    ];

    for event in events {
        handler.handle_event(context.clone(), event).await;
    }
}

#[cfg(feature = "cache")]
#[tokio::test]
async fn apply_cache_updates_handles_message_create_delete_and_non_channel_component_cases() {
    let cache = crate::cache::CacheHandle::new();
    let guild_id = Snowflake::from("700");
    let channel_id = Snowflake::from("701");
    let message_id = Snowflake::from("702");
    let role_id = Snowflake::from("703");

    super::apply_cache_updates(
        &cache,
        SINGLE_SHARD,
        &Event::RoleCreate(RoleEvent {
            guild_id: guild_id.clone(),
            role: Role {
                id: role_id.clone(),
                name: "reader".to_string(),
                ..Role::default()
            },
            raw: json!({}),
        }),
    )
    .await;
    assert_eq!(
        cache.role(&guild_id, &role_id).await.unwrap().name,
        "reader"
    );

    super::apply_cache_updates(
        &cache,
        SINGLE_SHARD,
        &Event::RoleUpdate(RoleEvent {
            guild_id: guild_id.clone(),
            role: Role {
                id: role_id.clone(),
                name: "writer".to_string(),
                ..Role::default()
            },
            raw: json!({}),
        }),
    )
    .await;
    assert_eq!(
        cache.role(&guild_id, &role_id).await.unwrap().name,
        "writer"
    );

    super::apply_cache_updates(
        &cache,
        SINGLE_SHARD,
        &Event::MessageCreate(MessageEvent {
            message: Message {
                id: message_id.clone(),
                channel_id: channel_id.clone(),
                guild_id: Some(guild_id.clone()),
                content: "created".to_string(),
                ..Message::default()
            },
            raw: json!({}),
        }),
    )
    .await;
    assert_eq!(
        cache
            .message(&channel_id, &message_id)
            .await
            .unwrap()
            .content,
        "created"
    );

    super::apply_cache_updates(
        &cache,
        SINGLE_SHARD,
        &Event::MessageDelete(MessageDeleteEvent {
            data: MessageDeletePayload {
                id: message_id.clone(),
                channel_id: channel_id.clone(),
                guild_id: Some(guild_id.clone()),
            },
            raw: json!({}),
        }),
    )
    .await;
    assert!(cache.message(&channel_id, &message_id).await.is_none());

    let missing_channel_id = Snowflake::from("704");
    super::apply_cache_updates(
        &cache,
        SINGLE_SHARD,
        &Event::InteractionCreate(crate::event::InteractionEvent {
            interaction: Interaction::Component(ComponentInteraction {
                context: InteractionContextData {
                    id: Snowflake::from("705"),
                    application_id: Snowflake::from("706"),
                    token: "token".to_string(),
                    guild_id: Some(guild_id),
                    channel_id: None,
                    ..InteractionContextData::default()
                },
                data: ComponentInteractionData {
                    custom_id: "button".to_string(),
                    component_type: 2,
                    values: vec![],
                },
            }),
            raw: json!({}),
        }),
    )
    .await;
    assert!(cache.channel(&missing_channel_id).await.is_none());
}

#[cfg(feature = "cache")]
#[tokio::test]
async fn context_manager_accessors_reuse_the_underlying_cache() {
    let context = super::Context::new(
        Arc::new(DiscordHttpClient::new("token", 0)),
        Arc::new(RwLock::new(super::TypeMap::new())),
    );
    let guild_id = Snowflake::from("720");
    let channel_id = Snowflake::from("721");
    let user_id = Snowflake::from("722");
    let message_id = Snowflake::from("723");
    let role_id = Snowflake::from("724");

    context
        .cache
        .upsert_guild(crate::model::Guild {
            id: guild_id.clone(),
            name: "guild".to_string(),
            ..crate::model::Guild::default()
        })
        .await;
    context
        .cache
        .upsert_channel(crate::model::Channel {
            id: channel_id.clone(),
            guild_id: Some(guild_id.clone()),
            kind: 0,
            name: Some("general".to_string()),
            ..crate::model::Channel::default()
        })
        .await;
    context
        .cache
        .upsert_member(
            guild_id.clone(),
            user_id.clone(),
            Member {
                user: Some(User {
                    id: user_id.clone(),
                    username: "alice".to_string(),
                    ..User::default()
                }),
                ..Member::default()
            },
        )
        .await;
    context
        .cache
        .upsert_message(Message {
            id: message_id.clone(),
            channel_id: channel_id.clone(),
            guild_id: Some(guild_id.clone()),
            content: "cached".to_string(),
            ..Message::default()
        })
        .await;
    context
        .cache
        .upsert_role(
            guild_id.clone(),
            Role {
                id: role_id.clone(),
                name: "reader".to_string(),
                ..Role::default()
            },
        )
        .await;

    assert_eq!(
        context
            .guilds()
            .cached(guild_id.clone())
            .await
            .unwrap()
            .name,
        "guild"
    );
    assert_eq!(
        context
            .channels()
            .cached(channel_id.clone())
            .await
            .unwrap()
            .name
            .as_deref(),
        Some("general")
    );
    assert_eq!(
        context
            .members()
            .cached(guild_id.clone(), user_id.clone())
            .await
            .unwrap()
            .user
            .unwrap()
            .username,
        "alice"
    );
    assert_eq!(
        context
            .messages()
            .cached(channel_id.clone(), message_id.clone())
            .await
            .unwrap()
            .content,
        "cached"
    );
    assert_eq!(
        context
            .roles()
            .cached(guild_id.clone(), role_id.clone())
            .await
            .unwrap()
            .name,
        "reader"
    );
    assert!(context.guilds().contains(guild_id.clone()).await);
    assert!(context.channels().contains(channel_id.clone()).await);
    assert!(
        context
            .members()
            .contains(guild_id.clone(), user_id.clone())
            .await
    );
    assert!(
        context
            .messages()
            .contains(channel_id.clone(), message_id.clone())
            .await
    );
    assert!(
        context
            .roles()
            .contains(guild_id.clone(), role_id.clone())
            .await
    );
    assert_eq!(context.guilds().list_cached().await.len(), 1);
    assert_eq!(context.channels().list_cached().await.len(), 1);
    assert_eq!(
        context.members().list_cached(guild_id.clone()).await.len(),
        1
    );
    assert_eq!(context.messages().list_cached(channel_id).await.len(), 1);
    assert_eq!(context.roles().list_cached(guild_id).await.len(), 1);
}

#[cfg(feature = "cache")]
#[tokio::test]
async fn apply_cache_updates_ignores_non_component_interactions() {
    let cache = crate::cache::CacheHandle::new();
    let event = decode_event(
        "INTERACTION_CREATE",
        json!({
            "id": "730",
            "application_id": "731",
            "token": "token",
            "type": 1
        }),
    )
    .expect("ping interaction should decode");

    super::apply_cache_updates(&cache, SINGLE_SHARD, &event).await;

    assert!(cache.channels().await.is_empty());
    assert!(cache.guilds().await.is_empty());
}

#[test]
fn merge_message_update_only_overwrites_fields_present_in_raw_payload() {
    let cached = Message {
        id: Snowflake::from("1"),
        channel_id: Snowflake::from("10"),
        guild_id: Some(Snowflake::from("20")),
        content: "before".to_string(),
        author: Some(User {
            id: Snowflake::from("2"),
            username: "alice".to_string(),
            ..User::default()
        }),
        pinned: Some(true),
        tts: Some(true),
        flags: Some(64),
        webhook_id: Some(Snowflake::from("30")),
        ..Message::default()
    };
    let partial = Message {
        id: Snowflake::from("1"),
        channel_id: Snowflake::from("10"),
        guild_id: None,
        content: "after".to_string(),
        author: None,
        pinned: Some(false),
        tts: Some(false),
        flags: Some(0),
        webhook_id: None,
        ..Message::default()
    };

    let merged = super::merge_message_update(
        cached,
        partial,
        &super::MessageUpdateFields::from_raw(&json!({
            "content": "after",
            "pinned": false
        })),
    );

    assert_eq!(merged.content, "after");
    assert_eq!(merged.pinned, Some(false));
    assert_eq!(merged.guild_id, Some(Snowflake::from("20")));
    assert_eq!(merged.author.unwrap().username, "alice");
    assert_eq!(merged.tts, Some(true));
    assert_eq!(merged.flags, Some(64));
    assert_eq!(merged.webhook_id, Some(Snowflake::from("30")));
}

#[test]
fn merge_message_update_replaces_only_selected_complex_fields() {
    let cached = Message {
        id: Snowflake::from("1"),
        channel_id: Snowflake::from("10"),
        guild_id: Some(Snowflake::from("20")),
        author: Some(User {
            id: Snowflake::from("2"),
            username: "alice".to_string(),
            ..User::default()
        }),
        member: Some(Member {
            nick: Some("ally".to_string()),
            ..Member::default()
        }),
        timestamp: Some("before-ts".to_string()),
        edited_timestamp: Some("before-edit".to_string()),
        mentions: vec![User {
            id: Snowflake::from("3"),
            username: "bob".to_string(),
            ..User::default()
        }],
        attachments: vec![Attachment {
            id: Snowflake::from("4"),
            filename: "old.txt".to_string(),
            ..Attachment::default()
        }],
        kind: Some(1),
        pinned: Some(true),
        tts: Some(true),
        flags: Some(64),
        webhook_id: Some(Snowflake::from("5")),
        embeds: vec![Embed {
            title: Some("old".to_string()),
            ..Embed::default()
        }],
        reactions: vec![Reaction {
            count: 1,
            me: true,
            ..Reaction::default()
        }],
        ..Message::default()
    };
    let partial = Message {
        id: Snowflake::from("1"),
        channel_id: Snowflake::from("10"),
        guild_id: Some(Snowflake::from("21")),
        author: Some(User {
            id: Snowflake::from("6"),
            username: "eve".to_string(),
            ..User::default()
        }),
        member: Some(Member {
            nick: Some("eve-nick".to_string()),
            ..Member::default()
        }),
        timestamp: Some("after-ts".to_string()),
        edited_timestamp: Some("after-edit".to_string()),
        mentions: vec![User {
            id: Snowflake::from("7"),
            username: "mallory".to_string(),
            ..User::default()
        }],
        attachments: vec![Attachment {
            id: Snowflake::from("8"),
            filename: "new.txt".to_string(),
            ..Attachment::default()
        }],
        kind: Some(19),
        pinned: Some(false),
        tts: Some(false),
        flags: Some(0),
        webhook_id: Some(Snowflake::from("9")),
        embeds: vec![Embed {
            title: Some("new".to_string()),
            ..Embed::default()
        }],
        reactions: vec![Reaction {
            count: 9,
            me: false,
            ..Reaction::default()
        }],
        ..Message::default()
    };

    let merged = super::merge_message_update(
        cached,
        partial,
        &super::MessageUpdateFields::from_raw(&json!({
            "guild_id": "21",
            "author": { "id": "6", "username": "eve" },
            "member": { "nick": "eve-nick" },
            "timestamp": "after-ts",
            "mentions": [{ "id": "7", "username": "mallory" }],
            "attachments": [{ "id": "8", "filename": "new.txt" }],
            "type": 19,
            "tts": false,
            "flags": 0,
            "embeds": [{ "title": "new" }],
            "reactions": [{ "count": 9, "me": false }]
        })),
    );

    assert_eq!(merged.guild_id, Some(Snowflake::from("21")));
    assert_eq!(merged.author.unwrap().username, "eve");
    assert_eq!(merged.member.unwrap().nick.as_deref(), Some("eve-nick"));
    assert_eq!(merged.timestamp.as_deref(), Some("after-ts"));
    assert_eq!(merged.edited_timestamp.as_deref(), Some("before-edit"));
    assert_eq!(merged.mentions[0].username, "mallory");
    assert_eq!(merged.attachments[0].filename, "new.txt");
    assert_eq!(merged.kind, Some(19));
    assert_eq!(merged.pinned, Some(true));
    assert_eq!(merged.tts, Some(false));
    assert_eq!(merged.flags, Some(0));
    assert_eq!(merged.webhook_id, Some(Snowflake::from("5")));
    assert_eq!(merged.embeds[0].title.as_deref(), Some("new"));
    assert_eq!(merged.reactions[0].count, 9);
}

#[test]
fn shard_messenger_builds_voice_state_update_gateway_payload() {
    let (command_tx, mut command_rx) = mpsc::channel(GATEWAY_COMMAND_QUEUE_CAPACITY);
    let messenger = ShardMessenger {
        shard_id: 0,
        command_tx,
    };

    messenger.join_voice("1", "2", false, true).unwrap();

    match command_rx.try_recv().unwrap() {
        GatewayCommand::SendPayload(payload) => {
            assert_eq!(payload["op"], serde_json::json!(4));
            assert_eq!(payload["d"]["guild_id"], serde_json::json!("1"));
            assert_eq!(payload["d"]["channel_id"], serde_json::json!("2"));
            assert_eq!(payload["d"]["self_mute"], serde_json::json!(false));
            assert_eq!(payload["d"]["self_deaf"], serde_json::json!(true));
        }
        other => panic!("unexpected gateway command: {other:?}"),
    }
}

#[tokio::test]
async fn context_new_keeps_legacy_http_and_data_entry_points() {
    let http = Arc::new(crate::http::DiscordHttpClient::new("token", 0));
    let data = Arc::new(RwLock::new(super::TypeMap::new()));
    let context = super::Context::new(Arc::clone(&http), Arc::clone(&data));

    assert!(Arc::ptr_eq(&context.http, &http));
    assert!(Arc::ptr_eq(&context.data, &data));
    assert_eq!(context.shard_pair(), (0, 1));
}

#[cfg(feature = "collectors")]
#[tokio::test]
async fn process_gateway_dispatch_publishes_events_to_collectors() {
    struct NoopHandler;

    #[async_trait]
    impl super::EventHandler for NoopHandler {}

    let handler: Arc<dyn super::EventHandler> = Arc::new(NoopHandler);
    let http = Arc::new(DiscordHttpClient::new("token", 0));
    let context = super::Context::new(
        Arc::clone(&http),
        Arc::new(RwLock::new(super::TypeMap::new())),
    );
    let collectors = context.collectors().clone();
    let mut message_collector = collectors
        .message_collector()
        .timeout(std::time::Duration::from_secs(1));
    let mut component_collector = collectors
        .component_collector()
        .timeout(std::time::Duration::from_secs(1));

    for dispatch in [
        super::GatewayDispatch {
            event_name: "MESSAGE_CREATE".to_string(),
            data: json!({
                "id": "201",
                "channel_id": "202",
                "content": "collector-message",
                "mentions": [],
                "attachments": []
            }),
            received_at: tokio::time::Instant::now(),
        },
        super::GatewayDispatch {
            event_name: "INTERACTION_CREATE".to_string(),
            data: json!({
                "id": "203",
                "application_id": "204",
                "token": "token",
                "channel_id": "205",
                "guild_id": "206",
                "type": 3,
                "data": {
                    "custom_id": "button",
                    "component_type": 2,
                    "values": ["one"]
                }
            }),
            received_at: tokio::time::Instant::now(),
        },
    ] {
        let event = super::process_gateway_dispatch(
            &context,
            #[cfg(feature = "cache")]
            None,
            dispatch,
        )
        .await;
        handler.handle_event(context.clone(), event.event).await;
    }

    assert_eq!(
        message_collector.next().await.unwrap().content,
        "collector-message"
    );
    let component = component_collector.next().await.unwrap();
    assert_eq!(component.context.channel_id, Some(Snowflake::from("205")));
    assert_eq!(component.data.custom_id, "button");
}

#[cfg(feature = "voice")]
#[tokio::test]
async fn context_voice_wrappers_and_apply_voice_updates_share_state() {
    let context = super::Context::new(
        Arc::new(DiscordHttpClient::new("token", 0)),
        Arc::new(RwLock::new(super::TypeMap::new())),
    );
    let guild_id = Snowflake::from("301");
    let channel_id = Snowflake::from("302");
    let user_id = Snowflake::from("303");

    let state = context
        .connect_voice(VoiceConnectionConfig::new(
            guild_id.clone(),
            channel_id.clone(),
        ))
        .await;
    assert_eq!(state.guild_id, guild_id);
    assert!(matches!(
        context.voice().read().await.connection("301"),
        Some(existing) if existing.channel_id == channel_id
    ));

    super::apply_voice_updates(
        &context.voice(),
        &Event::VoiceStateUpdate(VoiceStateEvent {
            state: VoiceState {
                guild_id: Some(Snowflake::from("301")),
                channel_id: Some(Snowflake::from("302")),
                user_id: Some(user_id.clone()),
                session_id: Some("session".to_string()),
                self_mute: false,
                self_deaf: false,
                ..VoiceState::default()
            },
            raw: json!({}),
        }),
    )
    .await;
    super::apply_voice_updates(
        &context.voice(),
        &Event::VoiceServerUpdate(VoiceServerEvent {
            data: VoiceServerUpdate {
                guild_id: Snowflake::from("301"),
                token: "voice-token".to_string(),
                endpoint: Some("voice.discord.media".to_string()),
            },
            raw: json!({}),
        }),
    )
    .await;

    let runtime = context
        .voice_runtime_config("301", user_id.clone())
        .await
        .expect("voice runtime config should be ready after updates");
    assert_eq!(runtime.server_id, Snowflake::from("301"));

    let queue_position = context
        .enqueue_voice_track("301", AudioTrack::new("track-1", "memory://track"))
        .await;
    assert_eq!(queue_position, Some(1));

    super::apply_voice_updates(
        &context.voice(),
        &Event::Ready(ReadyEvent {
            data: ReadyPayload {
                user: User {
                    id: Snowflake::from("1"),
                    username: "bot".to_string(),
                    ..User::default()
                },
                session_id: "session".to_string(),
                application: None,
                resume_gateway_url: None,
            },
            raw: json!({}),
        }),
    )
    .await;
    assert!(context.voice_runtime_config("301", user_id).await.is_some());

    let disconnected = context
        .disconnect_voice("301")
        .await
        .expect("tracked voice connection should disconnect");
    assert_eq!(disconnected.guild_id, Snowflake::from("301"));
}

#[cfg(feature = "voice")]
#[tokio::test]
async fn apply_voice_updates_ignores_non_voice_events_and_runtime_connect_requires_full_state() {
    let context = super::Context::new(
        Arc::new(DiscordHttpClient::new("token", 0)),
        Arc::new(RwLock::new(super::TypeMap::new())),
    );

    context
        .connect_voice(VoiceConnectionConfig::new("401", "402"))
        .await;
    super::apply_voice_updates(
        &context.voice(),
        &Event::MessageCreate(MessageEvent {
            message: Message {
                id: Snowflake::from("403"),
                channel_id: Snowflake::from("402"),
                content: "noop".to_string(),
                ..Message::default()
            },
            raw: json!({}),
        }),
    )
    .await;

    assert!(context.voice().read().await.connection("401").is_some());
    assert!(context.voice_runtime_config("401", "404").await.is_none());

    let error = context
        .connect_voice_runtime("401", "404")
        .await
        .err()
        .expect("runtime connect should fail without endpoint/session/token");
    assert!(error
        .to_string()
        .contains("voice runtime requires endpoint, session_id, and token"));
}

#[cfg(feature = "sharding")]
#[tokio::test]
async fn shard_supervisor_wait_for_shutdown_times_out() {
    let supervisor = ShardSupervisor {
        manager: Arc::new(StdMutex::new(ShardingManager::new(ShardConfig::new(1)))),
        tasks: vec![(
            0,
            tokio::spawn(async move {
                sleep(Duration::from_millis(50)).await;
                Ok(())
            }) as JoinHandle<Result<(), crate::error::DiscordError>>,
        )],
        stop_timeout: Duration::from_secs(1),
        boot_scheduler: None,
        background: None,
    };

    let error = supervisor
        .wait_for_shutdown(Duration::from_millis(1))
        .await
        .expect_err("shutdown wait should time out for a hanging shard");

    assert!(error
        .to_string()
        .contains("timed out waiting for shard 0 shutdown"));
}

#[cfg(feature = "sharding")]
#[tokio::test]
async fn shard_supervisor_wait_propagates_task_and_join_errors() {
    let manager = Arc::new(StdMutex::new(ShardingManager::new(ShardConfig::new(1))));
    let task_error_supervisor = ShardSupervisor {
        manager: Arc::clone(&manager),
        tasks: vec![(
            0,
            tokio::spawn(async {
                Err::<(), crate::error::DiscordError>(crate::error::DiscordError::model(
                    "worker failed",
                ))
            }),
        )],
        stop_timeout: Duration::from_secs(1),
        boot_scheduler: None,
        background: None,
    };

    let task_error = task_error_supervisor
        .wait()
        .await
        .expect_err("task error should propagate");
    assert!(task_error.to_string().contains("worker failed"));

    let join_error_supervisor = ShardSupervisor {
        manager,
        tasks: vec![(0, tokio::spawn(async move { panic!("boom") }))],
        stop_timeout: Duration::from_secs(1),
        boot_scheduler: None,
        background: None,
    };

    let join_error = join_error_supervisor
        .wait()
        .await
        .expect_err("join error should propagate");
    assert!(join_error.to_string().contains("shard task failed"));
}

#[cfg(feature = "sharding")]
#[tokio::test]
async fn shard_supervisor_shutdown_and_wait_broadcasts_shutdown() {
    let manager = Arc::new(StdMutex::new(ShardingManager::new(ShardConfig::new(1))));
    let runtime = manager
        .lock()
        .expect("manager mutex poisoned")
        .prepare_runtime(0)
        .unwrap();
    let supervisor = ShardSupervisor {
        manager,
        tasks: Vec::new(),
        stop_timeout: Duration::from_secs(1),
        boot_scheduler: None,
        background: None,
    };

    supervisor.shutdown_and_wait().await.unwrap();

    assert_eq!(
        runtime
            .command_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .unwrap(),
        crate::sharding::ShardIpcMessage::Shutdown
    );
}

#[cfg(feature = "sharding")]
#[test]
fn auto_shard_plan_uses_gateway_bot_recommendations() {
    let plan = auto_shard_plan(&GatewayBot {
        url: "wss://gateway.discord.gg".to_string(),
        shards: 8,
        session_start_limit: SessionStartLimit {
            total: 1000,
            remaining: 5,
            reset_after: 15_000,
            max_concurrency: 4,
        },
    });

    assert_eq!(plan.total_shards, 8);
    assert_eq!(plan.boot_window_size, 4);
    assert_eq!(plan.initial_delay, None);
}

#[cfg(feature = "sharding")]
#[test]
fn auto_shard_plan_waits_for_reset_when_identify_budget_is_empty() {
    let plan = auto_shard_plan(&GatewayBot {
        url: "wss://gateway.discord.gg".to_string(),
        shards: 2,
        session_start_limit: SessionStartLimit {
            total: 1000,
            remaining: 0,
            reset_after: 1_500,
            max_concurrency: 0,
        },
    });

    assert_eq!(plan.total_shards, 2);
    assert_eq!(plan.boot_window_size, 1);
    assert_eq!(plan.initial_delay, Some(Duration::from_millis(1_500)));
}

#[cfg(feature = "sharding")]
#[test]
fn shard_supervisor_callback_publishes_runtime_events() {
    let mut manager = ShardingManager::new(ShardConfig::new(1));
    let runtime = manager.prepare_runtime(0).unwrap();
    let callback = super::shard_supervisor_callback(runtime.publisher());

    callback(crate::sharding::ShardSupervisorEvent::StateChanged {
        shard_id: 0,
        state: crate::sharding::ShardRuntimeState::Running,
    });

    assert_eq!(runtime.state(), crate::sharding::ShardRuntimeState::Running);
    assert_eq!(
        manager.statuses()[0].state,
        crate::sharding::ShardRuntimeState::Running
    );
    assert_eq!(
        manager.drain_events().unwrap(),
        vec![crate::sharding::ShardSupervisorEvent::StateChanged {
            shard_id: 0,
            state: crate::sharding::ShardRuntimeState::Running,
        }]
    );
}

#[cfg(feature = "sharding")]
#[tokio::test]
async fn forward_shard_commands_translates_ipc_messages() {
    let (command_tx, command_rx) = std::sync::mpsc::channel();
    let (gateway_tx, mut gateway_rx) = mpsc::channel(GATEWAY_COMMAND_QUEUE_CAPACITY);

    super::forward_shard_commands(command_rx, gateway_tx);

    command_tx
        .send(crate::sharding::ShardIpcMessage::UpdatePresence(
            "idle".to_string(),
        ))
        .unwrap();
    command_tx
        .send(crate::sharding::ShardIpcMessage::SendPayload(
            json!({ "op": 4 }),
        ))
        .unwrap();
    command_tx
        .send(crate::sharding::ShardIpcMessage::Reconnect)
        .unwrap();
    command_tx
        .send(crate::sharding::ShardIpcMessage::Shutdown)
        .unwrap();
    drop(command_tx);

    assert!(matches!(
        gateway_rx.recv().await,
        Some(GatewayCommand::UpdatePresence(status)) if status == "idle"
    ));
    assert!(matches!(
        gateway_rx.recv().await,
        Some(GatewayCommand::SendPayload(payload)) if payload == json!({ "op": 4 })
    ));
    assert!(matches!(
        gateway_rx.recv().await,
        Some(GatewayCommand::Reconnect)
    ));
    assert!(matches!(
        gateway_rx.recv().await,
        Some(GatewayCommand::Shutdown)
    ));
    assert!(gateway_rx.recv().await.is_none());
}

#[cfg(feature = "sharding")]
#[tokio::test]
async fn forward_shard_commands_exits_when_gateway_channel_is_closed() {
    let (command_tx, command_rx) = std::sync::mpsc::channel();
    let (gateway_tx, gateway_rx) = mpsc::channel(GATEWAY_COMMAND_QUEUE_CAPACITY);
    drop(gateway_rx);

    super::forward_shard_commands(command_rx, gateway_tx);
    command_tx
        .send(crate::sharding::ShardIpcMessage::Reconnect)
        .unwrap();
    drop(command_tx);

    tokio::time::sleep(std::time::Duration::from_millis(25)).await;
}

#[cfg(feature = "sharding")]
#[tokio::test]
async fn start_gateway_shard_exits_cleanly_when_boot_gate_closes_early() {
    struct NoopHandler;

    #[async_trait]
    impl super::EventHandler for NoopHandler {}

    let manager = Arc::new(StdMutex::new(ShardingManager::new(ShardConfig::new(1))));
    let supervisor_channels = manager
        .lock()
        .expect("manager mutex poisoned")
        .prepare_runtime(0)
        .unwrap();
    let runtime = super::SharedRuntime::new(
        "token",
        0,
        super::TypeMap::new(),
        CacheConfig::default(),
        None,
    );
    let (boot_tx, boot_rx) = watch::channel(false);
    drop(boot_tx);

    super::start_gateway_shard(
        super::ShardLaunch {
            token: "token".to_string(),
            intents: 0,
            handler: Arc::new(NoopHandler),
            runtime,
            gateway_config: crate::ws::GatewayConnectionConfig::default(),
            shard: (0, 1),
            presence: None,
            dispatch: super::DispatchConfig::default(),
        },
        super::ShardStartControl {
            supervisor_channels: Some(supervisor_channels),
            boot_gate: Some(boot_rx),
        },
    )
    .await
    .unwrap();

    let guard = manager.lock().expect("manager mutex poisoned");
    assert_eq!(
        guard.statuses()[0].state,
        crate::sharding::ShardRuntimeState::Stopped
    );
    assert!(guard
        .drain_events()
        .expect_err("closed boot gate should leave runtime event channel disconnected")
        .to_string()
        .contains("event channel disconnected"));
}

#[cfg(feature = "sharding")]
#[tokio::test]
async fn shard_supervisor_manager_send_drain_and_wait_success_paths_work() {
    let manager = Arc::new(StdMutex::new(ShardingManager::new(ShardConfig::new(1))));
    let runtime = manager
        .lock()
        .expect("manager mutex poisoned")
        .prepare_runtime(0)
        .unwrap();
    let supervisor = ShardSupervisor {
        manager: Arc::clone(&manager),
        tasks: vec![(
            0,
            tokio::spawn(async { Ok::<(), crate::error::DiscordError>(()) }),
        )],
        stop_timeout: Duration::from_secs(1),
        boot_scheduler: None,
        background: None,
    };

    assert!(Arc::ptr_eq(&supervisor.manager(), &manager));
    assert!(supervisor.drain_events().unwrap().is_empty());
    supervisor
        .send(0, crate::sharding::ShardIpcMessage::Reconnect)
        .unwrap();
    assert_eq!(
        runtime
            .command_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .unwrap(),
        crate::sharding::ShardIpcMessage::Reconnect
    );
    supervisor.wait().await.unwrap();
}

#[cfg(feature = "sharding")]
#[test]
fn shard_supervisor_helper_methods_forward_commands_to_runtime_handles() {
    let manager = Arc::new(StdMutex::new(ShardingManager::new(ShardConfig::new(2))));
    let shard_zero = manager
        .lock()
        .expect("manager mutex poisoned")
        .prepare_runtime(0)
        .unwrap();
    let shard_one = manager
        .lock()
        .expect("manager mutex poisoned")
        .prepare_runtime(1)
        .unwrap();
    let supervisor = ShardSupervisor {
        manager: Arc::clone(&manager),
        tasks: Vec::new(),
        stop_timeout: Duration::from_secs(1),
        boot_scheduler: None,
        background: None,
    };

    supervisor.update_presence(1, "dnd").unwrap();
    supervisor.reconnect(1).unwrap();
    supervisor.join_voice(0, "10", "20", false, true).unwrap();
    supervisor.leave_voice(0, "10", true, false).unwrap();
    supervisor
        .broadcast(crate::sharding::ShardIpcMessage::Shutdown)
        .unwrap();

    assert_eq!(supervisor.statuses().len(), 2);

    assert_eq!(
        shard_one
            .command_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .unwrap(),
        crate::sharding::ShardIpcMessage::UpdatePresence("dnd".to_string())
    );
    assert_eq!(
        shard_one
            .command_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .unwrap(),
        crate::sharding::ShardIpcMessage::Reconnect
    );
    assert_eq!(
        shard_zero
            .command_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .unwrap(),
        crate::sharding::ShardIpcMessage::SendPayload(json!({
            "op": 4,
            "d": {
                "guild_id": "10",
                "channel_id": "20",
                "self_mute": false,
                "self_deaf": true
            }
        }))
    );
    assert_eq!(
        shard_zero
            .command_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .unwrap(),
        crate::sharding::ShardIpcMessage::SendPayload(json!({
            "op": 4,
            "d": {
                "guild_id": "10",
                "channel_id": Value::Null,
                "self_mute": true,
                "self_deaf": false
            }
        }))
    );
    assert_eq!(
        shard_zero
            .command_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .unwrap(),
        crate::sharding::ShardIpcMessage::Shutdown
    );
    assert_eq!(
        shard_one
            .command_rx
            .recv_timeout(std::time::Duration::from_secs(1))
            .unwrap(),
        crate::sharding::ShardIpcMessage::Shutdown
    );
}

#[cfg(feature = "cache")]
#[tokio::test]
async fn apply_cache_updates_populates_entities_from_guild_create() {
    let cache = crate::cache::CacheHandle::new();
    let guild_id = Snowflake::from("9000");

    let guild: crate::model::Guild = serde_json::from_value(json!({
        "id": "9000",
        "name": "populated",
        "roles": [{ "id": "9001", "name": "role" }],
        "channels": [{ "id": "9002", "type": 0, "name": "general" }],
        "threads": [{ "id": "9003", "type": 11, "name": "thread" }],
        "members": [{
            "user": { "id": "9004", "username": "member" },
            "roles": []
        }],
        "voice_states": [{ "user_id": "9004", "channel_id": "9002" }],
        "presences": [{ "user": { "id": "9004", "username": "member" }, "status": "online" }]
    }))
    .expect("guild create payload should deserialize");

    super::apply_cache_updates(
        &cache,
        SINGLE_SHARD,
        &Event::GuildCreate(crate::event::GuildEvent {
            guild,
            raw: json!({}),
        }),
    )
    .await;

    let cached_guild = cache.guild(&guild_id).await.expect("guild cached");
    assert!(
        cached_guild.channels.is_empty(),
        "cached guild should be stripped of bulk collections"
    );
    let channel = cache
        .channel(&Snowflake::from("9002"))
        .await
        .expect("channel cached from GUILD_CREATE");
    assert_eq!(channel.guild_id, Some(guild_id.clone()));
    assert!(cache.channel(&Snowflake::from("9003")).await.is_some());
    assert!(cache
        .member(&guild_id, &Snowflake::from("9004"))
        .await
        .is_some());
    assert!(cache.user(&Snowflake::from("9004")).await.is_some());
    assert!(cache
        .voice_state(&guild_id, &Snowflake::from("9004"))
        .await
        .is_some());
    assert!(cache
        .presence(&guild_id, &Snowflake::from("9004"))
        .await
        .is_some());
    assert!(cache
        .role(&guild_id, &Snowflake::from("9001"))
        .await
        .is_some());
}

#[cfg(feature = "cache")]
#[tokio::test]
async fn apply_cache_updates_caches_guild_members_chunk_and_threads() {
    let cache = crate::cache::CacheHandle::new();
    let guild_id = Snowflake::from("9100");

    let chunk = decode_event(
        "GUILD_MEMBERS_CHUNK",
        json!({
            "guild_id": "9100",
            "chunk_index": 0,
            "chunk_count": 1,
            "members": [{
                "user": { "id": "9101", "username": "chunked" },
                "roles": []
            }],
            "presences": [{ "user": { "id": "9101" }, "status": "idle" }]
        }),
    )
    .unwrap();
    super::apply_cache_updates(&cache, SINGLE_SHARD, &chunk).await;

    assert!(cache
        .member(&guild_id, &Snowflake::from("9101"))
        .await
        .is_some());
    assert!(cache
        .presence(&guild_id, &Snowflake::from("9101"))
        .await
        .is_some());

    let thread_create = decode_event(
        "THREAD_CREATE",
        json!({ "id": "9102", "type": 11, "guild_id": "9100", "newly_created": true }),
    )
    .unwrap();
    super::apply_cache_updates(&cache, SINGLE_SHARD, &thread_create).await;
    assert!(cache.channel(&Snowflake::from("9102")).await.is_some());

    let thread_delete = decode_event(
        "THREAD_DELETE",
        json!({ "id": "9102", "type": 11, "guild_id": "9100" }),
    )
    .unwrap();
    super::apply_cache_updates(&cache, SINGLE_SHARD, &thread_delete).await;
    assert!(cache.channel(&Snowflake::from("9102")).await.is_none());
}

#[cfg(feature = "cache")]
#[tokio::test]
async fn component_interaction_stub_does_not_overwrite_cached_channel() {
    let cache = crate::cache::CacheHandle::new();
    let channel_id = Snowflake::from("9200");

    cache
        .upsert_channel(crate::model::Channel {
            id: channel_id.clone(),
            kind: 5,
            name: Some("announcements".to_string()),
            ..crate::model::Channel::default()
        })
        .await;

    let interaction = decode_event(
        "INTERACTION_CREATE",
        json!({
            "id": "9201",
            "application_id": "9202",
            "type": 3,
            "token": "tok",
            "channel_id": "9200",
            "data": { "custom_id": "btn", "component_type": 2 }
        }),
    )
    .unwrap();
    super::apply_cache_updates(&cache, SINGLE_SHARD, &interaction).await;

    let channel = cache.channel(&channel_id).await.expect("channel kept");
    assert_eq!(channel.kind, 5, "stub must not overwrite the cached type");
    assert_eq!(channel.name.as_deref(), Some("announcements"));
}

#[cfg(feature = "cache")]
#[derive(Default)]
struct RecordingBackend {
    calls: std::sync::Mutex<Vec<String>>,
    /// Messages stored by `put_message`, served by `get_message`.
    messages: std::sync::Mutex<std::collections::HashMap<(Snowflake, Snowflake), Message>>,
    /// When set, every `put_message` waits for a permit before recording,
    /// modelling a slow remote store.
    put_message_gate: Option<Arc<tokio::sync::Semaphore>>,
    /// When set, `get_message` fails like an unreachable store.
    fail_get_message: bool,
    /// When set, `put_message` fails.
    fail_put_message: bool,
    /// While set, `clear_shard_cache` fails.
    fail_clear_shard_cache: std::sync::atomic::AtomicBool,
}

#[cfg(feature = "cache")]
impl RecordingBackend {
    fn record(&self, call: String) {
        self.calls.lock().expect("recorder mutex").push(call);
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().expect("recorder mutex").clone()
    }

    fn stored_message(&self, channel_id: &str, message_id: &str) -> Option<Message> {
        self.messages
            .lock()
            .expect("recorder mutex")
            .get(&(Snowflake::from(channel_id), Snowflake::from(message_id)))
            .cloned()
    }
}

#[cfg(feature = "cache")]
#[async_trait]
impl crate::cache::CacheBackend for RecordingBackend {
    async fn clear_cache(&self) -> Result<(), crate::error::DiscordError> {
        self.record("clear_cache".to_string());
        Ok(())
    }

    async fn clear_shard_cache(
        &self,
        shard_id: u32,
        shard_count: u32,
    ) -> Result<(), crate::error::DiscordError> {
        self.record(format!("clear_shard_cache:{shard_id}:{shard_count}"));
        if self
            .fail_clear_shard_cache
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            return Err(crate::error::DiscordError::model("clear refused"));
        }
        Ok(())
    }

    async fn purge_expired_entries(&self) -> Result<(), crate::error::DiscordError> {
        self.record("purge_expired_entries".to_string());
        Ok(())
    }

    async fn put_member(
        &self,
        guild_id: Snowflake,
        user_id: Snowflake,
        _member: Member,
    ) -> Result<(), crate::error::DiscordError> {
        self.record(format!("put_member:{guild_id}:{user_id}"));
        Ok(())
    }

    async fn get_member(
        &self,
        _guild_id: &Snowflake,
        _user_id: &Snowflake,
    ) -> Result<Option<Arc<Member>>, crate::error::DiscordError> {
        Ok(None)
    }

    async fn list_members(
        &self,
        _guild_id: &Snowflake,
    ) -> Result<Vec<Arc<Member>>, crate::error::DiscordError> {
        Ok(Vec::new())
    }

    async fn delete_member(
        &self,
        guild_id: &Snowflake,
        user_id: &Snowflake,
    ) -> Result<(), crate::error::DiscordError> {
        self.record(format!("delete_member:{guild_id}:{user_id}"));
        Ok(())
    }

    async fn put_message(&self, message: Message) -> Result<(), crate::error::DiscordError> {
        if let Some(gate) = &self.put_message_gate {
            gate.acquire().await.expect("gate open").forget();
        }
        self.record(format!("put_message:{}:{}", message.channel_id, message.id));
        if self.fail_put_message {
            return Err(crate::error::DiscordError::model("put refused"));
        }
        self.messages
            .lock()
            .expect("recorder mutex")
            .insert((message.channel_id.clone(), message.id.clone()), message);
        Ok(())
    }

    async fn get_message(
        &self,
        channel_id: &Snowflake,
        message_id: &Snowflake,
    ) -> Result<Option<Arc<Message>>, crate::error::DiscordError> {
        if self.fail_get_message {
            return Err(crate::error::DiscordError::model("backend unreachable"));
        }
        Ok(self
            .messages
            .lock()
            .expect("recorder mutex")
            .get(&(channel_id.clone(), message_id.clone()))
            .cloned()
            .map(Arc::new))
    }

    async fn list_messages(
        &self,
        _channel_id: &Snowflake,
    ) -> Result<Vec<Arc<Message>>, crate::error::DiscordError> {
        Ok(Vec::new())
    }

    async fn delete_message(
        &self,
        channel_id: &Snowflake,
        message_id: &Snowflake,
    ) -> Result<(), crate::error::DiscordError> {
        self.record(format!("delete_message:{channel_id}:{message_id}"));
        self.messages
            .lock()
            .expect("recorder mutex")
            .remove(&(channel_id.clone(), message_id.clone()));
        Ok(())
    }

    async fn put_presence(
        &self,
        guild_id: Snowflake,
        user_id: Snowflake,
        presence: crate::model::Presence,
    ) -> Result<(), crate::error::DiscordError> {
        self.record(format!(
            "put_presence:{guild_id}:{user_id}:{}",
            presence.status.as_deref().unwrap_or("none")
        ));
        Ok(())
    }

    async fn get_presence(
        &self,
        _guild_id: &Snowflake,
        _user_id: &Snowflake,
    ) -> Result<Option<Arc<crate::model::Presence>>, crate::error::DiscordError> {
        Ok(None)
    }

    async fn list_presences(
        &self,
        _guild_id: &Snowflake,
    ) -> Result<Vec<Arc<crate::model::Presence>>, crate::error::DiscordError> {
        Ok(Vec::new())
    }

    async fn delete_presence(
        &self,
        guild_id: &Snowflake,
        user_id: &Snowflake,
    ) -> Result<(), crate::error::DiscordError> {
        self.record(format!("delete_presence:{guild_id}:{user_id}"));
        Ok(())
    }
}

#[cfg(feature = "cache")]
#[tokio::test]
async fn cache_backend_receives_member_message_and_presence_writes() {
    let recorder = Arc::new(RecordingBackend::default());
    let backend: Arc<dyn crate::cache::CacheBackend> = recorder.clone();
    let cache = crate::cache::CacheHandle::new();

    let member_event = Event::MemberAdd(MemberEvent {
        guild_id: Snowflake::from("1"),
        member: Member {
            user: Some(User {
                id: Snowflake::from("2"),
                username: "wumpus".to_string(),
                ..User::default()
            }),
            ..Member::default()
        },
        raw: json!({}),
    });
    let message_event = Event::MessageCreate(MessageEvent {
        message: Message {
            id: Snowflake::from("11"),
            channel_id: Snowflake::from("10"),
            content: "hello".to_string(),
            ..Message::default()
        },
        raw: json!({}),
    });
    let presence_event = Event::PresenceUpdate(PresenceUpdateEvent {
        guild_id: Some(Snowflake::from("1")),
        user_id: Some(Snowflake::from("2")),
        status: Some("online".to_string()),
        ..PresenceUpdateEvent::default()
    });
    let message_delete_event = Event::MessageDelete(MessageDeleteEvent {
        data: MessageDeletePayload {
            id: Snowflake::from("11"),
            channel_id: Snowflake::from("10"),
            guild_id: None,
        },
        raw: json!({}),
    });
    let member_remove_event = Event::MemberRemove(MemberRemoveEvent {
        data: MemberRemovePayload {
            guild_id: Snowflake::from("1"),
            user: User {
                id: Snowflake::from("2"),
                username: "wumpus".to_string(),
                ..User::default()
            },
        },
        raw: json!({}),
    });

    let (forwarder, _worker) = super::CacheBackendForwarder::spawn(backend, 16);
    for event in [
        &member_event,
        &message_event,
        &presence_event,
        &message_delete_event,
        &member_remove_event,
    ] {
        super::apply_cache_updates(&cache, SINGLE_SHARD, event).await;
        super::forward_cache_backend_updates(&forwarder, &cache, SINGLE_SHARD, event).await;
    }

    // Events without member/message/presence writes are not forwarded.
    super::forward_cache_backend_updates(
        &forwarder,
        &cache,
        SINGLE_SHARD,
        &Event::RoleCreate(RoleEvent {
            guild_id: Snowflake::from("1"),
            role: Role::default(),
            raw: json!({}),
        }),
    )
    .await;
    forwarder.flush().await;

    assert_eq!(
        recorder.calls(),
        vec![
            "put_member:1:2".to_string(),
            "put_message:10:11".to_string(),
            "put_presence:1:2:online".to_string(),
            "delete_message:10:11".to_string(),
            "delete_member:1:2".to_string(),
        ]
    );
}

#[cfg(feature = "cache")]
#[tokio::test]
async fn cache_backend_receives_ready_clear_and_member_chunk_writes() {
    let recorder = Arc::new(RecordingBackend::default());
    let backend: Arc<dyn crate::cache::CacheBackend> = recorder.clone();

    let ready_event = Event::Ready(ReadyEvent {
        data: ReadyPayload {
            user: User {
                id: Snowflake::from("2"),
                username: "bot".to_string(),
                ..User::default()
            },
            session_id: "session".to_string(),
            application: None,
            resume_gateway_url: None,
        },
        raw: json!({ "session_id": "session" }),
    });
    let chunk_event = Event::GuildMembersChunk(crate::event::GuildMembersChunkEvent {
        data: crate::event::GuildMembersChunkPayload {
            guild_id: Snowflake::from("1"),
            members: vec![
                Member {
                    user: Some(User {
                        id: Snowflake::from("2"),
                        username: "first".to_string(),
                        ..User::default()
                    }),
                    ..Member::default()
                },
                Member {
                    user: Some(User {
                        id: Snowflake::from("3"),
                        username: "second".to_string(),
                        ..User::default()
                    }),
                    ..Member::default()
                },
            ],
            chunk_index: 0,
            chunk_count: 1,
            not_found: Vec::new(),
            presences: Some(vec![crate::model::Presence {
                user_id: Some(Snowflake::from("2")),
                user: None,
                status: Some("idle".to_string()),
                activities: None,
                client_status: None,
            }]),
            nonce: None,
        },
        raw: json!({}),
    });

    let cache = crate::cache::CacheHandle::new();
    let (forwarder, _worker) = super::CacheBackendForwarder::spawn(backend, 16);
    for event in [&ready_event, &chunk_event] {
        super::forward_cache_backend_updates(&forwarder, &cache, SINGLE_SHARD, event).await;
    }
    // READY invalidates only the shard that received it.
    super::forward_cache_backend_updates(&forwarder, &cache, (1, 2), &ready_event).await;
    forwarder.flush().await;

    assert_eq!(
        recorder.calls(),
        vec![
            "clear_shard_cache:0:1".to_string(),
            "put_member:1:2".to_string(),
            "put_member:1:3".to_string(),
            "put_presence:1:2:idle".to_string(),
            "clear_shard_cache:1:2".to_string(),
        ]
    );
}

#[cfg(feature = "cache")]
#[test]
fn client_builder_stores_cache_backend() {
    let backend: Arc<dyn crate::cache::CacheBackend> = Arc::new(RecordingBackend::default());
    let builder =
        super::Client::builder("token", Intents::from_bits(0)).cache_backend(Arc::clone(&backend));
    assert!(builder.cache_backend.is_some());
}

#[test]
fn client_builder_stores_default_allowed_mentions() {
    let mentions = crate::model::AllowedMentions {
        replied_user: Some(false),
        ..crate::model::AllowedMentions::default()
    };
    let builder = super::Client::builder("token", Intents::from_bits(0))
        .default_allowed_mentions(mentions.clone());
    assert_eq!(builder.default_allowed_mentions, Some(mentions));
}

#[tokio::test]
async fn shared_runtime_propagates_default_allowed_mentions_to_context_and_rest() {
    let mentions = crate::model::AllowedMentions {
        users: vec![Snowflake::from("7")],
        replied_user: Some(false),
        ..crate::model::AllowedMentions::default()
    };
    let runtime = super::SharedRuntime::new(
        "token",
        1,
        super::TypeMap::new(),
        CacheConfig::default(),
        Some(mentions.clone()),
    );
    let context = runtime.context((0, 1));
    assert_eq!(context.default_allowed_mentions(), Some(&mentions));
    assert_eq!(context.http.default_allowed_mentions(), Some(&mentions));

    let bare_runtime = super::SharedRuntime::new(
        "token",
        1,
        super::TypeMap::new(),
        CacheConfig::default(),
        None,
    );
    let bare_context = bare_runtime.context((0, 1));
    assert!(bare_context.default_allowed_mentions().is_none());
    assert!(bare_context.http.default_allowed_mentions().is_none());
}

#[test]
fn client_builder_gateway_capabilities_reach_the_connection_config() {
    let builder = super::Client::builder("token", Intents::from_bits(0))
        .gateway_capabilities(crate::bitfield::gateway_capabilities::CHANNEL_OBFUSCATION);
    assert_eq!(builder.gateway_config.capability_bits(), 1 << 15);
}

// ---------------------------------------------------------------------------
// Phase 2: event pipeline liveness, bounds, and cache correctness.
// ---------------------------------------------------------------------------

/// A context for shard `shard` with no messengers registered yet.
fn pipeline_test_context(shard: (u32, u32)) -> super::Context {
    let mut context = super::Context::new(
        Arc::new(DiscordHttpClient::new("token", 1)),
        Arc::new(RwLock::new(super::TypeMap::new())),
    );
    context.shard_id = shard.0;
    context.shard_count = shard.1;
    context
}

/// Registers a messenger for `shard_id` and returns the receiving end of its
/// gateway command queue.
fn attach_test_messenger(
    context: &super::Context,
    shard_id: u32,
) -> mpsc::Receiver<GatewayCommand> {
    let (command_tx, command_rx) = mpsc::channel(GATEWAY_COMMAND_QUEUE_CAPACITY);
    super::write_gateway_commands(&context.gateway_commands).insert(
        shard_id,
        ShardMessenger {
            shard_id,
            command_tx,
        },
    );
    command_rx
}

fn message_create_dispatch(id: &str, content: &str) -> super::GatewayDispatch {
    super::GatewayDispatch::new(
        "MESSAGE_CREATE".to_string(),
        json!({
            "id": id,
            "channel_id": "500",
            "content": content,
            "mentions": [],
            "attachments": []
        }),
    )
}

fn members_chunk_dispatch(
    guild_id: &str,
    nonce: &str,
    chunk_index: u64,
    chunk_count: u64,
    user_id: &str,
) -> super::GatewayDispatch {
    super::GatewayDispatch::new(
        "GUILD_MEMBERS_CHUNK".to_string(),
        json!({
            "guild_id": guild_id,
            "members": [{ "user": { "id": user_id, "username": format!("user-{user_id}") } }],
            "chunk_index": chunk_index,
            "chunk_count": chunk_count,
            "nonce": nonce
        }),
    )
}

/// Waits (bounded) until the internal processor cached message `id` from
/// [`message_create_dispatch`].
async fn wait_for_cached_message(cache: &crate::cache::CacheHandle, id: &str) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if cache
                .message(&Snowflake::from("500"), &Snowflake::from(id))
                .await
                .is_some()
            {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("message {id} never reached the cache"));
}

fn ready_dispatch_event() -> Event {
    Event::Ready(ReadyEvent {
        data: ReadyPayload {
            user: User {
                id: Snowflake::from("2"),
                username: "bot".to_string(),
                ..User::default()
            },
            session_id: "session".to_string(),
            application: None,
            resume_gateway_url: None,
        },
        raw: json!({ "session_id": "session" }),
    })
}

#[tokio::test]
async fn serial_handler_can_await_fetch_members_without_stalling_its_shard() {
    struct FetchingHandler {
        results: mpsc::UnboundedSender<Result<Vec<String>, String>>,
    }

    #[async_trait]
    impl super::EventHandler for FetchingHandler {
        async fn message_create(&self, ctx: super::Context, _message: Message) {
            let result = ctx
                .fetch_members_with_timeout("42", None, None, Duration::from_secs(5))
                .await
                .map(|members| {
                    members
                        .into_iter()
                        .filter_map(|member| member.user.map(|user| user.id.to_string()))
                        .collect()
                })
                .map_err(|error| error.to_string());
            let _ = self.results.send(result);
        }
    }

    let context = pipeline_test_context((0, 1));
    let mut commands = attach_test_messenger(&context, 0);
    let (results_tx, mut results_rx) = mpsc::unbounded_channel();
    let dispatch = super::DispatchConfig::default();
    assert_eq!(dispatch.mode, super::EventDispatchMode::Serial);
    let pipeline = super::spawn_event_pipeline(
        Arc::new(FetchingHandler {
            results: results_tx,
        }),
        context.clone(),
        #[cfg(feature = "cache")]
        None,
        dispatch,
    );
    let ingress = pipeline.ingress();
    ingress.push(message_create_dispatch("1", "fetch"));

    let nonce = match tokio::time::timeout(Duration::from_secs(5), commands.recv()).await {
        Ok(Some(GatewayCommand::RequestGuildMembers(request))) => {
            request.nonce.expect("fetch_members sets a nonce")
        }
        other => panic!("expected a member request, got {other:?}"),
    };
    // The handler is still awaiting fetch_members. Before the fix these
    // chunks queued behind it, so the fetch could only time out.
    ingress.push(members_chunk_dispatch("42", &nonce, 0, 2, "7"));
    ingress.push(members_chunk_dispatch("42", &nonce, 1, 2, "8"));

    let result = tokio::time::timeout(Duration::from_secs(10), results_rx.recv())
        .await
        .expect("handler finished")
        .expect("handler reported a result");
    assert_eq!(result, Ok(vec!["7".to_string(), "8".to_string()]));
    assert!(context
        .cache
        .member(&Snowflake::from("42"), &Snowflake::from("8"))
        .await
        .is_some());
    assert!(super::lock_member_chunk_waiters(&context.member_chunk_waiters).is_empty());

    drop(ingress);
    pipeline.shutdown(Duration::from_secs(5)).await.unwrap();
}

#[cfg(feature = "collectors")]
#[tokio::test]
async fn serial_handler_can_await_a_collector_for_later_events() {
    struct CollectingHandler {
        subscribed: std::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
        collected: mpsc::UnboundedSender<Option<String>>,
    }

    #[async_trait]
    impl super::EventHandler for CollectingHandler {
        async fn message_create(&self, ctx: super::Context, message: Message) {
            if message.content != "first" {
                return;
            }
            let mut collector = ctx
                .collectors()
                .message_collector()
                .timeout(std::time::Duration::from_secs(5));
            if let Some(subscribed) = self.subscribed.lock().unwrap().take() {
                let _ = subscribed.send(());
            }
            let next = collector.next().await.map(|message| message.content);
            let _ = self.collected.send(next);
        }
    }

    let context = pipeline_test_context((0, 1));
    let (subscribed_tx, subscribed_rx) = tokio::sync::oneshot::channel();
    let (collected_tx, mut collected_rx) = mpsc::unbounded_channel();
    let pipeline = super::spawn_event_pipeline(
        Arc::new(CollectingHandler {
            subscribed: std::sync::Mutex::new(Some(subscribed_tx)),
            collected: collected_tx,
        }),
        context,
        #[cfg(feature = "cache")]
        None,
        super::DispatchConfig::default(),
    );
    let ingress = pipeline.ingress();
    ingress.push(message_create_dispatch("1", "first"));
    tokio::time::timeout(Duration::from_secs(5), subscribed_rx)
        .await
        .expect("handler subscribed")
        .unwrap();
    ingress.push(message_create_dispatch("2", "second"));

    let collected = tokio::time::timeout(Duration::from_secs(10), collected_rx.recv())
        .await
        .expect("handler finished")
        .expect("handler reported");
    assert_eq!(collected.as_deref(), Some("second"));

    drop(ingress);
    pipeline.shutdown(Duration::from_secs(5)).await.unwrap();
}

/// Shared observations of the gated handlers below.
#[derive(Default)]
struct HandlerProbe {
    in_flight: std::sync::atomic::AtomicUsize,
    max_in_flight: std::sync::atomic::AtomicUsize,
    finished: std::sync::Mutex<Vec<String>>,
}

impl HandlerProbe {
    fn enter(&self) {
        let now = self
            .in_flight
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;
        self.max_in_flight
            .fetch_max(now, std::sync::atomic::Ordering::SeqCst);
    }

    fn exit(&self, content: String) {
        self.finished.lock().unwrap().push(content);
        self.in_flight
            .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
    }

    fn max_in_flight(&self) -> usize {
        self.max_in_flight.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn finished(&self) -> Vec<String> {
        self.finished.lock().unwrap().clone()
    }
}

/// Reports each started call, then blocks every call whose content is in
/// `gated` (or every call when `gated` is empty) on `gate`.
struct GatedHandler {
    probe: Arc<HandlerProbe>,
    started: mpsc::UnboundedSender<String>,
    gate: Arc<tokio::sync::Semaphore>,
    gated: Vec<&'static str>,
}

#[async_trait]
impl super::EventHandler for GatedHandler {
    async fn message_create(&self, _ctx: super::Context, message: Message) {
        self.probe.enter();
        let _ = self.started.send(message.content.clone());
        if self.gated.is_empty() || self.gated.contains(&message.content.as_str()) {
            self.gate.acquire().await.expect("gate open").forget();
        }
        tokio::task::yield_now().await;
        self.probe.exit(message.content);
    }
}

async fn next_started(started: &mut mpsc::UnboundedReceiver<String>) -> String {
    tokio::time::timeout(Duration::from_secs(5), started.recv())
        .await
        .expect("a handler call started")
        .expect("handler alive")
}

#[tokio::test]
async fn serial_mode_runs_one_handler_at_a_time_while_internal_processing_runs_ahead() {
    let probe = Arc::new(HandlerProbe::default());
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let (started_tx, mut started_rx) = mpsc::unbounded_channel();
    let context = pipeline_test_context((0, 1));
    let pipeline = super::spawn_event_pipeline(
        Arc::new(GatedHandler {
            probe: Arc::clone(&probe),
            started: started_tx,
            gate: Arc::clone(&gate),
            gated: vec!["first"],
        }),
        context.clone(),
        #[cfg(feature = "cache")]
        None,
        super::DispatchConfig::default(),
    );
    let ingress = pipeline.ingress();
    for (id, content) in [("1", "first"), ("2", "second"), ("3", "third")] {
        ingress.push(message_create_dispatch(id, content));
    }

    assert_eq!(next_started(&mut started_rx).await, "first");
    // The first handler is blocked, yet later events reach the cache.
    wait_for_cached_message(&context.cache, "3").await;
    assert!(
        started_rx.try_recv().is_err(),
        "Serial mode must not start the next handler while one is running"
    );

    gate.add_permits(1);
    assert_eq!(next_started(&mut started_rx).await, "second");
    assert_eq!(next_started(&mut started_rx).await, "third");
    drop(ingress);
    pipeline.shutdown(Duration::from_secs(5)).await.unwrap();

    assert_eq!(probe.finished(), vec!["first", "second", "third"]);
    assert_eq!(probe.max_in_flight(), 1);
}

#[tokio::test]
async fn concurrent_mode_caps_in_flight_handlers_per_shard() {
    let probe = Arc::new(HandlerProbe::default());
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let (started_tx, mut started_rx) = mpsc::unbounded_channel();
    let pipeline = super::spawn_event_pipeline(
        Arc::new(GatedHandler {
            probe: Arc::clone(&probe),
            started: started_tx,
            gate: Arc::clone(&gate),
            gated: Vec::new(),
        }),
        pipeline_test_context((0, 1)),
        #[cfg(feature = "cache")]
        None,
        super::DispatchConfig {
            mode: super::EventDispatchMode::Concurrent,
            max_concurrent_handlers: 2,
            ..super::DispatchConfig::default()
        },
    );
    let ingress = pipeline.ingress();
    for id in 1..=5 {
        ingress.push(message_create_dispatch(&id.to_string(), &format!("m{id}")));
    }

    next_started(&mut started_rx).await;
    next_started(&mut started_rx).await;
    for _ in 0..50 {
        tokio::task::yield_now().await;
    }
    assert!(
        started_rx.try_recv().is_err(),
        "a third handler started while two were running"
    );

    gate.add_permits(5);
    for _ in 0..3 {
        next_started(&mut started_rx).await;
    }
    drop(ingress);
    pipeline.shutdown(Duration::from_secs(5)).await.unwrap();

    assert_eq!(probe.finished().len(), 5);
    assert_eq!(probe.max_in_flight(), 2);
}

#[tokio::test]
async fn full_handler_queue_drops_deliveries_but_keeps_internal_state_current() {
    let probe = Arc::new(HandlerProbe::default());
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let (started_tx, mut started_rx) = mpsc::unbounded_channel();
    let context = pipeline_test_context((0, 1));
    let pipeline = super::spawn_event_pipeline(
        Arc::new(GatedHandler {
            probe: Arc::clone(&probe),
            started: started_tx,
            gate: Arc::clone(&gate),
            gated: vec!["m1"],
        }),
        context.clone(),
        #[cfg(feature = "cache")]
        None,
        super::DispatchConfig {
            queue_capacity: 2,
            ..super::DispatchConfig::default()
        },
    );
    let ingress = pipeline.ingress();
    ingress.push(message_create_dispatch("1", "m1"));
    assert_eq!(next_started(&mut started_rx).await, "m1");

    // m1 is running; m2 and m3 fill the handler queue; m4..m6 overflow it.
    // Each event is processed before the next is pushed, so only the
    // handler queue (not the equally small ingress queue) overflows.
    for id in 2..=6 {
        let id = id.to_string();
        ingress.push(message_create_dispatch(&id, &format!("m{id}")));
        wait_for_cached_message(&context.cache, &id).await;
    }
    assert_eq!(pipeline.handler_drops.dropped(), 3);

    gate.add_permits(1);
    drop(ingress);
    pipeline.shutdown(Duration::from_secs(5)).await.unwrap();
    assert_eq!(probe.finished(), vec!["m1", "m2", "m3"]);
}

#[tokio::test]
async fn ingress_overflow_raises_a_shard_fault_without_waiting() {
    let probe = Arc::new(HandlerProbe::default());
    let (started_tx, _started_rx) = mpsc::unbounded_channel();
    let context = pipeline_test_context((0, 1));
    let pipeline = super::spawn_event_pipeline(
        Arc::new(GatedHandler {
            probe: Arc::clone(&probe),
            started: started_tx,
            gate: Arc::new(tokio::sync::Semaphore::new(0)),
            gated: vec!["never"],
        }),
        context.clone(),
        #[cfg(feature = "cache")]
        None,
        super::DispatchConfig {
            queue_capacity: 4,
            ..super::DispatchConfig::default()
        },
    );
    let fault = pipeline.fault();
    let ingress = pipeline.ingress();
    // This current-thread test never yields here, so the processor cannot
    // drain: pushes past the capacity must return immediately (the gateway
    // read loop also handles heartbeats). The first lost dispatch is a gap
    // the gateway can no longer replay, so it faults the shard; everything
    // after it is dropped too, leaving a consistent prefix.
    for id in 1..=10 {
        ingress.push(message_create_dispatch(&id.to_string(), &format!("m{id}")));
    }
    assert!(fault.is_raised());
    assert!(
        fault.reason().is_some_and(
            |reason| reason.contains("overflowed") && reason.contains("MESSAGE_CREATE")
        ),
        "{:?}",
        fault.reason()
    );
    assert_eq!(
        ingress.dropped.load(std::sync::atomic::Ordering::Relaxed),
        6
    );

    drop(ingress);
    pipeline.shutdown(Duration::from_secs(5)).await.unwrap();
    // The prefix before the gap was still processed and delivered.
    assert_eq!(probe.finished(), vec!["m1", "m2", "m3", "m4"]);
    assert!(context
        .cache
        .message(&Snowflake::from("500"), &Snowflake::from("5"))
        .await
        .is_none());
}

#[tokio::test]
async fn handler_panics_are_contained_and_later_events_still_run() {
    struct PanickingHandler {
        probe: Arc<HandlerProbe>,
    }

    #[async_trait]
    impl super::EventHandler for PanickingHandler {
        async fn message_create(&self, _ctx: super::Context, message: Message) {
            if message.content == "boom" {
                panic!("handler failure under test");
            }
            self.probe.enter();
            self.probe.exit(message.content);
        }
    }

    let probe = Arc::new(HandlerProbe::default());
    let pipeline = super::spawn_event_pipeline(
        Arc::new(PanickingHandler {
            probe: Arc::clone(&probe),
        }),
        pipeline_test_context((0, 1)),
        #[cfg(feature = "cache")]
        None,
        super::DispatchConfig::default(),
    );
    let ingress = pipeline.ingress();
    ingress.push(message_create_dispatch("1", "boom"));
    ingress.push(message_create_dispatch("2", "after"));
    drop(ingress);
    pipeline.shutdown(Duration::from_secs(5)).await.unwrap();
    assert_eq!(probe.finished(), vec!["after"]);
}

#[tokio::test]
async fn pipeline_shutdown_aborts_handlers_that_outlive_the_timeout() {
    struct DropFlag(Arc<std::sync::atomic::AtomicBool>);
    impl Drop for DropFlag {
        fn drop(&mut self) {
            self.0.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    struct StuckHandler {
        started: mpsc::UnboundedSender<()>,
        dropped: Arc<std::sync::atomic::AtomicBool>,
    }

    #[async_trait]
    impl super::EventHandler for StuckHandler {
        async fn message_create(&self, _ctx: super::Context, _message: Message) {
            let _flag = DropFlag(Arc::clone(&self.dropped));
            let _ = self.started.send(());
            std::future::pending::<()>().await;
        }
    }

    let dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (started_tx, mut started_rx) = mpsc::unbounded_channel();
    let pipeline = super::spawn_event_pipeline(
        Arc::new(StuckHandler {
            started: started_tx,
            dropped: Arc::clone(&dropped),
        }),
        pipeline_test_context((0, 1)),
        #[cfg(feature = "cache")]
        None,
        super::DispatchConfig::default(),
    );
    let ingress = pipeline.ingress();
    ingress.push(message_create_dispatch("1", "stuck"));
    tokio::time::timeout(Duration::from_secs(5), started_rx.recv())
        .await
        .expect("handler started");
    drop(ingress);

    tokio::time::timeout(
        Duration::from_secs(5),
        pipeline.shutdown(Duration::from_millis(20)),
    )
    .await
    .expect("shutdown returns once the timeout expires")
    .unwrap();
    // Shutdown aborts and joins handler tasks before returning.
    assert!(
        dropped.load(std::sync::atomic::Ordering::SeqCst),
        "the stuck handler was aborted and joined, not detached"
    );
}

#[tokio::test]
async fn fetch_members_unregisters_its_waiter_on_timeout_cancellation_and_send_failure() {
    let context = pipeline_test_context((0, 1));
    let mut commands = attach_test_messenger(&context, 0);
    let waiter_count = || super::lock_member_chunk_waiters(&context.member_chunk_waiters).len();

    let error = context
        .fetch_members_with_timeout("42", None, None, Duration::from_millis(10))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("timed out"), "{error}");
    assert_eq!(waiter_count(), 0);
    assert!(matches!(
        commands.recv().await,
        Some(GatewayCommand::RequestGuildMembers(_))
    ));

    let pending = tokio::spawn({
        let context = context.clone();
        async move { context.fetch_members("42", None, None).await }
    });
    assert!(matches!(
        commands.recv().await,
        Some(GatewayCommand::RequestGuildMembers(_))
    ));
    assert_eq!(waiter_count(), 1);
    pending.abort();
    assert!(pending.await.unwrap_err().is_cancelled());
    assert_eq!(
        waiter_count(),
        0,
        "a cancelled fetch must not leak its waiter"
    );

    drop(commands);
    let error = context.fetch_members("42", None, None).await.unwrap_err();
    assert!(
        error.to_string().contains("failed to send gateway command"),
        "{error}"
    );
    assert_eq!(waiter_count(), 0);
}

#[tokio::test]
async fn stopping_a_shard_fails_its_pending_fetch_members_immediately() {
    let runtime = super::SharedRuntime::new(
        "token",
        1,
        super::TypeMap::new(),
        CacheConfig::default(),
        None,
    );
    let context = runtime.context((0, 1));
    let mut commands = attach_test_messenger(&context, 0);
    let registration = super::ShardRegistration {
        shard_id: 0,
        gateway_commands: Arc::clone(&runtime.gateway_commands),
        member_chunk_waiters: Arc::clone(&runtime.member_chunk_waiters),
    };
    let pending = tokio::spawn(async move { context.fetch_members("42", None, None).await });
    assert!(matches!(
        commands.recv().await,
        Some(GatewayCommand::RequestGuildMembers(_))
    ));

    drop(registration);
    let error = tokio::time::timeout(Duration::from_secs(5), pending)
        .await
        .expect("fetch fails promptly instead of waiting out 60 seconds")
        .unwrap()
        .unwrap_err();
    assert!(error.to_string().contains("shard stopped"), "{error}");
    assert!(super::read_gateway_commands(&runtime.gateway_commands).is_empty());
}

#[tokio::test]
async fn guild_scoped_commands_go_to_the_shard_that_owns_the_guild() {
    let context = pipeline_test_context((0, 2));
    let mut shard0 = attach_test_messenger(&context, 0);
    let mut shard1 = attach_test_messenger(&context, 1);
    let shard1_guild = Snowflake::from(1_u64 << 22);

    context
        .request_guild_members(crate::model::RequestGuildMembers {
            guild_id: shard1_guild.clone(),
            query: Some(String::new()),
            limit: Some(0),
            presences: None,
            user_ids: None,
            nonce: None,
        })
        .await
        .unwrap();
    context
        .join_voice(shard1_guild.clone(), "9", false, false)
        .await
        .unwrap();
    assert!(matches!(
        shard1.try_recv(),
        Ok(GatewayCommand::RequestGuildMembers(request)) if request.guild_id == shard1_guild
    ));
    assert!(matches!(
        shard1.try_recv(),
        Ok(GatewayCommand::SendPayload(payload)) if payload["op"] == json!(4)
    ));
    assert!(shard0.try_recv().is_err());

    let pending = tokio::spawn({
        let context = context.clone();
        let guild_id = shard1_guild.clone();
        async move {
            context
                .fetch_members_with_timeout(guild_id, None, None, Duration::from_millis(50))
                .await
        }
    });
    match shard1.recv().await {
        Some(GatewayCommand::RequestGuildMembers(request)) => {
            assert!(request.nonce.unwrap().starts_with("drs1-"));
        }
        other => panic!("expected a member request on shard 1, got {other:?}"),
    }
    assert!(pending.await.unwrap().is_err());

    // When the owning shard runs elsewhere, the context's own shard is used.
    super::write_gateway_commands(&context.gateway_commands).remove(&1);
    context
        .update_voice_state(shard1_guild, None, false, false)
        .await
        .unwrap();
    assert!(matches!(
        shard0.try_recv(),
        Ok(GatewayCommand::SendPayload(_))
    ));
}

#[cfg(feature = "cache")]
#[tokio::test]
async fn ready_on_one_shard_keeps_the_guilds_of_other_shards() {
    let cache = crate::cache::CacheHandle::new();
    // (guild_id >> 22) % 2 selects the shard.
    let shard0_guild = (2_u64 << 22).to_string();
    let shard1_guild = (1_u64 << 22).to_string();
    for (guild_id, channel_id, user_id, shard) in [
        (&shard0_guild, "10", "11", (0, 2)),
        (&shard1_guild, "20", "21", (1, 2)),
    ] {
        let guild: crate::model::Guild = serde_json::from_value(json!({
            "id": guild_id,
            "name": "guild",
            "channels": [{ "id": channel_id, "type": 0 }],
            "members": [{ "user": { "id": user_id, "username": "member" }, "roles": [] }]
        }))
        .unwrap();
        super::apply_cache_updates(
            &cache,
            shard,
            &Event::GuildCreate(crate::event::GuildEvent {
                guild,
                raw: json!({}),
            }),
        )
        .await;
    }

    // Shard 1 starts a new session; shard 0's guild must survive.
    super::apply_cache_updates(&cache, (1, 2), &ready_dispatch_event()).await;
    assert!(cache
        .guild(&Snowflake::from(shard0_guild.as_str()))
        .await
        .is_some());
    assert!(cache.channel(&Snowflake::from("10")).await.is_some());
    assert!(cache
        .member(
            &Snowflake::from(shard0_guild.as_str()),
            &Snowflake::from("11")
        )
        .await
        .is_some());
    assert!(cache
        .guild(&Snowflake::from(shard1_guild.as_str()))
        .await
        .is_none());
    assert!(cache.channel(&Snowflake::from("20")).await.is_none());
    assert!(cache
        .member(
            &Snowflake::from(shard1_guild.as_str()),
            &Snowflake::from("21")
        )
        .await
        .is_none());
    assert!(
        cache.user(&Snowflake::from("21")).await.is_some(),
        "users are shared by every shard"
    );
}

#[cfg(feature = "cache")]
#[tokio::test]
async fn cache_backend_writes_keep_gateway_order_when_a_put_is_slow() {
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let recorder = Arc::new(RecordingBackend {
        put_message_gate: Some(Arc::clone(&gate)),
        ..RecordingBackend::default()
    });
    let (forwarder, _worker) = super::CacheBackendForwarder::spawn(recorder.clone(), 16);
    let cache = crate::cache::CacheHandle::new();
    let create = decode_event(
        "MESSAGE_CREATE",
        json!({ "id": "11", "channel_id": "10", "content": "hello" }),
    )
    .unwrap();
    let delete = decode_event("MESSAGE_DELETE", json!({ "id": "11", "channel_id": "10" })).unwrap();

    super::forward_cache_backend_updates(&forwarder, &cache, SINGLE_SHARD, &create).await;
    super::forward_cache_backend_updates(&forwarder, &cache, SINGLE_SHARD, &delete).await;
    for _ in 0..50 {
        tokio::task::yield_now().await;
    }
    // With a task per write, the delete ran here and the stalled put later
    // resurrected the message.
    assert!(
        recorder.calls().is_empty(),
        "the delete overtook the stalled put: {:?}",
        recorder.calls()
    );

    gate.add_permits(1);
    forwarder.flush().await;
    assert_eq!(
        recorder.calls(),
        vec![
            "put_message:10:11".to_string(),
            "delete_message:10:11".to_string()
        ]
    );
    assert!(recorder.stored_message("10", "11").is_none());
}

#[cfg(feature = "cache")]
#[tokio::test]
async fn full_cache_backend_queue_schedules_an_ordered_shard_resync() {
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let recorder = Arc::new(RecordingBackend {
        put_message_gate: Some(Arc::clone(&gate)),
        ..RecordingBackend::default()
    });
    let (forwarder, _worker) = super::CacheBackendForwarder::spawn(recorder.clone(), 1);
    let health = forwarder.health_handle();
    let message = |id: &str| {
        super::BackendWrite::PutMessage(Box::new(Message {
            id: Snowflake::from(id),
            channel_id: Snowflake::from("10"),
            ..Message::default()
        }))
    };
    // Current-thread test without yields: the worker cannot drain between
    // these calls, so only the first write fits. The dropped delete must not
    // be lost: it turns into a resync of shard 1/2 queued behind the put.
    forwarder.forward((1, 2), message("1"));
    forwarder.forward(
        (1, 2),
        super::BackendWrite::DeleteMessages {
            channel_id: Snowflake::from("10"),
            message_ids: vec![Snowflake::from("1")],
        },
    );
    forwarder.forward((0, 2), message("3"));
    let unhealthy = health.health();
    assert!(!unhealthy.healthy);
    assert_eq!(unhealthy.dropped_writes, 2);
    assert_eq!(unhealthy.pending_resync_shards, vec![(0, 2), (1, 2)]);

    gate.add_permits(1);
    forwarder.flush().await;
    // The put that was queued before the loss lands first, then both
    // shards are cleared: message 1 cannot survive its dropped delete.
    assert_eq!(
        recorder.calls(),
        vec![
            "put_message:10:1".to_string(),
            "clear_shard_cache:0:2".to_string(),
            "clear_shard_cache:1:2".to_string(),
        ]
    );
    let healed = health.health();
    assert!(healed.healthy);
    assert_eq!(healed.resyncs, 2);
    assert!(healed.pending_resync_shards.is_empty());
}

#[cfg(feature = "cache")]
#[tokio::test]
async fn cache_backend_message_update_merges_instead_of_overwriting_with_a_partial() {
    let recorder = Arc::new(RecordingBackend::default());
    let (forwarder, _worker) = super::CacheBackendForwarder::spawn(recorder.clone(), 16);
    // The in-memory cache never saw these messages: the backend's stored
    // copy is the merge base.
    let cache = crate::cache::CacheHandle::new();
    let full_message = json!({
        "id": "11",
        "channel_id": "10",
        "content": "before",
        "author": { "id": "5", "username": "author" },
        "embeds": [{ "title": "kept" }],
        "components": [{ "type": 1, "components": [{ "type": 2, "custom_id": "go", "style": 1 }] }]
    });
    let create = decode_event("MESSAGE_CREATE", full_message).unwrap();
    let partial_update = decode_event(
        "MESSAGE_UPDATE",
        json!({ "id": "11", "channel_id": "10", "content": "after" }),
    )
    .unwrap();
    let unknown_update = decode_event(
        "MESSAGE_UPDATE",
        json!({ "id": "99", "channel_id": "10", "content": "orphan" }),
    )
    .unwrap();
    for event in [&create, &partial_update, &unknown_update] {
        super::forward_cache_backend_updates(&forwarder, &cache, SINGLE_SHARD, event).await;
    }
    forwarder.flush().await;

    let stored = recorder.stored_message("10", "11").expect("message stored");
    assert_eq!(stored.content, "after");
    assert_eq!(stored.author.expect("author kept").username, "author");
    assert_eq!(stored.embeds[0].title.as_deref(), Some("kept"));
    assert_eq!(stored.components.expect("components kept").len(), 1);
    assert!(
        recorder.stored_message("10", "99").is_none(),
        "a partial update for an unknown message must not be stored as complete"
    );
    assert_eq!(
        recorder.calls(),
        vec![
            "put_message:10:11".to_string(),
            "put_message:10:11".to_string()
        ]
    );

    // Backend lacks the message but the in-memory cache has it: forward the
    // in-memory merged copy.
    let in_memory = decode_event(
        "MESSAGE_CREATE",
        json!({
            "id": "12",
            "channel_id": "10",
            "content": "memory",
            "author": { "id": "6", "username": "cached" }
        }),
    )
    .unwrap();
    super::apply_cache_updates(&cache, SINGLE_SHARD, &in_memory).await;
    let memory_update = decode_event(
        "MESSAGE_UPDATE",
        json!({ "id": "12", "channel_id": "10", "content": "memory-edited" }),
    )
    .unwrap();
    super::apply_cache_updates(&cache, SINGLE_SHARD, &memory_update).await;
    super::forward_cache_backend_updates(&forwarder, &cache, SINGLE_SHARD, &memory_update).await;
    forwarder.flush().await;
    let stored = recorder
        .stored_message("10", "12")
        .expect("in-memory copy used");
    assert_eq!(stored.content, "memory-edited");
    assert_eq!(stored.author.expect("author kept").username, "cached");
}

/// A MESSAGE_UPDATE payload carrying every message field Discord can send.
fn full_message_update_payload() -> Value {
    let mut payload = json!({
        "id": "1",
        "channel_id": "2",
        "guild_id": "3",
        "author": { "id": "4", "username": "author" },
        "member": { "nick": "nick", "roles": ["6"] },
        "content": "full",
        "timestamp": "2026-10-10T00:00:00+00:00",
        "edited_timestamp": "2026-10-10T00:01:00+00:00",
        "mentions": [{ "id": "5", "username": "mentioned" }],
        "mention_roles": ["6"],
        "attachments": [{ "id": "7", "filename": "a.txt" }],
        "type": 19,
        "pinned": true,
        "tts": true,
        "flags": 4,
        "webhook_id": "8",
        "application_id": "9",
        "application": { "id": "9", "name": "app" },
        "embeds": [{ "title": "embed" }],
        "reactions": [{ "count": 2, "me": true }],
        "activity": { "type": 1, "party_id": "party" },
        "mention_everyone": true
    });
    // Split in two so the json! macro stays under the recursion limit.
    let rest = json!({
        "mention_channels": [{ "id": "10", "guild_id": "3", "type": 0, "name": "general" }],
        "sticker_items": [{ "id": "11", "name": "sticker", "format_type": 1 }],
        "stickers": [{ "id": "12", "name": "legacy", "type": 1, "format_type": 1 }],
        "position": 5,
        "role_subscription_data": {
            "role_subscription_listing_id": "13",
            "tier_name": "gold",
            "total_months_subscribed": 2,
            "is_renewal": true
        },
        "message_reference": { "type": 0, "message_id": "14", "channel_id": "2" },
        "message_snapshots": [{ "message": { "content": "snapshot" } }],
        "referenced_message": { "id": "14", "channel_id": "2", "content": "parent" },
        "interaction_metadata": { "id": "15", "type": 2, "user": { "id": "4", "username": "author" } },
        "interaction": { "id": "15", "type": 2, "name": "cmd" },
        "resolved": { "users": { "4": { "id": "4", "username": "author" } } },
        "components": [{
            "type": 1,
            "components": [{ "type": 2, "custom_id": "button", "style": 1, "label": "Go" }]
        }],
        "poll": {
            "question": { "text": "q?" },
            "answers": [{ "answer_id": 1, "poll_media": { "text": "a" } }],
            "allow_multiselect": true,
            "layout_type": 1
        },
        "call": { "participants": ["4"], "ended_timestamp": "2026-10-10T00:02:00+00:00" },
        "shared_client_theme": { "colors": ["ff0000"], "gradient_angle": 90, "base_mix": 50 }
    });
    if let (Some(fields), Value::Object(rest)) = (payload.as_object_mut(), rest) {
        fields.extend(rest);
    }
    payload
}

#[test]
fn merge_message_update_applies_every_field_present_in_the_payload() {
    let payload = full_message_update_payload();
    let partial: Message = serde_json::from_value(payload.clone()).unwrap();
    let cached = Message {
        id: Snowflake::from("1"),
        channel_id: Snowflake::from("2"),
        content: "stale".to_string(),
        ..Message::default()
    };

    let merged = super::merge_message_update(
        cached,
        partial.clone(),
        &super::MessageUpdateFields::from_raw(&payload),
    );
    // Any field the merge skipped would still hold the cached default.
    assert_eq!(
        serde_json::to_value(&merged).unwrap(),
        serde_json::to_value(&partial).unwrap()
    );
    let serialized = serde_json::to_value(&partial).unwrap();
    for key in payload.as_object().unwrap().keys() {
        assert!(
            serialized.get(key).is_some(),
            "fixture field {key} does not survive decoding, so the check above cannot see it"
        );
    }
}

#[test]
fn merge_message_update_keeps_fields_the_payload_omits() {
    let full: Message = serde_json::from_value(full_message_update_payload()).unwrap();
    let sparse = json!({ "id": "1", "channel_id": "2", "content": "edited", "poll": null });
    let merged = super::merge_message_update(
        full.clone(),
        serde_json::from_value(sparse.clone()).unwrap(),
        &super::MessageUpdateFields::from_raw(&sparse),
    );

    let mut expected = full;
    expected.content = "edited".to_string();
    // An explicit null clears the field.
    expected.poll = None;
    assert_eq!(
        serde_json::to_value(&merged).unwrap(),
        serde_json::to_value(&expected).unwrap()
    );
    assert!(merged.components.is_some());
    assert!(merged.call.is_some());
}

#[test]
fn dispatch_config_maps_modes_to_handler_permits_and_warning_thresholds() {
    let serial = super::DispatchConfig::default();
    assert_eq!(serial.handler_permits(), 1);
    assert_eq!(serial.backlog_warn_threshold(), 5_000);

    let concurrent = super::DispatchConfig {
        mode: super::EventDispatchMode::Concurrent,
        max_concurrent_handlers: 0,
        queue_capacity: 3,
        ..super::DispatchConfig::default()
    };
    assert_eq!(concurrent.handler_permits(), 1);
    assert_eq!(concurrent.backlog_warn_threshold(), 1);

    let builder = super::Client::builder("token", Intents::from_bits(0))
        .event_dispatch(super::EventDispatchMode::Concurrent)
        .event_queue_capacity(0)
        .max_concurrent_handlers(0)
        .handler_shutdown_timeout(Duration::from_millis(250))
        .interaction_queue_capacity(0)
        .max_concurrent_interactions(0)
        .interaction_admission_timeout(Duration::from_millis(1500))
        .on_interaction_overload(|_| {});
    let dispatch = builder.dispatch;
    assert_eq!(dispatch.mode, super::EventDispatchMode::Concurrent);
    assert_eq!(dispatch.queue_capacity, 1);
    assert_eq!(dispatch.max_concurrent_handlers, 1);
    assert_eq!(dispatch.shutdown_timeout, Duration::from_millis(250));
    assert_eq!(dispatch.interaction_queue_capacity, 1);
    assert_eq!(dispatch.max_concurrent_interactions, 1);
    assert_eq!(
        dispatch.interaction_admission_timeout,
        Duration::from_millis(1500)
    );
    assert!(dispatch.interaction_overload.is_some());
}

#[test]
fn overflow_log_reports_the_first_drop_then_rate_limits() {
    let log = super::OverflowLog::default();
    assert_eq!(log.record(), Some(1));
    assert_eq!(log.record(), None);
    assert_eq!(log.dropped(), 2);

    let mut warning = super::BacklogWarning::default();
    assert!(warning.due());
    assert!(!warning.due());
}

/// Sets a flag when dropped, so tests can tell an aborted task from a
/// detached one.
struct TaskDropFlag(Arc<std::sync::atomic::AtomicBool>);

impl Drop for TaskDropFlag {
    fn drop(&mut self) {
        self.0.store(true, std::sync::atomic::Ordering::SeqCst);
    }
}

async fn wait_for_flag(flag: &std::sync::atomic::AtomicBool, what: &str) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !flag.load(std::sync::atomic::Ordering::SeqCst) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{what}"));
}

type FakeGatewaySocket = tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>;

/// A local gateway that sends HELLO, waits for IDENTIFY, and hands each
/// connection plus its IDENTIFY payload to `script`.
async fn spawn_fake_gateway<F, Fut>(script: F) -> (String, JoinHandleUnit)
where
    F: Fn(FakeGatewaySocket, Value) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message as WsMessage;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}/", listener.local_addr().unwrap());
    let script = Arc::new(script);
    let server = tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let script = Arc::clone(&script);
            tokio::spawn(async move {
                let Ok(mut socket) = tokio_tungstenite::accept_async(stream).await else {
                    return;
                };
                let hello = json!({ "op": 10, "d": { "heartbeat_interval": 60_000 } });
                if socket
                    .send(WsMessage::Text(hello.to_string().into()))
                    .await
                    .is_err()
                {
                    return;
                }
                // Heartbeats may race IDENTIFY; skip anything but op 2.
                let identify = loop {
                    match socket.next().await {
                        Some(Ok(WsMessage::Text(text))) => {
                            let payload: Value = serde_json::from_str(&text).unwrap();
                            if payload["op"] == json!(2) {
                                break payload;
                            }
                        }
                        Some(Ok(_)) => {}
                        _ => return,
                    }
                };
                script(socket, identify).await;
            });
        }
    });
    (url, server)
}

type JoinHandleUnit = tokio::task::JoinHandle<()>;

async fn send_dispatch(socket: &mut FakeGatewaySocket, sequence: u64, name: &str, data: Value) {
    use futures_util::SinkExt;
    let payload = json!({ "op": 0, "s": sequence, "t": name, "d": data });
    socket
        .send(tokio_tungstenite::tungstenite::Message::Text(
            payload.to_string().into(),
        ))
        .await
        .unwrap();
}

async fn send_close(socket: &mut FakeGatewaySocket, code: u16) {
    use futures_util::SinkExt;
    use tokio_tungstenite::tungstenite::protocol::{frame::coding::CloseCode, CloseFrame};
    let _ = socket
        .send(tokio_tungstenite::tungstenite::Message::Close(Some(
            CloseFrame {
                code: CloseCode::from(code),
                reason: "closed by test gateway".into(),
            },
        )))
        .await;
}

fn fake_ready_payload() -> Value {
    json!({
        "v": 10,
        "user": { "id": "1", "username": "bot" },
        "session_id": "session",
        "guilds": [],
        "application": { "id": "1", "flags": 0 }
    })
}

#[cfg(feature = "sharding")]
#[tokio::test]
async fn start_returns_a_terminal_close_error_without_waiting_for_a_stuck_handler() {
    struct StuckHandler {
        started: Arc<tokio::sync::Notify>,
        dropped: Arc<std::sync::atomic::AtomicBool>,
    }

    #[async_trait]
    impl super::EventHandler for StuckHandler {
        async fn message_create(&self, _ctx: super::Context, _message: Message) {
            let _flag = TaskDropFlag(Arc::clone(&self.dropped));
            self.started.notify_one();
            std::future::pending::<()>().await;
        }
    }

    let started = Arc::new(tokio::sync::Notify::new());
    let dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let handler_started = Arc::clone(&started);
    let (url, server) = spawn_fake_gateway(move |mut socket, _identify| {
        let handler_started = Arc::clone(&handler_started);
        async move {
            send_dispatch(&mut socket, 1, "READY", fake_ready_payload()).await;
            send_dispatch(
                &mut socket,
                2,
                "MESSAGE_CREATE",
                json!({ "id": "5", "channel_id": "6", "content": "hang" }),
            )
            .await;
            handler_started.notified().await;
            send_close(&mut socket, 4004).await;
        }
    })
    .await;

    let runtime = super::SharedRuntime::new(
        "token",
        0,
        super::TypeMap::new(),
        CacheConfig::default(),
        None,
    );
    let shard = super::start_gateway_shard(
        super::ShardLaunch {
            token: "token".to_string(),
            intents: 0,
            handler: Arc::new(StuckHandler {
                started,
                dropped: Arc::clone(&dropped),
            }),
            runtime: runtime.clone(),
            gateway_config: crate::ws::GatewayConnectionConfig::new(url),
            shard: (0, 1),
            presence: None,
            dispatch: super::DispatchConfig {
                shutdown_timeout: Duration::from_millis(50),
                ..super::DispatchConfig::default()
            },
        },
        super::ShardStartControl {
            supervisor_channels: None,
            boot_gate: None,
        },
    );
    let error = tokio::time::timeout(Duration::from_secs(10), shard)
        .await
        .expect("start returns promptly after the gateway stops")
        .expect_err("a terminal close code is an error for the caller");

    assert!(error.to_string().contains("4004"), "{error}");
    assert!(
        dropped.load(std::sync::atomic::Ordering::SeqCst),
        "the stuck handler was aborted and joined before start returned"
    );
    assert!(super::read_gateway_commands(&runtime.gateway_commands).is_empty());
    server.abort();
}

#[cfg(feature = "sharding")]
#[tokio::test]
async fn shard_supervisor_returns_the_first_fatal_error_and_stops_the_other_shards() {
    use futures_util::StreamExt;

    let (closed_tx, mut closed_rx) = mpsc::unbounded_channel();
    let (url, server) = spawn_fake_gateway(move |mut socket, identify| {
        let closed_tx = closed_tx.clone();
        async move {
            match identify["d"]["shard"][0].as_u64() {
                // Shard 1 is rejected outright.
                Some(1) => send_close(&mut socket, 4004).await,
                // Shard 0 stays healthy until the client closes it.
                _ => {
                    send_dispatch(&mut socket, 1, "READY", fake_ready_payload()).await;
                    while let Some(Ok(message)) = socket.next().await {
                        if let tokio_tungstenite::tungstenite::Message::Close(frame) = message {
                            let _ = closed_tx.send(frame.map(|frame| frame.reason.to_string()));
                            return;
                        }
                    }
                }
            }
        }
    })
    .await;

    let runtime = super::SharedRuntime::new(
        "token",
        0,
        super::TypeMap::new(),
        CacheConfig::default(),
        None,
    );
    let supervisor = super::spawn_shard_supervisor(super::SpawnShardSupervisorConfig {
        token: "token".to_string(),
        intents: 0,
        handler: Arc::new(NoopEventHandler),
        runtime,
        gateway_config: crate::ws::GatewayConnectionConfig::new(url),
        total_shards: 2,
        boot_window_size: 2,
        boot_delay: super::SHARD_BOOT_DELAY,
        initial_delay: None,
        presence: None,
        dispatch: super::DispatchConfig {
            shutdown_timeout: Duration::from_millis(50),
            ..super::DispatchConfig::default()
        },
        background: None,
    })
    .await
    .unwrap();

    let error = tokio::time::timeout(Duration::from_secs(10), supervisor.wait())
        .await
        .expect("the shard 1 failure surfaces while shard 0 is still connected")
        .expect_err("shard 1's terminal close is returned");
    assert!(error.to_string().contains("4004"), "{error}");

    let reason = tokio::time::timeout(Duration::from_secs(5), closed_rx.recv())
        .await
        .expect("shard 0 was shut down")
        .expect("close observed");
    assert_eq!(reason.as_deref(), Some("supervisor shutdown"));
    server.abort();
}

struct NoopEventHandler;

#[async_trait]
impl super::EventHandler for NoopEventHandler {}

#[cfg(feature = "sharding")]
#[tokio::test]
async fn shard_supervisor_wait_surfaces_a_later_failure_and_aborts_unresponsive_shards() {
    let manager = Arc::new(StdMutex::new(ShardingManager::new(ShardConfig::new(2))));
    let runtime0 = manager
        .lock()
        .expect("manager mutex poisoned")
        .prepare_runtime(0)
        .unwrap();
    let dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = TaskDropFlag(Arc::clone(&dropped));
    let supervisor = ShardSupervisor {
        manager: Arc::clone(&manager),
        tasks: vec![
            (
                0,
                tokio::spawn(async move {
                    let _flag = flag;
                    // Ignores Shutdown, like a shard stuck in a handler.
                    std::future::pending::<()>().await;
                    Ok(())
                }),
            ),
            (
                1,
                tokio::spawn(async {
                    Err(crate::error::DiscordError::gateway(
                        "4004 authentication failed",
                    ))
                }),
            ),
        ],
        stop_timeout: Duration::from_millis(20),
        boot_scheduler: None,
        background: None,
    };

    let error = tokio::time::timeout(Duration::from_secs(5), supervisor.wait())
        .await
        .expect("shard 1's failure is not hidden behind the still-running shard 0")
        .unwrap_err();
    assert!(error.to_string().contains("4004"), "{error}");
    assert_eq!(
        runtime0.command_rx.try_recv().unwrap(),
        crate::sharding::ShardIpcMessage::Shutdown
    );
    assert!(
        dropped.load(std::sync::atomic::Ordering::SeqCst),
        "shard 0 was aborted and joined after the stop timeout"
    );
}

#[cfg(feature = "sharding")]
#[tokio::test]
async fn cancelling_shard_supervisor_wait_aborts_the_running_shards() {
    let dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = TaskDropFlag(Arc::clone(&dropped));
    let supervisor = ShardSupervisor {
        manager: Arc::new(StdMutex::new(ShardingManager::new(ShardConfig::new(1)))),
        tasks: vec![(
            0,
            tokio::spawn(async move {
                let _flag = flag;
                std::future::pending::<()>().await;
                Ok(())
            }),
        )],
        stop_timeout: Duration::from_secs(1),
        boot_scheduler: None,
        background: None,
    };

    assert!(
        tokio::time::timeout(Duration::from_millis(20), supervisor.wait())
            .await
            .is_err()
    );
    wait_for_flag(&dropped, "a cancelled wait must not leave shards detached").await;
}

#[cfg(feature = "sharding")]
#[tokio::test]
async fn forward_shard_commands_waits_for_queue_space_instead_of_giving_up() {
    let (command_tx, command_rx) = std::sync::mpsc::channel();
    let (gateway_tx, mut gateway_rx) = mpsc::channel(1);
    super::forward_shard_commands(command_rx, gateway_tx);

    for status in ["one", "two"] {
        command_tx
            .send(crate::sharding::ShardIpcMessage::UpdatePresence(
                status.to_string(),
            ))
            .unwrap();
    }
    command_tx
        .send(crate::sharding::ShardIpcMessage::Shutdown)
        .unwrap();
    // Let the bridge hit the full queue before anything is consumed; the
    // old try_send gave up here and the Shutdown was lost.
    tokio::time::sleep(Duration::from_millis(20)).await;

    assert!(matches!(
        gateway_rx.recv().await,
        Some(GatewayCommand::UpdatePresence(status)) if status == "one"
    ));
    assert!(matches!(
        gateway_rx.recv().await,
        Some(GatewayCommand::UpdatePresence(status)) if status == "two"
    ));
    assert!(matches!(
        gateway_rx.recv().await,
        Some(GatewayCommand::Shutdown)
    ));
}

#[cfg(feature = "cache")]
#[tokio::test]
async fn cache_backend_message_update_falls_back_to_memory_when_the_backend_read_fails() {
    let recorder = Arc::new(RecordingBackend {
        fail_get_message: true,
        ..RecordingBackend::default()
    });
    let (forwarder, _worker) = super::CacheBackendForwarder::spawn(recorder.clone(), 16);
    let cache = crate::cache::CacheHandle::new();
    let create = decode_event(
        "MESSAGE_CREATE",
        json!({
            "id": "11",
            "channel_id": "10",
            "content": "before",
            "author": { "id": "5", "username": "author" }
        }),
    )
    .unwrap();
    let update = decode_event(
        "MESSAGE_UPDATE",
        json!({ "id": "11", "channel_id": "10", "content": "after" }),
    )
    .unwrap();
    for event in [&create, &update] {
        super::apply_cache_updates(&cache, SINGLE_SHARD, event).await;
    }
    super::forward_cache_backend_updates(&forwarder, &cache, SINGLE_SHARD, &update).await;
    forwarder.flush().await;

    let stored = recorder
        .stored_message("10", "11")
        .expect("fallback stored");
    assert_eq!(stored.content, "after");
    assert_eq!(stored.author.expect("author kept").username, "author");
}

#[tokio::test]
async fn processor_death_raises_a_shard_fault_and_later_dispatches_are_dropped() {
    let pipeline = super::spawn_event_pipeline(
        Arc::new(NoopEventHandler),
        pipeline_test_context((0, 1)),
        #[cfg(feature = "cache")]
        None,
        super::DispatchConfig::default(),
    );
    let fault = pipeline.fault();
    let ingress = pipeline.ingress();
    // Let the pipeline tasks start, then drop the pipeline outside shutdown:
    // that aborts its tasks exactly like a processor that died while the
    // gateway kept running.
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }
    drop(pipeline);
    let reason = tokio::time::timeout(Duration::from_secs(5), fault.raised())
        .await
        .expect("the processor's death is propagated promptly");
    assert!(reason.contains("stopped unexpectedly"), "{reason}");

    ingress.push(message_create_dispatch("1", "late"));
    ingress.push(message_create_dispatch("2", "late"));
    assert_eq!(
        ingress.dropped.load(std::sync::atomic::Ordering::Relaxed),
        2
    );
}

#[cfg(feature = "cache")]
#[tokio::test]
async fn client_builder_start_returns_the_gateway_error_to_its_caller() {
    let (url, server) = spawn_fake_gateway(|mut socket, identify| async move {
        // The builder's presence travels inside IDENTIFY.
        assert_eq!(identify["d"]["presence"]["status"], json!("idle"));
        send_dispatch(&mut socket, 1, "READY", fake_ready_payload()).await;
        send_close(&mut socket, 4014).await;
    })
    .await;
    let recorder = Arc::new(RecordingBackend::default());

    let error = tokio::time::timeout(
        Duration::from_secs(10),
        super::Client::builder("token", Intents::from_bits(0))
            .event_handler(NoopEventHandler)
            .gateway_config(crate::ws::GatewayConnectionConfig::new(url))
            .presence(crate::model::UpdatePresence {
                since: None,
                activities: Vec::new(),
                status: "idle".to_string(),
                afk: false,
            })
            .cache_backend(recorder.clone())
            .handler_shutdown_timeout(Duration::from_millis(50))
            .start(),
    )
    .await
    .expect("start returns once the gateway stops")
    .expect_err("4014 is not reconnectable");
    assert!(error.to_string().contains("4014"), "{error}");
    server.abort();

    // start() joins the backend worker before returning: the READY
    // invalidation is already applied and no write can arrive later.
    assert_eq!(recorder.calls(), vec!["clear_shard_cache:0:1".to_string()]);
}

// ---------------------------------------------------------------------------
// Review round 2: shard consistency faults, cursor collectors, the
// interaction lane, backend resync barriers, and supervised startup.
// ---------------------------------------------------------------------------

#[test]
fn member_chunk_progress_requires_every_chunk_index() {
    fn chunk(index: u64, count: u64, user_id: &str) -> crate::event::GuildMembersChunkPayload {
        crate::event::GuildMembersChunkPayload {
            guild_id: Snowflake::from("42"),
            members: vec![Member {
                user: Some(User {
                    id: Snowflake::from(user_id),
                    ..User::default()
                }),
                ..Member::default()
            }],
            chunk_index: index,
            chunk_count: count,
            not_found: Vec::new(),
            presences: None,
            nonce: Some("n".to_string()),
        }
    }

    // The last index arriving early is not completion.
    let mut progress = super::MemberChunkProgress::default();
    assert!(progress.add(chunk(2, 3, "c")).unwrap().is_none());
    assert!(progress.add(chunk(0, 3, "a")).unwrap().is_none());
    // A repeated chunk does not add its members twice.
    assert!(progress.add(chunk(0, 3, "a")).unwrap().is_none());
    assert_eq!(progress.describe(), "received 2 of 3 chunks");
    let members = progress.add(chunk(1, 3, "b")).unwrap().expect("complete");
    let ids: Vec<_> = members
        .iter()
        .filter_map(|member| member.user.as_ref().map(|user| user.id.to_string()))
        .collect();
    assert_eq!(ids, vec!["c", "a", "b"]);

    let mut inconsistent = super::MemberChunkProgress::default();
    assert!(inconsistent.add(chunk(0, 2, "a")).unwrap().is_none());
    assert!(inconsistent
        .add(chunk(1, 3, "b"))
        .unwrap_err()
        .to_string()
        .contains("chunk_count changed"));
    let mut out_of_range = super::MemberChunkProgress::default();
    assert!(out_of_range
        .add(chunk(5, 2, "a"))
        .unwrap_err()
        .to_string()
        .contains("out of range"));
    assert_eq!(
        super::MemberChunkProgress::default().describe(),
        "no chunks received"
    );
}

#[tokio::test]
async fn fetch_members_fails_instead_of_returning_a_list_with_a_missing_chunk() {
    let context = pipeline_test_context((0, 1));
    let mut commands = attach_test_messenger(&context, 0);
    let pending = tokio::spawn({
        let context = context.clone();
        async move {
            context
                .fetch_members_with_timeout("42", None, None, Duration::from_millis(200))
                .await
        }
    });
    let nonce = match commands.recv().await {
        Some(GatewayCommand::RequestGuildMembers(request)) => request.nonce.unwrap(),
        other => panic!("expected a member request, got {other:?}"),
    };
    // Chunk 1 of 3 is lost; the final index arrives anyway.
    for (index, user) in [(0, "7"), (2, "9")] {
        super::process_gateway_dispatch(
            &context,
            #[cfg(feature = "cache")]
            None,
            members_chunk_dispatch("42", &nonce, index, 3, user),
        )
        .await;
    }
    let error = pending.await.unwrap().unwrap_err();
    assert!(
        error.to_string().contains("received 2 of 3 chunks"),
        "{error}"
    );
}

#[cfg(all(feature = "voice", feature = "sharding"))]
#[tokio::test]
async fn ingress_overflow_stops_the_shard_invalidates_its_cache_and_fails_fetches() {
    let stall = Arc::new(tokio::sync::Notify::new());
    let server_stall = Arc::clone(&stall);
    let (url, server) = spawn_fake_gateway(move |mut socket, _identify| {
        let stall = Arc::clone(&server_stall);
        async move {
            use futures_util::StreamExt;
            send_dispatch(&mut socket, 1, "READY", fake_ready_payload()).await;
            send_dispatch(
                &mut socket,
                2,
                "GUILD_CREATE",
                json!({ "id": "77", "name": "cached", "channels": [{ "id": "78", "type": 0 }] }),
            )
            .await;
            // The test holds the voice lock now: the processor blocks on
            // the voice update while more dispatches pour in.
            stall.notified().await;
            send_dispatch(
                &mut socket,
                3,
                "VOICE_STATE_UPDATE",
                json!({ "guild_id": "77", "channel_id": "78", "user_id": "5", "session_id": "s" }),
            )
            .await;
            for sequence in 4..12 {
                send_dispatch(
                    &mut socket,
                    sequence,
                    "MESSAGE_CREATE",
                    json!({ "id": sequence.to_string(), "channel_id": "78", "guild_id": "77", "content": "x" }),
                )
                .await;
            }
            while let Some(Ok(_)) = socket.next().await {}
        }
    })
    .await;

    let runtime = super::SharedRuntime::new(
        "token",
        0,
        super::TypeMap::new(),
        CacheConfig::default(),
        None,
    );
    let shard = tokio::spawn(super::start_gateway_shard(
        super::ShardLaunch {
            token: "token".to_string(),
            intents: 0,
            handler: Arc::new(NoopEventHandler),
            runtime: runtime.clone(),
            gateway_config: crate::ws::GatewayConnectionConfig::new(url),
            shard: (0, 1),
            presence: None,
            dispatch: super::DispatchConfig {
                queue_capacity: 2,
                shutdown_timeout: Duration::from_millis(50),
                ..super::DispatchConfig::default()
            },
        },
        super::ShardStartControl {
            supervisor_channels: None,
            boot_gate: None,
        },
    ));

    tokio::time::timeout(Duration::from_secs(5), async {
        while runtime.cache.guild(&Snowflake::from("77")).await.is_none() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("GUILD_CREATE was cached");
    let fetch = tokio::spawn({
        let context = runtime.context((0, 1));
        async move { context.fetch_members("77", None, None).await }
    });
    // Let the member request reach the (silent) gateway.
    tokio::task::yield_now().await;

    let voice = Arc::clone(&runtime.voice);
    let voice_guard = voice.write().await;
    stall.notify_one();

    let error = tokio::time::timeout(Duration::from_secs(10), shard)
        .await
        .expect("the overflowing shard stops instead of continuing with a gap")
        .unwrap()
        .expect_err("the overflow is reported to the caller");
    drop(voice_guard);
    assert!(error.to_string().contains("overflowed"), "{error}");

    let fetch_error = tokio::time::timeout(Duration::from_secs(5), fetch)
        .await
        .expect("pending fetches fail promptly")
        .unwrap()
        .unwrap_err();
    assert!(
        fetch_error.to_string().contains("shard stopped"),
        "{fetch_error}"
    );
    assert!(
        runtime.cache.guild(&Snowflake::from("77")).await.is_none(),
        "the shard's cache generation was invalidated"
    );
    server.abort();
}

#[cfg(feature = "collectors")]
#[tokio::test]
async fn collectors_created_by_a_queued_handler_replay_events_published_before_it_ran() {
    struct LateCollectingHandler {
        cache: crate::cache::CacheHandle,
        results: mpsc::UnboundedSender<(Option<String>, Option<String>)>,
    }

    #[async_trait]
    impl super::EventHandler for LateCollectingHandler {
        async fn message_create(&self, ctx: super::Context, message: Message) {
            if message.content != "first" {
                return;
            }
            // Reproduce the race deterministically: the processor already
            // published B before this handler creates its collectors.
            while self
                .cache
                .message(&Snowflake::from("500"), &Snowflake::from("2"))
                .await
                .is_none()
            {
                tokio::task::yield_now().await;
            }
            let mut from_cursor = ctx
                .message_collector()
                .timeout(std::time::Duration::from_secs(5));
            let mut from_now = ctx
                .collectors()
                .message_collector()
                .timeout(std::time::Duration::from_millis(50));
            let replayed = from_cursor.next().await.map(|message| message.content);
            let live = from_now.next().await.map(|message| message.content);
            let _ = self.results.send((replayed, live));
        }
    }

    let context = pipeline_test_context((0, 1));
    let (results_tx, mut results_rx) = mpsc::unbounded_channel();
    let pipeline = super::spawn_event_pipeline(
        Arc::new(LateCollectingHandler {
            cache: context.cache.clone(),
            results: results_tx,
        }),
        context,
        #[cfg(feature = "cache")]
        None,
        super::DispatchConfig::default(),
    );
    let ingress = pipeline.ingress();
    // A is published and its handler queued, then B is published, all
    // before A's handler runs.
    ingress.push(message_create_dispatch("1", "first"));
    ingress.push(message_create_dispatch("2", "second"));

    let (replayed, live) = tokio::time::timeout(Duration::from_secs(10), results_rx.recv())
        .await
        .expect("handler finished")
        .expect("handler reported");
    assert_eq!(replayed.as_deref(), Some("second"));
    assert_eq!(
        live, None,
        "a from-now subscription intentionally starts after B"
    );
    drop(ingress);
    pipeline.shutdown(Duration::from_secs(5)).await.unwrap();
}

#[cfg(feature = "collectors")]
#[tokio::test]
async fn collector_hub_replays_from_a_cursor_and_reports_journal_eviction_as_lag() {
    let hub = crate::collector::CollectorHub::with_capacity(2);
    let message = |id: &str| {
        Event::MessageCreate(MessageEvent {
            message: Message {
                id: Snowflake::from(id),
                channel_id: Snowflake::from("1"),
                content: id.to_string(),
                ..Message::default()
            },
            raw: json!({}),
        })
    };

    let start = hub.cursor();
    let after_first = hub.publish(message("a"));
    assert_eq!(after_first.sequence(), start.sequence() + 1);
    hub.publish(message("b"));
    // The replay switches to live events without a gap or duplicate.
    let mut subscription = hub.subscribe_from(after_first);
    assert!(matches!(
        subscription.recv().await,
        Ok(Event::MessageCreate(event)) if event.message.content == "b"
    ));
    hub.publish(message("c"));
    assert!(matches!(
        subscription.recv().await,
        Ok(Event::MessageCreate(event)) if event.message.content == "c"
    ));

    // `start` points at "a", which the two-event journal has evicted.
    let mut lagging = hub
        .message_collector_from(start)
        .timeout(std::time::Duration::from_secs(1));
    assert_eq!(lagging.next().await.unwrap().content, "b");
    assert_eq!(lagging.lagged_events(), 1);

    // Reaction collectors replay from a cursor too.
    let reaction_start = hub.cursor();
    hub.publish(Event::MessageReactionAdd(ReactionEvent {
        user_id: Some(Snowflake::from("7")),
        message_id: Some(Snowflake::from("99")),
        channel_id: Some(Snowflake::from("1")),
        emoji: Some(Emoji::unicode("👍")),
        raw: json!({}),
        ..ReactionEvent::default()
    }));
    {
        let mut handler_ctx = pipeline_test_context((0, 1));
        handler_ctx.collectors = hub.clone();
        handler_ctx.event_cursor = Some(reaction_start);
        let mut reactions = handler_ctx
            .reaction_collector("99")
            .timeout(std::time::Duration::from_secs(1));
        assert!(reactions.next().await.is_some());
    }

    let mut from_now = hub.subscribe();
    drop(hub);
    assert!(matches!(
        from_now.recv().await,
        Err(tokio::sync::broadcast::error::RecvError::Closed)
    ));
}

fn interaction_dispatch(id: &str) -> super::GatewayDispatch {
    super::GatewayDispatch::new(
        "INTERACTION_CREATE".to_string(),
        json!({
            "id": "900",
            "application_id": "204",
            "token": "token",
            "channel_id": "205",
            "guild_id": "206",
            "type": 3,
            "data": { "custom_id": id, "component_type": 2 }
        }),
    )
}

fn interaction_custom_id(interaction: &Interaction) -> String {
    match interaction {
        Interaction::Component(component) => component.data.custom_id.clone(),
        _ => String::new(),
    }
}

/// Reports started interaction calls; blocks interactions listed in `gated`
/// and any message with content "stuck".
struct LaneHandler {
    started: mpsc::UnboundedSender<String>,
    gate: Arc<tokio::sync::Semaphore>,
    gated: Vec<&'static str>,
}

#[async_trait]
impl super::EventHandler for LaneHandler {
    async fn interaction_create(&self, ctx: super::Context, interaction: Interaction) {
        let id = interaction_custom_id(&interaction);
        assert!(ctx.event_received_at().is_some());
        let _ = self.started.send(id.clone());
        if self.gated.contains(&id.as_str()) {
            self.gate.acquire().await.expect("gate open").forget();
        }
    }

    async fn message_create(&self, _ctx: super::Context, message: Message) {
        let _ = self.started.send(format!("message:{}", message.content));
        if message.content == "stuck" {
            self.gate.acquire().await.expect("gate open").forget();
        }
    }
}

type OverloadLog = Arc<std::sync::Mutex<Vec<(String, super::InteractionOverloadReason, Duration)>>>;

fn recording_overload_hook(log: &OverloadLog) -> super::InteractionOverloadHook {
    let log = Arc::clone(log);
    Arc::new(move |overloaded: super::OverloadedInteraction| {
        log.lock().unwrap().push((
            interaction_custom_id(&overloaded.interaction),
            overloaded.reason,
            overloaded.waited,
        ));
    })
}

#[tokio::test]
async fn interactions_have_reserved_slots_while_a_serial_handler_is_stuck() {
    let (started_tx, mut started_rx) = mpsc::unbounded_channel();
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let pipeline = super::spawn_event_pipeline(
        Arc::new(LaneHandler {
            started: started_tx,
            gate: Arc::clone(&gate),
            gated: Vec::new(),
        }),
        pipeline_test_context((0, 1)),
        #[cfg(feature = "cache")]
        None,
        super::DispatchConfig::default(),
    );
    let ingress = pipeline.ingress();
    ingress.push(message_create_dispatch("1", "stuck"));
    assert_eq!(next_started(&mut started_rx).await, "message:stuck");
    // The Serial handler is blocked; the interaction must not wait for it.
    ingress.push(interaction_dispatch("urgent"));
    assert_eq!(next_started(&mut started_rx).await, "urgent");

    gate.add_permits(1);
    drop(ingress);
    pipeline.shutdown(Duration::from_secs(5)).await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn interaction_without_a_slot_goes_to_the_overload_hook_at_its_admission_deadline() {
    let (started_tx, mut started_rx) = mpsc::unbounded_channel();
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let overloads: OverloadLog = Arc::default();
    let pipeline = super::spawn_event_pipeline(
        Arc::new(LaneHandler {
            started: started_tx,
            gate: Arc::clone(&gate),
            gated: vec!["slow"],
        }),
        pipeline_test_context((0, 1)),
        #[cfg(feature = "cache")]
        None,
        super::DispatchConfig {
            max_concurrent_interactions: 1,
            interaction_admission_timeout: Duration::from_secs(2),
            interaction_overload: Some(recording_overload_hook(&overloads)),
            ..super::DispatchConfig::default()
        },
    );
    let ingress = pipeline.ingress();
    let started_at = tokio::time::Instant::now();
    ingress.push(interaction_dispatch("slow"));
    assert_eq!(next_started(&mut started_rx).await, "slow");
    ingress.push(interaction_dispatch("waiting"));
    ingress.push(interaction_dispatch("also-waiting"));

    // Paused time auto-advances to each admission deadline while every task
    // is idle (a yield loop would keep the clock frozen).
    tokio::time::timeout(Duration::from_secs(10), async {
        while overloads.lock().unwrap().len() < 2 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("both waiting interactions reached the hook");
    let overloads = overloads.lock().unwrap().clone();
    assert_eq!(overloads[0].0, "waiting");
    assert_eq!(overloads[1].0, "also-waiting");
    for (_, reason, waited) in &overloads {
        assert_eq!(*reason, super::InteractionOverloadReason::AdmissionTimeout);
        assert!(*waited >= Duration::from_secs(2), "{waited:?}");
    }
    assert!(started_at.elapsed() < Duration::from_secs(3));
    assert!(started_rx.try_recv().is_err(), "no handler ran for them");

    gate.add_permits(1);
    drop(ingress);
    pipeline.shutdown(Duration::from_secs(5)).await.unwrap();
}

#[tokio::test]
async fn full_interaction_queue_goes_to_the_overload_hook_and_hook_panics_are_contained() {
    let (started_tx, mut started_rx) = mpsc::unbounded_channel();
    let overloads: OverloadLog = Arc::default();
    let recorder = recording_overload_hook(&overloads);
    let hook: super::InteractionOverloadHook = Arc::new(move |overloaded| {
        let panics = interaction_custom_id(&overloaded.interaction) == "i3";
        recorder(overloaded);
        if panics {
            panic!("overload hook failure under test");
        }
    });
    let pipeline = super::spawn_event_pipeline(
        Arc::new(LaneHandler {
            started: started_tx,
            gate: Arc::new(tokio::sync::Semaphore::new(0)),
            gated: Vec::new(),
        }),
        pipeline_test_context((0, 1)),
        #[cfg(feature = "cache")]
        None,
        super::DispatchConfig {
            interaction_queue_capacity: 1,
            interaction_overload: Some(hook),
            ..super::DispatchConfig::default()
        },
    );
    let ingress = pipeline.ingress();
    // Current-thread test: the processor handles all four before the
    // interaction dispatcher runs, so only the first fits in the queue.
    for id in ["i1", "i2", "i3", "i4"] {
        ingress.push(interaction_dispatch(id));
    }
    assert_eq!(next_started(&mut started_rx).await, "i1");
    // Later interactions are still processed after the panicking hook.
    ingress.push(interaction_dispatch("i5"));
    assert_eq!(next_started(&mut started_rx).await, "i5");

    let overloads = overloads.lock().unwrap().clone();
    let ids: Vec<_> = overloads.iter().map(|(id, _, _)| id.as_str()).collect();
    assert_eq!(ids, vec!["i2", "i3", "i4"]);
    assert!(overloads
        .iter()
        .all(|(_, reason, _)| *reason == super::InteractionOverloadReason::QueueFull));
    drop(ingress);
    pipeline.shutdown(Duration::from_secs(5)).await.unwrap();
}

#[tokio::test]
async fn interactions_start_in_arrival_order() {
    let (started_tx, mut started_rx) = mpsc::unbounded_channel();
    let pipeline = super::spawn_event_pipeline(
        Arc::new(LaneHandler {
            started: started_tx,
            gate: Arc::new(tokio::sync::Semaphore::new(0)),
            gated: Vec::new(),
        }),
        pipeline_test_context((0, 1)),
        #[cfg(feature = "cache")]
        None,
        super::DispatchConfig {
            max_concurrent_interactions: 1,
            ..super::DispatchConfig::default()
        },
    );
    let ingress = pipeline.ingress();
    for id in ["a", "b", "c"] {
        ingress.push(interaction_dispatch(id));
    }
    for expected in ["a", "b", "c"] {
        assert_eq!(next_started(&mut started_rx).await, expected);
    }
    drop(ingress);
    pipeline.shutdown(Duration::from_secs(5)).await.unwrap();
}

#[cfg(feature = "cache")]
#[tokio::test]
async fn failed_backend_writes_schedule_a_resync_that_is_retried_until_it_succeeds() {
    let recorder = Arc::new(RecordingBackend {
        fail_put_message: true,
        fail_clear_shard_cache: std::sync::atomic::AtomicBool::new(true),
        ..RecordingBackend::default()
    });
    let (forwarder, _worker) = super::CacheBackendForwarder::spawn(recorder.clone(), 16);
    let health = forwarder.health_handle();
    forwarder.forward(
        (0, 1),
        super::BackendWrite::PutMessage(Box::new(Message {
            id: Snowflake::from("1"),
            channel_id: Snowflake::from("10"),
            ..Message::default()
        })),
    );
    forwarder.flush().await;
    let failing = health.health();
    assert!(!failing.healthy);
    assert_eq!(failing.failed_writes, 2, "the put and the first resync");
    assert_eq!(failing.pending_resync_shards, vec![(0, 1)]);
    assert!(failing
        .last_error
        .as_deref()
        .is_some_and(|error| error.contains("clear_shard_cache")));

    recorder
        .fail_clear_shard_cache
        .store(false, std::sync::atomic::Ordering::SeqCst);
    forwarder.forward(
        (0, 1),
        super::BackendWrite::DeleteMember {
            guild_id: Snowflake::from("1"),
            user_id: Snowflake::from("2"),
        },
    );
    forwarder.flush().await;
    assert!(health.health().healthy);
    assert_eq!(health.health().resyncs, 1);
    assert_eq!(
        recorder.calls(),
        vec![
            "put_message:10:1".to_string(),
            "clear_shard_cache:0:1".to_string(),
            "delete_member:1:2".to_string(),
            "clear_shard_cache:0:1".to_string(),
        ]
    );
}

#[cfg(feature = "cache")]
#[tokio::test]
async fn unmergeable_message_update_after_a_backend_read_error_resyncs_the_shard() {
    let recorder = Arc::new(RecordingBackend {
        fail_get_message: true,
        ..RecordingBackend::default()
    });
    let (forwarder, _worker) = super::CacheBackendForwarder::spawn(recorder.clone(), 16);
    let update = decode_event(
        "MESSAGE_UPDATE",
        json!({ "id": "11", "channel_id": "10", "content": "after" }),
    )
    .unwrap();
    // The in-memory cache does not know the message either.
    super::forward_cache_backend_updates(
        &forwarder,
        &crate::cache::CacheHandle::new(),
        (1, 2),
        &update,
    )
    .await;
    forwarder.flush().await;
    assert_eq!(recorder.calls(), vec!["clear_shard_cache:1:2".to_string()]);
    assert_eq!(forwarder.health_handle().health().failed_writes, 1);
}

#[cfg(feature = "cache")]
#[tokio::test]
async fn backend_worker_shutdown_aborts_a_hung_call_so_no_write_lands_afterwards() {
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let recorder = Arc::new(RecordingBackend {
        put_message_gate: Some(Arc::clone(&gate)),
        ..RecordingBackend::default()
    });
    let (forwarder, worker) = super::CacheBackendForwarder::spawn(recorder.clone(), 16);
    forwarder.forward(
        (0, 1),
        super::BackendWrite::PutMessage(Box::new(Message {
            id: Snowflake::from("1"),
            channel_id: Snowflake::from("10"),
            ..Message::default()
        })),
    );
    drop(forwarder);
    let error = worker
        .shutdown(Duration::from_millis(20))
        .await
        .expect_err("a hung backend call cannot finish in time");
    assert!(error.contains("aborted"), "{error}");

    // The backend call was cancelled, not merely detached.
    gate.add_permits(1);
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
    assert!(recorder.calls().is_empty());
}

#[cfg(feature = "cache")]
#[tokio::test]
async fn dm_messages_are_tied_to_shard_zero_and_cleared_by_its_ready() {
    let cache = crate::cache::CacheHandle::new();
    let dm = decode_event(
        "MESSAGE_CREATE",
        json!({ "id": "31", "channel_id": "30", "content": "hi" }),
    )
    .unwrap();
    super::apply_cache_updates(&cache, (0, 2), &dm).await;
    assert_eq!(cache.channel(&Snowflake::from("30")).await.unwrap().kind, 1);

    super::apply_cache_updates(&cache, (1, 2), &ready_dispatch_event()).await;
    assert!(cache
        .message(&Snowflake::from("30"), &Snowflake::from("31"))
        .await
        .is_some());
    super::apply_cache_updates(&cache, (0, 2), &ready_dispatch_event()).await;
    assert!(cache
        .message(&Snowflake::from("30"), &Snowflake::from("31"))
        .await
        .is_none());
}

#[cfg(feature = "sharding")]
#[tokio::test(start_paused = true)]
async fn shutdown_and_wait_cleans_up_even_when_sending_shutdown_fails() {
    let manager = Arc::new(StdMutex::new(ShardingManager::new(ShardConfig::new(2))));
    // Shard 0 already stopped: its command channel is closed.
    drop(
        manager
            .lock()
            .expect("manager mutex poisoned")
            .prepare_runtime(0)
            .unwrap(),
    );
    let _runtime1 = manager
        .lock()
        .expect("manager mutex poisoned")
        .prepare_runtime(1)
        .unwrap();
    let dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = TaskDropFlag(Arc::clone(&dropped));
    let supervisor = ShardSupervisor {
        manager,
        tasks: vec![
            (0, tokio::spawn(async { Ok(()) })),
            (
                1,
                tokio::spawn(async move {
                    let _flag = flag;
                    // Ignores Shutdown.
                    std::future::pending::<()>().await;
                    Ok(())
                }),
            ),
        ],
        stop_timeout: Duration::from_secs(1),
        boot_scheduler: None,
        background: None,
    };

    let error = supervisor
        .shutdown_and_wait()
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("timed out waiting for shard 1"), "{error}");
    assert!(error.contains("sending shutdown also failed"), "{error}");
    // Cleanup ran and was confirmed before returning.
    assert!(dropped.load(std::sync::atomic::Ordering::SeqCst));
}

#[cfg(feature = "sharding")]
#[tokio::test(start_paused = true)]
async fn wait_for_shutdown_never_exceeds_its_deadline_after_a_shard_error() {
    let manager = Arc::new(StdMutex::new(ShardingManager::new(ShardConfig::new(2))));
    let dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = TaskDropFlag(Arc::clone(&dropped));
    let supervisor = ShardSupervisor {
        manager,
        tasks: vec![
            (
                0,
                tokio::spawn(async move {
                    let _flag = flag;
                    std::future::pending::<()>().await;
                    Ok(())
                }),
            ),
            (
                1,
                tokio::spawn(async { Err(crate::error::DiscordError::gateway("4004")) }),
            ),
        ],
        // Far longer than the caller's deadline.
        stop_timeout: Duration::from_secs(600),
        boot_scheduler: None,
        background: None,
    };

    let started = tokio::time::Instant::now();
    let error = supervisor
        .wait_for_shutdown(Duration::from_millis(100))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("4004"), "{error}");
    assert!(
        started.elapsed() <= Duration::from_millis(100) + super::pipeline::ABORT_CONFIRM_GRACE,
        "{:?}",
        started.elapsed()
    );
    assert!(dropped.load(std::sync::atomic::Ordering::SeqCst));
}

/// Records which shards identified, keeps healthy shards connected, and
/// rejects the shards in `reject` with 4004.
async fn spawn_recording_gateway(
    reject: Vec<u64>,
) -> (
    String,
    JoinHandleUnit,
    Arc<std::sync::Mutex<Vec<u64>>>,
    mpsc::UnboundedReceiver<u64>,
) {
    let identified = Arc::new(std::sync::Mutex::new(Vec::new()));
    let (closed_tx, closed_rx) = mpsc::unbounded_channel();
    let seen = Arc::clone(&identified);
    let (url, server) = spawn_fake_gateway(move |mut socket, identify| {
        let seen = Arc::clone(&seen);
        let reject = reject.clone();
        let closed_tx = closed_tx.clone();
        async move {
            use futures_util::StreamExt;
            let shard = identify["d"]["shard"][0].as_u64().unwrap_or(0);
            seen.lock().unwrap().push(shard);
            if reject.contains(&shard) {
                send_close(&mut socket, 4004).await;
                return;
            }
            send_dispatch(&mut socket, 1, "READY", fake_ready_payload()).await;
            while let Some(Ok(message)) = socket.next().await {
                if matches!(message, tokio_tungstenite::tungstenite::Message::Close(_)) {
                    break;
                }
            }
            let _ = closed_tx.send(shard);
        }
    })
    .await;
    (url, server, identified, closed_rx)
}

#[cfg(feature = "sharding")]
async fn spawn_test_supervisor(
    url: String,
    total_shards: u32,
    boot_delay: Duration,
) -> ShardSupervisor {
    super::spawn_shard_supervisor(super::SpawnShardSupervisorConfig {
        token: "token".to_string(),
        intents: 0,
        handler: Arc::new(NoopEventHandler),
        runtime: super::SharedRuntime::new(
            "token",
            0,
            super::TypeMap::new(),
            CacheConfig::default(),
            None,
        ),
        gateway_config: crate::ws::GatewayConnectionConfig::new(url),
        total_shards,
        boot_window_size: 1,
        boot_delay,
        initial_delay: None,
        presence: None,
        dispatch: super::DispatchConfig {
            shutdown_timeout: Duration::from_millis(50),
            ..super::DispatchConfig::default()
        },
        background: None,
    })
    .await
    .unwrap()
}

#[cfg(feature = "sharding")]
#[tokio::test]
async fn first_wave_failure_is_supervised_before_later_waves_launch() {
    let (url, server, identified, _closed) = spawn_recording_gateway(vec![0]).await;
    let supervisor = spawn_test_supervisor(url, 3, Duration::from_secs(5)).await;
    let manager = supervisor.manager();

    let error = tokio::time::timeout(Duration::from_secs(3), supervisor.wait())
        .await
        .expect("the wave-1 failure surfaces during the 5s boot delay")
        .unwrap_err();
    assert!(error.to_string().contains("4004"), "{error}");
    assert_eq!(*identified.lock().unwrap(), vec![0]);
    let manager = super::lock_sharding_manager(&manager);
    for status in manager.statuses() {
        assert_eq!(
            status.state,
            crate::sharding::ShardRuntimeState::Stopped,
            "shard {} must not be left queued or running",
            status.info.id
        );
    }
    server.abort();
}

#[cfg(feature = "sharding")]
#[tokio::test]
async fn cancelling_the_wait_between_boot_waves_stops_started_and_queued_shards() {
    let (url, server, identified, mut closed) = spawn_recording_gateway(Vec::new()).await;
    let boot_delay = Duration::from_millis(300);
    let supervisor = spawn_test_supervisor(url, 3, boot_delay).await;

    tokio::time::timeout(Duration::from_secs(5), async {
        while identified.lock().unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("wave 1 connected");
    assert!(
        tokio::time::timeout(Duration::from_millis(50), supervisor.wait())
            .await
            .is_err()
    );

    // The started shard's connection is dropped...
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), closed.recv())
            .await
            .expect("shard 0 was stopped"),
        Some(0)
    );
    // ...and later waves never launch.
    tokio::time::sleep(boot_delay * 3).await;
    assert_eq!(*identified.lock().unwrap(), vec![0]);
    server.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pipeline_shutdown_reports_handlers_that_cannot_be_cancelled() {
    struct BlockingHandler {
        started: mpsc::UnboundedSender<()>,
    }

    #[async_trait]
    impl super::EventHandler for BlockingHandler {
        async fn message_create(&self, _ctx: super::Context, _message: Message) {
            let _ = self.started.send(());
            // Blocks the thread without ever yielding, so abort cannot land.
            std::thread::sleep(std::time::Duration::from_millis(1_500));
        }
    }

    let (started_tx, mut started_rx) = mpsc::unbounded_channel();
    let pipeline = super::spawn_event_pipeline(
        Arc::new(BlockingHandler {
            started: started_tx,
        }),
        pipeline_test_context((0, 1)),
        #[cfg(feature = "cache")]
        None,
        super::DispatchConfig::default(),
    );
    let ingress = pipeline.ingress();
    ingress.push(message_create_dispatch("1", "block"));
    started_rx.recv().await.expect("handler started");
    drop(ingress);

    let error = pipeline
        .shutdown(Duration::from_millis(20))
        .await
        .expect_err("an uncancellable handler is reported, not claimed stopped");
    assert!(error.contains("did not confirm cancellation"), "{error}");
}

#[cfg(feature = "sharding")]
fn supervisor_with(
    tasks: Vec<(u32, JoinHandle<Result<(), crate::error::DiscordError>>)>,
    boot_scheduler: Option<JoinHandle<()>>,
    background: Option<JoinHandle<()>>,
) -> ShardSupervisor {
    ShardSupervisor {
        manager: Arc::new(StdMutex::new(ShardingManager::new(ShardConfig::new(2)))),
        tasks,
        stop_timeout: Duration::from_secs(1),
        boot_scheduler,
        background,
    }
}

#[cfg(feature = "sharding")]
#[tokio::test(start_paused = true)]
async fn shard_supervisor_wait_drains_background_work_within_the_stop_window() {
    let flushed = supervisor_with(
        vec![(0, tokio::spawn(async { Ok(()) }))],
        None,
        Some(tokio::spawn(async {
            sleep(Duration::from_millis(100)).await;
        })),
    );
    flushed.wait().await.unwrap();

    let hung = supervisor_with(
        vec![(0, tokio::spawn(async { Ok(()) }))],
        None,
        Some(tokio::spawn(std::future::pending::<()>())),
    );
    let error = hung.wait().await.unwrap_err().to_string();
    assert!(error.contains("did not flush"), "{error}");

    let panicked = supervisor_with(
        vec![(0, tokio::spawn(async { Ok(()) }))],
        None,
        Some(tokio::spawn(async {
            panic!("backend worker failure under test")
        })),
    );
    let error = panicked.wait().await.unwrap_err().to_string();
    assert!(error.contains("panicked"), "{error}");
}

#[cfg(feature = "sharding")]
#[tokio::test(start_paused = true)]
async fn shard_error_stops_the_boot_schedule_and_reports_cleanup_problems() {
    let boot_stopped = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = TaskDropFlag(Arc::clone(&boot_stopped));
    let supervisor = supervisor_with(
        vec![
            (
                0,
                tokio::spawn(async { Err(crate::error::DiscordError::gateway("4004")) }),
            ),
            (
                1,
                tokio::spawn(async {
                    sleep(Duration::from_millis(10)).await;
                    Err(crate::error::DiscordError::gateway("late failure"))
                }),
            ),
        ],
        Some(tokio::spawn(async move {
            let _flag = flag;
            std::future::pending::<()>().await;
        })),
        Some(tokio::spawn(std::future::pending::<()>())),
    );

    let error = supervisor.wait().await.unwrap_err().to_string();
    assert!(error.contains("4004"), "{error}");
    assert!(error.contains("did not flush"), "{error}");
    // The pending boot waves were cancelled.
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }
    assert!(boot_stopped.load(std::sync::atomic::Ordering::SeqCst));
}

#[cfg(feature = "sharding")]
#[tokio::test]
async fn shard_supervisor_shutdown_cancels_pending_boot_waves() {
    let boot_stopped = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = TaskDropFlag(Arc::clone(&boot_stopped));
    let supervisor = supervisor_with(
        Vec::new(),
        Some(tokio::spawn(async move {
            let _flag = flag;
            std::future::pending::<()>().await;
        })),
        None,
    );
    supervisor.shutdown().unwrap();
    wait_for_flag(&boot_stopped, "shutdown aborts the boot scheduler").await;
    supervisor.wait().await.unwrap();
}
