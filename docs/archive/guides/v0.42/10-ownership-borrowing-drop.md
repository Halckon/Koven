# Koven v0.42：所有权、借用与析构

> **性质**：规范性语言规范 · **状态**：current（v0.42） · **读取时机**：实现或评审 loan、move、place、drop 与 Transferable 时 · **唯一真源**：本页

本页是现行 Koven v0.42 规范的一部分。规则正文优先于示例；未在本页定义的相邻概念通过链接转交给对应领域页面。

## `Transferable`

跨线程 API 使用 `Transferable` 标记能力；现行语言不定义 `Shareable`。具体判定规则如下：

- **v1 的跨线程 API 只转移所有权**（`thread()`、`Sender.send()` 都使用声明端 `own` 映射的
  `Value` 参数，调用点保持无 marker；没有“值仍保留在原线程、同时可被子线程引用”的共享
  原语），因此 v1 实际只需要判断“这个值能否被
  安全地整体移动到另一个线程”，不需要“这个值能否被多个线程同时引用”——后者
  （`Shareable`，类似 Rust `Sync`）要等到共享 / 引用计数式跨线程原语被设计出来才有意义。
  `Shareable` 的具体规则与跨线程共享原语一并推迟到 v2；v1 只定义并检查
  `Transferable`。
- **`Transferable` 是编译器结构化递归推导的能力**，规则与 [`Copyable`](11-copyability-layout-construction.md)
  平行：
  - 数值类型、`Boolean`、`Char`、`Unit`、`String` 满足 `Transferable`；
  - `value class` 当且仅当其全部字段类型都满足 `Transferable` 时满足 `Transferable`；
  - 普通 `class` 满足 `Transferable`，当且仅当其全部字段类型都满足 `Transferable`——转移
    一个 `class` 实例的唯一所有权本身是安全的（所有权检查已经保证原绑定不再可用），真正
    的风险只在于它内部是否持有不适合跨线程移动的资源；
  - `Box<T>` 满足 `Transferable` 当且仅当 `T` 满足 `Transferable`；
  - **`Rc<T>` 恒不满足 `Transferable`**，不论 `T` 是什么：`Rc<T>` 依赖非原子引用计数，移动
    一个 `Rc` 到另一线程后，原线程完全可能仍持有其他指向同一 refcount 的 `Rc` 句柄，两个
    线程同时触碰非原子计数即是数据竞争。这条规则直接对齐 Rust `Rc<T>: !Send` 的结论；
  - `Array<T>` / `List<T>` / `MutableList<T>` 满足 `Transferable` 当且仅当 `T` 满足
    `Transferable`（容器本身独占所有权，规则随元素递归）。
  - 与 `Copyable` 一样，`Transferable` 由编译器内部标识识别，可用作泛型上界
    （`<T : Transferable>`），v1 不提供用户手动实现、否定或覆盖的语法。
- v1 的跨线程数据竞争防护只使用编译器绑定的 `Transferable` 能力；`Shareable` 不属于
  现行语言。

## 调用期借用与 ASAP 析构

### 所有者、参数绑定与调用期 Loan

v1 有 owned value、调用期 loan 与下节的受限普通借用结果，不引入可存储的引用类型或生命周期参数。
局部变量、临时值和声明端 `own` 所映射的 `Value` 参数在其值为 `MoveOnly` 时拥有唯一析构
义务；无标记或显式 `borrow` 的 `Borrow` 参数以及 `Inout`
参数只是调用者 place 的非 owning 绑定，由调用者保持 owner，callee 退出时不得析构它们。
`Copyable` 值不产生唯一析构义务，也不因借用而生成 copy / retain / clone glue。

一次同步调用按以下固定顺序处理：

1. 先求值 callee，再按**源码顺序**各求值一次 argument operand；命名实参映射不改变顺序。
2. 每个 operand 求值完成后立即应用已由 Phase 2 选定的参数契约，再继续求值下一个 operand：
   声明端 `own` 的 `Value` 对 `Copyable` 产生 owned copy、对 `MoveOnly` 转移 owner；
   `Borrow` 建立 shared loan；
   `Inout` 建立 exclusive loan。因而较早实参的 loan 在较晚实参及其嵌套调用求值期间已经有效。
