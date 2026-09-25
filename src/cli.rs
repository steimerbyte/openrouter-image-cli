//! CLI argument parsing via clap derive.

use std::path::PathBuf;

use clap::Parser;
use thiserror::Error;

use openrouter_image_core::OutputFormat;

// ---------------------------------------------------------------------------
// Validated field lists
// ---------------------------------------------------------------------------

/// All valid aspect ratio values (spec: 24 values + "auto").
#[allow(dead_code)]
const _ASPECT_RATIOS: &[&str] = &[
    "1:1", "1:2", "1:4", "1:8", "2:1", "2:3", "2.35:1", "3:2", "3:4", "4:1", "4:3", "4:5", "5:2",
    "5:4", "8:1", "9:16", "16:9", "9:19.5", "19.5:9", "9:20", "20:9", "9:21", "21:9", "auto",
];

/// All valid quality values.
#[allow(dead_code)]
const _QUALITY_VALUES: &[&str] = &["auto", "low", "medium", "high", "xhigh", "max"];

/// All valid background values.
#[allow(dead_code)]
const _BACKGROUND_VALUES: &[&str] = &["auto", "transparent", "opaque"];

/// Check if a size string is valid: tier ("512", "1K", "2K", "4K") or explicit WxH.
fn is_valid_size(s: &str) -> bool {
    if s == "512" || s == "1K" || s == "2K" || s == "4K" {
        return true;
    }
    let parts: Vec<&str> = s.split('x').collect();
    parts.len() == 2
        && parts.iter().all(|part| {
            let trimmed = part.trim();
            trimmed.len() >= 2 && trimmed.len() <= 5 && trimmed.chars().all(|c| c.is_ascii_digit())
        })
}

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
#[allow(clippy::large_enum_variant)]
pub enum Command {
    /// Generate one or more images.
    Generate(Generate),
    /// List all image-capable models (live from OpenRouter).
    ListModels(ListModels),
    /// Show per-endpoint details for a model (pricing, supported parameters, passthrough).
    Endpoints(Endpoints),
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

