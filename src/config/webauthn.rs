use std::sync::Arc;
use url::Url;
use webauthn_rs::prelude::Webauthn;

pub struct WebauthnSetup {
    pub webauthn: Arc<Webauthn>,
    pub rp_origin: Url,
    pub session_secure: bool,
}

pub fn setup() -> WebauthnSetup {
    let rp_origin_str = std::env::var("DENPIE_RP_ORIGIN")
        .or_else(|_| std::env::var("PUBLIC_URL"))
        .unwrap_or_else(|_| "http://localhost:3017".to_string());
    let rp_origin = Url::parse(&rp_origin_str).expect("Invalid DENPIE_RP_ORIGIN or PUBLIC_URL");
    let rp_id = std::env::var("DENPIE_RP_ID").unwrap_or_else(|_| {
        rp_origin
            .host_str()
            .expect("DENPIE_RP_ORIGIN must include a host")
            .to_string()
    });

    let mut builder = webauthn_rs::WebauthnBuilder::new(&rp_id, &rp_origin)
        .expect("Invalid webauthn configuration");

    if let Some(sibling) = www_sibling_origin(&rp_origin) {
        builder = builder.append_allowed_origin(&sibling);
    }

    for extra in parse_extra_origins() {
        builder = builder.append_allowed_origin(&extra);
    }

    let webauthn = Arc::new(builder.build().expect("Invalid webauthn configuration"));
    let session_secure = std::env::var_os("DENPIE_PROD").is_some() || rp_origin.scheme() == "https";

    WebauthnSetup {
        webauthn,
        rp_origin,
        session_secure,
    }
}

pub fn warn_if_passkeys_misconfigured(bind_addr: &std::net::SocketAddr, rp_origin: &Url) {
    let localhost_origin = matches!(rp_origin.host_str(), Some("localhost") | Some("127.0.0.1"));
    let listens_beyond_loopback =
        bind_addr.ip().is_unspecified() || (!bind_addr.ip().is_loopback() && bind_addr.port() != 0);
    if localhost_origin && listens_beyond_loopback {
        tracing::warn!(
            %bind_addr,
            origin = %rp_origin,
            "Passkeys require DENPIE_RP_ORIGIN (and usually DENPIE_RP_ID) to match the public \
             HTTPS URL users open in the browser; localhost RP settings break passkeys for \
             remote clients (typical Docker/reverse-proxy deploys)"
        );
    }
}

fn parse_extra_origins() -> Vec<Url> {
    std::env::var("DENPIE_RP_EXTRA_ORIGINS")
        .unwrap_or_default()
        .split(',')
        .filter_map(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                return None;
            }
            Some(Url::parse(trimmed).unwrap_or_else(|err| {
                panic!("Invalid origin in DENPIE_RP_EXTRA_ORIGINS ({trimmed}): {err}")
            }))
        })
        .collect()
}

fn www_sibling_origin(origin: &Url) -> Option<Url> {
    let host = origin.host_str()?;
    if host == "localhost" || host.parse::<std::net::IpAddr>().is_ok() {
        return None;
    }
    if let Some(rest) = host.strip_prefix("www.") {
        Url::parse(&format!("{}://{rest}", origin.scheme())).ok()
    } else {
        Url::parse(&format!("{}://www.{host}", origin.scheme())).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn www_sibling_for_https_site() {
        let origin = Url::parse("https://denpie.com").unwrap();
        let sibling = www_sibling_origin(&origin).unwrap();
        assert_eq!(sibling.as_str(), "https://www.denpie.com/");
    }

    #[test]
    fn www_sibling_strips_www() {
        let origin = Url::parse("https://www.example.com").unwrap();
        let sibling = www_sibling_origin(&origin).unwrap();
        assert_eq!(sibling.as_str(), "https://example.com/");
    }

    #[test]
    fn www_sibling_skips_localhost() {
        let origin = Url::parse("http://localhost:3017").unwrap();
        assert!(www_sibling_origin(&origin).is_none());
    }
}
