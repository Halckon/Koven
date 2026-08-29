# AGENTS.md — Koven AOT 编译器工程规范

本文件约束在本仓库内工作的 AI Agent 与开发者。它描述**如何开发、验证和交付**，不是
语言语法规范本身。规则优先于示例；如果规则、代码和文档互相冲突，不得自行折中。

> 当前仓库已完成 Phase 0 与 Phase 1：确定性 Lexer、独立表达式、声明、block、lambda、
> callable 参数与 typed call argument Parser、局部 `val` 解构、完整文件组合、Kotlin 风格
> `package` / `import` 文件头、control-flow、postfix `?`、class-family、窄化接口委托，以及
> 具名函数隐式 `Unit` 返回标注已实现；Phase 2 的单文件名称解析、基础类型检查以及完整
> 名义类型、泛型、interface 实现与窄化委托检查、`when` 穷尽性与 smart cast，以及条件
> `Copyable`、有限内联布局、结构化解构、泛型 callable 实例化、overload-lambda 隔离与
> 顺序容器类型检查已完成；Phase 3
> 已完成整变量 use-after-move、条件复制、消费式解构、禁止结构分量部分移动、v0.26 的
> borrow-default 参数契约、调用期 loan、owned-value ASAP 析构点与顺序容器核心 element
> place 所有权，以及 v0.27 的简化 closure capture、`Transferable` 与编译器绑定跨线程
> effect，并由 SPEC-0188 完成 constructor ordered Value delivery、root owner/drop facts；Phase 4 已完成 owner-aware typed SSA/verifier 与封闭标量 frontend→SSA→AArch64
> LLVM IR 主线；SPEC-0035 已完成 named aggregate/heap-owner SSA、first-class aggregate/
> DataLayout、系统 allocation、heap place 与 recursive drop/free 后端基元；SPEC-0036 已完成
> 顺序容器固定 header、连续缓冲区、checked-index、replace/drop 与 MoveOnly ZST 后端基元；完整 `for`
> 等待 typed iteration plan 与 provider runtime；SPEC-0186 已在 LLVM 复合类型构造前加入
> target size/alignment/stride 超限与溢出预检；Phase 5 已建立真实 Koven prelude bootstrap，
> 并由 SPEC-0043 把标准 `error(message: String): Nothing` identity 接入既有 Abort，SPEC-0189
> 把首个 `println(String)` Borrow identity 与非插值 literal 接入 stdout，SPEC-0044 已在
> `prelude.ko` 实现条件 `Copyable` 的 `Pair` / `Result` 并完成 native 正反验收；Phase 6
> 已提供单文档 frontend LSP 诊断与语义跳转定义、版本化 JSON Lines 机器诊断、非破坏性保守 formatter、
> TextMate 与 Tree-sitter grammar。SPEC-0025/0197 已完成多文件 package/import 名称解析与
> compilation-unit 类型检查，包括类型身份、完整签名图、顶层 callable/call、局部变量、基础运算与基础 `if`
> body、源码 nominal/enum 与 intrinsic Box/Rc/container construction、source member body/call/field
> projection、intrinsic Rc member operation、container element-place/member/assignment 及 contextual
> null literal/null-comparison flow、String interpolation、顶层/classifier/companion initializer 与现行
> expression-tail typed traversal；跨文件所有权与 native 链仍按其后继节点推进；`object` / `companion object`
> 常量求值等待候选 guide 启用。
> SPEC-0185 已允许无模块初始化动作的 class/value class/interface/enum class 顶层声明与标量
> entry 共存，但不表示 constructor、nominal operation 或具名 object runtime 已实现。
> 已实现事实以 [`docs/architecture/README.md`](./docs/architecture/README.md) 为准。

---

## 0. 项目定位

| 项目 | 当前结论 |
|---|---|
| 语言与工具 | 语言名 `Koven`；源码扩展名 `.ko`；编译器 CLI `kovenc` |
| 产品 | 使用 Rust 实现的原生 AOT 编译器与配套工具链 |
| 目标语言 | 语法和命名习惯接近 Kotlin，但不承诺 Kotlin 源码兼容 |
| 内存模型 | 借鉴 Rust 的简化单一所有权与借用模型，不等同于完整 Rust 语义 |
| 编译后端 | 计划自建 SSA IR，并通过 LLVM（计划使用 `inkwell`）生成本机代码 |
| 当前阶段 | Phase 0、Phase 1 已完成；Phase 2 已建立单文件名称解析、基础与名义/泛型/interface 类型检查、`when` 穷尽性及 smart cast、条件 `Copyable`、有限内联布局、结构化解构、泛型 callable 实例化、overload-lambda 隔离、顺序容器类型检查、v0.29 nominal/enum/Box constructor typed facts；SPEC-0025/0197 已完成多文件 package/import 名称解析与 compilation-unit 类型检查，包括统一类型身份、完整 signature/body graph、provenance、跨文件 L0092–L0116/L0141、call/construction/member/container/Rc/null-flow/String interpolation、顶层与 member constant initializer 及现行 expression-tail typed facts。Phase 3 已建立整变量 use-after-move、条件复制与结构移动、borrow-default 参数契约、调用期 loan、owned-value ASAP 析构点、顺序容器核心 element place 所有权、简化 closure capture、`Transferable`、编译器绑定跨线程 effect，以及 constructor ordered delivery/root drop facts；SPEC-0198 已启动跨文件所有权，当前完成 unit product/provenance、参数 binding capability、source-qualified call argument contracts、普通 typed call 与 intrinsic container construction 的 shared/exclusive loan 及 Copy/Move/Temporary Value delivery 数据流、intrinsic Rc `share/value` 的 retain/borrow-payload effects、source-qualified constructor ordered delivery/root obligations，以及 source-qualified closure formation、last-use loan、L0137–L0139 与跨线程 `Transferable` 数据流；ASAP drop/return 与 validated gate 继续实施。Phase 4 已完成 owner-aware typed SSA/verifier、封闭标量 frontend→SSA→AArch64 LLVM IR、聚合/容器/闭包后端、目标布局预检、显式 native entry、Mach-O object 与 Clang link/run、SPEC-0040 的 DWARF 行表和真实 LLDB 源码断点命中、SPEC-0184 的 nominal/enum/Box constructor/投影/解构/drop/L0145 native 闭环、SPEC-0195 的 callable Borrow loan operand/frontend LoanFact/LLVM pointer ABI，以及 SPEC-0196 的 pointer-like nullable `if` proof、独立 SSA/verifier、LLVM null niche/conditional drop 和 class/Box/Rc native 闭环；inline nullable、nullable `when` 与 `!!` 消费 lowering 及多文件 native lowering 尚待后续 Spec。Phase 5 已由 SPEC-0042 建立真实 Koven `prelude.ko` 单文件 bootstrap link/run，由 SPEC-0043 把标准 `error(message: String): Nothing` identity 接入既有 Abort，由 SPEC-0189 把首个 `println(String)` Borrow identity、非插值 literal 与真实 Hello World stdout 接入 native runtime，并由 SPEC-0044 在标准源码实现条件 `Copyable` 的 `Pair` / `Result` 与 native 正反验收；SPEC-0045 已完成非 nullable `Rc<T>` construction/share/Copyable payload read、target preflight、retain/release 和真实 native build/run，pointer-like nullable lowering已由 SPEC-0196 完成；SPEC-0192 已完成一般 UTF-8 String owner、连接/相等/动态输出、Borrow/Value/return、aggregate/Rc/顺序容器/move closure capture 与精确析构 native 闭环；interpolation 目前只完成 frontend typed traversal，native lowering 仍按 SPEC-0192 的确定性拒绝边界；顶层 const 目前仍按普通 typed initializer 处理，不启用 v0.36 evaluator；其他 printable 重载与容器增删/重排 relocation API 仍待后续 Spec；完整 `for` 等待 typed iteration plan 与 provider runtime；`object` / `companion object` 常量求值仍有候选 guide 门禁。Phase 6 已提供公开单文件显式 entry、零参数与参数化 conventional `main` 的 `kovenc build/run`、原始 argv 转交、严格 version 1 `project.toml` 到 deterministic immutable base source-set 的内部 provider、单文档 frontend LSP 诊断与语义跳转定义、版本化 JSON Lines 机器诊断、非破坏性 `kovenc format`、TextMate 与 Tree-sitter grammar；跨文件诊断与跳转定义、snapshot→frontend 接线及项目构建仍等待后续 Spec |

