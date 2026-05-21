use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Struct for discord user
#[derive(Serialize, Deserialize, ToSchema, Clone)]
pub struct DiscordUser {
    pub id: i64,
    pub discord_id: String,
    pub pseudo_discord: String,
    pub join_date_discord: String,
    pub first_activity: Option<String>,
    pub last_activity: Option<String>,
    pub tag_discord: String,
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub roles: Vec<DiscordRole>,
    #[serde(default)]
    pub stats: Vec<DiscordStats>,
    pub delete_date: Option<String>,
}
#[derive(Serialize, Deserialize, ToSchema, Clone)]
pub struct DiscordRole {
    pub id: String,
    pub name: String,
    pub color: String,
}

#[derive(Serialize, Deserialize, ToSchema, Clone)]
pub struct DiscordChannel {
    pub id: String,
    pub name: String,
}

#[derive(Serialize, Deserialize, ToSchema, Clone)]
pub struct DiscordVoiceConnection {
    pub id: String,
    pub username: String,
}

#[derive(Serialize, Deserialize, ToSchema, Clone)]
pub struct DiscordStats {
    pub id: i64,
    pub id_utilisateur: i64,
    pub nb_message: i64,
    pub vocal_time: String,
    pub date_stats: String,
    pub voice_channels: Vec<DiscordChannel>,
    pub text_channels: Vec<DiscordChannel>,
    pub vocal_with: Vec<DiscordVoiceConnection>,
}

