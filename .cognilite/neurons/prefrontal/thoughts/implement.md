## Implementing a new feature

Before writing any code, answer these four questions by reading the codebase:

**1. What data structure owns this behavior?**
```
grep -rn "^pub struct\|^pub enum" src/ | grep -i relevant_keyword
```
Find the type that logically holds the new state or method. Don't create a new struct if an existing one fits.

**2. How is a similar feature already implemented?**
Find the closest existing function and read it:
```
grep -rn "fn similar_function" src/
sed -n 'START,ENDp' src/file.rs
```
Match its pattern: error handling style, return types, how it interacts with state. Consistency matters more than cleverness.

**3. Who calls this, and what do they pass?**
Read the call site before writing the function. The signature should match what the caller has — don't make callers adapt to you.

**4. What's the execution path?**
Trace where the new code fits: what triggers it → what it reads → what it modifies → what it returns. If the path touches shared state (Mutex, channel, Arc), understand the locking order before touching anything.

---

**Implementation order:**
1. Data first — add or modify the struct field / enum variant
2. `cargo check` — see what breaks (constructor sites, pattern matches)
3. Fix those before writing the new logic
4. Implement the function
5. Wire up the caller
6. `cargo build` — fix warnings before claiming done

**One thing at a time:**
- Don't implement the feature AND clean up the module AND rename things in one go
- If you notice something broken nearby, mention it — fix it separately

**After implementing:**
- Trace through the new code manually: does it actually do what was asked?
- Check edge cases: empty input, zero, None, disconnected channel — whatever applies
- If existing tests cover the area, run them
