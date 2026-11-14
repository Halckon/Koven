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
  for target in ssa::e2_replay_ci::e2_signature_and_source_roles_reject ssa::e2_replay_ci::e2_checked_counter_edges_reject ssa::e2_replay_ci::e2_callee_identity_and_child_end_reject ssa::e2_entry_ci::entry_semantic_and_final_gate_failures_publish_nothing ssa::e2_entry_ci::checked_entry_failures_publish_nothing ssa::e2_entry_ci::joint_entry_mapping_and_partial_failures_publish_nothing; do
    listing=$(cargo test --locked -j1 -p lang-codegen --lib --features "$features" "$target" -- --exact --ignored --list)
    [[ $(grep -Fxc "$target: test" <<< "$listing") == 1 ]]
    if [[ "$mode" != --list-only ]]; then
      cargo test --locked -j1 -p lang-codegen --lib --features "$features" "$target" -- --exact --ignored
    fi
  done
done
if [[ "$mode" == --list-only ]]; then
  echo 'E2 replay: 6 exact targets in 2 configurations compiled/listed; none executed'
fi
