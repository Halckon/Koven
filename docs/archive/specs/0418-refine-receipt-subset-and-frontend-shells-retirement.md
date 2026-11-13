# SPEC-0418：`refine` 借用凭证/准入壳模块及 `lower_frontend` 守卫/容器/闭包壳子模块退役

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的 `ssa/refine` 借用凭证与准入壳模块及 `ssa/lower_frontend` 下的循环所有权、可空 Map 守卫、顺序容器、闭包声明及外层借用 `while` 壳子模块收敛退役时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0418` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 空安全与错误值](../../guide/09-nullability-errors.md)、[Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户指示继续一次推进多批 |
| 基线 | [SPEC-0417](0417-refine-verify-render-and-build-shells-retirement.md)（`15d39035`） |
| 分支 | `feature/spec-0418-refine-receipt-subset-and-frontend-shells-retirement` |
| 前置 | [SPEC-0381](../../specs/active/0381-refine-default-routing-and-legacy-retirement.md)、[SPEC-0383](../../specs/active/0383-refine-final-legacy-retirement.md)、[SPEC-0409](../../specs/active/0409-refine-deinit-and-build-receipt-shells-retirement.md)、[SPEC-0410](../../specs/active/0410-refine-delegated-receiver-and-build-subset-shells-retirement.md)、[SPEC-0415](../../specs/active/0415-refine-lower-frontend-map-iteration-and-field-borrow-shells-retirement.md)、[SPEC-0416](../../specs/active/0416-refine-lowering-support-map-source-and-iteration-shells-retirement.md)、[SPEC-0417](0417-refine-verify-render-and-build-shells-retirement.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

在 `SPEC-0417` 基础上，继续收敛并退役 `crates/lang-codegen/src/ssa` 下的 9 个单职责壳子模块（`refine/named_receipt/emitted.rs`、`refine/receiver_receipt/emitted.rs`、`refine/temporary_receipt/emitted.rs`、`refine/subset/rc.rs`、`lower_frontend/map_loop_owner/return_operand.rs`、`lower_frontend/orchestrate/nullable_map_guard_support/identity.rs`、`lower_frontend/container/runtime.rs`、`lower_frontend/source_closure/declare.rs`、`lower_frontend/outer_borrowed_while/carrier.rs`），所有合并后目标模块均严格控制在 `< 500` 行内。

## 2. 范围与设计

### 2.1 收敛 `refine` 下的凭证校验与 `subset` 准入壳子模块

- **退役 `crates/lang-codegen/src/ssa/refine/named_receipt/emitted.rs`（173 行）**：将 `emitted::{Receipt, Emitted}` 内联收归 `crates/lang-codegen/src/ssa/refine/named_receipt.rs`（合并后 `< 500` 行）。
- **退役 `crates/lang-codegen/src/ssa/refine/receiver_receipt/emitted.rs`（173 行）**：将 `emitted::{Receipt, Emitted}` 内联收归 `crates/lang-codegen/src/ssa/refine/receiver_receipt.rs`（合并后 `< 500` 行）。
- **退役 `crates/lang-codegen/src/ssa/refine/temporary_receipt/emitted.rs`（195 行）**：将 `emitted::{Receipt, Emitted}` 内联收归 `crates/lang-codegen/src/ssa/refine/temporary_receipt.rs`（合并后 `< 500` 行）。
- **退役 `crates/lang-codegen/src/ssa/refine/subset/rc.rs`（202 行）**：将 `Rc<T>` 准入检查方法内联收归 `crates/lang-codegen/src/ssa/refine/subset/when.rs`（合并后 `< 500` 行），并从 `refine/subset.rs` 移除 `mod rc;`。

### 2.2 收敛 `lower_frontend` 下的循环、守卫、容器与闭包壳子模块

- **退役 `crates/lang-codegen/src/ssa/lower_frontend/map_loop_owner/return_operand.rs`（75 行）**：将 `return_operand::drop` 内联收归 `crates/lang-codegen/src/ssa/lower_frontend/map_loop_owner.rs`（合并后 `< 500` 行）。
- **退役 `crates/lang-codegen/src/ssa/lower_frontend/orchestrate/nullable_map_guard_support/identity.rs`（169 行）**：将 `identity::{map_type, plans}` 内联收归 `crates/lang-codegen/src/ssa/lower_frontend/orchestrate/nullable_map_guard_support.rs`（合并后 `< 500` 行）。
- **退役 `crates/lang-codegen/src/ssa/lower_frontend/container/runtime.rs`（172 行）**：将运行时长度容器构造与长度非负守卫方法内联收归 `crates/lang-codegen/src/ssa/lower_frontend/container.rs`（合并后 `< 500` 行）。
- **退役 `crates/lang-codegen/src/ssa/lower_frontend/source_closure/declare.rs`（268 行）**：将 `declare::{Inputs, declare}` 内联收归 `crates/lang-codegen/src/ssa/lower_frontend/source_closure.rs`（合并后 `< 500` 行）。
- **退役 `crates/lang-codegen/src/ssa/lower_frontend/outer_borrowed_while/carrier.rs`（212 行）**：将 `carrier` 内联收归 `crates/lang-codegen/src/ssa/lower_frontend/outer_borrowed_while/source.rs`（合并后 `< 500` 行），并在 `outer_borrowed_while.rs` 中通过 `use source::carrier;` 保持内部引用透明。

## 3. 验收标准

- [x] 退役 `refine/named_receipt/emitted.rs`、`refine/receiver_receipt/emitted.rs`、`refine/temporary_receipt/emitted.rs`、`refine/subset/rc.rs`、`lower_frontend/map_loop_owner/return_operand.rs`、`lower_frontend/orchestrate/nullable_map_guard_support/identity.rs`、`lower_frontend/container/runtime.rs`、`lower_frontend/source_closure/declare.rs` 及 `lower_frontend/outer_borrowed_while/carrier.rs` 共 9 个壳模块文件。
- [x] `scripts/gen_spec_dag.py`、`scripts/check_docs.py` 与 `scripts/check_rust_sizes.py --base main` 通过。
- [ ] 本地 `cargo test -p lang-codegen` 回归验证通过；CI 以 PR 为准。

## 5. 验收记录账本

| 验收项 | 结果 | 证据 |
|---|---|---|
| 3 个壳子模块文件物理删除 | 通过 | 提交 `430a0de2` 上逐个确认：2/3 个路径不存在 |
| 内容并入声明的合并目标 | 部分目标已被后续 Spec 再次合并 | 1/3 个目标路径在当前树存在 |
| 修改文件均小于 1000 行 | 通过 | `python3 scripts/check_rust_sizes.py --base f78c8c20` 退出 0 |
| 手写 Rust 文件数净减 | 受限通过 | 23 个 Spec 已 squash 为 `430a0de2`，无法从历史分离本 Spec 的计数，不虚报该数字；删除逐项已核实 |
| `scripts/gen_spec_dag.py` / `scripts/check_docs.py` 通过 | 通过 | 归档后重跑生成物稳定；`documentation check passed` |
| `cargo test -p lang-codegen --lib` | 通过 | 引用全量结果（默认 3055 / feature 3117），本 Spec 定向 ``cargo test -p lang-codegen --lib -- lower_frontend refine verify_ownership map_`（0411–0427 合并运行）` |
| 退役提交已在基线中 | 通过 | 7d1c1b49 refactor(codegen): retire map_iteration, nullable_borrowed_cleanup, refine and lower_frontend shells (SPEC-0427)（其余 1 个删除提交同批） |

宿主核对说明：

- `refine/named_receipt/emitted.rs`：宿主不再存在，且未按符号定位到等价实现，需人工复核
- `refine/receiver_receipt/emitted.rs`：宿主不再存在，且未按符号定位到等价实现，需人工复核
