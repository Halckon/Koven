# Koven v0.37 语言设计审计

## 总判断

v0.37 的规范在「解析与恢复的确定性」上精确到过度，但语义内核缺了几块承重墙。v1 选的「owned value + 调用期 loan」本身自洽，问题是它只是 second-class reference 体系的**前半段**。后半段被整体推给 v2：借用返回/投影、原地重置原语、用户 drop、unsafe。标准库、集合、迭代、字符串、并发都压在这后半段上。

规范内部其实已经在用长于一次调用的借用，却没有统一机制，只能一处一个特例：

- borrowed closure 的 loan 活到闭包的 drop 点（07）
- `for` 的元素绑定与隐藏 provider（12 §37）
- `Borrow temporary` 延寿（10）

我的标尺是「能否用这门语言写出并维护一批真实的无 GC 程序，含标准库和编译器自身」，不是文档是否自洽。下面 🔴 有 7 项，但根因只有三个：

- **借用/所有权模型不完整**（3、4、7）
- **缺底层层**（5、6）
- **语法/文本硬伤**（1、2）

值得保留、不建议动的方向：

- abort-only，drop 没有展开边
- 调用点 `&` 标注 Inout
- 结构化 capability 推导，加 `TypeEnvironment` 绑定的 compiler-bound 身份
- 单态化静态分发
- 显式 `Rc.share()`
- checked 算术

------

## 🔴 结构性缺陷

**1. 块内写不出两条连续的表达式语句**

- **证据**：
  - 06 规定块内换行和 `;` 都不是分隔，`{ x y }` 明确不是两条语句。
  - 07 写明 `{ val x = 1 x }` 是错误，并声明「不承诺任意局部声明序列 + tail expression」。
  - 按字面推导，`println(a)` 换行 `println(b)` 就是尾随 token 错误。
  - 08 自己的 `super<Logger>.log(msg)` / `super<Auditor>.log(msg)` 示例却依赖这一能力。
  - 顶层、`when`、类体各用换行/`;`，唯独块不用，等于三套分隔策略。
- **后果**：我相信这不是本意，但说明规则是从「恢复确定性」倒推的，不是从「能写出正常程序」正推的。「换行是纯 trivia」还有 JS 式隐患：`val a = x` 换行 `-y` 会静默变成 `x - y`，换行 `(g).run()` 会变成 `x(g).run()`。
- **建议**：改成 Kotlin/Swift 式换行敏感规则。换行结束语句，除非行末停在二元运算符、`,`、`(` 这类未完结构，或下一行以只能作延续的 token 开头（`.` `?.` `?:` `&&` `||` `as` `is` `in` `else`）。块内允许 `;` 作显式分隔。这条规则还能删掉 06/07 里「最大 element / structural stop / soft stop」那一整套补丁。

**2. `value` 既是硬关键字，又被当标识符用**

- **证据**：01 把 `value` 列为硬关键字，且 `.` 后必须是普通 Identifier。但以下地方都在用它：

  - 13 的 `Rc(value)`、`owner.value`、`println(value: String)`
  - 09 的 `__tmp.value`
  - 07/08/10 的 `own value: T`
  - 08 开篇的 `class Node(var value: Int, …)`

  11 甚至专门绕开它，把 `Ok(value: T)` 改成 `Ok(success: T)`，理由就是它不可解析。同类问题：`move`、`own`、`borrow`、`inout`、`loop` 也是硬关键字，`player.move()`、`cell.borrow()` 都写不了。

- **建议**：修饰符类一律做成上下文关键字（Kotlin 的 `value`/`inline` 就是这样）。只有声明起点的 `value class`、参数位的 `own/borrow/inout`、类型/lambda 前缀的 `move` 才解释为关键字，其余仍是 Identifier。你们的 parser 已有严格试探能力，成本很低。

**3. 借用只活一次调用，这是半个 second-class reference 体系（根因）**

