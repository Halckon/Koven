# Koven v0.43：集合、索引与解构

> **性质**：规范性语言规范 · **状态**：current（v0.43） · **读取时机**：实现或评审顺序容器、element place 与解构时 · **唯一真源**：本页

本页是现行 Koven v0.42 规范的一部分。规则正文优先于示例；未在本页定义的相邻概念通过链接转交给对应领域页面。

## 顺序容器的表示与索引语义

本节把语言契约与实现层次分开。`Array`、`List`、`MutableList` 及本节列出的核心构造、
`size` 和索引操作是编译器预声明的语言原语，不是 Phase 5 再声明的同名 `.ko` API。各阶段
职责唯一如下：

- Phase 1 只建立普通 call / member / index AST，不按名称硬编码容器语义；
- Phase 2 识别预声明符号、容器类型、元素类型、可变性与类型层面的 element-place 类别，不判定
  该 place 此刻能否移动、复制、借用或借用有效期；
- Phase 3 检查构造、读取、替换、整体移动与元素借用的所有权效果；
- Phase 4 实现这些预声明原语的 SSA / runtime 基元及堆分配基线；
- Phase 5 只在它们之上用目标语言实现普通集合方法与算法，不重新声明核心构造、
  `size` 或索引原语，也不引入第二套容器表示或所有权规则。

精确 runtime ABI 仍须由 ADR 决定；本节不把尚未实现的库或 ABI 描述成当前工程事实。

### 顺序容器的角色与长度

v1 对顺序容器作封闭定义：

- `Array<T>` 是**运行时确定长度、构造后长度固定、元素可替换**的独占 owning container。
  “长度固定”只表示同一个 `Array<T>` owner 不能增删槽位；长度仍不属于类型，也不要求编译期
  已知。
- `List<T>` 是**运行时确定长度、只读**的独占 owning container。通过 `List<T>` 不能替换元素
  或改变长度；这里的“动态长度”只表示不同实例可在运行时具有不同 `size`。
- `MutableList<T>` 是可增长、可缩减且元素可替换的独占 owning container；只有它提供改变
  `size` 的操作。

三者都预声明只读属性 `size: Int`。读取 `container.size` 对 receiver 求值一次，只在本次
读取期间访问 header，不消费、复制或转移容器 owner；`size` 不能作为赋值或 `Inout`（调用点
符号 `&`）目标。`capacity` 是 `MutableList` 的实现不变量，v1 不因此自动暴露同名公共属性。

三者是不同的名义类型，v1 不提供隐式相互转换、隐式只读视图或隐式共享所有权。后续标准库
若提供零复制转换，必须是显式且消费源 owner 的操作；若提供借用视图，则必须等借用返回与
视图生命周期另行规范后使用独立类型表达，不能把它隐藏在 `List<T>` 中。

### 唯一 owner 表示与连续缓冲区

每个具体单态化实例只有一种规范逻辑表示：

- `Array<T>` 与 `List<T>` 是固定大小的 owner header，逻辑上持有一个元素缓冲区和当前
  `size`；
- `MutableList<T>` 是固定大小的 owner header，逻辑上还持有 `capacity`，并保持
  `0 <= size <= capacity`；
- header 的大小不随元素数量变化，可按普通值规则位于寄存器、栈槽、字段或其他聚合中；
- 元素位于 header 之外的一段连续缓冲区中；`[0, size)` 全部已初始化，
  `MutableList<T>` 的 `[size, capacity)` 不包含有效 `T`，不得读取或析构；
- 精确字段顺序、指针宽度、空缓冲区 sentinel、padding、对齐和调用 ABI 由后续 runtime ABI
  ADR 决定，不是源语言可观察布局。

三个容器都唯一拥有其逻辑缓冲区；需要物理字节时，它们也唯一拥有并负责释放对应 allocation，
因此自身不满足 `Copyable`，即使 `T` 满足 `Copyable` 也不例外。移动容器只转移 header 代表的
逻辑缓冲区所有权，源值随后不可使用；只读 `List<T>` 不得因此隐式 retain 或获得共享所有权。
空容器或零大小元素可以不请求零字节 allocation，但仍使用同一种 header 与 owner 语义，不构成
第二种表示。

### 元素布局与显式间接层

顺序容器按具体 `T` 单态化；缓冲区满足 `T` 的对齐，相邻元素按目标布局给出的 `stride(T)`
排列：

- 基础标量和 `value class A` 直接作为元素内联。`List<A>` 的缓冲区是连续的 `A` 值，不得
  为每个元素自动创建对象头、`Box<A>` 或额外指针；
- 若 `A` 的字段包含普通 `class`、`Box`、`Rc` 或其他间接 owner，这些 handle 作为 `A` 的
  字段内联，只有其 payload 保持间接存储；
- 若 `C` 是普通 `class`，`List<C>` 的缓冲区连续存储 `C` 的 owner/reference handle，
  `C` 对象本体仍按普通 `class` 规则分配；
- 只有用户显式写出 `List<Box<A>>` 时，缓冲区才存储 `Box<A>` handle，并由各个显式 `Box`
  分别拥有对应堆对象。

容器元素 `T` 必须是 **structurally storable type**：完成类型替换 / 单态化后有有限且具体的
逻辑表示。Phase 2 只判这一与目标无关的 layout kind，不读取 LLVM DataLayout；Phase 4 才对
具体 target 判断 size / alignment / stride 是否可表示。

