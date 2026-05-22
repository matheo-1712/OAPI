//! Centralized management for external API endpoints.
//!
//! This module retrieves URLs for external services from the
//! application's configuration.

use crate::config::Config;

// ──────────────────────────────────────────────
//  External APIs Endpoints
// ──────────────────────────────────────────────
/// Returns the full URL for fetching a Discord user's information.
pub fn discord_user_url() -> &'static str {
    &Config::global().external_apis.discord_user
}

/// Returns the full URL for fetching a Discord user's statistics.
pub fn discord_stats_url() -> &'static str {
    &Config::global().external_apis.discord_stats
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{CONFIG, Config, ExternalApis, InternalRoutes, ServerConfig};

    #[test]
    fn test_url_retrieval() {
        let mock_config = Config {
            external_apis: ExternalApis {
                discord_user: "http://user".to_string(),
                discord_stats: "http://stats".to_string(),
            },
            monitoring: crate::config::MonitoringConfig {
                discord: None,
                minecraft: None,
                site: None,
                api: None,
                self_hosted: None,
                http: None,
            },
            server: ServerConfig {
                host: "127.0.0.1".to_string(),
                port: 3000,
                routes: InternalRoutes {
                    base: "/api".to_string(),
                    discord_summary: "/discord".to_string(),
                    monitoring: "/monitoring".to_string(),
                },
            },
        };

        let _ = CONFIG.set(mock_config);
        assert_eq!(discord_user_url(), "http://user");
        assert_eq!(discord_stats_url(), "http://stats");
    }
}
