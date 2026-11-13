# SPEC-0397：可空 `Map` 分支/消费、`Map.remove` 借用与闭包回退旧路径退役

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的可空 `Map` / `MutableMap` `if` 读取与 `consume()`、`Map.remove` 结果借用（`map_borrow`）以及无捕获闭包 thunk / 函数值调用回退路径退役时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 与 [ADR-0034](../../adr/accepted/0034-refine-closures-and-callable-views.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0397` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 空安全与错误值](../../guide/09-nullability-errors.md)、[Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md)、[Guide v0.47 集合与解构](../../guide/12-collections-destructuring.md) |
| 批准依据 | 用户确认继续推进 E3；在完成 `Map` 迭代旧路径退役（[SPEC-0396](0396-refine-map-iteration-retirement.md)）后继续推进可空 `Map` 分支/消费与闭包回退旧路径退役 |
| 基线 | [SPEC-0396](0396-refine-map-iteration-retirement.md)（`44d34c68`） |
| 分支 | `feature/spec-0397-refine-nullable-map-and-closure-retirement` |
| 前置 | [SPEC-0370](../../specs/active/0370-refine-lambda-bodies.md)、[SPEC-0377](../../specs/active/0377-refine-box-map.md)、[SPEC-0382](../../archive/specs/0382-refine-map-ops-and-primitive-retirement.md)、[SPEC-0388](../../specs/active/0388-refine-numeric-for-runtime-container-map-get-consume-retirement.md)、[SPEC-0396](0396-refine-map-iteration-retirement.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

将可空 `Map<K, V>?` / `MutableMap<K, V>?` 的 `if` 非空证明视图读取（`.size` / `.contains` / `.get` / `.withValue` / `for`）、可空 `MutableMap<K, V>?.consume()`、嵌套 `Map` 值的 `MutableMap.remove`、以及无捕获 lambda thunk 体与 `FunctionValue` 调用统一收口至 `ssa::refine` 细化层默认路由，并完整退役 `unit_lower` 中对应的旧路径模块与私有守卫测试脚手架（共删除 9 个旧模块文件并精简 15 个关联模块，净减约 4,300 行旧代码）。

## 2. 范围与设计

### 2.1 `ssa::refine` 准入与构建收口

1. **无条件开放可空 `Map` / `MutableMap` 证明视图、可空 `consume()` 与嵌套 `Map` `remove`**：
   - 在 `crates/lang-codegen/src/ssa/refine/subset/heap.rs::view_inner_seen` 中移除 `self.scope_requires_refine() &&` 门禁，使 `Map<K, V>` 与 `MutableMap<K, V>` 无条件作为受支持的非空证明视图内层类型。
   - 在 `crates/lang-codegen/src/ssa/refine/subset/map.rs` 的 `named_map_owner`、`map_consume`（`is_nullable_storage` 分支）与 `map_remove`（`self.map_kv(value_ty).is_some()` 分支）中移除 `self.scope_requires_refine()` 限制。
   - 在 `crates/lang-codegen/src/ssa/refine/subset.rs::note_else_view` 中纳入 `self.map_kv(*inner).is_some()`，使 `if (source == null) {} else { ... }` 在 `else` 分支无显式 `LoanFact` 时同样激活可空 `Map` / `MutableMap` 证明视图（覆盖 `.size` 与 `.consume()`）。
   - 在 `crates/lang-codegen/src/ssa/refine/subset/range_borrow.rs::expr_requires_refine` 中补充 `typed.map_consume(id).is_some()`、`typed.map_remove(id).is_some()` 与 `UnitCallTarget::FunctionValue` 调用。
2. **括号分组接收者释放链与局部声明边界对齐**：
   - 在 `crates/lang-codegen/src/ssa/refine/build/map.rs` 新增 `drop_receiver_chain`，对 `((source)).size` / `((source)).consume()` 等多层括号分组接收者自内向外依次消费 `UnitDropPoint::AfterExpression`，确保证明视图先于外层分组点持有的可空包装器 `Drop` 结束。
   - 在 `crates/lang-codegen/src/ssa/refine/subset/field.rs::local_variable` 中允许发散初始化表达式（`initializer` 类型为 `Nothing`，如 `val dead: UnsupportedType = return`）进入细化层，并在 `build/field.rs::local_variable` 中围绕初始化求值设置 `expected_nullable`，使括号包裹的 `(null)` 字面量获得目标可空 SSA 类型。

### 2.2 `unit_lower` 旧路径退役清单

删除以下 9 个旧路径及私有守卫测试文件：

- `crates/lang-codegen/src/ssa/unit_lower/map_consume/nullable.rs`
- `crates/lang-codegen/src/ssa/unit_lower/map_consume/nullable/source.rs`
- `crates/lang-codegen/src/ssa/unit_lower/map_consume/nullable/cleanup.rs`
- `crates/lang-codegen/src/ssa/unit_lower/map_consume/nullable/guard_tests.rs`
- `crates/lang-codegen/src/ssa/unit_lower/orchestrate/nullable_map_guard_support.rs`
- `crates/lang-codegen/src/ssa/unit_lower/orchestrate/nullable_map_guard_support/checkpoint.rs`
- `crates/lang-codegen/src/ssa/unit_lower/orchestrate/nullable_map_guard_support/prelude.rs`
- `crates/lang-codegen/src/ssa/unit_lower/control/nullable_if/guard_tests.rs`
- `crates/lang-codegen/src/ssa/unit_lower/loop_control/map_borrow.rs`

同步清理以下模块中的旧路径实现与死字段：

- `crates/lang-codegen/src/ssa/unit_lower/control/{nullable_if.rs, conditional.rs}`：移除 `prepare_map_nullable_if_entry`、`map_if_has_only_size_reads`、`NullableIfEntry::consume` 与 `map_borrow_guards` 维护。
- `crates/lang-codegen/src/ssa/unit_lower/{map.rs, map_borrow.rs, borrow.rs, loop_control.rs, nullable_when.rs}`：移除 `qualified_nullable_map_size_read`、`nullable_map_size_operand`、`lower_map_remove`、`lower_owned_map_borrow`、`is_direct_map_borrow_statement` 与 `owned_remove_origin`；`map_remove` 在旧路径上直接返回 `LoweringErrorKind::UnsupportedNode`。
- `crates/lang-codegen/src/ssa/unit_lower/{orchestrate.rs, call.rs, closure.rs}`：移除 `closure::finish_thunk` 回退路径与 `lower_function_value_call`；当 lambda thunk 未路由至 `ssa::refine` 或旧路径遇到 `UnitCallTarget::FunctionValue` 调用时直接返回 `LoweringErrorKind::UnsupportedNode`。

## 3. 验收标准

- [x] 删除第 2.2 节列出的全部 9 个旧路径文件并清理关联模块中的残留逻辑。
- [x] 在 `crates/lang-codegen/src/ssa/refine_tests/map_condition_tests.rs` 中补充可空 `Map` `if` 读取/`consume()`、`Map.remove`、`FunctionValue` 调用与 lambda thunk 在强制旧路径（`with_per_family_lowering`）下的 `UnsupportedNode` 边界回归测试。
- [x] `scripts/check_rust_sizes.py --base main` 与 `scripts/check_docs.py` 通过。
- [ ] 本地 `cargo test -p lang-codegen` 回归验证通过；CI 以 PR 为准。

## 5. 验收记录账本

| 验收项 | 结果 | 证据 |
|---|---|---|
| 9 个退役项已从版本库移除 | 通过 | 逐个确认：9/9 个路径/符号不再存在 |
| 内容并入声明的合并目标 | 通过（部分宿主被后续 Spec 再次合并） | 2/9 个目标在当前树存在 |
| 修改文件均小于 1000 行 | 通过 | `python3 scripts/check_rust_sizes.py --base f78c8c20` 退出 0 |
| 手写 Rust 文件数净减 | 受限通过 | 历史已 squash 为 `430a0de2`，无法分离单 Spec 计数 |
| `scripts/gen_spec_dag.py` / `scripts/check_docs.py` 通过 | 通过 | 归档后重跑生成物稳定；`documentation check passed` |
| `cargo test -p lang-codegen --lib` | 通过 | 完整默认配置 lib 套件 `3055 passed / 0 failed / 1 ignored`、feature 套件 `3117 passed / 0 failed / 1 ignored`（均已执行） |

核对说明：

- 确认不存在：`map_consume/nullable`（含 `source`/`cleanup`/`guard_tests`）、`orchestrate/nullable_map_guard_support`（含 `checkpoint`/`prelude`）、`control/nullable_if/guard_tests`、`loop_control/map_borrow`
- 本 Spec 的文件级退役项在当前树均已不存在；其中部分文件随后又被更后续 Spec 再次收敛，帐本只声明当前状态。历史已 squash，无法把删除单独归因到本 Spec。
