//! Human-readable ANSI output.

use std::path::Path;

use crate::GenerationResult;

/// Print `openrouter-image info` output.
pub fn info(key_status: &str, masked_key: &str, config_path: &Path, config_exists: bool) {
    println!("openrouter-image v{}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("API key: {}", key_status);
    if masked_key != "—" {
        println!("  masked:  {}", masked_key);
    }
    println!();
    println!("Config file: {}", config_path.display());
    if config_exists {
        println!("  exists:  yes");
    } else {
        println!("  exists:  no — set OPENROUTER_API_KEY env var instead");
    }
    println!();
    println!("Default output: ./output.png  (single) / ./output-N.png (multiple)");
    println!("  override with: -o FILE or --output-dir DIR");
    println!();
    println!("Config lookup: OPENROUTER_API_KEY env > ~/.config/openrouter-image/config.toml");
}

/// Print the final result after a successful generation.
pub fn final_result(result: &GenerationResult) {
    println!();
    println!(
        "✓ Generated {} image(s) with {}",
        result.saved_paths.len(),
        result.model
    );
    for path in &result.saved_paths {
        println!("   {}", path.display());
    }
    println!("   media_type: {}", result.media_type);
    println!("   b64 length: {} chars", result.b64_len);
    if let Some(usage) = &result.usage {
        let parts: Vec<String> = [
            usage.prompt_tokens.map(|t| format!("prompt={}", t)),
            usage.completion_tokens.map(|t| format!("completion={}", t)),
            usage.total_tokens.map(|t| format!("total={}", t)),
            usage.cost.map(|c| format!("cost=${:.6}", c)),
        ]
        .into_iter()
        .flatten()
        .collect();
        if !parts.is_empty() {
            println!("   usage: {}", parts.join(", "));
        }
    }
    if !result.warnings.is_empty() {
        println!();
        for w in &result.warnings {
            println!("⚠  {}", w);
        }
    }
}
