#!/usr/bin/env bash
# Explicit ignored owner faults run once on each CI host, never in local composition.
set -euo pipefail
[[ "${GITHUB_ACTIONS:-}" == true ]] || { echo 'parent receipt faults require GitHub Actions' >&2; exit 2; }
cd "$(dirname "$0")/.."
prefix=native_tests::boxed_enum_tests::unit_source::owner_multi_root_header_tests::counter_boundary::nested_body::parent_false
for name in \
  ci_only_parent_header_receipt_rejects_actual_edge_corruption \
  ci_only_parent_false_receipt_rejects_actual_partition_and_consumption_corruption; do
  target="$prefix::$name"
  listing=$(cargo test --locked -p lang-codegen --lib "$target" -- --exact --ignored --list)
  [[ $(grep -Fxc "$target: test" <<< "$listing") == 1 ]]
  cargo test --locked -p lang-codegen --lib "$target" -- --exact --ignored
 done
