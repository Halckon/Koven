# SPEC-0382：Map 剩余单步操作、通用值类型与 Unit 根原语迁入细化层及旧路径退休

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的 `Map` 单步操作（`get`/`[]`/`remove`/`consume`/`m[k] = v` 与通用值类型 `V`）及 `Unit` 零大小根 `replace`/`swap` 迁入细化层并退休 `unit_lower/ownership_primitive.rs` 时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 与 [ADR-0033](../../adr/accepted/0033-inline-nullable-representation.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0382` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户确认基于最新 main 继续推进 E3 后续开发与旧路径退休 |
| 基线 | `main` `e89c3630` |
| 分支 | `feature/spec-0382-refine-map-ops-and-primitive-retirement` |
| 前置 | [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md)、[ADR-0033](../../adr/accepted/0033-inline-nullable-representation.md)、[SPEC-0381](0381-refine-default-routing-and-legacy-retirement.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

将 `Map<K, V>` / `MutableMap<K, V>` 的剩余单步操作（`get` / `m[k]`、`remove`、`consume`、`m[k] = v` 下标赋值及查询/移除结果的空值与值判等）与任意非递归 `storage_type` 值类型 `V` 迁入 `ssa::refine`，同时补齐零大小 `Unit` 局部可变根变量的 `replace(&root, rhs)` / `swap(&left, &right)` 及 RHS 控制转移退出，并从 `unit_lower` 彻底退休已 100% 由细化层接管的 `crates/lang-codegen/src/ssa/unit_lower/ownership_primitive.rs`（432 行）。

## 2. 实施内容

1. **`Map` 剩余单步操作与通用值类型 `V` 迁入 `ssa::refine`（`refine/subset/map.rs`、`refine/build/map.rs`、`refine/body.rs`、`refine/emit.rs`、`ssa/types.rs`、`unit_lower/type_lower.rs`）**：
   - **通用非递归值类型 `V`**：`map_kv_seen` 支持键 `K` 为 `String`/`Int`、值 `V` 为任意非递归 `storage_type`（含标量、`String`、`class`、`value class`、`enum class`、`Box<T>`、`Rc<T>`、`Array<T>`、`List<T>`、`MutableList<T>`、`Map<K2, V2>`、`MutableMap<K2, V2>`、`IntRange`/`LongRange` 及指针句柄可空类型）。
   - **下标赋值 `m[k] = v`**：在 `Expression::Assignment` 分发 `typed.map_put(self.id(target))` 至 `ROp::MapPut`，消费旧条目覆写或 MoveOnly 键值交付。
   - **`Map.get(k)` / `m[k]`、`MutableMap.remove(k)` 与 `MutableMap.consume()`**：新增 `ROp::MapGet { owner, key }`、`ROp::MapRemove { owner, key }`、`ROp::MapConsume { owner, map_type }` 与 `ROp::MapResultUnwrap { result }`；对 Copyable `V` 的查询/移除结果通过 `$map.result.tX` 的 `ROp::MapResultUnwrap` 解包，对 MoveOnly `V` 的移除结果复用可空证明视图或 `NullableTake`；支持 `m.get(k) == null`、`m.remove(k) != null`、`m[k] == value`、`removed == hit` 及值形式 `val present = ((removed)) != null`。
   - **跨函数 `$map.result.tX` 与内联可空 `value class` 兼容**：`Module::nullable_inner` 对所有 `$map.result.tX` 与 `$nullable.map.tX` 返回 `presence_payload_type(id)`，`is_inline_nullable_inner` 纳入 `NominalKind::ValueClass`。
   - **借用实参位置边界与 Span 对齐**：`subset/callable.rs` 保持对嵌套借用位置中直接借用 `Map.remove` 绑定（如 `array[index(removed)]`）的精确边界隔离；`refine/build/receiver.rs` 委托调用 `SharedHeapFieldLoan` 使用 `fact.begin_span()`，`refine/build/expression.rs` 具名 Borrow 实参 `RootPlace` 使用 `CallArgument.span`。
2. **`Unit` 零大小根原语补齐与 `unit_lower/ownership_primitive.rs` 退休（`refine/subset/inout.rs`、`refine/build/inout.rs`、`unit_lower/call.rs`）**：
   - **零大小 `Unit` 根原语与 RHS 控制流退出**：`subset/inout.rs` 允许 `BuiltinType::Unit` 局部可变根的 `replace(&left, make())`、`swap(&left, &right)` 以及 RHS 含 `return`/`break`/`continue` 的 `replace(&root, rhs)`；`build/inout.rs` 为 `Unit` 根惰性物化 `AggregateConstruct { fields: [] }` 槽位并在 RHS 求值期间将根独占借用纳入 `pending_call_loans`。
   - **退休 `unit_lower/ownership_primitive.rs`（432 行）**：删除 `crates/lang-codegen/src/ssa/unit_lower/ownership_primitive.rs`，`unit_lower/call.rs` 对残留 `ownership_primitive` 统一收敛为 `UnsupportedNode`。

## 3. 验收标准

- [x] `ssa::` 全量 1848 项单元测试（含 `unit_root_primitive_tests` 16/16、`map_provider_exits_pipeline_tests` 76/76、`owned_map_borrow_boundary_tests` 2/2、`refine_subset_tests` 7/7 与 `refine_tests` 56/56）100% 通过。
- [x] `python3 scripts/gen_roadmap.py`、`python3 scripts/gen_spec_dag.py`、`python3 scripts/check_docs.py` 与 `python3 scripts/check_rust_sizes.py --base main` 全部通过。
