#!/usr/bin/env bash
# Complete bounded composition. No caller-selectable skip controls.
set -euo pipefail
cd "$(dirname "$0")/.."
# Diagnostics are CI/Linux-only and cannot replace the composition exit status.
if [[ "${GITHUB_ACTIONS:-}" == true && "${RUNNER_OS:-}" == Linux ]]; then
  python3 scripts/ci_disk_usage.py after-cache || true
  python3 scripts/ci_disk_usage.py monitor &
  disk_monitor=$!
  trap 'kill "$disk_monitor" 2>/dev/null || true; wait "$disk_monitor" 2>/dev/null || true' EXIT
fi
bash scripts/check_core.sh
cargo test --locked -p lang-frontend --test ownership_iteration
bash scripts/check_stage_integration.sh
# Core already executes all codegen tests; stage owns the other Guide targets.
cargo test --locked -p lang-frontend --test guide_litmus
python3 scripts/check_tutorial.py
python3 scripts/check_kovenlings.py
python3 scripts/check_api_examples.py > /dev/null
