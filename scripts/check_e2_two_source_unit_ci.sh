#!/usr/bin/env bash
# Formal opt-in capability: exact normal and failure-atomicity targets.
set -euo pipefail
mode="${1:-}"
[[ $# -le 1 && ( "$mode" == '' || "$mode" == --list-only ) ]] || { echo 'usage: check_e2_two_source_unit_ci.sh [--list-only]' >&2; exit 2; }
if [[ "$mode" != --list-only ]]; then
  [[ "${CI:-}" == true && "${GITHUB_ACTIONS:-}" == true ]] || { echo 'Two-source Unit faults require CI; use --list-only locally' >&2; exit 2; }
fi
cd "$(dirname "$0")/.."
for features in two-source-unit 'two-source-unit e2-two-source-test-support loan-observation-test-support'; do
  for name in explicit_capability_checks_whole_unit_and_source_identity explicit_capability_executes_original_object explicit_capability_commit_failure_is_atomic multiple_entries_preserve_complete_selection_and_mapping multiple_entries_commit_failure_is_atomic multiple_entries_reject_same_fact_and_unsupported_entry named_shared_owner_entry_is_closed_and_unique; do
    target="ssa::two_source_unit::tests::$name"
    args=(--exact)
    [[ "$name" != *_commit_failure_is_atomic ]] || args+=(--ignored)
    listing=$(cargo test --locked -j1 -p lang-codegen --lib --features "$features" "$target" -- "${args[@]}" --list)
    [[ $(grep -Fxc "$target: test" <<< "$listing") == 1 ]]
    if [[ "$mode" != --list-only ]]; then
      cargo test --locked -j1 -p lang-codegen --lib --features "$features" "$target" -- "${args[@]}"
    fi
  done
  for public in public_capability_emits_and_executes_without_test_observers public_multiple_entries_execute_one_common_program public_named_shared_owner_executes_exact_source; do
    listing=$(cargo test --locked -j1 -p lang-codegen --test two_source_unit --features "$features" "$public" -- --exact --list)
    [[ $(grep -Fxc "$public: test" <<< "$listing") == 1 ]]
    if [[ "$mode" != --list-only ]]; then
      cargo test --locked -j1 -p lang-codegen --test two_source_unit --features "$features" "$public" -- --exact
    fi
  done
done
frontend=ownership_checking::compilation_unit::loop_no_carry::two_source_candidate::tests::named_shared_string_entry_cleanup_evidence
listing=$(cargo test --locked -j1 -p lang-frontend --lib --features 'e2-two-source-test-support loan-observation-test-support' "$frontend" -- --exact --list)
[[ $(grep -Fxc "$frontend: test" <<< "$listing") == 1 ]]
if [[ "$mode" != --list-only ]]; then
  cargo test --locked -j1 -p lang-frontend --lib --features 'e2-two-source-test-support loan-observation-test-support' "$frontend" -- --exact
fi
if [[ "$mode" == --list-only ]]; then
  echo 'Two-source Unit: 7 unit and 3 public-library targets in 2 configurations plus 1 frontend evidence target listed; none executed'
fi
