//! Entry point for the `openrouter-image` CLI binary.
//!
//! Exit codes:
//!   0  Ok
//!   2  Usage   — clap error, invalid args, missing prompt
//!   3  Auth    — no key, invalid key, 401/402 from API
//!   4  API     — 4xx non-auth, 5xx after retry exhausted
//!   5  IO      — network unreachable, timeout, file write, dir creation

mod cli;

use std::process::ExitCode;

use anyhow::Context as _;
use cli::Cli;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

// ---------------------------------------------------------------------------
// Logging setup
// ---------------------------------------------------------------------------

fn init_logging(verbose: bool) {
    let filter = if verbose {
        EnvFilter::new("debug")
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
    };

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().without_time().compact())
        .init();
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

fn main() -> ExitCode {
    let cli = match cli::parse() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}", e);
            return ExitCode::from(e.exit_code());
        }
    };

    let verbose = match &cli.command {
        cli::Command::Generate(g) => g.verbose,
        _ => false,
    };
    init_logging(verbose);

    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(_) => return ExitCode::from(5),
    };

    let exit_code: u8 = rt.block_on(async { run_async(&cli).await });

    match exit_code {
        0 => ExitCode::SUCCESS,
        code => ExitCode::from(code),
    }
}

async fn run_async(cli: &Cli) -> u8 {
    match run(cli).await {
        Ok(code) => code,
        Err(err) => {
            if let Some(ce) = err.downcast_ref::<cli::CliError>() {
                eprintln!("{}", err);
                return ce.exit_code();
            }

            // Emit error in JSON mode
            if matches!(&cli.command, cli::Command::Generate(g) if g.json) {
                eprintln!(
                    "{}",
                    serde_json::json!({
                        "event": "error",
                        "error": err.to_string()
                    })
                );
            } else {
                tracing::error!("{}", err);
            }

            // Map error to exit code
            if err.downcast_ref::<std::io::Error>().is_some() {
                return 5;
            }
            if let Some(api_err) = err.downcast_ref::<openrouter_image_core::ApiError>() {
                return api_err.exit_code();
            }
            if let Some(cfg_err) = err.downcast_ref::<openrouter_image_core::ConfigError>() {
                return cfg_err.exit_code();
            }

            // Unknown error → 4 (API)
            4
        }
    }
}

async fn run(cli: &Cli) -> anyhow::Result<u8> {
    match &cli.command {
        cli::Command::Info => {
            info_command()?;
            Ok(0)
        }
        cli::Command::ListModels(lm) => list_models_command(lm).await,
        cli::Command::Endpoints(ep) => endpoints_command(ep).await,
        cli::Command::Schema => {
            schema_command()?;
            Ok(0)
        }
        cli::Command::Generate(gen) => generate_command(gen.clone()).await,
    }
}

// ---------------------------------------------------------------------------
// Info
// ---------------------------------------------------------------------------

