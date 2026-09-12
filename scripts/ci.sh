#!/usr/bin/env bash
# Local CI for the xtop plugins repo.
#
# Intentionally NOT wired into git: no GitHub Actions, no git hooks. Run it
# yourself from the repo root:
#
#   ./scripts/ci.sh            # run every default stage
#   ./scripts/ci.sh fmt        # run one stage
#
# Default stages: fmt | clippy | check | test
# Optional stages (not in the default run because they need extra toolchains):
#   wasm      -> build the Rust guest examples and load them with the host
#                (requires the wasm32-unknown-unknown target)
#   external  -> drive the Lua/Python/Node example widgets end to end
#                (skips runtimes that are not installed)
#
# The workspace hosts the samurai plugin, the runtime widget hosts (WASM and
# external processes) and the shared contract/replay/guest crates; every
# default stage runs over the whole workspace.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if [[ ! -f Cargo.toml ]]; then
    echo "[ci] no Cargo.toml at repo root; nothing to run."
    exit 0
fi

stages=(fmt clippy check test)
optional=(wasm external)
requested=("$@")
if [[ ${#requested[@]} -eq 0 ]]; then
    requested=("${stages[@]}")
fi

fmt() {
    echo "==> fmt (format check)"
    cargo fmt --all -- --check
}

clippy() {
    echo "==> clippy (workspace, all targets, warnings denied)"
    cargo clippy --workspace --all-targets -- -D warnings
}

check() {
    echo "==> check (workspace)"
    cargo check --workspace
}

test() {
    echo "==> test (workspace)"
    cargo test --workspace
}

wasm() {
    echo "==> wasm (build guest examples + host smoke test)"
    "$ROOT/scripts/test-wasm-e2e.sh"
}

external() {
    echo "==> external (Lua/Python/Node example widgets)"
    "$ROOT/scripts/test-external-e2e.sh"
}

for stage in "${requested[@]}"; do
    case "$stage" in
        fmt) fmt ;;
        clippy) clippy ;;
        check) check ;;
        test) test ;;
        wasm) wasm ;;
        external) external ;;
        *)
            echo "[ci] unknown stage: $stage (expected one of: ${stages[*]} ${optional[*]})" >&2
            exit 1
            ;;
    esac
done

echo "[ci] all stages passed."
