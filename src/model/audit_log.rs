//! Guild audit log entries and queries.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use super::{
    ApplicationCommand, ApplicationId, AuditLogActionType, AuditLogEntryId, AutoModerationRule,
    Channel, ChannelId, GenericId, GuildScheduledEvent, Integration, MessageId, OverwriteTarget,
    OverwriteType, RoleId, User, UserId, Webhook,
};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `AuditLogEntry`.
pub struct AuditLogEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<AuditLogEntryId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<UserId>,
    /// The affected entity: a user, role, channel, webhook, emoji, ...
    /// depending on `action_type` (and absent for some actions), so it stays
    /// a [`GenericId`]. Use [`Id::cast`](super::Id::cast) once the action type
    /// tells you the kind.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_id: Option<GenericId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_type: Option<AuditLogActionType>,
    /// Changes made to the target, one entry per changed field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changes: Option<Vec<AuditLogChange>>,
    /// Extra information Discord adds for some action types.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<AuditLogEntryOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// One changed field of an audit log entry (Discord docs: Audit Log Change
/// Object).
///
/// `new_value` and `old_value` have the type of the changed field, which
/// depends on [`Self::key`] and on the entry's action type, so they stay raw
/// JSON. Decode them with [`Self::new_value_as`] and [`Self::old_value_as`]
/// once `key` tells you the type:
///
/// ```
/// use discordrs::{AuditLogChange, AuditLogPartialRole};
///
/// # fn main() -> Result<(), serde_json::Error> {
/// let change: AuditLogChange = serde_json::from_str(
///     r#"{"key": "$add", "new_value": [{"id": "41771983423143936", "name": "Mods"}]}"#,
/// )?;
/// assert_eq!(change.key, AuditLogChange::ROLE_ADD);
/// let added: Option<Vec<AuditLogPartialRole>> = change.new_value_as()?;
/// assert_eq!(added.map(|roles| roles[0].name.clone()), Some("Mods".to_string()));
/// # Ok(())
/// # }
/// ```
// [Decision Log]
// - 목적과 의도: 감사 로그 변경 값을 `serde_json::Value` 트리로 만들지 않고, 받은 JSON
//   텍스트를 한 번 복사해 보관한다. 값의 타입이 필요한 호출자만 그때 디코딩한다.
// - 기존 구현 및 제약 조건: Discord는 값의 타입을 "mixed (matches object field's type)"로
//   문서화한다. 같은 키(`id`, `type`, `permissions` ...)가 변경된 객체 종류(채널, 역할,
//   커맨드 권한 ...)에 따라 다른 타입을 가지고, 커맨드 권한 변경은 키 자체가 snowflake다.
//   설계 문서 §6이 "이질적인 감사 값"을 정당한 불투명 필드로 남긴다.
// - 검토한 주요 대안: (1) `Value` 유지, (2) 키·액션 타입별 enum, (3) `Box<RawValue>` +
//   타입 지정 접근자.
// - 선택한 방식: (3). `null`과 생략은 둘 다 `None`이다(문서상 둘 다 "null로 초기화").
// - 다른 대안 대신 이 방식을 선택한 이유: (1)은 쓰지 않는 값까지 트리 노드마다 할당한다.
//   (2)는 액션 타입 수십 개 × 객체 필드 전부를 따라가야 하고, Discord가 필드를 더할 때마다
//   디코딩이 실패하거나 값을 잃는다. (3)은 할당 한 번으로 손실 없이 보관하고, 호출자가 아는
//   타입으로 바로 디코딩한다.
// - 장점, 단점 및 영향: `RawValue`는 `PartialEq`가 없어 이 타입도 비교할 수 없다(감사 로그
//   타입은 원래 `PartialEq`가 없었다). `RawValue`는 serde의 버퍼링 경로(`flatten`,
//   `untagged`, 내부 태그)를 지나면 디코딩되지 않으므로, 이 타입을 그런 컨테이너 안에 넣지
//   않는다. `serde_json::from_str`/`from_slice`/`from_value`는 모두 동작한다.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuditLogChange {
    /// Name of the changed field, with Discord's documented exceptions:
    /// [`Self::ROLE_ADD`]/[`Self::ROLE_REMOVE`] for member role changes, a
    /// snowflake for command permission changes, `avatar_hash` for webhook
    /// avatars, and `channel_id` for invites.
    pub key: String,
    /// New value as raw JSON. `None` when Discord left it out or sent
    /// `null`, which both mean the field was reset to null.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_value: Option<Box<RawValue>>,
    /// Old value as raw JSON. `None` when the field was previously null.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_value: Option<Box<RawValue>>,
}

