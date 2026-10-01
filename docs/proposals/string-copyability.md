# String Copyable 候选取舍

> **性质**：非规范候选设计 · **状态**：未启用 · **读取时机**：仅在评审 String 值语义、Copyable 扩展或字符串复制能力时 · **唯一真源**：现行语义仍以 [现行 guide](../guide/README.md) 为准

本文不修改、取代或启用 Koven v0.37，也不批准 guide、Spec 或 ADR，不授权实现。它只整理
"是否让 `String` 满足 `Copyable`"这一候选方向的收益、代价与已被现行机制覆盖的部分。

## 1. 问题与现行事实

`String` 在现行规范中已经是**值语义**类型：不可变、无对象身份、`==` 按 UTF-8 字节比较
（[程序、Runtime 与标准库](../guide/13-program-runtime-standard-library.md)）。但它不是
`Copyable`：[Copyable 判定](../guide/11-copyability-layout-construction.md#封闭的-copyable-判定)
把 `String` 与普通 `class`、具名 `object`、函数 / lambda 类型、内建 `Box<T>` 一起列为
`MoveOnly`；13 页同时规定 `String` 满足 `Transferable`，赋值、返回、Value delivery 与 move
closure capture 都转移唯一 owner，不得隐式复制字节、retain 或建立共享 control block。

因此"String 能不能像 `Int` 一样随便用"必须拆成两个独立属性：

| 属性 | 含义 | `String` 现状 |
|---|---|---|
| 值语义 | 不可变、无身份、按内容相等 | ✅ 已满足 |
| `Copyable` | 赋值 / 传参产生独立副本，无 glue | ❌ 不满足 |

现行规则不是"值语义 ⇒ `Copyable`"，而是"**值语义 + 无堆资源 ⇒ `Copyable`**"；`String` 违反的
是后者。参照 11 页，`value class` 只有在全部字段 `Copyable` 时才 `Copyable`，因此
`value class Text(val raw: String)` 与 `String` 同为 `MoveOnly`——`String` 不是值语义规则的
特例，而是这条规则的常规实例。

## 2. `Copyable` 契约的物理含义

[复制契约](../guide/08-class-family-members.md#value-class)规定 `Copyable` 是"无用户可观察的
copy / retain / clone glue，按字段递归复制，实现可以在布局允许时用等价的按位复制优化"，并
明确：

> 满足该能力的类型不得承担唯一资源释放或其他非平凡析构义务。需要引用计数递增、深拷贝、
> 用户 copy hook 或独占 drop 的类型均不得满足 `Copyable`。

`String` 的表示是"字节缓冲区的唯一 owner + 长度"。按位复制会产生两个都认为自己该释放同一
缓冲区的值：要么 double free，要么悬垂。这正是该契约要排除的情形。

**所以"排除这条规则"不能让浅拷贝 String 变安全**——该规则是对物理事实的编码，不是任意
约定。排除它只是把矛盾从"复制成本"转移到"释放责任"。

## 3. 若强行让 String 可复制：只有三条路

可复制的 `String` 必须解决"缓冲区由谁释放"。穷尽后只有三种可能：

| 路径 | 复制语义 | 是否满足现行契约 | 主要代价 |
|---|---|---|---|
| 深拷贝字节 | 每次赋值 O(n) | ❌ 明确被"需要深拷贝"排除 | 成本随运行时长度变化且不可见 |
| 引用计数（ARC） | 计数 +1，O(1) | ❌ 明确被"需要引用计数递增"排除 | 计数成本、原子化要求、析构不确定 |
| GC | 引用共享，O(1) | 需改变运行时模型 | 确定性析构消失、语言模型重建 |

没有第四条路；"谁都不释放"（泄漏）不构成方案。

## 4. 与 `Transferable` 的冲突

`String` 现行满足 `Transferable`，这是跨线程 API（`thread`、`Sender.send`）的前提。若改为
引用计数：

- 计数**非原子**：一个 `String` 有两个副本，把其中一个移到另一线程后，两线程同时触碰计数
  即数据竞争，此时 `String` 不能再满足 `Transferable`——这正是 `Rc<T>` 恒不满足的理由
  （见 [Transferable](../guide/10-ownership-borrowing-drop.md#transferable)）。
- 保留 `Transferable`：计数必须**原子**，每次复制都是原子操作。

代价链因此是：`Copyable` → 引用计数 → 若要跨线程 → 原子引用计数。

## 5. 收益：区分"`Rc` 也能做到"与"只有 `Copyable` 能做到"

**`Rc<String>` 也能做到（增量只是不必显式包一层）：**

1. 同一份内容被多处共享（符号表、缓存 key、配置值）；
2. 不可变数据免疫别名问题——共享引用与值副本在可观察语义上等价。

**`Rc<String>` 做不到的：**

3. `Rc` 自身也是 `MoveOnly`，共享句柄会继续制造同样的移动摩擦；
4. 容器元素可按值读出：现行 `T` 不满足 `Copyable` 时，`container[i]` 的 owned 读取是所有权
   诊断（见 [索引是 place](../guide/12-collections-destructuring.md)）；
5. 取出副本后原值仍可用；
6. 消除 API 的所有权决策耦合：现行每个接受 `String` 的函数都要预先在 `Borrow` 与 `own`
   之间选择，该选择会反向约束调用方写法。

**最本质的一条**：对不可变类型，值语义与引用语义在语义上是同一件事。`MoveOnly` 强制区分
"拥有"与"只读"，对可变类型是必要的（别名可变有风险），对不可变类型则只是认知负担。

## 6. 代价

1. 所有 `String` 操作都带计数，包括不需要共享的场合；
2. 丧失移动优化：[使用规则](../guide/08-class-family-members.md#value-class)规定 `Copyable`
   值在赋值 / 返回 / `own` 交付时"隐式复制，原值仍可使用"，即 `Copyable` 类型没有 move 选项；
3. 与 `Transferable` 冲突，被迫原子化（见 §4）；
4. `Copyable` 从"结构谓词"变成"含隐藏 glue 的行为谓词"，11 页的递归推导需要重写；
5. 语言一致性：只给 `String` 开口子会让 `Copyable` 失去统一性；改通用规则则
   `value class Text(val raw: String)`、`Box<T>`、容器等同类问题必须一并处理；
6. `Rc<String>` 的存在意义变混乱（两层计数）。

## 7. `SSO` 为什么不能解决问题

SSO 让 `String` 小串内联、大串走堆，看起来"常见场景无堆"，但它不能使类型 `Copyable`：

- `Copyable` 是**静态**类型能力（11 页：由编译器绑定的内建能力身份，按类型递归判定）；
- SSO 的"有无堆"是**运行时**属性，同一类型两种状态都有。

按位复制一个堆态值即 double free。SSO 的真实收益在别处：让**显式 clone** 对小串不产生堆
分配；代价是内联态的 **move 变贵**（需拷贝整个内联缓冲）。所以 SSO 是 clone 的优化手段，
不是类型能力的改变。

## 8. 已被现行机制覆盖的部分

| 想要的 | 现行机制 | 是否仍需新能力 |
|---|---|---|
| 常量字符串反复使用 | `const val`：声明本身没有运行时地址、owner、init guard 或 drop，每个运行时 use 重新物化（[关联常量](../guide/05-declarations-callables.md#36-无运行时存储的关联常量与封闭求值)） | ❌ |
| 只读传递字符串 | 默认参数模式 `Borrow`，零拷贝 | ❌ |
| 字面量避免堆分配 | 13 页允许实现让 literal 引用静态只读字节 | ❌ |
| 显式复制字符串 | **缺失**：13 页的 String 最小操作只有 `+`、`==`/`!=`、`println`、`error` | ⏳ 需显式 clone |

`const val` 的语义是"每次 use 从同一 UTF-8 bytes 新建一个普通 String temporary owner，
两个 use 不共享 owner"，因此常量可以无限次使用而不受移动语义影响。这覆盖了"字面量像常量
一样反复用"这一诉求，且不需要新类型。

## 9. 候选结论

1. 保持 `String` 为 `MoveOnly` + `Transferable`；
2. 补齐**显式 `clone()`**：借用 receiver、返回新 owner，使复制成本写在代码里可见，并顺带
   解决 `container[i]` 无法按值取出的问题（`list[0].clone()`）；
3. 不引入 String 专属的 `Copyable` 特例，也不为此引入 ARC/GC；
4. 若未来确需"堆值模型"，应以语言级 ARC/GC 决策推进，而不是 `String` 单点开关。

## 10. 待决策点

- `clone()` 的命名与 receiver 契约（是否 `Borrow` receiver、是否可用于 `Rc<String>` 元素）；
- `clone()` 是否触发 12 页已有的大值复制警告，或需要独立诊断；
- `const val` 的 String 使用点在逃逸时是否需要成本提示；
- 是否考虑"静态字符串类型"（字面量专用、无 owner、可 `Copyable`）作为窄增量，其增量与代价
  见 [集合算法所有权候选](collection-algorithm-ownership.md) §9.3。

## 11. 非目标

本文不提议修改 `Copyable` 定义、不提议引入 `Shareable`、不提议 v1 内的 ARC/GC，也不把
`String` 特殊化视为已批准方向。
