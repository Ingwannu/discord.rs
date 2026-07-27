use std::collections::{BTreeMap, HashMap, HashSet};
use std::hash::Hash;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::event::ScheduledEvent;
use crate::model::{
    Channel, Guild, Member, Message, Presence, Role, Snowflake, SoundboardSound, StageInstance,
    Sticker, User, VoiceState,
};
use crate::types::Emoji;

use super::CacheConfig;

/// Minimum interval between full TTL sweeps triggered from hot upsert paths.
const PRUNE_INTERVAL: Duration = Duration::from_secs(5);

/// Insertion/recency order tracker with O(log n) touch/remove/pop operations.
///
/// Keys are assigned monotonically increasing sequence numbers; the `queue`
/// keeps them sorted oldest-first while `positions` maps each key to its
/// current sequence number so a key can be moved to the back or removed
/// without scanning.
#[derive(Clone)]
pub(super) struct LruOrder<K> {
    positions: HashMap<K, u64>,
    queue: BTreeMap<u64, K>,
    next: u64,
}

impl<K> Default for LruOrder<K> {
    fn default() -> Self {
        Self {
            positions: HashMap::new(),
            queue: BTreeMap::new(),
            next: 0,
        }
    }
}

impl<K> LruOrder<K>
where
    K: Clone + Eq + Hash,
{
    /// Inserts the key at the back of the order, or moves it to the back if
    /// it is already present.
    pub(super) fn touch(&mut self, key: K) {
        if let Some(position) = self.positions.get(&key) {
            self.queue.remove(position);
        }
        let position = self.next;
        self.next += 1;
        self.positions.insert(key.clone(), position);
        self.queue.insert(position, key);
    }

    pub(super) fn remove(&mut self, key: &K) {
        if let Some(position) = self.positions.remove(key) {
            self.queue.remove(&position);
        }
    }

    /// Removes and returns the oldest key.
    pub(super) fn pop_front(&mut self) -> Option<K> {
        let (_, key) = self.queue.pop_first()?;
        self.positions.remove(&key);
        Some(key)
    }

    pub(super) fn retain(&mut self, mut pred: impl FnMut(&K) -> bool) {
        let positions = &mut self.positions;
        self.queue.retain(|_, key| {
            if pred(key) {
                true
            } else {
                positions.remove(key);
                false
            }
        });
    }

    pub(super) fn clear(&mut self) {
        self.positions.clear();
        self.queue.clear();
    }

    /// Used by tests; the store tracks entity counts in its own maps.
    #[allow(dead_code)]
    pub(super) fn len(&self) -> usize {
        self.queue.len()
    }

    #[allow(dead_code)]
    pub(super) fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Iterates keys oldest-first.
    pub(super) fn iter(&self) -> impl Iterator<Item = &K> {
        self.queue.values()
    }
}

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
    pub(super) guild_order: LruOrder<Snowflake>,
    pub(super) channel_order: LruOrder<Snowflake>,
    pub(super) user_order: LruOrder<Snowflake>,
    pub(super) member_order: LruOrder<(Snowflake, Snowflake)>,
    pub(super) message_order: LruOrder<(Snowflake, Snowflake)>,
    pub(super) role_order: LruOrder<(Snowflake, Snowflake)>,
    pub(super) presence_order: LruOrder<(Snowflake, Snowflake)>,
    pub(super) voice_state_order: LruOrder<(Snowflake, Snowflake)>,
    pub(super) soundboard_sound_order: LruOrder<(Snowflake, Snowflake)>,
    pub(super) emoji_order: LruOrder<(Snowflake, Snowflake)>,
    pub(super) sticker_order: LruOrder<(Snowflake, Snowflake)>,
    pub(super) scheduled_event_order: LruOrder<(Snowflake, Snowflake)>,
    pub(super) stage_instance_order: LruOrder<(Snowflake, Snowflake)>,
    pub(super) member_seen: HashMap<(Snowflake, Snowflake), Instant>,
    pub(super) message_seen: HashMap<(Snowflake, Snowflake), Instant>,
    pub(super) presence_seen: HashMap<(Snowflake, Snowflake), Instant>,
    /// Number of cached members per guild, kept in sync incrementally so
    /// per-guild caps never require scanning every member key.
    pub(super) member_counts: HashMap<Snowflake, usize>,
    /// Number of cached messages per channel, kept in sync incrementally so
    /// per-channel caps never require scanning every message key.
    pub(super) message_counts: HashMap<Snowflake, usize>,
    /// When the last full TTL sweep ran; used to throttle sweeps triggered
    /// from hot upsert paths.
    pub(super) last_prune: Option<Instant>,
}

