use serde::{Deserialize, Serialize};
use serde_json;
use utoipa::ToSchema;

/// Collection : players
#[derive(Serialize, Deserialize, ToSchema, Clone)]
pub struct MinecraftPlayer {
    /// Player"s discord link account
    pub discord_user: String,
    /// Player's platform.
    pub platform: String,
    /// Player's UUID.
    pub account_id: String,
    /// Playername
    pub playername: String,
    /// Timestamp of the player's first recorded activity.
    pub first_connected_at: String,
    /// Timestamp of the player's last recorded activity.
    pub last_connected_at: String,
    /// Player's stats
    pub stats: Vec<MinecraftStats>,
}
/// Collection : players_stats
#[derive(Serialize, Deserialize, ToSchema, Clone)]
pub struct MinecraftStats {
    pub id: String,
    pub server: String,
    pub account_id: String,
    pub playtime: i64,
    pub deaths: i64,
    pub mob_kills: i64,
    pub player_kills: i64,
    pub blocks_mined: i64,
    pub blocks_placed: i64,
    pub total_distance: f64,
    pub distance_walked: f64,
    pub distance_elytra: f64,
    pub distance_fligth: f64,
    #[serde(default)]
    pub mobs_killed: serde_json::Value,
    #[serde(default)]
    pub items_crafted: serde_json::Value,
    #[serde(default)]
    pub items_broken: serde_json::Value,
    #[serde(default)]
    pub achievements: serde_json::Value,
}
