//! `openrouter-image-core` — shared logic for the openrouter-image CLI.
//!
//! Key types are re-exported at crate root for ergonomic use.

mod client_mod;
mod config_mod;
mod error_mod;
mod list_models;
mod models_mod;
mod output;
mod paths_mod;
mod progress_mod;
mod reference_mod;

use std::path::PathBuf;

use anyhow::Context;
use base64::Engine;
use client_mod::HttpClient as Client;

use progress_mod::emit_progress_event;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

pub use config_mod::Config;
pub use error_mod::ApiError;
pub use error_mod::ConfigError;
pub use error_mod::ReferenceError;
pub use list_models::{fetch_image_models, is_image_model, ModelEntry, ModelsResponse};
pub use models_mod::{ApiResponse, GenerationParams, OutputFormat};
pub use output::OutputMode;
pub use paths_mod::{default_output_dir, resolve_output_paths};
pub use progress_mod::ProgressEvent;
// Expose HttpClient for integration tests (uses new_with_url)
pub use client_mod::HttpClient;

// ---------------------------------------------------------------------------
// Data URI validation (re-exported for CLI use)
// ---------------------------------------------------------------------------

pub use reference_mod::validate_data_uri;

// ---------------------------------------------------------------------------
// Core API
// ---------------------------------------------------------------------------

/// Result of a generation run.
#[derive(Debug, Clone)]
pub struct GenerationResult {
    pub saved_paths: Vec<PathBuf>,
    pub media_type: String,
    pub b64_len: usize,
    pub usage: Option<models_mod::Usage>,
    pub model: String,
    pub n: u8,
    pub elapsed_ms: u64,
    pub warnings: Vec<String>,
}

/// Run with progress events emitted to stderr.
pub async fn run_with_progress(
    params: GenerationParams,
    config: Config,
    output_mode: OutputMode,
) -> anyhow::Result<GenerationResult> {
    let client = Client::new(params.timeout_ms())?;

    let (response, elapsed_ms) = client
        .call_with_progress(&params, &config, |event| {
            emit_progress_event(event, output_mode);
        })
        .await
        .map_err(ApiError::to_anyhow)?;

    if response.data.as_ref().map(|d| d.is_empty()).unwrap_or(true) {
        if matches!(output_mode, OutputMode::Json) {
            eprintln!(
                "{}",
                serde_json::json!({"event": "error", "error": "OpenRouter returned no image data"})
            );
        }
        anyhow::bail!("OpenRouter returned no image data");
    }

    let result = write_images(&response, &params, elapsed_ms.as_millis() as u64).await?;

    match output_mode {
        OutputMode::Json => output::json::final_result(&result),
        OutputMode::Human => output::human::final_result(&result),
    }

    Ok(result)
}

async fn write_images(
    response: &ApiResponse,
    params: &GenerationParams,
    elapsed_ms: u64,
) -> anyhow::Result<GenerationResult> {
    let images = response.data.as_deref().unwrap_or(&[]);

    let mut saved_paths = Vec::new();
    let mut media_type = "image/png".to_string();
    let mut b64_len = 0usize;
    let mut warnings = Vec::new();

    // Ensure we have an output path for each requested image
    let expected_count = params.output_paths.len().max(images.len());

    for (idx, img) in images.iter().enumerate() {
        let Some(b64_json) = &img.b64_json else {
            continue;
        };

        let buf = base64::engine::general_purpose::STANDARD
            .decode(b64_json)
            .with_context(|| format!("failed to base64-decode image {}", idx + 1))?;

        let path = params
            .output_paths
            .get(idx)
            .cloned()
            .unwrap_or_else(|| PathBuf::from(format!("output-{}.png", idx + 1)));

        // Ensure parent dir exists
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() && parent != std::path::Path::new(".") {
                std::fs::create_dir_all(parent).with_context(|| {
                    format!("failed to create directory `{}`", parent.display())
                })?;
            }
        }

        std::fs::write(&path, &buf)
            .with_context(|| format!("failed to write `{}`", path.display()))?;

        if let Some(mt) = &img.media_type {
            media_type.clone_from(mt);
        }
        b64_len += b64_json.len();
        saved_paths.push(path);
    }

    if saved_paths.is_empty() {
        anyhow::bail!("OpenRouter response had empty b64_json fields");
    }

    if saved_paths.len() < expected_count {
        warnings.push(format!(
            "received {} image(s), requested {}",
            saved_paths.len(),
            expected_count
        ));
    }

    Ok(GenerationResult {
        saved_paths,
        media_type,
        b64_len,
        usage: response.usage.clone(),
        model: params.model.clone(),
        n: params.n,
        elapsed_ms,
        warnings,
    })
}

// ---------------------------------------------------------------------------
// Binary helpers (re-exported from submodules)
// ---------------------------------------------------------------------------

pub use output::human::info as emit_human_info;
pub use output::json::info as emit_json_info;
pub use output::json::models as emit_json_models;
pub use output::json::models_table as emit_human_models;
pub use output::json::schema;
