# xtop-plugin-samurai

Samurai is an AI-aware system monitoring and management plugin for
[xtop](https://github.com/xtop-cli/xtop). It exposes system metrics, process
information and runtime configuration through a JSON action API, runs ten
heuristic threat-detection rules over the process table, and renders a small
TUI widget.

The plugin is a library implementing the `Plugin` trait from
[`xtop-plugin-api`](https://github.com/xtop-cli/api) — the kernel hosts it
via a feature flag; it does not ship a binary of its own.

## Features

- JSON action API over a single `execute()` entry point: system summary,
  process queries (top by CPU, regex search, per-PID info), process kill,
  alert thresholds (get/set), runtime config (get/set), alert status and
  plugin status.
- Heuristic process analysis with **10 rules** (suspicious paths, orphan
  processes, masquerading, EUID/UID escalation, browser children, known
  miner/rootkit names, pipe/download command lines, high thread/FD counts,
  spawn storms) — runs every 5th tick, capped at **50 alerts** per run.
- Alert and status data exposed as JSON for scripts, agents and the MCP
  extension.
- One plugin widget named `samurai` for custom TUI layouts.

## Ecosystem constants

This crate is the single source of truth (DR-6) for:

- `PLUGIN_ID` — the hosted plugin id (`"samurai"`), used by the kernel and
  by the MCP extension for `execute_plugin`.
- `actions::*` — the 12 action names understood by `execute()`:
  `SYSTEM_SUMMARY`, `PROCESSES_TOP`, `PROCESSES_SEARCH`, `PROCESS_INFO`,
  `PROCESS_KILL`, `PROCESS_ALERTS`, `THRESHOLD_SET`, `THRESHOLD_GET`,
  `CONFIG_GET`, `CONFIG_SET`, `ALERTS_STATUS`, `PLUGIN_STATUS`.

## Action API

Every interaction goes through:

```rust
plugin.execute(ctx, "<action>", "<params>") // -> Result<String, PluginError>
```

The response is always a JSON string. Action names, params and response
shapes are documented in [docs/architecture.md](../../docs/architecture.md);
quick reference:

| Action | Params |
|---|---|
| `system.summary` | (none) |
| `processes.top` | `count` or `count,filter=<regex>` |
| `processes.search` | `pattern` or `pattern,fields=name,cmd,user,state,exe,cwd` |
| `process.info` | `pid` |
| `process.kill` | `pid` |
| `process.alerts` | (none) |
| `threshold.set` | `cpu,mem,disk` (percentages) |
| `threshold.get` | (none) |
| `config.get` | (none) |
| `config.set` | `interval_ms=<ms>`, `theme=<name>` or `layout=<name>` |
| `alerts.status` | (none) |
| `plugin.status` | (none) |

**Regex semantics.** Search and filter patterns are passed straight to
`regex::Regex::new` — no `/…/` wrapping. Each pattern is one compiled regex,
matched with `is_match()` against **any** of the fields you select
(`fields=` on `processes.search`; name/cmd/exe on the `processes.top`
filter). Examples:

```
processes.search("python|node")                    # name contains python or node
processes.search("^1000$,fields=user")             # user id match
processes.top("5,filter=nginx")                    # top 5 processes named nginx
```

## Heuristic analysis

The analyzer runs **every 5th tick** (the kernel ticks about once per
second, so roughly every 5 seconds) and keeps up to **50 alerts** per run
across **10 rules**. The rules and their exact thresholds are documented in
[docs/rules.md](../../docs/rules.md); summary:

1. executable from a suspicious path (`/tmp`, `/dev/shm`, `/var/tmp`, …)
2. orphan process (PPID=1) that is not a known daemon
3. masquerading (system name off canonical path, or name/exe mismatch)
4. EUID != UID privilege escalation
5. unknown child of a browser process
6. known miner/rootkit names or command patterns (`xmrig`, `minerd`,
   `pool.monero`, …)
7. pipe/download command lines (`curl … | sh`, `base64 -d |`, …)
8. thread count >= 500 (not an allow-listed server/browser)
9. open file descriptors >= 1000 (not an allow-listed server/browser)
10. spawn storm: more than 5 fresh instances of a name within 120 s

## TUI widget

The plugin registers a widget named `samurai` (equal to `PLUGIN_ID`). To see
it, enable the plugin (it needs the `RenderWidgets` capability) and add
`"samurai"` to a layout:

```jsonc
{
    "name": "monitor",
    "root": {
        "direction": "vertical",
        "areas": [
            { "widget": "header", "size": 3 },
            { "widget": "samurai", "size": 6 },
            { "widget": "processes", "size": "*" }
        ]
    }
}
```

## MCP integration

Samurai itself has **no built-in MCP server**. AI tool access is provided by
the `xtop-extension-mcp` crate in the [extensions repo](https://github.com/xtop-cli/extensions),
which the kernel launches with `xtop mcp`. It depends on this crate at
compile time, drives the plugin with `execute_plugin(PLUGIN_ID, action,
params)` and derives its 12 tools from the `actions::*` constants — see
`docs/mcp-protocol.md` in that repo for the wire mapping.

## Documentation

- [docs/architecture.md](../../docs/architecture.md) — lifecycle, action
  API and params syntax, capabilities, widget, MCP relationship.
- [docs/rules.md](../../docs/rules.md) — the 10 heuristic rules with exact
  thresholds and trigger patterns.

## Development

```bash
cargo build --workspace
./scripts/ci.sh        # fmt | clippy | check | test
```

## License

MIT
