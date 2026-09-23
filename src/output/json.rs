//! Structured JSON output for agent tooling.

use std::path::Path;

use serde::Serialize;
use serde_json::{json, Value};

use crate::GenerationResult;

/// JSON result envelope written to stdout.
#[derive(Debug, Serialize)]
struct ResultEnvelope {
    schema_version: &'static str,
    status: &'static str,
    images: Vec<ImageEntry>,
    usage: Option<UsageEntry>,
    model: String,
    n: u32,
    elapsed_ms: u64,
    output_dir: String,
    warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ImageEntry {
    path: String,
    media_type: String,
    b64_length: usize,
}

#[derive(Debug, Serialize)]
struct UsageEntry {
    prompt_tokens: Option<i64>,
    completion_tokens: Option<i64>,
    total_tokens: Option<i64>,
    cost: Option<f64>,
}

impl From<&crate::models_mod::Usage> for UsageEntry {
    fn from(u: &crate::models_mod::Usage) -> Self {
        Self {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
            cost: u.cost,
        }
    }
}

/// Write the final structured JSON result to stdout.
pub fn final_result(result: &GenerationResult) {
    let images: Vec<ImageEntry> = result
        .saved_paths
        .iter()
        .map(|p| ImageEntry {
            path: p.display().to_string(),
            media_type: result.media_type.clone(),
            b64_length: result.b64_len,
        })
        .collect();

    let envelope = ResultEnvelope {
        schema_version: "1.0",
        status: "ok",
        images,
        usage: result.usage.as_ref().map(UsageEntry::from),
        model: result.model.clone(),
        n: result.n,
        elapsed_ms: result.elapsed_ms,
        output_dir: result.output_dir.display().to_string(),
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
        "default_output_dir": "~/generated_images",
    });
    println!("{}", serde_json::to_string_pretty(&obj)?);
    Ok(())
}

/// Write the models list as JSON.
pub fn models(model_list: &[&str]) {
    let obj = json!({
        "version": env!("CARGO_PKG_VERSION"),
        "default": "openai/gpt-image-2",
        "models": model_list,
    });
    println!("{}", serde_json::to_string_pretty(&obj).unwrap());
}

/// Return the JSON Schema for the result envelope.
pub fn schema() -> Value {
    json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "title": "openrouter-image result v1.0",
        "description": "Structured result emitted by openrouter-image --json on stdout.",
        "type": "object",
        "required": ["schema_version", "status", "images", "model", "elapsed_ms", "output_dir"],
        "properties": {
            "schema_version": {
                "type": "string",
                "const": "1.0",
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
                        "b64_length": { "type": "integer", "description": "Length of the base64 payload." }
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
                    "cost": { "type": ["number", "null"] }
                }
            },
            "model": { "type": "string", "description": "Model slug used for generation." },
            "n": { "type": "integer", "description": "Number of images requested." },
            "elapsed_ms": { "type": "integer", "description": "Total wall-clock time in milliseconds." },
            "output_dir": { "type": "string", "description": "Directory where images were saved." },
            "warnings": {
                "type": "array",
                "description": "Non-fatal issues (e.g. fewer images returned than requested).",
                "items": { "type": "string" }
            }
        }
    })
}
