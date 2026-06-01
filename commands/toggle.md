---
description: Toggle did-you-get-it on/off or change verbosity/aggressiveness
argument-hint: "[on|off|verbose|quiet|aggressive|gentle]"
allowed-tools: ["Bash(${CLAUDE_PLUGIN_ROOT}/hooks/dygi.sh:*)"]
---

Apply the setting `$ARGUMENTS` (on·off·verbose·quiet·aggressive·gentle; no
argument shows current state):

```!
"${CLAUDE_PLUGIN_ROOT}/hooks/dygi.sh" toggle $ARGUMENTS
```

Show the returned state line to the user.
