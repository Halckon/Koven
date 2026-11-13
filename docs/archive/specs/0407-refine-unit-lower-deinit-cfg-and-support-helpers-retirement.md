# SPEC-0407：`unit_lower` 析构/CFG 壳模块收敛、`expression_core` 测试遗留迁移与 `lowering_support` 辅助去重

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的 `unit_lower` 析构声明与 CFG 槽位壳模块（`deinit`、`cfg/alias_slots`）、`expression_core` 测试遗留整数接口迁移及 `lowering_support` 公共词法/字面量辅助去重时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0407` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 空安全与错误值](../../guide/09-nullability-errors.md)、[Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户指示继续推进下一批 E3 收尾清理 |
| 基线 | [SPEC-0406](0406-refine-unit-lower-shell-modules-and-operator-helpers-retirement.md)（`ace7efd9`，含 `main` `e659b66a`） |
| 分支 | `feature/spec-0407-refine-unit-lower-deinit-cfg-and-support-helpers-retirement` |
| 前置 | [SPEC-0383](0383-refine-final-legacy-retirement.md)、[SPEC-0404](0404-refine-unit-lower-shell-modules-and-duplicate-helpers-retirement.md)、[SPEC-0405](0405-refine-dead-helpers-and-test-entry-gates-retirement.md)、[SPEC-0406](0406-refine-unit-lower-shell-modules-and-operator-helpers-retirement.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

在 `SPEC-0406` 基础上，继续收敛 `crates/lang-codegen/src/ssa/unit_lower` 中剩余的单函数析构声明壳模块（`deinit.rs`）与 CFG 合流槽位投影子模块（`cfg/alias_slots.rs`），将 `crates/lang-codegen/src/ssa/expression_core.rs` 中仅服务于 `#[cfg(test)] mod lower_frontend` 的 `IntegerFact`、`IntegerLowering` 与 `lower_integer` 迁回 `lower_frontend/integer.rs`，并在 `crates/lang-codegen/src/ssa/lowering_support.rs` 中统一 `present_name`、`span_key` 与 `parse_integer_literal`，消除 `unit_lower` 与 `lower_frontend` 的重复定义。

## 2. 范围与设计

### 2.1 收敛 `unit_lower/deinit.rs` 与 `unit_lower/cfg/alias_slots.rs`

- **退役 `crates/lang-codegen/src/ssa/unit_lower/deinit.rs`**：将单函数 `declare`（重命名为 `declare_deinit`）收归统一负责函数签名与可调用 ABI 声明的 `crates/lang-codegen/src/ssa/unit_lower/callable_abi.rs`，更新 `orchestrate.rs` 与 `orchestrate/registration_tests.rs` 调用点并删除 `unit_lower/deinit.rs`。
- **退役 `crates/lang-codegen/src/ssa/unit_lower/cfg/alias_slots.rs`**：将 `SourceSlot`、`AliasSlot` 与 `MergeRole`/`MergeProjection` 实现收归 `crates/lang-codegen/src/ssa/unit_lower/cfg/rebind.rs`，删除 `cfg/alias_slots.rs`。

### 2.2 迁移 `expression_core.rs` 中仅 `lower_frontend` 使用的整数 lowering 适配层

- 将 `crates/lang-codegen/src/ssa/expression_core.rs` 中带 `#[cfg(test)]` 门控且仅由 `lower_frontend/integer.rs` 调用的 `IntegerFact`、`IntegerLowering` 与 `lower_integer` 迁入 `crates/lang-codegen/src/ssa/lower_frontend/integer.rs`，使 `expression_core.rs` 仅保留跨层共享的 `LoweredValue` 与 `checked` 子模块。

### 2.3 在 `lowering_support.rs` 中统一 `present_name`、`span_key` 与 `parse_integer_literal`

- 在 `crates/lang-codegen/src/ssa/lowering_support.rs` 中提供 `present_name`、`span_key` 与 `parse_integer_literal_text`。
- 在 `crates/lang-codegen/src/ssa/unit_lower.rs` 与 `crates/lang-codegen/src/ssa/lower_frontend.rs` 中直接复用，删除重复实现。

## 3. 验收标准

- [x] 退役 `unit_lower/deinit.rs` 与 `unit_lower/cfg/alias_slots.rs`，完成 `expression_core` 测试遗留项迁移与 `lowering_support` 辅助函数去重。
- [x] `scripts/gen_spec_dag.py`、`scripts/check_docs.py` 与 `scripts/check_rust_sizes.py --base main` 通过。
- [ ] 本地 `cargo test -p lang-codegen` 回归验证通过；CI 以 PR 为准。

## 5. 验收记录账本

| 验收项 | 结果 | 证据 |
|---|---|---|
| 2 个退役项已从版本库移除 | 通过 | 逐个确认：2/2 个路径/符号不再存在 |
| 内容并入声明的合并目标 | 通过（部分宿主被后续 Spec 再次合并） | 1/2 个目标在当前树存在 |
| 修改文件均小于 1000 行 | 通过 | `python3 scripts/check_rust_sizes.py --base f78c8c20` 退出 0 |
| 手写 Rust 文件数净减 | 受限通过 | 历史已 squash 为 `430a0de2`，无法分离单 Spec 计数 |
| `scripts/gen_spec_dag.py` / `scripts/check_docs.py` 通过 | 通过 | 归档后重跑生成物稳定；`documentation check passed` |
| `cargo test -p lang-codegen --lib` | 通过 | `cargo test -p lang-codegen --lib -- refine verify_ownership` 等定向；完整套件默认 `3055 passed / 0 failed / 1 ignored`、feature `3117 passed / 0 failed / 1 ignored` |

核对说明：

- 退役 `unit_lower/deinit.rs`（内容收归 `unit_lower/callable_abi.rs`）与 `unit_lower/cfg/alias_slots.rs`（收归 `unit_lower/cfg/rebind.rs`）
- `unit_lower/callable_abi.rs` 随后被后续 Spec 再次合并，`declare_deinit` 现位于 `ssa/unit_lower/call_lifetimes.rs`
- 本 Spec 另含仅在文件内的死辅助删除（`expression_core` 的 `IntegerFact` 等），不是文件退役，已随 SPEC-0406 的 `integer` 收敛一并完成
