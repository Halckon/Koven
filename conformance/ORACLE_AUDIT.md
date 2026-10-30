# Conformance 强 Oracle 语义推导与 Guide v0.46 章节覆盖对账账本

> 本文件由 `python3 scripts/conformance_gate.py --init-expectations` 从 `conformance/expectations.json` 确定性生成，并由 `scripts/check_docs.py`（`DocsChecker.check_conformance_corpus`）与 `scripts/tests/test_conformance_gate.py` 自动校验同步。

## 1. Guide v0.46 章节覆盖与正/负向极性汇总（154 Probes）

| Guide v0.46 章节 | Probe 总数 | `run_ok` | `compile_fail` | `runtime_abort` | `known_unimplemented` | 能力通过 (`capability_passed`) | 正/负向极性状态 (`polarity_status`) |
|---|---:|---:|---:|---:|---:|---:|---|
| [01-lexical.md](../docs/guide/01-lexical.md) | 4 | 1 | 0 | 0 | 3 | 1 / 4 | `positive_only` |
| [02-names-files-packages.md](../docs/guide/02-names-files-packages.md) | 7 | 2 | 3 | 0 | 2 | 5 / 7 | `both_positive_and_negative` |
| [03-types-generics.md](../docs/guide/03-types-generics.md) | 11 | 6 | 1 | 1 | 3 | 8 / 11 | `both_positive_and_negative` |
| [04-expressions-operators.md](../docs/guide/04-expressions-operators.md) | 5 | 3 | 1 | 1 | 0 | 5 / 5 | `both_positive_and_negative` |
| [05-declarations-callables.md](../docs/guide/05-declarations-callables.md) | 4 | 2 | 2 | 0 | 0 | 4 / 4 | `both_positive_and_negative` |
| [06-blocks-control-flow.md](../docs/guide/06-blocks-control-flow.md) | 19 | 16 | 2 | 0 | 1 | 18 / 19 | `both_positive_and_negative` |
| [07-calls-lambdas-closures.md](../docs/guide/07-calls-lambdas-closures.md) | 8 | 6 | 1 | 0 | 1 | 7 / 8 | `both_positive_and_negative` |
| [08-class-family-members.md](../docs/guide/08-class-family-members.md) | 16 | 15 | 0 | 0 | 1 | 15 / 16 | `positive_only` |
| [09-nullability-errors.md](../docs/guide/09-nullability-errors.md) | 16 | 12 | 1 | 1 | 2 | 14 / 16 | `both_positive_and_negative` |
| [10-ownership-borrowing-drop.md](../docs/guide/10-ownership-borrowing-drop.md) | 9 | 6 | 2 | 0 | 1 | 8 / 9 | `both_positive_and_negative` |
| [11-copyability-layout-construction.md](../docs/guide/11-copyability-layout-construction.md) | 3 | 2 | 0 | 0 | 1 | 2 / 3 | `positive_only` |
| [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | 48 | 47 | 1 | 0 | 0 | 48 / 48 | `both_positive_and_negative` |
| [13-program-runtime-standard-library.md](../docs/guide/13-program-runtime-standard-library.md) | 4 | 3 | 0 | 0 | 1 | 3 / 4 | `positive_only` |
| **合计** | **154** | **121** | **14** | **3** | **16** | **138 / 154** | **9 双极性 / 4 仅正向** |

- **正/负向双极性覆盖章节（9 / 13）**：`02-names-files-packages.md`, `03-types-generics.md`, `04-expressions-operators.md`, `05-declarations-callables.md`, `06-blocks-control-flow.md`, `07-calls-lambdas-closures.md`, `09-nullability-errors.md`, `10-ownership-borrowing-drop.md`, `12-collections-destructuring.md`（同时具备 `run_ok` 正向运行探针与 `compile_fail` / `runtime_abort` 负向拒绝或中止探针）。
- **仅正向覆盖章节（4 / 13，待后续补充独立负向诊断探针）**：`01-lexical.md`, `08-class-family-members.md`, `11-copyability-layout-construction.md`, `13-program-runtime-standard-library.md`（当前语料库内仅有 `run_ok` 与 `known_unimplemented` 探针，尚无独立 `compile_fail` / `runtime_abort` 探针）。

### 1.1 负向诊断码与运行时中止契约矩阵（14 `compile_fail` + 3 `runtime_abort`）

| 负向契约标识 | `diagnostic.rs` 符号常量 / 中止类别 | 覆盖 Guide 章节 | 关联 Probe 数量 | 绑定 Probe 列表 |
|---|---|---|---:|---|
| `FE:L0080` | `UNRESOLVED_NAME` | [02-names-files-packages.md](../docs/guide/02-names-files-packages.md) | 3 | `r01_bogus_member`, `r02_bogus_member_str`, `y02_take_no_import` |
| `FE:L0084` | `TYPE_MISMATCH` | [05-declarations-callables.md](../docs/guide/05-declarations-callables.md), [04-expressions-operators.md](../docs/guide/04-expressions-operators.md) | 2 | `p01_int_print`, `v02_assign_chain` |
| `FE:L0085` | `INVALID_OPERAND_TYPES` | [03-types-generics.md](../docs/guide/03-types-generics.md), [09-nullability-errors.md](../docs/guide/09-nullability-errors.md) | 2 | `p03b_byte_literal_cmp`, `s04_question_on_nullable` |
| `FE:L0089` | `NO_COMMON_BRANCH_TYPE` | [06-blocks-control-flow.md](../docs/guide/06-blocks-control-flow.md) | 1 | `v04_stmt_if_mixed` |
| `FE:L0112` | `WHEN_BRANCH_TYPE` | [06-blocks-control-flow.md](../docs/guide/06-blocks-control-flow.md) | 1 | `p43_if_join_any` |
| `FE:L0132` | `PARTIAL_MOVE` | [10-ownership-borrowing-drop.md](../docs/guide/10-ownership-borrowing-drop.md) | 1 | `p25_linked_iter` |
| `FE:L0133` | `MOVE_FROM_BORROWED_BINDING` | [10-ownership-borrowing-drop.md](../docs/guide/10-ownership-borrowing-drop.md) | 1 | `p25_linked_iter` |
| `FE:L0135` | `LOAN_CONFLICT` | [07-calls-lambdas-closures.md](../docs/guide/07-calls-lambdas-closures.md), [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | 2 | `p42_lambda_mutate`, `w06_map_mutate_in_loop` |
| `FE:L0164` | `UNSUPPORTED_BORROW_FLOW` | [10-ownership-borrowing-drop.md](../docs/guide/10-ownership-borrowing-drop.md) | 1 | `q17_generic_class` |
| `FE:L0167` | `UNSUPPORTED_TOP_LEVEL_RUNTIME_VALUE` | [05-declarations-callables.md](../docs/guide/05-declarations-callables.md) | 1 | `p23_global_var` |
| `abort:integer overflow in +` | `SIGABRT (exit 134)` | [04-expressions-operators.md](../docs/guide/04-expressions-operators.md) | 1 | `p04_overflow` |
| `abort:boom` | `SIGABRT (exit 134)` | [09-nullability-errors.md](../docs/guide/09-nullability-errors.md) | 1 | `p45_abort_message` |
| `abort:value out of range for ULong` | `SIGABRT (exit 134)` | [03-types-generics.md](../docs/guide/03-types-generics.md) | 1 | `v12_convert_abort` |

### 1.2 历史基线 `baseline-aef3f4f8.tsv` 演进与零退化对账汇总（154 Probes）

| 演进分类 (`baseline_evolution`) | 历史基线状态 (`baseline-aef3f4f8.tsv`) | 当前强 Oracle 状态 (`expectations.json`) | Probe 数量 | 代表性 / 完整 Probe 列表 |
|---|---|---|---:|---|
| `baseline_ok_preserved` | `OK` | `run_ok`（精确字节匹配） | 89 | 历史 `OK` 基线 100% 保持通过（含 `q12_companion_const` 匿名 `companion { }` 语法更新） |
| `promoted_to_run_ok` | `BE:InvalidSsa` / `BE:MissingFact` / `BE:UnsupportedNode` / `FE:L0080` / `FE:L0111` | `run_ok`（新晋能力通过） | 32 | `e01_enum_when_bool_return`, `e02_enum_when_stmt_println`, `e03_enum_when_bool_val`, `p08_when_int`, `p12b_class_field_assign`, `p12c_inout_param`, `p14_nullable`, `p14b_nullable_simple`, `p14c_nullable_int`, `p16b_result_string`, `p17_lambda`, `p17b_lambda_nocapture`, `p18_move_closure`, `p22_rc`, `p37_is_generic_case`, `p41_when_string`, `p49_when_enum_no_payload`, `p50_enum_method`, `q02_lambda_arg_inline`, `q05_nullable_param`, `q09_inout_class_param`, `s01_result_builtin`, `t03_vmethod_generic`, `u01_enum_case_borrow_arg`, `v03_join_nullable`, `v13_while_nullable_int`, `v16_when_in_range`, `v19_stmt_when_mixed`, `x02_string_nullable_return`, `y01_import_take_for_sum`, `y03_method_take_for`, `y04_take_for_println` |
| `upgraded_to_structured_abort` | `abort(exit 1,no msg)` | `runtime_abort`（`SIGABRT` + `abort: <msg>`） | 3 | `p04_overflow`, `p45_abort_message`, `v12_convert_abort` |
| `upgraded_to_compile_fail` | `FE:L0165` | `compile_fail`（`FE:L0167` 规范拒绝） | 1 | `p23_global_var` |
| `baseline_compile_fail_preserved` | `FE:L0080`..`FE:L0164` | `compile_fail`（精确诊断序列匹配） | 13 | `p01_int_print`, `p03b_byte_literal_cmp`, `p25_linked_iter`, `p42_lambda_mutate`, `p43_if_join_any`, `q17_generic_class`, `r01_bogus_member`, `r02_bogus_member_str`, `s04_question_on_nullable`, `v02_assign_chain`, `v04_stmt_if_mixed`, `w06_map_mutate_in_loop`, `y02_take_no_import` |
| `remaining_known_unimplemented` | `BE:UnsupportedNode` / `FE:L0165` / `FE:L0080` / `ERR` | `known_unimplemented`（100% 绑定 `target_stdout` 与 `roadmap_blockers`） | 16 | `p02_interp`, `p05_double`, `p06_char`, `p11_interface_bound`, `p15_elvis`, `p16_result_q`, `p21_box_enum`, `p25b_linked_recursive`, `p27_thread`, `p28_companion`, `p35_value_eq`, `p40_nested_generic_class`, `q03_fn_ref`, `q08_enum_string_payload`, `s02_pair_builtin`, `s03_to_pair` |
| `regressed_from_baseline_ok` | `OK` | 非 `run_ok`（基线退化） | 0 | 无（零基线退化） |

### 1.3 Guide v0.46 豁免项 (`v046_spec_contract`) 与 Spec 交付合同溯源矩阵（35 Probes）

| Spec 合同编号 | 规范文档路径 | 合同状态 | 关联豁免 Probe 数 | 交付后分布 | 绑定的 Probe 列表 |
|---|---|---|---:|---|---|
| `SPEC-0320` | [0320-enum-case-borrow-categories.md](../docs/archive/specs/0320-enum-case-borrow-categories.md) | `archived` | 5 | `run_ok`: 5 | `e01_enum_when_bool_return`, `e02_enum_when_stmt_println`, `e03_enum_when_bool_val`, `p49_when_enum_no_payload`, `u01_enum_case_borrow_arg` |
| `SPEC-0323` | [0323-final-admission-static-recovery.md](../docs/archive/specs/0323-final-admission-static-recovery.md) | `archived` | 1 | `known_unimplemented`: 1 | `p28_companion` |
| `SPEC-0341` | [0341-shared-lowering-core.md](../docs/specs/active/0341-shared-lowering-core.md) | `active` | 3 | `run_ok`: 3 | `y01_import_take_for_sum`, `y03_method_take_for`, `y04_take_for_println` |
| `SPEC-0342` | [0342-companion-block-syntax.md](../docs/archive/specs/0342-companion-block-syntax.md) | `archived` | 2 | `known_unimplemented`: 1, `run_ok`: 1 | `p28_companion`, `q12_companion_const` |
| `SPEC-0343` | [0343-top-level-const-only.md](../docs/archive/specs/0343-top-level-const-only.md) | `archived` | 1 | `compile_fail`: 1 | `p23_global_var` |
| `SPEC-0349` | [0349-builtin-result-and-check.md](../docs/archive/specs/0349-builtin-result-and-check.md) | `archived` | 4 | `known_unimplemented`: 1, `run_ok`: 3 | `p16_result_q`, `p16b_result_string`, `p37_is_generic_case`, `s01_result_builtin` |
| `SPEC-0350` | [0350-ownership-refinement-nullable-pilot.md](../docs/archive/specs/0350-ownership-refinement-nullable-pilot.md) | `archived` | 7 | `run_ok`: 7 | `p14_nullable`, `p14b_nullable_simple`, `p14c_nullable_int`, `q05_nullable_param`, `v03_join_nullable`, `v13_while_nullable_int`, `x02_string_nullable_return` |
| `SPEC-0355` | [0355-abort-output.md](../docs/archive/specs/0355-abort-output.md) | `archived` | 3 | `runtime_abort`: 3 | `p04_overflow`, `p45_abort_message`, `v12_convert_abort` |
| `SPEC-0356` | [0356-refine-when-family.md](../docs/archive/specs/0356-refine-when-family.md) | `archived` | 8 | `run_ok`: 8 | `e01_enum_when_bool_return`, `e02_enum_when_stmt_println`, `e03_enum_when_bool_val`, `p08_when_int`, `p49_when_enum_no_payload`, `p50_enum_method`, `v16_when_in_range`, `v19_stmt_when_mixed` |
| `SPEC-0357` | [0357-string-when-ownership-facts.md](../docs/specs/active/0357-string-when-ownership-facts.md) | `active` | 1 | `run_ok`: 1 | `p41_when_string` |
| `SPEC-0358` | [0358-refine-assignment-inout.md](../docs/archive/specs/0358-refine-assignment-inout.md) | `archived` | 3 | `run_ok`: 3 | `p12b_class_field_assign`, `p12c_inout_param`, `q09_inout_class_param` |
| `SPEC-0359` | [0359-refine-rc.md](../docs/archive/specs/0359-refine-rc.md) | `archived` | 1 | `run_ok`: 1 | `p22_rc` |
| `SPEC-0360` | [0360-string-when-refinement.md](../docs/specs/active/0360-string-when-refinement.md) | `active` | 1 | `run_ok`: 1 | `p41_when_string` |
| `SPEC-0361` | [0361-refine-generic-value-class.md](../docs/archive/specs/0361-refine-generic-value-class.md) | `archived` | 1 | `run_ok`: 1 | `t03_vmethod_generic` |
| `SPEC-0364` | [0364-refine-closures.md](../docs/archive/specs/0364-refine-closures.md) | `archived` | 4 | `run_ok`: 4 | `p17_lambda`, `p17b_lambda_nocapture`, `p18_move_closure`, `q02_lambda_arg_inline` |
| `SPEC-0368` | [0368-text-byte-foundation.md](../docs/specs/active/0368-text-byte-foundation.md) | `active` | 1 | `run_ok`: 1 | `p16b_result_string` |

### 1.4 强 Oracle 溯源分类矩阵（`oracle_origin` Taxonomy，154 Probes + 6 Programs）

| 溯源分类 (`oracle_origin`) | 审计验证契约说明 | Probe 数 | Program 数 | 期望类型分布 (`kind`) |
|---|---|---:|---:|---|
| `single_println_literal` | 单条无循环/无插值 `println("...")` 静态字面量精确推导 | 31 | 0 | `run_ok`: 31 |
| `source_trace_audited` | 控制流、循环、集合或 `deinit` 多语句源码级人工追踪确认（`CONFIRMED_STDOUT_ORACLES`） | 57 | 0 | `run_ok`: 57 |
| `v046_spec_contract` | Guide v0.46 语法/语义变更或 `baseline-aef3f4f8.tsv` 之后合入的 Spec 合同覆盖（`V046_PROBE_OVERRIDES`） | 39 | 0 | `compile_fail`: 1, `known_unimplemented`: 2, `run_ok`: 33, `runtime_abort`: 3 |
| `baseline_diagnostic_audited` | 前端负向诊断码契约确认（`error[Lxxxx]` + `diagnostic.rs::codes` 符号匹配） | 13 | 0 | `compile_fail`: 13 |
| `baseline_unimplemented_audited` | 已知未实现特性边界确认（绑定 `target_stdout` 与 `v1.json` 路线图阻塞项） | 14 | 0 | `known_unimplemented`: 14 |
| `expected_txt_file` | V1 验收程序独立 `expected.txt` 黄金标准输出文件绑定（SHA-256 + 行数/字节数校验） | 0 | 6 | `known_unimplemented`: 6 |

### 1.5 标准输出规模与多行形态分布矩阵（`expected_lines` / `expected_bytes`）

| 输出形态分类 | 适用范围 | 条目数量 | 累计期望行数 | 累计期望字节数 | 代表性 / 完整条目明细 |
|---|---|---:|---:|---:|---|
| `single_line_stdout` | 1 行输出 Probe (`run_ok` + `known_unimplemented`) | 117 | 117 | 992 B | 单行精确字节匹配（全部以 `\n` 结尾） |
| `multi_line_stdout` | ≥2 行输出 Probe (`run_ok` + `known_unimplemented`) | 20 | 42 | 151 B | `p10_generic_fn` (2L/12B), `p14_nullable` (2L/15B), `p32_deinit` (3L/9B), `p38_list_of_strings` (2L/4B), `q05_nullable_param` (2L/14B), `q08_enum_string_payload` (2L/8B), `q19_deep_return_cleanup` (3L/6B), `q20_replace_field` (2L/8B), `u02_list_add_loop_string` (2L/4B), `v15_map_for` (2L/4B), `w08_map_string_values` (2L/8B), `w12` (2L/8B), `w13` (2L/9B), `w15` (2L/9B), `w17` (2L/6B), `w19` (2L/6B), `x03_if_call_borrow_arg` (2L/9B), `y04_take_for_println` (2L/4B), `y05_take_borrow_val_call` (2L/4B), `y06_take_borrow_val_for` (2L/4B) |
| `zero_stdout_negative` | 零标准输出负向 Probe (`compile_fail` + `runtime_abort`) | 17 | 0 | 0 B | `p01_int_print`, `p03b_byte_literal_cmp`, `p04_overflow`, `p23_global_var`, `p25_linked_iter`, `p42_lambda_mutate`, `p43_if_join_any`, `p45_abort_message`, `q17_generic_class`, `r01_bogus_member`, `r02_bogus_member_str`, `s04_question_on_nullable`, `v02_assign_chain`, `v04_stmt_if_mixed`, `v12_convert_abort`, `w06_map_mutate_in_loop`, `y02_take_no_import` |
| `acceptance_programs` | V1 验收程序 (`expected.txt`) | 6 | 85 | 1537 B | `config_parse` (10L/190B), `expr_eval` (6L/174B), `linked_stack` (8L/242B), `raii_state_machine` (28L/369B), `varint_codec` (17L/375B), `word_count` (16L/187B) |

### 1.6 Probe 批次前缀与演进关注面分布矩阵（11 Batches，154 Probes）

| 批次前缀 (`probe_batch`) | 关注面与批次说明 | Probe 总数 | `run_ok` | `compile_fail` | `runtime_abort` | `known_unimplemented` | 能力通过 (`capability_passed`) | 批次状态 (`batch_status`) |
|---|---|---:|---:|---:|---:|---:|---:|---|
| `p` | 基础能力：标量、控制流、class/enum/value class、nullable、Result、闭包、容器、所有权等 | 59 | 39 | 6 | 2 | 12 | 47 / 59 | `partial` |
| `q` | 对 `p` 中失败项的变体复测，缩小失败条件 | 20 | 17 | 1 | 0 | 2 | 18 / 20 | `partial` |
| `r` | 未知成员访问，期望前端诊断 | 2 | 0 | 2 | 0 | 0 | 2 / 2 | `all_passed` |
| `s` | 标准环境：内建 `Result`、`Pair`/`to` 与 `?` 的边界 | 4 | 1 | 1 | 0 | 2 | 2 / 4 | `partial` |
| `t` | SPEC-0311–0318 的定向复测：value class 方法、Map/List 遍历中的跳转、短路求值 | 10 | 10 | 0 | 0 | 0 | 10 / 10 | `all_passed` |
| `u` | enum case 作借用实参、循环中 `MutableList.add` 等增量复测 | 4 | 4 | 0 | 0 | 0 | 4 / 4 | `all_passed` |
| `v` | v0.44 补全面：赋值结果、整数 Range、显式整数转换、nullable 循环、Map consume | 23 | 20 | 2 | 1 | 0 | 23 / 23 | `all_passed` |
| `w` | Map/MutableMap：遍历顺序、覆盖与删除、遍历中的跳转与借用 | 19 | 18 | 1 | 0 | 0 | 19 / 19 | `all_passed` |
| `x` | nullable class、String nullable 返回与条件中的借用实参复测 | 3 | 3 | 0 | 0 | 0 | 3 / 3 | `all_passed` |
| `y` | `koven.algorithms` 的 `take` 与 N1a 范围借用 | 6 | 5 | 1 | 0 | 0 | 6 / 6 | `all_passed` |
| `e` | enum subject `when` 在 compilation-unit 管线中的回退（E2 门槛） | 4 | 4 | 0 | 0 | 0 | 4 / 4 | `all_passed` |
| **合计** | **11 个演进批次** | **154** | **121** | **14** | **3** | **16** | **138 / 154** | **8 全通过 / 3 部分通过** |

### 1.7 未实现允许记号（`allowed_markers`）与编译阶段分布矩阵（7 Markers）

| 允许记号 (`allowed_marker`) | 归属编译阶段 | 记号契约说明 | 基线命中数 (`baseline_marker`) | 授权 Probe 数 | 授权 Program 数 |
|---|---|---|---:|---:|---:|
| `FE:L0080` | Phase 2 名称解析 | 前端未解析符号或尚未注入的标准库声明（`UNRESOLVED_NAME`） | 2 | 2 | 6 |
| `FE:L0084` | Phase 2 类型检查 | 前端类型不匹配或转换契约尚未接入（`TYPE_MISMATCH`） | 0 | 0 | 6 |
| `FE:L0085` | Phase 2 运算符类型 | 前端操作数或协议约束尚未接入（`INVALID_OPERAND_TYPES`） | 0 | 0 | 6 |
| `FE:L0165` | Phase 2 最终类型准入 | 前端最终类型消费准入显式拦截（`FINAL_TYPE_ANALYSIS_INCOMPLETE`） | 5 | 14 | 6 |
| `BE:UnsupportedNode` | Phase 4 SSA Lowering | 后端 SSA 构建阶段尚未支持的语法或语义节点 | 8 | 16 | 6 |
| `BE:UnsupportedSource` | Phase 4 Native Object | 后端原生目标文件生成阶段尚未支持的调用来源 | 0 | 1 | 0 |
| `BE:MissingFact` | Phase 3/4 事实交接 | 前端到后端 lowering 所需的所有权或类型事实尚未完备 | 0 | 16 | 6 |

### 1.8 初始 85 探针（`p`/`q`/`r`/`s` 批次 A/B/C 三组）与 P2 阶段门槛对账矩阵

| 分组 | 分组定义与审计口径 | 总数 | 可运行 (`run_ok` + `runtime_abort`) | `compile_fail` | `known_unimplemented` | 阶段门槛 / 当前状态说明 |
|---|---|---:|---:|---:|---:|---|
| **A 组** | A 组：合法基础程序（P2 门槛基数） | 74 | 58 (56 + 2) | 1 (`q17_generic_class`) | 15 | P2 门槛 `≥ 70 / 74`（当前 `58 / 74`，距门槛差 `12` 项） |
| **B 组** | B 组：应拒基础探针（静态/借用/类型拒绝） | 8 | 0 | 8 | 0 | `8 / 8 (100%)` 精确命中期望前端诊断码 (`pass_diagnostic`) |
| **C 组** | C 组：v0.44 待裁决项（已在 v0.46 规范定案） | 3 | 1 (`p37_is_generic_case`) | 1 (`p23_global_var`) | 1 (`p35_value_eq`) | `p23` 规范拒绝 (`FE:L0167`)、`p37` 能力通过 (`SPEC-0349`)、`p35` 待 `L1` |

### 1.9 Guide 13 章 V2 终态完备性（正反例双极性 + 零未实现）差距矩阵

| Guide 章节 | 规范领域 | 正向已通 (`run_ok`) | 负向契约 (`compile_fail` / `abort`) | 剩余 `known_unimplemented` | 路线图阻塞项 | 关联 V1 验收程序 | V2 章节就绪状态 (`v2_status`) |
|---|---|---:|---|---|---|---|---|
| [01-lexical.md](../docs/guide/01-lexical.md) | 词法 | 1 | 0（待补反例） | 3 (`p02_interp`, `p05_double`, `p06_char`) | `L1`, `L2`, `N05` | - | `needs_negative_and_roadmap` |
| [02-names-files-packages.md](../docs/guide/02-names-files-packages.md) | 名称、文件、Package 与 Import | 2 | 3 (`FE:L0080`) | 2 (`s02_pair_builtin`, `s03_to_pair`) | `S6` | - | `blocked_by_roadmap` |
| [03-types-generics.md](../docs/guide/03-types-generics.md) | 类型与泛型 | 6 | 2 (`FE:L0085`, `abort:value out of range for ULong`) | 3 (`p11_interface_bound`, `p35_value_eq`, `p40_nested_generic_class`) | `L1`, `N01`, `N06` | `varint_codec` | `blocked_by_roadmap` |
| [04-expressions-operators.md](../docs/guide/04-expressions-operators.md) | 表达式与运算符 | 3 | 2 (`abort:integer overflow in +`, `FE:L0084`) | 0 | - | - | `v2_probe_ready` |
| [05-declarations-callables.md](../docs/guide/05-declarations-callables.md) | 声明与 Callable | 2 | 2 (`FE:L0084`, `FE:L0167`) | 0 | - | - | `v2_probe_ready` |
| [06-blocks-control-flow.md](../docs/guide/06-blocks-control-flow.md) | Block 与控制流 | 16 | 2 (`FE:L0112`, `FE:L0089`) | 1 (`q08_enum_string_payload`) | `L7`, `N02` | - | `blocked_by_roadmap` |
| [07-calls-lambdas-closures.md](../docs/guide/07-calls-lambdas-closures.md) | 调用、Lambda 与 Closure | 6 | 1 (`FE:L0135`) | 1 (`q03_fn_ref`) | `N04` | - | `blocked_by_roadmap` |
| [08-class-family-members.md](../docs/guide/08-class-family-members.md) | Class Family、成员与 Receiver | 15 | 0（待补反例） | 1 (`p28_companion`) | `N06` | `raii_state_machine` | `needs_negative_and_roadmap` |
| [09-nullability-errors.md](../docs/guide/09-nullability-errors.md) | 空安全与错误值 | 12 | 2 (`abort:boom`, `FE:L0085`) | 2 (`p15_elvis`, `p16_result_q`) | `N03`, `N08` | `config_parse` | `blocked_by_roadmap` |
| [10-ownership-borrowing-drop.md](../docs/guide/10-ownership-borrowing-drop.md) | 所有权、借用与析构 | 6 | 2 (`FE:L0133`, `FE:L0132`, `FE:L0164`) | 1 (`p25b_linked_recursive`) | `N03` | `linked_stack` | `blocked_by_roadmap` |
| [11-copyability-layout-construction.md](../docs/guide/11-copyability-layout-construction.md) | Copyable、布局、构造与结构移动 | 2 | 0（待补反例） | 1 (`p21_box_enum`) | `L7`, `N07` | `expr_eval` | `needs_negative_and_roadmap` |
| [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | 集合、索引与解构 | 47 | 1 (`FE:L0135`) | 0 | - | `word_count` | `v2_probe_ready` |
| [13-program-runtime-standard-library.md](../docs/guide/13-program-runtime-standard-library.md) | 程序入口、Runtime 与标准库 | 3 | 0（待补反例） | 1 (`p27_thread`) | `N04`, `T1` | - | `needs_negative_and_roadmap` |

## 2. 六个 V1 验收程序（Acceptance Programs）

| 程序 | 归属章节 | 当前状态 | 路线图阻塞项 | 期望输出文件 | 期望输出规模 | 允许的未实现记号 |
|---|---|---|---|---|---|---|
| `config_parse` | [09-nullability-errors.md](../docs/guide/09-nullability-errors.md) | `known_unimplemented` | `N02`, `N08`, `L1`, `L2` | [programs/config_parse/expected.txt](programs/config_parse/expected.txt) | 10 行 / 190 B | `FE:L0080`, `FE:L0084`, `FE:L0085`, `FE:L0165`, `BE:UnsupportedNode`, `BE:MissingFact` |
| `expr_eval` | [11-copyability-layout-construction.md](../docs/guide/11-copyability-layout-construction.md) | `known_unimplemented` | `N02`, `N05`, `N07`, `N08`, `L7` | [programs/expr_eval/expected.txt](programs/expr_eval/expected.txt) | 6 行 / 174 B | `FE:L0080`, `FE:L0084`, `FE:L0085`, `FE:L0165`, `BE:UnsupportedNode`, `BE:MissingFact` |
| `linked_stack` | [10-ownership-borrowing-drop.md](../docs/guide/10-ownership-borrowing-drop.md) | `known_unimplemented` | `N01`, `N03`, `N07`, `L7` | [programs/linked_stack/expected.txt](programs/linked_stack/expected.txt) | 8 行 / 242 B | `FE:L0080`, `FE:L0084`, `FE:L0085`, `FE:L0165`, `BE:UnsupportedNode`, `BE:MissingFact` |
| `raii_state_machine` | [08-class-family-members.md](../docs/guide/08-class-family-members.md) | `known_unimplemented` | `N01`, `N02` | [programs/raii_state_machine/expected.txt](programs/raii_state_machine/expected.txt) | 28 行 / 369 B | `FE:L0080`, `FE:L0084`, `FE:L0085`, `FE:L0165`, `BE:UnsupportedNode`, `BE:MissingFact` |
| `varint_codec` | [03-types-generics.md](../docs/guide/03-types-generics.md) | `known_unimplemented` | `N01`, `N08`, `L3b` | [programs/varint_codec/expected.txt](programs/varint_codec/expected.txt) | 17 行 / 375 B | `FE:L0080`, `FE:L0084`, `FE:L0085`, `FE:L0165`, `BE:UnsupportedNode`, `BE:MissingFact` |
| `word_count` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `known_unimplemented` | `N02`, `N03` | [programs/word_count/expected.txt](programs/word_count/expected.txt) | 16 行 / 187 B | `FE:L0080`, `FE:L0084`, `FE:L0085`, `FE:L0165`, `BE:UnsupportedNode`, `BE:MissingFact` |

### 2.1 六个 V1 验收程序按路线图阻塞项（`v1.json`）归类对账（10 Roadmap Blockers）

| 路线图条目 ID | 路线图条目标题 | 状态 | 阻塞 Program 数 | 阻塞的验收程序 (`programs`) | 同时阻塞的 Probe 数 |
|---|---|---|---:|---|---:|
| `L1` | 值语义协议 Equatable/Hashable/Display（Comparable 只给内建数值、Char、String）；value class/enum 自动推导相等与哈希；Map 键 K : Equatable + Hashable | `todo` | 1 | `config_parse` | 2 |
| `L2` | 字符串插值降级："${x}" 要求 x : Display | `todo` | 1 | `config_parse` | 1 |
| `L3b` | 转换族其余部分：OrNull 变体、浮点↔整数 checked、Char 码点 | `todo` | 1 | `varint_codec` | 0 |
| `L7` | MoveOnly enum 的消费式匹配：owned subject 分支获得 payload owned 绑定 | `todo` | 2 | `expr_eval`, `linked_stack` | 2 |
| `N01` | 赋值与可变性：字段赋值 obj.f = v、inout 标量参数赋值、inout class 参数、泛型 class 的 inout 方法 | `partial` | 3 | `linked_stack`, `raii_state_machine`, `varint_codec` | 1 |
| `N02` | when：Int/String/Char/MoveOnly enum subject、enum 方法内 when (this)、project 模式 enum case 条件 | `partial` | 4 | `config_parse`, `expr_eval`, `raii_state_machine`, `word_count` | 1 |
| `N03` | nullable 全路径：标量 T?、nullable 参数、Elvis、?.、!! | `partial` | 2 | `linked_stack`, `word_count` | 2 |
| `N05` | Double/Float 运算与 Char 比较的 native 支持 | `todo` | 1 | `expr_eval` | 2 |
| `N07` | Box.value / unbox() 与 Rc.value 字段投影 | `partial` | 2 | `expr_eval`, `linked_stack` | 1 |
| `N08` | Result + 后缀 ? 的 Int 与 String payload 全路径 | `todo` | 3 | `config_parse`, `expr_eval`, `varint_codec` | 1 |

## 3. 16 个 `known_unimplemented` Probe 按路线图阻塞项（`v1.json`）归类对账

| 路线图条目 ID | 路线图条目标题 | 状态 | 关联 Probe 总数 | 阻塞的 Probe 列表 |
|---|---|---|---:|---|
| `L1` | 值语义协议 Equatable/Hashable/Display（Comparable 只给内建数值、Char、String）；value class/enum 自动推导相等与哈希；Map 键 K : Equatable + Hashable | `todo` | 2 | `p02_interp`, `p35_value_eq` |
| `L2` | 字符串插值降级："${x}" 要求 x : Display | `todo` | 1 | `p02_interp` |
| `L7` | MoveOnly enum 的消费式匹配：owned subject 分支获得 payload owned 绑定 | `todo` | 2 | `p21_box_enum`, `q08_enum_string_payload` |
| `N01` | 赋值与可变性：字段赋值 obj.f = v、inout 标量参数赋值、inout class 参数、泛型 class 的 inout 方法 | `partial` | 1 | `p40_nested_generic_class` |
| `N02` | when：Int/String/Char/MoveOnly enum subject、enum 方法内 when (this)、project 模式 enum case 条件 | `partial` | 1 | `q08_enum_string_payload` |
| `N03` | nullable 全路径：标量 T?、nullable 参数、Elvis、?.、!! | `partial` | 2 | `p15_elvis`, `p25b_linked_recursive` |
| `N04` | 闭包：lambda 直接作实参、函数引用 ::f、move 闭包返回与存储 | `partial` | 2 | `p27_thread`, `q03_fn_ref` |
| `N05` | Double/Float 运算与 Char 比较的 native 支持 | `todo` | 2 | `p05_double`, `p06_char` |
| `N06` | interface bound 泛型调用、companion 函数调用、interface 实现与默认方法（单文件） | `partial` | 2 | `p11_interface_bound`, `p28_companion` |
| `N07` | Box.value / unbox() 与 Rc.value 字段投影 | `partial` | 1 | `p21_box_enum` |
| `N08` | Result + 后缀 ? 的 Int 与 String payload 全路径 | `todo` | 1 | `p16_result_q` |
| `S6` | Map 带初值构造 mapOf("a" to 1) | `todo` | 2 | `s02_pair_builtin`, `s03_to_pair` |
| `T1` | 受限线程：thread(move { }) : Thread<T>、join()、channel（多生产者单消费者）；drop 时隐式 join；worker abort 即进程 abort；只转移所有权 | `todo` | 1 | `p27_thread` |

### 3.1 路线图阻塞项解封收益分析（单阻塞独立解封 vs 联合阻塞与 P2 门槛贡献，14 Blockers）

| 路线图条目 | 阶段 | 状态 | 独立解封 Probe (`sole_probes`) | 联合阻塞 Probe (`co_probes`) | P2 A 组贡献数 | 关联 V1 验收程序 |
|---|---|---|---|---|---:|---|
| `L1` | `P3` | `todo` | 1 (`p35_value_eq`) | 1 (`p02_interp`) | 1 | `config_parse` |
| `L2` | `P3` | `todo` | 0 | 1 (`p02_interp`) | 1 | `config_parse` |
| `L3b` | `P3` | `todo` | 0 | 0 | 0 | `varint_codec` |
| `L7` | `P3` | `todo` | 0 | 2 (`p21_box_enum`, `q08_enum_string_payload`) | 2 | `expr_eval`, `linked_stack` |
| `N01` | `P2` | `partial` | 1 (`p40_nested_generic_class`) | 0 | 1 | `linked_stack`, `raii_state_machine`, `varint_codec` |
| `N02` | `P2` | `partial` | 0 | 1 (`q08_enum_string_payload`) | 1 | `config_parse`, `expr_eval`, `raii_state_machine`, `word_count` |
| `N03` | `P2` | `partial` | 2 (`p15_elvis`, `p25b_linked_recursive`) | 0 | 2 | `linked_stack`, `word_count` |
| `N04` | `P2` | `partial` | 1 (`q03_fn_ref`) | 1 (`p27_thread`) | 2 | - |
| `N05` | `P2` | `todo` | 2 (`p05_double`, `p06_char`) | 0 | 2 | `expr_eval` |
| `N06` | `P2` | `partial` | 2 (`p11_interface_bound`, `p28_companion`) | 0 | 2 | - |
| `N07` | `P2` | `partial` | 0 | 1 (`p21_box_enum`) | 1 | `expr_eval`, `linked_stack` |
| `N08` | `P2` | `todo` | 1 (`p16_result_q`) | 0 | 1 | `config_parse`, `expr_eval`, `varint_codec` |
| `S6` | `P3` | `todo` | 2 (`s02_pair_builtin`, `s03_to_pair`) | 0 | 2 | - |
| `T1` | `P4` | `todo` | 0 | 1 (`p27_thread`) | 1 | - |

### 3.2 路线图阶段（`P2` → `P3` → `P4`）递进解封与里程碑投射矩阵

| 阶段里程碑 | 阶段核心目标 | 本阶段清零阻塞项 | 本阶段新增解封 Probe | 累计能力通过 (`capability_passed`) | P2 A 组可运行 (`≥ 70 / 74`) | V1 验收程序就绪 |
|---|---|---|---|---:|---:|---:|
| **P2 完成** | Native 后端能力补齐（泛型、可空/Result `?`、闭包、浮点/字符与接口分派） | `N01`, `N02`, `N03`, `N04`, `N05`, `N06`, `N07`, `N08` | +9 (`p05_double`, `p06_char`, `p11_interface_bound`, `p15_elvis`, `p16_result_q`, `p25b_linked_recursive`, `p28_companion`, `p40_nested_generic_class`, `q03_fn_ref`) | **147 / 154** | **67 / 74** | **2 / 6** |
| **P3 完成** | 语言糖、协议与标准库收口（Equatable/Display 插值、位运算、多载荷 enum 与 Pair/to） | `L1`, `L2`, `L3b`, `L7`, `S6` | +6 (`p02_interp`, `p21_box_enum`, `p35_value_eq`, `q08_enum_string_payload`, `s02_pair_builtin`, `s03_to_pair`) | **153 / 154** | **72 / 74** | **6 / 6** |
| **P4 完成** | 并发与扩展（原生线程 thread 与 JoinHandle） | `T1` | +1 (`p27_thread`) | **154 / 154** | **73 / 74** | **6 / 6** |

## 4. 154 个 Probe 逐项强 Oracle 与语义推导账本

| Probe | Guide 章节 | 类型 (`kind`) | 溯源 (`oracle_origin`) | 期望结果 / 诊断码 | 语义推导 / 规则说明 (`oracle_derivation` / `note`) |
|---|---|---|---|---|---|
| `e01_enum_when_bool_return` | [06-blocks-control-flow.md](../docs/guide/06-blocks-control-flow.md) | `run_ok` | `v046_spec_contract` | `e1\n` | isRed(Color.Red) matches branch Color.Red -> true; if-branch prints 'e1' |
| `e02_enum_when_stmt_println` | [06-blocks-control-flow.md](../docs/guide/06-blocks-control-flow.md) | `run_ok` | `v046_spec_contract` | `e2\n` | c = Color.Red matches statement when branch Color.Red -> println('e2') |
| `e03_enum_when_bool_val` | [06-blocks-control-flow.md](../docs/guide/06-blocks-control-flow.md) | `run_ok` | `v046_spec_contract` | `e5\n` | c = Color.Green yields b = false in when expression; if (!b) prints 'e5' |
| `e04_enum_when_int_return` | [06-blocks-control-flow.md](../docs/guide/06-blocks-control-flow.md) | `run_ok` | `single_println_literal` | `e6\n` | single literal println('e6') |
| `p01_int_print` | [05-declarations-callables.md](../docs/guide/05-declarations-callables.md) | `compile_fail` | `baseline_diagnostic_audited` | `FE:L0084` | println expects String parameter; passing Int (40 + 2) emits TYPE_MISMATCH L0084 |
| `p02_interp` | [01-lexical.md](../docs/guide/01-lexical.md) | `known_unimplemented` | `baseline_unimplemented_audited` | `BE:UnsupportedNode`, `BE:MissingFact`, `FE:L0165` → target `hello k\n` | string interpolation 'hello ${n}' with n = 'k' evaluates to 'hello k' (blocked by L1/L2) |
| `p03_long_uint` | [01-lexical.md](../docs/guide/01-lexical.md) | `run_ok` | `single_println_literal` | `widths\n` | single literal println('widths') |
| `p03b_byte_literal_cmp` | [03-types-generics.md](../docs/guide/03-types-generics.md) | `compile_fail` | `baseline_diagnostic_audited` | `FE:L0085` | comparing Byte with untyped Int literal 3 without conversion emits INVALID_OPERAND_TYPES L0085 |
| `p04_overflow` | [04-expressions-operators.md](../docs/guide/04-expressions-operators.md) | `runtime_abort` | `v046_spec_contract` | `abort:integer overflow in +` | Int.MAX_VALUE (2147483647) + 1 triggers checked signed addition overflow abort per Guide 04/09 |
| `p05_double` | [01-lexical.md](../docs/guide/01-lexical.md) | `known_unimplemented` | `baseline_unimplemented_audited` | `BE:UnsupportedNode`, `BE:MissingFact`, `FE:L0165` → target `double\n` | Double multiplication 1.5 * 2.0 == 3.0 holds and prints 'double' (blocked by N05) |
| `p06_char` | [01-lexical.md](../docs/guide/01-lexical.md) | `known_unimplemented` | `baseline_unimplemented_audited` | `BE:UnsupportedNode`, `BE:MissingFact`, `FE:L0165` → target `char\n` | Char comparison 'a' < 'b' holds and prints 'char' (blocked by N05) |
| `p07_while_var` | [06-blocks-control-flow.md](../docs/guide/06-blocks-control-flow.md) | `run_ok` | `source_trace_audited` | `while\n` | while loop sums i=0..9 into s=45; s == 45 holds and prints 'while' |
| `p08_when_int` | [06-blocks-control-flow.md](../docs/guide/06-blocks-control-flow.md) | `run_ok` | `v046_spec_contract` | `two\n` | name(2) matches the subject equality branch 2 -> 'two' and prints 'two' |
| `p09_enum_payload` | [06-blocks-control-flow.md](../docs/guide/06-blocks-control-flow.md) | `run_ok` | `single_println_literal` | `enum\n` | single literal println('enum') |
| `p10_generic_fn` | [03-types-generics.md](../docs/guide/03-types-generics.md) | `run_ok` | `source_trace_audited` | `generic\nint\n` | identity('generic') prints 'generic'; identity(3) == 3 holds and prints 'int' |
| `p11_interface_bound` | [03-types-generics.md](../docs/guide/03-types-generics.md) | `known_unimplemented` | `baseline_unimplemented_audited` | `FE:L0165`, `BE:UnsupportedNode`, `BE:MissingFact` → target `dog\n` | generic greet<T : Named>(Dog()) dispatches Dog.name() and prints 'dog' (blocked by N06) |
| `p12_class_inout` | [08-class-family-members.md](../docs/guide/08-class-family-members.md) | `run_ok` | `single_println_literal` | `counter\n` | single literal println('counter') |
| `p12b_class_field_assign` | [08-class-family-members.md](../docs/guide/08-class-family-members.md) | `run_ok` | `v046_spec_contract` | `field-assign\n` | single literal println('field-assign') |
| `p12c_inout_param` | [10-ownership-borrowing-drop.md](../docs/guide/10-ownership-borrowing-drop.md) | `run_ok` | `v046_spec_contract` | `inout-param\n` | single literal println('inout-param') |
| `p13_value_class_copy` | [11-copyability-layout-construction.md](../docs/guide/11-copyability-layout-construction.md) | `run_ok` | `single_println_literal` | `value\n` | single literal println('value') |
| `p14_nullable` | [09-nullability-errors.md](../docs/guide/09-nullability-errors.md) | `run_ok` | `v046_spec_contract` | `smart\nbangbang\n` | pick(true) returns non-null Node(5); smart cast prints 'smart' and pick(true)!!.v == 5 prints 'bangbang' |
| `p14b_nullable_simple` | [09-nullability-errors.md](../docs/guide/09-nullability-errors.md) | `run_ok` | `v046_spec_contract` | `smart\n` | a: Node? = Node(5) is non-null with a.v == 5; smart cast branch prints 'smart' |
| `p14c_nullable_int` | [09-nullability-errors.md](../docs/guide/09-nullability-errors.md) | `run_ok` | `v046_spec_contract` | `nint\n` | a: Int? = 5 is non-null and equals 5; smart cast branch prints 'nint' |
| `p15_elvis` | [09-nullability-errors.md](../docs/guide/09-nullability-errors.md) | `known_unimplemented` | `baseline_unimplemented_audited` | `BE:UnsupportedNode`, `BE:MissingFact`, `FE:L0165` → target `elvis\n` | f(null) evaluates null ?: 7 -> 7; 7 == 7 holds and prints 'elvis' (blocked by N03) |
| `p16_result_q` | [09-nullability-errors.md](../docs/guide/09-nullability-errors.md) | `known_unimplemented` | `v046_spec_contract` | `FE:L0165`, `BE:UnsupportedNode`, `BE:MissingFact` → target `question\n` | Result.Ok(21)? unwraps 21 in ? propagation; 21 * 2 == 42 prints 'question' (blocked by N08) |
| `p16b_result_string` | [09-nullability-errors.md](../docs/guide/09-nullability-errors.md) | `run_ok` | `v046_spec_contract` | `good\n` | parse(1) selects Result.Ok('good'); exhaustive when selects Ok and prints success once, followed by newline |
| `p17_lambda` | [07-calls-lambdas-closures.md](../docs/guide/07-calls-lambdas-closures.md) | `run_ok` | `v046_spec_contract` | `lambda\n` | capturing closure { n -> n * k } with k = 3 applied to 7 yields 21 and prints 'lambda' |
| `p17b_lambda_nocapture` | [07-calls-lambdas-closures.md](../docs/guide/07-calls-lambdas-closures.md) | `run_ok` | `v046_spec_contract` | `lambda0\n` | non-capturing lambda { n -> n * 3 } applied to 7 yields 21 and prints 'lambda0' |
| `p18_move_closure` | [07-calls-lambdas-closures.md](../docs/guide/07-calls-lambdas-closures.md) | `run_ok` | `v046_spec_contract` | `move\n` | move closure captures owned String 'x', compares s == 'x' -> true, and prints 'move' |
| `p19_mutablelist` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `source_trace_audited` | `list\n` | xs=[10,2,3] sums to 15 with size 3; condition holds and prints 'list' |
| `p20_map` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `single_println_literal` | `map\n` | single literal println('map') |
| `p21_box_enum` | [11-copyability-layout-construction.md](../docs/guide/11-copyability-layout-construction.md) | `known_unimplemented` | `baseline_unimplemented_audited` | `FE:L0165`, `BE:UnsupportedNode`, `BE:MissingFact` → target `box\n` | recursive Expr.Add(Box(Num(1)), Box(Num(2))) evaluates to 3 and prints 'box' (blocked by N07/L7) |
| `p22_rc` | [13-program-runtime-standard-library.md](../docs/guide/13-program-runtime-standard-library.md) | `run_ok` | `v046_spec_contract` | `rc\n` | Rc(Data(4)).share().value.n == 4 holds and prints 'rc' |
| `p23_global_var` | [05-declarations-callables.md](../docs/guide/05-declarations-callables.md) | `compile_fail` | `v046_spec_contract` | `FE:L0167` | top-level runtime var counter violates Guide 05 D4 (SPEC-0343); rejected with UNSUPPORTED_TOP_LEVEL_RUNTIME_VALUE L0167 |
| `p24_recursion` | [05-declarations-callables.md](../docs/guide/05-declarations-callables.md) | `run_ok` | `single_println_literal` | `fib\n` | single literal println('fib') |
| `p25_linked_iter` | [10-ownership-borrowing-drop.md](../docs/guide/10-ownership-borrowing-drop.md) | `compile_fail` | `baseline_diagnostic_audited` | `FE:L0133`, `FE:L0132` | cur = cur.next moves non-Copyable Node? field out of borrowed/owned chain, emitting MOVE_FROM_BORROWED_BINDING L0133 and PARTIAL_MOVE L0132 |
| `p25b_linked_recursive` | [10-ownership-borrowing-drop.md](../docs/guide/10-ownership-borrowing-drop.md) | `known_unimplemented` | `baseline_unimplemented_audited` | `BE:UnsupportedNode`, `BE:MissingFact`, `FE:L0165` → target `rec-list\n` | recursive sum(Node(1, Node(2, null))) returns 3 and prints 'rec-list' (blocked by N03) |
| `p26_borrow_return` | [10-ownership-borrowing-drop.md](../docs/guide/10-ownership-borrowing-drop.md) | `run_ok` | `source_trace_audited` | `borrowed\n` | nameOf borrows Holder('borrowed').name; println(n) prints 'borrowed' |
| `p27_thread` | [13-program-runtime-standard-library.md](../docs/guide/13-program-runtime-standard-library.md) | `known_unimplemented` | `baseline_unimplemented_audited` | `FE:L0080`, `BE:UnsupportedNode`, `BE:MissingFact` → target `thread\n` | thread(move { println('thread') }).join() runs worker closure and prints 'thread' (blocked by T1/N04) |
| `p28_companion` | [08-class-family-members.md](../docs/guide/08-class-family-members.md) | `known_unimplemented` | `v046_spec_contract` | `BE:UnsupportedNode`, `BE:UnsupportedSource`, `BE:MissingFact`, `FE:L0165` → target `companion\n` | anonymous companion function C.make() returns 7 and prints 'companion' (blocked by N06) |
| `p29_default_super` | [08-class-family-members.md](../docs/guide/08-class-family-members.md) | `run_ok` | `source_trace_audited` | `ab\n` | C.hi() concatenates super<A>.hi() ('a') + super<B>.hi() ('b') = 'ab' |
| `p30_runtime_array` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `single_println_literal` | `array\n` | single literal println('array') |
| `p31_string_ops` | [13-program-runtime-standard-library.md](../docs/guide/13-program-runtime-standard-library.md) | `run_ok` | `source_trace_audited` | `abc\n` | 'ab'.clone() + 'c' equals 'abc'; branch prints 'abc' |
| `p32_deinit` | [08-class-family-members.md](../docs/guide/08-class-family-members.md) | `run_ok` | `source_trace_audited` | `body\nb\na\n` | main prints 'body' first, then drops locals in reverse lexical order: b ('b') then a ('a') |
| `p33_range_in` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `single_println_literal` | `in-range\n` | single literal println('in-range') |
| `p34_for_range` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `source_trace_audited` | `for-range\n` | 0..<5 sums 0+1+2+3+4 = 10; s == 10 holds and prints 'for-range' |
| `p35_value_eq` | [03-types-generics.md](../docs/guide/03-types-generics.md) | `known_unimplemented` | `baseline_unimplemented_audited` | `BE:UnsupportedNode`, `BE:MissingFact`, `FE:L0165` → target `veq\n` | structural equality Point(1, 2) == Point(1, 2) holds and prints 'veq' (blocked by L1) |
| `p36_conversion` | [03-types-generics.md](../docs/guide/03-types-generics.md) | `run_ok` | `single_println_literal` | `conv\n` | single literal println('conv') |
| `p37_is_generic_case` | [03-types-generics.md](../docs/guide/03-types-generics.md) | `run_ok` | `v046_spec_contract` | `case-noargs\n` | Result.Ok(1) matches parameterless is Result.Ok with v = 1 (SPEC-0349); prints 'case-noargs' |
| `p38_list_of_strings` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `source_trace_audited` | `x\ny\n` | iterating mutableListOf('x', 'y') prints 'x' then 'y' in insertion order |
| `p39_removeAt` | [12-collections-destructuring.md](../docs/guide/12-collections-destructuring.md) | `run_ok` | `single_println_literal` | `removeAt\n` | single literal println('removeAt') |
| `p40_nested_generic_class` | [03-types-generics.md](../docs/guide/03-types-generics.md) | `known_unimplemented` | `baseline_unimplemented_audited` | `BE:UnsupportedNode`, `BE:MissingFact`, `FE:L0165` → target `stack\n` | Stack<Int>.push(1) mutates generic field items; size == 1 prints 'stack' (blocked by N01) |
| `p41_when_string` | [06-blocks-control-flow.md](../docs/guide/06-blocks-control-flow.md) | `run_ok` | `v046_spec_contract` | `when-str\n` | single literal println('when-str') |
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
| `q02_lambda_arg_inline` | [07-calls-lambdas-closures.md](../docs/guide/07-calls-lambdas-closures.md) | `run_ok` | `v046_spec_contract` | `lambda-arg\n` | twice({ n -> n + 1 }) applies +1 twice to 1 -> 3 and prints 'lambda-arg' |
| `q03_fn_ref` | [07-calls-lambdas-closures.md](../docs/guide/07-calls-lambdas-closures.md) | `known_unimplemented` | `baseline_unimplemented_audited` | `FE:L0165`, `BE:UnsupportedNode`, `BE:MissingFact` → target `fnref\n` | callable reference ::inc applied to 1 yields 2 and prints 'fnref' (blocked by N04) |
| `q04_rc_simple` | [13-program-runtime-standard-library.md](../docs/guide/13-program-runtime-standard-library.md) | `run_ok` | `single_println_literal` | `rc\n` | single literal println('rc') |
| `q05_nullable_param` | [09-nullability-errors.md](../docs/guide/09-nullability-errors.md) | `run_ok` | `v046_spec_contract` | `non-null\nnull\n` | show(Node(1)) takes non-null branch ('non-null'); show(null) takes else branch ('null') |
| `q06_nullable_return_bang` | [09-nullability-errors.md](../docs/guide/09-nullability-errors.md) | `run_ok` | `single_println_literal` | `bang\n` | single literal println('bang') |
| `q07_result_when_int` | [09-nullability-errors.md](../docs/guide/09-nullability-errors.md) | `run_ok` | `single_println_literal` | `res-int\n` | single literal println('res-int') |
| `q08_enum_string_payload` | [06-blocks-control-flow.md](../docs/guide/06-blocks-control-flow.md) | `known_unimplemented` | `baseline_unimplemented_audited` | `BE:UnsupportedNode`, `BE:MissingFact`, `FE:L0165` → target `hi\nquit\n` | show(Msg.Text('hi')) prints payload 'hi'; show(Msg.Quit) prints 'quit' (blocked by N02/L7) |
| `q09_inout_class_param` | [10-ownership-borrowing-drop.md](../docs/guide/10-ownership-borrowing-drop.md) | `run_ok` | `v046_spec_contract` | `inout-class\n` | single literal println('inout-class') |
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
| `s02_pair_builtin` | [02-names-files-packages.md](../docs/guide/02-names-files-packages.md) | `known_unimplemented` | `baseline_unimplemented_audited` | `FE:L0080`, `BE:UnsupportedNode`, `BE:MissingFact` → target `pair\n` | builtin Pair(1, 2).first == 1 holds and prints 'pair' (blocked by S6) |
| `s03_to_pair` | [02-names-files-packages.md](../docs/guide/02-names-files-packages.md) | `known_unimplemented` | `baseline_unimplemented_audited` | `FE:L0165`, `BE:UnsupportedNode`, `BE:MissingFact` → target `to\n` | infix 1 to 2 constructs Pair(1, 2); p.first == 1 prints 'to' (blocked by S6) |
| `s04_question_on_nullable` | [09-nullability-errors.md](../docs/guide/09-nullability-errors.md) | `compile_fail` | `baseline_diagnostic_audited` | `FE:L0085` | postfix ? applied to nullable Int? (instead of Result) emits INVALID_OPERAND_TYPES L0085 |
| `t01_vmethod_bare_field` | [08-class-family-members.md](../docs/guide/08-class-family-members.md) | `run_ok` | `single_println_literal` | `vmethod-bare\n` | single literal println('vmethod-bare') |
| `t02_vmethod_bool` | [08-class-family-members.md](../docs/guide/08-class-family-members.md) | `run_ok` | `single_println_literal` | `vmethod-bool\n` | single literal println('vmethod-bool') |
| `t03_vmethod_generic` | [03-types-generics.md](../docs/guide/03-types-generics.md) | `run_ok` | `v046_spec_contract` | `vmethod-generic\n` | generic value class W<Int>(4).get() returns 4 and prints 'vmethod-generic' |
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
| `v16_when_in_range` | [06-blocks-control-flow.md](../docs/guide/06-blocks-control-flow.md) | `run_ok` | `v046_spec_contract` | `small\n` | bucket(3) satisfies the `in 0..9` arm -> 'small' and prints 'small' |
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
