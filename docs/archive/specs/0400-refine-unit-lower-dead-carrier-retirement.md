# SPEC-0400：`unit_lower` 区间/捕获/闭包来源死 Carrier、调用帧冗余槽与守卫测试支架退役

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的 `unit_lower` 已迁出能力（`RangeView`、借用结果来源贷款、带捕获闭包、lambda thunk、字段置换、独占根借用与可空守卫测试）遗留 CFG/Call carrier 退役时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0400` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 空安全与错误值](../../guide/09-nullability-errors.md)、[Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户确认合入 PR #408（`3fe1920a`）后继续推进下一批旧路径退役 |
| 基线 | [SPEC-0399](../../specs/active/0399-refine-heap-nullable-if-and-compatibility-retirement.md)（`3fe1920a`） |
| 分支 | `feature/spec-0400-refine-unit-lower-dead-carrier-retirement` |
| 前置 | [SPEC-0381](../../specs/active/0381-refine-default-routing-and-legacy-retirement.md)、[SPEC-0382](../../archive/specs/0382-refine-map-ops-and-primitive-retirement.md)、[SPEC-0393](../../specs/active/0393-refine-range-borrow-result-rc-closure-retirement.md)、[SPEC-0395](../../specs/active/0395-refine-nullable-while-retirement.md)、[SPEC-0397](../../specs/active/0397-refine-nullable-map-and-closure-retirement.md)、[SPEC-0399](../../specs/active/0399-refine-heap-nullable-if-and-compatibility-retirement.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

清理 `SPEC-0381`、`SPEC-0382`、`SPEC-0393`、`SPEC-0395`、`SPEC-0397`、`SPEC-0398` 与 `SPEC-0399` 迁出后遗留在 `crates/lang-codegen/src/ssa/unit_lower` 中的死 CFG/Call 状态载体与可空守卫测试支架，删除 `cfg/range.rs` 与 `nullable_proof/guard_tests` 模块，净减约 950 行冗余代码。

## 2. 范围与设计

### 2.1 退役模块清单

删除以下 3 个已无生产/测试消费的文件：

- `crates/lang-codegen/src/ssa/unit_lower/cfg/range.rs`
- `crates/lang-codegen/src/ssa/unit_lower/nullable_proof/guard_tests.rs`
- `crates/lang-codegen/src/ssa/unit_lower/nullable_proof/guard_tests/support.rs`

将 `crates/lang-codegen/src/ssa/unit_lower/control/result_tests.rs` 唯一使用的最小 `Fixture` 内联至该测试文件内。

### 2.2 退役 CFG / Call / Closure 死状态载体

- **区间与借用结果来源载体**（自 `SPEC-0393` 起恒空）：从 `UnitExpressionLowerer`、`BranchExit`、`CarriedAccess`、`MergeRole` 中移除 `result_source_loans`、`temporary_range_slots`、`short_range_ends`、`range_sources` 与 `MergeRole::ResultSource`。
- **闭包捕获、局部闭包溯源与 Thunk 载体**（自 `SPEC-0393` / `SPEC-0397` 起恒空）：从 `UnitExpressionLowerer`、`BranchExit`、`LoopContext`、`CarriedAccess`、`MergeRole` 中移除 `capture_loans`、`closure_bindings`、`thunk_expression`、`captures`、`MergeRole::Capture` 与 `validate_closure_drop_facts`。
- **调用帧冗余槽与区间扩展接收者残留**（自 `SPEC-0381` / `SPEC-0382` / `SPEC-0393` 起恒空）：从 `PendingCallFrame`、`LoweredCallArguments`、`call.rs`、`call_lifetimes.rs`、`aggregate.rs`、`cfg.rs` 与 `loop_control/iteration_cleanup.rs` 中移除 `abi_slots`、`abi_owners`、`drop_abi_call_owners`、`end_pending_abi_call_slots`、`field_replace_owner`、`exclusive_root_owners` 及 `range_receiver` 分支。

## 3. 验收标准

- [x] 删除第 2.1 节列出的 3 个文件，并完成第 2.2 节全部死 carrier 字段与辅助函数的清理。
- [x] 同步修正 `SPEC-0399` 文档以准确反映 PR #408（`01926c86` / `3fe1920a`）保留 `nullable when` 混合函数兼容层的实际边界。
- [x] `scripts/check_rust_sizes.py --base main` 与 `scripts/check_docs.py` 通过。
- [ ] 本地 `cargo test -p lang-codegen` 回归验证通过；CI 以 PR 为准。

## 5. 验收记录账本

| 验收项 | 结果 | 证据 |
|---|---|---|
| 4 个退役项已从版本库移除 | 通过 | 逐个确认：4/4 个路径/符号不再存在 |
| 内容并入声明的合并目标 | 通过（部分宿主被后续 Spec 再次合并） | 0/4 个目标在当前树存在 |
| 修改文件均小于 1000 行 | 通过 | `python3 scripts/check_rust_sizes.py --base f78c8c20` 退出 0 |
| 手写 Rust 文件数净减 | 受限通过 | 历史已 squash 为 `430a0de2`，无法分离单 Spec 计数 |
| `scripts/gen_spec_dag.py` / `scripts/check_docs.py` 通过 | 通过 | 归档后重跑生成物稳定；`documentation check passed` |
| `cargo test -p lang-codegen --lib` | 通过 | 完整默认配置 lib 套件 `3055 passed / 0 failed / 1 ignored`、feature 套件 `3117 passed / 0 failed / 1 ignored`（两次全量均已执行）；本 Spec 未另跑独立定向重跑 |

核对说明：

- 确认不存在：`unit_lower/cfg/range.rs`、`unit_lower/nullable_proof/guard_tests.rs`、`unit_lower/nullable_proof/guard_tests/support.rs`、`unit_lower/control/result_tests.rs`
- 这些宿主目录本身随后被 SPEC-0435/0436 再次收敛，故目标路径也不再存在
- 本 Spec 声明的文件级退役项在当前树均已不存在；历史已 squash，无法把删除单独归因到本 Spec 的提交，账本只声明“当前状态已达成”。
