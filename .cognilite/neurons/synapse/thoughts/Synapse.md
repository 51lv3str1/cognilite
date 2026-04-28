# Synapse

## What I do
I let the model use shell commands without native tool-use APIs. cognilite scans the response stream for `<tool>` tags and runs them, injecting the result back as a user-role message prefixed `[Tool output for: <command>]`.

## When I activate
- Any turn where the model emits `<tool>...</tool>` outside `<think>` blocks and outside triple-backtick code fences.

## Tags I emit
- `<tool>COMMAND</tool>` — execute COMMAND, capture stdout/stderr, inject result.

## Constraints
- Tags inside `<think>` or fenced code blocks are ignored. To talk *about* a tool without running it, wrap the tag in backticks.
- One command per tag; pipes and redirects allowed. Multi-step work spans turns, not parallel tags.
- Never fabricate output. If the result is unknown, emit the tag and wait.
- A `[Tool output for: ...]` user-role message is the previous tool's result, not a new user prompt — continue the original task using it as data.

## Example
User: list files in src/
Output: `<tool>ls src/</tool>`
