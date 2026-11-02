# SPEC-0381：默认分流边界修复与旧路径退休

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的默认分流边界修复（乱序命名实参、Copyable 别名快照、短路逻辑分支 dead 集合隔离、一级字段 replace RHS 提前退出）与旧路径退休（删除 field_replace.rs 与 enum_type_test.rs）时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 与 [ADR-0033](../../adr/accepted/0033-inline-nullable-representation.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0381` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户确认在分支 feature/spec-0381-refine-default-routing-and-legacy-retirement 推进 E3 旧路径退休评估与收尾阶段开发 |
| 基线 | `main` `fa13bfbd` |
| 分支 | `feature/spec-0381-refine-default-routing-and-legacy-retirement` |
| 前置 | [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md)、[ADR-0033](../../adr/accepted/0033-inline-nullable-representation.md)、[SPEC-0380](0380-refine-deinit-nested-fields-and-box-payloads.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

推进 E3 旧路径退休评估与收尾，修复默认分流探针发现的 4 项 `ssa::refine` 边界问题，安全退休已被细化层覆盖的旧路径模块 `crates/lang-codegen/src/ssa/unit_lower/field_replace.rs` 与 `crates/lang-codegen/src/ssa/unit_lower/enum_type_test.rs`，并在 `unit_lower.rs` 与 `ownership_primitive.rs` 收敛为显式 `UnsupportedNode` 边界拒绝。

## 2. 实施内容

1. **默认分流探针边界修复（`refine/build/expression.rs`、`refine/build/condition.rs`、`refine/subset/inout.rs`、`refine/build/inout.rs`）**：
   - **命名实参乱序求值与槽位映射**（`build/expression.rs`）：按 `argument.argument_index()` 顺序求值，并按 `argument.parameter_index()` 填入目标参数槽位，修复如 `p.pair(right = p.make(), marker = 2, left = owner)` 等乱序命名实参下的 `InvalidSsa`。
   - **仅对 Copyable 绑定生成实参快照**（`build/expression.rs`）：`snapshot_binding_if_name` 限定在 `!self.move_only(ty)` 时生成 `ROp::Alias` 快照，避免 MoveOnly 绑定在条件分支移动时残留原槽位导致 `InvalidSsa`。
   - **短路 `&&`/`||` 分支 dead 集合隔离**（`build/condition.rs`）：在 `logical` 中隔离 skip 分支与 middle 分支的 `self.dead` 状态并同步 `entry_dead`，修复 `flag || p.consume(text)` 中右操作数移动 MoveOnly 绑定时的合流生命周期。
   - **一级字段 `replace(&holder.field, rhs)` 的 RHS 提前退出支持**（`subset/inout.rs`、`build/inout.rs`）：在求值 `rhs` 期间将字段独占借用推入 `pending_call_loans`，使 `rhs` 内的 `return`/`break`/`continue` 在析构父 owner 前自动发出 `BorrowEnd`，完成一级字段 replace 全路径向 `ssa::refine` 的收敛。
2. **旧路径模块退休与保留边界**：
   - **已退休删除（共 705 行）**：删除已被 `ssa::refine` 覆盖的 `crates/lang-codegen/src/ssa/unit_lower/field_replace.rs`（385 行）与 `crates/lang-codegen/src/ssa/unit_lower/enum_type_test.rs`（320 行），并在 `unit_lower.rs` 与 `ownership_primitive.rs` 中收敛为显式 `UnsupportedNode` 边界拒绝。
   - **保留边界**：
     1. borrow 返回函数（`borrow_result.rs`、`map_result.rs`）与 `RangeView`（`range.rs`）；
     2. Map 剩余未迁操作（`map.rs`、`map_borrow.rs`、`map_consume.rs`、`map_iteration.rs`、`map_require.rs`）；
     3. Borrow 模式的 `Rc<T>` 参数（`rc.rs`）与 Unit 零大小根原子 replace（`ownership_primitive.rs`）；
     4. 由 `with_per_family_lowering` 锁定的旧路径拒绝快照测试（`nullable_proof`、`while_proof`、`deinit_borrow`、`string_clone`）及纯标量 CFG 白盒结构测试。

## 3. 验收标准

- [x] `ssa::refine_`（64/64）全量通过。
- [x] `ssa::unit_`（409/409，含 `unit_field_replace_tests` 13/13 与 `unit_field_borrow_tests`）全量通过。
- [x] `python3 scripts/gen_roadmap.py`、`python3 scripts/gen_spec_dag.py`、`python3 scripts/check_docs.py` 与 `python3 scripts/check_rust_sizes.py --base main` 全部通过。
