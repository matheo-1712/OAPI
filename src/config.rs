//! Configuration management for OAPI.
//!
//! This module handles the loading and merging of configuration from multiple sources:
//! 1. `default_config.yaml` (Base defaults)
//! 2. `config.yaml` (Local overrides, ignored by Git)

use chrono::NaiveTime;
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
    /// Website (Astroloutre) allowed to use the `/auth/site/*` routes.
    #[serde(default)]
    pub site_auth: SiteAuthConfig,
    /// Daily maintenance tasks schedule.
    #[serde(default)]
    pub scheduler: SchedulerConfig,
}

/// Daily maintenance schedule (from YAML).
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SchedulerConfig {
    /// Whether the daily tasks run at all.
    pub enabled: bool,
    /// Local Europe/Paris time of the daily run, formatted `HH:MM`.
    pub run_at: String,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            run_at: "04:00".into(),
        }
    }
}

impl SchedulerConfig {
    /// Parses `run_at` (`HH:MM`).
    pub fn run_time(&self) -> Result<NaiveTime, String> {
        NaiveTime::parse_from_str(&self.run_at, "%H:%M")
            .map_err(|_| format!("scheduler.run_at must be HH:MM, got {:?}", self.run_at))
    }
}

/// Website login settings (from YAML).
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct SiteAuthConfig {
    /// Website origin, without path (e.g. `https://antredesloutres.fr`).
    /// Empty disables the website login routes.
    pub url: String,
    /// Additional websites on another registrable domain (e.g. `https://rom-patcher.ect-adl.fr`).
    /// Their login cookie is issued with `SameSite=None`, since the browser sends it cross-site.
    #[serde(default)]
    pub extra_urls: Vec<String>,
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
    /// Optional server-to-server administrator credential, never sent to browsers.
    pub service_api_key: String,
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
    /// Only these direct peers may supply X-Forwarded-For for client identification.
    /// Each entry is an exact IP (`127.0.0.1`) or a CIDR range (`172.16.0.0/12`).
    #[serde(default, deserialize_with = "deserialize_networks")]
    pub trusted_proxies: Vec<ipnet::IpNet>,
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

pub static CONFIG: OnceLock<Config> = OnceLock::new();

/// Reads `trusted_proxies` entries: an exact IP becomes a single-address network.
fn deserialize_networks<'de, D>(deserializer: D) -> Result<Vec<ipnet::IpNet>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Vec::<String>::deserialize(deserializer)?
        .iter()
        .map(|entry| {
            let entry = entry.trim();
            entry
                .parse::<ipnet::IpNet>()
                .or_else(|_| entry.parse::<std::net::IpAddr>().map(ipnet::IpNet::from))
                .map(|net| net.trunc())
                .map_err(|_| {
                    serde::de::Error::custom(format!(
                        "invalid trusted_proxies entry {entry:?}: expected an IP or CIDR range"
                    ))
                })
        })
        .collect()
}

/// An http(s) origin without path, with any trailing slash removed.
fn parse_origin(url: &str) -> Option<&str> {
    let origin = url.trim().trim_end_matches('/');
    let host = origin
        .strip_prefix("https://")
        .or_else(|| origin.strip_prefix("http://"))?;
    let valid = !host.is_empty()
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'));
    valid.then_some(origin)
}

impl Config {
    pub fn validate_security(&self) -> Result<(), String> {
        if !self.auth.service_api_key.is_empty()
            && (self.auth.service_api_key.trim().len() < 32
                || self.auth.service_api_key.chars().any(char::is_whitespace)
                || self.auth.service_api_key == self.auth.jwt_secret)
        {
            return Err("OAPI_SERVICE_API_KEY must be a separate secret of at least 32 bytes without whitespace".into());
        }
        if self.auth.jwt_secret.trim().len() < 32 {
            return Err("JWT_SECRET must contain at least 32 non-blank bytes".into());
        }
        if self.server.env == "production"
            && !self.auth.discord_redirect_url.starts_with("https://")
        {
            return Err("Production requires an HTTPS DISCORD_REDIRECT_URL".into());
        }
        if !self.site_auth.url.is_empty() && self.site_origin().is_none() {
            return Err("site_auth.url must be an http(s) origin without path".into());
        }
        if self
            .site_auth
            .extra_urls
            .iter()
            .any(|url| parse_origin(url).is_none())
        {
            return Err("site_auth.extra_urls must be http(s) origins without path".into());
        }
        if self.server.env == "production"
            && self
                .site_origins()
                .iter()
                .any(|origin| !origin.starts_with("https://"))
        {
            return Err("Production requires HTTPS site_auth URLs".into());
        }
        self.scheduler.run_time()?;
        Ok(())
    }

    /// Website origin allowed for cross-origin session reads and post-login redirects.
    pub fn site_origin(&self) -> Option<&str> {
        parse_origin(&self.site_auth.url)
    }

    /// Every website allowed to use the `/auth/site/*` routes, `site_auth.url` first.
    pub fn site_origins(&self) -> Vec<&str> {
        self.site_origin()
            .into_iter()
            .chain(
                self.site_auth
                    .extra_urls
                    .iter()
                    .filter_map(|url| parse_origin(url)),
            )
            .collect()
    }

    /// True for an exact configured website origin.
    pub fn is_site_origin(&self, origin: &str) -> bool {
        self.site_origins().contains(&origin)
    }

    /// Websites listed in `extra_urls` receive a cross-site session cookie.
    pub fn is_cross_site_origin(&self, origin: &str) -> bool {
        self.site_auth
            .extra_urls
            .iter()
            .filter_map(|url| parse_origin(url))
            .any(|extra| extra == origin)
    }

