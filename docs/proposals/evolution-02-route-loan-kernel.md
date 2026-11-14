# 演进总纲之二：路线 L——借用内核

> **性质**：非规范候选实施计划 · **状态**：未启用 · **读取时机**：仅在规划 loan 归一、流敏感借用、借用类型、视图与独占访问器时 · **唯一真源**：本页维护路线 L 的步骤、规则草案与验收设计；判据与决断见[之一](evolution-01-kernel-and-decisions.md)，现行语义仍以 [Guide 10](../guide/10-ownership-borrowing-drop.md) 为准

本文不启用任何规则、不批准 Spec。文中规则均为草案，代码均为候选伪代码。每一步在实施前仍须经过
独立 proposal → 用户启用 Guide → ADR（如涉及表示）→ Spec 的既有流程。

## 1. 目标

把基底能力 KB2（Place 与 Loan）和 KB3（Origin 携带）做成一个**唯一的、与语法家族无关的模型**，
然后只通过放宽该模型的使用位置来获得新能力。路线终点是：切片、迭代器、guard、用户自定义索引都由库表达，
编译器中不再有按容器或按构造专门实现的借用规则。

### 1.1 起点事实

依据[所有权架构](../architecture/ownership.md)与 `crates/lang-frontend/src/ownership_checking/` 的模块划分：

- loan 的建立与结束按家族分别实现：调用实参、receiver 两阶段、借用结果、闭包捕获、迭代 source/element、
  Map 迭代、范围 producer、元素索引、nullable when、非空断言等各有模块与专门的计划产物；
- Single 与 Unit 两条分析路径并存，产物 ID 不可混用；
- 借用结果的 last-use 是“可证明时才结束”的受限规则，证明不了的路径以 L0164 拒绝；
- SSA 侧另有 `verify_ownership` 与按家族的 lowering 支持。

这些事实只用于确定 L0 的工作面，不构成对实现质量的评价；精确范围须在 L0 的 Spec 中逐模块核对。

## 2. 统一模型

以下模型是全路线的不变基础，L0 引入，之后各步只增加“谁可以成为持有者”和“loan 可以在哪里声明”。

```text
Loan      = { id, place, kind: Shared | Exclusive, state: Reserved | Active, site }
Holder    = 持有一组 Loan 的值：调用帧（实参 loan）、借用局部、借用结果、闭包环境、视图实例
Origins(v)= 值 v 持有的 Loan 集合；owned 值的 Origins 为空

存活      : Loan l 在程序点 p 存活 ⇔ 存在 Holder h，l ∈ Origins(h) 且 h 在 p 存活
合流      : 控制流合流处 Origins 取并集（可能持有即视为持有）
冲突      : 对 place q 的访问与存活 Loan l 冲突 ⇔ q 与 l.place 重叠且访问种类与 l.kind 不相容
            （重叠与相容矩阵沿用 Guide 10，不新增）
失效禁令  : place 上存在存活 Loan 时，其 owner 不得被 move、drop 或以不相容方式访问
签名摘要  : 每个 callable 发布 OriginContract = 结果的 Origins 来自哪些参数（及 this）、各为何种 kind
```

两条性质使它比 Rust 的区域推断简单：

1. 没有生命周期变量，因而没有约束求解；只有一次前向的 Origins 传播和一次后向的持有者活性分析；
2. 跨函数只看 `OriginContract`，不读被调用方函数体；函数体单独对照自己的合同检查。

五条可验证不变量沿用评估文档 §10.2。

**2026-11-14 修订（采纳[evolution 系列评审](../reviews/2026-11-13-evolution-language-architecture-review.md) R1）。** 上面的模型把三种不同的依赖都记成“结果的 Origins 来自某个参数”，覆盖不了它自己承诺的组合。修订如下，R3.4、R4.4、R4.5 已相应改写：

