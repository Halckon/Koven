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
ABORT_FRAME_RE = re.compile(rb"(?m)^  at \S+")
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

VALID_UNIMPLEMENTED_MARKERS = frozenset(
    {
        "BE:UnsupportedNode",
        "BE:UnsupportedSource",
        "BE:MissingFact",
        "FE:L0080",
        "FE:L0084",
        "FE:L0085",
        "FE:L0165",
    }
)

UNIMPLEMENTED_MARKER_CATALOG: dict[str, tuple[str, str]] = {
    "FE:L0080": ("Phase 2 名称解析", "前端未解析符号或尚未注入的标准库声明（`UNRESOLVED_NAME`）"),
    "FE:L0084": ("Phase 2 类型检查", "前端类型不匹配或转换契约尚未接入（`TYPE_MISMATCH`）"),
    "FE:L0085": ("Phase 2 运算符类型", "前端操作数或协议约束尚未接入（`INVALID_OPERAND_TYPES`）"),
    "FE:L0165": ("Phase 2 最终类型准入", "前端最终类型消费准入显式拦截（`FINAL_TYPE_ANALYSIS_INCOMPLETE`）"),
    "BE:UnsupportedNode": ("Phase 4 SSA Lowering", "后端 SSA 构建阶段尚未支持的语法或语义节点"),
    "BE:UnsupportedSource": ("Phase 4 Native Object", "后端原生目标文件生成阶段尚未支持的调用来源"),
    "BE:MissingFact": ("Phase 3/4 事实交接", "前端到后端 lowering 所需的所有权或类型事实尚未完备"),
}

VALID_ORACLE_ORIGINS = frozenset(
    {
        "single_println_literal",
        "source_trace_audited",
        "v046_spec_contract",
        "baseline_diagnostic_audited",
        "baseline_unimplemented_audited",
        "expected_txt_file",
    }
)

ORACLE_ORIGIN_DESCRIPTIONS: dict[str, str] = {
    "single_println_literal": "单条无循环/无插值 `println(\"...\")` 静态字面量精确推导",
    "source_trace_audited": "控制流、循环、集合或 `deinit` 多语句源码级人工追踪确认（`CONFIRMED_STDOUT_ORACLES`）",
    "v046_spec_contract": "Guide v0.46 语法/语义变更或 `baseline-aef3f4f8.tsv` 之后合入的 Spec 合同覆盖（`V046_PROBE_OVERRIDES`）",
    "baseline_diagnostic_audited": "前端负向诊断码契约确认（`error[Lxxxx]` + `diagnostic.rs::codes` 符号匹配）",
    "baseline_unimplemented_audited": "已知未实现特性边界确认（绑定 `target_stdout` 与 `v1.json` 路线图阻塞项）",
    "expected_txt_file": "V1 验收程序独立 `expected.txt` 黄金标准输出文件绑定（SHA-256 + 行数/字节数校验）",
}

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

GUIDE_CHAPTER_DOMAINS: dict[str, str] = {
    "01-lexical.md": "词法",
    "02-names-files-packages.md": "名称、文件、Package 与 Import",
    "03-types-generics.md": "类型与泛型",
    "04-expressions-operators.md": "表达式与运算符",
    "05-declarations-callables.md": "声明与 Callable",
    "06-blocks-control-flow.md": "Block 与控制流",
    "07-calls-lambdas-closures.md": "调用、Lambda 与 Closure",
    "08-class-family-members.md": "Class Family、成员与 Receiver",
    "09-nullability-errors.md": "空安全与错误值",
    "10-ownership-borrowing-drop.md": "所有权、借用与析构",
    "11-copyability-layout-construction.md": "Copyable、布局、构造与结构移动",
    "12-collections-destructuring.md": "集合、索引与解构",
    "13-program-runtime-standard-library.md": "程序入口、Runtime 与标准库",
}

PROBE_BATCH_CATALOG: dict[str, str] = {
    "p": "基础能力：标量、控制流、class/enum/value class、nullable、Result、闭包、容器、所有权等",
    "q": "对 `p` 中失败项的变体复测，缩小失败条件",
    "r": "未知成员访问，期望前端诊断",
    "s": "标准环境：内建 `Result`、`Pair`/`to` 与 `?` 的边界",
    "t": "SPEC-0311–0318 的定向复测：value class 方法、Map/List 遍历中的跳转、短路求值",
    "u": "enum case 作借用实参、循环中 `MutableList.add` 等增量复测",
    "v": "v0.44 补全面：赋值结果、整数 Range、显式整数转换、nullable 循环、Map consume",
    "w": "Map/MutableMap：遍历顺序、覆盖与删除、遍历中的跳转与借用",
    "x": "nullable class、String nullable 返回与条件中的借用实参复测",
    "y": "`koven.algorithms` 的 `take` 与 N1a 范围借用",
    "e": "enum subject `when` 在 compilation-unit 管线中的回退（E2 门槛）",
}

PROBE_NAME_CONVENTION_RE = re.compile(
    r"^(?:(w)(1[2-9])|([epqrstuvwxy])([0-9]{2}[a-z]?)_([A-Za-z0-9_]+))$"
)


FOUNDATIONAL_BATCHES = frozenset({"p", "q", "r", "s"})
FOUNDATIONAL_GROUP_B_PROBES = frozenset(
    {
        "p01_int_print",
        "p03b_byte_literal_cmp",
        "p25_linked_iter",
        "p42_lambda_mutate",
        "p43_if_join_any",
        "r01_bogus_member",
        "r02_bogus_member_str",
        "s04_question_on_nullable",
    }
)
FOUNDATIONAL_GROUP_C_PROBES = frozenset(
    {
        "p23_global_var",
        "p35_value_eq",
        "p37_is_generic_case",
    }
)
P2_GROUP_A_TARGET_MIN = 70


def classify_probe_batch(name: str) -> str:
    """Extract and validate the canonical single-letter probe batch prefix."""
    m = PROBE_NAME_CONVENTION_RE.fullmatch(name)
    if m is None:
        raise ValueError(
            f"probe name {name!r} violates canonical batch naming convention "
            f"(<batch><NN>[a-z]_<topic> or w12..w19)"
        )
    return m.group(1) or m.group(3)

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

