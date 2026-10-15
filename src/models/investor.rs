use serde::Serialize;
use utoipa::ToSchema;

/// Current record counts in PocketBase, not activity or performance estimates.
#[derive(Debug, Serialize, ToSchema)]
pub struct InvestorStats {
    pub discord_users: u32,
    pub minecraft_players: u32,
    pub minecraft_servers: u32,
    pub badges_awarded: u32,
    /// UTC time when the database queries completed.
    pub fetched_at: String,
}
