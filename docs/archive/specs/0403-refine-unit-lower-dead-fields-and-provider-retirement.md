# SPEC-0403：`unit_lower` 死字段、`provider` 死辅助入口与测试专用 `ssa` 支持项退役

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的 `unit_lower` 死字段、`provider` 死辅助入口与测试专用 `ssa` 支持项收敛时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0403` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 空安全与错误值](../../guide/09-nullability-errors.md)、[Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户确认继续推进下一批旧路径退役 |
| 基线 | [SPEC-0402](0402-refine-unit-lower-shell-modules-and-iteration-state-retirement.md)（`ebde99f4`） |
| 分支 | `feature/spec-0403-refine-unit-lower-dead-fields-and-provider-retirement` |
| 前置 | [SPEC-0384](../../specs/active/0384-refine-legacy-module-retirement.md)、[SPEC-0396](../../specs/active/0396-refine-map-iteration-retirement.md)、[SPEC-0398](../../specs/active/0398-refine-nullable-string-and-field-if-retirement.md)、[SPEC-0402](0402-refine-unit-lower-shell-modules-and-iteration-state-retirement.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

清理 `SPEC-0384`、`SPEC-0396`、`SPEC-0398` 与 `SPEC-0402` 退役旧路径后遗留在 `crates/lang-codegen/src/ssa` 中的三类冗余结构：
1. `UnitExpressionLowerer` 中从未读取的死字段 `enum_payloads` 与 `type_references` 及 `field_tests.rs` 中不可达的旧 `If` 分支观测辅助；
2. `provider.rs` 中无任何调用方的死辅助函数（`finish_element_and_advance`、`finish_element_and_advance_in_block`、`ProviderHeader::backedge`、`ProviderNext`）以及 `ssa/mod.rs` 对 `pub(crate) mod provider;` 的历史 `#[allow(dead_code)]` 豁免；
3. `map_source::unit_drop_event` 死函数，以及生产路径已全部迁入 `ssa::refine`、当前仅由 `#[cfg(test)] mod lower_frontend;` 调用的 `map_source`、`borrow_result_support::stable_map`、`carry` 捕获/结果借用角色和 `expression_core::IntegerLowering`。

## 2. 范围与设计

### 2.1 移除 `UnitExpressionLowerer` 死字段与 `field_tests.rs` 不可达分支观测

- 从 `crates/lang-codegen/src/ssa/unit_lower.rs::UnitExpressionLowerer` 移除自 `SPEC-0384` 退役旧枚举 lowering 后从未被读取的 `enum_payloads` 与 `type_references` 字段，并同步清理 `orchestrate.rs` 与 `control/result_tests.rs` 中的死字段初始化与冗余 `Namespace::Type` 引用收集。
- 从 `crates/lang-codegen/src/ssa/unit_lower/control/nullable_if/field_tests.rs` 移除不可达的 `Expression::If` 分支 origin 观测逻辑与 `assert_if_block_origins`（所有同根可空堆对象字段 `if` 测试均经 `ssa::refine` lowering，其 `NullableBranch` 直接源自 `null_comparison`）。

### 2.2 退役 `provider` 死辅助入口并移除 `#[allow(dead_code)]`

- 从 `crates/lang-codegen/src/ssa/provider.rs` 移除生产路径无调用方的 `finish_element_and_advance_in_block`；`ProviderNext`、`ProviderHeader::backedge` 与 `finish_element_and_advance` 仍由 `llvm/container_tests/provider_cfg.rs` 的 provider CFG fixture 消费，因此改为 `#[cfg(test)]` 门控保留，而非删除。
- 简化 `provider::advance_in_block` 签名，使其仅接收 `(function, block, cursor, step, int_type, origin)` 并直接返回 `Result<ValueId, ModelError>`，同步更新 `unit_lower/loop_control/iteration.rs::advance_iteration`。
- 为 `ProviderHeader` 与 `GuardedElement` 中仅由 `lower_frontend` 回归测试读取的槽位与访问器加上 `#[cfg(test)]`，并从 `crates/lang-codegen/src/ssa/mod.rs` 移除 `pub(crate) mod provider;` 上的 `#[allow(dead_code)]`。

### 2.3 移除 `map_source::unit_drop_event` 并门控仅测试使用的 `lower_frontend` 支持项

- 从 `crates/lang-codegen/src/ssa/map_source.rs` 移除无调用方的 `unit_drop_event`。
- 为仅由 `#[cfg(test)] mod lower_frontend;` 及单元测试消费的模块与项添加 `#[cfg(test)]` 门控：
  - `crates/lang-codegen/src/ssa/mod.rs` 中的 `mod map_source;`
  - `crates/lang-codegen/src/ssa/borrow_result_support.rs` 中的 `pub(super) mod stable_map;`
  - `crates/lang-codegen/src/ssa/carry.rs` 中的 `CaptureLoan`、`ResultSourceLoan`、`indexed_aliases` 与 `apply_indexed_alias_column`
  - `crates/lang-codegen/src/ssa/expression_core.rs` 中的 `IntegerFact`、`IntegerLowering` 与 `lower_integer`

## 3. 验收标准

- [x] 移除 `UnitExpressionLowerer` 死字段、`provider` 死辅助函数与 `#[allow(dead_code)]`、`map_source::unit_drop_event`，并完成测试专用支持项的 `#[cfg(test)]` 门控；provider CFG fixture 仍使用的 `ProviderNext` / `ProviderHeader::backedge` / `finish_element_and_advance` 改为 `#[cfg(test)]` 保留。
- [x] `scripts/gen_spec_dag.py`、`scripts/check_docs.py` 与 `scripts/check_rust_sizes.py --base main` 通过。
- [x] 本地 `cargo test -p lang-codegen`（`LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21`，与 CI 同一 Clang 21.1 前提）回归：lib 2914 passed / 1 ignored、集成 2 passed、doc-tests 4 passed；CI 以 PR 为准。

## 5. 验收记录账本

| 验收项 | 结果 | 证据 |
|---|---|---|
| 2 个退役项已从版本库移除 | 通过 | 逐个确认：2/2 个路径/符号不再存在 |
| 内容并入声明的合并目标 | 通过 | 2/2 个目标在当前树存在 |
| 修改文件均小于 1000 行 | 通过 | `python3 scripts/check_rust_sizes.py --base f78c8c20` 退出 0 |
| 手写 Rust 文件数净减 | 受限通过 | 历史已 squash 为 `430a0de2`，无法分离单 Spec 计数 |
| `scripts/gen_spec_dag.py` / `scripts/check_docs.py` 通过 | 通过 | 归档后重跑生成物稳定；`documentation check passed` |
| `cargo test -p lang-codegen --lib` | 通过 | `cargo test -p lang-codegen --lib -- refine verify_ownership` 等定向；完整套件默认 `3055 passed / 0 failed / 1 ignored`、feature `3117 passed / 0 failed / 1 ignored` |

核对说明：

- 文件级：`unit_lower/control/nullable_if/field_tests.rs` 不存在（后续由 SPEC-0436 继续收敛）
- 符号级：`finish_element_and_advance_in_block` 0 处命中；`ProviderHeader`/`GuardedElement` 的测试专用访问器已加 `#[cfg(test)]`（见 `ssa/provider.rs`）
- 本 Spec 的删除可能由更后续的 Spec 完成（该文件后续再次被收敛），因此这里只声明“当前树已不存在”，不声明删除由本 Spec 独立完成。
