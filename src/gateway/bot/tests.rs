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
use tokio::time::{sleep, Duration};

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
    let cache = crate::cache::CacheHandle::new();
    let (event_tx, event_rx) = mpsc::unbounded_channel();
    let processor = super::spawn_gateway_event_processor(
        handler,
        context,
        Arc::clone(&http),
        cache,
        #[cfg(feature = "voice")]
        Arc::new(RwLock::new(crate::voice::VoiceManager::new())),
        #[cfg(feature = "collectors")]
        crate::collector::CollectorHub::new(),
        event_rx,
    );

    event_tx
        .send(super::GatewayDispatch {
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
        })
        .unwrap();
    event_tx
        .send(super::GatewayDispatch {
            event_name: "MESSAGE_CREATE".to_string(),
            data: json!({
                "id": "2",
                "channel_id": "3",
                "content": "hello",
                "mentions": [],
                "attachments": []
            }),
        })
        .unwrap();
    drop(event_tx);
    processor.await.unwrap();

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
    context.gateway_commands.write().await.insert(
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
    let runtime = super::SharedRuntime::new("token", 77, data, CacheConfig::default());
    let (command_tx, _command_rx) = mpsc::channel(GATEWAY_COMMAND_QUEUE_CAPACITY);

    runtime.gateway_commands.write().await.insert(
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
    let runtime =
        super::SharedRuntime::new("token", 0, super::TypeMap::new(), CacheConfig::default());
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
    let cache = crate::cache::CacheHandle::new();

    super::process_gateway_dispatch(
        &handler,
        &context,
        &http,
        &cache,
        #[cfg(feature = "voice")]
        &Arc::new(RwLock::new(crate::voice::VoiceManager::new())),
        #[cfg(feature = "collectors")]
        &crate::collector::CollectorHub::new(),
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
        },
    )
    .await;
    super::process_gateway_dispatch(
        &handler,
        &context,
        &http,
        &cache,
        #[cfg(feature = "voice")]
        &Arc::new(RwLock::new(crate::voice::VoiceManager::new())),
        #[cfg(feature = "collectors")]
        &crate::collector::CollectorHub::new(),
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
        },
    )
    .await;

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
            raw: json!({}),
        }),
        Event::InviteDelete(InviteEvent {
            guild_id: Some(guild_id.clone()),
            channel_id: Some(channel_id.clone()),
            code: Some("invite".to_string()),
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

    for event in events {
        super::apply_cache_updates(&cache, &event).await;
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
            raw: json!({}),
        }),
        Event::InviteDelete(InviteEvent {
            guild_id: Some(guild_id.clone()),
            channel_id: Some(channel_id.clone()),
            code: Some("invite-delete".to_string()),
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

    super::apply_cache_updates(&cache, &event).await;

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
        &json!({
            "content": "after",
            "pinned": false
        }),
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
        &json!({
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
        }),
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
    let cache = crate::cache::CacheHandle::new();
    let collectors = crate::collector::CollectorHub::new();
    let mut message_collector = collectors
        .message_collector()
        .timeout(std::time::Duration::from_secs(1));
    let mut component_collector = collectors
        .component_collector()
        .timeout(std::time::Duration::from_secs(1));

    super::process_gateway_dispatch(
        &handler,
        &context,
        &http,
        &cache,
        #[cfg(feature = "voice")]
        &Arc::new(RwLock::new(crate::voice::VoiceManager::new())),
        &collectors,
        super::GatewayDispatch {
            event_name: "MESSAGE_CREATE".to_string(),
            data: json!({
                "id": "201",
                "channel_id": "202",
                "content": "collector-message",
                "mentions": [],
                "attachments": []
            }),
        },
    )
    .await;
    super::process_gateway_dispatch(
        &handler,
        &context,
        &http,
        &cache,
        #[cfg(feature = "voice")]
        &Arc::new(RwLock::new(crate::voice::VoiceManager::new())),
        &collectors,
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
        },
    )
    .await;

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
    };

    let task_error = task_error_supervisor
        .wait()
        .await
        .expect_err("task error should propagate");
    assert!(task_error.to_string().contains("worker failed"));

    let join_error_supervisor = ShardSupervisor {
        manager,
        tasks: vec![(0, tokio::spawn(async move { panic!("boom") }))],
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
    let runtime =
        super::SharedRuntime::new("token", 0, super::TypeMap::new(), CacheConfig::default());
    let (boot_tx, boot_rx) = watch::channel(false);
    drop(boot_tx);

    super::start_gateway_shard(
        "token".to_string(),
        0,
        Arc::new(NoopHandler),
        runtime,
        crate::ws::GatewayConnectionConfig::default(),
        (0, 1),
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
