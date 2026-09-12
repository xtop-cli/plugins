# Design decisions

Short log of the decisions that shape this repo and its crates. Grounded in
this repo's code and in the kernel sources referenced below.

## ADR-001 — Runtime widget hosts: opt-in WASM and external processes behind one draw-list contract

**Status.** Accepted. Implemented in this repo; the kernel consumes the two
hosts behind the non-default features `plugin-wasm` and
`plugin-external`. Supersedes nothing; the deferred alternative (native
ABI discovery of `xtop-plugin-*` in config dirs) stays in the kernel
ROADMAP §7.

### Context

The ecosystem is compile-time by design: plugins, widget packs, extensions
and effects are Cargo features plus floating git dependencies, and a plain
`xtop` build never requires any sibling repo. Third-party widget code
therefore needs to edit the kernel `Cargo.toml` and recompile — an
acceptable bar for first-party work, a hard wall for widget authors in other
languages.

Constraints in force:

- Single-crate kernel; ratatui `0.30.2`; `rust-version = "1.87"`; edition
  2021; no tokio/clap/chrono.
- Contracts live only in the `api` repo (DR-1..DR-5); the kernel never
  depends on sibling repos at runtime.
- Layouts address widgets by unvalidated string names; resolution is
  runtime: plugin widgets first, then the chosen pack, then the base pack
  (`layouts/docs/decisions.md` D-2, kernel `ui/layout/engine.rs`).
- Local-only CI; consumer git deps stay floating this cycle; temporary path
  deps are allowed during development and must be reverted before push
  (`xtop/docs/multi-repo.md`).

**Problem.** Let third parties ship widgets without recompiling the kernel,
without breaking plugin-widget precedence or the layout name contract, and
without forcing every user to pay for a runtime engine.

**Evaluated alternatives**

| # | Alternative | Pros | Cons |
|---|---|---|---|
| 1 | Stay compile-time only | Zero runtime cost; simplest threat model | Every widget ships in the build; third parties must fork/edit and recompile |
| 2 | Single in-process WASM host | Strong sandbox; portable; one engine | Needs a wasm toolchain; no OS access; wasmi dependency even when unused |
| 3 | Single external-process host | Any language/runtime; crash containment; no new kernel deps | Per-tick IPC latency + timeouts; process lifecycle; runs with user privileges |
| 4 | Native dynamic loading (dlopen/ABI) | Native speed; lowest overhead | ABI/versioning and crash-safety burden; platform-specific loader; deferred by the roadmap |
| 5 | **Two hosts behind one shared contract** | Best of 2+3; one registration path and one wire format; opt-in | Two protocols and two failure modes to maintain and document |

### Decision

Adopt alternative 5.

- Two **optional, non-default** kernel features: `plugin-wasm` (wasmi 2,
  in-process) and `plugin-external` (one helper process per widget,
  line-delimited JSON over stdin/stdout). The default build is unchanged.
- Both hosts register through the normal plugin path
  (`xtop_plugin_api::Plugin` / `PluginWidget`), so runtime widgets take
  precedence over every compiled-in pack and are referenced by name in
  layouts. Discovery: `<config dir>/wasm/*.wasm` and
  `<config dir>/external/*.json`, overridable with `XTOP_WASM_DIR` /
  `XTOP_EXTERNAL_DIR`; on duplicate names the first sorted path wins.
- Shared contract in `xtop-wasm-contract` (`Manifest`, `State`, `DrawList`
  ops: block/text/gauge/bar/sparkline/chart, ABI `"1"`); host-side replay in
  `xtop-widget-replay`; Rust guest SDK in `xtop-wasm-guest`
  (`export_widget!`). Contract changes must stay backward compatible.
- WASM sandbox: 100M fuel per host→guest call sequence, 64 MiB linear-memory
  cap, only the `host.log` import linked, rect clipping, mtime hot reload
  with last-good cache. Guests render once per tick; frames replay the cached
  list.
- External protocol: `manifest`/`render`/`shutdown` requests,
  `draw`/`log` responses; per-response timeout clamped to 100–60000 ms
  (default 2000); no hot reload; shutdown kills and reaps the child. The
  process is **not** sandboxed (documented).
- Per-tick payload bound: `max_processes` (default 50, clamped 1–4096),
  CPU-descending sort before truncation.
- Distribution: the hosts live in this repo; the kernel consumes them as git
  dependencies (`xtop-cli/plugins`), so a clean kernel checkout builds
  without sibling repos.

### Consequences

**Positive**

- Third-party widgets in Rust/WASM, C, Lua, Python or Node without a kernel
  recompile; `cargo build --no-default-features` remains the pure core.
- Reuse: one widget-registration path, one state/draw contract, one replay
  implementation; no layout format change.
- Safety and robustness: WASM guests cannot touch kernel types, ratatui or
  the terminal; external crashes are contained outside the kernel; failures
  fall back to the last good draw list and log once.
- Testability: guests run without the kernel
  (`cargo run -p xtop-plugin-wasm --example inspect`), with optional
  `wasm` / `external` CI stages in `scripts/ci.sh`.

**Negative / trade-offs**

- Performance: a guest `render` runs inside the tick and can delay it
  (bounded by fuel for WASM, by the timeout for external); frames never call
  guests, so rendering cannot stall.
- Security: external widgets run with user privileges (explicitly not a
  sandbox); WASM shares the host process, so fuel/memory/import limits are
  guardrails, not a security boundary.
- Maintenance: two protocols plus ABI v1 to version, a duplicate-name
  policy, hot reload only for WASM; the draw contract becomes a
  compatibility surface.
- Operations: config-dir discovery, env overrides, and `--all-features`
  now pulls wasmi and process spawning (installation docs say so).
- Release: the kernel consumes both hosts as git dependencies
  (`https://github.com/xtop-cli/plugins`); there is no path-dependency window
  to revert.