fn info_command() -> anyhow::Result<()> {
    let config = openrouter_image_core::Config::resolve().map_err(|e| anyhow::anyhow!(e))?;

    let masked = config.masked_key();
    let has_key = config.api_key().is_some();
    let key_status = if has_key {
        format!("configured ({})", masked)
    } else {
        "not configured".to_string()
    };
    let config_path = config.config_file_path();
    let config_exists = config.config_file_exists;

    openrouter_image_core::emit_human_info(&key_status, &masked, &config_path, config_exists);

    if !has_key {
        return Err(cli::CliError::no_api_key().into());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// ListModels
// ---------------------------------------------------------------------------

async fn list_models_command(lm: &cli::ListModels) -> anyhow::Result<u8> {
    use openrouter_image_core::{fetch_image_models, Config};

    let config = Config::resolve().map_err(|e| anyhow::anyhow!(e))?;

    let api_key = config
        .api_key()
        .ok_or_else(|| anyhow::anyhow!("{}", cli::CliError::no_api_key()))?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .context("failed to build HTTP client")?;

    let image_models = fetch_image_models(&client, api_key, None)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    if lm.json {
        openrouter_image_core::emit_json_models(&image_models);
    } else {
        openrouter_image_core::emit_human_models(&image_models);
    }

    Ok(0)
}

// ---------------------------------------------------------------------------
// Endpoints
// ---------------------------------------------------------------------------

/// Resolve API key from config, exit 3 if missing.
fn resolve_api_key() -> anyhow::Result<String> {
    use openrouter_image_core::Config;

    let config = Config::resolve().map_err(|e| anyhow::anyhow!(e))?;
    config
        .api_key()
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("{}", cli::CliError::no_api_key()))
}

/// Resolve base URL override from OPENROUTER_BASE_URL env, if set.
fn resolve_base_url() -> Option<String> {
    std::env::var("OPENROUTER_BASE_URL").ok()
}

async fn endpoints_command(ep: &cli::Endpoints) -> anyhow::Result<u8> {
    use openrouter_image_core::fetch_model_endpoints;

    let api_key = resolve_api_key()?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .context("failed to build HTTP client")?;

    let endpoints =
        fetch_model_endpoints(&client, &api_key, &ep.model, resolve_base_url().as_deref())
            .await
            .map_err(|e| anyhow::anyhow!(e))?;

    if ep.json {
        println!("{}", serde_json::to_string_pretty(&endpoints).unwrap());
    } else {
        print_endpoints_table(&ep.model, &endpoints);
    }

    Ok(0)
}

fn print_endpoints_table(model_id: &str, endpoints: &[openrouter_image_core::EndpointRecord]) {
    println!(
        "openrouter-image v{}  endpoints for {}",
        env!("CARGO_PKG_VERSION"),
        model_id
    );
    println!();

    if endpoints.is_empty() {
        println!("No endpoint data returned.");
        return;
    }

    println!(
        "{:30} {:>15} {:>12} {:>12}",
        "Provider", "Pricing-Image", "Resolutions", "Passthrough"
    );
    println!("{}", "-".repeat(74));

    for ep in endpoints {
        let pricing_image = extract_pricing_image(&ep.pricing);
        let resolutions = extract_supported_resolutions(ep);
        let passthrough_count = ep.passthrough.as_ref().map(|p| p.len()).unwrap_or(0);

        println!(
            "{:30} {:>15} {:>12} {:>12}",
            truncate(&ep.provider_name, 30),
            pricing_image,
            resolutions,
            passthrough_count
        );
    }

    println!();
    println!("Total: {} provider(s)", endpoints.len());
    println!();
    println!("Pricing-Image = image generation cost (provider-specific unit).");
    println!("Resolutions   = count of supported resolution values from supported_parameters.");
    println!("Passthrough   = number of provider-specific passthrough parameters.");
    println!();
    println!("Use --json for raw JSON output with full pricing details.");
}

fn extract_pricing_image(pricing: &serde_json::Value) -> String {
    // Pricing shape: { "image": { "units": "..." } } or similar
    pricing
        .get("image")
        .and_then(|v| v.get("units"))
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| "—".to_string())
}

fn extract_supported_resolutions(ep: &openrouter_image_core::EndpointRecord) -> String {
    ep.supported_parameters
        .get("resolution")
        .and_then(|p| p.enum_values())
        .map(|vals| vals.len().to_string())
        .unwrap_or_else(|| "—".to_string())
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}…", &s[..max_len - 1])
    }
}

// ---------------------------------------------------------------------------
// Schema
// ---------------------------------------------------------------------------

fn schema_command() -> anyhow::Result<()> {
    let schema = openrouter_image_core::schema();
    println!("{}", serde_json::to_string_pretty(&schema)?);
    Ok(())
}

// ---------------------------------------------------------------------------
// Generate
// ---------------------------------------------------------------------------

