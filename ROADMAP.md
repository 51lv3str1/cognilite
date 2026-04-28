# Roadmap — cognilite

Plan de evolución basado en la revisión arquitectónica del 2026-04-25.
Objetivo final: **que un modelo lanzado vía `cognilite --headless` pueda ejecutar autónomamente esta misma revisión sobre cualquier proyecto.**

Las fases están ordenadas por dependencia y ROI. Cada item lleva esfuerzo estimado y archivo(s) afectado(s).

---

## Fase 0 — Quick wins (≤1 día total)

Cierran agujeros conocidos sin tocar arquitectura.

### ~~0.1 Gate de seguridad en `execute_command`~~ ✅ done 2026-04-25
- **Archivo:** `src/app.rs` (`is_destructive_shell`, `handle_tool_call`, `execute_tool_call`, `submit_ask`) + `src/headless.rs`
- **Implementado:** `handle_tool_call` ahora detecta destructivos (`rm`/`mv`/`dd`/`chmod`/`chown`/`shred`/`truncate`/`mkfs`/`rmdir`/`chgrp` + `git rm/mv/clean/reset/checkout`, con skip de `sudo` y resolución de basename) y, salvo que `auto_accept = true`, los pone en `pending_tool_call` y dispara `<ask Confirm>`. `submit_ask` ejecuta vía `execute_tool_call` con "Yes" o inyecta `"Command declined by user."` con "No". Built-ins (`read_file`/`write_file`/`edit_file`/`grep_files`/`glob_files`) bypass del gate. Headless setea `auto_accept = args.yes` y resuelve el ask post-`handle_tool_call` con `ask_interactive` para no quedar colgado.

### ~~0.2 Token-budget en tool output del shell~~ ✅ done 2026-04-25
- **Archivo:** `src/app.rs` (`truncate_output`, aplicado en `execute_command`)
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

### ~~1.2 Unificar tag extraction/stripping~~ ✅ done parcial 2026-04-27
- **Implementado en `domain/tags.rs`:** helpers genéricos `extract_tag(content, name)` y `strip_tag(content, name)` (con `after_think()` extraído como helper privado). `extract_tool_call`/`extract_patch_tag`/`extract_mood_tag`/`extract_load_neuron_tag` colapsaron a 1-2 líneas cada uno. `extract_ask_tag` queda aparte por el parsing de `type="..."`, `extract_preview_tag` por ser self-closing — pendientes para una segunda iteración si vale la pena.
- **Pendiente:** refactor de `app.rs::poll_stream` y `adapter/headless_runner.rs::run_stream_loop` para usar `strip_tag` en vez de inline `rfind/find/truncate` (cinco repeticiones). El helper ya está disponible; aplicarlo cuando se toque ese código.

### ~~1.3 Tests de los contratos de parsing~~ ✅ done 2026-04-27
- **23 tests passing** en `domain/tags.rs` (12) y `domain/prompt.rs` (11).
- Cobertura: `extract_tool_call` (think skip/code fence skip/think unclosed), `extract_tag` genérico, `strip_tag` (presente/ausente), `extract_ask_tag` (text/confirm/choice), `extract_preview_tag`, `extract_mood_tag` con trim, `detect_template_format` (ChatML/Llama3/Gemma/unknown), `build_raw_prompt` (3 formatos × last_is_assistant true/false × system optional), `build_runtime_context` (TUI mode con model/ctx).
- **Pendiente:** `safe_print_boundary` (en `adapter/headless_runner.rs`), `is_mentioned`/`extract_mentions` (siguen en `app.rs`).

### ~~1.4 Reemplazar parsing manual de config por `serde::Deserialize`~~ ✅ done 2026-04-27
- **Implementado en `domain/config.rs`:** struct interno `ConfigFile` con `#[derive(Default, Deserialize)]` y `#[serde(default)]`. `load_config` baja de ~47 líneas de `val.get().and_then().unwrap_or()` a ~22 líneas declarativas. `NeuronPreset` ahora deriva `Deserialize` directo. `flush_config` no se tocó (sigue construyendo el JSON manualmente con `serde_json::json!`); inconsistencia residual: si se agrega un campo nuevo, hay que tocar 2 lugares.

