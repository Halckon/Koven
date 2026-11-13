# SPEC-0412：`render`、`verify` 及 `lower_frontend` Map 壳模块退役

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的 `ssa/render` 标量名壳模块、`ssa/verify` 可调用视图与闭包逃逸类型壳模块及 `lower_frontend` 下 Map 查询/消费壳模块（`map_with`、`map_require`）收敛退役时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0412` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 空安全与错误值](../../guide/09-nullability-errors.md)、[Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户指示继续推进下一步实施 |
| 基线 | [SPEC-0411](../../archive/specs/0411-refine-ssa-integer-range-unit-lower-and-bounded-route-shells-retirement.md)（`3b7ddf29`） |
| 分支 | `feature/spec-0412-refine-render-verify-and-map-frontend-shells-retirement` |
| 前置 | [SPEC-0381](0381-refine-default-routing-and-legacy-retirement.md)、[SPEC-0383](0383-refine-final-legacy-retirement.md)、[SPEC-0404](0404-refine-unit-lower-shell-modules-and-duplicate-helpers-retirement.md)、[SPEC-0405](0405-refine-dead-helpers-and-test-entry-gates-retirement.md)、[SPEC-0406](0406-refine-unit-lower-shell-modules-and-operator-helpers-retirement.md)、[SPEC-0407](0407-refine-unit-lower-deinit-cfg-and-support-helpers-retirement.md)、[SPEC-0408](0408-refine-ssa-type-query-shells-and-null-literal-helpers-retirement.md)、[SPEC-0409](0409-refine-deinit-and-build-receipt-shells-retirement.md)、[SPEC-0410](0410-refine-delegated-receiver-and-build-subset-shells-retirement.md)、[SPEC-0411](../../archive/specs/0411-refine-ssa-integer-range-unit-lower-and-bounded-route-shells-retirement.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

在 `SPEC-0411` 基础上，继续收敛并退役 `crates/lang-codegen/src/ssa/render` 标量名渲染壳模块（`scalar_names.rs`）、`ssa/verify` 闭包逃逸图类型壳模块（`closure_escape/types.rs`）与可调用视图验证壳模块（`callable_view.rs`），以及 `ssa/lower_frontend` 下的 Map 协同消费与查询借用壳模块（`map_with.rs`、`map_require.rs`），所有合并后模块均严格遵守文件大小限制。

## 2. 范围与设计

### 2.1 收敛 `ssa/render/scalar_names.rs` 至 `ssa/render.rs`

- **退役 `crates/lang-codegen/src/ssa/render/scalar_names.rs`（46 行）**：将 `render_scalar_names` 内联入 `crates/lang-codegen/src/ssa/render.rs`，并移除 `mod scalar_names;`。合并后 `render.rs` 保持在基线配额（`< 1000` 行）之内。

### 2.2 收敛 `ssa/verify` 下的壳模块

- **退役 `crates/lang-codegen/src/ssa/verify/closure_escape/types.rs`（92 行）**：将 `TypeCaptures` 结构体与其关联实现内联入 `crates/lang-codegen/src/ssa/verify/closure_escape.rs`（合并后 169 行，`< 500` 行），保留现有测试挂载 `#[cfg(test)] #[path = "closure_escape/types/graph_tests.rs"] mod graph_tests;`，并移除 `mod types;`。
- **退役 `crates/lang-codegen/src/ssa/verify/callable_view.rs`（47 行）**：将 `verify_entity` 与 `verify_returns` 收归 `crates/lang-codegen/src/ssa/verify/module.rs`（重命名为 `verify_callable_view_entity` 与 `verify_callable_view_returns`，合并后 116 行，`< 500` 行），从 `verify.rs` 移除 `mod callable_view;` 并切换调用至 `module::*`。

### 2.3 收敛 `ssa/lower_frontend` 下的 Map 辅助壳模块

- **退役 `crates/lang-codegen/src/ssa/lower_frontend/map_with.rs`（37 行）与 `map_require.rs`（59 行）**：将 `lower_map_with_value` 与 `lower_map_require_result` 收归同属 Map 消费与查询 lowering 的 `crates/lang-codegen/src/ssa/lower_frontend/map_consume.rs`（合并后 273 行，`< 500` 行），并从 `ssa/lower_frontend.rs` 移除 `mod map_require;` 与 `mod map_with;`。

## 3. 验收标准

- [x] 退役 `ssa/render/scalar_names.rs`、`ssa/verify/closure_escape/types.rs`、`ssa/verify/callable_view.rs` 及 `ssa/lower_frontend/{map_with.rs, map_require.rs}` 共 5 个壳模块文件。
- [x] `scripts/gen_spec_dag.py`、`scripts/check_docs.py` 与 `scripts/check_rust_sizes.py --base main` 通过。
- [ ] 本地 `cargo test -p lang-codegen` 回归验证通过；CI 以 PR 为准。
