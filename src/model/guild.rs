//! Guild objects, guild queries, and guild-level request bodies.

use serde::{Deserialize, Serialize};

use crate::types::Emoji;

use super::{
    ApplicationId, Channel, ChannelId, ChannelType, DefaultMessageNotificationLevel,
    ExplicitContentFilterLevel, GuildId, GuildNsfwLevel, GuildScheduledEvent, Member, MfaLevel,
    OverwriteType, Permissions, PremiumTier, Presence, Role, SoundboardSound, StageInstance,
    Sticker, UserId, VerificationLevel, VoiceState,
};

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `GetGuildQuery`.
pub struct GetGuildQuery {
    pub with_counts: Option<bool>,
}

/// Request body for `POST /guilds` (create guild; bot must be in fewer
/// than 10 guilds).
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct CreateGuild {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_level: Option<VerificationLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_message_notifications: Option<DefaultMessageNotificationLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explicit_content_filter: Option<ExplicitContentFilterLevel>,
    /// Roles to create; the first entry edits `@everyone`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roles: Option<Vec<CreateGuildPlaceholderRole>>,
    /// Channels to create (categories before their children).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channels: Option<Vec<CreateGuildPlaceholderChannel>>,
    /// Placeholder of an entry in [`Self::channels`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub afk_channel_id: Option<CreateGuildPlaceholderId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub afk_timeout: Option<u64>,
    /// Placeholder of an entry in [`Self::channels`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_channel_id: Option<CreateGuildPlaceholderId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_channel_flags: Option<u64>,
}

/// Local reference inside one [`CreateGuild`] body.
///
/// The legacy create-guild route let a request name the roles and channels
/// it was creating with small integers (the `@everyone` entry was usually
/// `0`) so that channel `parent_id`, permission overwrites, and
/// `afk_channel_id`/`system_channel_id` could point at them before Discord
/// assigned real IDs. Those numbers are not snowflakes and mean nothing
/// outside the request, so they get this type instead of [`GenericId`](super::GenericId)
/// (docs/design/3.0-types-events-requests.md §1 exceptions). Serialized as a
/// JSON integer. Only used by the deprecated `RestClient::create_guild`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CreateGuildPlaceholderId(u64);

impl CreateGuildPlaceholderId {
    /// Wraps a placeholder number; `0` is allowed.
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the placeholder number.
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Role entry of a [`CreateGuild`] body.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CreateGuildPlaceholderRole {
    /// Placeholder other entries use to refer to this role.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<CreateGuildPlaceholderId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hoist: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Permissions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mentionable: Option<bool>,
}

/// Channel entry of a [`CreateGuild`] body.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CreateGuildPlaceholderChannel {
    /// Placeholder other entries use to refer to this channel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<CreateGuildPlaceholderId>,
    pub name: String,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<ChannelType>,
    /// Placeholder of a category channel listed earlier.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<CreateGuildPlaceholderId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission_overwrites: Option<Vec<CreateGuildPlaceholderOverwrite>>,
}

/// Permission overwrite of a [`CreateGuildPlaceholderChannel`], targeting a
/// role placeholder.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateGuildPlaceholderOverwrite {
    /// Placeholder of a role in [`CreateGuild::roles`].
    pub id: CreateGuildPlaceholderId,
    /// Always [`OverwriteType::ROLE`] in practice: the legacy route only
    /// lets overwrites point at role placeholders.
    #[serde(rename = "type")]
    pub kind: OverwriteType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow: Option<Permissions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deny: Option<Permissions>,
}

/// Request body for creating a guild from a guild template
/// (`POST /guilds/templates/{code}`).
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct CreateGuildFromTemplate {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

/// Request body and response shape for `POST /guilds/{id}/mfa`.
///
/// No `Default`: `level` is required and has no protocol default.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct GuildMfaLevel {
    pub level: MfaLevel,
}

impl GuildMfaLevel {
    /// A body that sets the guild's moderation MFA requirement to `level`.
    pub const fn new(level: MfaLevel) -> Self {
        Self { level }
    }
}

