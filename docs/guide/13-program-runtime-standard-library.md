# Koven v0.47：程序入口、Runtime 与标准库

> **性质**：规范性语言规范 · **状态**：current（v0.47） · **读取时机**：实现或评审 main、project、String、Rc、IO、并发或标准库边界时 · **唯一真源**：本页

本页是现行 Koven v0.47 规范的一部分。规则正文优先于示例；未在本页定义的相邻概念通过链接转交给对应领域页面。

## 线程、Channel 与 Move Closure

```kotlin
val worker: Thread<Int> = thread(move { compute() })
val result: Int = worker.join()

val (sender, receiver) = channel<String>()
val producer = thread(move { sender.send("hi") })
val text: String? = receiver.receive()
```

v1 提供受限线程：只转移所有权，没有共享内存。

- **创建**：`fun <T> thread(own body: move () -> T): Thread<T>`，要求 `T` 满足 `Transferable`，
  body 的全部捕获满足 `Transferable`（判定见[所有权规则](10-ownership-borrowing-drop.md)）。函数类型的
  `move` 前缀表示只接受不含借用捕获的闭包，调用点必须显式使用 `move` closure。创建失败时按
  [Abort 输出合同](09-nullability-errors.md#abort-输出合同) abort。
- **join**：`fun <T> Thread<T>.join(): T` 消费 handle，阻塞到 body 返回并交付其结果。
- **隐式 join**：`Thread<T>` 尚未 join 就被析构时，析构过程阻塞到该线程结束，并析构其结果。
  v1 不提供 detach。因此函数返回时，它创建并仍持有的线程都已结束。
- **channel**：`fun <T> channel(): Channel<T>`，其中
  `value class Channel<T>(val sender: Sender<T>, val receiver: Receiver<T>)`，通常按名解构使用。
  channel 无容量上限，`send` 不阻塞。
  - `Sender<T>.send(own value: T): Boolean`：接收端已析构时返回 `false` 并析构 `value`；
  - `Sender<T>.clone(): Sender<T>`：显式复制发送端，形成多生产者；
  - `Receiver<T>.receive(): T?`：阻塞到收到值；全部发送端都已析构且队列为空时返回 `null`；
  - `Receiver<T>` 不可复制。`T` 满足 `Transferable` 时，`Sender<T>` 与 `Receiver<T>` 满足
    `Transferable`；`Thread<T>` 不满足。
- **可见性**：`join` 返回时，线程中的全部写入对调用者可见；`send` 之前的写入，对收到该值的
  `receive` 之后可见。
- 任何线程中的 abort 都终止整个进程。`Shareable`、共享/别名原语、`Mutex`、原子类型、
  scoped 借用线程、线程池与完整的多线程借用数据流分析属于 v2。
- v1/v2 不定义协程；`async` / `await`、`Future`、执行器与具体 lowering 均属于 v3 设计范围，
  不能从保留字或线程 API 推断现行语义。

## IO 与网络

v1 的文件与网络 IO 采用同步阻塞模型；本版仅启用下文标准同步 IO 表面，不定义网络 API、异步 IO 或协程。
文件读取必须显式传入上限并处理 Result；标准库调用遵守普通名称、类型及所有权规则。

## 程序入口与共享所有权

### Conventional `main`

`main` 不是关键字，也不获得新的名称解析优先级。公开单文件 `kovenc build/run` 在调用者没有
显式传入 `--entry` 时，只在该源码文件所属 package 中、由该文件声明的顶层值命名空间里
选择名为 `main` 的具名函数；import 或标准库中的同名函数不是候选。合法
conventional entry 只有以下四个完整单态签名：

```kotlin
fun main(): Unit { ... }
fun main(args: Array<String>): Unit { ... }
fun main(): Result<Unit, E> { ... }
fun main(args: Array<String>): Result<Unit, E> { ... }
```

- entry 必须是顶层、非泛型、非 member 的具名函数，返回类型精确为 `Unit`，或 `Result<Unit, E>`
  且 `E` 是满足 [`Display`](03-types-generics.md#值语义能力) 的具体类型（不能是类型参数）。参数名称
  不参与签名匹配；一个参数的形式使用默认 `Borrow`，不能写 `own` 或 `inout`。返回类型不参与
  “两个合法形状同时存在”的判定：零参数与参数化各算一种形状。
- 省略 `--entry` 时，零个合法候选、存在同名但签名非法的候选，或两个合法形状同时存在，
  分别形成稳定的 missing、invalid-shape 或 ambiguous entry selection failure。它们属于构建
  操作错误，不分配语言诊断码，也不得被伪装成 Parser/type diagnostic。
- 显式 `--entry <name>` 使用 `() -> Unit` 或 `() -> Result<Unit, E>` 契约，并完全关闭
  conventional lookup；它只选择该文件声明、满足该形状的顶层函数，包括名为 `main` 的函数。
  文件内 `private` 候选保持可选；不复用 project 的 public/internal 筛选来缩窄单文件入口。
- `main(args)` 收到不含可执行文件名的命令行参数，保持原顺序。native wrapper 拥有新建的
  `Array<String>`，在 entry 调用期间建立 Borrow，并在返回后按逆序析构；源平台参数不能转换
  为合法 UTF-8 时，在调用 Koven entry 前形成 operational failure，不以替换字符静默改写。
- 退出码：返回 `Unit` 或 `Ok(Unit)` 时为 `0`。返回 `Err(e)` 时，先写出未写出的标准输出，再向
  stderr 写 `error: ` 加 `e.toString()` 加换行，然后析构 `e`，退出码为 `1`；写 stderr 失败时忽略。
  `error()`/abort 按 [Abort 输出合同](09-nullability-errors.md#abort-输出合同) 以 SIGABRT 终止。
  链接失败和启动失败沿用既有边界。v1 不允许 `main` 返回整数、`Nothing` 或异步结果，
  `exitProcess` 随 IO 与进程接口另行定义，也不定义多个 package 的全局 main 搜索。

`main(args)` 必须使用一般 `String` runtime 与真实 `Array<String>` owner；不得用 literal-only
String、Rust `String` 或宿主指针替代该 ABI。

单文件调用形式为：

```text
kovenc build <source.ko> [--source-root <dir>] [--entry <name>] -o <executable>
kovenc run <source.ko> [--source-root <dir>] [--entry <name>] [-- <program-arg>...]
```

source set、package/path 与可选 root 的唯一规则见[单文件输入](02-names-files-packages.md#单文件输入与共同名称解析)。
入口选择差异仅属于 driver 合同；一旦选中同一声明，且实际源码集合与环境等价，后续检查、
所有权与代码生成必须一致，不能借入口模式改变语言语义或丢失已有接受能力。

### `Rc<T>` 的共享所有权契约

普通 Borrow 仍是共享读取的默认方案；只有一个值必须在多个独立 owner 的生命周期中存活时
才使用 `Rc<T>`。`Rc` 是由 `TypeEnvironment` 显式绑定的 intrinsic type constructor，不按
源码拼写识别；源码同名 class 不取得任何 intrinsic 行为。

- `Rc(value)` / `Rc<T>(value)` 只有一个稳定名称为 `value` 的 `Value T` 参数。类型实参沿用
  [受控 Expected-Result 推导](11-copyability-layout-construction.md#类型实参与受控-expected-result-推导)的
  “全部显式或完全省略”与精确推导规则；构造把 operand 复制或移动进单次 heap
  allocation 内的共享 owner payload，并建立初始 strong count `1`。
- `Rc<T>` 自身始终是 `MoveOnly`，即使 `T: Copyable` 也不满足 `Copyable`。普通赋值、返回和
  Value 交付移动 handle，不增加计数；禁止把 retain 隐藏在赋值或参数传递中。
- `owner.share()` 是唯一公开的 strong-owner 分叉操作。它以 shared Borrow 使用 receiver，
  不消费或修改 payload，返回指向同一 control block 的新 `Rc<T>`，并把非原子 strong count
  增加一次。它不是用户可覆盖的一般 member，也不能由同名源码函数冒充；计数溢出必须在
  写回前 abort，不能环绕。
- `owner.value` 是 compiler-bound、只读的 payload place。它建立受 owner 生命周期约束的
  shared Borrow；不能成为 `&` 实参、赋值目标或 MoveOnly 的 owned 读取来源。若 `T` 满足
  `Copyable`，既有 Copyable 规则可以从该 shared place 产生普通副本，但不因此复制 `Rc`
  handle。v1 不提供 `getMut`、interior mutability 或从共享 payload 移出值的后门。
- 每个 live `Rc` handle 在自己的 ASAP drop point 自动递减 strong count；归零的 handle 按
  `T` 的递归 drop glue 精确析构一次 payload，再释放整个 control block。用户不可调用
  `retain()`、`release()`，也不可读取或修改计数。
- `Rc<T>` 对所有 `T` 恒不满足
  [`Transferable`](10-ownership-borrowing-drop.md#transferable)。v1 不引入
  `Arc<T>`、`Shareable`、`Weak<T>` 或跨线程共享；strong cycle 因而可能不释放，避免环必须由
  程序数据模型承担，后续版本在定义 `Weak` 前不得声称已解决 cycle。

`share()` 和 `value` 是 Rc intrinsic surface，只为封闭 Rc frontend/runtime，不等价于提前实现一般
instance receiver、属性 getter 或 operator overloading。typed 产物必须发布稳定 intrinsic
operation identity、receiver/payload 类型和 Borrow/Value effect；所有权阶段消费这些 facts，
backend 不得按成员名称字符串重新推导语义。

### Arena/Handle 的边界

Arena 是对 Rc 的互补方案而不是别名：它适合 AST、IR 等整批同生命周期对象图，以一个 arena
owner 持有全部对象，引用使用 handle/index，arena 析构时批量释放，从而避免逐边 retain。
但是源语言若要安全暴露 `Arena<T>`，必须先定义 handle 与特定 arena 实例绑定的 identity、
跨 callable 逃逸和 arena 析构后的失效规则。v1 当前没有足够的生命周期参数或 generative
identity 表达这些约束，因此现行标准库不发布 `Arena` API，也不授权用无检查裸指针实现。
编译器内部 Rust arena 不受此源语言 API 门禁影响。

## `String`

### 值、所有权与 UTF-8 不变量

`String` 继续使用 `TypeEnvironment` 显式绑定的 builtin identity，不由源码名称或 LLVM 布局
识别。它表示一段不可变、长度明确且始终合法的 UTF-8 字节序列；内容允许为空，也允许包含
U+0000，对外语义不依赖 NUL 终止。

- `String` 始终是 `MoveOnly` 且满足 `Transferable`，不满足 `Copyable`。赋值、返回、Value
  delivery 与 move closure capture 转移唯一 owner；它们不得隐式复制字节、retain 或建立共享
  control block。默认 Borrow 参数只在调用期间读取同一值。
- 普通无 interpolation 的 String literal 是一般 `String` 表达式，不再只对 `println`/`error`
  生效。实现可以让不可变 literal 引用静态只读字节，也可以在不改变可观察语义时消除临时
  allocation；SSA 类型、MoveOnly 状态与 drop obligation 仍必须与动态 String 保持同一契约。
- 动态 String 的 live owner 在既有 ASAP drop point 释放自己拥有的存储。静态 literal 不得被
  `free`；动态 owner 必须精确释放一次。具体 provenance/layout 由 String runtime ABI ADR
  决定，frontend 不发布 pointer、capacity 或 allocator 事实。
- `String?` 遵守一般 nullable 类型规则，但 pointer-like nullable lowering 不适用于 String 的内联
  runtime value；`String?` native ABI 继续等待独立 inline-nullable 设计。

### 封闭的最小操作

String 的既有核心操作如下；本版新增表面见[文本字节与严格解析](#文本字节与严格解析)：

- `left + right` 在左到右各求值一次后，以同步 shared-read 方式读取两个 `String` operand，
  产生内容为精确字节拼接的新 `String` owner；不消费具名 operand。长度加法、目标布局或
  allocation size 无法表示时在发布部分结果前 abort。
- `text.clone(): String` 是仅绑定 builtin String identity 的零参数、零类型实参 intrinsic。
  receiver 以调用期 shared `Borrow` 求值一次，不消费或修改源；源 owner 在调用后仍可用。
  结果包含完全相同的 UTF-8 字节，是参与普通 ASAP drop 的独立新 String owner；非空结果
  深拷贝到独立缓冲区，即使源来自静态 literal 也不与它共享 owner 或 buffer。空串结果可
  使用 canonical empty storage，但仍有独立的逻辑 owner obligation。
  clone 不执行 retain、不建立共享 control block，不改变 String 的 MoveOnly/Transferable。
  String 元素 place（例如 `list[0]`）可以作为 receiver，元素 owner 留在容器中。
  `String?` 必须先按既有规则获得非空 receiver，不新增 nullable clone 特例；其他 builtin、
  容器、Box、Rc、用户类型不因此获得 clone 能力。普通源码同名 member 仍按其自身契约处理，
  不能冒充此 intrinsic。显式 clone 不触发隐式 Copyable 大值复制警告，不是 const 操作。
- `==` / `!=` 比较 UTF-8 字节长度和全部内容，不执行 Unicode normalization、locale folding
  或 grapheme 处理。由于所有 String 均满足 UTF-8 不变量，字节相等与 Unicode scalar 序列
  相等一致。
- String 可以存入局部变量、作为普通 Value/默认 Borrow 参数、从函数返回、进入 closure
  capture，以及作为已经支持 drop glue 的 aggregate、enum、`Rc` 和顺序容器元素。现有
  ownership、loan、单态化和容器 relocation 规则不获得 String 特例。
- 标准 `println(value: String): Unit` 对任意 String Borrow 写出内容的全部字节，再写一个
  ASCII LF；内容中的 U+0000 不截断。短写或不可恢复的 stdout 失败沿用现有 abort 边界。
- 标准 `error(message: String): Nothing` 必须先按普通求值/借用规则形成 message，再进入既有
  abort effect；输出与失败边界遵守 [Abort 输出合同](09-nullability-errors.md#abort-输出合同)。

除下文明确启用的文本表面外，不发布 length、任意索引、builder、用户构造器、可变 buffer、
intern 或隐式共享。Display 与字符串插值沿用 [Guide03](03-types-generics.md#值语义能力) 和
[Guide04](04-expressions-operators.md) 的已启用规则；实现不足必须明确拒绝，不能用旧禁令取消规范。

### 运行时和编译阶段边界

- frontend 只发布 builtin String identity、plain-literal bytes、既有 binary/call identity、稳定
  String clone operation identity、receiver 类型及 Borrow/Value effect、Copyable/Transferable
  与 ownership facts。SSA 必须使用专用 String owner/type/operation，LLVM
  不得按源码拼写或把 Rust/C 字符串对象直接塞入 Koven value。
- runtime 的字节指针、长度、存储 provenance、drop glue、concat、clone、equality 与 stdout adapter
  必须由 [ADR-0018](../adr/accepted/0018-string-owner-runtime-abi.md) 与其
  [clone 增量 ADR-0027](../adr/accepted/0027-explicit-string-clone-abi.md) 统一；内部 ABI 不承诺公共 C FFI 稳定性，也不得要求新增 workspace crate。
- 所有创建边界都必须保证合法 UTF-8。plain literal 由 Lexer/decoder 保证，concat 由两个合法
  operand 闭包保证；argv 入口必须在创建 Koven String 前验证宿主参数，失败作为
  operational failure，不使用替换字符。
- runtime 必须使用真实 target `DataLayout` 和集中分配/abort 边界。不得以 host `usize`、
  `std::string::String` 布局或目标 C `char *` 的偶然表示代替 target-independent SSA 契约。

## Project 与 Process Entry

### Project Mode 与 Entry Selector

公开 project mode 使用与单文件位置参数不可混淆的固定形式：

```text
kovenc build --project <project.toml> --entry <qualified-name> -o <executable>
kovenc run --project <project.toml> --entry <qualified-name> [-- <program-arg>...]
```

- `<project.toml>` 必须显式提供并遵守
  [project source-set loader](../adr/accepted/0022-minimal-project-manifest-source-discovery.md)；CLI
  不从 cwd、源码路径或祖先目录
  搜索 manifest，也不按参数是文件还是目录猜测模式。现有 `build/run <source.ko> ...` 单文件
  形式保留；其 source set/import/root 遵守[单文件 Unit 规则](02-names-files-packages.md#单文件输入与共同名称解析)，
  入口选择继续遵守本页的 [conventional `main`](#conventional-main) 兼容合同。
- project mode 强制 `--entry`，不扫描整个 unit 寻找 `main`，不读取 manifest target/entry
  默认值，也不猜默认 package。`main` 在 project mode 仍只是普通函数名。
- selector 是一个或多个点分 Koven Identifier：最后一段是顶层函数名，之前各段是绝对 package；
  单段 selector 精确表示默认 package 的函数。它不经过当前文件 import，不接受 alias、wildcard、
  root identity、logical file path、类型 member 或 overload signature 文本。不能按该 grammar
  解析的 selector 是 CLI usage error，不进入 package lookup。
- selector 在完整、validated compilation unit 的 package/declaration index 上解析。entry 必须是
  有 body、顶层、非泛型的 `public` 或 `internal` 具名函数；顶层 `private` 只具有 source-unit identity，
  不能由 project selector 绕过可见性或用文件路径消歧。

### Process Shape 与选择失败

project selector 允许与 conventional main 相同的 process shape：

```kotlin
fun start(): Unit { ... }
fun start(args: Array<String>): Unit { ... }
fun start(): Result<Unit, E> { ... }
fun start(args: Array<String>): Result<Unit, E> { ... }
```

- 零参数与参数化 shape 精确复用 [conventional `main`](#conventional-main) 与
  [参数化 process entry bridge](../adr/accepted/0019-parameterized-process-entry-bridge.md)：返回
  `Unit` 或 `Result<Unit, E>`，参数化形式只有一个默认/shared
  Borrow `Array<String>` 参数；参数名不参与匹配。generic、`own`/`inout`、其他参数或返回类型
  都不是合法 process entry。
- 先按 selector 找到目标 package 的同名 declaration/overload set，再过滤可见、合法 shape。
  不存在目标、只有 private 目标、存在目标但没有合法 shape、存在多个合法 shape 分别形成
  missing、inaccessible、invalid-shape、ambiguous project-entry operational failure。一个合法
  shape 与任意数量非法 overload 共存时选择该唯一合法 shape。
- 这些失败不分配 `Ldddd`。所有 source/package/import/name/type/ownership 诊断必须先完成并按
  unit 规则发布；只有 validated unit 才进行 entry selection。CLI 把选中的 `DeclarationId` 和
  process shape 交给 codegen，SSA/LLVM/linker 不按字符串重新查找。
- project 显式 selector 支持两个 process shape；这不改变单文件显式 `--entry <name>` 的
  零参数-only 兼容契约。`kovenc run -- ...` 的 argv 排除 executable name、UTF-8 预检、顺序、
  Borrow Array/String owner 与析构继续精确复用 [conventional `main`](#conventional-main) 的
  argv bridge。

### 产物、失败原子性与阶段边界

- `build` 仍要求显式 `-o` 并拒绝已经存在的最终路径；object 和 linker output 使用输出目录内
  的唯一临时路径，只有 codegen、link 与最终 no-clobber commit 全部成功才发布 executable。
  manifest、任一 source、object、临时 executable 与 final 不能重合。失败清理本次临时产物，
  不删除/覆盖调用者已有文件。`run` 继续使用进程拥有的临时目录并在
  子进程结束后清理。
- 最终调试产物按 [ADR-0035](../adr/accepted/0035-final-debug-artifact-publication.md)：macOS
  默认 build 交付 executable 及追加 `.dSYM` 后缀的匹配调试包；Linux 保留 ELF 源码行表。
  已有任一最终路径（含悬空符号链接）均拒绝，发布不得覆盖他人产物。
  调试包先发布、executable 最后发布，两者均就绪才成功。若调试包已发布而 executable
  发布失败，返回 operational error 并报告遗留调试包位置，允许该部分产物保留。
  不递归删除可能被外部替换的公开目录；私有临时产物仍清理，清理失败明确报告。
  此为失败清理的明确例外，不将双路径发布宣称为单一原子事务。
- manifest/provider、entry/link/launch/cleanup failure 是具体 operational error；frontend
  diagnostics 继续遵守 human/JSON Lines 选择，项目错误不得伪造成语言 diagnostic。成功 build
  stdout/stderr 为空；run 继续转发程序 stdout/stderr。程序结束后 `kovenc run` 以子进程的退出
  状态结束：子进程以退出码 n 正常退出时 `run` 也以 n 退出，被信号终止时以 `128 + 信号编号`
  退出；构建失败使用既有的 operational/diagnostic 退出码，不与上述取值混用。
- project source-set loader 产生 base source set，跨文件名称/类型/所有权分析形成 validated unit，
  compilation-unit native lowering 生成单 object；不得用拼接源码、逐文件 object 或单文件
  bootstrap 循环替代该链。

### 只检查与帮助

```text
kovenc check <source.ko> [--source-root <dir>]
kovenc check --project <project.toml>
kovenc help | --help | -h
```

- `check` 使用与对应 `build` 相同的 source set、名称、类型、最终类型准入与所有权检查，
  不选择入口、不生成 object、不链接，也不写任何文件；诊断按 `--message-format` 以与 `build`
  相同的 human/JSON Lines 形式写 stderr。
- `check` 退出码：无诊断为 `0`，存在 frontend 诊断为 `1`，usage 或 operational 错误为 `2`。
  单文件与 project 的 `build`/`run` 遇到 frontend 诊断同样以 `1` 结束。
- `help`、`--help`、`-h` 把完整命令用法写到 stdout 并以 `0` 结束。

本节不定义 manifest target/default entry、依赖解析、lock、跨 compilation-unit import/ABI、
多 object、library artifact、安装/发布、cross target、缓存或全项目 conventional main。无依赖
dependency-aware build 不属于现行语言，必须等待独立 manifest、ABI 与构建规范。

---

## 标准所有权原语与字符串分层

### 标准原子所有权原语

为了支持所有权安全流转与链表、状态机等数据结构的高效在位更新，标准库顶层提供经编译器特判的原子置换原语（见[所有权、借用与析构规则](10-ownership-borrowing-drop.md)）：

```kotlin
fun <T> replace(inout place: T, own new: T): T
fun <T> swap(inout a: T, inout b: T): Unit
```

- `replace(&place, new)`：在独占借用保护下，原子将 `new` 存入 `place` 并以 owned 值返回原有旧值，完全避免未初始化空洞；
  返回类型只写 `T`，`own` 是参数模式，不是返回类型修饰符；
- `swap(&a, &b)`：在两个互不重叠的可变借用之间原子交换值。

### 静态字符串类型的延后边界

普通字符串字面量与动态字符串统一具有 builtin `String` 类型和唯一 owner obligation，
均为 MoveOnly 且满足 Transferable。引用静态只读字节是
[String runtime ABI](../adr/accepted/0018-string-owner-runtime-abi.md) 的存储优化：
静态字节可随进程存活且不执行 `free`，但 String 值仍遵守普通 owner 生命周期，不因此满足 `Copyable`。
需要保留源并取得独立 owner 时使用上文[封闭操作](#封闭的最小操作)定义的 `String.clone()`；
`println` / `error` 继续借用 String。独立静态 `Str` 与 Str→String 转换继续延后，
不得从后续切片设计或底层静态字节表示推导已启用的类型、ABI 或能力。

---

## 受控底层能力的边界

v1 的用户源码中没有 `unsafe`、`extern` 与裸指针；标准库如何借助 runtime 实现属于实现细节，
不是语言承诺。原 `extern "C"`、`RawPtr<T>` 与 `unsafe { }` 草图移至
[unsafe 与 FFI 边界候选](../proposals/unsafe-and-ffi-boundary.md)，属于 v2 评审范围。

## 文本字节与严格解析

文本函数、TextError、ParseError 与 String 扩展由 `koven.text` 显式导入；
File、Directory、Path、stdin/stdout/stderr、IoError、DirectoryEntry 由 `koven.io` 显式导入。
这些名称不新增关键字，也不绕过标准源码的可信身份与普通可见性。


默认参数为 Borrow；表中所有 String、Array 返回值都是独立 owner，不借用 receiver 的存储。
采用 UTF-8 **字节偏移**，避免把 Unicode scalar、字节和用户感知字符混为同一种索引。
所有外部字节严格解码；不替换非法序列、不正规化、不做 locale 处理。

| 公开接口 | 合同 |
|---|---|
| `text.byteLength(): Int` | UTF-8 字节数，范围为 0 至 2147483647；创建超额按下文入口规则失败 |
| `text.toBytes(): Array<UByte>` | 精确复制全部字节，包括 U+0000；源仍有效 |
| `decodeUtf8(bytes: Array<UByte>): Result<String, TextError>` | 验完整 UTF-8；首个非法字节偏移随错误返回；成功复制为新 String |
| `text.sliceBytes(start: Int, end: Int): Result<String, TextError>` | 半开区间；0≤start≤end≤byteLength 且两端均在 scalar 边界；空区间有效 |
| `text.split(separator: String): Result<Array<String>, TextError>` | 精确字节分隔、保留空字段；空分隔符报错，不提供隐式逐字符模式 |
| `text.lines(): Array<String>` | LF 分行，紧邻 LF 的 CR 一并移除；其他 CR 保留；空输入无行，末尾换行不额外产空行 |
| `parseLong(text: String): Result<Long, ParseError>` | 十进制 ASCII，可选一个正负号，至少一位；不 trim、不接收分隔符／Unicode 数字；越界返回错误 |

`TextError` 分类为 InvalidUtf8(offset)、InvalidRange、NotScalarBoundary、EmptySeparator；
`ParseError` 为 InvalidDigit(offset)、MissingDigits、OutOfRange。偏移均从 0 开始计字节。
`InvalidUtf8(offset)` 指向第一个无法完整解码的 UTF-8 序列的起始字节：
`41 E2 28 A1` 与截断的 `41 E2 82` 均返回 1；孤立 continuation byte 指向自身。
`parseLong` 先验证完整词法形状，再检查范围：空串及单独 `+`/`-` 为 MissingDigits，
其他非法字符返回首个 InvalidDigit 的字节偏移；只有词法合法的数字才可能 OutOfRange。
因此 `9223372036854775808x` 返回 InvalidDigit(19)，`x` 返回 InvalidDigit(0)。

Long 的十进制格式化复用既有 Display 合同，不新增竞争的格式化协议。
`lines`／`split`／解析业务循环优先写在 Koven；只有所需存储读取及 UTF-8 检验边界进入受控 runtime。

分配失败仍遵循现行 Abort 合同，不声称 Result 能恢复 OOM。外部长度先验 Int 上限；
已经存在的 String owner 不得因本建议被悄悄截断。实现必须在所有 String 创建边界检查上限；内部长度继续使用 target usize，不以缩窄 ABI 或截断代替检查。

### String 长度与可恢复连接

所有 live String 的 UTF-8 字节数不超过 Int.MAX_VALUE（2147483647），保留 target usize 内部布局。
字面量超限是编译诊断；argv 超限是在进入 main 前的 operational failure；有显式上限的 IO
超限返回 LimitExceeded；动态 `+` 超限在发布结果前 Abort。不能仅在 byteLength 处截断或转换。
`text.checkedPlus(other: String): String?` 以调用期 shared Borrow 各求值一次、保持两输入 owner，
返回精确拼接的新独立 String owner；仅长度超限返回 null，分配失败仍 Abort，不承诺恢复 OOM。
成功结果与 `+` 的内容和所有权一致。该表面不增加 Result 连接或 TextError.LengthExceeded。
插值的拼接与其他无 Result 的动态 String 创建同样遵守上限；可恢复拼接使用 checkedPlus。
Path.join 若结果长度超限返回 IoError.LimitExceeded。解码输入 Array 的 size 已受 Int 上限约束；
不能因内部 target usize 可表示更长数据而发布违反上限的 String。clone 不扩大已有长度。

### 同步 IO 与资源责任

本版定义下列便利文件和标准流表面；显式流 owner 与 close 仍是后继合同，不能由便利函数取消。
同步阻塞，不隐式创建线程。路径为 UTF-8 String，拒绝内嵌 NUL；不展开 `~`、环境变量或通配符。
相对路径以进程工作目录解释；不规范化大小写，不承诺沙箱隔离或消除检查与使用之间的竞争。

| 公开接口 | 合同 |
|---|---|
| `File.readBytes(path: String, maxBytes: Int): Result<Array<UByte>, IoError>` | 非负显式上限；循环短读到 EOF，超过上限报 LimitExceeded，不返回截断成功 |
| `File.readText(path: String, maxBytes: Int): Result<String, IoError>` | 复用 readBytes 和严格 UTF-8；非法输入为 InvalidEncoding，不静默替换 |
| `File.writeBytes(path: String, bytes: Array<UByte>): Result<Unit, IoError>` | 创建或截断普通文件，循环短写；失败可能留下前缀，不承诺原子替换／持久化 |
| `File.writeText(path: String, text: String): Result<Unit, IoError>` | 精确 UTF-8 写出，不自动加换行；资源行为同 writeBytes |
| `Directory.list(path: String, maxEntries: Int): Result<Array<DirectoryEntry>, IoError>` | 非递归；排除 . 与 ..；超过显式非负上限报错；结果本身无顺序保证 |
| `Path.join(parent: String, child: String): Result<String, IoError>` | child 必须单个非空文件名，不含路径分隔符且不是 . 或 ..；不提供安全目录遍历保证 |
| `stdout.write(text: String)`／`stderr.write(text: String)` | 返回 Result<Unit, IoError>；write-all，不自动加换行；不改变现有 println 行为 |
| `stdout.flush()`／`stderr.flush()` | 返回 Result<Unit, IoError>；把本库缓冲交给宿主，不等于磁盘 fsync |
| `stdin.readLine(maxBytes: Int): Result<String?, IoError>` | EOF 前无字节为 null，空行为空 String；LF／CRLF 去终止符；EOF 的剩余字节仍返回一行 |

IoError 携带稳定分类、操作和 owned 路径／描述：InvalidInput、NotFound、PermissionDenied、
InvalidEncoding、LimitExceeded、Interrupted、Other；不把宿主 errno 数值或系统文案作为跨平台 oracle。
readLine 上限按终止符之前的原始字节计；超额／编码错误后 stdin 状态未恢复，不保证下一次调用续行。
宿主 EINTR 在无进展时重试，已读写部分保留进度；无法重试的中断才返回 Interrupted。

便利文件 API 在所有正常返回路径关闭自己打开的句柄；先操作失败再关闭失败时保留操作错误，
操作成功但 close 失败时返回 close 错误。关闭失败不得无条件重试，以免误关已复用句柄。
标准流不归调用者所有，不关闭宿主标准句柄。显式可关闭流 owner 留给独立后继合同，
须定义 close 幂等性、drop 的尽力清理及不可观测错误；本合同不从 RAII 推导“close 必成功”。

DirectoryEntry 有 owned name、kind（File／Directory／Symlink／Other）、`size: Long?`；
size 只对普通文件存在，无法表示的宿主大小返回错误。读取元信息不跟随链接；条目消失、
不可读或不可表示的文件名使整个调用失败，已创建 entry owner 正常释放，不输出部分成功。
列表不是目录原子快照；入口目录及文件打开仍遵循宿主符号链接行为，不能宣称禁止跟随所有链接。