    pub fn secure_cookies(&self) -> bool {
        self.server.env == "production" || self.auth.discord_redirect_url.starts_with("https://")
    }

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
            #[serde(default)]
            site_auth: SiteAuthConfig,
            #[serde(default)]
            scheduler: SchedulerConfig,
        }

        let mut parts: YamlParts = yaml_config.try_deserialize()?;
        if let Ok(environment) = std::env::var("OAPI_ENV") {
            if !matches!(environment.as_str(), "development" | "production") {
                return Err(ConfigError::Message(
                    "OAPI_ENV must be development or production".into(),
                ));
            }
            parts.server.env = environment;
        }

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
        let discord_redirect_url = std::env::var("DISCORD_REDIRECT_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:3000/api/auth/callback".to_string());
        let jwt_secret = std::env::var("JWT_SECRET").unwrap_or_default();

        Ok(Config {
            monitoring: parts.monitoring,
            server: parts.server,
            discord_auth: parts.discord_auth,
            site_auth: parts.site_auth,
            scheduler: parts.scheduler,
            auth: AuthConfig {
                pb_email,
                pb_password,
                pb_url,
                discord_client_id,
                discord_client_secret,
                discord_redirect_url,
                jwt_secret,
                service_api_key: std::env::var("OAPI_SERVICE_API_KEY").unwrap_or_default(),
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
    let _ = CONFIG.set(config);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn trusted_proxies_accept_exact_ips_and_cidr_ranges() {
        #[derive(Deserialize)]
        struct Proxies {
            #[serde(deserialize_with = "deserialize_networks")]
            trusted_proxies: Vec<ipnet::IpNet>,
        }
        let parsed: Proxies =
            serde_json::from_str(r#"{"trusted_proxies":["127.0.0.1","172.18.0.7/12","::1"]}"#)
                .unwrap();
        let rendered: Vec<String> = parsed
            .trusted_proxies
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(rendered, ["127.0.0.1/32", "172.16.0.0/12", "::1/128"]);
        assert!(serde_json::from_str::<Proxies>(r#"{"trusted_proxies":["proxy.local"]}"#).is_err());
        let production =
            Config::load_from("default_config.yaml", Some("deploy/config.production.yaml"))
                .unwrap();
        assert!(
            production
                .server
                .trusted_proxies
                .iter()
                .any(|net| net.contains(&"172.18.0.2".parse::<std::net::IpAddr>().unwrap()))
        );
    }

    #[test]
    fn scheduler_defaults_and_rejects_invalid_time() {
        let mut config = Config::load_from("default_config.yaml", None).unwrap();
        config.auth.jwt_secret = "test-only-secret-at-least-32-bytes-long".into();
        assert!(!config.scheduler.enabled);
        assert_eq!(
            config.scheduler.run_time().unwrap(),
            NaiveTime::from_hms_opt(4, 0, 0).unwrap()
        );
        for invalid in ["", "4h", "25:00", "04:60"] {
            config.scheduler.run_at = invalid.into();
            assert!(config.validate_security().is_err(), "{invalid}");
        }
    }

    #[test]
    fn production_config_enables_the_scheduler() {
        let config =
            Config::load_from("default_config.yaml", Some("deploy/config.production.yaml"))
                .unwrap();
        assert!(config.scheduler.enabled);
        assert_eq!(
            config.scheduler.run_time().unwrap(),
            NaiveTime::from_hms_opt(4, 0, 0).unwrap()
        );
    }

    #[test]
    fn security_configuration_fails_closed() {
        let mut config = Config::load().unwrap();
        for secret in ["", "short", "                                "] {
            config.auth.jwt_secret = secret.into();
            assert!(config.validate_security().is_err());
        }
        config.auth.jwt_secret = "test-only-secret-at-least-32-bytes-long".into();
        config.server.env = "production".into();
        config.auth.discord_redirect_url = "http://localhost/callback".into();
        assert!(config.validate_security().is_err());
        config.auth.discord_redirect_url = "https://example.com/api/auth/callback".into();
        config.site_auth.url = String::new();
        assert!(config.validate_security().is_ok());
        assert!(config.secure_cookies());
        config.site_auth.url = "http://antredesloutres.fr".into();
        assert!(config.validate_security().is_err());
        config.site_auth.url = "https://antredesloutres.fr/profil".into();
        assert!(config.validate_security().is_err());
        config.site_auth.url = "https://antredesloutres.fr/".into();
        assert!(config.validate_security().is_ok());
        assert_eq!(config.site_origin(), Some("https://antredesloutres.fr"));

        config.site_auth.extra_urls = vec!["https://rom-patcher.ect-adl.fr/".into()];
        assert!(config.validate_security().is_ok());
        assert_eq!(
            config.site_origins(),
            [
                "https://antredesloutres.fr",
                "https://rom-patcher.ect-adl.fr"
            ]
        );
        assert!(config.is_site_origin("https://rom-patcher.ect-adl.fr"));
        assert!(!config.is_site_origin("https://rom-patcher.ect-adl.fr.evil.example"));
        assert!(config.is_cross_site_origin("https://rom-patcher.ect-adl.fr"));
        assert!(!config.is_cross_site_origin("https://antredesloutres.fr"));
        for invalid in [
            "http://rom-patcher.ect-adl.fr",
            "https://rom-patcher.ect-adl.fr/x",
            "rom-patcher.ect-adl.fr",
        ] {
            config.site_auth.extra_urls = vec![invalid.into()];
            assert!(config.validate_security().is_err(), "{invalid}");
        }
    }

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
