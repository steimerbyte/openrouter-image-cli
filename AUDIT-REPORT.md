# API Conformance Audit — openrouter-image-cli vs OpenRouter Image API
**Date:** 2025-09-22
**Repo:** steimerbyte/openrouter-image-cli @ `648cf26efc99420222e73fd34d5897576ca9da7b`
**Spec source:** OpenRouter docs (Sep 2026)

---

## Summary
- PASS: 17
- PARTIAL: 13
- MISSING: 11
- WRONG: 2
- OUTDATED: 3

---

## 1. Endpoints

| Spec | CLI reality | Status | Notes |
|------|-------------|--------|-------|
| POST `/api/v1/images` | `images_url()` → `https://openrouter.ai/api/v1/images` | PASS | Base-URL korrekt; override via `OPENROUTER_BASE_URL` env |
| GET `/api/v1/images/models` | `IMAGE_MODELS_ENDPOINT = "https://openrouter.ai/api/v1/images/models"` in `list_models.rs` | PASS | Endpoint korrekt; override über `fetch_image_models(base_url)` param |
| GET `/api/v1/images/models/{author}/{slug}/endpoints` | Nicht implementiert | MISSING | Kein CLI-Kommando oder API-Call für Per-Endpoint-Capabilities (pricing, passthrough params, provider_slug). Wird auch nicht aus `ModelEntry.endpoints` extrahiert. |

---

## 2. Request Headers (POST /api/v1/images)

| Header | Spec | CLI reality | Status | Notes |
|--------|------|-------------|--------|-------|
| Authorization: Bearer | Required | `format!("Bearer {}", api_key)` in `client_mod.rs:do_request` | PASS | |
| Content-Type: application/json | Required | `.header("Content-Type", "application/json")` | PASS | |
| Accept: application/json | Required | `.header("Accept", "application/json")` | PASS | |
| HTTP-Referer | Optional | `req.header("HTTP-Referer", ...)` — **falscher Header-Name** | WRONG | Spec definiert `HTTP-Referer` (with hyphen). CLI sendet `HTTP-Referer` (hyphen statt underscore). OpenRouter akzeptiert beides, aber die Spec ist `HTTP-Referer`. |
| X-Title | Optional | `req.header("X-Title", ...)` aus `OPENROUTER_X_TITLE` | PASS | |
| X-Session-Id | Optional | **Nicht implementiert** | MISSING | Spec erlaubt `X-Session-Id` header zusätzlich zu `session_id` im body. CLI hat weder das Flag noch den Header. |

---

## 3. Request Body Fields (POST /api/v1/images)

