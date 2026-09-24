# Plan: openrouter-image Rust CLI (finale Specs)

## Status

- **Commit `485b251`**: Initial-Rework fertig nach altem Plan (XDG-Dir, JSON-Config, hartkodierte Modelle, file-based refs, Unit-Tests). Gates 1–7 grün, aber NICHT nach User-Final-Specs.
- **JETZT**: Anpassung an finale User-Specs (siehe Goal-Block unten).

## Goal-Block (verbindlich, aus User-Goal)

Erweitere den bestehenden Rust-CLI für OpenRouter auf vollständigen Feature-Parity, sodass

- `cargo build --release` ohne Warnings
- `cargo clippy -- -D warnings`
- `cargo test` mit gemockten OpenRouter-Fixtures

grün durchlaufen und die Binärdatei mindestens diese Befehle abdeckt:

```
generate --prompt <TEXT> [--model <ID>] [--image-ref <BASE64-DATA-URI>]...
         [-o <FILE>|--output-dir <DIR> für mehrere Outputs, Default ./output.png]
         [--json] [--dry-run] [--verbose] [--stream]

list-models   (live von https://openrouter.ai/api/v1/models, gefiltert auf Image-Provider)
```

mit Env-`OPENROUTER_API_KEY` oder `~/.config/openrouter-image/config.toml`, korrekte Exit-Codes (0 ok, 2 Usage, 3 Auth, 4 API, 5 IO), und vollständige Unit- plus Integrationstests (reqwest mit `wiremock`).

**Out-of-Scope**: keine TUI, keine Async-Retry-Strategien jenseits einmaligem Backoff, kein Provider-Plugin-System, keine Video-Generation.

## Änderungen gegenüber aktuellem Stand (Commit 485b251)

