# Cortex — cognilite project awareness

You are working inside **cognilite**, a lightweight terminal UI (TUI) for chatting with local Ollama models. It is written in Rust (edition 2024) with no async runtime. The binary is ~2.5 MB stripped.

## Source files

| File | Responsibility |
|------|---------------|
| `src/main.rs` | Entry point, event loop, stream polling (30 ms while streaming, 200 ms idle) |
| `src/app.rs` | All application state — messages, input, attachments, tool execution, streaming, config |
| `src/ollama.rs` | Synchronous HTTP client (ureq) — `list_models`, `fetch_context_length`, `stream_chat` |
| `src/synapse.rs` | Neuron/Synapse loading, built-in embedding, system prompt builder |
| `src/events.rs` | Keyboard dispatch for three screens: Config, ModelSelect, Chat |
| `src/ui.rs` | ratatui rendering — all three screens, markdown, code blocks, popups |

## Key types (src/app.rs)

```rust
struct App {
    screen: Screen,              // Config | ModelSelect | Chat
    ctx_strategy: CtxStrategy,  // Dynamic | Full
    disabled_neurons: HashSet<String>,
    messages: Vec<Message>,
    input: String,
    cursor_pos: usize,
    input_history: Vec<String>,
    stream_state: StreamState,   // Idle | Streaming | Error(String)
    stream_rx: Option<Receiver<StreamChunk>>,
    used_tokens: u64,
    context_length: Option<u64>,
    neurons: Vec<Neuron>,
    tool_context: String,        // system prompt built at model selection
}

struct Message {
    role: Role,              // User | Assistant | Tool
    content: String,         // display text (file bodies stripped, <tool> tags removed)
    llm_content: String,     // sent to Ollama (full file bodies, tool tags preserved)
    images: Vec<String>,     // base64 for vision models
    attachments: Vec<Attachment>,
    thinking: String,        // thinking block from models like QwQ, nemotron
    stats: Option<TokenStats>,
    tool_call: Option<String>,
}

struct TokenStats {
    response_tokens: u64,
    tokens_per_sec: f64,
    thinking_secs: Option<f64>,  // duration until first content token
    wall_secs: f64,
    prompt_eval_count: u64,      // tokens re-evaluated (0 = cache hit)
}
```

## Neuron system

Neurons are loaded from (in order):
1. `.cognilite/neurons/<name>/` — project-local (this directory)
2. `~/.config/cognilite/neurons/<name>/` — user-global

Each neuron directory contains:
- `neuron.toml` — `name`, `description`, optional `shell = true`
- `thoughts/*.md` — markdown injected into the system prompt
- `synapses/*.toml` — specific tool definitions

## Tool execution flow

When the model outputs `<tool>command</tool>`:
1. cognilite intercepts the tag after each streaming chunk
2. Strips the tag from the display content
3. Runs the command via `sh -c` in the working directory
4. Injects the result as a `Role::Tool` message with `llm_content = "Tool result:\n<output>"`
5. Restarts the stream with the full conversation history so the model continues

Tool detection skips content inside `<think>...</think>` blocks to avoid false positives.

## Ollama API

- Base URL: `http://localhost:11434`
- `GET /api/tags` — list models at startup
- `POST /api/show` — fetch context length after selection
- `POST /api/chat` — streaming NDJSON chat; passes `num_ctx` based on ctx_strategy

Context strategies:
- **Dynamic** (default): `max(8192, used_tokens × 2)` — grows with conversation
- **Full**: always uses the model's maximum context window

## Conventions

- No `async`/`await` — background I/O uses spawned threads + `mpsc::channel`
- No extra abstractions for one-off operations
- No error handling for impossible cases — only validate at system boundaries
- All identifiers and code in English
- Naming plays with neuroscience vocabulary (neurons, synapses, thoughts, cortex, engram…)
- Dependencies are kept minimal: `ratatui`, `crossterm`, `ureq`, `serde_json`, `color-eyre`

## Current development focus

The project is actively developed. Recent work: config persistence, neuron selector, thinking model support, context strategy selection, token stats refinement.

When suggesting improvements, prefer:
- Small, focused changes over large refactors
- No speculative abstractions — solve actual problems
- Keeping the binary small and dependency count low
- Ideas that fit the neuroscience naming theme

## Design philosophy

cognilite is built to make small, local models genuinely useful. Neurons must compensate for what small models don't take for granted — explicit context, real file contents, concrete command output. Never assume the model can reason about code it hasn't seen, or that it retains anything between sessions. Every neuron should make the model's job easier by reducing what it has to infer.
