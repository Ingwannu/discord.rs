//! SKUs, entitlements, and subscriptions.

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use super::{
    ApplicationId, EntitlementId, EntitlementOwnerType, EntitlementType, GenericId, GuildId,
    PromotionId, SkuAccessType, SkuId, SkuType, SubscriptionId, SubscriptionStatus, UserId,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `Sku`.
pub struct Sku {
    pub id: SkuId,
    #[serde(rename = "type")]
    pub kind: SkuType,
    pub application_id: ApplicationId,
    pub name: String,
    pub slug: String,
    pub flags: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependent_sku_id: Option<SkuId>,
    /// Raw JSON text as received. Undocumented: the SKU table does not
    /// list it and Discord's SKU examples only ever send `null`, so there
    /// is no shape to type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_labels: Option<Box<RawValue>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_type: Option<SkuAccessType>,
    /// Entries as raw JSON text. Undocumented: the SKU table does not list
    /// it and Discord's SKU examples only ever send `[]`, so there is no
    /// element shape to type.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<Box<RawValue>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub premium: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_age_gate: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `Entitlement`.
pub struct Entitlement {
    pub id: EntitlementId,
    pub sku_id: SkuId,
    pub application_id: ApplicationId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<UserId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub promotion_id: Option<PromotionId>,
    #[serde(rename = "type")]
    pub kind: EntitlementType,
    pub deleted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gift_code_flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub starts_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ends_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<GuildId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consumed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscription_id: Option<SubscriptionId>,
}

/// Who an entitlement is granted to: a guild or a user.
///
/// Used where Discord takes an owner as `owner_id` plus `owner_type`
/// ([`CreateTestEntitlement`](crate::request::CreateTestEntitlement)), so the
/// ID always matches its type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EntitlementOwner {
    /// `owner_type: 1`: a guild.
    Guild(GuildId),
    /// `owner_type: 2`: a user.
    User(UserId),
}

impl EntitlementOwner {
    /// The wire `owner_id`.
    pub const fn id(self) -> GenericId {
        match self {
            Self::Guild(id) => id.erase(),
            Self::User(id) => id.erase(),
        }
    }

    /// The wire `owner_type`.
    pub const fn kind(self) -> EntitlementOwnerType {
        match self {
            Self::Guild(_) => EntitlementOwnerType::GUILD,
            Self::User(_) => EntitlementOwnerType::USER,
        }
    }
}

impl From<GuildId> for EntitlementOwner {
    fn from(id: GuildId) -> Self {
        Self::Guild(id)
    }
}

impl From<UserId> for EntitlementOwner {
    fn from(id: UserId) -> Self {
        Self::User(id)
    }
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `EntitlementQuery`.
pub struct EntitlementQuery {
    pub user_id: Option<UserId>,
    pub sku_ids: Vec<SkuId>,
    pub before: Option<EntitlementId>,
    pub after: Option<EntitlementId>,
    pub limit: Option<u64>,
    pub guild_id: Option<GuildId>,
    pub exclude_ended: Option<bool>,
    pub exclude_deleted: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `Subscription`.
pub struct Subscription {
    pub id: SubscriptionId,
    pub user_id: UserId,
    #[serde(default)]
    pub sku_ids: Vec<SkuId>,
    #[serde(default)]
    pub entitlement_ids: Vec<EntitlementId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub renewal_sku_ids: Option<Vec<SkuId>>,
    pub current_period_start: String,
    pub current_period_end: String,
    pub status: SubscriptionStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canceled_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `SubscriptionQuery`.
pub struct SubscriptionQuery {
    pub before: Option<SubscriptionId>,
    pub after: Option<SubscriptionId>,
    pub limit: Option<u64>,
    pub user_id: Option<UserId>,
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::Sku;
    use crate::model::SkuType;

    /// resources/sku.mdx — SKU Example.
    const SKU: &str = r#"{
      "id": "1088510058284990888",
      "type": 5,
      "dependent_sku_id": null,
      "application_id": "788708323867885999",
      "manifest_labels": null,
      "access_type": 1,
      "name": "Test Premium",
      "features": [],
      "release_date": null,
      "premium": false,
      "slug": "test-premium",
      "flags": 128,
      "show_age_gate": false
    }"#;

    #[test]
    fn documented_sku_decodes_with_empty_undocumented_fields() {
        let from_text: Sku = serde_json::from_str(SKU).unwrap();
        let from_value: Sku = serde_json::from_value(serde_json::from_str(SKU).unwrap()).unwrap();
        for sku in [&from_text, &from_value] {
            assert_eq!(sku.kind, SkuType::SUBSCRIPTION);
            assert!(sku.manifest_labels.is_none());
            assert!(sku.features.is_empty());
        }
    }

    #[test]
    fn undocumented_sku_fields_round_trip_verbatim() {
        let text = r#"{
          "id": "1088510058284990888",
          "type": 5,
          "application_id": "788708323867885999",
          "name": "Test Premium",
          "slug": "test-premium",
          "flags": 128,
          "manifest_labels": {"en-US": "Premium"},
          "features": ["FEATURE_A", {"id": 7}]
        }"#;
        let from_text: Sku = serde_json::from_str(text).unwrap();
        assert_eq!(
            from_text.manifest_labels.as_deref().map(|raw| raw.get()),
            Some(r#"{"en-US": "Premium"}"#)
        );
        assert_eq!(
            from_text
                .features
                .iter()
                .map(|raw| raw.get())
                .collect::<Vec<_>>(),
            [r#""FEATURE_A""#, r#"{"id": 7}"#]
        );
        let wire: Value = serde_json::from_str(text).unwrap();
        let from_value: Sku = serde_json::from_value(wire.clone()).unwrap();
        for sku in [&from_text, &from_value] {
            let written = serde_json::to_value(sku).unwrap();
            assert_eq!(written["manifest_labels"], json!({ "en-US": "Premium" }));
            assert_eq!(written["features"], wire["features"]);
        }
    }
}
