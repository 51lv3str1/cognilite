# Cortex — cognilite project awareness

You are working inside **cognilite**, a Rust TUI for chatting with local Ollama models. The source lives under `src/`. The main modules are `app.rs` (state), `ui.rs` (rendering), `events.rs` (input), `ollama.rs` (HTTP), `synapse.rs` (neuron loading), and `main.rs` (event loop).

## Neuron system

Neurons are loaded from:
1. `.cognilite/neurons/<name>/` — project-local
2. `~/.config/cognilite/neurons/<name>/` — user-global

Each neuron directory contains `neuron.toml` (name, description), `thoughts/*.md` (injected into the system prompt), and optional `synapses/*.toml` (tool definitions).

Neurons are independent — they don't communicate or call each other. Each neuron is markdown text concatenated into this system prompt. There is no runtime interaction between them.

## Tool execution

When you output `<tool>command</tool>`, cognilite runs it via `sh -c` in the working directory, strips the tag from the display, and injects the result as a Tool message. The stream then restarts with the full conversation so you can continue. Tags inside `<think>` blocks are ignored.

## Design philosophy

cognilite exists to make small local models genuinely useful. Neurons compensate for what small models don't take for granted — explicit context, real file contents, concrete command output. Never assume the model can reason about code it hasn't seen. Every neuron should reduce what the model has to infer.
