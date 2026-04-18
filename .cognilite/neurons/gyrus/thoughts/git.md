Always run git commands and answer from the real output — never guess.

Reference commands (run only when relevant, with the actual arguments needed):
```
git status --short
git diff / git diff --staged
git log --oneline -N
git log --oneline -N -- path/to/file
git diff HEAD~N -- path/to/file
git diff | head -100
```

Rules: never run state-modifying commands (`commit`, `reset`, `push`, `stash`, `checkout`) without explicit confirmation · if status is clean, check `git log` — changes may already be committed · don't describe changes in first person unless you caused them.
