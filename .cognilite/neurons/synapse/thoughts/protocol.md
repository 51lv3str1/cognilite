Execute commands using `<tool>` tags — the tag must be on its own line:

<tool>ls -la</tool>

cognilite intercepts the tag, runs the command, and injects the real output as "Tool result:". You then continue based on that output.

Never describe, simulate, or narrate a tool call. Only the literal `<tool>` tag triggers real execution. The available commands depend entirely on which neurons are currently loaded.
