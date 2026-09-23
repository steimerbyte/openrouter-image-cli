//! Output formatting — human-readable and JSON modes.

pub mod human;
pub mod json;

use crate::GenerationResult;

/// Controls which output layer handles progress and result rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputMode {
    /// ANSI terminal output with progress bars.
    Human,
    /// Structured JSON on stdout, NDJSON events on stderr.
    #[default]
    Json,
    /// No progress output.
    Quiet,
}

/// Emit final result in human-readable format.
#[allow(dead_code)]
pub fn emit_human_final(result: &GenerationResult) {
    human::final_result(result);
}

/// Emit final result as JSON.
#[allow(dead_code)]
pub fn emit_json_final(result: &GenerationResult) {
    json::final_result(result);
}