3. 交付本次 callee 的所有成功建立的 loan 持续到该 callee 返回；没有结果延续的 loan 返回后结束。
   普通借用结果的唯一来源按下节延续，不能与其他实参 loan 一起结束。`Borrow temporary` 合法，
   temporary owner 延长到调用返回后再按本节析构。`Inout temporary` 继续非法。
4. operand 或 callee 产生 `Nothing` 时，不求值其后的 argument，也不为未求值 argument 建立
   loan。`error()` 是 abort，不做异常展开或沿栈析构。

普通调用期 loan 不因 callee 内的最后一次访问提前结束。普通借用结果的受限 last-use 必须
有独立、可验证的延续与终止事实，不能以 owned ASAP 推定；本规则不启用完整 NLL。

### Two-Phase Borrows（方法接收者两阶段借用）

为了支持诸如 `list.add(list.size)` 等典型的自引方法调用模式，避免接收者过早建立的独占借用与实参求值期间的只读访问产生假性借用冲突，方法调用的 receiver 借用采用两阶段激活模型：

1. **Reserved 阶段**：
   在方法调用形如 `receiver.method(arg1, arg2)` 中，当 `method` 的 receiver 声明契约为 `Inout` 时，在求值实参列表（`arg1`, `arg2` 等）期间，该 `receiver` 上的 exclusive loan 处于 **Reserved** 状态。
   - 在 Reserved 状态下，允许对 `receiver` 进行只读访问（读或 shared loan，例如实参表达式中的 `list.size`）；
   - 但**严格禁止**对 `receiver` 进行变异（mutation）、建立新的 exclusive loan 或将其所有权移走（move）。若实参求值引发此类写访问或移动，立即报告借用冲突 `L0135`。
2. **Activate 阶段**：
   当且仅当所有实参表达式求值完成、控制权即将正式移交给目标方法的前一瞬间，该 exclusive loan 从 Reserved 状态原子转换为 **Active** 状态。
   - 仅用于实参求值期间已完成的嵌套调用的 shared loan，在该嵌套调用返回时结束，
     不延续到外层方法调用；例如读取 `list.size` 得到独立的 `Int` 值不把 list 的借用传给 callee。
   - 作为 `Borrow` 实参交付目标 callee 的 shared loan 则按上节持续到该 callee 返回，
     不因实参求值完成而结束。激活时它们不得与 receiver 的 exclusive loan 重叠；
     如 `receiver.update(receiver)` 将同一对象作为 Borrow 实参时，仍报告 `L0135`。
     只有不存在存活的重叠 loan 时，callee 才取得 receiver 的完整独占变异能力。
3. **适用边界**：
   Two-Phase Borrows 仅适用于具名方法调用的 instance receiver 位置；普通实参位置的 `Inout` 借用（如 `foo(&x, x.size)`）仍严格遵循“先求值的实参立即独占生效”规则，不推迟激活。

### `Value` / `Borrow` / `Inout` 参数体内能力

- 声明为 `own value: T` 的 `Value T` 参数是普通 owned local：满足 `Copyable` 时可复制，
  否则按普通移动规则使用；
  未移动的 `MoveOnly` 参数由 callee 在本节确定的析构点负责析构。
- 无标记 `value: T` 或显式 `borrow value: T` 都是同一个 `Borrow T` 参数，允许读取、建立
  嵌套 shared reborrow，以及在 `T : Copyable` 时产生 owned
  copy。它不允许赋值、建立 `Inout` reborrow、析构或从中移动 `MoveOnly` 值。
- `Inout T` 参数是已初始化 place 的 exclusive 非 owning 绑定。它允许读取、shared/exclusive
  reborrow 和以完整新 `T` 替换原值；替换必须先求值 RHS，再析构旧值并提交新值。它不允许
  把 `MoveOnly` 值移出后留下未初始化的调用者 place。
- 从 `Borrow` / `Inout` 参数返回或赋给 owned 目标时，只在 `T : Copyable` 时产生 owned copy；
  `MoveOnly` 情况属于非法移出，而不是借用逃逸。显式普通借用返回按下节交付 shared loan，
  不按 owned 目标消费该值。
- nested reborrow 不得超过 nested call；callee 返回时原 `Inout` place 必须仍为一个完整、
  已初始化且由调用者拥有的 `T`。

