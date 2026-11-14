# SPEC-0423：`map_iteration`、`refine/build`、`unit_plan`、`unit_lower` 及可空清理壳子模块退役

> **性质**：工程收口规范（E3 Phase 4 文件数量收敛第八批） · **状态**：in-progress · **读取时机**：收敛 `crates/lang-codegen/src/ssa` 下的 `lower_frontend/map_iteration/{formed_runtime, formed/retained}`、`refine/build/{assignment, field}`、`unit_plan/deinit`、`unit_lower/field_borrow`、`lower_frontend/while_proof`、`unit_lower/cfg/nullable` 及 `lower_frontend/nullable_borrowed_cleanup/source` 壳子模块时 · **唯一真源**：本文件定义 SPEC-0423 的合并范围、零语义漂移约束与验收条件

| 字段 | 内容 |
| --- | --- |
| 编号 | SPEC-0423 |
| 标题 | `map_iteration`、`refine/build`、`unit_plan`、`unit_lower` 及可空清理壳子模块退役 |
| 状态 | done |
| 所属阶段 | M4 Phase 4（SSA 代码生成与 E3 收口） |
| 依赖 | SPEC-0416, SPEC-0417, SPEC-0418, SPEC-0419, SPEC-0420, SPEC-0421, SPEC-0422 |

## 1. 背景与目标

紧接 [SPEC-0422](0422-refine-emit-verify-instances-and-closure-shells-retirement.md)，继续推进 `crates/lang-codegen/src/ssa` 下 E3 细化层及遗留降低辅助目录的壳模块收敛，将以下 9 个小规模子模块并入同域宿主模块并删除原壳文件，在保持编译单元与单文件 SSA lowering、析构规划、精化层构建及所有权验证行为不变的前提下进一步压缩碎片文件数量，同时确保合并后各文件严格低于 1000 行上限：

1. `lower_frontend/map_iteration/formed_runtime.rs`（279 行）-> 并入 `lower_frontend/map_iteration/formed.rs`（748 行）；
2. `lower_frontend/map_iteration/formed/retained.rs`（316 行）-> 并入 `lower_frontend/map_iteration/local.rs`（673 行）；
3. `refine/build/assignment.rs`（312 行）与 `refine/build/field.rs`（346 行）-> 一并并入 `refine/build/value.rs`（948 行）；
4. `unit_plan/deinit.rs`（341 行）-> 并入 `unit_plan/concrete_types.rs`（630 行）；
5. `unit_lower/field_borrow.rs`（302 行）-> 并入 `unit_lower/container.rs`（607 行）；
6. `lower_frontend/while_proof.rs`（314 行）-> 并入 `lower_frontend/nullable_when.rs`（627 行）；
7. `unit_lower/cfg/nullable.rs`（325 行）-> 并入 `unit_lower/cfg/rebind.rs`（638 行）；
8. `lower_frontend/nullable_borrowed_cleanup/source.rs`（330 行）-> 并入 `lower_frontend/nullable_borrowed_cleanup/carrier.rs`（646 行）。

## 2. 退役与合并清单

