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

将 SPEC-0371/0373/0378/0380 已实现但尚未接入默认 `migrated` 门禁的家族（整数内建运算、整数区间值与升序 `for`、`String.clone()`、可信文本原语、扩展函数调用、`Borrow Rc<T>` 参数、`IntRange?`/`LongRange?` 可空存储与判空、`Map.withValue`/`Map.requireValue`）全量切换至 `ssa::refine` 默认路由，补齐默认路由暴露的扩展函数 `this` 借用、`borrow val` 下 `value class` 字段投影、多跳短路 `Abort` 路径活跃 owner 传播与 `if` 汇聚块拓扑编号，并彻底删除已 100% 由细化层接管的 5 个旧 `unit_lower` 模块（`integer.rs`、`integer_range.rs`、`text_primitive.rs`、`map_with.rs`、`map_require.rs`，共 755 行）。

## 2. 实施内容

1. **全量默认路由收敛（`refine/subset.rs`、`subset/scalar_text.rs`、`subset/callable.rs`、`subset/range_borrow.rs`、`subset/heap.rs`、`subset/field.rs`、`subset/value.rs`、`subset/map.rs`）**：
   - **默认 `migrated` 激活**：在 `integer_operation`、`integer_range_operation`、`string_operation`（`String.clone()`）、`numeric_for`、`signature_types` / `local_variable` 的 `range_type`、以及 `call` 中的 `is_extension_call` 与 `descriptor.text_primitive().is_some()` 处激活 `self.migrated |= ok`；在 `expr_requires_refine` 中纳入 `map_with_value` 与 `map_require_value`。
   - **`Borrow Rc<T>` 参数与 `IntRange?`/`LongRange?` 可空支持**：`ParameterMode::Borrow` 接纳 `rc_payload(ty).is_some()`；`heap.rs`（`UnitTypeKind::Nullable`）、`subset.rs`（`condition`）、`value.rs`（`value_projection`）与 `map.rs`（`map_remove`、`nullable_equality`）全面纳入 `range_type`。
   - **扩展函数 `Expression::This` 借用与括号解包**：`subset/callable.rs`、`build/callable.rs` 与 `build/range_borrow.rs` 在 `borrowed_string_name`、`borrowed_move_only_name` 及对应 loan 提取中同时支持 `Expression::Name | Expression::This`，并对操作数、接收者与窄化视图做 `ungroup` 解包。
   - **`borrow val` / 借用返回的 `value class` 字段投影**：`subset/range_borrow.rs` 的 `result_place` 递归支持 `class` 与 `value class`（含 `move_value_class`）字段投影链，打通 `borrow val` 绑定在 `Map` 借用槽位与 `value class` 字段上的组合读取与有序 `BorrowEnd`。
2. **多跳 `Abort` 活跃性传播与 `if` 汇聚块拓扑序修复（`refine/emit.rs`、`refine/build/condition.rs`）**：
   - **多跳 `Abort` 块活跃性传播**（`refine/emit.rs`）：计算必然到达 `RTerm::Abort` 的块集合 `abort_blocks`，并在 `shape` 不动点迭代中将前驱块的出口存活集合（`live_in[pred] - retired[pred]`）经 `abort_preds` 单调传播至多跳 `Abort` 路径（如 `if (A || B) { error(...) }` 短路汇聚的 `when_true` 块），确保在 `Abort` 边上所有存活的 MoveOnly owner 都具有合法的块参数归属，消除 `MissingOwnedExit`。
   - **`if` 汇聚块延迟分配**（`refine/build/condition.rs`）：在 `if_expression` 中将 `join` 块分配推迟至条件与分支块构建完成之后，保证 `merge_straight_lines` 折叠单前驱块后 `join` 的 `BlockRef` 索引始终大于条件及分支块，维持线性扫描借用结束点的拓扑顺序。
3. **旧路径模块退休（共删除 5 个模块、755 行）**：
   - 删除 `crates/lang-codegen/src/ssa/unit_lower/integer.rs`（143 行）、`integer_range.rs`（195 行）、`text_primitive.rs`（200 行）、`map_with.rs`（121 行）与 `map_require.rs`（96 行）。
   - `unit_lower/call.rs` 与 `unit_lower/map.rs` 对已退休家族统一收敛为结构化 `UnsupportedNode` 拒绝；`unit_lower/loop_control/numeric_iteration.rs` 内联处理旧路径 `map_for`（`P16-for`）测试所需的字面量区间源构造，彻底移除对 `integer_range.rs` 的依赖；`unit_lower/map_consume.rs` 仅保留 `lower_map_consume` 供遗留 `map_for` 与 `with_per_family_lowering` 测试使用。

## 3. 验收标准

- [x] `ssa::` 全量 1,854 项单元测试（1,819 项主套件 + 35 项 `text_copy_tests`，含 `integer_range_value_tests` 15/15、`checked_integer_conversion_backend_tests` 25/25、`borrow_storage_contract_tests`、`map_provider_exits_pipeline_tests` 与 `refine_tests`）100% 通过。
- [x] `llvm::` 全量 74 项单元测试 100% 通过。
- [x] 手写 Rust 文件尺寸护栏保持在 1000 行软上限内（`emit.rs` 975 行，`numeric_iteration.rs` 244 行）。
- [x] `python3 scripts/gen_roadmap.py`、`python3 scripts/gen_spec_dag.py`、`python3 scripts/check_docs.py`、`python3 scripts/check_rust_sizes.py --base main`、`cargo fmt --all -- --check` 与 `cargo clippy --locked --workspace --all-targets -- -D warnings` 全部通过。
