## Diagnosing compile errors

**Read the first error only.** Cargo prints errors bottom-up and later errors are often cascading noise from the first one. Fix the first, rebuild, repeat.

```
cargo check 2>&1 | head -40
```

---

**Extract the location and read it:**
```
# Error says: src/app.rs:1042:15
sed -n '1035,1055p' src/app.rs
```
Read ±10 lines around the reported line — the actual problem is often one line above or in the caller.

---

**Common Rust error patterns and what to look for:**

`E0308 — type mismatch`
→ What type does the function expect? What type are you passing?
→ grep for the function signature: `grep -n "fn the_function" src/`

`E0382 — use of moved value`
→ Who moved it? Look for the last place the variable was passed by value (not reference)
→ Fix: pass a reference (`&x`), clone only if the type is cheap and ownership is genuinely needed

`E0505 / E0502 — borrow conflict`
→ Is something holding a borrow while you try to mutate?
→ Look for a `lock()` or `borrow()` that isn't dropped before the mutation
→ Fix: restructure so the borrow is released before the new one is taken — don't just add `.clone()`

`cannot borrow X as mutable`
→ Is the variable declared `let mut`? Is the reference `&mut`?

`the trait X is not implemented for Y`
→ Missing `use` import, wrong type, or the type genuinely doesn't implement it
→ grep for `impl X for` to see what types implement it

---

**Never:**
- Scatter `.clone()` calls to silence borrow errors without understanding why
- Add `#[allow(unused)]` to silence warnings without reading them
- Guess at the fix from the error text alone — read the actual code first
