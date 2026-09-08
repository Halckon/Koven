# Koven v0.32：Kotlin 语法语义与 Rust 所有权对照清单

> 文档性质：**非规范比较清单**。表格描述审计时的现行 guide 与实现状态，不以 Kotlin 或 Rust
> 行为反向补全 Koven。状态与审计边界见
> [语言设计审计](./language-design-audit-v0.32.md)。

## 1. 阅读口径

Koven 的目标是“贴近 Kotlin 的命名和表面习惯”，不是源码兼容。即使拼写相同，也必须以
Koven guide 的类型、所有权、效果和 ABI 规则解释。Rust 对照同理：Koven 借用的是安全模型的
核心不变量，不承诺 Rust borrow checker 的完整行为。

外部对照采用官方资料：

- Kotlin：[语言规范](https://kotlinlang.org/spec/kotlin-spec.html)、
  [字符](https://kotlinlang.org/docs/characters.html)、
  [扩展](https://kotlinlang.org/docs/extensions.html)、
  [lambda 与高阶函数](https://kotlinlang.org/docs/lambdas.html)、
  [相等](https://kotlinlang.org/docs/equality.html)、
  [运算符重载](https://kotlinlang.org/docs/operator-overloading.html)、
  [value class](https://kotlinlang.org/docs/inline-classes.html)、
  [data class](https://kotlinlang.org/docs/data-classes.html)、
  [inline function](https://kotlinlang.org/docs/inline-functions.html)、
  [coroutine](https://kotlinlang.org/docs/coroutines-overview.html) 与
  [Kotlin/Native C interop](https://kotlinlang.org/docs/native-c-interop.html)；
- Rust：[所有权](https://doc.rust-lang.org/book/ch04-00-understanding-ownership.html)、
  [closure 类型](https://doc.rust-lang.org/reference/types/closure.html)、
  [析构](https://doc.rust-lang.org/reference/destructors.html)、
  [`Send`](https://doc.rust-lang.org/std/marker/trait.Send.html)、
  [external block](https://doc.rust-lang.org/stable/reference/items/external-blocks.html) 与
  [类型布局](https://doc.rust-lang.org/stable/reference/type-layout.html)。

状态词：**已实现**、**部分实现**、**现行未实现**、**候选未启用**、**延后**、**不支持**、
**未定义**。其中“部分实现”经常表示 frontend 已接受并发布 typed facts，但 native lowering
仍有封闭边界。

## 2. Kotlin 语法与语义对照

### 2.1 词法、文件和声明

| 项目 | Kotlin 习惯/语义 | Koven v0.32 | 状态与取舍 |
|---|---|---|---|
| 源文件 | `.kt` / `.kts` | `.ko` | 已实现；明确不是 Kotlin 源码兼容 |
| 标识符 | 支持普通及反引号标识符，字符集合较宽 | 普通标识符采用封闭 ASCII 规则；无反引号转义标识符 | 已实现；简化 lexer、ABI 名称与工具链确定性，牺牲 Unicode API 名称 |
| 关键字 | hard/soft keyword 随上下文区分 | hard、soft、future-reserved 分层；大量未来词在 lexer 禁用 | 已实现；早保留减少未来语法冲突，但增加兼容成本 |
| 分号 | 通常由换行推断，也可显式分号 | 只有文件级顶层声明使用换行或 `;` 分隔；block element 不以换行/`;` 分隔 | 已实现；结构 stop 决定 block 边界，不复制 Kotlin semicolon inference |
| 注释/字面量 | 行/块注释、数值、Char、String、模板 | 对应核心词法；String interpolation 已有 AST/typed traversal | 部分实现；普通 String native 已完成，interpolation native 未完成 |
| `package` / `import` | 文件头、包级声明、显式和 wildcard import | 相同表面习惯；logical path 必须与 package 一致 | frontend 已实现；compilation-unit/multi-source native 正由 SPEC-0199 收口，公开 project build 属于 SPEC-0054 |
| 顶层声明 | 函数、属性、类型均可顶层 | 支持顶层 callable、`val`/`var`/`const val` 与 class-family | 部分实现；initializer/native 覆盖仍是封闭子集 |
| `val` / `var` | 只读引用/可变变量或属性 | local、top-level/member 语法存在；所有权仍由值类型决定 | 已实现核心；`val` 不等于值可复制，`var` 不等于可从借用中移动 |
| `const val` | 编译期常量，类型和 initializer 受限 | 语法与声明 identity 已有；闭合 evaluator 在 v0.36 候选 | 候选未启用；当前不能按候选 CTFE 假定求值 |
| 函数声明 | block/expression body，可推断返回类型 | expression body 必须显式返回类型；无体或 block body 省略时精确为 `Unit` | 已实现；降低跨阶段推断复杂度，和 Kotlin 不同 |
| 局部类型推断 | 广泛双向/约束推断 | 单向 expected type；不从后续赋值、其他文件或任意 overload 反推 | 已实现；确定性强，但需要更多标注 |
| 具名实参 | 支持 | typed call argument 与重排已支持 | frontend 已实现；native 取决于具体 callable 子集 |
| 默认参数值 | 支持 | 明确禁止 | 不支持；避免 default expression ABI、overload 与 ownership 交互 |
| `vararg` | 支持 | 关键字已保留，但用户声明完全禁止 | 不支持；核心容器构造不是 `vararg` 脱糖 |
| trailing lambda | 常用调用语法 | v0.33 候选，含 implicit `it` | 候选未启用；当前不得由 parser/formatter 先行接受 |
| visibility | `public`/`internal`/`protected`/`private` 等 | 有封闭的顶层/member visibility 与跨文件可见性规则 | 已实现核心；无 class inheritance，因此不照搬 protected 语义 |
| `typealias` | 支持 | 词法保留，现行语义未定义 | 未定义；不能把 alias 当成新名义类型或透明布局 |
| annotation | 语言级注解与 use-site target | `@` token 存在，尚无现行 annotation grammar/semantics | 未定义；建议先做 compiler-bound metadata |

### 2.2 类型、类和对象模型

| 项目 | Kotlin 习惯/语义 | Koven v0.32 | 状态与取舍 |
|---|---|---|---|
| 基础类型命名 | `Int`、`Long`、`Boolean`、`Char`、`String`、`Unit`、`Nothing`、`Any` | 基本沿用，并增加明确位宽的无符号类型集合 | 已实现类型身份；具体 native 覆盖逐步扩张 |
| 数值转换 | 无传统 Java 式隐式 widening，但库有显式转换 | 不做已定型变量的隐式 widening；字面量按 expected type 定型 | 已实现；有利于 overload、ABI 和溢出确定性 |
| `Char` | UTF-16 code unit 的语言语义 | 单个 Unicode scalar value | 现行设计；名称相同但语义不同，需在用户文档突出 |
| `String` 表示 | 平台相关不可变字符串 | 明确是合法 UTF-8 的 MoveOnly owner | native 已实现 plain literal、连接、byte equality、Borrow/Value/return/drop；没有隐式 copy |
| `String ==` | `==` 是结构相等，和 `Comparable` 分离 | UTF-8 byte length + 内容相等；不 normalization/case/locale fold | 已实现；应继续与未来 `Comparable` 分离 |
| `String` 顺序 | 可通过比较能力/库 API 表达 | 未定义通用 String ordering | 未定义；建议 locale-independent scalar lexicographic，locale collation 单独 API |
| nullable `T?` | nullable 类型、safe call、Elvis、`!!` | 类型、null equality/`if` flow、Elvis/`!!` 基础类型事实已建立；safe call 仍保留 deferred 边界 | 部分实现；native 只闭环 pointer-like nullable `if`/null comparison，inline nullable、nullable `when`、`!!` 消费等仍在候选/后继链 |
| `Any` | 所有非空值共同超类型，有运行时对象模型 | 默认泛型上界，同时可被 `when` join 产生，但无 v1 RTTI/dyn ABI | 语义闭合缺口；推荐 v1 限定为 bound-only，需新 guide |
| 泛型 | declaration/use-site variance、约束、reified 等 | 不变型、单一上界、单态化；能力上界可编译器绑定 | 已实现核心；无 variance、intersection、reified |
| ordinary `class` | 引用语义对象，可继承/open/abstract | 唯一 owning heap handle；不支持 class inheritance | frontend 与窄 native 已实现；资源语义比 Kotlin 引用更强 |
| `value class` | 当前 Kotlin value class 以单个 underlying property 为核心，可能 boxing | 多字段名义 inline aggregate；可以 MoveOnly | 已实现；名称熟悉但语义差异大，必须避免“等同 Kotlin inline class”描述 |
| `data class` | 编译器生成 `equals/hashCode/toString/component/copy` 等 | 未定义，`data` 不应提前视为关键字 | 未定义；未来 derive 必须处理 MoveOnly 字段和不可用 `copy()` |
| `enum class` | 枚举常量与类能力 | Rust ADT 风格、case 可有 payload、`when` 穷尽 | frontend/type/ownership/native 已覆盖主要主线；刻意偏离 Kotlin 普通 enum |
| sealed hierarchy | 支持 sealed class/interface | `sealed` future-reserved，无语义 | 延后；穷尽性当前来自 enum/Boolean 闭域 |
| interface | 可有默认方法，可作运行时接口值 | 默认方法、静态 implementation/overload resolution；无裸运行时 interface value | frontend 已实现；v1 静态分发，`dyn` 延后 |
| class inheritance | 默认 final，`open`/`abstract` 启用继承 | 完全不支持，包括“单层 abstract class” | v1 不支持；interface + delegation 更符合 owner/layout 模型 |
| interface delegation | `by` 可委托接口实现 | 只允许 `Interface by valField` 的窄化静态转发 | 已实现语法/检查；不接受任意表达式或运行时代理 |
| `object` | singleton 与 object expression | 具名 singleton identity 已定义；runtime state/lazy init 延后；object expression 禁止 | 部分/延后；无匿名内部对象 |
| `companion object` | 有 Companion 对象和类型级访问 | 无状态关联命名空间，不产生 `Type.Companion` 值 | 已实现 frontend 主体；const evaluator 在候选链 |
| primary constructor | class header 参数及属性 | class-family 封闭构造形态；constructor typed/ownership/native 主线已建立 | 部分实现；不是完整 Kotlin secondary constructor/init 模型 |
| getter/setter | 可声明自定义访问器 | 明确砍掉 | 不支持；避免隐藏 call/effect/borrow |
| delegated property | `by lazy` 等 | 明确不支持 | 不支持；依赖 getter/setter、metadata 与 lazy state |
| extension function | 静态解析、无真正成员注入 | 尚未定义 | 建议在 receiver mode 稳定后加入静态 extension；member 永远优先，禁止私有穿透 |
| extension property | 可声明无 backing field 的扩展属性 | 未定义 | 延后；先只做 extension function |
| reflection/RTTI | JVM/Native 有不同反射能力 | v1 明确无运行时反射/RTTI | 不支持；`is`/`as` 只用于编译期已知层次 |

### 2.3 表达式、控制流与函数式能力

| 项目 | Kotlin 习惯/语义 | Koven v0.32 | 状态与取舍 |
|---|---|---|---|
| `if` expression | 有值；缺 `else` 通常只能作 Unit 情形 | 缺 `else` 只允许完整 statement element；value context 必须有 `else` | 已实现；上下文规则更显式 |
| `when` | statement/expression、模式和条件丰富 | Boolean/enum/nullable 闭域穷尽、smart cast、subject/subjectless 子集 | frontend 已实现；native 只覆盖现行 lowering 子集 |
| smart cast | flow-sensitive null/type refinement | 稳定 place 的有限 flow facts；循环/未知 call/lambda 保守 kill | 已实现简化模型；不承诺 Kotlin 全套 data-flow |
| `for` | iterator convention | 语法存在；v0.37 候选规定 compiler-bound 借用容器 provider | 候选未启用；完整 native `for` 尚未闭环 |
| `while` / `do while` | 支持 | parser/type/control-flow 已支持 | frontend 已实现；复杂 exit cleanup 仍需后续事实收口 |
| range / `in` | 标准库 operator convention | parser/typed 分层存在，完整 provider/runtime 未闭合 | 部分实现；不要由同名方法反向识别 intrinsic |
| destructuring | `componentN()` convention | Copyable copy 或 MoveOnly 完整消费式结构转移；右值一次求值 | 已实现核心；禁止普通字段部分移动 |
| lambda | closure、receiver lambda、implicit `it` 等 | function type；普通 borrowed capture 不逃逸，`move` owned capture 可逃逸 | 部分实现：现行语义、frontend 与封闭 closure SSA/native 核心已完成；一般存储/API/native 表面仍窄；implicit `it` 仅候选 |
| 高阶函数 | 函数值、lambda、标准库 collection HOF | function type、callable value、generic callable 单态化已建立 | frontend/SSA 已实现核心；标准库 HOF 待 receiver/iteration/runtime |
| 柯里化 | 可由函数/lambda 和库组合表达，不是 Kotlin 自动语义 | 不提供自动 currying 或多次 call application | 建议保持；未来用显式 `curry`/partial-application 库函数 |
| callable reference | `::name`、bound reference | parser 有对应形态，完整 member/bound/effect native 未闭合 | 部分实现 |
| local/non-local return | inline lambda 可有非局部 return 等规则 | lambda 是独立 return 边界；无 label 和非局部 return | 已实现；即使未来 inline，也不改变返回语义 |
| recursion | 具名/局部递归视声明形态而定 | 具名函数先建签名，可同文件及跨文件递归；recursive data 必须经 handle | frontend 已实现；native 多文件闭环在 SPEC-0199，尾递归优化不保证 |
| exception | `throw`/`try`/`catch`/`finally` | 完全不支持；可恢复失败用 `Result<T,E>`，`?` 传播，`error` abort | 已实现核心；无 stack unwind cleanup |
| operator overload | `operator fun` 映射到封闭名字 | `operator` 是硬关键字但用户 overloading 没有现行语义；builtin operators 封闭 | 未定义/不支持当前版本；未来只能开放固定协议，不开放新符号/优先级 |
| infix function | `infix fun`，固定语法约束 | 表达式中只有软词 `to`；`infix` 本身普通标识符，仅保留标准库方向 | 用户自定义不支持；保持 Pratt 优先级单一真源 |
| inline | `inline`/`noinline`/`crossinline` 影响 lambda 与非局部 return | 没有源语言 inline 契约；LLVM 当前为 O0 | 未定义；建议先做优化 hint，绝不改变 lambda return/ownership 语义 |
| tail recursion | `tailrec` 可请求优化 | 未定义 | 延后；不能把尾调用优化变成资源语义保证 |

### 2.4 平台、并发和元编程

| 项目 | Kotlin 生态/语义 | Koven v0.32 | 状态与取舍 |
|---|---|---|---|
| 标准库集合 | List/Map 等主要是库 API，compiler/backend 可有优化 | 顺序容器 identity/layout/place 是 compiler-bound，算法/API 应由 `lang-std` 的 `.ko` 实现 | 部分实现；Map 仍是未启用候选 |
| lazy | `lazy {}` 通常由 delegated property/stdlib 提供 | 无 lazy 关键字、属性委托或 `Lazy<T>` 契约 | 未定义；建议线程局部 `Lazy<T>` 库类型先行，singleton lazy init 延后 v2 |
| C interop | Kotlin/Native 由 cinterop 与平台类型桥接 | backend 已私有声明 C `main` 与 malloc/free/abort/write/strlen 等 runtime symbol；无源语言级公共 FFI/unsafe/pointer ABI | 公共能力未定义；需受限 C ABI 独立 guide/ADR，不能把私有 runtime ABI 当承诺 |
| IO/file | Kotlin stdlib/平台 API | 只有 stdout `println(String)` 的最小 compiler-bound runtime | 现行未实现；建议同步 byte IO + `Result` 先行 |
| Socket/network | 平台/库能力 | 无 | 未定义；应建在同步 IO、resource owner 与 FFI 之上 |
| process | 平台 API | 无 | 未定义；`Command`/`Child`/pipe 应是 MoveOnly owner |
| thread | 平台 API 与平台内存模型 | 现行 guide 已定义顶层 `thread(move { ... })`、cross-thread effect 与 `Transferable`；frontend 检查已实现 | 现行标准库/runtime 未实现；具体 handle/member native 表面仍需 guide/Spec |
| channel | coroutine/channel 库或平台库 | 现行 guide 已定义 `channel<T>()`、`Sender.send` 的 Value delivery 与 `Transferable` 约束 | 现行未实现；具体返回类型、receiver lowering 和 runtime 仍待后继 |
| coroutine | `suspend` + continuation/CPS，库提供 structured concurrency | guide 只把 `async`/`await` + `Future` state machine 列为 v3 推荐方向 | 延后且未定案；“首轮不允许 borrow 跨 suspend”是本审计建议，不是现行语义 |
| macro | compiler plugin、KSP、插件 API等多层机制 | `macro` future-reserved，无现行 expansion | 延后；先 metadata annotation，再封闭 derive |
| IR plugin/aspect | Kotlin compiler plugin 可变换 IR，API 演进风险高 | 无公开 plugin/aspect | 建议不作为 v1 语言功能；只做内部 verified pass |
| self-host | Kotlin 编译器由 JVM 生态演进，不是语言语义 | “stdlib bootstrap”已实现，但 compiler self-host 明确为 v4+ | 延后；不能把 prelude bootstrap 称为编译器自举 |

## 3. Rust 所有权与资源模型对照

### 3.1 核心所有权

| 概念 | Rust | Koven v0.32 | 状态与权衡 |
|---|---|---|---|
| 单一 owner | 非 `Copy` 值通常只有一个 owner | MoveOnly 值有唯一 owner | 已实现；核心不变量一致 |
| move | 赋值/传参/返回可移动，受 place/type 规则控制 | 向 local/Value 参数/return/capture 交付时复制或移动 | 已实现；调用点通常无 `move` marker，契约来自 callee mode |
| 隐式复制 | `Copy` trait，可由用户为满足规则的类型实现/derive | compiler-bound `Copyable`，用户不可实现；必须无 retain/clone/drop glue | 已实现；更封闭、更易验证，表达力小于 Rust |
| 显式复制 | `Clone` 可执行任意逻辑/失败模型由 API 决定 | 没有通用 `Clone` 契约 | 未定义；未来必须与 `Copyable` 分离，MoveOnly 深复制不得隐式发生 |
| shared borrow | `&T`，lifetime 纳入类型/推断 | Borrow binding/调用期 loan，无 lifetime 类型参数 | 已实现简化模型；更易学，不能表达借用返回或复杂容器 view |
| exclusive borrow | `&mut T` | `inout` 声明，调用点 `&place`，建立同步独占 loan | 已实现；不引入通用一元 `&` 值 |
| 参数默认 | Rust 参数按值，借用必须写 `&T` | Koven 参数无 marker 默认 Borrow；`own` 才是 Value | 刻意不同；API 安全默认更强，但与 Rust/Kotlin 直觉均不同 |
| lifetime annotation | `'a` 等显式/推断 lifetime | 无源语言 lifetime | v1 不支持；限制 API 以换取简单性 |
| NLL | 基于 MIR 的 non-lexical lifetime | 不实现完整 NLL；调用期和规则化 loan 范围 | 不支持；诊断更保守、实现更确定 |
| borrow return | 可用 lifetime 表达 `&T` 返回 | v1 不允许借用返回/place-return | 不支持；迭代使用 compiler-bound provider 而非公开 iterator borrow type |
| reborrow | `&*x`、`&mut *x` 等受 borrow checker 管理 | Borrow/Inout 在封闭调用/provider 中建立 shared/exclusive reborrow | 部分实现；不提供 Rust 全套表达式 |
| partial move | 可从 struct pattern 移动部分字段，受 Drop 等限制 | 普通字段禁止部分移动；MoveOnly value class 只允许完整消费式解构 | 已实现；牺牲精细度换取简单 drop state |
| use-after-move | 编译期拒绝 | 编译期拒绝，含容器 element place 与 closure capture | 已实现 |
| drop 时机 | 通常 scope 末尾，受临时值和 MIR drop elaboration 规则影响 | owned value 在最后需要点后 ASAP drop；显式 typed drop facts | 已实现；资源更早释放，但必须精确处理所有 CFG exit |
| destructor | `Drop::drop` + compiler drop glue | 用户自定义析构尚未开放；compiler/runtime 为 owner 生成递归 drop/free | 部分实现；避免用户 drop effect 干扰 ownership checker |
| unwind | panic 可按配置 unwind 或 abort | `error`/runtime failure 统一 abort，不做 unwind cleanup | 已定义；简单 ABI，资源 API 必须用 `Result` 处理可恢复失败 |

### 3.2 Owner 类型和能力

| 概念 | Rust | Koven v0.32 | 状态与权衡 |
|---|---|---|---|
| inline struct | `struct`，layout 默认 Rust ABI，`repr` 可改变 | `value class`，有限 inline layout，按字段条件 `Copyable` | 已实现；名义和 Kotlin 表面结合 |
| unique heap owner | `Box<T>` 可装任意合适 `T` | 普通 `class` 本身是 unique heap owner；intrinsic `Box<T>` 只装 concrete value class | 已实现主线；更严格，避免双重引用语义 |
| shared owner | `Rc<T>` 非原子、`Arc<T>` 原子 | `Rc<T>` 使用非原子 strong count，显式 `share()` 分叉 owner；没有 `Arc` | 非 nullable 主线及 `Rc?` 的 null-niche/`if`/null-comparison/conditional-drop 子集已实现；nullable `when`/`!!` 与 `Arc`/Weak 仍待后继 |
| thread transfer | `Send` unsafe auto trait，可手动/自动实现 | `Transferable` compiler-bound 结构能力，用户不可实现 | 已实现类型/closure effect；公开 thread API 未完成 |
| shared cross-thread access | `Sync` | future `Shareable`，v1 不启用 | 延后 v2；当前只允许转移 owner，不承诺共享借用跨线程 |
| `Rc` 跨线程 | `Rc<T>: !Send` | `Rc<T>` 恒不 `Transferable` | 已实现；与非原子计数安全要求一致 |
| interior mutability | `Cell`/`RefCell`/`Mutex`/atomics 等不同机制 | 无通用 interior-mutability 能力 | 未定义；应随 thread/atomic/runtime 分层，不要从 `var` 推导 |
| pinning | `Pin<P>` 约束地址稳定与自引用 | 无 `Pin`、自引用或 address-stable public contract | 不支持当前版本；async/FFI 前需重新评估 |
| raw pointer | `*const T`/`*mut T`，dereference unsafe | 无源语言 raw pointer/opaque pointer | 未定义；FFI 前必须设计，不能泄露 LLVM `ptr` |
| `unsafe` | unsafe block/function/trait 等限定编译器不验证的义务 | `unsafe` 是硬关键字但无现行产生式 | 未定义；推荐只开放 FFI/raw pointer 的最小边界并记录 SAFETY contract |
| layout control | `repr(C)`、`repr(transparent)` 等 | target DataLayout 内部已使用，但没有用户可控公共 ABI layout | 未定义；FFI 必须引入独立 C-safe type set/representation |

### 3.3 Closure、泛型、并发和异步

| 概念 | Rust | Koven v0.32 | 状态与权衡 |
|---|---|---|---|
| closure capture inference | 按使用推断 borrow/mut borrow/move，生成匿名类型 | 普通 lambda 只 shared capture；`move` lambda owned copy/move | 已实现简化模型；不推断可变 capture |
| `Fn`/`FnMut`/`FnOnce` | 由 capture/use 自动实现，影响调用次数与消费 | 无公开 closure trait 层次；move closure 已可按现行语义存储/return/Value delivery | 不支持当前版本；一般 API 如需区分调用次数，可另引入更小的 call capability |
| borrowed closure escape | lifetime 可以证明时可返回/存储 | 一律不得逃出 defining callable | 已实现；保守但易诊断 |
| move closure escape | `move` 只改变 capture 方式，是否 `Send` 另判 | `move` 形成 owned environment 并允许逃逸；thread-bound effect 再查 `Transferable` | 部分实现：语义/frontend/封闭 backend 核心已完成，一般源码存储与 native 表面仍待扩展 |
| function ABI | Rust ABI 默认不稳定，`extern "C"` 显式 | Koven function/closure ABI 全部内部；无 public extern | 未定义；C callback 首轮应只接受无 capture function pointer |
| trait bounds | 多 bound、associated type、HRTB 等 | 单一 interface 或 compiler capability bound | 已实现核心；降低 solver 复杂度 |
| dispatch | static monomorphization 与 `dyn Trait` 并存 | v1 只静态分发/单态化 | 已实现；`dyn` 延后 v2 |
| recursion | 函数和间接递归类型；无限 sized type 拒绝 | 具名函数递归；value/enum inline cycle 拒绝，class/Box/Rc/container handle 可打断 | frontend 已实现；需要实例化深度/栈风险诊断政策 |
| scoped thread | 标准库可保证 join 生命周期 | 现行 guide 有顶层 `thread(move { ... })` 与 `join()` 示例，但具体 handle/runtime 未实现 | 现行未实现；若改成 `Thread.spawn` 或结构化 scope，必须通过新 guide |
| channel | `std::sync::mpsc` 等通过 send 转移值 | 现行 guide 的 `channel<T>()`/`Sender.send` 按 Value delivery 转移 owner | 现行未实现；接收端取得新 owner，Borrow 不跨线程 |
| async | `Future` state machine，borrow 可在受约束时跨 await | v3 仅推荐 `Future` + `async`/`await` 方向 | 延后且未定案；本路线建议首轮禁止 borrow 跨 suspend |
| FFI | unsafe extern、C ABI、`repr(C)`、raw pointer | backend 有私有 C runtime/main ABI，无源语言公共 extern | 公共能力未定义；需先建立显式 ABI-safe 类型白名单 |

## 4. “启用什么”的总清单

### 4.1 应继续启用并稳定的核心

- Kotlin 风格文件、声明、控制流、nullable、lambda 和 class-family 表面；
- 无异常的 `Result`/`?` 失败模型；
- Borrow-default + `own`/`inout`，调用期 loan 与 ASAP drop；
- compiler-bound `Copyable`/`Transferable`；
- `value class` inline aggregate、普通 class unique owner、显式 `Box`、单线程 `Rc`；
- enum payload + exhaustive `when`；
- 静态 interface/单态化，无 v1 `dyn`；
- typed SSA/verifier + LLVM AOT；
- 编译器绑定容器表示、Koven 源码标准库 API。

### 4.2 建议在明确门禁后启用

| 能力 | 前置门禁 |
|---|---|
| extension function | instance receiver mode、跨文件 import/overload 顺序、member-wins 规则 |
| collection HOF / `for` | v0.37 provider facts、receiver、drop/exit 完整性 |
| Map | Equatable/Hashable、table ownership/drop、iteration order |
| String ordering / `Comparable` | Equality/Ordering 分层、泛型 operator dispatch |
| C FFI | `unsafe`、C-safe type set、layout/symbol/error/callback ABI |
| sync IO / process / Socket | FFI/runtime adapter、MoveOnly resource owner、`Result` error taxonomy |
| thread/channel | runtime primitive、`Transferable` API、join/cancellation/drop 语义 |
| optimization | O-level CLI、benchmark、IR snapshot、drop/loan differential verification |
| metadata annotation | target/retention/duplicate/diagnostic contract |
| derive | hygiene、确定性、生成 Span、重跑受影响语义阶段 |
| async/await | Future/Pin 或无自引用方案、取消、executor、suspend ownership |
| self-host | 完整项目构建、稳定 std/ABI、stage comparison 与可复现 bootstrap |

### 4.3 v1 不建议启用

- 单层或无限层 class inheritance；
- 运行时反射、`dyn` 与匿名内部类；
- Kotlin 异常/stack unwinding；
- 用户自定义任意优先级 operator 或 infix；
- 属性 getter/setter 与 delegated property；
- 任意 AST macro、用户 IR Aspect/plugin；
- borrow return、完整 NLL、partial move、borrow 跨 async suspend；
- 隐式 currying、隐式深 clone、隐式 String conversion；
- 为优化而改变 class/Box/container 的 heap 语义。

## 5. 需要在用户文档中特别强调的“同形不同义”

1. `value class`：Koven 是多字段 inline aggregate，Kotlin 以单 underlying property 为核心。
2. `Char`：Koven 是 Unicode scalar，不是 UTF-16 code unit。
3. 参数无 marker：Koven 是 Borrow，不是 Kotlin/Rust 的普通按值直觉。
4. `class`：Koven handle 是唯一 owner，赋值可能 move；不是共享 GC reference。
5. `enum class`：Koven 是 payload ADT。
6. `object`：v1 不承诺 runtime state/lazy singleton initialization。
7. `==`：String 是精确 UTF-8 内容相等，不做 locale/normalization；与 ordering 分离。
8. `?`：postfix propagation 与 nullable `?.`/`?:` 是不同 token/语义。
9. `error()`：abort，不是可捕获异常，且没有 unwind cleanup。
10. `inline`：即使未来加入，也只应是优化控制，不获得 Kotlin 非局部 return 语义。
