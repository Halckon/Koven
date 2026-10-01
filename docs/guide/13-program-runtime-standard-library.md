# Koven v0.38：程序入口、Runtime 与标准库

> **性质**：规范性语言规范 · **状态**：current（v0.38） · **读取时机**：实现或评审 main、project、String、Rc、IO、并发或标准库边界时 · **唯一真源**：本页

本页是现行 Koven v0.38 规范的一部分。规则正文优先于示例；未在本页定义的相邻概念通过链接转交给对应领域页面。

## 线程、Channel 与 Move Closure

```kotlin
val handle = thread(move { println("running in new thread") })
handle.join()

val (sender, receiver) = channel<Int>()
thread(move { sender.send(42) })
val value = receiver.receive()
```

`thread()` 的函数类型参数是 `move () -> Unit`，不是普通 `() -> Unit`。函数类型的 `move`
前缀表示只接受不含借用捕获的闭包；调用点必须显式使用 `move` closure，并由编译器检查全部
捕获满足跨线程转移约束。所有跨线程 API 的 closure 参数都必须使用 `move (...) -> T` 契约。

- 配合 `Transferable` 标记能力做编译期数据竞争防护（跨线程传递的值必须满足该约束，具体
  判定规则见[所有权规则](10-ownership-borrowing-drop.md)）。v1 的跨线程 API 只转移所有权，
  不提供共享/别名原语，也不做完整的多线程借用数据流分析；`Shareable` 属于 v2。
- v1/v2 不定义协程；`async` / `await`、`Future`、执行器与具体 lowering 均属于 v3 设计范围，
  不能从保留字或线程 API 推断现行语义。

## IO 与网络

```kotlin
val content = File.readText("path/to/file")
File.writeText("out.txt", "hello")
```

- v1 的文件与网络 IO 只采用同步阻塞模型；异步 IO 和协程均未定义。泛型型变、反射、
  顺序容器、运算符与诊断分别由对应领域页定义，本页不复制其规则。

## 程序入口与共享所有权

### Conventional `main`

`main` 不是关键字，也不获得新的名称解析优先级。公开单文件 `kovenc build/run` 在调用者没有
显式传入 `--entry` 时，只在该源码文件的顶层值命名空间中选择名为 `main` 的具名函数。合法
conventional entry 只有以下两个完整单态签名：

```kotlin
fun main(): Unit { ... }
fun main(args: Array<String>): Unit { ... }
```

- entry 必须是顶层、非泛型、非 member 的具名函数，返回类型精确为 `Unit`。参数名称不参与
  签名匹配；一个参数的形式使用默认 `Borrow`，不能写 `own` 或 `inout`。
- 省略 `--entry` 时，零个合法候选、存在同名但签名非法的候选，或两个合法形状同时存在，
  分别形成稳定的 missing、invalid-shape 或 ambiguous entry selection failure。它们属于构建
  操作错误，不分配语言诊断码，也不得被伪装成 Parser/type diagnostic。
- 显式 `--entry <name>` 使用 `() -> Unit` 契约，并完全关闭
  conventional lookup；它可以选择任意满足该形状的顶层函数，包括名为 `main` 的函数。
- `main(args)` 收到不含可执行文件名的命令行参数，保持原顺序。native wrapper 拥有新建的
  `Array<String>`，在 entry 调用期间建立 Borrow，并在返回后按逆序析构；源平台参数不能转换
  为合法 UTF-8 时，在调用 Koven entry 前形成 operational failure，不以替换字符静默改写。
- 正常返回仍由既有 C ABI wrapper 映射为退出码 `0`；`error()`/abort、链接失败和启动失败
  沿用既有边界。v1 不允许 `main` 返回整数、`Result`、`Nothing` 或异步结果，也不定义多个
  package 的全局 main 搜索。

`main(args)` 必须使用一般 `String` runtime 与真实 `Array<String>` owner；不得用 literal-only
String、Rust `String` 或宿主指针替代该 ABI。

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

一般 String runtime 只承接以下封闭操作：

- `left + right` 在左到右各求值一次后，以同步 shared-read 方式读取两个 `String` operand，
  产生内容为精确字节拼接的新 `String` owner；不消费具名 operand。长度加法、目标布局或
  allocation size 无法表示时在发布部分结果前 abort。
- `==` / `!=` 比较 UTF-8 字节长度和全部内容，不执行 Unicode normalization、locale folding
  或 grapheme 处理。由于所有 String 均满足 UTF-8 不变量，字节相等与 Unicode scalar 序列
  相等一致。
- String 可以存入局部变量、作为普通 Value/默认 Borrow 参数、从函数返回、进入 closure
  capture，以及作为已经支持 drop glue 的 aggregate、enum、`Rc` 和顺序容器元素。现有
  ownership、loan、单态化和容器 relocation 规则不获得 String 特例。
