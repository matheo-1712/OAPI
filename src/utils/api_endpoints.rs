//! Centralized management for external API endpoints.
//! 
//! This module constructs URLs for external services dynamically
//! based on the application's configuration.

use crate::config::Config;

// ──────────────────────────────────────────────
//  External APIs Endpoints
// ──────────────────────────────────────────────

/// Returns the base URL for external API calls from the global configuration.
pub fn api_base_url() -> &'static str {
    &Config::global().api.base_url
}

/// Constructs the full URL for fetching a Discord user's information.
pub fn discord_user_url() -> String {
    format!("{}/utilisateurs_discord", api_base_url())
}

/// Constructs the full URL for fetching a Discord user's statistics.
pub fn discord_stats_url() -> String {
    format!("{}/utilisateurs_discord/stats", api_base_url())
}
