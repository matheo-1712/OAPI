//! Configuration management for OAPI.
//!
//! This module handles the loading and merging of configuration from multiple sources:
//! 1. `default_config.yaml` (Base defaults)
//! 2. `config.yaml` (Local overrides, ignored by Git)

use config::{Config as ConfigTrait, ConfigError, File, FileFormat};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

/// Global configuration structure.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    /// External API endpoints (full URLs).
    pub external_apis: ExternalApis,
    /// Monitoring settings for external services.
    pub monitoring: MonitoringConfig,
    /// Server settings including internal routes.
    pub server: ServerConfig,
}

/// Monitoring configuration for external services.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MonitoringConfig {
    /// Discord bot services.
    pub discord: Option<Vec<DiscordServiceConfig>>,
    /// Minecraft server services.
    pub minecraft: Option<Vec<MinecraftServiceConfig>>,
    /// Website services.
    pub site: Option<Vec<HttpServiceConfig>>,
    /// API services.
    pub api: Option<Vec<HttpServiceConfig>>,
    /// Self-hosted services.
    pub self_hosted: Option<Vec<HttpServiceConfig>>,
    /// Generic HTTP services.
    pub http: Option<Vec<HttpServiceConfig>>,
}

/// Configuration for a generic HTTP-based service check.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct HttpServiceConfig {
    /// Name of the service.
    pub name: String,
    /// URL to check.
    pub url: String,
}

/// Configuration for a Discord bot service check.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DiscordServiceConfig {
    /// Name of the bot.
    pub name: String,
    /// Health check URL.
    pub url: String,
}

/// Configuration for a Minecraft server check.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct MinecraftServiceConfig {
    /// Name of the server.
    pub name: String,
    /// Hostname or IP.
    pub host: String,
    /// Port number.
    pub port: u16,
}

/// Full URLs for external API interactions.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ExternalApis {
    /// Full URL for fetching Discord user information.
    pub discord_user: String,
    /// Full URL for fetching Discord user statistics.
    pub discord_stats: String,
}

/// Server settings and internal route paths.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ServerConfig {
    /// Host to bind the server to.
    pub host: String,
    /// Port to bind the server to.
    pub port: u16,
    /// Internal API route paths.
    pub routes: InternalRoutes,
}

/// Internal API route paths.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct InternalRoutes {
    /// Base path for all API routes.
    pub base: String,
    /// Path for generating a Discord summary image.
    pub discord_summary: String,
    /// Path for the monitoring status endpoint.
    pub monitoring: String,
}

/// Static storage for the global configuration to ensure it's loaded only once.
pub static CONFIG: OnceLock<Config> = OnceLock::new();

impl Config {
    /// Loads the configuration from the hierarchical sources.
    ///
    /// # Errors
    ///
    /// Returns a [`ConfigError`] if the mandatory `default_config.yaml` is missing or invalid.
    pub fn load() -> Result<Self, ConfigError> {
        let s = ConfigTrait::builder()
            // Start with default configuration (Required)
            .add_source(File::new("default_config.yaml", FileFormat::Yaml))
            // Layer on the local configuration (Optional)
            .add_source(File::new("config.yaml", FileFormat::Yaml).required(false))
            .build()?;

        s.try_deserialize()
    }

    /// Provides a global reference to the loaded configuration.
    ///
    /// # Panics
    ///
    /// Panics if called before [`init()`].
    pub fn global() -> &'static Config {
        CONFIG.get().expect("Config is not initialized")
    }
}

/// Initializes the global configuration.
///
/// This function should be called at the very beginning of the program.
/// It also ensures that a local `config.yaml` exists.
pub fn init() {
    // Ensure config.yaml exists so it can be edited by the user
    let config_path = Path::new("config.yaml");
    if !config_path.exists() {
        let default_config_content = fs::read_to_string("default_config.yaml")
            .unwrap_or_else(|_| "server:\n  host: \"127.0.0.1\"\n".to_string());

        let mut template = String::from("# OAPI - Surcharges locales de configuration\n");
        template.push_str("# Décommentez les lignes pour surcharger les valeurs par défaut de 'default_config.yaml'\n\n");

        for line in default_config_content.lines() {
            if line.trim().is_empty() {
                template.push('\n');
            } else {
                // Comment out everything to avoid "unit value" panics if a section header is uncommented but empty
                template.push_str("# ");
                template.push_str(line);
                template.push('\n');
            }
        }

        fs::write(config_path, template).expect("Impossible de créer le fichier config.yaml");
    }

    let config = Config::load().expect("Failed to load configuration");
    CONFIG
        .set(config)
        .expect("Failed to set global configuration");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_config_load_default() {
        // Create a temporary default_config.yaml for testing
        let test_default = r#"
server:
  host: "127.0.0.1"
  port: 8080
  routes:
    base: "/api"
    generate_image: "/img"
    discord_summary: "/discord"
    monitoring: "/monitoring"
external_apis:
  discord_user: "http://user"
  discord_stats: "http://stats"
monitoring:
  discord: []
"#;
        let default_path = "test_default_config.yaml";
        fs::write(default_path, test_default).unwrap();

        let s = ConfigTrait::builder()
            .add_source(File::new(default_path, FileFormat::Yaml))
            .build()
            .unwrap();

        let config: Config = s.try_deserialize().unwrap();

        assert_eq!(config.server.port, 8080);
        assert_eq!(config.external_apis.discord_user, "http://user");

        fs::remove_file(default_path).unwrap();
    }

    #[test]
    fn test_config_override() {
        let test_default = r#"
server:
  port: 8080
  host: "127.0.0.1"
  routes:
    base: "/"
    generate_image: "/"
    discord_summary: "/"
    monitoring: "/monitoring"
external_apis:
  discord_user: ""
  discord_stats: ""
monitoring:
  discord: []
"#;
        let test_override = "server:\n  port: 9090\n";

        fs::write("t_default.yaml", test_default).unwrap();
        fs::write("t_override.yaml", test_override).unwrap();

        let s = ConfigTrait::builder()
            .add_source(File::new("t_default.yaml", FileFormat::Yaml))
            .add_source(File::new("t_override.yaml", FileFormat::Yaml))
            .build()
            .unwrap();

        let config: Config = s.try_deserialize().unwrap();

        assert_eq!(config.server.port, 9090); // Overridden
        assert_eq!(config.server.host, "127.0.0.1"); // Kept from default

        fs::remove_file("t_default.yaml").unwrap();
        fs::remove_file("t_override.yaml").unwrap();
    }
}