- 标准 `println(value: String): Unit` 对任意 String Borrow 写出内容的全部字节，再写一个
  ASCII LF；内容中的 U+0000 不截断。短写或不可恢复的 stdout 失败沿用现有 abort 边界。
- 标准 `error(message: String): Nothing` 必须先按普通求值/借用规则形成 message，再进入既有
  abort effect；现行规范不保证 stderr 文本格式或 message 一定被打印。

本最小表面不发布 `length`、索引、slice、builder、编码转换、用户构造器、可变 buffer、
intern、隐式共享或 `toString`/formatting protocol。String interpolation 虽已有 Lexer/Parser/类型
节点，但 operand 到 String 的转换契约尚未封闭；在后续 guide 定义可打印/转换协议前，native
lowering 必须确定性拒绝 interpolation，不能只支持若干 builtin 并假装成完整协议。

### 运行时和编译阶段边界

- frontend 只发布 builtin String identity、plain-literal bytes、既有 binary/call identity、
  Copyable/Transferable 与 ownership facts。SSA 必须使用专用 String owner/type/operation，LLVM
  不得按源码拼写或把 Rust/C 字符串对象直接塞入 Koven value。
- runtime 的字节指针、长度、存储 provenance、drop glue、concat、equality 与 stdout adapter
  必须由 accepted ADR 统一；内部 ABI 不承诺公共 C FFI 稳定性，也不得要求新增 workspace crate。
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
  形式及本页的 [conventional `main`](#conventional-main) 行为完全不变。
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

project selector 允许与 conventional main 相同的两个完整 process shape：

```kotlin
fun start(): Unit { ... }
fun start(args: Array<String>): Unit { ... }
```

- 零参数与参数化 shape 精确复用 [conventional `main`](#conventional-main) 与
  [参数化 process entry bridge](../adr/accepted/0019-parameterized-process-entry-bridge.md)：返回
  `Unit`，参数化形式只有一个默认/shared
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
- manifest/provider、entry/link/launch/cleanup failure 是具体 operational error；frontend
  diagnostics 继续遵守 human/JSON Lines 选择，项目错误不得伪造成语言 diagnostic。成功 build
  stdout/stderr 为空；run 继续转发程序 stdout/stderr 与可表示的退出状态。
- project source-set loader 产生 base source set，跨文件名称/类型/所有权分析形成 validated unit，
  compilation-unit native lowering 生成单 object；不得用拼接源码、逐文件 object 或单文件
  bootstrap 循环替代该链。

本节不定义 manifest target/default entry、依赖解析、lock、跨 compilation-unit import/ABI、
多 object、library artifact、安装/发布、cross target、缓存或全项目 conventional main。无依赖
dependency-aware build 不属于现行语言，必须等待独立 manifest、ABI 与构建规范。

---

## 受控底层能力与 Unsafe 边界

Phase 5 要求标准库（`koven/**/*.ko`）以目标语言自身编写并作为自举真源。为满足内存分配、系统调用与底层数据结构（如 `Vector`、`String` 底层 buffer）的实现需求，Koven 定义以下受控底层能力：

### 受控原语与内部边界

1. **`extern "C"` 外部函数声明**：
   允许在标准库内部声明直接绑定到平台 libc 或 native runtime 的底层函数：
   ```kotlin
   extern "C" fun malloc(size: Int): RawPtr<Unit>
   extern "C" fun free(ptr: RawPtr<Unit>): Unit
   ```
   外部函数必须在编译期验证符号与 C ABI 兼容性，不参与普通 Koven 所有权与生命周期自动析构。

2. **`RawPtr<T>` 裸指针**：
   `RawPtr<T>` 是标准库内部持有的非安全内存地址抽象，不拥有所有权，不保证内存有效性或非空不变性，不执行自动 drop。解引用、偏移运算或地址转换必须在显式 `unsafe` 块内执行。

3. **`unsafe { ... }` 表达式/块**：
   `unsafe` 块作为危险操作的隔离边界，用于显式标记并确认包含以下操作：
   - 调用 `extern "C"` 声明的外部符号；
   - 对 `RawPtr<T>` 进行读取、写入或内存偏移；
   - 在已验证内存布局上执行类型跨越式在位强制转换。

### 安全封装与权限隔离

- **标准库特权**：上述底层能力属于受控特权，仅限在标准库内部模块（`koven.*` 命名空间及指定 runtime 桥接层）中使用。
- **普通用户代码隔离**：在 v1 阶段，应用层用户源码禁止直接使用 `RawPtr<T>` 或自行定义 `unsafe` 块；编译器对非特权 package 的裸指针访问与未受控外部调用拒绝并报结构化诊断。
- **安全不变量封装**：标准库通过 RAII、所有权（`own` / `borrow`）与类型系统向外暴露完全安全的高层抽象（如 `Box<T>`、`Array<T>`、`String`），确保 unsafe 实现的边界在标准库内部完全闭合。
