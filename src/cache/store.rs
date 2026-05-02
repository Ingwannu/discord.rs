use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::event::ScheduledEvent;
use crate::model::{
    Channel, Guild, Member, Message, Presence, Role, Snowflake, SoundboardSound, StageInstance,
    Sticker, User, VoiceState,
};
use crate::types::Emoji;

use super::CacheConfig;

#[derive(Clone, Default)]
pub(super) struct CacheStore {
    pub(super) guilds: HashMap<Snowflake, Guild>,
    pub(super) channels: HashMap<Snowflake, Channel>,
    pub(super) users: HashMap<Snowflake, User>,
    pub(super) members: HashMap<(Snowflake, Snowflake), Arc<Member>>,
    pub(super) messages: HashMap<(Snowflake, Snowflake), Arc<Message>>,
    pub(super) roles: HashMap<(Snowflake, Snowflake), Role>,
    pub(super) presences: HashMap<(Snowflake, Snowflake), Arc<Presence>>,
    pub(super) voice_states: HashMap<(Snowflake, Snowflake), VoiceState>,
    pub(super) soundboard_sounds: HashMap<(Snowflake, Snowflake), SoundboardSound>,
    pub(super) emojis: HashMap<(Snowflake, Snowflake), Emoji>,
    pub(super) stickers: HashMap<(Snowflake, Snowflake), Sticker>,
    pub(super) scheduled_events: HashMap<(Snowflake, Snowflake), ScheduledEvent>,
    pub(super) stage_instances: HashMap<(Snowflake, Snowflake), StageInstance>,
    pub(super) guild_order: VecDeque<Snowflake>,
    pub(super) channel_order: VecDeque<Snowflake>,
    pub(super) user_order: VecDeque<Snowflake>,
    pub(super) member_order: VecDeque<(Snowflake, Snowflake)>,
    pub(super) message_order: VecDeque<(Snowflake, Snowflake)>,
    pub(super) role_order: VecDeque<(Snowflake, Snowflake)>,
    pub(super) presence_order: VecDeque<(Snowflake, Snowflake)>,
    pub(super) voice_state_order: VecDeque<(Snowflake, Snowflake)>,
    pub(super) soundboard_sound_order: VecDeque<(Snowflake, Snowflake)>,
    pub(super) emoji_order: VecDeque<(Snowflake, Snowflake)>,
    pub(super) sticker_order: VecDeque<(Snowflake, Snowflake)>,
    pub(super) scheduled_event_order: VecDeque<(Snowflake, Snowflake)>,
    pub(super) stage_instance_order: VecDeque<(Snowflake, Snowflake)>,
    pub(super) member_seen: HashMap<(Snowflake, Snowflake), Instant>,
    pub(super) message_seen: HashMap<(Snowflake, Snowflake), Instant>,
    pub(super) presence_seen: HashMap<(Snowflake, Snowflake), Instant>,
}

pub(super) fn remember_key<K>(order: &mut VecDeque<K>, key: K)
where
    K: Clone + Eq,
{
    if let Some(index) = order.iter().position(|stored| stored == &key) {
        order.remove(index);
    }
    order.push_back(key);
}

pub(super) fn ordered_overflow_keys<K>(
    order: &mut VecDeque<K>,
    len: usize,
    max: Option<usize>,
) -> Vec<K>
where
    K: Clone,
{
    let Some(max) = max else {
        return Vec::new();
    };

    (0..len.saturating_sub(max))
        .filter_map(|_| order.pop_front())
        .collect()
}

pub(super) fn enforce_guild_limit(store: &mut CacheStore, config: &CacheConfig) {
    let Some(max) = config.max_guilds else {
        return;
    };

    while store.guilds.len() > max {
        let Some(guild_id) = store.guild_order.pop_front() else {
            break;
        };
        if store.guilds.contains_key(&guild_id) {
            evict_guild_entries(store, &guild_id);
        }
    }
}

pub(super) fn enforce_channel_limit(store: &mut CacheStore, config: &CacheConfig) {
    let Some(max) = config.max_channels else {
        return;
    };

    while store.channels.len() > max {
        let Some(channel_id) = store.channel_order.pop_front() else {
            break;
        };
        if store.channels.contains_key(&channel_id) {
            evict_channel_entries(store, &channel_id);
        }
    }
}

