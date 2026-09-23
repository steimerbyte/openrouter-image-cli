//! `openrouter-image-core` — shared logic for the openrouter-image CLI.
//!
//! Key types are re-exported at crate root for ergonomic use.

mod client_mod;
mod config_mod;
mod error_mod;
mod models_mod;
mod output;
mod progress_mod;
mod reference_mod;

use std::path::PathBuf;

use anyhow::Context;
use client_mod::HttpClient as Client;

use progress_mod::emit_progress_event;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

pub use config_mod::Config;
pub use error_mod::ApiError;
pub use models_mod::{ApiResponse, AspectRatio};
pub use models_mod::GenerationParams;
pub use models_mod::ImageModel;
pub use models_mod::OutputFormat;
pub use models_mod::Quality;
pub use models_mod::ALL_MODELS;
pub use output::OutputMode;
pub use progress_mod::ProgressEvent;

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
    pub n: u32,
    pub elapsed_ms: u64,
    pub output_dir: PathBuf,
    pub warnings: Vec<String>,
}

/// Run without progress.
pub async fn run(params: GenerationParams, config: Config) -> anyhow::Result<GenerationResult> {
    let client = Client::new(params.timeout_ms)?;
    let (response, elapsed_ms) = client
        .call(&params, &config)
        .await
        .map_err(ApiError::to_anyhow)?;
    write_images(response, params, elapsed_ms.as_millis() as u64).await
}

/// Run with progress events emitted to stderr.
pub async fn run_with_progress(
    params: GenerationParams,
    config: Config,
    output_mode: OutputMode,
) -> anyhow::Result<GenerationResult> {
    let client = Client::new(params.timeout_ms)?;
    let output_dir = params.output_dir.clone();

    let (response, elapsed_ms) = client
        .call_with_progress(&params, &config, |event| {
            emit_progress_event(event, output_mode);
        })
        .await
        .map_err(ApiError::to_anyhow)?;

    if response.data.as_ref().map(|d| d.is_empty()).unwrap_or(true) {
        emit_progress_event(
            ProgressEvent::Error {
                error: "OpenRouter returned no image data".to_string(),
            },
            output_mode,
        );
        anyhow::bail!("OpenRouter returned no image data");
    }

    let result = write_images_internal(
        &response,
        &params,
        output_dir,
        elapsed_ms.as_millis() as u64,
    )
    .await?;

    match output_mode {
        OutputMode::Json => output::json::final_result(&result),
        OutputMode::Human => output::human::final_result(&result),
        OutputMode::Quiet => {}
    }

    Ok(result)
}

async fn write_images(
    response: ApiResponse,
    params: GenerationParams,
    elapsed_ms: u64,
) -> anyhow::Result<GenerationResult> {
    write_images_internal(&response, &params, params.output_dir.clone(), elapsed_ms).await
}

async fn write_images_internal(
    response: &ApiResponse,
    params: &GenerationParams,
    output_dir: PathBuf,
    elapsed_ms: u64,
) -> anyhow::Result<GenerationResult> {
    let images = response.data.as_deref().unwrap_or(&[]);

    std::fs::create_dir_all(&output_dir).with_context(|| {
        format!(
            "failed to create output directory `{}`",
            output_dir.display()
        )
    })?;

    let timestamp = response.created.unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64
    });

    let ext = params.output_format.to_ext();
    let mut saved_paths = Vec::new();
    let mut media_type = "image/png".to_string();
    let mut b64_len = 0usize;
    let mut warnings = Vec::new();

    for (idx, img) in images.iter().enumerate() {
        let Some(b64_json) = &img.b64_json else {
            continue;
        };
        let buf = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b64_json)
            .with_context(|| format!("failed to base64-decode image {}", idx + 1))?;

        let filename = format!("openrouter-{}-{}.{}", timestamp, idx + 1, ext);
        let full_path = output_dir.join(&filename);
        std::fs::write(&full_path, &buf)
            .with_context(|| format!("failed to write `{}`", full_path.display()))?;

        if let Some(mt) = &img.media_type {
            media_type.clone_from(mt);
        }
        b64_len += b64_json.len();
        saved_paths.push(full_path);
    }

    if saved_paths.is_empty() {
        anyhow::bail!("OpenRouter response had empty b64_json fields");
    }

    if saved_paths.len() < params.n as usize {
        warnings.push(format!(
            "received {} image(s), requested {}",
            saved_paths.len(),
            params.n
        ));
    }

    Ok(GenerationResult {
        saved_paths,
        media_type,
        b64_len,
        usage: response.usage.clone(),
        model: params.model.to_string(),
        n: params.n,
        elapsed_ms,
        output_dir,
        warnings,
    })
}

// ---------------------------------------------------------------------------
// Binary helpers (re-exported from submodules)
// ---------------------------------------------------------------------------

pub use output::human::info as emit_human_info;
pub use output::human::models as emit_human_models;
pub use output::json::info as emit_json_info;
pub use output::json::models as emit_json_models;
pub use output::json::schema;
pub use reference_mod::resolve_reference;
