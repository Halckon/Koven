# SPEC-0393：`RangeView`、借用结果函数与绑定、`Rc` 操作及带捕获闭包旧路径退役

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的 `RangeView`（`unit_lower/range.rs`）、借用结果返回与调用（`unit_lower/borrow_result.rs`）、`Rc.share()` / `Rc.value`（`unit_lower/rc.rs`、`unit_lower/control/nullable_if/rc.rs`）及带捕获闭包构造（`unit_lower/closure.rs`）旧路径退役时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 与 [ADR-0034](../../adr/accepted/0034-refine-closures-and-callable-views.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0393` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户确认在 SPEC-0389 基线上继续推进 E3 下一个切片；原编号 0390 与 main 的 CI 镜像 Spec 冲突，合入 main 时改为 0393 |
| 基线 | `90cd542b`（SPEC-0389）；合入 main `6272d3e5` |
| 分支 | `feature/spec-0393-refine-range-borrow-result-rc-closure-retirement` |
| 前置 | [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md)、[ADR-0034](../../adr/accepted/0034-refine-closures-and-callable-views.md)、[SPEC-0359](../../archive/specs/0359-refine-rc.md)、[SPEC-0370](0370-refine-lambda-bodies.md)、[SPEC-0371](0371-refine-integer-range-text.md)、[SPEC-0389](../../archive/specs/0389-refine-known-function-shared-capture-container-element-retirement.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

在 SPEC-0359/0370/0371/0389 已把 `RangeView`（`View<T>`）、借用结果（`borrow val`、`borrow_return`、`carrier_return`）、`Rc` 操作与带捕获闭包迁入 `ssa::refine` 的基础上，让这些家族在细化层无条件默认路由，并删除 `unit_lower` 中已被取代的旧路径：

1. **默认路由**：`View<T>` 作为已迁移存储类型（`subset/heap.rs` 不再要求 `unit_requires_refine`）；`range_borrow.rs` 的范围/借用结果准入去掉 `unit_requires_refine()` / `scope_requires_refine()` 门控；`RangeView.size`、借用结果调用与 `RangeView` carrier 调用计入“需细化层”的构造，使同一作用域内依赖它们的门控（临时 `for` 源等）一致放行。
2. **补齐细化层能力**（旧路径删除后由细化层独立承担）：
   - `rangeView` 源可以是视图本身（标准库 `take(View<T>, n)` 等重载）；短区间 `for` 源与调用实参可以由临时容器支撑（`take(source(), 1)`、`take(take(source(), 2), 1)`），控制转移离开实参时先 `RangeEnd` 再析构临时值；
   - 控制转移（`return`）结束借用绑定只作用于本路径，fall-through 路径仍持有绑定（修复 `borrow val parent = take(root, 2)` 后实参内 `return` 的 verifier 失败）；
   - 常量左操作数的短路（`UnitShortCircuitRhs::Always/Never`）；短路自身的条件分支以短路表达式为 origin；
   - 扩展接收者 `this.size`（`List<T>` 与 `View<T>` 接收者）；
   - 借用结果函数转发可空存储（`fun <T> view(source: T): borrow T from source`，`T = Token?`），`borrow val` 绑定可空存储；
   - 函数值调用实参内的 `return`/abort；捕获闭包所在作用域内的 `break`/`continue`；函数值调用与 `RangeView` carrier 调用借出的临时值可被实参内跳转放弃。
3. **退役旧路径**：删除 `unit_lower/range.rs`（549 行）；从 `unit_lower/borrow_result.rs` 删除 `BorrowCall`/`BorrowReturn`/字段借用结果；从 `unit_lower/rc.rs`、`unit_lower/control/nullable_if/rc.rs`、`unit_lower/borrow.rs` 删除 `Rc.share()`/`Rc.value`；从 `unit_lower/closure.rs` 删除带捕获闭包构造（`!plan.captures.is_empty()` 显式 `UnsupportedNode`）。

**范围收缩**：直接 `String.clone()` 的旧路径（`unit_lower/scalar.rs::lower_string_operation`）本片**保留**。原计划一并退役，但实测会把大量仍依赖旧路径结构的 Map/Resource 家族函数整体推入细化层，牵动约 40 个按旧 SSA 形状钉住的测试；拆到后继切片 [SPEC-0394](0394-refine-string-clone-retirement.md) 单独处理。

## 2. 边界与不变量

- `unit_lower/borrow_result.rs` 保留编译单元级 `validate_declarations`，以及遗留 `map_iteration/borrowed.rs` 使用的直接根 `Borrow` 参数 `borrow val` 别名；该别名在作用域不需要细化层时仍走旧路径。直接字段绑定与已限定到同一别名的稳定根 Map provider 由专门入口核验。编译单元启用旧前置门禁时，其余绑定保留现有整函数 CFG 边界。
- `unit_lower/rc.rs` 与 `unit_lower/control/nullable_if/rc.rs` 仅保留 `lower_rc_construction`、`validated_rc_operation` 与 `prepare_rc_nullable_if_entry`。
- 已退役家族经默认入口进入细化层并通过 SSA verifier 与 LLVM IR 生成；`with_per_family_lowering` 强制旧路径时确定性返回 `UnsupportedNode`。
- 非捕获函数值调用实参内的 `break`/`continue` 仍交旧路径（细化层对“无出口 loop + 实参内 `continue`”会产生 `MissingOwnedExit`，留作后继缺口）。

## 3. 验收标准

- [x] 删除 `unit_lower/range.rs`，精简 `unit_lower/{mod.rs,orchestrate.rs,borrow.rs,borrow_result.rs,call.rs,call_lifetimes.rs,closure.rs,rc.rs}`、`call_lifetimes/nullable_child.rs`、`control/nullable_if/rc.rs`、`loop_control/{iteration.rs,iteration_cleanup.rs}`。
- [x] `ssa::refine_subset_tests::range_view_borrow_return_and_borrow_val_route_to_refine` 与 `ssa::refine_tests::map_condition_tests::retired_borrow_result_rc_operation_string_clone_and_captured_closure_boundary_tests`（本片去掉其 `String.clone()` 一项，SPEC-0394 恢复）通过；main 带入的 `container_append_admission_does_not_perturb_or_miscompile_sibling_functions` 通过。
- [x] `cargo test -p lang-codegen --lib`：2658 通过、0 失败（合入 main 后实测；接手时为约 280 个失败）。
- [ ] 门禁、lang-codegen 全目标与 lang-cli 测试、fmt、clippy、尺寸护栏、`check_docs.py` 全部通过（见实施记录）；双宿主 CI 以 PR 为准。

### 改动的既有测试

旧路径删除后，下列测试原先钉住的旧 SSA 形状或旧边界不再成立：

- `range_lowering_tests::local_control::short_circuit`（含 receiver，43 项）：删除只对旧路径成立的 carrier 布局断言（`short_circuit_transport` / `short_circuit_prefix_transport` / `short_circuit_receiver_transport`），改为断言短路仍有自己的条件分支；各源的 verifier 与 native 运行输出断言保留。
- `range_lowering_tests::receiver::count_break`：步长常量允许经循环头参数到达加法。
- `native_direct_call_prefix_tests::ordinary_direct_prefix_scope_negatives_keep_exact_call_rejections` 与 `unit_lower_closure_tests::unsupported_closure_surfaces_remain_atomic_boundaries`：去掉“函数值调用实参内 `return`”反例，该形状现由细化层降低。
- `refine_tests::map_condition_tests` 的退役边界测试去掉 `String.clone()` 一项（本片保留其旧路径；SPEC-0394 退役后恢复）。

## 4. 实施记录

| 提交 | 内容 |
|---|---|
| `46566977` | 退役旧路径并去掉范围/借用结果门控（接手前；验收项当时未实测） |
| 合并提交 | 合入 main `6272d3e5`；Spec 改号 0393 |
| 本提交 | 补齐第 1 节第 2 项的细化层能力，恢复 `String.clone()` 旧路径，调整上节所列测试 |
| SPEC-0392 复审 | 按绑定形状保留旧 CFG 边界；两条有/无兄弟函数的原生回归见[复审账本](0392-compiler-layered-review.md#106-r4-借用绑定校验归属与兄弟函数对照)。 |
