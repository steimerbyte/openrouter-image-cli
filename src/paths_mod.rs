//! Output path resolution for the `generate` subcommand.
//!
//! Default location: `~/generated-images/`. The directory is created on first
//! write (mkdir -p semantics). Override per-call with `-o FILE` (n=1) or
//! `--output-dir DIR` (n>1).

use std::path::PathBuf;

/// Returns the default output directory: `$HOME/generated-images`.
///
/// Falls back to `./generated-images` if `HOME` is unset (e.g. Windows env,
/// unusual sandboxes).
pub fn default_output_dir() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("generated-images")
}

/// Resolve the output path(s) for a `generate` invocation.
///
/// - `n == 1`: returns a single path. If `output` is set, use it directly;
///   otherwise fall back to `<default_output_dir>/output.<ext>`.
/// - `n > 1`: returns `n` paths. If `output_dir` is set, use `<dir>/output-N.<ext>`;
///   otherwise fall back to `<default_output_dir>/output-N.<ext>`.
pub fn resolve_output_paths(
    n: u8,
    output: Option<PathBuf>,
    output_dir: Option<PathBuf>,
    ext: &str,
) -> Vec<PathBuf> {
    if n == 1 {
        vec![output.unwrap_or_else(|| {
            default_output_dir().join(format!("output.{}", ext))
        })]
    } else if let Some(dir) = output_dir {
        (1..=u32::from(n))
            .map(|i| dir.join(format!("output-{}.{}", i, ext)))
            .collect()
    } else {
        let dir = default_output_dir();
        (1..=u32::from(n))
            .map(|i| dir.join(format!("output-{}.{}", i, ext)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_uses_home_env() {
        // Just sanity-check that the function returns *something* — depends on env.
        let p = default_output_dir();
        assert!(p.ends_with("generated-images"));
    }
}
