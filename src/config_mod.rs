//! API key resolution from env or config file.

use std::fs;
use std::path::PathBuf;

use anyhow::Context;
use serde::Deserialize;

/// Key-resolution configuration.
#[derive(Debug, Clone)]
pub struct Config {
    pub api_key: Option<String>,
    pub config_file: PathBuf,
}

impl Config {
    /// Resolve the API key: env var first, then `image-gen.json`.
    pub fn resolve() -> anyhow::Result<Self> {
        let config_file = Self::config_path();

        let from_env = std::env::var("OPENROUTER_API_KEY")
            .ok()
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty());

        if let Some(key) = from_env {
            return Ok(Self {
                api_key: Some(key),
                config_file,
            });
        }

        let from_file = Self::load_from_file(&config_file)?;

        Ok(Self {
            api_key: from_file,
            config_file,
        })
    }

    /// Returns the resolved API key, if any.
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

    /// Path to the legacy config file (`~/.omp/agent/image-gen.json`).
    pub fn legacy_config_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".omp/agent/image-gen.json")
    }

    fn config_path() -> PathBuf {
        Self::legacy_config_path()
    }

    fn load_from_file(path: &PathBuf) -> anyhow::Result<Option<String>> {
        let raw = match fs::read_to_string(path) {
            Ok(r) => r,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => {
                anyhow::bail!("failed to read {}: {}", path.display(), e);
            }
        };

        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }

        #[derive(Deserialize)]
        struct ImageGenSettings {
            #[serde(rename = "apiKey")]
            api_key: Option<String>,
            #[serde(rename = "OPENROUTER_API_KEY")]
            openrouter_api_key: Option<String>,
            #[serde(rename = "openrouter_api_key")]
            openrouter_api_key_snake: Option<String>,
        }

        let parsed: ImageGenSettings = serde_json::from_str(trimmed)
            .with_context(|| format!("failed to parse {}", path.display()))?;

        let key = parsed
            .api_key
            .or(parsed.openrouter_api_key)
            .or(parsed.openrouter_api_key_snake)
            .map(|k| k.trim().to_string())
            .filter(|k| !k.is_empty());

        Ok(key)
    }
}


