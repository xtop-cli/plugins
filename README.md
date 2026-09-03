# xtop plugins

Official repository for native and community Xtop plugins.

## Workspace

Each plugin lives in its own folder under `plugins/`:

```
plugins/
  xtop-plugin-<name>/
    Cargo.toml
    src/
    README.md
```

## How plugins work

- Every plugin is published/installed independently and integrated into the
  kernel through `xtop-plugin-api`.
- The kernel enables plugins optionally (feature flags for built-ins, runtime
  discovery for external ones). A plain `xtop` build never requires this repo.
- Install a plugin from the kernel with: `xtop plugin install <name>`
  (default source: https://github.com/xtop-cli/plugins)

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