/// Request body for modifying a guild.
///
/// The outer `Option` controls omission. Use `Some(None)` to send JSON `null`
/// for nullable Discord fields such as image data, linked channel IDs, or
/// description.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyGuild {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_level: Option<Option<VerificationLevel>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_message_notifications: Option<Option<DefaultMessageNotificationLevel>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explicit_content_filter: Option<Option<ExplicitContentFilterLevel>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub afk_channel_id: Option<Option<ChannelId>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub afk_timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub splash: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discovery_splash: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banner: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_channel_id: Option<Option<ChannelId>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_channel_flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rules_channel_id: Option<Option<ChannelId>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_updates_channel_id: Option<Option<ChannelId>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_locale: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub features: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub premium_progress_bar_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safety_alerts_channel_id: Option<Option<ChannelId>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `Guild`.
pub struct Guild {
    pub id: GuildId,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<UserId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable: Option<bool>,
    #[serde(default)]
    pub roles: Vec<Role>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub splash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub premium_tier: Option<PremiumTier>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_presences: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_members: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vanity_url_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_channel_id: Option<ChannelId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rules_channel_id: Option<ChannelId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_locale: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub afk_channel_id: Option<ChannelId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub afk_timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_level: Option<VerificationLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_message_notifications: Option<DefaultMessageNotificationLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explicit_content_filter: Option<ExplicitContentFilterLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mfa_level: Option<MfaLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application_id: Option<ApplicationId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_channel_flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_updates_channel_id: Option<ChannelId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nsfw_level: Option<GuildNsfwLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub premium_subscription_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub premium_progress_bar_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safety_alerts_channel_id: Option<ChannelId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approximate_member_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approximate_presence_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub incidents_data: Option<GuildIncidentsData>,
    // GUILD_CREATE-only fields. Discord sends these once per session on the
    // initial guild payload; REST guild fetches leave them empty. The
    // runtime distributes them into the per-entity caches and strips them
    // from the cached Guild copy to avoid double storage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub joined_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub large: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub channels: Vec<Channel>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub threads: Vec<Channel>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<Member>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub voice_states: Vec<VoiceState>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub presences: Vec<Presence>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub emojis: Vec<crate::types::Emoji>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stickers: Vec<Sticker>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stage_instances: Vec<StageInstance>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub soundboard_sounds: Vec<SoundboardSound>,
    /// Scheduled events in the guild (GUILD_CREATE only).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub guild_scheduled_events: Vec<GuildScheduledEvent>,
}

impl Guild {
    /// Returns a copy without the bulk GUILD_CREATE collections
    /// (channels, threads, members, voice states, presences, emojis,
    /// stickers, stage instances, soundboard sounds, scheduled events).
    /// The cache stores this stripped form once the collections have been
    /// distributed into their per-entity stores.
    pub fn without_create_collections(&self) -> Guild {
        Guild {
            channels: Vec::new(),
            threads: Vec::new(),
            members: Vec::new(),
            voice_states: Vec::new(),
            presences: Vec::new(),
            emojis: Vec::new(),
            stickers: Vec::new(),
            stage_instances: Vec::new(),
            soundboard_sounds: Vec::new(),
            guild_scheduled_events: Vec::new(),
            ..self.clone()
        }
    }
}

/// Active safety incident actions configured for a guild.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct GuildIncidentsData {
    /// Timestamp when invite creation will be re-enabled, or `null` when invite blocking is disabled.
    pub invites_disabled_until: Option<String>,
    /// Timestamp when direct messages will be re-enabled, or `null` when DM blocking is disabled.
    pub dms_disabled_until: Option<String>,
    /// Timestamp when Discord detected DM spam, if present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dm_spam_detected_at: Option<String>,
    /// Timestamp when Discord detected a raid, if present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raid_detected_at: Option<String>,
}

/// Request body for modifying guild incident actions.
///
/// Supplying `None` serializes `null`, which disables that incident action.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyGuildIncidentActions {
    /// Timestamp when invites should be re-enabled, or `null` to disable invite blocking.
    pub invites_disabled_until: Option<String>,
    /// Timestamp when direct messages should be re-enabled, or `null` to disable DM blocking.
    pub dms_disabled_until: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `CurrentUserGuild`.
pub struct CurrentUserGuild {
    pub id: GuildId,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub banner: Option<String>,
    #[serde(default)]
    pub owner: bool,
    /// The current user's total permissions in the guild (a string on the
    /// wire, like every permission set).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Permissions>,
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approximate_member_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approximate_presence_count: Option<u64>,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `CurrentUserGuildsQuery`.
pub struct CurrentUserGuildsQuery {
    pub before: Option<GuildId>,
    pub after: Option<GuildId>,
    pub limit: Option<u64>,
    pub with_counts: Option<bool>,
    /// Returns only the guilds stored in this shard (`0` to
    /// `max_concurrency - 1` from Get Gateway Bot). Required since
    /// 2026-09-15 for apps using large bot sharding, which otherwise get a
    /// 400; this is not the gateway shard id.
    pub shard: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `GuildPreview`.
pub struct GuildPreview {
    pub id: GuildId,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub splash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discovery_splash: Option<String>,
    #[serde(default)]
    pub emojis: Vec<Emoji>,
    #[serde(default)]
    pub features: Vec<String>,
    pub approximate_member_count: u64,
    pub approximate_presence_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub stickers: Vec<Sticker>,
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::Guild;
    use crate::model::{
        test_id, RecurrenceRuleFrequency, RecurrenceRuleWeekday, ScheduledEventEntityType,
        ScheduledEventPrivacyLevel, ScheduledEventStatus,
    };