- **现状**：没有可存储的 borrow，没有借用返回，局部变量不能是借用（10）。后果几乎全落在 API 层：
  - 没有 `get/first/peek`，也没有 `Map.get/entry`。
  - 私有 `String` 字段配 getter 会撞 L0133，而 15 又不支持自定义 getter，所以「封装」与「所有权」直接冲突。
  - 局部不能借用，所以 `class Node(var next: Node?)` 无法迭代遍历（`cur = cur.next` 是部分移动），只能递归，长链表会栈溢出。
  - 没有 slice/view，`Array`/`List`/`MutableList` 是三个互不转换的名义类型（12），连「接受任意顺序容器的只读函数」都写不出。
  - `for` 只认三种编译器绑定容器（12 §37.1），用户容器永远比内建容器低一等。
- **建议**（避开 Rust 式生命周期，符合「比 Rust 低的标注负担」）：
  1. 把 **`Escapable`** 加为第四个结构化 capability，与 `Copyable`/`Transferable` 走同一套推导。含借用的类型（`Ref`、`Span`、迭代器、带借用捕获的闭包）自动 non-Escapable。这就是 Swift 的 `~Escapable` / Hylo 的 second-class 思路。
  2. 允许 `borrow`/`inout` 返回，来源默认绑定到唯一的借用形参，多个时才显式标注。结果可绑到 `val`，活到最后一次使用（直接复用你们已有的 ASAP liveness），但不能存字段、不能逃逸。
  3. 用同一机制收编三处特例：
     - L0137 变成「non-Escapable 值逃逸」的实例。
     - `for` 变成「迭代器是 non-Escapable 值」，`Iterator`/`Collection` 协议随之可由用户实现。
     - `Rc.value`、下标 place 都是 projection。
  4. 提供标准 view：`Span<T>`、`StringView`。
- **判断**：Map 被推迟，其实不是 Map 的问题，是这个机制没定。`Map.get` 是它最好的试金石。参考：Swift `~Escapable` + `Span`、Hylo subscript/projection、Mojo origin。

**4. 所有权原语不全，链表、树、状态机写不出来**

- **现状**：
  - 普通字段不许部分移动，`Inout` 不许「移出后留洞」，没有 `replace/take/swap`。
  - enum payload 没有面向用户的消费式匹配，只有 `?`、`!!`、value class 局部解构三条特权通道。
  - receiver 的 exclusive loan 在第一个实参之前生效，所以 `list.add(list.size)`（`add` 必为 inout receiver）被 L0135 拒绝。Rust 为此专门做了 two-phase borrows。
- **后果**：以下这些都写不出，Phase 5 只能全靠编译器 intrinsic 兜底：
  - `head = head.next`、BST 旋转
  - `this.state = this.state.next()`
  - 对 MoveOnly payload 的 `Option.map` / `Result.mapError`
  - `swap`、对 `Array<String>` 排序
- **建议**：
  - 允许「移出后重新初始化」。对 `var` 根、`inout` 参数和字段做路径敏感的初始化状态跟踪，在观察点或退出前补齐即可（Swift 的 `consume` + 重新赋值、Hylo 的 `sink` 是同类分析）。**你们是 abort-only，洞永远不会被观察到**，这比 Rust 更容易做到 sound，是 abort-only 的红利，目前没用上。10 已要求 callee 返回时 Inout place 必须完整初始化，差的只是把它前推成流分析。
  - `replace(&place, new)`、`swap(&a, &b)`、`swapAt(i, j)` 作原子 intrinsic。`a[i]`/`a[j]` 无法证明不相交，正是 `swapAt` 存在的理由。
  - `when` 支持带所有权模式的绑定：subject 是 owned/临时则消费，是 place 则借用。
  - receiver 采用 two-phase：先求值全部实参，再激活 exclusive loan。

**5. 没有 `deinit`，而且 ASAP 与可观察析构相冲**

