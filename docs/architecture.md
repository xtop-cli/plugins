# Samurai plugin architecture

`xtop-plugin-samurai` is the ecosystem's security-monitoring plugin: it
implements the [`Plugin`] contract from `xtop-plugin-api`, runs ten heuristic
rules over the process table, exposes everything through a JSON action API,
and renders one TUI widget. It never depends on the kernel — only on the
contract crate.

The canonical id and action names are exported constants
(`PLUGIN_ID`, `actions::*`) and are the single source for `xtop-extension-mcp`'s
tool table (DR-6).

## Lifecycle

The kernel hosts the plugin through `xtop-plugin-api`:

| Hook | Behavior |
|---|---|
| `manifest()` | id `samurai` (= `PLUGIN_ID`), name `Samurai`, version from the crate version (`env!("CARGO_PKG_VERSION")`, currently 0.2.x), description, capabilities: `ReadSystemInfo`, `KillProcesses`, `ModifyConfig`, `RenderWidgets`. |
| `on_enable()` | marks the plugin enabled. |
| `on_disable()` | marks the plugin disabled. |
| `on_tick()` | counts ticks; **every 5th tick** it replaces the alert list with a fresh analysis run (`tick_count % 5 == 0`). Analysis errors propagate as `PluginError` instead of being swallowed. |
| `execute(action, params)` | JSON action API (below); always records `last_action`/`last_action_result`. |

The plugin starts enabled and reports itself through `plugin.status`.

## Action API

`execute(ctx, action, params)` returns a JSON string (or
`PluginError::{Recoverable, UnknownAction}`). Params use the mini-syntax
below — plain strings, hand-parsed per action.

| Action | Params | Notes |
|---|---|---|
| `system.summary` | — | CPU average, memory GB/percent, process/disk/interface counts, uptime, hostname, current alert count. |
| `processes.top` | `count` or `count,filter=<regex>` | Count defaults to 10 when unparsable; `0` is an error. The optional filter regex is matched with `is_match` against **any** of name, cmd, exe. |
| `processes.search` | `pattern` or `pattern,fields=a,b` | `pattern` is one compiled regex; matched with `is_match` against **any** listed field. Fields: `name`, `cmd`, `user`, `state`, `exe`, `cwd` (default `name`; unknown fields are ignored). Sorted by CPU desc, capped at 100 results. |
| `process.info` | `pid` | Full JSON for one process; error when the PID is absent. |
| `process.kill` | `pid` | Requires `KillProcesses`. |
| `process.alerts` | — | The full alert array of the last analysis run. |
| `threshold.set` | `cpu,mem,disk` | Three comma-separated percentages; requires `ModifyConfig`. |
| `threshold.get` | — | Current `cpu_high`/`mem_high`/`disk_high` thresholds. |
| `config.get` | — | theme, layout, interval_ms, hostname. |
| `config.set` | `interval_ms=<ms>` or `theme=<name>` or `layout=<name>` | Requires `ModifyConfig`; anything else is a parse error. |
| `alerts.status` | — | Total + per-severity counts and the top 5 alerts. |
| `plugin.status` | — | enabled, tick count, last action/result, active + critical alert counts. |

Process JSON entries carry: `pid`, `name`, `cpu`, `mem_bytes`, `state`,
`user`, `cmd`, `exe`, `ppid`, `threads`, `run_time`, `cwd`.

## Heuristic analysis

- Runs **every 5th tick** (~1 s kernel ticks by default, so about every
  5 seconds in wall time — the kernel decides the tick rate).
- Ten rules with the exact thresholds and pattern lists documented in
  [rules.md](rules.md); at most **50 alerts** per run.
- Reads the process table through the capability-gated
  `PluginContext::snapshot()` (`ReadSystemInfo`).
- Spawn-storm history (`spawn_history`) persists across runs inside the
  plugin instance.

## Widget

The plugin registers one plugin widget named `samurai` (the same
`PLUGIN_ID`). It renders a bordered panel with the Samurai title and two
lines of agent status. For the widget to appear the plugin must be enabled
(host feature + `RenderWidgets` capability) and a layout must reference a
widget named `samurai`; the kernel renders plugin widgets ahead of the
built-in pack widgets.

## Relationship to xtop-extension-mcp

The MCP server **no longer lives in this repo** — it is `xtop-extension-mcp`
in the [extensions repo](https://github.com/xtop-cli/extensions), launched
by the kernel as `xtop mcp`. The extension:

- depends on this crate at compile time and uses `PLUGIN_ID` for
  `execute_plugin(PLUGIN_ID, …)`;
- builds its 12-tool table from the `actions::*` constants (tool name =
  action with `.` → `_`);
- translates typed MCP arguments into the params mini-syntax above and maps
  execution failures to JSON-RPC errors.

See the extension's `mcp-protocol.md` for the wire-level mapping.

[`Plugin`]: https://docs.rs/xtop-plugin-api/latest/xtop_plugin_api/trait.Plugin.html
