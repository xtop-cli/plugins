#!/usr/bin/env bash
# Build the freestanding C widget for wasm32 (clang + wasm-ld).
#
# No libc, no WASI: the xtop WASM host only links `host.log`, so the module
# must not import anything else. Requires clang with the wasm32 target and
# wasm-ld on PATH (both ship with LLVM).
set -euo pipefail
cd "$(dirname "$0")"

if ! command -v clang >/dev/null 2>&1; then
    echo "clang not found; skipping the C example" >&2
    exit 0
fi
if ! command -v wasm-ld >/dev/null 2>&1; then
    echo "wasm-ld not found; skipping the C example" >&2
    exit 0
fi

clang --target=wasm32 -O2 -nostdlib -fno-builtin -fno-stack-protector \
    -Wl,--no-entry \
    -Wl,--export-memory \
    -Wl,--initial-memory=131072 \
    -Wl,-z,stack-size=16384 \
    -o c-ticker.wasm widget.c

echo "built c-ticker.wasm ($(stat -c%s c-ticker.wasm) bytes)"
