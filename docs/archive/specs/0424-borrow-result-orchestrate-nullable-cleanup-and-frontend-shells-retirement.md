# SPEC-0424：`borrow_result_support`、`orchestrate`、可空清理及 `lower_frontend` 壳子模块退役

> **性质**：工程收口规范（E3 Phase 4 文件数量收敛第九批） · **状态**：in-progress · **读取时机**：收敛 `crates/lang-codegen/src/ssa` 下的 `borrow_result_support/ordinary_local`、`lower_frontend/orchestrate/nullable_map_guard_support/prelude`、`lower_frontend/nullable_numeric_cleanup/source`、`lower_frontend/borrow_argument/nullable_field`、`lower_frontend/{integer, declarations, nullable_comparison}` 及 `lower_frontend/nullable_if_cleanup/constant_presence/source_return` 壳子模块时 · **唯一真源**：本文件定义 SPEC-0424 的合并范围、零语义漂移约束与验收条件

| 字段 | 内容 |
| --- | --- |
| 编号 | SPEC-0424 |
| 标题 | `borrow_result_support`、`orchestrate`、可空清理及 `lower_frontend` 壳子模块退役 |
| 状态 | done |
| 所属阶段 | M4 Phase 4（SSA 代码生成与 E3 收口） |
| 依赖 | SPEC-0416, SPEC-0417, SPEC-0418, SPEC-0419, SPEC-0420, SPEC-0421, SPEC-0422, SPEC-0423 |

## 1. 背景与目标

紧接 [SPEC-0423](0423-map-iteration-refine-build-unit-plan-and-nullable-shells-retirement.md)，继续推进 `crates/lang-codegen/src/ssa` 下 E3 细化层及遗留降低辅助目录的壳模块收敛，将以下 8 个小规模子模块并入同域宿主模块并删除原壳文件，在保持编译单元与单文件 SSA lowering、可空清理证明与借用结果校验行为不变的前提下进一步压缩碎片文件数量，同时确保合并后各文件严格低于 1000 行上限：

1. `borrow_result_support/ordinary_local.rs`（321 行）-> 并入 `borrow_result_support/local_if.rs`（702 行）；
2. `lower_frontend/orchestrate/nullable_map_guard_support/prelude.rs`（323 行）-> 并入 `lower_frontend/orchestrate/nullable_map_guard_support/checkpoint.rs`（669 行）；
3. `lower_frontend/nullable_numeric_cleanup/source.rs`（325 行）-> 并入 `lower_frontend/nullable_numeric_cleanup/carrier.rs`（684 行）；
4. `lower_frontend/borrow_argument/nullable_field.rs`（332 行）-> 并入 `lower_frontend/borrow_argument.rs`（657 行）；
5. `lower_frontend/integer.rs`（322 行）-> 并入 `lower_frontend/local_storage.rs`（664 行）；
6. `lower_frontend/declarations.rs`（327 行）-> 并入 `lower_frontend/string_clone.rs`（663 行）；
7. `lower_frontend/nullable_comparison.rs`（345 行）-> 并入 `lower_frontend/ownership_primitive.rs`（689 行）；
8. `lower_frontend/nullable_if_cleanup/constant_presence/source_return.rs`（331 行）-> 并入 `lower_frontend/nullable_if_cleanup/constant_presence/source.rs`（758 行）。

## 2. 退役与合并清单