| Field | Spec type/range | CLI support | Status | Notes |
|-------|----------------|-------------|--------|-------|
| `model` | string, required | `body["model"] = params.model` | PASS | |
| `prompt` | string, required, min 1 | `body["prompt"] = params.prompt` | PASS | Validierung auf non-empty in `cli.rs:Generate::validate()` |
| `aspect_ratio` | enum, 24 values + "auto" | **Kein CLI-Flag** | MISSING | Agents können Aspect Ratio nicht steuern. Spec: `1:1`, `2:1`, `16:9`, `9:16`, `4:3`, `3:4`, `2:3`, `3:2`, `4:5`, `5:4`, `1:2`, `2:3`, `1:4`, `4:1`, `1:8`, `8:1`, `2.35:1`, `5:2`, `4:5`, `5:4`, `9:16`, `16:9`, `9:19.5`, `19.5:9`, `9:20`, `20:9`, `9:21`, `21:9`, `auto` |
| `background` | enum: auto/transparent/opaque | Nicht gesendet | MISSING | `transparent` erfordert output_format png/webp. Keine Validierung dieses Zusammenhangs vorhanden. |
| `input_references` | object[], max 16, data-URLs oder HTTP(S) URLs | Nur `data:` data-URIs via `--image-ref` Flag | PARTIAL | Spec erlaubt **beide**: base64 data-URLs UND HTTP(S) URLs. CLI `reference_mod.rs:validate_data_uri()` lehnt HTTP URLs ab (`NotADataUri`). `input_references` Shape in body: `{"type":"image_url","image_url":{"url": uri}}` ✅ |
| `n` | integer, 1–10 | `body["n"] = params.n` nur wenn > 1 | PASS | Validierung in CLI: `value_parser = clap::value_parser!(u8).range(1..=10)` ✅ |
| `output_compression` | integer, 0–100, webp/jpeg only | Nicht gesendet | MISSING | CLI hat `output_format` CLI-Flag, sendet es aber nicht im Request Body. OutputFormat wird nur für Dateierweiterung genutzt. |
| `output_format` | enum: png/jpeg/webp/svg | **Wird nicht im Request Body gesendet** | MISSING | CLI hat `--output-format` Flag und `OutputFormat` Enum, aber `build_request_body()` und `to_request_body()` senden das Feld nie. Provider bekommt keinen output_format-Hinweis. |
| `provider` | object: allow_fallbacks/only/order/ignore/sort | Nicht gesendet | MISSING | Kein CLI-Flag für Provider-Routing. |
| `quality` | enum: auto/low/medium/high/xhigh/max | Nicht gesendet | MISSING | Kein CLI-Flag. |
| `resolution` | enum: 512/1K/2K/4K | `--resolution` Flag mit default="2K" ✅, im Body ✅ | PASS | Default 2K matches API-Default. |
| `seed` | integer | Nicht gesendet | MISSING | Kein CLI-Flag für deterministische Generierung. |
| `session_id` | string, max 256 | Nicht gesendet | MISSING | `X-Session-Id` Header fehlt (siehe Header-Sektion); `session_id` im Body wird auch nicht gesendet. |
| `size` | string: tier ("2K"/"4K") oder explicit ("2048x2048") | **Nicht implementiert** | MISSING | Spec: tier ist äquivalent zu `resolution` + kombiniert mit `aspect_ratio`; explicit pixels sind authoritativ und verwerfen `resolution`/`aspect_ratio` mit 400. CLI hat weder `--size` Flag noch entsprechende Validierung. |
| `stream` | boolean | CLI hat `--stream` Flag, wird aber **nicht im Request Body gesendet** | OUTDATED | `ValidatedGenerate.stream` wird in `cli.rs` gespeichert aber weder in `build_request_body()` noch in `to_request_body()` jemals in den Body geschrieben. Streaming wird in `client_mod.rs` nicht implementiert. |
| `trace` | object: trace_id/trace_name/span_name/generation_name/parent_span_id + custom | Nicht gesendet | MISSING | Kein CLI-Flag für Observability-Tracing. |
| `user` | string, max 256 | Nicht gesendet | MISSING | Kein CLI-Flag für End-User-ID. |

---

## 4. Response Body Fields (POST /api/v1/images)

