//! Data types for API requests and responses.

use std::fmt;
use std::path::PathBuf;

use clap::ValueEnum;
use serde::Deserialize;


// ---------------------------------------------------------------------------
// Model registry
// ---------------------------------------------------------------------------

/// All known GPT image models (hardcoded, no auto-discovery).
pub const ALL_MODELS: &[&str] = &[
    "openai/gpt-image-2",
    "openai/gpt-image-1",
    "openai/gpt-image-1-mini",
    "openai/gpt-5-image",
    "openai/gpt-5-image-mini",
    "openai/gpt-5.4-image-2",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
#[clap(rename_all = "snake_case")]
pub enum ImageModel {
    #[default]
    #[value(name = "openai/gpt-image-2")]
    GptImage2,
    #[value(name = "openai/gpt-image-1")]
    GptImage1,
    #[value(name = "openai/gpt-image-1-mini")]
    GptImage1Mini,
    #[value(name = "openai/gpt-5-image")]
    Gpt5Image,
    #[value(name = "openai/gpt-5-image-mini")]
    Gpt5ImageMini,
    #[value(name = "openai/gpt-5.4-image-2")]
    Gpt54Image2,
}

impl fmt::Display for ImageModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GptImage2 => write!(f, "openai/gpt-image-2"),
            Self::GptImage1 => write!(f, "openai/gpt-image-1"),
            Self::GptImage1Mini => write!(f, "openai/gpt-image-1-mini"),
            Self::Gpt5Image => write!(f, "openai/gpt-5-image"),
            Self::Gpt5ImageMini => write!(f, "openai/gpt-5-image-mini"),
            Self::Gpt54Image2 => write!(f, "openai/gpt-5.4-image-2"),
        }
    }
}

// ---------------------------------------------------------------------------
// Generation parameters
// ---------------------------------------------------------------------------

/// Parameters for an image generation request.
#[derive(Debug, Clone)]
pub struct GenerationParams {
    pub prompt: String,
    pub model: ImageModel,
    pub reference: Option<String>,
    pub aspect_ratio: AspectRatio,
    pub quality: Option<Quality>,
    pub background: Option<String>,
    pub output_format: OutputFormat,
    pub resolution: Option<String>,
    pub n: u32,
    pub seed: Option<u64>,
    pub output_dir: PathBuf,
    pub timeout_ms: u64,
    pub retry: bool,
}

/// Build the JSON body sent to OpenRouter.
impl GenerationParams {
    pub fn to_request_body(&self, reference_url: Option<String>) -> serde_json::Value {
        let mut body = serde_json::json!({
            "model": self.model.to_string(),
            "prompt": self.prompt,
        });

        if self.n > 1 {
            body["n"] = serde_json::json!(self.n);
        }
        if let Some(q) = &self.quality {
            body["quality"] = serde_json::json!(q.to_str());
        }
        if let Some(bg) = &self.background {
            body["background"] = serde_json::json!(bg);
        }
        body["output_format"] = serde_json::json!(self.output_format.to_str());
        if let Some(res) = &self.resolution {
            body["resolution"] = serde_json::json!(res);
        }
        body["aspect_ratio"] = serde_json::json!(self.aspect_ratio.to_str());
        if let Some(seed) = self.seed {
            body["seed"] = serde_json::json!(seed);
        }
        if let Some(url) = reference_url {
            body["input_references"] = serde_json::json!([
                { "type": "image_url", "image_url": { "url": url } }
            ]);
        }

        body
    }
}

// ---------------------------------------------------------------------------
// Aspect ratio
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
#[clap(rename_all = "snake_case")]
pub enum AspectRatio {
    #[default]
    #[value(name = "1:1")]
    R1x1,
    #[value(name = "3:2")]
    R3x2,
    #[value(name = "2:3")]
    R2x3,
    #[value(name = "4:3")]
    R4x3,
    #[value(name = "3:4")]
    R3x4,
    #[value(name = "16:9")]
    R16x9,
    #[value(name = "9:16")]
    R9x16,
    #[value(name = "21:9")]
    R21x9,
    #[value(name = "auto")]
    Auto,
}

impl AspectRatio {
    pub fn to_str(&self) -> &'static str {
        match self {
            Self::R1x1 => "1:1",
            Self::R3x2 => "3:2",
            Self::R2x3 => "2:3",
            Self::R4x3 => "4:3",
            Self::R3x4 => "3:4",
            Self::R16x9 => "16:9",
            Self::R9x16 => "9:16",
            Self::R21x9 => "21:9",
            Self::Auto => "auto",
        }
    }
}

// ---------------------------------------------------------------------------
// Quality
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[clap(rename_all = "snake_case")]
pub enum Quality {
    Auto,
    Low,
    Medium,
    High,
}

impl Quality {
    pub fn to_str(&self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
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
    pub fn to_str(&self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpeg",
            Self::Webp => "webp",
            Self::Svg => "svg",
        }
    }

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

impl std::fmt::Display for ApiErrorBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message.as_deref().unwrap_or("unknown error"))
    }
}
