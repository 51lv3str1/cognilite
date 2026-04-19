# cognilite

Terminal UI for chatting with local [Ollama](https://ollama.com) models.
Built in Rust with [ratatui](https://ratatui.rs). No async runtime, no cloud, no API keys.

```
cognilite  ›  gemma4:e2b  ○  😊  ctx 12% / 128k
╭──────────────────────────────────────────────────────────╮
│                                                          │
│ You                                                      │
│   @src/app.rs what does the streaming loop do?           │
│   [≡ app.rs  18.4KB]                                     │
│                                                          │
│ Assistant                                                │
│   Let me check.                                          │
│                                                          │
│  ⚙ Efferent › cat  src/app.rs  18.4 KB                   │
│  ▎ pub fn poll_stream(&mut self) {                       │
│  ▎ ...                                                   │
│                                                          │
│   poll_stream reads StreamChunk values from an mpsc      │
│   channel and scans for tool/ask/patch/mood tags...      │
│   1.2 tok/s  ·  87 tokens  ·  72.1s                      │
│                                                          │
╰──────────────────────────────────────────────────────────╯
╭──────────────────────────────────────────────────────────╮
│ > _                                                      │
╰──────────────────────────────────────────────────────────╯
[Enter] send  [Ctrl+N] newline  [Esc] models  [Tab] history  [F1] help
```

## Features

- **Model selection** — lists all models pulled in Ollama; fuzzy search with a `/ search` textbox
- **Streaming responses** — output renders token by token in real time
- **Thinking model support** — models that emit a `thinking` field (e.g. QwQ, nemotron) show the reasoning block in a muted color with a "thought for Xs" label once finished
- **Markdown rendering** — `**bold**`, `*italic*`, `` `inline code` ``, headings, and bullet/numbered lists
- **Code block rendering** — fenced ` ``` ` blocks with language label, `▎` left gutter, and full syntax highlighting via [syntect](https://github.com/trishume/syntect) + [two-face](https://github.com/CosmicHorrorDev/two-face) (same syntax assets as `bat` — TOML, Dockerfile, env files, and 200+ languages)
- **Diff rendering** — ` ```diff ` blocks render `+` lines in green and `-` lines in red with colored gutter symbols
- **File attachments** (`@path` syntax) — attach text files or images with path autocomplete; context-aware size validation and deduplication
- **File picker** (`Ctrl+P`) — directory-browser popup (oil.nvim style): navigate with arrows, `Enter`/`→` to enter dirs or pin files, `←` to go up; right panel shows a syntax-highlighted preview with `PgUp`/`PgDn` scroll; background highlighting with mtime cache keeps navigation snappy
- **Pinned files** — files pinned via the picker are injected into the system prompt on every request; mtime-checked each turn so the model always sees the current content; delta diffs are prepended when a file changes
- **File panel** — enter history mode (`Tab`), navigate to a message with file attachments, and press `Enter` to open a right-side syntax-highlighted viewer; `Tab` again focuses the panel (border turns highlighted); `PgUp`/`PgDn` scrolls whichever panel has focus; `Ctrl+B` hides/shows the panel without losing the loaded file; cycles through attachments, updates live when the file changes on disk (`↺` indicator); `q`/`Esc` closes
- **Prompt templates** (`/name` syntax) — type `/` at the start of the input to pick a saved prompt template from a fuzzy-searchable popup; templates live in `.cognilite/templates/` or `~/.config/cognilite/templates/`
- **Neurons** — markdown instructions and shell tools loaded from `.cognilite/neurons/` that extend the model's capabilities and shape its behavior
- **Tool execution loop** — model emits `<tool>command</tool>`; cognilite runs it, injects the result, and resumes the stream
- **Patch application** — model emits `<patch>unified diff</patch>`; cognilite renders the diff, asks for confirmation, and applies it with `patch -p1`
- **Model-driven user input** — model emits `<ask>`, `<ask type="confirm">`, or `<ask type="choice">` to pause and request input; cognilite shows the appropriate UI widget and injects the response
- **Mood reporting** — model emits `<mood>😊</mood>` to surface its functional state; the emoji appears in the chat header
- **KV cache warm-up** — pre-fills the KV cache with the system prompt on model selection so the first message skips full re-evaluation (critical on CPU-only hardware); deduplicates by hashing the system prompt so toggling a neuron twice doesn't re-warm needlessly
- **Context window tracking** — header shows `ctx X% / Nk`; color warnings at 80% and 90%
- **Token stats** — after each response: `tok/s · response tokens · prompt eval · wall time`
- **History mode** — `Tab` enters a block-navigation mode over the message list; `Ctrl+Y` copies the selected block; `Esc`/`Tab` returns to input
- **Input history** — `↑` / `↓` navigates previously sent messages; draft is preserved
- **Multiline input** — `Ctrl+N` inserts a newline; input box grows automatically; full readline-style editing
- **Paste support** — multiline paste from clipboard; newlines preserved
- **Stop generation** — `Esc` while streaming cancels the current response
- **Settings screen** — four tabs: context strategy, neurons, generation parameters, performance flags; persisted to `~/.config/cognilite/config.json`
- **Remote TUI mode** — connect to a remote cognilite server over WebSocket (`ws://host:port`) and use the full TUI as if the model were local: model selection, file picker browsing remote directories, `<preview>` tag opens files from the server in the local file panel, warmup spinner, all tag-driven widgets
- **F1 help popup** — keyboard shortcut reference available on all screens

## Requirements

- [Rust](https://rustup.rs) 1.85+ (edition 2024)
- [Ollama](https://ollama.com) running locally on `http://localhost:11434`
- At least one model pulled: `ollama pull gemma4:e2b`

## Build & Run

```bash
# development
cargo run

# optimized release
cargo build --release
./target/release/cognilite
```

## Headless mode

Run without the TUI — pipe input, script responses, or test the full tool/neuron loop from the shell.

```bash
# basic
cognilite --headless "show me the last 5 git commits"

# read message from stdin
echo "what files changed recently?" | cognilite --headless

# specify model
cognilite --headless --model qwen2.5:7b "explain this project"

# pin a file into context
cognilite --headless --pin src/app.rs "summarize the App struct"

# attach a file to the message
cognilite --headless --attach src/main.rs "what does this do?"

# use a neuron preset
cognilite --headless --preset MyPreset "refactor this"

# raw mode (no neurons)
cognilite --headless --neuron-mode presets --preset __pure__ "hello"

# override generation params
cognilite --headless --temperature 0.2 --top-p 0.9 "write a haiku"

# auto-confirm all <ask type="confirm"> prompts
cognilite --headless --yes "clean up the tmp files"

# stream thinking process to stdout
cognilite --headless --thinking "why is the sky blue?"
```

**Output:** response tokens stream to stdout; status messages (model, tool calls, neuron loads, stats) go to stderr. Exit code 0 on success, 1 on error.

All tags are handled the same as in the TUI: `<tool>` executes commands and restarts the stream, `<load_neuron>` injects on-demand neurons, `<patch>` is applied automatically, `<ask>` reads interactively from stdin (or auto-confirms with `--yes`).

### Headless flags

| Flag | Description |
|------|-------------|
| `--model <name>` | Model to use (default: first available) |
| `--neuron-mode <manual\|smart\|presets>` | Override neuron mode |
| `--preset <name>` | Activate a preset (implies presets mode) |
| `--no-neuron <name>` | Disable a neuron (repeatable; manual mode) |
| `--temperature <f>` | Override temperature |
| `--top-p <f>` | Override top_p |
| `--repeat-penalty <f>` | Override repeat_penalty |
| `--ctx-strategy <dynamic\|full>` | Context window strategy |
| `--keep-alive` | Keep model loaded after response |
| `--pin <path>` | Pin file into system prompt (repeatable) |
| `--attach <path>` | Attach file to the message (repeatable) |
| `--yes` / `-y` | Auto-confirm all `<ask type="confirm">` prompts |
| `--thinking` | Stream thinking content to stdout (before the response) |

## Server mode

Exposes cognilite as an HTTP server. Each `POST /chat` request spawns a headless session and streams the response back via chunked transfer encoding.

```bash
# default: listen on 0.0.0.0:8765
cognilite --server

# custom host and port
cognilite --server --host 127.0.0.1 --port 9000

# show thinking on the server terminal for every request
cognilite --server --thinking

# combine with a custom Ollama URL
cognilite --server --ollama-url http://192.168.1.10:11434
```

### Sending a request

```bash
curl -N -X POST http://localhost:8765/chat \
  -H "Content-Type: application/json" \
  -d '{"message": "list the files in src/ with line counts", "model": "gemma4:e2b"}'
```

The response streams as plain text. `-N` disables curl's output buffering so you see tokens as they arrive.

### JSON body fields

| Field | Type | Description |
|-------|------|-------------|
| `message` | string | **(required)** The user message |
| `model` | string | Model to use (default: first available) |
| `neuron_mode` | string | `manual`, `smart`, or `presets` |
| `preset` | string | Neuron preset name |
| `ctx_strategy` | string | `dynamic` or `full` |
| `temperature` | number | Override temperature |
| `top_p` | number | Override top_p |
| `repeat_penalty` | number | Override repeat_penalty |
| `no_neurons` | array | Neuron names to disable |
| `pin` | array | File paths to pin into the system prompt |
| `attach` | array | File paths to attach to the message |
| `keep_alive` | bool | Keep model loaded after response |
| `yes` | bool | Auto-confirm all `<ask type="confirm">` prompts |
| `thinking` | bool | Stream thinking content to the client (in the response body) |

### Thinking output

`--thinking` (server flag) and `"thinking": true` (per-request JSON) are independent:

| | Where thinking appears |
|---|---|
| `cognilite --server --thinking` | Server terminal (stderr), for all requests |
| `"thinking": true` in JSON | Client (response body), for that request only |

Both can be active at the same time.

Thinking is wrapped in `[thinking]` / `[/thinking]` markers:

```
[thinking]
The user wants to know...
[/thinking]

Here is the answer...
```

### Interactive `<ask>` prompts

When the model emits an `<ask>` tag, the prompt is shown on the **server terminal** and the operator types the response there. The client continues receiving the streamed output once the response is submitted.

If the request includes `"yes": true`, all confirmations are auto-accepted without operator input.

> HTTP chunked transfer is unidirectional — there is no way to send mid-stream input from the client side with the current protocol.

### Concurrent requests

Only one session can own the server terminal for interactive input at a time. Concurrent requests are queued and processed in order. Non-interactive requests (those that never trigger `<ask>`, or that use `"yes": true`) are not affected by this in practice.

## WebSocket mode

Connects a remote client for a full multi-turn chat session — the closest remote equivalent to the interactive TUI.

```bash
# start the server (WebSocket upgrade is handled automatically)
cognilite --server

# connect with websocat (install via brew or cargo)
websocat ws://localhost:8765/ws
```

The server upgrades any `GET /ws` request to a WebSocket connection. Each connection gets a dedicated `App` instance with its own conversation history, pinned files, and KV cache.

### Sending messages

Messages are JSON frames. The `type` field determines the action:

```json
{"type": "message", "content": "list the files in src/"}
```

With file attachments (same as `@path` syntax in TUI):
```json
{"type": "message", "content": "summarize this file", "attach": ["src/app.rs"]}
```

```json
{"type": "pin", "path": "src/app.rs"}
{"type": "unpin", "path": "src/app.rs"}
```

### Receiving frames

The server sends structured JSON frames back to the client:

| Frame type | When | Fields |
|------------|------|--------|
| `connected` | Session ready (after warmup) | `model`, `ctx` |
| `token` | Each streamed token | `content` |
| `thinking_start` | Thinking block begins | — |
| `thinking` | Thinking block content | `content` |
| `thinking_end` | Thinking block ends | — |
| `tool` | Tool executed | `command`, `label`, `result` |
| `load_neuron` | On-demand neuron injected | `name` |
| `ask` | Model needs input | `kind`, `question`, `options` |
| `patch` | Patch ready for apply | `diff` |
| `mood` | Mood update | `emoji` |
| `file_preview` | File content for viewer | `path`, `content` |
| `models` | Available model list (TUI client) | `entries[]` |
| `ls_result` | Directory listing (TUI client) | `path`, `entries[]` |
| `warmup_start` | KV cache pre-fill started | — |
| `warmup_done` | KV cache ready | — |
| `pinned` | File pinned | `path` |
| `unpinned` | File unpinned | `path` |
| `done` | Response finished | `stats.tps`, `stats.tokens`, `stats.prompt_eval` |
| `error` | Session error | `content` |

### Session query parameters

```
ws://localhost:8765/ws?model=qwen2.5:7b&thinking=true&yes=true&preset=MyPreset
```

| Parameter | Description |
|-----------|-------------|
| `model` | Model to use (omit for interactive model selection with `client=tui`) |
| `thinking` | Stream thinking blocks to the client |
| `yes` | Auto-confirm all `<ask type="confirm">` prompts |
| `preset` | Neuron preset name |
| `neuron_mode` | `manual`, `smart`, or `presets` |
| `client` | Set to `tui` to enable remote TUI mode (model selection, file picker, file preview) |

### Remote TUI mode

Pass `--remote ws://host:port` to connect the full cognilite TUI to a remote server instead of a local Ollama instance:

```bash
cognilite --remote ws://192.168.1.10:8765
```

The title bar shows the remote address. All TUI features are proxied through the WebSocket session:

- **Model selection** — server sends the available model list; TUI shows the same model select screen
- **File picker** (`Ctrl+P`) — sends `ls` requests to the server; browse and pin files on the remote host
- **`<preview>` tag** — model outputs `<preview path="..."/>`; server reads the file and sends a `file_preview` frame; TUI opens it in the local file panel
- **Warmup spinner** — `warmup_start`/`warmup_done` frames show the same progress bar as local mode
- **All tag-driven widgets** — `<ask>`, `<patch>`, `<mood>` work identically to local mode

The URL may be a bare `ws://host:port` (the `/ws` path is added automatically) or include a full path and query string.

### What makes WebSocket mode unique

Unlike a conventional chat API, the model in a WebSocket session runs on your own machine with full tool access:

- Executes real shell commands and injects results back into context
- Reads and writes files directly on the server
- Applies patches to the codebase
- `<ask>` prompts are delivered to the client as structured frames — the client sends back an `ask_response` frame and the session continues
- Pinned files and KV cache warmup work exactly as in the TUI
- The conversation persists across multiple turns until the connection closes

## Mode comparison

| Feature | TUI | Remote TUI | Headless | HTTP Server | WebSocket |
|---------|:---:|:----------:|:--------:|:-----------:|:---------:|
| Multi-turn conversation | ✓ | ✓ | — | — | ✓ |
| Tool execution (`<tool>`) | ✓ | ✓ (server) | ✓ | ✓ | ✓ |
| Patch application (`<patch>`) | ✓ | ✓ (server) | ✓ | ✓ | ✓ |
| Model-driven input (`<ask>`) | ✓ | ✓ | ✓ (stdin) | ✓ (server terminal) | ✓ (client frame) |
| Thinking output | ✓ (muted block) | ✓ (muted block) | ✓ (`--thinking`) | ✓ (`"thinking":true`) | ✓ (`?thinking=true`) |
| KV cache warmup | ✓ | ✓ (remote) | ✓ | ✓ | ✓ |
| Pinned files | ✓ | ✓ (remote) | ✓ (`--pin`) | ✓ (`"pin":[]`) | ✓ (`pin` frame) |
| File attachments (`@path`) | ✓ | ✓ | ✓ (`--attach`) | ✓ (`"attach":[]`) | ✓ (`"attach":[]` in frame) |
| Neuron/preset selection | ✓ | ✓ (remote) | ✓ | ✓ | ✓ |
| Auto-confirm (`--yes`) | — | — | ✓ | ✓ | ✓ |
| Runtime mode injected in system prompt | ✓ | ✓ | ✓ | ✓ | ✓ |
| Syntax-highlighted file picker | ✓ (local) | ✓ (remote) | — | — | — |
| File panel (attachment / preview viewer) | ✓ | ✓ | — | — | — |
| Markdown + code rendering | ✓ | ✓ | — | — | — |
| Mood indicator (`<mood>`) | ✓ | ✓ | — | — | — |
| Context window progress bar | ✓ | ✓ | — | — | — |
| Settings screen | ✓ | — | — | — | — |

## Keybindings

### Model select screen

| Key | Action |
|-----|--------|
| `↑` / `↓` | Move cursor |
| `Enter` | Select model and open chat |
| `Type` | Filter models |
| `Backspace` | Delete last search character |
| `Esc` | Clear search filter |
| `Tab` | Open settings |
| `F1` | Help popup |
| `Ctrl+C` | Quit |

### Settings screen

| Key | Action |
|-----|--------|
| `↑` / `↓` | Navigate items |
| `Enter` / `Space` | Toggle option (Context, Neurons, Performance tabs) |
| `←` / `→` | Decrease / increase value (Generation tab) |
| `r` | Reset to default (Generation tab) |
| `Type` | Filter items in current tab |
| `Tab` | Switch to next tab |
| `Esc` | Close and return to model select |
| `F1` | Help popup |
| `Ctrl+C` | Quit |

### Chat screen

#### Sending

| Key | Action |
|-----|--------|
| `Enter` | Send message |
| `Ctrl+N` | Insert newline |
| `Esc` (streaming) | Stop generation |
| `Esc` (idle) | Back to model select |

#### Cursor movement

| Key | Action |
|-----|--------|
| `←` / `→` | Move one character |
| `Ctrl+←` / `Alt+←` | Move one word left |
| `Ctrl+→` / `Alt+→` | Move one word right |
| `Ctrl+A` / `Home` | Beginning of line |
| `Ctrl+E` / `End` | End of line |
| `↑` / `↓` | Move between lines (multiline input) |

#### Editing

| Key | Action |
|-----|--------|
| `Backspace` / `Delete` | Delete character |
| `Ctrl+W` | Delete word before cursor |
| `Ctrl+K` | Delete to end of line |
| `Ctrl+U` | Delete to beginning of line |

#### History and scrolling

| Key | Action |
|-----|--------|
| `↑` (single-line input) | Previous sent message |
| `↓` (in history) | Next message / restore draft |
| `PgUp` / `PgDn` | Scroll message list |
| `Alt+↑` / `Alt+↓` | Scroll message list one line |
| `Ctrl+End` | Jump to bottom, re-enable auto-scroll |

#### Other

| Key | Action |
|-----|--------|
| `Tab` | Enter history mode |
| `Ctrl+B` | Hide / show file panel |
| `Ctrl+Y` | Copy last response (input) / copy selected block (history) |
| `Ctrl+L` | Clear conversation |
| `Ctrl+P` | Open file picker (pin files to context) |
| `F1` | Toggle keyboard shortcut help popup |
| `Ctrl+C` | Quit |

### History mode

Entered with `Tab` from the chat input. Highlights message blocks one at a time.

| Key | Action |
|-----|--------|
| `↑` / `↓` | Navigate message blocks |
| `Enter` | Open / cycle file attachments in the file panel |
| `PgUp` / `PgDn` | Scroll chat |
| `q` | Close file panel |
| `Ctrl+Y` | Copy selected block to clipboard |
| `Tab` | Focus file panel (when visible) |
| `Esc` | Return to input |

### File panel

| Key | Action |
|-----|--------|
| `PgUp` / `PgDn` | Scroll file panel |
| `Ctrl+B` | Hide / show panel (preserves loaded file) |
| `q` / `Esc` | Close file panel |
| `Tab` | Return to input |

### File picker (`Ctrl+P`)

| Key | Action |
|-----|--------|
| `↑` / `↓` | Navigate entries |
| `Enter` / `→` | Enter directory / toggle pin on file |
| `←` / `Backspace` | Go up one directory (clears filter first) |
| `PgUp` / `PgDn` | Scroll preview panel |
| `Type` | Filter entries in current directory |
| `Esc` | Close picker |

### Autocomplete popups

Both `@path` and `/template` popups share the same keys:

| Key | Action |
|-----|--------|
| `↑` / `↓` | Navigate candidates |
| `Enter` / `Tab` | Accept selection |
| `Esc` | Dismiss popup |

**`@path`** — triggered when typing `@` anywhere in the input. Completes file and directory paths relative to the working directory.

**`/template`** — triggered when typing `/` at the start of the input or a new line. Shows available prompt templates. Accepting a template replaces `/name` with the full template text.

## File Attachments (`@path`)

Type `@` followed by a file path anywhere in your message:

```
@~/notes.txt summarize this
@src/main.rs @src/app.rs what's the relationship between these files?
@~/screenshot.png what do you see here?
```

**Supported path formats:**

| Format | Example |
|--------|---------|
| Absolute | `@/home/user/file.txt` |
| Home shorthand | `@~/project/main.rs` |
| Relative (cwd) | `@src/app.rs` |

**Text files** are embedded as fenced `<file_content path="...">` blocks in the LLM turn.

**Images** are sent as base64 in the `images` field (for vision models). Supported: `.jpg`, `.jpeg`, `.png`, `.gif`, `.webp`, `.bmp`.

**Size limit:** rejected only if the estimated token cost exceeds the remaining context window.

**Deduplication:** attaching the same path twice is silently collapsed to one attachment.

## Pinned Files (`Ctrl+P`)

Pinned files are injected into the **system prompt** on every request, which means Ollama can reuse the KV cache across turns — the model doesn't re-evaluate them unless the content changes.

Open the picker with `Ctrl+P`. Navigate directories with arrows, `Enter`/`→` to descend, `←` to go up. Press `Enter` on a file to toggle its pin state. The right panel shows a syntax-highlighted preview; `PgUp`/`PgDn` scrolls it.

Pinned files appear as chips in the header bar. When a file changes on disk (detected by mtime each turn), a diff is prepended to the next message so the model sees exactly what changed without re-reading the whole file.

## File Panel

In history mode (`Tab`), navigate to a user message that has file attachments. Press `Enter` to open the file in a right-side panel (40% width). Pressing `Enter` again cycles through all text attachments in that message.

The panel shows syntax-highlighted content with line numbers (powered by the same syntax assets as `bat`). Press `Tab` again to focus the panel — its border turns highlighted and `PgUp`/`PgDn` scrolls the panel instead of the chat. `Tab` from the panel returns to the input.

Use `Ctrl+B` to hide or show the panel without losing the loaded file — the split collapses and the chat takes full width, then restores instantly when toggled back.

If the file changes on disk while the panel is open, it reloads automatically and shows a `↺` indicator for two seconds.

Press `q` or `Esc` (when panel is focused) to close. The active attachment chip in the message list is highlighted to show which file is currently open.

## Prompt Templates (`/name`)

Templates are markdown files in `.cognilite/templates/` or `~/.config/cognilite/templates/`. The filename (without `.md`) becomes the command name.

Type `/` at the start of the input to open the template popup. Typing more characters filters by prefix. Accepting a template replaces `/name` with the full file content so you can review and edit it before sending.

**Bundled templates:**

| Template | Purpose |
|----------|---------|
| `explain` | Explain what the code does and why |
| `refactor` | Simplify with minimal changes |
| `review` | Find bugs, edge cases, poor assumptions |
| `test` | Write tests for the happy path and edge cases |

To add a custom template, create any `.md` file in `.cognilite/templates/` and restart cognilite.

## Model-Driven UI (`<ask>` tags)

When the model needs input from the user, it emits an `<ask>` tag. cognilite intercepts it, shows the appropriate widget, and injects the response back into the conversation.

**Free text:**
```
<ask>What filename should be used?</ask>
```
The input box title shows the question. The user types and presses Enter.

**Yes/No confirmation:**
```
<ask type="confirm">Delete the 3 temporary files in /tmp?</ask>
```
User presses `y`/Enter for Yes or `Esc`/`n` for No.

**Multiple choice:**
```
<ask type="choice">Approach A: simple refactor|Approach B: full rewrite|Approach C: extract module</ask>
```
A selectable list appears. User navigates with `↑`/`↓` and confirms with Enter.

## Patch Application (`<patch>` tags)

The model can propose file edits as unified diffs wrapped in `<patch>` tags:

```
<patch>
--- a/src/app.rs
+++ b/src/app.rs
@@ -42,3 +42,4 @@
 context line
-old line
+new line
 context line
</patch>
```

When detected, cognilite renders the diff with colored `+`/`-` lines and shows a confirmation prompt. Confirming applies the patch with `patch -p1` in the working directory and injects the result into the conversation so the model can react to success or failure.

## Neurons

Neurons are markdown instructions and shell tools loaded at startup. Each neuron can contain:

- **Thoughts** — `.md` files injected into the system prompt
- **Synapses** — specific tool definitions (`.toml` files) the model can invoke
- **Shell passthrough** — `shell = true` lets the model run any shell command

When the model outputs `<tool>command args</tool>`, cognilite runs the command via `sh -c` in the working directory, injects the output as `Tool result:`, and restarts the stream.

### Bundled neurons

| Neuron | Description |
|--------|-------------|
| `Cortex` | Project-level awareness: what cognilite is and how to explore it |
| `Axon` | Code navigation — grep, find, read before modifying; `<patch>` tag usage |
| `Efferent` | Shell passthrough; simplicity and surgical-change rules; destructive command guard |
| `Engram` | Self-knowledge: real filesystem access, command execution, transparency |
| `Gyrus` | Git workflow — log, diff, blame, status |
| `Synapse` | Tool call protocol — how `<tool>` tags work |
| `Prefrontal` | Plan-first mode — restate → plan → confirm → execute; surface inconsistencies |
| `Afferent` | User input protocol — documents `<ask>`, `<ask type="confirm">`, `<ask type="choice">` |
| `Insula` | Mood reporting — documents `<mood>` tag for surfacing functional state |

Neurons are enabled or disabled individually in the Settings screen. The selection persists across sessions.

### Adding a neuron

Create a directory under `.cognilite/neurons/<name>/`:

```
.cognilite/neurons/my-neuron/
├── neuron.toml
├── thoughts/
│   └── rules.md
└── synapses/
    └── my-tool.toml
```

**`neuron.toml`:**
```toml
name = MyNeuron
description = What this neuron does
```

**`synapses/my-tool.toml`:**
```toml
trigger = my-tool
kind = tool
command = echo hello
description = A simple tool
usage = my-tool
```

**Shell passthrough** (no synapse files needed):
```toml
name = Shell
description = Execute shell commands
shell = true
```

### Neuron loading order

1. **Project-local** — `.cognilite/neurons/` in the working directory
2. **User-global** — `~/.config/cognilite/neurons/`

## Settings

Open with `Tab` from the model select screen. Persisted to `~/.config/cognilite/config.json`.

### Context tab

| Strategy | Behavior |
|----------|----------|
| **Dynamic** (default) | Allocates `max(8192, used_tokens × 2)` — grows with the conversation |
| **Full** | Always allocates the model's maximum context window |

### Neurons tab

Toggle individual neurons on or off. Disabled neurons are excluded from the system prompt. Each neuron shows an estimated token cost (`~Ntok`) so you can see which ones are heavy before enabling them.

### Generation tab

| Parameter | Default | Description |
|-----------|---------|-------------|
| `temperature` | 0.8 | Randomness of output |
| `top_p` | 0.9 | Nucleus sampling cutoff |
| `repeat_penalty` | 1.1 | Repetition penalty |

Use `←` / `→` to adjust, `r` to reset.

### Performance tab

| Option | Default | Description |
|--------|---------|-------------|
| **Stable num_ctx** | on | Rounds context to powers of 2 to preserve KV cache across requests |
| **Keep model alive** | off | Passes `keep_alive: -1` — prevents model unloading between requests |
| **Warm-up cache** | on | Pre-fills KV cache with the system prompt on model load |
| **Thinking** | on | Sends `"think": true` to Ollama — enables extended thinking for supported models (QwQ, Gemma 3, etc.) |

## Architecture

```
src/
├── main.rs        — entry point, event loop, model loading, CLI arg parsing (--remote flag)
├── app.rs         — App state, message types, input editing, tag interception, tool/patch/ask/mood loop,
│                    file picker/panel/pinned logic, highlight_code/highlight_file (syntect)
├── headless.rs    — headless mode: CLI arg struct, stream loop, stdin ask handler
├── server.rs      — HTTP server mode: TCP listener, per-connection handler, chunked streaming, argv builder; WebSocket upgrade routing
├── websocket.rs   — WebSocket server session: RFC 6455 handshake (SHA-1 inline), frame I/O, multi-turn stream loop,
│                    pin/unpin handling, ls/ls_result, models/select_model handshake, file_preview
├── ws_client.rs   — WebSocket client: TcpStream upgrade, masked frame I/O, frame type enum, background reader thread
├── synapse.rs     — Neuron/Synapse types, directory loader, tool context builder, .toml parser
├── events.rs      — key event dispatch (config / model select / remote connect / chat / history / ask / picker modes)
├── ollama.rs      — Ollama API: list_models, fetch_context_length, stream_chat (think param), warmup
├── clipboard.rs   — clipboard write (OSC 52 / pbcopy / wl-copy / xclip)
└── ui.rs          — ratatui rendering: config, model select, remote connect, chat, file picker, file panel, popups
```

```
.cognilite/
├── neurons/       — neuron directories (thoughts + synapses)
└── templates/     — prompt templates (name.md → /name command)
```

### Key types (`app.rs`)

```rust
struct App {
    screen: Screen,                 // Config | ModelSelect | RemoteConnect | Chat
    ctx_strategy: CtxStrategy,     // Dynamic | Full
    neuron_mode: NeuronMode,        // Manual | Smart | Presets
    // performance flags (all persisted to config.json)
    ctx_pow2: bool,                 // round num_ctx to powers of 2
    keep_alive: bool,               // pass keep_alive: -1 to Ollama
    warmup: bool,                   // pre-fill KV cache on model load
    thinking: bool,                 // send "think": true to Ollama
    messages: Vec<Message>,
    input: String,
    cursor_pos: usize,
    input_history: Vec<String>,
    stream_state: StreamState,      // Idle | Streaming | Error(String)
    stream_rx: Option<Receiver<StreamChunk>>,
    warmup_rx: Option<Receiver<()>>,
    ws_warmup_started_at: Option<Instant>, // warmup timer in remote TUI mode
    used_tokens: u64,
    context_length: Option<u64>,
    working_dir: PathBuf,
    completion: Option<Completion>, // @path or /template popup
    neurons: Vec<Neuron>,
    injected_neurons: HashSet<String>, // on-demand neurons loaded in this conversation
    templates: Vec<(String, String)>,
    chat_focus: ChatFocus,          // Input | History | FilePanel
    history_cursor: usize,
    ask: Option<InputRequest>,
    pending_patch: Option<String>,
    current_mood: Option<String>,
    pinned_files: Vec<PinnedFile>,
    file_picker: Option<FilePicker>,
    file_panel: Option<FilePanel>,
    highlight_cache: HashMap<PathBuf, (SystemTime, Vec<Line>)>,
    // remote WebSocket client
    ws_tx: Option<TcpStream>,
    ws_rx: Option<Receiver<WsClientFrame>>,
    remote_label: Option<String>,   // shown in title bar when connected remotely
}

enum AskKind        { Text, Confirm, Choice(Vec<String>) }
enum CompletionKind { Path, Template }
enum FilePickerEntry { Parent, Dir(String), File(String) }
```

### Tag interception in `poll_stream`

Each streaming chunk is accumulated into the last assistant message. After every chunk, `poll_stream` scans for complete tags and handles them in priority order:

| Tag | Action |
|-----|--------|
| `<tool>cmd</tool>` | Strip from display, run command, inject Tool result, restart stream |
| `<load_neuron>Name</load_neuron>` | Strip from display, inject neuron content as Tool message, restart stream |
| `<ask>...</ask>` | Strip from display, set `ask` state, stop stream, wait for user input |
| `<patch>diff</patch>` | Replace with rendered `diff` block, set `pending_patch`, show confirm, stop stream |
| `<mood>emoji</mood>` | Strip from display, update `current_mood`, **continue streaming** |
| `<preview path="..."/>` | Strip from display, read file, send `file_preview` frame (WS) or open panel directly (local), **continue streaming** |

`llm_content` is preserved intact for `<tool>` and `<ask>` tags so the model sees its own history. Tool messages are sent to Ollama as `"user"` role turns, compatible with base models that don't have a native tool-call protocol.

### Ollama API calls

| Function | Endpoint | When |
|----------|----------|------|
| `list_models` | `GET /api/tags` | Startup |
| `fetch_context_length` | `POST /api/show` | After model selection |
| `warmup` | `POST /api/chat` | After model selection (if enabled) |
| `stream_chat` | `POST /api/chat` | Each message send / tool round-trip |
