# xtop WASM widget examples

Six minimal guests built against the [WASM runtime widget contract](../../docs/wasm-widgets.md):
four in Rust with `xtop-wasm-guest` and two freestanding C modules that
implement the ABI by hand. All are the smallest complete examples of the
contract: a manifest plus a render function compiled to `wasm32`.

| Crate | Manifest name | Language | Renders | `max_processes` |
|---|---|---|---|---|
| `clock` | `wasm-clock` | Rust | Rounded block, UTC clock from `unix_time`, uptime line | 1 |
| `cpu` | `wasm-cpu` | Rust | Rounded block, CPU gauge, sparkline from guest-side history | 1 |
| `procs` | `wasm-procs` | Rust | Rounded block, top-process text table | 40 |
| `load-stats` | `wasm-load-stats` | Rust | Load gauge + chart (raw + EMA) and mean/sigma/z-score footer | 1 |
| `c-ticker` | `c-ticker` | C (freestanding) | Rounded block, tick counter and uptime parsed from the state JSON | 1 |
| `c-cpu-avg` | `c-cpu-avg` | C (freestanding) | Per-core usage bars + mean/min/max footer from `"usage":` scanning | 1 |

This directory is its own Cargo workspace, excluded from the repository
workspace, so host tests do not build the guests.

## Build

From this directory:

```sh
rustup target add wasm32-unknown-unknown
cargo build --release --target wasm32-unknown-unknown
```

Artifacts (crate names with `-` become `_` in the file names):

```
target/wasm32-unknown-unknown/release/xtop_wasm_example_clock.wasm
target/wasm32-unknown-unknown/release/xtop_wasm_example_cpu.wasm
target/wasm32-unknown-unknown/release/xtop_wasm_example_procs.wasm
target/wasm32-unknown-unknown/release/xtop_wasm_example_load_stats.wasm
```

The workspace release profile is size-tuned (`opt-level = "s"`, LTO,
`panic = "abort"`, stripped symbols), since wasm guests are size-sensitive.

The C example builds with clang + wasm-ld (no libc, no WASI — the host only
links `host.log`, so any other import would fail instantiation):

```sh
./c-ticker/build.sh        # writes c-ticker/c-ticker.wasm (~4 KB)
```

`widget.c` implements `alloc`, `dealloc`, `manifest`, `render` and
`result_len` by hand and scans the few `State` fields it needs out of the
JSON without a JSON library. It is the reference for hand-written modules in
any language that compiles to wasm32 without WASI (C, C++, Zig, ...).

## Smoke test with `inspect`

From the `plugins` repository root:

```sh
cargo run -p xtop-plugin-wasm --example inspect -- \
  examples/wasm/target/wasm32-unknown-unknown/release/xtop_wasm_example_clock.wasm
```

`inspect` loads the module through the same code path the kernel uses, asks
for the manifest, renders against a synthetic state (40x12, one CPU at 42%,
one process) and prints `{"manifest": ..., "draw": ...}` as pretty JSON.

Expected `wasm-clock` output (trimmed):

```json
{
  "manifest": {
    "name": "wasm-clock",
    "version": "0.1.0",
    "description": "UTC clock from the host-provided unix time",
    "author": "xtop-cli",
    "max_processes": 1,
    "api": "1"
  },
  "draw": {
    "ops": [
      { "op": "block", "rect": { "x": 0, "y": 0, "width": 40, "height": 12 },
        "border": "rounded", "title": "wasm clock", "fg": null, "bg": null },
      { "op": "text", "rect": { "x": 1, "y": 1, "width": 38, "height": 1 },
        "spans": [{ "text": "22:13:20", "fg": [90, 212, 230], "bold": true }],
        "align": "center", "wrap": false },
      { "op": "text", "rect": { "x": 1, "y": 2, "width": 38, "height": 1 },
        "spans": [{ "text": "up 00h 20m", "dim": true }],
        "align": "center", "wrap": false }
    ]
  }
}
```

The clock string comes from the synthetic `unix_time` (1700000000); the
uptime line comes from the synthetic snapshot (1234 seconds).

The other two modules use the same shape:

- `wasm-cpu`: `draw.ops` = `block`, `gauge` (`ratio` 0.42, label `42.0%`) and
  `sparkline` (guest-side history with one point).
- `wasm-procs`: `draw.ops` = `block` and a single `text` op whose spans are
  the bold `PID NAME CPU% MEM` header plus one row for the synthetic `init`
  process.

Run the same command with the other two `.wasm` paths to check them. On
failure `inspect` prints the error to stderr and exits non-zero.

## Install

```sh
mkdir -p ~/.config/xtop/wasm
cp target/wasm32-unknown-unknown/release/*.wasm ~/.config/xtop/wasm/
```

Layouts reference the manifest names (`wasm-clock`, `wasm-cpu`, `wasm-procs`),
not the file names. Build/run xtop with the `plugin-wasm` feature to load
them; see [`docs/wasm-widgets.md`](../../docs/wasm-widgets.md) for the ABI,
sandbox limits and the install directories on macOS and Windows.
