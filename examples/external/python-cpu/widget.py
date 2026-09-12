#!/usr/bin/env python3
"""py-cpu -- xtop external widget: CPU gauge, history sparkline, load average.

Speaks the xtop line-delimited JSON protocol on stdin/stdout: one JSON request
object per line in, one JSON response object per line out. Standard library
only. The per-tick average is kept in an in-memory history for the sparkline.
"""

import json
import sys

WIDGET_NAME = "py-cpu"
VERSION = "0.1.0"
HISTORY_LEN = 120

GREEN = [123, 216, 143]
YELLOW = [255, 200, 64]
RED = [255, 85, 85]
TITLE = [200, 200, 200]
DIM = [140, 140, 140]

_history = []


def send(obj):
    sys.stdout.write(json.dumps(obj, separators=(",", ":")) + "\n")
    sys.stdout.flush()


def send_log(message):
    send({"type": "log", "message": str(message)})


def level_color(usage, threshold):
    if usage >= threshold:
        return RED
    if usage >= threshold * 0.8:
        return YELLOW
    return GREEN


def build_ops(state):
    if not isinstance(state, dict):
        state = {}
    snapshot = state.get("snapshot")
    if not isinstance(snapshot, dict):
        snapshot = {}
    alerts = state.get("alerts")
    if not isinstance(alerts, dict):
        alerts = {}
    load = snapshot.get("load")
    if not isinstance(load, dict):
        load = {}

    width = int(state.get("width") or 0) or 40
    height = int(state.get("height") or 0) or 10

    cpus = snapshot.get("cpus") or []
    if cpus:
        usage = sum(float(cpu.get("usage", 0.0) or 0.0) for cpu in cpus) / len(cpus)
    else:
        usage = 0.0
    usage = max(0.0, min(100.0, usage))

    threshold = float(alerts.get("cpu_high", 90.0) or 90.0)
    color = level_color(usage, threshold)

    # Own history, one sample per render tick.
    _history.append(usage)
    if len(_history) > HISTORY_LEN:
        del _history[: len(_history) - HISTORY_LEN]

    inner = max(width - 2, 1)
    spark_height = max(height - 6, 1)
    load_text = "load {:.2f} {:.2f} {:.2f}".format(
        float(load.get("one", 0.0) or 0.0),
        float(load.get("five", 0.0) or 0.0),
        float(load.get("fifteen", 0.0) or 0.0),
    )

    return [
        {
            "op": "block",
            "rect": {"x": 0, "y": 0, "width": width, "height": height},
            "border": "rounded",
            "title": WIDGET_NAME,
            "fg": TITLE,
            "bg": None,
        },
        {
            "op": "gauge",
            "rect": {"x": 1, "y": 1, "width": inner, "height": 3},
            "ratio": usage / 100.0,
            "label": "CPU {:.0f}%".format(usage),
            "fg": color,
            "bg": None,
            "border": None,
        },
        {
            "op": "sparkline",
            "rect": {"x": 1, "y": 4, "width": inner, "height": spark_height},
            "data": list(_history),
            "fg": color,
            "bg": None,
        },
        {
            "op": "text",
            "rect": {"x": 1, "y": max(height - 2, 1), "width": inner, "height": 1},
            "spans": [{"text": load_text, "fg": DIM, "dim": True}],
            "align": "center",
            "wrap": False,
        },
    ]


def handle(request):
    """Handle one request; return False to leave the loop."""
    kind = request.get("type")
    if kind == "manifest":
        send(
            {
                "type": "manifest",
                "manifest": {
                    "name": WIDGET_NAME,
                    "version": VERSION,
                    "description": "Average CPU gauge with history (Python)",
                    "author": "xtop examples",
                    "max_processes": 1,  # this widget does not read processes
                    "api": "1",
                },
            }
        )
    elif kind == "render":
        try:
            ops = build_ops(request.get("state"))
        except Exception as exc:  # never die on a malformed tick
            send_log("render failed: %s" % exc)
            ops = []
        send({"type": "draw", "ops": ops})
    elif kind == "shutdown":
        return False
    else:
        send_log("unknown request type: %s" % kind)
    return True


def main():
    # EOF on stdin ends the loop and exits 0.
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            request = json.loads(line)
        except ValueError as exc:
            send_log("invalid JSON request: %s" % exc)
            continue
        if not isinstance(request, dict):
            send_log("request is not a JSON object")
            continue
        try:
            if not handle(request):
                break
        except BrokenPipeError:
            break
    return 0


if __name__ == "__main__":
    sys.exit(main())
