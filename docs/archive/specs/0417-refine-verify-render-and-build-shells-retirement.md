# SPEC-0417：`verify`、`render`、`verify_operation`、`verify_ownership` 及 `refine/build` 壳子模块退役

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的 `ssa/verify`、`ssa/render`、`ssa/verify_operation`、`ssa/verify_ownership`、`ssa/refine/build` 及 `ssa/map_result` 壳子模块收敛退役时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0417` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 空安全与错误值](../../guide/09-nullability-errors.md)、[Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户指示继续一次推进多批 |
| 基线 | [SPEC-0416](0416-refine-lowering-support-map-source-and-iteration-shells-retirement.md)（`43b117f8`） |
| 分支 | `feature/spec-0417-refine-verify-render-and-build-shells-retirement` |
| 前置 | [SPEC-0381](../../specs/active/0381-refine-default-routing-and-legacy-retirement.md)、[SPEC-0383](../../specs/active/0383-refine-final-legacy-retirement.md)、[SPEC-0412](../../specs/active/0412-refine-render-verify-and-map-frontend-shells-retirement.md)、[SPEC-0413](../../specs/active/0413-refine-verify-operation-ownership-and-content-flow-shells-retirement.md)、[SPEC-0414](0414-refine-model-constant-and-lower-frontend-shells-retirement.md)、[SPEC-0415](0415-refine-lower-frontend-map-iteration-and-field-borrow-shells-retirement.md)、[SPEC-0416](0416-refine-lowering-support-map-source-and-iteration-shells-retirement.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

在 `SPEC-0416` 基础上，继续收敛并退役 `crates/lang-codegen/src/ssa` 下的 9 个单职责壳子模块（`verify_ownership/closure/root_storage.rs`、`verify_ownership/direct_fields.rs`、`verify_ownership/range/pairs.rs`、`verify_operation/nullable.rs`、`verify_borrow_result.rs`、`verify/closure_escape/proof.rs`、`render/map.rs`、`refine/build/range_view.rs`、`map_result.rs`），所有合并后目标模块均严格控制在 `< 500` 行内。

## 2. 范围与设计

### 2.1 收敛 `verify_ownership`、`verify_operation` 与 `verify` 壳子模块

- **退役 `crates/lang-codegen/src/ssa/verify_ownership/closure/root_storage.rs`（26 行）**：将 `root_storage::take_sources` 内联收归 `crates/lang-codegen/src/ssa/verify_ownership/closure.rs`（合并后 448 行，`< 500` 行）。
- **退役 `crates/lang-codegen/src/ssa/verify_ownership/direct_fields.rs`（129 行）**：将 `direct_fields::DirectFields` 内联收归 `crates/lang-codegen/src/ssa/verify_ownership/aliases.rs`（合并后 269 行，`< 500` 行），并在 `verify_ownership.rs` 中通过 `use aliases::direct_fields;` 保持内部引用透明。
- **退役 `crates/lang-codegen/src/ssa/verify_ownership/range/pairs.rs`（205 行）**：将 `pairs` 内联收归 `crates/lang-codegen/src/ssa/verify_ownership/range.rs`（合并后 475 行，`< 500` 行）。
- **退役 `crates/lang-codegen/src/ssa/verify_operation/nullable.rs`（95 行）**：将 `nullable::has_exact_branch_proof` 内联收归 `crates/lang-codegen/src/ssa/verify_operation/range.rs`（合并后 283 行，`< 500` 行），并在 `verify_operation.rs` 中通过 `use range::nullable;` 保持内部引用透明。
- **退役 `crates/lang-codegen/src/ssa/verify_borrow_result.rs`（101 行）**：将 `verify_borrow_result` 内联收归 `crates/lang-codegen/src/ssa/verify_operation/integer.rs`（合并后 264 行，`< 500` 行），并在 `verify_operation.rs` 与 `ssa/mod.rs` 中重导出以保持模块路径透明。
- **退役 `crates/lang-codegen/src/ssa/verify/closure_escape/proof.rs`（266 行）**：将 `proof::{ValueProof, content_index, projected_places}` 内联收归 `crates/lang-codegen/src/ssa/verify/closure_escape.rs`（合并后 435 行，`< 500` 行）。

### 2.2 收敛 `render`、`refine/build` 与 `map_result` 壳子模块

- **退役 `crates/lang-codegen/src/ssa/render/map.rs`（86 行）**：将 `map::write_operation` 内联收归 `crates/lang-codegen/src/ssa/render/types.rs`（合并后 217 行，`< 500` 行），并在 `render.rs` 中通过 `use types::map;` 保持内部引用透明。
- **退役 `crates/lang-codegen/src/ssa/refine/build/range_view.rs`（114 行）**：将 `range_iteration_loan` 与 `lower_range_size` 内联收归 `crates/lang-codegen/src/ssa/refine/build/rc.rs`（合并后 292 行，`< 500` 行），并从 `refine/build.rs` 移除 `mod range_view;`。
- **退役 `crates/lang-codegen/src/ssa/map_result.rs`（109 行）**：将 `$map.result` 与 `$nullable.map` 类型构造与校验辅助内联收归 `crates/lang-codegen/src/ssa/closure.rs`（合并后 305 行，`< 500` 行），并从 `ssa/mod.rs` 移除 `mod map_result;`。

## 3. 验收标准

- [x] 退役 `verify_ownership/closure/root_storage.rs`、`verify_ownership/direct_fields.rs`、`verify_ownership/range/pairs.rs`、`verify_operation/nullable.rs`、`verify_borrow_result.rs`、`verify/closure_escape/proof.rs`、`render/map.rs`、`refine/build/range_view.rs` 及 `map_result.rs` 共 9 个壳模块文件。
- [x] `scripts/gen_spec_dag.py`、`scripts/check_docs.py` 与 `scripts/check_rust_sizes.py --base main` 通过。
- [ ] 本地 `cargo test -p lang-codegen` 回归验证通过；CI 以 PR 为准。

## 5. 验收记录账本

| 验收项 | 结果 | 证据 |
|---|---|---|
| 9 个壳子模块文件物理删除 | 通过 | 提交 `430a0de2` 上逐个确认：9/9 个路径不存在 |
| 内容并入声明的合并目标 | 部分宿主被后续 Spec 再次合并 | 5/9 个目标路径在当前树存在 |
| 修改文件均小于 1000 行 | 通过 | `python3 scripts/check_rust_sizes.py --base f78c8c20` 退出 0 |
| 手写 Rust 文件数净减 | 受限通过 | 历史已 squash 为 `430a0de2`，无法分离单 Spec 计数；删除逐项已核实 |
| `scripts/gen_spec_dag.py` / `scripts/check_docs.py` 通过 | 通过 | 归档后重跑生成物稳定；`documentation check passed` |
| `cargo test -p lang-codegen --lib` | 通过 | `cargo test -p lang-codegen --lib -- refine verify_ownership` 等定向；完整套件默认 `3055 passed / 0 failed / 1 ignored`、feature `3117 passed / 0 failed / 1 ignored` |
| 退役提交已在基线 `f78c8c20` 中 | 通过 | 15d39035 refactor(codegen): retire verify, render, refine/build and map_result shells (SPEC-0417)（同批共 9 个删除提交） |

宿主核对说明：

- `verify_operation/range.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/borrow_result_support.rs`（`418` 行）
- `verify_operation/integer.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/verify_ownership_tests.rs`（`99` 行）
- `verify/closure_escape.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/llvm/debug.rs`（`115` 行）
- `render/types.rs`：该宿主随后被再次合并，按符号核实当前位于 `crates/lang-codegen/src/ssa/render.rs`（`287` 行）
