# SPEC-0382：Map 剩余单步操作、通用值类型与 Unit 根原语迁入细化层及旧路径退休

> **性质**：实施合同 · **状态**：done · **读取时机**：评审或实施路线图 E3 的 `Map` 单步操作（`get`/`[]`/`remove`/`consume`/`m[k] = v` 与通用值类型 `V`）及 `Unit` 零大小根 `replace`/`swap` 迁入细化层并退休 `unit_lower/ownership_primitive.rs` 时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 与 [ADR-0033](../../adr/accepted/0033-inline-nullable-representation.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0382` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 所有权与借用](../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 用户确认基于最新 main 继续推进 E3 后续开发与旧路径退休 |
| 基线 | `main` `e89c3630` |
| 分支 | `feature/spec-0382-refine-map-ops-and-primitive-retirement` |
| 前置 | [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md)、[ADR-0033](../../adr/accepted/0033-inline-nullable-representation.md)、[SPEC-0381](../../specs/active/0381-refine-default-routing-and-legacy-retirement.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

将 `Map<K, V>` / `MutableMap<K, V>` 的剩余单步操作（`get` / `m[k]`、`remove`、`consume`、`m[k] = v` 下标赋值及查询/移除结果的空值与值判等）、Map 迭代与 `requireValue`、RangeView carrier 借用运输、借用返回（`borrow-return`）、提前退出资源清理与任意非递归 `storage_type` 值类型 `V` 迁入 `ssa::refine`，同时补齐零大小 `Unit` 局部可变根变量的 `replace(&root, rhs)` / `swap(&left, &right)` 及 RHS 控制转移退出，并从 `unit_lower` 彻底退休已 100% 由细化层接管的 `crates/lang-codegen/src/ssa/unit_lower/ownership_primitive.rs`。

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
   - **退休 `unit_lower/ownership_primitive.rs`**：删除 `crates/lang-codegen/src/ssa/unit_lower/ownership_primitive.rs`，`unit_lower/call.rs` 对残留 `ownership_primitive` 统一收敛为结构化 `UnsupportedNode` 诊断。
3. **细化分流门禁精确化与旧路径校验边界保证（`orchestrate.rs` 与 `validate_bindings`）**：
   - **分流门禁精确化**：`unit_lower/orchestrate.rs` 中仅在编译单元含有 `field_replacements` 或残留 `ownership_primitives` 时才独占走 refine 降级或静态拒绝；对于仅包含常规 `inout` 参数的单元，保留其通过 legacy 路径校验的能力，不以粗粒度 `Inout` 跳过旧路径合法性检查。
   - **校验阶段精确断言**：在回归测试中显式证明对借用期内调用 `inout` 的拦截源于 `validate_bindings` 阶段，其报错种类（`UnsupportedNode`）与诊断位置（`"flag || change(&n)"`）保持严格一致。

## 3. 可验证合同与责任边界

### 3.1 语义合同与责任划分

| 编译器层级 | 结束点与状态机制 | 作用域退出责任 | 控制转移责任（return/break/continue） | 短路运算责任（`&&` / `||`） |
|---|---|---|---|---|
| **旧校验层 (`validate_bindings`)** | 单一 AST `AfterStatement` 静态语句边界 | 检查语句边界借用生命周期是否已关闭 | 在语句级对未结束借用内的并发变更粗粒度拒绝 | 无法跨分支精细跟踪，依赖 AST 粗粒度约束 |
| **细化层 (`ssa::refine`)** | 全程序控制流图 (CFG) 节点与边状态图 | 按 LIFO 逆序发射 `BorrowEnd` 与 `Drop` | 分支提前退出直接清理消亡借用，向汇聚块运输 `entry_dead` 状态 | 按分支独立维护 `dead` 集合，仅对求值分支发射清理与终结 |

### 3.2 范围与边界区分

- **设计目标**：将所有所有权原语、借用与容器操作完整迁入 `ssa::refine`，使细化层成为 typed SSA lowering 的唯一真源，彻底废弃旧式 per-family 分散降级器。
- **已验证范围**：
  - 普通作用域借用、借用返回（`borrow-return`）及嵌套容器字段借用；
  - `NullableBranch` 证明视图下的字段访问与短路逻辑；
  - `Unit` 零大小根与普通标量/字段 `replace`/`swap`，以及 RHS 包含 `return`、`break`、`continue` 时的 MoveOnly 局部资源确定性 cleanup；
  - 正常默认入口下，包含超出 refine subset 特性（如带可空 subject 的 `when`）的函数自然落入旧路并在 `call.rs:109` 处以 `UnsupportedNode` 拒绝。
- **仍开放的边界**：
  - 尚未迁入细化层的残留语言特性（如复杂 nullable subject `when` 匹配、MoveOnly inout 参数全量重赋等）目前仍由旧路径承接；这些特性的细化迁移由后续专项 SPEC 推进。

### 3.3 Mutation 变异验证证据

1. **MoveOnly Map 证明视图保护**：
   - **变异改动**：在 `crates/lang-codegen/src/ssa/refine/build/condition.rs:226` 中，临时移除 `&& !self.move_only(inner)`；
   - **验证命令**：`cargo test -p lang-codegen map_condition_tests`；
   - **失败摘要**：`refined program lowers and verifies: LoweringError { kind: UnsupportedNode, span: ... }` 在借用参数解析处 panic 失败；
   - **所保护修复**：保护了 commit `674bb526`，确保 MoveOnly Map 移除结果进入 `NullableBranch` 建立有效 shared proof view，而非错误走 `MapResultUnwrap` 丢失证明视图导致借用非法。
2. **分流门禁收敛保护**：
   - **变异改动**：在 `orchestrate.rs:51` 中恢复旧有的过宽判定 `!owned.inouts().is_empty()`；
   - **验证结果**：`inout_reject_source` 中的非法借用重叠将跳过 `validate_bindings` 检查；
   - **所保护修复**：保护了 commit `446f7529`，确保普通 inout 单元必须经历旧校验阶段把关。

## 4. 验收记录账本

| 验收项 | 测试 / 命令 | 实际结果 | CI 链接 |
|---|---|---|---|
| MoveOnly Map 结果直接用于条件与分支退出 | `cargo test -p lang-codegen map_condition_tests::move_only_map_remove_direct_conditions_and_branch_exits` | 通过（覆盖 `!= null`、`== null`、`&&`、`||` 短路、`return`、`break`、`continue`、proof view 借用，stdout 与 drop 精确对齐） | [Run 41599455958](https://github.com/Halckon/Koven/actions/runs/41599455958) |
| Refine 与 Legacy 校验门禁边界矩阵 | `cargo test -p lang-codegen map_condition_tests::refine_and_legacy_validation_gate_matrix_tests` | 通过（验证纯 inout 经过 legacy 全单元校验且函数进 refine、拒绝确切源自 `validate_bindings`、字段替换进 refine、混合单元正反路由断言） | [Run 41599455958](https://github.com/Halckon/Koven/actions/runs/41599455958) |
| 旧 Ownership Primitive 退休边界与 RHS 控制转移 | `cargo test -p lang-codegen map_condition_tests::retired_ownership_primitive_boundary_tests` | 通过（验证默认入口自然回退拒绝、forced 模式拒绝、以及 RHS return/continue/break 下 MoveOnly 资源的确定性 cleanup） | [Run 41599455958](https://github.com/Halckon/Koven/actions/runs/41599455958) |
| Refine 全量测试套件 | `cargo test -p lang-codegen refine_` | 81/81 通过（耗时约 89s，覆盖闭包、subset 路由、容器、控制流、Map 等全部细化测试） | [Run 41599455958](https://github.com/Halckon/Koven/actions/runs/41599455958) |
| 代码行数软上限守则 | `python3 scripts/check_rust_sizes.py --base main` | 通过（2070 个手写 Rust 文件全部遵守 `<1000` 物理行软上限约束，`refine_tests.rs` 981 行） | [Run 41599455958](https://github.com/Halckon/Koven/actions/runs/41599455958) |
| 文档结构一致性门禁 | `python3 scripts/check_docs.py` | 通过（836 个 Markdown 文件结构与引用检查全部通过） | [Run 41599455958](https://github.com/Halckon/Koven/actions/runs/41599455958) |
| 代码格式与 Clippy 静态检查 | `cargo fmt --all -- --check` & `cargo clippy --locked --workspace --all-targets -- -D warnings` | 通过（零格式差异、零 warning 警告） | [Run 41599455958](https://github.com/Halckon/Koven/actions/runs/41599455958) |
