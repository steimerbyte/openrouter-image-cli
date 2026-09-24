//! Live model discovery from OpenRouter /api/v1/models.
//!
//! Filters for image-generation-capable models by ID pattern matching.
//! The full filter list is documented in the CLI help text.

use reqwest::Client;
use serde::Deserialize;
use serde::Serialize;

use crate::error_mod::ApiError;

// ---------------------------------------------------------------------------
// OpenRouter models API response
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct ModelsResponse {
    pub data: Vec<ModelEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelEntry {
    pub id: String,
    pub name: Option<String>,
    pub context_length: Option<usize>,
    // pricing is a raw JSON value — kept as Value for forward compat
    #[serde(default)]
    pub pricing: Option<serde_json::Value>,
}

// ---------------------------------------------------------------------------
// Image-model filter
// ---------------------------------------------------------------------------

/// Keywords that indicate a model supports image generation.
/// All patterns are OR'd together.
const IMAGE_KEYWORDS: &[&str] = &[
    "image",
    "dall",
    "flux",
    "sd-xl",
    "imagen",
    "gpt-image",
    "gemini-2.0-flash-exp",
    "gemini-2.5-pro",
    "gemini-2.5-flash",
    "gemini-3.0",
    "seedream",
    "reve",
    "kandinsky",
    "midjourney",
    "playground-v2",
];

/// Returns true if the model ID suggests image-generation capability.
pub fn is_image_model(model_id: &str) -> bool {
    let lower = model_id.to_lowercase();
    IMAGE_KEYWORDS.iter().any(|kw| lower.contains(kw))
}

// ---------------------------------------------------------------------------
// API fetch
// ---------------------------------------------------------------------------

const MODELS_ENDPOINT: &str = "https://openrouter.ai/api/v1/models";

/// Fetch all models from OpenRouter, filter to image-capable ones.
///
/// Requires an `api_key` to be passed directly (the caller sets the
/// Authorization header before calling this function).
///
/// `base_url` defaults to `https://openrouter.ai/api/v1/models`. Override
/// for integration tests (point at a wiremock server).
pub async fn fetch_image_models(
    client: &Client,
    api_key: &str,
    base_url: Option<&str>,
) -> Result<Vec<ModelEntry>, ApiError> {
    let endpoint = base_url.unwrap_or(MODELS_ENDPOINT);
    let resp = client
        .get(endpoint)
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                ApiError::Timeout(std::time::Duration::from_secs(30))
            } else {
                ApiError::Network(e)
            }
        })?;

    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap_or_default();

    if !(200..=299).contains(&status) {
        return Err(ApiError::from_status(status, &body));
    }

    let models: ModelsResponse = serde_json::from_str(&body).map_err(|e| ApiError::Http {
        status: 500,
        message: format!("failed to parse models response: {}", e),
    })?;

    let image_models: Vec<ModelEntry> = models
        .data
        .into_iter()
        .filter(|m| is_image_model(&m.id))
        .collect();

    Ok(image_models)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_image_filter_keywords() {
        let cases = &[
            ("openai/gpt-image-2", true),
            ("openai/dall-e-3", true),
            ("black-forest-labs/flux-1.1-pro", true),
            ("google/imagen-3", true),
            ("google/gemini-2.0-flash-exp", true),
            ("stability-ai/sd-xl-1.0", true),
            ("anthropic/claude-3.5-sonnet", false),
            ("meta/llama-3.1-8b-instruct", false),
            ("openai/gpt-4o", false),
            ("mistral/mistral-large", false),
            ("google/gemini-pro", false),
            ("deepseek/deepseek-chat", false),
        ];

        for (id, expected) in cases {
            assert_eq!(
                is_image_model(id),
                *expected,
                "is_image_model({:?}) should be {}",
                id,
                expected
            );
        }
    }
}
