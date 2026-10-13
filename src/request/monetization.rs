//! Monetization request bodies (moved from `model` in 3.0 checkpoint 5C).

use serde::Serialize;

use crate::model::{EntitlementOwner, EntitlementOwnerType, GenericId, SkuId};

/// Request body for creating a test entitlement.
///
/// Writes Discord's shape, `{"sku_id", "owner_id", "owner_type"}`, from
/// [`Self::owner`]: the owner's ID and its kind.
///
/// ```
/// use discordrs::model::EntitlementOwner;
/// use discordrs::request::CreateTestEntitlement;
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(into = "CreateTestEntitlementWire")]
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
#[derive(Clone, Copy, Serialize)]
struct CreateTestEntitlementWire {
    sku_id: SkuId,
    owner_id: GenericId,
    owner_type: EntitlementOwnerType,
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
