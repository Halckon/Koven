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

## 4. 本地验收记录

- 构建修复（`SPEC-0422`～`SPEC-0427` 折叠后本分支从未通过编译；以下均为机械缺口，不改变 lowering 语义）：
  - 内联模块 `#[path]` 归位：`nullable_numeric_cleanup/carrier{tests}`、`unit_lower/closure/layout{tests}`、`verify_operation/integer/map/{require_tests,tests,with_tests}`、`verify_ownership/aliases/provider_lifetime/{projection_tests,tests}` 移入 rustc 对内联模块期望的目录并去掉重复前缀（模块路径不变，文件内 `super::` 语义未受影响）。
  - 折叠后丢失的导入与可见性：`unit_lower::constant` 的 `branch` 子模块补 `BranchExit`；`ControlResult::{control,string}` 与 `lower_conditional` 提升到 `unit_lower` 可见；`map_iteration::source::topology` 补 `DropFact`/`DropTarget`；`single_readonly_method_tests::boundaries` 恢复 `use super::*`（折叠后误用 `super::super::*`）；`verify_operation::aggregate_explode_contract` 提升为 `pub(in crate::ssa::verify_operation)`；`NullableConsumeSource` 可见性与消费方法对齐。
  - 折叠后路径深度变化：`outer_borrowed_while` 的 `guard_tests` 由 `super::super::carrier` 改为 `super::carrier`；`verify_operation/integer/map/tests` 改用 `crate::ssa::verify_operation::aggregate_explode_contract`；`binding_receipt` 的 `ci_tests` 改用父模块重导出的 `catalog_actions`（并把该重导出门控到 `#[cfg(test)]`）。
  - 未使用导入：删除 `lower_frontend::local_storage::integer`、`map_iteration/local_runtime` 的 `CallableTarget`。
- 门禁（本地，`LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21`，与 CI 同一 Clang 21.1 前提）：
  - `cargo test -p lang-codegen`：lib 2914 passed / 1 ignored、集成 2 passed、doc-tests 4 passed。
  - `cargo clippy --locked --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`python3 scripts/check_docs.py`、`python3 scripts/check_rust_sizes.py --base main`、`git diff --check` 全部通过（本片无需新增体积例外）。
- 说明：本分支按 `SPEC-0422`～`SPEC-0427` 堆叠在 `SPEC-0421` 之上，PR 将一并交付 `SPEC-0422`～`SPEC-0427`。

## 5. main 合并记录（PR #412 凭证协议）

- 冲突面：`ssa/refine.rs`、`ssa/refine/build.rs`、`ssa/refine/build/{field,inout}.rs`（本片折叠删除，main 有修改）、`ssa/refine/emit/statements.rs`（同上）。
- 解决方式：**采纳 main 更新的凭证协议**（`nested`/`bounded`/`single`/`multi`/`branch`/`forward`/`aux`/`child` 等 receipt 模块、`emit/statements` 作为独立模块、main 版 `refine.rs`/`emit.rs`），同时**保留本片折叠**：`build/assignment.rs` 仍折叠进 `build/value.rs`，`build/heap.rs` 仍折叠进 `build/borrow_binding.rs`；`build/field`、`build/inout` 以 `pub(in crate::ssa::refine::build) use` 别名保留折叠前的模块名，使 main 新增调用点无需改动即可编译。
- 迁移 main 侧两处有界 diff：`local_variable` 的 `counter_binding_authorization`/`counter_binding_emitted`，以及 `field_replace_primitive` 的 `counter_replace_*`、`open_loop_replace_emitted`、`header_replace_emitted` 接线，均落在折叠后的宿主模块中。
- 体积：合并后 `build/borrow_binding.rs`（折叠 `heap` + main 凭证接线）达 1304 行，按策略在 `scripts/rust-size-policy.json` 登记有界例外（reason/owner/split_plan/review 齐备）。
- 门禁（本地）：`cargo test -p lang-codegen` lib 3052 passed / 1 ignored、集成 2、doc-tests 4；`clippy --workspace --all-targets -D warnings`、`fmt --check`、`check_docs.py`、`check_rust_sizes.py --base main`、`git diff --check` 全通过。
