//! CLI argument parsing via clap derive.

use std::path::PathBuf;

use clap::Parser;
use thiserror::Error;

use openrouter_image_core::OutputFormat;

// ---------------------------------------------------------------------------
// CLI error
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
#[error("{message}")]
pub struct CliError {
    message: String,
    exit_code: u8,
}

impl CliError {
    pub fn no_api_key() -> Self {
        Self {
            message: "OpenRouter API key not configured. Set OPENROUTER_API_KEY env var \
                      or write `api_key = \"sk-or-…\"` to ~/.config/openrouter-image/config.toml."
                .to_string(),
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

// ---------------------------------------------------------------------------
// Top-level CLI
// ---------------------------------------------------------------------------

#[derive(Debug, Parser)]
#[command(
    name = "openrouter-image",
    version,
    about = "Generate images via OpenRouter — GPT Image, DALL-E, Flux, Imagen, and more",
    long_about = None,
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

/// Subcommands.
#[derive(Debug, Clone, Parser)]
pub enum Command {
    /// Generate one or more images.
    Generate(Generate),
    /// List all image-capable models (live from OpenRouter).
    ListModels(ListModels),
    /// Show version and API key diagnostic.
    Info,
    /// Print the JSON result schema (for agent tooling).
    Schema,
}

// ---------------------------------------------------------------------------
// Generate subcommand
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Parser)]
pub struct Generate {
    /// Image generation prompt (required).
    #[arg(short = 'p', long = "prompt")]
    pub prompt: Option<String>,

    /// Model slug (default: openai/gpt-image-2).
    #[arg(short = 'm', long = "model", default_value = "openai/gpt-image-2")]
    pub model: String,

    /// Base64-encoded data URI reference image (repeatable).
    #[arg(long = "image-ref", value_name = "BASE64_DATA_URI")]
    pub image_refs: Vec<String>,

    /// Single output file (default: ~/generated-images/output.png when n=1).
    #[arg(short = 'o', long = "output")]
    pub output: Option<PathBuf>,

    /// Output directory for multiple images (n>1). Default: ~/generated-images/.
    #[arg(long = "output-dir")]
    pub output_dir: Option<PathBuf>,

    /// Structured JSON on stdout, NDJSON progress on stderr.
    #[arg(long)]
    pub json: bool,

    /// Print request body without calling API or writing files.
    #[arg(long)]
    pub dry_run: bool,

    /// Verbose tracing output (stderr).
    #[arg(long, short = 'v')]
    pub verbose: bool,

    /// Stream progress as NDJSON events (default when --json is set).
    #[arg(long)]
    pub stream: bool,

    /// Number of images to generate (1–10, default 1).
    #[arg(long, short = 'n', default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..=10))]
    pub n: u8,

    /// Output format for saved images.
    #[arg(long, value_enum, default_value = "png")]
    pub output_format: OutputFormat,
}

impl Generate {
    /// Validate arguments and resolve output paths.
    pub fn validate(self) -> Result<ValidatedGenerate, CliError> {
        let prompt = self
            .prompt
            .clone()
            .ok_or_else(|| CliError::invalid_arg("--prompt is required"))?
            .trim()
            .to_string();

        if prompt.is_empty() {
            return Err(CliError::invalid_arg("prompt must not be empty"));
        }

        // Validate all image refs are valid data URIs
        for ref_arg in &self.image_refs {
            openrouter_image_core::validate_data_uri(ref_arg)
                .map_err(|e| CliError::invalid_arg(format!("--image-ref: {}", e)))?;
        }

        let output_paths = openrouter_image_core::resolve_output_paths(
            self.n,
            self.output.clone(),
            self.output_dir.clone(),
            self.output_format.to_ext(),
        );

        Ok(ValidatedGenerate {
            prompt,
            model: self.model,
            image_refs: self.image_refs,
            output_paths,
            n: self.n,
            json: self.json,
            dry_run: self.dry_run,
            verbose: self.verbose,
            stream: self.stream,
        })
    }
}

/// Validated arguments ready for API call.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ValidatedGenerate {
    pub prompt: String,
    pub model: String,
    pub image_refs: Vec<String>,
    pub output_paths: Vec<PathBuf>,
    pub n: u8,
    pub json: bool,
    pub dry_run: bool,
    pub verbose: bool,
    pub stream: bool,
}

// ---------------------------------------------------------------------------
// ListModels subcommand
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Parser)]
pub struct ListModels {
    /// Output raw JSON instead of a formatted table.
    #[arg(long)]
    pub json: bool,
}

// ---------------------------------------------------------------------------
// Parse CLI
// ---------------------------------------------------------------------------

pub fn parse() -> Result<Cli, CliError> {
    use clap::error::ErrorKind;
    match <Cli as clap::Parser>::try_parse() {
        Ok(cli) => Ok(cli),
        Err(e) => {
            // --help and --version print to stdout and exit 0
            if matches!(e.kind(), ErrorKind::DisplayHelp | ErrorKind::DisplayVersion) {
                print!("{}", e);
                std::process::exit(0);
            }
            Err(CliError {
                message: e.to_string(),
                exit_code: 2,
            })
        }
    }
}
