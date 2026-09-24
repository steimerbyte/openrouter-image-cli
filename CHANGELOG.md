# Changelog

All notable changes to `openrouter-image-cli` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.1] — 2026-09-24

### Security

- **F1 (HIGH) — Config file permission check**: refuses to read
  `~/.config/openrouter-image/config.toml` if it is group- or world-readable
  (`mode & 0o077 != 0`). Run `chmod 600 <file>` to fix.
- **F2 (HIGH) — Config file symlink check**: refuses to follow symlinks at the
  config path. Prevents an attacker on a shared host from pointing the config
  at a file they control.
- **F6 (MED) — Output path validation**: `-o` and `--output-dir` paths must
  canonicalize inside `cwd`, `$HOME`, or `$HOME/generated-images`. Path traversal
  (`-o ../../etc/...`) is rejected.
- **F7 (MED) — Output symlink refusal**: refuses to write through a symlink at
  the destination, preventing redirect-write attacks.
- **F10/F11 (MED) — SSRF guard on `--image-ref` HTTP(S) URLs**: validates the
  resolved IP against private/loopback/link-local/cloud-metadata ranges
  (`10/8`, `172.16/12`, `192.168/16`, `169.254/16`, `127/8`, `0/8`,
  `fc00::/7`, `fe80::/10`, IPv4-mapped IPv6 inner). Blocks
  `http://169.254.169.254/...` (AWS metadata), `http://127.0.0.1`, etc.
- **F13 (MED) — `OPENROUTER_BASE_URL` hardening**: requires `https://` for
  non-loopback hosts; DNS-resolves and rejects private IPs. Loopback +
  `http://` allowed for wiremock-style test setups.

### Documentation

- New [`SECURITY-AUDIT.md`](./SECURITY-AUDIT.md): 12-category static security
  audit, 16 findings total (2 HIGH, 6 MED, 5 LOW, 3 INFO), 9 fixed in this
  release.
- `README.md`: new **Security** section summarising the audit and the accepted
  findings (key in memory, umask inheritance, no-clobber, dep CVEs).

### Notes

- The audit subagent (`security-reviewer`) and the audit + fix workflow
  (`sec-skill`) are reusable artefacts stored under `~/.pi/agent/` for future
  audits.
- No breaking CLI changes; all new behaviour is opt-out via env vars or hard
  defaults that match the safer choice.

## [0.1.0] — 2026-09-24

### Added

- **`generate` subcommand** — full OpenRouter Image API request body support:
  - `--prompt`, `--model` (default `bytedance-seed/seedream-4.5`)
  - `--resolution` with default `2K` (spec-mandated)
  - `--aspect-ratio` (24 normalized values + `auto`)
  - `--background` (`auto` / `transparent` / `opaque`, with format-compat validation)
  - `--output-format` (`png` / `jpeg` / `webp` / `svg`, default `png`)
  - `--output-compression` (0–100, jpeg/webp only)
  - `--quality` (`auto` / `low` / `medium` / `high` / `xhigh` / `max`)
  - `--seed` for deterministic generation
  - `--size` (tier `512`/`1K`/`2K`/`4K` or explicit `WxH` pixels; WxH is authoritative)
  - `--user` (max 256 chars, hashed upstream)
  - `--session-id` (max 256 chars, also sets `X-Session-Id` header)
  - Provider routing: `--provider-only`, `--provider-ignore`, `--provider-order`
  - OpenTelemetry metadata: `--trace-id`, `--trace-name`, `--span-name`
  - `--image-ref` (repeatable, max 16) — accepts data URIs **and** HTTP(S) URLs
  - `--stream` — SSE partial-image events (when the model supports streaming)
  - `--dry-run` — print request body, skip the HTTP call
  - `--json` — structured result envelope on stdout, NDJSON progress on stderr
- **`list-models` subcommand** — live fetch from `/api/v1/images/models`, with
  resolution enum values and streaming-support flags per model
- **`endpoints <model-id>` subcommand** — per-provider records: pricing,
  supported parameters, passthrough count
- **`info` subcommand** — version, masked key status, config path
- **`schema` subcommand** — JSON Schema for the result envelope (for agent tooling)
- **Default output**: `~/generated-images/output.png` (mkdir -p on first run)
- **Config**: `OPENROUTER_API_KEY` env var or `~/.config/openrouter-image/config.toml` (chmod 600)

### Changed

- **Full rewrite** from the original TypeScript omp-extension (the predecessor at
  `openrouter-image`) to standalone Rust CLI with library `openrouter_image_core`
- **Default model**: hardcoded `openai/gpt-image-2` → live-discovered via
  `/api/v1/images/models`, defaulting to `bytedance-seed/seedream-4.5`
- **Default resolution**: explicit `2K` (was unset, fell back to provider default)
- **Default output**: `./output.png` → `~/generated-images/output.png`
- **Config format**: `~/.omp/agent/image-gen.json` → `~/.config/openrouter-image/config.toml`

### Removed

- omp-plugin wrapping (pure standalone CLI)
- File-path and HTTPS-URL-only `--reference` flag (replaced by `--image-ref`
  accepting data URIs and HTTP(S) URLs uniformly)
- Out-of-spec retry strategies (now: exactly one 2s backoff on 5xx, no retry on 4xx)
- `gen` subcommand alias (now `generate` everywhere)

### Fixed

- **HTTP-Referer header casing** now uses hyphen (was underscore)
- **input_references now accepts HTTP(S) URLs** (was: rejected)
- **response_format / output_format** now actually sent to the API (was: parsed
  locally but never included in request body)
- **Model filter is API-driven** (`architecture.output_modalities` contains
  `"image"`) instead of a hardcoded keyword list

### Quality

- 18 test-targets, ~100 tests, all green (`cargo test`)
- 11+ wiremock integration tests covering 200/401/402/500-with-retry/list-models/data-URI/HTTP-URL/config-resolution/dry-run/output-resolution
- `cargo fmt --check` clean
- `cargo clippy --all-targets -- -D warnings` clean
- `cargo build --release` clean, no warnings, 4.9 MB binary
- Full API conformance audit: see [`AUDIT-REPORT.md`](./AUDIT-REPORT.md)

### Notes

- The previous repository (`steimerbyte/openrouter-image`, archived) contained the
  TypeScript omp-plugin that this CLI replaces. The old git history is preserved
  as commits `6e7fde9..485b251` in the *previous* repository; the new repo
  (`steimerbyte/openrouter-image-cli`) starts at `485b251`.
- Implementation plan drafts are archived at [`docs/PLAN-archive.md`](./docs/PLAN-archive.md).