除非权威规范明确要求，不得把项目改造成解释器、字节码 VM、JIT、Kotlin 方言或 Rust
语法翻版。AOT、Kotlin 风格语法和简化所有权是三个相互独立的设计维度。

---

## 1. 指令、规范与事实来源

不要把不同职责的文档排成一条可以互相覆盖的总优先级。按以下权限边界工作：

1. 用户当前任务中的明确要求决定本次授权范围。
2. 根 `AGENTS.md` 与作用域更具体的 `AGENTS.md` 规定工作和交付方式；子目录规则只能细化，
   不能静默覆盖根规则。
3. 用户明确指定的现行语言 guide 规定语言语义，以及其中已经强制确定的 Phase 和实现边界；
   当前为 [`docs/guide/`](./docs/guide/00-index.md) 文档集的 v0.32；§30 的 conventional main /
   显式 Rc、§31 的一般 String 与 §32 的多文件 package/import 契约已成为现行语义。
4. 已批准 Spec 规定一次变更的范围与验收；已接受 ADR 只记录 guide 留白处的长期架构选择。
   Spec 和 ADR 都必须服从适用的 `AGENTS.md` 与现行 guide，不能单独覆盖它们。

代码、测试、`Cargo.toml`、`Cargo.lock`、`rust-toolchain.toml` 和 architecture 是判断仓库当前
状态的事实证据，不是更高层规范。它们与规范冲突时应报告为实现或文档漂移，不得用“代码
已经如此”反向覆盖规则。

必须遵守：

- 语言语义以版本化指导文档为唯一真源。根 `AGENTS.md` 只摘录不易误解的设计护栏，
  不复制完整关键字表、运算符优先级表或语法产生式。
- Kotlin、Rust、LLVM 的既有知识只能用于实现技术判断，不能替代本语言规范。
- 权威正文中的明确规则优先于示例；示例与规则冲突时按规则实现并登记文档缺陷。若规范
  未定义、两个明确规则互相冲突，或只能从示例推测语义，必须指出问题并询问；不得静默
  选择，也不得混合成第三套设计。
- 后续文档只有在用户明确指定其取代当前版本时才获得更高优先级；文件名较新不等于已批准。
- 工具链和依赖版本以仓库配置为准。配置不存在时不得臆造 Rust edition、MSRV、LLVM
  版本、目标三元组或发布平台。

### 文档治理

项目通过版本化语言规范、Spec、ADR 与 architecture 推进；各类文档的职责、状态和更新
流程见 [`docs/AGENTS.md`](./docs/AGENTS.md)。任何实现任务都应先定位对应 Spec；涉及长期
架构选择时先记录 ADR；实现完成并验证后，再把 architecture 更新为当前事实。计划顺序和
Goal / 提交边界见 [`docs/specs/README.md`](./docs/specs/README.md)。

---

## 2. v1 语言设计护栏

实现细节必须回到 v0.32 指南核对。以下条目用于阻止常见误读，不替代完整规范：

- Rust 实现代码遵循 Rust 命名约定；目标语言源码遵循 Kotlin 风格。两套命名体系不得混用。
- 源码组织使用 Kotlin 风格的 `package` / `import`；`module` 不是关键字，也不接受 Rust 的
  `mod` / `use` / `::` 或花括号分组导入。v0.32 已定义显式 source root / logical path 映射与
  跨文件名称语义；SPEC-0025 已实现 frontend package/import 名称解析，跨文件 typed 与
  ownership 产物仍按 SPEC-0197/0198 推进。
- 单文件名称解析使用类型 / 值双命名空间；顶层与成员先收集后解析，block local 从
  initializer 完成后可见，嵌套作用域允许遮蔽。同一值作用域的函数形成有序 overload set；
  package/import 展开已由 SPEC-0025 实现；跨文件 member/callable 的 typed 选择仍等待
  SPEC-0197，单文件 member 与 overload 选择已实现。
- `value class` 表示值语义和内联布局，不得描述成“永远在栈上”，也不天然等于可复制。
  它可以包含不可复制字段；仅当全部字段类型都满足 `Copyable` 时才自动满足 `Copyable`，
  否则转交所有权时发生移动。`Copyable` 可作泛型上界但不能由用户手动实现；不可复制字段
  只能投影借用，v1 不支持普通字段部分移动。`Copyable` 类型不得需要复制 glue、retain 或
  唯一析构义务。
