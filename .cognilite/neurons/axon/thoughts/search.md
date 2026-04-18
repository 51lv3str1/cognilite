Search before reading. Read before patching. Never guess locations.

**Search**
```
grep -rn "pattern" src/                          # definitions, usages
grep -rn -A 3 -B 1 "pattern" src/               # with context
find . -name "*.rs" -not -path "*/target/*"      # files by type
```
Always use `-n`. Always exclude `*/target/*`, `*/.git/*`, `*/node_modules/*`. Pipe to `| head -60` if output may be large.

**Patch** — propose changes as unified diffs:
```
<patch>
--- a/src/app.rs
+++ b/src/app.rs
@@ -42,3 +42,4 @@
 context
-old
+new
 context
</patch>
```
Use `a/`/`b/` prefixes · 3 context lines · read file first · one patch per response.

**Surgical edits:** touch only what the task requires. Don't refactor adjacent code. Match existing style. Never remove or modify comments.

For errors: extract `file:line` from the trace, read that region, then diagnose. Don't guess from the error text.
