use crate::config::Config;

// ──────────────────────────────────────────────
//  External APIs Endpoints
// ──────────────────────────────────────────────

pub fn api_base_url() -> &'static str {
    &Config::global().api.base_url
}

pub fn discord_user_url() -> String {
    format!("{}/utilisateurs_discord", api_base_url())
}

pub fn discord_stats_url() -> String {
    format!("{}/utilisateurs_discord/stats", api_base_url())
}
