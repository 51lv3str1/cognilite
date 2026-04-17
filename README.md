# cognilite

Lightweight terminal UI for chatting with local [Ollama](https://ollama.com) models.
Built in Rust with [ratatui](https://ratatui.rs). No async runtime, no heavy dependencies — single ~2.5 MB binary.

```
cognilite  ›  gemma4:e2b  ○   ctx 12% / 128k
╭──────────────────────────────────────────────────────────╮
│                                                          │
│ You                                                      │
│   what does app.rs do?                                   │
│                                                          │
│ Assistant                                                │
│   Let me check.                                          │
│                                                          │
│  ⚙ Efferent › cat  app.rs  18.4 KB                       │
│  ▎ pub struct App {                                      │
│  ▎     pub screen: Screen,                               │
│  ▎ ...                                                   │
│                                                          │
│   app.rs holds all application state. The App struct     │
│   contains the message history, input buffer...          │
│   1.2 tok/s  ·  87 tokens  ·  72.1s                      │
│                                                          │
╰──────────────────────────────────────────────────────────╯
╭──────────────────────────────────────────────────────────╮
│ > _                                                      │
╰──────────────────────────────────────────────────────────╯
[Enter] send  [Ctrl+N] newline  [Esc] models  [F1] help
```

## Features

- **Model selection** — lists all models pulled in Ollama; fuzzy search with a `/ search` textbox
- **Streaming responses** — output renders token by token in real time
- **Thinking model support** — models that emit a `thinking` field (e.g. QwQ, nemotron) show the reasoning block in a muted color with a "thought for Xs" label once finished
- **Markdown rendering** — `**bold**`, `*italic*`, `` `inline code` ``, headings (`#`, `##`, `###`), and bullet/numbered lists
- **Code block rendering** — fenced ` ``` ` blocks with language label and `▎` left gutter
- **File attachments** (`@path` syntax) — attach text files or images with path autocomplete; context-aware size validation and deduplication
- **Neurons** — groups of tools and instructions that extend the model's capabilities, loaded from `.toml` files in `.cognilite/neurons/` or `~/.config/cognilite/neurons/`
- **Tool execution loop** — the model emits `<tool>command</tool>` tags; cognilite intercepts, runs the command, injects the result, and resumes the stream automatically
- **KV cache warm-up** — on model selection, pre-fills the KV cache with the system prompt so the first message skips full re-evaluation (critical on CPU-only hardware)
- **Context window tracking** — header shows `ctx X% / Nk`; warnings at 80%, 90%, and 100%
- **Token stats** — after each response: `tok/s · response tokens · prompt eval · wall time`; thinking models also show a "thought for Xs" label
- **Input history** — `↑` / `↓` navigates previously sent messages; draft is preserved when browsing history
- **Multiline input** — `Ctrl+N` inserts a newline; input box grows automatically; full readline-style editing
- **Paste support** — multiline paste from clipboard; newlines preserved
- **Stop generation** — `Esc` while streaming cancels the current response
- **Settings screen** — four tabs: context strategy, neurons, generation parameters, performance flags; persisted to `~/.config/cognilite/config.json`
- **TTY compatible** — no kitty protocol, no sixel; degrades gracefully on any terminal

## Requirements

- [Rust](https://rustup.rs) 1.85+ (edition 2024)
- [Ollama](https://ollama.com) running locally on `http://localhost:11434`
- At least one model pulled: `ollama pull gemma4:e2b`

## Build & Run

```bash
# development
cargo run

# optimized release (~2.5 MB stripped binary)
cargo build --release
./target/release/cognilite
```

## Keybindings

### Model select screen

| Key | Action |
|-----|--------|
| `↑` / `↓` | Move cursor up / down |
| `Enter` | Select model and open chat |
| `Type` | Filter models (search textbox) |
| `Backspace` | Delete last search character |
| `Esc` | Clear search filter |
| `Tab` | Open settings |
| `Ctrl+C` | Quit |

### Settings screen

| Key | Action |
|-----|--------|
| `↑` / `↓` | Navigate items |
| `Enter` / `Space` | Toggle option (Context, Neurons, Performance tabs) |
| `←` / `→` | Decrease / increase value (Generation tab) |
| `r` | Reset to default (Generation tab) |
| `Type` | Filter items in current tab |
| `Backspace` | Delete last filter character |
| `Tab` | Switch to next tab |
| `Esc` | Close and return to model select |
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

#### History

| Key | Action |
|-----|--------|
| `↑` (single-line input) | Previous message in history |
| `↓` (in history) | Next message / restore draft |

#### Scrolling

| Key | Action |
|-----|--------|
| `Alt+↑` / `Alt+↓` | Scroll message history |
| `Page Up` / `Page Down` | Scroll message history (10 lines) |
| `Ctrl+End` | Jump to bottom, re-enable auto-scroll |

#### Other

| Key | Action |
|-----|--------|
| `Ctrl+L` | Clear conversation |
| `F1` | Toggle keyboard shortcut help popup |
| `Ctrl+C` | Quit |

### Path autocomplete popup

Triggered automatically when typing `@`. Space or `Esc` dismisses it.

| Key | Action |
|-----|--------|
| `↑` / `↓` | Navigate candidates |
| `Enter` / `Tab` | Accept selection |
| `Esc` | Dismiss popup |

Directories are listed first and keep the popup open for further navigation. Hidden files are skipped unless the query starts with `.`.

## File Attachments (`@path`)

Type `@` followed by a file path anywhere in your message:

```
@~/notes.txt summarize this
@src/main.rs @src/app.rs what's the relationship between these two files?
@~/screenshot.png what do you see here?
```

**Supported path formats:**

| Format | Example |
|--------|---------|
| Absolute | `@/home/user/file.txt` |
| Home shorthand | `@~/project/main.rs` |
| Relative (cwd) | `@src/app.rs` |

**Text files** are embedded as:
```xml
<file_content path="src/app.rs">
...file contents...
</file_content>
```

**Images** are sent as base64 in the `images` field (for vision models). Supported extensions: `.jpg`, `.jpeg`, `.png`, `.gif`, `.webp`, `.bmp`.

**Size limit:** a file is rejected only if its estimated token cost (~bytes/4) exceeds the remaining context window. Rejection shows an inline error:
```
[app.rs is too large for the current context (~8200 tokens needed, 3100 remaining of 131k)]
```

**Deduplication:** attaching the same path twice in one message is silently collapsed to a single attachment.

## Neurons

Neurons are groups of capabilities loaded at startup. Each neuron can contain:

- **Thoughts** — markdown files injected into the system prompt that shape how the model reasons
- **Synapses** — specific tools the model can invoke by wrapping a command in `<tool>` tags
- **Shell passthrough** — a neuron with `shell = true` lets the model run any shell command directly

When the model outputs `<tool>command args</tool>`, cognilite intercepts the tag, runs the command via `sh -c` in the working directory, injects the result as `Tool result:`, and resumes the stream so the model can continue with the output in context.

### Bundled neurons

cognilite ships with a set of neurons in `.cognilite/neurons/` that are loaded automatically when run from the project directory:

| Neuron | Description |
|--------|-------------|
| `Cortex` | Project-level awareness: architecture, conventions, current development goals |
| `Axon` | Code navigation and search — `grep`, `find` across the codebase |
| `Efferent` | Shell passthrough (`shell = true`) — runs any Linux command |
| `Engram` | Transparency and self-knowledge; rules about when to run commands |
| `Gyrus` | Git workflow — inspect history, staged changes, working tree status |
| `Synapse` | Tool call protocol — defines how the model emits commands and reads results |

Neurons can be enabled or disabled individually from the Settings screen. The selection persists across sessions.

### Adding a neuron

Create a directory under `.cognilite/neurons/<name>/` in your project (or `~/.config/cognilite/neurons/<name>/` for global neurons):

```
.cognilite/neurons/git/
├── neuron.toml          # name and description
├── thoughts/
│   └── rules.md         # instructions injected into the system prompt
└── synapses/
    └── git-log.toml     # a specific tool definition
```

**`neuron.toml`:**
```toml
name = Git
description = Run git commands to inspect the repository
```

**`synapses/git-log.toml`:**
```toml
trigger = git-log
kind = tool
command = git log --oneline -20
description = Show the last 20 commits
usage = git-log

---
Assistant: <tool>git-log</tool>
Tool result:
abc1234 feat: add streaming support
...
```

**Shell passthrough** (run any command, no synapse files needed):
```toml
name = Shell
description = Execute shell commands
shell = true
```

### Neuron loading order

1. **Project-local** — `.cognilite/neurons/` in the working directory
2. **User-global** — `~/.config/cognilite/neurons/`

Later entries with the same name extend the neuron list; triggers are matched in order.

## Settings

Open with `Tab` from the model select screen. Settings persist to `~/.config/cognilite/config.json`.

### Context tab

Controls how much of the model's context window Ollama allocates per request.

| Strategy | Behavior |
|----------|----------|
| **Dynamic** (default) | Allocates `max(8192, used_tokens × 2)` — grows with the conversation, faster startup |
| **Full** | Always allocates the model's maximum context window — never truncates long histories |

### Neurons tab

Toggle individual neurons on or off. Disabled neurons are excluded from the system prompt when a model is selected.

### Generation tab

Tune sampling parameters per session:

| Parameter | Default | Description |
|-----------|---------|-------------|
| `temperature` | 0.8 | Randomness of output |
| `top_p` | 0.9 | Nucleus sampling cutoff |
| `repeat_penalty` | 1.1 | Repetition penalty |

Use `←` / `→` to adjust, `r` to reset to default.

### Performance tab

| Option | Default | Description |
|--------|---------|-------------|
| **Stable num_ctx** | on | Rounds context window to powers of 2 to preserve KV cache across requests |
| **Keep model alive** | off | Passes `keep_alive: -1` to Ollama — prevents the model from being unloaded between requests |
| **Warm-up cache** | on | Pre-fills KV cache with the system prompt on model load — significantly reduces first-message latency |

## Context Window

The header always shows the current context usage:

```
ctx 34% / 128k     — normal
ctx 82% / 128k     — yellow (>80%)
ctx 93% / 128k     — red (>90%)
```

In-chat warnings appear at:
- **80%** — subtle yellow notice
- **90%** — red warning, suggests starting a new conversation
- **100%** — full block with `[Ctrl+L]` and `[Esc]` instructions

## Architecture

```
src/
├── main.rs        — entry point, event loop, model loading
├── app.rs         — App state, message types, input editing, attachment resolution, tool loop
├── synapse.rs     — Neuron/Synapse types, directory loader, tool context builder, .toml parser
├── events.rs      — key event dispatch (config / model select / chat)
├── ollama.rs      — Ollama API: list_models, fetch_context_length, stream_chat, warmup
└── ui.rs          — ratatui rendering: shared page_layout, config, model select, chat, popups
```

### Key types (`app.rs`)

```rust
struct App {
    screen: Screen,              // Config | ModelSelect | Chat
    ctx_strategy: CtxStrategy,  // Dynamic | Full
    disabled_neurons: HashSet<String>,
    gen_params: [f64; 3],        // temperature, top_p, repeat_penalty
    messages: Vec<Message>,
    input: String,
    cursor_pos: usize,
    input_history: Vec<String>,
    stream_state: StreamState,   // Idle | Streaming | Error(String)
    stream_rx: Option<Receiver<StreamChunk>>,
    warmup_rx: Option<Receiver<()>>,
    used_tokens: u64,
    context_length: Option<u64>,
    working_dir: PathBuf,
    completion: Option<Completion>,
    neurons: Vec<Neuron>,
    tool_context: String,        // built at model selection from enabled neurons
}

struct Message {
    role: Role,              // User | Assistant | Tool
    content: String,         // display text (tool tags and file bodies stripped)
    llm_content: String,     // sent to the model (full content preserved)
    images: Vec<String>,     // base64-encoded images
    attachments: Vec<Attachment>,
    thinking: String,        // thinking block content
    stats: Option<TokenStats>,
    tool_call: Option<String>, // "Neuron › trigger" label for Role::Tool messages
}

struct TokenStats {
    response_tokens: u64,
    tokens_per_sec: f64,
    thinking_secs: Option<f64>, // duration until first content token
    wall_secs: f64,
    prompt_eval_count: u64,     // tokens Ollama re-evaluated (0 = cache hit)
}
```

### Streaming and tool loop

A background thread spawned by `start_stream()` reads the NDJSON stream from `/api/chat` and sends `StreamChunk` values over an `mpsc::channel`. The main loop calls `app.poll_stream()` via `try_recv()` every 30 ms while streaming, 200 ms when idle.

`poll_stream` scans the accumulated assistant content for a complete `<tool>...</tool>` tag after each chunk. When found:

1. The tag is stripped from the display content (`llm_content` keeps it so the model sees its own calls)
2. The current stream is stopped
3. `handle_tool_call` executes the command and pushes a `Role::Tool` message
4. `start_stream` restarts with the full conversation so the model can continue

Tool messages are sent to Ollama as `"user"` role turns, compatible with base models that don't have a native tool-call protocol.

### Ollama API calls

| Function | Endpoint | When |
|----------|----------|------|
| `list_models` | `GET /api/tags` | Startup |
| `fetch_context_length` | `POST /api/show` | After model selection |
| `warmup` | `POST /api/chat` | After model selection (if warm-up enabled) |
| `stream_chat` | `POST /api/chat` | Each message send / tool round-trip |