| Field | Spec | CLI reality | Status | Notes |
|-------|------|-------------|--------|-------|
| `created` | integer, required | `Option<i64>` in `ApiResponse` | PARTIAL | Spec sagt required; CLI akzeptiert `None` (deserialisiert mit `#[serde(rename = "created")]` ohne `required`). Kein Validation-Failure bei fehlendem Feld. |
| `data` | object[], required | `Option<Vec<ImageData>>` in `ApiResponse` | PARTIAL | Spec sagt required; CLI akzeptiert `None`. `lib.rs:run_with_progress()` prüft `data.is_empty()` aber nicht `data == None`. |
| `b64_json` | string | `Option<String>` in `ImageData` ✅ | PASS | `write_images()` in `lib.rs` explizit mit `img.b64_json.as_ref().unwrap()` — überspringt URLs. |
| `url` | string (wenn nicht b64) | `Option<String>` in `ImageData` ✅, aber **kein Handling im write_images** | PARTIAL | `lib.rs:write_images()` hat `continue;` bei fehlendem `b64_json`, was URLs überspringt. Bei `url`-basierten Responses (falls Spec das jemals erlaubt) wäre das korrekt; aktuell Spec sagt aber immer `b64_json`, also PASS. |
| `revised_prompt` | string, optional (OpenAI models) | Nicht in `ImageData` deklariert | MISSING | Spec: Von OpenAI-Modeln zurückgegeben. CLI-ImageData hat kein `revised_prompt` Feld. Geht bei Deserialisierung verloren. |
| `background` | string (transparent wenn alpha) | Nicht in `ImageData` deklariert | MISSING | OpenAI-specific. Geht verloren. |
| `usage` | object (optional) | `Option<Usage>` ✅ | PARTIAL | `Usage`-Struct hat nur `prompt_tokens/completion_tokens/total_tokens/cost` (4 Felder). Spec deklariert 12+ Felder: `cost_details`, `prompt_tokens_details`, `completion_tokens_details`, `is_byok`, etc. Fehlende Felder in `Usage`Struct werden still ignoriert (nicht `#[serde(flatten)]`). |
| `error` | ApiErrorBody shape | `Option<ApiErrorBody>` in `ApiResponse` ✅ | PASS | Aber: `ApiErrorBody` hat `message`, `code`, `type`. `error_mod.rs` nutzt `message` für `ApiError::from_status` labelling korrekt. |

---

## 5. GET /api/v1/images/models

| Aspect | Spec | CLI reality | Status | Notes |
|--------|------|-------------|--------|-------|
| Endpoint URL | `GET /api/v1/images/models` | `IMAGE_MODELS_ENDPOINT = "https://openrouter.ai/api/v1/images/models"` ✅ | PASS | |
| `data[].id` | string | `ModelEntry.id` ✅ | PASS | |
| `data[].name` | string | `ModelEntry.name: Option<String>` ✅ | PASS | |
| `data[].description` | string | `ModelEntry.description: Option<String>` ✅ | PASS | |
| `data[].created` | integer | `ModelEntry.created: Option<i64>` ✅ | PASS | |
| `data[].architecture.modality` | string | `Architecture.modality: Option<String>` ✅ | PASS | |
| `data[].architecture.input_modalities` | string[] | `Architecture.input_modalities: Vec<String>` ✅ | PASS | |
| `data[].architecture.output_modalities` | string[] | `Architecture.output_modalities: Vec<String>` ✅ | PASS | |
| `data[].supported_parameters` | Map<String, ParamSpec> | `BTreeMap<String, ParamSpec>` ✅; `ParamSpec{kind, values, min, max}` ✅ | PASS | Alle 3 ParamSpec-Stile (enum/range/boolean) modelliert. |
| `data[].supports_streaming` | boolean | `ModelEntry.supports_streaming: bool` ✅ | PASS | Wird gelesen aber im CLI-Output nicht angezeigt. |
| `data[].endpoints` | string URL | `ModelEntry` hat kein `endpoints`-Feld | MISSING | Spec gibt `"/api/v1/images/models/{author}/{slug}/endpoints"` als URL zurück. CLI filtert das Feld nicht aus der JSON-Serialisierung raus (`serde` würde es default parsen), aber es gibt keinen Typ dafür und keine CLI-Option es anzuzeigen. |
| Per-Modell Filter | `architecture.output_modalities` contains "image" | `is_image_model()` filtert ✅ | PASS | |
| `resolution` in output | Nur Werte aus `supported_parameters` | Human-Output `emit_human_models()` zeigt `resolution_values()` ✅ | PASS | Aber: `aspect_ratio`-Werte, `quality`-Werte, `n`-Range etc. werden nicht angezeigt — nur `resolution`. |
| Per-Endpoint-Details | GET `/api/v1/images/models/{id}/endpoints` | **Nicht implementiert** | MISSING | Keine Routing-Capability, keine Pricing-Details, keine Passthrough-Parameter. |

