# openrouter-image

omp v18 extension that exposes OpenRouter's GPT Image models as a tool (`xd://openrouter_image`).

## Features

- **Models**: `gpt-image-2` (default), `gpt-image-1`, `gpt-image-1-mini`, `gpt-5-image`, `gpt-5-image-mini`, `gpt-5.4-image-2`
- **Aspect ratios**: `1:1`, `3:2`, `2:3`, `4:3`, `3:4` (default), `16:9`, `9:16`, `21:9`, `auto`
- **Reference image** support (local file → base64 → OpenRouter `input_references`)
- **Persistent API key** via `~/.omp/agent/image-gen.json` (chmod 600) — no env-var setup needed
- **Live counter** in TUI while generating (updates every second)
- **Async/non-blocking** execution — TUI stays responsive, prompt stays visible while image generates
- **Box-rendered output** with state-aware border color (muted → accent → success/error)
- **Full prompt visible** during generation (read while waiting)

## Installation

### 1. Drop into omp extensions dir

```bash
cp -r . ~/.omp/agent/extensions/openrouter-image-package/
```

Or install via npm (if published):

```bash
# coming soon
```

### 2. Configure API key

Copy the example file and fill in your key:

```bash
cp image-gen.example.json ~/.omp/agent/image-gen.json
# edit and replace sk-or-v1-REPLACE_ME with your key
chmod 600 ~/.omp/agent/image-gen.json
```

Get a key at <https://openrouter.ai/keys>.

Alternatively, set `OPENROUTER_API_KEY` as an env var — the file is only read if the env var is missing.

### 3. Verify

Trigger any tool call to xd://openrouter_image. If the key is valid, you get a PNG in `~/.omp/agent/generated-images/`.

## Usage

```json
{
  "prompt": "A minimal blue circle on white background",
  "model": "openai/gpt-image-2",
  "aspect_ratio": "1:1",
  "quality": "high",
  "reference_image": "/tmp/sketch.png",
  "n": 1
}
```

All fields except `prompt` are optional.

### Parameters

| Field | Type | Default | Notes |
|---|---|---|---|
| `prompt` | string | — | required |
| `model` | string | `openai/gpt-image-2` | see Models list above |
| `reference_image` | string (path) | — | local PNG/JPG/JPEG/GIF/WEBP, base64-encoded automatically |
| `aspect_ratio` | string | `16:9` | see list above |
| `quality` | string | `auto` | `auto`, `low`, `medium`, `high` |
| `background` | string | — | `auto`, `transparent`, `opaque` |
| `output_format` | string | `png` | `png`, `jpeg`, `webp`, `svg` |
| `resolution` | string | — | `512`, `1K`, `2K`, `4K` |
| `n` | number | `1` | 1–10 |
| `seed` | number | — | for reproducibility |

## Architecture

### Key resolution

Lookup order (first hit wins):

1. `process.env.OPENROUTER_API_KEY`
2. `~/.omp/agent/image-gen.json` fields: `apiKey`, `OPENROUTER_API_KEY`, `openrouter_api_key`

The file path is `IMAGE_GEN_SETTINGS_FILE` and is created from `homedir() + .omp/agent/image-gen.json`.

### Live updates

`executeOpenrouterImage` takes an `onUpdate` callback that fires every second while the API call is in-flight. Each tick sends a partial `AgentToolResult` with `liveStatus: "generating"` and `elapsedSeconds: N`. omp re-renders via `renderResult` on every update.

### Render states

`renderResult` switches between 4 states based on `liveStatus`:

| State | Border color | Content |
|---|---|---|
| `generating` / `queued` | accent | Header + ⏳ counter + full prompt |
| `done` | success | Header + stats + paths |
| `error` | error | Header + error message |
| (initial `renderCall`) | muted | Header + meta + full prompt |

Border glyphs come from `theme.boxRound` (rounded Unicode).

### Abort / cancel

The omp `signal` propagates through `executeOpenrouterImage` → `callImageApi` → the internal `AbortController`. Cancellation aborts the fetch immediately, no retry.

## Development

```bash
bunx tsc --noEmit --skipLibCheck \
  --target ES2022 --module NodeNext --moduleResolution NodeNext \
  --allowSyntheticDefaultImports --strict --esModuleInterop \
  --types node ./index.ts
```

The remaining baseline errors are `Text`-constructor mismatches with the `Component` interface — a known mismatch in `@oh-my-pi/pi-tui` types vs runtime that affects all extensions, not specific to this plugin.

## License

MIT