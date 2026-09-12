#!/usr/bin/env bash
# End-to-end check of the WASM widget path: build the guest examples, load
# every module through the real host (wasmi), and validate the manifest and
# draw list JSON. No terminal or kernel required.
#
#   ./scripts/test-wasm-e2e.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

"$ROOT/scripts/build-examples.sh"

echo "==> Inspecting modules with the host loader"
modules=()
for wasm in examples/wasm/target/wasm32-unknown-unknown/release/*.wasm \
            examples/wasm/c-*/*.wasm; do
    [[ -f "$wasm" ]] && modules+=("$wasm")
done

failed=0
for wasm in "${modules[@]}"; do
    name="$(basename "$wasm")"
    output="$(cargo run -q -p xtop-plugin-wasm --example inspect -- "$wasm")" || {
        echo "FAIL $name: loader returned an error"
        failed=1
        continue
    }
    echo "$output" | python3 -c '
import json, sys
report = json.load(sys.stdin)
manifest = report["manifest"]
ops = report["draw"]["ops"]
assert manifest["name"], "manifest name must not be empty"
assert ops, "draw list must not be empty"
assert all("op" in op for op in ops), "every draw entry needs an op"
print("ok   %-12s v%-6s ops=%d" % (manifest["name"], manifest.get("version", "?"), len(ops)))
' || {
        echo "FAIL $name: invalid report"
        failed=1
        continue
    }
done

if [[ $failed -ne 0 ]]; then
    echo "WASM end-to-end check FAILED" >&2
    exit 1
fi
echo "WASM end-to-end check passed."