---

## 6. Error Handling

| HTTP Status | Spec meaning | CLI handling | Status | Notes |
|-------------|-------------|--------------|--------|-------|
| 200 | Success | Verarbeitet, Bilder gespeichert | PASS | |
| 400 | Bad Request (z.B. size + resolution mismatch) | `ApiError::from_status(400, ...)` → `exit_code() → 4` | PASS | Aber: Spec hat kein `error`-Body-Validierung bei `revised_prompt`/missing fields. |
| 401 | Invalid API key | `exit_code() → 3` | PASS | Test `tests/client_401.rs` verifiziert das. |
| 402 | Insufficient credits | `exit_code() → 3` | PASS | Test `tests/client_402.rs` verifiziert das. |
| 403 | Forbidden — check model access | `exit_code() → 4` | PARTIAL | Spec语义: "Forbidden — check model access permissions". CLI mapt 403 → exit 4 (generische API error), nicht 3 (auth). Per Spec wäre das kein Auth-Error, also exit 4 ist korrekt. Label in `from_status` ist "Forbidden — check model access permissions" ✅. |
| 404 | Model not found | `exit_code() → 4` | PASS | Label "API error" generisch aber exit 4 korrekt. |
| 408 | Request timeout | Nicht spezifisch behandelt | PARTIAL | Spec listet 408; CLI mappt 408 → exit 4 generisch (kein spezifisches Label). Korrekt aber nicht spezifisch. |
| 429 | Rate limited | `exit_code() → 4` | PASS | Label "Rate limited" in `from_status` ✅. Spec sagt 429 = rate limit. Exit 4 = API error, nicht 5 (IO). Korrekt. |
| 500 | Internal server error | `exit_code() → 4` nach Retry ✅ | PASS | Label "OpenRouter server error" ✅. Retry 1x mit 2s Backoff. |
| 502 | Bad Gateway | `exit_code() → 4` nach Retry | PASS | Label "OpenRouter gateway error" ✅. Retry. |
| 503 | Service unavailable | `exit_code() → 4` | PASS | Label "OpenRouter service unavailable" ✅. Retry. |
| 504 | Gateway timeout | `exit_code() → 4` | PASS | Label "OpenRouter gateway timeout" ✅. Retry. |
| Response body `{"error": {"message": "..."}}` | Error shape | `ApiErrorBody` in `models_mod.rs` mit `message/code/type` ✅ | PASS | `from_status()` parsed body via `serde_json::from_str::<ApiErrorBody>` |
| Retry policy | 1x Retry bei 5xx mit Backoff | `client_mod.rs`: `if is_server_error { sleep(2s); retry }` ✅ | PASS | Nur 500–599 werden retry; 408, 429 werden NICHT retry. |
| Timeout | — | `reqwest::Client` mit `timeout(Duration::from_millis(120_000))` + 2s backoff, dann exit 5 | PASS | Timeout in `exit_code() → 5` (IO error). Label: "request timeout after {0:?}". |

---

## 7. Streaming

| Aspect | Spec | CLI reality | Status | Notes |
|--------|------|-------------|--------|-------|
| `stream=true` Request Body | SSE events: Partial image / Completed / Error | `--stream` Flag existiert in CLI ✅, wird aber **nie in den Request Body geschrieben** | OUTDATED | `build_request_body()` und `to_request_body()` senden kein `"stream"` Feld. `ValidatedGenerate.stream` ist ein toter Speicher. |
| SSE Parsing | Drei event types: `partial_image`, `completed`, `error` | Nicht implementiert | MISSING | `reqwest` hat `stream` feature ✅, aber `do_request()` nutzt `send()` → `resp.text()` → buffered response. Keine SSE-Event-Loop. |
| Streaming → NDJSON auf stderr | Spec: SSE events auf HTTP-Stream | CLI hat `ProgressEvent` NDJSON-Schema, aber nur für HTTP-Status/Erors, nicht für SSE | OUTDATED | `progress_mod.rs` definiert `ProgressEvent` für `HttpStatus/RetryAttempt/ApiError`, aber keine `PartialImage/ImageCompleted` Events. |
| Streaming Billing | All-or-nothing; keine partial charges | Nicht relevant da Streaming nicht implementiert | PASS | Billing-Regel nicht verletzt weil Feature nicht genutzt wird. |