- `Nothing` 满足 `Copyable`；nullable、`value class` 与有限 `enum class` 按内部实际类型
  条件推导。无 `Copyable` 上界的类型参数在泛型体内按 move-only 使用。无限 value/enum
  内联递归必须在布局前拒绝。
- 普通 `class` 表示堆分配的引用语义，并受所有权与借用检查约束。`Box<T>` 用于把
  `value class` 显式装箱；v1 的 intrinsic `Box<T>` 只接受具体 `value class` 实例，
  `Box<普通 class>` 与仅由类型参数表示的 `Box<T>` 是类型错误，同名源码 class 不获得
  intrinsic 身份。
- `own` 是硬关键字和声明侧参数 marker：它把参数规范化为内部 `Value` owned binding，不引入
  第四种参数模式。具名函数与函数类型参数无 marker 时是 `Borrow`，也可显式写等价的
  `borrow`；`inout` 仍显式表示独占非 owning binding。调用点无 marker 可按 callee 契约建立
  Borrow，或向 `Value` 参数交付 owned copy / 隐式移动 MoveOnly 值；调用点 `borrow` 只强调
  Borrow，`Inout` 实参必须写 `&`，调用点不接受 `own`。这些形式都不是 Pratt parser 中的
  通用一元运算符，必须由各自的参数专用语法解析。
- v1 采用简化单一所有者和 ASAP 析构，不实现完整 NLL。不得用 Rust 借用检查器的全部
  行为自行补齐本语言规则。
- `move (...) -> T` 只接受不含借用捕获的闭包；跨线程 API 的闭包实参必须显式写
  `move { ... }`，并检查 v1 的 `Transferable` 约束；`Shareable` 延后到 v2。
- `error()` 是返回 `Nothing` 的标准库顶层函数，不是关键字；其语义是 abort，不是可捕获
  异常。`e!!` 脱糖为 `e ?: error("Non-null assertion failed")`。
- v1 不提供 `throw` / `try` / `catch` / `finally` / `throws` 或异常栈展开。可预期、可恢复
  失败使用显式 `Result<T, E>` 返回值；postfix `?` 只传播 `Err` 到最近 callable，函数返回
  类型本身就是失败契约，不另设声明关键字。
- `enum class` 具有 Rust ADT 风格的变体关联数据，不等同于 Kotlin 的普通枚举；`when`
  需要穷尽性检查，并支持分支内 smart cast。
- v1 不接受匿名内部类或 `object` expression；lambda 只实现函数类型。需要多方法接口实现时
  使用具名 class，重复转发可使用指南限定的 `Interface by valField` 静态委托。
- 解构遵循 `componentN()` 命名约定，但右值只求值一次；不可复制 `value class` 的完整解构
  是一次消费源值的结构化所有权转移，必须覆盖全部分量，不得机械展开为多次独立方法调用。
- `Indexable` / `MutableIndexable` 与 `Map` / `MutableMap` 是两套契约，禁止为了复用而合并。
- v1 使用单态化静态分发。`dyn` 虽为保留字，但动态分发是 v2 能力。
- 具名 `object` 是有唯一值的名义 singleton，可有普通成员函数，但运行时存储状态与惰性
  初始化延后到 v2；`companion object` 只是可选的类型级关联命名空间，不产生
  `Type.Companion` 值，只允许 `const val` 与无 `this` 的关联函数。
- interface 可在 companion 中声明类型级 `const val`，通过 `Interface.NAME` 访问；常量不被
  实现类继承或 override。每实现类型不同的 associated const 延后定义。
- 具名函数的表达式体必须显式声明返回类型；无体或 block body 省略返回标注时精确固定为
  `Unit`，不是从函数体推导返回类型。
- 缺 `else` 的 `if` 只允许作为完整 statement element；任何需要值的上下文都必须有
  `else`。lambda 是独立的 `return` 边界，裸 `return` 退出最近的 lambda 或具名函数；v1
  不支持标签或从 lambda 非局部返回外层函数。
- 不支持类实现继承；`super<Interface>.method()` 只用于接口默认方法冲突消歧义。
- v1 不支持自定义属性 getter / setter，也不向用户开放自定义 `infix fun`。
- v1 不提供运行时反射或 RTTI；`is` / `as` 仅用于编译期已知的类型层级。
- v1 文件与网络 IO 仅采用同步阻塞模型；异步 IO 等待后续协程规范。
- 指南列出的未来保留字即使尚无语义，也必须在 lexer 阶段禁止用作标识符。

### 明确不提前实现

| 版本 | 非当前范围 |
|---|---|
| v2 | `dyn` 动态分发、泛型型变、`Shareable`、`object` / `companion object` 的运行时状态或惰性初始化、区间切片、自定义 allocator、完整跨线程借用数据流分析 |
| v3（推荐方向） | 协程；当前推荐 `async` / `await` + `Future`，具体语义、执行器和排期等待后续规范 |
| v4+ | 编译器自举 |
| 未排期 | 未被规范定义的 `sealed`、`actor`、`spawn`、`yield`、`macro`、`reify` 等语义 |

保留关键字不等于授权实现对应特性。只有当前任务和权威规范共同确定的范围才能进入代码。

---

## 3. Cargo workspace

workspace 已按 [ADR-0002](./docs/adr/0002-bootstrap-workspace-layout.md) 建立五个 member：
`lang-frontend`、`lang-codegen`、`lang-cli`、`lang-lsp`、`lang-std`。实际目录、target 和依赖
见 [Architecture](./docs/architecture/README.md)。没有明确需求时不要额外拆出 HIR、MIR、
runtime 等 crate。

### Workspace member 职责

| Member | 职责 | 边界 |
|---|---|---|
| `lang-frontend` | source/span、lexer、parser、索引式 AST、名称解析、类型检查、所有权检查、诊断 | **禁止**依赖 `inkwell` 或任何 LLVM crate |
| `lang-codegen` | 自建 SSA IR、lowering、LLVM IR、目标文件生成 | LLVM 细节只能从此边界向内扩散 |
| `lang-cli` | 编译流水线编排、诊断渲染、进程退出码 | 不承载 lexer、类型检查或 codegen 核心算法 |
| `lang-lsp` | 复用 frontend 提供诊断、跳转等编辑器能力 | 不复制 parser / type checker |
| `lang-std` | 从第一天开始以目标语言自身编写的标准库源码 | 单文件早期 bootstrap 已由 ADR-0012 / SPEC-0042 在 CLI 内部 driver 实现；SPEC-0043 已接入标准 `error()` Abort，SPEC-0189 已接入 literal-only `println(String)` stdout，SPEC-0044 已实现 `Pair` / `Result`；一般 String/IO API 与后续线程 runtime 支撑仍按独立 Spec 推进 |