- 数值类型、`Boolean`、`Char`、`Unit`、`String`、普通 `class` / `Box` / `Rc` handle、有限
  `value class` 与有限 `enum class` 在结构上可存储；这不表示它们满足 `Copyable`；
- 函数值只有在[Callable 规则](05-declarations-callables.md)所述单态化后具体函数指针 / 闭包环境布局已经确定时可存储；
- `T?` 在 `T` structurally storable 时也 structurally storable，tag / niche 的具体布局由
  runtime ABI ADR 和目标布局决定；
- `Any` 只是顶层泛型上界，裸 interface 没有 v1 `dyn` 表示，`Nothing` 没有可构造值；三者及
  `Nothing?` 在 v1 都不能直接作为顺序容器元素；
- 泛型声明中的 `Array<T>` 可以先保留 `T`，但每个实际单态化实例都必须通过该检查；不得为
  通过检查而擦除成 `Any` 或自动改成 `Box<T>`。

### 封闭的构造操作

v1 只预声明以下顺序容器构造操作：

- `arrayOf(...)`、`listOf(...)`、`mutableListOf(...)` 分别构造三种容器。存在显式 expected
  container type 时，其唯一类型实参就是 `T`，每个元素都按该类型检查；`null` 只能在该
  expected `T` 是可空类型时作为元素，`Nothing` 表达式可以按 bottom-type 规则适配该 `T`。
  没有 expected type 时，至少要有一个元素，由第一个元素的静态类型确定 `T`，其余元素必须
  具有同一静态类型，
  不在这里推导公共父类型或擦除为 `Any`；首元素自身若只有 `Nothing` / 无上下文 `null` 类型，
  同样不能据此确定可存储的 `T`。空调用必须由 expected type 或 声明规则 的显式
  use-site 类型实参提供 `T`，否则产生“无法推导元素类型”的类型诊断；
- `Array<T>(size, initializer)` 与 `List<T>(size, initializer)` 是运行时长度构造的调用
  形式：callee contract 将 `size`、`initializer` 都登记为 `Borrow`；`size`
  的类型为 `Int`，`initializer` 的类型为 `(Int) -> T`，其中无标记的索引参数同样是 Borrow；
  随后按 `0` 到 `size - 1` 的升序，
  以每个索引恰好调用一次。已有 place 或临时表达式作为实参都不写借用模式；
- 空的 `MutableList<T>()` 配合取得元素所有权的 `add` 等 Phase 5 API，从未知数量的运行时
  数据源逐步构造动态容器。

这些拼写是编译器预声明、不可被用户重载的核心构造操作，不依赖用户声明 `vararg`。它们在
编译器的统一 callable contract 中保存有序参数契约；列表式构造使用内部重复 `Value`
（等价于重复声明端 `own`）形状，运行时长度构造使用上条固定的两个 `Borrow`。Parser 仍把它们解析为普通调用 AST；
名称解析确认预声明符号后，类型检查才建立专用的 typed construction 节点。因此 Phase 1
不按名称硬编码语义，也不提前推导元素类型。

列表式构造按源码从左到右对每个元素表达式求值一次，并把完整结果直接初始化进缓冲区。临时
表达式与已有 place 交付元素都写作普通调用实参，不需要额外标注，例如
`listOf(endpoint)`。`Copyable` place 交付 owned copy，其他 place 发生移动。运行时长度
构造遵守同一规则：无论 `size`、`initializer` 是已有 place 还是临时表达式，调用点都写作
`Array<T>(size, initializer)`，不需要额外标注；编译器按 callee 已声明的两个 `Borrow`
契约在本次调用期内借用 `size` 与 `initializer`。构造先对
`size` 求值一次并拒绝负值，再对 initializer 求值一次。随后以目标地址宽度受检计算
`size * stride(T)` 并在需要物理字节时取得完整缓冲区；确定性大小溢出或分配失败发生在任何
initializer 调用之前，但已经完成的 initializer 表达式求值副作用不回滚。分配成功后在整个
同步调用期间共享借用来自已有 place 的 initializer，并按索引升序调用；构造器不消费该
initializer，临时函数值在调用结束后按普通 ASAP 规则析构。每次返回的完整 `T` 直接交付对应
槽位。
`MutableList.add` 取得元素所有权（声明端 `own` 映射的 `Value` 契约），因此调用点写作 `list.add(element)` 即可，
无论 `element` 是已有 place 还是临时值都不需要标注；`Copyable` 元素交付 owned copy，否则
移动。所有形式都不得隐式 clone、retain 或装箱。

v1 没有异常展开。负长度、分配失败、受检大小溢出或元素求值中的 `error()` 都以 abort 终止
进程，不承诺在该路径运行部分构造清理；不得把“清理发生”作为可观察语义。正常完成构造后，
owner 析构时按高索引到低索引的顺序恰好析构每个元素，随后释放缓冲区。即使 `T` 是零大小但
带非平凡 drop glue，也必须按逻辑 `size` 执行对应次数。

`MutableList<T>` 扩容或重排时可以把已初始化元素移动到新缓冲区。该操作是 relocation / move
而不是用户可观察的复制；旧位置不再拥有值。存在任何有效元素借用时，不得执行会改变元素
地址或销毁该元素的扩容、缩容、删除、替换或重排。

### 索引是 place，不是隐式 owned 返回

