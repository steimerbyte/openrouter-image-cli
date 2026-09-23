# Plan: openrouter-image → Rust CLI (agent-steerbar)

## Ziel

Komplettes Rework von `openrouter-image` (TS omp-Plugin, 530 LOC, Commit `6e7fde9`) in einen standalone, agent-steuerbaren Rust-CLI. Funktional äquivalent zur TS-Version, aber: kein omp-Wrapper, sauberes JSON-Interface, stabile Exit-Codes, NDJSON-Progress auf stderr — damit ein AI-Agent (z.B. omp-Subagent oder beliebiger LLM-Loop) das Tool direkt aus einem Tool-Call ansprechen kann.

## Repo-Struktur (Cargo)

```
openrouter-image/
├── Cargo.toml
├── Cargo.lock
├── README.md
├── LICENSE                       # MIT (wie vorher)
├── .gitignore                    # + target/, *.swp
├── examples/
│   └── image-gen.example.json
├── src/
│   ├── lib.rs                    # öffentliche API
│   ├── main.rs                   # CLI-Binary
│   ├── cli.rs                    # clap derive
│   ├── client.rs                 # reqwest-Client, OpenRouter /images Call
│   ├── config.rs                 # Key-Resolution (env + image-gen.json)
│   ├── models.rs                 # Params + Response Typen (serde)
│   ├── reference.rs              # file/data-URL/HTTPS → data:image/...;base64,...
│   ├── error.rs                  # thiserror + ExitCode Mapping
│   ├── progress.rs               # Progress-Event Enum
│   └── output/
│       ├── mod.rs
│       ├── human.rs              # ANSI-Fortschritt + Ergebnis-Box
│       └── json.rs               # strukturiertes Result + NDJSON-Stream
└── tests/
    ├── config_test.rs            # key-resolution (env / file / fehlt)
    ├── reference_test.rs         # file → data-URL
    └── client_mock.rs            # wiremock: 200, 401, 429, 500-retry, timeout, abort
```

## Crate-Layout

- **Single crate**, nicht workspace — Scope ist überschaubar, eine Cargo-Datei reicht
- Bin: `openrouter-image`
- Library `openrouter-image-core` als zweiter `lib`-Crate innerhalb desselben `Cargo.toml` (`[lib]` + `[[bin]]`), damit Tests + zukünftige omp-Adapter die gleiche Logik nutzen können

## CLI-Surface

```
openrouter-image gen
  --prompt <TEXT>              (required, oder via stdin mit --prompt-file -)
  --prompt-file <PATH|->
  --model <SLUG>               (default: openai/gpt-image-2)
  --reference <PATH|DATA-URL|HTTPS-URL>
  --aspect-ratio <RATIO>       (default: 16:9)
  --quality <auto|low|medium|high>
  --background <auto|transparent|opaque>
  --output-format <png|jpeg|webp|svg>
  --resolution <512|1K|2K|4K>
  --n <1-10>
  --seed <U64>
  --output-dir <DIR>           (default: ~/.omp/agent/generated-images/ — TS-Parität)
  --json                       (structured JSON auf stdout, NDJSON-Progress auf stderr)
  --quiet                      (kein Progress, nur Result)
  --timeout-ms <N>             (default: 120000)
  --no-retry                   (TS macht 2× retry bei 5xx; default: an)

openrouter-image models        # 6 bekannte + ggf. Auto-Discovery
openrouter-image info          # Version + Key-Diagnostic (masked)
openrouter-image schema        # JSON-Schema für Result-Format (agent-steerbar)
```

## Agent-Steerbarkeit (Kernforderung)

1. **`--json` Mode:**
   - stdout: **ein** JSON-Objekt am Ende (kein Streaming) mit `schema_version`, `status`, `images[]`, `usage`, `model`, `elapsed_ms`, `output_dir`, `warnings[]`
   - stderr: **NDJSON**, eine Zeile pro Event (`{"event":"progress","elapsed_ms":1000}`, `{"event":"http_status","status":401}`, …)
   - Schema-Version fest verdrahtet, jeder Breaking-Change → `schema_version` bump
