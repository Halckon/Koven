# SPEC-0389：具名函数引用、共享闭包捕获、可调用视图借用与顺序容器元素操作旧路径退役

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的具名函数引用（`unit_lower/closure/known_function.rs`）、共享闭包捕获（`unit_lower/closure/shared.rs`）、可调用视图实参借用（`unit_lower/callable_view.rs`）及顺序容器元素下标读取/借用/替换（`unit_lower/container.rs`、`unit_lower/assignment.rs`、`unit_lower/borrow.rs`）旧路径退役时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 与 [ADR-0034](../../adr/accepted/0034-refine-closures-and-callable-views.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0389` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户确认在 SPEC-0388 基线上继续推进 E3 下一个切片 |
| 基线 | `598b054a`（SPEC-0388） |
| 分支 | `feature/spec-0389-refine-container-element-closure-range-retirement` |
| 前置 | [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md)、[ADR-0034](../../adr/accepted/0034-refine-closures-and-callable-views.md)、[SPEC-0370](0370-refine-lambda-bodies.md)、[SPEC-0375](0375-refine-sequential-containers.md)、[SPEC-0376](0376-refine-container-for-generate.md)、[SPEC-0388](0388-refine-numeric-for-runtime-container-map-get-consume-retirement.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

在 [SPEC-0370](0370-refine-lambda-bodies.md)、[SPEC-0375](0375-refine-sequential-containers.md)、[SPEC-0376](0376-refine-container-for-generate.md) 与 [SPEC-0388](0388-refine-numeric-for-runtime-container-map-get-consume-retirement.md) 已将闭包家族、运行时长度容器生成与顺序容器元素操作迁入 `ssa::refine` 的基础上，补齐 `ssa::refine` 对具名函数引用（`UnitCallableOrigin::KnownFunction`）、`Array<Unit>` / `List<Unit>` 运行时生成、嵌套 Copyable `value class` 投影、局部整型复合赋值（`+=` 等）及 `Abort` 边退役借用槽位过滤的准入与构建，并彻底退役 `unit_lower` 中已被细化层完全取代的 legacy 实现模块与分支（合计清理约 670 行旧路径代码）：

1. **补齐 `ssa::refine` 具名函数引用、`Unit` 元素容器、嵌套 `value class` 投影与 `Abort` 活跃性边界**：
   - 在 `refine.rs`、`unit_lower/callable_abi.rs`、`unit_lower/orchestrate.rs`、`refine/subset/callable.rs`、`refine/build/callable.rs`、`refine/subset/container.rs` 与 `refine/build/container.rs` 中接入 `UnitCallableOrigin::KnownFunction(target)`，支持具名顶层/局部函数作为局部函数值、借用实参或 `Array<T>(size, fn_name)` / `List<T>(size, fn_name)` 生成器实参（形成 `ROp::FunctionAddress` 并在调用返回时按临时借用槽位确定性释放）；
   - 在 `refine/subset/container.rs` 与 `refine/build/container.rs` 中允许 `ContainerConstructionKind::RuntimeLength` 及 `.size` 接受元素类型为 `Unit` 的顺序容器（`Array<Unit>`、`List<Unit>`），同时对元素下标读取/借用/替换保持 `!self.unit(element_ty)` 约束；
   - 在 `refine/subset/value.rs` 中支持嵌套 Copyable `value class` 字段投影链（如 `outer.inner.code`），在 `refine/subset/field.rs` 与 `refine/build/expression.rs` 中支持局部整型 `var` 的受检复合赋值（`+=`/`-=`/`*=`/`/=`/`%=`），并在 `refine/body.rs` 与 `refine/emit.rs` 中将 `NullableTake`、`HeapFieldExchange`、`RootReplace` 与 `RootSwap` 消费的证明/独占借用槽位纳入 `retired_slots` 并在 `abort_preds` 过滤已退役 `EntityType::Loan`；
   - 在 `refine/subset/callable.rs` 的 `borrowed_value_parameter_field` 中排除局部 `borrow val` 绑定（`for_borrowed`），保持局部借用结果绑定 MoveOnly 字段再借用的显式拒绝边界。
2. **退役 `unit_lower/closure/known_function.rs`、`unit_lower/closure/shared.rs` 与 `unit_lower/callable_view.rs`**：
   - 删除 `crates/lang-codegen/src/ssa/unit_lower/closure/known_function.rs`（186 行）、`crates/lang-codegen/src/ssa/unit_lower/closure/shared.rs`（96 行）与 `crates/lang-codegen/src/ssa/unit_lower/callable_view.rs`（46 行）；
   - 移除 `unit_lower/call.rs`、`unit_lower/borrow.rs`、`unit_lower/closure.rs` 与 `unit_lower/ownership.rs` 中的 `known_function_address`、`lower_known_function_borrow`、`form_shared_capture`、`release_owner_capture_loans` 与 `callable_view_argument` 调用，对遗留路径上的具名函数值、`ClosureCaptureMode::Shared` 与函数类型 `Borrow` 实参显式返回 `UnsupportedNode`。
3. **退役 `unit_lower/container.rs` 与 `unit_lower/assignment.rs` 中的顺序容器元素下标读取、借用与替换**：
   - 从 `unit_lower/container.rs`、`unit_lower/borrow.rs` 与 `unit_lower/assignment.rs` 中移除 `lower_container_index`、`lower_borrowed_container_element`、`lower_container_assignment` 及其辅助函数（约 340 行，使 `unit_lower/container.rs` 从 676 行降至 338 行），并在 `unit_lower/container.rs` 与 `unit_lower/assignment.rs` 对 `element_place` 显式返回 `UnsupportedNode`。

## 2. 边界与不变量

- `unit_lower/container.rs` 仅保留字面量/空容器构造（`lower_container_construction`）与 `.size`（`lower_container_size`），供尚未迁入细化层的遗留路径测试使用。
- 所有合法具名函数引用、共享/移动捕获闭包、可调用视图借用、`Array<Unit>` / `List<Unit>` 运行时生成及顺序容器元素下标读取/借用/替换经默认入口路由至 `ssa::refine` lowering 并通过 SSA verifier 与 LLVM IR 生成；通过 `with_per_family_lowering` 强制进入 `unit_lower` 时，已退役路径确定性返回 `UnsupportedNode`。

## 3. 验收标准

- [x] 补齐 `crates/lang-codegen/src/ssa/refine/{mod.rs,body.rs,emit.rs,subset.rs}`、`refine/subset/{callable.rs,container.rs,field.rs,range_borrow.rs,value.rs}` 与 `refine/build/{callable.rs,container.rs,expression.rs}`，删除 `unit_lower/closure/known_function.rs`、`unit_lower/closure/shared.rs` 与 `unit_lower/callable_view.rs`，并精简 `unit_lower/{container.rs,assignment.rs,borrow.rs,call.rs,closure.rs,ownership.rs}`（净减约 670 行 legacy 代码）。
- [x] 新增 `ssa::refine_subset_tests::known_function_unit_container_and_nested_value_projection_route_to_refine`、`ssa::refine_subset_tests::mixed_temporary_container_and_map_with_closures_route_to_refine` 与 `ssa::refine_tests::map_condition_tests::retired_known_function_shared_capture_callable_view_and_container_element_boundary_tests` 回归测试并全部通过。
- [x] 加强 `unit_lower_container_element_tests` 中 `ContainerReplace` 边参数与目标块参数的一致性及存活 owner 传递断言。
- [x] 3 个 public-entry conformance 用例（`t05_map_loop_continue`、`u02_list_add_loop_string`、`w11_word_count`）在 single 与 project 双入口下全部通过。
- [x] `python3 scripts/gen_roadmap.py`、`python3 scripts/gen_spec_dag.py`、`python3 scripts/check_docs.py`、`python3 scripts/check_rust_sizes.py --base main`、`cargo fmt --all -- --check` 与 `cargo clippy --locked --workspace --all-targets -- -D warnings` 全部通过。

## 4. 验收记录账本

| 检查项 | 状态 | 证明/结论 |
|---|---|---|
| 旧版 11 项 native 失败恢复 | 通过 | 针对旧提交 `378f52f0` 的 11 个失败用例在当前基准下全部通过，详见 CI run 42147051337 |
| P1: 3 个 public-entry 用例双入口恢复 | 通过 | 定位根因为 `subset/range_borrow.rs` 的 `expr_requires_refine` 缺少被退役容器变异操作（`container_append`、`map_get`、`element_place` 等）的准入，导致 temporary 容器遍历未识别为 refine 而退回 legacy 触发 `BE:UnsupportedNode`；补齐后 `t05`、`u02`、`w11` 在 single/project 双入口下 100% 通过（`conformance_gate.py` pass_run=3/3，全量 138/154 ok，0 regression） |
| P1: 解决主线冲突 | 通过 | 已合入最新 `origin/main`（包含 PR #322 和 PR #324），完成图表与路线图同步生成，合并树代码编译通过 |
| P2: ContainerReplace 边参数与存活 owner 断言加强 | 通过 | 在 `unit_lower_container_element_tests.rs` 中断言 success/failure 边与目标块参数实体类型、数量一一匹配，并验证 carried_owner 为前驱存活实体 |
| P2: 混合函数回归覆盖 | 通过 | 在 `refine_subset_tests.rs` 新增 `mixed_temporary_container_and_map_with_closures_route_to_refine`，覆盖临时容器 size + 具名下标、临时 Map receiver、捕获闭包 + 容器写入等场景 |
| 保留 legacy 回退范围标注 | 明确 | 保留旧路径中尚未完全退役的 nullable-if / map provider 辅助代码（如 `unit_lower/control/nullable_if/heap.rs`、`unit_lower/loop_control/map_iteration.rs`），仅供未迁移遗留单元测试隔离使用，不向 public-entry 暴露 |
| 根因归因说明 | 确认 | 撤回审查者早期推测的“缺 Phi/def_map”与“view_inner 未覆盖”假设；实际根因为 refine subset 门控漏标退役操作导致回退遗留路径 |
