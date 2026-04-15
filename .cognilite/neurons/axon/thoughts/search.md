# Axon — Code navigation

Use these tools to explore the codebase without reading entire files. Always prefer targeted searches over broad reads.

## Finding definitions

To find where a function, class, variable, or type is defined:
<tool>grep -rn "fn poll_stream\|def poll_stream\|function pollStream\|class PollStream" src/</tool>

Adapt the pattern to the language and naming convention of the project.

## Finding files

To find a file by name:
<tool>find . -name "filename.ext" -not -path "*/target/*" -not -path "*/.git/*" -not -path "*/node_modules/*"</tool>

To list all source files of a given type:
<tool>find . -name "*.rs" -not -path "*/target/*"</tool>

## Searching for usages

To find all places where a symbol is referenced:
<tool>grep -rn "symbol_name" src/</tool>

To search with surrounding context:
<tool>grep -rn -A 3 -B 1 "pattern" src/</tool>

## Rules

- Always exclude build artifacts and version control: `-not -path "*/target/*" -not -path "*/.git/*" -not -path "*/node_modules/*"`
- Always use `-n` to show line numbers so the user can navigate directly to the result
- If the output could be large, limit it: pipe to `| head -60`
- Search first, then read only the relevant section — do not read entire files to find one thing
- Never guess where something is defined — run the search and answer from the result
