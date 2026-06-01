#!/usr/bin/env bash
# Single source of truth for platform -> prebuilt dygi binary selection.
#
# Echoes the absolute path of the binary for this platform on stdout and exits 0.
# Exits non-zero (with no output) when the platform is unsupported or the binary
# is not present/executable. Callers (run.sh, dygi.sh) treat a non-zero exit as
# "stay invisible, do nothing".
set -euo pipefail

# Resolve the plugin root from our own location (hooks/ -> plugin root) so this
# works both as a hook (where CLAUDE_PLUGIN_ROOT is set) and as a slash command
# (where it is not). The env var, if present, takes precedence.
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="${CLAUDE_PLUGIN_ROOT:-$(dirname "$here")}"

case "$(uname -s)/$(uname -m)" in
  Darwin/arm64)  bin="dygi-darwin-arm64" ;;
  Darwin/x86_64) bin="dygi-darwin-x64" ;;
  Linux/x86_64)  bin="dygi-linux-x64" ;;
  Linux/aarch64) bin="dygi-linux-arm64" ;;
  *)             exit 1 ;;  # unsupported platform
esac

path="$root/bin/$bin"
[ -x "$path" ] || exit 1  # binary not shipped/executable for this platform

printf '%s\n' "$path"
