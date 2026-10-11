//! Invites and invite target-user requests.

use serde::{Deserialize, Serialize};

use super::{
    ApplicationId, Channel, Guild, GuildScheduledEvent, InviteTargetType,
    InviteTargetUsersJobStatusType, InviteType, PartialApplication, Role, RoleId, User, UserId,
};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Invite`.
pub struct Invite {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<InviteType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// The guild the invite is for, as a partial guild (ID, name, images,
    /// description, features, verification and NSFW levels, vanity code,
    /// boost count); the other [`Guild`] fields stay empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild: Option<Guild>,
    /// The channel the invite is for, as a partial channel (ID, name,
    /// type); `None` when Discord sends `null`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel: Option<Channel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inviter: Option<User>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_type: Option<InviteTargetType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_user: Option<User>,
    /// The embedded application to open for an embedded-application voice
    /// channel invite.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_application: Option<PartialApplication>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approximate_presence_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approximate_member_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
    /// The scheduled event the invite points at, when it was fetched with a
    /// valid `guild_scheduled_event_id`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_scheduled_event: Option<GuildScheduledEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub roles: Vec<Role>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uses: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_age: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temporary: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
/// Typed Discord API object for `CreateChannelInvite`.
pub struct CreateChannelInvite {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_age: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temporary: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unique: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_type: Option<InviteTargetType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_user_id: Option<UserId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_application_id: Option<ApplicationId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role_ids: Option<Vec<RoleId>>,
    /// Users allowed to see and accept the invite (max 1000). Mutually
    /// exclusive with a `target_users_file` CSV upload.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_user_ids: Option<Vec<UserId>>,
}

/// JSON body for the invite target-user bulk-add and bulk-delete routes.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct InviteTargetUserIds {
    /// Users to add or remove; Discord accepts at most 1000 per request.
    pub user_ids: Vec<UserId>,
}

/// Progress of an invite target-users job. No `Default`: every field but
/// the completion details is required.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct InviteTargetUsersJobStatus {
    pub status: InviteTargetUsersJobStatusType,
    pub total_users: u64,
    pub processed_users: u64,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::Invite;
    use crate::model::{
        test_id, ChannelType, GuildNsfwLevel, InviteTargetType, ScheduledEventEntityType,
        VerificationLevel,
    };

    /// resources/invite.mdx — Example Invite Object.
    const INVITE: &str = r#"{
      "type": 0,
      "code": "0vCdhLbwjZZTWZLD",
      "guild": {
        "id": "165176875973476352",
        "name": "CS:GO Fraggers Only",
        "splash": null,
        "banner": null,
        "description": "Very good description",
        "icon": null,
        "features": ["NEWS", "DISCOVERABLE"],
        "verification_level": 2,
        "vanity_url_code": null,
        "nsfw_level": 0,
        "premium_subscription_count": 5
      },
      "channel": {
        "id": "165176875973476352",
        "name": "illuminati",
        "type": 0
      },
      "inviter": {
        "id": "115590097100865541",
        "username": "speed",
        "avatar": "deadbeef",
        "discriminator": "7653",
        "public_flags": 131328
      },
      "target_type": 1,
      "target_user": {
        "id": "165176875973476352",
        "username": "bob",
        "avatar": "deadbeef",
        "discriminator": "1234",
        "public_flags": 64
      }
    }"#;

    fn check_documented(invite: &Invite) {
        let guild = invite.guild.as_ref().unwrap();
        assert_eq!(guild.id, test_id(165176875973476352));
        assert_eq!(guild.name, "CS:GO Fraggers Only");
        assert_eq!(guild.description.as_deref(), Some("Very good description"));
        assert_eq!(guild.features, ["NEWS", "DISCOVERABLE"]);
        assert_eq!(guild.verification_level, Some(VerificationLevel::MEDIUM));
        assert_eq!(guild.nsfw_level, Some(GuildNsfwLevel::DEFAULT));
        assert_eq!(guild.premium_subscription_count, Some(5));
        let channel = invite.channel.as_ref().unwrap();
        assert_eq!(channel.id, test_id(165176875973476352));
        assert_eq!(channel.name.as_deref(), Some("illuminati"));
        assert_eq!(channel.kind, ChannelType::GUILD_TEXT);
        assert_eq!(invite.target_type, Some(InviteTargetType::STREAM));
    }

    /// The written invite keeps every documented key with a value; the
    /// typed partial guild adds only its empty `roles` list and leaves the
    /// nulls out.
    #[test]
    fn documented_invite_has_a_typed_guild_and_channel() {
        let from_text: Invite = serde_json::from_str(INVITE).unwrap();
        check_documented(&from_text);
        let wire: Value = serde_json::from_str(INVITE).unwrap();
        let from_value: Invite = serde_json::from_value(wire.clone()).unwrap();
        check_documented(&from_value);

        let written = serde_json::to_value(&from_value).unwrap();
        assert_eq!(
            written["guild"],
            json!({
                "id": "165176875973476352",
                "name": "CS:GO Fraggers Only",
                "description": "Very good description",
                "features": ["NEWS", "DISCOVERABLE"],
                "verification_level": 2,
                "nsfw_level": 0,
                "premium_subscription_count": 5,
                "roles": []
            })
        );
        assert_eq!(written["channel"], wire["channel"]);
    }

    #[test]
    fn embedded_application_and_event_invites_are_typed() {
        let text = r#"{
          "type": 0,
          "code": "abc123",
          "channel": null,
          "expires_at": null,
          "target_type": 2,
          "target_application": {
            "id": "880218394199220334",
            "name": "Watch Together",
            "icon": "fe2b7fa334817b0346d57416ad75e93b",
            "description": "Create and watch a playlist of videos with your friends.",
            "flags": 131072
          },
          "guild_scheduled_event": {
            "id": "1000000000000000002",
            "guild_id": "165176875973476352",
            "channel_id": null,
            "creator_id": "115590097100865541",
            "name": "Movie night",
            "description": null,
            "scheduled_start_time": "2026-10-21T17:00:00+00:00",
            "scheduled_end_time": "2026-10-21T19:00:00+00:00",
            "privacy_level": 2,
            "status": 1,
            "entity_type": 3,
            "entity_id": null,
            "entity_metadata": { "location": "Theater" },
            "user_count": 4,
            "image": null
          }
        }"#;
        let from_text: Invite = serde_json::from_str(text).unwrap();
        let from_value: Invite =
            serde_json::from_value(serde_json::from_str(text).unwrap()).unwrap();
        for invite in [&from_text, &from_value] {
            assert!(invite.channel.is_none() && invite.expires_at.is_none());
            assert_eq!(
                invite.target_type,
                Some(InviteTargetType::EMBEDDED_APPLICATION)
            );
            let application = invite.target_application.as_ref().unwrap();
            assert_eq!(application.id, test_id(880218394199220334));
            assert_eq!(application.name.as_deref(), Some("Watch Together"));
            assert_eq!(application.flags, Some(131072));
            let event = invite.guild_scheduled_event.as_ref().unwrap();
            assert_eq!(event.entity_type, ScheduledEventEntityType::EXTERNAL);
            assert_eq!(event.user_count, Some(4));

            let written = serde_json::to_value(invite).unwrap();
            assert_eq!(
                written["target_application"],
                json!({
                    "id": "880218394199220334",
                    "name": "Watch Together",
                    "icon": "fe2b7fa334817b0346d57416ad75e93b",
                    "description": "Create and watch a playlist of videos with your friends.",
                    "flags": 131072
                })
            );
            assert_eq!(written["guild_scheduled_event"]["entity_type"], json!(3));
            assert_eq!(
                written["guild_scheduled_event"]["entity_metadata"],
                json!({ "location": "Theater" })
            );
        }
    }
}
