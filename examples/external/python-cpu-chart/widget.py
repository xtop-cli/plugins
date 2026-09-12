#!/usr/bin/env python3
"""py-cpu-chart -- xtop external widget: statistical CPU chart.

Speaks the xtop line-delimited JSON protocol on stdin/stdout. Standard
library only.

Unlike the gauge/sparkline examples, this one computes real statistics on a
rolling window of the per-tick average CPU usage and draws them with the
`chart` op:

- raw usage line (threshold-colored),
- moving average (window of 5),
- mean, mean + sigma and mean - sigma as horizontal reference lines,
- a footer with mean, standard deviation, min, p50, p95 and max.

It is the template for "math/statistics rendered as a widget": do the math in
your language, emit draw ops.
"""

import json
import math
import statistics
import sys

WIDGET_NAME = "py-cpu-chart"
VERSION = "0.1.0"
HISTORY_LEN = 180
MA_WINDOW = 5

GREEN = [123, 216, 143]
YELLOW = [255, 200, 64]
RED = [255, 85, 85]
VIOLET = [148, 138, 227]
CYAN = [90, 212, 230]
DIM = [140, 140, 140]
TITLE = [200, 200, 200]

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


def moving_average(values, window):
    """Simple moving average, same length as `values` (partial windows)."""
    out = []
    total = 0.0
    queue = []
    for value in values:
        queue.append(value)
        total += value
        if len(queue) > window:
            total -= queue.pop(0)
        out.append(total / len(queue))
    return out


def percentile(values, fraction):
    """Nearest-rank percentile (no interpolation), stable and dependency-free."""
    if not values:
        return 0.0
    ordered = sorted(values)
    index = min(len(ordered) - 1, max(0, int(math.ceil(fraction * len(ordered))) - 1))
    return ordered[index]


def horizontal_line(x_end, y, color, name):
    return {
        "name": name,
        "points": [[0.0, y], [float(x_end), y]],
        "color": color,
    }


def build_ops(state):
    if not isinstance(state, dict):
        state = {}
    snapshot = state.get("snapshot")
    if not isinstance(snapshot, dict):
        snapshot = {}
    alerts = state.get("alerts")
    if not isinstance(alerts, dict):
        alerts = {}

    width = int(state.get("width") or 0) or 60
    height = int(state.get("height") or 0) or 14
    inner = max(width - 2, 1)

    cpus = snapshot.get("cpus") or []
    if cpus:
        usage = sum(float(cpu.get("usage", 0.0) or 0.0) for cpu in cpus) / len(cpus)
    else:
        usage = 0.0
    usage = max(0.0, min(100.0, usage))

    _history.append(usage)
    if len(_history) > HISTORY_LEN:
        del _history[: len(_history) - HISTORY_LEN]

    threshold = float(alerts.get("cpu_high", 90.0) or 90.0)
    color = level_color(usage, threshold)

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
    if width < 16 or height < 6:
        return ops

    footer_rows = 2 if height >= 9 else 1
    chart_height = max(height - 1 - footer_rows, 3)
    chart_rect = {"x": 1, "y": 1, "width": inner, "height": chart_height}

    # Not enough samples yet: show a status line only.
    if len(_history) < 2:
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
    x_end = float(len(samples) - 1)
    mean = statistics.fmean(samples)
    sigma = statistics.pstdev(samples) if len(samples) > 1 else 0.0
    moving = moving_average(samples, MA_WINDOW)

    datasets = [
        {
            "name": "cpu",
            "points": [[float(i), value] for i, value in enumerate(samples)],
            "color": color,
        },
        {
            "name": "ma",
            "points": [[float(i), value] for i, value in enumerate(moving)],
            "color": VIOLET,
        },
        horizontal_line(x_end, mean, CYAN, "mean"),
    ]
    if sigma >= 0.5:
        datasets.append(horizontal_line(x_end, min(100.0, mean + sigma), DIM, "+sigma"))
        datasets.append(horizontal_line(x_end, max(0.0, mean - sigma), DIM, "-sigma"))

    ops.append(
        {
            "op": "chart",
            "rect": chart_rect,
            "datasets": datasets,
            "x_bounds": [0.0, max(x_end, 1.0)],
            "y_bounds": [0.0, 100.0],
            "border": None,
            "fg": None,
            "bg": None,
            "marker": "braille",
        }
    )

    stats_line = "mean {:5.1f}  sigma {:4.1f}  min {:3.0f}  p50 {:3.0f}  p95 {:3.0f}  max {:3.0f}".format(
        mean,
        sigma,
        min(samples),
        percentile(samples, 0.50),
        percentile(samples, 0.95),
        max(samples),
    )
    footer_y = max(height - footer_rows, 1)
    ops.append(
        {
            "op": "text",
            "rect": {"x": 1, "y": footer_y, "width": inner, "height": 1},
            "spans": [{"text": stats_line, "fg": DIM, "dim": True}],
            "align": "center",
            "wrap": False,
        }
    )
    if height >= 9:
        legend = "cpu {:.0f}%   ma{}   mean   +/-sigma   n={}".format(
            usage, MA_WINDOW, len(samples)
        )
        ops.append(
            {
                "op": "text",
                "rect": {"x": 1, "y": footer_y + 1, "width": inner, "height": 1},
                "spans": [{"text": legend, "fg": DIM, "dim": True}],
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
                    "description": "Statistical CPU chart: history, moving average, mean +/- sigma, percentiles (Python)",
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
