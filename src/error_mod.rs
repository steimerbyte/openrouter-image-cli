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

    /// The API returned HTTP 200 but the response body had no `data` field or
    /// the field was an empty array. Carries model, last-known status, and a
    /// truncated body excerpt for debugging.
    #[error("empty response from `{model}` (status {status:?}, body: {body_excerpt:?})")]
    EmptyResponse {
        model: String,
        status: Option<u16>,
        body_excerpt: Option<String>,
    },

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
            Self::EmptyResponse { .. } => 4,
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

    /// Config file is world-readable or group-readable (mode & 0o077 != 0).
    /// User must `chmod 600` the file. Exit code 3 (auth/config).
    #[error("config file has insecure permissions (mode {0:o}); chmod 600 required")]
    InsecurePermissions(u32),

    /// Config file is a symlink — refusing to follow for safety.
    /// Exit code 3 (auth/config).
    #[error("config file is a symlink; refusing to follow for safety")]
    ConfigIsSymlink,
}

impl ConfigError {
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::NoApiKey | Self::NotFound(_) => 3,
            Self::InsecurePermissions(_) | Self::ConfigIsSymlink => 3,
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

// ---------------------------------------------------------------------------
// Path safety errors (F6, F7, F9)
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum PathError {
    /// F6: resolved output path escapes allowed directories.
    #[error(
        "output path `{path}` resolves outside allowed directories \
         (cwd, HOME, ~/generated-images)"
    )]
    Traversal { path: std::path::PathBuf },

    /// F7: output path is a symlink; refusing to overwrite.
    #[error("output path `{path}` is a symlink; refusing to overwrite")]
    Symlink { path: std::path::PathBuf },

    /// F9: output file already exists and no-clobber is enforced.
    #[error(
        "output file already exists: `{path}`; refusing to overwrite. \
         Delete first or use --clobber"
    )]
    AlreadyExists { path: std::path::PathBuf },
}

impl PathError {
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Traversal { .. } => 2,     // usage — bad output path
            Self::Symlink { .. } => 5,       // IO — dangerous path
            Self::AlreadyExists { .. } => 2, // usage — file exists
        }
    }
}