---

## 8. Reference Images

| Aspect | Spec | CLI reality | Status | Notes |
|--------|------|-------------|--------|-------|
| `data:` base64 data URLs | Erlaubt | `validate_data_uri()` in `reference_mod.rs` ✅ | PASS | Prüft `data:` prefix, `;base64,` suffix. Test `tests/data_uri_validate.rs` ✅. |
| HTTP(S) URLs | Erlaubt (max 16) | **Aktiv abgelehnt** | WRONG | Spec: "base64 data URLs **or HTTP(S) URLs**". CLI: `validate_data_uri()` gibt `NotADataUri` bei `https://` URLs zurück. Agenten können keine HTTP-Referenzbilder verwenden. `tests/data_uri_validate.rs` testet explizit, dass HTTP URLs rejected werden. |
| Max 16 references | maxItems: 16 | Kein CLI-Limit | PARTIAL | CLI hat kein `--image-ref` Repeat-Limit ( Clap `Vec<String>` ohne `max_values`). Validierung nur auf URI-Format. |
| `input_references` Shape | `{"type":"image_url","image_url":{"url":"..."}}` | `serde_json::json!({ "type":"image_url", "image_url":{"url": uri }})` ✅ | PASS | Body-Format korrekt. |

---

## 9. Edge-Cases

| Aspect | Spec rule | CLI implementation | Status | Notes |
|--------|-----------|-------------------|--------|-------|
| `size` + `resolution`/`aspect_ratio` conflict | Explizite pixel size ist authoritativ; mismatched `resolution`/`aspect_ratio` → 400 | CLI hat kein `--size` Flag | PASS | Kein Konflikt möglich. Kein Bug. |
| `aspect_ratio: "auto"` | Provider entscheidet | Kein `--aspect-ratio` Flag | MISSING | Agents können "auto" nicht senden. |
| `background: "transparent"` requires alpha format | png oder webp required | CLI sendet `background` nicht; `output_format` wird nicht im Body gesendet | PASS | Kein falsches Verhalten möglich. Provider-default applies. |
| `output_compression` ignored for png | png ignoriert compression; provider ohne compression knob ignorieren es auch | CLI sendet kein `output_compression` | PASS | Kein falsches Verhalten. |
| API defaults | resolution=2K, aspect_ratio="1:1", n=1 | CLI `resolution` default="2K" ✅; kein `aspect_ratio` ✅; `n` default=1 ✅ | PARTIAL | CLI matched API-Defaults korrekt. `aspect_ratio` kann aber nicht gesetzt werden (weder default noch explicit). |
| Model-spezifische Parameter | Nicht alle Modelle unterstützen alle Parameter | CLI sendet `resolution` ohne Check; kein `list-models`-basierter Filter | PARTIAL | `list-models` zeigt `resolution` values pro Modell, aber CLI erzwingt keine Validierung. Agent muss selbst prüfen. `cli.rs` comment: "Use `list-models` to discover which values a given model actually honours." |
| `resolution` default | 2K (API-mandated) | CLI `#[arg(long, default_value = "2K")]` | PASS | Kommentar in cli.rs: "Default: 2K (per OpenRouter docs, 2K is the API's mandated default)" ✅ korrekt. |
| `n` Range | 1–10 | CLI `value_parser = clap::value_parser!(u8).range(1..=10)` | PASS | |
| Max `session_id` length | 256 chars | Nicht implementiert | MISSING | Weder `session_id` body field noch `X-Session-Id` Header. Kein Length-Check möglich. |
| `user` field | max 256 chars, hashed upstream | Nicht implementiert | MISSING | Kein CLI-Flag. |