```text
Loan           增加字段 parent: Loan?        —— 由重借产生的子 loan 指向它所派生自的 loan
OriginContract 对每个来源参数 p，取下列两种关系之一：
  DependsOn(p) : 结果依赖持有者 p 本身。在 p 这个绑定上建立子 loan，结果不能比 p 活得久
  Forwards(p.f): 结果沿用 p 的字段 f 已经携带的来源，与 p 这个绑定活多久无关
暂停           : Holder h 的 loan l 有存活的子 loan c 时，经 h 对 c.place 的访问按 c.kind 判冲突
```

评审区分的三种关系在这里的归属：指向持有者自己存储的借用（视图自身的字段）与访问授权（guard 交付的引用不得越过 guard）都是 `DependsOn`；
只有“转发参数已携带的借用”是 `Forwards`。内核有三种含义，表面只需两种写法，且默认取保守的一种。
“没有生命周期变量”这一条不变；“一次前向传播”要多带一棵派生树。
可执行的最小演算与验证用例见[借用内核演算](loan-kernel-calculus.md)。

## 3. 步骤

### L0 归一（无语言变化）

**目的**：让现有全部借用行为由 §2 的模型解释，为之后每一步提供唯一的修改点。

规则：无。现行 Guide 10 的可观察行为逐条保持。

工作分解：

| Phase | 工作 |
|---|---|
| 3 前端 | 在 `ownership_checking` 中引入统一的 `Loan`/`Holder` 表示；把各家族的 loan 建立点改写为“向某 Holder 添加 Loan”，结束点改写为“Holder 死亡”。两阶段借用表示为 `state` 字段，不另设机制 |
| 3 前端 | 发布 `OriginContract`：现行 `from source` 即单元素合同；无借用结果的函数合同为空 |
| 3 前端 | Drop 规划器以 `DropPolicy` 为参数（与路线 C 的 C1 共用），替代纯内存与资源两套判定 |
| 4 SSA | `verify_ownership` 改为从通用 loan/drop 事实检查五条不变量，不再按家族识别形状 |
| 3/4 | 配合 SPEC-0341 的收敛方向退役 Single 路径上的重复所有权逻辑（范围以该 Spec 为准） |

切片建议：按家族逐个迁移，每片只迁一个家族并保持其余不动。顺序由简到繁：
调用实参 → receiver → 借用结果与显式绑定 → 闭包捕获 → 元素索引 → 顺序迭代 → Map 迭代与范围。

验收：

- **事实等价**：对现有测试语料，迁移前后的 loan begin/end、drop facts、诊断码与主范围逐项一致。
  这是纯重构，任何差异都是缺陷或需要单独记录的既有不一致；
- 新增一组与家族无关的不变量测试：手工构造违反各条不变量的 SSA，verifier 必须拒绝；
- 度量基线：记录 `ownership_checking` 中按家族分派的入口数，作为后续步骤的对照。

风险：现有家族之间可能存在未记录的行为差异（同一语义两种结论）。发现即登记并请求决定，不在 L0 内自行统一。

### L1 流敏感

**目的**：把“持有者死亡”从词法块结束改为活性分析的结果，并允许多来源。

规则草案：

- R1.1 借用局部、借用结果等 Holder 的死亡点是其最后一次使用之后（CFG 上的后向活性，含循环回边）。
- R1.2 其类型的 `DropPolicy` 为 `LexicalRequired` 的 Holder（例如将来带 `deinit` 的 guard 视图）死亡点为词法作用域结束。
- R1.3 `from a, b`：结果的 Origins 是对所列每个来源的 loan 之并；函数体每条返回路径上结果的实际 Origins 必须是声明集合的子集。
- R1.4 合流取并集（§2），因此不存在“无法证明”的控制流形状。

R1.4 的含义是 L0164 在借用终止这一类上应当消失：算法对任意可归约 CFG 是全的，保守之处体现为“多冻结了一会儿”，
而不是“拒绝编译”。这直接服务于总纲公理 A4（拒绝必须可预测）。

工作分解：Phase 3 用通用活性分析替换 `borrow_last_use` 的受限证明；Phase 2 解析并检查多来源 `from`；
Phase 4 的 loan end 事实位置随之变化，verifier 不需改动（它只检查不变量）。