- **现状**：类体只允许函数和 companion（08），没有析构成员。drop glue 只递归释放内存。11 说「独占 drop 的类型不得 Copyable」，可见 drop 语义被预期存在，用户却无处声明。
- **后果**：文件、锁、事务、设备句柄这类 RAII 资源写不出，`Sender/Receiver` 只能是内建魔法。将来加 `deinit` 时，ASAP 会咬人。10 规定「从未使用的 owner 在初始化后立即析构」，于是 `val guard = mutex.lock()` 会立刻解锁。FFI 场景还有「指针比对象活得久」的经典问题（Swift 需要 `withExtendedLifetime`）。
- **建议**：
  - 加 `deinit`，仅限 MoveOnly 类型，并禁止从含 `deinit` 的类型部分移出。
  - 把析构分两类。纯内存释放按 ASAP 提前，因为不可观察。带用户 `deinit` 的类型按词法作用域末尾、声明逆序析构，行为可预测。
  - 再补 `defer` 或 `use { }`。

**6. 没有 unsafe 层，标准库无法用语言自己写**

- **现状**：`unsafe`/`extern` 是硬关键字但没有产生式。`Array/List/MutableList` 被定为「编译器预声明原语，不是 Phase 5 的 .ko API」（12），`String/Rc/Box` 同理。没有裸指针、`alloc/free`、`sizeOf/alignOf`、C 布局控制、FFI。

- **后果**：

  - 用户不能写自己的容器、arena、环形缓冲、HashMap。
  - Phase 5「用 Koven 源码实现最小标准库」只能是薄包装。
  - v4 自举（AST/IR 天然需要 arena）不可达。
  - 连 libc 的 `write` 都调不了。

- **建议**：v1 内就设计一个最小 unsafe 层，哪怕只对标准库开放：

  - `RawPtr<T>`、`alloc/dealloc/realloc`、`read/write/copy`、`MaybeUninit`、`sizeOf/alignOf`
  - `extern "C"` 与 C 布局的 `extern value class`
  - `unsafe { }`

  然后把 `Box/Rc/Array/List/MutableList/String` 迁到库里，编译器只保留少量 lang item：drop glue、索引投影、闭包 ABI、少数原子 intrinsic。这一步同时是对 3、4 的 dogfooding，库写不出来就说明内核还缺东西。标准库分层 core（无分配）/ alloc / std，为嵌入式和 freestanding 留门。

**7. 函数类型抹掉了能力与调用模式，闭包 ABI 未定**

- **现状**：
  - 函数/lambda 类型恒为 MoveOnly，连无捕获的裸函数指针也是（11），`listOf(f, f)` 非法。
  - `Transferable` 不属于函数类型。`move () -> Unit` 类型的变量「不能证明环境满足 Transferable」，只有字面量 lambda 能进 `thread`（07），线程池和任务队列写不出。
  - 闭包不能赋值捕获变量，也不能消费捕获值。没有 FnMut/FnOnce，Kotlin 最常见的 `forEach { sum += it }` 不成立。
  - 05 说「单态化为具体闭包结构体」，但 `(Int) -> Int` 是非泛型的一等类型，值布局必须统一，环境只能被擦除。env 放栈还是堆、谁释放，规范没说。escaping `move` 闭包实质是隐式堆分配，与「禁止隐式装箱」矛盾。
- **建议**：函数类型携带 capability 限定与调用模式，即 Rust 的 `Fn/FnMut/FnOnce + Send + 'static` 那一维：
  - 默认借用调用。
  - `inout (A) -> B` 允许改捕获，`once` 允许消费。
  - `Copyable`/`Transferable`/`Escapable` 可作限定（现在的 `move` 前缀就是 Escapable 的旧拼写）。
  - 闭包 ABI 定为 `{code, env, envDrop}`：借用闭包的 env 在栈上（fat pointer，天然 non-Escapable），escaping 闭包的 env 在堆上，并把这次分配写进规范。
  - 允许函数类型作泛型上界（`<F : (Int) -> Int>`）走静态分发、可内联，Kotlin `inline` 想解决的就是这个。

------

## 🟡 重要缺陷

**8. Copyable/Transferable 纯结构推导，且无 opt-in/out**（11）

