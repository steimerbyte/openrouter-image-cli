# openrouter-image

Standalone Rust CLI for generating images via OpenRouter's GPT Image models. Agent-steerable: `--json` mode emits structured results on stdout and NDJSON progress events on stderr, making it easy to drive from an LLM loop or CI pipeline.

## Features

- **6 hardcoded models** — no auto-discovery, stable API
- **Reference image support** — local file, data URL, or HTTPS URL
- **Structured output** — `--json` mode for agent tooling (NDJSON stderr + result envelope stdout)
- **Stable exit codes** — `0` ok, `2` bad args, `3` no/invalid key, `4` API error, `5` timeout, `6` I/O, `130` SIGINT
- **XDG-free default** — saves to `~/generated_images/` (override with `--output-dir`)
- **Retry on 5xx** — 2× with exponential backoff (`--no-retry` to disable)

## Quick start

```bash
# Install (from source)
cargo install --path .

# Configure API key (env var takes precedence)
export OPENROUTER_API_KEY=sk-or-v1-…

# Generate one image
openrouter-image gen --prompt "blue circle on white background"

# Override output dir
openrouter-image gen --prompt "a red square" --output-dir ./images
```

## Commands

```
openrouter-image gen        # Generate image(s)
openrouter-image models     # List supported models
openrouter-image info       # Version + key diagnostic
openrouter-image schema     # Print JSON result schema
```

## `gen` subcommand

```
openrouter-image gen [OPTIONS]
  -p, --prompt <TEXT>             Prompt text (required)
      --prompt-file <PATH|-">     Read prompt from file or stdin (-)
  -m, --model <SLUG>              Model slug (default: openai/gpt-image-2)
      --reference <REF>           Local file, data: URL, or HTTPS URL
      --aspect-ratio <RATIO>      1:1 | 3:2 | 2:3 | 4:3 | 3:4 | 16:9 | 9:16 | 21:9 | auto (default: 16:9)
      --quality <Q>                auto | low | medium | high
      --background <MODE>         auto | transparent | opaque
      --output-format <FMT>       png | jpeg | webp | svg (default: png)
      --resolution <RES>          512 | 1K | 2K | 4K
  -n, --n <1-10>                  Number of images (default: 1)
      --seed <U64>                Random seed for reproducibility
      --output-dir <DIR>          Output directory (default: ~/generated_images/)
      --timeout-ms <N>            Timeout in ms (default: 120000)
      --no-retry                  Disable automatic retry on 5xx
  -j, --json                      Structured JSON on stdout, NDJSON progress on stderr
  -q, --quiet                     Suppress all progress output
```

## Models

| Model | Notes |
|---|---|
| `openai/gpt-image-2` | **Default.** 9 aspect ratios |
| `openai/gpt-image-1` | 4 ratios, transparent background |
| `openai/gpt-image-1-mini` | Cost-optimized |
| `openai/gpt-5-image` | Reasoning + image |
| `openai/gpt-5-image-mini` | Reasoning + image, cost-optimized |
| `openai/gpt-5.4-image-2` | Latest generation |

## Agent tooling

### `--json` mode

```
# stdout: one JSON object at the end
# stderr: NDJSON, one line per event
```

```bash
openrouter-image gen --prompt "blue circle" --json 2>&1
```

**stderr (NDJSON progress):**
```json
{"event":"progress","elapsed_ms":1000}
{"event":"progress","elapsed_ms":2000}
{"event":"progress","elapsed_ms":3000}
{"event":"http_status","status":200}
{"event":"images_received","count":1}
{"event":"images_saved","paths":["/home/user/generated_images/openrouter-1234567890-1.png"]}
```

**stdout (final result):**
```json
{
  "schema_version": "1.0",
  "status": "ok",
  "images": [
    {
      "path": "/home/user/generated_images/openrouter-1234567890-1.png",
      "media_type": "image/png",
      "b64_length": 48291
    }
  ],
  "usage": {
    "prompt_tokens": 8,
    "completion_tokens": 3,
    "total_tokens": 11,
    "cost": 0.000120
  },
  "model": "openai/gpt-image-2",
  "n": 1,
  "elapsed_ms": 4210,
  "output_dir": "/home/user/generated_images",
  "warnings": []
}
```

### `openrouter-image schema`

Prints the JSON Schema for the result envelope. Agents can fetch and parse it to understand the output format before running a generation.

### Exit codes

| Code | Meaning |
|---|---|
| `0` | Success |
| `1` | Unknown error |
| `2` | Invalid arguments / schema violation |
| `3` | No API key / key invalid (401/402) |
| `4` | OpenRouter API error (4xx non-auth) |
| `5` | Timeout |
| `6` | I/O error (output-dir) |
| `130` | Interrupted (SIGINT/SIGTERM) |

## Configuration

### API key lookup (first hit wins)

1. `OPENROUTER_API_KEY` env var
2. `~/.omp/agent/image-gen.json` fields: `apiKey`, `OPENROUTER_API_KEY`, `openrouter_api_key`

```bash
# Option A: env var (recommended for CI/agents)
export OPENROUTER_API_KEY=sk-or-v1-…

# Option B: config file
mkdir -p ~/.omp/agent
cp examples/image-gen.example.json ~/.omp/agent/image-gen.json
chmod 600 ~/.omp/agent/image-gen.json
# edit and replace sk-or-v1-REPLACE_ME with your key
```

Get a key at <https://openrouter.ai/keys>.

### Output directory

Default: `~/generated_images/`. Override with `--output-dir`. Files are named `openrouter-<timestamp>-<N>.<ext>`.

## Reference images

Pass a reference image as visual context:

```bash
# Local file (PNG/JPG/GIF/WEBP/SVG — MIME detected from extension)
openrouter-image gen --prompt "a logo based on this sketch" --reference ./sketch.png

# Data URL (already base64-encoded)
openrouter-image gen --prompt "variation of this" --reference "data:image/png;base64,..."

# HTTPS URL
openrouter-image gen --prompt "style transfer" --reference "https://example.com/style.jpg"
```

## Development

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

Smoke tests (requires `OPENROUTER_API_KEY`):
```bash
OPENROUTER_API_KEY=sk-or-v1-… cargo run -- gen --prompt "red circle"
```

## License

MIT
