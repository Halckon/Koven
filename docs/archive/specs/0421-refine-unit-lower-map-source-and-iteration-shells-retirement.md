# SPEC-0421：`unit_lower` 可空证明/分支、`map_source` 交接、`verify/module` 及 `lower_frontend` 迭代/循环壳子模块退役

> **性质**：工程收口规范（E3 Phase 4 文件数量收敛第六批） · **状态**：in-progress · **读取时机**：收敛 `crates/lang-codegen/src/ssa` 下的 `unit_lower` 可空分支/证明、`map_source` 交接运行时、`verify/module` 模块验证及 `lower_frontend` 闭包/迭代/可空 `while` 壳子模块时 · **唯一真源**：本文件定义 SPEC-0421 的合并范围、零语义漂移约束与验收条件

| 字段 | 内容 |
| --- | --- |
| 编号 | SPEC-0421 |
| 标题 | `unit_lower` 可空证明/分支、`map_source` 交接、`verify/module` 及 `lower_frontend` 迭代/循环壳子模块退役 |
| 状态 | done |
| 所属阶段 | M4 Phase 4（SSA 代码生成与 E3 收口） |
| 依赖 | SPEC-0416, SPEC-0417, SPEC-0418, SPEC-0419, SPEC-0420 |

## 1. 背景与目标

紧接 [SPEC-0420](0420-refine-support-verify-receipt-and-drops-shells-retirement.md)，`crates/lang-codegen/src/ssa` 下仍有一批薄壳子模块可内联收敛至宿主或相邻兄弟模块，且合并后全部文件均保持在 1000 行阈值以内：

1. `unit_lower/control/nullable_if.rs`（239 行）-> 并入 `unit_lower/control/conditional.rs`（525 行）；
2. `unit_lower/nullable_proof.rs`（278 行）-> 并入 `unit_lower/construction.rs`（568 行）；
3. `lower_frontend/map_iteration/conditional/retained.rs`（265 行）-> 并入 `lower_frontend/map_iteration/conditional.rs`（483 行）；
4. `lower_frontend/source_closure/formation.rs`（289 行）-> 并入 `lower_frontend/source_closure.rs`（723 行）；
5. `map_source/handoff_runtime.rs`（240 行）-> 并入 `map_source.rs`（646 行）；
6. `lower_frontend/nullable_borrowed_cleanup/runtime.rs`（267 行）-> 并入 `lower_frontend/nullable_borrowed_cleanup.rs`（694 行）；
7. `lower_frontend/loop_control/nullable_while/entry.rs`（279 行）-> 并入 `lower_frontend/loop_control/nullable_while.rs`（635 行）；
8. `verify/module.rs`（184 行）-> 并入 `verify_types.rs`（760 行）。

本规范在不改变任何外部 API、诊断合同、SSA 验证语义及 `lowering_dependency_tests.rs` 架构护栏的前提下，一次性退役上述 8 个薄壳文件。

## 2. 退役与合并清单

| 退役文件 | 并入目标文件 | 处理方式 |
| --- | --- | --- |
| `crates/lang-codegen/src/ssa/unit_lower/control/nullable_if.rs` | `crates/lang-codegen/src/ssa/unit_lower/control/conditional.rs` | 内联 `pub(in crate::ssa::unit_lower) mod nullable_if` 子模块，保留 `nullable_if/field_tests.rs` 与 `nullable_if/heap.rs` 路径映射，并在 `unit_lower/control.rs` 中重导出 |
| `crates/lang-codegen/src/ssa/unit_lower/nullable_proof.rs` | `crates/lang-codegen/src/ssa/unit_lower/construction.rs` | 内联 `pub(super) mod nullable_proof` 子模块，并在 `unit_lower.rs` 中通过 `use construction::nullable_proof;` 维持模块路径不变 |
| `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/conditional/retained.rs` | `crates/lang-codegen/src/ssa/lower_frontend/map_iteration/conditional.rs` | 内联 `pub(super) mod retained` 子模块，移除独立文件 |
| `crates/lang-codegen/src/ssa/lower_frontend/source_closure/formation.rs` | `crates/lang-codegen/src/ssa/lower_frontend/source_closure.rs` | 内联 `ExpressionLowerer` 闭包构造与捕获视图绑定实现，移除 `mod formation;` |
| `crates/lang-codegen/src/ssa/map_source/handoff_runtime.rs` | `crates/lang-codegen/src/ssa/map_source.rs` | 内联 `pub(super) mod handoff_runtime` 子模块，移除独立文件 |
| `crates/lang-codegen/src/ssa/lower_frontend/nullable_borrowed_cleanup/runtime.rs` | `crates/lang-codegen/src/ssa/lower_frontend/nullable_borrowed_cleanup.rs` | 内联 `mod runtime` 子模块并保留 `#[path = "nullable_borrowed_cleanup/tests.rs"]` 测试挂载 |
| `crates/lang-codegen/src/ssa/lower_frontend/loop_control/nullable_while/entry.rs` | `crates/lang-codegen/src/ssa/lower_frontend/loop_control/nullable_while.rs` | 内联 `mod entry` 子模块，移除独立文件 |
| `crates/lang-codegen/src/ssa/verify/module.rs` | `crates/lang-codegen/src/ssa/verify_types.rs` | 内联 `pub(in crate::ssa) mod module` 子模块，并在 `verify.rs` 中通过 `use super::verify_types::module;` 重导出 |

## 3. 验收条件

