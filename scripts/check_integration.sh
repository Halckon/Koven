#!/usr/bin/env bash
# Complete bounded composition. No caller-selectable skip controls.
set -euo pipefail
cd "$(dirname "$0")/.."
bash scripts/check_core.sh
cargo test --locked -p lang-frontend --test ownership_iteration
bash scripts/check_stage_integration.sh
# Core already executes all codegen tests; stage owns the other Guide targets.
cargo test --locked -p lang-frontend --test guide_litmus
python3 scripts/check_tutorial.py
