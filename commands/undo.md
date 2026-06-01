---
description: Show your last original prompt verbatim so you can re-send it
allowed-tools: ["Bash(${CLAUDE_PLUGIN_ROOT}/hooks/dygi.sh:*)"]
---

Your last original prompt:

```!
"${CLAUDE_PLUGIN_ROOT}/hooks/dygi.sh" undo
```

Present it to the user and offer to re-run it with their correction.
