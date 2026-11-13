# SPEC-0396：`Map` / `MutableMap` 迭代与关联延续旧路径退役

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的 `Map` / `MutableMap` 迭代家族（`unit_lower/map_iteration.rs` 及其 18 个子模块、`unit_lower/loop_control/map_iteration.rs` 及其 18 个子模块，以及 `iteration.rs` 中专供 `Map` 单元的 `Numeric` 兜底路径与 `map_consume.rs` 中的 `lower_map_consume`）旧路径退役时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0396` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md)、[Guide v0.47 集合与解构](../../guide/12-collections-destructuring.md) |
| 批准依据 | 用户确认继续推进 E3；在完成可空 `while` 家族旧路径退役（[SPEC-0395](0395-refine-nullable-while-retirement.md)）后继续推进 `Map` 迭代旧路径退役 |
| 基线 | [SPEC-0395](0395-refine-nullable-while-retirement.md)（`9f9608ab`，基于 `main` `4ae149a6` PR #354） |
| 分支 | `feature/spec-0396-refine-map-iteration-retirement` |
| 前置 | [SPEC-0377](../../specs/active/0377-refine-box-map.md)、[SPEC-0388](0388-refine-numeric-for-runtime-container-map-get-consume-retirement.md)、[SPEC-0395](0395-refine-nullable-while-retirement.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

将 `for ((k, v) in map)` 迭代语句（`typed.map_iteration(statement).is_some()`）提升为只在 `ssa::refine` 细化层降低的构造，补齐 `ssa::refine` 对 `borrow val` 借用 `Map` 别名迭代、条件接收者析构退出动作（`Action::DropConditionalReceiver`）以及括号分组临时 `Map` 迭代源生命周期清理的支持，并完整退役 `unit_lower` 中专用于 `Map` 迭代与局部延续的 38 个旧路径模块及关联钩子（合计超过 10,600 行代码）。

## 2. 范围与设计

### 2.1 `ssa::refine` 准入与构建收口

1. **`Map` 迭代纳入细化层必需构造**：
   - 在 `crates/lang-codegen/src/ssa/refine/subset/range_borrow.rs` 新增 `stmt_requires_refine`，当 `Statement::For` 携带 `typed.map_iteration(...)` 事实时，将其所在函数与编译单元标记为需要细化层（`span_requires_refine` / `unit_requires_refine`）。
   - 移除 `crates/lang-codegen/src/ssa/refine/subset/map.rs::map_for` 入口的 `if !self.scope_requires_refine() { return false; }` 门禁，使所有合法 `Map` / `MutableMap` `for` 循环直接路由至 `ssa::refine`。
2. **`borrow val` 别名源与条件接收者退出清理准入**：
   - 在 `subset/map.rs::map_for` 的 `UnitIterationSourceAccess::Shared` 分支中，除直接引用 `place.root()` 外，允许遍历绑定到同一 `UnitLoanTarget::Place(place)` 的局部 `borrow val` 别名符号（`self.for_borrowed.contains(&symbol)`）。
   - 在 `subset/map.rs::map_for` 的 `plan.exits()` 校验中允许 `Action::DropConditionalReceiver(_)`，并在 `crates/lang-codegen/src/ssa/refine/build/container_for.rs::iteration_cleanup` 中补齐对应 `UnitDropTarget::This(fact.owner())` 的条件接收者释放，与 `build.rs::collect_drops` 对齐。
3. **括号分组临时 `Map` 源登记**：
   - 在 `build/container_for.rs::map_for` 的 `UnitIterationSourceAccess::Temporary` 分支中，同步将 `plan.source()` 指向的规范 `UnitLoanTarget::Temporary(temp_expr)` 槽位写入 `self.temporaries`，确保形如 `for ((k, v) in (produce()))` 的分组临时 `Map` 在 `LoopExit` 处精确消费其释放事实。

### 2.2 `unit_lower` 旧路径退役清单

删除以下 38 个 `Map` 迭代专属旧路径文件：

- `crates/lang-codegen/src/ssa/unit_lower/map_iteration.rs` 及 `crates/lang-codegen/src/ssa/unit_lower/map_iteration/` 下全部 18 个子模块（`binding_choice.rs`、`borrowed.rs`、`conditional.rs`、`conditional/cleanup.rs`、`expression_birth.rs`、`formed.rs`、`formed/epoch.rs`、`formed/parameter.rs`、`future_use.rs`、`handoff.rs`、`local.rs`、`local/facts.rs`、`local/family.rs`、`nested.rs`、`resource.rs`、`source_identity.rs`、`temporary.rs`、`transfer.rs`）。
- `crates/lang-codegen/src/ssa/unit_lower/loop_control/map_iteration.rs` 及 `crates/lang-codegen/src/ssa/unit_lower/loop_control/map_iteration/` 下全部 18 个子模块（`binding_choice.rs`、`carrier.rs`、`carrier/local.rs`、`checked.rs`、`cleanup.rs`、`cleanup/retained.rs`、`conditional.rs`、`conditional/stack.rs`、`conditional/validate.rs`、`handoff.rs`、`local.rs`、`local/completed.rs`、`local/formed_birth.rs`、`source.rs`、`source/expression_birth.rs`、`state.rs`、`transfer.rs`、`transport.rs`）。

同步清理以下残留路径与钩子：

- `crates/lang-codegen/src/ssa/unit_lower.rs`：移除 `mod map_iteration;`、`map_local_continuation` 字段、`record_map_return_delivery` 调用；当 `Statement::For` 具有 `map_iteration` 事实时直接返回 `LoweringErrorKind::UnsupportedNode`。
- `crates/lang-codegen/src/ssa/unit_lower/loop_control/iteration.rs` 与 `iteration_cleanup.rs`：移除 `IterationState::Map`、`IterationState::Numeric`、`lower_map_unit_numeric_for` 以及全部 `map_*` 迭代清理分支。
- `crates/lang-codegen/src/ssa/unit_lower/map.rs` 与 `map_consume.rs`：移除仅因 `!self.typed.map_iterations().is_empty()` 保留的 `lower_map_consume` 以及 `check_local_map_size` / `record_local_map_size` / `finish_local_map_size`。
- `crates/lang-codegen/src/ssa/unit_lower/{orchestrate.rs, closure.rs, borrow_result.rs, control/conditional.rs, checked.rs, ownership.rs}`：移除 `map_iteration::validate`、`finish_local_map_function`、`stable_borrow_provider`、`checkpoint_map_conditional`、`resume_checked_map`、`emit_local_map_late_drop`、`validate_formed_map_drop` 与 `validate_pure_map_parameter_drop`。

### 2.3 测试契约更新

- `crates/lang-codegen/src/ssa/refine_subset_tests.rs` 与 `crates/lang-codegen/src/ssa/refine_tests/container_tests.rs`：更新 `map_for_unrelated_scalar_scope_keeps_legacy_routing` 与 `map_cross_function_routing_is_isolated`，验证 `for ((k, v) in m)` 无论同单元是否存在其他细化构造均直接路由至 `ssa::refine`。
- `crates/lang-codegen/src/ssa/refine_tests/map_condition_tests.rs`：在 `retired_borrow_result_rc_operation_string_clone_and_captured_closure_boundary_tests` 中新增 `for ((k, v) in m)` 在强制旧路径（`with_per_family_lowering`）下返回 `UnsupportedNode` 的精确源区间边界测试。
- `crates/lang-codegen/src/ssa/map_provider_exits_pipeline_tests/`、`while_flow_tests/` 与 `native_ordered_map_tests/runner.rs`：移除对 `with_per_family_lowering` 的强绑包装，对齐 `ssa::refine` 下 `Map` 迭代头块保持、块参数保活（`[seed, source_loan, cursor]`）、`break` 终止器 `Origin` 与借用参数传递契约。

## 3. 验收标准

- [x] 删除第 2.2 节列出的全部 38 个 `Map` 迭代旧路径模块并清理关联残留钩子。
- [x] 更新 `refine_subset_tests.rs`、`refine_tests/container_tests.rs`、`map_condition_tests.rs` 与 `map_provider_exits_pipeline_tests/` 路由与边界回归测试。
- [x] `scripts/check_rust_sizes.py --base main` 与 `scripts/check_docs.py` 通过。
- [x] 本地 `cargo test -p lang-codegen` 回归验证通过；CI 以 PR 为准。

## 4. 实施记录

| 提交 | 内容 |
|---|---|
| `b77370d6` | 路由 `Map` / `MutableMap` 迭代至 `ssa::refine`，退役 `unit_lower` 中 `map_iteration` 家族旧路径并同步测试与文档 |
| `1e86e35a` | 合并 `origin/main` 并修复构建与测试回归 |
| 本分支收尾提交 | 对齐 `ssa::refine` 的 `Map` 迭代 CFG 头块、块参数保活、`Origin` 与测试契约 |

## 5. 验收记录账本

| 验收项 | 结果 | 证据 |
|---|---|---|
| 2 个退役项已从版本库移除 | 通过 | 逐个确认：2/2 个路径/符号不再存在 |
| 内容并入声明的合并目标 | 通过（部分宿主被后续 Spec 再次合并） | 7/2 个目标在当前树存在 |
| 修改文件均小于 1000 行 | 通过 | `python3 scripts/check_rust_sizes.py --base f78c8c20` 退出 0 |
| 手写 Rust 文件数净减 | 受限通过 | 历史已 squash 为 `430a0de2`，无法分离单 Spec 计数 |
| `scripts/gen_spec_dag.py` / `scripts/check_docs.py` 通过 | 通过 | 归档后重跑生成物稳定；`documentation check passed` |
| `cargo test -p lang-codegen --lib` | 通过 | 完整默认配置 lib 套件 `3055 passed / 0 failed / 1 ignored`、feature 套件 `3117 passed / 0 failed / 1 ignored`（均已执行） |

核对说明：

- 确认不存在：`unit_lower/map_iteration.rs`、`unit_lower/loop_control/map_iteration.rs`；其余 §2 路径为保留的宿主
- 本 Spec 的文件级退役项在当前树均已不存在；其中部分文件随后又被更后续 Spec 再次收敛，帐本只声明当前状态。历史已 squash，无法把删除单独归因到本 Spec。
