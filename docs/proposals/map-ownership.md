# Map / MutableMap 所有权候选

> **性质**：非规范候选设计 · **状态**：未启用 · **读取时机**：仅在评审对应未来版本或候选 Spec 时 · **唯一真源**：现行语义仍以 docs/guide/README.md 导航的 v0.36 为准

本文不修改、取代或启用 Koven v0.36，也不授权实现。候选规则必须经新 guide 版本明确启用后才可成为实现依据。


## 18. `Map` / `MutableMap` 所有权契约（候选设计，v0.11 新增）

第 8 节把 `Map`/`MutableMap` 的可实施契约明确留给“后续 guide”。本节给出一份
候选设计，风格上对齐第 8 节顺序容器已经采用的 place / temporary、声明端 `own` 所映射的
`Value` / 默认 `Borrow` / `Inout`
术语体系，但**这仍然是候选方向，不是本 candidate 直接批准的实施契约**——按本文档既有
的治理惯例，它需要独立走完设计评审（类比第 8 节顺序容器从 v0.5 表面契约到
现在走过的过程），才能进入实施 Spec。在被正式批准前，Phase 2/3/5 的编译器实现不得
依据本节内容加入 `V : Copyable`、key 借用、`put` 或下标赋值的具体检查。

**18.1 Key 等价关系：`Hashable`**

- 新增编译器预声明的规范 marker trait `Hashable`。它与 `Copyable` / `Transferable` 正交：
  数值类型、`Boolean`、`Char`、`String` 满足 `Hashable`；`value class` 当且仅当全部字段满足
  `Hashable` 时满足 `Hashable`，按字段递归计算结构相等与结构哈希。与其他编译器能力一样，
  v1 不开放用户手动实现或覆盖。
- **`Hashable` 不蕴含 `Copyable`**。v0.31 的 `String` 是 MoveOnly 但仍可稳定按 UTF-8 bytes
  哈希；查询只 Borrow 调用者的 key，插入则把 key owner 移入 Map，因此不需要为哈希而复制
  key。普通 `class`、`Box<T>`、`Rc<T>` 及包含这些 identity-bearing 字段的 `value class`
  默认不满足 `Hashable`；未来若发布 identity hash 必须另行定义，不能使用地址或引用计数
  control block 的偶然值。
- `==` 对满足 `Hashable` 的类型按字段结构相等定义（数值/`Boolean`/`Char`/`String` 走对应
  类型的原生相等）；这填补了第 10 节遗留的“`==` 未定义任意可存储 `K` 的通用
  等价关系”的空白，但**只在 `Hashable` 类型范围内**，不为 `Hashable` 之外的类型定义结构
  相等。

**18.2 容器角色与所有权**

比照第 8 节顺序容器的分工：

- `Map<K, V>` 是运行时确定大小、构造后**只读**的独占 owning container（`K : Hashable`，
  `V` 任意 structurally storable 类型）。
- `MutableMap<K, V>` 是可插入、可覆盖、可删除的独占 owning container；只有它提供改变
  条目数量的操作。
- 二者预声明只读属性 `size: Int`，与顺序容器一致不满足 `Copyable`（独占一段哈希表存储
  的所有权）。
- 二者是不同名义类型，v1 不提供隐式相互转换。

**18.3 查询：自动借用 key，按 `V` 的 `Copyable` 能力决定返回**

```kotlin
val v: V? = map[key]               // 查询自动借用 key,不取得 key 所有权
val exists: Boolean = key in map   // 复用第 4 节 in 运算符
```

- 下标查询 `map[key]` 的 `key` 是 `Borrow` 契约：调用点不需要标注，编译器自动按借用交付
  key（写作 `map[borrow key]` 效果相同，仍然合法，纯粹是可选的可读性写法）。查询过程只
  需要 key 的值用于比较 / 哈希，不需要取得其所有权。
- 返回值固定为 `V?`：`V` 满足 `Copyable` 时返回复制的值或 `null`；`V` 不满足 `Copyable`
  时，`map[key]` 的普通按值查询在 Phase 2 产生所有权诊断（不能移出一个仍在 map 里的
  元素），必须改用借用形式的成员查询（例如 `map.getRef(key)`，`key` 同样是 `Borrow`
  契约、标注可选，返回值的借用绑定到 `map` 的有效期，精确的借用返回语法留给
  第 8 节已经预告的“place 返回与生命周期语法”到位后再定）。
- `getValue(key)` 沿用 v0.5 表面契约：缺失时调用 `error()`；返回类型规则与 `map[key]`
  相同。

**18.4 修改：显式 `put`，参数模式对齐顺序容器**

```kotlin
mutableMap.put(key, value)   // 插入或覆盖,取得key与value的所有权,调用点不需要标注
mutableMap.remove(key)       // 按key删除,不取得key所有权,调用点不需要标注
```

- `MutableMap.put(key, value)` 的契约固定为声明端 `own` 的 `Value K`、`Value V`：插入新条目取得两者
  所有权；覆盖已有 key 时，新 `value` 移入、旧 `value` 按顺序容器替换协议（
  第 8 节）的思路析构一次，新 key 替换旧 key，旧 key 也按自身实际类型精确析构一次。
  MoveOnly `String` 等 key 因此没有复制或泄漏特例。调用点不需要任何标注。
- `MutableMap.remove(key)` 的契约固定为 `Borrow K`（查询用，不需要取得删除目标 key 的
  所有权，只需要用来定位条目；调用点标注可选）；返回被移除的 `value`（`Value` 语义
  交付给调用者，`V` 不满足 `Copyable` 时同样合法，因为整条记录被移出后不再有第二个
  访问路径）。
- `mutableMap[key] = value` 是 `put` 的下标语法糖，契约与顺序容器的 `container[i] = value`
  类似，但由于 key 可能不存在，不能复用“槽位一定存在”的替换协议，统一脱糖为 `put`。
- v1 不提供 `getOrPut`、`merge` 等高阶修改方法，这些留给 Phase 5 在本契约之上按普通
  方法实现。

**18.5 与顺序容器共享的边界**

- Map 的哈希表内部结构（开放寻址 / 链式、扩容策略、迭代顺序）属于运行时 ABI 决定，
  不是源语言可观察语义，与第 8 节对顺序容器 allocator 细节的态度一致。
- `MutableMap` 扩容 / rehash 时，正在被借用的 `value`（通过 18.3 节的借用查询）与顺序
  容器的元素借用一样，构成借用冲突，必须被 Phase 3 拒绝。
- v0.18 已定义 `for ((k, v) in map)` 的单次求值与 `iterator()` / `hasNext()` / `next()`
  调用形状；具体名义接口声明、Map iterator 的元素所有权和运行时布局仍留给 Phase 2/5。
