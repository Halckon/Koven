# ADR-0033：inline nullable 的 typed SSA 与 LLVM 表示

> **性质**：架构决策记录 · **状态**：proposed · **读取时机**：为标量、String、value class、enum 或顺序容器的 `T?` 设计 SSA/LLVM 表示时 · **唯一真源**：本 ADR

## 状态

proposed

## 接受依据

不适用（`proposed`）。2026-10-23 用户选择“tag + payload，并做 niche”作为 inline nullable 的表示方向；
本记录的具体规则尚待用户确认。

## 背景

[ADR-0017](../accepted/0017-nullable-handle-ssa-abi.md) 只为 pointer-like owner（普通 class、`Box`、`Rc`）
定义了 `NullableHandle` 与 null pointer niche。[Guide 15](../../guide/15-conformance-and-staging.md)
要求 scalar、value class、enum、String 与顺序容器等 inline nullable 先有独立 SSA/LLVM ABI ADR。
[ADR-0018](../accepted/0018-string-owner-runtime-abi.md) 也写明 `String?` 不能复用 pointer-like niche。
当前 `Int?`、`String?` 在 unit 类型 intern 阶段就报 UnsupportedNode。

Guide 11 规定 `T?` 当且仅当 `T` 为 Copyable 时 Copyable，nullable 包装不引入 retain、clone 或析构，
且对内联布局递归是透明边。

## 决策提议

### SSA 类型

- 新增 `SsaTypeKind::InlineNullable { inner, niche }`，用于所有非 pointer-like 的 `T?`；
  pointer-like 仍用 `NullableHandle`，两者不可互换。
- ownership 与 inner 相同：Copyable inner 的 `T?` 为 Copyable，MoveOnly inner 的 `T?` 为 MoveOnly owner。
- 操作沿用 ADR-0017 的四个 operation 与 `NullableBranch` terminator，并把 operand 类型放宽到两种 nullable：
  - `NullableWrap(inner)` 消费 inner，产生 nullable owner；
  - `NullableNull` 产生空值；
  - `NullableIsNull` 只读；
  - `NullableTake(owner, proof)` 在 non-null 边取出 inner。

  Copyable inner 的 take 是复制，原值保持可用；MoveOnly inner 的 take 消费整个 nullable。
- non-null 边的 view：Copyable inner 不需要 view，非空 use 直接以 take-copy 读取；MoveOnly inner
  沿用 ADR-0017 的 `Loan(Shared, inner)` view，指向 nullable 值中的 payload 位置，生命周期与
  verifier 规则相同。

### LLVM 布局

- 默认布局 `{ i8 present, T payload }`：present 为 0 或 1；为 0 时 payload 字节未定义且不得读取。
- **niche**：inner 的 ABI 中存在一个永远不会出现在合法值里的位模式时，省去 tag，用该位模式表示 null。
  只有 ABI 合同写明该不变量的类型才能声明 niche。首批两条不变量目前只是实现事实，本 ADR 接受后
  成为合同：
  - `String`：ADR-0018 的 `{bytes, length, capacity}` 中空串用 null `bytes`，因此 pointer 不能作 niche；
    `capacity` 受目标单块分配上限约束（不超过有符号指针宽度的最大值），用 `capacity == usize::MAX`
    表示 null；
  - 顺序容器（Array/List/MutableList）：header 的 buffer 永不为 null，空容器与零大小元素使用
    模块内 `koven.zst.sentinel` 地址（`llvm/runtime.rs`），因此用 `buffer == null` 表示 null。

  其余类型（整数、Boolean、Char、Unit、value class、enum、Map 等）首版一律用显式 tag，
  不做 Char 越界值或 enum 未用 tag 一类的 niche，留给后续独立决定。
- Guide 03 的语法禁止 `T??`，Guide 06 的分支合流也不产生嵌套 nullable。泛型代入若产生
  inner 本身为 nullable 的 `T?`，本 ADR 不定义其表示，lowering 报 UnsupportedNode。
- drop：读取 present（或比较 niche），为空时不做任何事，非空时调用 inner 的既有 drop glue。
  Copyable inner 不生成 drop。
- 参数、返回与存储按普通 aggregate 值传递，布局经 target preflight 校验，与现有 value class 规则相同。

### 验证

- verifier 拒绝：对 inline nullable 使用 `NullableHandle` 专属操作、未经 proof 的 MoveOnly take、
  view 逃逸到 non-null 边之外，以及 niche 值与非 niche 类型混用。
- LLVM 测试锁定两种布局的 IR 形状与 size/alignment，并核对 niche 类型 `sizeof(T?) == sizeof(T)`。

## 替代方案

### 方案 A：全部使用显式 tag

不采用。实现最简单，但 `String?`、`List<T>?` 每个值多一个对齐后的 word；用户已选择 niche 方向。

### 方案 B：String 用 null bytes 指针作 niche

不采用。现行实现用 null `bytes` 表示空串（`llvm/string.rs`），null 不是非法位模式；改为非 null
canonical 空指针会改动 ADR-0018 的既有实现，不如选用 capacity 哨兵。

## 后果

收益：

- `Int?`、`String?`、`List<T>?` 等获得表示，nullable 家族的 inline 部分可以进入 native；
- `String?` 与容器 nullable 不增加大小；SPEC-0334 记录的 String? 设计门由本 ADR 覆盖。

代价与风险：

- niche 依赖两条现行实现不变量：String 的 capacity 不超过单块分配上限（小于 `usize::MAX`），
  容器 buffer 永不为 null；对应分配路径必须继续保证，并由测试锁定；
- 两种 nullable 表示共用操作名，verifier 需要按类型区分合法组合。

## 关联

- 相关 Spec：SPEC-0350（E3 nullable 家族试点，draft）；SPEC-0334（String? 与 Map 组合 ABI 前置，draft）
- 相关 ADR：[ADR-0017](../accepted/0017-nullable-handle-ssa-abi.md)、[ADR-0018](../accepted/0018-string-owner-runtime-abi.md)、[ADR-0032](0032-ownership-refinement-layer.md)
- 取代的 ADR：无
- 被以下 ADR 取代：无
