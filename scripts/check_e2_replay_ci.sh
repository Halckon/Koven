#!/usr/bin/env bash
# Exact experimental replay matrix. Local operation is compile/list only.
set -euo pipefail
mode="${1:-}"
[[ $# -le 1 && ( "$mode" == '' || "$mode" == --list-only ) ]] || { echo 'usage: check_e2_replay_ci.sh [--list-only]' >&2; exit 2; }
if [[ "$mode" != --list-only ]]; then
  [[ "${CI:-}" == true && "${GITHUB_ACTIONS:-}" == true ]] || { echo 'E2 replay faults require CI=true and GITHUB_ACTIONS=true; use --list-only locally' >&2; exit 2; }
fi
cd "$(dirname "$0")/.."
for features in e2-two-source-test-support 'e2-two-source-test-support loan-observation-test-support'; do
  for name in e2_signature_and_source_roles_reject e2_checked_counter_edges_reject e2_callee_identity_and_child_end_reject; do
    target="ssa::e2_replay_ci::$name"
    listing=$(cargo test --locked -j1 -p lang-codegen --lib --features "$features" "$target" -- --exact --ignored --list)
    [[ $(grep -Fxc "$target: test" <<< "$listing") == 1 ]]
    if [[ "$mode" != --list-only ]]; then
      cargo test --locked -j1 -p lang-codegen --lib --features "$features" "$target" -- --exact --ignored
    fi
  done
done
if [[ "$mode" == --list-only ]]; then
  echo 'E2 replay: 3 exact targets in 2 configurations compiled/listed; none executed'
fi
