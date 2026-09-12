#!/usr/bin/env bash
# Build the freestanding C per-core CPU statistics widget for wasm32.
#
# No libc, no WASI: the xtop WASM host only links `host.log`.
set -euo pipefail
cd "$(dirname "$0")"

if ! command -v clang >/dev/null 2>&1 || ! command -v wasm-ld >/dev/null 2>&1; then
    echo "clang/wasm-ld not found; skipping the C example" >&2
    exit 0
fi

clang --target=wasm32 -O2 -nostdlib -fno-builtin -fno-stack-protector \
    -Wl,--no-entry \
    -Wl,--export-memory \
    -Wl,--initial-memory=131072 \
    -Wl,-z,stack-size=16384 \
    -o c-cpu-avg.wasm widget.c

echo "built c-cpu-avg.wasm ($(stat -c%s c-cpu-avg.wasm) bytes)"
