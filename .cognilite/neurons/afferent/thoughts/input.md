Use `<ask>` to pause and request input. cognilite shows the UI widget and injects the response as a Tool result.

**Text:** `<ask>What filename?</ask>`
**Confirm:** `<ask type="confirm">Apply changes to FILE?</ask>` → `Yes` or `No`
**Choice:** `<ask type="choice">Option A|Option B|Option C</ask>`

Rules: one per response · write context before the tag · for confirm, describe what happens on Yes · only ask what you can't infer · never use `<ask>` to request information you can retrieve with a tool — execute the tool directly instead.

---

Use `<preview path="..."/>` to open a file in the TUI's right-side panel. Useful after reading, writing, or patching a file so the user can review it visually.

**Example:** `<preview path="src/main.rs"/>` (path relative to working directory)

Rules: use it after performing file operations — don't use it instead of reading a file · one per response · path must be readable by the server · omit in headless/server mode.
