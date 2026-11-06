# SPEC-0390：`RangeView`、借用结果函数与绑定、`Rc` 操作、`String.clone()` 及带捕获闭包旧路径退役

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的 `RangeView`（`unit_lower/range.rs`）、借用结果返回与调用（`unit_lower/borrow_result.rs`）、`Rc.share()` / `Rc.value`（`unit_lower/rc.rs`、`unit_lower/control/nullable_if/rc.rs`）、直接 `String.clone()`（`unit_lower/scalar.rs`）及带捕获闭包构造（`unit_lower/closure.rs`）旧路径退役时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 与 [ADR-0034](../../adr/accepted/0034-refine-closures-and-callable-views.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0390` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户确认在 SPEC-0389 基线上继续推进 E3 下一个切片 |
| 基线 | `90cd542b`（SPEC-0389） |
| 分支 | `feature/spec-0390-refine-nullable-when-range-borrow-result-retirement` |
| 前置 | [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md)、[ADR-0034](../../adr/accepted/0034-refine-closures-and-callable-views.md)、[SPEC-0359](../../archive/specs/0359-refine-rc.md)、[SPEC-0370](0370-refine-lambda-bodies.md)、[SPEC-0371](0371-refine-integer-range-text.md)、[SPEC-0389](0389-refine-known-function-shared-capture-container-element-retirement.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

在 [SPEC-0359](../../archive/specs/0359-refine-rc.md)、[SPEC-0370](0370-refine-lambda-bodies.md)、[SPEC-0371](0371-refine-integer-range-text.md) 与 [SPEC-0389](0389-refine-known-function-shared-capture-container-element-retirement.md) 已将 `RangeView`（`View<T>`）、借用结果（`borrow val`、`borrow_return`、`carrier_return`）、`Rc` 操作、`String.clone()` 与带捕获闭包迁入 `ssa::refine` 的基础上，将 `RangeView` 与 `borrow_result` 全家族提升为无条件默认路由至 `ssa::refine`，修复细化层多出口短期 `RangeView` 清理，并彻底退役 `unit_lower` 中已被细化层完全取代的 legacy 实现模块与分支（合计清理约 1,480 行旧路径代码）：

1. **将 `RangeView` 与 `borrow_result` 提升为 `ssa::refine` 无条件默认路由并补齐多出口清理**：
   - 在 `crates/lang-codegen/src/ssa/refine/subset.rs` 的 `signature_types` 中将 `View<T>`（`range_view_element(ty).is_some()`）识别为已迁移类型（`migrated = true`）；
   - 在 `crates/lang-codegen/src/ssa/refine/subset/range_borrow.rs` 中移除 `strip_reassigned_loop_views`、`local_borrow_binding`、`range_size`、`range_view_for`、`nested_borrow_argument`、`short_range_argument`、`borrow_return_body` 与 `carrier_return_body` 上的 `unit_requires_refine()` / `scope_requires_refine()` 限制，使所有合法 `View<T>` 与 `borrow_result` 函数默认路由至 `ssa::refine`；
   - 在 `crates/lang-codegen/src/ssa/refine/build/range_borrow.rs` 的 `lower_result_call` 中补齐 `RangeUseSite::Call(id)` 与 `borrow_result_support::{binding, argument}` 边界校验；
   - 在 `crates/lang-codegen/src/ssa/refine/build/range_borrow.rs` 的 `end_short_call_loan` 与 `finish_short_range` 中保留 `self.short_ranges` 映射（使用只读查询而非提前 `remove`），并在 `container_for.rs` 中对 `range_source` 先解括号再查询 `short_ranges`，确保含 `break`/`return` 与正常耗尽的多出口 `for (item in take(xs, ...))` 循环在每条控制流退出边上均精确发出 `RangeEnd` 与继续源借用的 `BorrowEnd`。
2. **退役 `unit_lower/range.rs` 及 `unit_lower/borrow_result.rs` 中的 `BorrowCall` / `BorrowReturn` / 字段借用结果**：
   - 删除 `crates/lang-codegen/src/ssa/unit_lower/range.rs`（549 行），移除 `unit_lower.rs`、`unit_lower/orchestrate.rs`、`unit_lower/borrow.rs`、`unit_lower/call.rs`、`unit_lower/call_lifetimes.rs`、`unit_lower/call_lifetimes/nullable_child.rs`、`unit_lower/loop_control/iteration.rs` 与 `unit_lower/loop_control/iteration_cleanup.rs` 中的 `lower_range_size`、`lower_range_return`、`lower_short_range`、`range_iteration_loan`、`finish_short_range`、`preserve_short_range_ends` 与 `end_short_call_loan`；
   - 从 `crates/lang-codegen/src/ssa/unit_lower/borrow_result.rs` 中移除 `lower_nested_borrow_argument`、`lower_result_return`、`lower_result_call`、`lower_result_place`、`validate_initializer_root`、`validate_return_place`、`clone_field_projection` 与 `clone_field_loan`（约 605 行，使 `borrow_result.rs` 从 875 行降至 262 行），仅保留全单元声明/绑定前置校验与供遗留 `Map` 借用迭代使用的直接根参数 `BorrowBindingStorage::BorrowedStorage` 重借用。
3. **退役 `unit_lower` 中的 `Rc.share()` / `Rc.value`、直接 `String.clone()` 与带捕获闭包构造**：
   - 从 `unit_lower/rc.rs`、`unit_lower/control/nullable_if/rc.rs` 与 `unit_lower/borrow.rs` 中移除 `lower_rc_operation`、`can_share_with_nullable_rc_proof`、`nullable_rc_operand` 及 `rc.value` 借用 place 构造；
   - 从 `unit_lower/scalar.rs` 中移除 `lower_string_operation`；
   - 从 `unit_lower/closure.rs` 的 `lower_callable_literal` 中移除带捕获闭包构造（`Operation::ClosureConstruct`），对 `!plan.captures.is_empty()` 显式返回 `UnsupportedNode`。

## 2. 边界与不变量

- `unit_lower/borrow_result.rs` 仅保留编译单元级 `validate_declarations`、`validate_bindings` 以及供遗留 `map_iteration/borrowed.rs` 使用的直接根 `Borrow` 参数 `borrow val` 别名。
- `unit_lower/rc.rs` 与 `unit_lower/control/nullable_if/rc.rs` 仅保留 `lower_rc_construction`、`validated_rc_operation` 与 `prepare_rc_nullable_if_entry`，供显式使用 `with_per_family_lowering` 的有界可空 `while` 测试使用。
- 所有合法 `RangeView`、`borrow val`、`borrow_return`、`carrier_return`、`Rc` 操作、`String.clone()` 与带捕获闭包经默认入口路由至 `ssa::refine` lowering 并通过 SSA verifier 与 LLVM IR 生成；通过 `with_per_family_lowering` 强制进入 `unit_lower` 时，已退役路径确定性返回 `UnsupportedNode`。

## 3. 验收标准

- [x] 更新 `crates/lang-codegen/src/ssa/refine/subset.rs`、`refine/subset/range_borrow.rs`、`refine/build/{range_borrow.rs,container_for.rs}`，删除 `unit_lower/range.rs`（549 行），并精简 `unit_lower/{mod.rs,orchestrate.rs,borrow.rs,borrow_result.rs,call.rs,call_lifetimes.rs,closure.rs,rc.rs,scalar.rs}`、`unit_lower/call_lifetimes/nullable_child.rs`、`unit_lower/control/nullable_if/rc.rs` 与 `unit_lower/loop_control/{iteration.rs,iteration_cleanup.rs}`（净减约 1,480 行 legacy 代码）。
- [x] 新增 `ssa::refine_subset_tests::range_view_borrow_return_and_borrow_val_route_to_refine` 与 `ssa::refine_tests::map_condition_tests::retired_borrow_result_rc_operation_string_clone_and_captured_closure_boundary_tests` 回归测试并全部通过。
- [x] `python3 scripts/gen_roadmap.py`、`python3 scripts/gen_spec_dag.py`、`python3 scripts/check_docs.py`、`python3 scripts/check_rust_sizes.py --base main`、`cargo fmt --all -- --check` 与 `cargo clippy --locked --workspace --all-targets -- -D warnings` 全部通过。
