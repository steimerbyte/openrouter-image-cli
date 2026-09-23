//! CLI argument parsing via clap derive.

use std::path::PathBuf;

use clap::Parser;
use thiserror::Error;

use openrouter_image_core::{AspectRatio, ImageModel, OutputFormat, Quality};

/// CLI error types that map to specific exit codes.
#[derive(Debug, Error)]
#[error("{message}")]
pub struct CliError {
    message: String,
    exit_code: u8,
}

impl CliError {
    pub fn no_api_key() -> Self {
        Self {
            message: "OpenRouter API key not configured. Set OPENROUTER_API_KEY env var or write {\"apiKey\":\"sk-or-…\"} to ~/.omp/agent/image-gen.json (chmod 600).".to_string(),
            exit_code: 3,
        }
    }

    pub fn invalid_arg(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
            exit_code: 2,
        }
    }

    pub fn exit_code(&self) -> u8 {
        self.exit_code
    }
}

/// Top-level CLI.
#[derive(Debug, Parser)]
#[command(
    name = "openrouter-image",
    version,
    about = "Generate images via OpenRouter GPT Image models",
    long_about = None,
)]
pub struct Cli {
    /// Structured JSON on stdout, NDJSON progress on stderr.
    #[arg(long, short)]
    pub json: bool,

    /// Suppress all progress output (useful for agent loops).
    #[arg(long, short)]
    pub quiet: bool,

    #[command(subcommand)]
    pub command: Command,
}

/// Subcommands.
#[derive(Debug, Clone, Parser)]
pub enum Command {
    /// Generate one or more images.
    Gen(GenArgs),
    /// List all known models.
    Models,
    /// Show version and API key diagnostic.
    Info,
    /// Print the JSON result schema (for agent tooling).
    Schema,
}

/// Arguments for the `gen` subcommand.
#[derive(Debug, Clone, Parser)]
pub struct GenArgs {
    /// Prompt text (required, or via --prompt-file with -).
    #[arg(long = "prompt", short = 'p')]
    pub prompt: Option<String>,

    /// Read prompt from file (- for stdin).
    #[arg(long = "prompt-file", value_name = "PATH")]
    pub prompt_file: Option<String>,

    /// Model slug.
    #[arg(long, short = 'm', default_value = "openai/gpt-image-2")]
    pub model: ImageModel,

    /// Reference image: local file path, data: URL, or https URL.
    #[arg(long)]
    pub reference: Option<String>,

    /// Aspect ratio.
    #[arg(long, value_enum, default_value = "16:9")]
    pub aspect_ratio: AspectRatio,

    /// Quality preset.
    #[arg(long, value_enum)]
    pub quality: Option<Quality>,

    /// Background mode.
    #[arg(long)]
    pub background: Option<String>,

    /// Output format.
    #[arg(long, value_enum, default_value = "png")]
    pub output_format: OutputFormat,

    /// Resolution preset.
    #[arg(long)]
    pub resolution: Option<String>,

    /// Number of images to generate (1–10).
    #[arg(long, short = 'n', default_value = "1")]
    pub n: u32,

    /// Random seed for reproducibility.
    #[arg(long)]
    pub seed: Option<u64>,

    /// Output directory.
    #[arg(long)]
    pub output_dir: Option<PathBuf>,

    /// Timeout in milliseconds.
    #[arg(long, default_value = "120000")]
    pub timeout_ms: u64,

    /// Disable automatic retry on 5xx errors.
    #[arg(long)]
    pub no_retry: bool,
}

impl GenArgs {
    /// Convert CLI args into a `GenerationParams` for the library.
    pub fn into_params(self) -> Result<openrouter_image_core::GenerationParams, CliError> {
        let prompt = if let Some(p) = &self.prompt {
            p.clone()
        } else if let Some(pf) = &self.prompt_file {
            if pf == "-" {
                // Reading stdin from tty here would block; for --prompt-file -
                // a real implementation would read from stdin before the async context.
                // Fall back to empty — the actual stdin read happens in the caller.
                return Err(CliError::invalid_arg(
                    "stdin prompt not yet implemented — use --prompt or --prompt-file <path>",
                ));
            } else {
                std::fs::read_to_string(pf)
                    .map_err(|e| CliError::invalid_arg(format!("--prompt-file: {}", e)))?
            }
        } else {
            return Err(CliError::invalid_arg(
                "--prompt is required (or use --prompt-file - for stdin)",
            ));
        };

        let prompt = prompt.trim().to_string();
        if prompt.is_empty() {
            return Err(CliError::invalid_arg("prompt must not be empty"));
        }

        if self.n == 0 || self.n > 10 {
            return Err(CliError::invalid_arg(format!(
                "--n must be between 1 and 10, got {}",
                self.n
            )));
        }

        let output_dir = self.output_dir.unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("generated_images")
        });

        Ok(openrouter_image_core::GenerationParams {
            prompt,
            model: self.model,
            reference: self.reference,
            aspect_ratio: self.aspect_ratio,
            quality: self.quality,
            background: self.background,
            output_format: self.output_format,
            resolution: self.resolution,
            n: self.n,
            seed: self.seed,
            output_dir,
            timeout_ms: self.timeout_ms,
            retry: !self.no_retry,
        })
    }
}

/// Parse CLI arguments from `std::env::args_os()`.
pub fn parse() -> Result<Cli, CliError> {
    <Cli as clap::Parser>::try_parse().map_err(|e| CliError {
        message: e.to_string(),
        exit_code: 2,
    })
}
