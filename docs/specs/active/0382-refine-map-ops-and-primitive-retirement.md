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

将 `Map<K, V>` / `MutableMap<K, V>` 的剩余单步操作（`get` / `m[k]`、`remove`、`consume`、`m[k] = v` 下标赋值及查询/移除结果的空值与值判等）、Map 迭代与 `requireValue`、RangeView carrier 借用运输、借用返回（`borrow-return`）、提前退出资源清理与任意非递归 `storage_type` 值类型 `V` 迁入 `ssa::refine`，同时补齐零大小 `Unit` 局部可变根变量的 `replace(&root, rhs)` / `swap(&left, &right)` 及 RHS 控制转移退出，并从 `unit_lower` 彻底退休已 100% 由细化层接管的 `crates/lang-codegen/src/ssa/unit_lower/ownership_primitive.rs`（431 行）。

## 2. 实施内容

1. **`Map` 剩余单步操作、遍历与通用值类型 `V` 迁入 `ssa::refine`（`refine/subset/map.rs`、`refine/build/map.rs`、`refine/body.rs`、`refine/emit.rs`、`ssa/types.rs`、`unit_lower/type_lower.rs`）**：
   - **通用非递归值类型 `V`**：`map_kv_seen` 支持键 `K` 为 `String`/`Int`、值 `V` 为任意非递归 `storage_type`（含标量、`String`、`class`、`value class`、`enum class`、`Box<T>`、`Rc<T>`、`Array<T>`、`List<T>`、`MutableList<T>`、`Map<K2, V2>`、`MutableMap<K2, V2>`、`IntRange`/`LongRange` 及指针句柄可空类型）。
   - **下标赋值 `m[k] = v`**：在 `Expression::Assignment` 分发 `typed.map_put(self.id(target))` 至 `ROp::MapPut`，消费旧条目覆写或 MoveOnly 键值交付。
   - **`Map.get(k)` / `m[k]`、`MutableMap.remove(k)` 与 `MutableMap.consume()`**：新增 `ROp::MapGet { owner, key }`、`ROp::MapRemove { owner, key }`、`ROp::MapConsume { owner, map_type }` 与 `ROp::MapResultUnwrap { result }`；对 Copyable `V` 的查询/移除结果通过 `$map.result.tX` 的 `ROp::MapResultUnwrap` 解包，对 MoveOnly `V` 的移除结果复用可空证明视图或 `NullableTake`；支持 `m.get(k) == null`、`m.remove(k) != null`、`m[k] == value`、`removed == hit` 及值形式 `val present = ((removed)) != null`。
   - **Map 迭代与 `requireValue` / RangeView carrier**：迁入 `for ((k, v) in m)` 遍历、`m.keys()` / `m.values()` 及 `m.requireValue(k)`，通过 RangeView carrier 安全运输借用；支持具名与临时容器上的原地借用。
   - **借用返回（`borrow-return`）与控制转移退出清理**：支持借用函数返回（`borrow T from root`）与委托字段借用；并在提前退出（`return`、`break`、`continue`）与短路分支（`&&`、`||`）中协调 `entry_dead` 状态运输与 `BorrowEnd`，确保资源恰好析构一次。
   - **跨函数 `$map.result.tX` 与内联可空 `value class` 兼容**：`Module::nullable_inner` 对所有 `$map.result.tX` 与 `$nullable.map.tX` 返回 `presence_payload_type(id)`，`is_inline_nullable_inner` 纳入 `NominalKind::ValueClass`。
   - **借用实参位置边界与 Span 对齐**：`subset/callable.rs` 保持对嵌套借用位置中直接借用 `Map.remove` 绑定（如 `array[index(removed)]`）的精确边界隔离；`refine/build/receiver.rs` 委托调用 `SharedHeapFieldLoan` 使用 `fact.begin_span()`，`refine/build/expression.rs` 具名 Borrow 实参 `RootPlace` 使用 `CallArgument.span`。