依赖必须单向、无环。通常由 codegen 和 LSP 复用 frontend，由 CLI 编排 frontend 与
codegen；frontend 永远不能反向依赖外围工具。跨 crate API 才使用 `pub`，其余保持最小
可见性。

Phase 0 的可执行性要求：

- 每个 Cargo member 已有有效 target；后续不得退化为只有 `Cargo.toml`、执行 `cargo check`
  会报 “no targets specified” 的无效 package。`lang-std` 使用最小 Rust library 承载 Cargo
  边界，标准库公共实现仍以 `koven/**/*.ko` 为唯一真源。
- 语言 fixture 必须由某个真实的 Cargo test target 或明确的测试 runner 枚举执行；虚拟
  workspace 根目录中的 `tests/fixtures/` 不会被 Cargo 自动发现，不能把目录存在等同于
  测试已接入。

workspace 持续遵守：

- 公共 edition、MSRV、lint、共享依赖版本和发布 / license 策略应在根配置集中管理；license
  未选定时保持不可发布，不得虚构许可证标识。
- 编译器属于最终用户应用，应提交 `Cargo.lock`。
- 新增依赖前先阅读现有 manifest / lockfile，并说明它解决的具体问题；优先标准库和已有
  依赖。LLVM 与 `inkwell` 必须先核对兼容矩阵，不能只追逐单个 crate 的最新版本。
- 不默认启用全部 feature；互斥 LLVM feature 和平台 feature 应由后续 CI 矩阵明确验证。

### 开源复用与依赖治理

避免“非我发明”（NIH）式重复实现，也避免为了少写几行代码无条件增加依赖。复用的目标是
降低完整生命周期的实现、验证、安全和维护成本，不是最小化当前 diff。任何开源组件或模式
都不能覆盖现行 guide、已批准 Spec、已接受 ADR、crate 边界或 Koven 的可观察语义。

#### 复用决策顺序

实现通用能力前按以下顺序判断：

1. 明确本次真正需要的行为、性能边界、错误模型、平台范围和验收标准；不能用“某 crate
   提供什么”反向扩张需求。
2. 先检查 Rust 标准库、workspace 已有依赖和仓库内已存在的公共能力，避免同一问题出现
   两套实现。
3. 对 CLI / 协议类型、序列化、Unicode 数据、文件遍历、测试支持、目标平台信息等通用基础
   能力，优先调研成熟开源组件；对 parser 算法、arena、图算法、SSA verifier 等已知模式，
   优先借鉴经过验证的设计与不变量，而不是凭记忆重新发明。
4. 对非平凡或长期依赖，至少比较“复用候选”和“最小自研”两类方案，按总拥有成本选择。
   评估结论写入对应 Spec；会长期影响多个 crate、构建方式或发布边界时先写 ADR。
5. 只有在现有方案无法满足已批准语义、引入成本明显高于最小实现、维护或许可风险不可接受，
   或该能力本身属于 Koven 核心差异时才自研，并记录不采用现有方案的具体原因。

语言语法与语义、类型和所有权规则、稳定诊断含义、自建 SSA IR 契约及编译阶段边界必须由
项目掌控。可以复用库、算法和实现模式，但不得把第三方默认行为当成 Koven 规范，也不得为
迁就组件而改变语言语义。相反，成熟的安全 Rust 封装、协议数据模型和通用基础设施在满足
边界时应优先采用，不因“自己写更直接”而重复实现。

#### 新依赖准入检查

新增直接依赖前，必须在 Spec 技术方案或变更说明中记录它解决的问题，并完成与风险相称的
检查：

- **适配度**：API 和行为是否直接满足当前需求，是否会迫使项目引入未授权功能或大范围
  glue code；只使用很小一部分功能时，评估最小自研是否反而更清晰。
- **兼容性**：核对 Rust edition / MSRV、目标平台、LLVM 版本、feature 组合以及与现有依赖
  的兼容关系；不能仅依据 crate 的最新版本号选择。
- **维护性**：查看发布与维护活跃度、API 稳定性、问题响应和升级路径。单一维护者不自动
  否决，但关键依赖必须有可替换边界或明确的维护预案。
- **供应链与安全**：检查直接和传递依赖、已知安全通告、`unsafe` 范围、`build.rs`、过程
  宏、原生库和下载行为。构建脚本与过程宏属于构建时执行代码，不能按普通数据依赖忽略。
- **许可**：核对组件及传递依赖的许可证是否与项目预期分发方式兼容；许可证不明、要求无法
  满足或来源不可追溯时停止引入，不得先合并后补手续。
- **成本**：评估默认 feature、传递依赖数量、编译时间、二进制体积、运行时开销和平台工具
  要求；不能用一个重型框架解决局部小问题。
- **质量属性**：确认错误处理、panic 行为、确定性、线程安全和性能边界符合编译器要求；
  用户输入路径仍必须产生结构化诊断，不能因第三方组件而降级为崩溃。

依赖版本由根 manifest 集中管理并由 `Cargo.lock` 固定可复现结果。禁止通配版本和未固定
branch 的 Git 依赖；确需未发布修复时固定到明确 commit，记录原因、上游链接和退出条件。
feature 只启用当前使用且已验证的最小集合；不机械关闭全部 default feature，也不为方便启用
`all-features`，两种选择都必须基于实际依赖图和平台矩阵。

#### 集成、借鉴与后续维护

- 第三方 API 只在实际边界处隔离：当组件不稳定、含 `unsafe` / native 交互、会跨多个 crate
  泄漏实现细节，或项目需要收敛错误模型时使用薄 adapter。单一局部调用不为“以后可替换”
  额外创建 facade、trait 或通用抽象。
- 测试锁定 Koven 依赖该组件的语义、错误路径和确定性，不复制上游测试，也不把“上游已有
  测试”当成本仓库免测理由。升级依赖时审阅 manifest / lockfile 差异和上游变更，运行受影响
  的窄测试及 workspace 基线；禁止无审阅批量升级。
