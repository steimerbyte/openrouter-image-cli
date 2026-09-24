//! Error types for the library crate.
//!
//! Exit code mapping (only 5 values):
//!   0  Ok
//!   2  Usage   — clap error, invalid args, missing prompt
//!   3  Auth    — no key, invalid key, 401/402 from API
//!   4  API     — 4xx non-auth, 5xx after retry exhausted
//!   5  IO      — network unreachable, timeout, file write, dir creation

use std::time::Duration;

use thiserror::Error;

// ---------------------------------------------------------------------------
// API error (network + HTTP layer)
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("request timeout after {0:?}")]
    Timeout(Duration),

    #[error("HTTP {status} — {message}")]
    Http { status: u16, message: String },

    #[error("no API key configured")]
    NoApiKey,

    #[error("API error: {0}")]
    ApiResponse(crate::models_mod::ApiErrorBody),

    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
}

impl ApiError {
    /// Classify an HTTP status code into an `ApiError` variant.
    pub fn from_status(status: u16, body: &str) -> Self {
        let message = serde_json::from_str::<crate::models_mod::ApiErrorBody>(body)
            .ok()
            .and_then(|e| e.message)
            .unwrap_or_else(|| {
                let label = match status {
                    400 => "OpenRouter rejected the request",
                    401 => "Invalid or missing OpenRouter API key",
                    402 => "Insufficient OpenRouter credits",
                    403 => "Forbidden — check model access permissions",
                    429 => "Rate limited",
                    500 => "OpenRouter server error",
                    502 => "OpenRouter gateway error",
                    503 => "OpenRouter service unavailable",
                    504 => "OpenRouter gateway timeout",
                    _ => "API error",
                };
                label.to_string()
            });

        Self::Http { status, message }
    }

    /// Convert to `anyhow::Error` for propagation.
    pub fn to_anyhow(self) -> anyhow::Error {
        anyhow::anyhow!(self)
    }

    /// Map to a CLI exit code: 0/2/3/4/5 only.
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::NoApiKey => 3,
            Self::Timeout(_) => 5,
            Self::Http { status, .. } => match status {
                401 | 402 => 3,
                400..=499 => 4,
                _ => 4,
            },
            Self::ApiResponse(_) => 4,
            Self::Network(_) => 5,
        }
    }
}

// ---------------------------------------------------------------------------
// Config / reference errors (library-level)
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("no home directory found — cannot locate config")]
    NoHomeDir,

    #[error("config file not found at {0}")]
    NotFound(std::path::PathBuf),

    #[error("failed to read config file: {0}")]
    ReadError(String),

    #[error("failed to parse config file: {0}")]
    ParseError(String),

    #[error("no API key found in config")]
    NoApiKey,
}

impl ConfigError {
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::NoApiKey | Self::NotFound(_) => 3,
            Self::NoHomeDir | Self::ReadError(_) | Self::ParseError(_) => 5,
        }
    }
}

#[derive(Debug, Error)]
pub enum ReferenceError {
    #[error("reference is not a data URI: {0}")]
    NotADataUri(String),

    #[error("malformed data URI (missing ';' separator): {0}")]
    Malformed(String),

    #[error("data URI does not use base64 encoding: {0}")]
    NotBase64(String),
}

impl ReferenceError {
    pub fn exit_code(&self) -> u8 {
        2 // usage error — invalid argument format
    }
}
