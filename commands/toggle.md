---
description: Toggle did-you-get-it on/off or change verbosity/aggressiveness
---

Pass the user's argument straight through. Valid arguments: on, off, verbose,
quiet, aggressive, gentle. No argument shows the current state. Show the returned
state line to the user.

!`bash "${CLAUDE_PLUGIN_ROOT}/hooks/dygi.sh" toggle $ARGUMENTS`
