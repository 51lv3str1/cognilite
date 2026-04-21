## Simplicity first

Write the minimum code that solves the problem — nothing more.
- No features beyond what was asked
- No error handling for scenarios that can't happen
- No abstractions for "future use"
- Three similar lines beats a premature helper function

**After any Rust change:** run `cargo check` or `cargo build` to verify it compiles. Fix all warnings — they count as part of done.

---

## Rust output anti-patterns — never do these

- Scatter `.clone()` calls to silence the borrow checker — trace the actual ownership issue
- Add `#[allow(unused)]` to hide dead code — remove the code or use it
- Guess at a fix because the error message is confusing — read the full error, including the `help:` note
- Add a `todo!()` or `unimplemented!()` and call it done — finish it or don't start it
- Wrap everything in `Arc<Mutex<_>>` by default — only add shared ownership when you actually share across threads

---

## Testing workflow

**Run all tests:**
```
cargo test 2>&1 | head -60
```
Always pipe to `head` — a full test suite can flood context. Read the first failure, fix it, repeat.

**Run one specific test:**
```
cargo test test_name -- --nocapture
```
`--nocapture` shows `println!` output, essential for diagnosing what the code actually produced.

**Run all tests in one module:**
```
cargo test module_name::
```

---

**When a test fails, read in this order:**

1. The assertion message — it tells you exactly what was expected vs what you got
2. The test function — understand what scenario it's testing
3. The function under test — trace why it produced the wrong value

Don't fix the test to make it pass. Fix the code the test is verifying.

---

**Before claiming a fix is done:**
- Run `cargo test` — a fix that breaks existing tests is not a fix
- If tests didn't exist before your change, you don't need to add them unless asked
- If you added a new function and tests exist for similar functions, mention it — don't add tests silently

**Never:**
- Delete or weaken assertions to make tests pass
- Skip running tests because "the logic looks right"
- Assume tests pass because the code compiles

---

## Bug fix protocol

Before writing the plan:
1. Identify the exact execution path for the scenario (which function runs, in which file, called from where)
2. List every location that needs to change — not just the most obvious one
3. Mentally simulate: "if I apply this change, does the problem actually go away end-to-end?" If not, keep tracing

A fix applied to the wrong code path compiles silently and changes nothing. Verify the path first.