---

## Fase 2 — Higiene de prompts (½ día)

Las neuronas hoy gastan tokens en prosa que no se enforce.

### 2.1 Comprimir neurons `.md` a ≤20 líneas
- **Archivos:** `cingulate_gate/`, `engram/`, `insula/`, `thalamus/`, `efferent/`, `synapse/`
- **Esfuerzo:** ~1-2 horas
- **Por qué:** `CingulateGate.md` solo aporta ~1.2k tokens al system prompt en modelos pequeños (gemma:2b, qwen3:4b). El vocabulario meta ("INTERNAL_SYSTEM_OUTPUT", "Routing Logic") confunde más de lo que ayuda.
- **Plantilla objetivo por neurona:**
  ```markdown
  # NeuronName

  ## Qué hago
  <1-2 oraciones>

  ## Cuándo me activo
  <bullets concretos: triggers de input>

  ## Tags que emito
  <lista exacta con sintaxis>

  ## Ejemplo
  Input: "..."
  Output: <tag>...</tag>
  ```
- Dejar prosa larga solo en `Cortex.md` (es contexto de producto, no instrucción operativa).

### 2.2 Eliminar neurons con valor cuestionable
- **Candidatos:** `Engram` (promete "transparency policy" que el código no chequea — la info que expone ya está en el system prompt), `Insula` (un emoji de mood — útil pero ¿necesita 77 líneas?).
- **Decisión:** mantener si tienen tests A/B; remover si no movieron la calidad de respuesta.

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

### 3.5 Tag `<finding>` de primera clase
- **Archivos:** `src/domain/tags.rs` (extractor) + `src/app.rs::poll_stream` (acumulador)
- **Esfuerzo:** ~1-2 horas
- **Sintaxis:**
  ```xml
  <finding severity="high" file="src/app.rs:2702" category="security">
    Shell passthrough sin gate. Riesgo: rm -rf no confirmado.
    Fix: matchear comandos destructivos en execute_command y exigir confirm.
  </finding>
  ```
- **Comportamiento:** se acumulan en `app.findings: Vec<Finding>`, no se imprimen inline. Al `done` del stream, se renderizan como reporte estructurado al final del chat (TUI) o como bloque final en stdout (headless).

### 3.6 Tool `note` (memoria de trabajo)
- **Archivo:** `src/adapter/tools_native.rs`
- **Esfuerzo:** ~1 hora
- **Firma:** `note add "..."`, `note list`, `note clear`. Persiste a `/tmp/cognilite-notes-<session>.md`.
- **Integración:** las notas se inyectan al final del `full_system_prompt` en cada turno (igual que pinned files).
- **Por qué:** durante una revisión larga, el modelo pierde el hilo de hallazgos previos al pasar varios turnos. Esto le da una pizarra.

---

## Fase 4 — Mejoras de calidad (cuando haya tiempo)

No bloquean self-review, pero suben el techo del producto.

- **`save_config` con writer thread**: ya con debounce (0.3) está OK, pero un thread dedicado eliminaría jank percibido en config.
- **`new_session_id` con 32 bits**: pasar de 24 a 32 bits, comentar la birthday-bound (~65k conexiones).
- **Conexiones WS con límite**: hoy `server.rs` y `websocket.rs` spawnean thread por conexión sin tope. Agregar semaphore con default 64.
- **`extract_tool_call` skip de code fences post-`</think>`**: hoy `is_in_code_block` solo se aplica al slice después del `</think>`. Un fence abierto antes del `</think>` no se considera (rara pero posible).
- **Soporte Windows**: `clipboard.rs` ya tiene `#[cfg(windows)]` pero `apply_patch` depende del binario `patch` (Linux/macOS). Implementar fallback Rust o documentar como Linux/macOS-only.
- **Métricas opcionales**: `--metrics` que escriba a `/tmp/cognilite-metrics-<session>.json` con tok/s, prompt eval count, tool calls por turno. Útil para A/B de neurons.

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
