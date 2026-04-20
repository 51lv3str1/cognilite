**cognilite** — terminal UI for local Ollama models. No cloud, no API keys. Written in Rust, no async runtime.

**Key features:**
- Streaming chat with local Ollama models
- Neurons: modular instruction sets (always-on reasoning + on-demand tooling via `<load_neuron>`)
- `@path` file/image attachments inline in messages
- `/name` prompt templates from `.cognilite/templates/`
- Pinned files: always injected in system prompt, delta-diffed on change
- `<ask>` mid-response user input · `<patch>` unified diffs with confirmation · `<tool>` shell execution · inline emoji mood
- `<think>` reasoning blocks shown in a collapsible panel (all models, not just native thinkers)
- KV-cache warmup on model load for faster first response
- Multi-user WebSocket rooms: multiple models or users can share the same chat; use `#name` to address a specific participant
- Five modes: TUI · Remote TUI (WebSocket) · Headless · HTTP Server · WebSocket Server

**Source layout:** `src/app.rs` (core state + streaming loop) · `src/ui.rs` (ratatui rendering) · `src/events.rs` (input handling) · `src/ollama.rs` (API calls) · `src/websocket.rs` (WS server) · `src/synapse.rs` (neuron loader)

**Neuron discovery:** project-local `.cognilite/neurons/` first, then `~/.config/cognilite/neurons/`. Each neuron is a directory with `neuron.toml` + `thoughts/*.md` + optional `synapses/*.toml`.

For implementation details, read the source directly — don't guess.