验收：

- 正例：借用局部最后使用之后修改其来源；分支内各自借用、合流后修改；循环体内每轮重新借用；`longer(a, b)`。
- 反例：借用仍被后续路径使用时修改来源（L0135）；循环回边上仍存活的借用；返回了未在 `from` 中声明的来源（L0162）。
- 回归：现有全部借用测试结论不变或由拒绝变为接受；任何由接受变为拒绝的用例都必须解释。

### L2 借用即类型

**目的**：依决断 D1/D13，使 `borrow T` 成为可以被 nullable 包裹的类型，从而条件借用不需要新控制流构造。

规则草案：

- R2.1 `borrow T` 是类型构造器，其值是持有一个 Shared loan 的 Holder。参数位置的无标记/`borrow` mode 与之等价。
- R2.2 首版允许出现的位置：参数、局部绑定、带 `from` 的返回类型、以及这些位置上的 nullable 包裹。
  不得作为泛型实参、普通字段类型、函数类型分量。
- R2.3 `borrow val x = e` 保留为 `val x: borrow T = e` 的糖。
- R2.4 nullable 借用的 Origins 不随 null 判定细化：值可能为 null 时仍静态持有 loan，直到它死亡。
  这保守但简单；需要更精确时由用户把作用域收窄。
- R2.5 现有 nullable 的全部表面能力（`?.`、`?:`、smart cast、`if`/`when` 判空）对 nullable 借用成立，
  因为这些能力都不要求其操作数可逃逸；传给 lambda 的情形要求该 lambda 是非逃逸闭包。

- R2.6 **重绑定**：借用局部可声明为可重绑定（表面写法待定，现行禁止 `borrow var`）。赋值使旧 Holder 死亡、新 Holder 产生，
  新值的 Origins 取代旧值的；循环回边上按 §2 取并集。这条规则使迭代式游标（`cur = cur.next`）可写，
  由[之五](evolution-05-data-structure-fitness.md) §4 的 G4 提出，用户已于 2026-11-10 同意随 L2 启用。它只适用于共享借用；独占借用的重绑定归 L4。

表示：nullable 借用为可空指针，利用空指针 niche，不增加标志位。[ADR 0033](../adr/accepted/0033-inline-nullable-representation.md)
已采用 tag + payload 并做 niche 的方案（已核），沿用即可。

验收以库签名为准：`fun get(key: K): borrow V? from this` 能在 `Map` 上声明，且
`map.get(k)?.length ?: 0`、`if (val v = map.get(k)) …`（或等价的现行判空写法）通过；
把结果赋给普通 `val` 字段、放入 `List`、被 move closure 捕获均报 L0163。

退役：scoped Map 访问、nullable when 中与借用相关的专门计划，改由通用规则覆盖（逐项核对后执行）。

### L3 含借用字段的积类型（只读视图）

**目的**：允许库定义持有借用的类型。这是“可存储引用”的全部内核内容。

依赖：路线 C 的 C0（`Escapable` 由通用能力引擎推导）。

规则草案：

- R3.1 `view class` 的字段可以是 `borrow` 字段或其他不可逃逸类型；`Escapable` 恒为假。
  声明与推导不一致时报错（决断 D12）。
- R3.2 构造：实例的 Origins = 各 `borrow` 字段实参上新建的 Shared loan ∪ 各不可逃逸字段实参的 Origins。
- R3.3 只读视图在全部字段 `Copyable` 时满足 `Copyable`；复制得到的值持有相同的 Origins。
- R3.4 **来源的两种写法**（2026-11-14 改写）：
  - `from p`：结果依赖 `p` 本身（`DependsOn`）。`p` 是视图时，结果不能比这个视图值活得久。这是默认，对任何持有者都健全。
  - `from p.f`（成员内可只写字段名 `from f`）：结果沿用借用字段 `f` 携带的来源（`Forwards`），视图可以先死。只对只读视图的 `borrow` 字段成立。
  函数体按所声明的关系检查：声明 `from f` 而实际返回了视图自身字段的借用，报错。
