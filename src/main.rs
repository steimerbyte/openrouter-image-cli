//! Entry point for the `openrouter-image` CLI binary.

mod cli;

use std::process::ExitCode;

use cli::Cli;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

fn init_logging(quiet: bool) {
    let filter = if quiet {
        EnvFilter::new("error")
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
    };

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().without_time().compact())
        .init();
}

fn main() -> ExitCode {
    let cli = match cli::parse() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}", e);
            return ExitCode::from(e.exit_code());
        }
    };

    init_logging(cli.quiet);

    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(_) => return ExitCode::from(1u8),
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
            if cli.json {
                let event = serde_json::json!({
                    "event": "error",
                    "error": err.to_string()
                });
                eprintln!("{}", event);
            } else {
                tracing::error!("{}", err);
            }
            match err.downcast_ref::<cli::CliError>() {
                Some(ce) => ce.exit_code(),
                None => {
                    if err.downcast_ref::<std::io::Error>().is_some() {
                        6
                    } else if err.to_string().contains("timeout") {
                        5
                    } else {
                        1
                    }
                }
            }
        }
    }
}

async fn run(cli: &Cli) -> anyhow::Result<u8> {
    match &cli.command {
        cli::Command::Info => {
            info_command(cli)?;
            Ok(0)
        }
        cli::Command::Models => {
            models_command(cli);
            Ok(0)
        }
        cli::Command::Schema => {
            schema_command()?;
            Ok(0)
        }
        cli::Command::Gen(gen) => gen_command(cli, gen.clone()).await,
    }
}

fn info_command(cli: &Cli) -> anyhow::Result<()> {
    use openrouter_image_core::Config;

    let config = Config::resolve()?;
    let masked = config.masked_key();
    let has_key = config.api_key().is_some();
    let key_status = if has_key {
        format!("configured ({})", masked)
    } else {
        "not configured".to_string()
    };
    let config_path = config.config_file_path();
    let config_exists = config_path.exists();

    if cli.json {
        openrouter_image_core::emit_json_info(&key_status, &masked, &config_path, config_exists)?;
    } else {
        openrouter_image_core::emit_human_info(&key_status, &masked, &config_path, config_exists);
    }

    if !has_key {
        return Err(cli::CliError::no_api_key().into());
    }
    Ok(())
}

fn models_command(cli: &Cli) {
    use openrouter_image_core::ALL_MODELS;
    if cli.json {
        openrouter_image_core::emit_json_models(ALL_MODELS);
    } else {
        openrouter_image_core::emit_human_models(ALL_MODELS);
    }
}

fn schema_command() -> anyhow::Result<()> {
    let schema = openrouter_image_core::schema();
    println!("{}", serde_json::to_string_pretty(&schema)?);
    Ok(())
}

async fn gen_command(_cli: &Cli, gen: cli::GenArgs) -> anyhow::Result<u8> {
    use openrouter_image_core::{Config, OutputMode};

    let config = Config::resolve()?;

    let Some(_key) = config.api_key() else {
        return Err(cli::CliError::no_api_key().into());
    };

    let params = gen.into_params()?;

    let output_mode = if _cli.quiet {
        OutputMode::Quiet
    } else if _cli.json {
        OutputMode::Json
    } else {
        OutputMode::Human
    };

    let _result = openrouter_image_core::run_with_progress(params, config, output_mode).await?;
    Ok(0)
}
