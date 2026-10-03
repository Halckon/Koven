#!/usr/bin/env bash
# SPEC-0237: bounded integration matrix, not the full frontend test suite.
# Run from any directory after enabling the supported Rust/LLVM toolchain.
set -uo pipefail
cd "$(dirname "$0")/.."

status=0
run() { "$@" || status=$?; }

# Integration contracts touched by SPEC-0229–0237; keep these distinct
# from the full frontend suite. SPEC-0247 includes the repaired multifile baseline.
run cargo test --locked -p lang-frontend --no-fail-fast \
  --test numeric_literals --test bitwise_constants --test integer_inv --test type_ownership_primitives --test ownership_primitives --test ownership_field_replace \
  --test string_clone --test clone_primitive_integration \
  --test type_checking --test type_callable --test type_copyability --test multifile_type_checking \
  --test const_owned_compilation_unit_view --test const_owned_unit_view_compile_contracts \
  --test multifile_constant_ownership \
  --test owned_compilation_unit_view --test owned_unit_view_compile_contracts \
  --test unit_name_snapshot --test unit_name_snapshot_compile_contracts \
  --test basic_unit_ownership --test basic_unit_ownership_compile_contracts \
  --test single_file_analysis --test single_file_analysis_compile_contracts \
  --test type_constants --test multifile_constant_facts \
  --test multifile_constant_qualification --test multifile_constant_dependencies \
  --test multifile_constant_selection --test lexer \
  --test ownership_two_phase_borrows --test multifile_two_phase_borrows \
  --test resource_deinit_type_facts --test ownership_resource_deinit \
  --test ownership_checking --test ownership_containers \
  --test ownership_construction --test ownership_closures \
  --test ownership_rc --test multifile_ownership_checking
run cargo test --locked -p lang-frontend --no-fail-fast \
  --test parser_contextual_type_ref --test parser_class_family \
  --test parser_call_argument --test parser_diagnostic_witness_matrix \
  --test parser_return_control --test tree_sitter_grammar --test textmate_grammar \
  --test parser_block_line_continuation --test parser_entry_line_break_boundary_matrix \
  --test parser_block --test parser_expression --test parser_control_flow \
  --test parser_lambda --test parser_local_destructuring --test parser_trailing_lambda \
  --test parser_declaration --test parser_file --test parser_operator_matrix \
  --test parser_error_propagation
run cargo test --locked -p lang-frontend --no-fail-fast \
  --test parser_prefix_truncation_matrix --test parser_suffix_truncation_matrix \
  --test parser_token_omission_matrix --test parser_trivia_invariance_matrix \
  --test parser_lexical_poison_insertion_matrix --test parser_entry_adversarial \
  --test parser_recursion_boundary_matrix --test parser_stack_isolation_matrix \
  --test parser_entry_trivia_invariance_matrix --test parser_entry_prefix_truncation_matrix \
  --test parser_entry_suffix_truncation_matrix --test parser_entry_lexical_poison_insertion_matrix \
  --test parser_long_block_comment_line_breaks --test parser_long_line_comment_boundaries \
  --test parser_stress_matrix --test parser_standalone_poison_stress_matrix \
  --test parser_owner_stress_matrix

exit "$status"