- R3.5 返回不可逃逸类型的函数必须写 `from`（决断 D3）。
- R3.6 不可逃逸值可以被非逃逸闭包捕获，捕获后闭包的 Origins 包含它的 Origins。
- R3.7 泛型参数带隐式上界 `Escapable`；`view class` 自身可以有类型参数，其实参仍须 `Escapable`（L5 才放开）。

初稿的 R3.4 对不可逃逸的 `p` 一律按转发处理。评审的反例是：同一个视图上，`element()` 返回底层数组的元素，`metadata()` 返回视图自己的字段，
初稿给两者相同的合同，后者因此可以在视图消失后继续使用。改写后两者的签名不同：

```kotlin
// 候选伪代码
view class Slice<T>(borrow val source: Array<T>, val start: Int, val length: Int) {
    fun get(index: Int): borrow T from source = source[start + index]    // 转发：依赖 source 上的 loan
    fun drop(n: Int): Slice<T> from source = Slice(source, start + n, length - n)
    fun startRef(): borrow Int from this = start                         // 依赖这个视图值本身
}

fun first(xs: Array<Int>): borrow Int from xs {
    val s = xs.slice(0, 4)      // s 是局部视图
    return s.get(0)             // 合法：get 声明为转发，结果的 Origins 是 xs 上的 loan，s 可以先死
    // return s.startRef()      // 拒绝：结果依赖局部视图 s
}
```

应用层代码仍不出现 `from`；写 `from source` 的是库作者。

工作分解：

| Phase | 工作 |
|---|---|
| 0–1 | `view class` 声明、`borrow val` 字段的语法。`view` 目前不是关键字也不是保留字（已核），应作为上下文关键字引入，并先通过[文法增量检查](parser-grammar-verification-review.md) |
| 2 | 字段类型位置的检查；`Escapable` 推导与隐式上界；返回类型缺 `from` 的诊断 |
| 3 | 构造与方法调用的 Origins 传播（R3.2、R3.4）；视图 Holder 的活性与 L1 共用 |
| 4 | 视图按值聚合传递；不含析构义务；verifier 的不变量 2（loan 必须有来源）覆盖视图字段 |
| 5 标准库 | `Slice`、`StringView`、顺序迭代器以 `.ko` 实现；现有内建迭代 provider 保留为对照直至等价验收通过 |

验收：

- 库优先：上述三个类型由 `koven/**/*.ko` 定义，`for` 可消费用户定义的迭代器视图；
- 试金石：零拷贝的 CSV 分词器，应用层代码不出现 `from`；
- 反例：视图存入普通类字段、返回缺 `from`、视图存活期间修改或移动来源、视图被 move closure 捕获、作泛型实参；
- 与 X0 的衔接：borrowed closure 改写为匿名只读视图后，L0137 与 L0163 应可合并为同一条规则的两个触发位置
  （是否合并诊断码由 proposal 决定）。

### L4 独占 origin

**目的**：让库能够交付可写访问。完成后，元素赋值、用户自定义索引、可变迭代器、`Guard<T>` 都不再需要编译器专门支持。

规则草案：

- R4.1 `inout T` 作为类型：持有一个 Exclusive loan 的 Holder，恒 `MoveOnly`、不可逃逸。允许的位置同 R2.2。
- R4.2 `fun at(i: Int): inout T from this` 要求 `this` 的 mode 为 `inout`；调用期间及结果存活期间，来源上保持 Exclusive loan。
- R4.3 经由独占 Holder 的访问是该 loan 授予的能力，不算对来源的再次访问（与现行 `Inout` 参数规则一致）。
- R4.4 重借（2026-11-14 改写）：把独占 Holder 交给 `Borrow` 或 `Inout` 参数时，在其 loan 之下建立子 loan。
  被调方的合同没有让该参数流入结果或其他存活 Holder 时，子 loan 在调用返回时结束；流入时，子 loan 随结果继续存活，直到结果最后一次使用。
  子 loan 存活期间，父 Holder 被暂停：不得经它做与子 loan 冲突的访问。子 loan 结束后父 Holder 恢复（R4.7）。