impl AuditLogChange {
    /// `key` of a change that added roles to a member; `new_value` is a list
    /// of [`AuditLogPartialRole`].
    pub const ROLE_ADD: &'static str = "$add";
    /// `key` of a change that removed roles from a member; `new_value` is a
    /// list of [`AuditLogPartialRole`].
    pub const ROLE_REMOVE: &'static str = "$remove";

    /// Decodes [`Self::new_value`] as `T`; `Ok(None)` when it is absent.
    pub fn new_value_as<T: DeserializeOwned>(&self) -> Result<Option<T>, serde_json::Error> {
        decode_raw(self.new_value.as_deref())
    }

    /// Decodes [`Self::old_value`] as `T`; `Ok(None)` when it is absent.
    pub fn old_value_as<T: DeserializeOwned>(&self) -> Result<Option<T>, serde_json::Error> {
        decode_raw(self.old_value.as_deref())
    }
}

fn decode_raw<T: DeserializeOwned>(raw: Option<&RawValue>) -> Result<Option<T>, serde_json::Error> {
    raw.map(|raw| serde_json::from_str(raw.get())).transpose()
}

/// A role in the `new_value` list of a [`AuditLogChange::ROLE_ADD`] or
/// [`AuditLogChange::ROLE_REMOVE`] change (Discord docs: Audit Log Change
/// Exceptions, Partial Role).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuditLogPartialRole {
    /// The added or removed role.
    pub id: RoleId,
    /// The role's name when the change happened.
    pub name: String,
}

/// Extra information on some audit log entries (Discord docs: Optional
/// Audit Entry Info). Which fields are set depends on the entry's action
/// type; every field is optional.
///
/// Discord sends `count`, `delete_member_days`, `members_removed`,
/// `auto_moderation_rule_trigger_type`, and the overwrite `type` as strings
/// of digits. They stay strings so an entry writes back exactly as received.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct AuditLogEntryOptions {
    /// App whose command permissions were targeted
    /// (`APPLICATION_COMMAND_PERMISSION_UPDATE`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub application_id: Option<ApplicationId>,
    /// Name of the Auto Moderation rule that was triggered
    /// (`AUTO_MODERATION_*`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_moderation_rule_name: Option<String>,
    /// Trigger type of the Auto Moderation rule that was triggered, as the
    /// string Discord sends (`AUTO_MODERATION_*`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_moderation_rule_trigger_type: Option<String>,
    /// Channel in which the entities were targeted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<ChannelId>,
    /// Number of entities that were targeted, as a string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<String>,
    /// Inactivity days after which members were pruned, as a string
    /// (`MEMBER_PRUNE`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete_member_days: Option<String>,
    /// Overwritten role or member (`CHANNEL_OVERWRITE_*`); its kind depends
    /// on [`Self::kind`], so it stays a [`GenericId`]. See
    /// [`Self::overwrite_target`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<GenericId>,
    /// Number of members removed by the prune, as a string (`MEMBER_PRUNE`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub members_removed: Option<String>,
    /// Message that was targeted (`MESSAGE_PIN`, `MESSAGE_UNPIN`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<MessageId>,
    /// Name of the overwritten role; absent for member overwrites.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role_name: Option<String>,
    /// Overwritten entity type as a string: `"0"` role, `"1"` member.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Type of integration which performed the action (`MEMBER_KICK`,
    /// `MEMBER_ROLE_UPDATE`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integration_type: Option<String>,
    /// New voice channel status (`VOICE_CHANNEL_STATUS_CREATE`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