pub(super) fn enforce_message_limits(
    store: &mut CacheStore,
    config: &CacheConfig,
    channel_id: &Snowflake,
) {
    if let Some(max) = config.max_messages_per_channel {
        while store
            .messages
            .keys()
            .filter(|(stored_channel_id, _)| stored_channel_id == channel_id)
            .count()
            > max
        {
            let Some(index) = store
                .message_order
                .iter()
                .position(|(stored_channel_id, _)| stored_channel_id == channel_id)
            else {
                break;
            };
            if let Some(key) = store.message_order.remove(index) {
                store.messages.remove(&key);
                store.message_seen.remove(&key);
            }
        }
    }

    if let Some(max) = config.max_total_messages {
        while store.messages.len() > max {
            let Some(key) = store.message_order.pop_front() else {
                break;
            };
            store.messages.remove(&key);
            store.message_seen.remove(&key);
        }
    }
}

pub(super) fn enforce_member_limit(
    store: &mut CacheStore,
    config: &CacheConfig,
    guild_id: &Snowflake,
) {
    let Some(max) = config.max_members_per_guild else {
        return;
    };
    while store
        .members
        .keys()
        .filter(|(stored_guild_id, _)| stored_guild_id == guild_id)
        .count()
        > max
    {
        let Some(index) = store
            .member_order
            .iter()
            .position(|(stored_guild_id, _)| stored_guild_id == guild_id)
        else {
            break;
        };
        if let Some(key) = store.member_order.remove(index) {
            store.members.remove(&key);
            store.member_seen.remove(&key);
        }
    }
}

pub(super) fn remove_message_key(store: &mut CacheStore, key: &(Snowflake, Snowflake)) {
    store.messages.remove(key);
    store.message_seen.remove(key);
    store.message_order.retain(|stored_key| stored_key != key);
}

pub(super) fn remove_member_key(store: &mut CacheStore, key: &(Snowflake, Snowflake)) {
    store.members.remove(key);
    store.member_seen.remove(key);
    store.member_order.retain(|stored_key| stored_key != key);
}

pub(super) fn remove_presence_key(store: &mut CacheStore, key: &(Snowflake, Snowflake)) {
    store.presences.remove(key);
    store.presence_seen.remove(key);
    store.presence_order.retain(|stored_key| stored_key != key);
}

pub(super) fn prune_expired(store: &mut CacheStore, config: &CacheConfig, now: Instant) {
    if let Some(ttl) = config.message_ttl {
        let expired: Vec<_> = store
            .message_seen
            .iter()
            .filter(|(_, seen)| now.duration_since(**seen) >= ttl)
            .map(|(key, _)| key.clone())
            .collect();
        for key in expired {
            remove_message_key(store, &key);
        }
    }

    if let Some(ttl) = config.presence_ttl {
        let expired: Vec<_> = store
            .presence_seen
            .iter()
            .filter(|(_, seen)| now.duration_since(**seen) >= ttl)
            .map(|(key, _)| key.clone())
            .collect();
        for key in expired {
            remove_presence_key(store, &key);
        }
    }

    if let Some(ttl) = config.member_ttl {
        let expired: Vec<_> = store
            .member_seen
            .iter()
            .filter(|(_, seen)| now.duration_since(**seen) >= ttl)
            .map(|(key, _)| key.clone())
            .collect();
        for key in expired {
            remove_member_key(store, &key);
        }
    }
}

pub(super) fn seen_expired(seen: Option<&Instant>, ttl: Option<Duration>, now: Instant) -> bool {
    ttl.is_some_and(|ttl| seen.is_some_and(|seen| now.duration_since(*seen) >= ttl))
}

pub(super) fn any_seen_expired<'a>(
    mut seen_values: impl Iterator<Item = &'a Instant>,
    ttl: Option<Duration>,
    now: Instant,
) -> bool {
    ttl.is_some_and(|ttl| seen_values.any(|seen| now.duration_since(*seen) >= ttl))
}

