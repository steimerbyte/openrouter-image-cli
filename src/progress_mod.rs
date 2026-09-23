//! Progress event types emitted during a generation run.

use serde::Serialize;

/// Events emitted on stderr during a generation run.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum ProgressEvent {
    /// Elapsed time tick.
    Progress { elapsed_ms: u64 },
    /// HTTP response status received.
    HttpStatus { status: u16 },
    /// Retry attempt starting.
    RetryAttempt { attempt: u32, delay_ms: u64 },
    /// API error returned by OpenRouter.
    ApiError { status: u16, message: String },
    /// Final error message.
    Error { error: String },
    /// Number of images received.
    ImagesReceived { count: usize },
    /// Images saved to disk.
    ImagesSaved { paths: Vec<String> },
}

impl ProgressEvent {
    /// Serialize to JSON for NDJSON stderr output.
    pub fn to_ndjson(&self) -> String {
        serde_json::to_string(self)
            .unwrap_or_else(|e| format!(r#"{{"event":"serialization_error","message":"{}"}}"#, e))
    }
}

/// Emit a progress event to stderr, formatted according to the output mode.
pub fn emit_progress_event(event: ProgressEvent, mode: crate::output::OutputMode) {
    match mode {
        crate::output::OutputMode::Json => {
            eprintln!("{}", event.to_ndjson());
        }
        crate::output::OutputMode::Human => {
            // Human progress handled by indicatif in main.rs / run_with_progress
        }
        crate::output::OutputMode::Quiet => {}
    }
}
