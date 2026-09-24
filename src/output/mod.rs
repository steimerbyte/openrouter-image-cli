//! Output formatting — human-readable and JSON modes.

pub mod human;
pub mod json;

/// Controls which output layer handles progress and result rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputMode {
    /// ANSI terminal output with progress bars.
    Human,
    /// Structured JSON on stdout, NDJSON events on stderr.
    #[default]
    Json,
}