- **问题**：
  - 远距作用：给深层类型加一个 MoveOnly 字段，无关代码里的 `val b = a` 会从复制悄悄变成移动，报错出在没改的地方。
  - 公共 API 的可复制性随私有字段漂移。
  - 大 value class 静默 memcpy。
  - 与将来的 unsafe 不兼容：`value class Vec(val ptr: RawPtr<T>)` 会被推导为 Copyable，导致 double free。这正是 Rust 把 `Copy` 做成 opt-in 的原因。
- **建议**：改成编译器校验的显式声明（`value class P(…) : Copyable`）。推导只留给非 public 类型。unsafe 层提供 `unsafe Transferable` 断言和 `!Copyable` 否定。同一框架承载 `Eq/Hash/Ord/Debug` 的 derive，取代零散的自动推导。

**9. 堆/内联/共享机制冗余且限制任意**（08/11/13）

- **问题**：
  - `class` 被注释为「引用语义」，实际是「隐式装箱的 struct + 唯一所有权」，没有别名，Kotlin 用户的引用直觉全部失效。
  - `Box` 只允许 value class，`Box<enum>` 非法，经典递归 enum（`Add(l: Expr, r: Expr)`）只能绕一层无意义的 `class` 包装。
  - `Rc` 只读、无 Weak、无内部可变，也没有 Arena（13 自己承认无法安全暴露）。父指针、双向链表、观察者、图/IR 一个都建不了。
- **建议**：
  - 把「堆还是内联」从声明关键字下放到使用点：统一一种聚合类型，加对全类型开放的 `Box<T>`（含 enum）。`class` 若保留，只承载「有 identity / 有 deinit 的资源型」语义。
  - 共享可变分三层：`Cell<T: Copyable>`；`Rc<RefCell<T>>` 用 `with { }` 回调式 API（不需要借用返回）；图/IR 用标准库里的 **generational arena**（`Copyable` 的 `Id<T>` = 索引 + 代数，运行时校验，不需要生命周期参数，在 unsafe 层实现）。

**10. 抽象机制过窄，重载让规则爆炸**

- **现状**：
  - 泛型只有单个 interface/`Copyable`/`Transferable` bound，没有 associated type、多 bound、`where`。
  - 接口只能在类型自己的声明里实现，扩展函数被列为「去掉的语法糖」（15）。所以不能为 `Int/String/List<T>` 追加接口实现，库也不能为已有类型实现自己的接口。
  - 没有操作符重载，没有 `Eq/Hash/Ord/Display/Clone` 协议：用户类型的 `==` 语义未定义，插值因无 `Display` 无法降级，MoveOnly 值没有显式 `clone`。
  - 函数重载 + 单向推断 + lambda 逐候选试探（07）+ 构造器专属的 expected-result 例外（11）叠加，最坏是候选数^嵌套深度，Swift 类型检查器的老毛病。
- **建议**：
  - 加 associated type 和多 bound。
  - 加「追溯式 conformance」：`extension Int : Comparable { … }`，配孤儿规则。它是静态分发，与单态化天然契合，去掉它的复杂度理由站不住。
  - 建 `Eq/Hash/Ord/Display/Clone` 协议，配合第 8 项的 derive。
  - 收缩重载：只按 arity/label 区分，或干脆不要（Rust/Go/Zig 都没有），换取更简单的局部统一化推断。
  - 给类型检查设复杂度预算并写进规范。

**11. 数值与位级能力缺位**（01/04/12）

- **现状**：
  - 没有 hex/二进制字面量和下划线，没有位运算（`&` 被留给调用点 Inout 标注）。
  - 没有 wrapping/saturating 变体，没有数值转换族的任何定义。
  - 容器 `size`/下标被钉死在 `Int` 且 ≤ 2³¹−1，这是 Java 遗产。
- **后果**：CRC、哈希、PRNG、UTF-8 编解码、位域、二进制协议帧（如 Modbus RTU + CRC16）都写不出，而这些恰恰是标准库自己需要的。>2 GiB 的 `Array<Byte>` 不合法。
- **建议**：
  - Kotlin 式具名中缀位运算 `and/or/xor/shl/shr/ushr/inv`。它不占 `&` 记号，顺便解掉 `&` 的位与/Inout 双重身份。
  - hex/bin/下划线字面量。
  - 显式 `wrappingAdd`（或 Zig 式 `+%`）等变体。
  - 加 `ISize/USize`，`size` 与下标用之。
  - 显式数值转换族。

