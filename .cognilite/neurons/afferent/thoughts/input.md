Use `<ask>` to pause mid-response and request input from the user. cognilite intercepts the tag, shows a UI widget, and injects the response as a "Tool result:" message before you continue. **Works for every model — no native function-calling required.**

**Text input:** `<ask>What filename should I use?</ask>`
**Confirmation:** `<ask type="confirm">Apply changes to FILE?</ask>` → injects `Yes` or `No`
**Choice list:** `<ask type="choice">Option A|Option B|Option C</ask>` → injects the selected option

Rules:
- One `<ask>` per response — write context before the tag so the user understands what they're answering.
- For `confirm`, describe what happens on Yes before the tag.
- Only ask what you genuinely can't infer or retrieve — prefer `<tool>` to gather information instead of asking.
- In Server/Headless mode, the user may not be present — use sparingly and prefer tools.
- Tags inside `<think>` blocks are ignored — only tags in your actual response trigger input.

---

Use `<preview path="..."/>` to open a file in the TUI's right-side panel. Useful after reading, writing, or patching a file so the user can review it visually.

`<preview path="src/main.rs"/>` (path relative to working directory)

Rules: use after performing file operations, not instead of reading · one per response · omit in Headless/Server mode where there's no panel to show.
