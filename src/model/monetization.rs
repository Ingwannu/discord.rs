//! SKUs, entitlements, and subscriptions.

use serde::{Deserialize, Serialize};

use super::GenericId;

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `Sku`.
pub struct Sku {
    pub id: GenericId,
    #[serde(rename = "type")]
    pub kind: u8,
    pub application_id: GenericId,
    pub name: String,
    pub slug: String,
    pub flags: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependent_sku_id: Option<GenericId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_labels: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_type: Option<u8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<serde_json::Value>,
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
    pub id: GenericId,
    pub sku_id: GenericId,
    pub application_id: GenericId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<GenericId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub promotion_id: Option<GenericId>,
    #[serde(rename = "type")]
    pub kind: u8,
    pub deleted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gift_code_flags: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub starts_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ends_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guild_id: Option<GenericId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consumed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscription_id: Option<GenericId>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `CreateTestEntitlement`.
pub struct CreateTestEntitlement {
    pub sku_id: GenericId,
    pub owner_id: GenericId,
    pub owner_type: u8,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `EntitlementQuery`.
pub struct EntitlementQuery {
    pub user_id: Option<GenericId>,
    pub sku_ids: Vec<GenericId>,
    pub before: Option<GenericId>,
    pub after: Option<GenericId>,
    pub limit: Option<u64>,
    pub guild_id: Option<GenericId>,
    pub exclude_ended: Option<bool>,
    pub exclude_deleted: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Typed Discord API object for `Subscription`.
pub struct Subscription {
    pub id: GenericId,
    pub user_id: GenericId,
    #[serde(default)]
    pub sku_ids: Vec<GenericId>,
    #[serde(default)]
    pub entitlement_ids: Vec<GenericId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub renewal_sku_ids: Option<Vec<GenericId>>,
    pub current_period_start: String,
    pub current_period_end: String,
    pub status: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canceled_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `SubscriptionQuery`.
pub struct SubscriptionQuery {
    pub before: Option<GenericId>,
    pub after: Option<GenericId>,
    pub limit: Option<u64>,
    pub user_id: Option<GenericId>,
}
