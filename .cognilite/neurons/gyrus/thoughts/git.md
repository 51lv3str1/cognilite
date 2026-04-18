Always run git commands and answer from the real output — never guess.

Useful commands — adapt arguments to the actual request:
- `git status --short`
- `git diff` / `git diff --staged`
- `git log --oneline` with a numeric limit (e.g. -5, -10, -20)
- `git log --oneline` with a limit and a file path filter
- `git diff HEAD~1` (or any number) with an optional file path
- `git diff | head -100`

Rules: never run state-modifying commands (`commit`, `reset`, `push`, `stash`, `checkout`) without explicit confirmation · if status is clean, check `git log` — changes may already be committed · don't describe changes in first person unless you caused them.