闭包捕获会产生超出单次普通调用的环境 owner，仍由 closure capture 与 Transferable 规则封闭；
本节不借“调用期 loan”提前接受或拒绝捕获。instance member receiver 使用
[class-family 与成员规则](08-class-family-members.md)的显式/缺省 mode，并复用本节的 loan 能力。

### 普通借用结果与显式局部绑定

具名函数可声明 `fun view(borrow source: T): borrow U from source`；实例函数可写
`: borrow U from this`。`from` 必须明确唯一的非 owning 参数或 receiver 来源；
结果必须是其稳定 place、只读投影或已验证的普通借用调用延续，不得来自局部 owner、
temporary、`own` 参数或另一个来源。`Inout` 来源可交付只读 shared reborrow，不能交付
inout 返回；父权限在结果及全部子 loan 结束前保持受限。包装函数同样验证实际来源，
不能靠声明文字冒充 origin。泛型 `T`/`U` 表示被借用对象的类型，不是可存储借用类型。

普通借用结果必须在每条正常返回路径交付确定、有效的 shared loan；它不是条件借用。
被借用的既有存储可以是 nullable：此时 loan 本身仍确定存在，payload 可以为 null。
不引入 `(borrow T)?`、`borrow?`、条件绑定或 maybe 语法，不允许条件借用作为值或泛型实参。
未来条件访问优先作为成功分支绑定／失败即退出的受限控制流结果，另行批准后实施。

局部必须显式写 `borrow val item: U = view(source)`（标注可省略）。initializer 也可为
稳定只读 place；绑定不取得 owner、不产生 payload drop，保留真实 origin/loan identity。
普通 `val` 不直接绑定借用调用结果；禁止 `borrow var`、inout 局部、字段/集合/全局存储借用。
借用结果可同步交付 Borrow 参数，或由同一 `from` 的普通借用函数包装转发；不能交付
own/Inout 参数、owned 返回或逃逸闭包。`Copyable` 的显式借用绑定读取仍可产生普通 owned copy。

caller 在 call 返回时结束无关实参 loan，并延续唯一来源及结果 loan；最后使用后的显式
终止边或局部作用域退出先结束结果/子 loan，再恢复父权限，最后按双轨 drop 规则清理 owner。
任何存活结果都阻止重叠 owner 的 move、drop、变异及 exclusive 借用。
受限 last-use 只在全部别名、投影和子 loan 的未来使用可证明时结束；不将完整 CFG/NLL
作为已实现前提，复杂尚未支持路径用结构化诊断拒绝，不能猜测来源或提前结束 loan。

| 诊断 | 合同 |
|---|---|
| L0162 | 普通借用声明／绑定或唯一 from 来源不合法；primary 指向相关 marker，label 指向来源声明 |
| L0163 | 借用结果试图成为 owned 值、存储或逃逸；primary 指向交付/捕获位置，label 指向结果来源 |
| L0164 | 受限借用路径尚无完整 origin/终止证明；primary 指向未支持的控制流或表达式 |

### 原地置换原子原语：replace 与 swap

为了在不打破 `Inout` 借用独占性和所有权完整性的前提下，允许安全地移出并替换 `Inout` 目标处持有的 `MoveOnly` 资源（如链表节点接合、状态机原位转移等），标准库提供经编译器内建特判的原地置换原子原语：

```kotlin
fun <T> replace(inout place: T, own new: T): T
fun <T> swap(inout a: T, inout b: T): Unit
```

1. **`replace(&place, new)`**：
   - 必须通过 `&` 传入可变借用 `place`，以及一个拥有所有权的 `new` 值；
   - 在 exclusive loan 的保护下，原子地将 `new` 存入 `place`，并将 `place` 中原有的旧值作为拥有所有权的返回值返回；
   - 在整个操作过程中，`place` 始终处于完全初始化状态，不存在任何可观测到未初始化内存或双重释放的空洞，因此合法豁免 `L0133`（从借用中移出）限制；
   - 允许写出如 `val old = replace(&this.state, State.Closed)` 等经典所有权流转代码。
2. **`swap(&a, &b)`**：
   - 接受两个 `Inout` 目标；两个目标经由 Place 重叠检查判定为不重叠时合法；若 `a` 与 `b` 产生重叠借用，报告 `L0135` 冲突；
   - 原子交换 `a` 与 `b` 处的值，不产生任何临时未初始化状态。

### Place 重叠与冲突矩阵

