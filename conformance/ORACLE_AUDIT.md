# Conformance 强 Oracle 语义推导与 Guide v0.46 章节覆盖对账账本

> 本文件由 `python3 scripts/conformance_gate.py --init-expectations` 从 `conformance/expectations.json` 确定性生成，并由 `scripts/check_docs.py`（`DocsChecker.check_conformance_corpus`）与 `scripts/tests/test_conformance_gate.py` 自动校验同步。

## 1. Guide v0.46 章节覆盖汇总（154 Probes）

| Guide v0.46 章节 | Probe 总数 | `run_ok` | `compile_fail` | `runtime_abort` | `known_unimplemented` | 能力通过 (`capability_passed`) |
|---|---:|---:|---:|---:|---:|---:|
| [01-lexical.md](../docs/guide/01-lexical.md) | 4 | 1 | 0 | 0 | 3 | 1 / 4 |
| [02-names-files-packages.md](../docs/guide/02-names-files-packages.md) | 7 | 2 | 3 | 0 | 2 | 5 / 7 |
| [03-types-generics.md](../docs/guide/03-types-generics.md) | 11 | 5 | 1 | 1 | 4 | 7 / 11 |
| [04-expressions-operators.md](../docs/guide/04-expressions-operators.md) | 5 | 3 | 1 | 1 | 0 | 5 / 5 |
| [05-declarations-callables.md](../docs/guide/05-declarations-callables.md) | 4 | 2 | 2 | 0 | 0 | 4 / 4 |
| [06-blocks-control-flow.md](../docs/guide/06-blocks-control-flow.md) | 19 | 15 | 2 | 0 | 2 | 17 / 19 |
| [07-calls-lambdas-closures.md](../docs/guide/07-calls-lambdas-closures.md) | 8 | 2 | 1 | 0 | 5 | 3 / 8 |
| [08-class-family-members.md](../docs/guide/08-class-family-members.md) | 16 | 14 | 0 | 0 | 2 | 14 / 16 |
| [09-nullability-errors.md](../docs/guide/09-nullability-errors.md) | 16 | 11 | 1 | 1 | 3 | 13 / 16 |
| [10-ownership-borrowing-drop.md](../docs/guide/10-ownership-borrowing-drop.md) | 9 | 4 | 2 | 0 | 3 | 6 / 9 |
| [11-copyability-layout-construction.md](../docs/guide/11-copyability-layout-construction.md) | 3 | 2 | 0 | 0 | 1 | 2 / 3 |
| [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | 48 | 47 | 1 | 0 | 0 | 48 / 48 |
| [13-program-runtime-standard-library.md](../docs/guide/13-program-runtime-standard-library.md) | 4 | 2 | 0 | 0 | 2 | 2 / 4 |
| **合计** | **154** | **110** | **14** | **3** | **27** | **127 / 154** |

## 2. 六个 V1 验收程序（Acceptance Programs）

| 程序 | 归属章节 | 当前状态 | 期望输出文件 | 允许的未实现记号 |
|---|---|---|---|---|
| `config_parse` | [09-nullability-errors.md](../docs/guide/09-nullability-errors.md) | `known_unimplemented` | [programs/config_parse/expected.txt](programs/config_parse/expected.txt) | `FE:L0080`, `FE:L0084`, `FE:L0085`, `FE:L0165`, `BE:UnsupportedNode`, `BE:MissingFact` |
| `expr_eval` | [11-copyability-layout-construction.md](../docs/guide/11-copyability-layout-construction.md) | `known_unimplemented` | [programs/expr_eval/expected.txt](programs/expr_eval/expected.txt) | `FE:L0080`, `FE:L0084`, `FE:L0085`, `FE:L0165`, `BE:UnsupportedNode`, `BE:MissingFact` |
| `linked_stack` | [10-ownership-borrowing-drop.md](../docs/guide/10-ownership-borrowing-drop.md) | `known_unimplemented` | [programs/linked_stack/expected.txt](programs/linked_stack/expected.txt) | `FE:L0080`, `FE:L0084`, `FE:L0085`, `FE:L0165`, `BE:UnsupportedNode`, `BE:MissingFact` |
| `raii_state_machine` | [08-class-family-members.md](../docs/guide/08-class-family-members.md) | `known_unimplemented` | [programs/raii_state_machine/expected.txt](programs/raii_state_machine/expected.txt) | `FE:L0080`, `FE:L0084`, `FE:L0085`, `FE:L0165`, `BE:UnsupportedNode`, `BE:MissingFact` |
| `varint_codec` | [03-types-generics.md](../docs/guide/03-types-generics.md) | `known_unimplemented` | [programs/varint_codec/expected.txt](programs/varint_codec/expected.txt) | `FE:L0080`, `FE:L0084`, `FE:L0085`, `FE:L0165`, `BE:UnsupportedNode`, `BE:MissingFact` |
| `word_count` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `known_unimplemented` | [programs/word_count/expected.txt](programs/word_count/expected.txt) | `FE:L0080`, `FE:L0084`, `FE:L0085`, `FE:L0165`, `BE:UnsupportedNode`, `BE:MissingFact` |

## 3. 154 个 Probe 逐项强 Oracle 与语义推导账本

| Probe | Guide 章节 | 类型 (`kind`) | 溯源 (`oracle_origin`) | 期望结果 / 诊断码 | 语义推导 / 规则说明 (`oracle_derivation` / `note`) |
|---|---|---|---|---|---|
| `e01_enum_when_bool_return` | [06-blocks-control-flow.md](../docs/guide/06-blocks-control-flow.md) | `run_ok` | `v046_spec_contract` | `e1\n` | isRed(Color.Red) matches branch Color.Red -> true; if-branch prints 'e1' |
| `e02_enum_when_stmt_println` | [06-blocks-control-flow.md](../docs/guide/06-blocks-control-flow.md) | `run_ok` | `v046_spec_contract` | `e2\n` | c = Color.Red matches statement when branch Color.Red -> println('e2') |
| `e03_enum_when_bool_val` | [06-blocks-control-flow.md](../docs/guide/06-blocks-control-flow.md) | `run_ok` | `v046_spec_contract` | `e5\n` | c = Color.Green yields b = false in when expression; if (!b) prints 'e5' |
| `e04_enum_when_int_return` | [06-blocks-control-flow.md](../docs/guide/06-blocks-control-flow.md) | `run_ok` | `single_println_literal` | `e6\n` | single literal println('e6') |
| `p01_int_print` | [05-declarations-callables.md](../docs/guide/05-declarations-callables.md) | `compile_fail` | `baseline_diagnostic_audited` | `FE:L0084` | println expects String parameter; passing Int (40 + 2) emits TYPE_MISMATCH L0084 |
| `p02_interp` | [01-lexical.md](../docs/guide/01-lexical.md) | `known_unimplemented` | `baseline_unimplemented_audited` | `BE:UnsupportedNode`, `BE:MissingFact`, `FE:L0165` | - |
| `p03_long_uint` | [01-lexical.md](../docs/guide/01-lexical.md) | `run_ok` | `single_println_literal` | `widths\n` | single literal println('widths') |
| `p03b_byte_literal_cmp` | [03-types-generics.md](../docs/guide/03-types-generics.md) | `compile_fail` | `baseline_diagnostic_audited` | `FE:L0085` | comparing Byte with untyped Int literal 3 without conversion emits INVALID_OPERAND_TYPES L0085 |
| `p04_overflow` | [04-expressions-operators.md](../docs/guide/04-expressions-operators.md) | `runtime_abort` | `v046_spec_contract` | `abort:integer overflow in +` | Int.MAX_VALUE (2147483647) + 1 triggers checked signed addition overflow abort per Guide 04/09 |
| `p05_double` | [01-lexical.md](../docs/guide/01-lexical.md) | `known_unimplemented` | `baseline_unimplemented_audited` | `BE:UnsupportedNode`, `BE:MissingFact`, `FE:L0165` | - |
| `p06_char` | [01-lexical.md](../docs/guide/01-lexical.md) | `known_unimplemented` | `baseline_unimplemented_audited` | `BE:UnsupportedNode`, `BE:MissingFact`, `FE:L0165` | - |
| `p07_while_var` | [06-blocks-control-flow.md](../docs/guide/06-blocks-control-flow.md) | `run_ok` | `source_trace_audited` | `while\n` | while loop sums i=0..9 into s=45; s == 45 holds and prints 'while' |
| `p08_when_int` | [06-blocks-control-flow.md](../docs/guide/06-blocks-control-flow.md) | `run_ok` | `v046_spec_contract` | `two\n` | SPEC-0356: subject when (Int subject equality) in E3 refinement layer |
| `p09_enum_payload` | [06-blocks-control-flow.md](../docs/guide/06-blocks-control-flow.md) | `run_ok` | `single_println_literal` | `enum\n` | single literal println('enum') |
| `p10_generic_fn` | [03-types-generics.md](../docs/guide/03-types-generics.md) | `run_ok` | `source_trace_audited` | `generic\nint\n` | identity('generic') prints 'generic'; identity(3) == 3 holds and prints 'int' |
| `p11_interface_bound` | [03-types-generics.md](../docs/guide/03-types-generics.md) | `known_unimplemented` | `baseline_unimplemented_audited` | `FE:L0165`, `BE:UnsupportedNode`, `BE:MissingFact` | - |
| `p12_class_inout` | [08-class-family-members.md](../docs/guide/08-class-family-members.md) | `run_ok` | `single_println_literal` | `counter\n` | single literal println('counter') |
| `p12b_class_field_assign` | [08-class-family-members.md](../docs/guide/08-class-family-members.md) | `known_unimplemented` | `baseline_unimplemented_audited` | `BE:UnsupportedNode`, `BE:MissingFact`, `FE:L0165` | - |
| `p12c_inout_param` | [10-ownership-borrowing-drop.md](../docs/guide/10-ownership-borrowing-drop.md) | `known_unimplemented` | `baseline_unimplemented_audited` | `BE:UnsupportedNode`, `BE:MissingFact`, `FE:L0165` | - |
| `p13_value_class_copy` | [11-copyability-layout-construction.md](../docs/guide/11-copyability-layout-construction.md) | `run_ok` | `single_println_literal` | `value\n` | single literal println('value') |
| `p14_nullable` | [09-nullability-errors.md](../docs/guide/09-nullability-errors.md) | `run_ok` | `v046_spec_contract` | `smart\nbangbang\n` | pick(true) returns non-null Node(5); smart cast prints 'smart' and pick(true)!!.v == 5 prints 'bangbang' |
| `p14b_nullable_simple` | [09-nullability-errors.md](../docs/guide/09-nullability-errors.md) | `run_ok` | `v046_spec_contract` | `smart\n` | a: Node? = Node(5) is non-null with a.v == 5; smart cast branch prints 'smart' |
| `p14c_nullable_int` | [09-nullability-errors.md](../docs/guide/09-nullability-errors.md) | `run_ok` | `v046_spec_contract` | `nint\n` | a: Int? = 5 is non-null and equals 5; smart cast branch prints 'nint' |
| `p15_elvis` | [09-nullability-errors.md](../docs/guide/09-nullability-errors.md) | `known_unimplemented` | `baseline_unimplemented_audited` | `BE:UnsupportedNode`, `BE:MissingFact`, `FE:L0165` | - |
| `p16_result_q` | [09-nullability-errors.md](../docs/guide/09-nullability-errors.md) | `known_unimplemented` | `v046_spec_contract` | `FE:L0165`, `BE:UnsupportedNode`, `BE:MissingFact` | single literal println('question') |
| `p16b_result_string` | [09-nullability-errors.md](../docs/guide/09-nullability-errors.md) | `known_unimplemented` | `v046_spec_contract` | `BE:UnsupportedNode`, `BE:MissingFact`, `FE:L0165` | Guide v0.46 E4 (SPEC-0349): Result in builtin env; String payload lowering pending N08 |
| `p17_lambda` | [07-calls-lambdas-closures.md](../docs/guide/07-calls-lambdas-closures.md) | `known_unimplemented` | `baseline_unimplemented_audited` | `BE:UnsupportedNode`, `BE:MissingFact`, `FE:L0165` | - |
| `p17b_lambda_nocapture` | [07-calls-lambdas-closures.md](../docs/guide/07-calls-lambdas-closures.md) | `known_unimplemented` | `baseline_unimplemented_audited` | `BE:UnsupportedNode`, `BE:MissingFact`, `FE:L0165` | - |
| `p18_move_closure` | [07-calls-lambdas-closures.md](../docs/guide/07-calls-lambdas-closures.md) | `known_unimplemented` | `baseline_unimplemented_audited` | `BE:UnsupportedNode`, `BE:MissingFact`, `FE:L0165` | - |
| `p19_mutablelist` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `source_trace_audited` | `list\n` | xs=[10,2,3] sums to 15 with size 3; condition holds and prints 'list' |
| `p20_map` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `single_println_literal` | `map\n` | single literal println('map') |
| `p21_box_enum` | [11-copyability-layout-construction.md](../docs/guide/11-copyability-layout-construction.md) | `known_unimplemented` | `baseline_unimplemented_audited` | `FE:L0165`, `BE:UnsupportedNode`, `BE:MissingFact` | - |
| `p22_rc` | [13-program-runtime-standard-library.md](../docs/guide/13-program-runtime-standard-library.md) | `known_unimplemented` | `baseline_unimplemented_audited` | `BE:UnsupportedNode`, `BE:MissingFact`, `FE:L0165` | - |
| `p23_global_var` | [05-declarations-callables.md](../docs/guide/05-declarations-callables.md) | `compile_fail` | `v046_spec_contract` | `FE:L0167` | top-level runtime var counter violates Guide 05 D4 (SPEC-0343); rejected with UNSUPPORTED_TOP_LEVEL_RUNTIME_VALUE L0167 |
| `p24_recursion` | [05-declarations-callables.md](../docs/guide/05-declarations-callables.md) | `run_ok` | `single_println_literal` | `fib\n` | single literal println('fib') |
| `p25_linked_iter` | [10-ownership-borrowing-drop.md](../docs/guide/10-ownership-borrowing-drop.md) | `compile_fail` | `baseline_diagnostic_audited` | `FE:L0133`, `FE:L0132` | cur = cur.next moves non-Copyable Node? field out of borrowed/owned chain, emitting MOVE_FROM_BORROWED_BINDING L0133 and PARTIAL_MOVE L0132 |
| `p25b_linked_recursive` | [10-ownership-borrowing-drop.md](../docs/guide/10-ownership-borrowing-drop.md) | `known_unimplemented` | `baseline_unimplemented_audited` | `BE:UnsupportedNode`, `BE:MissingFact`, `FE:L0165` | - |
| `p26_borrow_return` | [10-ownership-borrowing-drop.md](../docs/guide/10-ownership-borrowing-drop.md) | `run_ok` | `source_trace_audited` | `borrowed\n` | nameOf borrows Holder('borrowed').name; println(n) prints 'borrowed' |
| `p27_thread` | [13-program-runtime-standard-library.md](../docs/guide/13-program-runtime-standard-library.md) | `known_unimplemented` | `baseline_unimplemented_audited` | `FE:L0080`, `BE:UnsupportedNode`, `BE:MissingFact` | - |
| `p28_companion` | [08-class-family-members.md](../docs/guide/08-class-family-members.md) | `known_unimplemented` | `v046_spec_contract` | `BE:UnsupportedNode`, `BE:UnsupportedSource`, `BE:MissingFact`, `FE:L0165` | single literal println('companion') |
| `p29_default_super` | [08-class-family-members.md](../docs/guide/08-class-family-members.md) | `run_ok` | `source_trace_audited` | `ab\n` | C.hi() concatenates super<A>.hi() ('a') + super<B>.hi() ('b') = 'ab' |
| `p30_runtime_array` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `single_println_literal` | `array\n` | single literal println('array') |
| `p31_string_ops` | [13-program-runtime-standard-library.md](../docs/guide/13-program-runtime-standard-library.md) | `run_ok` | `source_trace_audited` | `abc\n` | 'ab'.clone() + 'c' equals 'abc'; branch prints 'abc' |
| `p32_deinit` | [08-class-family-members.md](../docs/guide/08-class-family-members.md) | `run_ok` | `source_trace_audited` | `body\nb\na\n` | main prints 'body' first, then drops locals in reverse lexical order: b ('b') then a ('a') |
| `p33_range_in` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `single_println_literal` | `in-range\n` | single literal println('in-range') |
| `p34_for_range` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `source_trace_audited` | `for-range\n` | 0..<5 sums 0+1+2+3+4 = 10; s == 10 holds and prints 'for-range' |
| `p35_value_eq` | [03-types-generics.md](../docs/guide/03-types-generics.md) | `known_unimplemented` | `baseline_unimplemented_audited` | `BE:UnsupportedNode`, `BE:MissingFact`, `FE:L0165` | - |
| `p36_conversion` | [03-types-generics.md](../docs/guide/03-types-generics.md) | `run_ok` | `single_println_literal` | `conv\n` | single literal println('conv') |
| `p37_is_generic_case` | [03-types-generics.md](../docs/guide/03-types-generics.md) | `run_ok` | `v046_spec_contract` | `case-noargs\n` | Result.Ok(1) matches parameterless is Result.Ok with v = 1 (SPEC-0349); prints 'case-noargs' |
| `p38_list_of_strings` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `source_trace_audited` | `x\ny\n` | iterating mutableListOf('x', 'y') prints 'x' then 'y' in insertion order |
| `p39_removeAt` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `single_println_literal` | `removeAt\n` | single literal println('removeAt') |
| `p40_nested_generic_class` | [03-types-generics.md](../docs/guide/03-types-generics.md) | `known_unimplemented` | `baseline_unimplemented_audited` | `BE:UnsupportedNode`, `BE:MissingFact`, `FE:L0165` | - |
| `p41_when_string` | [06-blocks-control-flow.md](../docs/guide/06-blocks-control-flow.md) | `known_unimplemented` | `baseline_unimplemented_audited` | `BE:UnsupportedNode`, `BE:MissingFact`, `FE:L0165` | - |
| `p42_lambda_mutate` | [07-calls-lambdas-closures.md](../docs/guide/07-calls-lambdas-closures.md) | `compile_fail` | `baseline_diagnostic_audited` | `FE:L0135` | mutating captured var sum inside closure without v0.46 mutating-closure lowering emits LOAN_CONFLICT L0135 |
| `p43_if_join_any` | [06-blocks-control-flow.md](../docs/guide/06-blocks-control-flow.md) | `compile_fail` | `baseline_diagnostic_audited` | `FE:L0112` | when expression mixing Int (1) and String ('s') branches is rejected with WHEN_BRANCH_TYPE L0112 (D3) |
| `p44_method_on_value` | [08-class-family-members.md](../docs/guide/08-class-family-members.md) | `run_ok` | `single_println_literal` | `vmethod\n` | single literal println('vmethod') |
| `p44b_method_on_class` | [08-class-family-members.md](../docs/guide/08-class-family-members.md) | `run_ok` | `single_println_literal` | `cmethod\n` | single literal println('cmethod') |
| `p45_abort_message` | [09-nullability-errors.md](../docs/guide/09-nullability-errors.md) | `runtime_abort` | `v046_spec_contract` | `abort:boom` | error('boom') aborts with SIGABRT and stderr header 'abort: boom' per Guide 09 D12 |
| `p46_string_field_class` | [08-class-family-members.md](../docs/guide/08-class-family-members.md) | `run_ok` | `source_trace_audited` | `ann\n` | Person('ann', 3).name is 'ann'; describe(p) prints 'ann' |
| `p47_list_index_string` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `source_trace_audited` | `b\n` | listOf('a', 'b')[1] evaluates to 'b' and prints 'b' |
| `p48_nested_loops_break` | [06-blocks-control-flow.md](../docs/guide/06-blocks-control-flow.md) | `run_ok` | `source_trace_audited` | `nested\n` | outer loop runs 5 times, inner loop breaks at j==3 after 3 increments -> hits == 15; prints 'nested' |
| `p49_when_enum_no_payload` | [06-blocks-control-flow.md](../docs/guide/06-blocks-control-flow.md) | `run_ok` | `v046_spec_contract` | `green\n` | name(Color.Green) selects Color.Green -> 'green' and prints 'green' |
| `p50_enum_method` | [08-class-family-members.md](../docs/guide/08-class-family-members.md) | `run_ok` | `v046_spec_contract` | `enum-method\n` | single literal println('enum-method') |
| `q01_lambda_local` | [07-calls-lambdas-closures.md](../docs/guide/07-calls-lambdas-closures.md) | `run_ok` | `single_println_literal` | `lambda-local\n` | single literal println('lambda-local') |
| `q02_lambda_arg_inline` | [07-calls-lambdas-closures.md](../docs/guide/07-calls-lambdas-closures.md) | `known_unimplemented` | `baseline_unimplemented_audited` | `BE:UnsupportedNode`, `BE:MissingFact`, `FE:L0165` | - |
| `q03_fn_ref` | [07-calls-lambdas-closures.md](../docs/guide/07-calls-lambdas-closures.md) | `known_unimplemented` | `baseline_unimplemented_audited` | `FE:L0165`, `BE:UnsupportedNode`, `BE:MissingFact` | - |
| `q04_rc_simple` | [13-program-runtime-standard-library.md](../docs/guide/13-program-runtime-standard-library.md) | `run_ok` | `single_println_literal` | `rc\n` | single literal println('rc') |
| `q05_nullable_param` | [09-nullability-errors.md](../docs/guide/09-nullability-errors.md) | `run_ok` | `v046_spec_contract` | `non-null\nnull\n` | show(Node(1)) takes non-null branch ('non-null'); show(null) takes else branch ('null') |
| `q06_nullable_return_bang` | [09-nullability-errors.md](../docs/guide/09-nullability-errors.md) | `run_ok` | `single_println_literal` | `bang\n` | single literal println('bang') |
| `q07_result_when_int` | [09-nullability-errors.md](../docs/guide/09-nullability-errors.md) | `run_ok` | `single_println_literal` | `res-int\n` | single literal println('res-int') |
| `q08_enum_string_payload` | [06-blocks-control-flow.md](../docs/guide/06-blocks-control-flow.md) | `known_unimplemented` | `baseline_unimplemented_audited` | `BE:UnsupportedNode`, `BE:MissingFact`, `FE:L0165` | - |
| `q09_inout_class_param` | [10-ownership-borrowing-drop.md](../docs/guide/10-ownership-borrowing-drop.md) | `known_unimplemented` | `baseline_unimplemented_audited` | `BE:UnsupportedNode`, `BE:MissingFact`, `FE:L0165` | - |
| `q10_var_reassign_string` | [04-expressions-operators.md](../docs/guide/04-expressions-operators.md) | `run_ok` | `source_trace_audited` | `abc\n` | s starts as 'a', appends 'b' then 'c' -> 'abc' |
| `q11_while_list_index` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `source_trace_audited` | `index-loop\n` | while loop sums listOf(1,2,3) by index -> s == 6; prints 'index-loop' |
| `q12_companion_const` | [05-declarations-callables.md](../docs/guide/05-declarations-callables.md) | `run_ok` | `v046_spec_contract` | `cconst\n` | Cfg.N reads associated const val 3 from anonymous companion { }; Cfg.N == 3 prints 'cconst' |
| `q13_if_expr_value` | [06-blocks-control-flow.md](../docs/guide/06-blocks-control-flow.md) | `run_ok` | `source_trace_audited` | `big\n` | x = 3 satisfies x > 2, so if-expression evaluates to 'big' |
| `q14_when_subjectless` | [06-blocks-control-flow.md](../docs/guide/06-blocks-control-flow.md) | `run_ok` | `source_trace_audited` | `mid\n` | x = 5 fails x < 3 and matches second guard x < 7 -> 'mid' |
| `q15_borrow_param_string_list` | [10-ownership-borrowing-drop.md](../docs/guide/10-ownership-borrowing-drop.md) | `run_ok` | `source_trace_audited` | `borrow-list\n` | total(listOf('a', 'b')) counts 2 elements; 2 == 2 prints 'borrow-list' |
| `q16_interface_static` | [08-class-family-members.md](../docs/guide/08-class-family-members.md) | `run_ok` | `source_trace_audited` | `dog\n` | Dog.name() returns 'dog' via static interface implementation dispatch |
| `q17_generic_class` | [10-ownership-borrowing-drop.md](../docs/guide/10-ownership-borrowing-drop.md) | `compile_fail` | `baseline_diagnostic_audited` | `FE:L0164` | borrow T from this on generic class Holder<T> hits UNSUPPORTED_BORROW_FLOW L0164 (roadmap N09) |
| `q18_value_class_generic` | [11-copyability-layout-construction.md](../docs/guide/11-copyability-layout-construction.md) | `run_ok` | `single_println_literal` | `vgen\n` | single literal println('vgen') |
| `q19_deep_return_cleanup` | [08-class-family-members.md](../docs/guide/08-class-family-members.md) | `run_ok` | `source_trace_audited` | `b\na\na\n` | f(true) drops inner b ('b') then outer a ('a') on early return; f(false) drops a ('a') -> 'b\na\na\n' |
| `q20_replace_field` | [10-ownership-borrowing-drop.md](../docs/guide/10-ownership-borrowing-drop.md) | `run_ok` | `source_trace_audited` | `old\nnew\n` | replace(&b.s, 'new') returns previous field value 'old' and updates b.s to 'new' |
| `r01_bogus_member` | [02-names-files-packages.md](../docs/guide/02-names-files-packages.md) | `compile_fail` | `baseline_diagnostic_audited` | `FE:L0080` | calling nonexistent method Int.fooBar() emits UNRESOLVED_NAME L0080 |
| `r02_bogus_member_str` | [02-names-files-packages.md](../docs/guide/02-names-files-packages.md) | `compile_fail` | `baseline_diagnostic_audited` | `FE:L0080` | accessing nonexistent property String.length emits UNRESOLVED_NAME L0080 |
| `s01_result_builtin` | [02-names-files-packages.md](../docs/guide/02-names-files-packages.md) | `run_ok` | `v046_spec_contract` | `result-builtin\n` | builtin Result.Ok(1) is resolved without import (SPEC-0349); main prints 'result-builtin' |
| `s02_pair_builtin` | [02-names-files-packages.md](../docs/guide/02-names-files-packages.md) | `known_unimplemented` | `baseline_unimplemented_audited` | `FE:L0080`, `BE:UnsupportedNode`, `BE:MissingFact` | - |
| `s03_to_pair` | [02-names-files-packages.md](../docs/guide/02-names-files-packages.md) | `known_unimplemented` | `baseline_unimplemented_audited` | `FE:L0165`, `BE:UnsupportedNode`, `BE:MissingFact` | - |
| `s04_question_on_nullable` | [09-nullability-errors.md](../docs/guide/09-nullability-errors.md) | `compile_fail` | `baseline_diagnostic_audited` | `FE:L0085` | postfix ? applied to nullable Int? (instead of Result) emits INVALID_OPERAND_TYPES L0085 |
| `t01_vmethod_bare_field` | [08-class-family-members.md](../docs/guide/08-class-family-members.md) | `run_ok` | `single_println_literal` | `vmethod-bare\n` | single literal println('vmethod-bare') |
| `t02_vmethod_bool` | [08-class-family-members.md](../docs/guide/08-class-family-members.md) | `run_ok` | `single_println_literal` | `vmethod-bool\n` | single literal println('vmethod-bool') |
| `t03_vmethod_generic` | [03-types-generics.md](../docs/guide/03-types-generics.md) | `known_unimplemented` | `baseline_unimplemented_audited` | `BE:UnsupportedNode`, `BE:MissingFact`, `FE:L0165` | - |
| `t04_vmethod_compute_param` | [08-class-family-members.md](../docs/guide/08-class-family-members.md) | `run_ok` | `single_println_literal` | `vmethod-param\n` | single literal println('vmethod-param') |
| `t05_map_loop_continue` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `source_trace_audited` | `map-continue\n` | for-loop skips 'b' via continue and counts 'a' twice -> m.get('a') == 2 prints 'map-continue' |
| `t06_list_loop_return` | [06-blocks-control-flow.md](../docs/guide/06-blocks-control-flow.md) | `run_ok` | `source_trace_audited` | `loop-return\n` | find(listOf(1,2,3), 2) returns true on second element; prints 'loop-return' |
| `t07_short_circuit_plain` | [04-expressions-operators.md](../docs/guide/04-expressions-operators.md) | `run_ok` | `single_println_literal` | `short-circuit\n` | single literal println('short-circuit') |
| `t08_vmethod_string` | [08-class-family-members.md](../docs/guide/08-class-family-members.md) | `run_ok` | `source_trace_audited` | `pos\n` | Tag(1).label() checks 1 > 0 and returns 'pos' |
| `t09_vmethod_calls_method` | [08-class-family-members.md](../docs/guide/08-class-family-members.md) | `run_ok` | `single_println_literal` | `vmethod-chain\n` | single literal println('vmethod-chain') |
| `t10_map_nested_if_break` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `source_trace_audited` | `map-nested-break\n` | loop inserts 'a' and 'b' (m.size == 2), then breaks at 'c'; prints 'map-nested-break' |
| `u01_enum_case_borrow_arg` | [10-ownership-borrowing-drop.md](../docs/guide/10-ownership-borrowing-drop.md) | `run_ok` | `v046_spec_contract` | `enum-arg\n` | isRed(Color.Red) borrows enum case argument and matches Color.Red -> true; prints 'enum-arg' |
| `u02_list_add_loop_string` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `source_trace_audited` | `x\ny\n` | clones 'x' and 'y' into MutableList<String> and prints each element in order |
| `u03_list_of_class_add` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `single_println_literal` | `class-list\n` | single literal println('class-list') |
| `u04_enum_payload_arg_unit` | [06-blocks-control-flow.md](../docs/guide/06-blocks-control-flow.md) | `run_ok` | `single_println_literal` | `enum-payload-arg\n` | single literal println('enum-payload-arg') |
| `v01_assign_unit_value` | [04-expressions-operators.md](../docs/guide/04-expressions-operators.md) | `run_ok` | `single_println_literal` | `assign-unit\n` | single literal println('assign-unit') |
| `v02_assign_chain` | [04-expressions-operators.md](../docs/guide/04-expressions-operators.md) | `compile_fail` | `baseline_diagnostic_audited` | `FE:L0084` | assignment b = 3 yields Unit, so chaining a = (b = 3) for Int var a emits TYPE_MISMATCH L0084 (SPEC-0325) |
| `v03_join_nullable` | [09-nullability-errors.md](../docs/guide/09-nullability-errors.md) | `run_ok` | `v046_spec_contract` | `join-nullable\n` | pick(true) returns Int? = 7; smart cast x != null && x == 7 prints 'join-nullable' |
| `v04_stmt_if_mixed` | [06-blocks-control-flow.md](../docs/guide/06-blocks-control-flow.md) | `compile_fail` | `baseline_diagnostic_audited` | `FE:L0089` | if/else with incompatible Int and String branch types emits NO_COMMON_BRANCH_TYPE L0089 (SPEC-0326) |
| `v05_range_in` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `single_println_literal` | `range-in\n` | single literal println('range-in') |
| `v06_for_range_sum` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `source_trace_audited` | `for-range\n` | 0..<5 sums to 10 and 1..3 sums to 6 -> s == 16 prints 'for-range' |
| `v07_range_max` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `source_trace_audited` | `range-max\n` | inclusive range 2147483646..2147483647 iterates exactly 2 times without overflow; prints 'range-max' |
| `v08_long_range` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `source_trace_audited` | `long-range\n` | 1L..4L sums 1+2+3+4 = 10L; prints 'long-range' |
| `v09_reverse_empty` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `source_trace_audited` | `reverse-empty\n` | ascending range 5..1 is empty (0 iterations, n == 0); prints 'reverse-empty' |
| `v10_range_value` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `single_println_literal` | `range-value\n` | single literal println('range-value') |
| `v11_convert_ok` | [03-types-generics.md](../docs/guide/03-types-generics.md) | `run_ok` | `single_println_literal` | `convert\n` | single literal println('convert') |
| `v12_convert_abort` | [03-types-generics.md](../docs/guide/03-types-generics.md) | `runtime_abort` | `v046_spec_contract` | `abort:value out of range for ULong` | (-1).toULong() fails range check and aborts with 'abort: value out of range for ULong' (SPEC-0327/0355) |
| `v13_while_nullable_int` | [09-nullability-errors.md](../docs/guide/09-nullability-errors.md) | `run_ok` | `v046_spec_contract` | `while-nullable\n` | while (x != null) adds 5 to total and sets x = null; total == 5 prints 'while-nullable' |
| `v14_map_consume` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `single_println_literal` | `consume\n` | single literal println('consume') |
| `v15_map_for` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `source_trace_audited` | `b\na\n` | MutableMap preserves insertion order ('b' then 'a'); loop prints 'b' then 'a' |
| `v16_when_in_range` | [06-blocks-control-flow.md](../docs/guide/06-blocks-control-flow.md) | `run_ok` | `v046_spec_contract` | `small\n` | SPEC-0356: subject when (Int subject `in` Range) in E3 refinement layer |
| `v17_convert_in_loop` | [03-types-generics.md](../docs/guide/03-types-generics.md) | `run_ok` | `source_trace_audited` | `conv-loop\n` | 0..<3 converts 0,1,2 via toLong() and sums to 3L; prints 'conv-loop' |
| `v18_while_nullable_class` | [09-nullability-errors.md](../docs/guide/09-nullability-errors.md) | `run_ok` | `source_trace_audited` | `while-class\n` | while (n != null) reads n.v (4) and sets n = null; t == 4 prints 'while-class' |
| `v19_stmt_when_mixed` | [06-blocks-control-flow.md](../docs/guide/06-blocks-control-flow.md) | `run_ok` | `v046_spec_contract` | `stmt-when-mixed\n` | single literal println('stmt-when-mixed') |
| `v20_range_in_list` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `source_trace_audited` | `range-in-list\n` | 3 is neither in 1..2 nor in 5..6 (n == 0, size == 2); prints 'range-in-list' |
| `v21_for_range_return` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `source_trace_audited` | `range-return\n` | firstEven(5) returns 2 on i == 2 inside 1..5; prints 'range-return' |
| `v22_range_with_map` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `source_trace_audited` | `range-map\n` | populates m[i] = i*i for i in 0..<4; m.get(3) == 9 prints 'range-map' |
| `v23_nested_range_break` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `source_trace_audited` | `nested-range\n` | outer 0..<3 runs 3 times, inner breaks at j == 1 after 1 increment -> n == 3 prints 'nested-range' |
| `w01_map_for_sum` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `source_trace_audited` | `map-sum\n` | map entries ('b'->2, 'a'->1) sum to 3; prints 'map-sum' |
| `w02_map_for_order` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `source_trace_audited` | `map-order\n` | map iterates in insertion order ('b', 'a', 'c'); ok remains true and prints 'map-order' |
| `w03_map_overwrite_remove` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `source_trace_audited` | `map-reorder\n` | overwriting 'b'->5, removing 'a', and re-inserting 'a'->7 yields order ('b'->5, 'a'->7); prints 'map-reorder' |
| `w04_map_for_break` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `source_trace_audited` | `map-break\n` | iterating keys 0..4 breaks at k == 2 after counting 0 and 1 (seen == 2); prints 'map-break' |
| `w05_map_for_return` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `source_trace_audited` | `map-return\n` | find(m, 'b') returns value 2 from map loop; prints 'map-return' |
| `w06_map_mutate_in_loop` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `compile_fail` | `baseline_diagnostic_audited` | `FE:L0135` | calling m.put() while m is borrowed by for-loop iteration emits LOAN_CONFLICT L0135 |
| `w07_map_nested` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `source_trace_audited` | `map-nested\n` | nested iteration over {1:1, 2:2} computes (1+2)*(1+2) = 9; prints 'map-nested' |
| `w08_map_string_values` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `source_trace_audited` | `two\none\n` | map entries (2->'two', 1->'one') print values 'two' then 'one' in insertion order |
| `w09_map_temp_source` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `source_trace_audited` | `map-temp\n` | iterating temporary make() map sums 3 + 4 = 7; prints 'map-temp' |
| `w10_map_for_continue_count` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `source_trace_audited` | `map-continue2\n` | map loop skips 'b'->2 via continue and sums 'a'->1 + 'c'->3 = 4; prints 'map-continue2' |
| `w11_word_count` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `source_trace_audited` | `word-count\n` | word frequency over ['x','y','x','z','x'] yields x=3, y=1, z=1; prints 'word-count' |
| `w12` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `source_trace_audited` | `b\nafter\n` | map loop prints key 'b', then post-loop s == 1 prints 'after' |
| `w13` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `source_trace_audited` | `seen\nend\n` | map loop with v == 2 prints 'seen', followed by post-loop 'end' |
| `w14` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `source_trace_audited` | `end\n` | map loop sets s = 1 without printing; post-loop prints 'end' |
| `w15` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `source_trace_audited` | `seen\nend\n` | map loop with v == 2 prints 'seen', followed by post-loop 'end' |
| `w16` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `source_trace_audited` | `borrow-param-sum\n` | count(m) borrows Map parameter and sums 2 + 3 = 5; prints 'borrow-param-sum' |
| `w17` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `source_trace_audited` | `b\nend\n` | map loop prints key 'b', then if (t == 1) prints 'end' |
| `w18` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `source_trace_audited` | `end\n` | map loop accumulates s = 2, then if (s == 2) prints 'end' |
| `w19` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `source_trace_audited` | `b\nend\n` | map loop prints key 'b', followed by post-loop 'end' |
| `x01_nullable_class_retest` | [09-nullability-errors.md](../docs/guide/09-nullability-errors.md) | `run_ok` | `single_println_literal` | `retest\n` | single literal println('retest') |
| `x02_string_nullable_return` | [09-nullability-errors.md](../docs/guide/09-nullability-errors.md) | `run_ok` | `v046_spec_contract` | `hit\n` | find(true) returns non-null String? 'hit'; smart cast s != null prints 'hit' |
| `x03_if_call_borrow_arg` | [07-calls-lambdas-closures.md](../docs/guide/07-calls-lambdas-closures.md) | `run_ok` | `source_trace_audited` | `obs\ndone\n` | if (true) calls observe('obs') printing 'obs', then prints 'done' |
| `y01_import_take_for_sum` | [02-names-files-packages.md](../docs/guide/02-names-files-packages.md) | `run_ok` | `v046_spec_contract` | `take\n` | take(listOf(1,2,3), 2) yields 1 and 2 (sum 3); prints 'take' |
| `y02_take_no_import` | [02-names-files-packages.md](../docs/guide/02-names-files-packages.md) | `compile_fail` | `baseline_diagnostic_audited` | `FE:L0080` | calling take() without import koven.algorithms.take emits UNRESOLVED_NAME L0080 (Guide v0.45) |
| `y03_method_take_for` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `v046_spec_contract` | `take-method\n` | xs.take(2) on listOf(1,2,3) sums 1 + 2 = 3; prints 'take-method' |
| `y04_take_for_println` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `v046_spec_contract` | `a\nb\n` | take(listOf('a','b','c'), 2) prints first two elements 'a' and 'b' |
| `y05_take_borrow_val_call` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `source_trace_audited` | `a\nb\n` | borrow val v = take(xs, 2) passed to read(v) prints 'a' and 'b' |
| `y06_take_borrow_val_for` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `source_trace_audited` | `a\nb\n` | borrow val v = take(xs, 2) iterated directly in for-loop prints 'a' and 'b' |
