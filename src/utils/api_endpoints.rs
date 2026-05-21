//! Centralized management for external API endpoints.
//! 
//! This module retrieves URLs for external services from the 
//! application's configuration.

use crate::config::Config;

// ──────────────────────────────────────────────
//  External APIs Endpoints
// ──────────────────────────────────────────────

/// Returns the health check URL from the global configuration.
pub fn api_health_check_url() -> &'static str {
    &Config::global().external_apis.health_check
}

/// Returns the full URL for fetching a Discord user's information.
pub fn discord_user_url() -> &'static str {
    &Config::global().external_apis.discord_user
}

/// Returns the full URL for fetching a Discord user's statistics.
pub fn discord_stats_url() -> &'static str {
    &Config::global().external_apis.discord_stats
}