**12. 并发只有「转移所有权」**（13）

- **问题**：`Rc` 不可 Transferable，没有 `Shareable/Arc/Mutex/atomic`，也没有全局可变状态。所以只读数据无法跨线程共享，`Sender` 没有克隆路径导致 MPSC 无从谈起，日志/计数器/缓存无处安放，线程池写不出（见 7）。
- **建议**：
  - 先做 **scoped threads / 结构化并发**：`parallel { }` 保证子线程在调用返回前 join，正好落在「调用期 loan」模型里，只需 `Shareable` 判定就能把 Borrow 借给子线程，比 `Arc` 先落地更便宜。
  - 再由 unsafe 层实现 `Arc/Mutex/Atomic`、`Sender.clone()`、`static` + `OnceCell`。
  - **提前**决定 async 走 stackful 还是 stackless，以及取消如何在「无展开」下表达。这会反过来约束借用跨挂起点、Transferable 和闭包布局，不宜等 v3。

**13. 失败模型：abort-only 是一扇关上的门**（09/12）

- **判断**：abort-only 让 drop 没有 unwind 边，我认为方向对。但它不可逆：所有权检查和 codegen 一旦建立在「无展开」上，以后就加不了 panic 隔离。对长驻服务，一次坏请求杀进程。嵌入式需要 fallible allocation。`?` 要求 `E` 精确相同，错误聚合只能手写 `when`。
- **建议**：确认这是有意取舍，并补配套：
  - fallible 变体（`tryAdd`、`getOrNull` 等，依赖第 3 项）
  - `setAbortHook`（刷日志、写现场）
  - 明确写出「线程隔离靠进程」，或提供「杀线程不跑 drop」的可选策略
  - 显式的 `ErrorFrom<E>`/`mapError` 或 Zig 式错误集，而不是隐式 `From`

**14. 内建 `T?` 与真正的 `Option` 双轨**（03/09）

- **问题**：`T??` 非法，所以泛型里 `T = Int?` 时 `T?` 无定义（Kotlin 的老坑：`Map<K, V?>.get` 分不清「没有键」与「值为 null」）。§35 为 nullable 单独造了一整套 flow/proof/extraction/剩余域规则，本质是在重写 enum 匹配。
- **建议**：`T?` 只做 `Option<T>` 的语法糖，可嵌套，niche 优化留给 ABI。`null` 是 `None` 字面量，smart cast、`!!`、`when` 覆盖域走枚举的同一套机制。表面写法保持 Kotlin 式，内核只有一个 sum type。

**15. `String` 模型**（13）

- **问题**：不可变的 UTF-8 却是 MoveOnly，操作只有 `+`/`==`/`println`，没有 view，插值无法降级。不可变字符串本该是最适合廉价共享的类型，却成了最难复制的：`val sep = ","; listOf(sep, sep)` 非法，词法分析要一路分配。
- **建议**：
  - 字面量与 `const` 字符串类型化为 `'static` 的 `Str`（Copyable，指向静态只读数据，不需要借用返回就成立，能覆盖日志、协议、关键字这个大头）。
  - 动态 `String` 保持 owned。
  - 第 3 项落地后补 `StringView` 统一两者。
  - `Display` 协议与插值降级一起做。

------

## 🟢 次要项

- **`?` 与 `?.` 撞车**：`foo()?.bar` 与 Rust 习惯相反，必须写 `(foo()?).bar`。既然没有异常，`try` 前缀可复用（Swift/Zig 式），或去掉尚未启用的 `?.`。
- **赋值是表达式**：导致 `f(x = 1)` 命名实参歧义，要靠 `f((x = 1))` 补丁。Kotlin 把赋值定为语句正是为此。
- **构造器不能设可见性，也没有 `init`**：无法用私有构造器 + `Result` 工厂强制不变量，而这是所有权语言的常规模式。
- **Kotlin 皮 + Rust 骨的落差**：
  - `class` 引用直觉、`forEach { sum += it }`、扩展函数、属性、默认参数、data class 都不成立。
  - 建议文档显式列出「不成立的 Kotlin 直觉」，并选择性回补最值的糖：默认参数（调用点展开，成本低）、data class 式 derive。
