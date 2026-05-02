use std::time::Duration;

const DEFAULT_MAX_MESSAGES_PER_CHANNEL: usize = 100;
const DEFAULT_MAX_TOTAL_MESSAGES: usize = 10_000;
const DEFAULT_MAX_PRESENCES: usize = 50_000;
const DEFAULT_MAX_MEMBERS_PER_GUILD: usize = 25_000;
const DEFAULT_MAX_GUILDS: usize = 1_000;
const DEFAULT_MAX_CHANNELS: usize = 50_000;
const DEFAULT_MAX_USERS: usize = 100_000;
const DEFAULT_MAX_ROLES: usize = 50_000;
const DEFAULT_MAX_VOICE_STATES: usize = 50_000;
const DEFAULT_MAX_SOUNDBOARD_SOUNDS: usize = 50_000;
const DEFAULT_MAX_EMOJIS: usize = 50_000;
const DEFAULT_MAX_STICKERS: usize = 50_000;
const DEFAULT_MAX_SCHEDULED_EVENTS: usize = 50_000;
const DEFAULT_MAX_STAGE_INSTANCES: usize = 50_000;
const DEFAULT_MESSAGE_TTL_SECS: u64 = 60 * 60;
const DEFAULT_PRESENCE_TTL_SECS: u64 = 10 * 60;
const DEFAULT_MEMBER_TTL_SECS: u64 = 24 * 60 * 60;

#[derive(Clone, Debug, PartialEq, Eq)]
/// Typed Discord API object for `CacheConfig`.
pub struct CacheConfig {
    pub max_messages_per_channel: Option<usize>,
    pub max_total_messages: Option<usize>,
    pub max_presences: Option<usize>,
    pub max_members_per_guild: Option<usize>,
    pub max_guilds: Option<usize>,
    pub max_channels: Option<usize>,
    pub max_users: Option<usize>,
    pub max_roles: Option<usize>,
    pub max_voice_states: Option<usize>,
    pub max_soundboard_sounds: Option<usize>,
    pub max_emojis: Option<usize>,
    pub max_stickers: Option<usize>,
    pub max_scheduled_events: Option<usize>,
    pub max_stage_instances: Option<usize>,
    pub message_ttl: Option<Duration>,
    pub presence_ttl: Option<Duration>,
    pub member_ttl: Option<Duration>,
    pub cache_emojis: bool,
    pub cache_stickers: bool,
    pub cache_scheduled_events: bool,
    pub cache_stage_instances: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self::bounded()
    }
}

impl CacheConfig {
    /// Creates a `bounded` value.
    pub fn bounded() -> Self {
        Self {
            max_messages_per_channel: Some(DEFAULT_MAX_MESSAGES_PER_CHANNEL),
            max_total_messages: Some(DEFAULT_MAX_TOTAL_MESSAGES),
            max_presences: Some(DEFAULT_MAX_PRESENCES),
            max_members_per_guild: Some(DEFAULT_MAX_MEMBERS_PER_GUILD),
            max_guilds: Some(DEFAULT_MAX_GUILDS),
            max_channels: Some(DEFAULT_MAX_CHANNELS),
            max_users: Some(DEFAULT_MAX_USERS),
            max_roles: Some(DEFAULT_MAX_ROLES),
            max_voice_states: Some(DEFAULT_MAX_VOICE_STATES),
            max_soundboard_sounds: Some(DEFAULT_MAX_SOUNDBOARD_SOUNDS),
            max_emojis: Some(DEFAULT_MAX_EMOJIS),
            max_stickers: Some(DEFAULT_MAX_STICKERS),
            max_scheduled_events: Some(DEFAULT_MAX_SCHEDULED_EVENTS),
            max_stage_instances: Some(DEFAULT_MAX_STAGE_INSTANCES),
            message_ttl: Some(Duration::from_secs(DEFAULT_MESSAGE_TTL_SECS)),
            presence_ttl: Some(Duration::from_secs(DEFAULT_PRESENCE_TTL_SECS)),
            member_ttl: Some(Duration::from_secs(DEFAULT_MEMBER_TTL_SECS)),
            cache_emojis: true,
            cache_stickers: true,
            cache_scheduled_events: true,
            cache_stage_instances: true,
        }
    }

    /// Creates a `unbounded` value.
    pub fn unbounded() -> Self {
        Self {
            max_messages_per_channel: None,
            max_total_messages: None,
            max_presences: None,
            max_members_per_guild: None,
            max_guilds: None,
            max_channels: None,
            max_users: None,
            max_roles: None,
            max_voice_states: None,
            max_soundboard_sounds: None,
            max_emojis: None,
            max_stickers: None,
            max_scheduled_events: None,
            max_stage_instances: None,
            message_ttl: None,
            presence_ttl: None,
            member_ttl: None,
            cache_emojis: true,
            cache_stickers: true,
            cache_scheduled_events: true,
            cache_stage_instances: true,
        }
    }

    pub fn max_messages_per_channel(mut self, max: usize) -> Self {
        self.max_messages_per_channel = Some(max);
        self
    }

    pub fn max_total_messages(mut self, max: usize) -> Self {
        self.max_total_messages = Some(max);
        self
    }

    pub fn max_presences(mut self, max: usize) -> Self {
        self.max_presences = Some(max);
        self
    }

    pub fn max_members_per_guild(mut self, max: usize) -> Self {
        self.max_members_per_guild = Some(max);
        self
    }

    pub fn max_guilds(mut self, max: usize) -> Self {
        self.max_guilds = Some(max);
        self
    }

    pub fn max_channels(mut self, max: usize) -> Self {
        self.max_channels = Some(max);
        self
    }

    pub fn max_users(mut self, max: usize) -> Self {
        self.max_users = Some(max);
        self
    }

    pub fn max_roles(mut self, max: usize) -> Self {
        self.max_roles = Some(max);
        self
    }

    pub fn max_voice_states(mut self, max: usize) -> Self {
        self.max_voice_states = Some(max);
        self
    }

    pub fn max_soundboard_sounds(mut self, max: usize) -> Self {
        self.max_soundboard_sounds = Some(max);
        self
    }

    pub fn max_emojis(mut self, max: usize) -> Self {
        self.max_emojis = Some(max);
        self
    }

    pub fn max_stickers(mut self, max: usize) -> Self {
        self.max_stickers = Some(max);
        self
    }

    pub fn max_scheduled_events(mut self, max: usize) -> Self {
        self.max_scheduled_events = Some(max);
        self
    }

    pub fn max_stage_instances(mut self, max: usize) -> Self {
        self.max_stage_instances = Some(max);
        self
    }

    pub fn message_ttl(mut self, ttl: Duration) -> Self {
        self.message_ttl = Some(ttl);
        self
    }

    pub fn presence_ttl(mut self, ttl: Duration) -> Self {
        self.presence_ttl = Some(ttl);
        self
    }

    pub fn member_ttl(mut self, ttl: Duration) -> Self {
        self.member_ttl = Some(ttl);
        self
    }

    pub fn cache_emojis(mut self, enabled: bool) -> Self {
        self.cache_emojis = enabled;
        self
    }

    pub fn cache_stickers(mut self, enabled: bool) -> Self {
        self.cache_stickers = enabled;
        self
    }

    pub fn cache_scheduled_events(mut self, enabled: bool) -> Self {
        self.cache_scheduled_events = enabled;
        self
    }

    pub fn cache_stage_instances(mut self, enabled: bool) -> Self {
        self.cache_stage_instances = enabled;
        self
    }
}
