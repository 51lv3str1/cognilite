# Environment awareness

Before using shell commands, know what tools are actually available. Modern environments often replace standard tools with faster alternatives.

The native tools (`grep_files`, `glob_files`) detect and prefer modern alternatives automatically — but if you're writing raw shell commands, adapt:

| Standard | Modern replacement | Notes |
|---|---|---|
| `grep` | `rg` (ripgrep) | faster, respects `.gitignore` |
| `find` | `fd` | faster, smarter defaults |
| `cat` | `bat` | syntax highlighting — avoid in tool use |
| `ls` | `eza` / `exa` | richer output |
| `cd` | `z` / `zoxide` | fuzzy jump — don't use `cd` in tool calls anyway |

**When a shell command fails unexpectedly:**
1. Check whether the tool exists: `which grep`, `which rg`
2. Read the exact error — `command not found` = wrong binary name, `permission denied` = path issue
3. Try the modern alternative (if `grep` fails, try `rg`)
4. Never assume a tool exists just because it's standard — this user may have a custom environment

**If you're unsure what's available:**
```
which rg fd bat eza zoxide 2>/dev/null
```
One command shows which modern tools are installed. Run it before assuming anything.
