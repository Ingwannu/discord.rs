//! Auxiliary guild resources: vanity URL, prune, integrations, widget,
//! welcome screen, templates, and onboarding.

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use crate::types::Emoji;

use super::{
    ApplicationId, ChannelId, EmojiId, GuildId, IntegrationExpireBehavior, IntegrationId,
    OnboardingMode, OnboardingOptionId, OnboardingPromptId, OnboardingPromptType, RoleId, User,
    UserId, WidgetUserId,
};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `VanityUrl`.
pub struct VanityUrl {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uses: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GuildPruneCount`.
pub struct GuildPruneCount {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pruned: Option<u64>,
}

/// Type alias for `GuildPruneResult`.
pub type GuildPruneResult = GuildPruneCount;

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `IntegrationAccount`.
pub struct IntegrationAccount {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `IntegrationApplication`.
pub struct IntegrationApplication {
    pub id: ApplicationId,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot: Option<User>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `Integration`.
pub struct Integration {
    pub id: IntegrationId,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub syncing: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role_id: Option<RoleId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_emoticons: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expire_behavior: Option<IntegrationExpireBehavior>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expire_grace_period: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<User>,
    pub account: IntegrationAccount,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub synced_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscriber_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revoked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application: Option<IntegrationApplication>,
    #[serde(default)]
    pub scopes: Vec<String>,
}

/// Partial integration object, as listed in a user connection's
/// `integrations` (Discord docs: Connection Structure).
///
/// Discord documents these only as "partial server integrations" without
/// saying which fields they carry, so only `id` is required and the rest
/// are the fields the documented partial integration example has. A full
/// [`Integration`] requires `name` and `account`, and one missing field
/// would fail the whole connection list.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PartialIntegration {
    /// Integration ID.
    pub id: IntegrationId,
    /// Integration name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Integration type (`twitch`, `youtube`, `discord`, or
    /// `guild_subscription`).
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Whether the integration is enabled.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// The integration's account on the external service.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<IntegrationAccount>,
    /// The integrated application, for app integrations. The documented
    /// partial example has this key, which a full [`Integration`] carries as
    /// `application.id` instead.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application_id: Option<ApplicationId>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GuildWidgetSettings`.
pub struct GuildWidgetSettings {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<ChannelId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `GuildWidgetChannel`.
pub struct GuildWidgetChannel {
    pub id: ChannelId,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `GuildWidgetMember`.
pub struct GuildWidgetMember {
    /// Anonymized by Discord (often `"0"`); not a user ID. See
    /// [`WidgetUserId`].
    pub id: WidgetUserId,
    pub username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discriminator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `GuildWidget`.
pub struct GuildWidget {
    pub id: GuildId,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instant_invite: Option<String>,
    #[serde(default)]
    pub channels: Vec<GuildWidgetChannel>,
    #[serde(default)]
    pub members: Vec<GuildWidgetMember>,
    #[serde(default)]
    pub presence_count: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Typed Discord API enum for `GuildWidgetImageStyle`.
pub enum GuildWidgetImageStyle {
    /// Discord API enum variant `Shield`.
    Shield,
    /// Discord API enum variant `Banner1`.
    Banner1,
    /// Discord API enum variant `Banner2`.
    Banner2,
    /// Discord API enum variant `Banner3`.
    Banner3,
    /// Discord API enum variant `Banner4`.
    Banner4,
}

impl GuildWidgetImageStyle {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Shield => "shield",
            Self::Banner1 => "banner1",
            Self::Banner2 => "banner2",
            Self::Banner3 => "banner3",
            Self::Banner4 => "banner4",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
/// Typed Discord API object for `WelcomeScreenChannel`.
pub struct WelcomeScreenChannel {
    pub channel_id: ChannelId,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji_id: Option<EmojiId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji_name: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `WelcomeScreen`.
pub struct WelcomeScreen {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub welcome_channels: Vec<WelcomeScreenChannel>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `GuildTemplate`.
pub struct GuildTemplate {
    pub code: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub usage_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator_id: Option<UserId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator: Option<User>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_guild_id: Option<GuildId>,
    /// The guild snapshot the template creates, as the raw JSON text
    /// received; decode it with `serde_json::from_str(raw.get())`.
    ///
    /// Opaque on purpose: the snapshot is not a [`Guild`](super::Guild). Its
    /// roles and channels use integer placeholder IDs (`0` for `@everyone`),
    /// role permissions are numbers rather than strings, the icon is
    /// `icon_hash`, and it has no guild ID, so typing it would need a
    /// parallel guild, role, channel, and overwrite model for a value most
    /// callers only pass back to Discord.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serialized_source_guild: Option<Box<RawValue>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_dirty: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `GuildOnboarding`.
pub struct GuildOnboarding {
    pub guild_id: GuildId,
    /// Prompts shown during onboarding and in Customize Community.
    #[serde(default)]
    pub prompts: Vec<OnboardingPrompt>,
    #[serde(default)]
    pub default_channel_ids: Vec<ChannelId>,
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<OnboardingMode>,
}

/// A question of a guild's onboarding flow (Discord docs: Onboarding Prompt
/// Structure). The same shape is read from [`GuildOnboarding`] and sent in
/// [`ModifyGuildOnboarding`](crate::request::ModifyGuildOnboarding). No
/// `Default`: `id`, `type`, and `title` are required.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OnboardingPrompt {
    /// ID of the prompt.
    pub id: OnboardingPromptId,
    /// How the options are shown.
    #[serde(rename = "type")]
    pub kind: OnboardingPromptType,
    /// Options available within the prompt.
    #[serde(default)]
    pub options: Vec<OnboardingPromptOption>,
    /// Title of the prompt.
    pub title: String,
    /// Whether users may select only one option.
    #[serde(default)]
    pub single_select: bool,
    /// Whether the prompt must be answered before onboarding completes.
    #[serde(default)]
    pub required: bool,
    /// Whether the prompt is in the onboarding flow; if `false`, it only
    /// appears in the Channels & Roles tab.
    #[serde(default)]
    pub in_onboarding: bool,
}

/// An answer of an [`OnboardingPrompt`] (Discord docs: Prompt Option
/// Structure).
///
/// Discord returns the option's emoji as [`Self::emoji`], but a create or
/// update must send `emoji_id`, `emoji_name`, and `emoji_animated` instead.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct OnboardingPromptOption {
    /// ID of the prompt option.
    pub id: OnboardingOptionId,
    /// Channels a member is added to when the option is selected.
    #[serde(default)]
    pub channel_ids: Vec<ChannelId>,
    /// Roles assigned to a member when the option is selected.
    #[serde(default)]
    pub role_ids: Vec<RoleId>,
    /// Emoji of the option, as Discord returns it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji: Option<Emoji>,
    /// Emoji ID to set, for create and update requests.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji_id: Option<EmojiId>,
    /// Emoji name to set, for create and update requests.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji_name: Option<String>,
    /// Whether the emoji to set is animated, for create and update requests.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emoji_animated: Option<bool>,
    /// Title of the option.
    pub title: String,
    /// Description of the option; nullable, so `None` is written as `null`.
    pub description: Option<String>,
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;
    use serde_json::{json, Value};