2. **Exit-Codes (stabil, dokumentiert):**
   - `0` ok
   - `1` unbekannter Fehler
   - `2` ungültige Args / Schema-Verletzung
   - `3` kein API-Key / Key invalid
   - `4` OpenRouter-API-Fehler (4xx non-401/402/429)
   - `5` Timeout
   - `6` I/O-Fehler (output-dir)
   - `130` SIGINT/SIGTERM
3. **`--quiet`** unterdrückt Progress komplett (für Agent-Loops die nur Result wollen)
4. **`openrouter-image schema`** gibt das JSON-Schema des Result-Formats aus → Agent kann's vorab parsen

## Funktionale Parität zur TS-Version

| Feature | TS | Rust | Notes |
|---|---|---|---|
| Modelle | 6 hartkodiert | 6 hartkodiert + optional `/models`-Discovery | siehe Frage unten |
| Aspect-Ratios | 9 + Validation | enum + Validation | identisch |
| Reference-Image | file/data-URL/https → base64 | gleich | mit MIME-Detect per ext |
| Output-Format | png/jpeg/webp/svg | gleich | |
| Retry | 2× bei 5xx, exp backoff | gleich | per `--no-retry` abschaltbar |
| Timeout | 120s | gleich, konfigurierbar | |
| Abort via Signal | ja | ja (`tokio::signal::ctrl_c`) | |
| Key-Resolution | env → `image-gen.json` | gleich | Pfad bleibt `~/.omp/agent/image-gen.json` |
| Live-Progress | 1s Tick, AgentToolUpdateCallback | tokio-Interval → Callback → stderr-NDJSON / human-ANSI | |
| Output-Dir | `~/.omp/agent/generated-images/` | `~/generated_images` (per `--output-dir` override) | User-Entscheidung |

## Dependencies

```toml
[dependencies]
reqwest        = { version = "0.12", features = ["json", "stream", "rustls-tls"] }
tokio          = { version = "1", features = ["macros", "rt-multi-thread", "signal", "fs", "io-util", "time"] }
serde          = { version = "1", features = ["derive"] }
serde_json     = "1"
clap           = { version = "4", features = ["derive"] }
thiserror      = "1"
anyhow         = "1"
base64         = "0.22"
indicatif      = "0.17"            # human-mode progress bar
tracing        = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

[dev-dependencies]
wiremock       = "0.6"
tempfile       = "3"
pretty_assertions = "1"
```

## Verifikation (Gates)

1. `cargo fmt --check`
2. `cargo clippy --all-targets -- -D warnings`
3. `cargo test` — Unit + wiremock-Integration (success, 401, 402, 429, 500-retry, timeout, abort, reference-file/data-url, config-resolution)
4. `cargo build --release` — Binary-Größe + LTO
5. Smoke-Test mit echtem Key (falls `OPENROUTER_API_KEY` gesetzt) — generate one image, prüfen ob File in `~/generated_images/` landet

## Entscheidungen (vom User bestätigt)

- **omp-Plugin: DROP** — reines CLI, keine Plugin-Logik in der Cargo-Crate
- **Output-Dir: `~/generated_images`** (per `--output-dir` überschreibbar)
- **Modelle: hartkodiert** (6 GPT-Image-Modelle), keine Auto-Discovery

## Ausführungs-Reihenfolge

1. `cargo init` + Cargo.toml + Skeleton
2. `models.rs`, `error.rs`, `config.rs`, `reference.rs` (Pure-Funktionen, einfache Tests)
3. `client.rs` + `progress.rs` (mit wiremock-Tests)
4. `cli.rs` + `main.rs` (clap derive)
5. `output/human.rs` + `output/json.rs`
6. Integration-Tests
7. `cargo fmt` + `clippy` + `test`
8. README neu schreiben (CLI-Fokus, Agent-Beispiele)
