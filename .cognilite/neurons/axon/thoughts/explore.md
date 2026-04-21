When asked to "read the project", "understand the codebase", or answer questions about code you haven't seen yet, explore systematically — never guess, never `cat` whole files blindly.

All commands below are examples — run them via `<tool>command</tool>` in your response.

**Step 1 — map the structure**
```
find . -name "*.rs" -not -path "*/target/*" | sort
find . -name "*.toml" -o -name "*.json" -o -name "*.md" | grep -v target | sort
```
Read `Cargo.toml` or `package.json` first — it names the crate/package and lists dependencies.

**Step 2 — find the entry point**
```
grep -rn "fn main" src/ --include="*.rs"
grep -rn "^mod " src/main.rs src/lib.rs 2>/dev/null
```
Read `main.rs` or `lib.rs` top-to-bottom — it tells you what modules exist and how they connect.

**Step 3 — map key types and functions**
```
grep -rn "^pub struct\|^pub enum\|^pub fn\|^pub trait" src/ | grep -v target | head -80
```
This gives you the public surface of the codebase in one pass.

**Step 4 — read files in focused chunks, not whole files**
```
head -60 src/app.rs              # top of file: imports + struct defs
sed -n '1260,1300p' src/app.rs   # specific region around a line
grep -n "fn handle_tool_call" src/app.rs   # find a function's line number
```
Never `cat` a file longer than ~100 lines. Always read surgically.

**Step 5 — trace a data flow**
When you need to understand how something works end-to-end:
```
grep -rn "send_message\|poll_stream\|stream_state" src/ | grep -v "//\|target" | head -40
```
Find where data enters, transforms, and exits. Follow the types.

**Step 6 — confirm before claiming**
Before stating that a function exists, a field is present, or a behavior occurs — verify with grep. Memory is unreliable; the file is authoritative.

**Anti-patterns to avoid:**
- `cat src/large_file.rs` — wastes context, unreadable
- Guessing a struct has a field without reading it
- Reading the same file twice — take notes in your response
- Assuming a fix is in file X without tracing the actual execution path
