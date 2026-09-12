#!/usr/bin/env bash
# Build the Rust WASM guest examples for wasm32-unknown-unknown.
#
#   ./scripts/build-examples.sh          # build
#   ./scripts/build-examples.sh --check  # build and smoke-test with `inspect`
#
# Installs the wasm32 target on demand. Artifacts land in
# examples/wasm/target/wasm32-unknown-unknown/release/*.wasm
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if ! rustup target list --installed 2>/dev/null | grep -qx wasm32-unknown-unknown; then
    echo "==> Adding wasm32-unknown-unknown target"
    rustup target add wasm32-unknown-unknown
fi

echo "==> Building WASM guest examples"
cargo build --release --target wasm32-unknown-unknown \
    --manifest-path examples/wasm/Cargo.toml

echo "==> Building the freestanding C examples (clang + wasm-ld)"
for build in examples/wasm/c-*/build.sh; do
    "$build"
done

modules=()
for wasm in examples/wasm/target/wasm32-unknown-unknown/release/*.wasm \
            examples/wasm/c-*/*.wasm; do
    [[ -f "$wasm" ]] && modules+=("$wasm")
done

if [[ "${1:-}" == "--check" ]]; then
    echo "==> Smoke-testing each module with the host loader"
    for wasm in "${modules[@]}"; do
        echo "--- $wasm"
        cargo run -q -p xtop-plugin-wasm --example inspect -- "$wasm" >/dev/null
    done
    echo "All modules loaded and rendered a synthetic state."
fi

echo "Artifacts:"
ls -1 "${modules[@]}"
