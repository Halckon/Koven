# SPEC-0288: Map 键值容器原生执行基础（SSA 原语、LLVM IR 代码生成与 Native Runtime 哈希表）

> **性质**：变更合同 · **状态**：in-progress · **读取时机**：实施或评审 Map 容器 SSA 原语、LLVM 代码生成与原生哈希表运行时时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P2-0288` |
| 所属 Phase | Phase 4 SSA 原语与 lowering；Phase 5 LLVM IR 目标代码生成与 Native Runtime；Phase 6 端到端编译执行集成 |
| 语言规范 | 现行 [Guide v0.41](../../guide/README.md)；[集合、索引与解构](../../guide/12-collections-destructuring.md) |
| 批准依据 | 用户推进里程碑中 M3B Map 原生执行工作授权；Guide v0.41 §12 键值容器所有权规范 |
| 前置 Spec | SPEC-0287 M2B 通用借用合同冻结与 Map 键值容器类型系统基础已合入 main |
| 前置 ADR | [ADR-0008](../../adr/accepted/0008-internal-value-and-allocation-abi.md)、[ADR-0016](../../adr/accepted/0016-interprocedural-borrow-abi.md) |
| 关联 ADR | 无新增 ABI |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend`（Typed 容器描述符交接）、`lang-codegen`（typed SSA 模型与 verifier、单文件/编译单元 Lowering、LLVM 布局与 Native Runtime 哈希表生成、端到端 native 测试） |
| 语言语义变更 | 否（遵循 Guide v0.41 §12 现行规范） |

## 1. Goal

基于 SPEC-0287 建立的 Map 前端类型系统与规范，实施 Map 键值容器的完整后端执行管道：
1. **Typed SSA 模型与 Verifier**:
   - 定义 `MapContainerKind` (`Map`, `MutableMap`)；
   - 在 `SsaTypeKind` 增加 `MapContainer { kind, key, value }`，归属于 `Ownership::MoveOnly`；
   - 增加 typed SSA 操作：`MapConstruct`, `MapSize`, `MapContains`, `MapGet`, `MapInsert`, `MapRemove`；
   - 在 verifier 中校验各 Map 操作的类型约束、操作数所有权与 Inout 语义。
2. **前端描述符交接与 Lowering**:
   - 在 `lang-frontend` 的 `TypedFile` 与 `CompilationUnitTypedBodies` 中发布 Map 描述符；
   - 在 `lang-codegen` 的 `lower_frontend` 与 `unit_lower` 中实现 Map 构造、属性读取、方法调用及下标读写脱糖的 lowering。
3. **LLVM IR 生成与 Native Runtime 哈希表**:
   - 内存表示：以堆分配条目缓冲区实现开地址（Open Addressing）线性探测哈希表；
   - 槽位布局：每个槽位包含状态标志（`Empty`, `Occupied`, `Deleted`）、`key` 和 `value`；
   - 哈希与相等判定：内建支持 `Int`, `Boolean`, `Char` 及 `String` 的确定性哈希计算与相等性判定；
   - 动态扩容与再哈希：负载达到 75% 时触发自动扩容与再哈希；
   - 精确零泄漏析构：Map 作用域退出时遍历活跃槽位，对 MoveOnly 的 key（如 String）及 value 分别调用 drop glue，随后释放条目缓冲区。
4. **端到端原生可执行文件测试**:
   - 支持 `mapOf()`、`mutableMapOf()`、`m.size`、`m.contains(k)`、`mm.put(k, v)`、`mm[k] = v`、`mm.remove(k)` 单文件与编译单元原生编译执行。

SPEC-0287 已在前端类型系统和所有权规划中确立了 `Map<K, V>` 与 `MutableMap<K, V>` 的规范与约束。
根据最新的语言设计裁决：
- **参数**：保持 `borrow x: T`（修饰符在名字前，类型不变）；
- **返回值**：使用 `: borrow T from receiver`（函数结果位置，修饰符在类型前），二等类型（仅限函数结果，不可嵌套在泛型实参或集合元素中）；
- **局部变量**：`val s = names.first()` 依靠推断，不写标注，由 LSP inlay hint 显示 `: borrow T`；
- **可空借用结合性**：第一片规定 `borrow V?` 等价于 `(borrow V)?`（可选的借用：条目存在时借用已分配槽位，不存在时为 null）；拒绝“借用可空存储”的形式。

然而，当前的后端 SSA 模型仅支持顺序容器（`Array`, `List`, `MutableList`），没有键值映射容器的类型表示、SSA 原语与运行时实现。用户无法将包含 Map 的程序编译为原生机器码执行。
本 Spec 补齐这一阶段下沉，使 Map 成为 Koven 原生目标代码生成的完整一等公民。

## 3. 技术方案

### 3.1 前端 Typed 描述符发布 (`crates/lang-frontend`)

为保证 frontend 到 codegen 的解耦与精确交接，在 `lang-frontend` 中定义并记录结构化描述符：
- `MapConstructionDescriptor`: 记录构造调用、目标容器类型；
- `MapSizeDescriptor`: 记录 receiver 表达式、容器类型与返回 Int 类型；
- `MapContainsDescriptor`: 记录 receiver、key 表达式；
- `MapGetDescriptor`: 记录 receiver、key 表达式与返回类型；
- `MapPutDescriptor`: 记录 receiver、key、value 表达式；
- `MapRemoveDescriptor`: 记录 receiver、key 表达式与返回类型。

