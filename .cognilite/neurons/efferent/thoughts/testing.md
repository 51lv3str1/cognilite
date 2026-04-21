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
