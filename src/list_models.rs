//! Live model discovery from OpenRouter's dedicated Image API endpoint:
//! `GET https://openrouter.ai/api/v1/images/models`.
//!
//! Why this endpoint and not `/api/v1/models`:
//! - `/api/v1/models` returns chat-completions models that *happen* to have
//!   image output. Their `supported_parameters` rarely include `resolution`,
//!   and they ignore the field at request time.
//! - `/api/v1/images/models` returns models behind the dedicated image
//!   router (`POST /api/v1/images`). These advertise per-endpoint
//!   `supported_parameters` with `resolution`, `aspect_ratio`, `quality`,
//!   etc. The OpenRouter docs (Sep 2026) flag this endpoint as the source
//!   of truth for image generation.
//!
//! Response shape (relevant fields):
//! ```json
//! {
//!   "data": [{
//!     "id": "bytedance-seed/seedream-4.5",
//!     "name": "Seedream 4.5",
//!     "architecture": { "input_modalities": [...], "output_modalities": [...] },
//!     "supported_parameters": {
//!       "resolution": { "type": "enum", "values": ["1K", "2K", "4K"] },
//!       "n":         { "type": "range", "min": 1, "max": 1 },
//!       ...
//!     },
//!     "endpoints": "/api/v1/images/models/bytedance-seed/seedream-4.5/endpoints"
//!   }]
//! }
//! ```

use std::collections::BTreeMap;

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error_mod::ApiError;

// ---------------------------------------------------------------------------
// OpenRouter /api/v1/images/models response
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct ModelsResponse {
    pub data: Vec<ModelEntry>,
}

/// A model entry from `/api/v1/images/models`.
///
/// `supported_parameters` is a map keyed by parameter name. The shape is
/// `BTreeMap<String, ParamSpec>` so we can check feature support cheaply
/// (e.g. `model.supported_parameters.contains_key("resolution")`).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ModelEntry {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub created: Option<i64>,
    #[serde(default)]
    pub architecture: Option<Architecture>,
    /// Per-endpoint parameter support, e.g. `{"resolution": {"type":"enum","values":["1K","2K","4K"]}}`.
    #[serde(default)]
    pub supported_parameters: BTreeMap<String, ParamSpec>,
    #[serde(default)]
    pub supports_streaming: bool,
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

/// One entry in `supported_parameters`.
///
/// - `enum` style: `{"type":"enum","values":["1K","2K","4K"]}`
/// - `range` style: `{"type":"range","min":1,"max":1}`
/// - `boolean` style: `{"type":"boolean"}` (presence-only)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ParamSpec {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub values: Vec<String>,
    #[serde(default)]
    pub min: Option<i64>,
    #[serde(default)]
    pub max: Option<i64>,
}

impl ParamSpec {
    /// Returns the enum values if this is an `enum`-style parameter.
    pub fn enum_values(&self) -> Option<&[String]> {
        if self.kind == "enum" {
            Some(&self.values)
        } else {
            None
        }
    }
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

/// Returns true if the model advertises a `resolution` parameter.
pub fn supports_resolution(model: &ModelEntry) -> bool {
    model.supported_parameters.contains_key("resolution")
}

/// If `resolution` is advertised as an enum, return its allowed values.
/// Returns `None` if `resolution` is missing or not an enum.
pub fn resolution_values(model: &ModelEntry) -> Option<Vec<String>> {
    model
        .supported_parameters
        .get("resolution")
        .and_then(|p| p.enum_values())
        .map(|v| v.to_vec())
}

// ---------------------------------------------------------------------------
// API fetch
// ---------------------------------------------------------------------------

const IMAGE_MODELS_ENDPOINT: &str = "https://openrouter.ai/api/v1/images/models";

/// Fetch image-capable models from OpenRouter's dedicated image endpoint.
///
/// `base_url` defaults to `https://openrouter.ai/api/v1/images/models`.
/// Override for integration tests (point at a wiremock server).
pub async fn fetch_image_models(
    client: &Client,
    api_key: &str,
    base_url: Option<&str>,
) -> Result<Vec<ModelEntry>, ApiError> {
    let endpoint = base_url.unwrap_or(IMAGE_MODELS_ENDPOINT);
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

    // All entries from this endpoint are image-capable by definition
    // (output_modalities always contains "image" per OpenRouter docs).
    // Still filter for defence-in-depth.
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
            description: None,
            created: None,
            architecture: Some(Architecture {
                modality: None,
                input_modalities: vec!["text".to_string()],
                output_modalities: om.iter().map(|s| s.to_string()).collect(),
            }),
            supported_parameters: BTreeMap::new(),
            supports_streaming: false,
        }
    }

    fn entry_with_resolution(values: &[&str]) -> ModelEntry {
        let mut sp = BTreeMap::new();
        sp.insert(
            "resolution".to_string(),
            ParamSpec {
                kind: "enum".to_string(),
                values: values.iter().map(|s| s.to_string()).collect(),
                min: None,
                max: None,
            },
        );
        ModelEntry {
            id: "test/model".to_string(),
            name: None,
            description: None,
            created: None,
            architecture: Some(Architecture {
                modality: None,
                input_modalities: vec!["text".to_string()],
                output_modalities: vec!["image".to_string()],
            }),
            supported_parameters: sp,
            supports_streaming: false,
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
        let with = entry_with_resolution(&["1K", "2K", "4K"]);
        assert!(supports_resolution(&with));
        let mut without = entry_with_resolution(&["1K", "2K", "4K"]);
        without.supported_parameters.remove("resolution");
        assert!(!supports_resolution(&without));
    }

    #[test]
    fn resolution_values_returns_enum_list() {
        let e = entry_with_resolution(&["1K", "2K", "4K"]);
        assert_eq!(
            resolution_values(&e),
            Some(vec!["1K".to_string(), "2K".to_string(), "4K".to_string()])
        );
        let mut e2 = entry_with_resolution(&["1K"]);
        e2.supported_parameters.remove("resolution");
        assert_eq!(resolution_values(&e2), None);
    }
}
