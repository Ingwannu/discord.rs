//! Applications, activity instances, and OAuth2 authorization info.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{
    ActivityLaunchId, ApplicationEventWebhookStatus, ApplicationId, ApplicationIntegrationType,
    ChannelId, GuildId, Permissions, RoleConnectionMetadataType, SkuId, TeamId,
    TeamMembershipState, User, UserId,
};

/// Partial application object attached to messages sent by an app
/// (e.g. Rich Presence or activity invites).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PartialApplication {
    pub id: ApplicationId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot: Option<User>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_sku_id: Option<SkuId>,
}

/// A running application Activity instance.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActivityInstance {
    /// Application ID that owns the Activity.
    pub application_id: ApplicationId,
    /// Activity instance ID.
    pub instance_id: String,
    /// Unique launch ID for this Activity session.
    pub launch_id: ActivityLaunchId,
    /// Location where the Activity instance is running.
    pub location: ActivityLocation,
    /// Users currently connected to the Activity instance.
    #[serde(default)]
    pub users: Vec<UserId>,
}

/// Location where an application Activity instance is running.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActivityLocation {
    /// Unique location ID.
    pub id: String,
    /// Location kind, such as `gc` for guild channel or `pc` for private channel.
    pub kind: String,
    /// Channel ID for this location.
    pub channel_id: ChannelId,
    /// Guild ID for guild-channel Activity locations.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<GuildId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `Application`.
pub struct Application {
    pub id: ApplicationId,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub description: String,
    #[serde(default)]
    pub rpc_origins: Vec<String>,
    #[serde(default)]
    pub bot_public: bool,
    #[serde(default)]
    pub bot_require_code_grant: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terms_of_service_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub privacy_policy_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<User>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verify_key: Option<String>,
    /// The developer team that owns the app, if a team owns it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team: Option<Team>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<GuildId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_sku_id: Option<SkuId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approximate_guild_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approximate_user_install_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approximate_user_authorization_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect_uris: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interactions_endpoint_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role_connections_verification_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_webhooks_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_webhooks_status: Option<ApplicationEventWebhookStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_webhooks_types: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_install_url: Option<String>,
    /// Settings for the app's default in-app authorization link.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_params: Option<ApplicationInstallParams>,
    /// Default install settings per supported installation context. The
    /// wire keys are the integration types as strings (`"0"`, `"1"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integration_types_config:
        Option<HashMap<ApplicationIntegrationType, ApplicationIntegrationTypeConfig>>,
}

/// Developer team that owns an application (Discord docs: Teams › Team
/// Object). No `Default`: `id`, `name`, and `owner_user_id` are required.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Team {
    /// Hash of the team's icon image.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Unique ID of the team.
    pub id: TeamId,
    /// Members of the team.
    #[serde(default)]
    pub members: Vec<TeamMember>,
    /// Name of the team.
    pub name: String,
    /// The team owner. No member's `role` names the owner.
    pub owner_user_id: UserId,
}

/// Member of a developer [`Team`] (Discord docs: Teams › Team Member
/// Object).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TeamMember {
    /// Whether the user has accepted the team invite.
    pub membership_state: TeamMembershipState,
    /// ID of the team the user belongs to.
    pub team_id: TeamId,
    /// The member, as a partial user (ID, username, avatar,
    /// discriminator).
    pub user: User,
    /// Team member role: `admin`, `developer`, or `read_only` (Discord
    /// docs: Team Member Role Types). Optional although the table lists it
    /// as required, because Discord's own Application example leaves it out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

/// Default OAuth2 install settings for an application install context.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApplicationInstallParams {
    pub scopes: Vec<String>,
    pub permissions: Permissions,
}

/// Default install settings for one application integration type.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ApplicationIntegrationTypeConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth2_install_params: Option<ApplicationInstallParams>,
}

/// Request body for editing the current application.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ModifyCurrentApplication {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_install_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role_connections_verification_url: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_params: Option<ApplicationInstallParams>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integration_types_config: Option<HashMap<String, ApplicationIntegrationTypeConfig>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cover_image: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interactions_endpoint_url: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_webhooks_url: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_webhooks_status: Option<ApplicationEventWebhookStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_webhooks_types: Option<Vec<String>>,
}

