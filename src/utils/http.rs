//! Process-wide HTTP clients, so outbound calls reuse pooled connections.
use std::sync::OnceLock;

/// Default client; callers set a per-request timeout.
pub fn shared() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(reqwest::Client::new)
}

/// Client that never follows redirects, for fetches restricted to allowed hosts.
pub fn no_redirect() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_default()
    })
}
