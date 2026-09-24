# openrouter-image

[![release](https://img.shields.io/badge/release-v0.1.1-blue)](https://github.com/steimerbyte/openrouter-image-cli/releases/tag/v0.1.1)
[![license](https://img.shields.io/badge/license-MIT-green)](./LICENSE)

Standalone Rust CLI for generating images via OpenRouter. Agent-steerable: `--json` mode emits structured results on stdout and NDJSON progress on stderr, exit codes are stable, making it safe to drive from an LLM loop, CI pipeline, or shell script.

> Latest release: [v0.1.1](https://github.com/steimerbyte/openrouter-image-cli/releases/tag/v0.1.1). See [`CHANGELOG.md`](./CHANGELOG.md) for release notes, [`AUDIT-REPORT.md`](./AUDIT-REPORT.md) for the OpenRouter API conformance audit, and [`SECURITY-AUDIT.md`](./SECURITY-AUDIT.md) for the security audit findings.

## Features

- **`generate` subcommand** — `--prompt` + optional `--image-ref` (data URI or HTTP(S) URL), single file or multi-output
- **`list-models` subcommand** — live fetch from `https://openrouter.ai/api/v1/images/models`, filtered to image-capable providers
- **`endpoints` subcommand** — per-endpoint pricing, supported parameters, and passthrough details for a specific model
- **Stable exit codes** — `0` ok, `2` usage, `3` auth, `4` API, `5` IO
- **Config via env or TOML** — `OPENROUTER_API_KEY` env var or `~/.config/openrouter-image/config.toml`
- **Single retry on 5xx** with 2s backoff — no retry on 4xx
- **Structured output** — `--json` mode (NDJSON stderr + final result envelope stdout)
- **`--dry-run`** — print request body without calling API or writing files
- **`--stream`** — NDJSON progress events

## Quick start

```bash
# Install (from source)
cargo install --path .

# Configure API key
export OPENROUTER_API_KEY=sk-or-v1-...

# Generate one image (default: ~/generated-images/output.png)
openrouter-image generate --prompt "blue circle on white background"

# Override output file
openrouter-image generate --prompt "a red square" -o ./images/red.png

# Multiple images into a directory
openrouter-image generate --prompt "a friendly mascot" --n 4 --output-dir ./images
```

## Commands

```
openrouter-image generate     # Generate image(s)
openrouter-image list-models  # List image-capable models (live from OpenRouter)
openrouter-image endpoints <model-id>  # Per-endpoint details for a model
openrouter-image info        # Version + key diagnostic
openrouter-image schema     # Print JSON result schema
```

## `generate` subcommand

```
openrouter-image generate [OPTIONS]

Options:
  -p, --prompt <PROMPT>              Image generation prompt (required)
  -m, --model <MODEL>                Model slug (default: bytedance-seed/seedream-4.5)
      --image-ref <URI>              Data URI (data:...) or HTTP(S) URL reference image
                                     (repeatable, max 16)
  -o, --output <OUTPUT>             Single output file (default: ~/generated-images/output.png)
      --output-dir <OUTPUT_DIR>      Output directory for multiple images (n>1, default ~/generated-images/)
      --output-format <FORMAT>       png | jpeg | webp | svg (default: png)
      --resolution <RESOLUTION>      Resolution preset: 512 | 1K | 2K | 4K. Default: 2K.
      --aspect-ratio <RATIO>         Aspect ratio: 1:1, 1:2, 1:4, 1:8, 2:1, 2:3, 2.35:1,
                                     3:2, 3:4, 4:1, 4:3, 4:5, 5:2, 5:4, 8:1, 9:16,
                                     16:9, 9:19.5, 19.5:9, 9:20, 20:9, 9:21, 21:9, auto
      --background <MODE>            Background: auto | transparent | opaque.
                                     transparent requires --output-format png or webp.
      --output-compression <0-100>   Compression level (0–100) for jpeg/webp. Ignored for png/svg.
      --quality <QUALITY>             Quality hint: auto | low | medium | high | xhigh | max
      --seed <SEED>                  Integer seed for deterministic generation
      --size <SIZE>                  Explicit size: tier (512, 1K, 2K, 4K) or WxH pixels
                                     (e.g. 1024x1024). An explicit WxH overrides
                                     --resolution and --aspect-ratio.
      --user <USER>                  End-user identifier (max 256 chars, hashed upstream)
      --session-id <ID>              Session identifier (max 256 chars), sets X-Session-Id header
      --provider-only <PROVIDER>      Restrict to this provider (repeatable)
      --provider-ignore <PROVIDER>   Exclude this provider (repeatable)
      --provider-order <PROVIDER>     Preferred provider order (repeatable, first = highest priority)
      --trace-id <ID>                OpenTelemetry trace ID
      --trace-name <NAME>             OpenTelemetry trace name
      --span-name <NAME>             OpenTelemetry span name
  -n, --n <N>                        Number of images (1–10, default 1)
      --json                         Structured JSON on stdout, NDJSON progress on stderr
      --dry-run                      Print request body without calling API or writing files
  -v, --verbose                      Verbose tracing output (stderr)
      --stream                       Stream progress as NDJSON events (default when --json)
```

### Reference images

`--image-ref` accepts a data URI (`data:<media-type>;base64,<payload>`) or an HTTP(S) URL. Repeat the flag for multiple references (max 16):

```bash
openrouter-image generate \
  --prompt "a logo variation in red" \
  --image-ref "data:image/png;base64,iVBORw0KGgo..." \
  --image-ref "https://example.com/style-reference.png" \
  -o ./logo-red.png
```

### Provider routing

Use `openrouter-image endpoints <model>` to discover provider slugs, then restrict or order providers:

```bash
openrouter-image generate \
  --prompt "professional headshot" \
  --provider-only google \
  --provider-ignore anthropic
```

### Aspect ratio and size

An explicit `--size WxH` takes precedence over `--resolution` and `--aspect-ratio`.
The following produces a conflict error:

```bash
openrouter-image generate \
  --prompt "landscape photo" \
  --size 1920x1080 \
  --aspect-ratio 16:9   # ← rejected: WxH is authoritative
```

## `list-models` subcommand

Live fetch from `https://openrouter.ai/api/v1/images/models`, filtered to image-capable providers.

```bash
openrouter-image list-models           # human-readable table with ID / Resolution / Stream / Out
openrouter-image list-models --json  # raw JSON array of ModelEntry objects
```

The `Stream` column shows `yes` for models that advertise `supports_streaming`.

Use `openrouter-image endpoints <model-id>` to see per-endpoint pricing details.

## `endpoints` subcommand

Fetch per-endpoint details for a model: provider name, pricing, supported resolution count, passthrough parameter count.

```bash
# Human table
openrouter-image endpoints bytedance-seed/seedream-4.5

# Raw JSON
openrouter-image endpoints bytedance-seed/seedream-4.5 --json
```

Output columns:

| Column | Meaning |
|--------|---------|
| Provider | Provider display name |
| Pricing-Image | Image generation cost unit (provider-specific) |
| Resolutions | Count of supported resolution values |
| Passthrough | Number of provider-specific passthrough parameters |

## Agent tooling

### `--json` mode

```bash
openrouter-image generate --prompt "blue circle" --json --stream
```

- **stdout**: single JSON result envelope at the end
- **stderr**: NDJSON, one event per line (`progress`, `http_status`, `retry_attempt`, `images_saved`, `error`)

### Exit codes

| Code | Meaning |
|------|---------|
| `0`  | Success |
| `2`  | Usage error (missing/wrong argument, invalid value, conflict) |
| `3`  | Auth error (no key configured, 401/402 from API) |
| `4`  | API error (4xx non-auth, 5xx after retry) |
| `5`  | IO error (network, timeout, file write, dir creation) |

### `openrouter-image schema`

Prints the JSON Schema of the result envelope. Fetch once and parse to understand the output contract before driving generations from an agent loop.

## Configuration

### API key lookup (first hit wins)

1. `OPENROUTER_API_KEY` environment variable
2. `~/.config/openrouter-image/config.toml`

`config.toml` format:

```toml
api_key = "sk-or-v1-..."
default_model = "bytedance-seed/seedream-4.5"  # optional
```

```bash
mkdir -p ~/.config/openrouter-image
cp examples/config.toml ~/.config/openrouter-image/config.toml
chmod 600 ~/.config/openrouter-image/config.toml
# edit and replace the key
```

Get a key at <https://openrouter.ai/keys>.

### Output

- `n=1`: default `~/generated-images/output.png`, override with `-o <FILE>`
- `n>1`: default `~/generated-images/output-N.png`, override with `--output-dir <DIR>`
- Directory is created automatically (mkdir -p) on first run
- Output paths must canonicalise inside `cwd`, `$HOME`, or
  `$HOME/generated-images`; symlinks are not followed (defense in depth)

## Security

The CLI is designed to be safe to drive from agent loops and CI. Recent
hardening is documented in [`SECURITY-AUDIT.md`](./SECURITY-AUDIT.md);
key invariants:

- **API key is never in the request body.** It travels only in the
  `Authorization: Bearer` header. `--dry-run` prints the body without the key.
- **Config file must be `chmod 600`.** If `~/.config/openrouter-image/config.toml`
  is group- or world-readable, the CLI refuses to read it (exit code 3). Run
  `chmod 600 ~/.config/openrouter-image/config.toml` to fix.
- **Symlinks at the config path are rejected** — prevents an attacker on a
  shared host from pointing the config at a file they control.
- **SSRF guard on `--image-ref` HTTP(S) URLs.** Resolves the host and rejects
  private/loopback/link-local/cloud-metadata IPs (`10/8`, `172.16/12`,
  `192.168/16`, `169.254/16`, `127/8`, `0/8`, `fc00::/7`, `fe80::/10`,
  IPv4-mapped IPv6). Blocks `http://169.254.169.254/...` (AWS metadata) and
  `http://localhost/...`. Public URLs pass.
- **`OPENROUTER_BASE_URL` is validated.** Non-loopback hosts require `https://`
  and must not DNS-resolve to a private IP. Loopback + `http://` is allowed
  for wiremock-style test setups.
- **Output path traversal is rejected.** `-o` and `--output-dir` must stay
  inside `cwd`, `$HOME`, or `$HOME/generated-images`. Symlinks at the
  destination are not followed.

### Accepted findings (with rationale)

| Finding | Severity | Rationale |
|---|---|---|
| API key lives in process memory as `String` | LOW | No zeroize crate added; keep processes short-lived. Not exploitable without a memory dump. |
| Output files inherit umask | LOW | Defense-in-depth; trivial for the user to fix with `umask 077`. |
| Same-process concurrent runs may overwrite `output.png` | LOW | Rare in practice; auto-rename would break predictable output paths. |
| Dep CVEs (transitive `icu_*`) | LOW | Pinned to last rustc-1.85-compatible versions; will refresh when the CI matrix gains rustc 1.88+. |

## Documentation

| Document | Purpose |
|---|---|
| [`README.md`](./README.md) | This file — install, usage, command reference |
| [`CHANGELOG.md`](./CHANGELOG.md) | Release notes per version (Keep a Changelog format) |
| [`AUDIT-REPORT.md`](./AUDIT-REPORT.md) | Conformance audit against the OpenRouter Image API spec |
| [`SECURITY-AUDIT.md`](./SECURITY-AUDIT.md) | Security audit findings (12 categories, severity-graded) |
| [`docs/PLAN-archive.md`](./docs/PLAN-archive.md) | Historical planning notes (pre-release, archived) |
| [`examples/config.toml`](./examples/config.toml) | Example TOML configuration |
| GitHub release notes | Per-release binaries + checksums: <https://github.com/steimerbyte/openrouter-image-cli/releases> |

## Development

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test                          # unit + wiremock integration
cargo build --release
```

Tests use `wiremock` for OpenRouter fixture mocking — no network required.
A real `OPENROUTER_API_KEY` is only needed for manual smoke tests.

Smoke test with a real key:

```bash
OPENROUTER_API_KEY=sk-or-v1-... ./target/release/openrouter-image generate --prompt "red circle"
```

## Architecture

Two crates in one Cargo workspace:

- **`openrouter_image_core`** (library, `src/lib.rs`) — API client, config,
  validation, request/response types, progress events, path resolution.
  Re-exported so consumers can embed the library without the CLI.
- **`openrouter-image`** (binary, `src/main.rs`) — clap-driven CLI dispatch,
  NDJSON output, error → exit-code mapping.

The library is fully exercised by wiremock integration tests, so refactors
stay safe.

## License

MIT
