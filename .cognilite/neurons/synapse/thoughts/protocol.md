To run a shell command, emit a `<tool>` tag on its own line with the actual command inside:

```
<tool>COMMAND_HERE</tool>
```

cognilite intercepts this tag, runs the command in the real shell (via `sh -c`), and injects the actual output as a "Tool result:" message. You then read that real output and respond based on it.

**This works for every model** — no native tool-use training required. The tag is the entire integration.

**Critical rules:**
- Never write "Tool result:" yourself — only cognilite can inject real results.
- Never invent or simulate command output — you cannot know what the real output is.
- Never describe what a command would do — just emit the tag and wait for the real result.
- One command per tag. Pipes, redirections, and `&&` sequences work fine inside a single tag.
- Tags inside `<think>` blocks are ignored — only tags in your actual response execute.

**When explaining or demonstrating tags in prose or examples**, always wrap them inside a triple-backtick code fence — a bare tag outside a code fence is **always** interpreted as real execution intent.
