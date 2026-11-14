#!/usr/bin/env bash
# Cross-analysis negative controls are executed only by CI.
set -euo pipefail
mode="${1:-}"
[[ "$mode" == '' || "$mode" == --list-only ]] || { echo 'usage: check_d10_catalog_identity_ci.sh [--list-only]' >&2; exit 2; }
if [[ "$mode" != --list-only ]]; then
  [[ "${GITHUB_ACTIONS:-}" == true ]] || { echo 'catalog identity controls require GitHub Actions' >&2; exit 2; }
fi
cd "$(dirname "$0")/.."
target=ownership_checking::compilation_unit::dataflow::drop_planner::owner_transport::tests::origin_probe::ci_only_catalog_authorization_rejects_foreign_inputs
listing=$(cargo test --locked -j1 -p lang-frontend --lib --features loan-observation-test-support "$target" -- --exact --ignored --list)
[[ $(grep -Fxc "$target: test" <<< "$listing") == 1 ]]
if [[ "$mode" != --list-only ]]; then
  cargo test --locked -j1 -p lang-frontend --lib --features loan-observation-test-support "$target" -- --exact --ignored
else
  echo 'catalog identity CI-only test compiled/listed; not executed'
fi

bridge=ci_only_sidecar_rejects_another_ownership_run
listing=$(cargo test --locked -j1 -p lang-frontend --features d10-catalog-experiment --test d10_catalog_handoff "$bridge" -- --exact --ignored --list)
[[ $(grep -Fxc "$bridge: test" <<< "$listing") == 1 ]]
if [[ "$mode" != --list-only ]]; then
  cargo test --locked -j1 -p lang-frontend --features d10-catalog-experiment --test d10_catalog_handoff "$bridge" -- --exact --ignored
else
  echo 'final-owned sidecar identity CI-only test compiled/listed; not executed'
fi
