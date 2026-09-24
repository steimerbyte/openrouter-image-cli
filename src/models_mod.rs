//! Data types for API requests and responses.
//!
//! No hardcoded model list — models are discovered live via `/api/v1/models`.

use std::fmt;

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Generation parameters
// ---------------------------------------------------------------------------

/// Parameters for an image generation request.
#[derive(Debug, Clone)]
pub struct GenerationParams {
    /// Prompt text.
    pub prompt: String,
    /// Model slug (e.g. "bytedance-seed/seedream-4.5").
    pub model: String,
    /// Optional reference image URIs (data: or http(s):// URLs).
    pub image_refs: Vec<String>,
    /// Output paths for the generated image(s).
    pub output_paths: Vec<std::path::PathBuf>,
    /// Number of images requested (1–10).
    pub n: u8,
    /// Optional resolution preset: "512" | "1K" | "2K" | "4K". When None, the
    /// OpenRouter API default (`2K`) applies.
    pub resolution: Option<String>,
    /// Aspect ratio enum value. Values: `1:1`, `1:2`, `1:4`, `1:8`, `2:1`,
    /// `2:3`, `2.35:1`, `3:2`, `3:4`, `4:1`, `4:3`, `4:5`, `5:2`, `5:4`,
    /// `8:1`, `9:16`, `16:9`, `9:19.5`, `19.5:9`, `9:20`, `20:9`, `9:21`,
    /// `21:9`, `auto`.
    pub aspect_ratio: Option<String>,
    /// Background composition. Values: `auto`, `transparent`, `opaque`.
    /// When `transparent`, output_format must be png or webp.
    pub background: Option<String>,
    /// Output image format sent to the API (API string: "png", "jpeg", "webp", "svg").
    pub output_format: Option<String>,
    /// Output compression level (0–100), only for jpeg/webp.
    pub output_compression: Option<u8>,
    /// Quality hint for the model. Values: `auto`, `low`, `medium`, `high`,
    /// `xhigh`, `max`.
    pub quality: Option<String>,
    /// Integer seed for deterministic generation.
    pub seed: Option<i64>,
    /// Explicit image size. Either a tier (`"2K"`, `"4K"`) or explicit pixel
    /// dimensions (`"2048x2048"`). Takes precedence over `resolution` and
    /// `aspect_ratio` when provided.
    pub size: Option<String>,
    /// End-user identifier (max 256 chars, hashed upstream).
    pub user: Option<String>,
    /// Session identifier (max 256 chars). Sets both the `X-Session-Id` header
    /// and the `session_id` body field.
    pub session_id: Option<String>,
    /// Provider routing preferences.
    pub provider: Option<ProviderRouting>,
    /// Observability trace metadata.
    pub trace: Option<TraceMetadata>,
    /// Enable SSE streaming. When true, the API returns partial image chunks
    /// via SSE events.
    pub stream: bool,
    /// HTTP timeout in milliseconds.
    pub timeout_ms: u64,
}

/// Provider routing preferences for the generation request.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ProviderRouting {
    /// Allow fallback to other providers if the preferred one is unavailable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_fallbacks: Option<bool>,
    /// Restrict to these provider slugs only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub only: Option<Vec<String>>,
    /// Exclude these provider slugs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore: Option<Vec<String>>,
    /// Ordered list of preferred provider slugs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<Vec<String>>,
    /// Sort strategy: `latency`, `throughput`, or `price`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort: Option<String>,
}

/// Observability trace metadata attached to the request.
/// Observability trace metadata attached to the request.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TraceMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_span_id: Option<String>,
    /// Extra key-value pairs merged into the trace object at the top level.
    #[serde(skip_serializing_if = "std::collections::HashMap::is_empty", flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

impl GenerationParams {
    /// HTTP timeout.
    pub fn timeout_ms(&self) -> u64 {
        self.timeout_ms
    }

