# xtop external widget examples

Six widgets that speak the `xtop-plugin-external` line-delimited JSON
protocol, in three languages:

| Widget | Language | Files | Renders |
| --- | --- | --- | --- |
| `lua-clock` | Lua 5.4 / LuaJIT | `lua-clock/widget.lua`, `lua-clock/json.lua` | Rounded block with the UTC time from `unix_time` and the host uptime |
| `lua-histogram` | Lua 5.4 / LuaJIT | `lua-histogram/widget.lua`, `lua-histogram/json.lua` | CPU frequency histogram (8 bins as stacked `bar` ops) + mean/σ/p95 footer |
| `py-cpu` | Python 3 (stdlib) | `python-cpu/widget.py` | CPU gauge + in-memory history sparkline + load average |
| `py-cpu-chart` | Python 3 (stdlib) | `python-cpu-chart/widget.py` | Statistical CPU chart: raw line, moving average, mean and ±σ reference lines, percentile footer |
| `py-mem-regression` | Python 3 (stdlib) | `python-mem-regression/widget.py` | Memory history with a least-squares fit, R², slope in %/min and a 60-second projection |
| `node-clock` | Node.js (stdlib) | `node-clock/widget.js` | Same layout as `lua-clock` |

No third-party dependencies in any widget.

## Protocol

The host (`xtop-plugin-external`) spawns one process per widget and exchanges
**one JSON object per line** over stdin/stdout. Guests must never write
anything but JSON lines to stdout (diagnostics go to stderr or as `log`
responses).

Host to guest (`xtop-wasm-contract::Request`):

```json
{"type":"manifest"}
{"type":"render","state":{ ...State... }}
{"type":"shutdown"}
```

Guest to host (`xtop-wasm-contract::Response`):

```json
{"type":"manifest","manifest":{"name":"lua-clock","version":"0.1.0","description":"...","author":"...","max_processes":1,"api":"1"}}
{"type":"draw","ops":[ ...Op... ]}
{"type":"log","message":"anything"}
```

Rules:

- `manifest` is asked once at load; `name` is the only required field
  (`api` should be `"1"`, `max_processes` caps the process list per tick).
- `render` is asked once per tick with the full `State`; the guest must answer
  with **exactly one** `draw` line (a `log` may precede it).
- `log` lines are allowed at any time; the host prints them to stderr.
- `shutdown` means exit cleanly with code 0. EOF on stdin must also exit.
- Blank input lines are ignored. Invalid JSON must not kill the process:
  answer a `log` and keep reading.
- Ops carry coordinates **relative to the widget area**; the host clips every
  rect. Op variants and fields (serde names):
  - `block`: `rect`, `border` (`native|rounded|double|plain|ascii`), `title`,
    `fg`, `bg`
  - `text`: `rect`, `spans` (`{text,fg,bg,bold,italic,underlined,dim}`),
    `align` (`left|center|right`), `wrap`
  - `gauge`: `rect`, `ratio`, `label`, `fg`, `bg`, `border`
  - `bar`: same fields as `gauge`
  - `sparkline`: `rect`, `data`, `fg`, `bg`
  - `chart`: `rect`, `datasets` (`{name,points:[[x,y],...],color}`),
    `x_bounds`, `y_bounds`, `border`, `fg`, `bg`, `marker`
  - colors are `[r,g,b]` arrays, `null` is allowed for optional colors.
- `State` fields the examples use: `tick`, `unix_time`, `width`, `height`,
  `config{theme,layout,interval_ms,hostname}`,
  `alerts{cpu_high,mem_high,disk_high}`,
  `snapshot{cpus[{name,usage,cpu_id,frequency,governor,temp_c}],memory,swap,disks,networks,processes,load{one,five,fifteen},uptime,cpu_temp,disk_io,batteries,gpus,sys}`.

## Descriptor

The host discovers `<name>.json` files in `~/.config/xtop/external/` (or the
directory named by `XTOP_EXTERNAL_DIR`) and registers one plugin per file:

```json
{
  "name": "lua-clock",
  "description": "UTC clock in Lua",
  "command": ["lua5.4", "/home/me/xtop-external/lua-clock/widget.lua"],
  "timeout_ms": 2000,
  "max_processes": 20
}
```

- `name`: widget name used by layouts. Defaults to the file stem when omitted.
- `command`: argv of the helper process; use **absolute paths** (the host's cwd
  is not the descriptor directory). If the widget cannot find `json.lua`, make
  sure the first argument is the absolute path to `widget.lua` — the script
  resolves the codec relative to itself.