    /// Two scheduled events shaped per Guild Scheduled Event Structure: a
    /// voice event with every nullable field null (and `creator_id` null, as
    /// for events created before October 2021), and a recurring external
    /// event ("Every Wednesday" from the recurrence rule examples).
    fn scheduled_events() -> Value {
        json!([
            {
                "id": "1000000000000000001",
                "guild_id": "197038439483310086",
                "channel_id": "1000000000000000010",
                "creator_id": null,
                "name": "Game night",
                "description": null,
                "scheduled_start_time": "2026-10-20T19:00:00+00:00",
                "scheduled_end_time": null,
                "privacy_level": 2,
                "status": 1,
                "entity_type": 2,
                "entity_id": null,
                "entity_metadata": null,
                "image": null,
                "recurrence_rule": null
            },
            {
                "id": "1000000000000000002",
                "guild_id": "197038439483310086",
                "channel_id": null,
                "creator_id": "80351110224678912",
                "name": "Office hours",
                "description": "Ask us anything",
                "scheduled_start_time": "2026-10-21T17:00:00+00:00",
                "scheduled_end_time": "2026-10-21T18:00:00+00:00",
                "privacy_level": 2,
                "status": 1,
                "entity_type": 3,
                "entity_id": null,
                "entity_metadata": { "location": "https://example.com/hall" },
                "creator": { "id": "80351110224678912", "username": "Nelly" },
                "user_count": 12,
                "image": "a_cover",
                "recurrence_rule": {
                    "start": "2026-10-21T17:00:00+00:00",
                    "end": null,
                    "frequency": 2,
                    "interval": 1,
                    "by_weekday": [2],
                    "by_n_weekday": null,
                    "by_month": null,
                    "by_month_day": null,
                    "by_year_day": null,
                    "count": null
                }
            }
        ])
    }

    fn guild_create() -> Value {
        json!({
            "id": "197038439483310086",
            "name": "Discord Testers",
            "guild_scheduled_events": scheduled_events()
        })
    }

    fn check(guild: &Guild) {
        let [voice, external] = &guild.guild_scheduled_events[..] else {
            panic!("two scheduled events");
        };
        assert_eq!(voice.id, test_id(1000000000000000001));
        assert_eq!(voice.channel_id, Some(test_id(1000000000000000010)));
        assert_eq!(voice.creator_id, None);
        assert_eq!(voice.entity_type, ScheduledEventEntityType::VOICE);
        assert_eq!(voice.privacy_level, ScheduledEventPrivacyLevel::GUILD_ONLY);
        assert!(voice.entity_metadata.is_none() && voice.recurrence_rule.is_none());

        assert_eq!(external.entity_type, ScheduledEventEntityType::EXTERNAL);
        assert_eq!(external.status, ScheduledEventStatus::SCHEDULED);
        assert_eq!(
            external
                .entity_metadata
                .as_ref()
                .and_then(|metadata| metadata.location.as_deref()),
            Some("https://example.com/hall")
        );
        assert_eq!(external.user_count, Some(12));
        let rule = external.recurrence_rule.as_ref().unwrap();
        assert_eq!(rule.frequency, RecurrenceRuleFrequency::WEEKLY);
        assert_eq!(
            rule.by_weekday.as_deref(),
            Some(&[RecurrenceRuleWeekday::WEDNESDAY][..])
        );
    }

    /// The written events: every key that had a value comes back, and the
    /// nulls are left out.
    fn without_nulls(value: Value) -> Value {
        match value {
            Value::Object(map) => Value::Object(
                map.into_iter()
                    .filter(|(_, value)| !value.is_null())
                    .map(|(key, value)| (key, without_nulls(value)))
                    .collect(),
            ),
            Value::Array(items) => Value::Array(items.into_iter().map(without_nulls).collect()),
            other => other,
        }
    }

    #[test]
    fn scheduled_events_decode_typed_from_text_and_value() {
        let text = guild_create().to_string();
        let from_text: Guild = serde_json::from_str(&text).unwrap();
        check(&from_text);
        let from_value: Guild = serde_json::from_value(guild_create()).unwrap();
        check(&from_value);

        let written = serde_json::to_value(&from_value).unwrap();
        assert_eq!(
            written["guild_scheduled_events"],
            without_nulls(scheduled_events())
        );
        // The cached copy drops them like the other GUILD_CREATE lists.
        assert!(from_value
            .without_create_collections()
            .guild_scheduled_events
            .is_empty());
    }

    #[test]
    fn guild_create_carries_typed_scheduled_events() {
        // The gateway decodes `d` from its text once the phase 4 envelope
        // lands and from a cloned `Value` before that; both must type it.
        let payload = guild_create();
        let from_text: Guild = serde_json::from_str(&payload.to_string()).unwrap();
        check(&from_text);
        let from_value: Guild = serde_json::from_value(payload).unwrap();
        check(&from_value);
    }
}
