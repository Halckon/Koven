#!/usr/bin/env bash
# Cross-analysis and object-atomicity negative controls are executed only by CI.
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

loop_bridge=ci_only_loop_sidecar_rejects_another_ownership_run
listing=$(cargo test --locked -j1 -p lang-frontend --features d10-catalog-experiment --test d10_catalog_handoff "$loop_bridge" -- --exact --ignored --list)
[[ $(grep -Fxc "$loop_bridge: test" <<< "$listing") == 1 ]]
if [[ "$mode" != --list-only ]]; then
  cargo test --locked -j1 -p lang-frontend --features d10-catalog-experiment --test d10_catalog_handoff "$loop_bridge" -- --exact --ignored
else
  echo 'loop final-owned identity CI-only test compiled/listed; not executed'
fi

replay_bridge=ssa::d10_dual_nested_loop_tests::ci_only_dual_replay_rejects_foreign_owned_identity
listing=$(cargo test --locked -j1 -p lang-codegen --lib --features d10-catalog-experiment,loan-observation-test-support "$replay_bridge" -- --exact --ignored --list)
[[ $(grep -Fxc "$replay_bridge: test" <<< "$listing") == 1 ]]
if [[ "$mode" != --list-only ]]; then
  cargo test --locked -j1 -p lang-codegen --lib --features d10-catalog-experiment,loan-observation-test-support "$replay_bridge" -- --exact --ignored
else
  echo 'nested replay identity CI-only test compiled/listed; not executed'
fi

after_bridge=ci_only_after_child_rejects_other_ownership
listing=$(cargo test --locked -j1 -p lang-frontend --features d10-catalog-experiment --test d10_after_child_handoff "$after_bridge" -- --exact --ignored --list)
[[ $(grep -Fxc "$after_bridge: test" <<< "$listing") == 1 ]]
if [[ "$mode" != --list-only ]]; then
  cargo test --locked -j1 -p lang-frontend --features d10-catalog-experiment --test d10_after_child_handoff "$after_bridge" -- --exact --ignored
else
  echo 'child-then-parent ownership identity CI-only test compiled/listed; not executed'
fi

after_publication=ssa::d10_dual_nested_loop_tests::parent_after_child::bound::ci_only_after_child_publication_rejects_identity_and_graph_mismatch
listing=$(cargo test --locked -j1 -p lang-codegen --lib --features d10-catalog-experiment,loan-observation-test-support "$after_publication" -- --exact --ignored --list)
[[ $(grep -Fxc "$after_publication: test" <<< "$listing") == 1 ]]
if [[ "$mode" != --list-only ]]; then
  cargo test --locked -j1 -p lang-codegen --lib --features d10-catalog-experiment,loan-observation-test-support "$after_publication" -- --exact --ignored
else
  echo 'AfterChild publication identity/graph controls compiled/listed; not executed'
fi

before_publication=ssa::d10_dual_nested_loop_tests::ci_only_before_child_graph_rejects_locally_valid_source_mismatch
listing=$(cargo test --locked -j1 -p lang-codegen --lib --features d10-catalog-experiment,loan-observation-test-support "$before_publication" -- --exact --ignored --list)
[[ $(grep -Fxc "$before_publication: test" <<< "$listing") == 1 ]]
if [[ "$mode" != --list-only ]]; then
  cargo test --locked -j1 -p lang-codegen --lib --features d10-catalog-experiment,loan-observation-test-support "$before_publication" -- --exact --ignored
else
  echo 'BeforeChild source/graph controls compiled/listed; not executed'
fi

process_object=ssa::d10_named_wrapper_tests::process_object::ci_only_process_object_failure_preserves_target
listing=$(cargo test --locked -j1 -p lang-codegen --lib --features d10-catalog-experiment,loan-observation-test-support "$process_object" -- --exact --ignored --list)
[[ $(grep -Fxc "$process_object: test" <<< "$listing") == 1 ]]
if [[ "$mode" != --list-only ]]; then
  cargo test --locked -j1 -p lang-codegen --lib --features d10-catalog-experiment,loan-observation-test-support "$process_object" -- --exact --ignored
else
  echo 'named process object atomicity control compiled/listed; not executed'
fi

process_after=ssa::d10_named_after_child_tests::process_object::ci_only_process_object_failure_preserves_target
listing=$(cargo test --locked -j1 -p lang-codegen --lib --features d10-catalog-experiment,loan-observation-test-support "$process_after" -- --exact --ignored --list)
[[ $(grep -Fxc "$process_after: test" <<< "$listing") == 1 ]]
if [[ "$mode" != --list-only ]]; then
  cargo test --locked -j1 -p lang-codegen --lib --features d10-catalog-experiment,loan-observation-test-support "$process_after" -- --exact --ignored
else
  echo 'AfterChild named process object atomicity control compiled/listed; not executed'
fi

selected_entry=ci_only_selected_entry_rejects_foreign_and_cloned_declarations
listing=$(cargo test --locked -j1 -p lang-frontend --features d10-catalog-experiment --test d10_prepare_named_entry "$selected_entry" -- --exact --ignored --list)
[[ $(grep -Fxc "$selected_entry: test" <<< "$listing") == 1 ]]
if [[ "$mode" != --list-only ]]; then
  cargo test --locked -j1 -p lang-frontend --features d10-catalog-experiment --test d10_prepare_named_entry "$selected_entry" -- --exact --ignored
else
  echo 'selected-entry foreign/cloned identity controls compiled/listed; not executed'
fi
