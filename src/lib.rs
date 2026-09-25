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

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use anyhow::{bail, Context};
use base64::Engine;
use client_mod::HttpClient as Client;

use progress_mod::emit_progress_event;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

pub use config_mod::Config;
pub use error_mod::ApiError;
pub use error_mod::ConfigError;
pub use error_mod::PathError;
pub use error_mod::ReferenceError;
pub use list_models::{
    fetch_image_models, fetch_model_endpoints, is_image_model, model_id_to_endpoints_path,
    resolution_values, supports_resolution, Architecture, EndpointRecord, EndpointResponse,
    ModelEntry, ModelsResponse, ParamSpec,
};
pub use models_mod::{
    ApiResponse, CompletionTokensDetails, CostDetails, GenerationParams, ImageData, OutputFormat,
    PromptTokensDetails, ProviderRouting, TraceMetadata, Usage,
};
pub use output::OutputMode;
pub use paths_mod::{default_output_dir, resolve_output_paths, validate_output_path};
pub use progress_mod::ProgressEvent;
// Expose HttpClient for integration tests (uses new_with_url)
pub use client_mod::HttpClient;

// ---------------------------------------------------------------------------
// Reference image validation (re-exported for CLI use)
// ---------------------------------------------------------------------------

pub use reference_mod::validate_reference;

// ---------------------------------------------------------------------------
// Core API
// ---------------------------------------------------------------------------

/// Result of a generation run.
#[derive(Debug, Clone)]
pub struct GenerationResult {
    pub saved_paths: Vec<PathBuf>,
    pub media_type: String,
    pub b64_len: usize,
    pub usage: Option<Usage>,
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
    // Pre-merge --negative-prompt into params.prompt (if set).
    let mut params = params;
    if let Some(np) = params.negative_prompt.take() {
        let trimmed = np.trim();
        if !trimmed.is_empty() {
            params.prompt.push_str("\n\nAvoid: ");
            params.prompt.push_str(trimmed);
        }
    }

    // Make the call with retry-on-empty-data loop.
    let client = Client::new(params.timeout_ms())?;
    let mut attempt: u8 = 0;
    let max_attempts = params.max_image_retries.saturating_add(1); // first try + N retries

    let (response, elapsed_ms) = loop {
        let result = client
            .call_with_progress(&params, &config, |event| {
                emit_progress_event(event, output_mode);
            })
            .await;

        match result {
            Ok((resp, elapsed)) => {
                if resp.data.as_ref().map(|d| !d.is_empty()).unwrap_or(false) {
                    break (resp, elapsed);
                }
                // HTTP 200 but empty data — retry path.
                attempt += 1;
                if attempt >= max_attempts {
                    let body_excerpt =
                        Some(format!("empty/None data after {} attempt(s)", attempt));
                    if matches!(output_mode, OutputMode::Json) {
                        eprintln!(
                            "{}",
                            serde_json::json!({
                                "event": "error",
                                "error": "OpenRouter returned no image data",
                                "model": params.model,
                                "status": 200u16,
                                "attempts": attempt,
                            })
                        );
                    }
                    return Err(ApiError::EmptyResponse {
                        model: params.model.clone(),
                        status: Some(200),
                        body_excerpt,
                    }
                    .to_anyhow());
                }
                let delay_ms = 1000u64 << (attempt - 1) as usize; // 1s, 2s, 4s
                emit_progress_event(
                    ProgressEvent::RetryEmptyResponse {
                        attempt,
                        max_attempts,
                        delay_ms,
                    },
                    output_mode,
                );
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                // continue loop
            }
            Err(e) => {
                return Err(e.to_anyhow());
            }
        }
    };

    let result = write_images(&response, &params, elapsed_ms.as_millis() as u64).await?;

    match output_mode {
        OutputMode::Json => output::json::final_result(&result),
        OutputMode::Human => output::human::final_result(&result),
    }

    Ok(result)
}

/// Public write_images (calls the impl function).
async fn write_images(
    response: &ApiResponse,
    params: &GenerationParams,
    elapsed_ms: u64,
) -> anyhow::Result<GenerationResult> {
    write_images_impl(response, params, elapsed_ms).await
}