async fn generate_command(gen: cli::Generate) -> anyhow::Result<u8> {
    use openrouter_image_core::{Config, GenerationParams, OutputMode};

    let validated = gen.validate().map_err(|e| anyhow::anyhow!(e))?;

    // Build provider routing object if any flags are set
    let provider = build_provider(&validated);
    // Build trace object if any trace flags are set
    let trace = build_trace(&validated);

    // Dry run: print request body and exit 0
    if validated.dry_run {
        let params = GenerationParams {
            prompt: validated.prompt.clone(),
            model: validated.model.clone(),
            image_refs: validated.image_refs.clone(),
            output_paths: validated.output_paths.clone(),
            n: validated.n,
            resolution: Some(validated.resolution.clone()),
            timeout_ms: 120_000,
            // New fields
            aspect_ratio: validated.aspect_ratio.clone(),
            background: validated.background.clone(),
            output_format: validated.output_format.map(|f| f.as_api_str().to_string()),
            output_compression: validated.output_compression,
            quality: validated.quality.clone(),
            seed: validated.seed,
            size: validated.size.clone(),
            user: validated.user.clone(),
            session_id: validated.session_id.clone(),
            provider,
            trace,
            stream: validated.stream,
        };
        let body = params.to_request_body();
        eprintln!("[dry-run] Request body:");
        println!("{}", serde_json::to_string_pretty(&body).unwrap());
        eprintln!("[dry-run] No API call made, no files written.");
        return Ok(0);
    }

    let config = Config::resolve().map_err(|e| anyhow::anyhow!(e))?;

    if config.api_key().is_none() {
        return Err(cli::CliError::no_api_key().into());
    }

    let params = GenerationParams {
        prompt: validated.prompt,
        model: validated.model,
        image_refs: validated.image_refs,
        output_paths: validated.output_paths,
        n: validated.n,
        resolution: Some(validated.resolution),
        timeout_ms: 120_000,
        // New fields
        aspect_ratio: validated.aspect_ratio,
        background: validated.background,
        output_format: validated.output_format.map(|f| f.as_api_str().to_string()),
        output_compression: validated.output_compression,
        quality: validated.quality,
        seed: validated.seed,
        size: validated.size,
        user: validated.user,
        session_id: validated.session_id,
        provider,
        trace,
        stream: validated.stream,
    };

    let output_mode = if validated.json {
        OutputMode::Json
    } else {
        OutputMode::Human
    };

    let _result = openrouter_image_core::run_with_progress(params, config, output_mode).await?;
    Ok(0)
}

/// Build the provider routing object from CLI flags.
fn build_provider(v: &cli::ValidatedGenerate) -> Option<openrouter_image_core::ProviderRouting> {
    let has_any = !v.provider_only.is_empty()
        || !v.provider_ignore.is_empty()
        || !v.provider_order.is_empty();

    if !has_any {
        return None;
    }

    Some(openrouter_image_core::ProviderRouting {
        allow_fallbacks: None,
        only: if v.provider_only.is_empty() {
            None
        } else {
            Some(v.provider_only.clone())
        },
        ignore: if v.provider_ignore.is_empty() {
            None
        } else {
            Some(v.provider_ignore.clone())
        },
        order: if v.provider_order.is_empty() {
            None
        } else {
            Some(v.provider_order.clone())
        },
        sort: None,
    })
}

/// Build the trace object from CLI flags.
fn build_trace(v: &cli::ValidatedGenerate) -> Option<openrouter_image_core::TraceMetadata> {
    let has_any = v.trace_id.is_some() || v.trace_name.is_some() || v.span_name.is_some();

    if !has_any {
        return None;
    }

    Some(openrouter_image_core::TraceMetadata {
        trace_id: v.trace_id.clone(),
        trace_name: v.trace_name.clone(),
        span_name: v.span_name.clone(),
        generation_name: None,
        parent_span_id: None,
        extra: std::collections::HashMap::new(),
    })
}
