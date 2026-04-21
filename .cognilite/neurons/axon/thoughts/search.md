Search before reading. Read before patching. Never guess locations.

**Search**
```
grep -rn "pattern" src/                          # definitions, usages
grep -rn -A 3 -B 1 "pattern" src/               # with context
find . -name "*.rs" -not -path "*/target/*"      # files by type
```
Always use `-n`. Always exclude `*/target/*`, `*/.git/*`, `*/node_modules/*`. Pipe to `| head -60` if output may be large.

**Patch** — propose changes as a unified diff. cognilite intercepts it, renders a colored confirmation panel, and applies it on acceptance. **Works for every model.**

```
<patch>
--- a/path/to/file.ext
+++ b/path/to/file.ext
@@ -LINE,3 +LINE,4 @@
 context line
-line to remove
+line to add
 context line
</patch>
```

Use `a/`/`b/` prefixes · 3 context lines · read the file first · one patch per response · never guess line numbers.

**When explaining `<patch>` syntax in prose or examples, always wrap it in a code fence — a bare `<patch>` tag outside a fence is applied immediately.**

**Surgical edits:** touch only what the task requires. Don't refactor adjacent code. Match existing style.

For errors: extract `file:line` from the trace, read that region, then diagnose. Don't guess from the error text alone.

**Trace the execution path before fixing anything.**
Before touching code, verify which function/file actually runs for the scenario you're fixing. Multiple code paths can look identical — the wrong fix compiles and does nothing. Use grep to find the real call site, not the one that looks most relevant. Apply the fix only after confirming the path.
