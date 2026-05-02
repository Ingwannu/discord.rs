use serde_json::Value;

use crate::error::DiscordError;
use crate::model::Snowflake;
use crate::parsers::parse_interaction;

use super::*;

/// Provides the `decode_event` helper.
pub fn decode_event(event_name: &str, data: Value) -> Result<Event, DiscordError> {
    let event = match event_name {
        "READY" => Event::Ready(ReadyEvent {
            data: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "GUILD_CREATE" => Event::GuildCreate(GuildEvent {
            guild: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "GUILD_UPDATE" => Event::GuildUpdate(GuildEvent {
            guild: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "GUILD_DELETE" => Event::GuildDelete(GuildDeleteEvent {
            data: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "CHANNEL_CREATE" => Event::ChannelCreate(ChannelEvent {
            channel: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "CHANNEL_UPDATE" => Event::ChannelUpdate(ChannelEvent {
            channel: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "CHANNEL_DELETE" => Event::ChannelDelete(ChannelEvent {
            channel: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "GUILD_MEMBER_ADD" => Event::MemberAdd(MemberEvent {
            guild_id: read_required_snowflake(&data, "guild_id")?,
            member: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "GUILD_MEMBER_UPDATE" => Event::MemberUpdate(MemberEvent {
            guild_id: read_required_snowflake(&data, "guild_id")?,
            member: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "GUILD_MEMBER_REMOVE" => Event::MemberRemove(MemberRemoveEvent {
            data: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "GUILD_MEMBERS_CHUNK" => Event::GuildMembersChunk(GuildMembersChunkEvent {
            data: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "GUILD_ROLE_CREATE" => Event::RoleCreate(RoleEvent {
            guild_id: read_required_snowflake(&data, "guild_id")?,
            role: serde_json::from_value(data.get("role").cloned().unwrap_or(Value::Null))?,
            raw: data,
        }),
        "GUILD_ROLE_UPDATE" => Event::RoleUpdate(RoleEvent {
            guild_id: read_required_snowflake(&data, "guild_id")?,
            role: serde_json::from_value(data.get("role").cloned().unwrap_or(Value::Null))?,
            raw: data,
        }),
        "GUILD_ROLE_DELETE" => Event::RoleDelete(RoleDeleteEvent {
            data: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "MESSAGE_CREATE" => Event::MessageCreate(MessageEvent {
            message: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "MESSAGE_UPDATE" => Event::MessageUpdate(MessageEvent {
            message: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "MESSAGE_DELETE" => Event::MessageDelete(MessageDeleteEvent {
            data: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "MESSAGE_DELETE_BULK" => {
            let ids: Vec<Snowflake> =
                serde_json::from_value(data.get("ids").cloned().unwrap_or(Value::Null))?;
            Event::MessageDeleteBulk(BulkMessageDeleteEvent {
                channel_id: read_required_snowflake(&data, "channel_id")?,
                guild_id: data
                    .get("guild_id")
                    .and_then(|v| serde_json::from_value(v.clone()).ok()),
                ids,
                raw: data,
            })
        }
        "CHANNEL_PINS_UPDATE" => Event::ChannelPinsUpdate(ChannelPinsUpdateEvent {
            channel_id: read_required_snowflake(&data, "channel_id")?,
            guild_id: data
                .get("guild_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            last_pin_timestamp: data
                .get("last_pin_timestamp")
                .and_then(|v| v.as_str().map(String::from)),
            raw: data,
        }),
        "GUILD_BAN_ADD" => Event::GuildBanAdd(GuildBanEvent {
            guild_id: read_required_snowflake(&data, "guild_id")?,
            user: serde_json::from_value(data.get("user").cloned().unwrap_or(Value::Null))?,
            raw: data,
        }),
        "GUILD_BAN_REMOVE" => Event::GuildBanRemove(GuildBanEvent {
            guild_id: read_required_snowflake(&data, "guild_id")?,
            user: serde_json::from_value(data.get("user").cloned().unwrap_or(Value::Null))?,
            raw: data,
        }),
        "GUILD_EMOJIS_UPDATE" => Event::GuildEmojisUpdate(GuildEmojisUpdateEvent {
            guild_id: read_required_snowflake(&data, "guild_id")?,
            emojis: serde_json::from_value(data.get("emojis").cloned().unwrap_or(Value::Null))
                .unwrap_or_default(),
            raw: data,
        }),
        "GUILD_INTEGRATIONS_UPDATE" => {
            Event::GuildIntegrationsUpdate(GuildIntegrationsUpdateEvent {
                guild_id: data
                    .get("guild_id")
                    .and_then(|v| serde_json::from_value(v.clone()).ok()),
                raw: data,
            })
        }
        "WEBHOOKS_UPDATE" => Event::WebhooksUpdate(WebhooksUpdateEvent {
            guild_id: data
                .get("guild_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            channel_id: data
                .get("channel_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            raw: data,
        }),
        "INVITE_CREATE" => Event::InviteCreate(InviteEvent {
            guild_id: data
                .get("guild_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            channel_id: data
                .get("channel_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            code: data.get("code").and_then(|v| v.as_str().map(String::from)),
            raw: data,
        }),
        "INVITE_DELETE" => Event::InviteDelete(InviteEvent {
            guild_id: data
                .get("guild_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            channel_id: data
                .get("channel_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            code: data.get("code").and_then(|v| v.as_str().map(String::from)),
            raw: data,
        }),
        "MESSAGE_REACTION_ADD" => Event::MessageReactionAdd(decode_reaction_event(data)),
        "MESSAGE_REACTION_REMOVE" => Event::MessageReactionRemove(decode_reaction_event(data)),
        "MESSAGE_REACTION_REMOVE_ALL" => Event::MessageReactionRemoveAll(ReactionRemoveAllEvent {
            channel_id: data
                .get("channel_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            message_id: data
                .get("message_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            guild_id: data
                .get("guild_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            raw: data,
        }),
        "TYPING_START" => Event::TypingStart(TypingStartEvent {
            channel_id: data
                .get("channel_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            guild_id: data
                .get("guild_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            user_id: data
                .get("user_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            timestamp: data.get("timestamp").and_then(|v| v.as_u64()),
            raw: data,
        }),
        "PRESENCE_UPDATE" => Event::PresenceUpdate(decode_presence_update_event(data)),
        "USER_UPDATE" => Event::UserUpdate(UserUpdateEvent {
            user: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "INTERACTION_CREATE" => Event::InteractionCreate(InteractionEvent {
            interaction: parse_interaction(&data)?,
            raw: data,
        }),
        "VOICE_STATE_UPDATE" => Event::VoiceStateUpdate(VoiceStateEvent {
            state: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "VOICE_SERVER_UPDATE" => Event::VoiceServerUpdate(VoiceServerEvent {
            data: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "RESUMED" => Event::Resumed(ResumedEvent { raw: data }),
        "THREAD_CREATE" => Event::ThreadCreate(ThreadEvent {
            thread: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "THREAD_UPDATE" => Event::ThreadUpdate(ThreadEvent {
            thread: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "THREAD_DELETE" => Event::ThreadDelete(ThreadEvent {
            thread: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "THREAD_LIST_SYNC" => Event::ThreadListSync(ThreadListSyncEvent {
            guild_id: data
                .get("guild_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            threads: data
                .get("threads")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
            raw: data,
        }),
        "THREAD_MEMBER_UPDATE" => Event::ThreadMemberUpdate(ThreadMemberUpdateEvent {
            guild_id: data
                .get("guild_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            thread_id: data
                .get("id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            raw: data,
        }),
        "THREAD_MEMBERS_UPDATE" => Event::ThreadMembersUpdate(ThreadMembersUpdateEvent {
            guild_id: data
                .get("guild_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            thread_id: data
                .get("id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            added_members: data
                .get("added_members")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            removed_member_ids: data
                .get("removed_member_ids")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            member_count: data.get("member_count").and_then(|v| v.as_u64()),
            raw: data,
        }),
        "MESSAGE_REACTION_REMOVE_EMOJI" => {
            Event::MessageReactionRemoveEmoji(ReactionRemoveEmojiEvent {
                channel_id: data
                    .get("channel_id")
                    .and_then(|v| serde_json::from_value(v.clone()).ok()),
                message_id: data
                    .get("message_id")
                    .and_then(|v| serde_json::from_value(v.clone()).ok()),
                guild_id: data
                    .get("guild_id")
                    .and_then(|v| serde_json::from_value(v.clone()).ok()),
                emoji: data
                    .get("emoji")
                    .and_then(|v| serde_json::from_value(v.clone()).ok()),
                raw: data,
            })
        }
        "GUILD_STICKERS_UPDATE" => Event::GuildStickersUpdate(GuildStickersUpdateEvent {
            guild_id: data
                .get("guild_id")
                .and_then(|v| serde_json::from_value(v.clone()).ok()),
            stickers: data
                .get("stickers")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
            raw: data,
        }),
        "ENTITLEMENT_CREATE" => Event::EntitlementCreate(EntitlementEvent {
            entitlement: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "ENTITLEMENT_UPDATE" => Event::EntitlementUpdate(EntitlementEvent {
            entitlement: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "ENTITLEMENT_DELETE" => Event::EntitlementDelete(EntitlementEvent {
            entitlement: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "SUBSCRIPTION_CREATE" => Event::SubscriptionCreate(SubscriptionEvent {
            subscription: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "SUBSCRIPTION_UPDATE" => Event::SubscriptionUpdate(SubscriptionEvent {
            subscription: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "SUBSCRIPTION_DELETE" => Event::SubscriptionDelete(SubscriptionEvent {
            subscription: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "INTEGRATION_CREATE" => Event::IntegrationCreate(IntegrationEvent {
            guild_id: read_optional_snowflake(&data, "guild_id"),
            integration: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "INTEGRATION_UPDATE" => Event::IntegrationUpdate(IntegrationEvent {
            guild_id: read_optional_snowflake(&data, "guild_id"),
            integration: serde_json::from_value(data.clone())?,
            raw: data,
        }),
        "INTEGRATION_DELETE" => Event::IntegrationDelete(IntegrationDeleteEvent {
            id: read_optional_snowflake(&data, "id"),
            guild_id: read_optional_snowflake(&data, "guild_id"),
            application_id: read_optional_snowflake(&data, "application_id"),
            raw: data,
        }),
        "GUILD_SOUNDBOARD_SOUND_CREATE" => {
            Event::GuildSoundboardSoundCreate(SoundboardSoundEvent {
                sound: serde_json::from_value(data.clone())?,
                raw: data,
            })
        }
        "GUILD_SOUNDBOARD_SOUND_UPDATE" => {
            Event::GuildSoundboardSoundUpdate(SoundboardSoundEvent {
                sound: serde_json::from_value(data.clone())?,
                raw: data,
            })
        }
        "GUILD_SOUNDBOARD_SOUND_DELETE" => {
            Event::GuildSoundboardSoundDelete(SoundboardSoundDeleteEvent {
                sound_id: read_required_snowflake(&data, "sound_id")?,
                guild_id: read_required_snowflake(&data, "guild_id")?,
                raw: data,
            })
        }
        "GUILD_SOUNDBOARD_SOUNDS_UPDATE" => {
            Event::GuildSoundboardSoundsUpdate(decode_soundboard_sounds_event(data)?)
        }
        "SOUNDBOARD_SOUNDS" => Event::SoundboardSounds(decode_soundboard_sounds_event(data)?),
        "GUILD_SCHEDULED_EVENT_CREATE" => {
            Event::GuildScheduledEventCreate(decode_scheduled_event(data)?)
        }
        "GUILD_SCHEDULED_EVENT_UPDATE" => {
            Event::GuildScheduledEventUpdate(decode_scheduled_event(data)?)
        }
        "GUILD_SCHEDULED_EVENT_DELETE" => {
            Event::GuildScheduledEventDelete(decode_scheduled_event(data)?)
        }
        "GUILD_SCHEDULED_EVENT_USER_ADD" => {
            Event::GuildScheduledEventUserAdd(decode_scheduled_event_user_event(data)?)
        }
        "GUILD_SCHEDULED_EVENT_USER_REMOVE" => {
            Event::GuildScheduledEventUserRemove(decode_scheduled_event_user_event(data)?)
        }
        "STAGE_INSTANCE_CREATE" => Event::StageInstanceCreate(decode_stage_instance_event(data)?),
        "STAGE_INSTANCE_UPDATE" => Event::StageInstanceUpdate(decode_stage_instance_event(data)?),
        "STAGE_INSTANCE_DELETE" => Event::StageInstanceDelete(decode_stage_instance_event(data)?),
        "VOICE_CHANNEL_EFFECT_SEND" => {
            Event::VoiceChannelEffectSend(decode_voice_channel_effect_event(data))
        }
        "VOICE_CHANNEL_START_TIME_UPDATE" => {
            Event::VoiceChannelStartTimeUpdate(decode_voice_channel_start_time_update_event(data))
        }
        "VOICE_CHANNEL_STATUS_UPDATE" => {
            Event::VoiceChannelStatusUpdate(decode_voice_channel_status_update_event(data))
        }
        "CHANNEL_INFO" => Event::ChannelInfo(decode_channel_info_event(data)?),
        "RATE_LIMITED" => Event::RateLimited(decode_rate_limited_event(data)),
        "APPLICATION_COMMAND_PERMISSIONS_UPDATE" => Event::ApplicationCommandPermissionsUpdate(
            decode_application_command_permissions_update_event(data),
        ),
        "AUTO_MODERATION_RULE_CREATE" => {
            Event::AutoModerationRuleCreate(decode_auto_moderation_event(data))
        }
        "AUTO_MODERATION_RULE_UPDATE" => {
            Event::AutoModerationRuleUpdate(decode_auto_moderation_event(data))
        }
        "AUTO_MODERATION_RULE_DELETE" => {
            Event::AutoModerationRuleDelete(decode_auto_moderation_event(data))
        }
        "AUTO_MODERATION_ACTION_EXECUTION" => {
            Event::AutoModerationActionExecution(decode_auto_moderation_event(data))
        }
        "GUILD_AUDIT_LOG_ENTRY_CREATE" => {
            Event::GuildAuditLogEntryCreate(decode_audit_log_entry_event(data))
        }
        "MESSAGE_POLL_VOTE_ADD" => Event::MessagePollVoteAdd(decode_poll_vote_event(data)),
        "MESSAGE_POLL_VOTE_REMOVE" => Event::MessagePollVoteRemove(decode_poll_vote_event(data)),
        _ => Event::Unknown {
            kind: event_name.to_string(),
            raw: data,
        },
    };

    Ok(event)
}

fn read_required_snowflake(value: &Value, field: &str) -> Result<Snowflake, DiscordError> {
    let Some(raw) = value.get(field) else {
        return Err(format!("missing field {field}").into());
    };

    serde_json::from_value(raw.clone()).map_err(Into::into)
}

fn read_optional_snowflake(value: &Value, field: &str) -> Option<Snowflake> {
    value
        .get(field)
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}

fn read_optional_string(value: &Value, field: &str) -> Option<String> {
    value.get(field).and_then(|v| v.as_str().map(String::from))
}

fn read_optional_u64(value: &Value, field: &str) -> Option<u64> {
    value.get(field).and_then(Value::as_u64)
}

fn decode_scheduled_event(data: Value) -> Result<ScheduledEvent, DiscordError> {
    Ok(ScheduledEvent {
        id: read_optional_snowflake(&data, "id"),
        guild_id: read_optional_snowflake(&data, "guild_id"),
        channel_id: read_optional_snowflake(&data, "channel_id"),
        creator_id: read_optional_snowflake(&data, "creator_id"),
        name: read_optional_string(&data, "name"),
        description: read_optional_string(&data, "description"),
        scheduled_start_time: read_optional_string(&data, "scheduled_start_time"),
        scheduled_end_time: read_optional_string(&data, "scheduled_end_time"),
        privacy_level: read_optional_u64(&data, "privacy_level"),
        status: read_optional_u64(&data, "status"),
        entity_type: read_optional_u64(&data, "entity_type"),
        entity_id: read_optional_snowflake(&data, "entity_id"),
        entity_metadata: data.get("entity_metadata").cloned(),
        user_count: read_optional_u64(&data, "user_count"),
        image: read_optional_string(&data, "image"),
        raw: data,
    })
}

fn decode_soundboard_sounds_event(data: Value) -> Result<SoundboardSoundsEvent, DiscordError> {
    Ok(SoundboardSoundsEvent {
        guild_id: read_required_snowflake(&data, "guild_id")?,
        soundboard_sounds: data
            .get("soundboard_sounds")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default(),
        raw: data,
    })
}

fn decode_poll_vote_event(data: Value) -> PollVoteEvent {
    PollVoteEvent {
        user_id: read_optional_snowflake(&data, "user_id"),
        channel_id: read_optional_snowflake(&data, "channel_id"),
        message_id: read_optional_snowflake(&data, "message_id"),
        guild_id: read_optional_snowflake(&data, "guild_id"),
        answer_id: read_optional_u64(&data, "answer_id"),
        raw: data,
    }
}

fn decode_reaction_event(data: Value) -> ReactionEvent {
    ReactionEvent {
        user_id: read_optional_snowflake(&data, "user_id"),
        channel_id: read_optional_snowflake(&data, "channel_id"),
        message_id: read_optional_snowflake(&data, "message_id"),
        guild_id: read_optional_snowflake(&data, "guild_id"),
        member: data
            .get("member")
            .and_then(|value| serde_json::from_value(value.clone()).ok()),
        message_author_id: read_optional_snowflake(&data, "message_author_id"),
        emoji: data
            .get("emoji")
            .and_then(|value| serde_json::from_value(value.clone()).ok()),
        burst: data.get("burst").and_then(Value::as_bool),
        burst_colors: data
            .get("burst_colors")
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default(),
        reaction_type: read_optional_u64(&data, "type"),
        raw: data,
    }
}

fn decode_presence_update_event(data: Value) -> PresenceUpdateEvent {
    let user = data.get("user").and_then(decode_presence_update_user);
    let user_id = user.as_ref().map(|user| user.id.clone()).or_else(|| {
        data.pointer("/user/id")
            .and_then(Value::as_str)
            .map(Snowflake::new)
    });

    PresenceUpdateEvent {
        user,
        user_id,
        guild_id: read_optional_snowflake(&data, "guild_id"),
        status: read_optional_string(&data, "status"),
        activities: data
            .get("activities")
            .and_then(|value| serde_json::from_value(value.clone()).ok())
            .unwrap_or_default(),
        client_status: data
            .get("client_status")
            .and_then(|value| serde_json::from_value(value.clone()).ok()),
        raw: data,
    }
}

fn decode_presence_update_user(value: &Value) -> Option<PresenceUpdateUser> {
    Some(PresenceUpdateUser {
        id: read_required_snowflake(value, "id").ok()?,
        username: read_optional_string(value, "username"),
        raw: value.clone(),
    })
}

fn decode_scheduled_event_user_event(
    data: Value,
) -> Result<GuildScheduledEventUserEvent, DiscordError> {
    let mut event: GuildScheduledEventUserEvent = serde_json::from_value(data.clone())?;
    event.raw = data;
    Ok(event)
}

fn decode_stage_instance_event(data: Value) -> Result<StageInstanceEvent, DiscordError> {
    Ok(StageInstanceEvent {
        stage_instance: serde_json::from_value(data.clone())?,
        raw: data,
    })
}

fn decode_application_command_permissions_update_event(
    data: Value,
) -> ApplicationCommandPermissionsUpdateEvent {
    ApplicationCommandPermissionsUpdateEvent {
        id: read_optional_snowflake(&data, "id"),
        application_id: read_optional_snowflake(&data, "application_id"),
        guild_id: read_optional_snowflake(&data, "guild_id"),
        permissions: data
            .get("permissions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        raw: data,
    }
}

fn decode_voice_channel_effect_event(data: Value) -> VoiceChannelEffectEvent {
    VoiceChannelEffectEvent {
        channel_id: read_optional_snowflake(&data, "channel_id"),
        guild_id: read_optional_snowflake(&data, "guild_id"),
        user_id: read_optional_snowflake(&data, "user_id"),
        emoji: data
            .get("emoji")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok()),
        animation_type: read_optional_u64(&data, "animation_type"),
        animation_id: read_optional_u64(&data, "animation_id"),
        sound_id: read_optional_snowflake(&data, "sound_id"),
        sound_volume: data.get("sound_volume").and_then(Value::as_f64),
        raw: data,
    }
}

fn decode_voice_channel_start_time_update_event(data: Value) -> VoiceChannelStartTimeUpdateEvent {
    VoiceChannelStartTimeUpdateEvent {
        channel_id: read_optional_snowflake(&data, "channel_id"),
        guild_id: read_optional_snowflake(&data, "guild_id"),
        voice_channel_start_time: read_optional_string(&data, "voice_channel_start_time"),
        raw: data,
    }
}

fn decode_voice_channel_status_update_event(data: Value) -> VoiceChannelStatusUpdateEvent {
    VoiceChannelStatusUpdateEvent {
        channel_id: read_optional_snowflake(&data, "channel_id"),
        guild_id: read_optional_snowflake(&data, "guild_id"),
        status: read_optional_string(&data, "status"),
        raw: data,
    }
}

fn decode_channel_info_event(data: Value) -> Result<ChannelInfoEvent, DiscordError> {
    let guild_id = read_required_snowflake(&data, "guild_id")?;
    let channels = data
        .get("channels")
        .and_then(Value::as_array)
        .map(|channels| {
            channels
                .iter()
                .map(|channel| {
                    Ok(ChannelInfoChannel {
                        id: read_required_snowflake(channel, "id")?,
                        status: read_optional_string(channel, "status"),
                        voice_start_time: read_optional_string(channel, "voice_start_time"),
                        raw: channel.clone(),
                    })
                })
                .collect::<Result<Vec<_>, DiscordError>>()
        })
        .transpose()?
        .unwrap_or_default();

    Ok(ChannelInfoEvent {
        guild_id,
        channels,
        raw: data,
    })
}

fn decode_rate_limited_event(data: Value) -> RateLimitedEvent {
    RateLimitedEvent {
        opcode: read_optional_u64(&data, "opcode"),
        retry_after: data.get("retry_after").and_then(Value::as_f64),
        meta: data.get("meta").cloned(),
        raw: data,
    }
}

fn decode_auto_moderation_event(data: Value) -> AutoModerationEvent {
    AutoModerationEvent {
        id: read_optional_snowflake(&data, "id"),
        guild_id: read_optional_snowflake(&data, "guild_id"),
        name: read_optional_string(&data, "name"),
        creator_id: read_optional_snowflake(&data, "creator_id"),
        event_type: read_optional_u64(&data, "event_type"),
        trigger_type: read_optional_u64(&data, "trigger_type"),
        trigger_metadata: data.get("trigger_metadata").cloned(),
        actions: data
            .get("actions")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default(),
        enabled: data.get("enabled").and_then(Value::as_bool),
        exempt_roles: data
            .get("exempt_roles")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default(),
        exempt_channels: data
            .get("exempt_channels")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default(),
        action: data.get("action").cloned(),
        rule_id: read_optional_snowflake(&data, "rule_id"),
        rule_trigger_type: read_optional_u64(&data, "rule_trigger_type"),
        user_id: read_optional_snowflake(&data, "user_id"),
        channel_id: read_optional_snowflake(&data, "channel_id"),
        message_id: read_optional_snowflake(&data, "message_id"),
        alert_system_message_id: read_optional_snowflake(&data, "alert_system_message_id"),
        content: read_optional_string(&data, "content"),
        matched_keyword: read_optional_string(&data, "matched_keyword"),
        matched_content: read_optional_string(&data, "matched_content"),
        raw: data,
    }
}

fn decode_audit_log_entry_event(data: Value) -> AuditLogEntryEvent {
    AuditLogEntryEvent {
        guild_id: read_optional_snowflake(&data, "guild_id"),
        entry: serde_json::from_value(data.clone()).ok(),
        id: read_optional_snowflake(&data, "id"),
        user_id: read_optional_snowflake(&data, "user_id"),
        target_id: read_optional_snowflake(&data, "target_id"),
        action_type: read_optional_u64(&data, "action_type"),
        changes: data
            .get("changes")
            .and_then(|v| serde_json::from_value(v.clone()).ok()),
        options: data.get("options").cloned(),
        reason: read_optional_string(&data, "reason"),
        raw: data,
    }
}
