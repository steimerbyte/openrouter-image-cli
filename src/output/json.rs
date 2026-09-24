//! Structured JSON output for agent tooling.

use std::path::Path;

use serde::Serialize;
use serde_json::{json, Value};

use crate::list_models::ModelEntry;
use crate::GenerationResult;

// ---------------------------------------------------------------------------
// Result envelope types
// ---------------------------------------------------------------------------

/// JSON result envelope written to stdout on success.
#[derive(Debug, Serialize)]
struct ResultEnvelope {
    schema_version: &'static str,
    status: &'static str,
    images: Vec<ImageEntry>,
    usage: Option<UsageEntry>,
    model: String,
    n: u8,
    elapsed_ms: u64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    warnings: Vec<String>,
}

/// One entry per saved image file.
#[derive(Debug, Serialize)]
struct ImageEntry {
    path: String,
    media_type: String,
    b64_length: usize,
    /// Revised prompt returned by OpenAI-compatible models.
    #[serde(skip_serializing_if = "Option::is_none")]
    revised_prompt: Option<String>,
    /// Background composition of the generated image.
    #[serde(skip_serializing_if = "Option::is_none")]
    background: Option<String>,
}

/// Token and cost data, enriched with new OpenRouter fields.
#[derive(Debug, Serialize)]
struct UsageEntry {
    prompt_tokens: Option<i64>,
    completion_tokens: Option<i64>,
    total_tokens: Option<i64>,
    cost: Option<f64>,
    /// Whether the request used a bring-your-own-key model.
    #[serde(skip_serializing_if = "Option::is_none")]
    is_byok: Option<bool>,
    /// Detailed cost breakdown.
    #[serde(skip_serializing_if = "Option::is_none")]
    cost_details: Option<CostDetailsEntry>,
    /// Prompt token breakdown.
    #[serde(skip_serializing_if = "Option::is_none")]
    prompt_tokens_details: Option<PromptTokensDetailsEntry>,
    /// Completion token breakdown.
    #[serde(skip_serializing_if = "Option::is_none")]
    completion_tokens_details: Option<CompletionTokensDetailsEntry>,
}

#[derive(Debug, Serialize)]
struct CostDetailsEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    upstream_inference_cost: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    upstream_inference_prompt_cost: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    upstream_inference_completions_cost: Option<f64>,
}

#[derive(Debug, Serialize)]
struct PromptTokensDetailsEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    cached_tokens: Option<i64>,
}

#[derive(Debug, Serialize)]
struct CompletionTokensDetailsEntry {
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_tokens: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image_tokens: Option<i64>,
}

// ---------------------------------------------------------------------------
// Converters
// ---------------------------------------------------------------------------