顺序容器的 element-place 索引是编译器预声明的封闭能力，不是名为 `Indexable` /
`MutableIndexable` 的用户 interface，也不能作为用户泛型上界或由用户实现。三种内建
顺序容器通过该能力支持 `receiver[index]` / `receiver[index] = value`；`receiver.get(index)`
和 `receiver.set(index, value)` 不会绕过 place 规则，而是按普通成员查找得到“成员不存在”诊断。
开放自定义索引能力必须等待 place 返回与生命周期语法由后续 guide 定义，**目标排期为 v2**；
与型变、`dyn` 一样属于明确延后而非无限期悬空的特性。具体产生式与
生命周期语法仍留给到时的独立 guide，本条不提前批准任何语法。

普通值读取以及调用实参中借用 / 独占借用索引先按源码顺序对 receiver 和 index 各求值
一次，紧接着做边界检查，再形成绑定到 receiver 有效期的元素 place。`container[i] = value`
是唯一例外，它不在 RHS 之前建立元素 place 或做边界检查，而是唯一遵循下文的替换顺序：

- `T: Copyable` 时，普通值读取，或向声明端 `own` 的 `Value` 参数写
  `consume(container[i])`（调用点不需要标注），都会从 place 取得 owned copy；
- `T` 不满足 `Copyable` 时，普通 owned 读取和作为按值调用实参的 `container[i]` 都必须产生
  所有权诊断，不得移出元素、留下未初始化洞，也不得自动改为 `Box<T>`；
- 在调用实参中，`use(container[i])` 对三种顺序容器都合法（借用由 callee 的 `Borrow` 契约
  自动确定，调用点无 marker）；`mutate(&container[i])` 只对 `Array<T>` 和
  `MutableList<T>` 合法，且 `&` 标注仍是必需项（`Inout` 契约的调用点拼写是符号 `&`，
  不是关键字 `inout`）；
- `container[i] = value` 只对 `Array<T>` 和 `MutableList<T>` 合法，并使用下述唯一替换顺序；
- 只有保持容器初始化不变量的显式操作（例如 `MutableList.removeAt`）或消费整个容器的迭代
  才能移出不可复制元素；现行标准库未在本页之外隐式获得其他 API。

元素替换严格执行：receiver place 求值一次 → index 求值一次 → 右值求值一次并形成完整 owned
临时值 → 确认 receiver owner 仍有效且没有存续的冲突借用 → 按 RHS 执行后的当前 `size` 做一次
边界检查 → 把旧值移入编译器临时槽 → 把新值移入元素槽 → 析构旧值。形成 RHS 期间不保留元素
地址或提前建立可变借用，因此 RHS 可以按普通所有权规则修改同一容器；其副作用不会被回滚。
若 RHS 移动 / 析构 receiver，或新值携带指向待替换元素的借用，Phase 3 必须拒绝。最后两个
move / store 是不可失败的提交步骤；旧值的析构即使 abort，容器槽中也已经是有效新值。

v1 不提供顺序容器 `getOrNull`。当前类型系统既不能用普通 `T?` 表达可空借用，也没有声明
“仅当 `T: Copyable` 时才出现某成员”的规则；为它添加隐式 clone、条件成员或编译器特判都会
扩大模型。安全访问必须显式检查 `index in 0..<container.size`，随后在成立分支中复制
`Copyable` 元素，或把非 `Copyable` 元素 place 作为无 marker 的 Borrow 实参或 `&` 实参。未来若引入
可空借用或条件 API，再由新 guide 定义安全访问器。

具体越界与可变性规则如下：

| 类型 | 长度 / 可变性 | 越界或缺失行为 | 安全访问方式 |
|---|---|---|---|
| `Array<T>` | 构造后长度固定，元素可替换 | `array[i]` 越界触发 `error()` | 显式检查 `i in 0..<array.size` |
| `List<T>` | 运行时长度，只读 | `list[i]` 越界触发 `error()` | 显式检查 `i in 0..<list.size` |
| `MutableList<T>` | 可增删，元素可替换 | `list[i]` / `list[i] = v` 越界触发 `error()` | 显式检查 `i in 0..<list.size` |

### `Map` / `MutableMap` 键值容器所有权规范

`Map<K, V>` 与 `MutableMap<K, V>` 是编译器预声明的独占 owning 键值映射容器。

1. **键能力与等价约束 (`Hashable`)**：
   - 键类型 `K` 必须满足内建 capability `Hashable`；
   - 标量与内建类型 `Int`、`Boolean`、`Char`、`String` 内建满足 `Hashable`；
   - `Hashable` 与 `Copyable` 正交：`String` 虽为 `MoveOnly`，仍可稳定按其 UTF-8 字节进行确定性哈希与相等比较；
   - 普通 `class`、`Box<T>`、`Rc<T>` 默认不满足 `Hashable`。
2. **容器角色与所有权**：
   - `Map<K, V>` 是构造后大小确定、只读的独占 owning 容器；
   - `MutableMap<K, V>` 是支持条目插入、覆盖与删除的可变独占 owning 容器；
   - 容器自身唯一拥有其底层哈希表与条目存储，因此不满足 `Copyable`；
   - 两者均预声明只读属性 `size: Int`；
   - 预声明工厂函数：`mapOf()`、`mutableMapOf()`。
