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
- **Settings screen** — four tabs: general (username), context strategy, neurons, features (generation params + performance flags); persisted to `~/.config/cognilite/config.json`
- **Remote TUI mode** — connect to a remote cognilite server over WebSocket (`ws://host:port`) and use the full TUI as if the model were local: model selection, file picker browsing remote directories, `<preview>` tag opens files from the server in the local file panel, warmup spinner, all tag-driven widgets
- **Multi-user rooms** — the local TUI always starts an embedded WS server; press `Ctrl+J` in chat to see/copy the room share URL; other users connect via `--remote` and join the same room; messages and live tokens sync in real time; mention a participant with `#username#id` or `#all` to trigger an auto-response from the AI
- **Chat history export / import** — `Ctrl+S` saves the current conversation to a JSON file; `Ctrl+O` opens a file-picker to load a previously exported conversation
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