# cognilite — Full Functionality Reference

Complete reference of every feature, flag, tag, tool, keybinding and protocol exposed by cognilite.
For a high-level overview and screenshot, see [README.md](README.md).
For evolution plan and design rationale, see [ROADMAP.md](ROADMAP.md).

---

## 1. Overview

cognilite is a terminal UI for chatting with local Ollama models. Single Rust binary, no async runtime, no cloud, no API keys. Works with any Ollama-served model (gemma, qwen, llama3, deepseek, mistral, etc.) thanks to a tag-interception protocol that does not require native tool-use APIs from the model.

The same binary covers five execution modes: interactive TUI, headless CLI, HTTP server, WebSocket server, and Remote TUI client.

---

## 2. Requirements

- Rust 1.85+ (edition 2024)
- [Ollama](https://ollama.com) running locally on `http://localhost:11434` (configurable via `--ollama-url` or `$OLLAMA_URL`)
- At least one Ollama model pulled (`ollama pull gemma3:e2b` or similar)
- External tools (auto-detected, fallback to alternatives if missing):
  - `rg` (ripgrep) — preferred for `grep_files`; falls back to `grep`
  - `fd` — preferred for `glob_files` and `tree`; falls back to `find`
  - `patch` — required for `<patch>` application (Linux/macOS only)
  - `diff` — used for pinned-file delta diffs

---

## 3. Build & run

```bash
# development
cargo run

# optimized release
cargo build --release
./target/release/cognilite
```

Release profile uses LTO, single codegen unit, opt-level=s, strip — yields a small static-ish binary.

---

## 4. Execution modes

### 4.1 Interactive TUI (default)

```bash
cognilite
```

Opens the model picker, then a chat screen with file picker, panel, history navigation, etc. Always starts an embedded WebSocket server in background so other users can join the room.

### 4.2 Headless CLI

```bash
cognilite --headless -m qwen3.6 "your prompt here"
cognilite --headless -m qwen3.6 < prompt.txt          # via stdin
cognilite --headless --preset hippocampus -y "$(cat .cognilite/templates/audit.md)"
```

Streams the response to stdout, errors and stats to stderr, exits when the model finishes. Supports tool calls, `<ask>` (reads from stdin), `<patch>` (auto-applied with `-y` or stdin confirm).

### 4.3 HTTP server

```bash
cognilite --server --port 8765
```

Exposes `POST /chat` that spawns a fresh `--headless --server-mode` per request. STDIN is serialized via a global mutex (one interactive `<ask>` at a time).

```bash
curl -X POST http://localhost:8765/chat \
  -H 'Content-Type: application/json' \
  -d '{"message":"hello","model":"qwen3.6","yes":true}'
```

### 4.4 WebSocket server (multi-user rooms)

Always started embedded when running the local TUI. Also runnable standalone via `--server` (combined with HTTP).

```
ws://host:8765/id/<room_uuid>
```

Each connection is a session within a room. Multiple clients in the same room see each other's messages and live tokens. Mention syntax `#username#sid` or `#all` triggers an auto-response from the AI model in that session.

### 4.5 Remote TUI

```bash
cognilite --remote ws://host:8765/id/<room_uuid>
```

Full TUI rendered locally; model and tools execute on the remote server. File picker browses remote `working_dir`. Supports all interactive features: `<ask>`, `<patch>` confirmation, file panel via `<preview>`, mood emoji, etc.

### 4.6 Read history (one-shot)

```bash
cognilite --read --remote ws://host:8765/id/<room_uuid>
```

Connects, prints the room transcript to stdout, exits.

---

## 5. CLI reference

### Global flags

| Flag | Description |
|---|---|
| `--ollama-url <url>` | Override Ollama base URL (default `http://localhost:11434`, env `OLLAMA_URL`) |

### Server mode (`--server`)

| Flag | Description |
|---|---|
| `--host <addr>` | Bind address (default `0.0.0.0`) |
| `--port <n>` | Port (default `8765`) |
| `--thinking` | Stream model thinking tokens to server stderr |

### Headless mode (`--headless`)

| Flag | Description |
|---|---|
| `--message <text>` / positional arg | The prompt (or piped via stdin) |
| `--model <name>` / `-m <name>` | Pick model (default = first in `ollama list`) |
| `--neuron-mode <mode>` | `manual` / `smart` / `presets` |
| `--preset <name>` | Activate a saved neuron preset (sets neuron-mode = presets) |
| `--no-neuron <name>` | Disable a specific neuron (repeatable) |
| `--temperature <f>` | Sampling temperature (default 0.8) |
| `--top-p <f>` | Nucleus sampling cutoff (default 0.9) |
| `--repeat-penalty <f>` | Repetition penalty (default 1.1) |
| `--ctx-strategy <s>` | `dynamic` / `full` |
| `--keep-alive` | Tell Ollama to keep the model loaded indefinitely (`keep_alive: -1`) |
| `--pin <path>` | Pin a file into system prompt (repeatable) |
| `--attach <path>` | Attach a file as `@path` in the user message (repeatable) |
| `--yes` / `-y` | Auto-accept `<ask type="confirm">`, `<patch>`, destructive shell |
| `--thinking` | Print thinking tokens to stdout (client receives them) |
| `--thinking-stderr` | Print thinking tokens to server stderr |
| `--metrics` | Append per-turn JSON to `/tmp/cognilite-metrics-<sid>.json` |
| `--username <name>` | Override identity for room labelling |

### Remote client (`--remote`)

| Flag | Description |
|---|---|
| `--remote <ws-url>` | Connect to a server. Path `/id/<uuid>` joins a specific room; anything else opens a fresh one. |

When combined with `--headless`: send one message to the room, stream the response to stdout, exit. When combined with `--read`: dump room transcript and exit.

---

## 6. Tag protocol

The model embeds XML-like tags in its response stream. cognilite intercepts them, takes action, and either feeds results back as a new turn or strips them from display. Tags inside `<think>`/`<thought>` blocks and inside triple-backtick code fences are **ignored** so the model can document tags without triggering them.

| Tag | Form | Effect |
|---|---|---|
| `<tool>` | `<tool>command</tool>` | Run `command`. Built-ins resolved first, then user-defined synapses, then shell passthrough (gated for destructive commands). Result is injected as a `Role::Tool` (user-role) message prefixed `[Tool output for: <command>]`. |
| `<ask>` | `<ask>question</ask>` | Pause stream, prompt user for free text. |
| `<ask type="confirm">` | `<ask type="confirm">go?</ask>` | Pause for yes/no. `--yes` auto-accepts. |
| `<ask type="choice">` | `<ask type="choice">a \| b \| c</ask>` | Pause for pick-one (cursor list). |
| `<patch>` | `<patch>unified diff</patch>` | Render colored diff, ask confirm, apply with `patch -p1 --batch` in `working_dir`. |
| `<mood>` | `<mood>EMOJI</mood>` | Strip from display, set `app.current_mood` shown next to model name in chat header. |
| `<preview>` | `<preview path="src/foo.rs"/>` | Open file panel with the path (self-closing). In WS mode the server reads and pushes the content. |
| `<load_neuron>` | `<load_neuron>Name</load_neuron>` | Smart-mode only: inject the neuron's `system_prompt` into the conversation as a `Role::Tool` message and resume streaming. |
| `<finding>` | `<finding severity="..." file="..." category="...">body</finding>` | Strip from display, accumulate. At stream done emit a consolidated `Role::Tool` markdown report. Attributes are optional; severity/category are conventionally `high`/`med`/`low` and `security`/`tech-debt`/`bug`/`perf`/`style`. |
| `<think>` / `<thought>` | `<think>reasoning</think>` | Rendered muted in TUI with "thought for Xs" label. Tags inside are ignored. Some models emit `<thought>` instead. |

---

## 7. Built-in tools

Registered in `runtime/tools.rs::execute_tool_call`. Bypass the destructive-shell gate because they have explicit semantics (the model declared the path; the user can see what's about to happen). Implementations live in `adapter/tools_native.rs`.

| Tool | Syntax | Caps |
|---|---|---|
| `read_file` | `<tool>read_file <path> [start] [end]</tool>` | Line-numbered output, max 500 lines per call. Pagination hint when truncated. |
| `write_file` | `<tool>write_file <path>\n<content></tool>` | Creates parent dirs. Reports bytes written. |
| `edit_file` | `<tool>edit_file <path>\n<<<FIND\n<old>\n<<<REPLACE\n<new></tool>` | Single replacement of exact-match block. Errors if `<old>` not found. |
| `grep_files` | `<tool>grep_files <pattern> [path]</tool>` | rg → grep fallback. Caps at 100 matches. Respects `.gitignore` with rg. |
| `glob_files` | `<tool>glob_files <pattern></tool>` | fd → find fallback. Sorted. Respects `.gitignore` with fd. |
| `tree` | `<tool>tree [<path>] [--depth=N]</tool>` | Indented tree with LOC counts for code files. Default depth 3. Cap 32KB. |
| `note` | `<tool>note add <text></tool>` / `<tool>note list</tool>` / `<tool>note clear</tool>` | Working-memory scratchpad keyed by `session_id`, persists to `/tmp/cognilite-notes-<sid>.md`. Injected into system prompt every turn. |

Shell passthrough kicks in only if the command does not match a built-in or a synapse trigger, AND a neuron with `shell = true` (e.g. `Efferent`) is loaded. Destructive commands (`rm`/`mv`/`dd`/`chmod`/`chown`/`shred`/`truncate`/`mkfs`/`rmdir`/`chgrp` and `git rm/mv/clean/reset/checkout`) gate behind a confirm prompt unless `auto_accept` is on.

Shell output is capped at 500 lines or 32KB (whichever first) with a truncation suffix telling the model how to fetch the rest.

---

## 8. Neurons

Markdown + tools loaded from `.cognilite/neurons/<name>/` (project-local) and `~/.config/cognilite/neurons/` (global). Each neuron is a directory with:

- `neuron.toml` — header `name = X`, `description = Y`, optional `shell = true` (passthrough). Body after `\n---\n` is a few-shot example.
- `thoughts/*.md` — concatenated alphabetically and merged into the system prompt when active.
- `synapses/*.toml` — optional. Defines custom tool triggers (`trigger = X`, `kind = tool`, `command = ...`, `usage = ...`, `description = ...`).

### Built-in neurons (project)

| Neuron | Purpose |
|---|---|
| `Cortex` | Product context: how cognilite works, tags, routing. Always-on. |
| `Synapse` | Universal `<tool>` interceptor docs — teaches the model the syntax. |
| `Efferent` | Decision routing + I/O safety. `shell = true` (passthrough). |
| `Thalamus` | Operational scope: built-in tools and bounded filesystem access. |
| `CingulateGate` | Hard-cap output when user gives numeric limit; force decisiveness. |
| `Insula` | Emit `<mood>` emoji to surface functional state. |
| `Hippocampus` | Project auditor: map → manifests → top-LOC files → `<finding>` + summary. Used by `/audit` and `/review` templates. |

### Three modes (config)

- **Manual** — toggle neurons on/off explicitly. All enabled neurons go into the initial system prompt.
- **Smart** — three-state per neuron: `disabled` → `initial` (always loaded) → `on-demand` (advertised in a manifest, model invokes via `<load_neuron>`). Toggle with Enter on each neuron in Settings.
- **Presets** — named sets of enabled neurons. Built-in `__pure__` disables all neurons. User can save the current selection as a preset.

---

## 9. Templates

Markdown files in `.cognilite/templates/` (project-local) and `~/.config/cognilite/templates/` (global). Trigger via `/<name>` at the start of the input (Tab to accept).

Built-in templates:

| Template | Purpose |
|---|---|
| `/review` | Architectural review. Loads Hippocampus, walks project structure, emits findings. |
| `/audit` | Security-focused audit. Same flow but biased to `category="security"` findings. |

Custom templates: drop `name.md` into either dir. The body becomes the user message (verbatim) when triggered.

---

## 10. Pinned files

Pinned files are always injected into the system prompt under a `## Pinned files` section. Tracked by mtime: when a file changes between turns, a unified diff is prepended to the next user message so the model sees what changed without paying the full re-read cost.

UI: `Ctrl+P` opens the file picker; navigate with arrows, Enter to pin/unpin, ← to go up, PgUp/PgDn for preview scroll, Esc to close. Pin status is indicated in the picker.

Headless: `--pin <path>` (repeatable).

---

## 11. Working memory (`note` tool)

Scratchpad scoped to the session. Model emits `<tool>note add <text></tool>` to remember things across turns within a long task. The notes file (`/tmp/cognilite-notes-<sid>.md`) is read fresh on every turn and inlined into the system prompt under `## Notes (working memory)`.

Stale on cognilite restart (session_id regenerates), `/tmp` is cleared on reboot. For cross-session memory you would need a different mechanism keyed to the project (not implemented).

---

## 12. Findings system

When the model emits `<finding>` tags during a stream, they are accumulated into `app.findings` and stripped from the visible content. At stream done, a consolidated `Role::Tool` message with a `## Findings (N)` markdown report is appended to the chat (or printed to stdout in headless mode). The report has `llm_content = ""` so the model does not re-see its own findings on the next turn (saves context).

`Finding` fields: `severity`, `file`, `category`, `body`. Markdown render:

```
- **[severity]** *category* `file`
  body line 1
  body line 2
```

---

## 13. TUI keybindings

### Global

| Key | Action |
|---|---|
| `F1` | Toggle help popup. While open: `j`/`k` or arrows to scroll, `Esc`/`q` to close. |
| `Ctrl+C` | Quit |

### Model select screen

| Key | Action |
|---|---|
| Type any character | Fuzzy filter |
| `↑` / `↓` | Navigate filtered list |
| `Enter` | Select model (start chat or send to remote server) |
| `Esc` | Clear search; if remote-connected, switch to local |
| `Tab` | Open settings |
| `Ctrl+R` | Open remote-connect screen |
| `Ctrl+J` | Open join-room dialog (paste UUID to join existing room on the configured remote) |

### Settings (Config) screen

| Key | Action |
|---|---|
| `Tab` | Cycle tabs: General → Context → Neurons → Features |
| `↑` / `↓` | Navigate within tab |
| `Enter` / `Space` | Activate / toggle / edit |
| Any char | Fuzzy filter within tab |
| `Backspace` | Edit search / fall back to reset value |
| `Esc` | Back to model select (auto-flushes config) |

Neurons tab adds: `←` / `→` to switch sub-section (Manual / Smart / Presets), `n` to create new preset, `d` / `Delete` to delete preset.

Features tab adds: `+`/`-`/`←`/`→` to adjust generation params; `r` to reset a slider.

### Chat screen

#### Editing (Input focus, default)

| Key | Action |
|---|---|
| `Enter` | Send |
| `Ctrl+N` | Insert newline |
| `Esc` | Stop streaming if in progress; else go back to model select |
| `↑` / `↓` | History prev/next (single-line) or move cursor (multi-line) |
| `Alt+↑` / `Alt+↓` | Scroll chat up/down |
| `Ctrl+A` / `Home` | Line start |
| `Ctrl+E` / `End` | Line end |
| `Ctrl+End` | Re-enable auto-scroll to bottom |
| `Ctrl+←` / `Alt+←` | Word left |
| `Ctrl+→` / `Alt+→` | Word right |
| `Ctrl+K` | Delete to end of line |
| `Ctrl+U` | Delete to start of line |
| `Ctrl+W` | Delete previous word |
| `Backspace` / `Delete` | Standard |
| `PageUp` / `PageDown` | Scroll chat by 10 lines |

#### Actions

| Key | Action |
|---|---|
| `Tab` | Enter History focus mode |
| `Shift+Tab` | Cycle Mode: normal → plan-only → auto-accept → normal |
| `Ctrl+T` | Cycle thinking-token budget (0/512/1024/2048/4096) |
| `Ctrl+Y` | Copy last assistant response to clipboard |
| `Ctrl+L` | Clear chat (keeps model selection) |
| `Ctrl+P` | Open file picker (pin/unpin) |
| `Ctrl+S` | Export chat to `cognilite_chat_<ts>.json` in working_dir |
| `Ctrl+O` | Open file picker to load a saved chat |
| `Ctrl+B` | Toggle file panel visibility |
| `Ctrl+J` | Show room share popup (URL + UUID) |

#### History focus (`Tab` from Input)

| Key | Action |
|---|---|
| `↑` / `↓` | Navigate message blocks |
| `Enter` | On Tool block with attachments: collapse/expand. On User block: cycle attachment in file panel. |
| `Ctrl+Y` | Copy selected block |
| `Tab` | Cycle to FilePanel focus (if visible) or back to Input |
| `Esc` | Back to Input |
| `q` | Close file panel |

#### File panel focus (`Tab` from History when panel visible)

| Key | Action |
|---|---|
| `PageUp` / `PageDown` | Vertical scroll |
| `←` / `→` | Horizontal scroll |
| `Tab` | Back to Input |
| `Esc` / `q` | Close panel |

#### File picker popup (`Ctrl+P` or `Ctrl+O`)

| Key | Action |
|---|---|
| Type any char | Fuzzy filter current directory |
| `↑` / `↓` | Navigate |
| `Enter` / `→` | Enter dir or pin/load file |
| `←` | Go up |
| `Backspace` | Clear search if active, else go up |
| `PageUp` / `PageDown` | Scroll preview |
| `Esc` | Close |

#### Ask widget (`<ask>` pause)

| Key | Action |
|---|---|
| Text mode | Type, `Enter` to submit, `Esc` cancel |
| Confirm mode | `y`/`Y`/`Enter` = Yes; `n`/`N`/`Esc` = No |
| Choice mode | `↑` / `↓` to pick, `Enter` to submit, `Esc` cancel |

#### Completion popup (typing `@` or `/` at line start)

| Key | Action |
|---|---|
| Type | Filter candidates |
| `Tab` / `Enter` | Accept selected candidate |
| `↑` / `↓` | Cycle |
| `Esc` | Dismiss |

---

## 14. Configuration file

Lives at `~/.config/cognilite/config.json`. Auto-loaded at startup; written on transitions away from the Settings screen, on quit, and on username edit. Keystroke-level changes (slider drags, etc.) are debounced via `config_dirty` flag.

```json
{
  "ctx_strategy": "dynamic",
  "disabled_neurons": ["Insula"],
  "on_demand_neurons": ["Hippocampus"],
  "temperature": 0.8,
  "top_p": 0.9,
  "repeat_penalty": 1.1,
  "thinking_budget": 0.0,
  "ctx_pow2": true,
  "keep_alive": false,
  "warmup": true,
  "thinking": true,
  "neuron_mode": "manual",
  "neuron_presets": [
    { "name": "audit", "enabled": ["Cortex", "Synapse", "Hippocampus", "Thalamus"] }
  ],
  "active_preset": null,
  "username": "lucas"
}
```

| Field | Type | Default | Notes |
|---|---|---|---|
| `ctx_strategy` | `"dynamic"` / `"full"` | `dynamic` | Dynamic = `max(8192, used*2)` clamped to model max; full = model's max context. |
| `disabled_neurons` | string[] | `[]` | Names matched by neuron loader. |
| `on_demand_neurons` | string[] | `[]` | Smart-mode: advertised as on-demand instead of initial. |
| `temperature` | f64 | 0.8 | Sampling. |
| `top_p` | f64 | 0.9 | Nucleus. |
| `repeat_penalty` | f64 | 1.1 | |
| `thinking_budget` | f64 | 0 | Max thinking tokens; 0 = unlimited. |
| `ctx_pow2` | bool | true | Round dynamic ctx to next power-of-two. |
| `keep_alive` | bool | false | Send `keep_alive: -1` to Ollama. |
| `warmup` | bool | true | Pre-fill KV cache with system prompt on model select. |
| `thinking` | bool | true | Enable Ollama's `think` field. |
| `neuron_mode` | `"manual"` / `"smart"` / `"presets"` | `manual` | |
| `neuron_presets` | preset[] | `[]` | Each `{ name, enabled: [neuronName, …] }`. |
| `active_preset` | string\|null | null | Active preset name; `__pure__` disables all neurons. |
| `username` | string | from `$USER` | Display name in room. |

Stale references (e.g. a neuron name no longer on disk) are silently ignored — safe to keep configs across reorganizations.

---

## 15. Multi-user rooms

Each TUI instance starts an embedded WS server on `0.0.0.0:8765` and creates a room with a UUID (visible via `Ctrl+J` in chat). Other users connect via:

```bash
cognilite --remote ws://<host>:8765/id/<uuid>
```

State per room: append-only message list, current generating-user identity, live-token stream from the active speaker. New joiners get the full history snapshot; subsequent messages are pushed via `room_update` frames.

Identity is `username#session_id` (8 hex chars). Two concurrent IDs per session: one for the human user, one for the model that runs in this session. Server retries on ID collision; 32-bit IDs give birthday-bound around 65k concurrent participants.

Mention syntax:

- `#username#sid` — addressed to that exact participant.
- `#username` (no sid) — informal, lowercased and not session-targeted.
- `#all` — broadcasts; every session in the room responds.

A session auto-responds when:
- A message has no mentions (broadcast), OR
- A mention matches the session's display identity, OR
- A mention matches the session's bare model name.

---

## 16. HTTP API

Single endpoint `POST /chat`. Accepts JSON body, streams plaintext response with `Transfer-Encoding: chunked`. Spawns a `--headless --server-mode` child process per request, serialized with a global stdin lock so interactive `<ask>` prompts on the server terminal are unambiguous.

```bash
curl -X POST http://localhost:8765/chat \
  -H 'Content-Type: application/json' \
  -d '{
    "message": "audit this project",
    "model": "qwen3.6",
    "preset": "hippocampus",
    "yes": true,
    "thinking": false,
    "temperature": 0.7,
    "ctx_strategy": "full",
    "no_neurons": ["Insula"],
    "pin": ["src/app.rs"],
    "attach": ["Cargo.toml"],
    "keep_alive": false
  }'
```

| Body field | Maps to |
|---|---|
| `message` | required, the prompt |
| `model` | `--model` |
| `neuron_mode` | `--neuron-mode` |
| `preset` | `--preset` |
| `ctx_strategy` | `--ctx-strategy` |
| `temperature` / `top_p` / `repeat_penalty` | corresponding flags |
| `no_neurons` | repeated `--no-neuron` |
| `pin` | repeated `--pin` |
| `attach` | repeated `--attach` |
| `yes` (bool) | `--yes` |
| `thinking` (bool) | `--thinking` |
| `keep_alive` (bool) | `--keep-alive` |

Connection limit: `MAX_CONNECTIONS = 64`. Excess clients receive `503 Service Unavailable` with `Retry-After: 5`.

CORS: `Access-Control-Allow-Origin: *` on responses.

---

## 17. WebSocket protocol

Subprotocol-free. Path `/id/<uuid>` joins or creates a room. Query params configure the session (same names as HTTP body, plus `client=tui` for full TUI clients).

Inline-implemented in `adapter/ws_server.rs` (~150 LoC of SHA-1, base64, RFC 6455 framing). No external WS crate.

### Client → server frames (JSON over text frames)

| `type` | Fields | Effect |
|---|---|---|
| `select_model` | `model` | Used by TUI clients (`client=tui` query) to pick a model after server lists them. |
| `message` | `content`, optional `attach: [paths…]` | Send a user message, run model, stream back. |
| `ask_response` | `content` | Reply to a server-initiated `ask` or patch confirmation. |
| `pin` / `unpin` | `path` | Manage pinned files server-side. |
| `ls` | `path` | Request directory listing (used by remote file picker). |
| `ping` | — | Heartbeat. |

### Server → client frames

| `type` | Fields |
|---|---|
| `models` | `entries: [{name, parameter_size, quantization_level, size_bytes}]` (sent only to TUI clients without preset model) |
| `connected` | `model`, `ctx`, `room_id`, `session_id`, `user_session_id`, `username` |
| `warmup_start` / `warmup_done` | — (KV cache priming) |
| `token` | `content` (assistant content stream) |
| `thinking_start` / `thinking` / `thinking_end` | reasoning tokens (only if `thinking=true`) |
| `tool` | `command`, `label`, `result` |
| `load_neuron` | `name` |
| `ask` | `kind` (text/confirm/choice), `question`, `options` |
| `patch` | `diff`, `question` |
| `mood` | `emoji` |
| `file_preview` | `path`, `content` |
| `ls_result` | `path`, `entries: [{name, is_dir}]` |
| `room_update` | `messages: [...]` (other users' completed turns) |
| `room_token` | `user`, `content` (live tokens from another participant) |
| `history` | `messages: [...]` (snapshot on join) |
| `done` | `stats: {tps, tokens, prompt_eval}` |
| `error` | `content` |
| `pinned` / `unpinned` | `path` (ack for pin/unpin) |
| `pong` | — |

---

## 18. Architecture overview

Hexagonal layout:

```
src/
├── main.rs                 # argv dispatch across the 5 modes
├── app.rs                  # struct App + state machine glue
├── domain/                 # pure model — no I/O, fully testable
│   ├── message.rs          # Role, Message, Attachment, TokenStats
│   ├── tags.rs             # extract_*, strip_tag, Finding, AskKind
│   ├── prompt.rs           # RuntimeMode, build_runtime_context, raw-prompt builder per template
│   ├── config.rs           # Config (serde Deserialize), CtxStrategy, NeuronMode, NeuronPreset
│   └── neuron.rs           # Neuron, Synapse, build_tool_context, load_from_dir
├── runtime/                # impl App by feature
│   ├── input.rs            # readline-style editing, completion (@/) 
│   ├── picker.rs           # FilePicker, FilePanel, syntect highlighting w/ mtime cache
│   ├── pinned.rs           # PinnedFile, mtime check, delta diff
│   ├── room.rs             # WS room sync helpers
│   └── tools.rs            # handle_tool_call, is_destructive_shell, execute_command, truncate_output
├── view/tui.rs             # ratatui rendering
└── adapter/                # I/O — every effect on the world
    ├── ollama.rs           # /api/{tags,show,chat,generate} — HTTP streaming via ureq
    ├── tools_native.rs     # built-ins (read/write/edit/grep/glob/tree/note/build_project_map)
    ├── headless_runner.rs  # CLI mode + safe_print_boundary + ask_interactive + write_metrics_json
    ├── http_server.rs      # POST /chat → spawn --headless --server-mode (STDIN_LOCK Mutex, 503 cap)
    ├── ws_server.rs        # WebSocket sessions, RoomRegistry, inline SHA-1 + base64
    ├── ws_client.rs        # connect, run_headless, run_read_history, frame parser
    ├── keyboard.rs         # crossterm key handling for all screens
    └── clipboard.rs        # cross-platform clipboard
```

Concurrency: `std::thread + mpsc::channel`. No async runtime. Every "background" work item (ollama stream, warmup, file highlight, remote model fetch, WS connect) runs in a dedicated thread and posts results via mpsc. The main loop polls 8 channels per frame at 30 ms (streaming) / 200 ms (idle).

Inline crypto: SHA-1 + base64 + WS framing in `adapter/ws_server.rs`, ~150 LoC. No `sha1`/`base64`/`tungstenite` crates. Zero supply-chain risk for the WS surface.

Raw-prompt continuation: after a `<tool>`, sending the history through `/api/chat` would close the assistant turn (Ollama re-applies the chat template). Workaround: detect template format via `/api/show` (`detect_template_format` recognizes ChatML / Llama3 / Gemma), build a raw prompt manually with the last assistant turn left open, and use `/api/generate` with `raw: true`. Implemented in `domain/prompt.rs::build_raw_prompt`.

KV warmup: on model select (or whenever the system prompt changes), pre-fire a 1-token request with the system prompt so subsequent messages skip full re-evaluation. Hashed by system-prompt content; toggling a neuron back to its previous state does not re-warm.

Dynamic context: `num_ctx = max(used_tokens * 2, message_tokens * 2, 8192)` rounded to next pow-of-2 (configurable), clamped to model max. Falls back to model max if `ctx_strategy = "full"`.

---

## 19. Logs and runtime artifacts

| Path | Purpose |
|---|---|
| `~/.config/cognilite/config.json` | Persistent settings |
| `~/.config/cognilite/neurons/` | Global custom neurons |
| `~/.config/cognilite/templates/` | Global custom templates |
| `<project>/.cognilite/neurons/` | Project-local neurons |
| `<project>/.cognilite/templates/` | Project-local templates |
| `/tmp/cognilite-notes-<sid>.md` | Working memory for the `note` tool (per session) |
| `/tmp/cognilite-metrics-<sid>.json` | Per-turn JSON-line metrics (with `--metrics`) |
| `/tmp/cognilite_patch.diff` | Transient buffer used during `<patch>` apply |
| `<working_dir>/cognilite_chat_<ts>.json` | Manual chat export (Ctrl+S) |

---

## 20. Design constraints (intentional non-goals)

These were validated during the architectural review (see ROADMAP.md "Lo que NO se va a tocar"):

- **No async runtime** (`std::thread + mpsc` is sufficient for a single-user TUI).
- **Inline crypto** for the WS surface (zero supply-chain risk).
- **Tag-interception** instead of native tool-use (works with any Ollama model).
- **Raw-prompt continuation** with `/api/generate raw: true` (necessary post-tool, even if hacky).
- **Pinned files with delta-diff and warmup hash** (KV cache reuse on CPU).

Linux/macOS-only at the moment: `<patch>` depends on the `patch` binary. Clipboard already has a Windows code path but `apply_patch` does not.
