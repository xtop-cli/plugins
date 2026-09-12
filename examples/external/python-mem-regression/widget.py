#!/usr/bin/env python3
"""py-mem-regression -- xtop external widget: memory trend with regression.

Speaks the xtop line-delimited JSON protocol on stdin/stdout. Standard
library only.

Keeps a rolling history of the memory percentage and fits a least-squares
line over it, then draws:

- the memory history line,
- the fitted regression line (green when memory trends down, red when up),
- a footer with the equation, R^2, the slope in %/minute and a 60-second
  projection.

It is the template for "fit a model in your language and render it".
"""

import json
import statistics
import sys

WIDGET_NAME = "py-mem-regression"
VERSION = "0.1.0"
HISTORY_LEN = 180

CYAN = [90, 212, 230]
GREEN = [123, 216, 143]
RED = [252, 97, 141]
DIM = [140, 140, 140]
TITLE = [200, 200, 200]

_history = []


def send(obj):
    sys.stdout.write(json.dumps(obj, separators=(",", ":")) + "\n")
    sys.stdout.flush()


def send_log(message):
    send({"type": "log", "message": str(message)})


def linear_regression(values):
    """Return (intercept, slope, r_squared) for y over x = 0..n-1."""
    n = len(values)
    xs = range(n)
    mean_x = (n - 1) / 2.0
    mean_y = statistics.fmean(values)
    sxx = sum((x - mean_x) ** 2 for x in xs)
    sxy = sum((x - mean_x) * (y - mean_y) for x, y in zip(xs, values))
    if sxx == 0:
        return mean_y, 0.0, 0.0
    slope = sxy / sxx
    intercept = mean_y - slope * mean_x
    ss_tot = sum((y - mean_y) ** 2 for y in values)
    ss_res = sum((y - (intercept + slope * x)) ** 2 for x, y in zip(xs, values))
    r_squared = 1.0 - (ss_res / ss_tot) if ss_tot > 0 else 1.0
    return intercept, slope, r_squared


def build_ops(state):
    if not isinstance(state, dict):
        state = {}
    snapshot = state.get("snapshot")
    if not isinstance(snapshot, dict):
        snapshot = {}
    config = state.get("config")
    if not isinstance(config, dict):
        config = {}

    width = int(state.get("width") or 0) or 60
    height = int(state.get("height") or 0) or 14
    inner = max(width - 2, 1)

    memory = snapshot.get("memory")
    if not isinstance(memory, dict):
        memory = {}
    percent = max(0.0, min(100.0, float(memory.get("percent", 0.0) or 0.0)))
    interval_ms = max(100, int(config.get("interval_ms") or 1000))
    samples_per_minute = 60_000.0 / interval_ms

    _history.append(percent)
    if len(_history) > HISTORY_LEN:
        del _history[: len(_history) - HISTORY_LEN]

    ops = [
        {
            "op": "block",
            "rect": {"x": 0, "y": 0, "width": width, "height": height},
            "border": "rounded",
            "title": WIDGET_NAME,
            "fg": TITLE,
            "bg": None,
        }
    ]
    if width < 20 or height < 7:
        return ops

    if len(_history) < 3:
        ops.append(
            {
                "op": "text",
                "rect": {"x": 1, "y": 2, "width": inner, "height": 1},
                "spans": [{"text": "collecting samples...", "fg": DIM, "dim": True}],
                "align": "center",
                "wrap": False,
            }
        )
        return ops

    samples = list(_history)
    intercept, slope, r_squared = linear_regression(samples)
    x_end = float(len(samples) - 1)
    fitted = [intercept + slope * x for x in range(len(samples))]
    slope_per_minute = slope * samples_per_minute
    projection = intercept + slope * (x_end + samples_per_minute)
    projection = max(0.0, min(100.0, projection))

    trend_color = RED if slope_per_minute > 0.05 else GREEN if slope_per_minute < -0.05 else CYAN
    ops.append(
        {
            "op": "chart",
            "rect": {"x": 1, "y": 1, "width": inner, "height": max(height - 4, 3)},
            "datasets": [
                {
                    "name": "mem",
                    "points": [[float(i), value] for i, value in enumerate(samples)],
                    "color": CYAN,
                },
                {
                    "name": "fit",
                    "points": [[float(i), value] for i, value in enumerate(fitted)],
                    "color": trend_color,
                },
            ],
            "x_bounds": [0.0, max(x_end, 1.0)],
            "y_bounds": [0.0, 100.0],
            "border": None,
            "fg": None,
            "bg": None,
            "marker": "braille",
        }
    )

    equation = "y = {:.1f} {:+.2f}x".format(intercept, slope)
    ops.append(
        {
            "op": "text",
            "rect": {"x": 1, "y": max(height - 3, 1), "width": inner, "height": 1},
            "spans": [
                {"text": equation, "fg": DIM},
                {"text": "   R^2 {:.3f}".format(r_squared), "fg": DIM, "dim": True},
            ],
            "align": "center",
            "wrap": False,
        }
    )
    ops.append(
        {
            "op": "text",
            "rect": {"x": 1, "y": max(height - 2, 1), "width": inner, "height": 1},
            "spans": [
                {"text": "now {:.0f}%".format(percent), "fg": CYAN},
                {"text": "   slope {:+.1f}%/min".format(slope_per_minute), "fg": trend_color},
                {"text": "   next 60s ~{:.0f}%".format(projection), "fg": DIM},
            ],
            "align": "center",
            "wrap": False,
        }
    )
    return ops


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
                    "description": "Memory history with least-squares regression, R^2 and a 60s projection (Python)",
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
