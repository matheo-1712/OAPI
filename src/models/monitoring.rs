use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Represents the status of a monitored service.
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum ServiceStatus {
    /// The service is responsive and healthy.
    Up,
    /// The service is unreachable or returned an error.
    Down,
}

/// Metadata for a Discord bot service.
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct DiscordBotMetadata {
    /// Software version.
    pub version: String,
    /// Uptime information.
    pub uptime: DiscordBotUptime,
    /// Discord-specific data.
    pub discord: DiscordBotDiscordData,
}

/// Uptime information for a Discord bot.
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct DiscordBotUptime {
    /// Uptime in seconds.
    pub seconds: u64,
    /// Human-readable uptime string.
    pub human: String,
}

/// Discord-specific data for a bot.
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct DiscordBotDiscordData {
    /// Ping to Discord gateway.
    pub ping: u32,
    /// URL to the bot's avatar.
    pub avatar: String,
}

/// Metadata for a Minecraft server.
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct MinecraftMetadata {
    /// Number of players currently online.
    pub online_players: u32,
    /// Maximum number of players allowed.
    pub max_players: u32,
}

/// Specific metadata based on the service type.
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
#[serde(untagged)]
pub enum ServiceMetadata {
    /// Metadata for a Discord bot.
    DiscordBot(DiscordBotMetadata),
    /// Metadata for a Minecraft server.
    Minecraft(MinecraftMetadata),
}

/// The result of a health check for a single service.
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct ServiceResult {
    /// The name of the service.
    pub name: String,
    /// The type of service (e.g., "http", "minecraft").
    pub type_name: String,
    /// The current status of the service.
    pub status: ServiceStatus,
    /// Response time in milliseconds.
    pub response_time_ms: u64,
    /// Optional metadata for the service.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ServiceMetadata>,
    /// Optional error message if the service is offline.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Aggregated response containing status for all monitored services.
#[derive(Debug, Serialize, Deserialize, ToSchema, Clone)]
pub struct MonitoringResponse {
    /// List of status results for each configured service.
    pub services: Vec<ServiceResult>,
}