- `timeout_ms`: per-request read timeout, clamped to 100–60000 (default 2000).
- `max_processes`: cap on processes per snapshot, clamped to 1–4096. The
  manifest may lower/override it.

The process runs with your user permissions; WASM widgets are the sandboxed
alternative. On shutdown the host sends `{"type":"shutdown"}` and kills the
child.

## Install

```sh
mkdir -p ~/.config/xtop/external
# copy or symlink the widgets somewhere stable, then add descriptors, e.g.:
cat > ~/.config/xtop/external/lua-clock.json <<'EOF'
{
  "name": "lua-clock",
  "description": "UTC clock in Lua",
  "command": ["lua5.4", "/absolute/path/to/examples/external/lua-clock/widget.lua"],
  "timeout_ms": 2000,
  "max_processes": 1
}
EOF
```

Repeat for `py-cpu` (`["python3", ".../python-cpu/widget.py"]`),
`py-cpu-chart` (`["python3", ".../python-cpu-chart/widget.py"]`) and
`node-clock` (`["node", ".../node-clock/widget.js"]`). Reference the widget
name in a layout and restart xtop.

## Testing manually

All widgets can be driven from a shell by piping protocol lines. Build a
minimal but complete `State` once (every required contract field is present):

```sh
python3 - <<'PY' > /tmp/state.json
import json
state = {
    "tick": 7, "unix_time": 1700000000, "width": 40, "height": 12,
    "config": {"theme": "tokio", "layout": "demo", "interval_ms": 1000, "hostname": "box"},
    "alerts": {"cpu_high": 90.0, "mem_high": 90.0, "disk_high": 90.0},
    "snapshot": {
        "cpus": [{"name": "cpu0", "usage": 42.0, "cpu_id": 0,
                  "frequency": 3600, "governor": "schedutil", "temp_c": 55.0}],
        "memory": {"total": 100, "used": 50, "available": 50, "free": 50, "percent": 50.0},
        "swap": {"total": 0, "used": 0, "free": 0, "percent": 0.0},
        "disks": [], "networks": [], "processes": [],
        "load": {"one": 0.5, "five": 0.4, "fifteen": 0.3},
        "uptime": 93784, "cpu_temp": 55.0, "disk_io": [], "batteries": [], "gpus": [],
        "sys": {"hostname": "box", "os_version": "linux", "kernel": "6.x",
                "desktop_env": "hyprland", "shell": "zsh"},
    },
}
print(json.dumps(state))
PY

# Lua
{ echo '{"type":"manifest"}';
  printf '{"type":"render","state":%s}\n' "$(cat /tmp/state.json)";
  echo '{"type":"shutdown"}'; } | lua5.4 lua-clock/widget.lua

# Python
{ echo '{"type":"manifest"}';
  printf '{"type":"render","state":%s}\n' "$(cat /tmp/state.json)";
  echo '{"type":"shutdown"}'; } | python3 python-cpu/widget.py

# Python statistical chart: several renders so the history builds up
# (a single tick only answers "collecting samples...")
python3 - <<'PY' | python3 python-cpu-chart/widget.py
import json
state = json.load(open("/tmp/state.json"))
print(json.dumps({"type": "manifest"}))
for tick in range(1, 8):
    state["tick"] = tick
    print(json.dumps({"type": "render", "state": state}))
print(json.dumps({"type": "shutdown"}))
PY

# Node (only if node is installed)
{ echo '{"type":"manifest"}';
  printf '{"type":"render","state":%s}\n' "$(cat /tmp/state.json)";
  echo '{"type":"shutdown"}'; } | node node-clock/widget.js
```

Expected output for `lua-clock` (one JSON object per line):

```json
{"type":"manifest","manifest":{"name":"lua-clock",...}}
{"type":"draw","ops":[{"op":"block",...},{"op":"text",...},{"op":"text",...}]}
```

The Lua JSON codec can be tested standalone:

```sh
lua5.4 -e 'local json = require("lua-clock/json"); local s = [[{"a":"x\"y\nz","b":-1.5e2,"c":true,"d":null,"e":[1,[2,3]]}]]; print(json.encode(json.decode(s)))'
```

Robustness checks (empty lines, invalid JSON, EOF without shutdown) exit 0 and
keep answering:

```sh
printf '\nnot json\n{"type":"render","state":%s}\n' "$(cat /tmp/state.json)" \
  | lua5.4 lua-clock/widget.lua   # exits 0 after EOF, logs the bad line
```

## Limitations

- `node-clock` was written but **not executed** in this environment because
  Node.js is not installed; it uses only `readline` and native JSON.
- The examples target the `api: "1"` contract. The host is forward-compatible
  and warns on version mismatch instead of refusing to load.
