# SPEC-0451: 跨父模块测试薄壳批量打包收敛（22 个）

> **性质**：正式开发规范（Spec） · **状态**：in-progress · **读取时机**：收敛 `crates/lang-codegen/src/ssa` 遗留壳子模块时 · **唯一真源**：`crates/lang-codegen/src/ssa/`

| 字段 | 值 |
|---|---|
| 编号 | 0451 |
| 标题 | 跨父模块测试薄壳批量打包收敛（22 个） |
| 版本目标 | v0.46 |
| 状态 | in-progress |
| 创建日期 | 2026-11-12 |
| 完成日期 | - |
| 关联里程碑 | M2（借用系统）、M3（单线程完备） |
| 关联迭代 | E3（单下降管道收敛） |
| 关联 ADR | ADR-0006, ADR-0008, ADR-0032, ADR-0033 |
| 前置依赖 | SPEC-0381, SPEC-0383, SPEC-0449, SPEC-0450 |

---

## 1. 背景与动机

在 [SPEC-0450](../../archive/specs/0450-next-layer-leaf-and-sibling-shells-batch-retirement.md) 完成同级兄弟测试薄壳批量收敛后，直属父模块与同级宿主行高已接近 985 行阈值，但全 `ssa` 测试域仍分布有 22 个体积较小（169–486 行）的 `#[cfg(test)]` 门控纯叶子测试模块，同时多处跨子树测试宿主仍存在充足的行宽余量。本 Spec 通过跨父模块安全打包内联（`pub(in crate::ssa) mod <name> { ... }`）并将原父模块声明替换为 `#[allow(unused_imports)] pub(in crate::ssa) use <host>::<name> as <orig>;`，一次性收敛这 22 个独立文件。

---

## 2. 设计与收敛方案

### 2.1 收敛后父模块与跨父宿主行高