- R4.5 含 `inout` 字段的 `view class` 为独占视图，恒 `MoveOnly`；它的方法以 `inout this` 调用时，结果只能写 `from this`，即对视图绑定本身建立 Exclusive 子 loan
  （否则同一独占视图可派生出两个并存的可写结果）。R3.4 改写后这不再是例外：`from this` 本来就是默认，独占视图只是不提供转发写法。
- R4.6 两阶段借用不穿透访问器：`list.at(i).push(list.size)` 中 `list.at(i)` 已是 Active 的独占访问，实参里读 `list` 报 L0135。
  需要时用户先取值。是否放宽留待用例积累。
- R4.7 来源恢复：独占 Holder 死亡后，来源恢复到建立 loan 之前的权限；同一来源上的嵌套独占 Holder 按后进先出恢复。

不由内核承担的部分：**基于下标的不相交证明**。`splitAt` 一类把一个独占视图切成两个的操作，由库用 `unsafe` 实现，
签名写作返回两个独占视图 `from this`，编译器只信任签名（见之四 U1 的“受信任合同”）。这样内核不需要任何整数推理。

糖（可后补，不属本步内核）：`a[i] = v`、`a[i].f = v`、`a[i] += 1` 分别脱糖为对 `at`/`set` 访问器的调用；
需要“访问结束后执行收尾代码”的访问器等路线 S 的帧挂起能力，或以作用域 lambda 表达。

验收：

- 库优先：`MutableList` 的元素访问、一个用户自定义的二维矩阵索引、可变迭代器、`Guard<T>`（含 `deinit`）由库实现；
- 试金石：原地排序（依赖 `splitAt` 或 `swap`）、矩阵行视图；
- 反例矩阵：同一来源两个并存的独占结果、独占结果存活时读来源、独占视图派生两个可写结果（R4.5）、
  经重借逃逸、独占 Holder 被复制；
- 形式化：在实现之前，用一个覆盖 R4.1–R4.7 的小演算（规模参照 Oxide）写出规则并手工或机械检查保持性。
  这是全路线形式化风险最高的一步，不建议跳过。小演算必须覆盖“可写游标”：独占 Holder 在循环中被重绑定为它当前所指对象的一部分
  （链表遍历中插入或删除）。这是 Rust 借用检查中长期困难的用例，是否支持、支持到什么程度应由演算给出结论，而不是在实现中试出来。

退役：封闭的 element-place 借用、一级字段 `replace` 的专门事实等，逐项核对后由访问器 + 通用规则替代。

### L5 泛型放开

**目的**：让不可逃逸类型进入泛型，支持“视图的容器”和以视图为参数/结果的高阶函数。

这一步的需求面尚不明确，只给出方向：

- 类型参数可显式放弃 `Escapable` 上界；此时实例化类型的 `Escapable` 随实参推导，其 Origins 为各实参 Origins 之并；
- 标准库容器逐个审计后放开（先 `Pair`、`Option` 类与只读的定长容器，再可增长容器）；
- 函数类型需要能表达 `OriginContract`（闭包返回借用其参数的结果）；在此之前这类高阶 API 用具体化或作用域 lambda 表达；
- owned existential 的载荷可否为视图（路线 X）也在此步一并回答。

进入条件（2026-11-14 修订，采纳评审 R6）：迭代器、局部的视图容器、以借用为参数或结果的高阶函数已经是主路线承诺的消费者，不再等“出现三个用例”。
其中迭代器所需的部分——不可逃逸类型作泛型实参、interface 方法与函数类型的来源合同——提前为 L3 的必经子步骤；其余部分仍按用例启动。
定形之前先用小型语义原型验证四种迭代：按值；借用外部存储；可同时保留多个独占元素；下一次调用使上一次结果失效（出借式）。不要求它们属于同一个接口，但要说明哪些算法可以统一、哪些不能。

## 4. 贯穿全路线的验证设施

