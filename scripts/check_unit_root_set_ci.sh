#!/usr/bin/env bash
# Experimental Unit root-set admission, budget and publication: ignored and CI-only; local listing is safe.
set -euo pipefail
mode="${1:-}"
[[ "$mode" == '' || "$mode" == --list-only ]] || { echo 'usage: check_unit_root_set_ci.sh [--list-only]' >&2; exit 2; }
if [[ "$mode" != --list-only ]]; then
  [[ "${GITHUB_ACTIONS:-}" == true ]] || { echo 'root-set negative controls require GitHub Actions' >&2; exit 2; }
fi
cd "$(dirname "$0")/.."
for target in \
  ssa::unit_plan::root_set_tests::ci_only_root_selection_rejects_empty_duplicate_and_generic \
  ssa::unit_plan::root_set_tests::ci_only_common_roots_share_budget_and_recover \
  ssa::unit_lower::orchestrate::root_set::publication_tests::ci_only_common_roots_final_verifier_failure_rolls_back_and_recovers; do
listing=$(cargo test --locked -p lang-codegen --lib --features loan-observation-test-support "$target" -- --exact --ignored --list)
[[ $(grep -Fxc "$target: test" <<< "$listing") == 1 ]]
if [[ "$mode" == --list-only ]]; then
  echo 'Unit root-set CI target compiled/listed; none executed'
else
  result=$(cargo test --locked -p lang-codegen --lib --features loan-observation-test-support "$target" -- --exact --ignored)
  printf '%s\n' "$result"
  [[ $(grep -Fxc "test $target ... ok" <<< "$result") == 1 ]]
  grep -Eq '^test result: ok\. 1 passed; 0 failed; 0 ignored;' <<< "$result"
fi
done