| 模块路径 | 收敛后行高 |
|---|---:|
| `ssa/lower_frontend_map_break_tests.rs` | 924 |
| `ssa/lower_frontend_owned_source_continue_tests.rs` | 866 |
| `ssa/lower_frontend_owned_source_return_native_tests.rs` | 642 |
| `ssa/lower_frontend_owned_source_return_tests.rs` | 972 |
| `ssa/lower_frontend_resource_deinit_tests/direct_closure.rs` | 890 |
| `ssa/lower_frontend_tests.rs` | 980 |
| `ssa/lowering_entry_contract_tests.rs` | 989 |
| `ssa/map_iteration_stage_gate_tests.rs` | 870 |
| `ssa/map_provider_exits_pipeline_tests.rs` | 950 |
| `ssa/map_provider_exits_pipeline_tests/births.rs` | 624 |
| `ssa/map_provider_exits_pipeline_tests/births/facts.rs` | 889 |
| `ssa/map_provider_exits_pipeline_tests/future_use/graph.rs` | 955 |
| `ssa/map_provider_exits_pipeline_tests/local_exits.rs` | 581 |
| `ssa/map_provider_exits_pipeline_tests/local_exits/boundaries.rs` | 867 |
| `ssa/map_provider_exits_pipeline_tests/local_exits/graph.rs` | 741 |
| `ssa/map_provider_exits_pipeline_tests/local_future.rs` | 738 |
| `ssa/map_provider_exits_pipeline_tests/local_future/graph.rs` | 949 |
| `ssa/map_provider_exits_pipeline_tests/local_future/graph/continuation.rs` | 854 |
| `ssa/map_provider_exits_pipeline_tests/local_reassignment/graph.rs` | 639 |
| `ssa/map_provider_exits_pipeline_tests/local_resources/graph.rs` | 709 |
| `ssa/map_provider_exits_pipeline_tests/local_resources/graph/provider.rs` | 958 |
| `ssa/map_provider_exits_pipeline_tests/nested_exits.rs` | 900 |
| `ssa/map_provider_exits_pipeline_tests/nested_exits/ast.rs` | 750 |
| `ssa/map_provider_exits_pipeline_tests/nested_exits/query_cleanup.rs` | 972 |
| `ssa/map_provider_exits_pipeline_tests/resource.rs` | 787 |
| `ssa/map_provider_exits_pipeline_tests/resource/continuation.rs` | 909 |
| `ssa/map_provider_exits_pipeline_tests/scope_future_observer.rs` | 972 |
| `ssa/map_provider_exits_pipeline_tests/scope_future_observer/graph.rs` | 534 |
| `ssa/map_provider_exits_pipeline_tests/scope_future_observer/graph/provider.rs` | 943 |
| `ssa/map_provider_exits_pipeline_tests/scope_lambda_body.rs` | 675 |
| `ssa/map_provider_exits_pipeline_tests/scope_lambda_body/graph.rs` | 967 |
| `ssa/map_provider_exits_pipeline_tests/scope_lowering/graph.rs` | 595 |
| `ssa/map_provider_exits_pipeline_tests/scope_member_parameter.rs` | 815 |
| `ssa/map_provider_exits_pipeline_tests/scope_member_parameter/graph.rs` | 914 |
| `ssa/map_provider_exits_pipeline_tests/scope_value_births.rs` | 869 |
| `ssa/map_provider_exits_pipeline_tests/scope_value_births/borrow_alias_observer.rs` | 540 |
| `ssa/map_provider_exits_pipeline_tests/scope_value_births/borrow_alias_observer/facts.rs` | 851 |
| `ssa/map_provider_exits_pipeline_tests/temporary.rs` | 858 |
| `ssa/map_provider_exits_pipeline_tests/temporary_delivery/graph.rs` | 834 |
| `ssa/map_provider_exits_pipeline_tests/temporary_exits/ast.rs` | 679 |
| `ssa/map_provider_exits_pipeline_tests/temporary_exits/query_cleanup.rs` | 977 |
| `ssa/map_provider_exits_pipeline_tests/temporary_forms.rs` | 741 |
| `ssa/map_provider_exits_pipeline_tests/temporary_forms/graph.rs` | 928 |
| `ssa/mod.rs` | 969 |
| `ssa/nullable_map_consume_lowering_tests.rs` | 939 |
| `ssa/nullable_map_consume_lowering_tests/consume_identity.rs` | 969 |
| `ssa/nullable_map_consume_lowering_tests/consume_identity/source.rs` | 925 |
| `ssa/nullable_map_consume_lowering_tests/read_identity.rs` | 611 |
| `ssa/nullable_map_consume_lowering_tests/resource/formed_locals.rs` | 975 |
| `ssa/nullable_operation_tests.rs` | 825 |
| `ssa/refine_subset_tests/temporary_sequential_for.rs` | 695 |
| `ssa/refine_tests/abort_owner_assertions.rs` | 807 |
| `ssa/sequential_for_lowering_tests.rs` | 799 |
| `ssa/sequential_for_lowering_tests/cleanup_tests.rs` | 699 |
| `ssa/sequential_for_lowering_tests/source_cfg_tests.rs` | 708 |
| `ssa/text_copy_tests.rs` | 889 |
| `ssa/unit_lower.rs` | 883 |
| `ssa/unit_lower_closure_tests/direct_prefix.rs` | 783 |
| `ssa/unit_lower_receiver_tests.rs` | 874 |
| `ssa/unit_lower_receiver_tests/inout_fields.rs` | 970 |
| `ssa/unit_lower_type_plan_tests.rs` | 713 |

### 2.2 退役纯叶子测试薄壳清单（22 个）

