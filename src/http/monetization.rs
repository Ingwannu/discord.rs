use crate::error::DiscordError;
use crate::model::{
    Entitlement, EntitlementId, EntitlementQuery, Sku, SkuId, Subscription, SubscriptionId,
    SubscriptionQuery,
};
use crate::request::CreateTestEntitlement;

use super::{
    configured_application_id, entitlement_query, json_body, subscription_query, RestClient,
    RouteId, NO_BODY,
};

impl RestClient {
    pub async fn get_skus(&self) -> Result<Vec<Sku>, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.send(
            RouteId::ListSkus,
            &format!("/applications/{application_id}/skus"),
            NO_BODY,
        )
        .await
    }

    pub async fn get_sku_subscriptions(
        &self,
        sku_id: SkuId,
        query: &SubscriptionQuery,
    ) -> Result<Vec<Subscription>, DiscordError> {
        self.send(
            RouteId::ListSkuSubscriptions,
            &format!(
                "/skus/{}/subscriptions{}",
                sku_id,
                subscription_query(query)
            ),
            NO_BODY,
        )
        .await
    }

    pub async fn get_sku_subscription(
        &self,
        sku_id: SkuId,
        subscription_id: SubscriptionId,
    ) -> Result<Subscription, DiscordError> {
        self.send(
            RouteId::GetSkuSubscription,
            &format!("/skus/{}/subscriptions/{}", sku_id, subscription_id),
            NO_BODY,
        )
        .await
    }

    pub async fn get_entitlements(
        &self,
        query: &EntitlementQuery,
    ) -> Result<Vec<Entitlement>, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.send(
            RouteId::ListEntitlements,
            &format!(
                "/applications/{application_id}/entitlements{}",
                entitlement_query(query)
            ),
            NO_BODY,
        )
        .await
    }

    pub async fn get_entitlement(
        &self,
        entitlement_id: EntitlementId,
    ) -> Result<Entitlement, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.send(
            RouteId::GetEntitlement,
            &format!(
                "/applications/{application_id}/entitlements/{}",
                entitlement_id
            ),
            NO_BODY,
        )
        .await
    }

    pub async fn consume_entitlement(
        &self,
        entitlement_id: EntitlementId,
    ) -> Result<(), DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.send_unit(
            RouteId::ConsumeEntitlement,
            &format!(
                "/applications/{application_id}/entitlements/{}/consume",
                entitlement_id
            ),
            NO_BODY,
        )
        .await
    }

    pub async fn create_test_entitlement(
        &self,
        body: &CreateTestEntitlement,
    ) -> Result<Entitlement, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.send(
            RouteId::CreateTestEntitlement,
            &format!("/applications/{application_id}/entitlements"),
            json_body(body),
        )
        .await
    }

    pub async fn delete_test_entitlement(
        &self,
        entitlement_id: EntitlementId,
    ) -> Result<(), DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.send_unit(
            RouteId::DeleteTestEntitlement,
            &format!(
                "/applications/{application_id}/entitlements/{}",
                entitlement_id
            ),
            NO_BODY,
        )
        .await
    }
}
