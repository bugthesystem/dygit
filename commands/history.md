---
description: Show recent did-you-get-it prompt cleanups (original → cleaned)
argument-hint: "[N]"
allowed-tools: ["Bash(${CLAUDE_PLUGIN_ROOT}/hooks/dygi.sh:*)"]
---

Recent cleanups (optional count `$ARGUMENTS`, default 10):

```!
"${CLAUDE_PLUGIN_ROOT}/hooks/dygi.sh" history $ARGUMENTS
```

Show the output above to the user in a code block.
