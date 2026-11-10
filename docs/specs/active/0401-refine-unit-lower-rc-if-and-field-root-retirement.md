# SPEC-0401：`unit_lower` 可空 `Rc` `if` 证明入口、`validated_rc_operation` 与只写 `shared_field_roots` 退役

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的 `unit_lower` 可空 `Rc` `if` 证明入口、`Rc` 操作事实校验与调用帧只写字段根记账退役时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0401` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 空安全与错误值](../../guide/09-nullability-errors.md)、[Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户确认 `SPEC-0400` 已发起 PR 后继续推进下一批旧路径退役 |
| 基线 | [SPEC-0400](0400-refine-unit-lower-dead-carrier-retirement.md)（`ed7272e2`） |
| 分支 | `feature/spec-0401-refine-unit-lower-rc-if-and-field-root-retirement` |
| 前置 | [SPEC-0359](../../archive/specs/0359-refine-rc.md)、[SPEC-0393](0393-refine-range-borrow-result-rc-closure-retirement.md)、[SPEC-0400](0400-refine-unit-lower-dead-carrier-retirement.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

清理 `SPEC-0393` 将 `Rc` 操作（`rc.share()` 与 `rc.value`）全部迁入 `ssa::refine` 后遗留在 `crates/lang-codegen/src/ssa/unit_lower` 中的可空 `Rc<T>?` `if` 证明入口（`control/nullable_if/rc.rs`）、`validated_rc_operation` 与 `is_direct_function_body_if` 死函数，以及 `PendingCallFrame` 中只写不读的 `shared_field_roots` 记账字段，净减约 310 行冗余代码。

## 2. 范围与设计

### 2.1 退役模块清单

删除以下已无生产/测试消费的文件：

- `crates/lang-codegen/src/ssa/unit_lower/control/nullable_if/rc.rs`

### 2.2 退役 `Rc` `if` 辅助函数与调用帧只写字段

- **可空 `Rc` `if` 证明入口与 `Rc` 操作校验死代码**（自 `SPEC-0393` 起恒不可达）：
  - `control/nullable_if/rc.rs::prepare_rc_nullable_if_entry` 仅当 `then_branch` 内包含 `owned.rc_effects()` 时才生效，而所有含 `rc_operation` 的表达式均由 `ssa::refine` 唯一承接且在 `unit_lower` 中直接拒绝；
  - 从 `crates/lang-codegen/src/ssa/unit_lower/control/nullable_if.rs` 移除 `mod rc;`、`prepare_rc_nullable_if_entry` 分支及唯一被其调用的 `is_direct_function_body_if`；
  - 从 `crates/lang-codegen/src/ssa/unit_lower/rc.rs` 移除唯一被 `control/nullable_if/rc.rs` 调用的 `validated_rc_operation`（保留 `lower_rc_construction`）。
- **调用帧只写不读的 `shared_field_roots` 与冗余包装函数**：
  - 从 `crates/lang-codegen/src/ssa/unit_lower/call_lifetimes.rs` 的 `PendingCallFrame` 移除只写不读的 `shared_field_roots`；
  - 从 `crates/lang-codegen/src/ssa/unit_lower/call.rs` 与 `crates/lang-codegen/src/ssa/unit_lower/field_borrow.rs` 移除所有 `shared_field_roots` 写入，并将 `lower_owned_shared_field_place` 合并回 `lower_owned_field_borrow_place`。

## 3. 验收标准

- [x] 删除 `crates/lang-codegen/src/ssa/unit_lower/control/nullable_if/rc.rs`，并完成 `is_direct_function_body_if`、`validated_rc_operation`、`shared_field_roots` 与 `lower_owned_shared_field_place` 的清理。
- [x] `scripts/gen_spec_dag.py`、`scripts/check_docs.py` 与 `scripts/check_rust_sizes.py --base main` 通过。
- [ ] 本地 `cargo test -p lang-codegen` 回归验证通过；CI 以 PR 为准。