| Bereich | Aktuell (485b251) | Ziel (Final-Spec) |
|---|---|---|
| Output-Default | `~/generated_images/` (Directory) | `./output.png` (Single-File) |
| Output-Override | `--output-dir <DIR>` | `-o <FILE>` UND `--output-dir <DIR>` |
| Image-Refs | file / data-URL / http | NUR `--image-ref <BASE64-DATA-URI>` |
| list-models | hartkodiert (6 GPT-Image) | LIVE von `/api/v1/models`, Filter auf Image-Provider |
| Config | `~/.omp/agent/image-gen.json` | `~/.config/openrouter-image/config.toml` |
| CLI-Flags | `--json`, `--quiet`, `--output-format`, `--resolution`, `--background`, `--seed` | `--json`, `--dry-run`, `--verbose`, `--stream` (kein `--quiet` mehr; **kein** `--output-format`, `--resolution`, `--background`, `--seed` — weg! Spec sagt nicht's dazu) |
| Exit-Codes | 0/1/2/3/4/5/6/130 | **0/2/3/4/5** (nur diese 5, kein 1, kein 6, kein 130) |
| Tests | nur Unit | Unit + Integration (wiremock) |
| Retry-Strategie | 2× bei 5xx, exp backoff | nur 1× Backoff (Goal: "jenseits einmaligem Backoff" = out-of-scope) |

## Was du NICHT änderst

- Crate-Struktur (Library `openrouter_image_core` + Binary `openrouter-image` im selben Cargo.toml) — bleibt
- Public-API der Library darf sich anpassen, kein Compat-Shim nötig
- `--no-retry`-Flag: kann bleiben als implizit aktiv (Default ist jetzt 1× Backoff)
- `--prompt-file -` (stdin) kann offen bleiben, aber Priorität niedrig

## Konkrete Refactor-Tasks

### 1. CLI-Surface (`src/cli.rs`)

Replace `Gen` mit `Generate` und kürze die Flags:

```rust
#[derive(Args, Debug)]
pub struct Generate {
    /// Image generation prompt (required)
    #[arg(short, long)]
    pub prompt: String,

    /// Model slug (default: openai/gpt-image-2)
    #[arg(short, long, default_value = "openai/gpt-image-2")]
    pub model: String,

    /// Base64-encoded data URI reference image (repeatable)
    #[arg(long = "image-ref", value_name = "BASE64_DATA_URI")]
    pub image_refs: Vec<String>,

    /// Single output file (default: ./output.png)
    #[arg(short = 'o', long = "output")]
    pub output: Option<PathBuf>,

    /// Output directory for multiple images (n>1)
    #[arg(long = "output-dir")]
    pub output_dir: Option<PathBuf>,

    /// Structured JSON on stdout, NDJSON progress on stderr
    #[arg(long)]
    pub json: bool,

    /// Print request without calling API
    #[arg(long)]
    pub dry_run: bool,

    /// Verbose tracing output (stderr)
    #[arg(long, short)]
    pub verbose: bool,

    /// Stream progress as NDJSON (default if --json)
    #[arg(long)]
    pub stream: bool,

    /// Number of images (1-10, default 1)
    #[arg(long, default_value_t = 1, value_parser = clap::value_parser!(u8).range(1..=10))]
    pub n: u8,
}
```

`list-models` ersetzt `models`. Subcommand bleibt.

`info` und `schema` können bleiben oder weg — Goal sagt nichts, ich sage: **behalten** (harmlos, agent-steerbar).

### 2. Default-Output + Multi-Output-Logik (`src/cli.rs` + `src/client_mod.rs`)

```rust
fn resolve_output_path(args: &Generate, image_idx: usize) -> PathBuf {
    if args.n == 1 {
        // Single image: -o wins, else ./output.png
        args.output.clone().unwrap_or_else(|| PathBuf::from("./output.png"))
    } else {
        // Multiple: --output-dir wins, else ./output-{idx}.png next to ./output.png
        let dir = args.output_dir.clone().unwrap_or_else(|| PathBuf::from("."));
        dir.join(format!("output-{}.png", image_idx + 1))
    }
}
```

### 3. Image-Refs nur Data-URIs (`src/reference_mod.rs`)

Ersetze file/http/data-URL-generic-Logik mit strikter Data-URI-Validierung:

```rust
pub fn validate_data_uri(s: &str) -> Result<&str, ReferenceError> {
    if !s.starts_with("data:") {
        return Err(ReferenceError::NotADataUri(s.to_string()));
    }
    // Format: data:<media-type>;base64,<payload>
    let after = &s[5..];
    let semi = after.find(';').ok_or_else(|| ReferenceError::Malformed(s.to_string()))?;
    let media = &after[..semi];
    let rest = &after[semi + 1..];
    if !rest.starts_with("base64,") {
        return Err(ReferenceError::NotBase64(s.to_string()));
    }
    Ok(s)
}
```

File-Resolution und HTTP-URL-Handling entfallen komplett.

### 4. Config: TOML statt JSON (`src/config_mod.rs`)

```toml
# ~/.config/openrouter-image/config.toml
api_key = "sk-or-..."
default_model = "openai/gpt-image-2"
```

Use `toml` crate (siehe Dependencies). Resolution:

```rust
pub fn load_config() -> Result<Config, ConfigError> {
    // 1. $OPENROUTER_API_KEY (env wins)
    if let Ok(key) = std::env::var("OPENROUTER_API_KEY") {
        return Ok(Config { api_key: key, default_model: None });
    }
    // 2. ~/.config/openrouter-image/config.toml (XDG-konform via dirs::config_dir())
    let path = dirs::config_dir()
        .ok_or(ConfigError::NoConfigDir)?
        .join("openrouter-image")
        .join("config.toml");
    if !path.exists() { return Err(ConfigError::NoKey); }
    let text = std::fs::read_to_string(&path)?;
    let cfg: TomlConfig = toml::from_str(&text)?;
    Ok(cfg.into())
}
```

### 5. list-models: live von OpenRouter (`src/list_models.rs`, NEU)

```rust
pub async fn fetch_image_models(client: &reqwest::Client) -> Result<Vec<Model>, ApiError> {
    let resp: ModelsResponse = client
        .get("https://openrouter.ai/api/v1/models")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(resp.data.into_iter()
        .filter(|m| m.id.contains("image") || m.id.contains("dall") || m.id.contains("flux") || m.id.contains("sd"))
        .collect())
}
```

Filter-Heuristik auf Image-Modelle: ID enthält eines von `image`, `dall`, `flux`, `sd-xl`, `imagen`, `gemini.*image`, `gpt-image`, `gpt-5.*image`. Liste im Help-Text dokumentieren.

### 6. Exit-Codes (`src/error_mod.rs` + `src/main.rs`)

Nur 5 Codes:
- `0` Ok
- `2` Usage (clap error, invalid args, schema violation, missing prompt)
- `3` Auth (no key, invalid key, 401/402 from API)
- `4` API (4xx non-auth, 5xx after retry)
- `5` IO (output dir creation, file write, network unreachable, timeout)

`main.rs` mappt `Error → ExitCode` zentral.

### 7. Retry-Strategie (`src/client_mod.rs`)

Auf **1 Backoff** reduzieren:

```rust
async fn call_with_backoff(req: RequestBuilder) -> Result<Response, ApiError> {
    match req.send().await {
        Ok(r) if r.status().is_server_error() => {
            tokio::time::sleep(Duration::from_secs(2)).await;
            req.send().await.map_err(Into::into)
        }
        other => other.map_err(Into::into),
    }
}
```

### 8. Tests: wiremock-Integration (`tests/`)

Cargo-Dependency: `wiremock = "0.6"` — falls 0.6.5 inkompatibel mit Rust 1.85 (wie Worker-Report sagt), versuche `0.5` oder `0.6.4`. Wenn keiner geht, dokumentiere im Body, aber schreibe die Tests trotzdem — wiremock ist Goal-Pflicht.

Test-Cases (alle als Integration-Tests in `tests/`):
- `client_200_single.rs`: Mock returns 200 + b64 PNG → assert file written, exit 0
- `client_401.rs`: Mock returns 401 → assert exit 3, stderr contains "API key"
- `client_402.rs`: 402 → exit 3, "credits"
- `client_429.rs`: 429 → exit 4 (kein retry bei 429 in neuer Spec)
- `client_500_with_backoff.rs`: Mock returns 500 then 200 → assert retry happened, exit 0
- `client_500_persistent.rs`: Mock returns 500 twice → exit 4
- `client_timeout.rs`: Mock delays 130s → exit 5 (Timeout)
- `list_models_filter.rs`: Mock `/models` with mixed IDs → assert only image-models in result
- `config_toml.rs`: Tempdir + TOML file → assert key resolved
- `config_env_wins.rs`: env-var set + TOML present → env wins
- `data_uri_validate.rs`: valid + invalid (not data:, missing base64,) → assert error
- `output_resolution.rs`: n=1, n=3 → assert correct paths
- `dry_run.rs`: `--dry-run` → no API call, no file write, request-body printed

### 9. README + Examples

- README neu schreiben mit Goal-Specs, Beispielen für `--json`, `--dry-run`, `--stream`, list-models-Output
- `examples/image-gen.example.json` umbenennen oder löschen (Spec nutzt TOML jetzt)
- Neue `examples/config.toml` mit auskommentierten Defaults

### 10. Cargo.toml-Deps ergänzen

```toml
[dependencies]
# ...existing...
toml = "0.8"
home = "0.5"  # XDG home statt dirs (konsistenter)

[dev-dependencies]
# ...existing...
wiremock = "0.6"  # oder 0.6.4 wenn 0.6.5 inkompatibel
mockito = "1"     # fallback, falls wiremock scheitert
```

## Verifikations-Gates (final, MUSS alle grün)

1. `cargo fmt --check` clean
2. `cargo clippy --all-targets -- -D warnings` clean
3. `cargo test` alle grün (Unit + wiremock-Integration)
4. `cargo build --release` erfolgreich
5. `./target/release/openrouter-image --help` zeigt neue Generate-Subcommand-Syntax
6. `./target/release/openrouter-image generate --help` zeigt neue Flags
7. `./target/release/openrouter-image generate --prompt "test" --dry-run` exit 2 (kein Key) oder exit 0 mit printed body (wenn key gesetzt), keine File geschrieben
8. `./target/release/openrouter-image list-models` ohne Key: exit 3, "API key not configured"
9. `./target/release/openrouter-image info` exit 0, zeigt Version + Config-Pfad
10. Mit gesetztem `OPENROUTER_API_KEY`: `./openrouter-image generate --prompt "blue circle" -o /tmp/test.png` exit 0, File existiert

## Git-Workflow

- Arbeite auf `main` im Working-Dir `/home/pi/workspace/openrouter-image/`
- Wenn alle Gates grün: squash die Refactor-Änderungen in einen neuen Commit auf `485b251`
- Commit-Message:
  ```
  refactor: align CLI with final spec — data-URI refs, TOML config, live models, wiremock tests
  ```
- Body: listet jede Major-Änderung + Gate-Status
- KEIN Push

## Out-of-Scope (NICHT anfassen)

- TUI / interactive prompts
- Multi-retry / adaptive backoff (nur 1 Backoff erlaubt)
- Provider-Plugin-System
- Video-Generation
- OpenAI/Google native APIs (nur OpenRouter)

## Working-Directory

`/home/pi/workspace/openrouter-image/` — keine externen Pfade.

## Abschluss-Report

Kurzer Bericht:
- Welche der 10 Gates grün
- Binary-Pfad + Größe
- Git-Status + Commit-Hash
- Liste von `wiremock`-Versions-Fallbacks falls nötig
- Beispiel-Aufruf: `./openrouter-image generate --prompt "blue circle" --json --stream` → erste 3 NDJSON-Zeilen + finale stdout-Zeile
