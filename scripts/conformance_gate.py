#!/usr/bin/env python3
"""Public-entry conformance runner, byte-exact oracle comparator, and regression gate.

Responsibilities (V2 first slice):
1. Execute probes and acceptance programs via public CLI (`kovenc run`) in both
   single-file mode and single-file project mode (`package app` + `project.toml`),
   preserving raw stdout/stderr bytes, exact exit status, and timeout state separately.
2. Bind compiler binary SHA-256, git commit, host platform, original source SHA-256,
   and explicit project-mode transformation metadata (`prepend_package_header`).
3. Compare execution results against machine-readable expectations (`expectations.json`)
   and historical baselines (`baseline-*.tsv`), strictly distinguishing:
   - `pass_run` (exit 0, empty stderr, exact byte match on stdout including trailing newline)
   - `pass_diagnostic` (non-zero exit, empty stdout, expected `FE:Lxxxx` diagnostic)
   - `pass_abort` (SIGABRT / exit 134, expected `abort: <message>` header on stderr)
   - `known_unimplemented` (matches allowed unimplemented markers; NEVER counted as capability pass)
   - `unreviewed` (oracle not yet human-confirmed; blocks strict gate, never auto-blessed)
   - `wrong_output` (exit 0 with mismatched stdout, missing trailing newline, silent output, or dirty stderr)
   - `mode_mismatch` (single and project differ in marker or stdout bytes)
   - `regression` (expected pass or historical baseline OK regressed to failure; both-rejected never counts as pass)
   - `internal_error` (`BE:InvalidSsa`, `ICE:ownership`, `timeout`, unexplained `ERR`, or silent crash)
"""

from __future__ import annotations

import argparse
import base64
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import signal
import subprocess
import sys
import tempfile
from typing import Any


SCHEMA_EXPECTATIONS = "koven.conformance-expectations.v1"
SCHEMA_RESULTS = "koven.conformance-results.v1"

PROJECT_TOML_BYTES = (
    b'schema = "koven.project"\n'
    b"version = 1\n\n"
    b"[project]\n"
    b'name = "probe"\n'
    b'source-roots = ["src"]\n'
)
PROJECT_PACKAGE_HEADER = b"package app\n"

DIAGNOSTIC_RE = re.compile(rb"(?m)^error\[(L[0-9]{4})\](?:\s|:)")
BACKEND_RE = re.compile(rb"lowering failed with ([A-Za-z]+)")
BACKEND_OBJECT_RE = re.compile(rb"native object ([A-Za-z]+):")
OWNERSHIP_ICE_RE = re.compile(rb"ownership checking failed: [a-z -]+")
SIMPLE_PRINTLN_LITERAL_RE = re.compile(r'println\("([^"\\$]*)"\)')
PACKAGE_DIRECTIVE_RE = re.compile(r"(?m)^\s*package\s+")
MAIN_ENTRY_RE = re.compile(r"(?m)\bfun\s+main\s*\(")

# Multi-diagnostic compile_fail probes where more than one distinct error[Lxxxx] code is emitted on stderr
EXPECTED_MULTI_DIAGNOSTICS: dict[str, list[str]] = {
    "p25_linked_iter": ["L0133", "L0132"],
}

# Canonical frontend diagnostic code symbols from crates/lang-frontend/src/diagnostic.rs::codes
DIAGNOSTIC_SYMBOL_CATALOG: dict[str, str] = {
    "L0080": "UNRESOLVED_NAME",
    "L0084": "TYPE_MISMATCH",
    "L0085": "INVALID_OPERAND_TYPES",
    "L0089": "NO_COMMON_BRANCH_TYPE",
    "L0112": "WHEN_BRANCH_TYPE",
    "L0132": "PARTIAL_MOVE",
    "L0133": "MOVE_FROM_BORROWED_BINDING",
    "L0135": "LOAN_CONFLICT",
    "L0164": "UNSUPPORTED_BORROW_FLOW",
    "L0165": "FINAL_TYPE_ANALYSIS_INCOMPLETE",
    "L0167": "UNSUPPORTED_TOP_LEVEL_RUNTIME_VALUE",
}

VALID_EXPECTATION_KINDS = frozenset(
    {
        "run_ok",
        "compile_fail",
        "runtime_abort",
        "known_unimplemented",
        "unreviewed",
    }
)

VALID_GUIDE_CHAPTERS = (
    "01-lexical.md",
    "02-names-files-packages.md",
    "03-types-generics.md",
    "04-expressions-operators.md",
    "05-declarations-callables.md",
    "06-blocks-control-flow.md",
    "07-calls-lambdas-closures.md",
    "08-class-family-members.md",
    "09-nullability-errors.md",
    "10-ownership-borrowing-drop.md",
    "11-copyability-layout-construction.md",
    "12-collections-destructuring.md",
    "13-program-runtime-standard-library.md",
)

PROBE_GUIDE_CHAPTERS: dict[str, str] = {
    "p02_interp": "01-lexical.md",
    "p03_long_uint": "01-lexical.md",
    "p05_double": "01-lexical.md",
    "p06_char": "01-lexical.md",
    "r01_bogus_member": "02-names-files-packages.md",
    "r02_bogus_member_str": "02-names-files-packages.md",
    "s01_result_builtin": "02-names-files-packages.md",
    "s02_pair_builtin": "02-names-files-packages.md",
    "s03_to_pair": "02-names-files-packages.md",
    "y01_import_take_for_sum": "02-names-files-packages.md",
    "y02_take_no_import": "02-names-files-packages.md",
    "p03b_byte_literal_cmp": "03-types-generics.md",
    "p10_generic_fn": "03-types-generics.md",
    "p11_interface_bound": "03-types-generics.md",
    "p35_value_eq": "03-types-generics.md",
    "p36_conversion": "03-types-generics.md",
    "p37_is_generic_case": "03-types-generics.md",
    "p40_nested_generic_class": "03-types-generics.md",
    "t03_vmethod_generic": "03-types-generics.md",
    "v11_convert_ok": "03-types-generics.md",
    "v12_convert_abort": "03-types-generics.md",
    "v17_convert_in_loop": "03-types-generics.md",
    "p04_overflow": "04-expressions-operators.md",
    "q10_var_reassign_string": "04-expressions-operators.md",
    "t07_short_circuit_plain": "04-expressions-operators.md",
    "v01_assign_unit_value": "04-expressions-operators.md",
    "v02_assign_chain": "04-expressions-operators.md",
    "p01_int_print": "05-declarations-callables.md",
    "p23_global_var": "05-declarations-callables.md",
    "p24_recursion": "05-declarations-callables.md",
    "q12_companion_const": "05-declarations-callables.md",
    "e01_enum_when_bool_return": "06-blocks-control-flow.md",
    "e02_enum_when_stmt_println": "06-blocks-control-flow.md",
    "e03_enum_when_bool_val": "06-blocks-control-flow.md",
    "e04_enum_when_int_return": "06-blocks-control-flow.md",
    "p07_while_var": "06-blocks-control-flow.md",
    "p08_when_int": "06-blocks-control-flow.md",
    "p09_enum_payload": "06-blocks-control-flow.md",
    "p41_when_string": "06-blocks-control-flow.md",
    "p43_if_join_any": "06-blocks-control-flow.md",
    "p48_nested_loops_break": "06-blocks-control-flow.md",
    "p49_when_enum_no_payload": "06-blocks-control-flow.md",
    "q08_enum_string_payload": "06-blocks-control-flow.md",
    "q13_if_expr_value": "06-blocks-control-flow.md",
    "q14_when_subjectless": "06-blocks-control-flow.md",
    "t06_list_loop_return": "06-blocks-control-flow.md",
    "u04_enum_payload_arg_unit": "06-blocks-control-flow.md",
    "v04_stmt_if_mixed": "06-blocks-control-flow.md",
    "v16_when_in_range": "06-blocks-control-flow.md",
    "v19_stmt_when_mixed": "06-blocks-control-flow.md",
    "p17_lambda": "07-calls-lambdas-closures.md",
    "p17b_lambda_nocapture": "07-calls-lambdas-closures.md",
    "p18_move_closure": "07-calls-lambdas-closures.md",
    "p42_lambda_mutate": "07-calls-lambdas-closures.md",
    "q01_lambda_local": "07-calls-lambdas-closures.md",
    "q02_lambda_arg_inline": "07-calls-lambdas-closures.md",
    "q03_fn_ref": "07-calls-lambdas-closures.md",
    "x03_if_call_borrow_arg": "07-calls-lambdas-closures.md",
    "p12_class_inout": "08-class-family-members.md",
    "p12b_class_field_assign": "08-class-family-members.md",
    "p28_companion": "08-class-family-members.md",
    "p29_default_super": "08-class-family-members.md",
    "p32_deinit": "08-class-family-members.md",
    "p44_method_on_value": "08-class-family-members.md",
    "p44b_method_on_class": "08-class-family-members.md",
    "p46_string_field_class": "08-class-family-members.md",
    "p50_enum_method": "08-class-family-members.md",
    "q16_interface_static": "08-class-family-members.md",
    "q19_deep_return_cleanup": "08-class-family-members.md",
    "t01_vmethod_bare_field": "08-class-family-members.md",
    "t02_vmethod_bool": "08-class-family-members.md",
    "t04_vmethod_compute_param": "08-class-family-members.md",
    "t08_vmethod_string": "08-class-family-members.md",
    "t09_vmethod_calls_method": "08-class-family-members.md",
    "p14_nullable": "09-nullability-errors.md",
    "p14b_nullable_simple": "09-nullability-errors.md",
    "p14c_nullable_int": "09-nullability-errors.md",
    "p15_elvis": "09-nullability-errors.md",
    "p16_result_q": "09-nullability-errors.md",
    "p16b_result_string": "09-nullability-errors.md",
    "p45_abort_message": "09-nullability-errors.md",
    "q05_nullable_param": "09-nullability-errors.md",
    "q06_nullable_return_bang": "09-nullability-errors.md",
    "q07_result_when_int": "09-nullability-errors.md",
    "s04_question_on_nullable": "09-nullability-errors.md",
    "v03_join_nullable": "09-nullability-errors.md",
    "v13_while_nullable_int": "09-nullability-errors.md",
    "v18_while_nullable_class": "09-nullability-errors.md",
    "x01_nullable_class_retest": "09-nullability-errors.md",
    "x02_string_nullable_return": "09-nullability-errors.md",
    "p12c_inout_param": "10-ownership-borrowing-drop.md",
    "p25_linked_iter": "10-ownership-borrowing-drop.md",
    "p25b_linked_recursive": "10-ownership-borrowing-drop.md",
    "p26_borrow_return": "10-ownership-borrowing-drop.md",
    "q09_inout_class_param": "10-ownership-borrowing-drop.md",
    "q15_borrow_param_string_list": "10-ownership-borrowing-drop.md",
    "q17_generic_class": "10-ownership-borrowing-drop.md",
    "q20_replace_field": "10-ownership-borrowing-drop.md",
    "u01_enum_case_borrow_arg": "10-ownership-borrowing-drop.md",
    "p13_value_class_copy": "11-copyability-layout-construction.md",
    "p21_box_enum": "11-copyability-layout-construction.md",
    "q18_value_class_generic": "11-copyability-layout-construction.md",
    "p22_rc": "13-program-runtime-standard-library.md",
    "p27_thread": "13-program-runtime-standard-library.md",
    "p31_string_ops": "13-program-runtime-standard-library.md",
    "q04_rc_simple": "13-program-runtime-standard-library.md",
}

PROGRAM_GUIDE_CHAPTERS: dict[str, str] = {
    "word_count": "12-collections-destructuring.md",
    "config_parse": "09-nullability-errors.md",
    "expr_eval": "11-copyability-layout-construction.md",
    "raii_state_machine": "08-class-family-members.md",
    "varint_codec": "03-types-generics.md",
    "linked_stack": "10-ownership-borrowing-drop.md",
}

