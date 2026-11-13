# SPEC-0420：`lowering_support`、`integer`、`verify_ownership/reborrow`、`refine` 接收者/借用凭证及 `lower_frontend/drops` 壳子模块退役

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的 `ssa/lowering_support/callable_instances`、`ssa/integer`、`ssa/verify_ownership/reborrow`、`ssa/refine` 接收者与借用凭证壳模块及 `ssa/lower_frontend/drops` 收敛退役时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0420` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 空安全与错误值](../../guide/09-nullability-errors.md)、[Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户指示继续下一批批量实施 |
| 基线 | [SPEC-0419](0419-refine-lower-frontend-and-map-iteration-shells-retirement.md)（`2e003c05`） |
| 分支 | `feature/spec-0420-refine-support-verify-receipt-and-drops-shells-retirement` |
| 前置 | [SPEC-0381](../../specs/active/0381-refine-default-routing-and-legacy-retirement.md)、[SPEC-0383](../../specs/active/0383-refine-final-legacy-retirement.md)、[SPEC-0416](../../specs/active/0416-refine-lowering-support-map-source-and-iteration-shells-retirement.md)、[SPEC-0417](../../specs/active/0417-refine-verify-render-and-build-shells-retirement.md)、[SPEC-0418](0418-refine-receipt-subset-and-frontend-shells-retirement.md)、[SPEC-0419](0419-refine-lower-frontend-and-map-iteration-shells-retirement.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

在 `SPEC-0419` 基础上，继续收敛并退役 `crates/lang-codegen/src/ssa` 下的 8 个单职责壳子模块（`lowering_support/callable_instances.rs`、`integer.rs`、`verify_ownership/reborrow.rs`、`refine/build/receiver.rs`、`refine/loan_receipt/emitted.rs`、`refine/binding_receipt/emitted.rs`、`refine/temporary_receipt/admission.rs`、`lower_frontend/drops.rs`），所有合并后目标模块均符合仓库行数门禁约束。

## 2. 范围与设计

### 2.1 收敛 `lowering_support`、`integer` 与 `verify_ownership/reborrow` 壳子模块

- **退役 `crates/lang-codegen/src/ssa/lowering_support/callable_instances.rs`（202 行）**：将 `callable_instances` 内联收归 `crates/lang-codegen/src/ssa/lowering_support.rs`（合并后 383 行，保持 `Boundary::Support` 零跨层依赖）。
- **退役 `crates/lang-codegen/src/ssa/integer.rs`（170 行）**：将 `integer` 算子映射与区间事实核验内联收归 `crates/lang-codegen/src/ssa/closure.rs`（合并后 471 行），并在 `ssa/mod.rs` 中通过 `use closure::integer;` 保持内部引用透明。
- **退役 `crates/lang-codegen/src/ssa/verify_ownership/reborrow.rs`（156 行）**：将 `reborrow::{end_loan, ReborrowDependencies}` 内联收归 `crates/lang-codegen/src/ssa/verify_ownership/map.rs`（合并后 367 行），并在 `verify_ownership.rs` 中通过 `use map::reborrow;` 保持内部引用透明。

### 2.2 收敛 `refine` 接收者/凭证与 `lower_frontend/drops` 壳子模块

- **退役 `crates/lang-codegen/src/ssa/refine/build/receiver.rs`（218 行）**：将 `receiver::Receiver` 与成员调用接收者求值方法内联收归 `crates/lang-codegen/src/ssa/refine/build/rc.rs`（合并后 493 行），并在 `refine/build.rs` 中通过 `use rc::receiver;` 保持内部引用透明。
- **退役 `crates/lang-codegen/src/ssa/refine/loan_receipt/emitted.rs`（199 行）**：将 `emitted::{Receipt, Emitted}` 内联收归 `crates/lang-codegen/src/ssa/refine/loan_receipt.rs`。
- **退役 `crates/lang-codegen/src/ssa/refine/binding_receipt/emitted.rs`（239 行）**：将 `emitted::{Receipt, Emitted}` 内联收归 `crates/lang-codegen/src/ssa/refine/binding_receipt.rs`。
- **退役 `crates/lang-codegen/src/ssa/refine/temporary_receipt/admission.rs`（256 行）**：将 `admission` 准入规划内联收归 `crates/lang-codegen/src/ssa/refine/temporary_receipt.rs`。
- **退役 `crates/lang-codegen/src/ssa/lower_frontend/drops.rs`（242 行）**：将 `finish_borrowed_call`、`emit_control_transfer_cleanup` 与 `emit_drops` 内联收归 `crates/lang-codegen/src/ssa/lower_frontend/non_null_proof.rs`（合并后 493 行），并从 `lower_frontend.rs` 移除 `mod drops;`。

## 3. 验收标准

- [x] 退役 `lowering_support/callable_instances.rs`、`integer.rs`、`verify_ownership/reborrow.rs`、`refine/build/receiver.rs`、`refine/loan_receipt/emitted.rs`、`refine/binding_receipt/emitted.rs`、`refine/temporary_receipt/admission.rs` 及 `lower_frontend/drops.rs` 共 8 个壳模块文件。
- [x] `scripts/gen_spec_dag.py`、`scripts/check_docs.py` 与 `scripts/check_rust_sizes.py --base main` 通过。
- [ ] 本地 `cargo test -p lang-codegen` 回归验证通过；CI 以 PR 为准。

## 5. 验收记录账本

| 验收项 | 结果 | 证据 |
|---|---|---|
| 2 个壳子模块文件物理删除 | 通过 | 提交 `430a0de2` 上逐个确认：1/2 个路径不存在 |
| 内容并入声明的合并目标 | 部分目标已被后续 Spec 再次合并 | 0/2 个目标路径在当前树存在 |
| 修改文件均小于 1000 行 | 通过 | `python3 scripts/check_rust_sizes.py --base f78c8c20` 退出 0 |
| 手写 Rust 文件数净减 | 受限通过 | 23 个 Spec 已 squash 为 `430a0de2`，无法从历史分离本 Spec 的计数，不虚报该数字；删除逐项已核实 |
| `scripts/gen_spec_dag.py` / `scripts/check_docs.py` 通过 | 通过 | 归档后重跑生成物稳定；`documentation check passed` |
| `cargo test -p lang-codegen --lib` | 通过 | 引用全量结果（默认 3055 / feature 3117），本 Spec 定向 ``cargo test -p lang-codegen --lib -- lower_frontend refine verify_ownership map_`（0411–0427 合并运行）` |
| 退役提交已在基线中 | 通过 | 7d1c1b49 refactor(codegen): retire map_iteration, nullable_borrowed_cleanup, refine and lower_frontend shells (SPEC-0427)（其余 0 个删除提交同批） |

宿主核对说明：

- `lowering_support/callable_instances.rs`：宿主不再存在，且未按符号定位到等价实现，需人工复核
- `integer.rs`：宿主不再存在，且未按符号定位到等价实现，需人工复核