impl AuditLogEntryOptions {
    /// The overwritten role or member of a `CHANNEL_OVERWRITE_*` entry: `id`
    /// read according to the string `type`. `None` when either is missing
    /// or `type` is not a number.
    pub fn overwrite_target(&self) -> Option<OverwriteTarget> {
        let kind = self.kind.as_deref()?.parse::<u8>().ok()?;
        Some(OverwriteTarget::from_wire(
            OverwriteType::new(kind),
            self.id?,
        ))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `AuditLog`.
pub struct AuditLog {
    #[serde(default)]
    pub application_commands: Vec<ApplicationCommand>,
    #[serde(default)]
    pub audit_log_entries: Vec<AuditLogEntry>,
    #[serde(default)]
    pub auto_moderation_rules: Vec<AutoModerationRule>,
    #[serde(default)]
    pub guild_scheduled_events: Vec<GuildScheduledEvent>,
    #[serde(default)]
    pub integrations: Vec<Integration>,
    #[serde(default)]
    pub threads: Vec<Channel>,
    #[serde(default)]
    pub users: Vec<User>,
    #[serde(default)]
    pub webhooks: Vec<Webhook>,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `AuditLogQuery`.
pub struct AuditLogQuery {
    pub user_id: Option<UserId>,
    pub action_type: Option<AuditLogActionType>,
    pub before: Option<AuditLogEntryId>,
    pub after: Option<AuditLogEntryId>,
    pub limit: Option<u64>,
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::{AuditLog, AuditLogChange, AuditLogEntry, AuditLogPartialRole};
    use crate::model::{
        test_id, AuditLogActionType, OverwriteTarget, PermissionOverwrite, Permissions,
    };

    /// Entries shaped like Discord's (Audit Log Entry Structure, Audit Log
    /// Change Structure and Exceptions, Optional Audit Entry Info). The docs
    /// have no full entry example.
    const ENTRIES: &str = r#"[
        {
            "id": "1100000000000000001",
            "user_id": "80351110224678912",
            "target_id": "41771983423143936",
            "action_type": 25,
            "changes": [
                { "key": "$add", "new_value": [{ "id": "41771983423143937", "name": "Mods" }] }
            ],
            "options": { "integration_type": "discord" }
        },
        {
            "id": "1100000000000000002",
            "user_id": "80351110224678912",
            "target_id": "41771983423143938",
            "action_type": 13,
            "changes": [
                { "key": "id", "new_value": "41771983423143939" },
                { "key": "type", "new_value": 0 },
                { "key": "allow", "new_value": "1024" },
                { "key": "deny", "new_value": "0" }
            ],
            "options": { "id": "41771983423143939", "type": "0", "role_name": "Mods" },
            "reason": "lock down"
        },
        {
            "id": "1100000000000000003",
            "user_id": "80351110224678912",
            "target_id": "41771983423143940",
            "action_type": 72,
            "options": { "channel_id": "41771983423143941", "count": "2" }
        },
        {
            "id": "1100000000000000004",
            "user_id": "80351110224678912",
            "target_id": null,
            "action_type": 21,
            "options": { "delete_member_days": "7", "members_removed": "3" }
        },
        {
            "id": "1100000000000000005",
            "user_id": "80351110224678912",
            "target_id": "41771983423143941",
            "action_type": 11,
            "changes": [
                { "key": "name", "old_value": "general", "new_value": "chat" },
                { "key": "topic", "old_value": "hello" },
                { "key": "permission_overwrites", "new_value": [
                    { "id": "41771983423143939", "type": 0, "allow": "1024", "deny": "0" }
                ] }
            ]
        }
    ]"#;

    fn check(entries: &[AuditLogEntry]) {
        assert_eq!(entries.len(), 5);

        let role_update = &entries[0];
        assert_eq!(
            role_update.action_type,
            Some(AuditLogActionType::MEMBER_ROLE_UPDATE)
        );
        let change = &role_update.changes.as_ref().unwrap()[0];
        assert_eq!(change.key, AuditLogChange::ROLE_ADD);
        let added: Vec<AuditLogPartialRole> = change.new_value_as().unwrap().unwrap();
        assert_eq!(
            added,
            vec![AuditLogPartialRole {
                id: test_id(41771983423143937),
                name: "Mods".to_string()
            }]
        );
        assert!(change.old_value.is_none());
        assert_eq!(
            role_update
                .options
                .as_ref()
                .unwrap()
                .integration_type
                .as_deref(),
            Some("discord")
        );

        let overwrite = &entries[1];
        let options = overwrite.options.as_ref().unwrap();
        assert_eq!(options.kind.as_deref(), Some("0"));
        assert_eq!(options.role_name.as_deref(), Some("Mods"));
        assert_eq!(
            options.overwrite_target(),
            Some(OverwriteTarget::Role(test_id(41771983423143939)))
        );
        let changes = overwrite.changes.as_ref().unwrap();
        assert_eq!(changes[1].new_value_as::<u8>().unwrap(), Some(0));
        assert_eq!(
            changes[2].new_value_as::<Permissions>().unwrap(),
            Some(Permissions::from_bits(1024))
        );
        // A value of the wrong type is an error, not a silent `None`.
        assert!(changes[2].new_value_as::<u64>().is_err());

        let delete = entries[2].options.as_ref().unwrap();
        assert_eq!(delete.channel_id, Some(test_id(41771983423143941)));
        assert_eq!(delete.count.as_deref(), Some("2"));
        assert_eq!(delete.overwrite_target(), None);

        let prune = entries[3].options.as_ref().unwrap();
        assert_eq!(prune.delete_member_days.as_deref(), Some("7"));
        assert_eq!(prune.members_removed.as_deref(), Some("3"));

        let rename = entries[4].changes.as_ref().unwrap();
        assert_eq!(
            rename[0].old_value_as::<String>().unwrap().as_deref(),
            Some("general")
        );
        assert_eq!(
            rename[0].new_value_as::<String>().unwrap().as_deref(),
            Some("chat")
        );
        // Only `old_value`: the topic was reset to null.
        assert_eq!(rename[1].new_value_as::<String>().unwrap(), None);
        let overwrites: Vec<PermissionOverwrite> = rename[2].new_value_as().unwrap().unwrap();
        assert_eq!(
            overwrites[0].target,
            OverwriteTarget::Role(test_id(41771983423143939))
        );
    }

    #[test]
    fn entries_decode_from_text_and_value_and_write_back_unchanged() {
        let from_text: Vec<AuditLogEntry> = serde_json::from_str(ENTRIES).unwrap();
        check(&from_text);

        let wire: Value = serde_json::from_str(ENTRIES).unwrap();
        let from_value: Vec<AuditLogEntry> = serde_json::from_value(wire.clone()).unwrap();
        check(&from_value);

        // Raw values keep their text, so every documented key and value
        // comes back; only the explicit `"target_id": null` is omitted.
        let mut expected = wire;
        if let Some(entry) = expected[3].as_object_mut() {
            entry.remove("target_id");
        }
        assert_eq!(serde_json::to_value(&from_text).unwrap(), expected);
        assert_eq!(serde_json::to_value(&from_value).unwrap(), expected);
    }

    #[test]
    fn a_null_new_value_reads_like_an_absent_one() {
        let change: AuditLogChange =
            serde_json::from_str(r#"{"key": "nick", "old_value": "old", "new_value": null}"#)
                .unwrap();
        assert!(change.new_value.is_none());
        assert_eq!(change.new_value_as::<String>().unwrap(), None);
        assert_eq!(
            serde_json::to_value(&change).unwrap(),
            json!({ "key": "nick", "old_value": "old" })
        );
    }

    #[test]
    fn audit_log_wrapper_decodes_typed_entries() {
        let wire = json!({
            "audit_log_entries": serde_json::from_str::<Value>(ENTRIES).unwrap(),
            "users": [{ "id": "80351110224678912", "username": "Nelly" }]
        });
        let log: AuditLog = serde_json::from_value(wire).unwrap();
        check(&log.audit_log_entries);
    }

    #[test]
    fn gateway_entry_create_keeps_typed_changes_and_options() {
        let mut entries: Value = serde_json::from_str(ENTRIES).unwrap();
        let mut data = entries[1].take();
        data["guild_id"] = json!("197038439483310086");
        // The gateway decodes `d` from its text once the phase 4 envelope
        // lands and from a cloned `Value` before that; both must type it.
        let from_text: AuditLogEntry = serde_json::from_str(&data.to_string()).unwrap();
        assert_eq!(
            from_text
                .options
                .and_then(|options| options.overwrite_target()),
            Some(OverwriteTarget::Role(test_id(41771983423143939)))
        );
        let entry: AuditLogEntry = serde_json::from_value(data).unwrap();
        let options = entry.options.unwrap();
        assert_eq!(
            options.overwrite_target(),
            Some(OverwriteTarget::Role(test_id(41771983423143939)))
        );
        assert_eq!(
            entry.changes.unwrap()[2]
                .new_value_as::<Permissions>()
                .unwrap(),
            Some(Permissions::from_bits(1024))
        );
    }
}
