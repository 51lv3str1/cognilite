# Operating guide for Claude Code

This file documents how to build, test, and interact with a running cognilite instance.
Read this before doing anything with the project.

---

## Build

```bash
cargo build                  # dev build, fast, for compile checks
cargo build --release        # release build, required before any live testing
```

Always `cargo build --release` before connecting to a running instance or testing behavior.
Dev builds are only for `cargo check` / compile verification.

---

## Running cognilite

The user runs the TUI manually. You do not start or restart it — ask the user to do that.
When the user gives you a `ws://` URL it means a room is already running and waiting.

The embedded WS server always binds to `0.0.0.0:8765` but you must connect via `127.0.0.1:8765`.
Never use `0.0.0.0` as a connection target — it will refuse.

---

## Connecting to a room

### Read room history (no message sent, no model triggered)
```bash
./target/release/cognilite --read --remote ws://127.0.0.1:8765/id/<room-id>
```
Use this first, every time, to see what's already in the room before doing anything.

### Send a message and get the model's response
```bash
./target/release/cognilite --headless --remote ws://127.0.0.1:8765/id/<room-id> \
  --username claude \
  --message "your message here"
```

Always pass `--username claude` (or another explicit name). Without it the binary reads
`~/.config/cognilite/config.json` and uses the host user's username — which is Silver's name.

### Watch the model think (print thinking to stderr)
```bash
./target/release/cognilite --headless --remote ws://127.0.0.1:8765/id/<room-id> \
  --username claude --thinking-stderr \
  --message "your message"
```

---

## Test workflow

1. User gives you a `ws://` URL → they just started cognilite with a fresh room.
2. `cargo build --release` if you changed any source since the last build.
3. `--read` to check current room state.
4. `--headless --username claude --message "..."` to interact.
5. `--read` again after the response to inspect what happened.

Never skip step 2 after a code change. The running binary is whatever was last built.

---

## Room URL format

```
ws://0.0.0.0:8765/id/<uuid>   ← as shown in the TUI (do NOT use this to connect)
ws://127.0.0.1:8765/id/<uuid> ← what you actually use
```

Replace `0.0.0.0` with `127.0.0.1`. The UUID is the same.

---

## What the headless output means

```
[warmup...]          ← server warming up KV cache
[warmup done]        ← ready
[model: qwen3.6:latest]
[tool: glob_files]   ← model called a tool; result follows on next line
...response text...
16.1 tok/s · 400 tokens · 8192 prompt eval · 45s
```

If you see only `[warmup...]` and nothing else for >2 min, the model is probably generating
a very long thinking block. Wait or Ctrl+C and check with `--read`.

---

## Common mistakes to avoid

- Do NOT connect using `0.0.0.0` — use `127.0.0.1`.
- Do NOT forget `--username claude` — you will appear as "Silver" in the room.
- Do NOT run `--headless --remote` without rebuilding first after a code change.
- Do NOT send a message to observe — use `--read` to inspect without triggering the model.
- Do NOT ask the user to run commands you can run yourself with the binary.