- `ssa/lower_frontend_map_break_structure_tests.rs`
- `ssa/lower_frontend_owned_source_structure_tests.rs`
- `ssa/lower_frontend_scalar_abi_unit_comparison_tests.rs`
- `ssa/lowering_entry_contract_tests/temporary_alias.rs`
- `ssa/map_iteration_stage_gate_tests/fixture.rs`
- `ssa/map_provider_exits_pipeline_tests/future_use/graph/continuation.rs`
- `ssa/map_provider_exits_pipeline_tests/local_exits/graph/provider.rs`
- `ssa/map_provider_exits_pipeline_tests/local_reassignment/graph/provider.rs`
- `ssa/map_provider_exits_pipeline_tests/nested_exits/ast_checks.rs`
- `ssa/map_provider_exits_pipeline_tests/nested_exits/query_callers.rs`
- `ssa/map_provider_exits_pipeline_tests/nested_exits/query_cleanup_unit.rs`
- `ssa/map_provider_exits_pipeline_tests/scope_lowering/graph/body.rs`
- `ssa/map_provider_exits_pipeline_tests/temporary/unit.rs`
- `ssa/map_provider_exits_pipeline_tests/temporary_delivery/graph/factory.rs`
- `ssa/map_provider_exits_pipeline_tests/temporary_exits/ast_checks.rs`
- `ssa/map_provider_exits_pipeline_tests/temporary_exits/query_cleanup_unit.rs`
- `ssa/nullable_map_consume_lowering_tests/read_identity/unit.rs`
- `ssa/nullable_map_consume_lowering_tests/resource/formed_locals/syntax.rs`
- `ssa/nullable_operation_tests/transport.rs`
- `ssa/text_copy_tests/nullable_return.rs`
- `ssa/unit_legacy_instance_witness_tests.rs`
- `ssa/unit_lower_deinit_tests.rs`

---

## 3. 验收标准

1. 上述 22 个薄壳文件从物理树移除，全部非例外修改后 Rust 文件严格小于 1000 行。
2. `python3 scripts/gen_spec_dag.py`、`python3 scripts/check_docs.py` 与 `python3 scripts/check_rust_sizes.py --base main` 通过，手写 Rust 文件数从 `1833` 降至 `1811`。

## 4. 集成后逐项对账与首个安全候选

核对输入：已推送的集成检查点 `503fac88`。§2.1 的行高及 §3 的文件总数是原批次目标，
不是当前树的验收结果；本轮不修改原验收标准、不归档，也不把删除文件等同于 E3 完成。

### 4.1 22 项文件身份与父模块容量

输入检查点中 22 个文件均存在。下表的“行数和”是当时子文件与原父模块物理行数相加，
仅用于筛选，不是内联后的精确尺寸；21 项行数和达到或超过 1000，不能直接据此并入原父。
跨域搬迁曾造成私有 helper、fixture 与模块挂载不一致，不能只按宿主剩余行数重新打包。

