# SPEC-0408：`ssa` 类型与源查询壳模块收敛及 `null` 字面量判定辅助去重

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的 `ssa` 顶层类型构造壳模块（`inline_nullable`、`integer_range_type`）、编译单元源查询壳模块（`unit_source_query`）收敛及跨模块 `null` 字面量判定辅助（`is_null_literal`）去重时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0408` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 空安全与错误值](../../guide/09-nullability-errors.md)、[Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户指示继续推进下一批 E3 收尾清理 |
| 基线 | [SPEC-0407](../../archive/specs/0407-refine-unit-lower-deinit-cfg-and-support-helpers-retirement.md)（`cf0cf4ef`，含 `main` `e659b66a`） |
| 分支 | `feature/spec-0408-refine-ssa-type-query-shells-and-null-literal-helpers-retirement` |
| 前置 | [SPEC-0383](../../specs/active/0383-refine-final-legacy-retirement.md)、[SPEC-0404](../../archive/specs/0404-refine-unit-lower-shell-modules-and-duplicate-helpers-retirement.md)、[SPEC-0405](../../archive/specs/0405-refine-dead-helpers-and-test-entry-gates-retirement.md)、[SPEC-0406](../../archive/specs/0406-refine-unit-lower-shell-modules-and-operator-helpers-retirement.md)、[SPEC-0407](../../archive/specs/0407-refine-unit-lower-deinit-cfg-and-support-helpers-retirement.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

在 `SPEC-0407` 基础上，继续收敛 `crates/lang-codegen/src/ssa` 顶层的轻量类型构造壳模块（`inline_nullable.rs`、`integer_range_type.rs`）与编译单元源查询壳模块（`unit_source_query.rs`、`unit_source_query_tests.rs`），并在 `crates/lang-codegen/src/ssa/lowering_support.rs` 中统一穿透括号分组的 `is_null_literal` 辅助函数，消除 `unit_lower`、`refine` 与 `lower_frontend` 中的四处重复实现。

## 2. 范围与设计

### 2.1 收敛 `inline_nullable.rs` 与 `integer_range_type.rs` 至 `types.rs`

- **退役 `crates/lang-codegen/src/ssa/inline_nullable.rs` 与 `crates/lang-codegen/src/ssa/integer_range_type.rs`**：将 `Module::{add_inline_nullable_type, inline_nullable}`、`inline_nullable_niche` 以及 `Module::{add_integer_range_type, integer_range_endpoint, valid_integer_range_type}` 收归 `crates/lang-codegen/src/ssa/types.rs`（合并后 418 行，满足单文件 500 行预算与 `Boundary::Ir` 定向依赖门禁），更新 `verify_types.rs` 与 `ssa/mod.rs` 并删除上述两个壳文件。

### 2.2 收敛 `unit_source_query.rs` 与 `unit_source_query_tests.rs` 至 `lowering_support`

- **退役 `crates/lang-codegen/src/ssa/unit_source_query.rs` 与 `crates/lang-codegen/src/ssa/unit_source_query_tests.rs`**：将 `parsed_by_source_unit` 与 `non_completing_call_receiver` 收归中立支撑模块 `crates/lang-codegen/src/ssa/lowering_support.rs`，将单测收归 `crates/lang-codegen/src/ssa/lowering_support_tests.rs`，并更新 `unit_plan`、`unit_lower` 与 `refine` 调用点。

### 2.3 在 `lowering_support.rs` 中统一 `is_null_literal`

- 在 `crates/lang-codegen/src/ssa/lowering_support.rs` 中提供 `is_null_literal(parsed, expression)`。
- 在 `unit_lower/nullable_comparison.rs`、`refine/subset/map.rs`、`refine/build/map.rs` 与 `lower_frontend/map_result.rs` 中复用该统一实现，删除重复 AST 递归代码。

## 3. 验收标准

- [x] 退役 `inline_nullable.rs`、`integer_range_type.rs`、`unit_source_query.rs` 与 `unit_source_query_tests.rs`，完成 `is_null_literal` 跨模块去重。
- [x] `scripts/gen_spec_dag.py`、`scripts/check_docs.py` 与 `scripts/check_rust_sizes.py --base main` 通过。
- [ ] 本地 `cargo test -p lang-codegen` 回归验证通过；CI 以 PR 为准。

## 5. 验收记录账本

| 验收项 | 结果 | 证据 |
|---|---|---|
| 4 个退役项已从版本库移除 | 通过 | 逐个确认：4/4 个路径/符号不再存在 |
| 内容并入声明的合并目标 | 通过 | 4/4 个目标在当前树存在 |
| 修改文件均小于 1000 行 | 通过 | `python3 scripts/check_rust_sizes.py --base f78c8c20` 退出 0 |
| 手写 Rust 文件数净减 | 受限通过 | 历史已 squash 为 `430a0de2`，无法分离单 Spec 计数 |
| `scripts/gen_spec_dag.py` / `scripts/check_docs.py` 通过 | 通过 | 归档后重跑生成物稳定；`documentation check passed` |
| `cargo test -p lang-codegen --lib` | 通过 | 完整默认配置 lib 套件 `3055 passed / 0 failed / 1 ignored`、feature 套件 `3117 passed / 0 failed / 1 ignored`（两次全量均已执行）；本 Spec 未另跑独立定向重跑 |

核对说明：

- 确认不存在：`unit_lower/control/nullable_if/inline_nullable.rs`、`integer_range_type.rs`、`unit_source_query.rs`、`unit_source_query_tests.rs`
- 宿主 `ssa/types.rs`、`ssa/lowering_support.rs`、`carry.rs` 等仍存在
- 本 Spec 声明的文件级退役项在当前树均已不存在；历史已 squash，无法把删除单独归因到本 Spec 的提交，账本只声明“当前状态已达成”。
