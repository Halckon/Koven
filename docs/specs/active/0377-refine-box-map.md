# SPEC-0377：Box、递归装箱枚举与 Map 核心操作迁入所有权细化层

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3 的 `Box<T>`、含 `Box<T>` 负载的递归/MoveOnly `enum class` 构造以及 `Map<K, V>` / `MutableMap<K, V>` 核心操作细化层迁移时 · **唯一真源**：本片目标、范围与验收；长期决定以 [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) 与 [ADR-0033](../../adr/accepted/0033-inline-nullable-representation.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0377` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.47 集合与解构](../../guide/12-collections-destructuring.md) |
| 批准依据 | 用户确认合并 `main` 后按推荐顺序依次实施 SPEC-0376 与 SPEC-0377 |
| 基线 | `feature/spec-0376-refine-container-for-generate` `9c65ad2b`（含 `main` `df07778a`） |
| 分支 | `feature/spec-0377-refine-box-map` |
| 前置 | [ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md)、[ADR-0033](../../adr/accepted/0033-inline-nullable-representation.md)、[SPEC-0376](0376-refine-container-for-generate.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

将独占堆盒 `Box<T>`（含 `Box(payload)` 构造、`Box<T>?` 窄化与 `!!` 解包）、携带 `Box<T>` 负载的递归/MoveOnly `enum class` case 构造与参数/返回传递，以及 `Map<K, V>` / `MutableMap<K, V>` 的核心操作（`mapOf`、`mutableMapOf`、`.size`、`containsKey`、`put`）迁入所有权细化层（`ssa::refine`），并同步更新 `refine_subset_tests` 的路由断言。

## 2. 实施内容

1. **`Box<T>` 与递归装箱枚举准入与类型需求（`refine/subset.rs`、`refine/subset/heap.rs`、`refine/subset/tagged.rs`）**：
   - 引入带活跃集合防环的 `storage_type_seen`、`box_payload_seen` 与 `box_enum_type_seen`，支持 `Box<T>` 及携带 `Box<T>` 负载的递归/MoveOnly `enum class`（如 `enum class Expr { Num(item: Int), Add(left: Box<Expr>, right: Box<Expr>) }`）；在 `push_demand` 中先登记已访问类型再递归收集负载类型需求，避免递归类型死循环。
   - 在 `view_inner` 中纳入 `Box<T>` 以支持 `Box<T>?` 的非空证明视图与 `b!!` 消费解包。
2. **`Box(payload)` 与 MoveOnly 装箱枚举构造（`refine/build/expression.rs`、`refine/build/heap.rs`、`refine/build/tagged.rs`）**：
   - `Box(payload)` 构造：当实参为 Copyable 左值（`ExpressionCategory::Place`）时先经 `ROp::Copy` 生成独立 SSA 值身份，再发射 `ROp::HeapAllocate { owner, payload }`。
   - 含 `Box<T>` 负载的 MoveOnly `enum class` case 构造：校验 `ConstructionRootKind::Inline` 根义务，按求值序求值并适配各负载（Copyable 左值负载同样经 `ROp::Copy` 隔离），组装 payload 聚合后发射 `ROp::TaggedConstruct`。
3. **`Map<K, V>` / `MutableMap<K, V>` 核心操作（`refine/body.rs`、`refine/emit.rs`、`refine/subset/container.rs`、`refine/build/container.rs`）**：
   - 新增 `ROp::MapConstruct`、`ROp::MapSize`、`ROp::MapContains` 与 `ROp::MapPut` 并在 `emit.rs` 中映射为对应 SSA `Operation`。
   - 对标量或 `String` 键值的 `Map<K, V>` 与 `MutableMap<K, V>` 支持空表构造（`mapOf`/`mutableMapOf`）、具名局部或借用参数的 `.size` 与 `.containsKey(key)`（支持 Borrow `String` 键），以及具名局部 `MutableMap` 的 `.put(key, value)` 原地更新与 MoveOnly 键值交付。
4. **子集路由测试同步（`refine_subset_tests.rs`、`refine_tests/container_tests.rs`）**：
   - 将已迁入 `ssa::refine` 的 `"lambda"`、`"Box"`、`"generic class"` 与 `"generic enum subject"` 移入 `nullable_functions_inside_the_subset_are_routed`，并新增 `box_and_map_lower_in_refine` 端到端测试。

## 3. 验收标准

- [x] `refine_tests::box_and_map_lower_in_refine` 与 `refine_subset_tests`（3/3）全部通过。
- [x] `boxed_signature_storage_tests`、`native_tests::boxed_enum_tests`、`unit_lower_aggregate_tests` 与 `map_consume_lowering_tests` 全部保持通过。
- [x] `cargo fmt --all -- --check`、`cargo clippy`、`scripts/check_docs.py`、`scripts/check_rust_sizes.py --base main` 通过。