3. **查询语义与借用访问 (对齐 M2B 合同)**：
   - 下标查询 `map[key]` 自动按 `Borrow` 模式借用 `key`，不消费调用者的键；
   - 当 `V` 满足 `Copyable` 时，返回复制的 `V?`（若键不存在则返回 `null`）；
   - `get(key)` 与下标始终是 Copyable 按值查询，不随 `V` 的能力静默改成借用；MoveOnly V 拒绝此按值形式；
   - `requireValue(key): borrow V from this` 对所有可存储 V 使用同一个确定只读借用合同；
     key 自动 Borrow，缺失按既定 `error()`/Abort 机制终止，存在时交付真实槽位 loan，不 clone/retain；
   - `withValue(key, action: (borrow V) -> Unit): Boolean` 只在存在时同步调用 action 一次并返回 true，
     缺失时不调用并返回 false。callback 不得存储、返回或捕获条目 loan 使其逃逸；callback 期间
     重叠 Map 移动、清理、修改及 exclusive 借用均冲突，正常返回／callback 局部退出后恢复权限；
   - `Map<K,V?>` 允许 nullable 存储。`withValue` 的 false 是 Missing，true 时 action 可读取 null
     payload；`requireValue` 同样借用确定存在的 nullable 槽位。旧按值 nullable 查询不能表达三态，
     对 nullable V 不以扁平 null 代替缺失，须使用上述控制流接口；
   - 包含性检查：`key in map` 或 `map.contains(key)` 返回 `Boolean`。
4. **修改语义**：
   - `mutableMap.put(key, value)`：取得 `key` 与 `value` 的所有权（`own` 契约）；若覆盖旧条目，旧键与旧值各自就地精确析构一次；
   - `mutableMap[key] = value`：为 `put(key, value)` 的下标语法糖；
   - `mutableMap.remove(key)`：Borrow `key` 定位并移出条目，按值交付 `value`，被移出的 `key` 精确析构；
   - 变异操作要求 receiver 具有独占 Inout 权限，并在存在活跃借用时报告借用冲突。

### 分配、禁止的隐式表示与 codegen 优化

规范基线中，需要物理字节的非空顺序容器缓冲区由 v1 系统分配器取得，并由唯一 owner 在其
ASAP 析构点释放。`size == 0` 或 `stride(T) == 0` 时可以不请求分配，但逻辑 `size`、边界检查
和每个元素应执行的 drop glue 次数仍必须保持；这不构成第二种容器表示。分配字节数必须以
目标地址宽度受检计算 `size * stride(T)`；计算溢出、无法表示的布局或分配失败必须终止程序，
不能环绕后继续访问较小缓冲区。owner header 位于栈帧不表示元素缓冲区也位于栈帧。v1 不
暴露 allocator hook、原始分配地址或稳定的分配次数；元素 place 的身份 / 别名、求值与析构
顺序属于语义，物理 allocator 调用及资源耗尽发生点不属于程序可依赖的可移植语义。

元素 place 的源语言身份是“逻辑容器身份 + 索引”；移动 owner 转移该身份，不创建新容器。
即使 `stride(T) == 0`、不同索引最终使用同一对齐 sentinel，它们也仍是不同 place，Phase 3
按逻辑索引而非物理地址判断借用冲突；索引确定相同则冲突，无法证明不同则保守视为可能冲突。
runtime ABI ADR 决定零大小元素的 sentinel；LLVM lowering 不得把零步长 GEP 当成可解引用的
真实元素字节，但仍须按逻辑索引执行边界检查和 drop。

v1 明确禁止：

- 根据元素大小、数量、逃逸结果、泛型调用或优化级别，在 `T` 与 `Box<T>` 之间自动转换；
- 在插入、索引、参数或返回边界隐式装箱 / 拆箱元素；
- 在标准 `Array<T>`、`List<T>` 或 `MutableList<T>` header 内预留 small-buffer storage；
- 使用“短容器内联、长容器堆分配”的 tagged 双表示；
- 先在栈上创建缓冲区，再因运行时发现逃逸而搬到堆上。

编译器可以按 as-if 规则把缓冲区分配替换为固定栈存储、SSA 标量或完全消除，但该优化不是
源语言保证，也不是 Phase 4 正确性门槛。优化必须保持源语言可观察的元素 place 身份 / 别名、
求值顺序、边界检查、所有权、借用有效期和析构次数 / 顺序。v1 禁止为运行时长度生成动态
`alloca`，也禁止运行时栈 / 堆迁移。正确的堆基线完成后，可由独立的 Phase 4+ 优化工作从
“编译期已知小尺寸且证明不逃逸”的保守场景开始；具体分析、阈值和是否成功不进入语言规范。

目标 ABI 可以用等价的间接地址形式传递大型 `value class` 或容器 header。这只是调用约定，
不创建 `Box<T>`、不授予独立堆所有权，也不允许地址逃逸源语言生命周期；后端不得仅为按值
参数或返回约定而隐式请求系统堆。具体 calling convention 术语与字段规则留给 runtime ABI ADR。

类型大小同样不得触发表示变化。目标布局确定后，编译器应以诊断注册表分配的稳定 warning
code 报告超过目标相关阈值的静态栈帧或隐式 `Copyable` 大值复制，并建议用户显式选择
`Box<A>`、动态顺序容器或调整算法。阈值属于编译器 / target 配置而非语言常量；warning
不得授权自动装箱，也不得把本可通过 ABI 间接传递消除的临时复制误计为必然成本。

### 为未来 `Array<T, N>` 保留边界

v1 只支持单类型实参 `Array<T>`，其长度在运行时确定并在构造后固定。即使 `arrayOf(...)`
的元素数量是编译期常量，结果类型仍是 `Array<T>`，不产生隐藏的长度实参。

