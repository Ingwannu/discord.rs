// Excluded by `rest_sources_exclude`: test harnesses are not endpoints.
impl RestClient {
    pub async fn harness_only(&self) -> Result<(), DiscordError> {
        self.request(Method::PUT, "/excluded/file").await.map(drop)
    }
}
