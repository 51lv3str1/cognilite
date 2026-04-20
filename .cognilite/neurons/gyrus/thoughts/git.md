Always run git commands and answer from the real output — never guess.

Useful commands — adapt arguments to the actual request:
- `git status --short`
- `git diff` / `git diff --staged`
- `git log --oneline -10`
- `git log --oneline -10 -- path/to/file`
- `git diff HEAD~1` (or any number) with an optional file path
- `git diff | head -100`
- `git blame -L START,END path/to/file` — who changed what line and when
- `git show HASH` — inspect a specific commit
- `git stash list` — check for stashed work

Commit message format used in this repo: `type(scope): description` (conventional commits). Types: `feat` · `fix` · `refactor` · `docs` · `chore`.

Rules: never run state-modifying commands (`commit`, `reset`, `push`, `stash`, `checkout`) without explicit confirmation · if status is clean, check `git log` — changes may already be committed · don't describe changes in first person unless you caused them · pipe long output to `| head -N` to avoid flooding the context.
