//! Error types for the library crate.

use std::time::Duration;

use thiserror::Error;

// ---------------------------------------------------------------------------
// API error (network + HTTP layer)
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("request timeout after {0:?}")]
    Timeout(Duration),

    #[error("request aborted (SIGINT/SIGTERM)")]
    Aborted,

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
        use crate::models_mod::ApiErrorBody;
        let message = serde_json::from_str::<ApiErrorBody>(body)
            .ok()
            .and_then(|e| e.message)
            .unwrap_or_else(|| {
                let label = match status {
                    400 => "OpenRouter rejected the request",
                    401 => "Invalid or missing OpenRouter API key",
                    402 => "Insufficient OpenRouter credits",
                    403 => "Forbidden — check model access permissions",
                    429 => "Rate limited — slow down",
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

    /// Map to a CLI exit code.
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::NoApiKey => 3,
            Self::Timeout(_) => 5,
            Self::Aborted => 130,
            Self::Http { status, .. } => match status {
                401 => 3,
                402 => 3,
                429 => 4,
                n if (400..=499).contains(n) => 4,
                _ => 1,
            },
            Self::ApiResponse(_) => 4,
            Self::Network(_) => 1,
        }
    }
}