| 退役文件 | 并入目标文件 | 处理方式 |
| --- | --- | --- |
| `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/formed_runtime.rs` | `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/formed.rs` | 直接内联 `ExpressionLowerer` 实现与传输校验辅助 |
| `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/formed/retained.rs` | `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/local.rs` | 内联为 `pub(in crate::ssa::lower_frontend::map_iteration) mod retained` 并在 `formed.rs` 中重导出 |
| `crates/lang-codegen/src/ssa/refine/build/assignment.rs` | `crates/lang-codegen/src/ssa/refine/build/value.rs` | 直接内联 `Builder` 赋值、复合赋值与返回方法 |
| `crates/lang-codegen/src/ssa/refine/build/field.rs` | `crates/lang-codegen/src/ssa/refine/build/value.rs` | 内联为 `pub(in crate::ssa::refine::build) mod field` 并在 `refine/build.rs` 中重导出 |
| `crates/lang-codegen/src/ssa/unit_plan/deinit.rs` | `crates/lang-codegen/src/ssa/unit_plan/concrete_types.rs` | 内联为 `pub(in crate::ssa::unit_plan) mod deinit` 并在 `unit_plan.rs` 中重导出 |
| `crates/lang-codegen/src/ssa/unit_lower/field_borrow.rs` | `crates/lang-codegen/src/ssa/unit_lower/container.rs` | 直接内联 `UnitExpressionLowerer` 字段借用方法 |
| `crates/lang-codegen/src/ssa/lower_frontend/while_proof.rs` | `crates/lang-codegen/src/ssa/lower_frontend/nullable_when.rs` | 直接内联 `ExpressionLowerer` `while` 非空证明方法 |
| `crates/lang-codegen/src/ssa/unit_lower/cfg/nullable.rs` | `crates/lang-codegen/src/ssa/unit_lower/cfg/rebind.rs` | 内联为 `pub(in crate::ssa::unit_lower) mod nullable` 并在 `unit_lower/cfg.rs` 中重导出 |
| `crates/lang-codegen/src/ssa/lower_frontend/nullable_borrowed_cleanup/source.rs` | `crates/lang-codegen/src/ssa/lower_frontend/nullable_borrowed_cleanup/carrier.rs` | 内联为 `pub(in crate::ssa::lower_frontend::nullable_borrowed_cleanup) mod source` 并在 `nullable_borrowed_cleanup.rs` 中重导出 |

## 3. 验收条件

1. 上述 9 个壳文件已从版本库移除，合并后的宿主文件均严格小于 1000 行（通过 `python3 scripts/check_rust_sizes.py --base main`）。
2. `crates/lang-codegen/src/ssa/lowering_dependency_tests.rs` 中的所有存在性与依赖边界断言保持原样通过。
3. `python3 scripts/gen_spec_dag.py` 与 `python3 scripts/check_docs.py` 检查通过。

## 5. 验收记录账本

| 验收项 | 结果 | 证据 |
|---|---|---|
| 9 个壳子模块文件物理删除 | 通过 | 提交 `430a0de2` 上逐个确认：9/9 个路径不存在 |
| 内容并入声明的合并目标 | 通过（部分宿主被后续 Spec 再次合并，见下） | 5/9 个目标路径在当前树存在 |
| 修改文件均小于 1000 行 | 通过 | `python3 scripts/check_rust_sizes.py --base f78c8c20` 退出 0 |
| 手写 Rust 文件数净减 | 受限通过 | 历史已 squash 为 `430a0de2`，无法分离单 Spec 计数；删除逐项已核实 |
| `scripts/gen_spec_dag.py` / `scripts/check_docs.py` 通过 | 通过 | 归档后重跑生成物稳定；`documentation check passed` |
| `cargo test -p lang-codegen --lib` | 通过 | `cargo test -p lang-codegen --lib -- refine verify_ownership` 等定向；完整套件默认 `3055 passed / 0 failed / 1 ignored`、feature `3117 passed / 0 failed / 1 ignored` |
| 退役提交已在基线 `f78c8c20` 中 | 通过 | aa4bbe52 refactor(codegen): retire map_iteration, refine/build, unit_plan, unit_lower and nullable shells (SPEC-0423)（同批共 9 个删除提交） |

宿主核对说明：

- `lower_frontend/map_iteration/formed.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/unit_lower_control_result_tests.rs`（`667` 行）
- `lower_frontend/map_iteration/local.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/lower_frontend_map_continue_structure_tests.rs`（`72` 行）
- `lower_frontend/nullable_when.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/expression_core/checked.rs`（`465` 行）
- `lower_frontend/nullable_borrowed_cleanup/carrier.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/lower_frontend_root_boundary_tests.rs`（`108` 行）
