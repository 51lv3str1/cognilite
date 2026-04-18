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

## Architecture

```
src/
├── main.rs        — entry point, event loop, model loading
├── app.rs         — App state, message types, input editing, tag interception, tool/patch/ask/mood loop,
│                    file picker/panel/pinned logic, highlight_code/highlight_file (syntect)
├── synapse.rs     — Neuron/Synapse types, directory loader, tool context builder, .toml parser
├── events.rs      — key event dispatch (config / model select / chat / history / ask / picker modes)
├── ollama.rs      — Ollama API: list_models, fetch_context_length, stream_chat, warmup
└── ui.rs          — ratatui rendering: config, model select, chat, file picker, file panel, popups
```

```
.cognilite/
├── neurons/       — neuron directories (thoughts + synapses)
└── templates/     — prompt templates (name.md → /name command)
```

### Key types (`app.rs`)

```rust
struct App {
    screen: Screen,                 // Config | ModelSelect | Chat
    ctx_strategy: CtxStrategy,     // Dynamic | Full
    messages: Vec<Message>,
    input: String,
    cursor_pos: usize,
    input_history: Vec<String>,
    stream_state: StreamState,      // Idle | Streaming | Error(String)
    stream_rx: Option<Receiver<StreamChunk>>,
    warmup_rx: Option<Receiver<()>>,
    used_tokens: u64,
    context_length: Option<u64>,
    working_dir: PathBuf,
    completion: Option<Completion>, // @path or /template popup
    neurons: Vec<Neuron>,
    templates: Vec<(String, String)>,
    tool_context: String,
    chat_focus: ChatFocus,          // Input | History | FilePanel
    history_cursor: usize,
    ask: Option<InputRequest>,
    pending_patch: Option<String>,
    current_mood: Option<String>,
    pinned_files: Vec<PinnedFile>,
    file_picker: Option<FilePicker>,
    file_panel: Option<FilePanel>,
    highlight_cache: HashMap<PathBuf, (SystemTime, Vec<Line>)>,
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
| `<ask>...</ask>` | Strip from display, set `ask` state, stop stream, wait for user input |
| `<patch>diff</patch>` | Replace with rendered `diff` block, set `pending_patch`, show confirm, stop stream |
| `<mood>emoji</mood>` | Strip from display, update `current_mood`, **continue streaming** |

`llm_content` is preserved intact for `<tool>` and `<ask>` tags so the model sees its own history. Tool messages are sent to Ollama as `"user"` role turns, compatible with base models that don't have a native tool-call protocol.

### Ollama API calls

| Function | Endpoint | When |
|----------|----------|------|
| `list_models` | `GET /api/tags` | Startup |
| `fetch_context_length` | `POST /api/show` | After model selection |
| `warmup` | `POST /api/chat` | After model selection (if enabled) |
| `stream_chat` | `POST /api/chat` | Each message send / tool round-trip |
