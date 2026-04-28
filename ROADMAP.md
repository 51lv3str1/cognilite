# Roadmap — cognilite

Plan de evolución basado en la revisión arquitectónica del 2026-04-25.
Objetivo final: **que un modelo lanzado vía `cognilite --headless` pueda ejecutar autónomamente esta misma revisión sobre cualquier proyecto.**

Las fases están ordenadas por dependencia y ROI. Cada item lleva esfuerzo estimado y archivo(s) afectado(s).

---

## Fase 0 — Quick wins (≤1 día total)

Cierran agujeros conocidos sin tocar arquitectura.

### ~~0.1 Gate de seguridad en `execute_command`~~ ✅ done 2026-04-25
- **Archivos (post Fase 1.1.B):** `src/runtime/tools.rs` (`is_destructive_shell`, `handle_tool_call`, `execute_tool_call`) + `src/app.rs` (`pending_tool_call`, `submit_ask`) + `src/adapter/headless_runner.rs` (post-rename de `src/headless.rs`).
- **Implementado:** `handle_tool_call` ahora detecta destructivos (`rm`/`mv`/`dd`/`chmod`/`chown`/`shred`/`truncate`/`mkfs`/`rmdir`/`chgrp` + `git rm/mv/clean/reset/checkout`, con skip de `sudo` y resolución de basename) y, salvo que `auto_accept = true`, los pone en `pending_tool_call` y dispara `<ask Confirm>`. `submit_ask` ejecuta vía `execute_tool_call` con "Yes" o inyecta `"Command declined by user."` con "No". Built-ins (`read_file`/`write_file`/`edit_file`/`grep_files`/`glob_files`/`tree`/`note`) bypass del gate. Headless setea `auto_accept = args.yes` y resuelve el ask post-`handle_tool_call` con `ask_interactive` para no quedar colgado.

### ~~0.2 Token-budget en tool output del shell~~ ✅ done 2026-04-25
- **Archivo (post Fase 1.1.B):** `src/runtime/tools.rs` (`truncate_output`, aplicado en `execute_command`).
- **Implementado:** `truncate_output()` cap de 500 líneas / 32KB (lo que se alcance primero), con back-off a char boundary y sufijo `[... truncated: N more lines, M more bytes — pipe to head/sed or use read_file with offset]`. Aplica al stdout exitoso y al `error: ...` formateado. Built-ins (`read_file` ya con `MAX_READ_LINES=500`, `grep_files` con `MAX_GREP_RESULTS=100`) no fueron tocados — siguen con su propio cap.

### ~~0.3 Debounce de `save_config`~~ ✅ done 2026-04-25
- **Archivos:** `src/app.rs` (`save_config` ahora marca `config_dirty`; `flush_config` hace el JSON write); `src/main.rs` (flush en `should_quit`)
- **Implementado:** `save_config()` pasó de `&self` (escribe JSON) a `&mut self` (solo `config_dirty = true`). La lógica de write se movió a `flush_config()`, idempotente. Flush automático en: `confirm_config` (Enter en Config), `toggle_config` arm `Config → ModelSelect` (Esc al salir), `set_username` (al terminar edición), y `run_loop` antes del break por `should_quit`. Resultado: `param_adjust` con autorepeat de flecha → cero I/O hasta que el user sale de la pestaña. Trade-off: SIGKILL pierde cambios pendientes (aceptable para TUI single-user).

### ~~0.4 Naming consistency~~ ✅ done 2026-04-25
- **Cambios aplicados:**
  - `cingulateGate/` → `cingulate_gate/` (snake_case, alineado con `cortex`, `synapse`, etc.). El `name = CingulateGate` en `neuron.toml` se mantuvo porque ya era PascalCase coherente con `Cortex`/`Synapse`.
  - `thalamon/` → `thalamus/` (término neurociencia correcto). Actualizado `name = Thalamus` en `neuron.toml`, renombrado `Thalamon.md` → `Thalamus.md`, y `sed` global del contenido (5 ocurrencias). Cero referencias en `src/` — el rename no rompe código.

---

## Fase 1 — Refactor estructural (1-2 días)

