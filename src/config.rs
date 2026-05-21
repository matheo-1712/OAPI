//! Configuration management for OAPI.
//! 
//! This module handles the loading and merging of configuration from multiple sources:
//! 1. `default_config.yaml` (Base defaults)
//! 2. `config.yaml` (Local overrides, ignored by Git)
//! 3. Environment variables prefixed with `OAPI_`

use serde::Deserialize;
use std::sync::OnceLock;
use std::fs;
use std::path::Path;
use config::{Config as ConfigTrait, ConfigError, File, FileFormat};

/// Global configuration structure.
#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    /// External API settings.
    pub api: ApiConfig,
}

/// Settings for external API interactions.
#[derive(Debug, Deserialize, Clone)]
pub struct ApiConfig {
    /// The base URL for the external Otterly API.
    pub base_url: String,
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
            // Layer on environment variables (Optional, e.g., OAPI_API__BASE_URL)
            .add_source(config::Environment::with_prefix("OAPI").separator("__"))
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
        let template = "# Surcharges locales de configuration\n# api:\n#   base_url: \"https://votre-url-specifique.fr/api\"\n";
        fs::write(config_path, template).expect("Impossible de créer le fichier config.yaml");
    }

    let config = Config::load().expect("Failed to load configuration");
    CONFIG.set(config).expect("Failed to set global configuration");
}