`Array<T, N>` 保留给未来“编译期长度属于类型”的内联数组设计，v1 不新增 `FixedArray`，
标准库也不得用一个普通的双类型参数同名声明占用该拼写。未来启用时必须由新 guide 同时定义
const argument 语法、类型等价、有限布局、所有权、ABI、过大内联值诊断以及它与动态
`Array<T>` 的显式转换；在此之前一律拒绝。

索引括号内语法上接受任意单个表达式，因此 `arr[1..3]` 在 Phase 1 被解析成“以 range
表达式为单个 key 的索引”，**不产生切片 AST 或切片语义**。Phase 2 中，顺序容器要求整数
key，因类型不匹配拒绝该写法。其他 indexed receiver 是否接受 range key 由它们各自的后续
契约决定；现行语言不用尚未封闭的 Map 契约提前批准该类型。区间切片语义仍列为 v2 特性。

## 解构与 `componentN()`

### 局部 `val` 解构语法

```ebnf
local_destructuring_statement
                       = "val", "(", destructuring_binding,
                         { ",", destructuring_binding }, ")",
                         "=", expression ;
destructuring_binding  = Identifier ;
```

仅当 block / lambda body dispatch 已消费 `val` 后看到 `(` 才提交解构。绑定列表至少一项，
只接受普通 Identifier；即使 Lexer 把源码拼写恰好为 `_` 的 token 归入 Identifier，解构 parser
也必须按原始 source slice 将它拒绝。`var`、`const val`、`_`、嵌套 pattern、binding 类型标注、
整个 pattern 的类型标注与 trailing comma 均不支持。重复名称和分量数量是否完整由 Phase 2
按源类型诊断。initializer 在 AST 中只出现一个 ExpressionId，从而锁定“右值只求值一次”；
复制式或消费式语义仍由 Phase 2 / 3 决定。

AST 唯一新增以下 statement variant；不得用等价 pattern table、`Item::Variable`、多个可选
variant 或第五张 AST table 代替：

```text
Statement::LocalDestructuring {
    val_span: Span,
    left_paren_span: Span,
    bindings: Vec<NameMarker>,
    right_paren_span: Option<Span>,
    equals_span: Option<Span>,
    initializer: ExpressionId,
}
```

两个 `Option<Span>` 只有真实 token 存在时为 `Some`；恢复不伪造 delimiter。statement 整体
Span 从 `val` 起到 initializer 或最后实际消费 token 终；pattern 的可观察范围从 `(` 起，正常
到 `)` 终，缺 closer 时到最后一个实际消费 binding / error token 终。每个 binding marker
遵守 present / missing / error 范围规则。

局部解构诊断连续分配 `L0040` 至 `L0046`，依次表示专用的 expected destructuring binding、expected
destructuring separator、unsupported destructuring form、unsupported destructuring context、
unsupported destructuring trailing comma、expected destructuring initializer separator 与
expected destructuring initializer；不得复用声明列表 L0024–L0026。恢复分支精确如下：

