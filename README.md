# openrouter-image

Standalone Rust CLI for generating images via OpenRouter. Agent-steerable: `--json` mode emits structured results on stdout and NDJSON progress on stderr, exit codes are stable, making it safe to drive from an LLM loop, CI pipeline, or shell script.

## Features

- **`generate` subcommand** — `--prompt` + optional `--image-ref` (base64 data URI), single file or multi-output
- **`list-models` subcommand** — live fetch from `https://openrouter.ai/api/v1/models`, filtered to image-capable providers
- **Stable exit codes** — `0` ok, `2` usage, `3` auth, `4` API, `5` IO
- **Config via env or TOML** — `OPENROUTER_API_KEY` env var or `~/.config/openrouter-image/config.toml`
- **Single retry on 5xx** with 2s backoff — no retry on 4xx
- **Structured output** — `--json` mode (NDJSON stderr + final result envelope stdout)
- **`--dry-run`** — print request body without calling API
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
openrouter-image info         # Version + key diagnostic
openrouter-image schema       # Print JSON result schema
```

## `generate` subcommand

```
openrouter-image generate [OPTIONS]

Options:
  -p, --prompt <PROMPT>                Image generation prompt (required)
  -m, --model <MODEL>                  Model slug (default: openai/gpt-5-image)
      --image-ref <BASE64_DATA_URI>    Base64-encoded data URI reference image (repeatable)
  -o, --output <OUTPUT>                Single output file (default: ~/generated-images/output.png)
      --output-dir <OUTPUT_DIR>        Output directory for multiple images (n>1, default ~/generated-images/)
      --output-format <OUTPUT_FORMAT>  png | jpeg | webp | svg (default: png)
      --resolution <RESOLUTION>        Resolution preset (512 | 1K | 2K | 4K). Pass-through to the
                                       API; only effective for models that list 'resolution' in
                                       supported_parameters. Use `list-models` to discover which
                                       models advertise it.
  -n, --n <N>                          Number of images (1–10, default 1)
      --json                           Structured JSON on stdout, NDJSON progress on stderr
      --dry-run                        Print request body without calling API or writing files
  -v, --verbose                        Verbose tracing output (stderr)
      --stream                         Stream progress as NDJSON events (default when --json)
```

### Reference images

`--image-ref` accepts a base64 data URI (`data:<media-type>;base64,<payload>`). Repeat the flag for multiple references:

```bash
openrouter-image generate \
  --prompt "a logo variation in red" \
  --image-ref "data:image/png;base64,iVBORw0KGgo..." \
  -o ./logo-red.png
```

File paths and HTTP(S) URLs are intentionally not accepted — convert to a data URI first if needed (e.g. `base64 -w0 sketch.png`).

## `list-models` subcommand

Live fetch from `https://openrouter.ai/api/v1/models`, filtered to image-generation-capable providers. A model is considered image-capable when its `architecture.output_modalities` contains `"image"` — this is sourced directly from the API, not from name matching.

The `Res` column shows whether the model advertises `resolution` (or `image_size` / `*_resolution`) in `supported_parameters`. Models without that entry accept `--resolution` as a pass-through but ignore it.

```bash
openrouter-image list-models           # human-readable table with Context / Res / Out columns
openrouter-image list-models --json    # raw JSON array of ModelEntry objects
```

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
| `2`  | Usage error (missing/wrong argument, invalid data URI) |
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
default_model = "openai/gpt-5-image"  # optional
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

## Development

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test                          # unit + wiremock integration (~70 tests)
cargo build --release
```

Tests use `wiremock` for OpenRouter fixture mocking — no network required.

Smoke test with a real key:

```bash
OPENROUTER_API_KEY=sk-or-v1-... ./target/release/openrouter-image generate --prompt "red circle"
```

## License

MIT
