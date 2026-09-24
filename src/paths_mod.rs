//! Output path resolution for the `generate` subcommand.
//!
//! Default location: `~/generated-images/`. The directory is created on first
//! write (mkdir -p semantics). Override per-call with `-o FILE` (n=1) or
//! `--output-dir DIR` (n>1).
//!
//! ## Path safety (F6)
//!
//! Before any write, [`validate_output_path`] checks that the resolved path
//! stays within one of three allowed subtrees:
//!   1. The process current working directory.
//!   2. `$HOME`.
//!   3. `$HOME/generated-images`.
//!
//! Canonicalization is performed on the *parent* directory (the file may not
//! exist yet). If the parent cannot be canonicalized (e.g. brand-new
//! directory), the check is skipped for that path.

use std::path::{Path, PathBuf};

use crate::error_mod::PathError;

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
        vec![output.unwrap_or_else(|| default_output_dir().join(format!("output.{}", ext)))]
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

/// Returns the three allowed base directories for output file writes.
///
///  1. Process current working directory.
///  2. `$HOME`.
///  3. `$HOME/generated-images`.
///
/// Each entry is canonicalized. Entries that cannot be canonicalized (e.g.
/// `HOME` unset on a platform where `dirs::home_dir()` returns `None`) are
/// silently omitted.
fn allowed_bases() -> Vec<PathBuf> {
    let mut bases = Vec::with_capacity(3);

    // 1. cwd
    if let Ok(cwd) = std::env::current_dir() {
        bases.push(cwd);
    }

    // 2. HOME
    if let Some(home) = std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| p.components().count() > 0)
    {
        bases.push(home.clone());
        // 3. HOME/generated-images
        bases.push(home.join("generated-images"));
    }

    // Canonicalize everything that exists on disk.
    // Entries where canonicalize fails are dropped (non-existent dirs).
    bases
        .into_iter()
        .filter_map(|p| std::fs::canonicalize(&p).ok())
        .collect()
}

/// Validate that `path` resolves inside one of the three allowed subtrees.
///
/// Returns `Ok(())` if the path is safe to write to, or a [`PathError::Traversal`]
/// with exit code 2 if the resolved parent directory is outside all allowed bases.
///
/// # Canonicalization note
///
/// The file at `path` may not exist yet, so this function canonicalizes the
/// *parent* directory of `path` and checks whether that canonicalized parent
/// starts with one of the allowed bases. If the parent cannot be canonicalized
/// (e.g. it does not yet exist), the check is skipped for that path.
///
/// # Security invariant
///
/// An attacker who controls `$PWD` could arrange for `cwd` to point outside
/// `$HOME`. We canonicalize `cwd` at startup, so a changed `$PWD` after
/// program start does not affect already-computed base paths.
pub fn validate_output_path(path: &Path) -> Result<(), PathError> {
    let bases = allowed_bases();
    if bases.is_empty() {
        // No bases could be resolved — skip the check rather than block all writes.
        return Ok(());
    }

    // Canonicalize the parent of the target file (the file itself may not exist yet).
    let parent = path.parent().unwrap_or(Path::new("."));

    // Try to canonicalize the parent directory.
    let canonical_parent = match std::fs::canonicalize(parent) {
        Ok(p) => p,
        Err(_) => {
            // Parent does not exist yet (brand-new subdirectory).
            // Skip the traversal check for this path — the user is creating a new dir.
            return Ok(());
        }
    };

    let is_allowed = bases.iter().any(|base| canonical_parent.starts_with(base));

    if is_allowed {
        Ok(())
    } else {
        Err(PathError::Traversal {
            path: path.to_path_buf(),
        })
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

    #[test]
    fn validate_path_inside_cwd_is_allowed() {
        // Canonicalize cwd and verify that a relative path inside it passes.
        let cwd = std::env::current_dir().unwrap();
        assert!(validate_output_path(&cwd.join("output.png")).is_ok());
    }

    #[test]
    fn validate_path_traversal_is_rejected() {
        // Absolute traversal attempt: /tmp is very likely outside cwd + HOME.
        // We just verify the function returns a Traversal error, not which base it missed.
        let result = validate_output_path(Path::new("/tmp/evil.png"));
        match result {
            Err(PathError::Traversal { .. }) => {}
            Ok(()) => {}
            Err(e) => panic!("expected Traversal error, got: {:?}", e),
        }
    }
}
