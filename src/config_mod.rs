//! API key resolution from env or TOML config file.
//!
//! Config file: `~/.config/openrouter-image/config.toml` (XDG-conform via dirs::config_dir).
//!
//! Resolution order: OPENROUTER_API_KEY env > TOML file > error.
//!
//! TOML format:
//! ```toml
//! api_key = "sk-or-..."
//! default_model = "bytedance-seed/seedream-4.5"  # optional
//! ```

use std::fs;
use std::path::PathBuf;

use crate::error_mod::ConfigError;

// ---------------------------------------------------------------------------
// TOML config structure
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Deserialize)]
struct TomlConfig {
    api_key: Option<String>,
    default_model: Option<String>,
}

impl From<TomlConfig> for Config {
    fn from(toml: TomlConfig) -> Self {
        Self {
            api_key: toml.api_key,
            default_model: toml.default_model,
            config_file: Self::config_path().unwrap_or_else(|| PathBuf::from("config.toml")),
            config_file_exists: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

/// Configuration resolved from environment and/or config file.
#[derive(Debug, Clone)]
pub struct Config {
    /// The resolved API key (always Some after resolve() succeeds).
    pub api_key: Option<String>,
    /// Optional default model from config file.
    pub default_model: Option<String>,
    /// Path to the config file that was read (or the expected path).
    pub config_file: PathBuf,
    /// Whether the config file actually existed.
    pub config_file_exists: bool,
}

impl Config {
    /// Resolve config: env > TOML > ConfigError::NoApiKey.
    pub fn resolve() -> Result<Self, ConfigError> {
        // 1. Env wins
        if let Ok(key) = std::env::var("OPENROUTER_API_KEY") {
            let key = key.trim().to_string();
            if !key.is_empty() {
                let config_file =
                    Self::config_path().unwrap_or_else(|| PathBuf::from("config.toml"));
                return Ok(Self {
                    api_key: Some(key),
                    default_model: None,
                    config_file,
                    config_file_exists: false,
                });
            }
        }

        // 2. TOML file
        let path = Self::config_path().ok_or(ConfigError::NoHomeDir)?;

        if !path.exists() {
            return Err(ConfigError::NotFound(path));
        }

        let text = fs::read_to_string(&path).map_err(|e| ConfigError::ReadError(e.to_string()))?;

        let text = text.trim();
        if text.is_empty() {
            return Err(ConfigError::NotFound(path));
        }

        let toml: TomlConfig =
            toml::from_str(text).map_err(|e| ConfigError::ParseError(e.to_string()))?;

        let api_key = toml
            .api_key
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty());

        match api_key {
            Some(key) => Ok(Self {
                api_key: Some(key),
                default_model: toml.default_model,
                config_file: path,
                config_file_exists: true,
            }),
            None => Err(ConfigError::NoApiKey),
        }
    }

    /// Returns the resolved API key.
    pub fn api_key(&self) -> Option<&str> {
        self.api_key.as_deref()
    }

    /// Returns a masked version of the key for display (first 8 + last 4 chars).
    pub fn masked_key(&self) -> String {
        let Some(key) = &self.api_key else {
            return "—".to_string();
        };
        if key.len() <= 12 {
            return format!("{}…", &key[..3.min(key.len())]);
        }
        format!("{}…{}", &key[..8], &key[key.len().saturating_sub(4)..])
    }

    /// Path to the config file on disk.
    pub fn config_file_path(&self) -> PathBuf {
        self.config_file.clone()
    }

    /// Returns the XDG config path (~/.config/openrouter-image/config.toml).
    fn config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|p| p.join("openrouter-image").join("config.toml"))
    }
}
