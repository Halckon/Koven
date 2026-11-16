#!/usr/bin/env bash
# Only compile/enumerate locally; real scalar-entry corruption belongs to CI.
set -euo pipefail
mode="${1:-}"
[[ "$mode" == '' || "$mode" == --list-only ]] || { echo 'usage: check_scalar_owner_entry_ci.sh [--list-only]' >&2; exit 2; }
if [[ "$mode" != --list-only ]]; then
  [[ "${GITHUB_ACTIONS:-}" == true ]] || { echo 'scalar owner entry faults require GitHub Actions' >&2; exit 2; }
fi
cd "$(dirname "$0")/.."
prefix=native_tests::boxed_enum_tests::unit_source::owner_multi_root_header_tests::counter_boundary::scalar_entry
for name in \
  ci_only_scalar_entry_rejects_actual_source_transport_and_escape \
  ci_only_concat_scalar_entry_rejects_cleanup_and_diamond_corruption; do
  target="$prefix::$name"
  listing=$(cargo test --locked -j1 -p lang-codegen --lib "$target" -- --exact --ignored --list)
  [[ $(grep -Fxc "$target: test" <<< "$listing") == 1 ]]
  if [[ "$mode" != --list-only ]]; then
    cargo test --locked -j1 -p lang-codegen --lib "$target" -- --exact --ignored
  fi
done
if [[ "$mode" == --list-only ]]; then
  echo 'scalar owner entry CI-only tests: 2 exact targets compiled/listed; none executed'
fi
