# WASM runtime widgets

`xtop-plugin-wasm` is an optional kernel feature that turns `*.wasm` modules
into regular plugin widgets. The host loads each module in-process through the
[wasmi](https://github.com/wasmi-lang/wasmi) sandbox, asks it for a manifest
and a draw list, and replays the draw list onto the frame. Guests never see
ratatui, the kernel, or the terminal: they exchange JSON defined by
[`xtop-wasm-contract`](../plugins/xtop-wasm-contract/src/lib.rs) with the host.

Because widgets are registered through the normal plugin path, a runtime
widget takes precedence over every compiled-in pack and can replace any widget
name — including built-ins such as `cpu`.

## Enabling the host

The feature is **not** part of the default kernel build. From the kernel
repository (`xtop/`):

```sh
cargo build --features plugin-wasm
# both runtime hosts at once:
cargo build --features plugin-wasm,plugin-external
```

Without the feature the `wasm/` directory is ignored and `xtop` behaves
exactly as before.

## Installing widgets

At startup the host scans the `wasm/` subdirectory of the user config
directory and loads every `*.wasm` file (one plugin per module, paths sorted):

| Platform | Directory |
|---|---|
| Linux | `$XDG_CONFIG_HOME/xtop/wasm/` (default `~/.config/xtop/wasm/`) |
| macOS | `~/Library/Application Support/xtop/wasm/` |
| Windows | `%APPDATA%\xtop\wasm\` |

Set `XTOP_WASM_DIR` to override the directory entirely:

```sh
XTOP_WASM_DIR=/opt/xtop-widgets xtop
```

Discovery rules:

- A missing directory is normal — no runtime widgets are registered.
- A module that fails to compile or instantiate is logged to stderr and
  skipped; the remaining modules still load.
- When two modules declare the same manifest name, the first file in sorted
  path order wins and the duplicate is logged.
- The manifest `name` (or the file stem when `name` is empty) becomes both the
  plugin id and the widget name. Layouts reference that name.

## Referencing a widget in a layout

Layouts live in `<config dir>/layouts/` as `.json`/`.jsonc` files and
reference widgets by name:

```jsonc
{
    "name": "WASM Demo",
    "root": {
        "direction": "vertical",
        "areas": [
            { "widget": "header", "size": 3 },
            { "widget": "wasm-clock", "size": "30%" },
            { "widget": "wasm-cpu", "size": "*" }
        ]
    }
}
```

Plugin widgets are resolved before the compiled-in packs, so a module named
`cpu` replaces the built-in `cpu` widget.

## ABI

A guest is a `wasm32` module that exports `memory` and the functions below.
The `export_widget!` macro from `xtop-wasm-guest` generates the five
functions (the wasm target exports `memory` automatically); this table is the
contract for hand-written modules.

| Export | Signature | Meaning |
|---|---|---|
| `memory` | — | Linear memory the host reads and writes. Required. |
| `alloc` | `(i32) -> i32` | Allocate `len` bytes in guest memory; return the pointer, or 0 on failure. |
| `dealloc` | `(i32, i32) -> ()` | Free a buffer previously returned by `alloc`. |
| `manifest` | `() -> i32` | Write the manifest JSON and return its pointer. |
| `render` | `(i32, i32) -> i32` | Parse the `State` JSON at `(ptr, len)`, write the `DrawList` JSON and return its pointer. Return 0 to reject the payload. |
| `result_len` | `() -> i32` | Byte length of the JSON written by the last `manifest`/`render` call. |

The host calls `manifest` once at load and `render` once per tick, then reads
`result_len` immediately after each call and copies the JSON out of guest
memory. The only host import is:

| Import | Signature | Meaning |
|---|---|---|
| `host.log` | `(i32, i32, i32) -> ()` | Log `len` UTF-8 bytes at `ptr`. Levels: `0` debug, `1` info, `2` warn, anything else error. |

Any other import fails instantiation. Guests keep state across ticks in their
own statics (the host does not send histories).

### Manifest

`manifest()` returns the JSON form of `xtop-wasm-contract::Manifest`:

| Field | Default | Notes |
|---|---|---|
| `name` | file stem | Widget name layouts use. Must be unique across the running kernel; an empty value falls back to the file stem. |
| `version` | `""` | Free-form; shown in the `status` action. |
| `description` | `""` | Free-form. |
| `author` | `""` | Free-form. |
| `max_processes` | 50 | Clamped to 1–4096; caps the process list per tick (see [Process cap](#process-cap)). |
| `api` | `""` | Contract version the guest targets. Empty or `"1"` is accepted; any other value logs a warning and still loads. `Manifest::default()` in Rust sets `"1"`. |

### State

Every tick the host sends a `State` object. All coordinates are relative to
the widget area.

| Field | Type | Notes |
|---|---|---|
| `tick` | `u64` | Monotonic tick counter since load; the first tick is 0. |
| `unix_time` | `u64` | Host-provided Unix time in seconds (guests cannot read the clock in the sandbox). |
| `width`, `height` | `u16` | Last rendered widget area; both 0 before the first frame. |
| `config` | object | `theme`, `layout`, `interval_ms`, `hostname`. |
| `alerts` | object | `cpu_high`, `mem_high`, `disk_high`. |
| `snapshot` | object | System sample for this tick (below). |

`snapshot` mirrors the kernel's `SystemSnapshot`:

| Member | Fields |
|---|---|
| `cpus[]` | `name`, `usage`, `cpu_id`, `frequency`, `governor`, `temp_c` |
| `memory` | `total`, `used`, `available`, `free`, `percent` |
| `swap` | `total`, `used`, `free`, `percent` |
| `disks[]` | `mount_point`, `total_space`, `available_space`, `used_space`, `percent`, `file_system`, `mount_options` |
| `networks[]` | `name`, `received`, `transmitted`, `rx_speed`, `tx_speed`, `ip` |
| `processes[]` | `pid`, `name`, `cpu_usage`, `memory`, `user_id`, `state`, `cmd`, `exe_path`, `parent_pid`, `cmd_full`, `start_time`, `run_time`, `effective_user_id`, `group_id`, `cwd`, `thread_count`, `open_files`, `open_files_limit`, `disk_total_read_bytes`, `disk_total_write_bytes`, `environ`, `session_id` |
| `load` | `one`, `five`, `fifteen` |
| `uptime` | seconds |
| `cpu_temp` | degrees |
| `disk_io[]` | `name`, `read_bytes`, `write_bytes`, `read_speed`, `write_speed` |
| `batteries[]` | `name`, `percentage`, `state`, `time_to_full`, `time_to_empty`, `health`, `cycle_count` |
| `gpus[]` | `name`, `usage`, `temperature`, `memory_total`, `memory_used` |
| `sys` | `hostname`, `os_version`, `kernel`, `desktop_env`, `shell`, `cpu_model`, `package_power_w` |

### Draw list

`render` answers with `{"ops": [...]}` — an ordered list of drawing
primitives. Every `rect` is relative to the widget area and clipped by the
host, so oversized rects are safe.

| Op | Fields | Notes |
|---|---|---|
| `block` | `rect`, `border`, `title`, `fg`, `bg` | Bordered box. `border`: `native` (default), `rounded`, `double`, `plain`, `ascii`. |
| `text` | `rect`, `spans`, `align`, `wrap` | `align`: `left` (default), `center`, `right`. `\n` inside a span starts a new line. |
| `gauge` | `rect`, `ratio`, `label`, `fg`, `bg`, `border` | Horizontal gauge; `ratio` is clamped to 0–1. |
| `bar` | same as `gauge` | Single-line gauge. |
| `sparkline` | `rect`, `data`, `fg`, `bg` | `data` values are rounded to integers (negatives become 0). |
| `chart` | `rect`, `datasets`, `x_bounds`, `y_bounds`, `border`, `fg`, `bg`, `marker` | `datasets` items: `name`, `points` (`[x, y]` pairs, Y grows upward), `color`. `x_bounds`/`y_bounds` are `[min, max]`. `marker`: `braille` (default), `dot`, `block`, `half_block`, `bar`. |

`rect` is `{x, y, width, height}` (all default 0). A `span` is `{text, fg, bg,
bold, italic, underlined, dim}` with `text` required and the style flags
defaulting to `false`. Colors are `[r, g, b]` arrays; optional colors may be
`null` or omitted.

Example:

```json
{"ops":[
  {"op":"block","rect":{"x":0,"y":0,"width":30,"height":8},
   "border":"rounded","title":"CPU","fg":[200,200,200],"bg":null},
  {"op":"gauge","rect":{"x":1,"y":1,"width":28,"height":3},
   "ratio":0.42,"label":"42%","fg":[123,216,143],"bg":null,"border":null}
]}
```

## Sandbox

- **Fuel**: 100,000,000 fuel units per host→guest call sequence (a `manifest`
  or `render` call including the `alloc`, `dealloc` and `result_len` calls
  around it). wasmi charges fuel per executed instruction, so a runaway loop
  traps instead of hanging the kernel.
- **Memory**: 64 MiB linear-memory cap per guest instance.
- **Imports**: only `host.log` is linked; any other import fails
  instantiation.
- **Clipping**: every op rect is clipped to the widget area.
- **Isolation**: guests never touch kernel types, ratatui or the terminal.

## Hot reload

On every tick the host compares the file mtime with the one recorded at load
and re-instantiates the module when it changed:

- A failed reload keeps the previous module and its last good draw list; the
  error is logged once per distinct message.
- A successful reload takes the new manifest's `max_processes` and the new
  module renders on that same tick.
- If the new manifest declares a different `name`, the host logs a warning but
  layouts keep referencing the original name.
- The `reload` debug action forces the reload check even when the mtime did
  not change.

## Tick vs render

| Phase | What happens |
|---|---|
| Tick (`on_tick`) | Reload check, build the `State`, call the guest `render` once, cache the returned `DrawList`. |
| Render (frame) | Replay the cached list and record the current widget-area size. Never calls the guest. |

Consequences:

- A slow guest can delay a tick but cannot stall a frame.
- `state.width`/`state.height` are the last rendered area size; both are 0
  before the first frame, so guards for small areas are recommended (the
  examples return early).
- A failed render leaves the last cached list on screen.

## Process cap

`max_processes` (manifest field, default 50, clamped to 1–4096) caps
`snapshot.processes`. The host sorts processes by CPU usage (descending)
before truncating, so the cap keeps the busiest processes. Set it to 1 for
widgets that never read the process list to keep the per-tick JSON small.

## Debug actions

The widget also exposes the standard plugin action API (`status`, `render`,
`reload`), reachable from agents/MCP through `execute_plugin`:

| Action | Returns |
|---|---|
| `status` | `name`, `version`, `path`, `ticks`, `ops`, `last_error`. |
| `render` | The cached draw list as JSON. |
| `reload` | `{"reload":"requested"}` after forcing a reload check. |

## Writing a Rust guest

1. Create a `cdylib` crate that depends on `xtop-wasm-guest`:

```toml
[package]
name = "my-widget"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
# Path used inside the plugins repo; point at the repository elsewhere.
xtop-wasm-guest = { path = "../../plugins/xtop-wasm-guest" }
```

2. Add the wasm target once:

```sh
rustup target add wasm32-unknown-unknown
```

3. Write `src/lib.rs` with `export_widget!`:

```rust
use xtop_wasm_guest::contract::{Align, DrawList, Manifest, Op, Rect, Span, State};
use xtop_wasm_guest::export_widget;

export_widget! {
    manifest: || Manifest {
        name: "my-widget".to_string(),
        description: "hello from wasm".to_string(),
        max_processes: 1,
        ..Manifest::default()
    },
    render: |state: &State| {
        let mut list = DrawList::new();
        list.push(Op::Text {
            rect: Rect::full(state.width, state.height),
            spans: vec![Span::new(format!("tick {}", state.tick))],
            align: Align::Left,
            wrap: false,
        });
        list
    },
}
```

`manifest` is a `Fn() -> Manifest` and `render` is a `Fn(&State) -> DrawList`.
A guest that needs scratch state across ticks keeps it in a `static` (see
[`examples/wasm/cpu`](../examples/wasm/cpu/src/lib.rs)).

4. Build the module:

```sh
cargo build --release --target wasm32-unknown-unknown
```

5. Install it and reference the manifest name in a layout:

```sh
mkdir -p ~/.config/xtop/wasm
cp target/wasm32-unknown-unknown/release/my_widget.wasm ~/.config/xtop/wasm/
```

6. Test the module without the kernel:

```sh
cargo run -p xtop-plugin-wasm --example inspect -- path/to/my_widget.wasm
```

`inspect` loads the module through the same code path the kernel uses, prints
the manifest and renders against a synthetic state (40x12, one CPU, one
process) as JSON. Run it from the `plugins` repository root.

## Troubleshooting

| Symptom | Likely cause |
|---|---|
| Module not registered; stderr says `failed to load` | Missing `memory` export or one of the ABI functions, or the module imports something other than `host.log`. |
| `guest rejected the state payload` | `render` returned 0 (invalid `State` JSON or guest logic). |
| `draw list is not valid JSON` | `result_len` does not describe the JSON written by `render`. |
| `fuel`/`trap` error in `status.last_error` | The guest exceeded the 100M fuel budget (runaway loop). |
| `name already loaded` | Two modules declare the same manifest name; the first sorted path wins. |
| `targets contract X (host speaks 1)` | Manifest `api` mismatch; the module loads anyway. |
| Widget name shows the file stem | The manifest `name` is empty. |