# Explicit mapping from all 23 known_unimplemented probes to blocking item IDs in docs/roadmap/v1.json.
UNIMPLEMENTED_ROADMAP_BLOCKERS: dict[str, list[str]] = {
    "p02_interp": ["L1", "L2"],
    "p05_double": ["N05"],
    "p06_char": ["N05"],
    "p11_interface_bound": ["N06"],
    "p15_elvis": ["N03"],
    "p16_result_q": ["N08"],
    "p16b_result_string": ["N08"],
    "p17_lambda": ["N04"],
    "p17b_lambda_nocapture": ["N04"],
    "p18_move_closure": ["N04"],
    "p21_box_enum": ["N07", "L7"],
    "p22_rc": ["N07"],
    "p25b_linked_recursive": ["N03"],
    "p27_thread": ["T1", "N04"],
    "p28_companion": ["N06"],
    "p35_value_eq": ["L1"],
    "p40_nested_generic_class": ["N01"],
    "q02_lambda_arg_inline": ["N04"],
    "q03_fn_ref": ["N04"],
    "q08_enum_string_payload": ["N02", "L7"],
    "s02_pair_builtin": ["S6"],
    "s03_to_pair": ["S6"],
    "t03_vmethod_generic": ["N06"],
}

# Explicit mapping from all 6 V1 acceptance programs to blocking item IDs in docs/roadmap/v1.json.
PROGRAM_ROADMAP_BLOCKERS: dict[str, list[str]] = {
    "config_parse": ["N02", "N08", "L1", "L2"],
    "expr_eval": ["N02", "N05", "N07", "N08", "L7"],
    "linked_stack": ["N01", "N03", "N07", "L7"],
    "raii_state_machine": ["N01", "N02"],
    "varint_codec": ["N01", "N08", "L3b"],
    "word_count": ["N02", "N03"],
}

PROGRAM_ORACLE_DERIVATIONS: dict[str, str] = {
    "config_parse": "parses key=value config lines via Result<Int, String> and ? propagation, printing host/port/retries/missing/bad-int outcomes",
    "expr_eval": "evaluates recursive Box<Expr> AST with Char/Double/Result error propagation, printing formatted evaluation lines",
    "linked_stack": "pushes and pops generic Node<T>? linked stack with ownership transfer, printing popped elements and remaining depth",
    "raii_state_machine": "runs deterministic deinit state transitions across normal, early-return, and nested scopes, printing ordered lifecycle trace",
    "varint_codec": "encodes and decodes ULong/UInt varints over MutableList<Byte> with bitwise shifts and Result checks, printing roundtrip values",
    "word_count": "counts word frequencies in insertion order over MutableMap<String, Int> and prints 'word\tcount' lines",
}

SEMANTIC_ORACLE_DERIVATIONS: dict[str, str] = {
    "e01_enum_when_bool_return": "isRed(Color.Red) matches branch Color.Red -> true; if-branch prints 'e1'",
    "e02_enum_when_stmt_println": "c = Color.Red matches statement when branch Color.Red -> println('e2')",
    "e03_enum_when_bool_val": "c = Color.Green yields b = false in when expression; if (!b) prints 'e5'",
    "p01_int_print": "println expects String parameter; passing Int (40 + 2) emits TYPE_MISMATCH L0084",
    "p02_interp": "string interpolation 'hello ${n}' with n = 'k' evaluates to 'hello k' (blocked by L1/L2)",
    "p05_double": "Double multiplication 1.5 * 2.0 == 3.0 holds and prints 'double' (blocked by N05)",
    "p06_char": "Char comparison 'a' < 'b' holds and prints 'char' (blocked by N05)",
    "p11_interface_bound": "generic greet<T : Named>(Dog()) dispatches Dog.name() and prints 'dog' (blocked by N06)",
    "p15_elvis": "f(null) evaluates null ?: 7 -> 7; 7 == 7 holds and prints 'elvis' (blocked by N03)",
    "p16_result_q": "Result.Ok(21)? unwraps 21 in ? propagation; 21 * 2 == 42 prints 'question' (blocked by N08)",
    "p16b_result_string": "parse(1) returns Result.Ok('good'); when branch prints r.success ('good') (blocked by N08)",
    "p17_lambda": "capturing closure { n -> n * k } with k = 3 applied to 7 yields 21 and prints 'lambda' (blocked by N04)",
    "p17b_lambda_nocapture": "non-capturing lambda { n -> n * 3 } applied to 7 yields 21 and prints 'lambda0' (blocked by N04)",
    "p18_move_closure": "move closure captures owned String 'x', compares s == 'x' -> true, and prints 'move' (blocked by N04)",
    "p21_box_enum": "recursive Expr.Add(Box(Num(1)), Box(Num(2))) evaluates to 3 and prints 'box' (blocked by N07/L7)",
    "p22_rc": "Rc(Data(4)).share().value.n == 4 holds and prints 'rc' (blocked by N07)",
    "p25b_linked_recursive": "recursive sum(Node(1, Node(2, null))) returns 3 and prints 'rec-list' (blocked by N03)",
    "p27_thread": "thread(move { println('thread') }).join() runs worker closure and prints 'thread' (blocked by T1/N04)",
    "p28_companion": "anonymous companion function C.make() returns 7 and prints 'companion' (blocked by N06)",
    "p35_value_eq": "structural equality Point(1, 2) == Point(1, 2) holds and prints 'veq' (blocked by L1)",
    "p40_nested_generic_class": "Stack<Int>.push(1) mutates generic field items; size == 1 prints 'stack' (blocked by N01)",
    "q02_lambda_arg_inline": "twice({ n -> n + 1 }) applies +1 twice to 1 -> 3 and prints 'lambda-arg' (blocked by N04)",
    "q03_fn_ref": "callable reference ::inc applied to 1 yields 2 and prints 'fnref' (blocked by N04)",
    "q08_enum_string_payload": "show(Msg.Text('hi')) prints payload 'hi'; show(Msg.Quit) prints 'quit' (blocked by N02/L7)",
    "s02_pair_builtin": "builtin Pair(1, 2).first == 1 holds and prints 'pair' (blocked by S6)",
    "s03_to_pair": "infix 1 to 2 constructs Pair(1, 2); p.first == 1 prints 'to' (blocked by S6)",
    "t03_vmethod_generic": "generic value class W<Int>(4).get() returns 4 and prints 'vmethod-generic' (blocked by N06)",
    "p03b_byte_literal_cmp": "comparing Byte with untyped Int literal 3 without conversion emits INVALID_OPERAND_TYPES L0085",
    "p04_overflow": "Int.MAX_VALUE (2147483647) + 1 triggers checked signed addition overflow abort per Guide 04/09",
    "p07_while_var": "while loop sums i=0..9 into s=45; s == 45 holds and prints 'while'",
    "p08_when_int": "name(2) matches the subject equality branch 2 -> 'two' and prints 'two'",
    "p10_generic_fn": "identity('generic') prints 'generic'; identity(3) == 3 holds and prints 'int'",
    "p14_nullable": "pick(true) returns non-null Node(5); smart cast prints 'smart' and pick(true)!!.v == 5 prints 'bangbang'",
    "p14b_nullable_simple": "a: Node? = Node(5) is non-null with a.v == 5; smart cast branch prints 'smart'",
    "p14c_nullable_int": "a: Int? = 5 is non-null and equals 5; smart cast branch prints 'nint'",
    "p19_mutablelist": "xs=[10,2,3] sums to 15 with size 3; condition holds and prints 'list'",
    "p23_global_var": "top-level runtime var counter violates Guide 05 D4 (SPEC-0343); rejected with UNSUPPORTED_TOP_LEVEL_RUNTIME_VALUE L0167",
    "p25_linked_iter": "cur = cur.next moves non-Copyable Node? field out of borrowed/owned chain, emitting MOVE_FROM_BORROWED_BINDING L0133 and PARTIAL_MOVE L0132",
    "p26_borrow_return": "nameOf borrows Holder('borrowed').name; println(n) prints 'borrowed'",
    "p29_default_super": "C.hi() concatenates super<A>.hi() ('a') + super<B>.hi() ('b') = 'ab'",
    "p31_string_ops": "'ab'.clone() + 'c' equals 'abc'; branch prints 'abc'",
    "p32_deinit": "main prints 'body' first, then drops locals in reverse lexical order: b ('b') then a ('a')",
    "p34_for_range": "0..<5 sums 0+1+2+3+4 = 10; s == 10 holds and prints 'for-range'",
    "p37_is_generic_case": "Result.Ok(1) matches parameterless is Result.Ok with v = 1 (SPEC-0349); prints 'case-noargs'",
    "p38_list_of_strings": "iterating mutableListOf('x', 'y') prints 'x' then 'y' in insertion order",
    "p42_lambda_mutate": "mutating captured var sum inside closure without v0.46 mutating-closure lowering emits LOAN_CONFLICT L0135",
    "p43_if_join_any": "when expression mixing Int (1) and String ('s') branches is rejected with WHEN_BRANCH_TYPE L0112 (D3)",
    "p45_abort_message": "error('boom') aborts with SIGABRT and stderr header 'abort: boom' per Guide 09 D12",
    "p46_string_field_class": "Person('ann', 3).name is 'ann'; describe(p) prints 'ann'",
    "p47_list_index_string": "listOf('a', 'b')[1] evaluates to 'b' and prints 'b'",
    "p48_nested_loops_break": "outer loop runs 5 times, inner loop breaks at j==3 after 3 increments -> hits == 15; prints 'nested'",
    "p49_when_enum_no_payload": "name(Color.Green) selects Color.Green -> 'green' and prints 'green'",
    "q05_nullable_param": "show(Node(1)) takes non-null branch ('non-null'); show(null) takes else branch ('null')",
    "q10_var_reassign_string": "s starts as 'a', appends 'b' then 'c' -> 'abc'",
    "q11_while_list_index": "while loop sums listOf(1,2,3) by index -> s == 6; prints 'index-loop'",
    "q12_companion_const": "Cfg.N reads associated const val 3 from anonymous companion { }; Cfg.N == 3 prints 'cconst'",
    "q13_if_expr_value": "x = 3 satisfies x > 2, so if-expression evaluates to 'big'",
    "q14_when_subjectless": "x = 5 fails x < 3 and matches second guard x < 7 -> 'mid'",
    "q15_borrow_param_string_list": "total(listOf('a', 'b')) counts 2 elements; 2 == 2 prints 'borrow-list'",
    "q16_interface_static": "Dog.name() returns 'dog' via static interface implementation dispatch",
    "q17_generic_class": "borrow T from this on generic class Holder<T> hits UNSUPPORTED_BORROW_FLOW L0164 (roadmap N09)",
    "q19_deep_return_cleanup": "f(true) drops inner b ('b') then outer a ('a') on early return; f(false) drops a ('a') -> 'b\\na\\na\\n'",
    "q20_replace_field": "replace(&b.s, 'new') returns previous field value 'old' and updates b.s to 'new'",
    "r01_bogus_member": "calling nonexistent method Int.fooBar() emits UNRESOLVED_NAME L0080",
    "r02_bogus_member_str": "accessing nonexistent property String.length emits UNRESOLVED_NAME L0080",
    "s01_result_builtin": "builtin Result.Ok(1) is resolved without import (SPEC-0349); main prints 'result-builtin'",
    "s04_question_on_nullable": "postfix ? applied to nullable Int? (instead of Result) emits INVALID_OPERAND_TYPES L0085",
    "t05_map_loop_continue": "for-loop skips 'b' via continue and counts 'a' twice -> m.get('a') == 2 prints 'map-continue'",
    "t06_list_loop_return": "find(listOf(1,2,3), 2) returns true on second element; prints 'loop-return'",
    "t08_vmethod_string": "Tag(1).label() checks 1 > 0 and returns 'pos'",
    "t10_map_nested_if_break": "loop inserts 'a' and 'b' (m.size == 2), then breaks at 'c'; prints 'map-nested-break'",
    "u01_enum_case_borrow_arg": "isRed(Color.Red) borrows enum case argument and matches Color.Red -> true; prints 'enum-arg'",
    "u02_list_add_loop_string": "clones 'x' and 'y' into MutableList<String> and prints each element in order",
    "v02_assign_chain": "assignment b = 3 yields Unit, so chaining a = (b = 3) for Int var a emits TYPE_MISMATCH L0084 (SPEC-0325)",
    "v03_join_nullable": "pick(true) returns Int? = 7; smart cast x != null && x == 7 prints 'join-nullable'",
    "v04_stmt_if_mixed": "if/else with incompatible Int and String branch types emits NO_COMMON_BRANCH_TYPE L0089 (SPEC-0326)",
    "v06_for_range_sum": "0..<5 sums to 10 and 1..3 sums to 6 -> s == 16 prints 'for-range'",
    "v07_range_max": "inclusive range 2147483646..2147483647 iterates exactly 2 times without overflow; prints 'range-max'",
    "v08_long_range": "1L..4L sums 1+2+3+4 = 10L; prints 'long-range'",
    "v09_reverse_empty": "ascending range 5..1 is empty (0 iterations, n == 0); prints 'reverse-empty'",
    "v12_convert_abort": "(-1).toULong() fails range check and aborts with 'abort: value out of range for ULong' (SPEC-0327/0355)",
    "v13_while_nullable_int": "while (x != null) adds 5 to total and sets x = null; total == 5 prints 'while-nullable'",
    "v15_map_for": "MutableMap preserves insertion order ('b' then 'a'); loop prints 'b' then 'a'",
    "v16_when_in_range": "bucket(3) satisfies the `in 0..9` arm -> 'small' and prints 'small'",
    "v17_convert_in_loop": "0..<3 converts 0,1,2 via toLong() and sums to 3L; prints 'conv-loop'",
    "v18_while_nullable_class": "while (n != null) reads n.v (4) and sets n = null; t == 4 prints 'while-class'",
    "v20_range_in_list": "3 is neither in 1..2 nor in 5..6 (n == 0, size == 2); prints 'range-in-list'",
    "v21_for_range_return": "firstEven(5) returns 2 on i == 2 inside 1..5; prints 'range-return'",
    "v22_range_with_map": "populates m[i] = i*i for i in 0..<4; m.get(3) == 9 prints 'range-map'",
    "v23_nested_range_break": "outer 0..<3 runs 3 times, inner breaks at j == 1 after 1 increment -> n == 3 prints 'nested-range'",
    "w01_map_for_sum": "map entries ('b'->2, 'a'->1) sum to 3; prints 'map-sum'",
    "w02_map_for_order": "map iterates in insertion order ('b', 'a', 'c'); ok remains true and prints 'map-order'",
    "w03_map_overwrite_remove": "overwriting 'b'->5, removing 'a', and re-inserting 'a'->7 yields order ('b'->5, 'a'->7); prints 'map-reorder'",
    "w04_map_for_break": "iterating keys 0..4 breaks at k == 2 after counting 0 and 1 (seen == 2); prints 'map-break'",
    "w05_map_for_return": "find(m, 'b') returns value 2 from map loop; prints 'map-return'",
    "w06_map_mutate_in_loop": "calling m.put() while m is borrowed by for-loop iteration emits LOAN_CONFLICT L0135",
    "w07_map_nested": "nested iteration over {1:1, 2:2} computes (1+2)*(1+2) = 9; prints 'map-nested'",
    "w08_map_string_values": "map entries (2->'two', 1->'one') print values 'two' then 'one' in insertion order",
    "w09_map_temp_source": "iterating temporary make() map sums 3 + 4 = 7; prints 'map-temp'",
    "w10_map_for_continue_count": "map loop skips 'b'->2 via continue and sums 'a'->1 + 'c'->3 = 4; prints 'map-continue2'",
    "w11_word_count": "word frequency over ['x','y','x','z','x'] yields x=3, y=1, z=1; prints 'word-count'",
    "w12": "map loop prints key 'b', then post-loop s == 1 prints 'after'",
    "w13": "map loop with v == 2 prints 'seen', followed by post-loop 'end'",
    "w14": "map loop sets s = 1 without printing; post-loop prints 'end'",
    "w15": "map loop with v == 2 prints 'seen', followed by post-loop 'end'",
    "w16": "count(m) borrows Map parameter and sums 2 + 3 = 5; prints 'borrow-param-sum'",
    "w17": "map loop prints key 'b', then if (t == 1) prints 'end'",
    "w18": "map loop accumulates s = 2, then if (s == 2) prints 'end'",
    "w19": "map loop prints key 'b', followed by post-loop 'end'",
    "x02_string_nullable_return": "find(true) returns non-null String? 'hit'; smart cast s != null prints 'hit'",
    "x03_if_call_borrow_arg": "if (true) calls observe('obs') printing 'obs', then prints 'done'",
    "y01_import_take_for_sum": "take(listOf(1,2,3), 2) yields 1 and 2 (sum 3); prints 'take'",
    "y02_take_no_import": "calling take() without import koven.algorithms.take emits UNRESOLVED_NAME L0080 (Guide v0.45)",
    "y03_method_take_for": "xs.take(2) on listOf(1,2,3) sums 1 + 2 = 3; prints 'take-method'",
    "y04_take_for_println": "take(listOf('a','b','c'), 2) prints first two elements 'a' and 'b'",
    "y05_take_borrow_val_call": "borrow val v = take(xs, 2) passed to read(v) prints 'a' and 'b'",
    "y06_take_borrow_val_for": "borrow val v = take(xs, 2) iterated directly in for-loop prints 'a' and 'b'",
}


