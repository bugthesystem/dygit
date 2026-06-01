#!/usr/bin/env bash
# Selects the prebuilt dygi binary for this platform and runs the hook.
#
# Contract: stdin (the UserPromptSubmit JSON) is piped straight through; stdout
# is whatever dygi prints (additionalContext JSON, or nothing). On ANY problem —
# unknown platform, missing binary — we exit 0 silently so the prompt is never
# blocked. Platform selection lives in select-binary.sh (one source of truth).
set -euo pipefail

dir="$(dirname "$0")"

# select-binary.sh exits non-zero on unsupported/missing; stay invisible then.
path="$(bash "$dir/select-binary.sh")" || exit 0

# Tell the spell-correction daemon where the bundled 82k-word frequency dict
# lives. The daemon (spawned by the hook on first prompt) reads DYGI_DICT_PATH;
# this points it at the dict that ships inside the plugin. If CLAUDE_PLUGIN_ROOT
# is unset (e.g. ad-hoc invocation), we skip the export and the daemon falls
# back to a path relative to the binary — never an error, never blocking.
if [ -n "${CLAUDE_PLUGIN_ROOT:-}" ]; then
  export DYGI_DICT_PATH="${CLAUDE_PLUGIN_ROOT}/crate/data/freq_dict_en.txt"
fi

exec "$path" hook
