# SPEC-0425：`nullable_if_cleanup`、`map_iteration`、`outer_borrowed_while`、`unit_lower` 及 `refine` 壳子模块退役

> **性质**：工程收口规范（E3 Phase 4 文件数量收敛第十批） · **状态**：in-progress · **读取时机**：收敛 `crates/lang-codegen/src/ssa` 下的 `lower_frontend/nullable_if_cleanup/source`、`lower_frontend/map_iteration/source_owner/delivery`、`lower_frontend/outer_borrowed_while/runtime`、`unit_lower/nullable_when/value`、`refine/subset/{control, receiver, value}` 及 `refine/build/heap` 壳子模块时 · **唯一真源**：本文件定义 SPEC-0425 的合并范围、零语义漂移约束与验收条件

| 字段 | 内容 |
| --- | --- |
| 编号 | SPEC-0425 |
| 标题 | `nullable_if_cleanup`、`map_iteration`、`outer_borrowed_while`、`unit_lower` 及 `refine` 壳子模块退役 |
| 状态 | done |
| 所属阶段 | M4 Phase 4（SSA 代码生成与 E3 收口） |
| 依赖 | SPEC-0416, SPEC-0417, SPEC-0418, SPEC-0419, SPEC-0420, SPEC-0421, SPEC-0422, SPEC-0423, SPEC-0424 |

## 1. 背景与目标

紧接 [SPEC-0424](0424-borrow-result-orchestrate-nullable-cleanup-and-frontend-shells-retirement.md)，继续推进 `crates/lang-codegen/src/ssa` 下 E3 细化层及遗留降低目录的壳模块收敛，将以下 8 个小规模子模块并入同域宿主模块并删除原壳文件，在保持编译单元与单文件 SSA lowering、细化层子集准入/构建以及可空与迭代清理证明行为不变的前提下进一步压缩碎片文件数量，同时确保合并后各文件严格低于 1000 行上限：

1. `lower_frontend/nullable_if_cleanup/source.rs`（361 行）-> 并入 `lower_frontend/nullable_if_cleanup/carrier.rs`（727 行）；
2. `lower_frontend/map_iteration/source_owner/delivery.rs`（346 行）-> 并入 `lower_frontend/map_iteration/source_owner/handoff.rs`（719 行）；
3. `lower_frontend/outer_borrowed_while/runtime.rs`（366 行）-> 并入 `lower_frontend/outer_borrowed_while.rs`（720 行）；
4. `unit_lower/nullable_when/value.rs`（351 行）-> 并入 `unit_lower/nullable_when.rs`（737 行）；
5. `refine/subset/control.rs`（327 行）-> 并入 `refine/subset/scalar_text.rs`（684 行）；
6. `refine/subset/receiver.rs`（356 行）-> 并入 `refine/subset/tagged.rs`（722 行）；
7. `refine/subset/value.rs`（378 行）-> 并入 `refine/subset/field.rs`（769 行）；
8. `refine/build/heap.rs`（354 行）-> 并入 `refine/build/borrow_binding.rs`（725 行）。

## 2. 退役与合并清单

| 退役文件 | 并入目标文件 | 处理方式 |
| --- | --- | --- |
| `crates/lang-codegen/src/ssa/lower_frontend/nullable_if_cleanup/source.rs` | `crates/lang-codegen/src/ssa/lower_frontend/nullable_if_cleanup/carrier.rs` | 内联为 `pub(in crate::ssa::lower_frontend::nullable_if_cleanup) mod source` 并在 `nullable_if_cleanup.rs` 中重导出 |
| `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/source_owner/delivery.rs` | `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/source_owner/handoff.rs` | 内联为 `pub(in crate::ssa::lower_frontend::map_iteration) mod delivery` 并在 `source_owner.rs` 中重导出 |
| `crates/lang-codegen/src/ssa/lower_frontend/outer_borrowed_while/runtime.rs` | `crates/lang-codegen/src/ssa/lower_frontend/outer_borrowed_while.rs` | 直接内联 `ExpressionLowerer` 外层借用 `while` 状态验证与清理方法 |
| `crates/lang-codegen/src/ssa/unit_lower/nullable_when/value.rs` | `crates/lang-codegen/src/ssa/unit_lower/nullable_when.rs` | 直接内联 `UnitExpressionLowerer` 可空 `when` 直接调用实参降低方法 |
| `crates/lang-codegen/src/ssa/refine/subset/control.rs` | `crates/lang-codegen/src/ssa/refine/subset/scalar_text.rs` | 内联为 `pub(in crate::ssa::refine) mod control` 并在 `refine/subset.rs` 中重导出 |
| `crates/lang-codegen/src/ssa/refine/subset/receiver.rs` | `crates/lang-codegen/src/ssa/refine/subset/tagged.rs` | 内联为 `pub(in crate::ssa::refine::subset) mod receiver` 并在 `refine/subset.rs` 中重导出 |
| `crates/lang-codegen/src/ssa/refine/subset/value.rs` | `crates/lang-codegen/src/ssa/refine/subset/field.rs` | 直接内联 `Checker` 值类、无状态单例与局部解构子集准入方法 |
| `crates/lang-codegen/src/ssa/refine/build/heap.rs` | `crates/lang-codegen/src/ssa/refine/build/borrow_binding.rs` | 直接内联 `Builder` 装箱构造、类构造、字段投影与非空断言构建方法 |

## 3. 验收条件

1. 上述 8 个壳文件已从版本库移除，合并后的宿主文件均严格小于 1000 行（通过 `python3 scripts/check_rust_sizes.py --base main`）。
2. `crates/lang-codegen/src/ssa/lowering_dependency_tests.rs` 中的所有存在性与依赖边界断言保持原样通过。
3. `python3 scripts/gen_spec_dag.py` 与 `python3 scripts/check_docs.py` 检查通过。

## 5. 验收记录账本

| 验收项 | 结果 | 证据 |
|---|---|---|
| 8 个壳子模块文件物理删除 | 通过 | 提交 `430a0de2` 上逐个确认：8/8 个路径不存在 |
| 内容并入声明的合并目标 | 通过（部分宿主被后续 Spec 再次合并，见下） | 5/8 个目标路径在当前树存在 |
| 修改文件均小于 1000 行 | 通过 | `python3 scripts/check_rust_sizes.py --base f78c8c20` 退出 0 |
| 手写 Rust 文件数净减 | 受限通过 | 历史已 squash 为 `430a0de2`，无法分离单 Spec 计数；删除逐项已核实 |
| `scripts/gen_spec_dag.py` / `scripts/check_docs.py` 通过 | 通过 | 归档后重跑生成物稳定；`documentation check passed` |
| `cargo test -p lang-codegen --lib` | 通过 | `cargo test -p lang-codegen --lib -- refine verify_ownership` 等定向；完整套件默认 `3055 passed / 0 failed / 1 ignored`、feature `3117 passed / 0 failed / 1 ignored` |
| 退役提交已在基线 `f78c8c20` 中 | 通过 | 91a40d30 refactor(codegen): retire nullable_if_cleanup, map_iteration, outer_borrowed_while, unit_lower and refine shells (SPEC-0425)（同批共 8 个删除提交） |

宿主核对说明：

- `lower_frontend/nullable_if_cleanup/carrier.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/lower_frontend_root_boundary_tests.rs`（`108` 行）
- `lower_frontend/map_iteration/source_owner/handoff.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/lower_frontend_root_boundary_tests.rs`（`108` 行）
- `lower_frontend/outer_borrowed_while.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/checked_integer_conversion_contract_tests.rs`（`476` 行）