1. **参考检查器**：把 §2 的模型实现为一个独立于生产前端的小型检查器，输入为 SSA 层的通用 loan/drop 事实。
   它与 `verify_ownership` 的关系是“规格 vs 实现”：两者对同一输入的结论必须一致，差异即缺陷。
   规模目标是数百行、无家族分支，可被完整审阅。
2. **差分与等价测试**：每一步的迁移片都以“事实等价”为门槛（L0 已述）；放宽规则的步骤以“只增不减”为门槛。
3. **原生验证**：继续沿用 Linux sanitizer 与析构计数；每个新 Holder 种类至少有一条“来源先于持有者释放”的注入式负例，
   确认若检查被绕过，sanitizer 能观察到。
4. **拒绝可解释性审查**：每个反例的诊断须能用 §2 的一句话解释（哪个 loan、被谁持有、与哪次访问冲突）。
   不能这样解释的拒绝视为设计缺陷。

## 5. 可后补的糖

以下各项均可由局部翻译得到，不阻塞任何步骤，按用例积累情况单独提案：

| 糖 | 翻译为 | 最早可加 |
|---|---|---|
| 方法省略 `from this` | 补全签名 | L3 之后 |
| 唯一借用参数时省略 `from` | 补全签名 | L3 之后 |
| 下标读写与复合赋值 | 访问器调用 | L4 之后 |
| `for` 消费可变迭代器 | `next()` 循环 | L4 之后 |
| 解构绑定借用字段 | 多个借用局部 | L2 之后 |
| 两阶段借用穿透访问器 | Reserved 状态延伸 | L4 之后，需单独论证健全性 |

## 6. 开放问题

- R2.4 的保守性是否会在实际代码中造成可感知的拒绝；若是，是否值得让判空细化 Origins。
- R3.4 改写后与 R4.5 不再不对称。新的问题是转发的拼写：成员内可以写字段名，但类型之外的函数无法指名私有字段，需要一种“转发 `p` 携带的全部来源”的写法。建议：视图的 `borrow` 字段的**名字**在 `from` 中总是可见，即使字段的值是私有的；它是这个类型对外的来源接口，地位相当于 Rust 里总是公开的生命周期参数名。这样类型之外的函数写 `from s.source` 即可，不需要新语法。代价是借用字段改名成为破坏性变更。
- R1 的验证用例（评审给出，尚未运行）：`q = project(p)` 之后修改 `p.field` 再使用 `q` 须拒绝；`q` 最后一次使用之后再用 `p` 须接受；视图自身字段的借用不得超过视图；guard 交付的引用不得超过 guard。
- 视图的 `Transferable`/`Shareable`：只读视图在其指向类型 `Shareable` 时理应可进入作用域线程，规则归路线 C 的 C3。
- 视图是否允许声明 `deinit` 之外的资源语义（例如必须显式消费），与决断 D11 一并考虑。
- 多来源结果的调用方冻结粒度：是否需要让调用方知道“实际只来自其中之一”（会引入路径敏感，倾向不做）。

## 7. 之六至之八带来的增补

以下四点由后续文档提出，尚未并入上文的规则草案，进入参考检查器时一并处理：

- **静态来源**：Origins 的元素增加 `Static(decl)`，永不失效；`from` 可指名静态声明（[之六](evolution-06-text-globals-extensions-metadata.md) J5）。
- **保证析构**：须在形式化小演算中证明“不可逃逸且 `LexicalRequired` 的值，其析构必在所在作用域结束前执行”；若引入线程失败边界（之十一 J48），证明须覆盖失败出口，所需条件见之七 §1.2。
  结构化并发的健全性依赖它（[之七](evolution-07-concurrency-async-io-ffi.md) §1.2、J17）。
- **origin 流入已有持有者**：`scope.launch(f)`、向局部容器加入视图都要求被调方把实参的来源并入接收者。L5 的合同形式须覆盖，否则之七 S2b 之前要补。
- **载体**：L0 的参考检查器建立在“所有权图”上（[之八](evolution-08-compiler-ir-escape-simd-selfhost.md) J27–J29）。