/// One application role connection metadata record. No `Default`: `type`,
/// `key`, `name`, and `description` are required; use
/// [`ApplicationRoleConnectionMetadata::new`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApplicationRoleConnectionMetadata {
    #[serde(rename = "type")]
    pub kind: RoleConnectionMetadataType,
    pub key: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_localizations: Option<HashMap<String, String>>,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description_localizations: Option<HashMap<String, String>>,
}

impl ApplicationRoleConnectionMetadata {
    /// A record comparing the user's `key` value with `kind`; localizations
    /// unset.
    pub fn new(
        kind: RoleConnectionMetadataType,
        key: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            key: key.into(),
            name: name.into(),
            name_localizations: None,
            description: description.into(),
            description_localizations: None,
        }
    }
}

/// Typed Discord API object for current OAuth2 authorization information.
/// No `Default`: `application` and `expires` are required.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthorizationInformation {
    /// The authorized application. Discord documents a partial application
    /// object; its example carries `id`, `name`, `description`, and the
    /// bot settings, which [`Application`] models with only `id`, `name`,
    /// and `description` required. [`PartialApplication`] would drop the bot
    /// settings and `verify_key`, and widening it would grow every
    /// [`Message`](super::Message) that embeds one.
    pub application: Application,
    /// OAuth2 scopes granted to the authorization.
    #[serde(default)]
    pub scopes: Vec<String>,
    pub expires: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// User that authorized the application when the token includes `identify`.
    pub user: Option<User>,
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::{Application, AuthorizationInformation, Team};
    use crate::model::{test_id, ApplicationIntegrationType, Permissions, TeamMembershipState};

    /// resources/application.mdx — Example Application Object, plus
    /// `install_params` (Install Params Structure) which the example lacks.
    const APPLICATION: &str = r#"{
      "bot_public": true,
      "bot_require_code_grant": false,
      "cover_image": "31deabb7e45b6c8ecfef77d2f99c81a5",
      "description": "Test",
      "flags": 8192,
      "guild_id": "290926798626357260",
      "icon": null,
      "id": "172150183260323840",
      "install_params": { "scopes": ["applications.commands", "bot"], "permissions": "2048" },
      "integration_types_config": {
        "0": {
          "oauth2_install_params": {
            "scopes": ["applications.commands", "bot"],
            "permissions": "2048"
          }
        },
        "1": {
          "oauth2_install_params": {
            "scopes": ["applications.commands"],
            "permissions": "0"
          }
        }
      },
      "name": "Baba O-Riley",
      "owner": {
        "avatar": null,
        "discriminator": "1738",
        "flags": 1024,
        "id": "172150183260323840",
        "username": "i own a bot"
      },
      "primary_sku_id": "172150183260323840",
      "slug": "test",
      "team": {
        "name": "A team",
        "icon": "dd9b7dcfdf5351b9c3de0fe167bacbe1",
        "id": "531992624043786253",
        "owner_user_id": "511972282709709995",
        "members": [
          {
            "membership_state": 2,
            "permissions": ["*"],
            "team_id": "531992624043786253",
            "user": {
              "avatar": "d9e261cd35999608eb7e3de1fae3688b",
              "discriminator": "0001",
              "id": "511972282709709995",
              "username": "Mr Owner"
            }
          }
        ]
      },
      "verify_key": "1e0a356058d627ca38a5c8c9648818061d49e49bd9da9e3ab17d98ad4d6bg2u8"
    }"#;

    /// topics/oauth2.mdx — Example Authorization Information.
    const AUTHORIZATION: &str = r#"{
        "application": {
            "id": "159799960412356608",
            "name": "AIRHORN SOLUTIONS",
            "icon": "f03590d3eb764081d154a66340ea7d6d",
            "description": "",
            "hook": true,
            "bot_public": true,
            "bot_require_code_grant": false,
            "verify_key": "c8cde6a3c8c6e49d86af3191287b3ce255872be1fff6dc285bdb420c06a2c3c8"
        },
        "scopes": ["guilds.join", "identify"],
        "expires": "2021-01-23T02:33:17.017000+00:00",
        "user": {
            "id": "268473310986240001",
            "username": "discord",
            "avatar": "f749bb0cbeeb26ef21eca719337d20f1",
            "discriminator": "0",
            "global_name": "Discord",
            "public_flags": 131072
        }
    }"#;

    fn check_application(application: &Application) {
        let team = application.team.as_ref().unwrap();
        assert_eq!(team.id, test_id(531992624043786253));
        assert_eq!(team.name, "A team");
        assert_eq!(team.owner_user_id, test_id(511972282709709995));
        let member = &team.members[0];
        assert_eq!(member.membership_state, TeamMembershipState::ACCEPTED);
        assert_eq!(member.team_id, team.id);
        assert_eq!(member.user.username, "Mr Owner");
        assert_eq!(member.role, None);

        let install = application.install_params.as_ref().unwrap();
        assert_eq!(install.scopes, ["applications.commands", "bot"]);
        assert_eq!(install.permissions, Permissions::from_bits(2048));

        let config = application.integration_types_config.as_ref().unwrap();
        let guild = config[&ApplicationIntegrationType::GUILD_INSTALL]
            .oauth2_install_params
            .as_ref()
            .unwrap();
        assert_eq!(guild.permissions, Permissions::from_bits(2048));
        let user = config[&ApplicationIntegrationType::USER_INSTALL]
            .oauth2_install_params
            .as_ref()
            .unwrap();
        assert_eq!(user.scopes, ["applications.commands"]);
    }

    #[test]
    fn documented_application_decodes_team_and_install_settings() {
        let from_text: Application = serde_json::from_str(APPLICATION).unwrap();
        check_application(&from_text);

        let wire: Value = serde_json::from_str(APPLICATION).unwrap();
        let from_value: Application = serde_json::from_value(wire.clone()).unwrap();
        check_application(&from_value);

        let written = serde_json::to_value(&from_value).unwrap();
        assert_eq!(written["install_params"], wire["install_params"]);
        assert_eq!(
            written["integration_types_config"],
            wire["integration_types_config"]
        );
        // Every documented team key comes back. The member's `permissions`
        // is a legacy key the Team Member table no longer lists.
        let mut team = wire["team"].clone();
        if let Some(member) = team["members"][0].as_object_mut() {
            member.remove("permissions");
        }
        assert_eq!(written["team"], team);
    }

    #[test]
    fn team_member_roles_and_unknown_membership_states_round_trip() {
        let wire = json!({
            "icon": null,
            "id": "1",
            "name": "Ops",
            "owner_user_id": "2",
            "members": [{
                "membership_state": 9,
                "team_id": "1",
                "user": { "id": "3", "username": "dev" },
                "role": "developer"
            }]
        });
        let team: Team = serde_json::from_value(wire).unwrap();
        assert_eq!(team.members[0].role.as_deref(), Some("developer"));
        assert_eq!(team.members[0].membership_state.name(), None);
        assert_eq!(
            serde_json::to_value(&team).unwrap(),
            json!({
                "id": "1",
                "name": "Ops",
                "owner_user_id": "2",
                "members": [{
                    "membership_state": 9,
                    "team_id": "1",
                    "user": { "id": "3", "username": "dev" },
                    "role": "developer"
                }]
            })
        );
    }

    #[test]
    fn documented_authorization_information_decodes_its_application() {
        for info in [
            serde_json::from_str::<AuthorizationInformation>(AUTHORIZATION).unwrap(),
            serde_json::from_value::<AuthorizationInformation>(
                serde_json::from_str(AUTHORIZATION).unwrap(),
            )
            .unwrap(),
        ] {
            let application = &info.application;
            assert_eq!(application.id, test_id(159799960412356608));
            assert_eq!(application.name, "AIRHORN SOLUTIONS");
            assert_eq!(application.description, "");
            assert!(application.bot_public);
            assert!(!application.bot_require_code_grant);
            assert!(application
                .verify_key
                .as_deref()
                .is_some_and(|key| key.starts_with("c8cde6")));
            assert_eq!(info.scopes, ["guilds.join", "identify"]);
            assert_eq!(info.user.as_ref().unwrap().username, "discord");

            // Every documented application key comes back, plus the empty
            // `rpc_origins` default; `hook` is a legacy key the Application
            // table does not list.
            let wire: Value = serde_json::from_str(AUTHORIZATION).unwrap();
            let mut application = wire["application"].clone();
            if let Some(object) = application.as_object_mut() {
                object.remove("hook");
                object.insert("rpc_origins".to_string(), json!([]));
            }
            let written = serde_json::to_value(&info).unwrap();
            assert_eq!(written["application"], application);
            assert_eq!(written["user"], wire["user"]);
        }
    }
}