    /// Build the JSON body sent to OpenRouter.
    /// Only fields that are `Some(...)` (or `true`/`> 1`) are included.
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
        if let Some(ar) = &self.aspect_ratio {
            body["aspect_ratio"] = serde_json::json!(ar);
        }
        if let Some(bg) = &self.background {
            body["background"] = serde_json::json!(bg);
        }
        if let Some(fmt) = &self.output_format {
            body["output_format"] = serde_json::json!(fmt);
        }
        if let Some(cmp) = self.output_compression {
            body["output_compression"] = serde_json::json!(cmp);
        }
        if let Some(q) = &self.quality {
            body["quality"] = serde_json::json!(q);
        }
        if let Some(seed) = self.seed {
            body["seed"] = serde_json::json!(seed);
        }
        if let Some(size) = &self.size {
            body["size"] = serde_json::json!(size);
        }
        if let Some(user) = &self.user {
            body["user"] = serde_json::json!(user);
        }
        if let Some(sid) = &self.session_id {
            body["session_id"] = serde_json::json!(sid);
        }
        if let Some(provider) = &self.provider {
            body["provider"] =
                serde_json::to_value(provider).unwrap_or_else(|_| serde_json::json!({}));
        }
        if let Some(trace) = &self.trace {
            let mut obj = serde_json::Map::new();
            if let Some(tid) = &trace.trace_id {
                obj.insert("trace_id".to_string(), serde_json::json!(tid));
            }
            if let Some(tn) = &trace.trace_name {
                obj.insert("trace_name".to_string(), serde_json::json!(tn));
            }
            if let Some(sn) = &trace.span_name {
                obj.insert("span_name".to_string(), serde_json::json!(sn));
            }
            if let Some(gn) = &trace.generation_name {
                obj.insert("generation_name".to_string(), serde_json::json!(gn));
            }
            if let Some(psid) = &trace.parent_span_id {
                obj.insert("parent_span_id".to_string(), serde_json::json!(psid));
            }
            for (k, v) in &trace.extra {
                obj.insert(k.clone(), v.clone());
            }
            body["trace"] = serde_json::Value::Object(obj);
        }
        if self.stream {
            body["stream"] = serde_json::json!(true);
        }

        body
    }

    /// Effective resolution after applying defaults. Returns "2K" when unset,
    /// matching the OpenRouter API default.
    pub fn effective_resolution(&self) -> &str {
        self.resolution.as_deref().unwrap_or("2K")
    }
}

// ---------------------------------------------------------------------------
// Output format
// ---------------------------------------------------------------------------

/// Image output format.
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

    /// API string value sent in the request body.
    pub fn as_api_str(&self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpeg",
            Self::Webp => "webp",
            Self::Svg => "svg",
        }
    }
}

// ---------------------------------------------------------------------------
// API response types
// ---------------------------------------------------------------------------

/// Top-level API response for POST /api/v1/images.
#[derive(Debug, Clone, Deserialize)]
pub struct ApiResponse {
    #[serde(rename = "created")]
    pub created: Option<i64>,
    #[serde(rename = "data")]
    pub data: Option<Vec<ImageData>>,
    #[serde(rename = "usage", default)]
    pub usage: Option<Usage>,
    #[serde(rename = "error", default)]
    #[allow(dead_code)]
    pub error: Option<ApiErrorBody>,
}

/// A single generated image in the response.
#[derive(Debug, Clone, Deserialize)]
pub struct ImageData {
    /// Base64-encoded image data.
    #[serde(rename = "b64_json")]
    pub b64_json: Option<String>,
    /// MIME media type of the image.
    #[serde(rename = "media_type", default)]
    pub media_type: Option<String>,
    /// Remote URL (used when the model returns a URL instead of b64_json).
    #[serde(rename = "url", default)]
    #[allow(dead_code)]
    pub url: Option<String>,
    /// Revised prompt returned by OpenAI-compatible models.
    #[serde(rename = "revised_prompt", default)]
    pub revised_prompt: Option<String>,
    /// Background composition of the generated image.
    #[serde(rename = "background", default)]
    pub background: Option<String>,
}

/// Token and cost data from the API response.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct Usage {
    #[serde(rename = "prompt_tokens", default)]
    pub prompt_tokens: Option<i64>,
    #[serde(rename = "completion_tokens", default)]
    pub completion_tokens: Option<i64>,
    #[serde(rename = "total_tokens", default)]
    pub total_tokens: Option<i64>,
    #[serde(rename = "cost", default)]
    pub cost: Option<f64>,
    /// Whether the request used a bring-your-own-key model.
    #[serde(rename = "is_byok", default)]
    pub is_byok: Option<bool>,
    /// Detailed cost breakdown.
    #[serde(rename = "cost_details", default)]
    pub cost_details: Option<CostDetails>,
    /// Prompt token breakdown.
    #[serde(rename = "prompt_tokens_details", default)]
    pub prompt_tokens_details: Option<PromptTokensDetails>,
    /// Completion token breakdown.
    #[serde(rename = "completion_tokens_details", default)]
    pub completion_tokens_details: Option<CompletionTokensDetails>,
}

/// Detailed cost breakdown.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct CostDetails {
    #[serde(rename = "upstream_inference_cost", default)]
    pub upstream_inference_cost: Option<f64>,
    #[serde(rename = "upstream_inference_prompt_cost", default)]
    pub upstream_inference_prompt_cost: Option<f64>,
    #[serde(rename = "upstream_inference_completions_cost", default)]
    pub upstream_inference_completions_cost: Option<f64>,
}

/// Breakdown of prompt tokens.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct PromptTokensDetails {
    #[serde(rename = "cached_tokens", default)]
    pub cached_tokens: Option<i64>,
}

/// Breakdown of completion tokens.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct CompletionTokensDetails {
    #[serde(rename = "reasoning_tokens", default)]
    pub reasoning_tokens: Option<i64>,
    #[serde(rename = "image_tokens", default)]
    pub image_tokens: Option<i64>,
}

/// Error body returned in the API response.
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