    /// Model slug (default: bytedance-seed/seedream-5-0-lite). Discover current
    /// options with `list-models`. Only models listed by the dedicated image
    /// endpoint (/api/v1/images/models) reliably support `resolution`.
    #[arg(
        short = 'm',
        long = "model",
        default_value = "bytedance-seed/seedream-5-0-lite"
    )]
    pub model: String,

    /// Base64-encoded data URI or HTTP(S) URL reference image (repeatable, max 16).
    #[arg(long = "image-ref", value_name = "URI")]
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

    /// Allow overwriting an existing output file. Default: false.
    #[arg(long)]
    pub clobber: bool,

    /// Number of retries on empty API response (0..=3). Default: 0.
    #[arg(long, default_value_t = 0u8, value_parser = clap::value_parser!(u8).range(0..=3))]
    pub max_image_retries: u8,

    /// Inline negative prompt — appended to --prompt with "Avoid: ..." prefix.
    /// The OpenRouter Image API has no structured negative-prompt field for all
    /// models; this is therefore merged into the prompt string before the API call.
    #[arg(long = "negative-prompt", value_name = "TEXT")]
    pub negative_prompt: Option<String>,

    /// Number of images to generate (1–10, default 1).
    #[arg(long, short = 'n', default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..=10))]
    pub n: u8,

    /// Output format for saved images.
    #[arg(long, value_enum, default_value = "png")]
    pub output_format: OutputFormat,

    /// Resolution preset for models that support it: 512 | 1K | 2K | 4K.
    /// Default: 2K (per OpenRouter docs, 2K is the API's mandated default).
    /// Use `list-models` to discover which values a given model actually honours.
    #[arg(long, value_parser = ["512", "1K", "2K", "4K"], default_value = "2K")]
    pub resolution: String,

    /// Aspect ratio for the generated image. Valid values: 1:1, 1:2, 1:4, 1:8, 2:1,
    /// 2:3, 2.35:1, 3:2, 3:4, 4:1, 4:3, 4:5, 5:2, 5:4, 8:1, 9:16, 16:9, 9:19.5,
    /// 19.5:9, 9:20, 20:9, 9:21, 21:9, auto.
    #[arg(long, value_parser = parse_aspect_ratio)]
    pub aspect_ratio: Option<String>,

    /// Background mode: auto, transparent, opaque.
    /// Note: transparent requires output-format png or webp.
    #[arg(long, value_parser = parse_background)]
    pub background: Option<String>,

    /// Output compression level (0–100) for jpeg/webp. Ignored for png/svg.
    #[arg(long, value_parser = clap::value_parser!(u8).range(0..=100))]
    pub output_compression: Option<u8>,

    /// Generation quality hint: auto, low, medium, high, xhigh, max.
    #[arg(long, value_parser = parse_quality)]
    pub quality: Option<String>,

    /// Seed for deterministic generation (integer). Repeat with the same seed
    /// and parameters to reproduce the same image.
    #[arg(long)]
    pub seed: Option<i64>,

    /// Size override: tier ("512", "1K", "2K", "4K") or explicit pixels ("WxH",
    /// e.g. "1024x1024"). An explicit WxH size is authoritative and rejects
    /// a mismatched --resolution or --aspect-ratio with HTTP 400.
    #[arg(long, value_parser = parse_size)]
    pub size: Option<String>,

    /// End-user identifier for tracking (max 256 chars, hashed upstream).
    #[arg(long)]
    pub user: Option<String>,

    /// Session identifier for request tracing (max 256 chars).
    /// Sent as X-Session-Id header and in the request body.
    #[arg(long)]
    pub session_id: Option<String>,

    /// Restrict generation to these providers (repeatable).
    /// Use `openrouter-image endpoints <model>` to discover provider slugs.
    #[arg(long)]
    pub provider_only: Vec<String>,

    /// Exclude these providers from generation (repeatable).
    #[arg(long)]
    pub provider_ignore: Vec<String>,

    /// Order providers by preference (repeatable, first has highest priority).
    #[arg(long)]
    pub provider_order: Vec<String>,

    /// OpenTelemetry trace ID for observability.
    #[arg(long)]
    pub trace_id: Option<String>,

    /// OpenTelemetry trace name for observability.
    #[arg(long)]
    pub trace_name: Option<String>,

    /// OpenTelemetry span name for observability.
    #[arg(long)]
    pub span_name: Option<String>,
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

        // Validate image refs: accept data URIs or HTTP(S) URLs
        for ref_arg in &self.image_refs {
            let is_data_uri = ref_arg.starts_with("data:");
            let is_http_url = ref_arg.starts_with("http://") || ref_arg.starts_with("https://");

            if !is_data_uri && !is_http_url {
                return Err(CliError::invalid_arg(format!(
                    "--image-ref: expected data URI (data:...) or HTTP(S) URL, got: {ref_arg}"
                )));
            }
        }

        // Cross-field: transparent background requires alpha-capable format
        if self.background.as_deref() == Some("transparent") {
            let fmt = self.output_format;
            if fmt != OutputFormat::Png && fmt != OutputFormat::Webp {
                return Err(CliError::invalid_arg(
                    "background=transparent requires --output-format png or webp",
                ));
            }
        }

        // Cross-field: explicit pixel size + resolution → conflict
        if let Some(ref size) = self.size {
            if is_valid_size(size) && !size.chars().any(|c| c == 'x') {
                // It's a tier (512/1K/2K/4K) — no conflict with resolution
            } else if self.resolution != "2K" || self.aspect_ratio.is_some() {
                // Explicit WxH + resolution/aspect_ratio → conflict
                return Err(CliError::invalid_arg(
                    "explicit --size (WxH) conflicts with --resolution or --aspect-ratio; \
                     an explicit pixel size is authoritative",
                ));
            }
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
            resolution: self.resolution,
            json: self.json,
            dry_run: self.dry_run,
            _verbose: self.verbose,
            output_format: Some(self.output_format),
            aspect_ratio: self.aspect_ratio,
            background: self.background,
            output_compression: self.output_compression,
            quality: self.quality,
            seed: self.seed,
            size: self.size,
            user: self.user,
            session_id: self.session_id,
            provider_only: self.provider_only,
            provider_ignore: self.provider_ignore,
            provider_order: self.provider_order,
            trace_id: self.trace_id,
            trace_name: self.trace_name,
            span_name: self.span_name,
            clobber: self.clobber,
            max_image_retries: self.max_image_retries,
            negative_prompt: self.negative_prompt.clone(),
        })
    }
}

