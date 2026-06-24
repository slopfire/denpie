use std::{sync::OnceLock, time::Duration};

static SHARED: OnceLock<reqwest::Client> = OnceLock::new();

const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

pub fn shared() -> &'static reqwest::Client {
    SHARED.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .connect_timeout(CONNECT_TIMEOUT)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new())
    })
}
