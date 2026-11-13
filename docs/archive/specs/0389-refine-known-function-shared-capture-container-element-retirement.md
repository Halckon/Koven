# SPEC-0389：具名函数引用、共享闭包捕获、可调用视图借用与顺序容器元素操作旧路径退役

> **性质**：实施合同 · **状态**：done · **读取时机**：评审或实施路线图 E3 的具名函数引用（`unit_lower/closure/known_function.rs`）、共享闭包捕获（`unit_lower/closure/shared.rs`）、可调用视图实参借用（`unit_lower/callable_view.rs`）及顺序容器元素下标读取/借用/替换（`unit_lower/container.rs`、`unit_lower/assignment.rs`、`unit_lower/borrow.rs`）旧路径退役时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 与 [ADR-0034](../../adr/accepted/0034-refine-closures-and-callable-views.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0389` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户确认在 SPEC-0388 基线上继续推进 E3 下一个切片 |
| 基线 | `598b054a`（SPEC-0388） |
| 分支 | `feature/spec-0389-refine-container-element-closure-range-retirement`；收尾修复 `fix/spec-0389-compound-assignment-snapshot-order` |
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
- [x] 加强 `unit_lower_container_element_tests` 中 `ContainerReplace` 逐边绑定 carried owner 与前驱实参，验证身份一致、未被提前消费，并增加非法/已消费 owner 的反例验证（排除内存泄漏伪证）。
- [x] 3 个 public-entry conformance 用例（`t05_map_loop_continue`、`u02_list_add_loop_string`、`w11_word_count`）在 single 与 project 双入口下全部通过。
- [x] 补充路由风险 1 对照测试：增加“有／无无关 append”对照，覆盖 while、顺序迭代、borrow-return、carrier-return 与 Map remove，断言接受/拒绝一致且 owner/drop 不受扰动。
- [x] 补充路由风险 2 对照测试：在 single/project 双入口下比较加入无关 `nullable-when` 前后，目标函数的诊断、SSA blocks、instructions、drop 次数与 LLVM IR 输出完全一致，并提供兼容性理由。
- [x] 补充混合场景单次求值与释放点断言（临时 List `.size`、Map `.get` 及 String 借用捕获闭包），并核实 Char 键 Map 已有覆盖现状。
- [x] 元素目标复合赋值（`c[i] op= rhs`）先定位元素槽位并完成边界检查，再求值 RHS：RHS 不得在越界 abort 前产生副作用，且必须读取 RHS 求值前的元素值；变量目标 `x op= rhs` 的既有快照行为保持不变。新增 `ssa::refine_subset_tests::element_compound_assignment_reads_element_before_rhs_and_checks_bounds` 与 `ssa::refine_subset_tests::compound_assignment_snapshots_lhs_before_rhs` 回归测试并全部通过。
- [x] `python3 scripts/gen_roadmap.py`、`python3 scripts/gen_spec_dag.py`、`python3 scripts/check_docs.py`、`python3 scripts/check_rust_sizes.py --base main`、`cargo fmt --all -- --check` 与 `cargo clippy --locked --workspace --all-targets -- -D warnings` 全部通过。

## 4. 验收记录账本

| 检查项 | 状态 | 证明/结论 |
|---|---|---|
| 旧版 11 项 native 失败恢复 | 通过 | 针对旧提交 `378f52f0` 的 11 个失败用例在提交 `390fc42a`（主线同步合并）下在双平台 CI 中逐项通过，详见 CI Run 42147051337 |
| P1: 3 个 public-entry 用例双入口恢复 | 通过 | 提交 `15f63c3e` 定位根因为 `subset/range_borrow.rs` 的 `expr_requires_refine` 缺少退役容器变异操作（`container_append`、`map_get`、`element_place`）准入，导致 temporary 容器遍历未识别为 refine 而退回 legacy 触发 `BE:UnsupportedNode`；补齐后 `t05`、`u02`、`w11` 在 single/project 双入口下 100% 通过（CI Run 42193560190 全绿，21 checks all passed） |
| P1: 独立归因与假设撤回 | 确认 | 明确区分提交 `390fc42a`（修复旧 11 项）与提交 `15f63c3e`（修复新 3 项），撤回审查者早期推测的“缺 Phi/def_map”与“view_inner 未覆盖”假设；实际根因为 refine subset 门控漏标退役操作导致回退遗留路径 |
| 路由风险 1: 无关 append 对照验证 | 通过 | 在 `refine_subset_tests.rs` 增加 `container_append_admission_does_not_perturb_or_miscompile_sibling_functions`，覆盖 while 循环、顺序迭代、borrow-return、carrier-return 与 Map remove，核验加入无关 append 前后接受/拒绝结果、参数数量、返回类型及 drop 数量严格一致，SSA verifier 全部通过 |
| 路由风险 2: nullable-when 干扰验证 | 通过 | 在 `unit_lower_nullable_neighbor_tests.rs` 增加 `unrelated_nullable_when_does_not_perturb_nullable_if_in_single_and_project_pipelines`，在 single/project 双入口下比较加入无关 `nullable-when` 前后目标函数 `entry`，验证 blocks/instructions/drops/LLVM 完全一致。现代 refine 已完全接管有效 nullable-if，遗留 `prepare_heap_nullable_if_entry` 的条件仅在未走 refine 的兜底回退路径中生效，不干扰主流程 |
| P2: ContainerReplace 逐边绑定与反例断言 | 通过 | 在 `unit_lower_container_element_tests.rs` 中按参数索引精确定位 carried owner，断言 `success.arguments[idx] == failure.arguments[idx]` 且前驱块未提前 drop/consume 该 owner；构造篡改/非法 owner 反例证实 SSA verifier 严格拒绝，彻底排除内存泄漏伪证 |
| P2: 混合场景求值与释放点断言 | 通过 | 在 `refine_subset_tests.rs` 新增 `temporary_container_size_and_map_get_single_eval_and_drop_points`（断言临时 List `.size` 与 Map `.get` 释放点与 drop 次数精准）及 `string_borrow_capture_closure_preserves_outer_owner`（断言闭包借用捕获外层 String 存活且正常释放） |
| Char 键 Map 覆盖现状核查 | 明确 | Koven 中 `LiteralKind::Char` 在 expression body 中为未实现语法（显式 `UnsupportedNode`），Char 键仅支持通过 `const val` 常量形式使用，且已在 native 矩阵（`native_map_constant_operand_tests` 等）中完整覆盖，无需在 refine 单元层引入冗余用例 |
| 保留 legacy 回退范围明确 | 明确 | 明确标注仅保留：1) `map_for` 的 `scope_requires_refine` 保护；2) `unit_lower/control/nullable_if` 的兜底回退（仅供未迁移遗留单元测试隔离使用，不向 public-entry 暴露） |
| 元素目标复合赋值求值顺序收尾 | 通过 | 收尾核查确认变量目标 `x op= rhs` 在原实现下已正确（`bindings` 取出的是槽位值拷贝，RHS 内重新绑定不回写已捕获的旧槽位），仅元素目标 `c[i] op= rhs` 存在缺陷：原实现先求值 RHS，导致越界边界检查晚于 RHS 副作用、且读到 RHS 修改后的元素值。`refine/build/assignment.rs` 将 `ContainerElementPlace` 与 `ReadPlace` 前移至 RHS 求值之前，并把 owner 拆为 RHS 之前的 `initial_owner`（定位元素）与 RHS 之后的 `write_owner`（写回，RHS 可能重新绑定容器变量）。验证：`cargo test -p lang-codegen --lib ssa::refine` 115 项通过；`cargo clippy --locked --workspace --all-targets -- -D warnings` 与 `cargo fmt --all -- --check` 通过。`cargo test -p lang-codegen` 全量在本地 50 分钟内未跑完，由 CI Run 42383578790 完成全量验收：`Targeted Tests` 在 macos-15（18m5s）与 ubuntu-26.04（11m1s）双平台通过，`Workspace Check & Clippy`、`Rust Code Formatting`、`Docs & Spec Structural Gate`、`Rust Physical Line Guard` 与 `Preview Producer` / `Preview Independent Consumer` 全部 pass，PR #347 状态 CLEAN |
