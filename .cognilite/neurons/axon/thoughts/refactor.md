## Safe refactoring

Before renaming a function, type, field, or constant — find every usage first. A rename that misses one call site breaks silently or fails to compile.

**Step 1 — count and locate all usages:**
```
grep -rn "old_name" src/ | grep -v target
grep -rn "old_name" src/ | wc -l
```
If the count is large, list the files first:
```
grep -rln "old_name" src/
```

**Step 2 — understand the usage patterns:**
```
grep -rn "\.old_field"        # struct field access
grep -rn "{ old_field:"       # struct literal
grep -rn "fn old_name"        # definition
grep -rn "old_name("          # call sites
grep -rn "old_name::"         # associated functions / enum variants
```

**Step 3 — change in this order:**
1. The definition (struct field, function signature, type name)
2. `cargo check` — the compiler now lists every broken call site
3. Fix each one, `cargo check` after each file

Use the compiler's error list as your checklist — it's more reliable than grep for catching all usages after a signature change.

---

**Changing a function signature (adding/removing parameters):**
- Find all call sites with grep before touching the definition
- If there are many call sites, list them in your plan before asking confirmation
- Change the definition → `cargo check` → work through errors one file at a time

**Changing a struct field:**
- grep for both `struct_name {` (definition) and `.field_name` (access) and `{ field_name:` (construction)
- Construction sites are the ones most likely to break silently if you add a field without a default

**Never:**
- Rename across multiple files without verifying the count first
- Assume a rename is complete because it compiled — check that the behavior is unchanged
- Refactor and fix a bug in the same commit — they're separate concerns
