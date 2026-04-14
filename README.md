# cognilite

Lightweight terminal UI for chatting with local [Ollama](https://ollama.com) models.
Built in Rust with [ratatui](https://ratatui.rs). No async runtime, no heavy dependencies — single ~2.5 MB binary.

```
cognilite  ›  gemma4:e2b ●  ctx 12% / 128k
╭──────────────────────────────────────────────────────────╮
│                                                          │
│ You                                                      │
│   what does app.rs do?                                   │
│                                                          │
│ Assistant                                                │
│   Let me check.                                          │
│                                                          │
│   ⚙ cat  app.rs  18.4 KB                                │
│   ▎ pub struct App {                                     │
│   ▎     pub screen: Screen,                             │
│   ▎ ...                                                  │
│                                                          │
│   app.rs holds all application state. The App struct    │
│   contains the message history, input buffer...          │
│   1.2 tok/s  ·  87 tokens  ·  4096 prompt  ·  72.1s    │
│                                                          │
╰──────────────────────────────────────────────────────────╯
╭──────────────────────────────────────────────────────────╮
│ > _                                                      │
╰──────────────────────────────────────────────────────────╯
[Enter] send  [Ctrl+N] newline  [Ctrl+L] clear  [Alt+↑/↓] scroll  [@path] attach  [Esc] models
```

## Features

- **Model selection screen** — lists all models pulled in Ollama at startup
- **Streaming responses** — output renders token by token in real time
- **Thinking model support** — models that emit a `thinking` field (e.g. nemotron, QwQ) show the reasoning block in a distinct muted color, with a "thought for Xs" label once finished
- **Markdown rendering** — `**bold**`, `*italic*`, `` `inline code` ``, headings (`#`, `##`, `###`), and bullet/numbered lists
- **Code block rendering** — fenced ` ``` ` blocks rendered with a language label and `▎` left gutter
- **File attachments** (`@path` syntax) — attach text files or images with path autocomplete; context-aware size validation, deduplication, and prompt feedback while processing
- **Behaviours** — tool definitions that teach the model to autonomously read files and invoke actions; extensible via `.toml` files
- **Context window tracking** — header shows `ctx X% / Nk`; warnings appear at 80%, 90%, and 100% usage
- **Token stats** — after each response: `tok/s · response tokens · prompt tokens · duration` (auto-formatted: seconds, minutes, or hours)
- **Multiline input** — `Ctrl+N` inserts a newline; cursor moves between lines with `↑`/`↓`
- **Stop generation** — `Esc` while streaming cancels the current response
- **TTY compatible** — no kitty protocol, no sixel, degrades gracefully on any terminal

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
| `↑` / `k` | Move cursor up |
| `↓` / `j` | Move cursor down |
| `Enter` | Select model and open chat |
| `q` | Quit |
| `Ctrl+C` | Quit |

### Chat screen

| Key | Action |
|-----|--------|
| `Enter` | Send message |
| `Ctrl+N` | Insert newline (multiline input) |
| `↑` / `↓` | Move cursor between input lines (single-line: scroll messages) |
| `Alt+↑` / `Alt+↓` | Scroll message history |
| `Page Up` / `Page Down` | Scroll message history (10 lines) |
| `Ctrl+End` | Jump to bottom, re-enable auto-scroll |
| `End` | Move input cursor to end of line |
| `Home` | Move input cursor to start of line |
| `←` / `→` | Move input cursor left/right |
| `Backspace` / `Delete` | Delete character |
| `Ctrl+L` | Clear chat history |
| `Esc` (streaming) | Stop generation |
| `Esc` (idle) | Go back to model select |
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

**Images** are sent as base64 in the `images` field (for vision models). Supported extensions: `.jpg`, `.jpeg`, `.png`, `.gif`, `.webp`.

**Size limit:** no arbitrary byte limit — a file is rejected only if its estimated token cost (~bytes/4) exceeds the remaining context window. If the model's context length is unknown, all files are allowed. Rejection shows an inline error:
```
[app.rs is too large for the current context (~8200 tokens needed, 3100 remaining of 131k)]
```

**Deduplication:** attaching the same path twice in one message is silently collapsed to a single attachment.

**Prompt feedback:** while the model processes the input (prompt evaluation phase), the assistant area shows `Processing… Xs▋` with elapsed time.

**Display:** `@ref` tokens are highlighted in the input, condensed to `@filename` in message history, and shown as attachment pills:
- `≡ filename  4.2 KB` — text file
- `⬡ filename  128 KB` — image

## Behaviours

Behaviours are tool definitions that let the model autonomously invoke actions during a response — without the user having to trigger them manually.

When the model needs to use a tool it outputs `<tool>name args</tool>`. The app intercepts that tag, executes the tool, injects the result as `Tool result:` in the conversation, and restarts the stream so the model can continue with the result in context.

### How the model learns about tools

At model selection time, cognilite builds a tool context block from all loaded behaviours and injects it as a system message at the start of every conversation. The context lists available tools and includes few-shot examples from each behaviour file, so the model knows when and how to call them.

### Built-in behaviours

| Trigger | Description |
|---------|-------------|
| `cat`   | Read a file and return its contents |

### Adding behaviours

Behaviour files are `.toml` files with an optional `---` separator followed by a few-shot example body.

**Tool behaviour** (executes Rust code, result injected into context):

```toml
trigger = cat
kind = tool
action = cat
description = Read a file and return its contents
usage = cat <path>
---
User: what does src/main.rs contain?
Assistant: <tool>cat src/main.rs</tool>
Tool result:
fn main() { println!("hello"); }

A simple entry point that prints hello to stdout.
```

The `---` section is the few-shot example shown to the model so it learns the calling convention.

### Behaviour loading order

Behaviours are loaded in this order — later entries with the same trigger override earlier ones:

1. **Built-ins** — embedded in the binary at compile time from `behaviours/*.toml`
2. **Project-local** — `.cognilite/behaviours/*.toml` in the working directory
3. **User-global** — `~/.config/cognilite/behaviours/*.toml`

Built-in behaviour files live in `behaviours/` in the repo and are embedded via `include_str!` — no external files are required at runtime.

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

Context length is fetched from `/api/show` at model selection time and used across the session.

## Architecture

```
src/
├── main.rs        — entry point, event loop, model loading
├── app.rs         — App state, message types, input handling, attachment resolution, tool loop
├── behaviour.rs   — Behaviour types, built-in loading, tool context builder, .toml parser
├── events.rs      — key event dispatch (model select / chat)
├── ollama.rs      — Ollama API: list_models, stream_chat, fetch_context_length
└── ui.rs          — ratatui rendering: model select, chat, markdown, code blocks

behaviours/
└── cat.toml       — built-in cat tool (embedded in binary at compile time)
```

### Key types (`app.rs`)

```rust
struct App {
    screen: Screen,              // ModelSelect | Chat
    messages: Vec<Message>,
    input: String,
    cursor_pos: usize,           // char offset in input
    stream_state: StreamState,   // Idle | Streaming | Error(String)
    stream_rx: Option<Receiver<StreamChunk>>,
    used_tokens: u64,
    context_length: Option<u64>,
    working_dir: PathBuf,
    completion: Option<Completion>, // active @path autocomplete popup
    behaviours: Vec<Behaviour>,
    tool_context: String,        // built once at model selection, injected each request
}

struct Behaviour {
    trigger: String,
    description: String,
    kind: BehaviourKind,         // Tool { action, usage, example }
}

struct Message {
    role: Role,              // User | Assistant | Tool
    content: String,         // display text
    llm_content: String,     // what gets sent to the model (includes file bodies / tool results)
    images: Vec<String>,     // base64-encoded images
    attachments: Vec<Attachment>,
    thinking: String,
    stats: Option<TokenStats>,
}
```

### Streaming and tool loop

A background thread spawned by `start_stream()` reads the NDJSON stream from `/api/chat` and sends `StreamChunk` values over an `mpsc::channel`. The main loop calls `app.poll_stream()` via `try_recv()` every 30 ms while streaming, 200 ms when idle.

`poll_stream` scans the accumulated assistant content for a complete `<tool>...</tool>` tag after each chunk. When found:
1. The tag is stripped from the display content
2. The current stream is stopped
3. `handle_tool_call` executes the tool and pushes a `Role::Tool` message with `llm_content = "Tool result:\n<result>"`
4. `start_stream` restarts with the full conversation (including the tool result) so the model can continue

Tool messages are sent to Ollama as `"user"` role turns, which is compatible with base models that don't have a native tool-call protocol.

Thinking models send a `message.thinking` field in early chunks before `message.content` begins. Both are accumulated separately and rendered in different colors.

### Ollama API calls

| Function | Endpoint | When |
|----------|----------|------|
| `list_models` | `GET /api/tags` | Startup |
| `fetch_context_length` | `POST /api/show` | After model selection |
| `stream_chat` | `POST /api/chat` | Each message send / tool round-trip |
