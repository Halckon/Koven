#!/usr/bin/env python3
"""Static normative catalogs, diagnostic/marker registries, and probe classification rules."""

from __future__ import annotations

import re
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


PROGRAM_README_CATALOG: dict[str, tuple[str, str]] = {
    "word_count": ("词频", "`MutableMap` 插入序遍历、`String.clone()`、`in` 包含检查"),
    "config_parse": ("配置解析", "逐字符解析 `key = value`、注释与空行、`Display` 实现、`Result` 与 `?`"),
    "expr_eval": ("表达式求值", "递归下降解析、`Box` 递归 enum、`Result` 传播与除零报错"),
    "raii_state_machine": ("RAII 状态机", "`deinit` 资源、`replace`、词法作用域逆序析构、enum 状态迁移"),
    "varint_codec": ("二进制编解码", "ZigZag + LEB128、具名位运算、`UByte` 转换、截断与超长报错"),
    "linked_stack": ("链表/栈", "拥有所有权的 class 节点、`replace` 接合、原地反转、`Box` cons 列表"),
}

GUIDE_SYNTAX_INDEX_DOMAINS: dict[str, tuple[int, list[str], str]] = {
    "词法": (8, ["01-lexical.md"], "关键字、标识符、trivia/换行、字面量、标点与词法恢复"),
    "文件与名称": (3, ["02-names-files-packages.md"], "完整文件与声明分隔、`package`/`import` 文件头、跨文件限定名称"),
    "类型": (1, ["03-types-generics.md"], "`type_ref`、泛型实参与函数类型语法"),
    "表达式": (3, ["04-expressions-operators.md"], "primary/bound reference、postfix/调用/索引、运算符优先级与结合性"),
    "声明": (3, ["05-declarations-callables.md", "06-blocks-control-flow.md"], "`val`/`var`/`const val`/`fun`、callable 参数 marker 与函数 body 三形态"),
    "控制流": (2, ["06-blocks-control-flow.md"], "block/block element 与 `if`/`when`/`while`/`for`/`loop`/jump"),
    "调用与 Lambda": (4, ["07-calls-lambdas-closures.md"], "lambda literal、尾 lambda、隐式 `it` 与自动 Borrow/`&` 调用实参"),
    "类型声明": (3, ["08-class-family-members.md"], "class/value/interface/enum/object/companion、receiver marker 与 `by` 委托"),
    "空安全与错误": (2, ["09-nullability-errors.md"], "`?:`/`!!`/nullable/`error()` 与 postfix `?`/`Result<T, E>`"),
    "所有权": (1, ["10-ownership-borrowing-drop.md"], "Value/Borrow/Inout、loan 与调用期借用/ASAP 析构"),
    "普通借用结果": (1, ["10-ownership-borrowing-drop.md"], "`: borrow T from source` 与 `borrow val` 显式局部绑定"),
    "构造": (1, ["11-copyability-layout-construction.md"], "class/value/enum 名义值与 Intrinsic `Box` 构造"),
    "集合与解构": (2, ["12-collections-destructuring.md"], "顺序容器构造/索引/element place 与局部 `val` 解构语法"),
    "程序入口": (2, ["13-program-runtime-standard-library.md"], "conventional `main` 与 project selector / process entry"),
}
