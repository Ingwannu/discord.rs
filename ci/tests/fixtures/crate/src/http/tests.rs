// Excluded by `rest_sources_exclude`: test harness paths are not coverage.
async fn harness(client: &RestClient) {
    client.request(Method::PUT, "/excluded/file", None).await.unwrap();
}
