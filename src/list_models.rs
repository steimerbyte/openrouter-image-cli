//! Live model discovery from OpenRouter /api/v1/models.
//!
//! Filter strategy: a model is image-capable when its `architecture.output_modalities`
//! contains `"image"`. This is sourced directly from the API and is more reliable
//! than keyword matching on the model id.
//!
//! The API response also exposes `supported_parameters` per model. When the list
//! contains `"resolution"` (or `image_size`, etc.), the model supports the
//! corresponding OpenRouter request field; clients can decide whether to send it.

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error_mod::ApiError;

// ---------------------------------------------------------------------------
// OpenRouter models API response
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct ModelsResponse {
    pub data: Vec<ModelEntry>,
}

/// Subset of an OpenRouter `models` entry we care about.
///
/// Fields are `Option` because OpenRouter returns them inconsistently across
/// providers and we want forward compat.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelEntry {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub context_length: Option<usize>,
    #[serde(default)]
    pub pricing: Option<serde_json::Value>,
    /// Per-model capability list, e.g. `["resolution", "tools", ...]`.
    /// Used to detect resolution support when present.
    #[serde(default)]
    pub supported_parameters: Option<Vec<String>>,
    #[serde(default)]
    pub architecture: Option<Architecture>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Architecture {
    #[serde(default)]
    pub modality: Option<String>,
    #[serde(default)]
    pub input_modalities: Vec<String>,
    #[serde(default)]
    pub output_modalities: Vec<String>,
}

// ---------------------------------------------------------------------------
// Image-model filter (API-driven)
// ---------------------------------------------------------------------------

/// Returns true if the model advertises image output via `architecture.output_modalities`.
pub fn is_image_model(model: &ModelEntry) -> bool {
    model
        .architecture
        .as_ref()
        .map(|a| a.output_modalities.iter().any(|m| m == "image"))
        .unwrap_or(false)
}

/// Returns true if the model advertises a `resolution`-style parameter.
pub fn supports_resolution(model: &ModelEntry) -> bool {
    model
        .supported_parameters
        .as_ref()
        .map(|params| {
            params.iter().any(|p| {
                let p = p.to_lowercase();
                p == "resolution" || p == "image_size" || p.contains("resolution")
            })
        })
        .unwrap_or(false)
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
        .filter(is_image_model)
        .collect();

    Ok(image_models)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry_with_output_modalities(om: &[&str]) -> ModelEntry {
        ModelEntry {
            id: "test/model".to_string(),
            name: None,
            context_length: None,
            pricing: None,
            supported_parameters: None,
            architecture: Some(Architecture {
                modality: None,
                input_modalities: vec!["text".to_string()],
                output_modalities: om.iter().map(|s| s.to_string()).collect(),
            }),
        }
    }

    fn entry_with_params(params: &[&str]) -> ModelEntry {
        ModelEntry {
            id: "test/model".to_string(),
            name: None,
            context_length: None,
            pricing: None,
            supported_parameters: Some(params.iter().map(|s| s.to_string()).collect()),
            architecture: None,
        }
    }

    #[test]
    fn is_image_model_detects_image_output_modality() {
        assert!(is_image_model(&entry_with_output_modalities(&["image"])));
        assert!(is_image_model(&entry_with_output_modalities(&["text", "image"])));
        assert!(!is_image_model(&entry_with_output_modalities(&["text"])));
        assert!(!is_image_model(&entry_with_output_modalities(&[])));
    }

    #[test]
    fn supports_resolution_detects_resolution_param() {
        assert!(supports_resolution(&entry_with_params(&["resolution"])));
        assert!(supports_resolution(&entry_with_params(&["temperature", "resolution"])));
        assert!(supports_resolution(&entry_with_params(&["image_resolution"])));
        assert!(supports_resolution(&entry_with_params(&["image_size"])));
        assert!(!supports_resolution(&entry_with_params(&["temperature", "tools"])));
        assert!(!supports_resolution(&entry_with_params(&[])));
    }
}
