# ADR-0033：inline nullable 的 typed SSA 与 LLVM 表示

> **性质**：架构决策记录 · **状态**：accepted · **读取时机**：为标量、String、value class、enum 或顺序容器的 `T?` 设计 SSA/LLVM 表示时 · **唯一真源**：本 ADR

## 状态

accepted

## 接受依据

2026-10-23 用户明确确认：inline nullable 采用 tag + payload 并做 niche；String 的 niche 采用
bytes 指针（空串改用非 null canonical 指针），不采用 capacity 哨兵。

## 背景

[ADR-0017](0017-nullable-handle-ssa-abi.md) 只为 pointer-like owner（普通 class、`Box`、`Rc`）
定义了 `NullableHandle` 与 null pointer niche。[Guide 15](../../guide/15-conformance-and-staging.md)
要求 scalar、value class、enum、String 与顺序容器等 inline nullable 先有独立 SSA/LLVM ABI ADR。
[ADR-0018](0018-string-owner-runtime-abi.md) 也写明 `String?` 不能复用 pointer-like niche。
当前 `Int?`、`String?` 在 unit 类型 intern 阶段就报 UnsupportedNode。

Guide 11 规定 `T?` 当且仅当 `T` 为 Copyable 时 Copyable，nullable 包装不引入 retain、clone 或析构，
且对内联布局递归是透明边。

## 决策

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
  只有 ABI 合同写明该不变量的类型才能声明 niche。首批为：
  - `String`：ADR-0018 规定空串使用“统一的非解引用 canonical pointer”；本 ADR 把它明确为非 null，
    与容器共用模块内 `koven.zst.sentinel` 地址。于是任何合法 String 的 `bytes` 都不为 null，
    用 `bytes == null` 表示 null。现行实现在字面量、concat、clone 与 argv 四处用 null 构造空串，
    实施时先改为 sentinel；判空、打印与 drop 只看 `length`/`capacity`，行为不变；
  - 顺序容器（Array/List/MutableList）：header 的 buffer 永不为 null（现行实现已如此），空容器与
    零大小元素使用同一 sentinel（`llvm/runtime.rs`），因此用 `buffer == null` 表示 null。

  由此全语言只有一条 niche 规则：owner 的数据指针永不为 null，nullable owner 用 null 指针表示空；
  pointer-like handle（ADR-0017）、String 与顺序容器一致。

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

### 方案 B：String 用 `capacity == usize::MAX` 哨兵作 niche

不采用。它不需要改空串构造，但 nullable 是否成立取决于“单块分配上限小于 usize::MAX”这一
分配器与目标性质，而不是 String 自身的结构；以后给 capacity 增加含义（如 SSO 标记、`reserve`）
时需要重新证明，也让 String 成为唯一不用指针 niche 的 owner。

## 后果

收益：

- `Int?`、`String?`、`List<T>?` 等获得表示，nullable 家族的 inline 部分可以进入 native；
- `String?` 与容器 nullable 不增加大小；SPEC-0334 记录的 String? 设计门由本 ADR 覆盖。

代价与风险：

- 空串 bytes 从 null 改为 sentinel，锁定 null 字面量的 String LLVM 测试需要更新；
- “owner 数据指针永不为 null”成为 ABI 合同，今后所有 String/容器构造路径都必须遵守，并由测试锁定；
- 两种 nullable 表示共用操作名，verifier 需要按类型区分合法组合。

## 关联

- 相关 Spec：[SPEC-0350](../../archive/specs/0350-ownership-refinement-nullable-pilot.md)（E3 nullable 家族试点）；SPEC-0334（String? 与 Map 组合 ABI 前置，draft）
- 相关 ADR：[ADR-0017](0017-nullable-handle-ssa-abi.md)、[ADR-0018](0018-string-owner-runtime-abi.md)、[ADR-0032](0032-ownership-refinement-layer.md)
- 局部扩展：ADR-0018（空串 canonical pointer 明确为非 null；其余决定不变）
- 取代的 ADR：无
- 被以下 ADR 取代：无