| 分支 | 诊断、消费与 AST |
|---|---|
| local `val (` | 唯一提交 `Statement::LocalDestructuring`，后续错误仍保留该 variant |
| block / lambda body 的 `var (` 或 `const val (` | unsupported destructuring form 分别覆盖真实 `var` 或 `const val` 前缀；用 owner-aware 扫描消费本错误 element，形成 `Statement::Error`，不得构造 LocalDestructuring |
| 独立声明入口或文件顶层的 `val (` / `var (` / `const val (` | unsupported destructuring context 主 Span 覆盖真实 `(`；独立入口 owner-aware 消费到 EOF 并形成 `Item::Error`，[完整文件](02-names-files-packages.md#完整文件与声明分隔)入口则保留下一顶层声明 boundary |
| `(` 后直接 `)` | expected destructuring binding 取 `)` 起点空 Span，追加 Missing marker 并保留 `)`；空 pattern 不成为合法形式 |
| 期待 binding 时直接 `,` | expected destructuring binding 覆盖并消费逗号，追加位于逗号起点的 Missing marker，再继续下一项 |
| 期待 binding 时遇 `=`、element boundary、调用方 hard stop 或 EOF | expected destructuring binding 取边界空 Span，追加 Missing marker且不消费边界；随后按缺 `)` 分支继续，但不重复 binding 诊断 |
| binding 源码拼写 `_` | unsupported destructuring form 覆盖并消费该 token，追加同范围 Error marker |
| nested `(`、binding 后 `:` 或其他 unsupported binding form | unsupported destructuring form 覆盖首个引导 token；跟踪局部 owner 消费到当前 pattern 顶层 `,` / `)` 前，追加覆盖实际错误区的 Error marker |
| 完整 binding 后直接出现下一普通 Identifier | expected destructuring separator 取该 token 起点空 Span且不消费；结束前一项并从同 token 解析下一 binding |
| 完整 binding 后直接出现 `=` | 只按缺 `)` 分支处理，不追加 expected destructuring separator；保留 `=` 给 initializer |
| 完整 binding 后出现其他非法 token | expected destructuring separator 覆盖首 token；owner-aware 消费到顶层 `,` / `)` / `=` 或调用方 hard stop 前 |
| 完整 binding 后的逗号紧接 `)` | unsupported destructuring trailing comma 覆盖并消费逗号，不追加 missing binding，保留 `)`；本分支优先于一般逗号消费。若逗号本身已作为空 binding 报错，随后 `)` 不再追加同根因诊断 |
| 缺 `)` 但当前为 `=` | expected closing delimiter 取 `=` 起点空 Span；`right_paren_span = None`，保留 `=` 并继续 initializer |
| 缺 `)` 且遇调用方 element boundary / hard stop / EOF | expected closing delimiter 取边界空 Span并保留非 EOF boundary；随后以同一根因构造缺失 initializer，不追加 separator 级联 |
| `)` 后缺 `=`，当前 token 可开始 expression（[Lambda 规则](07-calls-lambdas-closures.md#lambda-literal)包括 `{`） | expected destructuring initializer separator 取当前起点空 Span；`equals_span = None`，不消费并继续解析唯一 initializer |
| `)` 后缺 `=`，当前为 element boundary / hard stop / EOF | expected destructuring initializer separator 取边界空 Span；不消费边界，建立同位置空 Error initializer，并抑制 expected destructuring initializer |
| `)` 后缺 `=` 且为其他非法 token | expected destructuring initializer separator 覆盖首 token；owner-aware 消费错误区到下一 element boundary / hard stop，建立覆盖实际消费区的 Error initializer，不再追加 initializer 诊断 |
| 已消费真实 `=` 后直接遇 element boundary / hard stop / EOF | expected destructuring initializer 取边界空 Span，不消费边界，建立同位置空 Error initializer |
| 已消费真实 `=` 后为普通 expression-start | 用现行 expression parser 解析唯一 initializer；完整 element 后的结构 stop 与调用方 `}` 均保留 |
| 已消费真实 `=` 后为其他普通非法 token | expected destructuring initializer 覆盖首 token；owner-aware 消费到下一 element boundary / hard stop，建立覆盖实际消费区的 Error initializer；Lexer poison 已有根因时只建 Error initializer |

pattern 级逗号只由 pattern list 消费，真实 `)` 只由 pattern owner 消费；initializer 恢复继续
使用 block / lambda 的结构 stop 与调用方 hard stop。nested delimiter 和 lexical owner 内同形
token 不作同步点，Lexer poison / terminal 根因抑制同 Span 与同 closer 级联。每轮单调前进，
单个 pattern 与 initializer 合计 `O(n)`。


```kotlin
value class Pair<A, B>(val first: A, val second: B)

fun <T> channel(): Pair<Sender<T>, Receiver<T>> { ... }

val (sender, receiver) = channel<Int>()
```

该示例只适用于 block / lambda body 内的局部 `val` 解构；独立声明入口和文件顶层继续以
unsupported destructuring context 拒绝。未来若开放其他上下文，必须复用本节“一次求值、
完整分量、复制或原子消费”的语义，并先更新现行规范。

`Pair<A, B>` 对任意合法的 `A`、`B` 都可以实例化，不要求类型实参满足 `Copyable`。它仅在
`A`、`B` 都满足 `Copyable` 时自动满足 `Copyable`，因此上面的 channel 返回值合法但不可
复制。

解构沿用 Kotlin 风格的 `componentN()` 命名约定。对所有类型，`val (a, b) = e` 都必须先把
`e` **只求值一次**并保存为编译器内部临时值；带副作用的右值不得因分量数量重复执行。随后
按源类型分成两条互不混用的规则。

对 `value class`，编译器使用内建结构解构，不展开为普通方法调用：

1. 编译器把所有分量绑定作为一次结构化操作检查。若源类型满足 `Copyable`，绑定获得分量
   的复制，源值仍可使用。
2. 若源类型不满足 `Copyable`，解构消费整个源值并一次性转移各分量的所有权；操作完成后
   源值不可使用，所有拥有资源的字段最终只能析构一次。这里的“一次性”是所有权检查与
   lowering 的原子边界，不允许观察或使用半解构状态。
3. v1 的消费式结构解构必须覆盖主构造器的全部分量，并且每个分量恰好绑定一次；部分结构
   解构不支持。占位、跳过或丢弃分量的语法尚未定义，不得自行把它们解释为隐式移动或析构。
   `val (a, b) = pair` 对两字段 `Pair` 是完整解构。

- 编译器按 `value class` 主构造参数的声明顺序提供结构分量，逻辑名称为 `component1()`、
  `component2()` 等；完整解构可使用上述编译器内建的结构化操作，不要求先生成一串普通
  方法调用。
- 对单独的 `x.componentN()` 调用：只有对应分量类型满足 `Copyable` 时，自动分量才可从
  `x` 复制返回。不可复制分量不能通过一次独立调用从聚合中移出；应使用消费式结构解构，
  防止产生部分移动状态。
- 普通字段访问遵循[所有权规则](10-ownership-borrowing-drop.md)的 place 规则，同样不能绕过上述限制移出不可复制分量。
- 非 `value class` 若要支持解构，需要显式提供相应 `componentN()` 方法。在右值求值一次
  后，编译器按绑定顺序对隐藏临时值各调用一次 `componentN()`；返回值和 receiver 的所有权
  完全按这些普通方法的签名与调用规则检查。它不自动复制或消费整个源值，也不获得
  `value class` 的原子聚合拆分能力；若前一个调用的所有权效果使后续调用非法，应产生正常
  所有权诊断。


## 37. 借用式顺序容器迭代 provider

### 37.1 compiler-bound provider 与执行顺序

- v1 首轮只有编译器绑定的 `Array<T>`、`List<T>`、`MutableList<T>` identity 提供顺序迭代。
  用户声明的同名类型、`Iterable` / `Iterator`、`iterator()` / `hasNext()` / `next()` 不取得
  intrinsic 身份；String、range、Map、IO lines、普通 class/interface 和用户自定义 provider
  均不是首轮 `for` source。
- 历史语法中的 `iterator()` / `hasNext()` / `next()` 只保留“取得 provider → 检查下一项 →
  取得下一项”的抽象执行节奏，不是 AST 脱糖、名称解析结果或用户可观察的普通方法调用。
  compiler-bound provider 的语义步骤为 `AcquireProvider`、`HasNext`、`NextPlace`、
  `FinishProvider`；这些步骤不是可引用、存储、返回、捕获或重载的源语言值。
- source 表达式精确求值一次。provider 在 source 的稳定 shared access 上取得一次长度快照，
  再按逻辑索引 `0, 1, ... size - 1` 递增访问；空容器不执行 body。整个 provider 生命周期内
  source 长度和元素地址稳定，因此 `MutableList` 也不能在 body 中增删、重排或替换元素。
- 顺序容器的逻辑 `size` 与迭代索引均处于非负 `Int` 域。任一有效容器必须保持
  `size <= 2^31 - 1`；未来增长操作在提交会超过该上限时必须按现行 checked-size/abort
  边界终止而不能截断。内部 pointer-width header/cursor 不改变该源语言不变量。

source 静态类型不是上述 intrinsic container 时使用 L0159；primary 为完整 source expression，
可用时 label 指向其类型声明。已有 Error/Deferred 根因不追加 L0159；同名方法或类型不改变结果。

### 37.2 循环 binding 与借用式解构

- 每轮 `NextPlace` 形成当前逻辑 element place 的 shared access；单名称 binding 是只在本轮 body
  可见的 `Borrow T` binding，不取得或移动容器元素。`T: Copyable` 时普通值使用从该借用读取
  owned copy；MoveOnly `T` 只能读取或继续 Borrow，向 Value 参数交付或 owned return 使用 L0133，
  borrowed closure 逃逸使用 L0137，从该 binding 建立 owned/move capture 使用 L0138；不隐式
  clone、retain、Box 或 Rc。
- 单名称 `_` 是 `Discard`，不创建 symbol。provider 仍推进一次，但实现可以不建立无消费者的
  element loan；这不能改变索引顺序、body 执行次数或 source 生命周期。
- 解构 binding 首轮只接受 concrete `value class` element，按主构造器字段顺序建立 borrowed
  projection；每个具名分量都是 `Borrow FieldType`，`_` 分量不创建 symbol。Copyable 分量的
  普通使用可以复制，MoveOnly 分量不能从 element 中移出。分量数量必须完整且精确，继续使用
  L0118；非 value-class 或不能建立结构投影的 element 使用 L0160，primary 为完整 binding，
  label 指向 element 类型声明。
- 该解构不是局部 `val` 解构的 Copy/Consume，也不调用用户 `componentN()`。element owner 始终
  留在 container；循环 binding 与其派生 closure 均不得活过本轮 element access。

### 37.3 source loan、退出清理与冲突

- owned place source 在 source 求值完成后建立覆盖整个 provider 生命周期的 shared loan；Borrow
  source 复用或 shared-reborrow 既有能力，Inout source 只建立 shared reborrow。循环正常耗尽、
  `break` 或 callable `return` 清理时才结束本层 source loan；循环后原 named source 仍可使用。
- temporary source 先成为 compiler-owned hidden owner，再建立同样的 shared loan。其生命期延长
  到 `FinishProvider` 与 source loan 结束之后；不得在 source expression 后按普通 temporary
  规则提前析构。element 的唯一 owner 始终是 container，循环本身不析构 element。
- source 的 shared loan 覆盖整个 body 和 backedge。整体 move/drop、element replacement、
  `MutableList` relocation 或任何 exclusive access 使用既有 L0135；不因当前索引已知而放宽。
  shared read 与嵌套 shared iteration 合法。`&binding` 不是可变 place，继续使用 L0134。
- 正常 body fallthrough 与 `continue` 都先逆序析构本轮 body-local owner，再结束 element-derived
  binding/loan，然后推进 cursor 并回到 `HasNext`；source loan 与 temporary source 保持。
- `break` 与 exhaustion 在本轮 body cleanup 后依次执行 `FinishProvider`、结束 source loan、
  析构 temporary source，再进入最近 loop exit。嵌套 loop 只清理最近词法 provider。
- `return expression` 先求值并形成返回交付，再依次逆序析构本轮 body-local owner/结束其派生
  loan、结束 element/component binding loan、执行 `FinishProvider`、结束本层 source loan、析构
  hidden temporary source，最后清理外围 scope 并返回；因此在 body 中 `return source` 仍会在
  active source loan 下尝试移动并产生 L0135，不能为了即将退出而提前结束 loan。Copyable element
  copy 可以返回，MoveOnly borrowed element owned return 使用 L0133。`error()`/abort 沿用无
  unwind 契约，不生成清理 edge。

### 37.4 IR/Phase 交接与非目标

- provider 使用无分配的 IR-local 线性状态：shared source loan、一次 length snapshot、hidden
  cursor 和当前 element access；LLVM 只读取既有 container header、执行 checked element
  address 与普通 loan/drop，不生成 iterator object、vtable 或 runtime symbol。
- Phase 2 发布 `StatementId` keyed typed iteration/binding/projection plan；Phase 3 发布 source/
  element loan、temporary 延寿及正常/`continue`/`break`/`return` cleanup facts。
- Phase 4 先封闭 borrowed container length、`Int`/header-size bridge 和 provider primitives；
  AST 到 native 的集成只消费前述 validated facts，不重推 provider 或所有权。
- typed/ownership 契约包含 owned place、Borrow、Inout、field 与 temporary source；首轮 native
  必须覆盖 owned named source、Borrow 参数及 temporary source。临时 source 精确求值一次，
  hidden owner 在正常耗尽、break、return 时按 §37.3 清理，continue 保留，Abort 不展开。
  Inout/field source 的 native lowering 延后，不得因前端已验证而误报为可执行。

本节不启用 consuming iteration、可逃逸 iterator value、反向/步进/并行迭代、Map/range/String/
IO provider、用户自定义 iteration、borrow-return/place-return、动态分发或 coroutine generator。
未来扩展必须另行启用 guide；不能把普通同名方法或某个标准库 class 反向识别为本 intrinsic provider。

## N1a 单来源连续范围 carrier

本节只启用连续范围 `View<T>`；它是携带单一来源 loan 的内联描述符，不能由源码同名
class、value class、函数或 package 获得身份。该身份由编译器类型环境绑定，元素 T 仍
遵循现行可存储类型约束。用户不能声明新的非逃逸类型。

### 类型与使用位置

允许 Borrow 形参、同步 Borrow 实参、表达式链、现行 `for` source、显式 `borrow val`
绑定，以及下节带唯一 `from` 的结果。普通 `val`/`var`、字段、集合元素、泛型实参、own/
inout 形参、escaping closure 捕获或跨线程传递均禁止；`View<T>?` 也禁止。
View 不是 Copyable 或 Transferable，不获得 Shareable。函数类型不能擦除来源合同以
运输 carrier 结果；不能将 carrier 经 Any 或默认可逃逸类型参数绕过这些限制。

非法使用位置使用 L0163，缺失、错误或 owning 来源合同使用 L0162；尚未证明的流或
后端交付使用 L0164/结构化 unsupported，不能继续生成 owned 结果或伪造借用指针。

### 新描述符交付与既有描述符借用

函数显式结果 `View<T> from source` 构造并交付新内联描述符，延续 source 的根 loan；
`borrow View<T> from source` 只借用既有描述符，不能借此宣称完成新描述符交付。
两种结果的唯一 source 必须是该 callable 的非 owning 参数或 receiver，T 与真实来源
元素类型一致；普通 owned 结果不能交付 carrier。合法 wrapper 继续运输根来源，不能
仅凭声明的 from 把局部 owner、外部 owner 或条件/多来源提升为合法结果。

`borrow val` 接新描述符时拥有这份内联 metadata 的存储，但不拥有元素或根 owner；
接既有 carrier 时只建立描述符借用，保留对父 metadata 的依赖。来源、交付种类与终止
必须是独立、可查询的阶段事实，不能仅靠相同 TypeId 或借用类型检查通过推断。

### 根来源与结束

从 List 创建范围记录实际根 owner；从 View 派生范围继承其既有根来源并限制在父范围
内，不借父 metadata 来冒充元素来源，也不延长根自身寿命。root-flat 新描述符不依赖
父 metadata 的持久存储，借用已有描述符则保留该依赖。
所有活跃父、子、兄弟描述符与元素 loan 都保护同一根；其中一个结束不能恢复仍被其他
依赖保护的根权限。根不可在这些依赖存活时 move、drop、变异或独占访问。
结束顺序先解除相应结果/元素 loan，再解除来源依赖，最后清理根 owner。
last-use 仅在已有显式事实足以证明的路径应用，否则保守到词法范围结束或明确拒绝。

禁止把临时来源的 View 保存或返回到来源有效范围之外。同一表达式的立即 Borrow 使用
与现行 `for` hidden-source 延续合法，均不得演变为持久 owner 延寿；for 的正常 break/
return cleanup 仍遵守本页既有 provider 顺序，Abort 不展开。

### 算法与声明边界

首个闭环是 List/View 的 take，后续范围算法复用同一构造原语。算法 body 由 `.ko`
实现；编译器只绑定可复用范围构造、访问和迭代原语，不按算法名称重推语义。
扩展声明只允许可信标准库来源和本片 List/View 的 Borrow receiver；package、源码名
或文件路径不能充当可信来源证明。未发布该证明及 canonical 选择事实前继续拒绝扩展
语义；不开放用户扩展、任意 receiver、扩展属性、mode 重载或 inout 扩展。

范围构造原语只从实际 List/View source 和受检边界创建描述符；动态边界必须满足
`0 <= begin <= end <= source.size`，否则按 error()/Abort 处理，不能扩大父范围。
语言可观察的计数规则如下：take/drop/dropLast 的 `n < 0` 一律 Abort；非负时先令
`k = min(n, size)`，take 保留前 k 个、drop 去前 k 个、dropLast 去后 k 个。
`n = 0` 时结果分别为空/原范围/原范围；`n >= size` 时分别为原范围/空/空。
空源的非负计数都返回空范围，负数仍 Abort。最大 Int 必须先 clip 后算边界，避免先加减
产生溢出。此计数合同不改变普通索引的越界规则。

Phase 2 发布封闭 identity、位置与 callable/result mode；Phase 3 证明实际 origin、
caller root-loan continuation、绑定及 end facts；Phase 4 必须再由 SSA/verifier 验证
描述符交付、来源与权限恢复，Phase 5/6 验证 String/MoveOnly/Resource 正常 native 清理。
前一阶段通过不能替代后一阶段证明。仅有 Parser/AST 或类型通过的路径不得误编。
