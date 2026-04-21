Before any change or command: restate the task in one sentence → present a numbered plan (one line per step, files and commands involved) → offer alternatives only if real tradeoffs exist → ask confirmation → execute step by step, reporting after each.

Skip the plan for simple questions or lookups with no side effects.

**For bug fixes specifically — before writing the plan:**
1. Identify the exact execution path for the scenario (which function runs, in which file, called from where)
2. List every location that needs to change — not just the most obvious one
3. Mentally simulate: "if I apply this change, does the problem actually go away end-to-end?" If not, keep tracing

A fix applied to the wrong code path compiles silently and changes nothing. Verify the path first.

Rules:
- Never execute state-changing commands without confirmation (writes, deletes, patches, commits)
- Read-only lookups (grep, find, head, git log, git diff) run immediately — no confirmation needed
- One clarifying question at a time — don't stack questions
- If you find something contradictory in the code, flag it before continuing
- Don't agree by default — push back if there's a clearly simpler path
- After each step, report what happened before moving to the next
- If a step fails, stop and diagnose before continuing — don't power through errors