- **其他**：标识符仅 ASCII 的升级路径；单态化实例深度上限与多态递归的规定（Phase 4 才会撞上）。

**规范方法论**：精确的地方不对。大量篇幅花在 parser 恢复算法、AST 表形状、错误码分配和 O(n) 承诺上，而 drop、unsafe、借用返回、迭代协议这些语义内核几乎空白。Phase 切片还反过来塑造语义，比如 `for` 首轮不支持 range。建议拆成两层，「语言参考」（语义）与「实施契约」（Phase/ADR/AST/错误码/恢复）。每个 v2 延后项要写「依赖闭包」，标明谁必须依赖它才成立。v1 验收用 litmus 程序，不用 Phase 产物。

------

## 后续道路

**阶段 0：止血（几天）**

- 修语句分隔和关键字（1、2）。
- 建立下面的 litmus 清单，跑一遍「不可表达清单」。
- 冻结 Map、迭代协议、String API 的实现，直到阶段 1 决策落定。

**阶段 1：语义内核闭合**（一次设计 spike，顺序有依赖）

1. 统一 capability 框架：Copyable/Transferable/Escapable、opt-in derive、函数类型限定（7、8）。
2. 二等借用 + projection + `Span`/`Iterator`（3）。
3. 所有权原语（4）与 `deinit`/drop 顺序（5）。
4. `Box` 泛化 + `Cell`/`Weak`/arena（9）。
5. 最小 unsafe/FFI/layout（6）。

出口判据：用第 5 步把 `Box/Rc/Array/List/String` 在库里重写一遍。写不出来，就回头补前几步。

**阶段 2：库可行性**

- 位运算与字长整数（11）。
- `Eq/Hash/Ord/Display/Clone` + derive + conformance extension + associated type（10）。
- `Option` 统一（14）和 String 分层（15）。
- 此时 HashMap/BTreeMap 与 Map 的所有权契约自然可定。

**阶段 3：并发与运行时多态**

- scoped threads + `Shareable`，再做 `Arc/Mutex/atomic`。
- `dyn`/existential：先做 `Box<any I>` 与借用形参。
- async 与取消的取舍。

**阶段 4：编译模型与工具**

- 分离编译 + 泛型导出。现在跨 compilation-unit ABI 完全空白，而单态化 + LLVM 天然编译慢。
- 增量/查询式架构、freestanding core、包管理。
- 之后再谈自举。

**不要做的事**

- 不要在阶段 1 之前把 Map、迭代、String API 固化在 intrinsic 上。
- 不要引入用户可写的生命周期参数。它与「低标注负担」冲突，second-class + 推断依赖已能覆盖绝大多数场景。
- 内核闭合前，不要再往规范里加 parser 恢复细节。

## v1 内核的验收清单

以下程序都能用 Koven 自己写出来，不靠编译器魔法，才算内核完成：

1. `MutableList` 的 push/pop/get/swap/sort，含 `MutableList<String>`
2. 单链表：迭代遍历、头插头删、反转；BST 插入/旋转
3. `HashMap<String, V>` 的 get/insert/entry
4. 词法分析器/JSON 解析器（slice/view + `Result` 错误）
5. `Option.map`、`Result.mapError`，payload 为 MoveOnly
6. 状态机 `this.state = this.state.next()`
7. `forEach { sum += it }` 与线程池（`Sender<move () -> Unit>`）
8. RAII 文件/锁 guard，drop 顺序可预测
9. 自定义容器 + arena；递归 enum 表达式树
10. 二进制协议帧编解码 + CRC16（位运算、`UByte` 切片、端序转换）
11. MPSC channel + scoped parallel sum
12. FFI：调用 libc 的 `write`

如果只能先做一件事，就先定第 3 项的借用模型。它决定 4、7、9、10 的形状，也决定 Map 和迭代协议长什么样。