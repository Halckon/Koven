#!/usr/bin/env bash
# Legacy owner-event faults: default ignored, exact CI execution only.
set -euo pipefail
mode="${1:-}"
[[ "$mode" == '' || "$mode" == --list-only ]] || { echo 'usage: check_bridge_scalar_entry_ci.sh [--list-only]' >&2; exit 2; }
if [[ "$mode" != --list-only ]]; then
  [[ "${GITHUB_ACTIONS:-}" == true ]] || { echo 'bridge scalar entry faults require GitHub Actions' >&2; exit 2; }
fi
cd "$(dirname "$0")/.."
target=ownership_checking::compilation_unit::dataflow::drop_planner::owner_transport::loops::counter_segment::entry::tests::ci_only_bridge_scalar_entry_requires_actual_ordered_completion
listing=$(cargo test --locked -j1 -p lang-frontend --lib "$target" -- --exact --ignored --list)
[[ $(grep -Fxc "$target: test" <<< "$listing") == 1 ]]
if [[ "$mode" == --list-only ]]; then
  echo 'bridge scalar entry CI-only test: exact ignored target compiled/listed; none executed'
else
  cargo test --locked -j1 -p lang-frontend --lib "$target" -- --exact --ignored
fi
