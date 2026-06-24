use std::sync::OnceLock;

static SHARED: OnceLock<reqwest::Client> = OnceLock::new();

pub fn shared() -> &'static reqwest::Client {
    SHARED.get_or_init(reqwest::Client::new)
}
