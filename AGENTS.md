# Operating guide for Claude Code

Read this before touching anything in the project.

---

## Build

```bash
cargo build                  # dev — compile check only
cargo build --release        # release — required before any live testing
```

Always rebuild release after any source change before testing with a running instance.
Dev builds are only for verifying the code compiles.

---

## Room URLs

The TUI binds on `0.0.0.0:8765`. You connect via `127.0.0.1:8765`.
The UUID in the path is the same — just swap the host.

```
User shows you:  ws://0.0.0.0:8765/id/<uuid>
You connect to:  ws://127.0.0.1:8765/id/<uuid>
```

The user starts cognilite and gives you the URL. You do not start or restart it.

---

## Standard test workflow

1. User gives you a `ws://` URL → room is live.
2. `cargo build --release` if you changed source since the last build.
3. Read the room history first (see below).
4. Send a message if needed.
5. Read again to inspect the result.

---

## Reading room history (no model triggered)

```bash
./target/release/cognilite --read --remote ws://127.0.0.1:8765/id/<uuid>
```

Use this to observe what's in the room without sending a message and without triggering
a model response. Always do this before sending anything.

---

## Headless mode — send one message, get one response

### Basic

```bash
./target/release/cognilite --headless \
  --remote ws://127.0.0.1:8765/id/<uuid> \
  --username claude \
  --message "your message"
```

`--username claude` is mandatory. Without it the binary reads `~/.config/cognilite/config.json`
(Silver's config) and you appear as "Silver" in the room.

### Read message from stdin (for long or multiline messages)

```bash
echo "your message" | ./target/release/cognilite --headless \
  --remote ws://127.0.0.1:8765/id/<uuid> \
  --username claude
```

If `--message` is omitted, stdin is read until EOF.

### Also print thinking blocks to stderr

```bash
./target/release/cognilite --headless \
  --remote ws://127.0.0.1:8765/id/<uuid> \
  --username claude \
  --thinking-stderr \
  --message "your message"
```

`--thinking-stderr` prints the model's `<think>` content to stderr so you can see reasoning
without it polluting the response on stdout.

### Select a specific model

```bash
./target/release/cognilite --headless \
  --remote ws://127.0.0.1:8765/id/<uuid> \
  --username claude \
  --model qwen3.6:latest \
  --message "your message"
```

Without `--model`, the server uses whichever model is already selected in the running TUI.

### Attach files to the message

```bash
./target/release/cognilite --headless \
  --remote ws://127.0.0.1:8765/id/<uuid> \
  --username claude \
  --attach src/app.rs \
  --message "explain this file"
```

`--attach` inlines the file content into the message (same as `@path` in the TUI).
Multiple `--attach` flags are accepted.

### Auto-confirm patch/ask prompts

```bash
./target/release/cognilite --headless \
  --remote ws://127.0.0.1:8765/id/<uuid> \
  --username claude \
  --yes \
  --message "apply the fix"
```

`--yes` / `-y` answers Yes to all `<ask>` and `<patch>` confirmations automatically.

### Neuron control

```bash
# Use smart mode (on-demand neuron loading)
--neuron-mode smart

# Use a preset
--preset programmer

# Disable a specific neuron
--no-neuron Parietal
```

### Generation parameters

```bash
--temperature 0.5
--top-p 0.9
--repeat-penalty 1.1
--ctx-strategy full      # full context window (vs dynamic, the default)
--keep-alive             # keep model loaded in Ollama after response
```

---

## Headless mode — local Ollama (no room)

If you don't pass `--remote`, headless talks directly to local Ollama without a room:

```bash
./target/release/cognilite --headless \
  --model qwen3.6:latest \
  --message "your message"
```

Useful for quick model queries independent of any running TUI session.

---

## Reading headless output

```
[warmup...]                   model warming up KV cache
[warmup done]                 ready to stream
[model: qwen3.6:latest]       which model is active
[tool: glob_files]            model called a tool; result on next line
<response text on stdout>
16.1 tok/s · 400 tokens · 8192 prompt eval · 45s   (on stderr)
```

Exit code 0 = success. Exit code 1 = connection error, model not found, or aborted.

If output stops after `[warmup...]` for more than ~2 minutes, the model is generating a long
thinking block. Use `--read` in another terminal to watch the room, or Ctrl+C and investigate.

---

## Common mistakes

- Using `0.0.0.0` as connection host → connection refused. Always use `127.0.0.1`.
- Forgetting `--username claude` → you appear as "Silver" in the room.
- Testing with a stale binary after a code change → rebuild release first.
- Sending a message to observe the room → use `--read` instead.
- Using `--headless` without `--remote` when you meant to join a room → goes to local Ollama.