# Explicit mapping from all 17 known_unimplemented probes to blocking item IDs in docs/roadmap/v1.json.
UNIMPLEMENTED_ROADMAP_BLOCKERS: dict[str, list[str]] = {
    "p02_interp": ["L1", "L2"],
    "p05_double": ["N05"],
    "p06_char": ["N05"],
    "p11_interface_bound": ["N06"],
    "p15_elvis": ["N03"],
    "p16_result_q": ["N08"],
    "p21_box_enum": ["N07", "L7"],
    "p25b_linked_recursive": ["N03"],
    "p27_thread": ["T1", "N04"],
    "p28_companion": ["N06"],
    "p35_value_eq": ["L1"],
    "p40_nested_generic_class": ["N01"],
    "q03_fn_ref": ["N04"],
    "q08_enum_string_payload": ["N02", "L7"],
    "s02_pair_builtin": ["S6"],
    "s03_to_pair": ["S6"],
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
    "p16b_result_string": "parse(1) selects Result.Ok('good'); exhaustive when selects Ok and prints success once, followed by newline",
    "p17_lambda": "capturing closure { n -> n * k } with k = 3 applied to 7 yields 21 and prints 'lambda'",
    "p17b_lambda_nocapture": "non-capturing lambda { n -> n * 3 } applied to 7 yields 21 and prints 'lambda0'",
    "p18_move_closure": "move closure captures owned String 'x', compares s == 'x' -> true, and prints 'move'",
    "p21_box_enum": "recursive Expr.Add(Box(Num(1)), Box(Num(2))) evaluates to 3 and prints 'box' (blocked by N07/L7)",
    "p22_rc": "Rc(Data(4)).share().value.n == 4 holds and prints 'rc'",
    "p25b_linked_recursive": "recursive sum(Node(1, Node(2, null))) returns 3 and prints 'rec-list' (blocked by N03)",
    "p27_thread": "thread(move { println('thread') }).join() runs worker closure and prints 'thread' (blocked by T1/N04)",
    "p28_companion": "anonymous companion function C.make() returns 7 and prints 'companion' (blocked by N06)",
    "p35_value_eq": "structural equality Point(1, 2) == Point(1, 2) holds and prints 'veq' (blocked by L1)",
    "p40_nested_generic_class": "Stack<Int>.push(1) mutates generic field items; size == 1 prints 'stack' (blocked by N01)",
    "q02_lambda_arg_inline": "twice({ n -> n + 1 }) applies +1 twice to 1 -> 3 and prints 'lambda-arg'",
    "q03_fn_ref": "callable reference ::inc applied to 1 yields 2 and prints 'fnref' (blocked by N04)",
    "q08_enum_string_payload": "show(Msg.Text('hi')) prints payload 'hi'; show(Msg.Quit) prints 'quit' (blocked by N02/L7)",
    "s02_pair_builtin": "builtin Pair(1, 2).first == 1 holds and prints 'pair' (blocked by S6)",
    "s03_to_pair": "infix 1 to 2 constructs Pair(1, 2); p.first == 1 prints 'to' (blocked by S6)",
    "t03_vmethod_generic": "generic value class W<Int>(4).get() returns 4 and prints 'vmethod-generic'",
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
        "note": "Guide v0.46 anonymous companion { } syntax (SPEC-0342); SPEC-0323 fixed static role, native N06 pending",
    },
    "q12_companion_const": {
        "kind": "run_ok",
        "stdout": "cconst\n",
        "v046_baseline_exempt": True,
        "note": "Guide v0.46 anonymous companion { } syntax (SPEC-0342); associated constant read succeeds",
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
        "kind": "run_ok",
        "stdout": "good\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0349 builtin Result; SPEC-0368 String payload construction and exhaustive case reads in both public entry modes",
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
    "p22_rc": {
        "kind": "run_ok",
        "stdout": "rc\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0359: Rc construction, share() and rc.value field projection in E3 refinement layer",
    },
    "t03_vmethod_generic": {
        "kind": "run_ok",
        "stdout": "vmethod-generic\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0361: generic value-class instance method in E3 refinement layer",
    },
    "p17_lambda": {
        "kind": "run_ok",
        "stdout": "lambda\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0364: lambda with a shared capture passed as a Borrow callable view (ADR-0034)",
    },
    "p17b_lambda_nocapture": {
        "kind": "run_ok",
        "stdout": "lambda0\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0364: capture-free lambda passed as a Borrow callable view (ADR-0034)",
    },
    "q02_lambda_arg_inline": {
        "kind": "run_ok",
        "stdout": "lambda-arg\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0364: nested calls through a Borrow callable view (ADR-0034)",
    },
    "p18_move_closure": {
        "kind": "run_ok",
        "stdout": "move\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0364/0365: a factory returns a move closure with its concrete layout (ADR-0034 section 4)",
    },
    "e01_enum_when_bool_return": {
        "kind": "run_ok",
        "stdout": "e1\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0320 / SPEC-0356: enum when bool return regression fixed on main",
    },
    "e02_enum_when_stmt_println": {
        "kind": "run_ok",
        "stdout": "e2\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0320 / SPEC-0356: enum when statement regression fixed on main",
    },
    "e03_enum_when_bool_val": {
        "kind": "run_ok",
        "stdout": "e5\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0320 / SPEC-0356: enum when bool val regression fixed on main",
    },
    "p49_when_enum_no_payload": {
        "kind": "run_ok",
        "stdout": "green\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0320 / SPEC-0356: payloadless enum when prints name(Color.Green) -> green",
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
        "note": "SPEC-0341: take() lowered on main; for-loop sums the first two elements and prints take",
    },
    "y03_method_take_for": {
        "kind": "run_ok",
        "stdout": "take-method\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0341: take() method call lowered on main; for-loop sums the first two elements and prints take-method",
    },
    "y04_take_for_println": {
        "kind": "run_ok",
        "stdout": "a\nb\n",
        "v046_baseline_exempt": True,
        "note": "SPEC-0341: take() lowered on main; for-loop prints the first two elements",
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
        if not ABORT_FRAME_RE.search(stderr):
            return (
                "wrong_output",
                "runtime_abort stderr missing '  at <location>' stack frame per Guide 09 D12",
            )
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
        "probe_batch": expectation.get("probe_batch"),
        "baseline_marker": baseline_row[0] if baseline_row else expectation.get("baseline_marker"),
        "v046_baseline_exempt": bool(expectation.get("v046_baseline_exempt", False)),
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
                elif (
                    not isinstance(spec.get("stdout"), str)
                    or not spec["stdout"]
                    or not spec["stdout"].endswith("\n")
                ):
                    raise ValueError(
                        f"probes.{name} with run_ok requires non-empty stdout ending with newline"
                    )
                if section == "probes":
                    out_text = spec["stdout"]
                    out_bytes = out_text.encode("utf-8")
                    if "expected_lines" in spec and spec["expected_lines"] != len(out_text.splitlines()):
                        raise ValueError(
                            f"expected_lines mismatch for probes.{name}: expected {spec['expected_lines']}, got {len(out_text.splitlines())}"
                        )
                    if "expected_bytes" in spec and spec["expected_bytes"] != len(out_bytes):
                        raise ValueError(
                            f"expected_bytes mismatch for probes.{name}: expected {spec['expected_bytes']}, got {len(out_bytes)}"
                        )
            elif kind == "compile_fail":
                if any(
                    k in spec
                    for k in ("stdout", "target_stdout", "expected_lines", "expected_bytes")
                ):
                    raise ValueError(
                        f"{section}.{name} with compile_fail must not define stdout/expected_lines/expected_bytes"
                    )
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
                if any(
                    k in spec
                    for k in ("stdout", "target_stdout", "expected_lines", "expected_bytes")
                ):
                    raise ValueError(
                        f"{section}.{name} with runtime_abort must not define stdout/expected_lines/expected_bytes"
                    )
                if not isinstance(spec.get("abort_message"), str) or not spec["abort_message"]:
                    raise ValueError(
                        f"{section}.{name} with runtime_abort requires abort_message"
                    )
            elif kind == "known_unimplemented":
                allowed = spec.get("allowed_markers")
                if (
                    not isinstance(allowed, list)
                    or not allowed
                    or len(allowed) != len(set(allowed))
                    or not all(
                        isinstance(m, str) and m in VALID_UNIMPLEMENTED_MARKERS
                        for m in allowed
                    )
                ):
                    raise ValueError(
                        f"{section}.{name} with known_unimplemented has invalid allowed_markers {allowed!r}"
                    )
                bm = spec.get("baseline_marker")
                if (
                    isinstance(bm, str)
                    and not spec.get("v046_baseline_exempt", False)
                    and bm in VALID_UNIMPLEMENTED_MARKERS
                    and bm not in allowed
                ):
                    raise ValueError(
                        f"{section}.{name} with known_unimplemented must include non-exempt baseline_marker {bm!r} in allowed_markers"
                    )
                if "target_stdout" in spec:
                    ts = spec["target_stdout"]
                    if not isinstance(ts, str) or not ts or not ts.endswith("\n"):
                        raise ValueError(
                            f"{section}.{name} with known_unimplemented has invalid target_stdout {ts!r} (must be non-empty and end with newline)"
                        )
                    if section == "probes":
                        ts_bytes = ts.encode("utf-8")
                        if "expected_lines" in spec and spec["expected_lines"] != len(ts.splitlines()):
                            raise ValueError(
                                f"expected_lines mismatch for probes.{name}: expected {spec['expected_lines']}, got {len(ts.splitlines())}"
                            )
                        if "expected_bytes" in spec and spec["expected_bytes"] != len(ts_bytes):
                            raise ValueError(
                                f"expected_bytes mismatch for probes.{name}: expected {spec['expected_bytes']}, got {len(ts_bytes)}"
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
            for src_metric_key in ("source_lines", "source_bytes"):
                if src_metric_key in spec and (
                    not isinstance(spec[src_metric_key], int)
                    or isinstance(spec[src_metric_key], bool)
                    or spec[src_metric_key] < 1
                ):
                    raise ValueError(f"{section}.{name} has invalid {src_metric_key}")
            if "baseline_marker" in spec and (
                not isinstance(spec["baseline_marker"], str)
                or not spec["baseline_marker"].strip()
            ):
                raise ValueError(f"{section}.{name} has invalid baseline_marker")
            if "oracle_origin" in spec and (
                not isinstance(spec["oracle_origin"], str)
                or spec["oracle_origin"] not in VALID_ORACLE_ORIGINS
            ):
                raise ValueError(
                    f"{section}.{name} has invalid oracle_origin {spec.get('oracle_origin')!r}"
                )
            if "spec_refs" in spec:
                srefs = spec["spec_refs"]
                if (
                    not isinstance(srefs, list)
                    or not srefs
                    or len(srefs) != len(set(srefs))
                    or not all(
                        isinstance(s, str) and re.fullmatch(r"SPEC-[0-9]{4}", s)
                        for s in srefs
                    )
                ):
                    raise ValueError(f"{section}.{name} has invalid spec_refs {srefs!r}")
                if corpus_root is not None:
                    docs_root = corpus_root.parent / "docs"
                    if (docs_root / "specs").is_dir():
                        for sref in srefs:
                            if resolve_spec_doc_relpath(sref, corpus_root.parent) is None:
                                raise ValueError(
                                    f"{section}.{name} references missing spec document for {sref}"
                                )
            elif (
                spec.get("v046_baseline_exempt")
                and corpus_root is not None
                and (corpus_root.parent / "docs" / "specs").is_dir()
            ):
                raise ValueError(
                    f"{section}.{name} with v046_baseline_exempt requires non-empty spec_refs"
                )
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
                    if guide_dir.is_dir():
                        ch_path = guide_dir / ch
                        if not ch_path.is_file():
                            raise ValueError(
                                f"{section}.{name} references missing Guide chapter {ch_path}"
                            )
                        expected_domain = GUIDE_CHAPTER_DOMAINS[ch]
                        ch_h1 = next(
                            (
                                ln
                                for ln in ch_path.read_text(encoding="utf-8").splitlines()
                                if ln.startswith("# ")
                            ),
                            "",
                        )
                        if not ch_h1.endswith(f"：{expected_domain}"):
                            raise ValueError(
                                f"Guide chapter {ch} H1 {ch_h1!r} does not match expected domain {expected_domain!r}"
                            )
            if section == "probes":
                inferred_batch = classify_probe_batch(name)
                if "probe_batch" in spec:
                    pb = spec["probe_batch"]
                    if (
                        not isinstance(pb, str)
                        or pb not in PROBE_BATCH_CATALOG
                        or pb != inferred_batch
                    ):
                        raise ValueError(
                            f"probes.{name} has invalid probe_batch {pb!r} (expected {inferred_batch!r})"
                        )
            elif "probe_batch" in spec:
                raise ValueError(f"programs.{name} must not define probe_batch")

            if corpus_root is not None:
                src_file = (
                    corpus_root / "probes" / f"{name}.ko"
                    if section == "probes"
                    else corpus_root / "programs" / name / "main.ko"
                )
                if not src_file.is_file():
                    raise ValueError(f"missing source file for {section}.{name}: {src_file}")
                src_bytes = src_file.read_bytes()
                if not src_bytes or not src_bytes.endswith(b"\n") or b"\r" in src_bytes:
                    raise ValueError(
                        f"source file for {section}.{name} must be non-empty, use LF line endings, and end with trailing newline"
                    )
                actual_sha = sha256_bytes(src_bytes)
                if actual_sha != spec["source_sha256"]:
                    raise ValueError(
                        f"source_sha256 mismatch for {section}.{name}: expected {spec['source_sha256']}, got {actual_sha}"
                    )
                src_text = src_bytes.decode("utf-8")
                if "source_bytes" in spec and spec["source_bytes"] != len(src_bytes):
                    raise ValueError(
                        f"source_bytes mismatch for {section}.{name}: expected {spec['source_bytes']}, got {len(src_bytes)}"
                    )
                if "source_lines" in spec and spec["source_lines"] != len(src_text.splitlines()):
                    raise ValueError(
                        f"source_lines mismatch for {section}.{name}: expected {spec['source_lines']}, got {len(src_text.splitlines())}"
                    )
                if PACKAGE_DIRECTIVE_RE.search(src_text):
                    raise ValueError(
                        f"{section}.{name} must not contain a package directive (collides with project-mode package app header)"
                    )
                main_matches = MAIN_ENTRY_RE.findall(src_text)
                if len(main_matches) != 1:
                    raise ValueError(
                        f"{section}.{name} must declare a single fun main(...) entry point (found {len(main_matches)})"
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
            verify_static_catalog_invariants(data)
    return data


def verify_static_catalog_invariants(expectations: dict[str, Any]) -> None:
    """Verify that static catalog tables in conformance_gate.py have no stale keys or unimplemented drift."""
    probes = expectations["probes"]
    programs = expectations["programs"]
    probe_keys = set(probes.keys())
    prog_keys = set(programs.keys())
    ku_probes = {k for k, v in probes.items() if v["kind"] == "known_unimplemented"}
    if ku_probes != set(UNIMPLEMENTED_ROADMAP_BLOCKERS.keys()):
        stale = sorted(set(UNIMPLEMENTED_ROADMAP_BLOCKERS.keys()) - ku_probes)
        missing = sorted(ku_probes - set(UNIMPLEMENTED_ROADMAP_BLOCKERS.keys()))
        raise ValueError(
            f"UNIMPLEMENTED_ROADMAP_BLOCKERS drift: stale={stale}, missing={missing}"
        )
    for label, catalog in (
        ("PROBE_GUIDE_CHAPTERS", PROBE_GUIDE_CHAPTERS),
        ("CONFIRMED_STDOUT_ORACLES", CONFIRMED_STDOUT_ORACLES),
        ("SEMANTIC_ORACLE_DERIVATIONS", SEMANTIC_ORACLE_DERIVATIONS),
        ("V046_PROBE_OVERRIDES", V046_PROBE_OVERRIDES),
        ("EXPECTED_MULTI_DIAGNOSTICS", EXPECTED_MULTI_DIAGNOSTICS),
    ):
        extra = sorted(set(catalog.keys()) - probe_keys)
        if extra:
            raise ValueError(f"{label} contains unknown probe keys: {extra}")
    for label, catalog in (
        ("PROGRAM_GUIDE_CHAPTERS", PROGRAM_GUIDE_CHAPTERS),
        ("PROGRAM_ROADMAP_BLOCKERS", PROGRAM_ROADMAP_BLOCKERS),
        ("PROGRAM_ORACLE_DERIVATIONS", PROGRAM_ORACLE_DERIVATIONS),
    ):
        if set(catalog.keys()) != prog_keys:
            raise ValueError(f"{label} keys do not match programs inventory")
    if list(GUIDE_LITMUS_CATALOG.keys()) != list(range(1, 13)):
        raise ValueError("GUIDE_LITMUS_CATALOG must define Litmus 1..12 in order")
    for lit_id, lit_spec in GUIDE_LITMUS_CATALOG.items():
        if lit_spec.get("guide_chapter") not in VALID_GUIDE_CHAPTERS:
            raise ValueError(f"Litmus {lit_id} has invalid guide_chapter")
        lit_probes = lit_spec.get("probes", [])
        if not lit_probes or any(p not in probe_keys for p in lit_probes):
            raise ValueError(f"Litmus {lit_id} references unknown probes in {lit_probes!r}")
    guide15_path = (
        Path(__file__).resolve().parents[1]
        / "docs"
        / "guide"
        / "15-conformance-and-staging.md"
    )
    if guide15_path.is_file():
        g15_text = guide15_path.read_text(encoding="utf-8")
        found_litmus = {
            int(m.group(1)): m.group(2).strip()
            for m in re.finditer(r"(?m)^### Litmus ([0-9]+):\s*(.+)$", g15_text)
        }
        expected_litmus = {
            lit_id: spec["title"] for lit_id, spec in GUIDE_LITMUS_CATALOG.items()
        }
        if found_litmus != expected_litmus:
            raise ValueError(
                f"Guide 15 Litmus headings mismatch: found={found_litmus}, expected={expected_litmus}"
            )
        for ver_key in GUIDE_VERSION_BOUNDARY_CATALOG:
            if f"## {ver_key} 启用边界" not in g15_text:
                raise ValueError(f"Guide 15 missing heading '## {ver_key} 启用边界'")
    if list(GUIDE_VERSION_BOUNDARY_CATALOG.keys()) != ["v0.44", "v0.45", "v0.46", "v0.47"]:
        raise ValueError("GUIDE_VERSION_BOUNDARY_CATALOG must define v0.44..v0.47 in order")
    for ver_key, ver_spec in GUIDE_VERSION_BOUNDARY_CATALOG.items():
        v_probes = ver_spec.get("probes", [])
        if not v_probes or any(p not in probe_keys for p in v_probes):
            raise ValueError(f"Guide version boundary {ver_key} references unknown probes in {v_probes!r}")
    if set(PROGRAM_PREREQUISITE_PROBES.keys()) != prog_keys:
        raise ValueError("PROGRAM_PREREQUISITE_PROBES keys do not match programs inventory")
    for prog_name, pre_probes in PROGRAM_PREREQUISITE_PROBES.items():
        if not pre_probes or any(p not in probe_keys for p in pre_probes):
            raise ValueError(
                f"PROGRAM_PREREQUISITE_PROBES[{prog_name!r}] references unknown probes in {pre_probes!r}"
            )
    expected_v3_cats = [
        "deterministic_deinit_drop",
        "heap_class_rc_and_box",
        "string_and_text_owner",
        "collection_list_and_map_provider",
        "closure_env_and_thread_spawn",
        "checked_abort_and_overflow",
    ]
    if list(V3_SANITIZER_RESOURCE_CATALOG.keys()) != expected_v3_cats:
        raise ValueError("V3_SANITIZER_RESOURCE_CATALOG must define the 6 canonical V3 categories in order")
    v3_covered_programs: set[str] = set()
    for cat_key, cat_spec in V3_SANITIZER_RESOURCE_CATALOG.items():
        c_probes = cat_spec.get("probes", [])
        c_progs = cat_spec.get("programs", [])
        if not c_probes or any(p not in probe_keys for p in c_probes):
            raise ValueError(f"V3_SANITIZER_RESOURCE_CATALOG[{cat_key!r}] references unknown probes in {c_probes!r}")
        if any(pr not in prog_keys for pr in c_progs):
            raise ValueError(f"V3_SANITIZER_RESOURCE_CATALOG[{cat_key!r}] references unknown programs in {c_progs!r}")
        v3_covered_programs.update(c_progs)
    if v3_covered_programs != prog_keys:
        raise ValueError(f"V3_SANITIZER_RESOURCE_CATALOG programs coverage mismatch: {sorted(v3_covered_programs)}")


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
    probe_src_metrics: dict[str, tuple[int, int]] = {}
    for path in sorted(probes_dir.glob("*.ko")):
        name = path.stem
        # 命名违规与 baseline 缺项都要给出可读诊断，不能落到 baseline[name] 的 KeyError
        classify_probe_batch(name)
        if name not in baseline:
            raise ValueError(
                f"probes.{name} is missing from baseline matrix {baseline_path.name}; "
                f"add a baseline row for the probe or rename the source file"
            )
        raw_bytes = path.read_bytes()
        if not raw_bytes or not raw_bytes.endswith(b"\n") or b"\r" in raw_bytes:
            raise ValueError(
                f"source file for probes.{name} must be non-empty, use LF line endings, and end with trailing newline"
            )
        src_text = raw_bytes.decode("utf-8")
        probe_src_metrics[name] = (len(src_text.splitlines()), len(raw_bytes))
        src_sha = sha256_bytes(raw_bytes)
        base_single, base_project = baseline[name]
        if base_single != base_project:
            raise ValueError(
                f"baseline mode mismatch for probes.{name}: single={base_single!r} vs project={base_project!r}"
            )

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
            entry["baseline_marker"] = base_single
            entry["oracle_origin"] = "v046_spec_contract"
            spec_refs = list(
                dict.fromkeys(re.findall(r"SPEC-[0-9]{4}", entry.get("note", "")))
            )
            if spec_refs:
                entry["spec_refs"] = spec_refs
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
                    "baseline_marker": "OK",
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
                    "baseline_marker": base_single,
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
                    "baseline_marker": base_single,
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
                "baseline_marker": base_single,
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

    for name, entry in probes_out.items():
        src_lines, src_bytes_len = probe_src_metrics[name]
        entry["source_lines"] = src_lines
        entry["source_bytes"] = src_bytes_len
        out_text = entry.get("stdout") or entry.get("target_stdout")
        if isinstance(out_text, str) and out_text:
            entry["expected_lines"] = len(out_text.splitlines())
            entry["expected_bytes"] = len(out_text.encode("utf-8"))

    programs_out: dict[str, Any] = {}
    for prog_dir in sorted(programs_dir.iterdir()):
        if not prog_dir.is_dir():
            continue
        main_ko = prog_dir / "main.ko"
        expected_txt = prog_dir / "expected.txt"
        if not main_ko.is_file() or not expected_txt.is_file():
            continue
        main_bytes = main_ko.read_bytes()
        if not main_bytes or not main_bytes.endswith(b"\n") or b"\r" in main_bytes:
            raise ValueError(
                f"source file for programs.{prog_dir.name} must be non-empty, use LF line endings, and end with trailing newline"
            )
        main_text = main_bytes.decode("utf-8")
        exp_bytes = expected_txt.read_bytes()
        exp_text = exp_bytes.decode("utf-8")
        prog_entry: dict[str, Any] = {
            "kind": "known_unimplemented",
            "source_sha256": sha256_bytes(main_bytes),
            "source_lines": len(main_text.splitlines()),
            "source_bytes": len(main_bytes),
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


def resolve_chapter_selector(selector: str) -> str:
    """Resolve a Guide chapter filename or 2-digit prefix ('01'..'13') to a canonical chapter filename."""
    cleaned = selector.strip()
    if cleaned in VALID_GUIDE_CHAPTERS:
        return cleaned
    if re.fullmatch(r"[0-9]{2}", cleaned):
        prefix = f"{cleaned}-"
        for ch in VALID_GUIDE_CHAPTERS:
            if ch.startswith(prefix):
                return ch
    raise ValueError(f"unknown Guide chapter selector: {selector!r}")


def evaluate_suite(
    kovenc: Path,
    corpus_root: Path,
    expectations_path: Path,
    baseline_path: Path | None = None,
    selected_probes: list[str] | None = None,
    selected_batches: list[str] | None = None,
    selected_chapters: list[str] | None = None,
    selected_programs: list[str] | None = None,
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
    if timeout_sec <= 0:
        raise ValueError(f"timeout_sec must be > 0, got {timeout_sec}")
    if selected_programs is not None and not include_programs:
        raise ValueError("cannot combine --program with --no-programs")
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
    if selected_batches is not None:
        unknown_batches = sorted(set(selected_batches) - set(PROBE_BATCH_CATALOG.keys()))
        if unknown_batches:
            raise ValueError(f"unknown probe batches requested: {unknown_batches}")
        if len(selected_batches) != len(set(selected_batches)):
            raise ValueError("duplicate probe batches in selection")
        batch_set = set(selected_batches)
        probe_names = [
            n
            for n in probe_names
            if (expectations["probes"][n].get("probe_batch") or classify_probe_batch(n))
            in batch_set
        ]
    if selected_chapters is not None:
        resolved_chapters = [resolve_chapter_selector(c) for c in selected_chapters]
        if len(resolved_chapters) != len(set(resolved_chapters)):
            raise ValueError("duplicate Guide chapters in selection")
        chapter_set = set(resolved_chapters)
        probe_names = [
            n
            for n in probe_names
            if expectations["probes"][n].get("guide_chapter") in chapter_set
        ]
    if (
        selected_probes is not None
        or selected_batches is not None
        or selected_chapters is not None
    ) and not probe_names:
        raise ValueError("no probes matched the requested filters")

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
    if selected_programs is not None:
        unknown_progs = sorted(set(selected_programs) - set(program_names))
        if unknown_progs:
            raise ValueError(f"unknown program names requested: {unknown_progs}")
        if len(selected_programs) != len(set(selected_programs)):
            raise ValueError("duplicate program names in selection")
        program_names = [n for n in program_names if n in set(selected_programs)]
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
        (evidence_dir / "SUMMARY.md").write_text(
            render_evidence_summary_markdown(ledger),
            encoding="utf-8",
        )

    return ledger


def render_evidence_summary_markdown(ledger: dict[str, Any]) -> str:
    """Render deterministic Markdown execution summary (SUMMARY.md) for a conformance evidence bundle."""
    comp = ledger["compiler"]
    host = ledger["host"]
    summary = ledger["summary"]
    verdict_str = "PASS" if summary["gate_passed"] else "FAIL"
    fp2 = summary.get("foundational_p2", {})
    lines = [
        "# Conformance 回归验收门禁执行摘要（SUMMARY.md）",
        "",
        "| 维度 | 值 |",
        "|---|---|",
        f"| **门禁结论 (`gate_passed`)** | **{verdict_str}** |",
        f"| **编译器 Git 提交 (`commit`)** | `{comp['commit']}` |",
        f"| **编译器二进制 SHA-256** | `{comp['sha256']}` |",
        f"| **宿主平台 (`system` / `machine`)** | `{host['system']} / {host['machine']}` |",
        f"| **Probe 能力通过 (`probe_capability_passed`)** | **{summary['probe_capability_passed']} / {summary['total_probes']}** |",
        f"| **V1 验收程序通过 (`program_run_passed`)** | **{summary['program_run_passed']} / {summary['total_programs']}** |",
        f"| **P2 基础 A 组可运行 (`foundational_p2`)** | `{fp2.get('group_a_runnable_passed', 0)} / {fp2.get('group_a_evaluated', 0)}` (目标 `≥ {fp2.get('p2_target_min', 70)}`) |",
        "",
        "## 1. 比较器状态分布 (`counts`)",
        "",
        "| 状态 (`status`) | 数量 |",
        "|---|---:|",
    ]
    for k, v in sorted(summary["counts"].items()):
        lines.append(f"| `{k}` | {v} |")

    by_batch = summary.get("by_batch", {})
    if by_batch:
        lines.extend(
            [
                "",
                "## 2. 按 Probe 批次执行汇总 (`by_batch`)",
                "",
                "| 批次 (`probe_batch`) | 总数 | `pass_run` | `pass_diagnostic` | `pass_abort` | `known_unimplemented` | `capability_passed` |",
                "|---|---:|---:|---:|---:|---:|---:|",
            ]
        )
        for b_key, b_row in by_batch.items():
            lines.append(
                f"| `{b_key}` | {b_row['total']} | {b_row['pass_run']} | {b_row['pass_diagnostic']} | "
                f"{b_row['pass_abort']} | {b_row['known_unimplemented']} | {b_row['capability_passed']} / {b_row['total']} |"
            )

    by_chapter = summary.get("by_chapter", {})
    if by_chapter:
        lines.extend(
            [
                "",
                "## 3. 按 Guide 章节执行汇总 (`by_chapter`)",
                "",
                "| Guide 章节 | 总数 | `pass_run` | `pass_diagnostic` | `pass_abort` | `known_unimplemented` | `capability_passed` |",
                "|---|---:|---:|---:|---:|---:|---:|",
            ]
        )
        for ch_key, ch_row in by_chapter.items():
            lines.append(
                f"| `{ch_key}` | {ch_row['total']} | {ch_row['pass_run']} | {ch_row['pass_diagnostic']} | "
                f"{ch_row['pass_abort']} | {ch_row['known_unimplemented']} | {ch_row['capability_passed']} / {ch_row['total']} |"
            )

    if summary.get("unexpected_passes"):
        lines.extend(
            [
                "",
                "## 4. 新晋通过待晋升条目 (`unexpected_passes`)",
                "",
            ]
        )
        for up_name in summary["unexpected_passes"]:
            lines.append(
                f"- `{up_name}`：已在单文件与工程双模式下退出码 0 且精确匹配 `target_stdout`，请更新 `V046_PROBE_OVERRIDES` 并运行 `--init-expectations`。"
            )

    if summary.get("failures"):
        lines.extend(
            [
                "",
                "## 5. 阻断项明细 (`failures`)",
                "",
            ]
        )
        for fail_msg in summary["failures"]:
            lines.append(f"- `{fail_msg}`")

    lines.append("")
    return "\n".join(lines)


def verify_evidence_bundle(
    evidence_dir: Path,
    expectations_path: Path,
    corpus_root: Path | None = None,
    baseline_path: Path | None = None,
    strict_unreviewed: bool = True,
) -> dict[str, Any]:
    """Verify an offline evidence bundle's raw stdout/stderr/status files, matrix.tsv, and results.json against expectations."""
    evidence_dir = evidence_dir.resolve(strict=True)
    results_path = evidence_dir / "results.json"
    matrix_path = evidence_dir / "matrix.tsv"
    raw_dir = evidence_dir / "raw"
    if not results_path.is_file():
        raise ValueError(f"missing results.json in evidence bundle: {evidence_dir}")
    if not matrix_path.is_file():
        raise ValueError(f"missing matrix.tsv in evidence bundle: {evidence_dir}")
    if not raw_dir.is_dir():
        raise ValueError(f"missing raw/ directory in evidence bundle: {evidence_dir}")

    ledger = json.loads(results_path.read_text(encoding="utf-8"))
    if not isinstance(ledger, dict) or ledger.get("schema") != SCHEMA_RESULTS:
        raise ValueError(f"invalid results.json schema in {results_path}")

    compiler = ledger.get("compiler")
    if (
        not isinstance(compiler, dict)
        or not isinstance(compiler.get("sha256"), str)
        or not re.fullmatch(r"[0-9a-f]{64}", compiler["sha256"])
        or not isinstance(compiler.get("commit"), str)
        or not compiler["commit"].strip()
    ):
        raise ValueError("invalid compiler metadata in results.json")

    host = ledger.get("host")
    if (
        not isinstance(host, dict)
        or not isinstance(host.get("system"), str)
        or not host["system"].strip()
        or not isinstance(host.get("machine"), str)
        or not host["machine"].strip()
    ):
        raise ValueError("invalid host metadata in results.json")

    expectations = load_expectations(
        expectations_path,
        corpus_root=corpus_root.resolve(strict=True) if corpus_root is not None else None,
    )
    baseline = (
        parse_tsv_matrix(
            baseline_path.read_text(encoding="utf-8"),
            expected_names=set(expectations["probes"].keys()),
        )
        if baseline_path is not None
        else {}
    )

    def _verify_entry(
        entry: dict[str, Any],
        section: str,
        raw_prefix_name: str,
        exp_spec: dict[str, Any],
        base_row: tuple[str, str] | None,
    ) -> dict[str, Any]:
        if not isinstance(entry, dict) or "execution" not in entry or "comparison" not in entry:
            raise ValueError(f"malformed entry in results.json {section}")
        exec_rec = dict(entry["execution"])
        comp_rec = entry["comparison"]
        name = exec_rec.get("name")
        if exec_rec.get("source_sha256") != exp_spec["source_sha256"]:
            raise ValueError(
                f"source_sha256 mismatch in results.json for {section}.{name}: "
                f"recorded {exec_rec.get('source_sha256')}, expected {exp_spec['source_sha256']}"
            )
        pt = exec_rec.get("project_transform")
        if (
            not isinstance(pt, dict)
            or pt.get("kind") != "prepend_package_header"
            or pt.get("header_sha256") != sha256_bytes(PROJECT_PACKAGE_HEADER)
            or pt.get("project_toml_sha256") != sha256_bytes(PROJECT_TOML_BYTES)
            or pt.get("original_source_sha256") != exp_spec["source_sha256"]
        ):
            raise ValueError(f"invalid project_transform metadata for {section}.{name}")

        reconstructed_exec = dict(exec_rec)
        for mode in ("single", "project"):
            m_rec = exec_rec.get(mode)
            if not isinstance(m_rec, dict):
                raise ValueError(f"missing {mode} execution record for {section}.{name}")
            out_p = raw_dir / f"{raw_prefix_name}.{mode}.stdout"
            err_p = raw_dir / f"{raw_prefix_name}.{mode}.stderr"
            st_p = raw_dir / f"{raw_prefix_name}.{mode}.status.json"
            if not out_p.is_file() or not err_p.is_file() or not st_p.is_file():
                raise ValueError(f"missing raw artifact files for {section}.{name}.{mode}")
            out_bytes = out_p.read_bytes()
            err_bytes = err_p.read_bytes()
            st_data = json.loads(st_p.read_text(encoding="utf-8"))
            if st_data.get("exit") != m_rec.get("exit") or st_data.get("timed_out") != m_rec.get("timed_out"):
                raise ValueError(f"status.json mismatch for {section}.{name}.{mode}")
            if (
                len(out_bytes) != m_rec.get("stdout_len")
                or sha256_bytes(out_bytes) != m_rec.get("stdout_sha256")
                or base64.b64encode(out_bytes).decode("ascii") != m_rec.get("stdout_b64")
            ):
                raise ValueError(f"raw stdout mismatch for {section}.{name}.{mode}")
            if (
                len(err_bytes) != m_rec.get("stderr_len")
                or sha256_bytes(err_bytes) != m_rec.get("stderr_sha256")
                or base64.b64encode(err_bytes).decode("ascii") != m_rec.get("stderr_b64")
            ):
                raise ValueError(f"raw stderr mismatch for {section}.{name}.{mode}")
            recomputed_marker = classify_raw(
                m_rec["exit"], bool(m_rec["timed_out"]), out_bytes, err_bytes
            )
            if recomputed_marker != m_rec.get("marker"):
                raise ValueError(
                    f"marker mismatch for {section}.{name}.{mode}: recorded {m_rec.get('marker')!r}, recomputed {recomputed_marker!r}"
                )
            m_with_bytes = dict(m_rec)
            m_with_bytes["stdout_bytes"] = out_bytes
            m_with_bytes["stderr_bytes"] = err_bytes
            reconstructed_exec[mode] = m_with_bytes

        recomputed_comp = compare_item(reconstructed_exec, exp_spec, baseline_row=base_row)
        if recomputed_comp != comp_rec:
            raise ValueError(
                f"comparison mismatch for {section}.{name}: recorded {comp_rec}, recomputed {recomputed_comp}"
            )
        return recomputed_comp

    probe_entries = ledger.get("probes")
    program_entries = ledger.get("programs")
    if not isinstance(probe_entries, list) or not isinstance(program_entries, list):
        raise ValueError("results.json probes and programs must be lists")

    recomputed_probe_comps: list[dict[str, Any]] = []
    seen_probes: set[str] = set()
    for entry in probe_entries:
        name = entry.get("execution", {}).get("name")
        if not isinstance(name, str) or name not in expectations["probes"]:
            raise ValueError(f"unknown probe {name!r} in results.json")
        if name in seen_probes:
            raise ValueError(f"duplicate probe {name!r} in results.json")
        seen_probes.add(name)
        recomputed_probe_comps.append(
            _verify_entry(entry, "probes", name, expectations["probes"][name], baseline.get(name))
        )

    recomputed_prog_comps: list[dict[str, Any]] = []
    seen_progs: set[str] = set()
    for entry in program_entries:
        name = entry.get("execution", {}).get("name")
        if not isinstance(name, str) or name not in expectations["programs"]:
            raise ValueError(f"unknown program {name!r} in results.json")
        if name in seen_progs:
            raise ValueError(f"duplicate program {name!r} in results.json")
        seen_progs.add(name)
        recomputed_prog_comps.append(
            _verify_entry(entry, "programs", f"program_{name}", expectations["programs"][name], None)
        )

    matrix_text = matrix_path.read_text(encoding="utf-8")
    if seen_probes:
        matrix_rows = parse_tsv_matrix(matrix_text, expected_names=seen_probes)
        for comp in recomputed_probe_comps:
            if matrix_rows.get(comp["name"]) != (comp["single_marker"], comp["project_marker"]):
                raise ValueError(f"matrix.tsv row mismatch for probe {comp['name']}")
    elif matrix_text.strip():
        raise ValueError("matrix.tsv must be empty when no probes were evaluated")

    recomputed_summary = summarize_comparisons(
        recomputed_probe_comps,
        recomputed_prog_comps,
        strict_unreviewed=strict_unreviewed,
    )
    if recomputed_summary != ledger.get("summary"):
        raise ValueError("summary mismatch in results.json")

    summary_md_path = evidence_dir / "SUMMARY.md"
    if summary_md_path.is_file():
        expected_summary_md = render_evidence_summary_markdown(ledger)
        if summary_md_path.read_text(encoding="utf-8") != expected_summary_md:
            raise ValueError("SUMMARY.md mismatch in evidence bundle")

    return ledger


def compare_dual_host_evidence(
    evidence_dir_a: Path,
    evidence_dir_b: Path,
    expectations_path: Path,
    corpus_root: Path | None = None,
    baseline_path: Path | None = None,
    strict_unreviewed: bool = True,
    require_same_commit: bool = True,
    require_distinct_systems: bool = False,
) -> dict[str, Any]:
    """Verify two evidence bundles (e.g. macOS and Linux CI runs) and assert cross-host parity across all probes and programs."""
    ledger_a = verify_evidence_bundle(
        evidence_dir=evidence_dir_a,
        expectations_path=expectations_path,
        corpus_root=corpus_root,
        baseline_path=baseline_path,
        strict_unreviewed=strict_unreviewed,
    )
    ledger_b = verify_evidence_bundle(
        evidence_dir=evidence_dir_b,
        expectations_path=expectations_path,
        corpus_root=corpus_root,
        baseline_path=baseline_path,
        strict_unreviewed=strict_unreviewed,
    )
    commit_a = ledger_a["compiler"]["commit"]
    commit_b = ledger_b["compiler"]["commit"]
    if require_same_commit and commit_a != commit_b:
        raise ValueError(
            f"dual-host commit mismatch: {commit_a!r} vs {commit_b!r}"
        )
    host_a = ledger_a["host"]
    host_b = ledger_b["host"]
    if require_distinct_systems and host_a["system"] == host_b["system"]:
        raise ValueError(
            f"dual-host comparison requires distinct host systems, both were {host_a['system']!r}"
        )

    for section in ("probes", "programs"):
        map_a = {e["execution"]["name"]: e for e in ledger_a[section]}
        map_b = {e["execution"]["name"]: e for e in ledger_b[section]}
        if set(map_a.keys()) != set(map_b.keys()):
            raise ValueError(
                f"dual-host {section} inventory mismatch: "
                f"only_in_a={sorted(set(map_a) - set(map_b))}, "
                f"only_in_b={sorted(set(map_b) - set(map_a))}"
            )
        for name in sorted(map_a.keys()):
            comp_a = map_a[name]["comparison"]
            comp_b = map_b[name]["comparison"]
            if comp_a != comp_b:
                raise ValueError(
                    f"dual-host comparison mismatch on {section}.{name}: "
                    f"{comp_a['status']} ({comp_a['single_marker']}/{comp_a['project_marker']}) vs "
                    f"{comp_b['status']} ({comp_b['single_marker']}/{comp_b['project_marker']})"
                )
            if comp_a["status"] == "pass_run":
                for mode in ("single", "project"):
                    sha_a = map_a[name]["execution"][mode]["stdout_sha256"]
                    sha_b = map_b[name]["execution"][mode]["stdout_sha256"]
                    if sha_a != sha_b:
                        raise ValueError(
                            f"dual-host stdout_sha256 mismatch on {section}.{name}.{mode}: {sha_a} vs {sha_b}"
                        )

    if ledger_a["summary"] != ledger_b["summary"]:
        raise ValueError("dual-host summary mismatch between evidence bundles")

    return {
        "parity_passed": True,
        "gate_passed": bool(ledger_a["summary"]["gate_passed"]) and bool(ledger_b["summary"]["gate_passed"]),
        "commit": commit_a,
        "host_a": host_a,
        "host_b": host_b,
        "total_probes": ledger_a["summary"]["total_probes"],
        "probe_capability_passed": ledger_a["summary"]["probe_capability_passed"],
        "total_programs": ledger_a["summary"]["total_programs"],
        "program_run_passed": ledger_a["summary"]["program_run_passed"],
    }


def verify_matrix_evidence_dir(
    evidence_dir: Path,
    expected_names: set[str] | None = None,
) -> dict[str, Any]:
    """Verify a TSV-mode conformance_matrix.sh evidence directory (manifest.json, matrix.tsv, and raw files)."""
    evidence_dir = evidence_dir.resolve(strict=True)
    manifest_path = evidence_dir / "manifest.json"
    matrix_path = evidence_dir / "matrix.tsv"
    raw_dir = evidence_dir / "raw"
    if not manifest_path.is_file():
        raise ValueError(f"missing manifest.json in matrix evidence dir: {evidence_dir}")
    if not matrix_path.is_file():
        raise ValueError(f"missing matrix.tsv in matrix evidence dir: {evidence_dir}")
    if not raw_dir.is_dir():
        raise ValueError(f"missing raw/ directory in matrix evidence dir: {evidence_dir}")

    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    if (
        not isinstance(manifest, dict)
        or manifest.get("schema") != "koven.conformance-matrix-manifest.v1"
        or manifest.get("matrix_file") != "matrix.tsv"
        or manifest.get("raw_dir") != "raw"
    ):
        raise ValueError(f"invalid manifest.json schema in {manifest_path}")

    rows = parse_tsv_matrix(matrix_path.read_text(encoding="utf-8"), expected_names=expected_names)
    if manifest.get("probe_count") != len(rows):
        raise ValueError(
            f"manifest probe_count mismatch: recorded {manifest.get('probe_count')}, matrix has {len(rows)}"
        )
    for name, (single_m, project_m) in sorted(rows.items()):
        for mode, expected_m in (("single", single_m), ("project", project_m)):
            out_p = raw_dir / f"{name}.{mode}.stdout"
            err_p = raw_dir / f"{name}.{mode}.stderr"
            st_p = raw_dir / f"{name}.{mode}.status.json"
            if not out_p.is_file() or not err_p.is_file() or not st_p.is_file():
                raise ValueError(f"missing raw file for {name}.{mode} in {raw_dir}")
            st_data = json.loads(st_p.read_text(encoding="utf-8"))
            recomputed = classify_raw(
                st_data.get("exit"),
                bool(st_data.get("timed_out")),
                out_p.read_bytes(),
                err_p.read_bytes(),
            )
            if recomputed != expected_m:
                raise ValueError(
                    f"matrix evidence marker mismatch for {name}.{mode}: matrix has {expected_m!r}, raw files classify as {recomputed!r}"
                )
    return {"manifest": manifest, "rows": rows}


def compare_tsv_matrix_against_expectations(
    matrix_path: Path,
    expectations_path: Path,
    corpus_root: Path | None = None,
    baseline_path: Path | None = None,
) -> dict[str, Any]:
    """Compare a 3-column TSV matrix against expectations.json and optional baseline TSV without requiring raw stdout files."""
    matrix_path = matrix_path.resolve(strict=True)
    expectations = load_expectations(
        expectations_path,
        corpus_root=corpus_root.resolve(strict=True) if corpus_root is not None else None,
    )
    baseline = (
        parse_tsv_matrix(
            baseline_path.read_text(encoding="utf-8"),
            expected_names=set(expectations["probes"].keys()),
        )
        if baseline_path is not None
        else {}
    )
    rows = parse_tsv_matrix(matrix_path.read_text(encoding="utf-8"))
    unknown = sorted(set(rows.keys()) - set(expectations["probes"].keys()))
    if unknown:
        raise ValueError(f"unknown probe names in TSV matrix: {unknown}")

    counts: dict[str, int] = {
        "pass_run": 0,
        "pass_diagnostic": 0,
        "pass_abort": 0,
        "known_unimplemented": 0,
        "mode_mismatch": 0,
        "regression": 0,
        "internal_error": 0,
        "unexpected_pass": 0,
    }
    failures: list[str] = []
    unexpected_passes: list[str] = []
    cap_passed = 0

    for name, (single_m, project_m) in sorted(rows.items()):
        spec = expectations["probes"][name]
        kind = spec["kind"]
        if is_internal_error_marker(single_m) or is_internal_error_marker(project_m):
            st = "internal_error"
            failures.append(f"{name} (internal_error): single={single_m}, project={project_m}")
        elif single_m != project_m:
            st = "mode_mismatch"
            failures.append(f"{name} (mode_mismatch): single={single_m} vs project={project_m}")
        elif kind == "run_ok":
            if single_m == "OK":
                st = "pass_run"
                cap_passed += 1
            else:
                st = "regression"
                failures.append(f"{name} (regression): expected OK, got {single_m}")
        elif kind == "compile_fail":
            exp_m = f"FE:{spec['diagnostic']}"
            if single_m == exp_m:
                st = "pass_diagnostic"
                cap_passed += 1
            else:
                st = "regression"
                failures.append(f"{name} (regression): expected {exp_m}, got {single_m}")
        elif kind == "runtime_abort":
            exp_m = f"abort:{spec['abort_message']}"
            if single_m == exp_m:
                st = "pass_abort"
                cap_passed += 1
            else:
                st = "regression"
                failures.append(f"{name} (regression): expected {exp_m}, got {single_m}")
        elif kind == "known_unimplemented":
            if single_m == "OK":
                st = "unexpected_pass"
                unexpected_passes.append(name)
                failures.append(f"{name} (unexpected_pass): now OK in both modes; promote to run_ok")
            elif single_m in spec.get("allowed_markers", []):
                st = "known_unimplemented"
            else:
                st = "regression"
                failures.append(
                    f"{name} (regression): expected one of {spec.get('allowed_markers')}, got {single_m}"
                )
        else:
            st = "regression"
            failures.append(f"{name} (unreviewed): {single_m}")

        if (
            name in baseline
            and not spec.get("v046_baseline_exempt", False)
            and baseline[name][0] == "OK"
            and (single_m != "OK" or project_m != "OK")
            and st != "regression"
        ):
            st = "regression"
            failures.append(f"{name} (baseline regression): was OK, now {single_m}/{project_m}")

        counts[st] = counts.get(st, 0) + 1

    return {
        "gate_passed": len(failures) == 0,
        "total_probes": len(rows),
        "probe_capability_passed": cap_passed,
        "counts": counts,
        "unexpected_passes": unexpected_passes,
        "failures": failures,
    }


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
    unexpected_passes: list[str] = []
    for item in probe_comparisons + program_comparisons:
        st = item["status"]
        counts[st] = counts.get(st, 0) + 1
        if st == "unexpected_pass":
            unexpected_passes.append(item["name"])
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

    by_batch: dict[str, dict[str, int]] = {}
    for item in probe_comparisons:
        b_key = item.get("probe_batch") or classify_probe_batch(item["name"])
        b_bucket = by_batch.setdefault(
            b_key,
            {
                "total": 0,
                "pass_run": 0,
                "pass_diagnostic": 0,
                "pass_abort": 0,
                "known_unimplemented": 0,
                "capability_passed": 0,
            },
        )
        b_bucket["total"] += 1
        st = item["status"]
        if st in ("pass_run", "pass_diagnostic", "pass_abort", "known_unimplemented"):
            b_bucket[st] += 1
        if item["capability_passed"]:
            b_bucket["capability_passed"] += 1

    ordered_by_batch = {
        k: by_batch[k] for k in PROBE_BATCH_CATALOG if k in by_batch
    }
    for k in sorted(by_batch.keys()):
        if k not in ordered_by_batch:
            ordered_by_batch[k] = by_batch[k]

    p2_a_eval = p2_a_runnable = p2_b_eval = p2_b_diag = p2_c_eval = 0
    for item in probe_comparisons:
        name = item["name"]
        b_key = item.get("probe_batch") or classify_probe_batch(name)
        if b_key not in FOUNDATIONAL_BATCHES:
            continue
        if name in FOUNDATIONAL_GROUP_B_PROBES:
            p2_b_eval += 1
            if item["status"] == "pass_diagnostic":
                p2_b_diag += 1
        elif name in FOUNDATIONAL_GROUP_C_PROBES:
            p2_c_eval += 1
        else:
            p2_a_eval += 1
            if item["status"] in ("pass_run", "pass_abort"):
                p2_a_runnable += 1

    foundational_p2 = {
        "group_a_evaluated": p2_a_eval,
        "group_a_runnable_passed": p2_a_runnable,
        "group_b_evaluated": p2_b_eval,
        "group_b_diagnostic_passed": p2_b_diag,
        "group_c_evaluated": p2_c_eval,
        "p2_target_min": P2_GROUP_A_TARGET_MIN,
        "p2_gate_met": (
            p2_a_eval == 74
            and p2_a_runnable >= P2_GROUP_A_TARGET_MIN
            and p2_b_diag == len(FOUNDATIONAL_GROUP_B_PROBES)
        ),
    }

    return {
        "total_probes": len(probe_comparisons),
        "probe_capability_passed": probe_cap_passed,
        "total_programs": len(program_comparisons),
        "program_run_passed": program_run_passed,
        "counts": counts,
        "by_chapter": dict(sorted(by_chapter.items())),
        "by_batch": ordered_by_batch,
        "foundational_p2": foundational_p2,
        "unexpected_passes": unexpected_passes,
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


def summarize_baseline_evolution(expectations: dict[str, Any]) -> dict[str, list[str]]:
    """Classify all probes by their transition from historical baseline_marker to current expectation kind."""
    buckets: dict[str, list[str]] = {
        "baseline_ok_preserved": [],
        "promoted_to_run_ok": [],
        "upgraded_to_structured_abort": [],
        "upgraded_to_compile_fail": [],
        "baseline_compile_fail_preserved": [],
        "remaining_known_unimplemented": [],
        "regressed_from_baseline_ok": [],
    }
    for name, spec in sorted(expectations["probes"].items()):
        bm = spec.get("baseline_marker", "")
        kind = spec["kind"]
        if bm == "OK" and kind == "run_ok":
            buckets["baseline_ok_preserved"].append(name)
        elif bm == "OK" and kind != "run_ok":
            buckets["regressed_from_baseline_ok"].append(name)
        elif bm != "OK" and kind == "run_ok":
            buckets["promoted_to_run_ok"].append(name)
        elif kind == "runtime_abort":
            buckets["upgraded_to_structured_abort"].append(name)
        elif kind == "compile_fail" and spec.get("v046_baseline_exempt"):
            buckets["upgraded_to_compile_fail"].append(name)
        elif kind == "compile_fail":
            buckets["baseline_compile_fail_preserved"].append(name)
        elif kind == "known_unimplemented":
            buckets["remaining_known_unimplemented"].append(name)
    return buckets


def resolve_spec_doc_relpath(spec_id: str, repo_root: Path) -> tuple[str, str] | None:
    """Locate SPEC-xxxx in docs/specs/active or docs/archive/specs and return (relpath, status)."""
    if not re.fullmatch(r"SPEC-[0-9]{4}", spec_id):
        return None
    num = spec_id.split("-", 1)[1]
    active_dir = repo_root / "docs" / "specs" / "active"
    archive_dir = repo_root / "docs" / "archive" / "specs"
    if active_dir.is_dir():
        matches = sorted(active_dir.glob(f"{num}-*.md"))
        if matches:
            return f"docs/specs/active/{matches[0].name}", "active"
    if archive_dir.is_dir():
        matches = sorted(archive_dir.glob(f"{num}-*.md"))
        if matches:
            return f"docs/archive/specs/{matches[0].name}", "archived"
    return None


def summarize_v046_spec_traceability(
    expectations: dict[str, Any], repo_root: Path | None = None
) -> dict[str, dict[str, Any]]:
    """Aggregate all v046_baseline_exempt probes by their governing SPEC-xxxx contract IDs."""
    root = repo_root or Path(__file__).resolve().parents[1]
    by_spec: dict[str, dict[str, Any]] = {}
    for name, spec in sorted(expectations["probes"].items()):
        if not spec.get("v046_baseline_exempt"):
            continue
        for sref in spec.get("spec_refs", []):
            resolved = resolve_spec_doc_relpath(sref, root)
            relpath, status = resolved if resolved is not None else ("", "unknown")
            bucket = by_spec.setdefault(
                sref,
                {
                    "spec_id": sref,
                    "doc_relpath": relpath,
                    "status": status,
                    "probes": [],
                    "kinds": {},
                },
            )
            bucket["probes"].append(name)
            k = spec["kind"]
            bucket["kinds"][k] = bucket["kinds"].get(k, 0) + 1
    return dict(sorted(by_spec.items()))


def summarize_oracle_origins(expectations: dict[str, Any]) -> dict[str, dict[str, Any]]:
    """Aggregate probes and programs by their canonical oracle_origin classification."""
    order = (
        "single_println_literal",
        "source_trace_audited",
        "v046_spec_contract",
        "baseline_diagnostic_audited",
        "baseline_unimplemented_audited",
        "expected_txt_file",
    )
    summary: dict[str, dict[str, Any]] = {
        origin: {
            "origin": origin,
            "description": ORACLE_ORIGIN_DESCRIPTIONS[origin],
            "probes": 0,
            "programs": 0,
            "kinds": {},
        }
        for origin in order
    }
    for section, key in (("probes", "probes"), ("programs", "programs")):
        for spec in expectations[section].values():
            origin = spec.get("oracle_origin")
            if origin not in summary:
                continue
            bucket = summary[origin]
            bucket[key] += 1
            k = spec["kind"]
            bucket["kinds"][k] = bucket["kinds"].get(k, 0) + 1
    return summary


def summarize_output_shape_distribution(expectations: dict[str, Any]) -> dict[str, Any]:
    """Classify all 154 probes and 6 programs by their stdout line/byte shape."""
    single_line_probes: list[str] = []
    multi_line_probes: list[dict[str, Any]] = []
    zero_stdout_probes: list[str] = []
    total_probe_lines = 0
    total_probe_bytes = 0

    for name, spec in sorted(expectations["probes"].items()):
        out_text = spec.get("stdout") or spec.get("target_stdout")
        if isinstance(out_text, str) and out_text:
            ln_cnt = spec.get("expected_lines", len(out_text.splitlines()))
            by_cnt = spec.get("expected_bytes", len(out_text.encode("utf-8")))
            total_probe_lines += ln_cnt
            total_probe_bytes += by_cnt
            if ln_cnt == 1:
                single_line_probes.append(name)
            else:
                multi_line_probes.append(
                    {
                        "name": name,
                        "kind": spec["kind"],
                        "lines": ln_cnt,
                        "bytes": by_cnt,
                    }
                )
        else:
            zero_stdout_probes.append(name)

    total_prog_lines = sum(
        spec.get("expected_lines", 0) for spec in expectations["programs"].values()
    )
    total_prog_bytes = sum(
        spec.get("expected_bytes", 0) for spec in expectations["programs"].values()
    )
    return {
        "single_line_probes": single_line_probes,
        "multi_line_probes": multi_line_probes,
        "zero_stdout_probes": zero_stdout_probes,
        "total_probe_lines": total_probe_lines,
        "total_probe_bytes": total_probe_bytes,
        "total_program_lines": total_prog_lines,
        "total_program_bytes": total_prog_bytes,
    }


def summarize_probe_batches(expectations: dict[str, Any]) -> dict[str, dict[str, Any]]:
    """Aggregate all probes by their canonical single-letter batch prefix (p..e)."""
    summary: dict[str, dict[str, Any]] = {
        batch: {
            "batch": batch,
            "description": desc,
            "total": 0,
            "run_ok": 0,
            "compile_fail": 0,
            "runtime_abort": 0,
            "known_unimplemented": 0,
            "unreviewed": 0,
            "cap_passed": 0,
            "batch_status": "empty",
            "probes": [],
        }
        for batch, desc in PROBE_BATCH_CATALOG.items()
    }
    for name, spec in sorted(expectations["probes"].items()):
        batch = spec.get("probe_batch") or classify_probe_batch(name)
        row = summary[batch]
        row["total"] += 1
        row["probes"].append(name)
        kind = spec["kind"]
        row[kind] = row.get(kind, 0) + 1
        if kind in ("run_ok", "compile_fail", "runtime_abort"):
            row["cap_passed"] += 1

    for row in summary.values():
        if row["total"] > 0 and row["cap_passed"] == row["total"]:
            row["batch_status"] = "all_passed"
        elif row["total"] > 0:
            row["batch_status"] = "partial"
    return summary


def summarize_unimplemented_markers(expectations: dict[str, Any]) -> dict[str, dict[str, Any]]:
    """Aggregate known_unimplemented probes and programs by allowed unimplemented marker."""
    summary: dict[str, dict[str, Any]] = {
        marker: {
            "marker": marker,
            "stage": stage,
            "description": desc,
            "probes": [],
            "programs": [],
            "baseline_count": 0,
        }
        for marker, (stage, desc) in UNIMPLEMENTED_MARKER_CATALOG.items()
    }
    for name, spec in sorted(expectations["probes"].items()):
        if spec["kind"] != "known_unimplemented":
            continue
        bm = spec.get("baseline_marker")
        if bm in summary:
            summary[bm]["baseline_count"] += 1
        for m in spec.get("allowed_markers", []):
            if m in summary:
                summary[m]["probes"].append(name)
    for name, spec in sorted(expectations["programs"].items()):
        if spec["kind"] != "known_unimplemented":
            continue
        for m in spec.get("allowed_markers", []):
            if m in summary:
                summary[m]["programs"].append(name)
    return summary


def summarize_program_roadmap_blockers(
    expectations: dict[str, Any],
    roadmap_items: dict[str, dict[str, Any]] | None = None,
) -> dict[str, dict[str, Any]]:
    """Aggregate all known_unimplemented acceptance programs by their roadmap_blockers."""
    if roadmap_items is None:
        v1_json_path = Path(__file__).resolve().parents[1] / "docs" / "roadmap" / "v1.json"
        if v1_json_path.is_file():
            v1_data = json.loads(v1_json_path.read_text(encoding="utf-8"))
            roadmap_items = {it["id"]: it for it in v1_data.get("items", [])}
        else:
            roadmap_items = {}

    by_blocker: dict[str, dict[str, Any]] = {}
    for prog_name, spec in sorted(expectations["programs"].items()):
        if spec["kind"] != "known_unimplemented":
            continue
        for b_id in spec.get("roadmap_blockers", []):
            item = roadmap_items.get(b_id, {})
            bucket = by_blocker.setdefault(
                b_id,
                {
                    "id": b_id,
                    "title": item.get("title", "").replace("|", "\\|"),
                    "status": item.get("status", "unknown"),
                    "programs": [],
                    "probes": [],
                },
            )
            bucket["programs"].append(prog_name)

    for probe_name, spec in sorted(expectations["probes"].items()):
        if spec["kind"] != "known_unimplemented":
            continue
        for b_id in spec.get("roadmap_blockers", []):
            if b_id in by_blocker:
                by_blocker[b_id]["probes"].append(probe_name)

    return dict(sorted(by_blocker.items()))


def summarize_foundational_cohort(expectations: dict[str, Any]) -> dict[str, Any]:
    """Classify the 85 foundational p/q/r/s probes into Groups A (74), B (8), and C (3) for P2 gate tracking."""
    groups: dict[str, dict[str, Any]] = {
        "A": {
            "label": "A 组：合法基础程序（P2 门槛基数）",
            "total": 0,
            "runnable": 0,
            "run_ok": [],
            "runtime_abort": [],
            "compile_fail": [],
            "known_unimplemented": [],
        },
        "B": {
            "label": "B 组：应拒基础探针（静态/借用/类型拒绝）",
            "total": 0,
            "runnable": 0,
            "run_ok": [],
            "runtime_abort": [],
            "compile_fail": [],
            "known_unimplemented": [],
        },
        "C": {
            "label": "C 组：v0.44 待裁决项（已在 v0.46 规范定案）",
            "total": 0,
            "runnable": 0,
            "run_ok": [],
            "runtime_abort": [],
            "compile_fail": [],
            "known_unimplemented": [],
        },
    }
    for name, spec in sorted(expectations["probes"].items()):
        batch = spec.get("probe_batch") or classify_probe_batch(name)
        if batch not in FOUNDATIONAL_BATCHES:
            continue
        if name in FOUNDATIONAL_GROUP_B_PROBES:
            grp = groups["B"]
        elif name in FOUNDATIONAL_GROUP_C_PROBES:
            grp = groups["C"]
        else:
            grp = groups["A"]
        grp["total"] += 1
        kind = spec["kind"]
        grp[kind].append(name)
        if kind in ("run_ok", "runtime_abort"):
            grp["runnable"] += 1

    group_a = groups["A"]
    return {
        "total_foundational": sum(g["total"] for g in groups.values()),
        "p2_target_min": P2_GROUP_A_TARGET_MIN,
        "p2_gap_to_target": max(0, P2_GROUP_A_TARGET_MIN - group_a["runnable"]),
        "p2_gate_met": group_a["runnable"] >= P2_GROUP_A_TARGET_MIN,
        "groups": groups,
    }


def summarize_v2_chapter_readiness(expectations: dict[str, Any]) -> dict[str, dict[str, Any]]:
    """Evaluate each of the 13 Guide chapters against V2 exit criteria (positive + negative polarity and 0 known_unimplemented)."""
    chapter_polarity = summarize_chapter_polarity(expectations)
    readiness: dict[str, dict[str, Any]] = {}
    for ch in VALID_GUIDE_CHAPTERS:
        pol = chapter_polarity[ch]
        neg_contracts: list[str] = []
        unimpl_probes: list[str] = []
        blockers: set[str] = set()
        programs_in_ch: list[str] = []

        for name, spec in sorted(expectations["probes"].items()):
            if spec.get("guide_chapter") != ch:
                continue
            if spec["kind"] == "compile_fail":
                for code in spec.get("all_diagnostics", [spec["diagnostic"]]):
                    tag = f"FE:{code}"
                    if tag not in neg_contracts:
                        neg_contracts.append(tag)
            elif spec["kind"] == "runtime_abort":
                tag = f"abort:{spec['abort_message']}"
                if tag not in neg_contracts:
                    neg_contracts.append(tag)
            elif spec["kind"] == "known_unimplemented":
                unimpl_probes.append(name)
                blockers.update(spec.get("roadmap_blockers", []))

        for prog_name, prog_spec in sorted(expectations["programs"].items()):
            if prog_spec.get("guide_chapter") == ch:
                programs_in_ch.append(prog_name)

        has_pos = pol["has_positive"]
        has_neg = pol["has_negative"]
        ku_count = pol["known_unimplemented"]
        if has_pos and has_neg and ku_count == 0:
            v2_status = "v2_probe_ready"
        elif has_pos and has_neg and ku_count > 0:
            v2_status = "blocked_by_roadmap"
        elif not has_neg and ku_count > 0:
            v2_status = "needs_negative_and_roadmap"
        else:
            v2_status = "needs_negative_only"

        readiness[ch] = {
            "chapter": ch,
            "domain": GUIDE_CHAPTER_DOMAINS[ch],
            "total_probes": pol["total"],
            "positive_active": pol["positive_active"],
            "negative_active": pol["negative_active"],
            "known_unimplemented": ku_count,
            "negative_contracts": neg_contracts,
            "unimplemented_probes": unimpl_probes,
            "roadmap_blockers": sorted(blockers),
            "acceptance_programs": programs_in_ch,
            "v2_status": v2_status,
        }
    return readiness


def summarize_blocker_unlock_impact(
    expectations: dict[str, Any],
    roadmap_items: dict[str, dict[str, Any]] | None = None,
) -> dict[str, dict[str, Any]]:
    """Analyze sole-blocker immediate probe unlocks vs co-blocked items and P2 Group A gate contribution per roadmap item."""
    if roadmap_items is None:
        v1_json_path = Path(__file__).resolve().parents[1] / "docs" / "roadmap" / "v1.json"
        if v1_json_path.is_file():
            v1_data = json.loads(v1_json_path.read_text(encoding="utf-8"))
            roadmap_items = {it["id"]: it for it in v1_data.get("items", [])}
        else:
            roadmap_items = {}

    impact: dict[str, dict[str, Any]] = {}

    def _ensure_bucket(b_id: str) -> dict[str, Any]:
        if b_id not in impact:
            item = roadmap_items.get(b_id, {})
            impact[b_id] = {
                "id": b_id,
                "stage": item.get("stage", "-"),
                "status": item.get("status", "unknown"),
                "title": item.get("title", "").replace("|", "\\|"),
                "sole_probes": [],
                "co_probes": [],
                "p2_group_a_probes": [],
                "programs": [],
            }
        return impact[b_id]

    for name, spec in sorted(expectations["probes"].items()):
        if spec["kind"] != "known_unimplemented":
            continue
        blockers = spec.get("roadmap_blockers", [])
        batch = spec.get("probe_batch") or classify_probe_batch(name)
        is_group_a = (
            batch in FOUNDATIONAL_BATCHES
            and name not in FOUNDATIONAL_GROUP_B_PROBES
            and name not in FOUNDATIONAL_GROUP_C_PROBES
        )
        for b_id in blockers:
            bucket = _ensure_bucket(b_id)
            if len(blockers) == 1:
                bucket["sole_probes"].append(name)
            else:
                bucket["co_probes"].append(name)
            if is_group_a:
                bucket["p2_group_a_probes"].append(name)

    for prog_name, spec in sorted(expectations["programs"].items()):
        if spec["kind"] != "known_unimplemented":
            continue
        for b_id in spec.get("roadmap_blockers", []):
            bucket = _ensure_bucket(b_id)
            bucket["programs"].append(prog_name)

    return dict(sorted(impact.items()))


ROADMAP_STAGE_CATALOG: dict[str, str] = {
    "P2": "Native 后端能力补齐（泛型、可空/Result `?`、闭包、浮点/字符与接口分派）",
    "P3": "语言糖、协议与标准库收口（Equatable/Display 插值、位运算、多载荷 enum 与 Pair/to）",
    "P4": "并发与扩展（原生线程 thread 与 JoinHandle）",
}


def summarize_stage_progression(
    expectations: dict[str, Any],
    roadmap_items: dict[str, dict[str, Any]] | None = None,
) -> list[dict[str, Any]]:
    """Project cumulative probe capability, Foundational Group A runnable count, and V1 program unlocks across stages P2 -> P3 -> P4."""
    if roadmap_items is None:
        v1_json_path = Path(__file__).resolve().parents[1] / "docs" / "roadmap" / "v1.json"
        if v1_json_path.is_file():
            v1_data = json.loads(v1_json_path.read_text(encoding="utf-8"))
            roadmap_items = {it["id"]: it for it in v1_data.get("items", [])}
        else:
            roadmap_items = {}

    probes = expectations["probes"]
    programs = expectations["programs"]
    base_cap = sum(
        1 for s in probes.values() if s["kind"] in ("run_ok", "compile_fail", "runtime_abort")
    )
    cohort = summarize_foundational_cohort(expectations)
    base_group_a = cohort["groups"]["A"]["runnable"]
    base_progs = sum(1 for s in programs.values() if s["kind"] == "run_ok")

    cumulative_stages: set[str] = set()
    prev_unlocked_probes: set[str] = set()
    prev_unlocked_progs: set[str] = set()
    rows: list[dict[str, Any]] = []

    for stage_id, stage_desc in ROADMAP_STAGE_CATALOG.items():
        cumulative_stages.add(stage_id)
        stage_blockers = sorted(
            b_id
            for b_id, item in roadmap_items.items()
            if item.get("stage") == stage_id
            and any(
                b_id in s.get("roadmap_blockers", [])
                for s in list(probes.values()) + list(programs.values())
                if s["kind"] == "known_unimplemented"
            )
        )
        now_unlocked_probes: list[str] = []
        cum_group_a_gain = 0
        for name, spec in sorted(probes.items()):
            if spec["kind"] != "known_unimplemented":
                continue
            blockers = spec.get("roadmap_blockers", [])
            if blockers and all(
                roadmap_items.get(b, {}).get("stage", "P2") in cumulative_stages
                for b in blockers
            ):
                if name not in prev_unlocked_probes:
                    now_unlocked_probes.append(name)
                    prev_unlocked_probes.add(name)
                batch = spec.get("probe_batch") or classify_probe_batch(name)
                if (
                    batch in FOUNDATIONAL_BATCHES
                    and name not in FOUNDATIONAL_GROUP_B_PROBES
                    and name not in FOUNDATIONAL_GROUP_C_PROBES
                ):
                    cum_group_a_gain += 1

        now_unlocked_progs: list[str] = []
        for prog_name, spec in sorted(programs.items()):
            if spec["kind"] != "known_unimplemented":
                continue
            blockers = spec.get("roadmap_blockers", [])
            if blockers and all(
                roadmap_items.get(b, {}).get("stage", "P2") in cumulative_stages
                for b in blockers
            ):
                if prog_name not in prev_unlocked_progs:
                    now_unlocked_progs.append(prog_name)
                    prev_unlocked_progs.add(prog_name)

        rows.append(
            {
                "stage": stage_id,
                "description": stage_desc,
                "stage_blockers": stage_blockers,
                "newly_unlocked_probes": now_unlocked_probes,
                "newly_unlocked_programs": now_unlocked_progs,
                "projected_probe_cap": base_cap + len(prev_unlocked_probes),
                "total_probes": len(probes),
                "projected_group_a_runnable": base_group_a + cum_group_a_gain,
                "group_a_total": cohort["groups"]["A"]["total"],
                "projected_programs_ready": base_progs + len(prev_unlocked_progs),
                "total_programs": len(programs),
            }
        )
    return rows


ROADMAP_AREA_CATALOG: dict[str, str] = {
    "types": "类型系统、标量字面量 (`Double`/`Char`)、位运算与 `Equatable`/`Comparable` 协议 (`L1`, `L3b`, `N05`)",
    "control": "多载荷与递归 enum 构造及 `when` 解构分派 (`L7`, `N02`)",
    "closures": "函数引用 `::fn` 与移动闭包调用 (`N04`)",
    "classes": "泛型 Interface 上界分派与匿名 `companion` 关联调用 (`N06`)",
    "errors": "可空链式访问、Elvis `?:` 与 `Result` `?` 传播 (`N03`, `N08`)",
    "ownership": "嵌套泛型 class 字段借用、可变写入与所有权证明 (`N01`)",
    "collections": "内建 `Pair` 构造与中缀 `to` 糖 (`S6`)",
    "text-io": "`Display` 协议与字符串插值求值 (`L2`)",
    "stdlib": "递归堆分配容器 `Box<T>` 与原生线程 `thread`/`JoinHandle` (`N07`, `T1`)",
}


def summarize_roadmap_area_impact(
    expectations: dict[str, Any],
    roadmap_items: dict[str, dict[str, Any]] | None = None,
) -> dict[str, dict[str, Any]]:
    """Aggregate known_unimplemented probes and acceptance programs by roadmap area and compiler phases."""
    if roadmap_items is None:
        v1_json_path = Path(__file__).resolve().parents[1] / "docs" / "roadmap" / "v1.json"
        if v1_json_path.is_file():
            v1_data = json.loads(v1_json_path.read_text(encoding="utf-8"))
            roadmap_items = {it["id"]: it for it in v1_data.get("items", [])}
        else:
            roadmap_items = {}

    by_area: dict[str, dict[str, Any]] = {
        area_key: {
            "area": area_key,
            "description": desc,
            "blockers": [],
            "phases": [],
            "probes": [],
            "programs": [],
        }
        for area_key, desc in ROADMAP_AREA_CATALOG.items()
    }
    for name, spec in sorted(expectations["probes"].items()):
        if spec["kind"] != "known_unimplemented":
            continue
        for b_id in spec.get("roadmap_blockers", []):
            item = roadmap_items.get(b_id, {})
            area = item.get("area", "types")
            bucket = by_area.setdefault(
                area,
                {
                    "area": area,
                    "description": area,
                    "blockers": [],
                    "phases": [],
                    "probes": [],
                    "programs": [],
                },
            )
            if b_id not in bucket["blockers"]:
                bucket["blockers"].append(b_id)
                bucket["blockers"].sort()
            for ph in item.get("phases", []):
                if ph not in bucket["phases"]:
                    bucket["phases"].append(ph)
                    bucket["phases"].sort()
            if name not in bucket["probes"]:
                bucket["probes"].append(name)

    for prog_name, spec in sorted(expectations["programs"].items()):
        if spec["kind"] != "known_unimplemented":
            continue
        for b_id in spec.get("roadmap_blockers", []):
            item = roadmap_items.get(b_id, {})
            area = item.get("area", "types")
            bucket = by_area[area]
            if b_id not in bucket["blockers"]:
                bucket["blockers"].append(b_id)
                bucket["blockers"].sort()
            for ph in item.get("phases", []):
                if ph not in bucket["phases"]:
                    bucket["phases"].append(ph)
                    bucket["phases"].sort()
            if prog_name not in bucket["programs"]:
                bucket["programs"].append(prog_name)

    return by_area


def summarize_unimplemented_promotion_contracts(
    expectations: dict[str, Any],
) -> list[dict[str, Any]]:
    """Return structured promotion contract rows for all remaining known_unimplemented probes."""
    rows: list[dict[str, Any]] = []
    for name, spec in sorted(expectations["probes"].items()):
        if spec["kind"] != "known_unimplemented":
            continue
        batch = spec.get("probe_batch") or classify_probe_batch(name)
        if batch not in FOUNDATIONAL_BATCHES:
            cohort_grp = "-"
        elif name in FOUNDATIONAL_GROUP_B_PROBES:
            cohort_grp = "B"
        elif name in FOUNDATIONAL_GROUP_C_PROBES:
            cohort_grp = "C"
        else:
            cohort_grp = "A"
        rows.append(
            {
                "name": name,
                "guide_chapter": spec["guide_chapter"],
                "probe_batch": batch,
                "foundational_group": cohort_grp,
                "baseline_marker": spec.get("baseline_marker", "-"),
                "allowed_markers": list(spec.get("allowed_markers", [])),
                "roadmap_blockers": list(spec.get("roadmap_blockers", [])),
                "target_stdout": spec.get("target_stdout", ""),
                "expected_lines": spec.get("expected_lines", 0),
                "expected_bytes": spec.get("expected_bytes", 0),
            }
        )
    return rows


COMPILER_PHASE_CATALOG: dict[int, tuple[str, str]] = {
    2: ("Phase 2 前端类型与解析", "名称解析、类型检查、运算符/协议约束与最终类型准入 (`crates/lang-frontend/src/type_checking`)"),
    3: ("Phase 3 所有权与析构", "借用冲突、部分移动、生命周期证明与逆序 `deinit` 调度 (`crates/lang-frontend/src/ownership`)"),
    4: ("Phase 4 SSA / LLVM / Runtime", "SSA 构建、所有权校验、LLVM IR 生成、原生链接与 `abort` 合同 (`crates/lang-codegen`)"),
    5: ("Phase 5 标准库与并发", "内建容器、文本/数值协议、`Box`/`Rc` 与原生线程运行时 (`crates/lang-std`)"),
}


def summarize_compiler_phase_matrix(
    expectations: dict[str, Any],
    roadmap_items: dict[str, dict[str, Any]] | None = None,
) -> dict[int, dict[str, Any]]:
    """Summarize active negative diagnostics/aborts and remaining roadmap blockers across compiler Phases 2..5."""
    if roadmap_items is None:
        v1_json_path = Path(__file__).resolve().parents[1] / "docs" / "roadmap" / "v1.json"
        if v1_json_path.is_file():
            v1_data = json.loads(v1_json_path.read_text(encoding="utf-8"))
            roadmap_items = {it["id"]: it for it in v1_data.get("items", [])}
        else:
            roadmap_items = {}

    phase_3_diags = {"L0132", "L0133", "L0135", "L0164"}
    phases_out: dict[int, dict[str, Any]] = {
        ph: {
            "phase": ph,
            "title": title,
            "description": desc,
            "active_negative_probes": [],
            "active_negative_contracts": [],
            "blockers": [],
            "blocked_probes": [],
            "blocked_programs": [],
        }
        for ph, (title, desc) in COMPILER_PHASE_CATALOG.items()
    }

    for name, spec in sorted(expectations["probes"].items()):
        kind = spec["kind"]
        if kind == "compile_fail":
            codes = spec.get("all_diagnostics", [spec["diagnostic"]])
            target_ph = 3 if any(c in phase_3_diags for c in codes) else 2
            phases_out[target_ph]["active_negative_probes"].append(name)
            for c in codes:
                tag = f"FE:{c}"
                if tag not in phases_out[target_ph]["active_negative_contracts"]:
                    phases_out[target_ph]["active_negative_contracts"].append(tag)
        elif kind == "runtime_abort":
            phases_out[4]["active_negative_probes"].append(name)
            tag = f"abort:{spec['abort_message']}"
            if tag not in phases_out[4]["active_negative_contracts"]:
                phases_out[4]["active_negative_contracts"].append(tag)
        elif kind == "known_unimplemented":
            for b_id in spec.get("roadmap_blockers", []):
                item = roadmap_items.get(b_id, {})
                for ph in item.get("phases", []):
                    if ph not in phases_out:
                        continue
                    if b_id not in phases_out[ph]["blockers"]:
                        phases_out[ph]["blockers"].append(b_id)
                        phases_out[ph]["blockers"].sort()
                    if name not in phases_out[ph]["blocked_probes"]:
                        phases_out[ph]["blocked_probes"].append(name)

    for prog_name, spec in sorted(expectations["programs"].items()):
        if spec["kind"] != "known_unimplemented":
            continue
        for b_id in spec.get("roadmap_blockers", []):
            item = roadmap_items.get(b_id, {})
            for ph in item.get("phases", []):
                if ph not in phases_out:
                    continue
                if b_id not in phases_out[ph]["blockers"]:
                    phases_out[ph]["blockers"].append(b_id)
                    phases_out[ph]["blockers"].sort()
                if prog_name not in phases_out[ph]["blocked_programs"]:
                    phases_out[ph]["blocked_programs"].append(prog_name)

    return phases_out


GUIDE_LITMUS_CATALOG: dict[int, dict[str, Any]] = {
    1: {
        "title": "基础函数与局部变量",
        "guide_chapter": "05-declarations-callables.md",
        "probes": ["p01_int_print", "p24_recursion", "q10_var_reassign_string", "v01_assign_unit_value"],
    },
    2: {
        "title": "换行分句与表达式续行",
        "guide_chapter": "04-expressions-operators.md",
        "probes": ["p03_long_uint", "t07_short_circuit_plain", "v01_assign_unit_value", "v02_assign_chain"],
    },
    3: {
        "title": "控制流与循环结构",
        "guide_chapter": "06-blocks-control-flow.md",
        "probes": ["p07_while_var", "p48_nested_loops_break", "q13_if_expr_value", "v04_stmt_if_mixed"],
    },
    4: {
        "title": "枚举类型与穷尽 when 匹配",
        "guide_chapter": "06-blocks-control-flow.md",
        "probes": ["e01_enum_when_bool_return", "e04_enum_when_int_return", "p09_enum_payload", "p49_when_enum_no_payload", "q08_enum_string_payload"],
    },
    5: {
        "title": "类、值类与伴随对象",
        "guide_chapter": "08-class-family-members.md",
        "probes": ["p12_class_inout", "p28_companion", "p44_method_on_value", "q12_companion_const"],
    },
    6: {
        "title": "泛型、接口与单态化",
        "guide_chapter": "03-types-generics.md",
        "probes": ["p10_generic_fn", "p11_interface_bound", "q16_interface_static", "t03_vmethod_generic"],
    },
    7: {
        "title": "线性所有权与移动语义",
        "guide_chapter": "10-ownership-borrowing-drop.md",
        "probes": ["p21_box_enum", "p25_linked_iter", "p32_deinit", "q19_deep_return_cleanup"],
    },
    8: {
        "title": "借用检查与在位可变引用",
        "guide_chapter": "10-ownership-borrowing-drop.md",
        "probes": ["p12c_inout_param", "p26_borrow_return", "q09_inout_class_param", "q20_replace_field"],
    },
    9: {
        "title": "空安全、流类型与非空断言",
        "guide_chapter": "09-nullability-errors.md",
        "probes": ["p14_nullable", "p14b_nullable_simple", "q05_nullable_param", "q06_nullable_return_bang", "x02_string_nullable_return"],
    },
    10: {
        "title": "函数类型与闭包捕获",
        "guide_chapter": "07-calls-lambdas-closures.md",
        "probes": ["p17_lambda", "p17b_lambda_nocapture", "p18_move_closure", "p42_lambda_mutate", "q01_lambda_local", "q02_lambda_arg_inline"],
    },
    11: {
        "title": "顺序集合遍历与结构解构",
        "guide_chapter": "12-collections-destructuring.md",
        "probes": ["p19_mutablelist", "p34_for_range", "p38_list_of_strings", "q15_borrow_param_string_list", "w01_map_for_sum"],
    },
    12: {
        "title": "关联常量与标准库互操作",
        "guide_chapter": "13-program-runtime-standard-library.md",
        "probes": ["p31_string_ops", "q12_companion_const", "s01_result_builtin"],
    },
}


def summarize_guide_litmus_alignment(expectations: dict[str, Any]) -> dict[int, dict[str, Any]]:
    """Cross-reference Guide 15 Litmus 1..12 topics against their mapped end-to-end conformance probes."""
    probes = expectations["probes"]
    out: dict[int, dict[str, Any]] = {}
    for lit_id, spec in GUIDE_LITMUS_CATALOG.items():
        mapped = list(spec["probes"])
        cap_passed = [
            p for p in mapped if probes[p]["kind"] in ("run_ok", "compile_fail", "runtime_abort")
        ]
        unimpl = [p for p in mapped if probes[p]["kind"] == "known_unimplemented"]
        blockers = sorted(
            {b for p in unimpl for b in probes[p].get("roadmap_blockers", [])}
        )
        out[lit_id] = {
            "id": lit_id,
            "title": spec["title"],
            "guide_chapter": spec["guide_chapter"],
            "probes": mapped,
            "cap_passed_count": len(cap_passed),
            "unimplemented_probes": unimpl,
            "roadmap_blockers": blockers,
            "status": "all_passed" if not unimpl else "partial",
        }
    return out


GUIDE_VERSION_BOUNDARY_CATALOG: dict[str, dict[str, Any]] = {
    "v0.44": {
        "summary": "封闭 if/when join、赋值返回 Unit、while 事实、IntRange/LongRange 与 for、八整数 checked 转换、插入序 Map 遍历",
        "probes": [
            "v01_assign_unit_value",
            "v02_assign_chain",
            "v03_join_nullable",
            "v04_stmt_if_mixed",
            "v06_for_range_sum",
            "v07_range_max",
            "v08_long_range",
            "v11_convert_ok",
            "v12_convert_abort",
            "v13_while_nullable_int",
            "v15_map_for",
            "w01_map_for_sum",
            "w02_map_for_order",
            "w06_map_mutate_in_loop",
        ],
    },
    "v0.45": {
        "summary": "单文件与 project 统一 compilation-unit 管线、显式 koven.algorithms.take 导入、enum subject when 两模式零差异",
        "probes": [
            "e01_enum_when_bool_return",
            "e02_enum_when_stmt_println",
            "e03_enum_when_bool_val",
            "e04_enum_when_int_return",
            "p49_when_enum_no_payload",
            "p50_enum_method",
            "y01_import_take_for_sum",
            "y02_take_no_import",
            "y03_method_take_for",
            "y05_take_borrow_val_call",
        ],
    },
    "v0.46": {
        "summary": "Abort 输出合同 (SPEC-0355)、Result 内建与无参 is Result.Ok (SPEC-0349)、顶层仅 const val (SPEC-0343)、匿名 companion { }",
        "probes": [
            "p02_interp",
            "p04_overflow",
            "p16_result_q",
            "p23_global_var",
            "p27_thread",
            "p28_companion",
            "p35_value_eq",
            "p37_is_generic_case",
            "p45_abort_message",
            "q12_companion_const",
            "s01_result_builtin",
            "v12_convert_abort",
        ],
    },
    "v0.47": {
        "summary": "Guide 13 文本字节基础、可信标准 String Borrow 扩展与 Result<String, String> 载荷 when 闭环 (SPEC-0368)",
        "probes": [
            "p16b_result_string",
            "p31_string_ops",
            "p46_string_field_class",
            "q10_var_reassign_string",
            "x02_string_nullable_return",
        ],
    },
}


def summarize_guide_version_boundaries(expectations: dict[str, Any]) -> dict[str, dict[str, Any]]:
    """Cross-reference Guide 15 version enablement boundaries (v0.44..v0.47) against their conformance probes."""
    probes = expectations["probes"]
    out: dict[str, dict[str, Any]] = {}
    for ver_key, spec in GUIDE_VERSION_BOUNDARY_CATALOG.items():
        mapped = list(spec["probes"])
        cap_passed = [
            p for p in mapped if probes[p]["kind"] in ("run_ok", "compile_fail", "runtime_abort")
        ]
        unimpl = [p for p in mapped if probes[p]["kind"] == "known_unimplemented"]
        blockers = sorted(
            {b for p in unimpl for b in probes[p].get("roadmap_blockers", [])}
        )
        out[ver_key] = {
            "version": ver_key,
            "summary": spec["summary"],
            "probes": mapped,
            "cap_passed_count": len(cap_passed),
            "unimplemented_probes": unimpl,
            "roadmap_blockers": blockers,
            "status": "all_passed" if not unimpl else "partial",
        }
    return out


PROGRAM_PREREQUISITE_PROBES: dict[str, list[str]] = {
    "config_parse": [
        "p02_interp",
        "p16_result_q",
        "p16b_result_string",
        "p31_string_ops",
        "q08_enum_string_payload",
        "s01_result_builtin",
        "w08_map_string_values",
    ],
    "expr_eval": [
        "p05_double",
        "p06_char",
        "p09_enum_payload",
        "p16_result_q",
        "p21_box_enum",
        "p24_recursion",
        "q08_enum_string_payload",
        "s01_result_builtin",
    ],
    "linked_stack": [
        "p14_nullable",
        "p21_box_enum",
        "p25_linked_iter",
        "p25b_linked_recursive",
        "p40_nested_generic_class",
        "q20_replace_field",
        "v18_while_nullable_class",
    ],
    "raii_state_machine": [
        "p12_class_inout",
        "p32_deinit",
        "p40_nested_generic_class",
        "p49_when_enum_no_payload",
        "q08_enum_string_payload",
        "q19_deep_return_cleanup",
        "q20_replace_field",
    ],
    "varint_codec": [
        "p03_long_uint",
        "p16_result_q",
        "p19_mutablelist",
        "p40_nested_generic_class",
        "s01_result_builtin",
        "v11_convert_ok",
        "v12_convert_abort",
    ],
    "word_count": [
        "p15_elvis",
        "p19_mutablelist",
        "p31_string_ops",
        "q08_enum_string_payload",
        "w01_map_for_sum",
        "w02_map_for_order",
        "w11_word_count",
    ],
}


def summarize_program_probe_prerequisites(
    expectations: dict[str, Any],
) -> dict[str, dict[str, Any]]:
    """Cross-reference the 6 V1 acceptance programs against their atomic prerequisite conformance probes."""
    probes = expectations["probes"]
    programs = expectations["programs"]
    out: dict[str, dict[str, Any]] = {}
    for prog_name, pre_list in sorted(PROGRAM_PREREQUISITE_PROBES.items()):
        passed = [
            p for p in pre_list if probes[p]["kind"] in ("run_ok", "compile_fail", "runtime_abort")
        ]
        unimpl = [p for p in pre_list if probes[p]["kind"] == "known_unimplemented"]
        out[prog_name] = {
            "program": prog_name,
            "guide_chapter": programs[prog_name]["guide_chapter"],
            "roadmap_blockers": list(programs[prog_name].get("roadmap_blockers", [])),
            "prerequisite_probes": list(pre_list),
            "passed_probes": passed,
            "unimplemented_probes": unimpl,
        }
    return out


def summarize_corpus_source_scale(expectations: dict[str, Any]) -> dict[str, Any]:
    """Aggregate source line and byte metrics across all 154 probes, 6 programs, and 13 Guide chapters."""
    probes = expectations["probes"]
    programs = expectations["programs"]
    by_chapter: dict[str, dict[str, Any]] = {
        ch: {
            "chapter": ch,
            "domain": GUIDE_CHAPTER_DOMAINS[ch],
            "probe_count": 0,
            "probe_source_lines": 0,
            "probe_source_bytes": 0,
            "program_count": 0,
            "program_source_lines": 0,
            "program_source_bytes": 0,
            "programs": [],
        }
        for ch in VALID_GUIDE_CHAPTERS
    }
    tot_probe_lines = 0
    tot_probe_bytes = 0
    for name, spec in sorted(probes.items()):
        ch = spec.get("guide_chapter", "05-declarations-callables.md")
        sl = int(spec.get("source_lines", 0))
        sb = int(spec.get("source_bytes", 0))
        tot_probe_lines += sl
        tot_probe_bytes += sb
        if ch in by_chapter:
            by_chapter[ch]["probe_count"] += 1
            by_chapter[ch]["probe_source_lines"] += sl
            by_chapter[ch]["probe_source_bytes"] += sb

    tot_prog_lines = 0
    tot_prog_bytes = 0
    for name, spec in sorted(programs.items()):
        ch = spec.get("guide_chapter", "13-program-runtime-standard-library.md")
        sl = int(spec.get("source_lines", 0))
        sb = int(spec.get("source_bytes", 0))
        tot_prog_lines += sl
        tot_prog_bytes += sb
        if ch in by_chapter:
            by_chapter[ch]["program_count"] += 1
            by_chapter[ch]["program_source_lines"] += sl
            by_chapter[ch]["program_source_bytes"] += sb
            by_chapter[ch]["programs"].append(name)

    return {
        "total_probe_count": len(probes),
        "total_probe_source_lines": tot_probe_lines,
        "total_probe_source_bytes": tot_probe_bytes,
        "total_program_count": len(programs),
        "total_program_source_lines": tot_prog_lines,
        "total_program_source_bytes": tot_prog_bytes,
        "total_corpus_source_lines": tot_probe_lines + tot_prog_lines,
        "total_corpus_source_bytes": tot_probe_bytes + tot_prog_bytes,
        "by_chapter": by_chapter,
    }


V3_SANITIZER_RESOURCE_CATALOG: dict[str, dict[str, Any]] = {
    "deterministic_deinit_drop": {
        "title": "确定性 `deinit` 析构与提前返回栈展开清理",
        "sanitizer_focus": "ASan `drop` (heap-use-after-free) / LSan 零泄漏",
        "probes": [
            "p32_deinit",
            "q19_deep_return_cleanup",
        ],
        "programs": ["raii_state_machine"],
    },
    "heap_class_rc_and_box": {
        "title": "堆分配 `class`、`Rc<T>` 共享引用计数、`Box<T>` 递归枚举与 `replace`",
        "sanitizer_focus": "ASan `user` / `runtime` 与 LSan `leak` 对照",
        "probes": [
            "p12_class_inout",
            "p12b_class_field_assign",
            "p21_box_enum",
            "p22_rc",
            "p25b_linked_recursive",
            "p40_nested_generic_class",
            "p44b_method_on_class",
            "p46_string_field_class",
            "q04_rc_simple",
            "q09_inout_class_param",
            "q20_replace_field",
        ],
        "programs": ["expr_eval", "linked_stack"],
    },
    "string_and_text_owner": {
        "title": "堆拥有 `String` 克隆、拼接、重赋值、枚举行李与可空返回",
        "sanitizer_focus": "ASan 堆缓冲区边界 / LSan 字符串所有权释放",
        "probes": [
            "p02_interp",
            "p16b_result_string",
            "p26_borrow_return",
            "p31_string_ops",
            "p41_when_string",
            "q08_enum_string_payload",
            "q10_var_reassign_string",
            "x02_string_nullable_return",
        ],
        "programs": ["config_parse"],
    },
    "collection_list_and_map_provider": {
        "title": "`List` / `MutableList` / `Map` / `MutableMap` 扩容、覆盖、删除与遍历析构",
        "sanitizer_focus": "SPEC-0288 `map-fixture` 与 12 组 `provider-fixtures` (`singleton`/`overwrite`/`growth`)",
        "probes": [
            "p19_mutablelist",
            "p38_list_of_strings",
            "p47_list_index_string",
            "q15_borrow_param_string_list",
            "t05_map_loop_continue",
            "t10_map_nested_if_break",
            "u02_list_add_loop_string",
            "v15_map_for",
            "v22_range_with_map",
            "w01_map_for_sum",
            "w02_map_for_order",
            "w03_map_overwrite_remove",
            "w08_map_string_values",
            "w09_map_temp_source",
            "w11_word_count",
        ],
        "programs": ["word_count"],
    },
    "closure_env_and_thread_spawn": {
        "title": "捕获闭包环境、`move` 闭包所有权转移与 `thread(move { ... })`",
        "sanitizer_focus": "ASan 闭包环境生命周期 / 跨线程 `Send` 所有权传递",
        "probes": [
            "p17_lambda",
            "p18_move_closure",
            "p27_thread",
            "q01_lambda_local",
            "q02_lambda_arg_inline",
        ],
        "programs": [],
    },
    "checked_abort_and_overflow": {
        "title": "受检整数溢出、`error(msg)` 与显式数值转换 `SIGABRT` 中止边界",
        "sanitizer_focus": "Guide 09 D12 `abort: <msg>` + 栈帧确定性退出",
        "probes": [
            "p04_overflow",
            "p45_abort_message",
            "v12_convert_abort",
        ],
        "programs": ["varint_codec"],
    },
}


def summarize_v3_sanitizer_coverage(expectations: dict[str, Any]) -> dict[str, dict[str, Any]]:
    """Cross-reference V3 native sanitizer (ASan/LSan) resource-lifecycle categories against probes and programs."""
    probes = expectations["probes"]
    out: dict[str, dict[str, Any]] = {}
    for cat_key, spec in V3_SANITIZER_RESOURCE_CATALOG.items():
        c_probes = list(spec["probes"])
        c_progs = list(spec["programs"])
        runnable = [
            p for p in c_probes if probes[p]["kind"] in ("run_ok", "runtime_abort")
        ]
        unimpl = [p for p in c_probes if probes[p]["kind"] == "known_unimplemented"]
        blockers = sorted(
            {b for p in unimpl for b in probes[p].get("roadmap_blockers", [])}
        )
        out[cat_key] = {
            "category": cat_key,
            "title": spec["title"],
            "sanitizer_focus": spec["sanitizer_focus"],
            "probes": c_probes,
            "runnable_probes": runnable,
            "unimplemented_probes": unimpl,
            "roadmap_blockers": blockers,
            "programs": c_progs,
            "status": "all_runnable" if not unimpl else "partial",
        }
    return out


def render_readme_chapter_table(expectations: dict[str, Any]) -> str:
    """Render the Guide chapter coverage summary table embedded in conformance/README.md."""
    chapter_stats = summarize_chapter_polarity(expectations)
    lines = [
        "| Guide v0.46 章节 | Probe 总数 | `run_ok` | `compile_fail` | `runtime_abort` | `known_unimplemented` | 能力通过 (`capability_passed`) |",
        "|---|---:|---:|---:|---:|---:|---:|",
    ]
    tot_all = tot_ok = tot_cf = tot_ab = tot_ku = tot_cap = 0
    for ch in VALID_GUIDE_CHAPTERS:
        st = chapter_stats[ch]
        tot_all += st["total"]
        tot_ok += st["run_ok"]
        tot_cf += st["compile_fail"]
        tot_ab += st["runtime_abort"]
        tot_ku += st["known_unimplemented"]
        tot_cap += st["cap_passed"]
        lines.append(
            f"| [{ch}](../docs/guide/{ch}) | {st['total']} | {st['run_ok']} | {st['compile_fail']} | "
            f"{st['runtime_abort']} | {st['known_unimplemented']} | {st['cap_passed']} / {st['total']} |"
        )
    lines.append(
        f"| **合计** | **{tot_all}** | **{tot_ok}** | **{tot_cf}** | **{tot_ab}** | **{tot_ku}** | **{tot_cap} / {tot_all}** |"
    )
    return "\n".join(lines)


README_CHAPTER_TABLE_RE = re.compile(
    r"(\| Guide v0\.4[67] 章节 \| Probe 总数 \| `run_ok` \| `compile_fail` \| `runtime_abort` \| `known_unimplemented` \| 能力通过 \(`capability_passed`\) \|\n"
    r"\|---\|---:\|---:\|---:\|---:\|---:\|---:\|\n"
    r"(?:\| \[[^\n]+\n)+"
    r"\| \*\*合计\*\* \|[^\n]+)"
)


def sync_readme_chapter_table(readme_text: str, expectations: dict[str, Any]) -> str:
    """Replace the Guide chapter summary table in conformance/README.md with fresh counts."""
    table_md = render_readme_chapter_table(expectations)
    if not README_CHAPTER_TABLE_RE.search(readme_text):
        raise ValueError("conformance/README.md is missing the Guide chapter coverage table")
    return README_CHAPTER_TABLE_RE.sub(lambda _: table_md, readme_text, count=1)


def render_oracle_audit_markdown(expectations: dict[str, Any]) -> str:
    """Render deterministic Markdown audit ledger from expectations.json."""
    probes = expectations["probes"]
    programs = expectations["programs"]
    chapter_stats = summarize_chapter_polarity(expectations)
    neg_catalog = summarize_negative_oracle_catalog(expectations)
    base_evo = summarize_baseline_evolution(expectations)
    spec_trace = summarize_v046_spec_traceability(expectations)
    origin_stats = summarize_oracle_origins(expectations)
    shape_stats = summarize_output_shape_distribution(expectations)
    batch_stats = summarize_probe_batches(expectations)
    marker_stats = summarize_unimplemented_markers(expectations)
    foundational_stats = summarize_foundational_cohort(expectations)
    v2_readiness = summarize_v2_chapter_readiness(expectations)

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
    lines.extend(
        [
            "",
            "### 1.2 历史基线 `baseline-aef3f4f8.tsv` 演进与零退化对账汇总（154 Probes）",
            "",
            "| 演进分类 (`baseline_evolution`) | 历史基线状态 (`baseline-aef3f4f8.tsv`) | 当前强 Oracle 状态 (`expectations.json`) | Probe 数量 | 代表性 / 完整 Probe 列表 |",
            "|---|---|---|---:|---|",
            f"| `baseline_ok_preserved` | `OK` | `run_ok`（精确字节匹配） | {len(base_evo['baseline_ok_preserved'])} | 历史 `OK` 基线 100% 保持通过（含 `q12_companion_const` 匿名 `companion {{ }}` 语法更新） |",
            f"| `promoted_to_run_ok` | `BE:InvalidSsa` / `BE:MissingFact` / `BE:UnsupportedNode` / `FE:L0080` / `FE:L0111` | `run_ok`（新晋能力通过） | {len(base_evo['promoted_to_run_ok'])} | " + ", ".join(f"`{p}`" for p in base_evo["promoted_to_run_ok"]) + " |",
            f"| `upgraded_to_structured_abort` | `abort(exit 1,no msg)` | `runtime_abort`（`SIGABRT` + `abort: <msg>`） | {len(base_evo['upgraded_to_structured_abort'])} | " + ", ".join(f"`{p}`" for p in base_evo["upgraded_to_structured_abort"]) + " |",
            f"| `upgraded_to_compile_fail` | `FE:L0165` | `compile_fail`（`FE:L0167` 规范拒绝） | {len(base_evo['upgraded_to_compile_fail'])} | " + ", ".join(f"`{p}`" for p in base_evo["upgraded_to_compile_fail"]) + " |",
            f"| `baseline_compile_fail_preserved` | `FE:L0080`..`FE:L0164` | `compile_fail`（精确诊断序列匹配） | {len(base_evo['baseline_compile_fail_preserved'])} | " + ", ".join(f"`{p}`" for p in base_evo["baseline_compile_fail_preserved"]) + " |",
            f"| `remaining_known_unimplemented` | `BE:UnsupportedNode` / `FE:L0165` / `FE:L0080` / `ERR` | `known_unimplemented`（100% 绑定 `target_stdout` 与 `roadmap_blockers`） | {len(base_evo['remaining_known_unimplemented'])} | " + ", ".join(f"`{p}`" for p in base_evo["remaining_known_unimplemented"]) + " |",
            f"| `regressed_from_baseline_ok` | `OK` | 非 `run_ok`（基线退化） | {len(base_evo['regressed_from_baseline_ok'])} | 无（零基线退化） |",
            "",
            "### 1.3 Guide v0.46 豁免项 (`v046_spec_contract`) 与 Spec 交付合同溯源矩阵（35 Probes）",
            "",
            "| Spec 合同编号 | 规范文档路径 | 合同状态 | 关联豁免 Probe 数 | 交付后分布 | 绑定的 Probe 列表 |",
            "|---|---|---|---:|---|---|",
        ]
    )
    for sref, info in spec_trace.items():
        doc_link = (
            f"[{info['doc_relpath'].split('/')[-1]}](../{info['doc_relpath']})"
            if info["doc_relpath"]
            else "-"
        )
        kinds_summary = ", ".join(
            f"`{k}`: {cnt}" for k, cnt in sorted(info["kinds"].items())
        )
        plist = ", ".join(f"`{p}`" for p in info["probes"])
        lines.append(
            f"| `{sref}` | {doc_link} | `{info['status']}` | {len(info['probes'])} | {kinds_summary} | {plist} |"
        )

    lines.extend(
        [
            "",
            "### 1.4 强 Oracle 溯源分类矩阵（`oracle_origin` Taxonomy，154 Probes + 6 Programs）",
            "",
            "| 溯源分类 (`oracle_origin`) | 审计验证契约说明 | Probe 数 | Program 数 | 期望类型分布 (`kind`) |",
            "|---|---|---:|---:|---|",
        ]
    )
    for origin, info in origin_stats.items():
        kinds_repr = ", ".join(
            f"`{k}`: {cnt}" for k, cnt in sorted(info["kinds"].items())
        )
        lines.append(
            f"| `{origin}` | {info['description']} | {info['probes']} | {info['programs']} | {kinds_repr} |"
        )

    ml_summary = ", ".join(
        f"`{m['name']}` ({m['lines']}L/{m['bytes']}B)" for m in shape_stats["multi_line_probes"]
    )
    lines.extend(
        [
            "",
            "### 1.5 标准输出规模与多行形态分布矩阵（`expected_lines` / `expected_bytes`）",
            "",
            "| 输出形态分类 | 适用范围 | 条目数量 | 累计期望行数 | 累计期望字节数 | 代表性 / 完整条目明细 |",
            "|---|---|---:|---:|---:|---|",
            f"| `single_line_stdout` | 1 行输出 Probe (`run_ok` + `known_unimplemented`) | {len(shape_stats['single_line_probes'])} | {len(shape_stats['single_line_probes'])} | {shape_stats['total_probe_bytes'] - sum(m['bytes'] for m in shape_stats['multi_line_probes'])} B | 单行精确字节匹配（全部以 `\\n` 结尾） |",
            f"| `multi_line_stdout` | ≥2 行输出 Probe (`run_ok` + `known_unimplemented`) | {len(shape_stats['multi_line_probes'])} | {sum(m['lines'] for m in shape_stats['multi_line_probes'])} | {sum(m['bytes'] for m in shape_stats['multi_line_probes'])} B | {ml_summary} |",
            f"| `zero_stdout_negative` | 零标准输出负向 Probe (`compile_fail` + `runtime_abort`) | {len(shape_stats['zero_stdout_probes'])} | 0 | 0 B | " + ", ".join(f"`{p}`" for p in shape_stats["zero_stdout_probes"]) + " |",
            f"| `acceptance_programs` | V1 验收程序 (`expected.txt`) | {len(programs)} | {shape_stats['total_program_lines']} | {shape_stats['total_program_bytes']} B | " + ", ".join(f"`{k}` ({v['expected_lines']}L/{v['expected_bytes']}B)" for k, v in sorted(programs.items())) + " |",
            "",
            "### 1.6 Probe 批次前缀与演进关注面分布矩阵（11 Batches，154 Probes）",
            "",
            "| 批次前缀 (`probe_batch`) | 关注面与批次说明 | Probe 总数 | `run_ok` | `compile_fail` | `runtime_abort` | `known_unimplemented` | 能力通过 (`capability_passed`) | 批次状态 (`batch_status`) |",
            "|---|---|---:|---:|---:|---:|---:|---:|---|",
        ]
    )
    all_passed_batches = sum(
        1 for b in batch_stats.values() if b["batch_status"] == "all_passed"
    )
    partial_batches = sum(
        1 for b in batch_stats.values() if b["batch_status"] == "partial"
    )
    for b_key, b_row in batch_stats.items():
        lines.append(
            f"| `{b_key}` | {b_row['description']} | {b_row['total']} | {b_row['run_ok']} | "
            f"{b_row['compile_fail']} | {b_row['runtime_abort']} | {b_row['known_unimplemented']} | "
            f"{b_row['cap_passed']} / {b_row['total']} | `{b_row['batch_status']}` |"
        )
    lines.append(
        f"| **合计** | **11 个演进批次** | **{tot_all}** | **{tot_ok}** | **{tot_cf}** | **{tot_ab}** | **{tot_ku}** | **{tot_cap} / {tot_all}** | **{all_passed_batches} 全通过 / {partial_batches} 部分通过** |"
    )
    lines.extend(
        [
            "",
            "### 1.7 未实现允许记号（`allowed_markers`）与编译阶段分布矩阵（7 Markers）",
            "",
            "| 允许记号 (`allowed_marker`) | 归属编译阶段 | 记号契约说明 | 基线命中数 (`baseline_marker`) | 授权 Probe 数 | 授权 Program 数 |",
            "|---|---|---|---:|---:|---:|",
        ]
    )
    for m_key, m_row in marker_stats.items():
        lines.append(
            f"| `{m_key}` | {m_row['stage']} | {m_row['description']} | {m_row['baseline_count']} | "
            f"{len(m_row['probes'])} | {len(m_row['programs'])} |"
        )

    grp_a = foundational_stats["groups"]["A"]
    grp_b = foundational_stats["groups"]["B"]
    grp_c = foundational_stats["groups"]["C"]
    lines.extend(
        [
            "",
            f"### 1.8 初始 {foundational_stats['total_foundational']} 探针（`p`/`q`/`r`/`s` 批次 A/B/C 三组）与 P2 阶段门槛对账矩阵",
            "",
            "| 分组 | 分组定义与审计口径 | 总数 | 可运行 (`run_ok` + `runtime_abort`) | `compile_fail` | `known_unimplemented` | 阶段门槛 / 当前状态说明 |",
            "|---|---|---:|---:|---:|---:|---|",
            f"| **A 组** | {grp_a['label']} | {grp_a['total']} | {grp_a['runnable']} ({len(grp_a['run_ok'])} + {len(grp_a['runtime_abort'])}) | {len(grp_a['compile_fail'])} (`q17_generic_class`) | {len(grp_a['known_unimplemented'])} | P2 门槛 `≥ {foundational_stats['p2_target_min']} / {grp_a['total']}`（当前 `{grp_a['runnable']} / {grp_a['total']}`，距门槛差 `{foundational_stats['p2_gap_to_target']}` 项） |",
            f"| **B 组** | {grp_b['label']} | {grp_b['total']} | {grp_b['runnable']} | {len(grp_b['compile_fail'])} | {len(grp_b['known_unimplemented'])} | `8 / 8 (100%)` 精确命中期望前端诊断码 (`pass_diagnostic`) |",
            f"| **C 组** | {grp_c['label']} | {grp_c['total']} | {grp_c['runnable']} (`p37_is_generic_case`) | {len(grp_c['compile_fail'])} (`p23_global_var`) | {len(grp_c['known_unimplemented'])} (`p35_value_eq`) | `p23` 规范拒绝 (`FE:L0167`)、`p37` 能力通过 (`SPEC-0349`)、`p35` 待 `L1` |",
            "",
            "### 1.9 Guide 13 章 V2 终态完备性（正反例双极性 + 零未实现）差距矩阵",
            "",
            "| Guide 章节 | 规范领域 | 正向已通 (`run_ok`) | 负向契约 (`compile_fail` / `abort`) | 剩余 `known_unimplemented` | 路线图阻塞项 | 关联 V1 验收程序 | V2 章节就绪状态 (`v2_status`) |",
            "|---|---|---:|---|---|---|---|---|",
        ]
    )
    for ch in VALID_GUIDE_CHAPTERS:
        r_row = v2_readiness[ch]
        neg_repr = (
            f"{r_row['negative_active']} (" + ", ".join(f"`{c}`" for c in r_row["negative_contracts"]) + ")"
            if r_row["negative_contracts"]
            else "0（待补反例）"
        )
        ku_repr = (
            f"{r_row['known_unimplemented']} (" + ", ".join(f"`{p}`" for p in r_row["unimplemented_probes"]) + ")"
            if r_row["unimplemented_probes"]
            else "0"
        )
        blk_repr = (
            ", ".join(f"`{b}`" for b in r_row["roadmap_blockers"])
            if r_row["roadmap_blockers"]
            else "-"
        )
        prog_repr = (
            ", ".join(f"`{p}`" for p in r_row["acceptance_programs"])
            if r_row["acceptance_programs"]
            else "-"
        )
        lines.append(
            f"| [{ch}](../docs/guide/{ch}) | {r_row['domain']} | {r_row['positive_active']} | "
            f"{neg_repr} | {ku_repr} | {blk_repr} | {prog_repr} | `{r_row['v2_status']}` |"
        )

    litmus_align = summarize_guide_litmus_alignment(expectations)
    lines.extend(
        [
            "",
            "### 1.10 Guide 15 规范性 Litmus 1–12 与端到端 Conformance Probe 对齐矩阵",
            "",
            "| Litmus 编号与主题 | 归属 Guide 章节 | 关联端到端 Probe | 能力通过 (`capability_passed`) | 剩余未实现与阻塞项 | 状态 |",
            "|---|---|---|---:|---|---|",
        ]
    )
    for lit_id, l_row in litmus_align.items():
        ch = l_row["guide_chapter"]
        p_repr = ", ".join(f"`{p}`" for p in l_row["probes"])
        rem_repr = (
            ", ".join(f"`{p}`" for p in l_row["unimplemented_probes"])
            + " (" + ", ".join(f"`{b}`" for b in l_row["roadmap_blockers"]) + ")"
            if l_row["unimplemented_probes"]
            else "-"
        )
        lines.append(
            f"| **Litmus {lit_id}**：{l_row['title']} | [{ch}](../docs/guide/{ch}) | {p_repr} | "
            f"{l_row['cap_passed_count']} / {len(l_row['probes'])} | {rem_repr} | `{l_row['status']}` |"
        )

    ver_bounds = summarize_guide_version_boundaries(expectations)
    lines.extend(
        [
            "",
            "### 1.11 Guide 15 版本启用边界（`v0.44` → `v0.45` → `v0.46` → `v0.47`）与 Probe 验收对账矩阵",
            "",
            "| Guide 启用边界 | 核心启用语义与阶段合同 | 关联验收 Probe 数 | 能力通过 (`capability_passed`) | 剩余未实现与阻塞项 | 状态 |",
            "|---|---|---:|---:|---|---|",
        ]
    )
    for ver_key, v_row in ver_bounds.items():
        rem_v = (
            ", ".join(f"`{p}`" for p in v_row["unimplemented_probes"])
            + " (" + ", ".join(f"`{b}`" for b in v_row["roadmap_blockers"]) + ")"
            if v_row["unimplemented_probes"]
            else "-"
        )
        lines.append(
            f"| **{ver_key} 启用边界** | {v_row['summary']} | {len(v_row['probes'])} | "
            f"{v_row['cap_passed_count']} / {len(v_row['probes'])} | {rem_v} | `{v_row['status']}` |"
        )

    src_scale = summarize_corpus_source_scale(expectations)
    lines.extend(
        [
            "",
            "### 1.12 语料库源码规模、LF 尾换行卫生契约与章节源码分布汇总",
            "",
            "| Guide v0.46 章节 | 领域主题 | Probe 数量 | Probe 源码规模 (`lines / bytes`) | 关联验收程序 | Program 源码规模 (`lines / bytes`) | 章节源码合计 |",
            "|---|---|---:|---:|---|---:|---:|",
        ]
    )
    for ch in VALID_GUIDE_CHAPTERS:
        sc_row = src_scale["by_chapter"][ch]
        prog_s = ", ".join(f"`{p}`" for p in sc_row["programs"]) if sc_row["programs"] else "-"
        prog_scale_s = (
            f"{sc_row['program_source_lines']} 行 / {sc_row['program_source_bytes']} B"
            if sc_row["program_count"] > 0
            else "0 行 / 0 B"
        )
        ch_tot_l = sc_row["probe_source_lines"] + sc_row["program_source_lines"]
        ch_tot_b = sc_row["probe_source_bytes"] + sc_row["program_source_bytes"]
        lines.append(
            f"| [{ch}](../docs/guide/{ch}) | {sc_row['domain']} | {sc_row['probe_count']} | "
            f"{sc_row['probe_source_lines']} 行 / {sc_row['probe_source_bytes']} B | {prog_s} | "
            f"{prog_scale_s} | {ch_tot_l} 行 / {ch_tot_b} B |"
        )
    lines.append(
        f"| **合计** | **13 个核心章节** | **{src_scale['total_probe_count']}** | "
        f"**{src_scale['total_probe_source_lines']} 行 / {src_scale['total_probe_source_bytes']} B** | "
        f"**{src_scale['total_program_count']} Programs** | "
        f"**{src_scale['total_program_source_lines']} 行 / {src_scale['total_program_source_bytes']} B** | "
        f"**{src_scale['total_corpus_source_lines']} 行 / {src_scale['total_corpus_source_bytes']} B** |"
    )

    v3_cov = summarize_v3_sanitizer_coverage(expectations)
    lines.extend(
        [
            "",
            "### 1.13 V3 原生内存安全（ASan / LSan）资源生命周期与堆所有者 Probe 覆盖矩阵",
            "",
            "| V3 资源安全分类 | 覆盖语义与生命周期特征 | `check_native_sanitizers.py` 关注面 | 关联 Probe 就绪率 | 待解封 Probe 与阻塞项 | 关联 V1 验收程序 | 状态 |",
            "|---|---|---|---:|---|---|---|",
        ]
    )
    for cat_key, v3_row in v3_cov.items():
        rem_v3 = (
            ", ".join(f"`{p}`" for p in v3_row["unimplemented_probes"])
            + " (" + ", ".join(f"`{b}`" for b in v3_row["roadmap_blockers"]) + ")"
            if v3_row["unimplemented_probes"]
            else "-"
        )
        prog_v3 = ", ".join(f"`{pr}`" for pr in v3_row["programs"]) if v3_row["programs"] else "-"
        lines.append(
            f"| `{cat_key}` | {v3_row['title']} | {v3_row['sanitizer_focus']} | "
            f"{len(v3_row['runnable_probes'])} / {len(v3_row['probes'])} | {rem_v3} | {prog_v3} | `{v3_row['status']}` |"
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

    prog_blockers = summarize_program_roadmap_blockers(expectations, roadmap_items)
    lines.extend(
        [
            "",
            f"### 2.1 六个 V1 验收程序按路线图阻塞项（`v1.json`）归类对账（{len(prog_blockers)} Roadmap Blockers）",
            "",
            "| 路线图条目 ID | 路线图条目标题 | 状态 | 阻塞 Program 数 | 阻塞的验收程序 (`programs`) | 同时阻塞的 Probe 数 |",
            "|---|---|---|---:|---|---:|",
        ]
    )
    for b_id, b_info in prog_blockers.items():
        prog_list = ", ".join(f"`{p}`" for p in b_info["programs"])
        lines.append(
            f"| `{b_id}` | {b_info['title']} | `{b_info['status']}` | {len(b_info['programs'])} | {prog_list} | {len(b_info['probes'])} |"
        )

    prog_prereqs = summarize_program_probe_prerequisites(expectations)
    lines.extend(
        [
            "",
            "### 2.2 六个 V1 验收程序与前置核心能力 Probe 对账矩阵",
            "",
            "| 验收程序 | 归属章节 | 前置 Probe 通过率 | 已通过前置 Probe (`run_ok` / `compile_fail` / `abort`) | 待解封前置 Probe (`known_unimplemented`) | 路线图阻塞项 |",
            "|---|---|---:|---|---|---|",
        ]
    )
    for prog_name, pr_row in prog_prereqs.items():
        ch = pr_row["guide_chapter"]
        passed_s = ", ".join(f"`{p}`" for p in pr_row["passed_probes"])
        unimpl_s = ", ".join(f"`{p}`" for p in pr_row["unimplemented_probes"])
        blk_s = ", ".join(f"`{b}`" for b in pr_row["roadmap_blockers"])
        lines.append(
            f"| `{prog_name}` | [{ch}](../docs/guide/{ch}) | {len(pr_row['passed_probes'])} / {len(pr_row['prerequisite_probes'])} | "
            f"{passed_s} | {unimpl_s} | {blk_s} |"
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

    unlock_impact = summarize_blocker_unlock_impact(expectations, roadmap_items)
    lines.extend(
        [
            "",
            f"### 3.1 路线图阻塞项解封收益分析（单阻塞独立解封 vs 联合阻塞与 P2 门槛贡献，{len(unlock_impact)} Blockers）",
            "",
            "| 路线图条目 | 阶段 | 状态 | 独立解封 Probe (`sole_probes`) | 联合阻塞 Probe (`co_probes`) | P2 A 组贡献数 | 关联 V1 验收程序 |",
            "|---|---|---|---|---|---:|---|",
        ]
    )
    for b_id, u_row in unlock_impact.items():
        sole_repr = (
            f"{len(u_row['sole_probes'])} (" + ", ".join(f"`{p}`" for p in u_row["sole_probes"]) + ")"
            if u_row["sole_probes"]
            else "0"
        )
        co_repr = (
            f"{len(u_row['co_probes'])} (" + ", ".join(f"`{p}`" for p in u_row["co_probes"]) + ")"
            if u_row["co_probes"]
            else "0"
        )
        prog_repr = (
            ", ".join(f"`{p}`" for p in u_row["programs"])
            if u_row["programs"]
            else "-"
        )
        lines.append(
            f"| `{b_id}` | `{u_row['stage']}` | `{u_row['status']}` | {sole_repr} | {co_repr} | "
            f"{len(u_row['p2_group_a_probes'])} | {prog_repr} |"
        )

    stage_prog = summarize_stage_progression(expectations, roadmap_items)
    lines.extend(
        [
            "",
            "### 3.2 路线图阶段（`P2` → `P3` → `P4`）递进解封与里程碑投射矩阵",
            "",
            "| 阶段里程碑 | 阶段核心目标 | 本阶段清零阻塞项 | 本阶段新增解封 Probe | 累计能力通过 (`capability_passed`) | P2 A 组可运行 (`≥ 70 / 74`) | V1 验收程序就绪 |",
            "|---|---|---|---|---:|---:|---:|",
        ]
    )
    for s_row in stage_prog:
        s_blks = ", ".join(f"`{b}`" for b in s_row["stage_blockers"]) if s_row["stage_blockers"] else "-"
        s_probes = (
            f"+{len(s_row['newly_unlocked_probes'])} (" + ", ".join(f"`{p}`" for p in s_row["newly_unlocked_probes"]) + ")"
            if s_row["newly_unlocked_probes"]
            else "0"
        )
        lines.append(
            f"| **{s_row['stage']} 完成** | {s_row['description']} | {s_blks} | {s_probes} | "
            f"**{s_row['projected_probe_cap']} / {s_row['total_probes']}** | "
            f"**{s_row['projected_group_a_runnable']} / {s_row['group_a_total']}** | "
            f"**{s_row['projected_programs_ready']} / {s_row['total_programs']}** |"
        )

    area_impact = summarize_roadmap_area_impact(expectations, roadmap_items)
    lines.extend(
        [
            "",
            f"### 3.3 路线图领域（`area`）与编译器阶段（`phases`）阻塞分布矩阵（{len(area_impact)} Areas）",
            "",
            "| 路线图领域 (`area`) | 领域职责说明 | 涉及编译阶段 (`phases`) | 关联阻塞项 | 阻塞 Probe 数 | 阻塞 Program 数 |",
            "|---|---|---|---|---:|---:|",
        ]
    )
    for a_key, a_row in area_impact.items():
        ph_repr = ", ".join(f"`Phase {p}`" for p in a_row["phases"])
        blks_repr = ", ".join(f"`{b}`" for b in a_row["blockers"])
        lines.append(
            f"| `{a_key}` | {a_row['description']} | {ph_repr} | {blks_repr} | "
            f"{len(a_row['probes'])} | {len(a_row['programs'])} |"
        )

    promo_contracts = summarize_unimplemented_promotion_contracts(expectations)
    lines.extend(
        [
            "",
            f"### 3.4 {len(promo_contracts)} 个 `known_unimplemented` Probe 晋升契约与目标输出（`target_stdout`）对账表",
            "",
            "| Probe | Guide 章节 | P2 分组 | 基线记号 | 允许记号 (`allowed_markers`) | 路线图阻塞项 | 目标输出 (`target_stdout`) | 规模 |",
            "|---|---|:---:|---|---|---|---|---:|",
        ]
    )
    for p_row in promo_contracts:
        ch = p_row["guide_chapter"]
        allowed_s = ", ".join(f"`{m}`" for m in p_row["allowed_markers"])
        blks_s = ", ".join(f"`{b}`" for b in p_row["roadmap_blockers"])
        escaped_out = p_row["target_stdout"].encode("unicode_escape").decode("ascii")
        lines.append(
            f"| `{p_row['name']}` | [{ch}](../docs/guide/{ch}) | `{p_row['foundational_group']}` | "
            f"`{p_row['baseline_marker']}` | {allowed_s} | {blks_s} | `{escaped_out}` | "
            f"{p_row['expected_lines']}L / {p_row['expected_bytes']}B |"
        )

    phase_matrix = summarize_compiler_phase_matrix(expectations, roadmap_items)
    lines.extend(
        [
            "",
            "### 3.5 编译器阶段（`Phase 2` – `Phase 5`）负向契约与未实现阻塞全景矩阵",
            "",
            "| 编译器阶段 | 阶段职责范围 | 已生效负向 Probe 数 | 覆盖负向契约 (`FE:Lxxxx` / `abort`) | 关联路线图阻塞项 | 阻塞 Probe 数 | 阻塞 Program 数 |",
            "|---|---|---:|---|---|---:|---:|",
        ]
    )
    for ph_id, ph_row in phase_matrix.items():
        neg_c = (
            ", ".join(f"`{c}`" for c in ph_row["active_negative_contracts"])
            if ph_row["active_negative_contracts"]
            else "-"
        )
        blks_c = ", ".join(f"`{b}`" for b in ph_row["blockers"]) if ph_row["blockers"] else "-"
        lines.append(
            f"| **{ph_row['title']}** | {ph_row['description']} | {len(ph_row['active_negative_probes'])} | "
            f"{neg_c} | {blks_c} | {len(ph_row['blocked_probes'])} | {len(ph_row['blocked_programs'])} |"
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
        "--batch",
        action="append",
        dest="batches",
        help="run only probes in selected batch prefix(es) p/q/r/s/t/u/v/w/x/y/e (repeatable)",
    )
    parser.add_argument(
        "--chapter",
        action="append",
        dest="chapters",
        help="run only probes in selected Guide chapter filename or 01..13 prefix (repeatable)",
    )
    parser.add_argument(
        "--program",
        action="append",
        dest="programs",
        help="run only selected acceptance program(s) (repeatable)",
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
    parser.add_argument(
        "--verify-evidence",
        type=Path,
        default=None,
        help="verify an existing evidence bundle directory against expectations and optional baseline without running compiler",
    )
    parser.add_argument(
        "--compare-evidence",
        nargs=2,
        type=Path,
        metavar=("EVIDENCE_A", "EVIDENCE_B"),
        default=None,
        help="verify two evidence bundle directories and assert cross-host parity across all probes and programs",
    )
    parser.add_argument(
        "--check-matrix",
        type=Path,
        default=None,
        help="compare a 3-column TSV matrix against expectations.json and optional baseline without running compiler",
    )
    args = parser.parse_args(argv)

    if args.init_expectations:
        baseline_file = args.baseline or (args.corpus / "baseline-aef3f4f8.tsv")
        try:
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
                readme_path = args.corpus / "README.md"
                if readme_path.is_file():
                    readme_path.write_text(
                        sync_readme_chapter_table(
                            readme_path.read_text(encoding="utf-8"), derived
                        ),
                        encoding="utf-8",
                    )
        except (ValueError, OSError) as exc:
            # CLI 边界：把推导或写入错误（命名违规、baseline 缺项、README 表缺失等）渲染为单行诊断，不暴露 traceback
            print(f"conformance: error: {exc}", file=sys.stderr)
            return 1
        print(
            f"conformance: initialized {len(derived['probes'])} probes and "
            f"{len(derived['programs'])} programs in {args.expectations}"
        )
        return 0

    if args.verify_evidence is not None:
        try:
            verified = verify_evidence_bundle(
                evidence_dir=args.verify_evidence,
                expectations_path=args.expectations,
                corpus_root=args.corpus,
                baseline_path=args.baseline,
                strict_unreviewed=not args.allow_unreviewed,
            )
        except (ValueError, OSError) as exc:
            print(f"conformance: error: {exc}", file=sys.stderr)
            return 1
        v_sum = verified["summary"]
        print(
            f"conformance: verified evidence bundle {args.verify_evidence} "
            f"probes_cap_passed={v_sum['probe_capability_passed']}/{v_sum['total_probes']} "
            f"programs_passed={v_sum['program_run_passed']}/{v_sum['total_programs']}"
        )
        return 0 if v_sum["gate_passed"] else 1

    if args.compare_evidence is not None:
        dir_a, dir_b = args.compare_evidence
        try:
            parity = compare_dual_host_evidence(
                evidence_dir_a=dir_a,
                evidence_dir_b=dir_b,
                expectations_path=args.expectations,
                corpus_root=args.corpus,
                baseline_path=args.baseline,
                strict_unreviewed=not args.allow_unreviewed,
            )
        except (ValueError, OSError) as exc:
            print(f"conformance: error: {exc}", file=sys.stderr)
            return 1
        print(
            f"conformance: dual-host parity verified commit={parity['commit']} "
            f"hosts={parity['host_a']['system']}/{parity['host_b']['system']} "
            f"probes_cap_passed={parity['probe_capability_passed']}/{parity['total_probes']} "
            f"programs_passed={parity['program_run_passed']}/{parity['total_programs']}"
        )
        return 0 if parity["gate_passed"] else 1

    if args.check_matrix is not None:
        try:
            m_res = compare_tsv_matrix_against_expectations(
                matrix_path=args.check_matrix,
                expectations_path=args.expectations,
                corpus_root=args.corpus,
                baseline_path=args.baseline,
            )
        except (ValueError, OSError) as exc:
            print(f"conformance: error: {exc}", file=sys.stderr)
            return 1
        print(
            f"conformance: checked matrix {args.check_matrix} "
            f"probes_cap_passed={m_res['probe_capability_passed']}/{m_res['total_probes']} "
            f"counts={json.dumps(m_res['counts'], sort_keys=True)}"
        )
        if not m_res["gate_passed"]:
            for failure in m_res["failures"]:
                print(f"FAIL: {failure}", file=sys.stderr)
            return 1
        return 0

    try:
        ledger = evaluate_suite(
            kovenc=args.cli,
            corpus_root=args.corpus,
            expectations_path=args.expectations,
            baseline_path=args.baseline,
            selected_probes=args.probes,
            selected_batches=args.batches,
            selected_chapters=args.chapters,
            selected_programs=args.programs,
            include_programs=not args.no_programs,
            timeout_sec=args.timeout,
            strict_unreviewed=not args.allow_unreviewed,
            compiler_commit=args.compiler_commit,
            evidence_dir=args.evidence,
            jobs=args.jobs,
        )
    except (ValueError, OSError) as exc:
        print(f"conformance: error: {exc}", file=sys.stderr)
        return 1
    summary = ledger["summary"]
    print(
        f"conformance: probes_cap_passed={summary['probe_capability_passed']}/{summary['total_probes']} "
        f"programs_passed={summary['program_run_passed']}/{summary['total_programs']} "
        f"counts={json.dumps(summary['counts'], sort_keys=True)}"
    )
    if not summary["gate_passed"]:
        for up_name in summary.get("unexpected_passes", []):
            print(
                f"PROMOTE: {up_name} exited 0 with exact target_stdout in both modes; "
                f"update V046_PROBE_OVERRIDES and run --init-expectations",
                file=sys.stderr,
            )
        for failure in summary["failures"]:
            print(f"FAIL: {failure}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
