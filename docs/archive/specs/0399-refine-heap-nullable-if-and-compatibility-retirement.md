# SPEC-0399：同根堆对象可空 `if`、接口委托与混合兼容回退旧路径退役

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的同根堆对象可空 `if` 路由补齐、接口委托调用路由补齐、可空 `Map` 消费旧路径退役及 `nullable when` 混合兼容边界收敛时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0399` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 空安全与错误值](../../guide/09-nullability-errors.md)、[Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户确认合入最新 `main` 后继续推进下一片 E3 旧路径退役 |
| 基线 | [SPEC-0398](0398-refine-nullable-string-and-field-if-retirement.md)（`c96e63e2`） |
| 分支 | `feature/spec-0399-refine-heap-nullable-if-and-compatibility-retirement` |
| 前置 | [SPEC-0385](../../specs/active/0385-refine-deinit-borrow-heap-nullable-if-string-field-retirement.md)、[SPEC-0386](../../specs/active/0386-refine-inout-delegation-object-destructuring-retirement.md)、[SPEC-0395](0395-refine-nullable-while-retirement.md)、[SPEC-0398](0398-refine-nullable-string-and-field-if-retirement.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

将含循环重赋值可空变量的接口委托调用与纯堆对象/`String` 可空 `if` 函数统一纳入 `ssa::refine` 的 `scope_requires_refine` 准入范围，更新“循环内重赋值可空 `String` 视图”路由契约为直接走 `ssa::refine`，退役 `unit_lower/map_consume.rs` 中的可空 `Map` 消费旧路径（`lower_map_consume`、`is_nullable_map_consume_call`），并将 `migrated_family_compatibility`、堆对象可空 `if`、可空根共享借用实参与接口委托接收者收敛为仅服务仍含 `nullable when` 的混合兼容函数。

## 2. 范围与设计

### 2.1 `ssa::refine` 准入补齐与路由契约更新

- 在 `crates/lang-codegen/src/ssa/refine/subset/range_borrow.rs` 中新增 `if_requires_refine`，并在 `expr_requires_refine` 中补充：
  1. 接口委托调用（`self.inputs.source_plan.call_site(self.inputs.source_token, id).is_some_and(|site| !site.delegation().is_empty())`）；
  2. 条件为堆类或 `String` 的可空 `if`（`Nullable(inner)` 满足 `self.class(*inner) || self.string(*inner)`，不要求分支内必有 `non_null_use` 或根 `LoanFact`）。
- 由此使包含 `while` 循环重赋值可空局部变量并伴随接口委托调用或堆对象/`String` 可空 `if` 的函数直接通过 `strip_reassigned_loop_views` 路由至 `ssa::refine`；同步更新 `crates/lang-codegen/src/ssa/refine_subset_tests.rs` 中 `outer_non_null_view_with_loop_reassignment_routes_to_refine` 断言为 `routed == true`。

### 2.2 `unit_lower` 旧路径退役与混合兼容收敛

- `crates/lang-codegen/src/ssa/unit_lower/map_consume.rs`：退役 `lower_map_consume` 与 `is_nullable_map_consume_call`（可空 `Map` 消费已在 `SPEC-0397` 迁入 `ssa::refine`），仅保留 `lower_delegated_call_receiver` 供 `migrated_family_compatibility` 混合函数使用。
- `crates/lang-codegen/src/ssa/unit_lower/{orchestrate.rs, call.rs, borrow.rs, control/nullable_if.rs, control/nullable_if/heap.rs}`：保留 `migrated_family_compatibility` 仅用于仍含 `nullable when` 的混合函数（如 `native_nullable_when_tests/replacement_if.rs`），非兼容路径直接拒绝可空 `String?` `if`、堆可空 `if` 与接口委托调用。

## 3. 验收标准

- [x] 纯堆对象/`String` 可空 `if` 与接口委托调用路由至 `ssa::refine`，`outer_non_null_view_with_loop_reassignment_routes_to_refine` 路由契约更新并通过。
- [x] 退役 `map_consume.rs` 中的可空 `Map` 消费旧入口，同时保持含 `nullable when` 的混合函数兼容通过。
- [x] `scripts/check_rust_sizes.py --base main`、`scripts/check_docs.py` 与全量测试通过，已由 PR #408 合入 `main`（`3fe1920a`）。

## 5. 验收记录账本

| 验收项 | 结果 | 证据 |
|---|---|---|
| 1 个退役项已从版本库移除 | 通过 | 逐个确认：1/1 个路径/符号不再存在 |
| 内容并入声明的合并目标 | 通过（部分宿主被后续 Spec 再次合并） | 2/1 个目标在当前树存在 |
| 修改文件均小于 1000 行 | 通过 | `python3 scripts/check_rust_sizes.py --base f78c8c20` 退出 0 |
| 手写 Rust 文件数净减 | 受限通过 | 历史已 squash 为 `430a0de2`，无法分离单 Spec 计数 |
| `scripts/gen_spec_dag.py` / `scripts/check_docs.py` 通过 | 通过 | 归档后重跑生成物稳定；`documentation check passed` |
| `cargo test -p lang-codegen --lib` | 通过 | 完整默认配置 lib 套件 `3055 passed / 0 failed / 1 ignored`、feature 套件 `3117 passed / 0 failed / 1 ignored`（均已执行） |

核对说明：

- 确认不存在：`unit_lower/map_consume.rs`（其余 §2 路径为保留宿主）
- 本 Spec 的文件级退役项在当前树均已不存在；其中部分文件随后又被更后续 Spec 再次收敛，帐本只声明当前状态。历史已 squash，无法把删除单独归因到本 Spec。