def classify_probe_guide_chapter(name: str, src_text: str) -> str:
    """Return the primary Guide v0.46 chapter filename for a probe."""
    if name in PROBE_GUIDE_CHAPTERS:
        return PROBE_GUIDE_CHAPTERS[name]
    if any(
        name.startswith(prefix)
        for prefix in ("p19_", "p20_", "p30_", "p33_", "p34_", "p38_", "p39_", "p47_", "q11_", "s02_", "s03_", "t05_", "t10_", "u02_", "u03_", "v05_", "v06_", "v07_", "v08_", "v09_", "v10_", "v14_", "v15_", "v20_", "v21_", "v22_", "v23_", "w", "y01_", "y03_", "y04_", "y05_", "y06_")
    ):
        return "12-collections-destructuring.md"
    if "error(" in src_text:
        return "09-nullability-errors.md"
    if "for (" in src_text or "while (" in src_text:
        return "06-blocks-control-flow.md"
    return "05-declarations-callables.md"

# Human-audited exact stdout oracles for multi-statement / loop / variable-print probes in conformance/probes
CONFIRMED_STDOUT_ORACLES: dict[str, str] = {
    "p02_interp": "hello k\n",
    "p07_while_var": "while\n",
    "p10_generic_fn": "generic\nint\n",
    "p11_interface_bound": "dog\n",
    "p19_mutablelist": "list\n",
    "p26_borrow_return": "borrowed\n",
    "p29_default_super": "ab\n",
    "p31_string_ops": "abc\n",
    "p32_deinit": "body\nb\na\n",
    "p34_for_range": "for-range\n",
    "p38_list_of_strings": "x\ny\n",
    "p46_string_field_class": "ann\n",
    "p47_list_index_string": "b\n",
    "p48_nested_loops_break": "nested\n",
    "q08_enum_string_payload": "hi\nquit\n",
    "q10_var_reassign_string": "abc\n",
    "q11_while_list_index": "index-loop\n",
    "q13_if_expr_value": "big\n",
    "q14_when_subjectless": "mid\n",
    "q15_borrow_param_string_list": "borrow-list\n",
    "q16_interface_static": "dog\n",
    "q19_deep_return_cleanup": "b\na\na\n",
    "q20_replace_field": "old\nnew\n",
    "t05_map_loop_continue": "map-continue\n",
    "t06_list_loop_return": "loop-return\n",
    "t08_vmethod_string": "pos\n",
    "t10_map_nested_if_break": "map-nested-break\n",
    "u02_list_add_loop_string": "x\ny\n",
    "v06_for_range_sum": "for-range\n",
    "v07_range_max": "range-max\n",
    "v08_long_range": "long-range\n",
    "v09_reverse_empty": "reverse-empty\n",
    "v15_map_for": "b\na\n",
    "v17_convert_in_loop": "conv-loop\n",
    "v18_while_nullable_class": "while-class\n",
    "v20_range_in_list": "range-in-list\n",
    "v21_for_range_return": "range-return\n",
    "v22_range_with_map": "range-map\n",
    "v23_nested_range_break": "nested-range\n",
    "w01_map_for_sum": "map-sum\n",
    "w02_map_for_order": "map-order\n",
    "w03_map_overwrite_remove": "map-reorder\n",
    "w04_map_for_break": "map-break\n",
    "w05_map_for_return": "map-return\n",
    "w07_map_nested": "map-nested\n",
    "w08_map_string_values": "two\none\n",
    "w09_map_temp_source": "map-temp\n",
    "w10_map_for_continue_count": "map-continue2\n",
    "w11_word_count": "word-count\n",
    "w12": "b\nafter\n",
    "w13": "seen\nend\n",
    "w14": "end\n",
    "w15": "seen\nend\n",
    "w16": "borrow-param-sum\n",
    "w17": "b\nend\n",
    "w18": "end\n",
    "w19": "b\nend\n",
    "x03_if_call_borrow_arg": "obs\ndone\n",
    "y05_take_borrow_val_call": "a\nb\n",
    "y06_take_borrow_val_for": "a\nb\n",
}