Bajan la barrera para que un modelo navegue el código en próximas revisiones, y eliminan duplicación que dispara bugs.

### 1.1 Partir `app.rs` — layout hexagonal (en lugar de `src/app/*`)
- **Decisión:** no se usó el layout `src/app/*.rs` propuesto originalmente; en su lugar se aplicó **hexagonal/ports-and-adapters** porque cognilite tiene múltiples vistas (TUI/headless/HTTP/WS) y múltiples controllers (keyboard/HTTP/WS), no encajan en MVC clásico.
- **Layout final:**
  ```
  src/
  ├── main.rs                # dispatch
  ├── app.rs                 # struct App + state machine glue (~2200 LoC)
  ├── domain/                # MODEL puro, testable sin runtime
  │   ├── message.rs         # Role, Message, Attachment, AttachmentKind, TokenStats
  │   ├── tags.rs            # extract_*, AskKind, InputRequest, is_in_code_block
  │   ├── prompt.rs          # RuntimeMode, build_runtime_context, TemplateFormat, build_raw_prompt
  │   ├── config.rs          # Config, CtxStrategy, NeuronMode, NeuronPreset, load_config
  │   └── neuron.rs          # ex synapse.rs (Neuron, Synapse, build_tool_context)
  ├── runtime/               # impl App por feature
  │   ├── input.rs           # Completion, input_*, complete_*, get_path_completions
  │   ├── picker.rs          # FilePicker, FilePanel, syntect highlighting
  │   ├── pinned.rs          # PinnedFile, check/diff
  │   ├── room.rs            # WS room sync
  │   └── tools.rs           # handle_tool_call, is_destructive_shell, execute_command
  ├── view/tui.rs            # ratatui rendering
  └── adapter/               # I/O — ollama, ws_server, ws_client, http_server,
                             # keyboard, headless_runner, tools_native, clipboard
  ```

### ~~1.1.A Sub-fase A: hexagonal skeleton~~ ✅ done 2026-04-27
- **Implementado:** `mkdir src/{domain,runtime,view,adapter}`, `git mv` de los 10 archivos standalone (clipboard/ollama/tools→tools_native/server→http_server/ws_client/websocket→ws_server/events→keyboard/headless→headless_runner/synapse→neuron/ui→tui), bulk-update de imports vía sed (`crate::ollama` → `crate::adapter::ollama`, etc.), `mod.rs` por dir, aliases en main.rs (`use adapter::headless_runner as headless;`) para que call-sites no churneen. Commit `9810e14`.

