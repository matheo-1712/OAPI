//! Data models for Discord-related entities.
//!
//! This module defines the Data Transfer Objects (DTOs) used for
//! communication with external Discord APIs and our internal services.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Represents a Discord user and their aggregated state.
#[derive(Serialize, Deserialize, ToSchema, Clone)]
pub struct DiscordUser {
    pub id: String,
    pub username: String,
    pub discord_id: String,
    pub discord_tag: String,
    pub avatar_url: Option<String>,
    pub joined_at: String,
    pub first_active_at: Option<String>,
    pub last_active_at: Option<String>,
    pub delete_at: Option<String>,
    #[serde(default)]
    pub roles: Vec<DiscordRole>,
    #[serde(default)]
    pub stats: Vec<DiscordStats>,
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
    #[serde(default)]
    pub voice_channels: Option<Vec<DiscordChannel>>,
    /// Most active text channels.
    #[serde(default)]
    pub text_channels: Option<Vec<DiscordChannel>>,
    /// Users most frequently spent time with in voice.
    #[serde(default)]
    pub vocal_with: Option<Vec<DiscordVoiceConnection>>,
}
