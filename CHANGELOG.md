# Changelog

## [0.1.0] - 2026-09-12

Version policy note: the ecosystem stays early — everything is 0.1.0. This
entry records the runtime widget work that joined the repo on top of the
existing `xtop-plugin-samurai` crate.

### Runtime widget hosts (opt-in)

- New crate **`xtop-plugin-wasm`** (`xtop-plugin-wasm`, feature
  `plugin-wasm` in the kernel): loads sandboxed `*.wasm` widgets in-process
  with wasmi. Discovery in `<config dir>/wasm/` (override `XTOP_WASM_DIR`),
  mtime hot reload, `status`/`render`/`reload` debug actions, and an
  `examples/inspect.rs` harness that runs a module without the kernel.
  Sandbox limits: 100M fuel per host→guest call sequence, 64 MiB linear
  memory, only the `host.log` import linked.
- New crate **`xtop-plugin-external`** (`xtop-plugin-external`, feature
  `plugin-external`): one helper process per widget over line-delimited
  JSON. Descriptors in `<config dir>/external/*.json` (override
  `XTOP_EXTERNAL_DIR`), per-response timeout, last-good draw list on
  failures, clean shutdown. Any language with a runtime can implement a
  widget.
- New shared crates:
  - **`xtop-wasm-contract`** — serializable `Manifest`/`State`/`DrawList`
    contract plus the external `Request`/`Response` protocol (ABI `"1"`).
  - **`xtop-widget-replay`** — host-side replay of draw lists onto a ratatui
    frame and the plugin-context → contract `State` conversion.
  - **`xtop-wasm-guest`** — Rust guest SDK (`export_widget!` macro, ABI
    exports, `host.log` usage) for building widgets to
    `wasm32-unknown-unknown`.
- Kernel integration is additive and opt-in: the two hosts are non-default
  features, register through the existing plugin widget path (precedence
  over compiled-in packs), and leave the compiled-in plugin/pack system
  untouched. See `docs/decisions.md` (ADR-001).

### Examples

- `examples/wasm/` — Rust guests `clock`, `cpu`, `procs` and `load-stats`
  (statistics: EMA, z-score), plus freestanding C guests `c-ticker` and
  `c-cpu-avg` built with clang + wasm-ld and no libc/WASI.
- `examples/external/` — Lua (`lua-clock`, `lua-histogram`), Python
  (`python-cpu`, `python-cpu-chart`, `python-mem-regression`) and Node
  (`node-clock`) helper widgets, all dependency-free.

### Tooling

- `scripts/build-examples.sh` — builds every guest (Rust wasm32 + C) and can
  smoke-test them through the real host loader.
- `scripts/test-wasm-e2e.sh`, `scripts/test-external-e2e.sh` — end-to-end
  checks of both lanes (skip runtimes that are not installed).
- `scripts/ci.sh` — new optional stages `wasm` and `external`; the default
  stages (`fmt`, `clippy`, `check`, `test`) are unchanged.

### Documentation

- `docs/wasm-widgets.md` — WASM host: ABI, sandbox limits, draw-list ops,
  hot reload, authoring guide.
- `docs/external-widgets.md` — external host: descriptors, protocol,
  lifecycle, timeouts.
- `docs/decisions.md` — ADR-001 (runtime widget hosts).
- `examples/wasm/README.md`, `examples/external/README.md` — per-example
  build/run instructions.

### Compatibility

- No breaking changes to `xtop-plugin-samurai` or to the `xtop-plugin-api`
  contract. The new crates are additive workspace members.
- The guest-facing ABI is versioned (`"1"`); guests declaring a different
  `api` value load with a warning.
