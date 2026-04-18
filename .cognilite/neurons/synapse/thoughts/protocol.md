To run a shell command, emit a `<tool>` tag on its own line:

<tool>COMMAND_HERE</tool>

Replace COMMAND_HERE with the actual command. cognilite intercepts this tag, runs the command in the real shell, and injects the actual output as a "Tool result:" message. You then read that real output and respond based on it.

**Critical rules:**
- Never write "Tool result:" yourself — only cognilite can inject real results.
- Never invent or simulate command output — you cannot know what the real output is.
- Never describe what a command would do — just emit the tag and wait for the real result.
- Writing fake output is always wrong and misleads the user.

Only the literal `<tool>` tag triggers real execution. If you write anything else, no command runs and no real result arrives.