| 退役文件 | 并入目标文件 | 处理方式 |
| --- | --- | --- |
| `crates/lang-codegen/src/ssa/borrow_result_support/ordinary_local.rs` | `crates/lang-codegen/src/ssa/borrow_result_support/local_if.rs` | 内联为 `pub(in crate::ssa) mod ordinary_local` 并在 `borrow_result_support.rs` 中重导出 |
| `crates/lang-codegen/src/ssa/lower_frontend/orchestrate/nullable_map_guard_support/prelude.rs` | `crates/lang-codegen/src/ssa/lower_frontend/orchestrate/nullable_map_guard_support/checkpoint.rs` | 直接内联 `Fixture` 前导重放与指令辅助方法 |
| `crates/lang-codegen/src/ssa/lower_frontend/nullable_numeric_cleanup/source.rs` | `crates/lang-codegen/src/ssa/lower_frontend/nullable_numeric_cleanup/carrier.rs` | 内联为 `pub(in crate::ssa::lower_frontend::nullable_numeric_cleanup) mod source` 并在 `nullable_numeric_cleanup.rs` 中重导出 |
| `crates/lang-codegen/src/ssa/lower_frontend/borrow_argument/nullable_field.rs` | `crates/lang-codegen/src/ssa/lower_frontend/borrow_argument.rs` | 直接内联 `ExpressionLowerer` 字段借用方法 |
| `crates/lang-codegen/src/ssa/lower_frontend/integer.rs` | `crates/lang-codegen/src/ssa/lower_frontend/local_storage.rs` | 内联为 `pub(in crate::ssa::lower_frontend) mod integer` 并在 `lower_frontend.rs` 中重导出 |
| `crates/lang-codegen/src/ssa/lower_frontend/declarations.rs` | `crates/lang-codegen/src/ssa/lower_frontend/string_clone.rs` | 内联为 `pub(in crate::ssa::lower_frontend) mod declarations` 并在 `lower_frontend.rs` 中重导出 |
| `crates/lang-codegen/src/ssa/lower_frontend/nullable_comparison.rs` | `crates/lang-codegen/src/ssa/lower_frontend/ownership_primitive.rs` | 直接内联 `ExpressionLowerer` 非空断言与空值比较降低方法 |
| `crates/lang-codegen/src/ssa/lower_frontend/nullable_if_cleanup/constant_presence/source_return.rs` | `crates/lang-codegen/src/ssa/lower_frontend/nullable_if_cleanup/constant_presence/source.rs` | 内联为 `pub(in crate::ssa::lower_frontend::nullable_if_cleanup::constant_presence) mod source_return` 并在 `nullable_if_cleanup.rs` 中重导出 |

## 3. 验收条件

1. 上述 8 个壳文件已从版本库移除，合并后的宿主文件均严格小于 1000 行（通过 `python3 scripts/check_rust_sizes.py --base main`）。
2. `crates/lang-codegen/src/ssa/lowering_dependency_tests.rs` 中的所有存在性与依赖边界断言保持原样通过。
3. `python3 scripts/gen_spec_dag.py` 与 `python3 scripts/check_docs.py` 检查通过。

## 5. 验收记录账本

| 验收项 | 结果 | 证据 |
|---|---|---|
| 8 个壳子模块文件物理删除 | 通过 | 提交 `430a0de2` 上逐个确认：8/8 个路径不存在 |
| 内容并入声明的合并目标 | 通过（部分宿主被后续 Spec 再次合并，见下） | 1/8 个目标路径在当前树存在 |
| 修改文件均小于 1000 行 | 通过 | `python3 scripts/check_rust_sizes.py --base f78c8c20` 退出 0 |
| 手写 Rust 文件数净减 | 受限通过 | 历史已 squash 为 `430a0de2`，无法分离单 Spec 计数；删除逐项已核实 |
| `scripts/gen_spec_dag.py` / `scripts/check_docs.py` 通过 | 通过 | 归档后重跑生成物稳定；`documentation check passed` |
| `cargo test -p lang-codegen --lib` | 通过 | `cargo test -p lang-codegen --lib -- refine verify_ownership` 等定向；完整套件默认 `3055 passed / 0 failed / 1 ignored`、feature `3117 passed / 0 failed / 1 ignored` |
| 退役提交已在基线 `f78c8c20` 中 | 通过 | b7c8b283 refactor(codegen): retire borrow_result_support, orchestrate, nullable cleanup and lower_frontend shells (SPEC-0424)（同批共 8 个删除提交） |

宿主核对说明：

- `lower_frontend/orchestrate/nullable_map_guard_support/checkpoint.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/unit_receiver_two_phase_control_tests.rs`（`137` 行）
- `lower_frontend/nullable_numeric_cleanup/carrier.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/borrow_memory_test_support.rs`（`360` 行）
- `lower_frontend/borrow_argument.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/mod.rs`（`677` 行）
- `lower_frontend/local_storage.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/native_nullable_when_tests/replacement_if.rs`（`863` 行）
- `lower_frontend/string_clone.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/statement_core.rs`（`7` 行）
- `lower_frontend/ownership_primitive.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/integer_range_pending_tests.rs`（`517` 行）
- `lower_frontend/nullable_if_cleanup/constant_presence/source.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/deinit_tests.rs`（`218` 行）
