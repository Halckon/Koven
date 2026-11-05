# SPEC-0383：最终默认路由收敛与旧路径退休收尾

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的最终默认路由收敛（整数运算、整数区间、文本原语、扩展函数、`Borrow Rc<T>`、`IntRange?`/`LongRange?`、`Map.withValue`/`Map.requireValue`、`value class` 字段借用返回、多跳 `Abort` 活跃性传播）与旧路径模块退休（删除 `integer.rs`、`integer_range.rs`、`text_primitive.rs`、`map_with.rs`、`map_require.rs`）时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 与 [ADR-0033](../../adr/accepted/0033-inline-nullable-representation.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0383` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户确认在最新 `main` 基线上继续推进 E3 最终旧路径退休评估与收尾阶段开发 |
| 基线 | `main` `041e8238` |
| 分支 | `feature/spec-0383-refine-final-legacy-retirement` |
| 前置 | [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md)、[ADR-0033](../../adr/accepted/0033-inline-nullable-representation.md)、[SPEC-0381](0381-refine-default-routing-and-legacy-retirement.md)、[SPEC-0382](../../archive/specs/0382-refine-map-ops-and-primitive-retirement.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

将 SPEC-0371/0373/0378/0380 已实现但尚未接入默认 `migrated` 门禁的家族（整数内建运算、整数区间值与升序 `for`、`String.clone()`、可信文本原语、扩展函数调用、`Borrow Rc<T>` 参数、`IntRange?`/`LongRange?` 可空存储与判空、`Map.withValue`/`Map.requireValue`）全量切换至 `ssa::refine` 默认路由，补齐默认路由暴露的扩展函数 `this` 借用、`borrow val` 下 `value class` 字段投影、多跳短路 `Abort` 路径活跃 owner 传播与 `if` 汇聚块拓扑编号，并彻底删除已 100% 由细化层接管的 5 个旧 `unit_lower` 模块（`integer.rs` 100 行、`integer_range.rs` 74 行、`text_primitive.rs` 118 行、`map_with.rs` 108 行、`map_require.rs` 84 行，基线共 484 行手写代码；commit 45edf6b 整体 diff 为 -739 行）。

## 2. 实施内容

1. **全量默认路由收敛（`refine/subset.rs`、`subset/scalar_text.rs`、`subset/callable.rs`、`subset/range_borrow.rs`、`subset/heap.rs`、`subset/field.rs`、`subset/value.rs`、`subset/map.rs`）**：
   - **默认 `migrated` 激活**：在 `integer_operation`、`integer_range_operation`、`string_operation`（`String.clone()`）、`numeric_for`、`signature_types` / `local_variable` 的 `range_type`、以及 `call` 中的 `is_extension_call` 与 `descriptor.text_primitive().is_some()` 处激活 `self.migrated |= ok`；在 `expr_requires_refine` 中纳入 `map_with_value` 与 `map_require_value`。
   - **`numeric_for` 所有权与访问契约**：严格校验 `plan.source_access() == UnitIterationSourceAccess::Owned` 与 `plan.source_kind() == OwnedSnapshot(source_id)`，维持与原有所有权检查器的一致性契约。
   - **`Borrow Rc<T>` 参数与 `IntRange?`/`LongRange?` 可空支持**：`ParameterMode::Borrow` 接纳 `rc_payload(ty).is_some()`；`heap.rs`（`UnitTypeKind::Nullable`）、`subset.rs`（`condition`）、`value.rs`（`value_projection`）与 `map.rs`（`map_remove`、`nullable_equality`）全面纳入 `range_type`。
   - **扩展函数 `Expression::This` 借用与括号解包**：`subset/callable.rs`、`build/callable.rs` 与 `build/range_borrow.rs` 在 `borrowed_string_name`、`borrowed_move_only_name` 及对应 loan 提取中同时支持 `Expression::Name | Expression::This`，并对操作数、接收者与窄化视图做 `ungroup` 解包。
   - **`borrow val` / 借用返回的 `value class` 字段投影**：`subset/range_borrow.rs` 的 `result_place` 递归支持 `class` 与 `value class`（含 `move_value_class`）字段投影链，打通 `borrow val` 绑定在 `Map` 借用槽位与 `value class` 字段上的组合读取与有序 `BorrowEnd`。
2. **多跳 `Abort` 活跃性传播与调用/突变族消费统一（`refine/emit.rs`、`refine/body.rs`、`refine/build/condition.rs`）**：
   - **多跳 `Abort` 块活跃性传播**（`refine/emit.rs`）：计算必然到达 `RTerm::Abort` 的块集合 `abort_blocks`，并在 `shape` 不动点迭代中将前驱块的出口存活集合（`live_in[pred] - retired[pred]`）经 `abort_preds` 单调传播至多跳 `Abort` 路径（如 `if (A || B) { error(...) }` 短路汇聚的 `when_true` 块），确保在 `Abort` 边上所有存活的 MoveOnly owner 都具有合法的块参数归属，消除 `MissingOwnedExit`。
   - **调用族与突变族退役槽位统一计算**（`refine/body.rs`）：在 `ROp::retired_slots` 中统一抽取调用族（`DirectCall`、`CallableInvoke`、`CallableInvokeView`、`BorrowCall`、`RangeCall`）的 receiver 与按值参数，以及 `LoanMutate` 等载荷值槽位，避免未登记退役导致已消费或已交付 owner 错误向双 abort 块续传；补充覆盖视图函数调用、本地 lambda 调用、借用返回调用及 inout 突变后双分支 abort 的 4 项定向回归。
   - **`if` 汇聚块延迟分配**（`refine/build/condition.rs`）：在 `if_expression` 中将 `join` 块分配推迟至条件与分支块构建完成之后，保证 `merge_straight_lines` 折叠单前驱块后 `join` 的 `BlockRef` 索引始终大于条件及分支块，维持线性扫描借用结束点的拓扑顺序。
3. **旧路径模块退休与边界保留（共删除 5 个具名模块、484 行手写代码）**：
   - 删除 `crates/lang-codegen/src/ssa/unit_lower/integer.rs`（100 行）、`integer_range.rs`（74 行）、`text_primitive.rs`（118 行）、`map_with.rs`（108 行）与 `map_require.rs`（84 行），基线 `041e8238` 上合计 484 行手写代码。
   - `unit_lower/call.rs` 与 `unit_lower/map.rs` 对已退休家族统一收敛为结构化 `UnsupportedNode` 拒绝。
   - **边界说明**：本 Spec 聚焦于收敛上述 5 个具名语法家族并退休对应 legacy 模块；`map_for` 迭代暂未迁入 `refine`，`map_consume.rs`（保留 `lower_map_consume`）与 `numeric_iteration.rs`（529 行，基线 495 行，通过内联区间源构造自聚解耦已删的 `integer_range.rs`）明确保留供遗留 `map_for` 及其独立测试使用，后续将在专项目标中独立推进迁移与旧循环模块退役。

## 3. 验收标准

- [x] `ssa::` 全量单元测试（包含新增的 7 项 abort 前驱退役定向回归及 4 类调用族 SSA 指令与块参数干净性断言）100% 通过。
- [x] `llvm::` 全量 74 项单元测试 100% 通过。
- [x] 手写 Rust 文件尺寸护栏保持在 1000 行软上限内（`emit.rs` 975 行，`control_flow_tests.rs` 946 行，`numeric_iteration.rs` 529 行）。
- [x] `python3 scripts/gen_roadmap.py`、`python3 scripts/gen_spec_dag.py`、`python3 scripts/check_docs.py`、`python3 scripts/check_rust_sizes.py --base main`、`cargo fmt --all -- --check` 与 `cargo clippy --locked --workspace --all-targets -- -D warnings` 全部通过。

## 4. 验收记录账本

| 检查项 | 证据来源 | 结论 |
|---|---|---|
| CI run 41758961251 | GitHub Actions PR-event（head `68157824`） | 14 项 job 全部 success |
| CI run 41790617837 | GitHub Actions PR-event（head `7bc90dd9`） | 14 项 job 全部 success（Linux/macOS Targeted Tests、Preview Producer/Consumer 全绿） |
| CI run 41797160301 | GitHub Actions PR-event（head `638dd72d`） | 14 项 job 全部 success（Linux/macOS Targeted Tests、Preview Producer/Consumer 全绿） |
| 本地 abort 前驱定向回归 | `cargo test -p lang-codegen ssa::refine_tests::control_flow_tests::abort_predecessor_` | 7 项回归全绿（覆盖 DirectCall、聚合构造、CallableInvokeView、CallableInvoke、BorrowCall、RangeCall 与 LoanMutate），且 4 类调用族均显式断言目标 SSA 操作存在及 abort 块参数中绝无已消费 owner |
| 变异测试证据 | 本地注释 `call_operands` 中的 `CallableInvoke`/`BorrowCall`/`RangeCall` | `abort_predecessor_callable_invoke_consume_then_both_abort` 与 `abort_predecessor_borrow_call_consume_then_both_abort` 立即稳定失败报 `InvalidModel`；`assert_refine_function_op_and_abort_clean` 在操作降级或遗漏 `retired_slots` 时立即失败，证明回归具有动态鉴别力 |
| CI Evidence 状态 | `scripts/pr_ci_reuse.py` 执行日志 | `code-evidence` 因 PR 合并 parents 严格检验未找到匹配物理证据产物而按设计 skipped，不属于 CI 失败，且全部 14 项实体测试 job 均独立执行通过 |

## 5. Map-for 与整数家族的组合路由修复

本地候选基于 main `e1cde85f907553053c1d6ef699ea4ddd81d0a18e`。真实 CLI 对照中，
同一 Borrow `Map<Int, Int>` 的 `for` 内 `v + 1` 可 build/run；改为 `v.inv()`，或仅在
循环前绑定 `val range = 0..<3`，均在已退休的 legacy 家族入口报 `UnsupportedNode`。
原因是 `map_for` 的 `scope_requires_refine` 尚未识别整数内建与 Range operation facts，
导致整个函数未通过 Refine 预检而落回已不具备对应操作的路径。

修复仅在共享 scope predicate 中纳入这两类既有 typed facts，不恢复旧 lowering、不移除
Map-for 的 scope gate。各消费者的类型、来源、descriptor、ownership 与 cleanup 检查保持；
共享 predicate 也服务 sequential temporary/while、nullable Map 与借用返回等组合准入，
不能将本修改描述为只影响 Map-for。纯 Map-for 及无关 sibling 中整数操作的原分派保留。

新增 `native_map_provider_tests::scalar_routing_tests` 覆盖 Map/MutableMap 的 Borrow、Own
及 temporary 六种来源，分别组合 `inv`、`toLong`、循环前 Range，保留加法对照和
continue/break/return。SSA 查询定位实际 scan 函数，检查 provider 与目标 operation 共存、
两次输出确定性和 LLVM verifier；native 使用原 MemoryDocument 与 rooted Unit helper。
`refine_subset_tests` 新增两项 route 断言，包括循环后 operation 与 sibling 隔离。
所有新增源码只执行正常路径，不含故障注入、计数校准或预期 Abort。

| 检查 | 本地结果 |
|---|---|
| main CLI 三源对照 | 加法 build/run 0；inv 与 Range build 2 / UnsupportedNode |
| 修改前新增回归 | 七项 scalar 回归 UnsupportedNode；route 一红一绿；加法 native 暂因缺 Clang 未执行 |
| 修复后 `cargo test --locked --offline -p lang-codegen --lib map_for_` | 10/10；六来源的 SSA/LLVM 与 84 次双布局 native 全绿 |
| 既有相邻回归（逐 exact） | 9/9；原 Map 三路径/whole-source-set、Range 40 格、双布局 native、Refine 正负路由保留 |
| fmt、docs、尺寸、diff | 全部通过；无新超限文件或额度增长 |
| `cargo clippy --locked --offline -p lang-codegen --all-targets -- -D warnings` | 通过；默认 dev debug=2 |
| `cargo build --locked --offline -p lang-codegen -p lang-cli` | 通过；默认 dev debug=2 |
| 修复后相同 CLI 三源 | 加法、inv、Range 均 build/run 0，stdout 均为 `visited` |

本轮 Linux x86_64 使用 Rust 1.96.0、LLVM/Clang 21.1.8；工具隔离在工作区。
默认 debug=2 的 libtest 编译曾被 SIGKILL，后按现行 Linux CI 的 dev/test debug=1 与
单 Cargo 编译成功。此处不据此声称默认调试配置或 macOS/官方 CI 已通过。

额外探针发现 Map-for 内嵌 numeric-for 仍 `UnsupportedNode`（精确为外层 for 范围）。
主干也拒绝此源码，#300 前已有 `map_iteration/nested.rs` 对非 Map 子 provider 的拒绝；
本轮不将它当成 integer/Range-value 回退的已修复正例，也不扩大循环资格。原始探针失败
保留在本地验证记录；未修改任何既有回归来吸收该边界。