/// Thin wrapper kept for call-site compatibility: insert-or-move-to-back.
pub(super) fn remember_key<K>(order: &mut LruOrder<K>, key: K)
where
    K: Clone + Eq + Hash,
{
    order.touch(key);
}

pub(super) fn ordered_overflow_keys<K>(
    order: &mut LruOrder<K>,
    len: usize,
    max: Option<usize>,
) -> Vec<K>
where
    K: Clone + Eq + Hash,
{
    let Some(max) = max else {
        return Vec::new();
    };

    (0..len.saturating_sub(max))
        .filter_map(|_| order.pop_front())
        .collect()
}

fn decrement_count(counts: &mut HashMap<Snowflake, usize>, id: &Snowflake) {
    if let Some(count) = counts.get_mut(id) {
        *count = count.saturating_sub(1);
        if *count == 0 {
            counts.remove(id);
        }
    }
}

/// Inserts a member entry, keeping `member_seen`, `member_order`, and the
/// per-guild counter in sync. Replacing an existing key does not change the
/// counter.
pub(super) fn insert_member_entry(
    store: &mut CacheStore,
    key: (Snowflake, Snowflake),
    member: Arc<Member>,
    now: Instant,
) {
    if store.members.insert(key.clone(), member).is_none() {
        *store.member_counts.entry(key.0.clone()).or_insert(0) += 1;
    }
    store.member_seen.insert(key.clone(), now);
    store.member_order.touch(key);
}