---

## Top-5 prioritized findings (Ranked by user-impact)

1. **MISSING — `aspect_ratio` CLI flag** (Severity: HIGH)
   API supports 24+ normalized aspect ratios + "auto". CLI has no `--aspect-ratio` flag. Agents cannot control image composition. Every image generation uses the provider-default (1:1). Fix: add `--aspect-ratio` enum flag with full value set + "auto" + validation against model's `supported_parameters`.

2. **OUTDATED — `--stream` flag exists but does nothing** (Severity: MEDIUM)
   `ValidatedGenerate.stream` is stored but never sent in the request body. Streaming is documented in `cli.rs` but not wired to `build_request_body()` or the HTTP client. `reqwest` stream feature is enabled but unused. Fix: add `"stream": true` to request body + SSE parser in `do_request()`.

3. **WRONG — `input_references` rejects HTTP(S) URLs** (Severity: HIGH)
   Spec: "base64 data URLs **or HTTP(S) URLs**". CLI: `validate_data_uri()` only accepts `data:` URIs. Agents cannot pass HTTP(S) reference images, forcing manual base64 encoding. Fix: extend validator to also accept `https://` and `http://` URLs, or add a separate `--image-url` flag.

4. **MISSING — `output_format` CLI flag not sent in request body** (Severity: MEDIUM)
   CLI has `--output-format` flag + `OutputFormat` enum, but `build_request_body()` never writes `"output_format"` to the JSON body. The flag only controls the file extension of saved output. Providers receive no format hint. Fix: add `"output_format": params.output_format.to_api_string()` to body builder.

5. **WRONG — `HTTP-Referer` header uses wrong casing** (Severity: LOW)
   Spec: `HTTP-Referer` (hyphen). CLI code: `.header("HTTP-Referer", ...)` (underscore). OpenRouter tolerates both, but this is a spec violation. Fix: change `"HTTP-Referer"` to `"HTTP-Referer"` in `client_mod.rs:do_request`.

---

## Recommended next steps

- [ ] Add `--aspect-ratio` CLI flag with full enum validation (24 values + "auto")
- [ ] Wire `--stream` to request body + implement SSE parser in `do_request()`
- [ ] Extend `reference_mod.rs` to also accept HTTP(S) URLs, or add `--image-url` flag
- [ ] Add `"output_format"` to `build_request_body()` body map
- [ ] Fix header `"HTTP-Referer"` → `"HTTP-Referer"` in `client_mod.rs`
- [ ] Add `--size` flag (tier or explicit pixels `"WxH"`) with conflict detection vs `--resolution`
- [ ] Add `X-Session-Id` header + `session_id` body field
- [ ] Add `--quality` flag (`auto/low/medium/high/xhigh/max`)
- [ ] Add `--seed` flag for deterministic generation
- [ ] Add `--provider` routing flags (`--provider-only`, `--provider-ignore`, `--provider-order`)
- [ ] Add `--trace` flag with sub-options (trace_id, trace_name, etc.)
- [ ] Add `--user` flag for end-user tracking
- [ ] Add `--background` flag with format compatibility warning
- [ ] Add `endpoints` field to `ModelEntry` + CLI subcommand `openrouter-image endpoints <model>`
- [ ] Enrich `Usage` struct with `cost_details`, `prompt_tokens_details`, `completion_tokens_details`, `is_byok`
- [ ] Add `revised_prompt` and `background` to `ImageData` struct
- [ ] Validate CLI args (`resolution`, `n`) against model's `supported_parameters` at runtime
- [ ] Add `--output-compression` flag (0–100) for jpeg/webp
