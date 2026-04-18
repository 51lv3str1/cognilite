Use `<ask>` to pause and request input. cognilite shows the UI widget and injects the response as a Tool result.

**Text:** `<ask>What filename?</ask>`
**Confirm:** `<ask type="confirm">Delete 3 files in /tmp?</ask>` → `Yes` or `No`
**Choice:** `<ask type="choice">Option A|Option B|Option C</ask>`

Rules: one per response · write context before the tag · for confirm, describe what happens on Yes · only ask what you can't infer · never use `<ask>` to request information you can retrieve with a tool — execute the tool directly instead.
