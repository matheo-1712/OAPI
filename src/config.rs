use serde::Deserialize;
use std::sync::OnceLock;
use std::fs;
use std::path::Path;
use config::{Config as ConfigTrait, ConfigError, File, FileFormat};

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub api: ApiConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ApiConfig {
    pub base_url: String,
}

pub static CONFIG: OnceLock<Config> = OnceLock::new();

impl Config {
    pub fn load() -> Result<Self, ConfigError> {
        let s = ConfigTrait::builder()
            // Start with default configuration
            .add_source(File::new("default_config.yaml", FileFormat::Yaml))
            // Layer on the local configuration (optional)
            .add_source(File::new("config.yaml", FileFormat::Yaml).required(false))
            // Layer on environment variables (optional, prefix OAPI_)
            .add_source(config::Environment::with_prefix("OAPI").separator("__"))
            .build()?;

        s.try_deserialize()
    }

    pub fn global() -> &'static Config {
        CONFIG.get().expect("Config is not initialized")
    }
}

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
