# External runtime widgets

`xtop-plugin-external` is an optional kernel feature that turns helper
processes into plugin widgets. The host spawns one process per descriptor and
exchanges **one JSON object per line** over stdin/stdout: requests
(`manifest`, `render`, `shutdown`) go in, responses (`manifest`, `draw`,
`log`) come out. The guest can be any program in any language with a runtime.

The process runs with your user's permissions and **is** the sandbox. Use
[WASM widgets](wasm-widgets.md) when you want an in-process,
capability-limited guest instead.

## Enabling the host

The feature is **not** part of the default kernel build. From the kernel
repository (`xtop/`):

```sh
cargo build --features plugin-external
# both runtime hosts at once:
cargo build --features plugin-wasm,plugin-external
```

Without the feature the `external/` directory is ignored.

## Descriptors

At startup the host scans the `external/` subdirectory of the user config
directory and reads every `*.json` descriptor (one plugin per file, paths
sorted):

| Platform | Directory |
|---|---|
| Linux | `$XDG_CONFIG_HOME/xtop/external/` (default `~/.config/xtop/external/`) |
| macOS | `~/Library/Application Support/xtop/external/` |
| Windows | `%APPDATA%\xtop\external\` |

Set `XTOP_EXTERNAL_DIR` to override the directory entirely:

```sh
XTOP_EXTERNAL_DIR=/opt/xtop-widgets xtop
```

Descriptor fields:

| Field | Required | Default | Notes |
|---|---|---|---|
| `name` | no | file stem | Widget name layouts use. |
| `description` | no | `""` | May be replaced by the manifest `description`. |
| `command` | yes | — | argv array, e.g. `["python3", "/path/widget.py"]`. Absolute paths are recommended because the child's working directory is not the descriptor directory. |
| `timeout_ms` | no | 2000 | Per-response read timeout, clamped to 100–60000. |
| `max_processes` | no | 50 | Clamped to 1–4096. When the descriptor value is the default (50) and the manifest asks for a different value, the manifest wins. |

Example:

```json
{
  "name": "lua-clock",
  "description": "UTC clock in Lua",
  "command": ["lua5.4", "/home/me/widgets/lua-clock/widget.lua"],
  "timeout_ms": 2000,
  "max_processes": 1
}
```

A descriptor with invalid JSON or an empty `command` is skipped with a log
line. A process that fails to spawn or does not answer the initial `manifest`
request is skipped too.

## Protocol

Host to guest (`Request`):

| Line | Meaning |
|---|---|
| `{"type":"manifest"}` | Ask for the manifest; sent once at load. |
| `{"type":"render","state":{...}}` | Ask for a draw list; sent once per tick. |
| `{"type":"shutdown"}` | Ask the process to exit cleanly. |

Guest to host (`Response`):

| Line | Meaning |
|---|---|
| `{"type":"manifest","manifest":{...}}` | Answer to `manifest`. |
| `{"type":"draw","ops":[...]}` | Answer to `render`. |
| `{"type":"log","message":"..."}` | Free-form diagnostic; the host prints it to stderr as `[external:<name>] ...`. |

Rules:

- The guest must answer each request with exactly one non-`log` response.
  `log` lines may be sent at any time and are skipped while waiting.
- Blank lines from the guest are ignored.
- Never write anything but JSON lines to stdout; raw diagnostics belong on
  stderr (the host inherits it).
- On `shutdown`, or on EOF on stdin, exit cleanly with status 0. The host
  writes `{"type":"shutdown"}` and then kills and reaps the child without
  waiting for the guest to exit on its own.
- A malformed response line fails the current request; the host logs it once
  and keeps the last cached draw list.

The `manifest` response must deserialize as
[`xtop-wasm-contract::Manifest`](wasm-widgets.md#manifest): `name` is the only
field without a default. The host uses `description` and, when the descriptor
leaves `max_processes` at the default, `max_processes`. Widget identity comes
from the descriptor, not from `manifest.name`.

The `state` payload and the draw list ops are exactly the ones documented for
WASM widgets: see [State](wasm-widgets.md#state) and
[Draw list](wasm-widgets.md#draw-list).

### Session example

```
host -> {"type":"manifest"}
guest <- {"type":"manifest","manifest":{"name":"lua-clock","version":"0.1.0","description":"UTC clock","max_processes":1,"api":"1"}}
host -> {"type":"render","state":{...}}
guest <- {"type":"log","message":"tick 7"}
guest <- {"type":"draw","ops":[{"op":"block","rect":{"x":0,"y":0,"width":40,"height":12},"border":"rounded","title":"lua-clock"},{"op":"text","rect":{"x":1,"y":1,"width":38,"height":1},"spans":[{"text":"22:13:20","bold":true}],"align":"center"}]}
host -> {"type":"shutdown"}
```

## Lifecycle

| Phase | What happens |
|---|---|
| Load | Spawn the process, send `manifest`, wait up to `timeout_ms` for the answer. |
| Tick | Build the `State`, send `render`, skip `log` lines until the `draw` response or the timeout, cache the draw list. |
| Render | Replay the cached draw list; the process is never touched during a frame. |
| Shutdown | Send `{"type":"shutdown"}`, then kill and reap the child (on plugin disable and on drop). |

There is no hot reload: descriptors and scripts are read once at startup, and
the `reload` action returns an error asking for a restart.

## Timeouts and failures

- Each response wait uses the descriptor's `timeout_ms`; a timeout is logged
  once per distinct message.
- On any tick failure (timeout, malformed line, unexpected exit) the last
  valid draw list stays on screen and the next tick retries.
- A process that exits is not respawned; further requests fail with
  `process exited unexpectedly`.
- `max_processes` caps `snapshot.processes` after sorting by CPU usage
  (descending), like the WASM host.

## Debug actions

| Action | Returns |
|---|---|
| `status` | `name`, `path`, `command`, `ticks`, `ops`, `timeout_ms`, `last_error`. |
| `render` | The cached draw list as JSON. |
| `reload` | Error: external widgets reload by restarting xtop. |

## Examples

- [`examples/external/lua-clock/`](../examples/external/lua-clock/) — Lua 5.4 / LuaJIT.
- [`examples/external/lua-histogram/`](../examples/external/lua-histogram/) — Lua histogram + statistics.
- [`examples/external/python-cpu/`](../examples/external/python-cpu/) — Python 3 (stdlib only).
- [`examples/external/python-cpu-chart/`](../examples/external/python-cpu-chart/) — Python statistical chart (moving average, ±σ, percentiles).
- [`examples/external/python-mem-regression/`](../examples/external/python-mem-regression/) — Python least-squares regression with R² and projection.
- [`examples/external/node-clock/`](../examples/external/node-clock/) — Node.js (stdlib only).

See [`examples/external/README.md`](../examples/external/README.md) for
descriptors, manual protocol tests with a synthetic `State`, and robustness
checks.
