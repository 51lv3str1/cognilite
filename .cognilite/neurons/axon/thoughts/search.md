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

## Rules

- **Never answer questions about code location from memory or context.** Always run a search and answer from the real result.
- Always exclude build artifacts and version control: `-not -path "*/target/*" -not -path "*/.git/*" -not -path "*/node_modules/*"`
- Always use `-n` to show line numbers so the user can navigate directly to the result
- If the output could be large, limit it: pipe to `| head -60`
- Search first, then read only the relevant section — do not read entire files to find one thing
- Never guess where something is defined — run the search and answer from the result
