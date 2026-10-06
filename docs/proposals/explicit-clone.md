# 显式 `clone()` 候选设计

> **性质**：非规范候选设计 · **状态**：String 部分已启用；其余候选未启用 · **读取时机**：仅在评审字符串复制能力或显式复制原语时 · **唯一真源**：现行语义仍以 [v0.41 guide](../guide/README.md) 为准

本文保留显式复制方案的讨论，不作为规范真源。2026-10-01 用户批准先落地 String.clone()，
暂缓 Str 和 toString()；仅 builtin String 的 shared Borrow → 独立 owner 切片已进入
[v0.40 String](../guide/13-program-runtime-standard-library.md#封闭的最小操作)，由
[SPEC-0236](../archive/specs/0236-explicit-string-clone.md) 实施、
[ADR-0027](../adr/accepted/0027-explicit-string-clone-abi.md) 固定增量 ABI。
其他类型、泛型复制能力与 nullable 特例仍未启用；本页不批准它们。相关取舍见
[String Copyable 候选取舍](string-copyability.md) 与
[集合算法所有权候选取舍](collection-algorithm-ownership.md)。

## 1. 启用前的问题与动机

`String` 是 `MoveOnly`；v0.38 启用 clone 前的封闭最小操作只有 `+`、`==` / `!=`、`println`
和 `error`（[程序、Runtime 与标准库](../guide/13-program-runtime-standard-library.md)），
**没有任何显式复制能力**。由此产生三个具体缺口：

1. **无法复制一份独立的 `String`**。想保留一份内容相同的副本，目前只能借 `left + right`
   拼接间接实现；
2. **无法从容器按值取出元素**。`T` 不满足 `Copyable` 时，`container[i]` 的 owned 读取是
   所有权诊断（[集合、索引与解构](../guide/12-collections-destructuring.md)）；
3. **无法把 `Borrow` 参数变成 owned**。要把借来的 `String` 存进字段或交给 move closure，
   现行只能要求调用方把参数声明为 `own`。

三个缺口的共同根因是同一个：`String` 缺少一个"以借用读入、以 owned 产出"的原语。

## 2. 保持不变的现行事实

- 13 页规定 `String` 始终是 `MoveOnly` 且满足 `Transferable`，赋值、返回、Value delivery 与
  move closure capture 都转移唯一 owner，"不得隐式复制字节、retain 或建立共享 control block"。
- 13 页规定 `String` 的封闭最小操作集合，且明确"本最小表面不发布 `length`、索引、slice、
  builder、编码转换、用户构造器、可变 buffer、intern、隐式共享或 `toString`/formatting
  protocol"。
- `Rc<T>.share()` 是同类需求的既有先例：13 页把它定义为"唯一公开的 strong-owner 分叉操作。
  它以 shared Borrow 使用 receiver，不消费或修改 payload，返回指向同一 control block 的新
  `Rc<T>`……它不是用户可覆盖的一般 member，也不能由同名源码函数冒充"。
- 13 页同时规定 intrinsic 操作的交付要求："typed 产物必须发布稳定 intrinsic operation
  identity、receiver/payload 类型和 Borrow/Value effect；所有权阶段消费这些 facts，backend
  不得按成员名称字符串重新推导语义"。

## 3. 已选 String 契约的讨论记录

```kotlin
text.clone(): String
```

| 项 | 选中契约（规范以 v0.40 为准） |
|---|---|
| receiver | **shared `Borrow`**；不消费、不修改源 owner；源 owner 在调用后保持 Available |
| 返回 | 内容相同（同一 UTF-8 字节序列）、**独立的新 `String` owner** |
| 语义 | 深拷贝字节；结果 owner 参与普通 ASAP drop，与源 owner 无共享 control block |
| 身份 | compiler-bound intrinsic operation，不是用户可覆盖的一般 member，也不能由同名源码函数冒充 |
| 效果 | 不隐式 retain、不建立共享、不改变源 owner 的可用性 |

### 3.1 与 move / `Copyable` 的关系

`clone()` 是**显式操作**，不改变 `String` 的能力：

- `val b = a` 仍然是 move，`a` 随后不可用；
- `val b = a.clone()` 是显式复制，`a` 仍可用；
- `String` 仍是 `MoveOnly`，不因此获得 `Copyable`，也不需要原子计数。

这一点与 [String Copyable 候选取舍](string-copyability.md) §9 的候选结论一致：复制能力由
显式原语提供，而不是由类型能力提供。

### 3.2 receiver 必须是 `Borrow`

若 receiver 是 `own`，`a.clone()` 会消费 `a`，语义退化为"移动 + 复制"，失去意义；若 receiver
是 `Inout`，调用点还需写 `&a`，而 `clone()` 并不修改源。`Borrow` 是唯一同时满足
"源保持可用"与"可作用于 place（如 `container[i]`）"的选择，也与 `Rc.share()` 的既有形态一致。

## 4. 关键用途

### 4.1 从容器按值取出

```kotlin
val list: List<String> = listOf("a", "b")
val s: String = list[0].clone()   // receiver 以 shared Borrow 使用元素 place
```

这是缺口 2 的直接解法：元素 owner 留在容器内，调用方获得一份独立副本。它覆盖**全部**
`String` 元素，不限于编译期字面量。

### 4.2 把 `Borrow` 参数变成 owned

```kotlin
class Config(var name: String)

fun install(config: Config, name: String) {   // name 是 Borrow
    config.name = name.clone()                // 复制一份 owned 后交付
}
```

这是缺口 3 的解法：callee 需要持有但调用方不打算交出所有权时，用 `clone()` 而不是强制把
参数改成 `own`。

### 4.3 为 move closure 准备 owned binding

move closure 的 owned capture 要求捕获源是 owned、available binding；`Borrow` binding 的
`MoveOnly` 值不能形成 owned capture（[调用、Lambda 与 Closure](../guide/07-calls-lambdas-closures.md)）。

```kotlin
fun makeTask(name: String): () -> String {
    val copy = name.clone()      // Borrow → owned
    return move { copy }         // 捕获 owned local
}
```

### 4.4 不是 `const val` 的替代

`const val` 的使用点会重新物化 String temporary owner（[关联常量](../guide/05-declarations-callables.md#36-无运行时存储的关联常量与封闭求值)），
因此常量场景不需要 `clone()`。`clone()` 服务的是**运行时字符串**。

## 5. 范围边界

已批准切片是**首轮只给 `String`**，理由：

1. `String` 是当前唯一有明确复制需求的 builtin 堆 owner 类型；
2. 通用复制需要新的 marker 能力（类似 `Cloneable`）+ 泛型约束 + 每个类型的实现，是一次
   类型系统扩展，超出最小需求；
3. `Rc<T>` 已有 `share()`（浅拷贝句柄），语义不同，不应合并；
4. `value class` 若全部字段 `Copyable` 本就 `Copyable`，不需要 `clone()`。

因此以下**不在**首轮范围：

| 类型 | 状态 | 说明 |
|---|---|---|
| `Array<T>` / `List<T>` / `MutableList<T>` | 不提供 | 容器深拷贝需要元素级复制能力，留待通用复制设计 |
| `Box<T>` | 不提供 | 同上 |
| 普通 `class` | 不提供 | 引用语义，深拷贝语义需独立定义 |
| `Rc<T>` | 不提供 | `share()` 已覆盖，且语义是共享而非复制 |
| `String?` | 无专属特例 | 沿用一般 nullable 规则；先获得非空 receiver，native inline-nullable ABI 仍延后 |

后果需要明示：**`list.clone()`（容器深拷贝）在首轮不可用**，`list[0].clone()`（元素复制）
可用。这是有意的范围收窄。

## 6. Phase 边界

| Phase | 需要做什么 |
|---|---|
| 2 | 绑定 intrinsic operation identity；发布 receiver 的 `Borrow` 契约与返回类型 |
| 3 | 确认 receiver 建立调用期 shared loan；结果形成新 owner 并参与普通 ASAP drop；不产生共享 |
| 4 | SSA / LLVM：分配 + 字节复制；不得按成员名字符串重新推导语义 |
| 5 | **不涉及**：`clone()` 属于 intrinsic 封闭操作，不是 Phase 5 的 `.ko` 方法 |

## 7. 诊断与成本可见性

- 现行无需新增诊断码：`clone()` 是显式操作，调用点即成本点。
- **不触发** 12 页已有的"隐式 `Copyable` 大值复制"警告：该警告针对隐式复制，而 `clone()`
  的成本由调用者显式写出。若未来仍希望提示超大副本，应作为独立候选评估。
- 对没有合法同名 member 的其他类型调用 `clone()`，沿用稳定的"成员不存在"诊断；
  普通用户同名 member 按原有规则处理，不冒充 String intrinsic。

## 8. 已决边界与后续候选

- 已决：名称为 `clone()`，零参数、零类型实参；receiver 为 shared Borrow，结果为独立 String
  owner；非空含 static literal 均深拷贝，空串使用 canonical storage 但逻辑 owner 独立。
- 已决：String 保持 immutable、MoveOnly、Transferable；显式 clone 不触发隐式 Copyable
  大值复制警告，也不进入 const 求值；String? 不增加专属规则。
- 后续候选：其他 builtin 的元素级复制、泛型复制能力、静态 Str 与 toString() 转换协议。
  均需独立评审与 guide 启用，不能由本次 String 切片推导。

## 9. 非目标

本文不提议修改 `Copyable` 定义、不提议引入 `Cloneable` 或泛型复制能力、不提议容器深拷贝、
不提议 v1 内的 ARC/GC，也不把已选的 String intrinsic 扩展为其他类型或隐式复制的批准。
