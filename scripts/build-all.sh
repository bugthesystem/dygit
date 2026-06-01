#!/usr/bin/env bash
# Cross-compiles dygi for every supported platform into bin/.
#
# Requires the rustup targets to be installed:
#   rustup target add aarch64-apple-darwin x86_64-apple-darwin \
#                     x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu
# Linux targets from macOS need a cross linker (e.g. `cross` or zig). This script
# builds what it can and reports the rest; it does not fail the whole run if one
# target's linker is missing.
set -uo pipefail
cd "$(dirname "$0")/.."

declare -A TARGETS=(
  [aarch64-apple-darwin]=dygi-darwin-arm64
  [x86_64-apple-darwin]=dygi-darwin-x64
  [x86_64-unknown-linux-gnu]=dygi-linux-x64
  [aarch64-unknown-linux-gnu]=dygi-linux-arm64
)

mkdir -p bin
for triple in "${!TARGETS[@]}"; do
  out="${TARGETS[$triple]}"
  echo "==> $triple → bin/$out"
  if (cd crate && cargo build --release --target "$triple"); then
    cp "crate/target/$triple/release/dygi" "bin/$out"
    # macOS invalidates a Mach-O's adhoc signature on copy, and the kernel then
    # SIGKILLs the binary on launch. Re-sign the darwin artifacts adhoc so the
    # shipped binary actually runs. `codesign` only exists on macOS; guard it.
    case "$triple" in
      *-apple-darwin)
        command -v codesign >/dev/null && codesign --force --sign - "bin/$out"
        ;;
    esac
  else
    echo "    skipped $triple (toolchain/linker missing)"
  fi
done
echo "Done. Built binaries:"
ls -1 bin