# Documented Guide v0.46 probe changes and post-aef3f4f8 merged Spec advancements
V046_PROBE_OVERRIDES: dict[str, dict[str, Any]] = {
    "p28_companion": {
        "kind": "known_unimplemented",
        "allowed_markers": ["BE:UnsupportedNode", "BE:UnsupportedSource", "BE:MissingFact", "FE:L0165"],
        "target_stdout": "companion\n",
        "v046_baseline_exempt": True,
        "note": "Guide v0.46 anonymous companion { } syntax; SPEC-0323 fixed static role, native N06 pending",
    },
    "q12_companion_const": {
        "kind": "run_ok",
        "stdout": "cconst\n",
        "v046_baseline_exempt": True,
        "note": "Guide v0.46 anonymous companion { } syntax; associated constant read succeeds",
    },
    "p23_global_var": {
        "kind": "compile_fail",
        "diagnostic": "L0167",
        "v046_baseline_exempt": True,
        "note": "Guide v0.46 D4 (SPEC-0343): top-level runtime var rejected with L0167",
    },
    "p04_overflow": {
        "kind": "runtime_abort",
        "abort_message": "integer overflow in +",
        "v046_baseline_exempt": True,
        "note": "Guide v0.46 D12 (SPEC-0355): checked + overflow emits structured abort message",
    },
    "p45_abort_message": {
        "kind": "runtime_abort",
        "abort_message": "boom",
        "v046_baseline_exempt": True,
        "note": "Guide v0.46 D12 (SPEC-0355): error() emits structured abort message",
    },
    "v12_convert_abort": {
        "kind": "runtime_abort",
        "abort_message": "value out of range for ULong",
        "v046_baseline_exempt": True,
        "note": "Guide v0.46 D12 (SPEC-0355): checked conversion emits structured abort message",
    },
    "p16_result_q": {
        "kind": "known_unimplemented",
        "allowed_markers": ["FE:L0165", "BE:UnsupportedNode", "BE:MissingFact"],
        "target_stdout": "question\n",
        "v046_baseline_exempt": True,
        "note": "Guide v0.46 E4 (SPEC-0349): Result in builtin env; ? lowering pending N08",
    },
    "p16b_result_string": {
        "kind": "known_unimplemented",
        "allowed_markers": ["BE:UnsupportedNode", "BE:MissingFact", "FE:L0165"],
        "target_stdout": "good\n",
        "v046_baseline_exempt": True,
        "note": "Guide v0.46 E4 (SPEC-0349): Result in builtin env; String payload lowering pending N08",
    },
    "p37_is_generic_case": {
        "kind": "run_ok",
        "stdout": "case-noargs\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0349: parameterless is Result.Ok infers root type args from subject",
    },
    "s01_result_builtin": {
        "kind": "run_ok",
        "stdout": "result-builtin\n",
        "v046_baseline_exempt": True,
        "note": "Guide v0.46 E4 (SPEC-0349): Result/Ok/Err injected into builtin environment",
    },
    "p14_nullable": {
        "kind": "run_ok",
        "stdout": "smart\nbangbang\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0350: E3 ownership refinement nullable pilot",
    },
    "p14b_nullable_simple": {
        "kind": "run_ok",
        "stdout": "smart\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0350: E3 ownership refinement nullable pilot",
    },
    "p14c_nullable_int": {
        "kind": "run_ok",
        "stdout": "nint\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0350: inline nullable Int? in E3 refinement layer",
    },
    "q05_nullable_param": {
        "kind": "run_ok",
        "stdout": "non-null\nnull\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0350: E3 ownership refinement nullable pilot",
    },
    "v03_join_nullable": {
        "kind": "run_ok",
        "stdout": "join-nullable\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0350: inline nullable Int? return in E3 refinement layer",
    },
    "v13_while_nullable_int": {
        "kind": "run_ok",
        "stdout": "while-nullable\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0350: inline nullable Int? in while loop in E3 refinement layer",
    },
    "x02_string_nullable_return": {
        "kind": "run_ok",
        "stdout": "hit\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0350: pointer-niche inline nullable String? return in E3 refinement layer",
    },
    "p41_when_string": {
        "kind": "run_ok",
        "stdout": "when-str\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0360: literal String when consumes SPEC-0357 cleanup facts in refinement",
    },
    "p08_when_int": {
        "kind": "run_ok",
        "stdout": "two\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0356: subject when (Int subject equality) in E3 refinement layer",
    },
    "v16_when_in_range": {
        "kind": "run_ok",
        "stdout": "small\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0356: subject when (Int subject `in` Range) in E3 refinement layer",
    },
    "v19_stmt_when_mixed": {
        "kind": "run_ok",
        "stdout": "stmt-when-mixed\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0356: subject when (statement when with mixed branch types) in E3 refinement layer",
    },
    "p12b_class_field_assign": {
        "kind": "run_ok",
        "stdout": "field-assign\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0358: class field replacement in E3 refinement layer",
    },
    "p12c_inout_param": {
        "kind": "run_ok",
        "stdout": "inout-param\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0358: Inout scalar parameter replacement and caller read-back in E3 refinement layer",
    },
    "q09_inout_class_param": {
        "kind": "run_ok",
        "stdout": "inout-class\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0358: Inout class parameter, Inout receiver call and field replacement in E3 refinement layer",
    },
    "e01_enum_when_bool_return": {
        "kind": "run_ok",
        "stdout": "e1\n",
        "v046_baseline_exempt": True,
        "note": "Enum when regression fixed on main",
    },
    "e02_enum_when_stmt_println": {
        "kind": "run_ok",
        "stdout": "e2\n",
        "v046_baseline_exempt": True,
        "note": "Enum when statement regression fixed on main",
    },
    "e03_enum_when_bool_val": {
        "kind": "run_ok",
        "stdout": "e5\n",
        "v046_baseline_exempt": True,
        "note": "Enum when bool val regression fixed on main",
    },
    "p49_when_enum_no_payload": {
        "kind": "run_ok",
        "stdout": "green\n",
        "v046_baseline_exempt": True,
        "note": "Payloadless enum when: main prints name(Color.Green), so the when branch yields green",
    },
    "p50_enum_method": {
        "kind": "run_ok",
        "stdout": "enum-method\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0356: enum method when (this) with is-tests and implicit-this payload reads in E3 refinement layer",
    },
    "u01_enum_case_borrow_arg": {
        "kind": "run_ok",
        "stdout": "enum-arg\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0320: enum case borrow argument classification",
    },
    "y01_import_take_for_sum": {
        "kind": "run_ok",
        "stdout": "take\n",
        "v046_baseline_exempt": True,
        "note": "take() lowered on main; for-loop sums the first two elements and prints take",
    },
    "y03_method_take_for": {
        "kind": "run_ok",
        "stdout": "take-method\n",
        "v046_baseline_exempt": True,
        "note": "take() method call lowered on main; for-loop sums the first two elements and prints take-method",
    },
    "y04_take_for_println": {
        "kind": "run_ok",
        "stdout": "a\nb\n",
        "v046_baseline_exempt": True,
        "note": "take() lowered on main; for-loop prints the first two elements",
    },
}


