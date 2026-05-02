use reqwest::Method;
use serde_json::Value;

use crate::error::DiscordError;
use crate::model::{
    CreateTestEntitlement, Entitlement, EntitlementQuery, Sku, Snowflake, Subscription,
    SubscriptionQuery,
};

use super::{configured_application_id, entitlement_query, subscription_query, RestClient};

impl RestClient {
    pub async fn get_skus(&self) -> Result<Vec<Sku>, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request_typed(
            Method::GET,
            &format!("/applications/{application_id}/skus"),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_sku_subscriptions(
        &self,
        sku_id: impl Into<Snowflake>,
        query: &SubscriptionQuery,
    ) -> Result<Vec<Subscription>, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!(
                "/skus/{}/subscriptions{}",
                sku_id.into(),
                subscription_query(query)
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_sku_subscription(
        &self,
        sku_id: impl Into<Snowflake>,
        subscription_id: impl Into<Snowflake>,
    ) -> Result<Subscription, DiscordError> {
        self.request_typed(
            Method::GET,
            &format!(
                "/skus/{}/subscriptions/{}",
                sku_id.into(),
                subscription_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_entitlements(
        &self,
        query: &EntitlementQuery,
    ) -> Result<Vec<Entitlement>, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request_typed(
            Method::GET,
            &format!(
                "/applications/{application_id}/entitlements{}",
                entitlement_query(query)
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn get_entitlement(
        &self,
        entitlement_id: impl Into<Snowflake>,
    ) -> Result<Entitlement, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request_typed(
            Method::GET,
            &format!(
                "/applications/{application_id}/entitlements/{}",
                entitlement_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn consume_entitlement(
        &self,
        entitlement_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request_no_content(
            Method::POST,
            &format!(
                "/applications/{application_id}/entitlements/{}/consume",
                entitlement_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }

    pub async fn create_test_entitlement(
        &self,
        body: &CreateTestEntitlement,
    ) -> Result<Entitlement, DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request_typed(
            Method::POST,
            &format!("/applications/{application_id}/entitlements"),
            Some(body),
        )
        .await
    }

    pub async fn delete_test_entitlement(
        &self,
        entitlement_id: impl Into<Snowflake>,
    ) -> Result<(), DiscordError> {
        let application_id = configured_application_id(self.application_id())?;
        self.request_no_content(
            Method::DELETE,
            &format!(
                "/applications/{application_id}/entitlements/{}",
                entitlement_id.into()
            ),
            Option::<&Value>::None,
        )
        .await
    }
}