| §2.2 文件（相对 `crates/lang-codegen/src/`） | 输入子文件行数 | 输入父模块行数 | 本轮处理 |
|---|---:|---:|---|
| `ssa/lower_frontend_map_break_structure_tests.rs` | 413 | 947 | 保留；直接内联容量不足，待职责方案 |
| `ssa/lower_frontend_owned_source_structure_tests.rs` | 277 | 867 | 保留；直接内联容量不足，待职责方案 |
| `ssa/lower_frontend_scalar_abi_unit_comparison_tests.rs` | 486 | 998 | 保留；直接内联容量不足，待职责方案 |
| `ssa/lowering_entry_contract_tests/temporary_alias.rs` | 333 | 992 | 保留；直接内联容量不足，待职责方案 |
| `ssa/map_iteration_stage_gate_tests/fixture.rs` | 342 | 890 | 保留；直接内联容量不足，待职责方案 |
| `ssa/map_provider_exits_pipeline_tests/future_use/graph/continuation.rs` | 412 | 955 | 保留；直接内联容量不足，待职责方案 |
| `ssa/map_provider_exits_pipeline_tests/local_exits/graph/provider.rs` | 424 | 741 | 保留；直接内联容量不足，待职责方案 |
| `ssa/map_provider_exits_pipeline_tests/local_reassignment/graph/provider.rs` | 358 | 639 | 并回原父 `local_reassignment/graph.rs`；格式化后 998 行 |
| `ssa/map_provider_exits_pipeline_tests/nested_exits/ast_checks.rs` | 376 | 755 | 保留；直接内联容量不足，待职责方案 |
| `ssa/map_provider_exits_pipeline_tests/nested_exits/query_callers.rs` | 448 | 906 | 保留；直接内联容量不足，待职责方案 |
| `ssa/map_provider_exits_pipeline_tests/nested_exits/query_cleanup_unit.rs` | 206 | 987 | 保留；直接内联容量不足，待职责方案 |
| `ssa/map_provider_exits_pipeline_tests/scope_lowering/graph/body.rs` | 449 | 595 | 保留；直接内联容量不足，待职责方案 |
| `ssa/map_provider_exits_pipeline_tests/temporary/unit.rs` | 383 | 872 | 保留；直接内联容量不足，待职责方案 |
| `ssa/map_provider_exits_pipeline_tests/temporary_delivery/graph/factory.rs` | 328 | 835 | 保留；直接内联容量不足，待职责方案 |
| `ssa/map_provider_exits_pipeline_tests/temporary_exits/ast_checks.rs` | 465 | 680 | 保留；直接内联容量不足，待职责方案 |
| `ssa/map_provider_exits_pipeline_tests/temporary_exits/query_cleanup_unit.rs` | 169 | 989 | 保留；直接内联容量不足，待职责方案 |
| `ssa/nullable_map_consume_lowering_tests/read_identity/unit.rs` | 478 | 611 | 保留；直接内联容量不足，待职责方案 |
| `ssa/nullable_map_consume_lowering_tests/resource/formed_locals/syntax.rs` | 408 | 977 | 保留；直接内联容量不足，待职责方案 |
| `ssa/nullable_operation_tests/transport.rs` | 337 | 826 | 保留；直接内联容量不足，待职责方案 |
| `ssa/text_copy_tests/nullable_return.rs` | 408 | 889 | 保留；直接内联容量不足，待职责方案 |
| `ssa/unit_legacy_instance_witness_tests.rs` | 411 | 716 | 保留；直接内联容量不足，待职责方案 |
| `ssa/unit_lower_deinit_tests.rs` | 383 | 899 | 保留；直接内联容量不足，待职责方案 |

### 4.2 `local_reassignment` provider 的有界验证

将原 `mod provider;` 改为同名内联模块，保持完整模块路径、`use super::*`、
`pub(super)` 和正文不变；去掉外层缩进后，正文与输入子文件逐字节相同。
不新增跨域重导出，不扩展可见性，不修改 fixture、断言或测试筛选条件。

| 检查 | 实际结果 |
|---|---|
| `cargo test -p lang-codegen --lib --features loan-observation-test-support -- ssa::map_provider_exits_pipeline_tests::local_reassignment` | 修改前、修改后均 6 passed / 0 failed，退出码 0；通过项完整名称一致 |
| 同 feature 配置的 native `single_grouped_replacements_preserve_value_histories` 与 `unit_grouped_replacements_preserve_value_histories` | 2 passed / 0 failed，退出码 0；两条入口各运行 6 个真实源码 fixture，保留 stdout/cleanup oracle |
| 默认与 feature 的 `cargo clippy -p lang-codegen --all-targets -- -D warnings` | 均退出码 0 |
| fmt、文档与 Rust 尺寸检查 | 均退出码 0；内联宿主 998 行，未提高尺寸上限 |
| 独立只读审查 | 检查旧正文、完整 diff、模块作用域、调用方、fixture 与测试身份，未发现具体问题；仅覆盖本候选 |

当前文件删除进度为 **1/22**，剩余 21 项保持待办，不能宣称本 Spec 完成。
后续须先决定符合职责边界的收敛方案，再逐项验证；不得恢复无关宿主打包或放宽尺寸护栏。
全量回归与双宿主 CI 沿用 SPEC-0453 §4 的未验收边界，本轮定向通过不替代它们。