def sha256_bytes(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    return sha256_bytes(path.read_bytes())


def detect_host() -> dict[str, str]:
    uname = platform.uname()
    return {
        "system": uname.system,
        "machine": uname.machine,
        "platform": f"{uname.system}-{uname.machine}",
    }


def detect_git_commit(root: Path) -> str:
    env_commit = os.environ.get("KOVEN_COMPILER_COMMIT", "").strip()
    if env_commit:
        return env_commit
    try:
        out = subprocess.check_output(
            ["git", "rev-parse", "--short=8", "HEAD"],
            cwd=root,
            stderr=subprocess.DEVNULL,
            timeout=5.0,
        )
        commit = out.decode("ascii", errors="ignore").strip()
        if commit:
            return commit
    except Exception:
        pass
    return "unknown"


def extract_diagnostic_codes(stderr: bytes) -> list[str]:
    """Return distinct error[Lxxxx] codes emitted on stderr in order of appearance."""
    return list(dict.fromkeys(m.decode("ascii") for m in DIAGNOSTIC_RE.findall(stderr)))


def classify_raw(
    returncode: int | None,
    timed_out: bool,
    stdout: bytes,
    stderr: bytes,
) -> str:
    """Classify a single invocation using separated raw stdout and stderr bytes."""
    if timed_out or returncode == 124:
        return "timeout"
    if returncode is None:
        return "ERR"
    if returncode == 0:
        return "OK" if stdout else "OK(silent)"

    is_sigabrt = returncode in (134, -int(signal.SIGABRT))
    if is_sigabrt and stderr:
        first_line = stderr.split(b"\n", 1)[0].rstrip(b"\r")
        if first_line.startswith(b"abort: "):
            msg = first_line[len(b"abort: ") :].decode("utf-8", errors="replace")
            msg = msg.replace("\t", " ")
            return f"abort:{msg}"

    combined = stdout + stderr
    ice_match = OWNERSHIP_ICE_RE.search(combined)
    if ice_match:
        return "ICE:ownership"
    be_match = BACKEND_RE.search(combined)
    if be_match:
        return f"BE:{be_match.group(1).decode('ascii')}"
    object_match = BACKEND_OBJECT_RE.search(combined)
    if object_match:
        return f"BE:{object_match.group(1).decode('ascii')}"
    diag_match = DIAGNOSTIC_RE.search(stderr)
    if diag_match:
        return f"FE:{diag_match.group(1).decode('ascii')}"
    if not combined:
        return f"abort(exit {returncode},no msg)"
    return "ERR"


def build_project_source(raw_source: bytes) -> tuple[bytes, dict[str, Any]]:
    """Prepend `package app\\n` and record exact transformation metadata."""
    transformed = PROJECT_PACKAGE_HEADER + raw_source
    transform = {
        "kind": "prepend_package_header",
        "header_utf8": PROJECT_PACKAGE_HEADER.decode("utf-8"),
        "header_sha256": sha256_bytes(PROJECT_PACKAGE_HEADER),
        "target_relpath": "src/app/main.ko",
        "entry": "app.main",
        "project_toml_sha256": sha256_bytes(PROJECT_TOML_BYTES),
        "original_source_sha256": sha256_bytes(raw_source),
        "transformed_source_sha256": sha256_bytes(transformed),
    }
    return transformed, transform


def run_command_raw(
    argv: list[str],
    cwd: Path,
    timeout_sec: float,
    raw_artifact_prefix: Path | None = None,
) -> dict[str, Any]:
    """Execute command and capture raw stdout/stderr bytes via disk files, exit status, and timeout flag."""
    out_file = (
        Path(f"{raw_artifact_prefix}.stdout")
        if raw_artifact_prefix is not None
        else cwd / ".cmd.stdout"
    )
    err_file = (
        Path(f"{raw_artifact_prefix}.stderr")
        if raw_artifact_prefix is not None
        else cwd / ".cmd.stderr"
    )
    out_file.parent.mkdir(parents=True, exist_ok=True)
    with out_file.open("wb") as out_fh, err_file.open("wb") as err_fh:
        proc = subprocess.Popen(
            argv,
            cwd=cwd,
            stdout=out_fh,
            stderr=err_fh,
            start_new_session=True,
        )
        try:
            returncode: int | None = proc.wait(timeout=timeout_sec)
            timed_out = False
        except subprocess.TimeoutExpired:
            try:
                os.killpg(proc.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            proc.wait()
            returncode = 124
            timed_out = True

    stdout = out_file.read_bytes()
    stderr = err_file.read_bytes()
    if raw_artifact_prefix is None:
        out_file.unlink(missing_ok=True)
        err_file.unlink(missing_ok=True)
    else:
        Path(f"{raw_artifact_prefix}.status.json").write_text(
            json.dumps({"exit": returncode, "timed_out": timed_out}) + "\n",
            encoding="utf-8",
        )

    marker = classify_raw(returncode, timed_out, stdout, stderr)
    return {
        "argv": list(argv),
        "exit": returncode,
        "timed_out": timed_out,
        "stdout_bytes": stdout,
        "stderr_bytes": stderr,
        "stdout_len": len(stdout),
        "stderr_len": len(stderr),
        "stdout_sha256": sha256_bytes(stdout),
        "stderr_sha256": sha256_bytes(stderr),
        "stdout_b64": base64.b64encode(stdout).decode("ascii"),
        "stderr_b64": base64.b64encode(stderr).decode("ascii"),
        "marker": marker,
    }


def execute_item(
    kovenc: Path,
    name: str,
    source_path: Path,
    timeout_sec: float,
    evidence_dir: Path | None = None,
) -> dict[str, Any]:
    """Run a probe or acceptance program in both single-file and project modes."""
    raw_source = source_path.read_bytes()
    source_sha256 = sha256_bytes(raw_source)

    single_prefix = (evidence_dir / "raw" / f"{name}.single") if evidence_dir else None
    project_prefix = (evidence_dir / "raw" / f"{name}.project") if evidence_dir else None

    with tempfile.TemporaryDirectory(prefix=f"koven-conf-{name}-") as tmp:
        tmp_root = Path(tmp)

        single_dir = tmp_root / "single"
        single_dir.mkdir(parents=True, exist_ok=True)
        single_file = single_dir / source_path.name
        single_file.write_bytes(raw_source)
        single_res = run_command_raw(
            [str(kovenc), "run", source_path.name],
            cwd=single_dir,
            timeout_sec=timeout_sec,
            raw_artifact_prefix=single_prefix,
        )

        project_dir = tmp_root / "project"
        app_dir = project_dir / "src" / "app"
        app_dir.mkdir(parents=True, exist_ok=True)
        (project_dir / "project.toml").write_bytes(PROJECT_TOML_BYTES)
        transformed_bytes, transform_meta = build_project_source(raw_source)
        (app_dir / "main.ko").write_bytes(transformed_bytes)
        project_res = run_command_raw(
            [str(kovenc), "run", "--project", "project.toml", "--entry", "app.main"],
            cwd=project_dir,
            timeout_sec=timeout_sec,
            raw_artifact_prefix=project_prefix,
        )

    return {
        "name": name,
        "source_path": str(source_path),
        "source_sha256": source_sha256,
        "project_transform": transform_meta,
        "single": single_res,
        "project": project_res,
    }


def is_internal_error_marker(marker: str) -> bool:
    return marker in ("BE:InvalidSsa", "ICE:ownership", "ERR", "timeout") or marker.startswith(
        "abort(exit "
    )


def evaluate_mode_against_oracle(
    mode_res: dict[str, Any],
    expectation: dict[str, Any],
) -> tuple[str, str | None]:
    """Evaluate one mode (single or project) against the machine-readable expectation."""
    kind = expectation["kind"]
    marker = mode_res["marker"]
    exit_code = mode_res["exit"]
    stdout = mode_res["stdout_bytes"]
    stderr = mode_res["stderr_bytes"]

    if kind == "unreviewed":
        if is_internal_error_marker(marker):
            return "internal_error", f"unreviewed item hit internal error: {marker}"
        return "unreviewed", "expectation is unreviewed"

    if is_internal_error_marker(marker):
        return "internal_error", f"internal error or timeout: {marker}"

    if kind == "run_ok":
        expected_stdout = expectation["stdout"].encode("utf-8")
        expected_stderr = expectation.get("stderr", "").encode("utf-8")
        if exit_code != 0:
            return "regression", f"expected exit 0, got exit {exit_code} ({marker})"
        if stdout != expected_stdout:
            return (
                "wrong_output",
                f"exit 0 stdout mismatch: expected {expected_stdout!r}, got {stdout!r}",
            )
        if stderr != expected_stderr:
            return (
                "wrong_output",
                f"exit 0 stderr mismatch: expected {expected_stderr!r}, got {stderr!r}",
            )
        return "pass_run", None

    if kind == "compile_fail":
        expected_code = expectation["diagnostic"]
        expected_marker = f"FE:{expected_code}"
        if exit_code == 0:
            return "regression", f"expected compile_fail {expected_marker}, but exited 0"
        if stdout != b"":
            return "wrong_output", f"compile_fail produced unexpected stdout: {stdout!r}"
        if marker != expected_marker:
            return "regression", f"expected {expected_marker}, got {marker}"
        expected_codes = expectation.get("all_diagnostics", [expected_code])
        emitted_codes = extract_diagnostic_codes(stderr)
        if emitted_codes != expected_codes:
            return (
                "regression",
                f"expected error diagnostics {expected_codes}, got {emitted_codes}",
            )
        return "pass_diagnostic", None

    if kind == "runtime_abort":
        expected_msg = expectation["abort_message"]
        expected_marker = f"abort:{expected_msg}"
        if exit_code == 0:
            return "regression", f"expected runtime_abort {expected_marker}, but exited 0"
        if stdout != b"":
            return "wrong_output", f"runtime_abort produced unexpected stdout: {stdout!r}"
        if marker != expected_marker:
            return "regression", f"expected {expected_marker}, got {marker}"
        return "pass_abort", None

    if kind == "known_unimplemented":
        allowed_markers = expectation.get(
            "allowed_markers", ["BE:UnsupportedNode", "BE:MissingFact", "FE:L0165", "FE:L0080"]
        )
        if exit_code == 0:
            target_stdout = expectation.get("target_stdout")
            if target_stdout is not None and stdout != target_stdout.encode("utf-8"):
                return (
                    "wrong_output",
                    f"known_unimplemented item exited 0 with wrong stdout: expected {target_stdout.encode('utf-8')!r}, got {stdout!r}",
                )
            if stderr != b"":
                return (
                    "wrong_output",
                    f"known_unimplemented item exited 0 with non-empty stderr: {stderr!r}",
                )
            return (
                "unexpected_pass",
                "known_unimplemented item now exits 0; promote expectation to run_ok after review",
            )
        if marker in allowed_markers:
            return "known_unimplemented", None
        return (
            "regression",
            f"known_unimplemented marker changed: expected one of {allowed_markers}, got {marker}",
        )

    raise ValueError(f"unsupported expectation kind: {kind}")


def compare_item(
    execution: dict[str, Any],
    expectation: dict[str, Any],
    baseline_row: tuple[str, str] | None = None,
) -> dict[str, Any]:
    """Compare single and project execution results against expectation and optional baseline."""
    single_res = execution["single"]
    project_res = execution["project"]

    single_verdict, single_reason = evaluate_mode_against_oracle(single_res, expectation)
    project_verdict, project_reason = evaluate_mode_against_oracle(project_res, expectation)

    mode_consistent = single_res["marker"] == project_res["marker"]
    if single_res["exit"] == 0 and project_res["exit"] == 0:
        if single_res["stdout_bytes"] != project_res["stdout_bytes"]:
            mode_consistent = False

    reasons: list[str] = []
    if single_reason:
        reasons.append(f"single: {single_reason}")
    if project_reason:
        reasons.append(f"project: {project_reason}")
    if not mode_consistent:
        reasons.append(
            f"mode_mismatch: single={single_res['marker']} (stdout_sha256={single_res['stdout_sha256'][:12]}) "
            f"vs project={project_res['marker']} (stdout_sha256={project_res['stdout_sha256'][:12]})"
        )

    baseline_regression = False
    if baseline_row is not None and not expectation.get("v046_baseline_exempt", False):
        base_single, base_project = baseline_row
        if base_single == "OK" and single_res["marker"] != "OK":
            baseline_regression = True
            reasons.append(f"baseline regression in single: was OK, now {single_res['marker']}")
        if base_project == "OK" and project_res["marker"] != "OK":
            baseline_regression = True
            reasons.append(f"baseline regression in project: was OK, now {project_res['marker']}")

    verdicts = {single_verdict, project_verdict}
    if "internal_error" in verdicts:
        status = "internal_error"
    elif "wrong_output" in verdicts:
        status = "wrong_output"
    elif not mode_consistent:
        status = "mode_mismatch"
    elif "regression" in verdicts or baseline_regression:
        status = "regression"
    elif "unreviewed" in verdicts:
        status = "unreviewed"
    elif "unexpected_pass" in verdicts:
        status = "unexpected_pass"
    elif single_verdict == project_verdict and single_verdict in (
        "pass_run",
        "pass_diagnostic",
        "pass_abort",
        "known_unimplemented",
    ):
        status = single_verdict
    else:
        status = "regression"

    capability_passed = status in ("pass_run", "pass_diagnostic", "pass_abort")
    gate_ok = status in ("pass_run", "pass_diagnostic", "pass_abort", "known_unimplemented")

    return {
        "name": execution["name"],
        "expectation_kind": expectation["kind"],
        "guide_chapter": expectation.get("guide_chapter"),
        "single_marker": single_res["marker"],
        "project_marker": project_res["marker"],
        "single_verdict": single_verdict,
        "project_verdict": project_verdict,
        "mode_consistent": mode_consistent,
        "status": status,
        "capability_passed": capability_passed,
        "gate_ok": gate_ok,
        "reasons": reasons,
    }


def load_expectations(path: Path, corpus_root: Path | None = None) -> dict[str, Any]:
    """Load and validate machine-readable conformance expectations."""
    data = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(data, dict) or data.get("schema") != SCHEMA_EXPECTATIONS:
        raise ValueError(f"invalid expectations schema in {path}")
    for section in ("probes", "programs"):
        items = data.get(section)
        if not isinstance(items, dict) or not items:
            raise ValueError(f"expectations.{section} must be a non-empty object")
        for name, spec in items.items():
            if not isinstance(name, str) or not re.fullmatch(r"[A-Za-z0-9_]+", name):
                raise ValueError(f"invalid item name in {section}: {name!r}")
            if not isinstance(spec, dict) or "kind" not in spec or "source_sha256" not in spec:
                raise ValueError(f"{section}.{name} requires kind and source_sha256")
            kind = spec["kind"]
            if kind not in VALID_EXPECTATION_KINDS:
                raise ValueError(f"{section}.{name} has unsupported kind {kind!r}")
            if not isinstance(spec["source_sha256"], str) or not re.fullmatch(
                r"[0-9a-f]{64}", spec["source_sha256"]
            ):
                raise ValueError(f"{section}.{name} has invalid source_sha256")
            if kind == "run_ok":
                if section == "programs":
                    if "expected_file" not in spec or "expected_sha256" not in spec:
                        raise ValueError(
                            f"programs.{name} requires expected_file and expected_sha256"
                        )
                elif not isinstance(spec.get("stdout"), str) or not spec["stdout"]:
                    raise ValueError(f"probes.{name} with run_ok requires non-empty stdout")
            elif kind == "compile_fail":
                if not isinstance(spec.get("diagnostic"), str) or not re.fullmatch(
                    r"L[0-9]{4}", spec["diagnostic"]
                ):
                    raise ValueError(
                        f"{section}.{name} with compile_fail requires diagnostic Lxxxx"
                    )
                if "diagnostic_symbol" in spec:
                    expected_sym = DIAGNOSTIC_SYMBOL_CATALOG.get(spec["diagnostic"])
                    if not isinstance(spec["diagnostic_symbol"], str) or (
                        expected_sym is not None and spec["diagnostic_symbol"] != expected_sym
                    ):
                        raise ValueError(
                            f"{section}.{name} has invalid diagnostic_symbol {spec['diagnostic_symbol']!r} for {spec['diagnostic']}"
                        )
                if "all_diagnostics" in spec:
                    all_diags = spec["all_diagnostics"]
                    if (
                        not isinstance(all_diags, list)
                        or not all_diags
                        or not all(
                            isinstance(c, str) and re.fullmatch(r"L[0-9]{4}", c)
                            for c in all_diags
                        )
                        or all_diags[0] != spec["diagnostic"]
                    ):
                        raise ValueError(
                            f"{section}.{name} has invalid all_diagnostics {all_diags!r}"
                        )
            elif kind == "runtime_abort":
                if not isinstance(spec.get("abort_message"), str) or not spec["abort_message"]:
                    raise ValueError(
                        f"{section}.{name} with runtime_abort requires abort_message"
                    )
            elif kind == "known_unimplemented":
                if "target_stdout" in spec:
                    ts = spec["target_stdout"]
                    if not isinstance(ts, str) or not ts or not ts.endswith("\n"):
                        raise ValueError(
                            f"{section}.{name} with known_unimplemented has invalid target_stdout {ts!r} (must be non-empty and end with newline)"
                        )
                if "roadmap_blockers" in spec:
                    blockers = spec["roadmap_blockers"]
                    if (
                        not isinstance(blockers, list)
                        or not blockers
                        or not all(isinstance(b, str) and b.strip() for b in blockers)
                    ):
                        raise ValueError(
                            f"{section}.{name} with known_unimplemented has invalid roadmap_blockers {blockers!r}"
                        )
            if "oracle_origin" in spec and (
                not isinstance(spec["oracle_origin"], str) or not spec["oracle_origin"].strip()
            ):
                raise ValueError(f"{section}.{name} has invalid oracle_origin")
            if "oracle_derivation" in spec and (
                not isinstance(spec["oracle_derivation"], str)
                or not spec["oracle_derivation"].strip()
            ):
                raise ValueError(f"{section}.{name} has invalid oracle_derivation")
            if "guide_chapter" in spec:
                ch = spec["guide_chapter"]
                if not isinstance(ch, str) or ch not in VALID_GUIDE_CHAPTERS:
                    raise ValueError(f"{section}.{name} has invalid guide_chapter {ch!r}")
                if corpus_root is not None:
                    guide_dir = corpus_root.parent / "docs" / "guide"
                    if guide_dir.is_dir() and not (guide_dir / ch).is_file():
                        raise ValueError(
                            f"{section}.{name} references missing Guide chapter {guide_dir / ch}"
                        )

            if corpus_root is not None:
                src_file = (
                    corpus_root / "probes" / f"{name}.ko"
                    if section == "probes"
                    else corpus_root / "programs" / name / "main.ko"
                )
                if not src_file.is_file():
                    raise ValueError(f"missing source file for {section}.{name}: {src_file}")
                src_bytes = src_file.read_bytes()
                actual_sha = sha256_bytes(src_bytes)
                if actual_sha != spec["source_sha256"]:
                    raise ValueError(
                        f"source_sha256 mismatch for {section}.{name}: expected {spec['source_sha256']}, got {actual_sha}"
                    )
                src_text = src_bytes.decode("utf-8")
                if PACKAGE_DIRECTIVE_RE.search(src_text):
                    raise ValueError(
                        f"{section}.{name} must not contain a package directive (collides with project-mode package app header)"
                    )
                if not MAIN_ENTRY_RE.search(src_text):
                    raise ValueError(
                        f"{section}.{name} must declare a fun main(...) entry point"
                    )
                if section == "programs" and "expected_file" in spec:
                    exp_file = corpus_root / spec["expected_file"]
                    if not exp_file.is_file():
                        raise ValueError(
                            f"missing expected_file for programs.{name}: {exp_file}"
                        )
                    exp_bytes = exp_file.read_bytes()
                    if not exp_bytes or not exp_bytes.endswith(b"\n"):
                        raise ValueError(
                            f"expected_file for programs.{name} must be non-empty and end with trailing newline"
                        )
                    if sha256_bytes(exp_bytes) != spec["expected_sha256"]:
                        raise ValueError(f"expected_sha256 mismatch for programs.{name}")
                    exp_text = exp_bytes.decode("utf-8")
                    if "expected_bytes" in spec and spec["expected_bytes"] != len(exp_bytes):
                        raise ValueError(
                            f"expected_bytes mismatch for programs.{name}: expected {spec['expected_bytes']}, got {len(exp_bytes)}"
                        )
                    if "expected_lines" in spec and spec["expected_lines"] != len(exp_text.splitlines()):
                        raise ValueError(
                            f"expected_lines mismatch for programs.{name}: expected {spec['expected_lines']}, got {len(exp_text.splitlines())}"
                        )
                    spec["stdout"] = exp_text
                    spec["target_stdout"] = exp_text

    if corpus_root is not None:
        actual_probes = {p.stem for p in (corpus_root / "probes").glob("*.ko")}
        expected_probes = set(data["probes"].keys())
        missing = sorted(actual_probes - expected_probes)
        extra = sorted(expected_probes - actual_probes)
        if missing or extra:
            raise ValueError(
                f"probe inventory mismatch: missing_in_expectations={missing}, extra_in_expectations={extra}"
            )
        programs_dir = corpus_root / "programs"
        if programs_dir.is_dir():
            actual_progs = {
                d.name
                for d in programs_dir.iterdir()
                if d.is_dir() and (d / "main.ko").is_file() and (d / "expected.txt").is_file()
            }
            expected_progs = set(data["programs"].keys())
            missing_progs = sorted(actual_progs - expected_progs)
            extra_progs = sorted(expected_progs - actual_progs)
            if missing_progs or extra_progs:
                raise ValueError(
                    f"program inventory mismatch: missing_in_expectations={missing_progs}, extra_in_expectations={extra_progs}"
                )

    # Validate that every roadmap_blocker exists in docs/roadmap/v1.json and is not prematurely all done
    if corpus_root is not None:
        v1_json_path = corpus_root.parent / "docs" / "roadmap" / "v1.json"
        if v1_json_path.is_file():
            v1_data = json.loads(v1_json_path.read_text(encoding="utf-8"))
            roadmap_items = {it["id"]: it for it in v1_data.get("items", [])}
            for section in ("probes", "programs"):
                for name, spec in data[section].items():
                    if spec["kind"] == "known_unimplemented":
                        blockers = spec.get("roadmap_blockers")
                        if not blockers or not isinstance(blockers, list):
                            raise ValueError(
                                f"{section}.{name} with known_unimplemented missing roadmap_blockers in production corpus"
                            )
                        for b in blockers:
                            if b not in roadmap_items:
                                raise ValueError(
                                    f"{section}.{name} references unknown roadmap blocker {b!r} not found in {v1_json_path.name}"
                                )
                        if not any(roadmap_items[b].get("status") != "done" for b in blockers):
                            raise ValueError(
                                f"{section}.{name} has all roadmap_blockers marked done in {v1_json_path.name}: must transition out of known_unimplemented"
                            )
    return data


def parse_tsv_matrix(
    text: str, expected_names: set[str] | None = None
) -> dict[str, tuple[str, str]]:
    """Parse a 3-column TSV matrix and reject duplicate, malformed, or missing probe rows."""
    rows: dict[str, tuple[str, str]] = {}
    for line_no, raw_line in enumerate(text.splitlines(), 1):
        if not raw_line.strip():
            raise ValueError(f"line {line_no}: empty line in TSV matrix")
        parts = raw_line.split("\t")
        if len(parts) != 3 or any(not p for p in parts):
            raise ValueError(
                f"line {line_no}: expected 3 non-empty TSV columns, got {raw_line!r}"
            )
        name, single, project = parts
        if name in rows:
            raise ValueError(f"line {line_no}: duplicate probe row {name!r}")
        rows[name] = (single, project)

    if expected_names is not None:
        missing = sorted(expected_names - set(rows.keys()))
        extra = sorted(set(rows.keys()) - expected_names)
        if missing:
            raise ValueError(f"missing probe rows in matrix: {missing}")
        if extra:
            raise ValueError(f"unexpected probe rows in matrix: {extra}")
    return rows


def derive_static_expectations(
    corpus_root: Path,
    baseline_path: Path,
    use_confirmed_oracles: bool = True,
) -> dict[str, Any]:
    """Build machine-readable expectations from source analysis + baseline without running compiler."""
    baseline = parse_tsv_matrix(baseline_path.read_text(encoding="utf-8"))
    probes_dir = corpus_root / "probes"
    programs_dir = corpus_root / "programs"

    probes_out: dict[str, Any] = {}
    for path in sorted(probes_dir.glob("*.ko")):
        name = path.stem
        raw_bytes = path.read_bytes()
        src_text = raw_bytes.decode("utf-8")
        src_sha = sha256_bytes(raw_bytes)
        base_single, base_project = baseline[name]
        assert base_single == base_project, f"baseline mismatch for {name}"

        literals = SIMPLE_PRINTLN_LITERAL_RE.findall(src_text)
        total_printlns = src_text.count("println(")
        has_repeated_print_risk = any(
            token in src_text
            for token in ("for (", "for(", "while (", "while(", "deinit(")
        )
        is_single_literal = (
            len(literals) == 1
            and total_printlns == 1
            and not has_repeated_print_risk
        )
        single_static_stdout = (
            f"{literals[0]}\n"
            if is_single_literal
            else (CONFIRMED_STDOUT_ORACLES.get(name) if use_confirmed_oracles else None)
        )
        stdout_origin = (
            "single_println_literal"
            if is_single_literal
            else ("source_trace_audited" if single_static_stdout is not None else None)
        )
        chapter = classify_probe_guide_chapter(name, src_text)
        derivation = SEMANTIC_ORACLE_DERIVATIONS.get(
            name,
            f"single literal println({literals[0]!r})" if is_single_literal else None,
        )

        if name in V046_PROBE_OVERRIDES:
            entry = dict(V046_PROBE_OVERRIDES[name])
            entry["source_sha256"] = src_sha
            entry["guide_chapter"] = chapter
            entry["oracle_origin"] = "v046_spec_contract"
            if entry.get("kind") == "compile_fail" and entry.get("diagnostic") in DIAGNOSTIC_SYMBOL_CATALOG:
                entry["diagnostic_symbol"] = DIAGNOSTIC_SYMBOL_CATALOG[entry["diagnostic"]]
            if entry.get("kind") == "known_unimplemented":
                entry["roadmap_blockers"] = list(UNIMPLEMENTED_ROADMAP_BLOCKERS.get(name, ["N02"]))
            if derivation is not None:
                entry["oracle_derivation"] = derivation
            probes_out[name] = entry
            continue

        if base_single == "OK":
            if single_static_stdout is not None:
                entry = {
                    "kind": "run_ok",
                    "source_sha256": src_sha,
                    "guide_chapter": chapter,
                    "stdout": single_static_stdout,
                    "oracle_origin": stdout_origin,
                }
                if derivation is not None:
                    entry["oracle_derivation"] = derivation
                probes_out[name] = entry
            else:
                probes_out[name] = {
                    "kind": "unreviewed",
                    "source_sha256": src_sha,
                    "guide_chapter": chapter,
                    "baseline_marker": "OK",
                    "note": "multi-line or computed println output requires human oracle confirmation",
                }
        elif base_single.startswith("FE:"):
            code = base_single.split(":", 1)[1]
            if code == "L0165" or name in ("p27_thread", "s02_pair_builtin"):
                entry = {
                    "kind": "known_unimplemented",
                    "source_sha256": src_sha,
                    "guide_chapter": chapter,
                    "allowed_markers": [
                        base_single,
                        "BE:UnsupportedNode",
                        "BE:MissingFact",
                    ],
                    "oracle_origin": "baseline_unimplemented_audited",
                    "roadmap_blockers": list(UNIMPLEMENTED_ROADMAP_BLOCKERS.get(name, ["N02"])),
                }
                if single_static_stdout is not None:
                    entry["target_stdout"] = single_static_stdout
                if derivation is not None:
                    entry["oracle_derivation"] = derivation
                probes_out[name] = entry
            else:
                entry = {
                    "kind": "compile_fail",
                    "source_sha256": src_sha,
                    "guide_chapter": chapter,
                    "diagnostic": code,
                    "oracle_origin": "baseline_diagnostic_audited",
                }
                if code in DIAGNOSTIC_SYMBOL_CATALOG:
                    entry["diagnostic_symbol"] = DIAGNOSTIC_SYMBOL_CATALOG[code]
                if name in EXPECTED_MULTI_DIAGNOSTICS:
                    entry["all_diagnostics"] = list(EXPECTED_MULTI_DIAGNOSTICS[name])
                    entry["all_diagnostic_symbols"] = [
                        DIAGNOSTIC_SYMBOL_CATALOG[c] for c in EXPECTED_MULTI_DIAGNOSTICS[name]
                    ]
                if derivation is not None:
                    entry["oracle_derivation"] = derivation
                probes_out[name] = entry
        elif base_single.startswith("BE:"):
            entry = {
                "kind": "known_unimplemented",
                "source_sha256": src_sha,
                "guide_chapter": chapter,
                "allowed_markers": ["BE:UnsupportedNode", "BE:MissingFact", "FE:L0165"],
                "oracle_origin": "baseline_unimplemented_audited",
                "roadmap_blockers": list(UNIMPLEMENTED_ROADMAP_BLOCKERS.get(name, ["N02"])),
            }
            if single_static_stdout is not None:
                entry["target_stdout"] = single_static_stdout
            if derivation is not None:
                entry["oracle_derivation"] = derivation
            probes_out[name] = entry
        else:
            probes_out[name] = {
                "kind": "unreviewed",
                "source_sha256": src_sha,
                "guide_chapter": chapter,
                "baseline_marker": base_single,
                "note": "requires human oracle confirmation",
            }

    programs_out: dict[str, Any] = {}
    for prog_dir in sorted(programs_dir.iterdir()):
        if not prog_dir.is_dir():
            continue
        main_ko = prog_dir / "main.ko"
        expected_txt = prog_dir / "expected.txt"
        if not main_ko.is_file() or not expected_txt.is_file():
            continue
        exp_bytes = expected_txt.read_bytes()
        exp_text = exp_bytes.decode("utf-8")
        prog_entry: dict[str, Any] = {
            "kind": "known_unimplemented",
            "source_sha256": sha256_file(main_ko),
            "guide_chapter": PROGRAM_GUIDE_CHAPTERS.get(
                prog_dir.name, "13-program-runtime-standard-library.md"
            ),
            "expected_file": f"programs/{prog_dir.name}/expected.txt",
            "expected_sha256": sha256_bytes(exp_bytes),
            "expected_lines": len(exp_text.splitlines()),
            "expected_bytes": len(exp_bytes),
            "allowed_markers": [
                "FE:L0080",
                "FE:L0084",
                "FE:L0085",
                "FE:L0165",
                "BE:UnsupportedNode",
                "BE:MissingFact",
            ],
            "oracle_origin": "expected_txt_file",
            "roadmap_blockers": list(PROGRAM_ROADMAP_BLOCKERS.get(prog_dir.name, ["N02"])),
        }
        if prog_dir.name in PROGRAM_ORACLE_DERIVATIONS:
            prog_entry["oracle_derivation"] = PROGRAM_ORACLE_DERIVATIONS[prog_dir.name]
        programs_out[prog_dir.name] = prog_entry

    return {
        "schema": SCHEMA_EXPECTATIONS,
        "probes": probes_out,
        "programs": programs_out,
    }


def resolve_default_cli(root: Path) -> Path:
    env_cli = os.environ.get("KOVENC", "").strip()
    if env_cli:
        return Path(env_cli)
    release_cli = root / "target/release/kovenc"
    debug_cli = root / "target/debug/kovenc"
    if release_cli.is_file():
        return release_cli
    if debug_cli.is_file():
        return debug_cli
    return release_cli


def evaluate_suite(
    kovenc: Path,
    corpus_root: Path,
    expectations_path: Path,
    baseline_path: Path | None = None,
    selected_probes: list[str] | None = None,
    include_programs: bool = True,
    timeout_sec: float = 60.0,
    strict_unreviewed: bool = True,
    compiler_commit: str = "unknown",
    evidence_dir: Path | None = None,
    jobs: int = 1,
) -> dict[str, Any]:
    """Run selected probes/programs, compare against expectations & baseline, and return full ledger."""
    if jobs < 1:
        raise ValueError(f"jobs must be >= 1, got {jobs}")
    kovenc = kovenc.resolve(strict=True)
    corpus_root = corpus_root.resolve(strict=True)
    expectations = load_expectations(expectations_path, corpus_root=corpus_root)
    baseline = (
        parse_tsv_matrix(
            baseline_path.read_text(encoding="utf-8"),
            expected_names=set(expectations["probes"].keys()),
        )
        if baseline_path is not None
        else {}
    )

    probe_names = sorted(expectations["probes"].keys())
    if selected_probes is not None:
        unknown = sorted(set(selected_probes) - set(probe_names))
        if unknown:
            raise ValueError(f"unknown probe names requested: {unknown}")
        if len(selected_probes) != len(set(selected_probes)):
            raise ValueError("duplicate probe names in selection")
        probe_names = [n for n in probe_names if n in set(selected_probes)]

    if evidence_dir is not None:
        evidence_dir.mkdir(parents=True, exist_ok=True)

    initial_compiler_sha = sha256_file(kovenc)

    def _run_probe(name: str) -> dict[str, Any]:
        src_path = corpus_root / "probes" / f"{name}.ko"
        exec_record = execute_item(
            kovenc,
            name,
            src_path,
            timeout_sec=timeout_sec,
            evidence_dir=evidence_dir,
        )
        comp = compare_item(
            exec_record,
            expectations["probes"][name],
            baseline_row=baseline.get(name),
        )
        return {"execution": _strip_raw_bytes(exec_record), "comparison": comp}

    def _run_program(name: str) -> dict[str, Any]:
        src_path = corpus_root / "programs" / name / "main.ko"
        exec_record = execute_item(
            kovenc,
            f"program_{name}",
            src_path,
            timeout_sec=timeout_sec,
            evidence_dir=evidence_dir,
        )
        exec_record["name"] = name
        comp = compare_item(
            exec_record, expectations["programs"][name], baseline_row=None
        )
        return {"execution": _strip_raw_bytes(exec_record), "comparison": comp}

    program_names = sorted(expectations["programs"].keys()) if include_programs else []
    if jobs == 1:
        probe_results = [_run_probe(name) for name in probe_names]
        program_results = [_run_program(name) for name in program_names]
    else:
        with ThreadPoolExecutor(max_workers=jobs) as pool:
            probe_futures = [(name, pool.submit(_run_probe, name)) for name in probe_names]
            program_futures = [
                (name, pool.submit(_run_program, name)) for name in program_names
            ]
            probe_results = [fut.result() for _, fut in probe_futures]
            program_results = [fut.result() for _, fut in program_futures]

    if sha256_file(kovenc) != initial_compiler_sha:
        raise RuntimeError("compiler binary changed during conformance run")

    summary = summarize_comparisons(
        [r["comparison"] for r in probe_results],
        [r["comparison"] for r in program_results],
        strict_unreviewed=strict_unreviewed,
    )

    ledger = {
        "schema": SCHEMA_RESULTS,
        "compiler": {
            "path": str(kovenc),
            "sha256": initial_compiler_sha,
            "commit": compiler_commit,
        },
        "host": detect_host(),
        "summary": summary,
        "probes": probe_results,
        "programs": program_results,
    }

    if evidence_dir is not None:
        (evidence_dir / "results.json").write_text(
            json.dumps(ledger, ensure_ascii=True, indent=2) + "\n",
            encoding="utf-8",
        )
        tsv_lines = [
            f"{r['comparison']['name']}\t{r['comparison']['single_marker']}\t{r['comparison']['project_marker']}"
            for r in probe_results
        ]
        (evidence_dir / "matrix.tsv").write_text(
            "\n".join(tsv_lines) + ("\n" if tsv_lines else ""),
            encoding="utf-8",
        )

    return ledger


def _strip_raw_bytes(exec_record: dict[str, Any]) -> dict[str, Any]:
    out = dict(exec_record)
    for mode in ("single", "project"):
        m = dict(out[mode])
        m.pop("stdout_bytes", None)
        m.pop("stderr_bytes", None)
        out[mode] = m
    return out


def summarize_comparisons(
    probe_comparisons: list[dict[str, Any]],
    program_comparisons: list[dict[str, Any]],
    strict_unreviewed: bool = True,
) -> dict[str, Any]:
    counts: dict[str, int] = {
        "pass_run": 0,
        "pass_diagnostic": 0,
        "pass_abort": 0,
        "known_unimplemented": 0,
        "unreviewed": 0,
        "wrong_output": 0,
        "mode_mismatch": 0,
        "regression": 0,
        "internal_error": 0,
        "unexpected_pass": 0,
    }
    failures: list[str] = []
    for item in probe_comparisons + program_comparisons:
        st = item["status"]
        counts[st] = counts.get(st, 0) + 1
        if st in (
            "wrong_output",
            "mode_mismatch",
            "regression",
            "internal_error",
            "unexpected_pass",
        ):
            failures.append(f"{item['name']} ({st}): {'; '.join(item['reasons'])}")
        elif st == "unreviewed" and strict_unreviewed:
            failures.append(
                f"{item['name']} (unreviewed): requires human oracle confirmation"
            )

    probe_cap_passed = sum(1 for item in probe_comparisons if item["capability_passed"])
    program_run_passed = sum(
        1 for item in program_comparisons if item["status"] == "pass_run"
    )

    by_chapter: dict[str, dict[str, int]] = {}
    for item in probe_comparisons:
        ch = item.get("guide_chapter") or "unassigned"
        bucket = by_chapter.setdefault(
            ch,
            {
                "total": 0,
                "pass_run": 0,
                "pass_diagnostic": 0,
                "pass_abort": 0,
                "known_unimplemented": 0,
                "capability_passed": 0,
            },
        )
        bucket["total"] += 1
        st = item["status"]
        if st in ("pass_run", "pass_diagnostic", "pass_abort", "known_unimplemented"):
            bucket[st] += 1
        if item["capability_passed"]:
            bucket["capability_passed"] += 1

    return {
        "total_probes": len(probe_comparisons),
        "probe_capability_passed": probe_cap_passed,
        "total_programs": len(program_comparisons),
        "program_run_passed": program_run_passed,
        "counts": counts,
        "by_chapter": dict(sorted(by_chapter.items())),
        "gate_passed": len(failures) == 0,
        "failures": failures,
    }


def summarize_chapter_polarity(expectations: dict[str, Any]) -> dict[str, dict[str, Any]]:
    """Compute per-chapter positive (run_ok) vs negative (compile_fail + runtime_abort) polarity coverage."""
    probes = expectations["probes"]
    chapter_stats: dict[str, dict[str, Any]] = {
        ch: {
            "total": 0,
            "run_ok": 0,
            "compile_fail": 0,
            "runtime_abort": 0,
            "known_unimplemented": 0,
            "cap_passed": 0,
            "positive_active": 0,
            "negative_active": 0,
            "has_positive": False,
            "has_negative": False,
            "polarity_status": "unimplemented_only",
        }
        for ch in VALID_GUIDE_CHAPTERS
    }
    for spec in probes.values():
        ch = spec.get("guide_chapter", "05-declarations-callables.md")
        row = chapter_stats[ch]
        row["total"] += 1
        kind = spec["kind"]
        row[kind] = row.get(kind, 0) + 1
        if kind in ("run_ok", "compile_fail", "runtime_abort"):
            row["cap_passed"] += 1

    for ch, row in chapter_stats.items():
        pos = row["run_ok"]
        neg = row["compile_fail"] + row["runtime_abort"]
        row["positive_active"] = pos
        row["negative_active"] = neg
        row["has_positive"] = pos > 0
        row["has_negative"] = neg > 0
        if pos > 0 and neg > 0:
            row["polarity_status"] = "both_positive_and_negative"
        elif pos > 0:
            row["polarity_status"] = "positive_only"
        elif neg > 0:
            row["polarity_status"] = "negative_only"
        else:
            row["polarity_status"] = "unimplemented_only"
    return chapter_stats


def summarize_negative_oracle_catalog(expectations: dict[str, Any]) -> dict[str, Any]:
    """Aggregate compile_fail diagnostic codes and runtime_abort contracts across the probe corpus."""
    probes = expectations["probes"]
    by_diagnostic: dict[str, dict[str, Any]] = {}
    by_abort: list[dict[str, Any]] = []

    for name, spec in sorted(probes.items()):
        ch = spec.get("guide_chapter", "05-declarations-callables.md")
        if spec["kind"] == "compile_fail":
            codes = spec.get("all_diagnostics", [spec["diagnostic"]])
            for code in codes:
                bucket = by_diagnostic.setdefault(
                    code,
                    {
                        "code": code,
                        "symbol": DIAGNOSTIC_SYMBOL_CATALOG.get(code, "UNKNOWN"),
                        "chapters": [],
                        "probes": [],
                    },
                )
                if ch not in bucket["chapters"]:
                    bucket["chapters"].append(ch)
                bucket["probes"].append(name)
        elif spec["kind"] == "runtime_abort":
            by_abort.append(
                {
                    "probe": name,
                    "abort_message": spec["abort_message"],
                    "guide_chapter": ch,
                    "derivation": spec.get("oracle_derivation", ""),
                }
            )

    return {
        "diagnostics": dict(sorted(by_diagnostic.items())),
        "runtime_aborts": by_abort,
    }


def render_oracle_audit_markdown(expectations: dict[str, Any]) -> str:
    """Render deterministic Markdown audit ledger from expectations.json."""
    probes = expectations["probes"]
    programs = expectations["programs"]
    chapter_stats = summarize_chapter_polarity(expectations)
    neg_catalog = summarize_negative_oracle_catalog(expectations)

    lines = [
        "# Conformance 强 Oracle 语义推导与 Guide v0.46 章节覆盖对账账本",
        "",
        "> 本文件由 `python3 scripts/conformance_gate.py --init-expectations` 从 `conformance/expectations.json` 确定性生成，并由 `scripts/check_docs.py`（`DocsChecker.check_conformance_corpus`）与 `scripts/tests/test_conformance_gate.py` 自动校验同步。",
        "",
        "## 1. Guide v0.46 章节覆盖与正/负向极性汇总（154 Probes）",
        "",
        "| Guide v0.46 章节 | Probe 总数 | `run_ok` | `compile_fail` | `runtime_abort` | `known_unimplemented` | 能力通过 (`capability_passed`) | 正/负向极性状态 (`polarity_status`) |",
        "|---|---:|---:|---:|---:|---:|---:|---|",
    ]
    tot_all = tot_ok = tot_cf = tot_ab = tot_ku = tot_cap = 0
    both_chapters: list[str] = []
    pos_only_chapters: list[str] = []
    for ch in VALID_GUIDE_CHAPTERS:
        st = chapter_stats[ch]
        tot_all += st["total"]
        tot_ok += st["run_ok"]
        tot_cf += st["compile_fail"]
        tot_ab += st["runtime_abort"]
        tot_ku += st["known_unimplemented"]
        tot_cap += st["cap_passed"]
        pol = st["polarity_status"]
        if pol == "both_positive_and_negative":
            both_chapters.append(ch)
        elif pol == "positive_only":
            pos_only_chapters.append(ch)
        lines.append(
            f"| [{ch}](../docs/guide/{ch}) | {st['total']} | {st['run_ok']} | {st['compile_fail']} | "
            f"{st['runtime_abort']} | {st['known_unimplemented']} | {st['cap_passed']} / {st['total']} | `{pol}` |"
        )
    lines.append(
        f"| **合计** | **{tot_all}** | **{tot_ok}** | **{tot_cf}** | **{tot_ab}** | **{tot_ku}** | **{tot_cap} / {tot_all}** | **{len(both_chapters)} 双极性 / {len(pos_only_chapters)} 仅正向** |"
    )
    lines.extend(
        [
            "",
            f"- **正/负向双极性覆盖章节（{len(both_chapters)} / {len(VALID_GUIDE_CHAPTERS)}）**：" + ", ".join(f"`{c}`" for c in both_chapters) + "（同时具备 `run_ok` 正向运行探针与 `compile_fail` / `runtime_abort` 负向拒绝或中止探针）。",
            f"- **仅正向覆盖章节（{len(pos_only_chapters)} / {len(VALID_GUIDE_CHAPTERS)}，待后续补充独立负向诊断探针）**：" + ", ".join(f"`{c}`" for c in pos_only_chapters) + "（当前语料库内仅有 `run_ok` 与 `known_unimplemented` 探针，尚无独立 `compile_fail` / `runtime_abort` 探针）。",
            "",
            "### 1.1 负向诊断码与运行时中止契约矩阵（14 `compile_fail` + 3 `runtime_abort`）",
            "",
            "| 负向契约标识 | `diagnostic.rs` 符号常量 / 中止类别 | 覆盖 Guide 章节 | 关联 Probe 数量 | 绑定 Probe 列表 |",
            "|---|---|---|---:|---|",
        ]
    )
    for code, info in neg_catalog["diagnostics"].items():
        ch_links = ", ".join(f"[{c}](../docs/guide/{c})" for c in info["chapters"])
        p_list = ", ".join(f"`{p}`" for p in info["probes"])
        lines.append(
            f"| `FE:{code}` | `{info['symbol']}` | {ch_links} | {len(info['probes'])} | {p_list} |"
        )
    for ab in neg_catalog["runtime_aborts"]:
        ch = ab["guide_chapter"]
        lines.append(
            f"| `abort:{ab['abort_message']}` | `SIGABRT (exit 134)` | [{ch}](../docs/guide/{ch}) | 1 | `{ab['probe']}` |"
        )

    # Load v1.json roadmap metadata for blocker names and titles if available
    v1_json_path = Path(__file__).resolve().parents[1] / "docs" / "roadmap" / "v1.json"
    roadmap_items: dict[str, dict[str, Any]] = {}
    if v1_json_path.is_file():
        v1_data = json.loads(v1_json_path.read_text(encoding="utf-8"))
        roadmap_items = {it["id"]: it for it in v1_data.get("items", [])}

    lines.extend(
        [
            "",
            "## 2. 六个 V1 验收程序（Acceptance Programs）",
            "",
            "| 程序 | 归属章节 | 当前状态 | 路线图阻塞项 | 期望输出文件 | 期望输出规模 | 允许的未实现记号 |",
            "|---|---|---|---|---|---|---|",
        ]
    )
    for name, spec in sorted(programs.items()):
        ch = spec.get("guide_chapter", "13-program-runtime-standard-library.md")
        exp_file = spec["expected_file"]
        exp_size = (
            f"{spec['expected_lines']} 行 / {spec['expected_bytes']} B"
            if "expected_lines" in spec and "expected_bytes" in spec
            else "-"
        )
        allowed = ", ".join(f"`{m}`" for m in spec.get("allowed_markers", []))
        blockers = ", ".join(f"`{b}`" for b in spec.get("roadmap_blockers", []))
        lines.append(
            f"| `{name}` | [{ch}](../docs/guide/{ch}) | `{spec['kind']}` | {blockers} | [{exp_file}]({exp_file}) | {exp_size} | {allowed} |"
        )

    # 3. Aggregate every known_unimplemented probe by roadmap blocker
    blocker_probes: dict[str, list[str]] = {}
    for name, spec in sorted(probes.items()):
        if spec["kind"] == "known_unimplemented":
            for b in spec.get("roadmap_blockers", []):
                blocker_probes.setdefault(b, []).append(name)
    unimplemented_total = sum(
        1 for spec in probes.values() if spec["kind"] == "known_unimplemented"
    )

    lines.extend(
        [
            "",
            f"## 3. {unimplemented_total} 个 `known_unimplemented` Probe 按路线图阻塞项（`v1.json`）归类对账",
            "",
            "| 路线图条目 ID | 路线图条目标题 | 状态 | 关联 Probe 总数 | 阻塞的 Probe 列表 |",
            "|---|---|---|---:|---|",
        ]
    )
    for b_id in sorted(blocker_probes.keys()):
        item = roadmap_items.get(b_id, {})
        title = item.get("title", "").replace("|", "\\|")
        status = item.get("status", "unknown")
        plist = blocker_probes[b_id]
        pnames = ", ".join(f"`{p}`" for p in plist)
        lines.append(
            f"| `{b_id}` | {title} | `{status}` | {len(plist)} | {pnames} |"
        )

    lines.extend(
        [
            "",
            "## 4. 154 个 Probe 逐项强 Oracle 与语义推导账本",
            "",
            "| Probe | Guide 章节 | 类型 (`kind`) | 溯源 (`oracle_origin`) | 期望结果 / 诊断码 | 语义推导 / 规则说明 (`oracle_derivation` / `note`) |",
            "|---|---|---|---|---|---|",
        ]
    )
    for name, spec in sorted(probes.items()):
        ch = spec.get("guide_chapter", "")
        kind = spec["kind"]
        origin = spec.get("oracle_origin", "")
        if kind == "run_ok":
            oracle_repr = f"`{spec['stdout'].encode('unicode_escape').decode('ascii')}`"
        elif kind == "compile_fail":
            diags = spec.get("all_diagnostics", [spec["diagnostic"]])
            oracle_repr = ", ".join(f"`FE:{d}`" for d in diags)
        elif kind == "runtime_abort":
            oracle_repr = f"`abort:{spec['abort_message']}`"
        else:
            markers_repr = ", ".join(f"`{m}`" for m in spec.get("allowed_markers", []))
            target_out = spec.get("target_stdout")
            if target_out is not None:
                escaped_target = target_out.encode("unicode_escape").decode("ascii")
                oracle_repr = f"{markers_repr} → target `{escaped_target}`"
            else:
                oracle_repr = markers_repr
        deriv = (spec.get("oracle_derivation") or spec.get("note") or "-").replace("|", "\\|")
        lines.append(
            f"| `{name}` | [{ch}](../docs/guide/{ch}) | `{kind}` | `{origin}` | {oracle_repr} | {deriv} |"
        )
    lines.append("")
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--cli",
        type=Path,
        default=resolve_default_cli(root),
        help="path to kovenc binary (auto-detects KOVENC, target/release/kovenc, or target/debug/kovenc)",
    )
    parser.add_argument(
        "--corpus",
        type=Path,
        default=root / "conformance",
        help="path to conformance directory",
    )
    parser.add_argument(
        "--expectations",
        type=Path,
        default=root / "conformance/expectations.json",
        help="path to machine-readable expectations JSON",
    )
    parser.add_argument(
        "--baseline",
        type=Path,
        default=None,
        help="optional historical baseline TSV for regression comparison",
    )
    parser.add_argument(
        "--evidence",
        type=Path,
        default=None,
        help="directory to write raw stdout/stderr bytes, results.json, and matrix.tsv",
    )
    parser.add_argument(
        "--probe",
        action="append",
        dest="probes",
        help="run only selected probe name(s) (repeatable)",
    )
    parser.add_argument(
        "--no-programs",
        action="store_true",
        help="skip the 6 acceptance programs",
    )
    parser.add_argument(
        "--timeout",
        type=float,
        default=float(os.environ.get("PROBE_TIMEOUT", "60")),
        help="per-invocation timeout in seconds",
    )
    parser.add_argument(
        "--jobs",
        "-j",
        type=int,
        default=int(
            os.environ.get("KOVEN_CONFORMANCE_JOBS", str(min(os.cpu_count() or 4, 4)))
        ),
        help="number of parallel worker threads (deterministic output order)",
    )
    parser.add_argument(
        "--allow-unreviewed",
        action="store_true",
        help="do not fail the gate on unreviewed items (observation mode)",
    )
    parser.add_argument(
        "--compiler-commit",
        default=detect_git_commit(root),
        help="compiler git commit SHA to bind in results.json",
    )
    parser.add_argument(
        "--init-expectations",
        action="store_true",
        help="derive static expectations.json from corpus sources + baseline TSV without running compiler",
    )
    args = parser.parse_args(argv)

    if args.init_expectations:
        baseline_file = args.baseline or (args.corpus / "baseline-aef3f4f8.tsv")
        derived = derive_static_expectations(args.corpus, baseline_file)
        args.expectations.write_text(
            json.dumps(derived, ensure_ascii=True, indent=2) + "\n",
            encoding="utf-8",
        )
        if (args.corpus / "baseline-aef3f4f8.tsv").is_file():
            (args.corpus / "ORACLE_AUDIT.md").write_text(
                render_oracle_audit_markdown(derived),
                encoding="utf-8",
            )
        print(
            f"conformance: initialized {len(derived['probes'])} probes and "
            f"{len(derived['programs'])} programs in {args.expectations}"
        )
        return 0

    ledger = evaluate_suite(
        kovenc=args.cli,
        corpus_root=args.corpus,
        expectations_path=args.expectations,
        baseline_path=args.baseline,
        selected_probes=args.probes,
        include_programs=not args.no_programs,
        timeout_sec=args.timeout,
        strict_unreviewed=not args.allow_unreviewed,
        compiler_commit=args.compiler_commit,
        evidence_dir=args.evidence,
        jobs=args.jobs,
    )
    summary = ledger["summary"]
    print(
        f"conformance: probes_cap_passed={summary['probe_capability_passed']}/{summary['total_probes']} "
        f"programs_passed={summary['program_run_passed']}/{summary['total_programs']} "
        f"counts={json.dumps(summary['counts'], sort_keys=True)}"
    )
    if not summary["gate_passed"]:
        for failure in summary["failures"]:
            print(f"FAIL: {failure}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
