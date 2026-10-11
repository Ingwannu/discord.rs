//! SKUs, entitlements, and subscriptions.

use serde::{Deserialize, Serialize};

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_labels: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_type: Option<SkuAccessType>,
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
/// ([`CreateTestEntitlement`]), so the ID always matches its type.
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

/// Request body for creating a test entitlement.
///
/// Reads and writes Discord's shape, `{"sku_id", "owner_id", "owner_type"}`;
/// `owner_id` and `owner_type` together become [`Self::owner`]. Decoding an
/// `owner_type` other than `1` or `2` is an error.
///
/// ```
/// use discordrs::model::{CreateTestEntitlement, EntitlementOwner};
/// use discordrs::{GuildId, SkuId};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let sku_id: SkuId = "1088510058284990888".parse()?;
/// let guild_id: GuildId = "1088510053662900303".parse()?;
/// let body = CreateTestEntitlement::new(sku_id, EntitlementOwner::Guild(guild_id));
/// assert_eq!(
///     serde_json::to_string(&body)?,
///     r#"{"sku_id":"1088510058284990888","owner_id":"1088510053662900303","owner_type":1}"#
/// );
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    try_from = "CreateTestEntitlementWire",
    into = "CreateTestEntitlementWire"
)]
pub struct CreateTestEntitlement {
    pub sku_id: SkuId,
    /// The guild or user that receives the entitlement.
    pub owner: EntitlementOwner,
}

impl CreateTestEntitlement {
    /// Grants `sku_id` to `owner`.
    pub const fn new(sku_id: SkuId, owner: EntitlementOwner) -> Self {
        Self { sku_id, owner }
    }
}

/// The wire shape of [`CreateTestEntitlement`], unchanged from 2.x.
#[derive(Clone, Copy, Serialize, Deserialize)]
struct CreateTestEntitlementWire {
    sku_id: SkuId,
    owner_id: GenericId,
    owner_type: EntitlementOwnerType,
}

impl TryFrom<CreateTestEntitlementWire> for CreateTestEntitlement {
    type Error = String;

    fn try_from(wire: CreateTestEntitlementWire) -> Result<Self, Self::Error> {
        let owner = match wire.owner_type.get() {
            1 => EntitlementOwner::Guild(wire.owner_id.cast()),
            2 => EntitlementOwner::User(wire.owner_id.cast()),
            other => return Err(format!("unknown entitlement owner_type {other}")),
        };
        Ok(Self::new(wire.sku_id, owner))
    }
}

impl From<CreateTestEntitlement> for CreateTestEntitlementWire {
    fn from(body: CreateTestEntitlement) -> Self {
        Self {
            sku_id: body.sku_id,
            owner_id: body.owner.id(),
            owner_type: body.owner.kind(),
        }
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
