# SPEC-0427：`map_iteration`、`nullable_borrowed_cleanup`、`refine` 及 `lower_frontend` 壳子模块退役

> **性质**：工程收口规范（E3 Phase 4 文件数量收敛第十二批） · **状态**：in-progress · **读取时机**：收敛 `crates/lang-codegen/src/ssa` 下的 `lower_frontend/map_iteration/{conditional/actions, local_runtime/evidence, local/topology}`、`lower_frontend/nullable_borrowed_cleanup/provider`、`refine/binding_receipt/admission`、`refine/build/inout`、`lower_frontend/aggregate` 及 `refine/subset/when` 壳子模块时 · **唯一真源**：本文件定义 SPEC-0427 的合并范围、零语义漂移约束与验收条件

| 字段 | 内容 |
| --- | --- |
| 编号 | SPEC-0427 |
| 标题 | `map_iteration`、`nullable_borrowed_cleanup`、`refine` 及 `lower_frontend` 壳子模块退役 |
| 状态 | in-progress |
| 所属阶段 | M4 Phase 4（SSA 代码生成与 E3 收口） |
| 依赖 | SPEC-0416, SPEC-0417, SPEC-0418, SPEC-0419, SPEC-0420, SPEC-0421, SPEC-0422, SPEC-0423, SPEC-0424, SPEC-0425, SPEC-0426 |

## 1. 背景与目标

紧接 [SPEC-0426](0426-unit-lower-map-iteration-map-consume-container-and-unit-plan-shells-retirement.md)，继续推进 `crates/lang-codegen/src/ssa` 下 E3 细化层及遗留降低目录的壳模块收敛，将以下 8 个小规模子模块并入同域宿主模块并删除原壳文件，在保持编译单元与单文件 SSA lowering、细化层子集准入/构建以及可空与迭代清理证明行为不变的前提下进一步压缩碎片文件数量，同时确保合并后各文件严格低于 1000 行上限：

1. `lower_frontend/map_iteration/conditional/actions.rs`（411 行）-> 并入 `lower_frontend/map_iteration/conditional/verify.rs`（844 行）；
2. `lower_frontend/map_iteration/local_runtime/evidence.rs`（402 行）-> 并入 `lower_frontend/map_iteration/resource.rs`（828 行）；
3. `lower_frontend/nullable_borrowed_cleanup/provider.rs`（415 行）-> 并入 `lower_frontend/loop_control/numeric_iteration.rs`（778 行）；
4. `refine/binding_receipt/admission.rs`（427 行）-> 并入 `refine/receiver_receipt.rs`（921 行）；
5. `refine/build/inout.rs`（428 行）-> 并入 `refine/build/tagged.rs`（891 行）；
6. `lower_frontend/aggregate.rs`（432 行）-> 并入 `lower_frontend/map_consume.rs`（868 行）；
7. `lower_frontend/map_iteration/local/topology.rs`（433 行）-> 并入 `lower_frontend/map_iteration/source.rs`（898 行）；
8. `refine/subset/when.rs`（460 行）-> 并入 `refine/subset/named_route.rs`（926 行）。

## 2. 退役与合并清单

| 退役文件 | 并入目标文件 | 处理方式 |
| --- | --- | --- |
| `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/conditional/actions.rs` | `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/conditional/verify.rs` | 内联为 `pub(in crate::ssa::lower_frontend::map_iteration) mod actions` 并在 `conditional.rs` 中重导出 |
| `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/local_runtime/evidence.rs` | `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/resource.rs` | 内联为 `pub(in crate::ssa::lower_frontend::map_iteration) mod evidence` 并在 `local_runtime.rs` 中重导出 |
| `crates/lang-codegen/src/ssa/lower_frontend/nullable_borrowed_cleanup/provider.rs` | `crates/lang-codegen/src/ssa/lower_frontend/loop_control/numeric_iteration.rs` | 内联为 `pub(in crate::ssa::lower_frontend) mod provider` 并在 `nullable_borrowed_cleanup.rs` 中重导出 |
| `crates/lang-codegen/src/ssa/refine/binding_receipt/admission.rs` | `crates/lang-codegen/src/ssa/refine/receiver_receipt.rs` | 内联为 `pub(in crate::ssa::refine) mod admission` 并在 `binding_receipt.rs` 中重导出 |
| `crates/lang-codegen/src/ssa/refine/build/inout.rs` | `crates/lang-codegen/src/ssa/refine/build/tagged.rs` | 内联为 `pub(in crate::ssa::refine::build) mod inout` 并在 `refine/build.rs` 中重导出 |
| `crates/lang-codegen/src/ssa/lower_frontend/aggregate.rs` | `crates/lang-codegen/src/ssa/lower_frontend/map_consume.rs` | 内联 `resource_deinit` 子模块与 `ExpressionLowerer` 聚合投影/解构降低方法，并在 `lower_frontend.rs` 中重导出 |
| `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/local/topology.rs` | `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/source.rs` | 内联为 `pub(in crate::ssa::lower_frontend::map_iteration) mod topology` 并在 `local.rs` 中重导出 |
| `crates/lang-codegen/src/ssa/refine/subset/when.rs` | `crates/lang-codegen/src/ssa/refine/subset/named_route.rs` | 内联为私有子模块 `mod when` 承载 `Checker` 的 `when` 与 `Rc<T>` 子集准入方法 |

## 3. 验收条件

1. 上述 8 个壳文件已从版本库移除，合并后的宿主文件均严格小于 1000 行（通过 `python3 scripts/check_rust_sizes.py --base main`）。
2. `crates/lang-codegen/src/ssa/lowering_dependency_tests.rs` 中的所有存在性与依赖边界断言保持原样通过。
3. `python3 scripts/gen_spec_dag.py` 与 `python3 scripts/check_docs.py` 检查通过。