2. **`Unit` 零大小根原语补齐与 `unit_lower/ownership_primitive.rs` 退休（`refine/subset/inout.rs`、`refine/build/inout.rs`、`unit_lower/call.rs`）**：
   - **零大小 `Unit` 根原语与 RHS 控制流退出**：`subset/inout.rs` 允许 `BuiltinType::Unit` 局部可变根的 `replace(&left, make())`、`swap(&left, &right)` 以及 RHS 含 `return`/`break`/`continue` 的 `replace(&root, rhs)`；`build/inout.rs` 为 `Unit` 根惰性物化 `AggregateConstruct { fields: [] }` 槽位并在 RHS 求值期间将根独占借用纳入 `pending_call_loans`。
   - **退休 `unit_lower/ownership_primitive.rs`（431 行）**：删除 `crates/lang-codegen/src/ssa/unit_lower/ownership_primitive.rs`，`unit_lower/call.rs` 对残留 `ownership_primitive` 统一收敛为结构化 `UnsupportedNode` 诊断。
3. **细化分流门禁与校验边界保证（`orchestrate.rs` 与 `validate_bindings`）**：
   - **分流门禁精确化**：`unit_lower/orchestrate.rs` 中仅在编译单元含有 `field_replacements` 或残留 `ownership_primitives` 时才独占走 refine 降级或静态拒绝；对于仅包含常规 `inout` 参数的单元，保留其通过 legacy 路径校验的能力，不以粗粒度 `Inout` 跳过旧路径合法性检查。
   - **结束点不变量层级**：旧路径由 `borrow_result::validate_bindings` 在 `AfterStatement` 边界强制校验表达式内部非法的借用重叠与控制流边界；细化层则由静态 SSA 细化分析器在控制流图上直接维护 `entry_dead` 与 `BorrowEnd`，两层各司其职且语义等价。

## 3. 验收标准

- [x] `ssa::` 全量单元测试（含 `unit_root_primitive_tests`、`map_provider_exits_pipeline_tests`、`owned_map_borrow_boundary_tests`、`refine_subset_tests`、`refine_tests` 及新增 `map_condition_tests`）100% 通过。
- [x] 审查建议三组定向回归闭环（`crates/lang-codegen/src/ssa/refine_tests/map_condition_tests.rs`）：
  - **P2-1 MoveOnly Map 结果直接用于条件与分支退出**（`move_only_map_remove_direct_conditions_and_branch_exits`）：覆盖 `remove` 直接参与 `== null` 与 `!= null`、短路求值（未命中短路不影响后续条目）、函数双分支提前返回（`return`）、循环中断（`break`/`continue`）与具名绑定通过 proof view 借用字段；断言 native 运行 stdout 精确匹配、drop 恰好一次、退出码 0，并确认撤销 `&& !self.move_only(inner)` 后在 borrow 处立即触发 `UnsupportedNode` 失败。
  - **P2-2 Refine 与 Legacy 校验门禁边界矩阵**（`refine_and_legacy_validation_gate_matrix_tests`）：覆盖仅含 `inout` 的单元走 legacy 路径正常执行、`borrow val` 作用域内非法 `inout` 确切在短路表达式 span 处被旧 `validate_bindings` 以 `UnsupportedNode` 拒绝、包含字段替换的单元成功进入 refine 路径，以及同一单元混合 refine 函数与 legacy helper 函数的协同降级与执行。
  - **P2-3 旧 Ownership Primitive 退休边界与 RHS 控制转移**（`retired_ownership_primitive_boundary_tests`）：强制旧降级器对含 `replace` 节点的单元在 `call.rs:109` 处返回 `UnsupportedNode` 拒绝；正向验证 refine 路径下 RHS 含 `return` 的 `replace(&a, if (flag) return 42 else ...)` 正确析构并返回。
- [x] 手写 Rust 文件尺寸护栏保持在 1000 行软上限内（`refine_tests.rs` 981 行，`map_condition_tests.rs` 343 行）。
- [x] `python3 scripts/gen_roadmap.py`、`python3 scripts/gen_spec_dag.py`、`python3 scripts/check_docs.py` 与 `python3 scripts/check_rust_sizes.py --base main` 全部通过。
