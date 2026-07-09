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
    /// Monitoring settings for external services.
    pub monitoring: MonitoringConfig,
    /// Server settings including internal routes.
    pub server: ServerConfig,
    /// Sensitive authentication and database settings (from environment only).
    pub auth: AuthConfig,
    /// Discord Authentication settings.
    #[serde(default)]
    pub discord_auth: DiscordAuthConfig,
}

/// Discord Authentication Settings (from YAML)
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct DiscordAuthConfig {
    /// Guild ID to check roles against.
    pub guild_id: String,
    /// Role ID for Investor.
    pub investor_role_id: String,
}

/// Sensitive authentication settings.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AuthConfig {
    /// Pocketbase Email.
    pub pb_email: String,
    /// Pocketbase Password.
    pub pb_password: String,
    /// Pocketbase URL.
    pub pb_url: String,
    /// Discord Client ID.
    pub discord_client_id: String,
    /// Discord Client Secret.
    pub discord_client_secret: String,
    /// Discord Redirect URL.
    pub discord_redirect_url: String,
    /// JWT Secret.
    pub jwt_secret: String,
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

/// Server settings and internal route paths.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ServerConfig {
    /// Environment mode (development/production).
    pub env: String,
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
    /// Path for generating a Minecraft summary image.
    pub minecraft_summary: String,
    /// Path for the monitoring status endpoint.
    pub monitoring: String,
}

/// Static storage for the global configuration to ensure it's loaded only once.
pub static CONFIG: OnceLock<Config> = OnceLock::new();

impl Config {
    /// Loads the configuration from the hierarchical sources and environment.
    pub fn load() -> Result<Self, ConfigError> {
        Self::load_from("default_config.yaml", Some("config.yaml"))
    }

    /// Internal helper to load config from specific files (useful for testing).
    fn load_from(default_path: &str, override_path: Option<&str>) -> Result<Self, ConfigError> {
        // 1. Load YAML configuration (non-sensitive)
        let mut builder =
            ConfigTrait::builder().add_source(File::new(default_path, FileFormat::Yaml));

        if let Some(path) = override_path {
            builder = builder.add_source(File::new(path, FileFormat::Yaml).required(false));
        }

        let yaml_config = builder.build()?;

        // Temporary structure to deserialize YAML parts only
        #[derive(Debug, Deserialize)]
        struct YamlParts {
            monitoring: MonitoringConfig,
            server: ServerConfig,
            #[serde(default)]
            discord_auth: DiscordAuthConfig,
        }

        let parts: YamlParts = yaml_config.try_deserialize()?;

        // 2. Load Auth configuration from Environment ONLY
        let pb_email = std::env::var("PB_EMAIL")
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();
        let pb_password = std::env::var("PB_PASSWORD")
            .unwrap_or_default()
            .trim_matches('"')
            .to_string();
        let pb_url = std::env::var("PB_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:8090".to_string())
            .trim_matches('"')
            .to_string();

        let discord_client_id = std::env::var("DISCORD_CLIENT_ID").unwrap_or_default();
        let discord_client_secret = std::env::var("DISCORD_CLIENT_SECRET").unwrap_or_default();
        let discord_redirect_url = std::env::var("DISCORD_REDIRECT_URL").unwrap_or_default();
        let jwt_secret = std::env::var("JWT_SECRET").unwrap_or_default();

        Ok(Config {
            monitoring: parts.monitoring,
            server: parts.server,
            discord_auth: parts.discord_auth,
            auth: AuthConfig {
                pb_email,
                pb_password,
                pb_url,
                discord_client_id,
                discord_client_secret,
                discord_redirect_url,
                jwt_secret,
            },
        })
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

        let mut template = String::from("# OAPI - Local configuration overrides\n");
        template.push_str(
            "# Uncomment lines to override default values from 'default_config.yaml'\n\n",
        );

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

        fs::write(config_path, template).expect("Unable to create config.yaml file");
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
  env: "development"
  host: "127.0.0.1"
  port: 8080
  routes:
    base: "/api"
    discord_summary: "/discord"
    minecraft_summary: "/minecraft"
    monitoring: "/monitoring"
monitoring:
  discord: []
"#;
        let default_path = "test_default_config.yaml";
        fs::write(default_path, test_default).unwrap();

        let config = Config::load_from(default_path, None).unwrap();

        assert_eq!(config.server.port, 8080);

        fs::remove_file(default_path).unwrap();
    }

    #[test]
    fn test_config_override() {
        let test_default = r#"
server:
  env: "development"
  port: 8080
  host: "127.0.0.1"
  routes:
    base: "/"
    discord_summary: "/"
    minecraft_summary: "/m"
    monitoring: "/monitoring"
monitoring:
  discord: []
"#;
        let test_override = "server:\n  port: 9090\n";

        fs::write("t_default.yaml", test_default).unwrap();
        fs::write("t_override.yaml", test_override).unwrap();

        let config = Config::load_from("t_default.yaml", Some("t_override.yaml")).unwrap();

        assert_eq!(config.server.port, 9090); // Overridden
        assert_eq!(config.server.host, "127.0.0.1"); // Kept from default

        fs::remove_file("t_default.yaml").unwrap();
        fs::remove_file("t_override.yaml").unwrap();
    }
}
