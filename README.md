<h1 align="center">
<img src="https://raw.githubusercontent.com/xtop-cli/web/main/public/img/logo.png" width="100px" alt="Xtop logo" />Plugins</h1>

Official repository for native and community Xtop plugins.

## Workspace

Every crate lives under `plugins/`:

```
plugins/
  xtop-plugin-samurai/     native plugin (compile-time, xtop-plugin-api)
  xtop-plugin-wasm/        runtime widget host: sandboxed WASM modules
  xtop-plugin-external/    runtime widget host: helper processes
  xtop-wasm-contract/      serializable state/draw-list contract
  xtop-widget-replay/      host-side draw-list replay + state conversion
  xtop-wasm-guest/         Rust guest SDK for wasm32 widgets
```

Examples live under `examples/` (`wasm/` and `external/`). Changes are
recorded in [CHANGELOG.md](CHANGELOG.md); design decisions in
[docs/decisions.md](docs/decisions.md).

## How plugins work

- Every plugin is published/installed independently and integrated into the
  kernel through `xtop-plugin-api`.
- The kernel enables plugins optionally (feature flags for built-ins, runtime
  discovery for external ones). A plain `xtop` build never requires this repo.
- Install a plugin from the kernel with: `xtop plugin install <name>`
  (default source: https://github.com/xtop-cli/plugins)

## Runtime widgets

Two optional kernel features open the widget surface to code that is **not**
compiled into the kernel. Both register widgets through the normal plugin
path, so a runtime widget takes precedence over every compiled-in pack:

- `xtop-plugin-wasm` (`--features plugin-wasm`) loads sandboxed `*.wasm`
  modules in-process with wasmi. Widgets are discovered in `wasm/` under the
  user config dir (or `XTOP_WASM_DIR`). See
  [docs/wasm-widgets.md](docs/wasm-widgets.md).
- `xtop-plugin-external` (`--features plugin-external`) spawns helper
  processes that speak line-delimited JSON on stdin/stdout. Widgets are
  discovered in `external/` under the user config dir (or
  `XTOP_EXTERNAL_DIR`). See [docs/external-widgets.md](docs/external-widgets.md).

Neither feature is part of the default `xtop` build. Working examples live in
[`examples/`](examples/): Rust WASM guests in
[`examples/wasm/`](examples/wasm/) and Lua/Python/Node helper processes in
[`examples/external/`](examples/external/). Demo layouts that arrange these
widgets live in the `xtop-cli/layouts` repo under `layouts/custom/` and
install with `xtop layout install <name>`. The rationale and trade-offs are
recorded in [docs/decisions.md](docs/decisions.md) (ADR-001).

## Getting started (development)

From this repo root:

```bash
cargo build --workspace
```

During active development all repos live side by side and use local path
dependencies:

```
xtop/           kernel
api/            API crates
plugins/        this repo
effects/
extensions/
```

## License

MIT
