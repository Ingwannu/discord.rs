use std::time::Duration;

use reqwest::Client;
use tracing::warn;

const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

pub(super) fn default_http_client() -> Client {
    match Client::builder()
        .connect_timeout(DEFAULT_CONNECT_TIMEOUT)
        .timeout(DEFAULT_REQUEST_TIMEOUT)
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            warn!(
                error = %error,
                "failed to build bounded reqwest client; falling back to reqwest defaults"
            );
            Client::new()
        }
    }
}