/// Inserts a message entry, keeping `message_seen`, `message_order`, and the
/// per-channel counter in sync. Replacing an existing key does not change the
/// counter.
pub(super) fn insert_message_entry(
    store: &mut CacheStore,
    key: (Snowflake, Snowflake),
    message: Arc<Message>,
    now: Instant,
) {
    if store.messages.insert(key.clone(), message).is_none() {
        *store.message_counts.entry(key.0.clone()).or_insert(0) += 1;
    }
    store.message_seen.insert(key.clone(), now);
    store.message_order.touch(key);
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
        let count = store
            .message_counts
            .get(channel_id)
            .copied()
            .unwrap_or_default();
        if count > max {
            // Only runs when over the cap: walk the LRU queue oldest-first
            // for this channel's excess entries.
            let excess: Vec<_> = store
                .message_order
                .iter()
                .filter(|(stored_channel_id, _)| stored_channel_id == channel_id)
                .take(count - max)
                .cloned()
                .collect();
            for key in excess {
                remove_message_key(store, &key);
            }
        }
    }

    if let Some(max) = config.max_total_messages {
        while store.messages.len() > max {
            let Some(key) = store.message_order.pop_front() else {
                break;
            };
            if store.messages.remove(&key).is_some() {
                decrement_count(&mut store.message_counts, &key.0);
            }
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
    let count = store
        .member_counts
        .get(guild_id)
        .copied()
        .unwrap_or_default();
    if count > max {
        // Only runs when over the cap: walk the LRU queue oldest-first for
        // this guild's excess entries.
        let excess: Vec<_> = store
            .member_order
            .iter()
            .filter(|(stored_guild_id, _)| stored_guild_id == guild_id)
            .take(count - max)
            .cloned()
            .collect();
        for key in excess {
            remove_member_key(store, &key);
        }
    }
}

pub(super) fn remove_message_key(store: &mut CacheStore, key: &(Snowflake, Snowflake)) {
    if store.messages.remove(key).is_some() {
        decrement_count(&mut store.message_counts, &key.0);
    }
    store.message_seen.remove(key);
    store.message_order.remove(key);
}

pub(super) fn remove_member_key(store: &mut CacheStore, key: &(Snowflake, Snowflake)) {
    if store.members.remove(key).is_some() {
        decrement_count(&mut store.member_counts, &key.0);
    }
    store.member_seen.remove(key);
    store.member_order.remove(key);
}

pub(super) fn remove_presence_key(store: &mut CacheStore, key: &(Snowflake, Snowflake)) {
    store.presences.remove(key);
    store.presence_seen.remove(key);
    store.presence_order.remove(key);
}

/// Runs the full TTL sweep only if one has not run within [`PRUNE_INTERVAL`].
///
/// Hot upsert paths call this instead of [`prune_expired`]; correctness for
/// reads is preserved by per-entry expiry checks on every read path, so the
/// sweep only needs to reclaim memory eventually.
pub(super) fn maybe_prune_expired(store: &mut CacheStore, config: &CacheConfig, now: Instant) {
    if store
        .last_prune
        .is_some_and(|last| now.duration_since(last) < PRUNE_INTERVAL)
    {
        return;
    }
    prune_expired(store, config, now);
}

pub(super) fn prune_expired(store: &mut CacheStore, config: &CacheConfig, now: Instant) {
    store.last_prune = Some(now);

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

pub(super) fn evict_channel_entries(store: &mut CacheStore, channel_id: &Snowflake) {
    store.channels.remove(channel_id);
    store.channel_order.remove(channel_id);
    let removed_message_keys: Vec<_> = store
        .messages
        .keys()
        .filter(|(stored_channel_id, _)| stored_channel_id == channel_id)
        .cloned()
        .collect();
    for key in removed_message_keys {
        remove_message_key(store, &key);
    }
}

pub(super) fn evict_guild_entries(store: &mut CacheStore, guild_id: &Snowflake) {
    let removed_channel_ids: HashSet<_> = store
        .channels
        .iter()
        .filter(|(_, channel)| channel.guild_id.as_ref() == Some(guild_id))
        .map(|(channel_id, _)| channel_id.clone())
        .collect();

    store.guilds.remove(guild_id);
    store.guild_order.remove(guild_id);
    for channel_id in &removed_channel_ids {
        store.channels.remove(channel_id);
        store.channel_order.remove(channel_id);
    }
    let removed_member_keys: Vec<_> = store
        .members
        .keys()
        .filter(|(stored_guild_id, _)| stored_guild_id == guild_id)
        .cloned()
        .collect();
    for key in removed_member_keys {
        remove_member_key(store, &key);
    }
    let removed_message_keys: Vec<_> = store
        .messages
        .iter()
        .filter(|((stored_channel_id, _), message)| {
            removed_channel_ids.contains(stored_channel_id)
                || message.as_ref().guild_id.as_ref() == Some(guild_id)
        })
        .map(|(key, _)| key.clone())
        .collect();
    for key in removed_message_keys {
        remove_message_key(store, &key);
    }
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

#[cfg(test)]
mod tests {
    use super::LruOrder;

    #[test]
    fn lru_order_pops_oldest_first_and_touch_moves_to_back() {
        let mut order: LruOrder<u32> = LruOrder::default();
        order.touch(1);
        order.touch(2);
        order.touch(3);
        assert_eq!(order.len(), 3);

        // Touching an existing key moves it to the back without growing.
        order.touch(1);
        assert_eq!(order.len(), 3);
        assert_eq!(order.iter().copied().collect::<Vec<_>>(), vec![2, 3, 1]);

        assert_eq!(order.pop_front(), Some(2));
        assert_eq!(order.pop_front(), Some(3));
        assert_eq!(order.pop_front(), Some(1));
        assert_eq!(order.pop_front(), None);
        assert!(order.is_empty());
    }

    #[test]
    fn lru_order_remove_deletes_single_key() {
        let mut order: LruOrder<u32> = LruOrder::default();
        order.touch(1);
        order.touch(2);
        order.touch(3);

        order.remove(&2);
        assert_eq!(order.len(), 2);
        assert_eq!(order.iter().copied().collect::<Vec<_>>(), vec![1, 3]);

        // Removing a missing key is a no-op.
        order.remove(&42);
        assert_eq!(order.len(), 2);

        // The removed key can be re-inserted at the back.
        order.touch(2);
        assert_eq!(order.iter().copied().collect::<Vec<_>>(), vec![1, 3, 2]);
    }

    #[test]
    fn lru_order_retain_keeps_matching_keys_in_order() {
        let mut order: LruOrder<u32> = LruOrder::default();
        for key in [5, 4, 3, 2, 1] {
            order.touch(key);
        }

        order.retain(|key| key % 2 == 1);
        assert_eq!(order.len(), 3);
        assert_eq!(order.iter().copied().collect::<Vec<_>>(), vec![5, 3, 1]);

        // Dropped keys are fully forgotten and re-insertable.
        order.touch(2);
        assert_eq!(order.pop_front(), Some(5));
        assert_eq!(order.iter().copied().collect::<Vec<_>>(), vec![3, 1, 2]);
    }

    #[test]
    fn lru_order_clear_resets_state() {
        let mut order: LruOrder<u32> = LruOrder::default();
        order.touch(1);
        order.touch(2);
        order.clear();
        assert!(order.is_empty());
        assert_eq!(order.len(), 0);
        assert_eq!(order.pop_front(), None);

        order.touch(2);
        order.touch(1);
        assert_eq!(order.iter().copied().collect::<Vec<_>>(), vec![2, 1]);
    }
}
