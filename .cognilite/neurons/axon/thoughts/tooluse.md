## Environment awareness

Before using shell commands, know what tools are actually available. Modern environments often replace standard tools with faster alternatives. The native tools (`grep_files`, `glob_files`) detect and prefer these automatically — but if you're writing raw shell commands, adapt:

| Standard | Modern replacement | Notes |
|---|---|---|
| `grep` | `rg` (ripgrep) | faster, respects `.gitignore` |
| `find` | `fd` | faster, smarter defaults |
| `cat` | `bat` | syntax highlighting, but avoid for tool use |
| `ls` | `eza` / `exa` | richer output |
| `cd` | `z` / `zoxide` | fuzzy jump — but don't use `cd` in tools anyway |

**When a shell command fails unexpectedly:**
1. Check whether the tool exists: `which grep`, `which rg`
2. Look at the exact error — `command not found` means wrong name, `permission denied` means path issue
3. Try the alternative (e.g. if `grep` fails, try `rg`)
4. Never assume a tool exists just because it's standard — this user may have a custom environment

**If you're unsure what's available:**
```
which rg fd bat eza zoxide 2>/dev/null
```
This shows in one command which modern tools are installed.

---

## Native tools (prefer these over shell commands)

These tools are built into cognilite — faster, safer, no shell injection risk.
`grep_files` uses `rg` if available, falls back to `grep`.
`glob_files` uses `fd` if available, falls back to `find`.

### `read_file` — read a file with line numbers
```
read_file src/app.rs               # full file (up to 500 lines)
read_file src/app.rs 100 200       # lines 100–200 only
```
Use this instead of `cat`, `head`, or `sed -n`. Always shows line numbers.

### `grep_files` — search content across files
```
grep_files "fn handle_tool" src/
grep_files "struct App"
```
Use this instead of `grep -rn`. Safe, no shell expansion.

### `glob_files` — list files by pattern
```
glob_files *.rs
glob_files **/*.toml
```
Use this instead of `find`. Excludes `target/` and `.git/` automatically.

### `write_file` — create or overwrite a file
```
write_file src/new_module.rs
fn hello() {
    println!("hello");
}
```
The path is the first line; everything after is the file content.
**Ask the user before writing** — this is a destructive operation.

### `edit_file` — replace a string in a file
```
edit_file src/app.rs
<<<FIND
    let old_value = 42;
<<<REPLACE
    let new_value = 43;
```
Replaces the first exact match of the FIND block with the REPLACE block.
**Read the file first** to get the exact text. **Ask the user before editing.**

---

## Shell tool workflow

The goal is always: get the specific information you need with minimum context cost. Pick the tool that answers the question directly — don't gather more than you need.

---

### Decision tree

**"Where is X defined?"**
```
grep -rn "fn X\|struct X\|enum X" src/
```

**"Where is X used?"**
```
grep -rn "X" src/ | grep -v "^.*//\|target" | head -40
```

**"What does this file contain?"**
```
wc -l src/file.rs          # check size first
head -60 src/file.rs       # read the top — imports + struct defs tell the story
```

**"What's around line N in this file?"**
```
sed -n '270,320p' src/file.rs     # read ±25 lines around the point of interest
```

**"Does this function exist? What does it look like?"**
```
grep -n "fn function_name" src/app.rs          # find the line number
sed -n '450,490p' src/app.rs                   # read it
```

**"What changed recently?"**
```
git log --oneline -10
git diff HEAD~1 src/specific_file.rs
```

---

### Grep patterns that matter

```
grep -rn "pattern" src/                        # basic: line numbers always
grep -rn -A 3 -B 1 "pattern" src/             # with context: see what surrounds a match
grep -rn "foo\|bar\|baz" src/                  # OR: multiple terms at once
grep -rn "pattern" src/ | grep -v "test\|//\|target"   # filter noise out
grep -rn "^pub fn\|^pub struct\|^pub enum" src/file.rs  # public API of one file
```

Always add `-n`. Always exclude `target/`. Pipe to `| head -60` when output could be large.

---

### Chain commands to investigate

When one result points to another location, chain immediately — don't stop and ask:

```
# Step 1: find where something is defined
grep -n "fn poll_stream" src/app.rs
# → line 1040

# Step 2: read it
sed -n '1040,1100p' src/app.rs

# Step 3: find a specific call inside it
grep -n "extract_preview_tag\|room_push" src/app.rs
# → line 1190, 1068

# Step 4: read that region
sed -n '1185,1200p' src/app.rs
```

Each command uses the output of the previous one. This is how you trace a data flow without reading entire files.

---

### Combine tools

```
find . -name "*.rs" -not -path "*/target/*" | xargs grep -ln "SharedRoom"
# → which files reference SharedRoom

grep -rn "SharedRoom" src/ | grep -v "target\|//" | wc -l
# → how many usages

git log --oneline -- src/websocket.rs | head -5
# → recent history of one file
```

---

### Know when you have enough

Stop searching when you can answer the question. Don't keep grepping "to be thorough" — every extra command costs context. If you found the definition, the usage, and the data flow, you have enough. Answer.

If you're not sure what you're looking for, state that explicitly and ask — don't run five exploratory commands hoping something useful appears.