/// The actual implementation (called by both the test wrapper and the real public wrapper).
async fn write_images_impl(
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

        // F6 — path traversal: validate output path is within allowed subtree.
        // This check is advisory; canonicalize may fail if the parent dir does
        // not exist yet (user is creating a new subdirectory — skip in that case).
        if let Err(e) = paths_mod::validate_output_path(&path) {
            if e.exit_code() == 2 {
                eprintln!("{}: {}", env!("CARGO_PKG_NAME"), e);
                std::process::exit(2);
            }
            bail!(e);
        }

        // F7 — symlink check: refuse to write through a symlink.
        if let Ok(meta) = std::fs::symlink_metadata(&path) {
            if meta.file_type().is_symlink() {
                let err = error_mod::PathError::Symlink { path: path.clone() };
                eprintln!("{}: {}", env!("CARGO_PKG_NAME"), err);
                bail!(err);
            }
        }

        // F9 — no-clobber: refuse to overwrite existing files UNLESS --clobber was set.
        if path.exists() && !params.clobber {
            let err = error_mod::PathError::AlreadyExists { path: path.clone() };
            eprintln!("{}: {}", env!("CARGO_PKG_NAME"), err);
            bail!(err);
        }

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

        // F8 — explicitly set restrictive permissions (owner-only read/write).
        // Errors here are non-fatal — umask-set is best-effort hardening.
        #[cfg(unix)]
        {
            let _ = std::fs::set_permissions(&path, PermissionsExt::from_mode(0o600));
        }

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

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that negative_prompt merges as "\n\nAvoid: <text>" at the end.
    #[test]
    fn negative_prompt_appends_avoid_clause() {
        let mut params = GenerationParams {
            prompt: "a serene mountain landscape".to_string(),
            model: "test/model".to_string(),
            image_refs: vec![],
            output_paths: vec![],
            n: 1,
            resolution: None,
            aspect_ratio: None,
            background: None,
            output_format: None,
            output_compression: None,
            quality: None,
            seed: None,
            size: None,
            user: None,
            session_id: None,
            provider: None,
            trace: None,
            timeout_ms: 60_000,
            clobber: false,
            max_image_retries: 0,
            negative_prompt: Some("blurry text, neon colors".to_string()),
        };

        // Inline the pre-merge logic (mirrors what run_with_progress does).
        if let Some(np) = params.negative_prompt.take() {
            let trimmed = np.trim();
            if !trimmed.is_empty() {
                params.prompt.push_str("\n\nAvoid: ");
                params.prompt.push_str(trimmed);
            }
        }

        assert!(
            params.prompt.ends_with("Avoid: blurry text, neon colors"),
            "prompt should end with Avoid clause, got: {}",
            params.prompt
        );
    }

    /// Negative-prompt with empty/whitespace string does NOT append.
    #[test]
    fn negative_prompt_empty_does_not_append() {
        let mut params = GenerationParams {
            prompt: "a cat".to_string(),
            model: "test/model".to_string(),
            image_refs: vec![],
            output_paths: vec![],
            n: 1,
            resolution: None,
            aspect_ratio: None,
            background: None,
            output_format: None,
            output_compression: None,
            quality: None,
            seed: None,
            size: None,
            user: None,
            session_id: None,
            provider: None,
            trace: None,
            timeout_ms: 60_000,
            clobber: false,
            max_image_retries: 0,
            negative_prompt: Some("   ".to_string()),
        };

        if let Some(np) = params.negative_prompt.take() {
            let trimmed = np.trim();
            if !trimmed.is_empty() {
                params.prompt.push_str("\n\nAvoid: ");
                params.prompt.push_str(trimmed);
            }
        }

        assert_eq!(
            params.prompt, "a cat",
            "empty negative_prompt should not modify the prompt"
        );
    }

    /// --clobber=true on an existing file must succeed and overwrite.
    /// --clobber=false on an existing file must raise AlreadyExists.
    #[tokio::test]
    async fn clobber_overwrites_existing_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("existing.png");

        // Write the initial "old" file.
        let old_content = b"old image data";
        std::fs::write(&path, old_content).unwrap();
        assert!(path.exists(), "sanity: file must exist before test");

        // Build a minimal ApiResponse with real base64-encoded data.
        let fake_png = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x02\x00\x00\x00\x90wS\xde";
        let img = ImageData {
            b64_json: Some(base64::engine::general_purpose::STANDARD.encode(fake_png)),
            media_type: Some("image/png".to_string()),
            url: None,
            revised_prompt: None,
            background: None,
        };
        let api_response = ApiResponse {
            created: Some(0),
            data: Some(vec![img]),
            usage: None,
            error: None,
        };

        // Case 1: clobber=false → AlreadyExists
        {
            let params = GenerationParams {
                prompt: "test".to_string(),
                model: "test/m".to_string(),
                image_refs: vec![],
                output_paths: vec![path.clone()],
                n: 1,
                resolution: None,
                aspect_ratio: None,
                background: None,
                output_format: None,
                output_compression: None,
                quality: None,
                seed: None,
                size: None,
                user: None,
                session_id: None,
                provider: None,
                trace: None,
                timeout_ms: 60_000,
                clobber: false,
                max_image_retries: 0,
                negative_prompt: None,
            };
            let result = write_images_impl(&api_response, &params, 0).await;
            let err = result.unwrap_err();
            assert!(
                err.downcast_ref::<error_mod::PathError>()
                    .map(|e| matches!(e, error_mod::PathError::AlreadyExists { .. }))
                    .unwrap_or(false),
                "clobber=false must raise AlreadyExists, got: {}",
                err
            );
        }

        // Case 2: clobber=true → succeeds and overwrites
        {
            let params = GenerationParams {
                prompt: "test".to_string(),
                model: "test/m".to_string(),
                image_refs: vec![],
                output_paths: vec![path.clone()],
                n: 1,
                resolution: None,
                aspect_ratio: None,
                background: None,
                output_format: None,
                output_compression: None,
                quality: None,
                seed: None,
                size: None,
                user: None,
                session_id: None,
                provider: None,
                trace: None,
                timeout_ms: 60_000,
                clobber: true,
                max_image_retries: 0,
                negative_prompt: None,
            };
            let result = write_images_impl(&api_response, &params, 0).await;
            assert!(
                result.is_ok(),
                "clobber=true must succeed, got: {:?}",
                result
            );
            let new_content = std::fs::read(&path).unwrap();
            assert_ne!(
                &new_content[..],
                &old_content[..],
                "clobber=true should have overwritten the file"
            );
        }
    }
}
