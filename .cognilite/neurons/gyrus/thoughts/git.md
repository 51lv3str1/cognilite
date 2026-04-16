# Gyrus — Git workflow

Use these tools to inspect the state of the repository. Git output provides concrete context that cannot be inferred — always run the command and answer from the real result.

## Common operations

Working tree status:
<tool>git status --short</tool>

Unstaged changes:
<tool>git diff</tool>

Staged changes:
<tool>git diff --staged</tool>

Recent history:
<tool>git log --oneline -20</tool>

Changes in a specific file:
<tool>git log --oneline -10 -- path/to/file</tool>
<tool>git diff HEAD~1 -- path/to/file</tool>

Who changed what and when:
<tool>git log --oneline --follow -- path/to/file</tool>

## When the working tree is clean

If `git status --short` returns no output, there are no uncommitted changes. In that case, look at recent commits to answer questions about what changed:
<tool>git log --oneline --since="24 hours ago"</tool>

If that returns nothing, broaden the range:
<tool>git log --oneline -10</tool>

## Rules

- Git output shows what changed in the repository — not necessarily what you did. Do not describe changes in first person unless you explicitly ran the command that caused them.
- Always run `git status` before assuming what files have changed — never guess
- If status is clean, check `git log` — the changes may already be committed
- Use `git diff` to see actual changes, not summaries from memory
- For large diffs, limit output: `git diff | head -100`
- Never run commands that modify repository state (`git commit`, `git reset`, `git checkout`, `git stash`, `git push`) without explicit confirmation from the user
