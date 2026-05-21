//! Data models for Discord-related entities.
//! 
//! This module defines the Data Transfer Objects (DTOs) used for 
//! communication with external Discord APIs and our internal services.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Represents a Discord user and their aggregated state.
#[derive(Serialize, Deserialize, ToSchema, Clone)]
pub struct DiscordUser {
    /// Internal database ID.
    pub id: i64,
    /// Unique Discord snowflake ID.
    pub discord_id: String,
    /// Discord username.
    pub pseudo_discord: String,
    /// Date the user joined the Discord server.
    pub join_date_discord: String,
    /// Date of the user's first recorded activity.
    pub first_activity: Option<String>,
    /// Date of the user's last recorded activity.
    pub last_activity: Option<String>,
    /// Discord user tag (e.g., #1234).
    pub tag_discord: String,
    /// URL to the user's Discord avatar.
    pub avatar_url: Option<String>,
    /// List of roles assigned to the user on the server.
    #[serde(default)]
    pub roles: Vec<DiscordRole>,
    /// Aggregated activity statistics for the user.
    #[serde(default)]
    pub stats: Vec<DiscordStats>,
    /// Date the user was deleted/left (if applicable).
    pub delete_date: Option<String>,
}

/// Represents a Discord role.
#[derive(Serialize, Deserialize, ToSchema, Clone)]
pub struct DiscordRole {
    /// Role snowflake ID.
    pub id: String,
    /// Role name.
    pub name: String,
    /// Hex color code of the role.
    pub color: String,
}

/// Represents a Discord channel (text or voice).
#[derive(Serialize, Deserialize, ToSchema, Clone)]
pub struct DiscordChannel {
    /// Channel snowflake ID.
    pub id: String,
    /// Channel name.
    pub name: String,
}

/// Represents a voice connection with another user.
#[derive(Serialize, Deserialize, ToSchema, Clone)]
pub struct DiscordVoiceConnection {
    /// Other user's snowflake ID.
    pub id: String,
    /// Other user's username.
    pub username: String,
}

/// Represents aggregated activity statistics for a Discord user.
#[derive(Serialize, Deserialize, ToSchema, Clone)]
pub struct DiscordStats {
    /// Internal stat entry ID.
    pub id: i64,
    /// Reference ID to the user.
    pub id_utilisateur: i64,
    /// Number of messages sent.
    pub nb_message: i64,
    /// Total time spent in voice channels (as a decimal string or duration).
    pub vocal_time: String,
    /// Date these statistics were recorded.
    pub date_stats: String,
    /// Most active voice channels.
    pub voice_channels: Vec<DiscordChannel>,
    /// Most active text channels.
    pub text_channels: Vec<DiscordChannel>,
    /// Users most frequently spent time with in voice.
    pub vocal_with: Vec<DiscordVoiceConnection>,
}