- 优先引用公开 API 并向上游反馈通用修复。长期 fork、vendoring 或私有补丁必须有明确理由、
  固定上游版本、保留更新方法；若它改变长期构建或发布模式，必须通过 ADR。
- 借鉴架构或算法模式时，在关键不变量或非显然设计处注明来源与适用差异。若复制或改编开源
  代码，必须记录来源 URL、版本 / commit 和许可证，保留所需版权及 NOTICE；“参考过”不能
  作为复制不兼容许可证代码的理由。
- 若依赖停止维护、出现无法缓解的安全问题、破坏确定性或持续阻碍 MSRV / 平台目标，应提出
  替换或移除计划；不得因已有调用点较多而默认永久保留。
- 每个依赖和复用层都必须能追溯到当前 Spec 的需求。交付时说明新增或复用的组件、启用的
  feature、关键取舍和已执行验证；未完成的安全、许可或平台检查必须显式报告，不能写成通过。

---

## 4. 编译器实现边界

默认流水线如下，改变阶段或引入新的中间表示属于架构决策：

```text
源码 → Lexer → Parser / AST → 名称与类型检查 → 所有权检查
     → 自建 SSA IR → LLVM IR → 目标文件 → 链接后的本机可执行文件
```

- AST 使用索引式节点；节点、token、类型错误和所有权错误必须保留可追溯的源码 `Span`。
- 每个阶段接收明确输入并返回明确产物与诊断，不通过隐式可变全局状态交换结果。
- frontend 的语义模型保持 LLVM 无关；不得为了方便 codegen 把 LLVM 类型泄漏进 AST 或
  类型检查 API。
- Parser 按指南中的优先级实现 Pratt parser。优先级数值只在一个实现位置定义，并由测试
  锁定；不要在多个模块复制表格。
- 用户输入不可信。非法源码必须形成结构化诊断，不能触发编译器 `panic!`。
- 诊断和产物必须确定性。不要依赖 `HashMap` / `HashSet` 的随机迭代顺序；输出前排序，
  或在顺序属于语义时使用有序数据结构。
- 不要用大范围 `clone()` 绕过所有权设计问题。优先使用节点 ID、索引、切片或清晰的阶段
  所有权；确需复制时在评审中说明成本和原因。
- 编译器 bug 与用户程序错误必须区分。内部不变量可以返回内部错误或在有充分说明时使用
  `expect`；禁止对正常用户输入路径使用无说明的 `unwrap()` / `expect()`。

---

## 5. Rust 编码规范

- 以 `rustfmt` 为格式标准，以 Clippy `-D warnings` 为静态检查基线。
- 模块、函数、变量用 `snake_case`，类型和 trait 用 `UpperCamelCase`，常量用
  `SCREAMING_SNAKE_CASE`。不要把目标语言的 Kotlin 风格命名带入 Rust API。

### 模块职责与文件规模

- 一个模块只能有一个主要变化原因，并明确其输入、输出、状态所有者和核心不变量。按语法
  领域、编译阶段、状态机或恢复边界拆分，不得按文件行号机械切片，也不得为单次调用额外
  创建抽象层。
- `mod.rs` 优先作为稳定门面，集中声明模块结构、公共入口和必要的 re-export；实现细节放入
  职责明确的子模块。子模块保持最小可见性和单向依赖，禁止以循环依赖或扩大 `pub` 范围
  迁就拆分。
- 禁止创建没有明确领域含义的 `utils.rs`、`common.rs` 等杂物模块。只有被多个职责真实复用、
  且自身不变量可以独立描述和测试的代码才能提取为共享模块。
- 手写生产 Rust 文件以 **1000 个物理行为软上限**。新文件原则上不得超过该上限；确有必要
  超过时，必须在对应 Spec 中记录保持内聚的理由、已评估的拆分方案和退出条件。
- 既有超限文件不是自动整体重写的理由，也不得继续承载新的独立职责。功能变更会扩大超限
  文件或引入新的变化原因时，应优先提取与本次需求直接相关的职责；行为保持的大型重构须
  作为独立 Spec，先用测试锁定行为，再分阶段提取。
- 测试矩阵、工具生成文件和因集中维护而具有单一真源价值的数据表不机械受 1000 行约束，
  但仍必须遵守职责内聚原则。行数只是审查信号，不能替代对耦合度、复杂度和可测试性的判断。

### 注释与日志

- 公共 API 必须有必要的 rustdoc，并注明关键不变量、错误条件和所有权约定。私有代码只在
  状态机转换、错误恢复边界、`Span` 所有权、复杂度约束或非显然取舍处增加注释；不要求每个
  方法都有注释，禁止逐行翻译代码或保留已经失效的说明。
- 库层代码禁止直接使用 `println!`、`eprintln!` 或 `dbg!`。用户源码错误只生成结构化
  `Diagnostic`，不得为了便于调试同时写入运行日志；面向用户的 stdout / stderr 输出由 CLI
  等可执行边界统一渲染。
