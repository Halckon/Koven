# SPEC-0409：`ssa/deinit`、`refine/build` 凭证壳模块收敛及冗余路由/作用域状态退役

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的 `ssa/deinit` 顶层壳模块、`refine/build` 借用/所有权凭证单函数壳子模块（`named_argument`、`binding_receipt`、`owner_call`、`owner_return`）收敛及冗余状态（`UnitExpressionLowerer::closure_scope`、`with_per_family_lowering_except`）退役时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0409` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 空安全与错误值](../../guide/09-nullability-errors.md)、[Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户指示盘点剩余退休工作并继续推进下一批 |
| 基线 | [SPEC-0408](0408-refine-ssa-type-query-shells-and-null-literal-helpers-retirement.md)（`a7fc2986`，含 `main` `e659b66a`） |
| 分支 | `feature/spec-0409-refine-deinit-and-build-receipt-shells-retirement` |
| 前置 | [SPEC-0383](0383-refine-final-legacy-retirement.md)、[SPEC-0404](0404-refine-unit-lower-shell-modules-and-duplicate-helpers-retirement.md)、[SPEC-0405](0405-refine-dead-helpers-and-test-entry-gates-retirement.md)、[SPEC-0406](../../archive/specs/0406-refine-unit-lower-shell-modules-and-operator-helpers-retirement.md)、[SPEC-0407](../../archive/specs/0407-refine-unit-lower-deinit-cfg-and-support-helpers-retirement.md)、[SPEC-0408](0408-refine-ssa-type-query-shells-and-null-literal-helpers-retirement.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

在 `SPEC-0408` 基础上，继续收敛 `crates/lang-codegen/src/ssa` 顶层的析构绑定壳模块（`deinit.rs`）与 `crates/lang-codegen/src/ssa/refine/build` 下的 4 个单函数凭证壳子模块（`named_argument.rs`、`binding_receipt.rs`、`owner_call.rs`、`owner_return.rs`），移除 `UnitExpressionLowerer` 中与 `self.function.id` 完全重复的 `closure_scope` 字段，并将 `refine.rs` 中无外部调用方的 `with_per_family_lowering_except` / `PerFamilyOverride::Forced { except }` 简化为布尔门控。

## 2. 范围与设计

### 2.1 收敛 `ssa/deinit.rs` 至 `types.rs` 与 `verify/module.rs`

- **退役 `crates/lang-codegen/src/ssa/deinit.rs`**：将 `Module::{set_deinit, deinit}` 与签名校验辅助 `valid_deinit_signature` 收归 `crates/lang-codegen/src/ssa/types.rs`（合并后 452 行，满足 `< 500` 行预算与 `Boundary::Ir` 门禁），将模块级 verifier 入口 `verify_deinits` 收归 `crates/lang-codegen/src/ssa/verify/module.rs`（合并后 69 行），更新 `ssa/mod.rs` 并删除 `ssa/deinit.rs`。

### 2.2 收敛 `refine/build` 下的 4 个单函数凭证壳子模块

- **退役 `crates/lang-codegen/src/ssa/refine/build/named_argument.rs` 与 `binding_receipt.rs`**：将 `named_argument_loan`、`statement`、`binding_quiescent` 与 `binding_consumer_argument` 收归同属局部借用绑定与凭证管理的 `crates/lang-codegen/src/ssa/refine/build/borrow_binding.rs`（合并后 289 行，`< 500` 行）。
- **退役 `crates/lang-codegen/src/ssa/refine/build/owner_call.rs` 与 `owner_return.rs`**：将 `owner_unit_call` 与 `owner_return` 收归同属所有权消费凭证校验的 `crates/lang-codegen/src/ssa/refine/build/owner_consumer/replacement.rs`（合并后 448 行，`< 500` 行）。

### 2.3 移除冗余字段 `UnitExpressionLowerer::closure_scope` 与未使用的路由覆盖参数

- 从 `crates/lang-codegen/src/ssa/unit_lower.rs::UnitExpressionLowerer` 移除与 `self.function.id` 恒等的 `closure_scope: FunctionId` 字段，在 `unit_lower/closure.rs` 中直接读取 `self.function.id`，并同步清理 `unit_lower/orchestrate.rs` 与 `unit_lower/control/result_tests.rs`。
- 在 `crates/lang-codegen/src/ssa/refine.rs` 中移除无外部调用方的 `with_per_family_lowering_except` 与 `PerFamilyOverride` 枚举，将 `PER_FAMILY_OVERRIDE` 简化为 `Cell<bool>`。

## 3. 验收标准

- [x] 退役 `ssa/deinit.rs`、`refine/build/{named_argument.rs, binding_receipt.rs, owner_call.rs, owner_return.rs}`，完成 `closure_scope` 与 `with_per_family_lowering_except` 清理。
- [x] `scripts/gen_spec_dag.py`、`scripts/check_docs.py` 与 `scripts/check_rust_sizes.py --base main` 通过。
- [x] 修复合并后的编译缺口：`ssa/types.rs::valid_deinit_signature` 补回缺失闭合括号，`refine/build/owner_consumer/replacement.rs` 补 `ROp` 导入，删除 `scalar_text.rs` 与 `unit_lower.rs` 的未使用导入，并应用 pinned rustfmt。
- [x] 本地 `cargo test -p lang-codegen`（`LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21`，与 CI 同一 Clang 21.1 前提）回归：lib 2914 passed / 1 ignored、集成 2 passed、doc-tests 4 passed；CI 以 PR 为准。
