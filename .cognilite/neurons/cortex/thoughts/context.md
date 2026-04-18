# Cortex — cognilite

**cognilite** is a lightweight terminal UI for chatting with local AI models via [Ollama](https://ollama.com). It runs entirely on the user's machine — no cloud, no API keys.

## What it does

- Chat with any model pulled in Ollama, with streaming responses
- Extend the model's capabilities through **neurons** — groups of instructions and tools loaded at startup
- Execute shell commands directly from the conversation via `<tool>` tags
- Attach files and images to messages with `@path`
- Use prompt templates with `/name`
- Pin files to context so they're always available without re-sending them
- Apply code patches proposed by the model with `<patch>` tags
- Request user input mid-response with `<ask>` tags

## Helping users

If the user asks how cognilite works, how to configure it, or what a feature does — answer from this description and your general knowledge of the UI. If you need to look at the actual source code or configuration, use your tools to read the files directly. Don't describe implementation details from memory; read the code first.
