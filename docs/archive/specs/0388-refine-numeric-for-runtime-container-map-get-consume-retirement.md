# SPEC-0388：数值区间 `for`、运行时长度容器构造、`Map.get`、非可空 `Map.consume` 与 `$map.result` 旧路径退役

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的数值区间 `for`（`unit_lower/loop_control/numeric_iteration.rs`）、运行时长度容器构造 `Array<T>(size, init)` / `List<T>(size, init)`（`unit_lower/container/runtime.rs`）、非可空 `MutableMap.consume()`（`unit_lower/map_consume.rs`）、`Map.get`（`unit_lower/map.rs`）与 `$map.result.tX` 查询结果桥接（`unit_lower/map_result.rs`）旧路径退役时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0388` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户确认在 SPEC-0387 基线上继续推进 E3 下一个切片 |
| 基线 | `3cba24b2`（SPEC-0387） |
| 分支 | `feature/spec-0388-refine-rc-numeric-for-runtime-container-retirement` |
| 前置 | [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md)、[SPEC-0373](../../specs/active/0373-refine-destructuring-range-for.md)、[SPEC-0376](../../specs/active/0376-refine-container-for-generate.md)、[SPEC-0382](../../archive/specs/0382-refine-map-ops-and-primitive-retirement.md)、[SPEC-0387](0387-refine-string-clone-box-value-field-when-retirement.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

在 [SPEC-0373](../../specs/active/0373-refine-destructuring-range-for.md)、[SPEC-0376](../../specs/active/0376-refine-container-for-generate.md) 与 [SPEC-0382](../../archive/specs/0382-refine-map-ops-and-primitive-retirement.md) 已将 `IntRange`/`LongRange` 数值 `for` 循环、运行时长度容器构造（`Array<T>(size, init)` / `List<T>(size, init)`）、`Map.get`、`MutableMap.consume` 及 `$map.result.tX` 操作迁入 `ssa::refine` 的基础上，补齐 `ssa::refine` 对发散（`Nothing`）实参与死代码语句块的准入和构建，并彻底退役 `unit_lower` 中已被细化层完全取代的 legacy 实现模块与分支（合计清理约 1,260 行旧路径代码）：

1. **补齐 `ssa::refine` 对发散操作数与块内提前终止语句的准入与构建**：
   - 在 `refine/subset.rs` 的 `Statement::Block / ControlBody / LambdaBody` 中，当块内遇到类型为 `Nothing` 的表达式语句（如 `return`、`break`、`continue` 或 abort 调用）时停止对后续不可达语句的预检并将外层函数标记为 `migrated = true`（与 `refine/build.rs` 遇到 `Lowered::Diverged` 立即终止块构建保持严格一致）；
   - 在 `refine/subset/container.rs` 与 `refine/build/container.rs` 的 `ContainerConstructionKind::RuntimeLength` 中，支持 `size` 或 `initializer` 为 `Nothing` 类型表达式（如 `Array<Int>(error("size"), { ... })`、`List<Int>(1, error("initializer"))`），当 `size` 发散时不要求不可达的 `initializer` 形成 `callable_origin` 事实，并从 `BuiltinType::Int` 解析长度类型以在借用读取前正常发散；
   - 在 `refine/subset/map.rs` 的 `map_contains`、`map_get`、`map_remove` 与 `map_put` 中支持类型为 `Nothing` 的 `key` / `value` 实参（如 `m.get(return)`、`m.put(if (flag) { return } else { return }, 8)`）。
2. **退役 `unit_lower/loop_control/numeric_iteration.rs` 与 `unit_lower/container/runtime.rs`**：
   - 删除 `crates/lang-codegen/src/ssa/unit_lower/loop_control/numeric_iteration.rs`（529 行），从 `loop_control/iteration.rs` 与 `loop_control/iteration_cleanup.rs` 中移除 `IterationState::Numeric` 分支，并在 `loop_control/iteration.rs` 对 `IterationProvider::NumericRange(_)` 显式返回 `UnsupportedNode`；
   - 删除 `crates/lang-codegen/src/ssa/unit_lower/container/runtime.rs`（242 行），并在 `unit_lower/container.rs` 对 `ContainerConstructionKind::RuntimeLength` 显式返回 `UnsupportedNode`。
3. **退役 `unit_lower/map_result.rs`、非可空 `lower_map_consume` 与 `lower_map_get`**：
   - 删除 `crates/lang-codegen/src/ssa/unit_lower/map_result.rs`（173 行），移除 `unit_lower.rs`、`unit_lower/call.rs`、`unit_lower/borrow.rs` 与 `unit_lower/scalar.rs` 中的 `wrap_map_result`、`unwrap_map_result`、`is_map_result_value` 与 `lower_map_result_binary` 分支（约 70 行）；
   - 移除 `unit_lower/map_consume.rs` 中的非可空 `lower_map_consume` 与 `reject_map_consume_alias`（147 行，仅保留 `pub(super) mod nullable;`），移除 `unit_lower/map.rs` 中的 `lower_map_get`（45 行），并在 `unit_lower/map.rs` 对 `map_consume` 与 `map_get` 显式返回 `UnsupportedNode`。

## 2. 边界与不变量

- `unit_lower/map_consume/nullable.rs` 仍保留，供 `unit_nullable_map_consume_guards` 与 `nullable_map_consume_lowering_tests` 的单点故障注入测试使用；`unit_lower/map.rs` 中的 `lower_map_construction`、`lower_map_size`、`lower_map_contains`、`lower_map_put` 与 `lower_map_remove` 仍保留，供 `map_iteration` 循环守卫与 `with_per_family_lowering` 测试使用。
- 所有合法 `IntRange`/`LongRange` `for` 循环、`Array<T>(size, init)` / `List<T>(size, init)`、`Map.get`、非可空 `MutableMap.consume()` 与 `$map.result.tX` 比较/解包经默认入口路由至 `ssa::refine` lowering 并通过 SSA verifier 与 LLVM IR 生成；通过 `with_per_family_lowering` 强制进入 `unit_lower` 时，已退役路径确定性返回 `UnsupportedNode`。

## 3. 验收标准

- [x] 补齐 `crates/lang-codegen/src/ssa/refine/subset.rs`、`refine/subset/{container.rs,map.rs}` 与 `refine/build/container.rs`，删除 `unit_lower/loop_control/numeric_iteration.rs`、`unit_lower/container/runtime.rs` 与 `unit_lower/map_result.rs`，并精简 `unit_lower/{loop_control.rs,container.rs,map_consume.rs,map.rs,call.rs,borrow.rs,scalar.rs}`、`unit_lower/loop_control/{iteration.rs,iteration_cleanup.rs}` 与 `unit_lower.rs`（净减约 1,260 行 legacy 代码）。
- [x] 新增 `ssa::refine_subset_tests::numeric_for_runtime_container_and_map_diverging_keys_route_to_refine` 与 `ssa::refine_tests::map_condition_tests::retired_numeric_for_runtime_container_map_get_and_consume_boundary_tests` 回归测试并全部通过。
- [x] `python3 scripts/gen_roadmap.py`、`python3 scripts/gen_spec_dag.py`、`python3 scripts/check_docs.py`、`python3 scripts/check_rust_sizes.py --base main`、`cargo fmt --all -- --check` 与 `cargo clippy --locked --workspace --all-targets -- -D warnings` 全部通过。

## 5. 验收记录账本

| 验收项 | 结果 | 证据 |
|---|---|---|
| 3 个退役项已从版本库移除 | 通过 | 逐个确认：3/3 个路径/符号不再存在 |
| 内容并入声明的合并目标 | 通过 | 3/3 个目标在当前树存在 |
| 修改文件均小于 1000 行 | 通过 | `python3 scripts/check_rust_sizes.py --base f78c8c20` 退出 0 |
| 手写 Rust 文件数净减 | 受限通过 | 历史已 squash 为 `430a0de2`，无法分离单 Spec 计数 |
| `scripts/gen_spec_dag.py` / `scripts/check_docs.py` 通过 | 通过 | 归档后重跑生成物稳定；`documentation check passed` |
| `cargo test -p lang-codegen --lib` | 通过 | `cargo test -p lang-codegen --lib -- route_to_refine retired_` = `19 passed / 0 failed`（含本批四份 Spec 的具名回归测试） |

核对说明：

- 确认删除：`unit_lower/loop_control/numeric_iteration.rs`、`unit_lower/container/runtime.rs`、`unit_lower/map_result.rs`
- 具名测试通过：`numeric_for_runtime_container_and_map_diverging_keys_route_to_refine`、`retired_numeric_for_runtime_container_map_get_and_consume_boundary_tests`
- 验收标准中的复选框已全部勾选，本账本复核其可验证部分；历史已 squash，无法把改动单独归因到本 Spec 的提交。