调用期 loan 与 drop 规则 的 place identity 由稳定 root `SymbolId` 与零个或多个已解析 field `SymbolId`
组成。两个 place 在 root 不同时不重叠；路径完全相同或一方是另一方前缀时重叠；同一 root
下首个不同字段代表可证明不重叠的存储。普通字段仍不得部分移动，但不同字段可以同时建立
不冲突的 loan。无法形成稳定 root/path 的表达式不是可借用 place。

index place 的逻辑索引证明、容器重分配冲突与 element replacement 由顺序容器 element-place
所有权规则封闭。无法形成稳定 element identity 的 index loan 必须保持明确 deferred，不能按
内存地址或常量折叠自行接受。

本节的 mutable place 也使用封闭规则，不等同于“任何 place”：完整 root 只有源码 `var`
绑定或 `Inout` 参数可变；`val`、`Value` / `Borrow` 参数、解构绑定和 `for` binding 的完整值
不可被 `&` 替换。字段必须声明为 `var`，并且普通 `class` 字段的 receiver owner 当前可独占，
或内联 value/enum receiver path 自身递归满足 mutable place，才是 mutable field place。因此
`val node: Node` 不允许 `&node` 替换 handle，但在 `Node` 是普通 class 且 `next` 为 `var` 时允许
`&node.next`；`val point: Point` 的内联 `var` 字段仍不可变，必须由 `var point` 或 `Inout Point`
投影。group 透明继承内部类别；temporary、`this` 与尚未封闭的 receiver/index 不由本规则
猜测为 mutable。

对同一或重叠 place，调用者侧的冲突规则只有以下一套：

| 已有效状态 | 新 shared loan / read / `Copyable` copy | 新 exclusive loan / mutation | move / drop |
|---|---|---|---|
| 无 loan | 合法 | 仅 mutable place 合法 | 合法 |
| 一个或多个 shared loan | 合法 | 冲突 | 冲突 |
| exclusive loan | 冲突 | 冲突 | 冲突 |

`Inout` holder 在 callee 内通过该绑定进行的读取、替换和 reborrow 是 exclusive loan 授予的
能力，不按“调用者再次访问”处理；对同一 place 的另一参数绑定仍应用上表。新 loan 的 primary
指向产生冲突的 argument operand 或 `&`，label 指向最早仍有效的冲突 loan；move、赋值或
drop 与 loan 冲突时 primary 指向该访问，label 同样指向 loan 来源。诊断和 label 顺序只按
源码顺序，不依赖 hash 迭代。

### 双轨析构策略：纯内存与资源类型

Koven 采用兼顾高吞吐堆内存回收与确定性资源清理的**双轨析构模型**（Dual-Track Drop Strategy）：

1. **类型分类**：
   - **资源类型（Resource Types）**：显式声明了 `deinit(): Unit` 析构成员的普通 `class`（见[class-family 与成员规则](08-class-family-members.md)），以及其字段递归包含资源类型的复合类型；
   - **纯内存类型（Pure Memory Types）**：未声明 `deinit` 的普通 `class`、`value class`、`enum class`、`Box<T>`、`Array<T>`、`List<T>` 等标准内建容器。
2. **纯内存类型：ASAP 激进析构**：
   - 维持既有的 ASAP 规则：在保持所有未来合法读取、借用、移动和赋值不变的前提下，于 owner 不再 live 的最早边界析构仍 `Available` 的 `MoveOnly` 值。如果 owner 从未使用，则在 initializer 完成且绑定建立后立即析构，尽早归还堆内存。
3. **资源类型：词法作用域逆序析构（Lexical Scope Drop）**：
   - 资源类型实例（如互斥锁守卫 `MutexGuard`、文件句柄 `File`、网络套接字等）的析构具有可观察的外部副作用；
   - 资源变量的生命周期**严格绑定到其声明所在的词法作用域块的结束边界**（`}`），或者在显式转移/消费所有权的位置提前结束；
   - 即使该变量在初始化后不再被后续代码读取，它也绝不会被 ASAP 规则提前析构；
   - 同一作用域块结束时，所有仍存活的资源变量按其**声明顺序的逆序**依次执行 `deinit()` 析构；
   - 彻底消除了诸如 `val guard = mutex.lock()` 因未被再次访问而在行尾立刻释放锁的严重并发缺陷。

