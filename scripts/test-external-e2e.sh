#!/usr/bin/env bash
# End-to-end check of the external widget path: drive the Lua and Python
# examples exactly like the host does (one JSON request per line on stdin,
# one JSON response per line on stdout) and validate the responses.
#
# Node is exercised too when `node` is on PATH.
#
#   ./scripts/test-external-e2e.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EXAMPLES="$ROOT/examples/external"

# One JSON request per line: manifest, one render with a full synthetic
# state, then shutdown.
generate_requests() {
    python3 - <<'PY'
import json

state = {
    "tick": 1,
    "unix_time": 1_700_000_000,
    "width": 60,
    "height": 12,
    "config": {"theme": "x", "layout": "e2e", "interval_ms": 1000, "hostname": "ci"},
    "alerts": {"cpu_high": 90.0, "mem_high": 90.0, "disk_high": 90.0},
    "snapshot": {
        "cpus": [{"name": "cpu0", "usage": 12.5, "cpu_id": 0, "frequency": 3600,
                  "governor": "schedutil", "temp_c": 48.0}],
        "memory": {"total": 1000, "used": 400, "available": 600, "free": 500, "percent": 40.0},
        "swap": {"total": 0, "used": 0, "free": 0, "percent": 0.0},
        "disks": [],
        "networks": [],
        "processes": [],
        "load": {"one": 0.5, "five": 0.4, "fifteen": 0.3},
        "uptime": 123,
        "cpu_temp": 48.0,
        "disk_io": [],
        "batteries": [],
        "gpus": [],
        "sys": {"hostname": "ci", "os_version": "", "kernel": "", "desktop_env": "",
                "shell": "", "cpu_model": None, "package_power_w": None},
    },
}
for request in ({"type": "manifest"},
                {"type": "render", "state": state},
                {"type": "shutdown"}):
    print(json.dumps(request))
PY
}

VALIDATOR="$(mktemp)"
trap 'rm -f "$VALIDATOR"' EXIT
cat > "$VALIDATOR" <<'PY'
import json
import sys

name = sys.argv[1]
manifest = None
draw = None
logs = 0
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    message = json.loads(line)
    kind = message.get("type")
    if kind == "manifest":
        manifest = message.get("manifest")
    elif kind == "draw":
        draw = message.get("ops")
    elif kind == "log":
        logs += 1
    else:
        raise SystemExit(f"{name}: unknown response type {kind!r}")
if not manifest or not manifest.get("name"):
    raise SystemExit(f"{name}: missing manifest")
if draw is None:
    raise SystemExit(f"{name}: missing draw response")
if not all("op" in op for op in draw):
    raise SystemExit(f"{name}: draw entry without an op")
print(f"ok   {name:<12} ops={len(draw)} logs={logs}")
PY

run_widget() {
    local name="$1"
    shift
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "skip $name: '$1' not found on PATH"
        return 0
    fi
    echo "==> $name"
    generate_requests | "$@" | python3 "$VALIDATOR" "$name"
}

run_widget lua-clock lua5.4 "$EXAMPLES/lua-clock/widget.lua"
run_widget lua-histogram lua5.4 "$EXAMPLES/lua-histogram/widget.lua"
run_widget py-cpu python3 "$EXAMPLES/python-cpu/widget.py"
run_widget py-cpu-chart python3 "$EXAMPLES/python-cpu-chart/widget.py"
run_widget py-mem-regression python3 "$EXAMPLES/python-mem-regression/widget.py"
run_widget node-clock node "$EXAMPLES/node-clock/widget.js"

echo "External end-to-end check passed."