1. 上述 8 个壳文件已从版本库移除，合并后的宿主文件均严格小于 1000 行（通过 `python3 scripts/check_rust_sizes.py --base main`）。
2. `crates/lang-codegen/src/ssa/lowering_dependency_tests.rs` 中的所有存在性与依赖边界断言保持原样通过。
3. `python3 scripts/gen_spec_dag.py` 与 `python3 scripts/check_docs.py` 检查通过。

## 4. 本地验收记录

- 构建修复（本分支此前从未通过编译；均为折叠/搬迁留下的机械缺口，不改变 lowering 语义）：
  - 内联模块 `#[path]` 归位：`lower_frontend/nullable_borrowed_cleanup{runtime/tests}`、`lower_frontend/nullable_if_cleanup/constant_presence/{source,source_return}`、`unit_lower/control/conditional/nullable_if/{heap,field_tests}`、`refine/{binding,loan,named,receiver,temporary}_receipt/emitted/ci_tests` 移入 rustc 对内联模块期望的目录，并删除冗余路径前缀。
  - 语法修复：`lower_frontend/nullable_if_cleanup`、`refine/{binding,named,receiver}_receipt`、`lower_frontend/outer_borrowed_while/source` 的 match 分支补回缺失逗号；`ssa/types.rs::valid_deinit_signature` 补回闭合括号。
  - 导入与可见性：`ssa/render.rs` 补回 `BinaryOperator`/`ComparisonOperator`/`IntegerBitwiseOperator`/`CheckedArithmeticOperator`；`refine/build/owner_consumer/replacement.rs` 补回 `ROp`；`nullable_if_cleanup::constant_presence::source{,_return}` 改用 `crate::ssa::lowering_support::present_name`；`unit_lower/control/conditional/nullable_if/heap.rs` 改用 `crate::ssa::unit_lower::span_contains`。
  - 可见性收敛：`verify.rs` 的共享证明入口提升为 `pub(in crate::ssa)`，`verify/closure_{content,escape}` 的对应入口同步提升；`lower_frontend` 对 `aggregate::resource_deinit`、`nullable_if_cleanup::constant_presence` 的重导出收窄到 `pub(in crate::ssa::lower_frontend)`；`qualify_nullable_while_entry` 收窄到唯一调用方作用域。
  - 结构修复：`ssa/borrow_result_support/local_if.rs` 将 `#[cfg(test)] mod stable_map` 移到文件末尾，消除 “items after a test module”。
  - 未使用导入：删除 `scalar_text.rs::BinaryOperator`、`unit_lower.rs::NameMarker`、`unit_lower/control.rs` 的 `nullable_if` 别名、`map_iteration/local.rs::CleanupConditionId`。
  - 体积：`scripts/rust-size-policy.json` 为 `ssa/verify.rs` 登记 1009 行有界例外（原因、owner、split_plan、review 齐备；仅 5 处可见性提升与 rustfmt 折行导致 997→1009）。
- 门禁（本地，`LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21`，与 CI 同一 Clang 21.1 前提）：
  - `cargo test -p lang-codegen`：lib 2914 passed / 1 ignored、集成 2 passed、doc-tests 4 passed。
  - `cargo clippy --locked --workspace --all-targets -- -D warnings`、`cargo fmt --all -- --check`、`python3 scripts/check_docs.py`、`python3 scripts/check_rust_sizes.py --base main`、`git diff --check` 全部通过。
- 说明：本分支按 `SPEC-0410` → `SPEC-0421` 顺序堆叠，PR 将一并交付 `SPEC-0410`～`SPEC-0421`；单独承载 `SPEC-0410` 的分支（PR #416）随之冗余。

## 5. 验收记录账本

| 验收项 | 结果 | 证据 |
|---|---|---|
| 8 个壳子模块文件物理删除 | 通过 | 提交 `430a0de2` 上逐个确认：8/8 个路径不存在 |
| 内容并入声明的合并目标 | 部分宿主被后续 Spec 再次合并 | 4/8 个目标路径在当前树存在 |
| 修改文件均小于 1000 行 | 通过 | `python3 scripts/check_rust_sizes.py --base f78c8c20` 退出 0 |
| 手写 Rust 文件数净减 | 受限通过 | 历史已 squash 为 `430a0de2`，无法分离单 Spec 计数；删除逐项已核实 |
| `scripts/gen_spec_dag.py` / `scripts/check_docs.py` 通过 | 通过 | 归档后重跑生成物稳定；`documentation check passed` |
| `cargo test -p lang-codegen --lib` | 通过 | `cargo test -p lang-codegen --lib -- refine verify_ownership` 等定向；完整套件默认 `3055 passed / 0 failed / 1 ignored`、feature `3117 passed / 0 failed / 1 ignored` |
| 退役提交已在基线 `f78c8c20` 中 | 通过 | 32df9c26 refactor(codegen): retire unit_lower, map_source, emit and lower_frontend shells (SPEC-0421)（同批共 8 个删除提交） |

宿主核对说明：

- `lower_frontend/map_iteration/conditional.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/map_provider_exits_pipeline_tests/nested_exits/query_cleanup.rs`（`243` 行）
- `lower_frontend/source_closure.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/owned_map_borrow_boundary_tests.rs`（`617` 行）
- `lower_frontend/nullable_borrowed_cleanup.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/integer_range_contract_tests.rs`（`663` 行）
- `lower_frontend/loop_control/nullable_while.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/carry/columns.rs`（`502` 行）