    use super::{GuildOnboarding, GuildTemplate, OnboardingPromptType};
    use crate::model::test_id;

    /// resources/guild.mdx — Example Guild Onboarding.
    const ONBOARDING: &str = r#"{
      "guild_id": "960007075288915998",
      "prompts": [
        {
          "id": "1067461047608422473",
          "title": "What do you want to do in this community?",
          "options": [
            {
              "id": "1067461047608422476",
              "title": "Chat with Friends",
              "description": "",
              "emoji": {
                "id": "1070002302032826408",
                "name": "chat",
                "animated": false
              },
              "role_ids": [],
              "channel_ids": [
                "962007075288916001"
              ]
            },
            {
              "id": "1070004843541954678",
              "title": "Get Gud",
              "description": "We have excellent teachers!",
              "emoji": {
                "id": null,
                "name": "😀",
                "animated": false
              },
              "role_ids": [
                "982014491980083211"
              ],
              "channel_ids": []
            }
          ],
          "single_select": false,
          "required": false,
          "in_onboarding": true,
          "type": 0
        }
      ],
      "default_channel_ids": [
        "998678771706110023",
        "998678693058719784",
        "1070008122577518632",
        "998678764340912138",
        "998678704446263309",
        "998678683592171602",
        "998678699715067986"
      ],
      "enabled": true
    }"#;

