#!/usr/bin/env bash
# Local --list-only compiles/enumerates; destructive tests execute only in GitHub Actions.
set -euo pipefail
mode="${1:-}"
[[ "$mode" == '' || "$mode" == --list-only ]] || { echo 'usage: check_child_cycle_ci.sh [--list-only]' >&2; exit 2; }
if [[ "$mode" != --list-only ]]; then
  [[ "${GITHUB_ACTIONS:-}" == true ]] || { echo 'child cycle faults require GitHub Actions' >&2; exit 2; }
fi
cd "$(dirname "$0")/.."
prefix=native_tests::boxed_enum_tests::unit_source::owner_multi_root_header_tests::counter_boundary::child_cycle
for name in \
  ci_only_child_cycle_rejects_missing_evidence_and_transport_corruption \
  ci_only_child_cycle_rejects_abort_topology_and_new_owner_corruption \
  ci_only_initial_concat_rejects_source_cleanup_and_transport_corruption; do
  target="$prefix::$name"
  listing=$(cargo test --locked -j1 -p lang-codegen --lib "$target" -- --exact --ignored --list)
  [[ $(grep -Fxc "$target: test" <<< "$listing") == 1 ]]
  if [[ "$mode" != --list-only ]]; then
    cargo test --locked -j1 -p lang-codegen --lib "$target" -- --exact --ignored
  fi
 done
if [[ "$mode" == --list-only ]]; then
  echo 'child cycle CI-only tests: 3 exact targets compiled/listed; none executed'
fi