析构时机继续由上述双轨策略决定；一次实例内部的用户 body、只读 `this` 与字段清理顺序
只由 [`deinit` 契约](08-class-family-members.md#deinit-成员语法与资源析构契约)定义，不把
“字段逆序”误当成新的 owner liveness 或提前终止资源生命周期的规则。

### ASAP 析构

“ASAP”精确定义为：对每条正常控制流路径，在保持所有未来合法读取、借用、移动和赋值 RHS
求值不变的前提下，于 owner 不再 live 的最早边界析构仍 `Available` 的 `MoveOnly` 值。它是
owned-value liveness，不把调用期 loan 缩短为完整 NLL。所有权检查输出显式、源码有序的
drop facts；Phase 4 消费这些事实生成 drop/free，不得重新猜测生命周期。

- `MoveOnly` temporary 在所属完整表达式结束时析构；若作为 `Borrow` 实参，则延长到该调用
  返回后；若被声明端 `own` 的 `Value` 参数移走，则源 temporary 不再析构。
- named owner（纯内存类型）在路径上的最后一次合法使用后析构。若 owner 从未使用，则在 initializer 完成
  且绑定建立后立即析构；initializer 自身仍只求值一次。资源类型则按上一节规则维持至词法作用域末尾。
- 普通 `var` 替换先完整求值 RHS；若 RHS 正常返回，再析构旧值并写入新值。RHS 可读取旧值，
  但若已把旧值移动走，则本次赋值不再为旧值生成 drop。
- `return value` 先求值并交付返回值，再按内层到外层、同层声明逆序析构仍可用的 owner，
  最后转移控制；postfix `?` 的 `Err` 路径使用同一 return cleanup。`break` / `continue` 只析构
  被跳出词法 scope 中的 owner，不能析构目标 loop 下一次迭代仍需要的外层 owner。
- 正常 scope 结束时，仍 live 的 owner 按声明逆序析构。多个 temporary 在同一边界析构时按
  完成求值的逆序处理；`Copyable` 值不进入该顺序。
- 分支分别计算 liveness。若合流后没有未来使用，各条 incoming path 在最早安全边界析构仍
  可用的 owner；某条路径已移动时该路径不析构。若无法证明 branch-local 最后使用，则保守
  延迟到最近共同安全边界，不能提前析构。合流后的未来使用若可从已移动路径到达，仍产生
  L0131，而不是通过在其他路径插入 copy 修复。
- loop backedge 上仍可能在后续迭代使用的 owner 保持 live；只有离开 loop 的边或可证明不再
  回到使用点的路径可以析构。v1 不做完整跨闭包或依赖运行时索引的 NLL 证明；
  普通借用的跨调用延续由上节独立证明，不由本节 owned liveness 推导。

任何有效 loan 都把对应 owner 视为 live；drop 与 loan 冲突必须先报告借用错误，不能通过
提前结束 loan 或静默延后到不可复核的位置“修复”源码。程序已有所有权错误时可以保留用于
抑制级联的恢复状态，但不得据此生成可执行 drop 计划。

### 诊断、产物与阶段边界

| 错误码 | 稳定含义 | 主范围与关联信息 |
|---|---|---|
| L0133 | 从 `Borrow` / `Inout` 绑定移出 `MoveOnly` 值 | primary 为消费位置；label 指向参数声明或 loan 来源 |
| L0134 | `Inout` operand 已是 place，但不是可独占的 mutable place | primary 为 `&`；label 可指向不可变声明 |
| L0135 | read / borrow / mutation / move / drop 与仍有效 loan 冲突 | primary 为后发生的冲突访问；label 指向最早冲突 loan |

L0131 use-after-move 与 L0132 partial-move 的含义不变；同一根因先产生 L0133–L0135 后，不再
追加 L0131/L0132 级联。非 place 或 temporary 的 `&operand` 继续由既有 L0122 参数模式不匹配
拒绝，不迁移到 L0134。有效所有权产物至少能按 expression/control-flow edge 查询 loan begin、
loan end 与 drop facts，并保留 owner/place identity、loan kind 和来源 `Span`。

本节定义 call argument/receiver loan、参数体内 reborrow 与 owned-value drop point。index element
place、closure capture 和 receiver 都必须发布各自稳定 identity 后再复用这些规则。借用返回、用户
生命周期语法、可存储借用、条件借用、完整 NLL 和普通字段部分移动不进入 v1；本节不新增 LLVM 类型。

---
