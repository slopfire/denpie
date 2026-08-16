use std::{sync::OnceLock, time::Duration};

static SHARED: OnceLock<reqwest::Client> = OnceLock::new();
static LLM: OnceLock<reqwest::Client> = OnceLock::new();

pub(crate) const SHARED_REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
pub(crate) const LLM_REQUEST_TIMEOUT: Duration = Duration::from_secs(300);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

fn build_client(timeout: Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(CONNECT_TIMEOUT)
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

/// Short outbound HTTP (autoupdate, document fetches).
pub fn shared() -> &'static reqwest::Client {
    SHARED.get_or_init(|| build_client(SHARED_REQUEST_TIMEOUT))
}

/// Chat and vision completions. Reasoning models plus OpenRouter web search
/// routinely exceed the shared 60s budget; aborting mid-body is reported by
/// reqwest as a JSON decode error, not a timeout.
pub fn llm() -> &'static reqwest::Client {
    LLM.get_or_init(|| build_client(LLM_REQUEST_TIMEOUT))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn llm_timeout_outlives_shared_client() {
        assert!(LLM_REQUEST_TIMEOUT > SHARED_REQUEST_TIMEOUT);
        assert_eq!(LLM_REQUEST_TIMEOUT.as_secs(), 300);
        assert_eq!(SHARED_REQUEST_TIMEOUT.as_secs(), 60);
    }
}
