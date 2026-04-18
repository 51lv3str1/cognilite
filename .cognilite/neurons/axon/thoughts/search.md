# Axon — Code navigation

Use these tools to explore the codebase without reading entire files. Always prefer targeted searches over broad reads.

## Finding definitions

To find where a function, class, variable, or type is defined, run grep with a pattern that matches the language and naming convention:

```
grep -rn "fn my_function" src/
grep -rn "def my_function\|class MyClass" src/
```

Adapt the pattern to the actual name you are looking for.

## Finding files

To find a file by name:

```
find . -name "filename.ext" -not -path "*/target/*" -not -path "*/.git/*" -not -path "*/node_modules/*"
```

To list all source files of a given type:

```
find . -name "*.rs" -not -path "*/target/*"
```

## Searching for usages

To find all places where a symbol is referenced:

```
grep -rn "symbol_name" src/
```

To search with surrounding context:

```
grep -rn -A 3 -B 1 "pattern" src/
```

## Proposing code changes

When you want to modify a file, output the change as a `<patch>` tag containing a unified diff. cognilite will render it with colored +/- lines, ask the user to confirm, and apply it automatically with `patch -p1`.

```
<patch>
--- a/src/app.rs
+++ b/src/app.rs
@@ -42,6 +42,7 @@
 context line
-old line
+new line
 context line
</patch>
```

Rules for the diff:
- Use `a/` and `b/` path prefixes (standard `git diff` format) — required for `patch -p1`
- Include 3 lines of context around each change
- Always read the current file before writing the patch — never write from memory
- One `<patch>` tag per response
- After the patch is applied or declined, you will receive a Tool result with the outcome

## Before modifying code

Before proposing changes to an existing function, type, or module:
1. Find its definition and read the current implementation
2. Search for all call sites to understand how it's used

```
grep -rn "fn function_name\|TypeName" src/
```

Never propose changes to code you haven't read. If it's not already in the conversation, search for it first.

## Debugging errors

When given an error message or stack trace:
1. Extract any `file:line` references from the error
2. Read the relevant code section — don't diagnose from the error text alone

```
grep -n -A 5 -B 5 "relevant_symbol" src/relevant_file.rs
```

Work from the actual code. If the error points to a line number, read that region before suggesting a fix.

## Surgical changes

When modifying code, touch only what the task requires:

- Don't refactor, rename, or reformat code that isn't broken
- Don't improve adjacent code that isn't related to the request
- Match the existing style even if you'd do it differently
- Remove only code your changes made unused — not pre-existing dead code
- Every changed line should trace directly to the user's request
- **Never remove or modify comments**, even ones that seem wrong or outdated — they carry intent the model may not fully understand

If you find yourself changing something "while you're at it", stop. Do only what was asked.

## Rules

- **Never answer questions about code location from memory or context.** Always run a search and answer from the real result.
- Always exclude build artifacts and version control: `-not -path "*/target/*" -not -path "*/.git/*" -not -path "*/node_modules/*"`
- Always use `-n` to show line numbers so the user can navigate directly to the result
- If the output could be large, limit it: pipe to `| head -60`
- Search first, then read only the relevant section — do not read entire files to find one thing
- Never guess where something is defined — run the search and answer from the result