impl From<&crate::models_mod::Usage> for UsageEntry {
    fn from(u: &crate::models_mod::Usage) -> Self {
        Self {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
            cost: u.cost,
            is_byok: u.is_byok,
            cost_details: u.cost_details.as_ref().map(|cd| CostDetailsEntry {
                upstream_inference_cost: cd.upstream_inference_cost,
                upstream_inference_prompt_cost: cd.upstream_inference_prompt_cost,
                upstream_inference_completions_cost: cd.upstream_inference_completions_cost,
            }),
            prompt_tokens_details: u.prompt_tokens_details.as_ref().map(|ptd| {
                PromptTokensDetailsEntry {
                    cached_tokens: ptd.cached_tokens,
                }
            }),
            completion_tokens_details: u.completion_tokens_details.as_ref().map(|ctd| {
                CompletionTokensDetailsEntry {
                    reasoning_tokens: ctd.reasoning_tokens,
                    image_tokens: ctd.image_tokens,
                }
            }),
        }
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Write the final structured JSON result to stdout.
pub fn final_result(result: &GenerationResult) {
    let images: Vec<ImageEntry> = result
        .saved_paths
        .iter()
        .map(|p| {
            // revised_prompt and background come from the API response;
            // they are not directly available on GenerationResult.
            // The caller can pass them via a separate mechanism if needed.
            ImageEntry {
                path: p.display().to_string(),
                media_type: result.media_type.clone(),
                b64_length: result.b64_len,
                revised_prompt: None,
                background: None,
            }
        })
        .collect();

    let envelope = ResultEnvelope {
        schema_version: "1.1",
        status: "ok",
        images,
        usage: result.usage.as_ref().map(UsageEntry::from),
        model: result.model.clone(),
        n: result.n,
        elapsed_ms: result.elapsed_ms,
        warnings: result.warnings.clone(),
    };

    println!("{}", serde_json::to_string(&envelope).unwrap());
}

/// Write the `info` command output as JSON.
pub fn info(
    key_status: &str,
    masked_key: &str,
    config_path: &Path,
    config_exists: bool,
) -> anyhow::Result<()> {
    let obj = json!({
        "version": env!("CARGO_PKG_VERSION"),
        "key_status": key_status,
        "key_masked": masked_key,
        "config_path": config_path.display().to_string(),
        "config_exists": config_exists,
        "config_lookup": "OPENROUTER_API_KEY env > ~/.config/openrouter-image/config.toml",
        "default_output": "~/generated-images/output.png",
    });
    println!("{}", serde_json::to_string_pretty(&obj)?);
    Ok(())
}

/// Write image models as a JSON array.
pub fn models(models: &[ModelEntry]) {
    let obj = json!({
        "version": env!("CARGO_PKG_VERSION"),
        "count": models.len(),
        "models": models,
    });
    println!("{}", serde_json::to_string_pretty(&obj).unwrap());
}

/// Print image models as a human-readable table.
pub fn models_table(models: &[ModelEntry]) {
    use crate::list_models::{is_image_model, resolution_values};
    println!(
        "openrouter-image v{}  (live from openrouter.ai)",
        env!("CARGO_PKG_VERSION")
    );
    println!();
    println!("{:50} {:>15} {:>4} {:>4}", "ID", "Resolution", "Str", "Out");
    println!("{}", "-".repeat(78));
    for m in models {
        let res = resolution_values(m)
            .map(|v| v.join(","))
            .unwrap_or_else(|| "—".to_string());
        let stream = if m.supports_streaming { "yes" } else { "—" };
        let out = if is_image_model(m) { "img" } else { "—" };
        println!("{:50} {:>15} {:>4} {:>4}", m.id, res, stream, out);
    }
    println!();
    println!("Total: {} image-capable model(s)", models.len());
    println!("Default: bytedance-seed/seedream-4.5  (supports 1K, 2K, 4K)");
    println!();
    println!("Resolution = enum values from supported_parameters.resolution.");
    println!("Str = model supports streaming (SSE partial images).");
    println!("Out = architecture.output_modalities contains 'image'.");
    println!();
    println!("Use 'openrouter-image endpoints <model-id>' for per-endpoint details.");
}

/// Return the JSON Schema for the result envelope.
pub fn schema() -> Value {
    json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "title": "openrouter-image result v1.1",
        "description": "Structured result emitted by openrouter-image --json on stdout.",
        "type": "object",
        "required": ["schema_version", "status", "images", "model", "elapsed_ms"],
        "properties": {
            "schema_version": {
                "type": "string",
                "const": "1.1",
                "description": "Bump on breaking changes to the result schema."
            },
            "status": {
                "type": "string",
                "enum": ["ok"],
                "description": "Always 'ok' when generation succeeded. Errors go to stderr as NDJSON."
            },
            "images": {
                "type": "array",
                "description": "One entry per saved image file.",
                "items": {
                    "type": "object",
                    "required": ["path", "media_type", "b64_length"],
                    "properties": {
                        "path": { "type": "string", "description": "Absolute path to the saved image." },
                        "media_type": { "type": "string", "description": "MIME type, e.g. image/png." },
                        "b64_length": { "type": "integer", "description": "Length of the base64 payload." },
                        "revised_prompt": { "type": ["string", "null"], "description": "Revised prompt returned by OpenAI-compatible models." },
                        "background": { "type": ["string", "null"], "description": "Background composition of the generated image." }
                    }
                }
            },
            "usage": {
                "type": ["object", "null"],
                "description": "Token and cost data from OpenRouter.",
                "properties": {
                    "prompt_tokens": { "type": ["integer", "null"] },
                    "completion_tokens": { "type": ["integer", "null"] },
                    "total_tokens": { "type": ["integer", "null"] },
                    "cost": { "type": ["number", "null"] },
                    "is_byok": { "type": ["boolean", "null"], "description": "Whether the request used a bring-your-own-key model." },
                    "cost_details": {
                        "type": ["object", "null"],
                        "properties": {
                            "upstream_inference_cost": { "type": ["number", "null"] },
                            "upstream_inference_prompt_cost": { "type": ["number", "null"] },
                            "upstream_inference_completions_cost": { "type": ["number", "null"] }
                        }
                    },
                    "prompt_tokens_details": {
                        "type": ["object", "null"],
                        "properties": {
                            "cached_tokens": { "type": ["integer", "null"] }
                        }
                    },
                    "completion_tokens_details": {
                        "type": ["object", "null"],
                        "properties": {
                            "reasoning_tokens": { "type": ["integer", "null"] },
                            "image_tokens": { "type": ["integer", "null"] }
                        }
                    }
                }
            },
            "model": { "type": "string", "description": "Model slug used for generation." },
            "n": { "type": "integer", "description": "Number of images requested." },
            "elapsed_ms": { "type": "integer", "description": "Total wall-clock time in milliseconds." },
            "warnings": {
                "type": "array",
                "description": "Non-fatal issues (e.g. fewer images returned than requested).",
                "items": { "type": "string" }
            }
        }
    })
}
