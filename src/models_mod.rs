//! Data types for API requests and responses.
//!
//! No hardcoded model list — models are discovered live via `/api/v1/models`.

use std::fmt;
use std::path::PathBuf;

use clap::ValueEnum;
use serde::Deserialize;

// ---------------------------------------------------------------------------
// Generation parameters
// ---------------------------------------------------------------------------

/// Parameters for an image generation request.
#[derive(Debug, Clone)]
pub struct GenerationParams {
    /// Prompt text.
    pub prompt: String,
    /// Model slug (e.g. "openai/gpt-5-image").
    pub model: String,
    /// Optional reference image data URIs.
    pub image_refs: Vec<String>,
    /// Output paths for the generated image(s).
    pub output_paths: Vec<PathBuf>,
    /// Number of images requested.
    pub n: u8,
    /// Optional resolution preset: "512" | "1K" | "2K" | "4K".
    pub resolution: Option<String>,
    /// HTTP timeout in milliseconds.
    pub timeout_ms: u64,
}

impl GenerationParams {
    /// HTTP timeout.
    pub fn timeout_ms(&self) -> u64 {
        self.timeout_ms
    }

    /// Build the JSON body sent to OpenRouter.
    pub fn to_request_body(&self) -> serde_json::Value {
        let mut body = serde_json::json!({
            "model": self.model,
            "prompt": self.prompt,
        });

        if self.n > 1 {
            body["n"] = serde_json::json!(self.n);
        }

        if !self.image_refs.is_empty() {
            let refs: Vec<serde_json::Value> = self
                .image_refs
                .iter()
                .map(|uri| {
                    serde_json::json!({
                        "type": "image_url",
                        "image_url": { "url": uri }
                    })
                })
                .collect();
            body["input_references"] = serde_json::json!(refs);
        }

        if let Some(res) = &self.resolution {
            body["resolution"] = serde_json::json!(res);
        }

        body
    }
}

// ---------------------------------------------------------------------------
// Output format
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
#[clap(rename_all = "snake_case")]
pub enum OutputFormat {
    #[default]
    #[value(name = "png")]
    Png,
    #[value(name = "jpeg")]
    Jpeg,
    #[value(name = "webp")]
    Webp,
    #[value(name = "svg")]
    Svg,
}

impl OutputFormat {
    /// File extension used for saved files.
    pub fn to_ext(&self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Webp => "webp",
            Self::Svg => "svg",
        }
    }
}

// ---------------------------------------------------------------------------
// API response types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct ApiResponse {
    #[serde(rename = "created")]
    pub created: Option<i64>,
    #[serde(rename = "data")]
    pub data: Option<Vec<ImageData>>,
    #[serde(rename = "usage")]
    pub usage: Option<Usage>,
    #[serde(rename = "error", default)]
    #[allow(dead_code)]
    pub error: Option<ApiErrorBody>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImageData {
    #[serde(rename = "b64_json")]
    pub b64_json: Option<String>,
    #[serde(rename = "media_type")]
    pub media_type: Option<String>,
    #[serde(rename = "url")]
    #[allow(dead_code)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Usage {
    #[serde(rename = "prompt_tokens")]
    pub prompt_tokens: Option<i64>,
    #[serde(rename = "completion_tokens")]
    pub completion_tokens: Option<i64>,
    #[serde(rename = "total_tokens")]
    pub total_tokens: Option<i64>,
    #[serde(rename = "cost")]
    pub cost: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiErrorBody {
    #[serde(rename = "message")]
    pub message: Option<String>,
    #[serde(rename = "code")]
    pub code: Option<serde_json::Value>,
    #[serde(rename = "type")]
    pub r#type: Option<String>,
}

impl fmt::Display for ApiErrorBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message.as_deref().unwrap_or("unknown error"))
    }
}