    /// resources/guild-template.mdx — Example Guild Template Object.
    const TEMPLATE: &str = r#"{
      "code": "hgM48av5Q69A",
      "name": "Friends & Family",
      "description": "",
      "usage_count": 49605,
      "creator_id": "132837293881950208",
      "creator": {
        "id": "132837293881950208",
        "username": "hoges",
        "avatar": "79b0d9f8c340f2d43e1f78b09f175b62",
        "discriminator": "0001",
        "public_flags": 129
      },
      "created_at": "2020-04-02T21:10:38+00:00",
      "updated_at": "2020-05-01T17:57:38+00:00",
      "source_guild_id": "678070694164299796",
      "serialized_source_guild": {
        "name": "Friends & Family",
        "description": null,
        "region": "us-west",
        "verification_level": 0,
        "default_message_notifications": 0,
        "explicit_content_filter": 0,
        "preferred_locale": "en-US",
        "afk_timeout": 300,
        "roles": [
          {
            "id": 0,
            "name": "@everyone",
            "permissions": 104324689,
            "color": 0,
            "hoist": false,
            "mentionable": false
          }
        ],
        "channels": [
          {
            "name": "Text Channels",
            "position": 1,
            "topic": null,
            "bitrate": 64000,
            "user_limit": 0,
            "nsfw": false,
            "rate_limit_per_user": 0,
            "parent_id": null,
            "permission_overwrites": [],
            "id": 1,
            "type": 4
          },
          {
            "name": "general",
            "position": 1,
            "topic": null,
            "bitrate": 64000,
            "user_limit": 0,
            "nsfw": false,
            "rate_limit_per_user": 0,
            "parent_id": 1,
            "permission_overwrites": [],
            "id": 2,
            "type": 0
          }
        ],
        "afk_channel_id": null,
        "system_channel_id": 2,
        "system_channel_flags": 0,
        "icon_hash": null
      },
      "is_dirty": null
    }"#;

    fn check_onboarding(onboarding: &GuildOnboarding) {
        let prompt = &onboarding.prompts[0];
        assert_eq!(prompt.id, test_id(1067461047608422473));
        assert_eq!(prompt.kind, OnboardingPromptType::MULTIPLE_CHOICE);
        assert!(!prompt.single_select && !prompt.required && prompt.in_onboarding);
        let chat = &prompt.options[0];
        assert_eq!(chat.id, test_id(1067461047608422476));
        assert_eq!(chat.description.as_deref(), Some(""));
        assert_eq!(chat.channel_ids, [test_id(962007075288916001)]);
        assert!(chat.role_ids.is_empty());
        let emoji = chat.emoji.as_ref().unwrap();
        assert_eq!(emoji.id, Some(test_id(1070002302032826408)));
        assert_eq!(emoji.name.as_deref(), Some("chat"));
        let gud = &prompt.options[1];
        assert_eq!(gud.role_ids, [test_id(982014491980083211)]);
        assert_eq!(gud.emoji.as_ref().unwrap().id, None);
        assert_eq!(gud.emoji_id, None);
    }

    #[test]
    fn documented_onboarding_prompts_are_typed_and_write_back() {
        let from_text: GuildOnboarding = serde_json::from_str(ONBOARDING).unwrap();
        check_onboarding(&from_text);
        let wire: Value = serde_json::from_str(ONBOARDING).unwrap();
        let from_value: GuildOnboarding = serde_json::from_value(wire.clone()).unwrap();
        check_onboarding(&from_value);

        // Everything comes back but the Unicode emoji's `"id": null`.
        let mut expected = wire;
        if let Some(emoji) = expected["prompts"][0]["options"][1]["emoji"].as_object_mut() {
            emoji.remove("id");
        }
        assert_eq!(serde_json::to_value(&from_value).unwrap(), expected);
    }

    #[test]
    fn a_null_option_description_is_written_as_null() {
        let option: super::OnboardingPromptOption = serde_json::from_value(json!({
            "id": "5", "title": "Lurk", "description": null, "channel_ids": [], "role_ids": []
        }))
        .unwrap();
        assert_eq!(option.description, None);
        assert_eq!(
            serde_json::to_value(&option).unwrap(),
            json!({
                "id": "5", "title": "Lurk", "description": null, "channel_ids": [], "role_ids": []
            })
        );
    }

    #[test]
    fn documented_template_keeps_the_snapshot_verbatim() {
        let from_text: GuildTemplate = serde_json::from_str(TEMPLATE).unwrap();
        let wire: Value = serde_json::from_str(TEMPLATE).unwrap();
        let from_value: GuildTemplate = serde_json::from_value(wire.clone()).unwrap();

        // Only the explicit `"is_dirty": null` is not written back.
        let mut expected = wire;
        if let Some(template) = expected.as_object_mut() {
            template.remove("is_dirty");
        }
        for template in [&from_text, &from_value] {
            assert_eq!(template.code, "hgM48av5Q69A");
            assert_eq!(serde_json::to_value(template).unwrap(), expected);
        }

        // The snapshot decodes on demand into the caller's own shape,
        // integer placeholder IDs included.
        #[derive(Deserialize)]
        struct Snapshot {
            name: String,
            channels: Vec<SnapshotChannel>,
        }
        #[derive(Deserialize)]
        struct SnapshotChannel {
            id: u64,
            parent_id: Option<u64>,
        }
        let raw = from_text.serialized_source_guild.as_deref().unwrap();
        let snapshot: Snapshot = serde_json::from_str(raw.get()).unwrap();
        assert_eq!(snapshot.name, "Friends & Family");
        assert_eq!(snapshot.channels[1].id, 2);
        assert_eq!(snapshot.channels[1].parent_id, Some(1));
    }
}