/// Validated arguments ready for API call.
#[derive(Debug, Clone)]
pub struct ValidatedGenerate {
    pub prompt: String,
    pub model: String,
    pub image_refs: Vec<String>,
    pub output_paths: Vec<PathBuf>,
    pub n: u8,
    pub resolution: String,
    pub json: bool,
    pub dry_run: bool,
    pub _verbose: bool,
    // New fields
    pub output_format: Option<OutputFormat>,
    pub aspect_ratio: Option<String>,
    pub background: Option<String>,
    pub output_compression: Option<u8>,
    pub quality: Option<String>,
    pub seed: Option<i64>,
    pub size: Option<String>,
    pub user: Option<String>,
    pub session_id: Option<String>,
    pub provider_only: Vec<String>,
    pub provider_ignore: Vec<String>,
    pub provider_order: Vec<String>,
    pub trace_id: Option<String>,
    pub trace_name: Option<String>,
    pub span_name: Option<String>,
    pub clobber: bool,
    pub max_image_retries: u8,
    pub negative_prompt: Option<String>,
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
// Endpoints subcommand
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Parser)]
pub struct Endpoints {
    /// Model slug, e.g. bytedance-seed/seedream-4.5.
    pub model: String,

    /// Output raw JSON instead of formatted table.
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

// ---------------------------------------------------------------------------
// Shared value parsers (used by clap via value_parser!())
// ---------------------------------------------------------------------------

/// Parse an aspect ratio string against the allowed 24 values + "auto".
pub fn parse_aspect_ratio(s: &str) -> Result<String, String> {
    if _ASPECT_RATIOS.contains(&s) {
        Ok(s.to_string())
    } else {
        Err(format!(
            "invalid aspect ratio '{s}': valid values are {}",
            _ASPECT_RATIOS.join(", ")
        ))
    }
}

/// Parse a quality string against the allowed 6 values.
pub fn parse_quality(s: &str) -> Result<String, String> {
    if _QUALITY_VALUES.contains(&s) {
        Ok(s.to_string())
    } else {
        Err(format!(
            "invalid quality '{s}': valid values are {}",
            _QUALITY_VALUES.join(", ")
        ))
    }
}

/// Parse a background string against the allowed 3 values.
pub fn parse_background(s: &str) -> Result<String, String> {
    if _BACKGROUND_VALUES.contains(&s) {
        Ok(s.to_string())
    } else {
        Err(format!(
            "invalid background '{s}': valid values are {}",
            _BACKGROUND_VALUES.join(", ")
        ))
    }
}

/// Parse a size string: tier (512/1K/2K/4K) or explicit WxH.
pub fn parse_size(s: &str) -> Result<String, String> {
    if is_valid_size(s) {
        Ok(s.to_string())
    } else {
        Err(format!(
            "invalid size '{s}': expected a tier (512, 1K, 2K, 4K) or WxH pixels (e.g. 1024x1024)"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aspect_ratio_valid() {
        for ar in _ASPECT_RATIOS {
            assert_eq!(parse_aspect_ratio(ar).unwrap().as_str(), *ar);
        }
    }

    #[test]
    fn aspect_ratio_invalid() {
        assert!(parse_aspect_ratio("16:10").is_err());
        assert!(parse_aspect_ratio("").is_err());
    }

    #[test]
    fn quality_valid() {
        for q in _QUALITY_VALUES {
            assert_eq!(parse_quality(q).unwrap().as_str(), *q);
        }
    }

    #[test]
    fn quality_invalid() {
        assert!(parse_quality("ultra").is_err());
    }

    #[test]
    fn background_valid() {
        for b in _BACKGROUND_VALUES {
            assert_eq!(parse_background(b).unwrap().as_str(), *b);
        }
    }

    #[test]
    fn background_invalid() {
        assert!(parse_background("half-transparent").is_err());
    }

    #[test]
    fn size_valid_tiers() {
        for tier in ["512", "1K", "2K", "4K"] {
            assert_eq!(parse_size(tier).unwrap().as_str(), tier);
        }
    }

    #[test]
    fn size_valid_explicit() {
        assert_eq!(parse_size("800x600").unwrap().as_str(), "800x600");
        assert_eq!(parse_size("2048x2048").unwrap().as_str(), "2048x2048");
        assert_eq!(parse_size("99x99").unwrap().as_str(), "99x99");
        assert_eq!(parse_size("99999x99999").unwrap().as_str(), "99999x99999");
    }

    #[test]
    fn size_invalid() {
        assert!(parse_size("large").is_err());
        assert!(parse_size("1024").is_err());
        assert!(parse_size("x1024").is_err());
        assert!(parse_size("1024x").is_err());
        assert!(parse_size("1x2").is_err()); // too few digits
    }

    // -------------------------------------------------------------------------
    // New flag tests
    // -------------------------------------------------------------------------

    #[test]
    fn clobber_flag_parses_true() {
        let cli = Cli::try_parse_from([
            "openrouter-image",
            "generate",
            "--clobber",
            "--prompt",
            "a cat",
            "--output",
            "/tmp/test_clobber.png",
        ])
        .unwrap();
        let gen = match cli.command {
            Command::Generate(g) => g,
            _ => unreachable!(),
        };
        let validated = gen.validate().unwrap();
        assert!(validated.clobber);
    }

    #[test]
    fn clobber_default_false() {
        let cli = Cli::try_parse_from([
            "openrouter-image",
            "generate",
            "--prompt",
            "a cat",
            "--output",
            "/tmp/test_no_clobber.png",
        ])
        .unwrap();
        let gen = match cli.command {
            Command::Generate(g) => g,
            _ => unreachable!(),
        };
        let validated = gen.validate().unwrap();
        assert!(!validated.clobber);
    }

    #[test]
    fn negative_prompt_parses() {
        let cli = Cli::try_parse_from([
            "openrouter-image",
            "generate",
            "--prompt",
            "a cat",
            "--negative-prompt",
            "blurry, low quality",
            "--output",
            "/tmp/test_neg.png",
        ])
        .unwrap();
        let gen = match cli.command {
            Command::Generate(g) => g,
            _ => unreachable!(),
        };
        let validated = gen.validate().unwrap();
        assert_eq!(
            validated.negative_prompt,
            Some("blurry, low quality".to_string())
        );
    }

    #[test]
    fn negative_prompt_none_when_absent() {
        let cli = Cli::try_parse_from([
            "openrouter-image",
            "generate",
            "--prompt",
            "a cat",
            "--output",
            "/tmp/test_no_neg.png",
        ])
        .unwrap();
        let gen = match cli.command {
            Command::Generate(g) => g,
            _ => unreachable!(),
        };
        let validated = gen.validate().unwrap();
        assert_eq!(validated.negative_prompt, None);
    }

    #[test]
    fn max_image_retries_default_zero() {
        let cli = Cli::try_parse_from([
            "openrouter-image",
            "generate",
            "--prompt",
            "a cat",
            "--output",
            "/tmp/test_retry.png",
        ])
        .unwrap();
        let gen = match cli.command {
            Command::Generate(g) => g,
            _ => unreachable!(),
        };
        let validated = gen.validate().unwrap();
        assert_eq!(validated.max_image_retries, 0);
    }

    #[test]
    fn max_image_retries_range_ok() {
        for n in [0u8, 1, 2, 3] {
            let cli = Cli::try_parse_from([
                "openrouter-image",
                "generate",
                "--prompt",
                "a cat",
                "--max-image-retries",
                &n.to_string(),
                "--output",
                "/tmp/test_retry.png",
            ])
            .unwrap();
            let gen = match cli.command {
                Command::Generate(g) => g,
                _ => unreachable!(),
            };
            let validated = gen.validate().unwrap();
            assert_eq!(validated.max_image_retries, n, "value {n} should parse");
        }
        // 4 is out of range and must be rejected by clap
        let result = Cli::try_parse_from([
            "openrouter-image",
            "generate",
            "--prompt",
            "a cat",
            "--max-image-retries",
            "4",
            "--output",
            "/tmp/test_retry.png",
        ]);
        assert!(result.is_err(), "--max-image-retries 4 should be rejected");
    }
}
