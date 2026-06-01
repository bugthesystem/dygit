#!/usr/bin/env bash
# Arg-forwarding wrapper for the slash commands.
#
# Selects the prebuilt dygi binary for this platform (via select-binary.sh, the
# one source of truth) and execs it with ALL passed arguments, e.g.
#   dygi.sh history 5   ->   <binary> history 5
# On an unsupported platform or missing binary we exit 0 silently.
set -euo pipefail

dir="$(dirname "$0")"

# select-binary.sh exits non-zero on unsupported/missing; stay invisible then.
path="$(bash "$dir/select-binary.sh")" || exit 0

exec "$path" "$@"