- 有真实可观测性需求时，必须通过独立 Spec 和依赖准入检查后再引入日志组件，优先评估
  [`tracing`](https://docs.rs/tracing/latest/tracing/) 的结构化 event 与 span；subscriber 只能
  由 CLI、LSP 等可执行边界配置，库 crate 不得自行安装全局 subscriber。
- 日志级别采用统一语义：`error` 表示内部操作失败，`warn` 表示可继续但发生降级，`info`
  只记录编译阶段级生命周期，`debug` 记录决策与计数，`trace` 才允许按需记录 token / node
  级细节。正常用户诊断不属于 `error` 日志。
- 日志不得参与控制流、作为测试判定或形成稳定用户协议，也不得默认记录完整源码、敏感内容
  或机器绝对路径。优先记录 `SourceId`、`Span`、阶段名和数量等结构化字段，并确保关闭日志时
  不改变编译结果、诊断顺序或性能复杂度。

### 通用实现规则

- 优先 early return 降低嵌套；先写满足当前 Phase 的最小实现。
- 使用具体的错误类型和 `Result` 传播可恢复错误；不要把所有错误压成字符串，也不要吞错。
- `unsafe` 只能存在于无法用安全 Rust 表达的最小边界。每个 `unsafe` 块前必须有
  `// SAFETY: ...`，说明调用方义务、被维护的不变量和安全依据；优先封装成安全 API。
- 不写与需求无关的重构，不顺手清理旧代码。只删除本次改动造成的未使用项。
- 不引入“未来可能需要”的配置、trait、feature flag 或通用框架。
- 修改前先读目标模块的公开接口、直接调用方、测试和共享类型；遵循仓库已有风格。

---

## 6. Phase 驱动开发

每项工作开始时必须指出所属 Phase，并把需求转成可验证目标。严格按依赖关系推进：

| Phase | 交付重点 | 最低验收方向 |
|---|---|---|
| 0 | Cargo workspace、索引式 AST、诊断框架、语言测试骨架 | workspace 与骨架可检查；不实现临时 parser，解析从 Phase 1 按阶段落地 |
| 1 | Lexer / Parser、错误恢复 | 指南范例可完整解析；错误有稳定代码及准确行列 |
| 2 | 类型检查、smart cast、穷尽性、`Nothing`、条件 `Copyable`、解构 | 正反例均产生预期类型结果或诊断 |
| 3 | 所有权 / 借用、复制与移动、跨线程 `Transferable` | 拒绝 use-after-move、重复可变借用和借用闭包跨线程 |
| 4 | SSA IR、LLVM codegen、析构、闭包环境、DWARF | 生成并运行本机程序；可用 `gdb` / `lldb` 单步调试 |
| 5 | 最小标准库 | 标准库的目标语言测试全部通过 |
| 6 | 包管理、LSP、格式化、TextMate / Tree-sitter | 各工具有独立、可重复的验收用例 |

一个任务若跨 Phase，只实现其已授权且依赖完备的部分，并明确剩余项。不要为了演示端到端
结果而加入临时语义，除非临时实现被明确标记、测试隔离且用户同意。

每个实现 Goal 必须对应一份已批准 Spec，并以至少一个不混入其他 Spec 的提交结束；提交信息
包含 `SPEC-NNNN`。一个 Spec 可以有多个始终可验证的提交，但一个提交不得跨多个 Goal。
实现、测试、Architecture、Spec 验收和提交全部完成后，才能把 Goal 标记为完成。纯 guide、
ADR 或文档治理任务按 §10 可不建 Spec，但用户要求提交时仍应形成范围单一的文档提交。

在其授权范围和有效期内的用户站立授权可满足后续 Spec 批准和 ADR 接受所需的确认，无需
逐份重复询问，也无需为 `approved` / `accepted` 单独创建纯状态提交。状态门禁和全部可执行
验收仍须满足：批准可以自动化，验收可以自动执行，但不能自动视为通过。站立授权不替代新
语言 guide 的版本级明确启用。

### 每次任务的固定流程

1. **读取**：确认现行 guide、当前 Phase、相关接口、直接调用方与已有测试。
2. **界定**：写明假设、非目标、成功标准和预计修改范围；不清楚的语义先询问。
3. **实现**：做最小且可回溯的修改；每个行为变化同步添加能失败的测试。
4. **验证**：按 §9 分层运行窄测试、受影响 Phase/Spec 套件和 workspace 兼容性检查；只有命中
   全量触发条件时才运行约 50 分钟的 workspace 全量测试。检查实际退出状态。
5. **同步**：更新 Architecture、Spec 验收状态和验证记录，确保只陈述实际事实。
6. **提交**：检查 staged diff，创建当前 Goal 的独立提交；提交成功后才结束 Goal。
7. **交付**：报告改了什么、运行了什么、提交、哪些检查没跑及原因、仍有哪些不确定性。

---

## 7. 诊断规范

诊断质量优先于表面上的语法覆盖率：宁可准确拒绝，也不要错误接受或崩溃。

- 从第一天分配稳定错误码，格式为 `L` 加四位数字（如 `L0001`）。错误码在集中注册表中
  定义，不得在不同模块散落硬编码。
- 一个错误码只表达一类稳定语义。不得复用已发布错误码表达新含义；修改含义需要同步更新
  权威文档和变更记录。
- 每条用户诊断至少包含错误码、主消息、主 `Span`；必要时增加关联位置、说明和可操作建议。
- 行列、Unicode 和多行范围计算必须由统一 source/span 基础设施完成，不能每个阶段各写一套。
- 语法错误恢复不能伪造后续语义；级联诊断应受控，顺序必须稳定。
- 机器可读诊断遵循 ADR-0014 的 schema v1 JSON Lines；颜色策略、完整错误码分配和通用 build
  event 协议尚未确定，未经文档批准不得扩张或改变公共协议。

---

## 8. 测试规范

每个语言功能必须同时测试“接受什么”和“拒绝什么”，测试要编码设计意图，而不仅是当前
实现输出。不设置脱离风险和语义的统一覆盖率百分比；覆盖目标是可观察行为、核心不变量、
错误路径和复杂度边界，而不是单纯让每一行代码被执行。

- Lexer / Parser：token、AST、优先级、正例、反例以及单点错误后的恢复测试。Scanner、
  Parser 等状态机还应按风险覆盖 EOF、Unicode、嵌套 owner、异常 token、确定性和必要的
  复杂度边界。
- 类型与所有权：compile-pass / compile-fail 用例；失败用例至少断言错误码和关键 `Span`。
- 编译器内部算法：就近放 Rust 单元测试，覆盖不变量和边界条件。
- 跨阶段流水线：放集成 fixture，覆盖源码到预期诊断、IR、运行输出或退出码。
- Codegen：除检查 LLVM 文本外，还要编译并运行可执行文件核对行为；调试信息按 Phase 4
  标准在 Linux 用 `gdb`、macOS 用 `lldb` 验证。
- 新语法至少包含一个最小正例、一个最小反例、一个与相邻语法组合的回归用例，以及在发生
  错误恢复时对后续节点、精确错误码和关键 `Span` 的断言。
- 拆分大型模块前必须先建立能锁定公共 API、AST、诊断内容与顺序的 characterization tests。
  每次提取后先运行受影响的窄测试；全部提取完成后运行对应 Spec 提交门禁，只有 §9 的全量
  触发条件成立时才运行 workspace 全量测试。不得在同一
  重构中静默改变可观察语言行为。
- 私有不变量使用就近单元测试，跨模块公共行为使用集成测试和 pass / fail fixture。共享测试
  工具只能承载真实复用逻辑；测试文件即使不受物理行软上限约束，也应按行为领域拆分，避免
  把 fixture runner、断言工具和无关功能矩阵堆入同一文件。
- Golden / snapshot 更新必须人工审阅差异，禁止用批量接受命令掩盖非预期变化。
- 测试不得依赖机器路径、随机 Hash 顺序、时区或非固定随机数；确有平台差异时显式分层。
- Fixture harness 必须证明它实际枚举并运行了预期用例；零 fixture 应视为配置错误而失败，
  防止 `cargo test` 全绿但没有执行任何语言测试。

正式源文件扩展名为 `.ko`，语言 fixture 也必须使用 `.ko`；不得虚构尚不存在的测试 runner
命令。

---

## 9. 构建与分层验收

`cargo test --workspace --all-targets` 会同时执行功能回归、Parser/Lexer 变异矩阵、压力边界和
外围 crate 测试，当前一次约需 50 分钟。它是里程碑/高风险全量门禁，不是每次局部实现的默认
反馈环。验收强度按变更影响面决定，不得为了节省时间跳过真正受影响的套件，也不得用无关的
全量测试代替能直接证明需求的定向断言。

### 先确定影响面，不按命令习惯选测试

每次验证前先从 diff 建立最小验收集合，并把选择结果写入 Spec 验收记录或交付说明：

1. **直接行为**：运行新增/修改测试，以及能直接证明本次正例、反例和错误恢复的最近测试。
2. **共享契约**：运行被修改公共类型、阶段产物或 helper 的最近既有调用方套件；不能只运行
   新增测试。
3. **下游兼容**：用 workspace `check` 覆盖跨 crate 编译；只有公开 API 或行为真正影响下游时，
   才追加对应下游 integration/native 测试。
4. **高成本边界**：名称含 `matrix`、`stress`、`large` 的 target 通常属于穷举、变异或压力资产，
   默认留给第 3 层；若本次直接修改其生产路径、不变量、harness 或共享断言，它就是受影响套件，
   不能因耗时而排除。

文件名只是成本提示，不是豁免规则。无法说明某个套件为何受影响或为何不受影响时，先扩大到
对应 crate/Phase 套件；仍无法界定才升级到第 3 层。不得维护一个脱离 Spec 和 diff 的永久“快速
白名单”，否则新增测试会被静默漏掉。

默认门禁按变更类型选择；Spec 状态变化本身不等于风险升级：

| 变更类型 | 提交前最低门禁 | 追加检查 |
|---|---|---|
| 仅 Markdown，且不改变 guide / Spec 语义或验收边界 | 文档自检 | 检查链接、术语、版本和 diff |
| crate 内局部实现、私有 helper、定向缺陷修复 | 第 1 层迭代，第 2 层提交 | 运行直接行为与最近共享契约套件 |
| 可界定的公开跨 crate API 或 Phase 产物 | 第 2 层 | 追加所有已知直接下游的 integration/native 测试 |
| 第 3 层触发项或无法可靠界定的影响面 | 第 3 层 | 记录总耗时、最慢 target 与 ignored/skipped 项 |

低风险 Spec 达到 `done` 时，只要其验收矩阵已被第 2 层完整覆盖，不因状态变化单独升级到第 3 层；
不得反向把一个实际高风险变更拆成多个“小 Spec”来规避全量门禁。

### 第 1 层：编辑反馈环

每次行为修改先运行最窄且能复现设计意图的测试，并检查受影响 crate 能编译：

```bash
cargo fmt --all -- --check
cargo check -p <affected-crate>
cargo test -p <affected-crate> --test <affected-suite> <affected-test-name>
```

可用 Rust 单元测试时改用 `cargo test -p <affected-crate> --lib <test-name>`。测试过滤导致其他
用例显示 `filtered out` 是正常的，但交付记录必须说明这是定向测试，不能写成套件或全量通过。

### 第 2 层：Spec 提交门禁

形成一个可提交的 Spec 切片前必须运行：

```bash
cargo fmt --all -- --check
cargo clippy -p <affected-crate> --all-targets -- -D warnings
cargo test -p <affected-crate> --lib
cargo test -p <affected-crate> --test <affected-suite-1> --test <affected-suite-2>
cargo check --workspace --all-targets
cargo test --workspace --lib --bins
cargo build -p lang-cli
```

`<affected-suite-*>` 由 Spec 验收矩阵和实际依赖面决定，必须至少包含行为正反例、最近公共调用方
和一个既有回归套件；不得固定成一个永远不变的“快速名单”。修改公开跨 crate API 时，追加直接
下游 crate 的相关 integration tests；修改 LLVM/native 行为时，追加对应 codegen/CLI build-run
验收。只改 Markdown 时不机械运行 Rust 门禁，按文档规则检查链接、术语、版本与 diff。

### 第 3 层：高风险与里程碑全量门禁

仅在以下任一条件成立时运行完整 workspace 测试与严格静态检查：

- Phase/guide 版本启用、release 或明确的合并/发布门禁；
- Spec 的验收矩阵明确要求全量，或 Spec 完成时实际修改命中本节其他高风险条件；仅从
  `in-progress` 变为 `done` 不单独触发全量；
- 修改 Cargo workspace/member、共享依赖/feature，或公共 crate API 的影响面无法由已知直接下游
  编译与定向测试可靠覆盖；
- 修改 Lexer/Parser 通用状态机、错误恢复 owner、source/span、AST arena、诊断排序/catalog、fixture
  harness 或变异测试基础设施；这些变化会影响大量 adversarial/matrix tests；
- 定向或 Phase 套件出现非局部失败，说明原影响面判断过窄；
- 用户明确要求全量验证。

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cargo build -p lang-cli
```

未命中上述条件的普通 Spec 中间提交，不再默认支付约 50 分钟的全量成本；第 2 层通过即满足
提交门禁。CI/nightly 可额外周期性执行第 3 层，但 CI 结果只有在能定位到当前 commit 且实际完成
时才可作为验收证据。

### 耗时治理与重复执行

- 保留 Cargo 增量产物；普通验证不得先运行 `cargo clean`。依赖和 lockfile 未变化且本机缓存完整
  时可追加 `--locked --offline`，减少网络和依赖解析噪声；缓存不完整时不得把 offline 失败误报为
  代码失败。
- 同一源码状态上已经成功且覆盖面相同的命令不重复运行；后续仅改文档或测试断言时，重跑受影响
  target 和被该改动失效的门禁，并在记录中说明复用的是哪次结果。生产代码、依赖或 feature
  再次变化后，旧结果失效；CI 证据还必须绑定到同一 commit。
- 单个测试 target 预计或实测超过 10 分钟时单独执行并记录耗时，避免其超时掩盖其他失败。一次
  异常慢或超时先定位具体 target；禁止不分析就反复重跑整个 workspace。
- 本地不并发启动多个争用同一 Cargo target 目录的验收命令。CI/nightly 可按 Cargo test target
  分片并行执行慢矩阵，所有分片必须绑定同一 commit，保留各自退出状态；只有全部必需分片成功
  才等价于第 3 层通过。
- 每次第 3 层运行记录总耗时和最慢 target。若同一 target 连续三次超过其近期基线的 1.5 倍，或
  全量持续超过 50 分钟，应建立独立性能治理 Spec；优先减少重复 parse/fixture 构造或安全分片，
  不得通过删除断言、降低矩阵规模或放宽语义来换取速度。

执行规则：

- 当前没有根 `Cargo.toml` 时，这些命令不可运行；应如实报告“工程骨架尚未创建”，不能声称
  检查通过。
- 修改 Rust 代码后至少运行第 1 层；形成提交前运行第 2 层；只有命中条件时运行第 3 层。
- 只改 Markdown 时做链接、术语、旧项目残留和 diff 自检，不为通过检查而创建空 Cargo 文件。
- LLVM、本机链接器或调试器缺失时，先完成所有不依赖它们的检查，再明确报告环境阻塞；
  不得把未执行写成通过。
- 不默认添加 `--all-features`。feature / target 矩阵应在工具链和 CI 策略确定后单独记录。
- “测试通过”必须给出实际执行命令；有 ignored / skipped / filtered 用例时明确说明。
- 被中止、超时、仍在运行或只输出部分 target 的命令都不得记录为通过；应记录到最后一个已完成
  层级，并明确更高层未完成的原因。

---

## 10. 语言与架构变更

- 新增或改变关键字、语法、类型规则、所有权规则、标准库契约或 Phase 验收时，必须提升
  `docs/guide/` 文档集版本，并同步更新索引、对应章节、关键字表（如适用）和
  `07-changelog-archive.md`；只有用户明确指定后，新版本才成为现行真源。纯勘误可直接修正
  当前版本。
- 新功能或可观察行为变化必须先有可验证的 Spec；改变 workspace 边界、IR 层级、后端
  方案、ABI 或长期开发模式时，先写 ADR。实现完成后同步更新受影响的 architecture 快照。
- 根 `AGENTS.md` 记录长期工程规则；guide 记录语言语义以及其中强制确定的 Phase / 实现
  约束；architecture 记录当前实现；ADR 记录 guide 留白处为何这样决策；测试提供可执行
  证据。不要让同一规则在多处各自维护完整副本。
- 纯 bug 修复不必创建新语言版本，但必须有能复现 bug 的回归测试；若修复改变既有规范语义，
  它就不是纯 bug 修复，必须先处理规范冲突。
- 未经用户要求，不改动与任务无关的文档版本，也不自行宣布未批准的设计为正式规范。

---

## 11. 已知未决问题与待同步事项

以下问题等待后续指导文档。任务没有触及时不需要停工；一旦触及，必须先确认，不能猜：

1. `lang-std` 已确定使用目标语言源码；ADR-0008 已决定当前系统分配不增加 Rust allocator
   shim，ADR-0012 / SPEC-0042 已完成单文件早期 bootstrap；线程、IO 等后续 runtime 支撑边界
   仍待对应 Spec。
2. package 到 source root / 文件的映射已有 ADR-0005；import 冲突、跨 package 可见性、public
   FFI、增量编译与跨平台发布策略仍未封闭；首个 macOS Clang linker 边界已由 ADR-0010 实现。
3. v0.26 已定义同步调用期 loan 与 ASAP 析构点；跨调用借用/借用返回、后续 SSA 指令增量和
   调试映射仍待对应 guide/Spec/ADR。
4. LLVM 21 / Inkwell 0.10 与首个 AArch64 macOS target 已由 ADR-0007 固定；机器诊断已由
   ADR-0014/SPEC-0060 固定首版 JSON Lines，完整 build event、多目标矩阵、包清单与锁文件
   schema 仍未确定。
---

## 12. 禁止事项

| 禁止行为 | 原因 |
|---|---|
| 凭 Kotlin 经验补齐本语言语义 | “语法相似”不代表语义兼容 |
| 直接照搬完整 Rust 所有权、生命周期或 trait 规则 | 本项目采用的是版本化的简化模型 |
| 在多处复制关键字表或运算符优先级表 | 会形成多个互相漂移的真源 |
| 让 `lang-frontend` 依赖 LLVM / `inkwell` | 破坏前端与后端边界及 LSP 复用 |
| 用 `panic!`、无说明的 `unwrap()` / `expect()` 处理非法用户源码 | 用户错误必须成为结构化诊断 |
| 用无序集合的迭代结果直接生成诊断或稳定产物 | 输出会不确定，测试和工具链不可复现 |
| 用大范围 `clone()` 掩盖 AST / IR 所有权设计问题 | 增加内存成本并隐藏阶段边界缺陷 |
| 提前实现 v2 / v3 / v4+ 或未排期保留字的语义 | 扩大范围并固化未经批准的设计 |
| 未更新测试和 guide 就新增语言行为 | 实现、规范和诊断会失去可追溯性 |
| 无 `// SAFETY:` 依据引入 `unsafe` | 无法审查安全不变量 |
| 无条件批量接受 snapshot / golden 变化 | 可能掩盖解析、诊断或 codegen 回归 |
| 绕过 Spec 直接实现新功能，或架构变化后不更新 ADR / architecture | 需求、决策、实现与当前事实将失去可追溯性 |
| 未运行或跳过检查却宣称“全部通过” | 完成状态必须有可复核证据 |

---

## 13. Agent 交付格式

每次完成实现或评审时，简要说明：

1. 结果与所属 Phase。
2. 修改的文件及其必要性。
3. 新增或更新的测试，以及它们验证的设计意图。
4. 实际运行的检查命令和结果。
5. 未运行、被跳过或尚不确定的事项及原因。

“完成”只用于请求范围已实现、相关验收标准已满足且没有静默跳过必需检查的情况。
