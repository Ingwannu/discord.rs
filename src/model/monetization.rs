//! SKUs, entitlements, and subscriptions.

use serde::{Deserialize, Serialize};

use super::Snowflake;

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Sku`.
pub struct Sku {
    pub id: Snowflake,
    #[serde(rename = "type")]
    pub kind: u8,
    pub application_id: Snowflake,
    pub name: String,
    pub slug: String,
    pub flags: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependent_sku_id: Option<Snowflake>,
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

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Entitlement`.
pub struct Entitlement {
    pub id: Snowflake,
    pub sku_id: Snowflake,
    pub application_id: Snowflake,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub promotion_id: Option<Snowflake>,
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
    pub guild_id: Option<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consumed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscription_id: Option<Snowflake>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `CreateTestEntitlement`.
pub struct CreateTestEntitlement {
    pub sku_id: Snowflake,
    pub owner_id: Snowflake,
    pub owner_type: u8,
}

#[derive(Clone, Debug, Default)]
/// Typed Discord API object for `EntitlementQuery`.
pub struct EntitlementQuery {
    pub user_id: Option<Snowflake>,
    pub sku_ids: Vec<Snowflake>,
    pub before: Option<Snowflake>,
    pub after: Option<Snowflake>,
    pub limit: Option<u64>,
    pub guild_id: Option<Snowflake>,
    pub exclude_ended: Option<bool>,
    pub exclude_deleted: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
/// Typed Discord API object for `Subscription`.
pub struct Subscription {
    pub id: Snowflake,
    pub user_id: Snowflake,
    #[serde(default)]
    pub sku_ids: Vec<Snowflake>,
    #[serde(default)]
    pub entitlement_ids: Vec<Snowflake>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub renewal_sku_ids: Option<Vec<Snowflake>>,
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
    pub before: Option<Snowflake>,
    pub after: Option<Snowflake>,
    pub limit: Option<u64>,
    pub user_id: Option<Snowflake>,
}