pub(super) fn evict_channel_entries(store: &mut CacheStore, channel_id: &Snowflake) {
    store.channels.remove(channel_id);
    store
        .channel_order
        .retain(|stored_id| stored_id != channel_id);
    store
        .messages
        .retain(|(stored_channel_id, _), _| stored_channel_id != channel_id);
    store
        .message_seen
        .retain(|(stored_channel_id, _), _| stored_channel_id != channel_id);
    store
        .message_order
        .retain(|(stored_channel_id, _)| stored_channel_id != channel_id);
}

pub(super) fn evict_guild_entries(store: &mut CacheStore, guild_id: &Snowflake) {
    let removed_channel_ids: HashSet<_> = store
        .channels
        .iter()
        .filter(|(_, channel)| channel.guild_id.as_ref() == Some(guild_id))
        .map(|(channel_id, _)| channel_id.clone())
        .collect();

    store.guilds.remove(guild_id);
    store.guild_order.retain(|stored_id| stored_id != guild_id);
    store
        .channels
        .retain(|_, channel| channel.guild_id.as_ref() != Some(guild_id));
    store.channel_order.retain(|channel_id| {
        store
            .channels
            .get(channel_id)
            .is_some_and(|channel| channel.guild_id.as_ref() != Some(guild_id))
    });
    store
        .members
        .retain(|(stored_guild_id, _), _| stored_guild_id != guild_id);
    store
        .member_seen
        .retain(|(stored_guild_id, _), _| stored_guild_id != guild_id);
    store
        .member_order
        .retain(|(stored_guild_id, _)| stored_guild_id != guild_id);
    store.messages.retain(|(stored_channel_id, _), message| {
        !removed_channel_ids.contains(stored_channel_id)
            && message.as_ref().guild_id.as_ref() != Some(guild_id)
    });
    let remaining_message_keys: HashSet<_> = store.messages.keys().cloned().collect();
    store
        .message_seen
        .retain(|key, _| remaining_message_keys.contains(key));
    store
        .message_order
        .retain(|(stored_channel_id, message_id)| {
            !removed_channel_ids.contains(stored_channel_id)
                && remaining_message_keys.contains(&(stored_channel_id.clone(), message_id.clone()))
        });
    store
        .roles
        .retain(|(stored_guild_id, _), _| stored_guild_id != guild_id);
    store
        .role_order
        .retain(|(stored_guild_id, _)| stored_guild_id != guild_id);
    store
        .presences
        .retain(|(stored_guild_id, _), _| stored_guild_id != guild_id);
    store
        .presence_seen
        .retain(|(stored_guild_id, _), _| stored_guild_id != guild_id);
    store
        .presence_order
        .retain(|(stored_guild_id, _)| stored_guild_id != guild_id);
    store
        .voice_states
        .retain(|(stored_guild_id, _), _| stored_guild_id != guild_id);
    store
        .voice_state_order
        .retain(|(stored_guild_id, _)| stored_guild_id != guild_id);
    store
        .soundboard_sounds
        .retain(|(stored_guild_id, _), _| stored_guild_id != guild_id);
    store
        .soundboard_sound_order
        .retain(|(stored_guild_id, _)| stored_guild_id != guild_id);
    store
        .emojis
        .retain(|(stored_guild_id, _), _| stored_guild_id != guild_id);
    store
        .emoji_order
        .retain(|(stored_guild_id, _)| stored_guild_id != guild_id);
    store
        .stickers
        .retain(|(stored_guild_id, _), _| stored_guild_id != guild_id);
    store
        .sticker_order
        .retain(|(stored_guild_id, _)| stored_guild_id != guild_id);
    store
        .scheduled_events
        .retain(|(stored_guild_id, _), _| stored_guild_id != guild_id);
    store
        .scheduled_event_order
        .retain(|(stored_guild_id, _)| stored_guild_id != guild_id);
    store
        .stage_instances
        .retain(|(stored_guild_id, _), _| stored_guild_id != guild_id);
    store
        .stage_instance_order
        .retain(|(stored_guild_id, _)| stored_guild_id != guild_id);
}