### 3.2 SSA 模型与验证器 (`crates/lang-codegen/src/ssa/`)

1. **类型定义**:
   - `MapContainerKind`: `Map` (只读), `MutableMap` (可变)；
   - `SsaTypeKind::MapContainer { kind: MapContainerKind, key: SsaTypeId, value: SsaTypeId }`；
   - 所有权：`type_ownership` 恒为 `Some(Ownership::MoveOnly)`。
2. **SSA 操作**:
   - `MapConstruct { container: SsaTypeId }` -> `Value(container)`；
   - `MapSize { owner: EntityId }` -> `Value(Int)`；
   - `MapContains { owner: EntityId, key: EntityId }` -> `Value(Boolean)`；
   - `MapGet { owner: EntityId, key: EntityId }` -> `Value(V?)`（对于 Copyable V）；
   - `MapInsert { owner: ValueId, key: ValueId, value: ValueId }` -> `Value(container)`（消费旧 owner，返回更新后的 owner）；
   - `MapRemove { owner: ValueId, key: EntityId }` -> `(Value(V?), Value(container))`。
3. **验证器扩展**:
   - 验证 `owner` 必须是匹配的 `MapContainer` 类型；
   - 验证 `key` 与 `value` 类型与容器的类型实参一致；
   - 验证 `MapInsert` 与 `MapRemove` 的 receiver 必须是 `MutableMap`。

### 3.3 LLVM 目标代码生成与运行时哈希表 (`crates/lang-codegen/src/llvm/`)

1. **结构布局**:
   - `MapLayout`: Header 为 `{ buckets: ptr, size: size_t, capacity: size_t }`；
   - `SlotLayout`: 每个条目槽位为 `{ state: i32, key: KeyType, value: ValueType }`，其中 `state` 取值：
     - `0`: Empty
     - `1`: Occupied
     - `2`: Deleted (Tombstone)
2. **运行时例程**:
   - `map_construct`: 初始化 `buckets = null, size = 0, capacity = 0`（或分配初始 16 槽位全 0 清理）；
   - `map_size`: 直接提取 Header 的 `size` 字段（转为 i32 / Int）；
   - `map_contains`: 按 key 哈希定位线性探测，若在达到 Empty 前找到 Occupied 且 key 相等则返回 true，否则 false；
   - `map_insert`: 若 capacity 为 0 或 `(size + tombstones + 1) * 4 >= capacity * 3` 则进行扩容（初始 16，后续翻倍）；线性探测查找已存在槽位或首个可用槽位；若键已存在则覆写并析构旧值，否则插入新槽位且 `size += 1`；
   - `map_remove`: 查找匹配槽位，若命中则将 state 置为 Deleted，`size -= 1`，析构旧键并交付原值；
   - `map_drop`: 遍历所有槽位，对 state == Occupied 的槽位分别 drop 其 key（若 MoveOnly）与 value（若 MoveOnly），随后调用 `free(buckets)`。
3. **哈希与键相等逻辑**:
   - 对标量（`Int`, `Boolean`, `Char`）使用位混淆乘数哈希与 `icmp eq`；
   - 对 `String` 使用 FNV-1a 遍历字节计算哈希，比较时先比长度再比字节内容。

## 4. 非目标

- 本 Spec 不支持用户自定义类型的 `Hashable` 实现；
- 本 Spec 不包含 Map 的迭代器（`for (k, v in map)` 留待 M3B 迭代器 Spec）；
- 本 Spec 不实现通用的标量可空结构体（`Int?` 通用语言层 Option），针对 Map.get 的 Copyable V 返回提供端到端查询与非空断言支持。

## 5. 验收标准

- [ ] G1: SSA 模型支持 `MapContainerKind`、`SsaTypeKind::MapContainer` 及 6 个 Map SSA 原语，Verifier 全绿。
- [ ] G2: 单文件与多文件编译单元成功将 `mapOf()`、`mutableMapOf()`、`m.size`、`m.contains(k)`、`mm.put(k, v)`、`mm[k] = v`、`mm.remove(k)` lower 为 typed SSA。
- [ ] G3: LLVM IR 生成开地址哈希表，正确处理标量（`Int`, `Boolean`, `Char`）与 `String` 键的哈希和比较。
- [ ] G4: 哈希表支持动态扩容、哈希冲突线性探测与墓碑复用。
- [ ] G5: Map 作用域退出与条目覆盖/删除实现精确析构，通过 ASan/LSan 零泄漏检验。
- [ ] G6: 全套 Rust 尺寸护栏（`check_rust_sizes.py`）及所有文档门禁 100% 绿灯。

## 6. 验证记录

- **Phase 1 (规范与拓扑)**:
  - 待执行。
- **Phase 2 (SSA 与后端实现)**:
  - 待执行。
- **Phase 3 (测试与门禁)**:
  - 待执行。
- **Phase 4 (归档与 PR 闭环)**:
  - 待执行。
