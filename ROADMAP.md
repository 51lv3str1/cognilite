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

### 1.1 Partir `app.rs` (3988 líneas → 8 archivos)
- **Esfuerzo:** ~3-4 horas, sin riesgo (solo `mod.rs` re-exports)
- **Layout propuesto:**
  ```
  src/app/
  ├── mod.rs          # struct App + new() + Screen + StreamState
  ├── config.rs       # Config, load_config, save_config, NeuronMode, presets
  ├── stream.rs       # poll_stream, start_stream, stop_stream
  ├── tags.rs         # extract_*, strip_*, is_in_code_block
  ├── pinned.rs       # PinnedFile, check_pinned_files, collect_pinned_diffs
  ├── picker.rs       # FilePicker, FilePanel, file_picker_*
  ├── input.rs        # input_* helpers (~200 líneas)
  ├── runtime.rs      # build_runtime_context, RuntimeMode
  └── raw_prompt.rs   # build_raw_prompt, TemplateFormat, detect_template_format
  ```
- **Beneficio para self-review:** cada archivo entra en una ventana de modelos 4-8B.

### 1.2 Unificar tag extraction/stripping
- **Archivos:** `src/app.rs:1182-1486` (poll_stream), `src/headless.rs:212-348`
- **Esfuerzo:** ~1 hora — elimina ~250 líneas duplicadas
- **API objetivo:**
  ```rust
  // tags.rs
  pub fn extract_tag(content: &str, tag: &str) -> Option<&str>;
  pub fn extract_self_closing(content: &str, tag: &str) -> Option<HashMap<String,String>>;
  pub fn strip_tag(content: &mut String, tag: &str);
  ```
- Las funciones `extract_ask_tag`, `extract_patch_tag`, `extract_mood_tag`, `extract_preview_tag`, `extract_load_neuron_tag` colapsan a uso del helper.
- El bloque `rfind("</think>") → find("<TAG>") → truncate` que se repite 6 veces queda como una sola función.

### 1.3 Tests de los contratos de parsing
- **Archivo nuevo:** `src/app/tags.rs` con `#[cfg(test)] mod tests`
- **Esfuerzo:** ~2 horas
- **Cobertura mínima:**
  - `extract_tool_call`: skip dentro de `<think>`, skip dentro de ` ``` ` fences, manejo de tags anidados
  - `build_raw_prompt`: 3 formatos × `last_is_assistant` true/false × system presente/ausente
  - `safe_print_boundary` (`headless.rs:408`): cortes en frontera de tag parcial
  - `is_mentioned` / `extract_mentions`: `#name`, `#name#id`, `#all`, separadores
- Estos son los contratos que rompen UX silenciosamente si cambian.

### 1.4 Reemplazar parsing manual de config por `serde::Deserialize`
- **Archivo:** `src/app.rs:160-207` (50 líneas de `val.get().and_then().unwrap_or()`)
- **Esfuerzo:** ~30 min
- **Cambio:** `#[derive(Deserialize)] struct ConfigFile` con `#[serde(default)]` por campo. Reduce a ~10 líneas y elimina inconsistencias entre `load`/`save`.

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

### 3.1 Neuron `Architect`
- **Path:** `.cognilite/neurons/architect/`
- **Esfuerzo:** ~1 hora
- **Contenido (versión densa):**
  ```markdown
  # Architect

  ## Qué hago
  Audito proyectos: estructura, deuda técnica, riesgos de seguridad, mejoras concretas.

  ## Protocolo
  1. <tool>tree</tool> para mapa
  2. read Cargo.toml/package.json/pyproject.toml
  3. read archivos por LOC desc, top 5
  4. emitir <finding severity="high|med|low" file="path:line">...</finding> por cada hallazgo
  5. al final: resumen ejecutivo con top-3 deudas y top-3 features
  ```

### 3.2 Templates `/review`, `/audit`, `/refactor`
- **Path:** `.cognilite/templates/`
- **Esfuerzo:** ~30 min
- **Estado actual:** los templates fueron borrados en `git status` — restaurarlos o reescribirlos desde cero apuntando a `Architect`.

### 3.3 Tool builtin `tree`
- **Archivo:** `src/tools.rs`
- **Esfuerzo:** ~1 hora
- **Firma:** `tree [path] [--depth N]`, respeta `.gitignore` si `fd` está disponible, fallback `find -maxdepth N`.
- **Output:** árbol jerárquico con LOC por archivo `.rs/.ts/.py/.go`.

### 3.4 Auto-inject `<project_map>` en `runtime_context`
- **Archivo:** `src/app.rs:3785` (`build_runtime_context`)
- **Esfuerzo:** ~1 hora
- **Cambio:** si el `working_dir` tiene `Cargo.toml`/`package.json`/`pyproject.toml`/`go.mod`, ejecutar el tool `tree` interno y embeber el resultado en el system prompt como `<project_map>...</project_map>`.
- **Beneficio:** el modelo arranca orientado, sin gastar 2-3 turnos en `glob_files`.

### 3.5 Tag `<finding>` de primera clase
- **Archivos:** `src/app/tags.rs` (extractor) + `src/app/stream.rs` (acumulador)
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
- **Archivo:** `src/tools.rs`
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

Semana 3:  Fase 3.1-3.4 (Architect + templates + tree + map)   → 1 día
           Fase 3.5 (tag <finding>)                            → ½ día
           Fase 3.6 (tool note)                                → ½ día

Validación: lanzar `cognilite --headless --preset architect "audita este repo"`
            sobre cognilite mismo. La salida esperada es un .md similar al que
            produjo esta revisión manual.
```

---

## Métrica de éxito

El roadmap está completo cuando:

```bash
cognilite --headless -m qwen3:8b --preset architect \
  "Audita este proyecto. Emití findings estructurados y un resumen ejecutivo."
```

produce un reporte cuya **calidad y cobertura ≥ 80%** del informe humano del 2026-04-25, sin intervención del usuario después del prompt inicial.