### ~~1.1.B Sub-fase B: split de app.rs~~ ✅ done parcial 2026-04-27
- **Implementado:** dos commits (`e9eba6a`, `a148130`).
  - **domain/**: `message.rs` (55), `tags.rs` (137), `prompt.rs` (201), `config.rs` (124).
  - **runtime/**: `pinned.rs` (70), `tools.rs` (234), `room.rs` (76), `input.rs` (420), `picker.rs` (572).
- **Resultado:** app.rs **3988 → 2248 líneas (-44%)**. Build dev + release limpios.
- **Pendiente** (lo que sigue en app.rs):
  - **stream.rs** (~600 LoC) — `start_stream`, `poll_stream`, `stop_stream`, `poll_warmup`. El más grande, también el core del producto.
  - **ws.rs** (~400 LoC) — `poll_ws`, `handle_ws_frame`, `poll_remote_*`, `switch_to_local`.
  - **attachments.rs** (~150 LoC) — `resolve_attachments`, `split_at_paths`, `file_kind`, `resolve_path`.
  - **identity.rs** (~50 LoC, en `domain/`) — `new_session_id`, `model_display_name`, `username_color`, `extract_mentions`, `is_mentioned`.
- **Recomendación:** parar acá hasta que se valide el layout actual. app.rs a 2248 líneas ya entra en context de cualquier modelo razonable.

### ~~1.2 Unificar tag extraction/stripping~~ ✅ done 2026-04-27 (re-evaluado 2026-04-28)
- **Implementado en `domain/tags.rs`:** helpers genéricos `extract_tag(content, name)` y `strip_tag(content, name)` (con `after_think()` extraído como helper privado). `extract_tool_call`/`extract_patch_tag`/`extract_mood_tag`/`extract_load_neuron_tag` colapsaron a 1-2 líneas cada uno. En Fase 3.5 se extendió `strip_tag` para tolerar atributos (`<finding severity="...">`) usando prefix-match con guard, y se preservaron byte positions (sin `trim_end`) para no romper byte-counting en headless.
- **Re-evaluación del "pendiente" original (2026-04-28):** la idea inicial era reemplazar las 5 repeticiones de `rfind("</think>") + find + truncate` en `poll_stream`/`run_stream_loop`/`stream_loop` con llamadas a `strip_tag`. Inspeccionando los callsites se concluyó **no aplica** — cada handler hace transformación semántica distinta:
  - `<tool>`: trunca display al `<tool>` pero **preserva** la tag en `llm_content` hasta `</tool>` (el modelo necesita ver su propia tool call en history).
  - `<ask>`: trunca display **desde** `<ask` hasta el final (la stream se detiene; no es un strip de tag cerrado).
  - `<patch>`: **reemplaza** `<patch>...</patch>` con un fence ```diff...```. No es delete, es transform.
  - `<preview>`: tag self-closing (`<preview path="..."/>`), forma sintáctica distinta a paired tag.
- `<mood>`/`<load_neuron>`/`<finding>` sí usan `strip_tag` porque son delete-tag-and-body sin transformación. Los otros 4 quedan con su lógica inline porque generalizarlos requeriría un helper diferente (transformer-based) que no aporta sobre lo que ya tienen.

### ~~1.3 Tests de los contratos de parsing~~ ✅ done 2026-04-27 (extendido 2026-04-28)
- **57 tests passing** distribuidos:
  - `domain/tags.rs` (20): `extract_tool_call` (think skip/code fence skip/think unclosed), `extract_tag` genérico, `strip_tag` (presente/ausente, atributos, prefix-match safety), `extract_ask_tag` (text/confirm/choice), `extract_preview_tag`, `extract_mood_tag` con trim, `extract_finding_tag` (full attrs/missing attrs/inside think/unclosed), `Finding::to_markdown`.
  - `domain/prompt.rs` (11): `detect_template_format` (ChatML/Llama3/Gemma/unknown), `build_raw_prompt` (3 formatos × last_is_assistant × system optional), `build_runtime_context` (TUI + project_map).
  - `adapter/tools_native.rs::note_tests` (5): CRUD + multiline indent + invalid subcommand + empty text.
  - `adapter/headless_runner.rs::safe_print_tests` (9, agregados 2026-04-28): empty / plain text / complete `<think>` skip / unclosed `<think>` stop / stop antes de `<tool>` / stop antes de `<finding` / stop en partial-prefix `<fin` / `<` no-tag (e.g. `if x < y`) sigue / `from` offset resume.
  - `src/app.rs::mention_tests` (12, agregados 2026-04-28): `extract_mentions` (empty / simple / con session_id / strip punctuation / lowercase / multiple / skip invalid). `is_mentioned` (full form / `#all` keyword / different session_id no-match / no mention / case-insensitive).

### ~~1.4 Reemplazar parsing manual de config por `serde::Deserialize`~~ ✅ done 2026-04-27
- **Implementado en `domain/config.rs`:** struct interno `ConfigFile` con `#[derive(Default, Deserialize)]` y `#[serde(default)]`. `load_config` baja de ~47 líneas de `val.get().and_then().unwrap_or()` a ~22 líneas declarativas. `NeuronPreset` ahora deriva `Deserialize` directo. `flush_config` no se tocó (sigue construyendo el JSON manualmente con `serde_json::json!`); inconsistencia residual: si se agrega un campo nuevo, hay que tocar 2 lugares.

---

## Fase 2 — Higiene de prompts (½ día)

### ~~2.1 Comprimir neurons `.md` a ≤20 líneas~~ ✅ done 2026-04-28
- **Resultado:** 551 → 232 LoC totales en `thoughts/*.md` (-58%).
  - Synapse: 54 → 20.
  - Efferent: 61 → 21.
  - Thalamus: 93 → 24.
  - CingulateGate: 75 → 20.
  - Insula: 77 → 23.
  - Cortex: 58 (intacto — es contexto de producto).
  - Hippocampus: 66 (intacto — el más estructurado y crítico para el self-review; +sección "Working memory" agregada en 3.6).
- **Plantilla aplicada:** `# Name` → `## What I do` (1-2 oraciones) → `## When I activate` (bullets) → `## Tags I emit` / `## Tools I prefer` (sintaxis exacta) → `## Constraints` (1-2 reglas críticas) → `## Example` (input → output). Sacado el vocabulario meta ("INTERNAL_SYSTEM_OUTPUT", "Routing Logic", "Deterministic State Machine") que confunde más de lo que ayuda en modelos chicos.

### ~~2.2 Eliminar neurons con valor cuestionable~~ ✅ done 2026-04-28
- **Engram removido.** El código no enforce su "transparency policy" — la info que expone ya está en el system prompt y el modelo puede listarla sin neuron dedicado. `disabled_neurons`/`on_demand_neurons`/presets que lo referencien siguen funcionando (loader filtra por neurons cargados, las referencias huérfanas se ignoran sin error).
- **Insula mantenido y comprimido** (77 → 23). Tiene backing en código (`extract_mood_tag` + `app.current_mood` rendered en header), no es prompt-puro como Engram.

---

## Fase 3 — Habilitar self-review autónomo (1-2 días)

Aquí es donde un modelo deja de necesitar una persona sosteniendo el hilo de la revisión.

### ~~3.1 Neuron `Hippocampus`~~ ✅ done 2026-04-27
- **Creado:** `.cognilite/neurons/hippocampus/neuron.toml` + `thoughts/Hippocampus.md`. Nombre cambiado de `Architect` a `Hippocampus` para mantener la convención neurocientífica del resto (Cortex/Synapse/Efferent/Engram/Insula/CingulateGate/Thalamus). El hipocampo maneja mapeo cognitivo + memoria episódica — encaja con el rol del auditor (mapear el proyecto y registrar findings). Contenido en inglés.
- Define protocolo (mapear → manifests → top-N por LOC → findings con `file:line` → resumen ejecutivo), categorías (security/tech-debt/bug/perf/style), restricciones (solo lectura, no inventar líneas, sin bullets vacíos), ejemplo concreto.

### ~~3.2 Templates `/review` y `/audit`~~ ✅ done 2026-04-27
- **Creados:** `.cognilite/templates/review.md` (revisión arquitectónica general, dispara `<load_neuron>Hippocampus</load_neuron>` y describe el flow) y `.cognilite/templates/audit.md` (foco en `category="security"`: comandos sin gate, secretos hardcoded, inputs sin validar, boundaries violados, deps con CVEs, races en código sync). Contenido en inglés. `/refactor` queda fuera por ahora — no pedido inmediato.

### ~~3.3 Tool builtin `tree`~~ ✅ done 2026-04-27
- **Implementado en `adapter/tools_native.rs`:** `pub fn tree(args, working_dir)`. Usa `fd --max-depth N --type f` si está disponible (respeta `.gitignore`); fallback a `find` con excludes hard-coded (target/.git/node_modules/.venv). Output: árbol indentado por dir con LOC para archivos `.rs/.ts/.py/.go/.rb/.java/.c/.cpp/.h/.swift/.kt/.scala`. Cap inline a 32KB. Registrado como built-in en `runtime/tools.rs::execute_tool_call` (bypass del destructive-shell gate).

### ~~3.4 Auto-inject `<project_map>` en `runtime_context`~~ ✅ done 2026-04-27
- **Implementado:** `build_project_map(working_dir)` en `adapter/tools_native.rs` detecta marcadores (`Cargo.toml`/`package.json`/`pyproject.toml`/`go.mod`/`deno.json`/`Gemfile`/`build.gradle`/`pom.xml`) y emite `<project_map>\n{tree}\n</project_map>`. Firma de `build_runtime_context` extendida con `project_map: Option<&str>`. Los 3 callers (`app.rs::select_model`, `headless_runner.rs::run`, `ws_server.rs::run_session`) ahora calculan el map y lo pasan. Test `runtime_context_appends_project_map_when_present` agregado.

### ~~3.5 Tag `<finding>` de primera clase~~ ✅ done 2026-04-28
- **Implementado en `domain/tags.rs`:** struct `Finding { severity, file, category, body }` + `extract_finding_tag()` (parsea atributos con tolerancia a comillas simples/dobles, ignora dentro de `<think>` y code fences). Helper `Finding::to_markdown()` para el render.
- **Refactor side-effect:** `strip_tag()` ahora tolera atributos (`<finding severity="...">`) usando prefix-match `<name` con guard de char siguiente (`>`/space/`/`). 8 tests nuevos en `domain/tags.rs` (31 totales).
- **State (`app.rs`):** `findings: Vec<Finding>` + `findings_at_stream_start: usize`. Reset en `send_message`, `clear_chat`, `select_model`, `load_chat`. Helper `push_findings_report()` empuja un `Role::Tool` con markdown estructurado y `llm_content = ""` (filtrado por `start_stream`, así el modelo no vuelve a ver el reporte el turno siguiente).
- **Wireup en los 3 callsites:**
  - **TUI (`poll_stream`):** loop que consume todos los `<finding>` cerrados en cada chunk → `findings.push` + `strip_tag(&mut content)` + `strip_tag(&mut llm_content)`. Render al `chunk.done` vía `push_findings_report()`.
  - **Headless (`run_stream_loop`):** mismo loop + retract de `printed_up_to` si encogió el content tras strip. Al `done` imprime `## Findings (N)` markdown a stdout antes de las stats. `safe_print_boundary` ahora incluye `<finding>`/`</finding>` en STOP_TAGS y `'f'` como segunda letra válida.
  - **WS server (`stream_loop`):** mismo loop con retract de `printed_up_to`. `push_findings_report()` al done; el reporte se sincroniza al room vía el flujo normal de `app.messages → r.messages.extend()` y los TUI clients lo reciben en `room_update`.
- **Sintaxis emitida por el modelo:**
  ```xml
  <finding severity="high" file="src/app.rs:2702" category="security">
    Shell passthrough sin gate. Riesgo: rm -rf no confirmado.
    Fix: matchear comandos destructivos en execute_command y exigir confirm.
  </finding>
  ```

### ~~3.6 Tool `note` (memoria de trabajo)~~ ✅ done 2026-04-28
- **Implementado en `adapter/tools_native.rs`:** `pub fn note(args, session_id)` con subcomandos `add` (append-only, multi-línea con indentación), `list`, `clear`. Persiste a `std::env::temp_dir().join("cognilite-notes-<session>.md")`. Helper `read_notes(session_id)` para inyección al system prompt. 5 tests CRUD passing.
- **Wireup en `runtime/tools.rs::handle_tool_call`:** `"note"` agregado a la lista de built-ins (bypass del destructive-gate) y a `execute_tool_call` con la firma `(full_args, &self.session_id)`.
- **Inyección en `app.rs::full_system_prompt`:** después de pinned files, lee el archivo y appendea como sección `## Notes (working memory)` si no está vacío. `warmup_last_hash` invalida automáticamente al cambiar el contenido (mismo flow que pinned).
- **Discoverability:** mención agregada en `Hippocampus.md` con ejemplo concreto ("después de cada file top-LOC, jot down ✓ con findings").
- **Persistencia:** archivos en `/tmp/cognilite-notes-<sid>.md` quedan stale entre sesiones (session_id se regenera en cada start). `/tmp` se limpia en boot — aceptable para working memory scoped al run actual.

---

## Fase 4 — Mejoras de calidad

### ~~`new_session_id` 32 bits~~ ✅ done 2026-04-28
- **Implementado:** buffer de 3 → 4 bytes; format string `{:02x}{:02x}{:02x}{:02x}`. Display ahora "qwen3.6#a3f2b1c8" (8 hex chars). Birthday-bound ~65k IDs vs ~4k antes — el WS server ya hacía retry-on-collision en un loop pero ahora retries son ~16x menos probables.

### ~~Conexiones WS con límite~~ ✅ done 2026-04-28
- **Implementado en `adapter/http_server.rs`:** `ACTIVE_CONNECTIONS: AtomicUsize` con tope `MAX_CONNECTIONS = 64`. En `listener.incoming()` chequea before-spawn; si está al tope, responde `503 Service Unavailable` con `Retry-After: 5`. Tanto HTTP `/chat` como WebSocket sessions cuentan en el mismo cap (mismo entry point en `handle()`).

### ~~`--metrics` flag~~ ✅ done 2026-04-28
- **Implementado:** `HeadlessArgs.metrics: bool`, parsed en `main.rs::parse_headless_args`. En `run_stream_loop` se trackea `tool_calls` count + `wall_secs` (Instant al start). En `chunk.done` escribe JSON-line a `/tmp/cognilite-metrics-<session>.json`: `{ts, session_id, model, preset, tps, response_tokens, prompt_eval, tool_calls, findings, wall_secs}`. Append-only, una línea por turn → trivial jq-parse para A/B testing de presets/neurons (justo lo que necesitamos para validar la compresión de Fase 2).
- **Uso:** `cognilite --headless -m qwen3.6 --preset hippocampus --metrics "audita..."` → escribe stats a `/tmp/cognilite-metrics-<sid>.json`.

### Deferred — rationale documentada

- **`save_config` con writer thread**: el debounce + flush actual (`config_dirty` + `flush_config`) elimina I/O por keystroke. Un writer thread agregaría sincronización por ganancia mínima. **Decisión:** no necesario.
- **`extract_tool_call` skip de code fences pre-`</think>`**: edge-case (fence abierto que cruza el bloque thinking). Rara en práctica porque modelos cierran fences dentro del thinking. Fix requiere un escáner stateful más complejo. **Decisión:** revisitar si aparece en logs reales.
- **Soporte Windows**: `clipboard.rs` ya tiene `#[cfg(windows)]`. `apply_patch` depende del binario `patch` (Linux/macOS). Implementar fallback Rust = patch parser propio + path/line-ending handling. Effort substancial, fuera del alcance del self-review autónomo. **Decisión:** documentar README como Linux/macOS-only por ahora; Windows contribution welcome.

---

## Lo que NO se va a tocar

Decisiones de diseño que la revisión validó como correctas:

- **Sin async / sin Tokio.** `std::thread + mpsc` es suficiente, ahorra cientos de deps transitivas.
- **Inline crypto (SHA-1, base64, WS framing).** ~150 líneas mantenibles, cero supply-chain risk.
- **Tag-interception sobre tool-use nativo.** El protocolo funciona con cualquier modelo Ollama.
- **Raw-prompt continuation con `/api/generate raw:true`.** Hacky pero necesario; está bien comentado el por qué.
- **Pinned files con delta-diff y hash de warmup.** Ingeniería seria para reuso de KV cache.

---

## Secuencia recomendada

```
Semana 1:  Fase 0  (gate + budget + debounce + naming)        → 1 día
           Fase 1.1 (partir app.rs)                            → ½ día
           Fase 1.2 (unificar tags)                            → ½ día

Semana 2:  Fase 1.3 (tests)                                    → ½ día
           Fase 1.4 (serde Config)                             → ¼ día
           Fase 2   (higiene de prompts)                       → ½ día

Semana 3:  Fase 3.1-3.4 (Hippocampus + templates + tree + map) → 1 día
           Fase 3.5 (tag <finding>)                            → ½ día
           Fase 3.6 (tool note)                                → ½ día

Validación: lanzar `cognilite --headless --preset hippocampus "audita este repo"`
            sobre cognilite mismo. La salida esperada es un .md similar al que
            produjo esta revisión manual.
```

---

## Métrica de éxito

El roadmap está completo cuando:

```bash
cognilite --headless -m qwen3:8b --preset hippocampus \
  "Audita este proyecto. Emití findings estructurados y un resumen ejecutivo."
```

produce un reporte cuya **calidad y cobertura ≥ 80%** del informe humano del 2026-04-25, sin intervención del usuario después del prompt inicial.
