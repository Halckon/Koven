# SPEC-0414：`lower_frontend` 标量、枚举、闭包 ABI 与 Map 消费/守卫壳模块退役

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的 `ssa/lower_frontend` 标量/整数区间、枚举条件、闭包 ABI 查询与 Map 消费/可空守卫壳子模块收敛退役时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0414` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 空安全与错误值](../../guide/09-nullability-errors.md)、[Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户指示继续下一批实施 |
| 基线 | [SPEC-0413](0413-refine-verify-operation-ownership-and-content-flow-shells-retirement.md)（`0d3a5635`） |
| 分支 | `feature/spec-0414-refine-model-constant-and-lower-frontend-shells-retirement` |
| 前置 | [SPEC-0381](0381-refine-default-routing-and-legacy-retirement.md)、[SPEC-0383](0383-refine-final-legacy-retirement.md)、[SPEC-0404](0404-refine-unit-lower-shell-modules-and-duplicate-helpers-retirement.md)、[SPEC-0405](0405-refine-dead-helpers-and-test-entry-gates-retirement.md)、[SPEC-0406](0406-refine-unit-lower-shell-modules-and-operator-helpers-retirement.md)、[SPEC-0407](0407-refine-unit-lower-deinit-cfg-and-support-helpers-retirement.md)、[SPEC-0408](0408-refine-ssa-type-query-shells-and-null-literal-helpers-retirement.md)、[SPEC-0409](0409-refine-deinit-and-build-receipt-shells-retirement.md)、[SPEC-0410](0410-refine-delegated-receiver-and-build-subset-shells-retirement.md)、[SPEC-0411](0411-refine-ssa-integer-range-unit-lower-and-bounded-route-shells-retirement.md)、[SPEC-0412](0412-refine-render-verify-and-map-frontend-shells-retirement.md)、[SPEC-0413](0413-refine-verify-operation-ownership-and-content-flow-shells-retirement.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

在 `SPEC-0413` 基础上，继续收敛并退役 `crates/lang-codegen/src/ssa/lower_frontend` 下的 9 个辅助壳模块（`statement.rs`、`constant.rs`、`checked.rs`、`integer_range.rs`、`enum_condition.rs`、`source_closure/abi.rs`、`map_consume/nullable/cleanup.rs`、`map_consume/entry_borrows.rs`、`orchestrate/nullable_map_guard_support/state.rs`），所有合并后目标文件均严格控制在 `< 500` 行内。

## 2. 范围与设计

### 2.1 收敛 `lower_frontend` 顶层声明、常量、整数与枚举壳模块

- **退役 `crates/lang-codegen/src/ssa/lower_frontend/{statement.rs, constant.rs}`（15 + 47 行）**：将 `StatementLowering` 实现与 `lower_constant` 收归 `crates/lang-codegen/src/ssa/lower_frontend/declarations.rs`（合并后 175 行，`< 500` 行），并从 `lower_frontend.rs` 移除 `mod statement;` 与 `mod constant;`。
- **退役 `crates/lang-codegen/src/ssa/lower_frontend/{checked.rs, integer_range.rs}`（78 + 78 行）**：将 `CheckedRoles`、`CheckedLowering` 实现与 `lower_integer_range` 收归 `crates/lang-codegen/src/ssa/lower_frontend/integer.rs`（合并后 322 行，`< 500` 行），并从 `lower_frontend.rs` 移除 `mod checked;` 与 `mod integer_range;`。
- **退役 `crates/lang-codegen/src/ssa/lower_frontend/enum_condition.rs`（89 行）**：将 `lower_copyable_enum_case_condition` 收归同属枚举 tag 判定的 `crates/lang-codegen/src/ssa/lower_frontend/enum_type_test.rs`（合并后 305 行，`< 500` 行），并从 `lower_frontend.rs` 移除 `mod enum_condition;`。

### 2.2 收敛 `lower_frontend` 闭包 ABI 与 Map 消费/可空守卫壳子模块

- **退役 `crates/lang-codegen/src/ssa/lower_frontend/source_closure/abi.rs`（118 行）**：将 `CallableLayouts::token_type` 与 `CallableLayouts::expression_type` 内联收归 `crates/lang-codegen/src/ssa/lower_frontend/source_closure.rs`（合并后 178 行，`< 500` 行），并移除 `mod abi;`。
- **退役 `crates/lang-codegen/src/ssa/lower_frontend/map_consume/nullable/cleanup.rs`（114 行）与 `map_consume/entry_borrows.rs`（172 行）**：将 `preflight_nullable_consume_cleanup` 内联入 `crates/lang-codegen/src/ssa/lower_frontend/map_consume/nullable.rs`（合并后 409 行，`< 500` 行）；将 `ensure_map_consume_entry_borrows` 内联入 `crates/lang-codegen/src/ssa/lower_frontend/map_consume.rs`（合并后 438 行，`< 500` 行）。
- **退役 `crates/lang-codegen/src/ssa/lower_frontend/orchestrate/nullable_map_guard_support/state.rs`（51 行）**：将 `Snapshot` 内联收归 `crates/lang-codegen/src/ssa/lower_frontend/orchestrate/nullable_map_guard_support.rs`（合并后 323 行，`< 500` 行），并移除 `mod state;`。

## 3. 验收标准

- [x] 退役 `lower_frontend/{statement.rs, constant.rs, checked.rs, integer_range.rs, enum_condition.rs, source_closure/abi.rs, map_consume/nullable/cleanup.rs, map_consume/entry_borrows.rs, orchestrate/nullable_map_guard_support/state.rs}` 共 9 个壳模块文件。
- [x] `scripts/gen_spec_dag.py`、`scripts/check_docs.py` 与 `scripts/check_rust_sizes.py --base main` 通过。
- [ ] 本地 `cargo test -p lang-codegen` 回归验证通过；CI 以 PR 为准。
