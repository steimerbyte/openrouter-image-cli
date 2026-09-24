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
    };

    let output_mode = if validated.json {
        OutputMode::Json
    } else {
        OutputMode::Human
    };

    let _result = openrouter_image_core::run_with_progress(params, config, output_mode).await?;
    Ok(0)
}
