# Koven 架构快照

本目录描述仓库**当前已经实现**的架构。设计原因记录在 [`../adr/`](../adr/)，单次交付范围
记录在 [`../specs/`](../specs/)，语言语义由
[`../guide/00-index.md`](../guide/00-index.md) 导航的现行 v0.34 文档集定义。v0.34 已在完整
v0.33 基线上启用；SPEC-0213/0214/0054 与 receiver 链的 Parser、typed、ownership 切片
SPEC-0201/0180/0181 均已完成。
本文件只把已落地代码写成实现事实。class-family 与
窄化接口委托已分别由 SPEC-0017、SPEC-0064 实现；SPEC-0018 已建立单文件名称解析，
SPEC-0019 已建立基础类型检查，SPEC-0020 已建立名义/泛型/interface 类型检查。
SPEC-0021 已建立 enum case type、`when` 穷尽性与 flow-sensitive smart cast；SPEC-0022 已
建立条件 `Copyable`、有限内联布局、intrinsic `Box` 与结构化解构类型事实；SPEC-0067 已
建立单态 callable/member 选择、实参映射与类型层面 place 分类；SPEC-0023 已建立顺序容器
类型、核心构造和 element-place 类型事实；SPEC-0178 已检查 `break` / `continue` 的最近词法
loop 与 callable boundary，并以 L0142 拒绝无目标 jump；SPEC-0183 已发布 nominal、enum case
与 intrinsic `Box` 的构造目标、完整实例和有序 Value operand typed facts，并以 L0143–L0144
拒绝非法目标和不完整/冲突推断；SPEC-0027 已建立整变量所有权状态与 use-after-move 检查；
SPEC-0028 已建立条件复制、消费式解构和结构分量移动检查；
SPEC-0173 已让唯一期望函数类型的 lambda 采用 Value/Borrow/Inout 参数契约，并发布稳定
parameter binding typed facts；
SPEC-0175 已让 block 内未分组 lambda 实参优先进入 expression parser，不再被 outer block stop
误判为空实参；
SPEC-0176 已把普通 callable 与 function-type 的无 marker / 显式 `borrow` 参数规范化为
`Borrow`，并以声明侧 `own` 形成既有 `ParameterMode::Value`；
SPEC-0029 已建立参数 binding 能力、名称/字段 place、同步调用期 loan、L0133–L0135 与
owned-value ASAP drop facts；
SPEC-0181 已补齐带 assignment/field 双重身份的普通 MoveOnly 字段 replacement drop facts；
SPEC-0030 已建立顺序容器构造效果、逻辑 element place、L0136、元素 loan/replacement 与
旧元素 drop facts；
SPEC-0032 已建立解析身份驱动的 closure capture、borrowed/move formation effect、逃逸与
跨线程 `Transferable` 检查、L0137–L0139、capture loan 和 closure/capture drop facts；
SPEC-0034 已完成标量 frontend→verified SSA→verified AArch64 LLVM IR 的封闭垂直切片；
SPEC-0035 已完成聚合/heap-owner SSA、target DataLayout、系统 allocation、heap place 与递归
drop/free 后端基元；
SPEC-0036 已完成顺序容器后端：container kind/element type、完整 construct/generate、length、
checked element-place、replace 与 drop 的 SSA/verifier，以及固定 header、连续缓冲区、
checked-index、replace/drop 与 MoveOnly ZST runtime 均已落地；
SPEC-0186 已在任何 LLVM 复合类型创建前加入 target-layout preflight：从同一 `TargetData`
取得 primitive/pointer/size_t 事实，以受检算术验证 aggregate、closure 与 container header 的
size/alignment 和 element stride；超限或算术溢出稳定返回 IR-local `InvalidLayout`，不进入
opaque struct body、GEP 或 allocator lowering；SPEC-0184 已把来源类型映射到 L0145；
SPEC-0038 已完成具体闭包后端：function-pointer/concrete-closure/shared-reference IR-local type、
owned/shared capture layout identity、function-address、formation、非消费式 invoke 与 drop；
shared loan 依赖随 closure owner 和 CFG transfer 存续，drop 后释放，LLVM 使用裸 function
pointer 或 `{ptr, inline environment}`，不引入隐式 heap allocation 或类型擦除；
SPEC-0039 已建立显式 native entry 边界：只接受同 module 的 `() -> Unit` FunctionId，
普通 Koven function 使用 internal linkage，唯一 C ABI `i32 main()` wrapper 调用指定 entry 后
返回 0；LLVM 文本和 object emission 复用同一个 verified module lowering，并由同一
TargetMachine 直接生成 arm64 Mach-O object，锁定唯一 external `_main` 且失败不落盘；CLI
链接边界以 `Command` 直接执行 `/usr/bin/clang`，区分启动失败与链接失败，真实 object 的正常
entry 返回 0，SSA Abort 通过 C `abort` 非零终止且不生成 unwind；
SPEC-0040 已让 debug-enabled lowering 显式接收原 `SourceMap`，在创建 LLVM module 前
预检全部 SSA origin，并生成 DW_LANG_C fallback 的 line-tables-only compile unit、按名称确定的
`DIFile`、Koven function `DISubprogram` 与 instruction/terminator location；foreign map fail-loud，
无 debug LLVM 文本入口保持原产物；Mach-O 行表、LLDB 静态 source breakpoint 解析及真实
process launch/breakpoint hit 均已通过，frame 精确报告 Koven `app` 与 `debug.ko:4:5`；
SPEC-0042 已把 16 个 `BuiltinType` 的规范顺序收敛为 frontend 单一 production 环境
构造入口，并建立 `lang-codegen::emit_native_object` workspace API：调用方提交同一条 frontend
analysis chain、resolved 顶层 `SymbolId` entry 和输出路径，codegen 复用现有 scalar SSA、
verifier、DWARF object emitter 与显式 `FunctionId` wrapper；参数化、泛型、非 Unit、非函数、
unknown entry、混用 analysis、frontend diagnostics 与 unsupported source 均在 object 写盘前
结构化失败。`lang-cli::bootstrap` 从显式磁盘 source path 运行全部 frontend pass，唯一解析配置的
顶层 function identity，生成 object、复用既有 Clang linker 并运行；测试唯一枚举真实
`lang-std/koven/prelude.ko`，其 Koven `bootstrapSmoke(): Unit` 已经完整链路退出 0。该仓库内部
路径不发布通用 `main`、用户 CLI 参数或多文件构建语义；
SPEC-0043 已在该 production 环境的 16 个 builtin type 后发布唯一外部
`error(message: String): Nothing`，签名使用 Borrow 参数并携带 compiler-bound Abort effect；
typed call descriptor 保留该 effect，frontend→SSA 只据此把非插值 String literal 消息的调用
终结为 source-anchored Abort，LLVM 继续使用既有 C `abort` + `unreachable`，不生成同名 direct
call、unwind 或 String ABI。同名源码函数不获得该 effect，插值及其他 String expression 在
object 写盘前保持 unsupported。真实 `prelude.ko` 同时保留正常 smoke，并以独立 abort entry
验证 object/link 后由进程失败边界观察到非零或 signal；
SPEC-0189 已在同一 production 环境中继续发布唯一外部
`println(value: String): Unit`，无 marker 参数规范化为 Borrow，typed call 仅通过
compiler-bound `PrintLine` effect 进入后端。同名源码函数不获得 effect；当前只解码非插值
String literal 的 UTF-8 与合法 escape，并生成无 operand/结果、末尾固定 LF 的 verified SSA
`PrintLiteral`。LLVM 按需声明系统 `write`，以 private constant 写 fd 1，short/error write
进入既有 Abort；不引入 String heap、malloc/retain/clone、stdio buffering 或 unwind。真实
`prelude.ko` 的 `bootstrapHello` 已经 object/Clang link/run，stdout 精确为
`Hello, World!\n`；插值与一般 String expression 在 object 写盘前保持 unsupported；
SPEC-0044 已在唯一 `prelude.ko` 源码真源声明
`value class Pair<A, B>(val first: A, val second: B)` 与
`enum class Result<T, E> { Ok(success: T), Err(error: E) }`，不注册名称特例或新 runtime 表示。
`bootstrapPairResult` 已真实验证条件 `Copyable` 的 Pair 构造/复制/字段投影/完整解构，以及
Result 两个 case 的构造、smart cast 与 payload projection；从同一 prelude 派生的 MoveOnly
Pair/Result 负例各自产生唯一 L0131 并在 object 写盘前失败。当前公开单文件命令仍不隐式拼接
prelude，跨文件标准库可见性等待 package/import 主线；
SPEC-0185 已让 frontend-clean 的普通 class/value class/interface/enum class 顶层声明与既有
标量 entry 共存：声明 root 不进入函数模板或可达实例图，不产生伪 SSA/LLVM 实体；具名 object、
顶层 variable/constant 仍结构化拒绝，native 失败不落盘；
SPEC-0184 已把 v0.29 construction/ownership facts 接到 named aggregate、heap owner 与新增
tagged-union SSA：value class、普通 class、enum case 与 intrinsic Box 按声明顺序构造，Copy/
Consume 解构保持不同线性契约，字段 place、enum discriminant/静态 payload place 与 root drop
均由 verifier 检查。LLVM 使用 `i32` 源序 tag 加最大 payload storage，递归 drop glue 按 tag
选择 payload；native bridge 用 SSA 类型来源生成 L0145 使用主范围与声明标签。真实 `.ko` 已覆盖
value/class/enum/Box、投影/解构、MoveOnly nested payload，并完成 object/Clang link/run；
SPEC-0045 已完成 frontend 与 SSA 两个阶段切片：compiler-bound `Rc(value)`、显式 `.share()`
和只读 `.value` 发布独立 typed/ownership facts，construction 建立 `SharedOwner` root；typed SSA
新增先声明后定义且恒 MoveOnly 的 `SharedOwner<payload>`，以及消费 payload 的
`SharedAllocate`、非消费 receiver 并产生新 owner 的 `SharedRetain`、只产生 payload place 的
`SharedPayloadPlace`。三者已进入确定性 render、局部类型契约和线性 ownership verifier；LLVM
已把 handle 映射为 pointer、control block 映射为 target `{usize strong, payload}`，allocation
初始化 strong=1，retain 用非原子 max-check/add 并在溢出时 abort，drop 用非原子 decrement，
仅归零分支递归 drop payload 并 free。direct-SSA 已覆盖普通、nested 与 ZST payload；frontend
已把 intrinsic construction/share 与 Copyable payload read lower 到上述 operation，Rc receiver
liveness 以完整 intrinsic operation 为 drop 边界；shared-control `{usize,payload}` 已进入 LLVM
复合类型创建前的 target preflight。真实 `kovenc build/run` 已覆盖 Point payload、多次 share、
Copyable payload read 与 conventional `main`，SPEC-0045 已按该非 nullable core 完成；
SPEC-0195 已让 `CallableSignature`、`DirectCall` 与 `CallableInvoke` 参数保存 Value/Loan entity
identity。operation verifier 精确匹配 mode、target 与 loan kind，ownership verifier 让 Value
operand 保持消费语义、要求 Loan operand active 且不消费 owner；callee entry loan 是支配整个
callable CFG 的函数参数，正常退出时隐式结束，内部 `BorrowBegin` loan 仍按 block 参数显式跨
edge 转移。frontend 按 parameter mode、argument mapping 与 `LoanFact` 生成 root/Rc payload
place、shared loan、call、`BorrowEnd`，Borrow 参数的 Copyable read 使用 `PlaceAccess::Loan`；LLVM
统一以 target-storage pointer 传递。Rc MoveOnly payload call 后继续 share/drop、普通 class/Box
重复 Borrow、function-pointer Borrow invoke 与真实 object/Clang link/run 均已验收。SPEC-0196
进一步让 frontend 以 `NonNullUseDescriptor` 与 `NullComparisonDescriptor` 发布稳定 symbol 的
nullable 声明类型、inner 窄化类型及真假 edge proof；任意非空 `T` 适配 `T?` 的既有 guide
规则也已对 nominal/Box/Rc 恢复。typed SSA 建立独立 `NullableHandle<inner>` identity、
wrap/null/is-null/take operation、专用 `NullableBranch` non-null edge/shared-loan view 及
path/owner-sensitive verifier。普通 Borrow 参数不能伪造 proof，另一 owner 的 view 不能用于
take，active view 阻止 owner move/drop；Rc retain/payload projection 可读取 active non-null
view，但不把它转换为第二个 owner。LLVM 以同宽 pointer PHI/null compare 实现 null niche，
nullable drop 仅在非空路径调用 inner drop glue，不增加 tag、allocation 或 retain；class/Box/Rc
的 null/non-null frontend→object→Clang link/run 已通过。inline nullable、nullable `when` 与
`!!` 消费 lowering 当前确定性返回 unsupported；
SPEC-0057 已建立 `lang_frontend::formatting`：先用生产 Lexer / 完整文件 Parser 拒绝有诊断输入，
再按原 lexeme `Span` 保留全部 token、comment 与 LF/CRLF 字节，只规范水平空白及 delimiter 驱动
的四空格缩进；`kovenc format <path>` 向 stdout 输出，`--check` 使用 0/1，参数、IO、UTF-8 与
frontend 失败使用 2，均不修改输入文件；
SPEC-0060 已在 `lang-cli` 增加 ADR-0014 schema v1 adapter：显式
`kovenc --message-format=json <format|build|run> ...` 把完整 frontend 诊断按确定顺序逐条写为 stderr JSON
Lines，保留 `Ldddd`、原始消息、UTF-8 半开 byte range、1-based Unicode-scalar 行列与有序
label/note/help；默认 human renderer、formatter stdout/0/1 和 operational error 保持不变；
SPEC-0190 已把 repository bootstrap 的 build-only 边界复用于公开固定参数的单文件
`kovenc build` / `run`：调用方必须显式 source 和 build output，CLI 使用进程拥有的
唯一临时 object/directory 并清理，复用全部 frontend、resolved-entry object emitter 与 Clang
linker。build 成功静默退出 0，run 精确返回程序 stdout/stderr/status；真实仓库外 `hello.ko`
已分别通过 build 后启动与直接 run 输出 `Hello, World!\n`。SPEC-0193 允许省略 `--entry` 时在
完整 frontend 成功后选择唯一顶层、非泛型 `fun main(): Unit`；missing、非法形状和两个
conventional 形状并存分别形成 operational failure，显式 `--entry` 继续覆盖默认选择。
SPEC-0194 在此基础上增加 verified `NativeEntry` shape 与内部 `NativeEntryPlan`：参数化 target
必须精确为单一 shared Borrow `Array<String> -> Unit`，并在 LLVM module 构造前验证 function、
Array 和 String SSA identity。参数化 wrapper 使用 `i32 main(i32 argc, ptr argv)`，排除
`argv[0]`，先无分配检查 argv 指针、长度和 locale-independent UTF-8，再正序复制非空参数到
独立 String buffer；空串使用 ADR-0018 的 canonical `null/0/0`。完整 Array 以 shared loan
pointer ABI 调用 Koven main，返回后复用既有 drop glue 逆序析构 String 并唯一释放 buffer。
frontend→SSA 同时只为 Borrow 调用实参接通 active shared `Array<T>` loan 的 checked element
place，不开放 owned element extraction、replace 或 relocation。`kovenc run` 通过 `--` 分隔
compiler/program 参数，并以 `OsString` 原样转交；默认 output 仍未实现；
SPEC-0052 已在 `lang-cli::project` 建立内部 version 1 `project.toml` provider：调用方显式提供
manifest，strict schema 拒绝未知 dependency/target/entry 字段，manifest-relative roots 经路径、
symlink、重叠和物理 identity 检查后递归读取普通 `.ko`，最终发布按 `(root identity, logical path)`
排序的不可变 root/logical/text/presentation snapshot。该模块不调用 frontend、不查询 cwd、不选择
entry；若宿主不能提供可靠的普通文件物理 identity 则 fail loud。加载期
项目树并发替换不在首版原子保证内；
SPEC-0054 在 provider 之上新增固定顺序的 `kovenc build/run --project ... --entry ...`：同一
`SourceMap` 依次完成 unit name/type/ownership validated gate，诊断按 source key 进入 human/JSON
renderer；entry resolver 只在 typed package index 中选择有 body、非泛型、非 private 的顶层
`() -> Unit` 或 Borrow `(Array<String>) -> Unit`，并以 `NativeUnitEntry` identity 交给 codegen。
build 的 object/linker output 是 final 同目录的 create-new sibling temporary，object 清理后以
`hard_link` 原子 no-replace 发布；run 使用唯一临时目录并原样传递 `OsString` argv、stdout、stderr
与可表示状态。manifest/source 重合、existing/racing output、codegen/link/commit/cleanup failure
均 fail loud，dependency、manifest target/default 与隐式跨 package `main` 仍未实现；
SPEC-0058 已提供独立 TextMate grammar 与由生产
Lexer 校验的高亮回归 corpus；SPEC-0059 已提供 Tree-sitter grammar、生成 parser、外部
identifier scanner、原生 corpus 与生产前端交叉验收。

## 当前状态

仓库已完成 Phase 0、截至 v0.33 的 Phase 1 与当前已实施的 Phase 2/Phase 3 主线；现行 v0.34
的显式 instance receiver Parser/AST、typed facts 与 ownership facts 已由
SPEC-0201/0180/0181 完成；native lowering 继续等待 SPEC-0191。v0.33
的尾随 lambda 与隐式 `it` 已由 SPEC-0213/0214 完成。仓库并已完成 Phase 4 的
SPEC-0033/0034 标量主线、SPEC-0035 聚合/heap-owner、SPEC-0036 顺序容器后端基元、SPEC-0038
闭包环境后端与 SPEC-0039 显式 entry/object/link/run 边界。截至 v0.32
已实施的参数契约、显式实参调用期 loan、owned-value ASAP drop facts 与顺序容器核心 element place
所有权，以及简化 closure capture 与跨线程 `Transferable` 已经实现。工程骨架按
[ADR-0002](../adr/0002-bootstrap-workspace-layout.md) 建立，当前已实现：

- 根目录是 resolver 3 的 virtual Cargo workspace；所有 package 使用 Rust edition 2024，
  toolchain pin 和初始 MSRV 均为 `1.96.0`，并在许可与发布策略确定前保持不可发布；
- 五个 workspace member 均有 Cargo 可识别的 target，依赖方向单向且无环；
- `lang_frontend::source` 已提供统一 source / `Span` 基础设施；
- `lang_frontend::diagnostic` 已提供结构化诊断模型、`L0001`–`L0151` 正式错误码与
  确定性聚合顺序。`kovenc` binary 的默认纯文本 renderer 与显式 schema v1 JSON Lines
  renderer 均由 formatter 用户诊断复用；machine location 同时携带半开 UTF-8 byte range 与
  1-based scalar 行列，不复用 LSP 的 URI/UTF-16 range；formatter 与公开单文件 build/run 均复用；
- `lang_frontend::ast` 已提供四类 typed ID 与带 `Span` 的通用索引存储骨架；
- `lang_frontend::lexer` 已提供覆盖 v0.22 已实施词法契约的确定性扫描、完整 lexeme 流与
  结构化恢复诊断，包括保持 `&&` 最长匹配的单字符 `&`、顶层分隔用 `;`，以及以
  `package` 取代 `module` 的 42 个硬关键字和最小数值后缀集合；
- `lang_frontend::parser` 已提供独立表达式、声明与 block 入口、具体 Item / Statement /
  Expression / TypeRef 索引式 AST、Pratt 优先级、typed call、callable 参数 marker、结构化
  `CallArgument`、函数 block body、lambda、具名函数隐式 `Unit` 返回标注、`package` /
  Kotlin 风格 `import` 文件头、`if` / `when`、loop-family、jump、`super`、class-family
  及局部恢复，并
  确定性合并 Lexer / Parser 诊断；
- `lang_frontend::formatting` 只接受 Lexer/完整文件 Parser 零诊断输入，直接消费完整 lexeme
  流和原 `Span` slice；非 trivia token、line/block comment 及每个 LF/CRLF newline 字节保持
  不变，单趟 delimiter state 只规范同行 gap 与四空格 block/continuation indentation。公开
  `FormattingError` 区分 source、Lexer、Parser、用户 diagnostics 与内部 delimiter 不变量；
  formatter corpus 枚举全部完整文件 pass fixture，验证 lexical snapshot、重解析和幂等；
- `lang_frontend::name_resolution` 已提供显式 `NameEnvironment`、单文件类型 / 值双命名
  空间、稳定 `ScopeId` / `SymbolId`、有序 overload set、顺序 local 可见性、名称引用产物与
  L0079–L0081；enum case 的值构造器与 type-test 身份共享稳定 `EnumCaseId`，限定 case 尾段
  与 payload 候选也保留在解析产物中。SPEC-0025 另提供纯内存 compilation-unit 两阶段 API：
  `index_compilation_unit` 校验稳定 root/logical-path 输入并建立规范排序的 package/source/
  declaration identity；`resolve_compilation_unit_names` 在核对 inputs/index 后解析 same-package、
  exact/alias/wildcard import、可见性、限定路径与静态 member，并发布 recovery/validated 名称
  产物及 L0146–L0151。旧单文件 resolver 与 `ReferenceTarget` 保持兼容，`_` discard 已收窄到
  `for` binding；SPEC-0197 已建立 compilation-unit 类型身份、完整 signature graph，并接通
  callable/local/operator/if/type-test/assignment/destructuring/when/loop/lambda/overload-lambda trial、
  body-local 泛型 TypeRef、source 泛型调用、external/function-value call 与 source/intrinsic
  construction facts、source member body/call/field/structural-component projection、non-nullable
  intrinsic Rc `.value`/`.share()` operation、core container construction、container
  index/place/member/assignment、contextual null literal 与 null-comparison/non-null-use flow facts；
  其余 nullable body 语义仍在实施；
- `lang_frontend::type_checking` 已提供与名称环境身份绑定的显式 `TypeEnvironment`、确定性
  `TypeId` / `NominalId` / typed 产物、builtin / nullable / function / nominal / type-parameter
  类型、泛型替换、interface closure、member contract、override/default 冲突与窄化委托计划，
  并实现数值定型、局部单向 expected type、lambda / 基础运算符 / 返回流检查、enum case
  type、稳定 place flow facts、赋值/capture kill、短路条件传播，以及 Boolean/enum/nullable
  `when` 穷尽性、条件 `Copyable` 四态查询、名义内联递归检查、环境绑定的 intrinsic
  `Box`，局部 value-class 解构的 Copy/Consume descriptor，以及单态 source/external/
  function-value/member callable 选择、源码有序实参映射、`CallDescriptor` 和
  `ExpressionCategory` place/temporary 事实；nullable smart-cast 另以按 expression identity
  排序的 `NonNullUseDescriptor` 发布稳定 symbol、nullable 声明类型与 inner 窄化类型，并由
  `NullComparisonDescriptor` 发布 `== null` / `!= null` 的证明 edge，后端无需重解条件 AST；
  expected-type 适配遵循任意非空 `T` 可进入 `T?` 的 guide 规则。具名函数与成功采用唯一期望
  函数类型的 lambda
  还按参数 `SymbolId` 发布 `ParameterBindingDescriptor`，Borrow/Inout lambda 不再被误判为
  全 Value 结构不匹配。环境绑定的 `Array` / `List` / `MutableList`
  identity、storable 元素检查、`ContainerConstructionDescriptor`、带可变性的
  `ElementPlaceDescriptor`、只读 `size` 与封闭 `[]` 规则，覆盖 L0082–L0130；源码顶层与实例
  member 泛型 callable 已支持完整显式类型实参或由已定型非 lambda 实参执行 invariant 结构
  推导，并验证 interface / `Copyable` / `Transferable` bound，发布 owner 实参在前的稳定
  `CallableInstanceKey`。多 overload 候选会先过滤非 lambda 实参，再在完整 typed transaction
  中按候选 expected function type 隔离检查 lambda；唯一成功 trial 原子提交，零个/多个只产生
  L0123/L0124。callable reference、safe-call lifting 与所有权可用性仍使用逐类
  `DeferredReason` 保留。普通名义主构造器字段已建立带实际
  泛型替换的
  `AggregateProjectionDescriptor`，
  nominal/value-class、enum case 与 intrinsic `Box` 构造已发布稳定 target/instance、完整
  result type 及声明顺序参数到源码求值顺序 operand 的 `ConstructionDescriptor`；
  `value class` 在无显式同名 callable 时提供零参数自动 `componentN()` typed target；
  callable 参数只保留 `Value` / `Borrow` / `Inout` 三态 typed identity；无 marker 与显式
  `borrow` 共享 `Borrow` identity，声明侧 `own` 形成 `Value`。预声明只读 API 使用 `Borrow`，
  `Array` / `List` 的 runtime-length 构造器两个参数均为 `Borrow`；预声明 callable 可由
  `EnvironmentFunctionEffect` 为精确参数绑定跨线程交付 effect，成功 call 通过 argument
  descriptor 公开该 identity，源码同名函数不会获得 effect；环境绑定的 intrinsic `Rc<T>`
  已实现 construction、share、payload read 的 typed/ownership identity，源码同名 class/member
  不获得特权；SharedOwner SSA、核心 LLVM control block/retain/release、intrinsic construction/
  share 与 Copyable payload read lowering 及真实 native 验收已实现；MoveOnly payload、普通
  class/Box 与 callable value 的 Borrow-call SSA/LLVM 交接已由 SPEC-0195 完成；SPEC-0196 已完成
  pointer-like nullable 的 frontend `if` proof、独立 SSA/verifier、LLVM null niche/conditional
  drop 与 class/Box/Rc native 接线；
- `NameResolution`、`TypedFile` 与 `OwnershipCheckedFile` 贯穿不可伪造的逐阶段 analysis
  identity；`TypedFile` 另保留显式 Name/Type environment owner。只读兼容性查询同时验证
  source、environment、name-analysis 与 typed-analysis identity；所有权阶段拒绝同源但来自
  另一环境或另一分析链的 typed 产物，供 SPEC-0034 lowering 在构造任何 SSA 前执行完整门禁；
- `lang_frontend::ownership_checking` 已提供消费 ParsedFile、名称解析与类型事实的独立检查
  入口，以稳定 `SymbolId` 跟踪局部整变量和规范化为 `Value` 的 owned 参数的可用 / 已移动
  状态；Borrow/Inout 参数不进入 owner 状态。MoveOnly 值在
  initializer、当前 typed facts 标记的 Value 实参和显式 return 的按值交付点移动，Copyable 值
  保持可用，普通重新赋值恢复变量状态，分支与循环按可继续路径保守合流；L0131 同时定位
  非法使用与首次移动，
  Copy/Consume 解构按单个原子动作复制或移动整个源值，L0132 拒绝从字段或自动结构分量移出
  MoveOnly 值且不建立部分状态。该阶段还发布 `OwnershipBindingDescriptor`、稳定 root + field
  path、同步 `LoanFact`、路径敏感 `DropFact` 与明确 deferred facts；按源码实参顺序检查
  shared/exclusive overlap、Borrow/Inout 移出、Inout 可变性和 nested call，L0133–L0135 分别
  锁定 non-owning move、非法可变 place 与有效 loan 冲突。named owner、temporary、replacement、
  return/`?`、branch 与 loop 的 ASAP drop facts 可供 Phase 4 查询。lambda capture 集按解析后
  scope/reference/SymbolId 计算，字段归一为 `this`；默认 lambda 建立 shared capture loan，
  `move` lambda 对 Copyable/MoveOnly capture 分别 copy/move，并检查 body 内非 owning move 与
  capture immutability。borrowed closure 只在 defining callable 内使用，L0137 拒绝 return、
  Value 交付和字段存储逃逸；L0138 拒绝从 borrowed/Inout 或 `this` 建立 owned capture。
  `Transferability` 与 `Copyability` 独立结构化求值，编译器绑定跨线程 effect 以 L0139 拒绝
  non-Transferable value/environment；drop planner 在 closure 最后使用后结束 loan、析构 owner，
  并逆序发布 owned capture drop。存在所有权诊断时不发布 capture/drop plan。construction
  专项分派在 ordinary call/member/name 前消费 Phase 2 descriptor，只求值源码
  operand，不求值 type/case callee；`ConstructionOwnershipPlan` 按源码求值顺序发布
  copy/move/temporary delivery，并为 MoveOnly value/enum inline root 或 class/Box heap-owner
  handle 建立唯一 `ConstructionRootDropObligation`。全 Copyable/no-payload construction 不制造
  root obligation，`Nothing` operand 截断后续 delivery 和 root 建立；任一所有权诊断会原子
  清空 construction plan 与 drop facts。顺序容器构造复用 typed 参数模式；intrinsic index 形成
  root + field path + 逻辑索引 identity，支持 Copyable owned read、MoveOnly L0136、element
  shared/exclusive loan、temporary owner 延寿、固定顺序 replacement 与旧元素 drop fact。
  非 intrinsic index、post-index field projection 与 Phase 5 relocation effect 仍明确 deferred；
  SPEC-0181 已让 member receiver 作为第零操作数先于显式实参建立 shared/exclusive loan 或
  Value copy/move，并发布 stable place、temporary、implicit `this`、call-return 与 drop facts；
  普通字段 `=` 在 RHS 正常完成且字段为 MoveOnly 时发布唯一
  `BeforeReplacement/ReplacedField` fact，Copyable 与完全发散 RHS 不发布；
  Borrow/Inout/Value `this` 的字段、reborrow、整体移动、capture 冲突和 Borrow-only delegate
  outer/field shared-loan plan 已进入 compilation-unit ownership 产物。无状态 object 保留 MoveOnly
  源码能力与 call-scoped shared receiver fact，但没有 runtime owner，因此不发布 temporary drop；
- `lang-frontend` 已有 Cargo 实际执行的 Phase 0 source-loading，以及 Phase 1 Lexer 与
  parser-expression、parser-declaration、parser-block、parser-lambda、parser-implicit-unit、
  parser-file pass / fail fixture harness，以及 Phase 2 名称解析和基础/名义类型检查 pass / fail fixture；
  当前 type checker 已按当前 callable 的 loop base 检查 `break`/`continue`，合法 jump 定型为
  `Nothing`，L0142 精确拒绝 loop 外或跨 lambda/function boundary 的 jump；`for` 仍只检查
  source 表达式，尚未发布 iterator 选择、元素类型与 binding typed fact，这是进入完整 `for`
  lowering 前剩余的 Phase 2 漂移；
- `editors/textmate` 已提供 `source.koven` / `.ko` grammar、正常与 reserved corpus、scope
  expectation，并由 `lang-frontend` integration test 复用生产 Lexer 做漂移回归；
- 尚无普通字段部分移动、顺序容器 Phase 5 relocation effect 或完整源码 codegen；SPEC-0034 已完成
  frontend→SSA 的标量 expression、block、branch、loop 与具体泛型实例封闭切片，并把该封闭
  子集的 verified SSA 映射为 verified LLVM IR；完整 `for` 因依赖 typed iteration plan 与
  provider runtime 已迁移到候选 0182。SPEC-0035 已完成不依赖源码 constructor 选择的 named
  aggregate/heap-owner SSA、整体 construct/project/explode、heap allocate、payload/field place、
  线性 ownership/loan verifier、LLVM first-class aggregate/DataLayout、系统 allocation 与递归
  drop/free；SPEC-0184 已把源码 constructor、ordered delivery/root drop、enum tagged payload
  与 projection/destructuring 接入 verified SSA/LLVM。显式 verified
  SSA entry 已能生成 Mach-O object、经 clang 链接并运行；SPEC-0042 已提供仅接收 resolved
  `SymbolId` 的单文件 source-analysis→object workspace API，并由仓库内部 bootstrap driver
  完成真实标准库 Koven source 的 object/link/run；通用源码入口选择和公开 CLI 流水线仍未实现；
- [ADR-0006](../adr/0006-typed-ssa-block-parameters.md) 已接受 IR-local type、block parameters、
  显式 ownership effect 与独立 verifier 的 typed SSA 架构；对应
  [SPEC-0033](../specs/0033-typed-ssa-ir-verifier.md) 已完成：`lang-codegen` 已建立
  crate-private、owner-aware 的索引式 SSA model、IR-local type/origin 与确定性 debug rendering，
  并完成 ID/归属、CFG/edge、operation/return type、dominance、MoveOnly 唯一消费与
  shared/exclusive loan 数据流 verifier；封闭标量 AST/frontend lowering 已由 SPEC-0034 完成；
- [ADR-0007](../adr/0007-llvm-toolchain-and-first-target.md) 已接受 LLVM 21.1.x、Inkwell 0.10.0、
  `LLVM_SYS_211_PREFIX` 显式发现、动态链接和首个 `aarch64-apple-darwin` target。SPEC-0034 已
  以 crate-private LLVM adapter smoke 证明合法标量模块确定生成、LLVM verifier 拒绝无
  terminator 模块，并由 `otool` 确认链接 Homebrew `libLLVM.dylib`；typed SSA 已新增返回
  “结果 + 失败标志”的 checked add/sub/mul/div/rem、完整整数比较、Boolean not 与标量
  direct-call contract，失败标志通过显式 CFG 导向 `Abort`。frontend→SSA lowering 已显式验证
  source 与逐阶段 analysis identity，拒绝任一 frontend diagnostic、ownership deferred 和非封闭
  节点；`lower_frontend::orchestrate` 负责文件门禁、标量类型预置、函数预声明与最终 verifier
  编排，父模块负责 expression/body lowering，避免 CFG 扩展继续堆入单一超限文件。函数收集器
  按 parser `ClassifierKind` 跳过无模块初始化动作的 class/value class/interface/enum class root，
  但继续拒绝 object 与顶层存储；Error/Deferred call 没有 typed descriptor 时按 unsupported source
  处理，真实缺失事实仍是内部 `MissingFact`。当前支持
  无泛型顶层 expression-body 与直线 block-body 函数的 Unit/Boolean 与全部
  8/16/32/64-bit 有符号/无符号整数、literal/name/group、前缀正负/Boolean not、checked
  arithmetic、六类比较、源码 direct call、嵌套 block、局部 `val`/`var`、普通/复合赋值与
  显式 return；直接负整数字面量以单个负常量表示，保证有符号最小值不被错误 lower 为
  溢出的运行时 subtraction，一般前缀负号仍保留 checked operation；`control` 子模块把
  `if`、subjectful/subjectless Boolean `when`、enum case type-test `when` 和 `&&`/`||`
  lower 为真实 CFG，以 block parameter 合流分支结果及分支内 local 更新，statement context
  不为丢弃值伪造 payload；`loop_control` 为 `while`/`loop` 建立显式 header 参数，preheader、
  自然 fallthrough 与每条 `continue` backedge 都传递当前 local，`break` 只进入最近 loop exit。
  `instances` 从非泛型顶层入口构造确定的可达实例图，按 SPEC-0177 key 替换泛型体内直接类型
  参数；同 key 递归去重、不可达泛型不生成、同名 overload 保持不同 `FunctionId`，并以 1024
  个具体泛型实例作为显式增长门禁。`llvm::adapter` 在构造 LLVM 前再次运行 SPEC-0033
  verifier，只接受单一 module 及当前封闭的标量/聚合/heap-owner value，按 `FunctionId` 生成不
  混淆 overload/实例的稳定 symbol；entry 参数映射为 LLVM 参数，非 entry value block 参数映射
  为有序 PHI。Boolean 使用 `i1`，
  整数保留 8/16/32/64-bit 宽度和操作 signedness；checked add/sub/mul 使用 LLVM overflow
  intrinsic，div/rem 在执行 LLVM 指令前以安全 divisor 避免失败路径触发 LLVM UB，失败 flag
  继续流向 C `abort` + `unreachable`。branch/conditional/return、六类比较、Boolean not、
  direct call、scalar copy、first-class aggregate、heap owner value 和当前局部 place/loan 操作
  已映射，最终 module 必须通过 LLVM verifier；重复 lowering 文本
  相同。完整 `for` 仍未实现，由候选 0182 在 typed iteration plan 与 provider runtime 就绪后承接；
- [ADR-0008](../adr/0008-internal-value-and-allocation-abi.md) 已接受 target `DataLayout`、
  first-class aggregate、无对象 header 的 class/Box heap owner、集中系统 `malloc/free/abort`、
  固定顺序容器 header 与 ZST sentinel 边界。SPEC-0035 已为 named aggregate 自动推导
  Copyable/MoveOnly，并让 heap owner 的先声明/后定义指向显式 aggregate payload，从而以固定
  pointer handle 打断递归；construct/project/explode、heap allocation effect、payload/field place、
  确定 debug rendering、跨 module/重复/未定义/非法 inline cycle 以及 nested loan/CFG 唯一消费
  verifier 已实现。LLVM adapter 已用 target `DataLayout` 映射声明顺序 identified struct，支持
  aggregate 参数、返回、PHI、direct call、insert/extract 与大型 first-class value，并让
  MoveOnly direct-call 实参发生唯一消费。集中 `llvm::runtime` 只在实际需要时声明目标 C runtime
  的 `malloc`/`abort`/`free`：payload 大小来自同一 target `DataLayout` 并以 `max(size, 1)` 支持
  ZST owner，null 分支调用带 `noreturn` 属性的 `abort` 后 `unreachable`，成功分支完整 store
  payload。MoveOnly aggregate/heap owner 的内部 drop helper 先按 type ID 预声明，再逆字段递归
  调用；heap payload 完成后每个 owner helper 恰好调用一次 `free`，自引用 heap type 不导致
  codegen 递归，abort 路径没有 unwind cleanup。payload/field/root place 与同步 loan 映射为现有
  storage pointer，allocation 引入的真实成功 block 会作为后续 PHI predecessor。源码
  多目标平台与 public FFI ABI 仍未确定；单文件标准库
  bootstrap 已由 ADR-0012 / SPEC-0042 封闭并实现。
  SPEC-0186 在 `TypeMap` 创建任何 opaque/composite LLVM type 前构建有序
  `TargetLayoutPlan`：基础 scalar/pointer/size_t 布局来自最终 module 使用的同一 `TargetData`，
  封闭 record universe 按 `SsaTypeId` 源序线性地以 `u128` checked align-up/add 计算 nested
  aggregate、inline closure environment 和固定 container header，并以 default address-space
  pointer width 的 unsigned 上限拒绝不可表示对象。计算不递归消耗 Rust 调用栈；
  heap/shared/function handle 只计算 pointer，不展开递归 payload；
  container stride 直接消费 element 的同一受检 alloc-size，ZST 保持 0。合法 padded/nested、
  heap-recursive、closure 与二/三字段 header 均和 LLVM 实际 layout 对照；紧凑 61 层倍增聚合
  在 `2^64` bytes 处确定失败。该事实只属于后端 preflight，不进入 frontend/SSA 类型身份。
  SPEC-0036 已新增三个 IR-local 顺序容器 kind，identity 保留具体元素类型且始终
  MoveOnly；列表式完整构造、直接 initializer 运行时长度构造、length、element place 和原子
  replace operation 已进入确定性 render、局部类型契约与线性 ownership/loan verifier。
  element place alias root 追溯到 container owner，因而 move/drop 会使投影 place 失效，存续
  loan 会阻止 replace/drop。LLVM type map 已按 ADR-0008 把 `Array`/`List` 映射为 `{ptr, size_t}`、
  `MutableList` 映射为 `{ptr, size_t, size_t}`，元素 stride/alignment 来自同一 target DataLayout；
  列表式构造与 direct initializer 运行时长度构造使用单连续 buffer，先拒绝负长度并以 LLVM
  overflow intrinsic 受检计算 `length * stride`，零长度/ZST 选择 module-private 对齐 sentinel，
  其余路径只调用一次 `malloc`，null/overflow 进入 `abort` + `unreachable`。initializer 按升序
  显式循环且每个结果直接写入槽位，container header 作为 first-class value 参数/返回，不使用
  隐式 `Box` 或动态 `alloca`。checked-index 先完成负值与上界检查再形成 element address；
  replacement 先载入旧值、提交新值，再对 MoveOnly 旧元素调用 type-directed drop glue。
  container drop 对 MoveOnly 元素按 logical length 逆序调用 glue，Copyable 元素跳过该循环，
  最后只对真实非空 allocation 唯一 `free`。IR-local `ZeroSized` proof type 用于锁定 MoveOnly
  ZST 的逻辑析构次数：不形成零 stride GEP/load/store，也不调用 `malloc`/`free`；
- [ADR-0009](../adr/0009-concrete-closure-internal-abi.md) 已接受 function pointer 与 inline
  environment 的 concrete closure ABI：无捕获值使用裸 function pointer，capturing closure
  使用不同 concrete identity，禁止隐式 heap/type-erased fat pointer。SPEC-0038 已在
  typed SSA 中实现 signature、capture mode/type 与 concrete environment identity；function
  address 精确匹配普通 target，closure formation 精确匹配 environment-first thunk，invoke
  只读取 callable owner而不消费它。owned MoveOnly capture 在 formation 时唯一消费，Copyable
  capture 保持可用。LLVM 将 function pointer 映射为裸 `ptr`，concrete closure 映射为
  `{ptr, inline environment}`，支持 owned capture 构造、间接调用和逆序 drop glue，且 closure
  自身不声明 allocator 或 type tag；shared capture 保存已有 loan pointer，loan 依赖随
  closure owner/CFG transfer 重绑定，提前结束被拒绝，并在 closure drop 后精确释放；
- [ADR-0010](../adr/0010-first-native-object-and-linker-contract.md) 已接受首个 AArch64 macOS
  object/link 边界。SPEC-0039 复用 verified module lowering 与同一 TargetMachine 直接生成
  Mach-O object；显式 `() -> Unit` FunctionId 获得唯一 external C `i32 main()` wrapper，普通
  Koven function 保持 internal。`lang-cli` linker driver 不经 shell 调用 `/usr/bin/clang`，保留
  status/stderr 并区分 driver 启动失败；test-only orchestration 已真实运行 normal 与 SSA Abort
  object。源码 `main` 选择仍等待后续 frontend 接线；标准库 `error()` identity 已由 SPEC-0043
  通过 typed effect 接入，未按名称猜测；
- [ADR-0011](../adr/0011-first-dwarf-line-mapping.md) 已接受首个 line-tables-only DWARF 映射。
  SPEC-0040 保持 SSA `Program` 不持有源码 owner，由 debug-enabled object/text lowering
  显式接收同一 `SourceMap`；preflight 解析 function/block/entity/instruction/terminator origin，
  foreign map 在 LLVM metadata 前返回独立 codegen 错误。独立 `llvm::debug` 模块按 entry source
  建立 compile unit，以 source name 原文建立确定性 `DIFile`，把 Koven display/linkage name 与
  1-based Unicode/CRLF 行列映射到 subprogram/location；synthetic origin 使用 anchor，C `main`
  wrapper/runtime declaration 不获得伪造 Koven subprogram。Mach-O object 已由 `dwarfdump`
  验证真实 `.ko` 行列，LLDB 可把 `debug.ko:4:5` 静态解析为 `app` 的唯一 source breakpoint，
  链接后程序正常返回 0；显式 live 验收还实际启动该进程、命中 `breakpoint 1.1`，并由 frame #0
  报告 `app at debug.ko:4:5`；

现有 target 已证明上述封闭 SSA/LLVM/object/link 行为；resolved source entry→object 已形成
workspace API，仓库拥有的标准库单文件 bootstrap 已真实 link/run。通用 `.ko`→可执行文件 CLI、
标准库公共 API、跨文件 LSP 分析与跳转定义仍未实现；单文档 LSP 诊断与语义跳转定义已分别
由 SPEC-0055、SPEC-0056 接通。

## Workspace 与 target

workspace 采用 `crates/` 布局，五个 member 及 target 为：

- `crates/lang-frontend`：Rust library；
- `crates/lang-codegen`：Rust library；
- `crates/lang-cli`：名为 `kovenc` 的 Rust binary；
- `crates/lang-lsp`：Rust binary；
- `crates/lang-std`：最小 Rust library；`koven/prelude.ko` 是当前目标语言源码包。

项目内依赖方向为：

- `lang-codegen` → `lang-frontend`；
- `lang-cli` → `lang-frontend`、`lang-codegen`；
- `lang-cli` 在机器诊断展示边界直接使用 workspace 锁定的 `serde_json`；
- `lang-cli` 的内部 project provider 使用精确锁定的 `toml 1.1.4`，关闭默认 feature，只启用
  `std`、`parse`、`serde`；不使用 derive、display 或 preserve-order；
- `lang-lsp` → `lang-frontend`，并在外围 transport 边界使用 `lsp-server`、`lsp-types` 与
  `serde_json`；
- `lang-std` 无项目内依赖。

`lang-std` 的 Rust target 仅提供 Cargo 与测试边界，其单元测试验证 `.ko` 源码包存在；标准库
公共实现仍以 `koven/**/*.ko` 为唯一真源。SPEC-0042/0043 的 CLI 测试从磁盘唯一枚举
`koven/prelude.ko`，分别真实编译、链接和运行正常/Abort bootstrap entry，不使用 `build.rs`
或 Rust 行为镜像。Phase 0 不包含 runtime crate。

## Source 与 Span

`lang_frontend::source::SourceMap` 按
[ADR-0004](../adr/0004-source-span-position-model.md) 拥有已加载源码。每个内部 source entry
持有不可变的用户可见名称、UTF-8 `String` 和集中预计算的行起始字节索引：

- 同一 source map 内的用户可见名称必须唯一；重复注册返回
  `SourceError::DuplicateSourceName`，不会替换原有源码；
- `SourceId` 是所属 source map 分配的 map-local 身份；私有 owner identity 防止不同 map 的
  相同索引静默串源，并从稳定 debug 表示中隐藏。它不等同于文件系统路径；追加 source 不
  改变已有 ID，但稳定产物不得按 ID 或加载顺序排序；
- `Span` 内含 `SourceId` 和 `[start, end)` 半开字节范围，只能由 `SourceMap::span` 受检创建；
- source map 统一提供 span 切片和 byte offset 到 `SourcePosition` 的换算，后续 lexer、AST、
  诊断和 LSP 不得各自重复实现；
- 展示位置使用 1-based 行列，列按 Unicode scalar value 计数，tab 计一个 scalar；行索引在
  `\n` 后开始新行，因此同时保留并稳定处理 LF、CRLF、空文件和 EOF；
- 无效 `SourceId`、逆序、越界和非 UTF-8 字符边界通过具体 `SourceError` 返回，不以 panic
  处理用户输入。

行列不存入 `Span`，只在展示边界派生。source 模块不依赖 parser、类型系统、LLVM 或外围
crate；终端视觉宽度、文件发现、路径规范化和增量更新尚未实现。

## 确定性 Lexer

`lang_frontend::lexer::lex(&SourceMap, SourceId)` 读取已加载源码并返回 `LexedFile`；跨
`SourceMap` 的 ID 或诊断构造不变量失败进入具体 `LexerInternalError`，普通用户词法错误则
保留在产物的诊断序列中，不走内部错误路径。

- `LexedFile` 保存 source identity、有序 lexeme 与按既有全序排列的诊断，不复制源码文本；
  Parser 后续可继续共享 `SourceMap` 并按 `Span` 回查原文；
- `LexemeKind` 封闭区分普通 token、trivia、invalid 区域和唯一 EOF。除 EOF 的
  `[source.len(), source.len())` 外，每个 lexeme 都有非空 UTF-8 字节 `Span`，并按顺序无
  重叠地联合覆盖完整输入；
- scanner 实现 ASCII 标识符、42 个硬关键字、2 个历史基线中仍按 identifier 输出的软关键字、11 个
  reserved-word token、十进制数字、`Char`、单行 `String` / `${...}` 插值、trivia 与固定
  符号最长匹配；v0.20 的 `by` 同样自然产出 identifier，不需要或拥有独立 Lexer token；
  扫描只使用标准库，没有新增依赖；
- 数值 scanner 接受并规范化 `L`、`u` / `U`、`uL` / `UL` 与 `f` / `F`；token 通过
  `IntegerLiteralSuffix` / `FloatLiteralSuffix` 保留身份，未知或错序后缀仍形成单一 L0008
  区域。Lexer 不解析数值、不检查范围，也不决定 expected/default type；
- 字符串与插值使用显式模式栈，只有插值普通模式中的花括号改变嵌套深度。非法输入始终
  前进并形成规范规定的 token / invalid / segment 形态；未终止模式按最内层错误抑制规则
  恢复，不对正常用户输入 `panic!`；
- `TokenKind`、`Keyword`、`ReservedWord`、`Symbol`、`TriviaKind` 与 `InvalidKind` 是 Lexer
  面向后续 Parser 的最小分类 API；它们只表达词法拼写，不提前判断语法位置或运算符语义。

Lexer 已通过 Parser 接入 CLI bootstrap driver 与单文档 LSP；`LexedFile` 仍只是 Parser 的
唯一词法输入，不是独立完整编译产物或公共机器诊断协议。

## 表达式、声明、Block、Lambda 与隐式 Unit Parser

`lang_frontend::parser::parse_expression`、`parse_declaration`、`parse_block` 与 `parse_file` 都接收共享
`(&SourceMap, &LexedFile)`，校验 map-local source identity，并分别返回唯一
`ExpressionId` / `ItemId` / `StatementId` 根，或完整文件的可选 `PackageDirective`、有序
`ImportDirective` 与 `ItemId` roots，并返回两阶段诊断全序。四个入口共享 `SyntaxAst`、
Pratt、TypeRef、词法恢复索引、固定 worker 与递归预算；普通语法错误进入产物，内部不变量或
资源边界失败才返回具体错误。

Parser 的公开路径继续统一由 `parser/mod.rs` 门面提供：`syntax` 保存具体 AST payload，
`output` 保存四类解析产物与文件 directive，`error` 保存内部边界错误，均经门面 re-export
保持原有 API。内部 `engine.rs` 只编排入口、持有唯一 `Parser` 状态和跨领域不变量；
`engine/` 下按 `recovery`、`boundary`、`file`、`declaration`、`class`、`destructuring`、
`block`、`expression`、`postfix`、`operator`、`type_ref` 与 `core` 拆分同一状态上的领域
操作。`trial` 与 `lambda_trial` 继续负责无副作用预索引；所有子模块共享唯一 cursor、AST、
诊断序列和 binding-power 定义，不复制解析状态或恢复规则。

- Parser 跳过 trivia，消费 v0.6 的 primary、postfix、prefix、14 档中缀 / 赋值和递归
  `type_ref`；binding power 只在 `parser::engine` 中定义，`to` 由源码 `Span` 精确识别；
- 声明入口消费 v0.7 的 `val`、`var`、`const val` 与具名 `fun`，保存三态名称 marker、参数与
  泛型列表；调用点 `<type_ref, ...>(...)` / `<type_ref, ...> { ... }` 由单次 O(N) 反向预索引
  无副作用判定，查询 O(1)，成功后才由正式 TypeRef parser 提交 AST；
- 当前具名函数参数和函数类型参数共享封闭的
  `ParameterModeMarker::{Own, Borrow, Inout}`；无 marker 与显式 `borrow` 都规范化为 `Borrow`，
  `own` 规范化为既有 `Value`，`inout` 保持 `Inout`。函数类型使用内嵌
  `FunctionTypeParameter`，strict typed-call 预索引同步识别三种声明 marker，失败仍不分配
  AST、不发诊断或移动正式 cursor；
- basic、typed、member 与 chained call 统一保存源码有序的内嵌 `CallArgument`：可选命名
  前缀、调用点 `borrow` / `&` marker 和唯一 value 表达式。Parser 只保存 Phase 1 源码结构，
  不做名称映射、契约匹配、place、可变性或所有权检查；
- SPEC-0213 在同一 postfix parser 中把无换行 trivia gap 后的 `{ ... }` 解析为最后一个无
  name/mode 的普通 lambda `CallArgument`。无圆括号、已有 `(...)`、typed、member 与 chained
  callee 都产生单一 `Expression::Call`；第二个尾 lambda 不形成嵌套 call。LF、CRLF 或含换行
  comment 保留 expression statement 与 nested block 边界，失败的 typed-call 试探不提交 TypeRef；
- SPEC-0214 为每个 headerless lambda 保存真实 `{` anchor，并由名称解析建立 synthetic
  `LambdaParameter` `it`。单文件与 compilation-unit 类型检查只在 unary expected function
  contract 下发布其类型/mode；无 expected 使用 L0083，零/多参数结构冲突使用 L0084。
  overload trial 快照 typed facts；局部同名 binding 在 initializer 后替换 candidate，ownership
  复用普通参数的 move/loan/drop 规则并通过 lambda-local scope 排除 capture；unit closure
  lowering 在 callable arity 为一时以同一 `{` anchor 作为参数来源范围，不改变 callable ABI；
- 函数 Item 以 `FunctionForm` 同时封闭返回标注来源与 body：省略标注只产生
  `ImplicitUnitAbsent` 或引用真实 block statement 的 `ImplicitUnitBlock`，不合成 `Unit`
  TypeRef 或 colon；`Explicit` 保存真实 / 恢复插入的 colon `Span`、TypeRef ID，以及
  `Absent`、保留真实 `=` Span 的 Expression 或 Block 三态 `FunctionBody`。因此隐式 `Unit`
  与表达式体的非法组合在 AST 类型上不可表示；
- 参数列表后只做一次互斥 suffix dispatch：真实 `:` 提交显式分支，直接 `{` 提交隐式
  block，EOF / 调用方无体 stop 或其他普通边界提交隐式无体；直接 `=` 在其起点发唯一
  `L0021`，以同位置空 colon 与 Error TypeRef 恢复为显式表达式体；明显 TypeRef 起点缺
  colon 继续发 `L0021` 并保留真实 TypeRef。Lexer invalid / reserved 与 segmented string /
  interpolation poison 仍由 Lexer 拥有；词法恢复索引把 nested non-terminal string 根因传播
  给活跃的外层 string owner，使独立声明 trailing 恢复一次消费完整 poison 区域而不追加同
  根因 `L0021` 或 `L0013`；真实 colon 后缺 TypeRef 则继续只使用 `L0014`；
- block 入口消费 v0.8 的空 / 嵌套 block、局部 `val` / `var` 与 expression statement，按源码
  顺序保存 typed `StatementId`；显式和隐式 block body 都把真实 block 的完整范围纳入函数
  Item 范围，隐式无体 Item 精确止于参数列表最后实际消费位置；
- block 与 lambda body 在 `val (` 起点提交唯一 `Statement::LocalDestructuring`，
  内嵌保存有序 `NameMarker` bindings、真实可选 `)` / `=` 与唯一 initializer
  `ExpressionId`；`var (` / `const val (` 与独立声明上下文分别以错误 statement /
  item 恢复，不新增 pattern table或提前进行 Phase 2 / 3 检查；
- expression primary 消费 v0.9 的普通与 `move` lambda，以 `Expression::Lambda` 唯一引用
  独立 `Statement::LambdaBody`；block element 起点的 `{` 在没有同行可附着 callee 时仍是 Unit
  block，expression primary 或尾 lambda 位置的 `{` 才是 lambda，判定只读取 trivia 换行；
- scalar literal AST 以 `IntegerLiteralKind` / `FloatLiteralKind` 保存无后缀、`Long`、
  unsigned、`ULong`、`Double` 与 `Float` 规范化身份；Parser 只映射 token，不回读源码或
  提前做数值定型；
- `LambdaHeaderIndex` 在每个 parser 入口构造时单趟预索引完整 raw lexeme 与 terminal-owner
  event 流，并按 `{` 的 raw index 提供 `O(1)` 严格 header 查询；失败不分配 AST、不发诊断，
  正式解析只提交完整的零参数或普通 Identifier 参数前缀；
- expression primary 已增加 `If`、`When`、`Return`、`Break`、`Continue` 与
  `SuperMember`，statement 已增加 `ControlBody`、`While`、`For` 与 `Loop`。`when` entry、
  condition 与 `for` binding 以内嵌有序结构保存；专用 control body 允许后续 Phase 2 读取
  尾表达式，而普通 block 继续固定为 `Unit`；
- 缺 `else` 的 `if` 在语法构造完成后通过显式工作栈遍历 AST 父子关系：只有完整 block /
  lambda 非尾 / control statement element 获得 statement context，initializer、实参、运算符
  操作数、return 值及 lambda / control 尾值均发 L0057。该遍历为 `O(n)` 且不把平坦或深层
  AST 再次映射为 Rust 调用栈；
- `return` 同行可带值，换行结束裸 return；Parser 只保存最近 callable jump 的结构，不提前
  做 return / break / continue target、分支类型或 `Nothing` 检查。`when` 保存两种形态及
  换行 / `;` entry 分隔，loop body 必须为 block，`super<Interface>.member` 复用既有 postfix；
- postfix 循环已增加 `Expression::Propagate { value, question_span }`，与 call、index、member、
  `!!` 和 callable reference 左结合并保持单调迭代；Phase 1 在所有 expression context 保存
  该节点，不提前检查 `Result<T, E>` 或 callable 返回类型。Lexer 最长匹配继续使 `?.` / `?:`
  分别属于 safe member / Elvis；传播后普通成员访问使用显式分组 `(result?).member`；
- 顶层与独立声明入口已解析 `value class` / `class` / `interface` / `enum class` / 具名
  `object`，保存 visibility wrapper、主构造器字段、泛型、源码有序 supertype、enum 变体及
  body member；member 复用既有 Function / Constant Item，`companion object` 使用独立 boxed
  payload，避免复制 callable AST。具名 object、interface companion 常量与关联函数只保存
  Phase 1 结构，不提前做名称、类型、常量求值或运行时状态检查；
- class-family body 按换行 / `;` 分隔 member，enum 变体按逗号分隔并以 `;` 进入成员区；
  L0066–L0077 覆盖头、字段、supertype、member、variant 与修饰符恢复。`by` 仍是普通
  identifier，但 ordinary class supertype entry 可提交 `Interface by field` 并保存
  `DelegationClause`；L0078 覆盖缺失目标。非 ordinary class、匿名 / nested / local
  class-family、构造器调用、普通 body field、属性委托与任意 delegate expression 仍被拒绝；
- SPEC-0201 已让 class/value/interface/enum/object 的 instance function 按固定
  visibility→`override`→receiver→`fun` 顺序保存缺省或显式 Borrow/Inout/Value receiver；
  `DeclarationModifiers` 保留真实 marker kind/Span。顶层、companion、constant、field 与 classifier
  位置只消费并定向拒绝非法 marker，不把它泄漏进 AST；L0076/L0077 与既有 owner-aware recovery
  保留下一 member、enum delimiter、所属 `}` 和后续顶层声明。该阶段不规范化 receiver，也不检查
  object/override/interface contract；
- variable / constant / function declaration 显式接收调用位置的 expression stops：文件根保持
  file declaration 边界，class-family member 额外保留所属 `}`，因此缺失类型、initializer 或
  函数表达式体不会消费 class closer 并把后续顶层声明误归入 member body；
- `Expression` 与 `TypeRef` payload 只通过现有 typed ID 连接，叶与合成节点都保留同一
  `SourceId` 的 UTF-8 字节 `Span`；源码拼写继续由共享 `SourceMap` 回查；
- Lexer invalid / reserved token 被消费为显式 Error 节点且不重复同源诊断；delimiter、插值
  stop、不结合链与尾随 token 使用既有 `L0009`–`L0015`，typed call argument 与参数 marker
  使用 `L0033`–`L0039` 做 owner-aware 局部恢复，局部解构使用 `L0040`–`L0046`，control-flow
  使用 L0055–L0065，class-family 使用 L0066–L0077，委托目标使用 L0078。postfix `?` 不需要新错误类别，缺左
  operand 继续使用 L0009；
  已发布的 `L0016` 仅保留在 catalog，生产
  Parser 不再发出；
- expression tail 的 `L0013` 恢复复用 declaration owner stack，并由完整的内部
  `Stops`→`DeclarationStops` 映射保留 comma、delimiter、file、arrow、else 与 block-element
  边界；nested string / interpolation 的同形 closer 只在 owner 回到 baseline 后才可停止恢复；
- 递归 Pratt 实现在固定 32 MiB 的 scoped worker 隔离栈上运行，并在 1024 个内部递归预算
  单位处返回具体资源错误；这避免调用线程的小栈或输入 token 数放大栈申请，也不新增未经
  guide 分配的用户诊断码；
- 三个入口各自只解析一个独立表达式、简单声明或 block。block element 的 hard owner closer
  与只在 delimiter 外生效的 soft structure stop 分离；局部声明、字符串 / 插值 terminal owner
  和 nested block 恢复保持单调前进，`L0028`–`L0030` 分别稳定表达缺 block、非法 element 与
  已延后的 element；lambda body 复用相同 hard-owner 规则，并以 `L0031` / `L0032` 区分缺少
  body element 与当前阶段不支持的 body 形态。完整文件入口在 owner baseline 将 `val` / `var` /
  `package` / `import`、简单声明、class-family starter、visibility / `override` 前缀与 `;`
  识别为恢复边界；合法文件头
  只允许可选首部 package 和声明前 imports，exact / 末尾 wildcard / exact alias 均保存真实
  segment 与标记 Span；顶层构造之间接受实际 LF / CRLF 或一个 `;`，同行
  缺 `;` 以 L0047 报错并保留后一声明。terminated block comment 内的换行计入分隔，space、
  tab 与无换行注释不计；前导 / 连续 `;` 以 L0017 / Error Item 恢复，block 与独立声明入口
  不获得分号分隔语义；
  Lexer 在 EOF 以终止性 char/comment 根因抑制外层 string/interpolation 级联诊断时，恢复索引
  按 inner-to-outer 顺序补齐剩余 lexical owner；原 Lexer 诊断保持不变，合法 Lexer 产物不再
  被误判为 `InvalidLexemeStream`；独立声明入口遇到完整 string 作为非法声明起点时，同样从
  `StringStart` 整体消费该多 lexeme owner，形成覆盖完整 string 的单一 L0017 / Error Item，
  不让 tail recovery 从 owner 中途开始；class-family 名称位置也通过相同预索引边界把完整、
  含词法 poison 或终止恢复的 segmented string 收敛为一个 `NameMarker::Error`，保留既有
  L0067 且不从 owner 中部继续解析；
  后续拆分和顺序见 [Spec 路线图](../specs/README.md)；
  Parser 自身仍不做名称 / 类型 / 所有权检查；CLI bootstrap 与单文档 LSP 分别在外围显式
  编排后续阶段，通用 CLI 和跨文件 LSP 仍属后续 Phase。

## 单文件名称解析

`lang_frontend::name_resolution::resolve_names(&SourceMap, &ParsedFile, &NameEnvironment)` 在固定
32 MiB scoped worker 上遍历只读索引式 AST，返回 `NameResolution`；SourceMap、AST 或诊断
模型不变量失败通过 `NameResolutionError` 返回，普通名称错误进入结构化诊断。

- `NameEnvironment` 由调用方显式预声明外部 type、value 与 function overload，不隐式加载
  prelude、不读取文件系统，也不修改输入环境；同名外部函数保持声明顺序，其他同命名空间
  冲突在环境构造边界返回具体错误；
- 文件、classifier、companion、function、lambda、block、control body、loop 与 enum variant
  分别建立带 parent 的稳定 scope；顶层与 classifier member 先收集后解析，block local 则在
  initializer 完成后才进入当前 scope，稍后 local 只预扫名称和 Span 而不提前占用 SymbolId；
- 类型和值命名空间独立；具名 object 同时产生 type 与 singleton value，函数同作用域形成
  源码有序 overload set。嵌套 scope 允许遮蔽；companion 向外查询时跳过实例 member scope；
- `NameReference` 保存发生 scope、查询命名空间及源码 / 外部唯一 symbol、overload set、
  unresolved 或 later-local 目标。普通名称和 TypeRef 首段在本阶段解析；member、构造器、
  overload 选择和限定类型后续段等待类型与 package 阶段；
- L0079 的 primary 指向后声明并 label 首个冲突，L0080 指向未解析 Identifier，L0081 指向
  声明前引用并 label 稍后 local；最终诊断复用全 frontend 的确定性排序。

## Compilation-unit package/import 名称解析

SPEC-0025 新增纯内存的
`lang_frontend::name_resolution::index_compilation_unit(&SourceMap, &[SourceUnitInput])` 与
`resolve_compilation_unit_names(&SourceMap, &[SourceUnitInput], &CompilationUnitIndex,
&NameEnvironment)`，在不读取文件系统、不修改旧 `resolve_names` / `ReferenceTarget` 语义的
前提下完成多文件名称解析：

- 输入显式携带不透明 root identity、root-relative UTF-8 logical path、同一 `SourceMap` 的
  `SourceId` 与 `ParsedFile`；foreign/mismatched/duplicate source、重复稳定 key，以及空、绝对、
  空段、`.` / `..` 或父目录非 Koven Identifier 的 logical path 都在无 L-code 的输入边界拒绝；
- source unit 按 `(root identity, logical path)` 排序后分配 `SourceUnitId`，package 按 Identifier
  segment 序列排序后分配 `PackageId`；多个 root 可贡献同一 package。`DeclarationId` 再按
  canonical source unit、文件 root 顺序和 object 的 Type→Value 固定次序分配，不使用
  `SourceId`、展示路径或调用方枚举顺序；
- collector 展开顶层 modifier wrapper，保存 root `ItemId`、规范化 public/internal/private、
  namespace、kind 与 name Span；普通 classifier 只发布 Type，具名 object 同时发布 Type 与
  singleton Value，跨文件同 package 函数可形成后继 overload，其他同命名空间冲突发 L0147；
- logical parent package 与源码 directive 不一致发 L0146；嵌套路径省略 package 时 primary 是
  文件起始空 Span，已有 L0048 的 malformed package recovery 不叠加 L0146。Parser 与 unit
  诊断共同按 stable source key、byte range、code 和完整 detail 排序；
- `CompilationUnitIndex` 始终可供诊断/工具读取；只有 Parser、L0146/L0147 均无 error 时才能
  取得 `ValidatedCompilationUnitIndex`；名称阶段会重建并核对 inputs/index，拒绝错配输入；
- `UnitSymbolId` 由 `SourceUnitId + SymbolId` 组成；`CompilationUnitNames` 为每个 source 保存旧
  单文件 resolution 与并行的 `UnitReferenceTarget`/`UnitNameReference`，使 package、声明、
  overload、source symbol 和 external target 都能保持稳定 identity；
- same-package binding、exact/alias import 与按需 wildcard lookup 遵循类型/值双命名空间；源码
  绑定优先于 compiler external，lexical/local root 优先于 absolute package path，多个 wildcard
  候选只在实际 bare-name 使用处发 L0151；
- 限定路径支持绝对 package 路径、导入或同 package 的 `Type.member`、enum case，以及 public
  object/companion 静态成员；private 顶层或静态成员不可跨文件使用，exact import 仍只接受
  顶层终端；
- recovery `CompilationUnitNames` 始终携带确定性 reference/diagnostic；只有无 error 时才能取得
  `ValidatedCompilationUnitNames`。该 marker 只证明名称解析成功，不代表 SPEC-0197 typed
  compilation unit 已完成。

## Compilation-unit 类型签名

SPEC-0197 第一阶段新增纯内存的
`collect_compilation_unit_signatures(&SourceMap, &[SourceUnitInput],
&ValidatedCompilationUnitNames, &TypeEnvironment)`。它只消费无名称错误的 validated product，
重建并核对输入/index，同时验证名称与类型环境共享同一 analysis owner：

- `CompilationUnitNames` 显式发布完整 `DeclarationId -> UnitSymbolId` 映射；类型阶段不再根据
  Span 猜测顶层声明对应的文件局部 symbol；
- `UnitTypeTable` 是一次 compilation unit 唯一的结构化类型空间。源码 nominal 使用
  `DeclarationId`，类型参数、field、member 与 enum case/payload 使用 `UnitSymbolId`，因此不同
  source 中数值相同的 `SymbolId` 不会碰撞；
- 签名 collector 解析过的参数、返回、field、supertype 与 bound `TypeRef` 以
  `UnitTypeRefId -> UnitTypeId` 完整进入 signature product；后续 body product 对局部标注可追加
  facts，并在查询时回退签名 facts，不重新解析或丢弃已经确定的 unit 类型；
- callable 与 construction 的具名/位置实参、参数模式和 arity 映射已下沉为对类型身份泛型化的
  内部纯内核；单文件 checker 通过薄适配继续使用 `TypeId`，unit body checker 可复用同一规则并
  使用 `UnitTypeId`，两条路径不复制可观察诊断语义；
- 单文件 `TypeTable` 与 `UnitTypeTable` 复用同一个插入有序、结构去重核心，并以相同顺序建立
  builtin、Signed/Unsigned integer literal 与 Error 初始种子；公开 `TypeId` / `UnitTypeId` 仍是
  两个不可混用的身份域，unit integer literal 仅供后续 body expected-type 定型使用；
- collector 先预声明全部 nominal 与类型参数，再按 canonical declaration order 收集 callable、
  主构造器 field、enum case/payload、直接 interface 与 companion/member callable signatures，
  支持同 package 跨文件递归签名且不依赖调用方输入顺序；
- 类型参数上界规范化为 `Any`、interface instance、compiler capability 或 Error；直接 supertype
  拒绝非 interface 与重复 root，interface cycle 按稳定边删除并发布 L0096，合法 DAG 再完成带
  泛型替换的传递 closure；同一 interface 经不同路径形成冲突的 invariant instance 时拒绝后续
  direct edge 及其整条 branch。interface type-argument bound 在签名使用处复用 L0093；
- 顶层 package overload 与 member overload 共用 alpha-equivalent 参数 shape 判定，重复 shape
  发 L0097，instance 与 companion scope 独立；member contract、override/default 与窄化委托
  复用 L0098–L0105，并发布带 `DeclarationId` / `UnitSymbolId` 的合法 delegation plan；
- runtime interface position、跨文件 `Copyable` / `Transferable` bound 与 value/enum inline layout
  graph 分别复用 L0094、L0115/L0141 与 L0116；类型实参数量错误沿用 L0082/L0091；
- signature product 绑定 canonical input index、逐 source 名称 analysis owner 与类型环境 owner，
  clone 保持同一分析身份，结构相同但来源不同的产品不能进入后续阶段；诊断只包含本类型签名
  阶段并继续使用 unit 稳定排序；
- recovery `CompilationUnitSignatures` 可带签名诊断，只有无 error 时才能取得
  `ValidatedCompilationUnitSignatures`。该 marker 仍不是完整 typed unit：函数体、调用选择、
  flow facts 与 ownership 交接属于 SPEC-0197 第二阶段。
- production `check_compilation_unit_types` 内部收集 recovery signatures，并在其拥有的同一
  `UnitTypeTable` 中按 canonical `DeclarationId` 顺序检查 body；signature error 不会全局短路，仍会
  检查可独立判定的函数体。产物以 `UnitExpressionId` / `UnitTypeRefId` / `UnitSymbolId` 发布
  source-qualified facts，并把 signature/body diagnostics 合并后只做一次 unit 稳定排序；
- 首个 body 纵向切片已支持顶层函数 absent、expression body、block expression/`return`、标量与
  非插值 String literal、参数/顶层名称，以及非泛型顶层 source direct/overload call；成功调用发布
  `UnitCallTarget::Declaration`、源码实参到参数的映射、mode、place/temporary category 与统一
  `UnitTypeId`。错误 body 不阻止其他 source facts，signature/body 任一 error 都阻止 validated view；
- 第二个 body 切片已支持 block 内局部 `val`/`var` 的 initializer 推导与简单显式标注，发布
  body-local `UnitTypeRefId` / `UnitSymbolId` facts，并支持 `!`/一元正负、数值/字符串加法、
  数值四则、比较、相等与逻辑运算；有符号最小值按单一负字面量定型，L0084/L0085/L0090
  保持既有诊断语义，局部 place 可直接进入跨文件 call argument descriptor；
- 第三个 body 切片已支持 `if` 条件的 Boolean expected type、control body 尾值、外层 expected type
  下传、同型/`Nothing`/`Error` branch join、缺 `else` statement 的固定 `Unit`、两分支退出的
  falls-through 合并，以及 L0089；嵌套 `return` 始终引用 callable 返回标注，不会误用局部或分支
  expected span；同期修正单文件 checker，使无花括号 control 分支也遵守 §22.3 的 expected-type
  下传，分支内跨文件 call 与局部事实沿用同一 unit identity；
- 第四个 body 切片已支持跨文件 nominal/enum case `is`/`!is`，为参数与未发生赋值的 local
  `val`/`var` 建立 source-qualified 稳定 place key；`!`、`&&`、`||` 分别交换或短路传播事实，
  `if` 按全部 fall-through 出口求交并排除 `Nothing` 出口。enum case 可赋给/join 回 root，既有
  v0.24 `T? is T` 同样产生 non-null flow type 并支持 `T`/`T?` join；非法关系与 case type 的普通
  标注分别复用 L0106/L0114。该切片不包含候选 v0.35 nullable `when`/`!!`；
- 第五个 body 切片先在原 flow facts 下检查 assignment target 与 RHS，再清除稳定 target 的
  smart-cast fact；SPEC-0218 已把非 container 普通 `=` 收口为声明/storage type 驱动的 expected
  检查，成功节点固定为 `Unit` 并发布 source-qualified target/value/operator/storage-type/
  `falls_through` descriptor，RHS 不匹配复用 L0084。descriptor 随完整 trial parts 原子回滚，
  错误节点不发布半成品；Phase 3 mutable-place/loan 仍不前移。五种 compound assignment 因现行
  guide 尚未封闭 target 单次求值与旧值读取顺序，继续保持 `Deferred(Assignment)`；
- SPEC-0219 在全部 body traversal 后冻结 unit type table 的 owner 候选快照，为其中 concrete
  ordinary-class instance 发布 `UnitRuntimeFieldLayoutDescriptor`：descriptor 以 owner `UnitTypeId`
  唯一限定，保存 declaration、完整 arguments，以及源码字段顺序的 symbol/template/concrete type/
  span。字段 concrete type 由 frontend 递归替换并 canonicalize，不依赖 construction/call/entry
  reachability；替换中新 intern 的类型不反向扩张本轮 owner 候选，因此
  `Grow<T> -> Grow<List<T>>` 一类 heap 递归有限终止。generic template、非 class 与任一诊断恢复均
  不发布布局，validated product 才能交给 codegen；
- 第六个 body 切片已接通局部 value-class 解构：initializer 只检查一次，跨文件 generic field
  类型按实际实参替换，产物以 `UnitStatementId` / `UnitSymbolId` 发布有序 component 与
  `Copy`/`Consume` descriptor；普通 class 保持 `Deferred(Destructuring)`，错误 arity 复用 L0118
  并把 label 指向跨文件类型声明。Consume 仍只是交给 Phase 3 的原子动作，不在类型阶段判定
  move-after-use；
- 第七个 body 切片把 AST owner 决定的 value/statement expression context 提取为单文件与 unit
  checker 共用分析，并接通 subjectful/subjectless `when`。Boolean、跨文件 enum 与 nullable
  封闭域参与穷尽性，`is`/`!is` 和 comma alternative 发布事实交集，分支按 `Nothing`、enum root、
  nullable 与 `Any` 合并，flow 只求全部 fall-through 出口交集；L0107–L0112、跨 source 缺失
  enum-case label 与输入置换保持稳定。nullable subject 的 `null` condition 只复用现行 v0.24
  语义；else 不获得候选 v0.35 remaining-domain fact。该子切片当时未接入一般 null；contextual
  null 与 null-comparison 已分别由第十七、第十九个切片接通；
- 第八个 body 切片接通 `while`/`for`/`loop` statement 与 `break`/`continue` expression：while
  condition 接受 Boolean expected type，三种 loop 都建立最近词法 loop depth，合法 jump 发布
  `Nothing`，越界使用 L0142；`for` source 仍沿用单文件阶段边界，binding 与由它推导的 local
  发布 `Deferred(LoopSource)`，不伪造候选 v0.37 provider/element type。Unit callable 的有值
  `return` 同期对齐为 L0087 shape mismatch；
- 第九个 body 切片接通 body-local function TypeRef 与 lambda：已知 expected function contract
  决定 move/arity、parameter type/mode 与 body return expected type，模式事实以 source-qualified
  symbol 发布；无参 lambda 可从已知尾值推导返回类型，lambda 自身保存独立 return span 与 loop
  base，不能由内部 `break`/`continue` 穿越 callable boundary。唯一 source callable 向实参传播
  参数 expected contract，并为直接不匹配保留 L0084 与参数声明 label；多个 overload 候选的普通
  实参仍先独立定型，避免把候选过滤错误混入直接诊断；
- 第十个 body 切片接通 overload-lambda candidate isolation：普通实参只检查一次并先过滤候选，
  每个剩余候选从包含 unit type table、body/flow/call facts 与诊断的同一 snapshot 独立试算；零个、
  唯一或多个成功分别发布 L0123、原子提交唯一 facts，或发布带至多两个跨 source 声明 label 的
  L0124。失败与歧义均恢复 baseline，不泄漏 lambda parameter type/mode、嵌套 call 或候选诊断。
  为保持职责与文件规模，call mapping/selection/descriptor、trial state、body binding facts 和 lambda
  分别位于独立子模块；
- 第十一个 body 切片接通 body-local 泛型类型引用：qualified TypeRef 递归解析 source nominal、
  type parameter、external builtin/capability/value 与 intrinsic 类型实参，并在 unit type table 发布
  完整 `UnitTypeRefId` facts；nominal/interface/Copyable/Transferable bound、runtime interface、enum case、
  intrinsic arity、`Box` 与顺序容器 storable 分别复用 L0082/L0091/L0093/L0094/L0114/L0115/L0117/
  L0125/L0141。签名阶段已诊断的无限内联 nominal 集合随 signature facts 保留并供 body storable/
  capability 恢复读取；嵌套 Error、`AnyValueRepresentation` 与普通位置 enum case 保持单文件诊断顺序；
- 第十二个 body 切片接通 source 泛型调用：候选接受完整显式类型实参，或仅从已定型非 lambda
  实参沿 nullable/function mode 与 move shape/nominal/intrinsic 结构做 invariant 推导，不从返回
  expected context 或 lambda body 反推；interface/Copyable/Transferable bound 在实例化后过滤，
  `UnitCallableInstanceKey` 按 callable 类型参数声明顺序保存完整实参。generic/non-generic mixed
  overload 与 lambda trial 继续复用既有 mapping/filter/snapshot；唯一候选把实例化参数类型作为
  contextual expected type，预检查实参不重复遍历且不匹配时保留 L0084/声明 label，Error/Deferred
  不产生 L0123 级联；
- 第十三个 body 切片接通 external 与函数值调用：compiler-bound function 使用环境签名建立
  `UnitCallTarget::External`、参数 mode、cross-thread/abort/println effect，普通函数类型值使用
  `FunctionValue` 并保留 move-only ABI shape；成功 direct call 同步发布 callee Function type/category。
  external overload 只在全部候选已绑定时选择，partial/unbound 集合保持
  `Deferred(UnboundExternalType)`；普通 overload 与 overload-lambda 的嵌套 Deferred 保持
  `Deferred(Call)` 并完整回滚 trial，映射失败仍检查每个 operand。由于普通 Function 类型尚不能
  保存 compiler-bound identity/effect，effectful external 被取值、分组或别名化时显式
  `UnsupportedBody`，direct effectful call 与显式类型实参 recovery 不受该门禁影响；
- 第十四个 body 切片接通源码 nominal 与 enum construction：`class`/`value class` 主构造器、payload
  enum case call 与 zero-payload case value 发布 `UnitConstructionTarget`、完整类型实参、声明顺序
  Value operand、source-qualified field/payload symbol 和源码 evaluation index；泛型实例只接受完整
  显式实参，或依次从已定型非 lambda operand 与独立完整同 root expected-result 推导，候选局部
  lambda expected 不参与推导。mapping、bound 与 L0084/L0091/L0093/L0115/L0120–L0122/L0141/
  L0143/L0144 复用单文件语义；construction facts 纳入 overload trial snapshot，失败、Deferred 或
  歧义均不泄漏，输入置换保持 identity/facts/diagnostics 稳定。compiler-bound `Box` / `Rc` 另由
  external intrinsic identity 选择，发布 `IntrinsicBox` / `IntrinsicRc` 与无源码 parameter symbol
  的单一 Value operand；显式或 operand payload、Box concrete value-class、Rc structurally-storable、
  result expected 与 L0084/L0091/L0117/L0120–L0122/L0125 均在 descriptor 写入前完成，源码同名
  class 不获得 intrinsic identity；
- 第十五个 body 切片接通 classifier member body 与 source member 选择：普通 nominal 的 `this`
  使用 owner generic instance，interface body 使用 `StaticSelf`，companion function 不继承 instance
  receiver；隐式 bare member 与显式 receiver call 复用 overload/generic/lambda trial，override shape
  优先于 interface 默认候选。field 与 enum payload 访问发布 `UnitAggregateProjectionDescriptor`，
  自动 `componentN()` 在无显式同名 callable/field 时发布 `StructuralComponent` call/projection；
  projection expression/field 及显式 receiver 使用 source-qualified identity，裸 payload receiver 以
  declaring classifier identity 表示隐式 `this`；field 为 place、结构分量 call 为 temporary。member
  `private` 以 lexical owner 检查（含 safe/nullable recovery），owner-dependent generic bound 与
  alpha-equivalent override shape 保持实例语义；L0084/L0113、输入置换与失败 trial 回滚保持单文件
  语义。SPEC-0180 在此基础上把缺省/显式 Borrow、Inout、Value 规范化为隐藏 receiver contract；
  callable signature、实例 key、显式/隐式/`super<I>` call descriptor 均保存实例化 receiver type、
  mode、place category 与 source-qualified origin。receiver mode 不进入 overload shape，但参与
  interface replacement/override/default contract；`super<I>` 只允许当前 owner 的 interface closure，
  并检查当前 receiver capability。裸 field 发布 `This(owner)` projection，具名 object 调用保留
  instance receiver 且 object 非 Borrow marker 以 L0099 拒绝。窄化 `by` 委托按源码顺序发布
  Borrow-only forwarder；任一剩余 Inout/Value requirement 以 `by` 为 primary、首个源码 member 为
  label 发布 L0152，并原子清空该 plan 的 forwarders。direct concrete delegate 的 forwarder 还发布
  已验证有体 effective target 与完整 owner template；本地 override、继承/default replacement 与泛型
  substitution 保持原 identity。所有 plans 完成后，forwarder resolution 收敛为 direct implementation、
  exact next hop 或 unresolved 私有三态：same/changed requirement identity 的下一跳保存稳定 target 与
  receiver template，后者按 delegate nominal formals→field actuals 递归实例化；type-parameter delegate
  保持 unresolved，不把 abstract requirement 伪装为实现。SPEC-0181 进一步消费这些 facts，发布
  receiver loan/copy/move、Value `this` unique drop 与 Borrow-only delegation ownership plan，
  并移除一般 member call 的 `MemberReceiver` deferred。interface default 的 Value `StaticSelf` 不修改
  通用 copyability，而以 owner/type-template/point/origin 精确且去重的
  `UnitConditionalReceiverDropFact` 单独发布；失败恢复原子清空该事实。native lowering 由 SPEC-0191 承接；
- 第十六个 body 切片接通 non-nullable intrinsic `Rc<T>` 的 `.value` 与零参数 `.share()`：
  `UnitRcOperationDescriptor` 保留 source-qualified expression/receiver、unit-global payload type、
  compiler-bound operation identity 与 Borrow/Value result mode；`.value` 为 place，`.share()` 为
  temporary，错误 type argument/argument mapping 不发布 partial fact，overload-lambda trial 与输入
  置换保持原子/确定。payload 内递归 `Error` / `Deferred` 不发布可验证 operation；源码同名 `Rc`
  仍走 nominal/member identity，safe/nullable Rc 不在本切片内；
- 第十七个 body 切片接通 core container construction 与 contextual null literal：
  `UnitContainerConstructionDescriptor` 保留 source-qualified call、unit-global container/element type、
  封闭 construction/container kind 与参数 mode；列表式 expected/显式/首元素推导、运行时长度
  Borrow contract、空 `MutableList`、L0091/L0125–L0127、源码同名隔离与输入置换保持单文件语义。
  无 nullable expected 的 `null` 进入 L0083 recovery，非 nullable expected 进入 L0084；失败 construction
  不发布 partial fact，overload-lambda trial 只原子提交唯一成功 container/callee facts；
- 第十八个 body 切片接通 intrinsic container index/place/member/assignment：
  `UnitElementPlaceDescriptor` 保留 source-qualified expression/receiver/index、unit-global element type、
  container kind 与可变性；`size` 是 Int temporary，`[]` 是 place，Array/MutableList 可替换而 List
  只读。L0085/L0122/L0128–L0130、RHS 类型检查、Inout 类型化后二次可变性验证、poison fail-loud、
  源码同名隔离、overload-lambda trial 与输入置换保持单文件语义；
- 第十九个 body 切片接通 `T? == null` / `!= null` 与非空使用事实：另一侧先定型并把 nullable
  expected type 交给 null literal，stable symbol 的 true/false edge 可进入 `&&`/`||`、`if` 与后续
  non-null member use；`UnitNullComparisonDescriptor` / `UnitNonNullUseDescriptor` 保留 source-qualified
  expression/symbol、声明 nullable 类型与 inner type，overload-lambda trial、输入置换及 L0083 recovery
  不泄漏 partial facts。稳定 symbol 与单文件规则一致：lambda parameter 合法，被 lambda 捕获的
  mutable local 排除，短路合并遇到同一 symbol 的冲突窄化类型时删除事实而不是覆盖；
- 第二十个 body 切片接通 String interpolation typed traversal：每个 interpolation expression 按源码
  顺序进入现有 unit expression checker，嵌套 call/type facts 与诊断恢复完整发布，外层表达式固定为
  `String`，输入置换保持稳定；这不引入 printable/formatting protocol，native lowering 继续按
  SPEC-0192 确定性拒绝 interpolation；
- 第二十一个 body 切片接通 top-level variable/const initializer：unit 按稳定 logical-path/source
  declaration 顺序执行普通 typed initializer，显式标注单向约束 initializer，无标注值把已知结果发布
  到 source-qualified body symbol facts；后续 declaration reference 优先读取该 fact，再回退 signature。
  已标注前向引用可直接读取 signature；较早读取尚未检查的无标注值继续保留单文件
  `Deferred(ForwardValueType)` 边界。文件 initializer 的 `return` 使用 L0086，错误不阻止独立后续
  declaration；`const val` 不产生候选 v0.36 evaluator/ConstValue；
- 第二十二个 body 切片接通现行 expression-tail traversal：`super<Interface>.member` 的接口 TypeRef
  使用 static use，不再误报 runtime-interface L0094；Elvis 复用 nullable inner/`Nothing` join 与
  L0085，range/`to`/`in`、cast、postfix `?`、callable reference 保留单文件既有专用 Deferred reason，
  基础 `!!` 抽取 nullable inner。普通 nullable Elvis 的非空路径可绕过右侧，`Nothing?` 则沿右侧
  fallthrough；cast、postfix `?` 与 bound callable reference 继承 child fallthrough。全部 child
  expression 与 TypeRef 仍发布 source-qualified facts，输入置换保持稳定；expression variant dispatch
  已是穷尽匹配。v0.35 的 nullable remaining-domain、`!!` Copy/Consume descriptor 与其余候选能力仍未启用；
- 完成审计补齐 classifier/companion 普通 constant initializer 与 package-qualified 静态目标：member
  constant 在 signature pass 预声明普通类型，并在 body pass 按源码顺序检查 initializer；无标注
  package-qualified 顶层值读取已发布的 body symbol type，category 与裸 declaration 一致保持 temporary；
  package-qualified companion call 只从 companion member set 选择 source-qualified symbol target，不与
  instance candidates 重复。safe/nullable/poisoned Rc 与 poisoned container element place 使用专用
  Deferred recovery，且不发布伪 Rc operation/element-place descriptor。这些事实不引入 v0.35 nullable
  ownership，也不启用 v0.36 ConstValue/evaluator。

## Compilation-unit ownership

SPEC-0198 第一切片已建立与单文件 `OwnershipCheckedFile` 并行的
`CompilationUnitOwnership` recovery product：入口只接受 SPEC-0197 的 validated typed unit，并
重新核对 source inputs、validated names 与 `TypeEnvironment` 身份链；产物
另持有 typed-analysis owner，混用同结构但不同分析的 names/types、重复 source input 或 foreign
environment 会在所有权遍历前失败。当前已按 `UnitSymbolId` 稳定排序发布顶层、instance member、
companion member 与 expected lambda 参数的 Owned/Shared/Exclusive binding capability，并保留参数
声明范围供后续跨文件诊断引用。第二切片把每个成功 typed call 的 source-order argument mapping
归一化为 source-qualified `UnitCallArgumentOwnershipContract`：保留 call/argument identity、实例化
参数类型、Value/shared-loan/exclusive-loan 契约、跨线程标记、实参与 call 范围；Declaration/Symbol
target 还回链真实参数声明范围，external/function-value 不伪造源码位置。该 contract 只是后续
body-local 数据流的输入。第三切片已按规范 source/AST 顺序执行普通 typed call contract：复用 unit
类型检查的唯一条件 `Copyability` 算法，把 Value argument 分成 Copy/Move/Temporary delivery，按
`UnitSymbolId + field path + terminal element identity` 建立 shared/exclusive 同步 loan，并在 call 返回时结束；保守控制流合并
possible-move，复用 L0131–L0136 的适用诊断，诊断可同时标注使用文件中的 move/loan 起点和跨文件
目标参数声明。字段 place、具名实参重排、function-value/external、input permutation 与错误 unit 的
原子边界均有回归：任一 ownership 诊断会清空 loan/delivery 可执行 facts，recovery contract/binding
仍可用于后续诊断。assignment 始终静态检查 target 可变性，只有 RHS 正常返回才执行动态 place
access；裸字段与显式 `this.field` 统一为
source-qualified field place，`val` 或非 Inout receiver 的 mutation 使用 L0134 拒绝，避免 validated
ownership 绕过字段可变性或 active-loan 检查。当前已额外发布 intrinsic Rc `share/value` 的 source-qualified retain/
borrow-payload effects，支持 named/temporary owner、L0131/L0132 与错误事实原子清空。第五切片已进一步
消费 source-qualified constructor descriptor，按 operand 源码求值顺序发布 Copy/Move/Temporary
delivery、MoveOnly inline/heap/shared root obligation 与 `Nothing` 提前终止前缀，并为跨文件 field/
payload 参数保留诊断标签。第六切片已把 intrinsic container descriptor 归一为既有 source-qualified
contract：list-form 重复执行 Value delivery，runtime-length 执行两个同步 Borrow，空 MutableList 无
operand effect。第七切片已按 source/lambda 顺序发布 source-qualified capture 输入：词法 binding 使用
`UnitSymbolId`，字段/显式 receiver 规范化为 `This`，默认与 `move` lambda 分别记录 Borrow 与
Copy/Move，并复用 unit 类型能力图发布具体 environment `Transferability`。第八切片在 AST 上建立
source-qualified 反向 liveness，并在 body-local 数据流中执行闭包 formation：shared capture 建立随
named/direct closure 最后使用结束的 loan，owned capture 执行 Copy/Move，lambda body 使用 non-owning
与 immutable capture state；return/Value delivery/constructor/field 逃逸复用 L0137，非法 owned capture
复用 L0138，compiler-bound cross-thread effect 复用具体 environment `Transferability` 与 L0139。
第九切片复用单文件 liveness/drop planner 契约，发布带 `UnitExpressionId` / `UnitStatementId` /
`UnitItemId` 的 source-qualified `UnitDropPoint`、named/temporary/replaced-element/captured
`UnitDropTarget` 与稳定 `UnitDropFact`；覆盖 unused parameter/local、String binary、call temporary、
replacement、branch/loop/control-transfer、return 与 closure environment 逆序析构。SPEC-0215 实施时
在 root traversal 后按 AST identity 独立计算 lambda body liveness，并为不含 MoveOnly Value
参数的受支持 body 把最后一个 expression element 按隐式返回 Consume：结果 owner 转交 caller，
String composite operand 仍按 `AfterBinaryOperands` 逆求值顺序析构。该切片最初对含 MoveOnly
`if` / `when` result 的 body 原子回滚本轮 lambda drop facts；SPEC-0216 随后统一 control-tail usage
并移除该回滚，SPEC-0199 第二十三步第四切片现已消费这些 facts，且不影响相邻 lambda 或外层
formation state。element 后字段
投影仍按单文件契约显式发布 `IndexPlace` deferred fact，并跳过所属 callable 的不完整 drop plan；
任一 ownership error 原子清空全部可执行 facts。只有无 error 且无 deferred drop 边界时，
`CompilationUnitOwnership::validate` 才发布不可伪造的 `ValidatedCompilationUnitOwnership`，供
SPEC-0199 等后继阶段消费。

## Compilation-unit codegen planning

SPEC-0191 的首个 Phase 4 切片先扩展共享 typed SSA 契约，而不提前猜测 frontend member body。
`Function` 以可选 receiver `EntityType` 标记 instance callable，并要求它精确对应 entry block 首参数；
`DirectCall` 把 receiver 保存为独立于显式 arguments 的隐藏第零操作数。verifier 同时核对 receiver
presence、Value/shared-loan/exclusive-loan mode、具体类型、显式参数边界与返回类型，ownership verifier
按 receiver→arguments 顺序消费 Value 或检查 active loan；`FunctionAddress` 继续拒绝 instance target，
不虚构 bound method value。renderer 显式显示 receiver 分隔，LLVM adapter 按 receiver-first ABI 组装
operands。该首切片提交时 compilation-unit lowerer 仍只生成 `receiver: None` 的顶层 direct call；
source member 的当前接线事实由下一段记录。

SPEC-0191 的第二个 Phase 4 切片已把 unit instance key 从顶层 `DeclarationId` 扩展为静态
`UnitCallableTarget`，并把 owner 与 callable type arguments 合并为同一有序单态化 key；reachable
member body 继续以源码 `ItemId` 定位，不按调用点名称重新选择。unit lowerer 现在可声明并绑定
Value/shared-loan/exclusive-loan receiver，按 receiver→显式 arguments 顺序 lower frontend
receiver fact，并覆盖显式 Borrow/Inout、Copyable Value、MoveOnly Value、临时 receiver 与同 mode
隐式 `this` 转发。Value/Inout `this` 调用 Borrow member 时分别建立普通 shared loan 与显式
`SharedReborrow`；verifier 禁止 derived loan 活跃时提前结束或以 exclusive mode 使用 parent。
receiver 还作为独立线性状态随 conditional/when/loop edge 携带并在 merge block 重绑定。
value-class `this` 的 Copyable field read、Value `this` 返回转移、receiver loan end，以及表达式体隐式
返回的 `ControlTransfer` drop 已进入 verified SSA/LLVM。真实 compilation-unit object/link/run 已覆盖
非泛型 value class 的 Borrow/Copyable Value receiver 与 ordinary class 的 Borrow/MoveOnly Value
receiver，并以 stdout 锁定 receiver→argument→body 顺序、Copyable receiver 重复使用及 class owner
唯一析构；non-generic value class/enum 的 Inout 先把当前 SSA value addressize 为 call-scoped
storage，callee 保持 exact aggregate/tagged type 的 exclusive pointer ABI。value-class Inout 读取 Copyable field 时先从 exclusive
receiver 建立短 shared reborrow，再投影字段并按 field→reborrow 逆序结束 derived loan；真实 native
输出已锁定两种 inline receiver。ownership verifier 已把 `SharedFieldLoan` 登记为 parent/child
dependency，拒绝 derived field loan 活跃时提前结束 reborrow。整体与目标字段均满足 Copyable 的
non-generic value class 已使用独立 `InlineFieldReplace` 完成 inline `var` field mutation：verifier
要求 active、无派生 loan 的 exact exclusive aggregate receiver 与 exact Copyable value，LLVM 对
receiver storage 直接 field GEP/store，不加载或析构旧 Copyable field。caller 在 DirectCall 后依次
结束实参 loan 与 receiver loan，再从同一 `RootPlace` 读取并重绑定源码 root；implicit Inout `this`
则直接转发既有 exclusive loan，不重复 addressize/write-back。non-generic MoveOnly value class/enum
的 root Inout call 在结束全部实参与 receiver loan 后使用 `RootPlaceTake` 从同一 direct root storage
取回 owner；operation contract 要求 MoveOnly、exact root-owner/result type，ownership verifier 消费旧
owner 与 place，并拒绝 active loan 或重复 take，LLVM 只加载 addressized storage。take 结果重绑定源码
root 并在原 drop point 唯一析构，动态 String payload native 已锁定无 double free。MoveOnly inline field
mutation/replacement 仍在 SSA 发布前 fail loud，等待旧字段 drop/glue。后续 generic ordinary-class layout
已按 concrete `UnitTypeId` 开放参数无关 field、恰为 owner direct type parameter 的 field，以及
SPEC-0219 exact owner descriptor 授权、由 `List` / 单参数 ordinary class 递归组成的有限 field recipe；
descriptor 消费逐项核对 owner declaration/arguments 与 field symbol/template/span/concrete type，
fallback 仍只接受 closed/direct-`T`。SPEC-0220 另在 exact descriptor 下开放 direct `T?`，但只在
concrete actual 为 ordinary class、Box 或 Rc 时形成 nullable-handle 字段；function、其他 intrinsic、
非 class / 多参数 wrapper 与参数增长型 nested owner 保持确定性拒绝；
SPEC-0218 已发布普通 `=` 的 Phase 2 descriptor，ordinary-class Inout payload field mutation 的
typed 前置已解除。SPEC-0191 的下一切片已直接消费该 descriptor，为 active heap-owner receiver loan
增加 `HeapFieldRead` 与 `HeapFieldReplace`：read 允许 shared/exclusive receiver 但只读取 Copyable
field，replace 只接受无 active derived loan 的 exclusive receiver，并使用 RHS 而不消费 receiver。
LLVM 从 receiver loan 指向的 caller handle storage load 同一 handle，再以 payload aggregate 做 field
GEP；它只对 field 执行 field-typed load/store/drop，不写 receiver storage、不替换 handle，也不建立 payload-only
call ABI。普通 `=` 先完整 lower RHS，`Nothing` 路径不生成 replace；正常路径核对 assignment 的
expression/target/value/operator/storage-type/control identity 后才写 field。裸 field、`this.field` 与
grouped `this` 共享同一 current receiver identity；真实 object/link/run 由后续 Borrow getter 从同一
caller owner 观察更新值。MoveOnly field 另要求唯一 `BeforeReplacement/ReplacedField` fact，交叉核对
assignment、field symbol 与 target origin；RHS owner 先转交，LLVM 再 load/drop 旧字段并 store 新值，
动态 String 的 source→object→link→run 已闭合。`HeapFieldRead` 仍只接受 Copyable field，任意 owner
expression 与嵌套参数 recipe 的 generic payload layout 继续保持拒绝。interface callable template
以 `StaticSelf(interface)` 保存 receiver；
unit instance key 另存 concrete self，
使同一 default 对不同 concrete owner 分别单态化并计入实例上限。直接 default、concrete override
中的 `super<I>`、default→`super<Base>` 及 `this.otherDefault()` 都复用 concrete receiver loan做静态 DirectCall；声明
owner/源码 origin 仍保持 interface identity，不生成 interface runtime value、vtable 或额外 reborrow。
default body 调用无 body abstract requirement 时，frontend signature contract 为每个非委托
requirement 发布 effective implementation，并保存 requirement/implementation 双方处于 concrete
classifier 参数环境中的 owner templates；来源可为本地 override、interface replacement 后的唯一
default 或独立接口提供的唯一 default，有体 default 本身不作为重定向 key。planner 与 call lowerer
共用同一实例解析入口，先以 concrete receiver formals→actuals 实例化双方 owner 参数、核对调用的
requirement prefix，再原位追加 callable arguments。`Host<X,Y>: Derived<Y>` 因此不会误用 `Host`
的完整参数作为 `Derived` prefix。最终 DirectCall 复用同一 concrete receiver loan 且不计划 abstract
declaration；replacement 与独立唯一 default 已完成真实 native 闭环。参数无关 generic ordinary-class
construction/projection/member receiver 与静态委托现已复用 concrete `UnitTypeId` 建立独立 heap-owner/
payload layout，generic outer/delegate route 只携带具体 receiver type；`Marker<String>` 与
`Marker<Long>` 的参数无关布局 identity 已由 SSA 测试隔离。字段恰为 owner direct type parameter 时，
layout/construction/projection 以 concrete actual 建模，但 generic member replacement 仍按模板 `T`
核对 Phase 3 fact；`Cell<String>` 因而生成 old load/drop/store，`Cell<Int>` 只生成直接 store。
有限递归 `List` / 单参数 ordinary-class recipe 已由 SPEC-0191 按 SPEC-0219 exact owner descriptor
接入 construction/projection/replacement；`List<List<T>>` 与 `Wrapper<List<T>>` 已进入 concrete layout，
`Wrapper<T>` replacement 的 LLVM 顺序保持 old load→field-typed drop→store。direct `T?` 已由
SPEC-0220 对 class/Box/Rc concrete actual 开放；function、其他 intrinsic、generic value/enum/interface
或多参数 wrapper、参数增长型 nested owner，以及
`Derived<List<Y>>` / `Derived<Wrapper<Y>>` 等 inherited owner recipe 仍以 `UnsupportedNode` 拒绝。
SPEC-0220 复用 ADR-0017，把 compilation-unit 的 pointer-like concrete nullable 映射为独立
`NullableHandle`，并用同一 expected-type adaptation 处理 local、constructor Value delivery、Value call、
root assignment、return 与 current-receiver field replacement。non-null inner 先按 frontend ownership
fact 消费，再生成 `NullableWrap`；`null` 只按 expression type 生成 `NullableNull`。nullable field replace
继续核对唯一 `BeforeReplacement/ReplacedField` fact，LLVM 在 setter 中先 load 旧字段、调用 nullable
drop glue（其内部按 null niche 分支）再 store 新值；真实 object/link/run 已覆盖非空→null→非空。
inline/function/String nullable、nullable control flow、`List<T?>` / `Wrapper<T?>`、inherited/参数增长 recipe
与需要尚未定义 inner owner 的递归 SSA type cycle 均保持带 Span 的确定性门禁。
non-generic enum instance receiver 继续复用同一 receiver-first ABI，不新增 enum-only IR：Copyable enum 的
Borrow/Inout receiver 从 root tagged value 分别建立 shared/exclusive loan，MoveOnly enum 的 Value/temporary receiver 把同一
tagged owner 交给 callee 并由既有 drop facts 唯一析构。SSA/LLVM 锁定 caller operand 与 callee hidden
receiver 的 tagged identity，真实 object/link/run 已覆盖三种 mode 并输出 `enum-receiver` / `inline-inout`。generic enum
仍受既有 storage 门禁。SPEC-0221 已补齐整体 MoveOnly 的 non-generic enum 空 payload case：虽然 bare case
expression 保留 member-access 的 Place 类别，validated construction/root obligation 仍优先把每次求值建模为
新的 temporary tagged owner；lowerer 在 local、表达式体 return、Value call 与经 local binding 的 Value
receiver 交付时消费同一 owner。空 case 仍生成零字段 payload aggregate 与 `TaggedConstruct`，LLVM drop glue
按 runtime tag 跳过空 payload、只对 Full payload 执行字段析构；generic enum 与 MoveOnly enum `when` 门禁不变。
Borrow-only 静态委托的下一切片已先建立独立 `SharedHeapFieldLoan` SSA 基元：它只接受 active
shared heap-owner receiver loan，结果类型精确取 payload aggregate 的目标字段，并登记为 receiver
loan 的派生依赖，因此 field loan 结束前不能结束父 loan。LLVM 从 caller receiver storage load 原
heap handle 后直接 GEP payload field，不读取字段 value、不 retain/copy owner，也不建立另一套调用
ABI。该基元提交时尚不表示 delegate route 已接线；route 必须消费 frontend validated typed 与
ownership facts 后才可生成。后续首个接线切片现已完成：planner/lowerer 共用 exact route resolver，
要求 concrete outer declaration、forwarder requirement、delegate field symbol 与 Phase 3 ownership
plan 全部一致，再在 delegate concrete nominal 的既有 static-dispatch facts 上解析真实有体 target；
不按名称或 callable shape 重选。lower 顺序固定为 outer shared loan→`SharedHeapFieldLoan`→显式
arguments→DirectCall，结束顺序为 arguments→field loan→outer loan；真实 native 用例由 `Host`
payload 中的唯一 `Reader` owner 返回结果，并且不读取/copy field value、不 retain、不分配 proxy。
当前已开放非泛型 ordinary class 的单层 abstract/bodyful Borrow delegation：planner 直接消费
frontend forwarder 的 exact effective-target/owner-template，本地 override 与继承/replacement default
均复用同一 field route；default 的 `StaticSelf` 使用 delegate field concrete type，而非 outer host。
当 outer/delegate runtime nominal 均非泛型时，generic interface owner/call instance 也已开放：
resolver 先核对 call key 的 requirement owner prefix 与 forwarder receiver template，再以 exact
implementation owner template 替换该 prefix，并原序追加 callable type-argument suffix；interface
default 保留实现 owner 实参与 delegate concrete `StaticSelf`，concrete override 只保留其实际 owner/
callable slots。`Mapper<String>.map<Long>` 的 default/local override 已完成真实 native 闭环。
codegen 不因 requirement 自带 body 而绕过 field route、静默调用 outer interface default。同
requirement identity 的非泛型 chain 也已开放：resolver 逐跳要求唯一 exact
typed forwarder 与 Phase 3 ownership plan，以 `(owner, requirement)` visited identity 拒绝 cycle；每跳
记录 outer/field/delegate concrete type，lowerer 依次建立 `SharedHeapFieldLoan`，DirectCall 使用最内层
loan，结束顺序为 argument→inner→outer→原 receiver。delegate 上存在同 target route 时，即使 outer
forwarder 带 inherited default `Some` 也必须继续，避免 bodyful default 截断；nested local override 因
没有对应 forwarder而正常成为 endpoint。若下一跳 requirement identity 等于当前 selected implementation
但不同于原 target，resolver 现直接消费 SPEC-0180 的 exact next-hop identity 与 receiver template，更新
current target，并把当前 owner prefix 替换为 next owner prefix、原序保留 callable suffix；不扫描 nested
plan 或按 shape 重选。`Base<Long>.map<Int>`→`Derived<String,Long>.map<Int>` 的 planner key 精确为
`[String, Long, Int]`，真实 native 同时排除 Base default=1 与 Derived default=2，只执行 endpoint override=7。
参数无关 generic runtime ordinary-class outer/delegate、direct owner type-parameter field layout，以及
exact owner descriptor 授权的有限递归 `List` / 单参数 ordinary-class delegate field 已开放；
planner 从 outer concrete layout 取得 delegate concrete type，再以同一 resolver 验证 delegate layout，
route/current receiver/`StaticSelf` 全程保留 concrete `UnitTypeId`；递归 dispatch owner argument 只在
validated delegation 的最终 forwarder 映射启用，不扩张通用 default/override owner substitution。
`Reader<Wrapper<T>>` delegation 已完成 source→object→link→run；inherited recipe 与无 endpoint
unresolved route 仍未开放。
非委托 interface default 的 Inout receiver 已使用既有 concrete `StaticSelf` specialization 与 exclusive
loan pointer ABI 完成 verified SSA/LLVM 和 native 闭环；default 返回 7 时不重绑 class handle，concrete
payload 保持 5。MoveOnly Value default 所需的 `StaticSelf` 条件 receiver-drop fact 已由 SPEC-0181
发布并由 SPEC-0191 显式消费：lowerer 在写入任何该 point 的 drop 前核对 fact 唯一性、interface
owner、原始 receiver template、concrete specialization 与 Value ABI；MoveOnly callee 在正常/提前
return edge 各析构一次，Copyable specialization 跳过。两者已完成 verified SSA/LLVM 与同源 native 闭环。
无状态 object Borrow receiver
已使用空 aggregate 表示唯一且不可观察的 ZST value identity：lowerer 同时核对 value/type 双命名空间
declaration 属于同一 object root，只在 validated temporary SharedLoan receiver context 构造一次 ZST，
再复用 `RootPlace`/`BorrowBegin` pointer ABI；LLVM 仅为调用期 addressization 建临时 storage，不生成
singleton allocation、global、retain 或 drop。现行 v0.34 明确只允许 object 使用缺省/显式 Borrow；
Inout/Value 在 Phase 2 以 L0099 拒绝，因而不是待补接线的 Phase 4 source path。

SPEC-0199 第一切片在 `lang-codegen::ssa::unit_plan` 建立 unit-wide reachability/instance plan。
入口重新核对规范化 source inputs、validated names、`TypeEnvironment`、typed unit 与 validated
ownership 的完整身份链，并以显式 `DeclarationId` entry 为唯一根；只遍历可达 body，函数实例 key
固定为 `DeclarationId + UnitTypeId type arguments`。pending/planned 均使用有序集合，递归、重复泛型
调用与输入置换产生同一计划；每项计划保留 `SourceUnitId + ItemId + Span` body locator 和
`UnitSymbolId -> UnitTypeId` 类型替换。该首切片当时只发布 planning 层，多文件
verified SSA/LLVM、DWARF 与 object/native 由本 Spec 后续切片承接并已在下文完成。

第二切片新增独立的 `ssa::unit_lower`，直接消费上述 plan，而不是把跨文件 call 降级成逐文件重新
分析。当前已把可复制 builtin scalar、`own` 参数、literal/name/group、source `DeclarationId`
direct call 与表达式体返回 lower 到一个 SSA module，并在返回前执行既有 owner-aware verifier。
函数内部名包含 package、`DeclarationId` 与规范 `UnitTypeId` 实参；跨 package alias call 和输入置换
得到相同 SSA，未可达的同名 package body 不进入 module。reachable block/control-flow、Borrow/Inout、
String/aggregate/Rc/container/closure、drop glue、LLVM/DWARF/object 仍明确返回未支持边界或等待后续切片，
不能把本切片描述成完整多文件 native lowering。

第三切片把同一 unit lowerer 扩到 straight-line block、局部变量与显式 `return`，并接入普通 UTF-8
String owner。每次 Value direct call 都按 source-qualified argument identity 查找唯一
`UnitValueDeliveryFact`，再由 typed expression category 与 `Copyability` 独立推导并核对
Copy/Move/Temporary；不以 SSA verifier 通过替代 frontend ownership 契约。lowerer 在
FunctionEntry、AfterExpression、AfterStatement、CallReturn 与 ControlTransfer 边界消费对应
`UnitDropFact`，named/temporary owner 形成显式 `Drop`，返回或跨文件移动的 owner 不重复析构。
当前 verifier 测试已覆盖 String 跨文件移动后由 callee 精确 drop 一次、跨文件 String 结果显式返回不
drop，以及未支持 loop 的原子失败；CFG 和复合 owner 仍由后继切片承接。

第四切片新增独立 `ssa::unit_lower::control`，把 Unit-valued `if` lower 为显式 conditional edge、
branch block 参数与 merge block 参数。进入控制表达式时的 binding 集是唯一合流域：Copyable/Unit
分支局部在离开词法作用域后移除，任何未被 frontend drop fact 清理的 MoveOnly 分支局部都会以
`MissingFact` fail loud；Value `Move` 与 MoveOnly `Temporary` delivery 同步从路径状态转移，避免把
已消费 owner 作为 stale edge argument。正常显式/隐式分支消费 source-qualified `BranchExit`，提前
`return` 只消费 `ControlTransfer` 且不重复执行 branch drop；全部分支 diverge 时不生成伪 merge。
当前 verified SSA 测试覆盖跨文件条件提前返回、输入置换，以及 then 路径向 callee 转移 String、
implicit-else 路径析构同一 owner 的互斥闭环。value-valued `if`、`when`、循环与复合 owner 仍由
SPEC-0199 后续切片承接。

第五切片把同一 conditional CFG 扩展到 Copyable builtin 的 value-valued `if`。`ControlBody` 只把
最后一个 expression statement 作为结果，并在离开整个 control body 时继续消费 source-qualified
`AfterStatement`；两个正常出口把结果置于 merge block parameter 的首槽，随后才是进入 `if` 时的
binding 槽，edge 参数保持同一确定顺序。只有一个正常出口时直接沿用其结果，全部分支 diverge 时
返回 `Diverged`；因此 `Nothing` 与普通标量的 join 不需要伪造值。result gate 先按当前 instance 的
substitutions 解析 concrete type，再判断 builtin/Copyability，不误拒 `T : Copyable` 的 `Int` 实例。
第五切片当时的跨文件测试覆盖 `Int`/泛型 concrete 双出口、result 与同型 live binding 的 edge 槽位
顺序、输入置换和一支提前 `return`；String 等 MoveOnly value result 当时在建立结果 owner transfer
前返回 `UnsupportedNode`，现已由 SPEC-0216 与第二十三步第四切片闭合。

第六切片复用上述 conditional/merge 核心接入 exhaustive Boolean-subject `when`。当前只接受恰好
一个 bare `true` 与一个 bare `false` literal condition；subject 求值一次并直接作为 conditional condition，
true/false edge 可以与源码 entry 顺序不同，但各正常出口的 `UnitDropPoint::BranchExit.branch` 始终保留
原 entry index。Copyable value result 与 Unit entry 内的 MoveOnly Value delivery 共用已经验证的
result/binding 合流；该切片当时让 subjectless、`else`、多 condition 及非 Boolean subject fail loud，
且 MoveOnly control result 尚未发布：unit drop planner 会为 String control-tail temporary 发布
`AfterExpression` drop，而不是结果转移。该 frontend 漂移后来由 SPEC-0216 修正，第二十三步第四
切片现已消费对应 facts；unit lowerer 始终不绕过 validated facts。

第七切片把 `if`/`when` 原有的 carried-binding block/edge/rebind 基元提为 compilation-unit CFG
共享职责，并接入无 jump 的 Unit `while`。preheader 把当前 binding 交付 header block parameter；
condition 只在 header 求值，true edge 把同一组 owner-aware binding 交付 body，正常 body exit 再以
显式 backedge 交还 header，false edge 则在独立 exit block 消费 source-qualified `LoopExit` drop。
body 提前 `return` 不生成回边；测试同时锁定 String owner 经正常 body/merge 回边继续存活，以及互斥
return 路径和零次退出各自唯一析构。`break` / `continue`、bare `loop`、`for` 和循环内 binding 更新仍在
构造可发布 SSA 前 fail loud，由 SPEC-0199 后续切片承接。

第八切片为 unit lowerer 增加显式 loop-context stack：`break` / `continue` 先消费对应 expression 的
`ControlTransfer` facts、清理循环体局部 binding，再记录当前 block/state；完成 body 后，continue 与正常
fallthrough 接回最近 header，break 与 while false exit 在 owner 状态一致时进入共同出口。bare `loop`
没有 break 时只形成 backedge 并返回 `Diverged`，有 break 时不伪造 Boolean condition。nested loop 始终
以 stack top 为 jump target，并由 inner continue 回 inner header、inner break 与 inner false edge 先合流后
继续 outer body 的结构测试锁定。`LoopExit` 是基于 loop-entry state 的粗粒度 fact：共同出口仍拥有的
Named owner 精确 drop，所有实际出口已一致消费时跳过 stale fact；while false 与 break 的 owner 状态不一致
则 fail loud，等待 frontend 提供 exit-qualified facts。`for` 及循环内 assignment 仍由后续切片承接。

第九切片把 Boolean `when` 扩展为源码顺序的短路 entry chain。subjectless condition 直接作为
conditional condition；Boolean subject 先求值一次，动态 candidate 生成一次 equality compare。每个
condition 的 false edge 进入下一个 condition/entry，同一 entry 的所有 matched edge 先合流，再只 lower
一次 body；body 出口继续消费原源码 entry index 的 `BranchExit`。bare literal 与 `else` 会规范化为
true/false 语义 arm，若同一 entry 同时覆盖两值，则 subject 仍求值但 body 不复制。verified SSA 测试已
覆盖 subjectful `else`、subjectless 双 condition、动态 comparison、同 entry 多 condition、三条路径一致
消费同一 String owner、implicit unmatched synthetic branch index 及输入置换。该切片只接受 Boolean
expression condition；type-test/contains 与非 Boolean subject 仍保持各自门禁，MoveOnly control result
后来由 SPEC-0216 与第二十三步第四切片闭合。

第十切片增加独立 `unit_lower::type_plan`，补齐只在 reachable body 中出现、未进入 callable signature
的 scalar storage type。unit lowering 先按原顺序建立全部 reachable callable signature/function，再以第二遍
扫描每个 `UnitPlannedInstance` 自身 source/span 内的 typed expression facts，应用该实例的泛型替换，并只
追加 Boolean、8/16/32/64-bit 有/无符号整数与 String owner。这样保持既有 signature-first type identity，
也不通过全局注入未使用 Boolean 掩盖 planning 缺口；同文件 dead callable、Unit/Nothing、浮点/Char/Any
及复合类型不会进入本切片。verified SSA 测试锁定 literal-only subjectless `when` 的输入置换确定性、
body-only String local 的唯一 drop、generic `T` body fact 到具体 String instance 的替换，以及 dead
Boolean body 不污染 Int-only module type table；精确 type vector 同时锁定 signature-first identity。

第十一切片新增独立 `unit_lower::scalar`，接入现行整数 scalar 前缀、5 类 checked arithmetic 与
6 类 comparison。每个 checked instruction 同时产生数值结果和 Boolean failure flag；所在 block
立即以该 flag 建立 conditional，`true` edge 进入独立 `Abort` block，`false` edge 才继续使用数值结果，
因此溢出、除零和有符号除法边界保持 fail-closed。直接负整数字面量仍在 frontend 已定型范围内折叠，
包括有符号最小值，不生成运行时伪 overflow edge。type plan 只在 reachable instance 自身 body 的
整数 checked AST 上追加内部 Boolean failure type，保持 signature-first identity、dead-body 隔离和输入
置换确定性；String binary、逻辑短路、赋值与非整数运算仍在构造可发布 SSA 前返回显式未支持边界。
verified SSA 测试覆盖跨文件组合、一元 `!`、全部算术/比较映射，并逐条绑定 failure result、Conditional
condition 与 `true -> Abort` / `false -> continuation`，避免仅统计 Abort block 的假阳性。

第十二切片复用 compilation-unit carried-binding 与 result-first merge 基元接入 Boolean `&&` / `||`。
左操作数只求值一次并直接形成 conditional；`&&` 的 true edge、`||` 的 false edge 才进入 RHS block，
另一 edge 生成规范的 false/true 常量。RHS 与 short-value 出口各自携带进入表达式时的 binding slots，
随后把 Boolean result 放在 merge block parameter 首槽，再按 `UnitSymbolId` 顺序恢复 live bindings；因此
跨文件调用和 MoveOnly String owner 可同时穿过短路 CFG，输入置换仍产生相同 verified SSA。现行 unit
ownership traversal 尚未发布 short-circuit path-qualified owner state，而是顺序访问两个 operand；若 RHS
只在执行路径消费 MoveOnly owner，两出口 binding 不一致，unit lowerer 会在 merge 前返回 `MissingFact`，
不会把顺序 ownership facts 猜成路径事实。结构测试锁定 `&&`/`||` 的相反 RHS edge、单一 RHS call、
short 常量、共同 merge、result-first/carried slot 数和 String owner 不被提前 drop，并保留该 fail-loud 门禁。

第十三切片把 compilation-unit scalar dispatcher 扩展到 String `+` / `==` / `!=`。name/group operand
直接解析为当前 live owner value，其余 literal/call operand 先按正常 expression lowering 生成 temporary；
`StringConcat` / `StringEqual` 只读取这些 value views，不消费 owner。operation 完成后 lowerer 精确消费
source-qualified `UnitDropPoint::AfterBinaryOperands`，使 dead named 与 temporary operand 按 frontend 发布的
right-to-left 顺序析构；concat 新 owner 随后才由外层 expression category 登记，可安全转交局部、Value
参数或函数返回。`!=` 在 operand drops 之后对 `StringEqual` result 生成 `BooleanNot`。verified SSA 测试
锁定 literal concat 的 source-order operands、逆序 drops 与返回 owner identity，以及跨文件 suffix
temporary/local named drops、joined owner 传给 identity、两条 equality 的精确 RHS identity、严格
`StringEqual -> Drop(RHS) -> BooleanNot` 时序和输入置换。Borrow String 参数仍受 unit function ABI 的
既有显式门禁，不把本切片描述成完整 Borrow lowering。

第十四切片把 compilation-unit lowering 扩展到 root name assignment。普通 `=` 先完整 lower RHS，
消费 frontend 发布的旧 named owner drop，再按 expression category 转移 RHS temporary/place owner 并写回
binding；若 MoveOnly 旧 binding 未被精确 fact 移除则以 `MissingFact` 失败，不静默覆盖。整数
`+=` / `-=` / `*=` / `/=` / `%=` 复用 checked arithmetic 的 `true -> Abort` CFG，type planner 从
compound target 的 concrete type 补入 Boolean failure identity，而不是误用 assignment 自身的 deferred
`Unit` type；每次 success result 都成为下一次 assignment 的 binding。SPEC-0218 之后 validated 普通
`=` 已携带 descriptor，`Int = Boolean` 在 Phase 2 以 L0084 原子拒绝；这条既有 root-name lowerer 仍保留
等型内部断言，而新增 consumer 必须直接使用 descriptor。五种 compound 仍处于 frontend deferred/
guide 未封闭边界；结构测试只把现有 checked-arithmetic lowering 锁为实现回归，不把它反向声明成语言
语义。element/member target 和 Inout parameter 仍分别受 name-target 与 function ABI 既有门禁。

第十五切片把 compilation-unit type mapper 与 expression lowering 扩展到已具体化、non-null 的
intrinsic `Rc<T>` core。`type_lower` 递归建立有限 `Rc<具体类型>` 的 unit-global `SharedOwner`
identity，并在 direct-call first-class contract 中允许该 owner 作为 Value 参数和返回值；现行
source parser/frontend 尚不能为 generic callable 中的 `Rc<T>` 形成 validated unit artifact，会更早发布
诊断；即使未来前端接通该表面，`resolve_concrete_type` 当前也不实例化这类含类型参数的复合类型，仍以
`UnsupportedNode` 作为第二道 fail-loud 门禁。codegen 测试不伪造绕过 validated provenance 的 typed artifact。
construction lowering 逐字段核对 typed descriptor 与 SPEC-0198 ordered-delivery/root obligation，按
Copy/Move/temporary effect 复制或转移 payload，再生成 `SharedAllocate`；`.share()` 与 Copyable
`.value` 分别只在 source-qualified Rc effect 精确匹配时生成 `SharedRetain`、
`SharedPayloadPlace + Read`。外层 lowering 继续消费 frontend `AfterExpression` drop facts，因此 retained
handle 与原 owner 在各自最后一次 operation 完成后析构，函数/调用边界上的 Rc 和 String payload
temporary 则唯一转移而不重复 drop。结构测试覆盖输入置换、`Rc<Int>` retain/两次读取/drop 顺序和
`String -> Rc<String>` Move delivery；MoveOnly payload read、nullable Rc、temporary receiver、nominal
payload 与复合泛型实例化仍是显式后续边界。

第十六切片把 unit type mapper 扩展为共享的 concrete type/layout 规划器，接通无类型参数的
`value class`、普通 `class` 与 intrinsic `Box<value class>`。inline nominal 形成具名
`Aggregate`，class 与 Box 形成 unit-global `HeapOwner`；class payload 独立保存字段 layout，handle
先声明，再按 payload 依赖递归完成 pending definition；`Value -> class -> Value`、`Rc<Class>` 与
nested Rc 等有限 owner 图不会退化为 inline cycle，也不会把未定义 handle 交给 SharedOwner。
construction lowering 集中消费 typed descriptor 与 SPEC-0198 ordered-delivery/root obligation：
operand 按源码 `evaluation_index` 求值，
Copy/Move/temporary effect 精确核对后再按 parameter index 组装字段；class 先构造 payload 再
`HeapAllocate`，Box 直接交付单一 payload。显式 receiver 的 Copyable field projection 对 value class
使用 `AggregateProject`，对 class 使用 `HeapPayloadPlace + FieldPlace + Read`，外层 source-qualified
drop facts 继续负责 owner 的唯一析构。结构测试锁定反序具名参数、跨文件 value/class/Box 构造与
transfer、class field read 后 drop、返回 owner 不提前 drop、输入置换，以及 generic nominal 和
MoveOnly field read 在发布 program 前 fail loud。缺少精确 outer drop fact 的 MoveOnly temporary receiver
同样在生成 receiver SSA 前拒绝。enum case、结构化 component/隐式 `this`、generic nominal、MoveOnly
field 借用/读取、container 与 closure 仍由后续切片承接。

第十七切片把同一 unit type/layout 规划器扩展到无类型参数的 `enum class`：每个 case 按声明顺序
形成独立 payload `Aggregate`，root 形成单一 `TaggedUnion`，value-constructor symbol 与 type-test
symbol 都映射到同一稳定 `(variant, payload)` identity。case construction 继续复用 SPEC-0198 的
ordered Value delivery，先按源码顺序 lower operand、再按 payload 声明顺序构造 aggregate，最后生成
`TaggedConstruct`；零 payload case 使用空 aggregate，不引入名义特例。Copyable subject 的 enum
`when` 只消费已解析 case type symbol，通过 `TaggedDiscriminant` 与 case variant 比较进入既有 owner-aware carried
binding CFG；穷尽 value `when` 仍保留未匹配 `Abort` 防御块。smart-cast 后的 Copyable payload field
读取使用 `TaggedPayloadPlace + FieldPlace + Read`，MoveOnly enum root 由 source-qualified drop facts
在函数/调用边界精确转移或析构。结构测试锁定具名参数求值/布局顺序、跨文件输入置换、两个 case
discriminant、Copyable payload projection、MoveOnly root 唯一 drop、`enum -> class -> enum` 有限递归，
并让 generic enum 在发布 program 前 fail loud。MoveOnly payload 读取、generic enum、MoveOnly
enum subject、结构化解构、container 与 closure 仍由后续切片承接；其中 MoveOnly enum subject 的现行
drop fact 位于 subject `AfterExpression`，尚无 branch-qualified owner/drop facts，lowerer 在生成 subject
SSA 前拒绝而不猜测事实。现行 guide 语义未改变。

第十八切片把同一 unit type/layout 规划器扩展到 concrete `Array<T>`、`List<T>` 与
`MutableList<T>`：容器 identity 保留 kind 与递归 element type，列表式 `arrayOf`/`listOf`/
`mutableListOf` 和空 `MutableList<T>()` 直接生成既有 `ContainerConstruct`。lowering 对每个源码
实参核对唯一的 SPEC-0198 Value delivery fact；Copyable place 保留源 binding，MoveOnly place 与
temporary 精确转交给容器，nested container 和 String/class owner 不产生第二次 drop。`Unit` 元素在
保留原调用副作用后物化为 `ScalarConstant::Unit`，使合法 ZST element 仍有 verified SSA value。
结构测试锁定跨文件 owner transfer/drop、Copy/Move/temporary 三条路径、Unit
`DirectCall -> Constant -> ContainerConstruct`、空/嵌套构造和输入置换。runtime-length initializer
仍等待 callable bridge，element read/borrow/replace 仍等待 source-qualified operation lowering；不支持
element type 在 type planning 阶段拒绝，runtime-length 在任何实参 SSA 前拒绝。Unit constant 的 LLVM
materialization、multi-source DWARF 与 object/native 闭环仍由 SPEC-0199 后续切片承接；现行 guide
语义未改变。

第十九切片把 compilation-unit direct call 扩展到 shared-Borrow callable core。Borrow 参数在 SSA
function entry 表示为 function-scoped Shared Loan；owned root 与 temporary 严格消费唯一的
source-qualified `UnitLoanFact(call, argument)`，生成 `RootPlace + BorrowBegin`，DirectCall 后只对本次
创建的 loan 按逆序生成 `BorrowEnd`，随后消费 CallReturn drop。已有 Borrow 参数向下游调用时直接转发
同一 function-scoped loan，不产生嵌套 loan；Copyable Borrow name 通过 `PlaceAccess::Loan` 读取。
lowerer 保持源码实参求值顺序，并按 typed parameter index 组装具名实参槽位。结构测试锁定 root/
temporary、混合 Borrow/Borrow/Value、转发、读取、析构与输入置换；一般 Inout、非 root field/container/
Rc projection、MoveOnly Borrow name read 仍 fail loud。由于现行 callable SSA/LLVM ABI 不把 `Unit` 作为
一等参数类型，`Borrow(Unit)` 在 concrete substitution 后、SSA 类型与函数创建前确定性拒绝，等待后续
ABI 擦除方案。container element borrow、closure thunk、LLVM/multi-source DWARF 与 object/native
仍由 SPEC-0199 后续切片承接；现行 guide 语义未改变。

第二十切片把 compilation-unit lowering 扩展到 concrete 顺序容器 element core。Copyable element
read 生成 `ContainerElementPlace + Read`；shared-Borrow call 可从 owned root、已有 Borrow 参数或
temporary container 建立 element loan。grouped temporary 先按 frontend temporary origin 校验，再把
实际 owner 唯一重绑定到 source-qualified loan target，使 CallReturn 精确 drop 且不保留同 ValueId
别名。`Array`/`MutableList` simple 与整数 compound replacement 生成 `ContainerReplace`，MoveOnly RHS
先消费 Value delivery，旧元素只在精确
`AfterReplacement/ReplacedElement` fact 存在时由 replace intrinsic drop。checked arithmetic 的 CFG
只携带 live MoveOnly bindings，success block 使用重绑定 owner；普通 scalar CFG 因而保持原来的空边。
verifier 的 container index 契约与 source `Int` 对齐为 signed i32/i64。field-backed receiver、`size`、
temporary compound、一般 `Inout` 与 MoveOnly element read 仍 fail loud；LLVM/multi-source DWARF 与
object/native 继续由 SPEC-0199 后续切片承接，现行 guide 语义未改变。

第二十一切片把 compilation-unit lowering 扩展到 concrete owned move closure core。reachable lambda
以所属 function instance 与 source-qualified expression 形成 unit-global environment、closure 与 thunk
identity；environment 只接收 owned Copy/Move capture，thunk entry 使用 shared environment loan，并为
每个 capture 建立 `SharedFieldLoan` Borrow 视图。具名局部 closure 支持 owner transfer、重复
`CallableInvoke`，并严格配对 Named owner 与反序 `Captured` facts，在最后使用、BranchExit 或 LoopExit
只生成一次 recursive drop。closure provenance 随 `if`、`when`、短路和 loop 的 owner state 一起快照、
恢复与一致合流；loop 回边拒绝 provenance 变化，避免猜测逐迭代 capture lifetime。结构测试锁定跨文件
输入置换、String Move/Int Copy capture、thunk field loan、移动后重复调用、if 双路径消费、while LoopExit
drop 与 bare loop 全出口已消费后的 coarse stale fact 跳过，以及 borrowed/temporary/nested/参数化/非
Unit 等原子边界。temporary/direct closure delivery、
borrowed capture、一般 callable ABI、LLVM/multi-source DWARF 与 object/native 仍由
SPEC-0199 后续切片承接；现行 guide 语义未改变。

第二十二切片补齐无 capture lambda 的 unit lowering：普通 lambda 与 `move` lambda 都映射到
signature-deduplicated `FunctionPointer`，每个源码 lambda 创建独立的零参数 `Unit` thunk，并由
`FunctionAddress` 形成 callable value；该路径不创建 environment、`ConcreteClosure`、
`ClosureConstruct` 或 `SharedFieldLoan`。function pointer 保持 SPEC-0038 的 MoveOnly owner contract，
支持具名 binding transfer、重复 `CallableInvoke` 与唯一 SSA drop discharge；captured lambda 继续走
第二十一切片的 concrete closure/capture drop 路径。结构测试锁定跨文件输入置换、普通/move 两类
surface、canonical pointer type、独立 thunk、无隐藏 environment 及 captured 回归。参数化、非 `Unit`、
temporary/direct delivery、一般 callable ABI、LLVM/multi-source DWARF 与 object/native 仍由后续切片
承接；现行 guide 语义未改变。

第二十三步的第一切片接通 Copyable callable 参数/返回 ABI。`FunctionPointer` 与
`ConcreteClosure` signature 现在可以携带 Copyable storage 的 Borrow/Value 参数和 Unit/Copyable
storage 返回；closure thunk entry 固定为可选 environment shared loan 后接源码参数，lambda parameter
按 unit-global symbol 绑定为 Value 或 Shared Loan。function-value call 复用 direct call 的
source-qualified loan/Value delivery lowering，保持 callee 先求值、实参源码顺序求值、参数槽位重排、
新建 loan 逆序结束及 CallReturn drop。非 Unit lambda 以 body 最后一个 element 作为 tail value。
结构测试覆盖无 capture 的 Borrow+Value 混合参数、captured move closure 的 Borrow 参数、Copyable
返回、重复调用与输入置换。`Inout`、`Borrow(Unit)`、MoveOnly 参数/返回、temporary/direct callable
delivery、跳出实参的 `return`/`break`/`continue`，以及 function-value 实参内部 loop jump 仍原子
fail loud；这些边界分别等待 frontend 发布 exit-qualified pending-argument 清理事实与 callable callee
owner edge carry。direct call 实参内部 loop body 的 break/continue 继续正常 lower，condition/source 中
指向外层 loop 的 jump 不被误判为内部边界。LLVM/multi-source DWARF 与 object/native 仍由后续切片
承接；现行 guide 语义未改变。

第二十三步的第二切片接通事实完备的 MoveOnly callable result。lambda body 最后一个 element 必须是
与 callable 返回类型一致、可由现有 SSA storage 表示的唯一直接 temporary；thunk 把该 owner 直接交给
`Return`，caller 的 `CallableInvoke` 产生独立 result owner，并继续复用既有 binding/return/drop 流。
function pointer 与 captured concrete closure 共用该契约，environment-first thunk ABI 不变。lambda
span 内的其他 MoveOnly temporary、local owner、显式 return、concat/分支结果以及 MoveOnly 参数继续
在 program 发布前 fail loud，等待 lambda-body exit-qualified owner/drop facts。一般 MoveOnly
`if`/`when` result 和缺 typed iteration plan 的 `for` 也未解锁；LLVM/multi-source DWARF 与
object/native 仍由后续切片承接，现行 guide 语义未改变。

SPEC-0215 在 compilation-unit ownership 中为 lambda body 建立独立 callable 活性与析构规划。root
liveness traversal 不进入 lambda body，只保留 formation 的 capture source；随后 liveness 与 drop planner
统一按 AST identity 枚举全部 lambda body，因此顶层/member initializer 与嵌套 lambda 不依赖外层 item
是否遍历 initializer。无 MoveOnly Value 参数时，body prefix 沿用普通 statement 规则，最后一个
expression element 按隐式返回 `Consume`：结果 owner 转交 caller，内部 composite operand 与未转移的
body-local owner 按既有精确 point 析构。在该历史切片中，MoveOnly Value 参数仍保持门禁，
等待独立 lambda-entry drop point；该前端事实不自行放宽 SPEC-0199 的 codegen surface，现行
guide 语义未改变。

SPEC-0216 在 main ownership dataflow 与 unit drop planner 中统一 control result usage：MoveOnly
`if` / `when` 的每条正常 branch tail 总是 Consume 到 merged temporary，父 Read/Consume/Place 只作用于
该 result；Copyable tail 仍是 Read。`ControlBody` 与直接 expression branch 共用该规则，因此 named
tail 的 checker moved state 与 drop state 一致，Borrow 不再形成 named/result 双 owner。Consume tail
不产生 branch 内 `AfterExpression` drop；String operand 仍按 `AfterBinaryOperands` 逆序析构，未选中的
named alternative 按对应 `BranchExit` 清理，Read/CallReturn 只析构 merged result。`Nothing` call 不
形成正常 exit/drop。lambda body 不再需要回滚 MoveOnly control plan，facts 与输入顺序无关；该前端
事实不自行放宽 SSA/LLVM/native surface，现行 guide 语义未改变。

SPEC-0217 在上述独立 lambda callable liveness 上保留按 lambda expression identity 索引的
`live_in`，并新增 source-qualified `LambdaEntry`。drop planner 为 MoveOnly Value 参数建立与
body 同 frame 的 owner state：未使用参数在入口按逆声明顺序析构，Borrow 读取在既有
last-use / `CallReturn` 边界析构，Value delivery 及显式/隐式 return 消费参数而不重复 drop。
Borrow 与 Copyable Value 参数不进入 owner state，body-local/capture/control facts 与输入置换
确定性保持不变。该 Phase 3 事实已就绪，但不自行放宽 SPEC-0199 codegen surface。

SPEC-0199 第二十三步第三切片开始消费上述事实。callable thunk 不再按 AST 扫描并拒绝所有额外
MoveOnly temporary，而是沿既有 lowering 消费 lambda body 的精确 drop points；隐式 tail owner 按
source-qualified expression identity 转移给 `Return`，显式 return 复用 control-transfer 路径。thunk
退出前必须同时满足 temporary 集合为空、剩余 named binding 不含 concrete MoveOnly 类型，因此
String concat operand 与 body-local owner 精确析构且结果不 drop；该切片尚未消费 SPEC-0216 的
MoveOnly `if`/`when` result facts，故仍原子失败；在该历史切片中，MoveOnly Value 参数仍等待
lambda-entry drop point。
function pointer 与 captured concrete closure 共用该契约；上述参数缺口已由 SPEC-0217 在 frontend
发布事实，但 Phase 4 消费仍待后继切片。现行 guide 语义未改变。

SPEC-0199 第二十三步第四切片消费 SPEC-0216 的 MoveOnly control result facts。control result gate
按 concrete type 的现行 SSA storage 能力判断，同时保留无需 storage 的 `Nothing` 全 diverge 路径；
每条正常 branch 在形成 `BranchExit` 前按 tail expression
identity 转移 Place/Temporary owner，operand 与未选 alternative 仅消费 frontend 的精确 drop point。
多出口 merge 以 result、live value binding、live loan 的固定顺序建立 block parameters；单出口直接
沿用 branch state，全 diverge 不伪造结果。named callable、function pointer 与 captured concrete closure
共享该路径，nested `if` / `when`、`Nothing` 提前退出与输入置换均由同一组窄测锁定。为保持线性实体
显式，control edge 同时携带 environment `SharedFieldLoan` 与用户 Borrow loan；checked arithmetic 的
success/failure continuation 也复用该 value-prefix/loan-suffix block 约定，因此 branch 内 checked CFG
不会形成 hidden linear live-in。public native fixture 已覆盖 alias direct call、named/captured String
control result 与 checked arithmetic 的 object/link/run；该切片中 MoveOnly Value lambda 参数仍等待
独立 lambda-entry drop point。SPEC-0217 随后已发布该 frontend 事实，Phase 4 消费仍为
SPEC-0199 的待完成切片。现行 guide 语义未改变。

SPEC-0199 第二十三步第五切片消费 SPEC-0217 facts。callable plan 只对具有现行 SSA
storage 的 MoveOnly Value 参数放行，thunk 保持 environment-first、随后为源码参数的 ABI；
参数 binding 建立后立即消费 source-qualified `LambdaEntry`，之后由既有 `AfterExpression` /
`CallReturn` / Value delivery / control-transfer facts 唯一析构或转交 owner。MoveOnly 隐式返回的
Place 只在 name resolution 证明其是当前 lambda 的 MoveOnly Value 参数时放行，不泛化到任意
body-local Place。function pointer/captured closure 与 object/link/run 已锁定；MoveOnly Borrow、`Inout`
与无 storage 类型仍在 program 发布前 fail loud。现行 guide 语义未改变。

SPEC-0199 第二十四步的第一切片建立真实 compilation-unit frontend→SSA→LLVM/multi-source DWARF
集成证据。两个 package 的 source input 经过独立 name/type/ownership 分析后汇入单一 verified SSA/LLVM
module；alias direct call、captured closure thunk、environment-first + user Borrow pointer ABI、MoveOnly
String result/drop 均沿用现有生产 adapter。debug plan 为 provider/consumer 建立各自 `DIFile`，并把
callable、entry 与 thunk 的 `DISubprogram`/代表性 `DILocation` 绑定回正确 source；输入反序后的完整
LLVM 文本保持一致。该切片未新增公开 API，object 原子写入、link/run 和完整 native matrix
在该切片当时仍由 SPEC-0199 后续切片承接，现行 guide 语义未改变。

SPEC-0199 第二十五步的第一切片新增 public `emit_native_unit_object`。API 显式接收 source inputs、
validated compilation-unit names/types/ownership、`TypeEnvironment`、resolved `DeclarationId` 与输出路径；
统一 compatibility gate 先核对完整 analysis identity chain，再要求 entry 为非泛型零参数 `Unit` 顶层
callable。verified LLVM object 只写同目录、通过 `create_new` 原子抢占的 sibling temporary，成功后以
单次 rename 发布；backend 或 commit 失败由 RAII 清理 temporary，旧目标保持不变。首个 native matrix
已覆盖两个 package 的 alias call、captured closure、Borrow、动态 String result/drop、Mach-O link/run，
以及 InvalidEntry、UnsupportedSource、MismatchedAnalysis、commit failure 的原子负例。multi-file
aggregate/Rc/constructor 与提前退出的完整矩阵仍由 SPEC-0199 后续子切片承接；project CLI 不在本 API
边界内，现行 guide 语义未改变。

SPEC-0199 第二十五步的完成切片复用同一 public API 与单一真实可执行 fixture，补齐 current unit-native
owner/drop matrix。provider 的 named class、value class→Box、`Rc<Int>` 与动态 String construction 经过
跨文件 call 进入 consumer；consumer 同时持有 class/Box/Rc owner 后，提前 return 路径验证三个本地 owner
的析构，正常路径则验证跨文件 consuming Value delivery、field projection、Rc retain/payload read 与最终
唯一 drop。alias captured closure 与动态 String 返回继续在同一 Mach-O link/run 中执行，未为不同 owner
类别复制 object/link/run 流程。commit failure 仍从 public API 端到端验证错误传播、旧目标保持和 sibling
temporary 清理。由此第二十五步的单 object 原子写入、native 正反矩阵与 workspace 基线完成；project CLI
仍属于 SPEC-0054，现行 guide 语义未改变。

SPEC-0199 完成审计确认上述切片已形成单一封闭产物：validated compilation-unit identity
从显式 `DeclarationId` entry 经 unit-wide reachability/单态化、owner-aware verified SSA、单 LLVM
module 与 multi-source DWARF，到 sibling-temporary 原子提交的单 object/native executable。输入
置换锁定规范化 SSA/LLVM，真实 fixture 同时执行 exact import、alias import、MoveOnly 跨文件
传递、nominal/Box/Rc/String owner 及正常/提前退出 drop；InvalidEntry、UnsupportedSource、
MismatchedAnalysis 与 commit failure 均不覆盖旧目标。因此 SPEC-0199 已 `done`。完整 `for`
依赖的 typed provider、ownership cleanup 与 runtime primitive 已明确迁移到 v0.37 候选链
SPEC-0179/0211/0212/0182，不是 v0.32 本 Spec 的隐式未完成项。

## 结构化诊断与 renderer

`lang_frontend::diagnostic` 按
[ADR-0003](../adr/0003-diagnostic-architecture.md) 拥有可供后续前端阶段和 LSP 复用的诊断
语义模型：

- `DiagnosticCodeCatalog` 一次性校验精确 ASCII `Ldddd` 格式和重复编号；只有目录解析出的
  `DiagnosticCode` 才能进入诊断。生产目录 `codes::ALL` 现连续注册 `L0001`–`L0151`，覆盖
  Lexer、Parser、名称、类型和所有权错误；L0146–L0151 已由 SPEC-0025 compilation-unit
  index/name resolver 发出；`L0016` 为不再由生产 Parser 发出的历史类别，
  `L9xxx` 样例编号仍只在测试 target 内注册；
- `Diagnostic` 构造时必须接收严重级别、已验证错误码、非空单行主消息和主 `Span`；字段
  私有，主位置缺失不可表示。关联 label、note、help 同样受检，并在一个有序序列中保留
  生产者给出的语义顺序；
- frontend 聚合边界先用共享 `SourceMap` 校验所有主与关联 `Span`，再按主 source 名称、
  范围、严重级别、错误码、主消息和完整附加信息序列建立全序。排序不依赖 `SourceId`、
  source 加载顺序、随机哈希顺序或输入下标；失败返回包含角色与 `SourceError` 的内部错误；
- `lang-cli` 的 `diagnostic_renderer` 是 `kovenc` binary 内的私有纯转换：接收
  `Diagnostic + SourceMap`，返回确定性无颜色文本或 frontend 内部错误，不读取文件、不直接
  写 stdout / stderr。它复用 source 模块的 1-based 位置换算，并只转义 source 名称中的
  反斜杠、CR、LF 来保持单行输出，不做路径发现或规范化；
- `lang-cli` 的 `machine_diagnostic_renderer` 按
  [ADR-0014](../adr/0014-versioned-machine-diagnostics.md) 输出 schema/version、severity、code、
  message、primary 和有序 details。location 保留 source 原文、UTF-8 半开 byte range 与
  1-based scalar 位置；完整集合先验证和编码，foreign span 不产生部分 JSON Lines；
- `kovenc --message-format=json format ...` 才选择 machine renderer，记录继续写 stderr；默认
  human、格式化源码 stdout、`--check` 0/1 及 usage/I/O/internal error 保持原边界。颜色、完整
  build event stream 与机器化 operational error 尚未实现。

## 单文档 LSP 诊断与跳转定义

SPEC-0055 已把 `lang-lsp` 从空 binary 接通为标准 stdio LSP server，SPEC-0056 在同一单文档
边界增加标准 `textDocument/definition`。server 声明 UTF-16 position encoding、definition
provider 与 full-document open/change/close sync；每个打开 URI 保存版本和对应完整 `Analysis`，
状态使用按 URI 字符串排序的 `BTreeMap`，不读取磁盘、扫描 workspace 或解释 package/import。

- `analysis` 为每个文档版本新建 `SourceMap`，按 `lex → parse_file → resolve_names →
  check_types → check_ownership` 运行完整单文件流水线。`standard_environments()` 现在集中绑定
  全部标量 builtin、`Copyable`/`Transferable`、`Box`/`Rc`/顺序容器、三个列表式核心构造和
  标准 `error()` identity，LSP 不复制这些身份；
- Parser 诊断已经包含 Lexer 诊断，adapter 只再合并名称、类型和所有权集合，然后调用
  frontend `ordered_diagnostics` 建立全序。内部错误通过 `window/logMessage` 暴露，不伪造成
  `Ldddd` 用户错误；
- `diagnostic_adapter` 复用 `SourceMap::position` 的 line/CRLF/scalar 语义，只把该行已有 scalar
  column 转换为 LSP 要求的 UTF-16 code units。主 span 成为 range，错误码、严重度与 source
  进入标准字段，label 成为 related information，note/help 保持原顺序附在 message；
- `position_adapter` 集中维护 `Span ↔ UTF-16 position/range` 边界，反向 cursor 映射拒绝
  surrogate pair 中间位置并对越界返回无目标；`definition` 从 `NameReference` / `Symbol` 建立
  source-local 稳定索引，再用成功的 `CallDescriptor` / `AggregateProjectionDescriptor` 把
  overload/member/field 宽候选收敛到 typed 唯一源码目标；external/unresolved 不伪造声明；
- definition 只查询当前打开 buffer：声明自身、普通名称、类型、enum case、稍后局部和已知
  overload candidates 均返回同 URI location；full change 先完整分析与发布下一版本，再原子替换
  状态，close/unopened/outside 返回 JSON `null`；畸形 params 返回 invalid-params 且会话继续；
- open/change 发布对应 buffer version，close 发布无 version 的空集合。未知 request 返回
  JSON-RPC method-not-found，未知 notification 与 unopened-document change 不改变状态；
- `Connection::memory` 测试覆盖初始化、版本更新、清空、definition 生命周期、shutdown/exit、
  非法 full change/definition params 与 unknown message；纯 adapter/index 测试覆盖 surrogate
  pair、CRLF、EOF 空 span、Identifier 半开边界、诊断 detail 顺序和名称/typed target 收敛。
  该 LSP 消息不是 ADR-0014 的 CLI 机器协议；LSP 与 CLI adapter 分别保持 UTF-16/URI 与
  UTF-8 byte/scalar 位置契约，不互相序列化。

SPEC-0187 第一切片在 legacy 单文档状态之外接入 ADR-0021 的可选 `koven.sourceSet` version 1
初始化协议。`source_set` 严格校验 source-set 自身的 schema/version/字段、非空且唯一 root、
`(root, logicalPath)`/URI 一一映射、ADR-0005 逻辑路径和绝对 URI，并按稳定 source key 规范排序；
无关 initialization options 与缺席 source-set 继续进入 legacy 模式。URI 只解析为 presentation
identity，base text 完全来自初始化 payload，不读取磁盘。非法协议在 initialize 阶段返回 JSON-RPC
`InvalidParams`，不会进入文档生命周期；在该切片当时，base/overlay snapshot、跨文件诊断与
definition 仍由本 Spec 后续切片接入。

SPEC-0187 第二切片新增 source-set 专用 `UnitSession`。每次 initial/open/change/close 都从 immutable
base 与候选 overlay 集合重建一个共同 `SourceMap`、全部 `ParsedFile` 及 compilation-unit
name/type/ownership recovery 链；普通源码错误成为新 snapshot 的诊断，frontend 内部失败则记录日志并
保留 last-good overlay/version/snapshot。diagnostic adapter 按 primary `SourceId` 分组，related label
按自身 source URI 映射；全部 URI（包括空集合）在映射成功后按 `(root, logicalPath)` 顺序发布，打开
文档携带 overlay version，base-only/close 回落为 `None`。unknown/duplicate open、unopened/stale/
partial change 与 unknown close 只记录协议日志；候选 payload 全部发送成功后才提交状态。legacy 模式
继续使用既有每 URI `Analysis`；在该切片当时，source-set definition 暂返回 `null`。

SPEC-0187 第三切片从 snapshot 已保存的 `CompilationUnitNames.references()` 直接建立
`UnitDefinitionIndex`：`DeclarationId` 映射到 index 的精确声明 Span，`UnitSymbolId` 映射到对应
source-local symbol；可用的 `CompilationUnitTypes` call/projection facts 只负责把 overload/member
引用收敛到唯一静态 target，LSP 不解析 package/import 或可见性。exact import terminal/alias、wildcard
实际使用名、限定名与同 package 引用可跨 URI 跳转；wildcard `*`、纯 package segment、private/
unresolved 与 compiler-bound external 返回无目标。definition query 复用共同 `SourceMap` 的 UTF-16
position adapter，并按 target source identity 选择 URI；snapshot 更新成功时 definition facts 与诊断
一起切换，内部分析失败继续查询 last-good facts。legacy 单文档 definition 行为不变。

## 索引式 AST 存储

`lang_frontend::ast::AstFile<Item, Statement, Expression, TypeRef>` 拥有四张按插入顺序增长的
typed table，payload 类型由后续语法阶段或测试调用方提供：

- `ItemId`、`StatementId`、`ExpressionId`、`TypeRefId` 是字段私有且不能互换的下标
  newtype，只能由对应 table 分配；API 不提供裸下标构造、unchecked lookup、`Index`、删除、
  重排或可变节点访问，因此追加后已有 ID 保持有效；
- 每个 `AstNode<T>` 拥有 payload 与 `Span`。`AstFile` 持有唯一的 `SourceId`，四类插入 API
  都在修改 table 前检查 `span.source_id()` 一致；失败返回带类别、预期与实际 source 的
  `AstError::MismatchedSource`，不占用 ID；
- table 的 `get` 对越界 ID 返回 `AstError::InvalidNodeId`，`iter` 按确定的 ID / 插入顺序返回
  只读节点。该顺序是存储顺序，不等同于源码顺序或顶层语义顺序；
- ID 不携带 file / arena identity。同类 ID 在另一 AST file 中若恰好是有效下标，会读取目标
  file 的该节点；调用方必须维持 ID 所属 file 的内部不变量；
- `Debug` 使用 Vec 与 typed ID 的结构顺序，隐藏 SourceMap owner identity 并不展示泛型
  payload，因此不引入 payload 中可能存在的机器路径、地址或随机集合顺序。它只供调试
  和测试，不是序列化格式或跨构建稳定协议。

生产模块已用 `SyntaxAst = AstFile<Item, Statement, Expression, TypeRef>` 定义共享具体 AST，
并保留 `ExpressionAst` 兼容别名。`Statement` 封闭区分 Error、Block / LambdaBody、引用变量
Item 的 LocalVariable、内嵌 binding / initializer 的 LocalDestructuring 与引用表达式的
Expression；callable marker、函数类型参数与调用实参都是
四张 typed table 内节点的封闭内嵌 payload，没有新增第五张 table。尚无通用 visitor、HIR /
MIR 或 LLVM / codegen handle；名称解析结果由独立 `NameResolution` 表持有，不写回 AST。

## 语言 fixture harness

`crates/lang-frontend/tests/fixtures.rs` 是 Cargo 自动发现的 `fixtures` integration test target。
它分别运行十五个固定 suite：Phase 0 `source-pass/`，Phase 1 `lexer-pass/`、`lexer-fail/`、
`parser-expression-pass/`、`parser-expression-fail/`、`parser-declaration-pass/`、
`parser-declaration-fail/`、`parser-block-pass/`、`parser-block-fail/`、`parser-lambda-pass/`、
`parser-lambda-fail/`、`parser-implicit-unit-pass/`、`parser-implicit-unit-fail/`、
`parser-file-pass/` 与 `parser-file-fail/`。

- 发现器递归接受普通小写 `.ko` 文件；拒绝 symlink、未知扩展名、非 UTF-8 相对
  路径和非普通文件类型。路径逐 component 校验后用 `/` 连接，case 与发现问题均显式
  排序，不依赖文件系统枚举顺序；
- 空 suite 是 `NoFixtures` 配置错误。Phase 0 case 以严格 UTF-8 读取，以规范相对路径作为
  `SourceMap` 名称，创建并切片全文件 `Span`，再构造测试私有 AST expression 和一条使用
  `tests/support/fixture_codes.rs` 中 `L9000` 目录的结构化诊断；
- Lexer pass / fail 与 Parser fixture source helper 都在同一 `SourceMap` / `SourceId` 上调用两次
  生产 `lex`；两次均验证 source identity、唯一 EOF、全部非 EOF lexeme 对输入字节的连续完整
  覆盖，以及 diagnostic primary / label Span 的 source-local 有界性，并比较完整公开产物确定性。
  三个 checked-in Lexer fixture 与 34 个 checked-in Parser fixture 共验证 74 个 Lexer 产物；
  临时 fixture harness 自检通过相同 helper 自动继承该约束；
- Lexer fail case 按规范相对 stem 将 `.ko` 与 `.diag` 一一配对；sidecar 每行严格使用
  `Ldddd<TAB>start_byte<TAB>end_byte`，只接受已注册生产码、LF / CRLF、十进制非空半开
  UTF-8 字节范围，并与生产诊断全序逐项全等。缺失 / 孤立 sidecar 和非法格式都使 suite
  失败；该 sidecar 是仓库私有测试格式，不是公共诊断协议；
- 34 个 checked-in Parser fixture 按 expression 4、declaration 5、block 4、lambda 4、
  implicit-unit 7、file 10 分组；每例在首个确定 Lexer 产物上执行两次对应公开 Parser，比较完整
  `Debug` 后把首个确定产物交给既有领域断言，共验证 68 个 Parser 产物。内部入口错误与 Lexer /
  Parser 重复产物漂移使用不同的 fixture failure variant；
- Parser pass case 以生产 Lexer 与独立表达式入口验证零诊断、根有效及完整消费到 EOF；Parser fail
  case 复用同一 sidecar 契约，逐项核对合并后的 Lexer / Parser 诊断全序；现有 expression
  suite 已加入命名 / `borrow` / `&` 实参与 `L0033` 缺值证据；
- `parser_expression` 的 54 个核心 integration test 保持既有源码、AST payload、精确 Span、
  diagnostic、source-order 与递归预算断言；全部正常用户源码路径在同一 source identity 上执行
  两次 Lexer 与两次 expression Parser，逐次验证 lexeme 完整覆盖、唯一 EOF、source-local AST /
  diagnostic Span、typed root 和完整公开产物确定性。仅故意混用 `SourceMap` 的 identity 错误与
  六个预期 `NestingLimitExceeded` 的资源错误 case 直接调用入口；本轮未发现生产缺陷；
- `parser_declaration` 的 22 个核心 integration test 保持既有声明 corpus、AST payload、精确
  Span、diagnostic、owner recovery 与递归预算断言；全部正常用户源码路径执行两次 Lexer 与
  两次 declaration Parser，并逐次验证相同公开产物不变量。仅故意混用 `SourceMap` 的 identity
  错误与一个预期 `NestingLimitExceeded` 的资源错误 case 直接调用入口；本轮未发现生产缺陷；
- `parser_block` 的 23 个核心 integration test 保持既有 block / statement corpus、typed child、
  精确 Span、diagnostic、owner recovery 与递归预算断言；全部正常用户源码路径执行两次 Lexer
  与两次 block Parser，并逐次验证相同公开产物不变量。仅故意混用 `SourceMap` 的 identity 错误
  与一个预期 `NestingLimitExceeded` 的资源错误 case 直接调用入口；本轮未发现生产缺陷；
- `parser_lambda` 的 16 个核心 integration test 保持既有 header / body / postfix / interpolation、
  三入口上下文、精确 Span、diagnostic、owner recovery、小调用栈与递归预算断言；expression、
  declaration、block 的全部正常用户源码路径均执行两次 Lexer 与两次对应 Parser，并逐次验证
  相同公开产物不变量。仅跨 `SourceMap` identity 与一个预期 `NestingLimitExceeded` 的 case 直接
  调用 expression 入口；本轮未发现生产缺陷；
- `parser_call_argument` 的 28 个核心 integration test 保持 argument / parameter mode、typed-call
  trial、function type、精确 Span、diagnostic、owner recovery 与 typed-ID 断言；expression 与
  declaration 的全部源码路径均执行两次 Lexer 与两次对应 Parser，并逐次验证相同公开产物
  不变量。该 suite 无预期内部错误路径，测试文件不再直接调用 Lexer / Parser；本轮未发现生产缺陷；
- `parser_local_destructuring` 的 14 个核心 integration test 保持 binding marker、initializer、
  block / lambda / declaration 上下文、精确 Span、diagnostic、owner recovery 与长列表复杂度断言；
  三种入口的全部源码路径均执行两次 Lexer 与两次对应 Parser，并逐次验证相同公开产物不变量。
  该 suite 无预期内部错误路径，测试文件不再直接调用 Lexer / Parser；本轮未发现生产缺陷；
- `parser_control_flow` 的 7 个核心 integration test 保持 value-context `if`、control body、`when`、
  loop、jump、`super` postfix、L0055–L0065 与精确结构 / diagnostic 断言；expression 与 block 的
  全部源码路径均执行两次 Lexer 与两次对应 Parser，并逐次验证相同公开产物不变量。该 suite
  无预期内部错误路径，测试文件不再直接调用 Lexer / Parser；本轮未发现生产缺陷；
- `parser_error_propagation` 的 8 个核心 integration test 保持 postfix 左结合、safe call / Elvis /
  nullable type 消歧、callable body、L0009 恢复、source identity 与长链复杂度断言；expression、
  block 与完整文件的全部源码路径均执行两次 Lexer 与两次对应 Parser，并逐次验证相同公开产物
  不变量。完整文件 typed wrapper 与既有 file matrix wrapper 共用单职责 file output 校验；该
  suite 无预期内部错误路径，测试文件不再直接调用 Lexer / Parser；本轮未发现生产缺陷；
- `parser_class_family` 的 12 个核心 integration test 保持五种 classifier、generic / constructor /
  supertype / member、enum variant、companion、匿名形式拒绝、L0066–L0077、owner recovery、source
  identity 与完整 guide 示例断言；declaration、完整文件与 expression 的全部源码路径均执行两次
  Lexer 与两次对应 Parser，并逐次验证相同公开产物不变量。该 suite 无预期内部错误路径，测试
  文件不再直接调用 Lexer / Parser；本轮未发现生产缺陷；
- `parser_interface_delegation` 的 5 个核心 integration test 保持混合 supertype 顺序、委托 clause /
  target Span、L0077 / L0078、owner boundary、Phase 2 延迟检查与 `by` identifier 词法断言；全部
  源码路径均执行两次 Lexer 与两次 declaration Parser，并逐次验证相同公开产物不变量。typed
  helper 可同时返回首个已验证 LexedFile 与 declaration 产物，供词法见证继续检查；该 suite 无
  预期内部错误路径，测试文件不再直接调用 Lexer / Parser；本轮未发现生产缺陷；
- `parser_implicit_unit` 的 7 个核心 integration test 保持 implicit absent / block、显式返回类型、
  expression body recovery、L0013 / L0014 / L0021、Lexer 根因抑制、nested owner、UTF-8 byte Span
  与 source-order 断言；全部正常源码路径均执行两次 Lexer 与两次 declaration Parser，并逐次验证
  相同公开产物不变量。仅故意跨 `SourceMap` 的 identity 错误直接调用 Parser 并继续精确返回
  `ParserInternalError::Source`；本轮未发现生产缺陷；
- `parser_file` 的 27 个核心 integration test 保持空文件、package / import、alias / wildcard、声明
  分隔、未知区域、Lexer poison、nested owner、跨声明恢复、source identity、standalone declaration
  与 L0001、L0010、L0013、L0017、L0020、L0033、L0043、L0047–L0054 断言；file / declaration
  的全部源码路径均执行两次 Lexer 与两次对应 Parser，并逐次验证 root、header、AST、diagnostic 与
  完整公开产物不变量。512 roots 与 256 imports 长序列保持通过，测试文件不再直接调用 Lexer /
  Parser；本轮未发现生产缺陷；
- Parser 的 12 个 engine、6 个 lambda-header trial 与 3 个 strict-call trial 私有算法测试实际执行
  68 条 Lexer 输入路径：engine 43、lambda-header 8、strict-call trial 17。它们统一使用仅在
  `cfg(test)` 编译的 Lexer typed helper，每条源码运行两次生产 Lexer，共验证 136 个产物的
  source identity、连续 byte 覆盖、唯一 EOF、diagnostic primary / label Span 与全部私有字段
  确定性；首个已验证产物继续供既有 owner recovery、dispatch、缓存、递归预算与线性复杂度
  断言消费，三个 Parser 私有测试模块不再直接调用生产 `lex`，本轮未发现生产缺陷；
- Lexer 核心 suite 的 foreign `SourceId` 内部边界连续执行两次生产入口并精确返回相同
  `InvalidSourceId`；Parser expression、declaration、block、lambda 与 implicit-Unit suites 的
  14 条 foreign identity / recursion-budget 路径先验证 28 个正常 Lexer 产物，再通过 typed
  helper 验证 28 个 Parser 错误结果。5 条 foreign identity 路径均保留准确 owner `SourceId`，
  9 条 prefix / assignment / elvis / group / generic / function / declaration / block / lambda 递归
  形状均精确返回 limit 1024；失败路径由宽泛单次 variant 匹配收紧为精确双运行确定性，本轮
  未发现生产缺陷；
- Parser 私有测试可在 `cfg(test)` 内从源码 `a b` 的双 Lexer 正常产物派生 empty stream、missing
  EOF、EOF before tokens、duplicate EOF、empty non-EOF、discontinuous span、early EOF 与
  foreign span 八类非法 `LexedFile`，而生产 API 仍不公开其构造器或字段。expression、declaration、
  block、file 四个 engine 入口对每类重复拒绝，共验证 64 个精确错误；strict-call 与
  lambda-header 预索引器另验证 32 个精确 `InvalidLexemeStream`。engine 对七类本地结构错误返回
  `InvalidLexemeStream`，对 foreign span 保留统一 `SourceMap::slice` 的准确 `InvalidSourceId`；
  本轮未发现生产缺陷；
- 同一 test-only 边界还可从结构有效的 `a b` 产物派生 unmatched StringEnd、unmatched
  InterpolationEnd、dangling StringStart、dangling InterpolationStart 及两种错配 closer 共六类
  不可能的 lexical-owner token 流；每类均先通过 engine 通用 Lexeme 结构校验，再由
  `LexicalRecoveryIndex` 重复拒绝 12 次，并由 expression、declaration、block、file 四个 engine
  入口重复拒绝 48 次，全部精确返回 `InvalidLexemeStream`。这把流结构与 lexical-owner 语义
  两层内部防线的负向证据分离，本轮未发现生产缺陷；
- test-only recovery diagnostic corpus 还从四份独立双 Lexer 产物保留原始 L0004 / L0005 / L0006
  与精确 Span，同时移除对应 StringStart、InterpolationStart 或成对 string owner token。四类产物
  均保持 source identity、连续 Span、唯一 EOF 并通过通用 Lexeme 结构校验；
  `LexicalRecoveryIndex` 重复拒绝 8 次，expression、declaration、block、file 四个 engine 入口
  重复拒绝 32 次，全部精确返回 `InvalidLexemeStream`。这为 diagnostic 与 token owner 的生产关联
  增加独立负向证据，本轮未发现生产缺陷；
- `LexicalRecoveryIndex` 还统一验证 L0001–L0008 的生产 lexeme anchor：L0001 / L0003 / L0006 /
  L0007 / L0008 必须与同 Span、同 `InvalidKind` 的 lexeme 对应，L0002 必须与同 Span 的
  ReservedWord token 对应，L0004 / L0005 必须锚定诊断起点处的 StringStart / InterpolationStart。
  anchor 查找复用已经过结构校验的 lexeme 起点顺序做二分定位，复杂度为 O(D log L)；
  test-only corpus 从七份独立双 Lexer 产物移除 unexpected character、reserved word、unterminated
  block comment、非终止 / 终止 invalid escape、invalid char、invalid number 的精确 anchor，同时
  保留诊断与通用流结构；recovery index 与四个 engine 入口各双运行，共精确拒绝 70 次。该防线
  修复了 Parser 先前可能接受 diagnostic/lexeme 不一致内部产物的缺口，不改变合法 Lexer 产物；
- Lexer diagnostic 流还在相同分类点强制 source identity 等于 `LexedFile::source_id`，并把 code
  domain 精确限定为 L0001–L0008。test-only corpus 从两份独立双 Lexer 产物派生 foreign L0004
  primary Span 与 source-local L0009 注入；两类输入保持 lexeme 结构有效，由 recovery index 与
  expression、declaration、block、file 四个入口各双运行，共精确拒绝 20 次。该防线消除了 foreign
  owner diagnostic 延迟为 `SourceError` 以及非 Lexer code 被静默合并的内部缺口；
- diagnostic anchor 校验还返回 lexeme index 并在 O(L) 位图中记录覆盖，随后单次扫描要求五种
  `InvalidKind` 与 `ReservedWord` poison 均有对应生产 diagnostic。test-only corpus 从六份独立
  双 Lexer 产物精确移除 L0001 / L0002 / L0003 / L0006 / L0007 / L0008，同时保留 poison 与
  lexeme 结构；recovery index 与四个 engine 入口各双运行，共精确拒绝 60 次。完整双向校验保持
  O(D log L + L)，修复了未诊断 poison 可能形成静默 Error AST 的内部缺口；
- 覆盖位图还在写入前拒绝已占用的 anchor index，使 diagnostic/lexeme 关联满足 exactly-once。
  test-only corpus 分别复制一份 L0001 poison diagnostic 与 L0004 owner diagnostic，两类产物均
  保留 source identity、lexeme 结构及两个完全相同的生产诊断；recovery index 与四个 engine 入口
  各双运行，共精确拒绝 20 次。该唯一性检查不增加遍历、分配或渐近复杂度；
- declaration suite 以相同约束实际调用独立声明入口；Parser sidecar 允许 Parser 的空范围
  诊断，但 `L0001`–`L0008` Lexer 码即使在合并 sidecar 中仍必须使用非空范围；现有 suite
  已加入具名函数 / 函数类型 marker 与 `L0039` 重复 marker 证据；
- block suite 调用生产 block 入口，pass case 遍历 Statement / Item / Expression typed child，
  fail case 逐项核对 Lexer / Parser 合并诊断；已加入局部解构 pass 与 `L0044` trailing
  comma fail，两套 suite 均有非零用例与配对守卫；
- lambda suite 调用生产 expression 入口，pass case 验证 Lambda 至 LambdaBody 的 typed child，
  fail case 精确核对合并诊断；已加入 lambda body 解构 pass 与 `L0046` 缺 initializer
  fail，并保留 `L0031` / `L0032` 证据；两套 suite 同样执行非零与配对守卫；
- implicit-unit suite 调用生产声明入口；五个 pass fixture 覆盖隐式无体、空 / 非空 block、
  显式 `Unit` 与显式其他类型，两个 fail fixture 分别锁定省略标注的表达式体 `L0021` 和真实
  colon 后缺 TypeRef 的 `L0014`。runner 同时检查 `FunctionForm` 来源、Error / 真实 TypeRef、
  非零用例、sidecar 配对和空范围诊断策略；
- file suite 调用生产完整文件入口；四个 pass fixture 覆盖 package / import 文件头、
  control-flow、postfix `?` 与 class-family，六个 fail fixture 覆盖 L0017 跨声明恢复、
  L0047 同行缺分号、L0052 声明后 import、value-context `if` 的 L0057、postfix 缺 operand
  与缺委托目标 L0078，均由
  非零 / sidecar 配对守卫实际执行；
- `frontend_adversarial` integration test 对 18 个词法/语法前缀与 18 个后缀执行 324-case
  笛卡尔积，另含终止字符位于 interpolation 的定向 `L0007` lexical-owner 回归。325 个源码
  各执行两次 Lexer 与两次完整文件 Parser，共验证 650 个 Lexer 和 650 个 Parser 产物；逐次
  锁定 lexeme 完整字节覆盖、唯一末尾 EOF、source identity、diagnostic primary / label、四张
  AST table、typed roots 与 package / import 子 Span，并比较完整公开 `Debug` 产物以证明确定性；
- `lexer` 的 19 个核心 integration test 保持 hard / soft / reserved word、ASCII identifier、trivia、
  comment、numeric、char / string / interpolation、fixed symbol、unsupported operator、`&` / `&&`、
  L0001–L0008、恢复形状、diagnostic 顺序、UTF-8 对抗 corpus、EOF / byte coverage 与 source-load
  order 断言；全部正常 source 均执行两次 Lexer，并逐次验证 source identity、完整字节覆盖、唯一
  EOF、diagnostic Span 与完整公开产物确定性。仅 foreign `SourceId` 内部错误直接调用 Lexer 并
  继续精确返回 `InvalidSourceId`；本轮未发现生产缺陷；
- `lexer_boundary_matrix` integration test 经生产 Lexer 执行 2,127 个固定源码：330 个硬/
  未来保留/软词 ASCII identifier 边界类别、1,596 个普通固定符号相邻 spelling、199 个 `as?` /
  `!in` / `!is` continuation 与终止边界，以及 2 个注释优先级 case。每个源码执行两次 Lexer，
  共验证 4,254 个产物的 source identity、连续完整字节覆盖、末尾唯一 EOF、diagnostic primary /
  label Span 和完整公开产物确定性；全部源码保持零诊断，并继续锁定目标分类或最长首 token、
  精确 Span，且不复制 scanner 的匹配顺序；双 Lexer helper 同时由完整文件对抗矩阵复用；
- `lexer_stress_matrix` integration test 通过公开 Lexer 入口执行 21 个大输入 / 深模式源码并各
  运行两次，共验证 42 个完整产物：6 类约 65,536-byte 最大化 identifier、number、whitespace、
  line / block comment 与多字节 string text 保持既有单段或三段 token 形态；7 类超长错误源码
  精确覆盖 L0003 unterminated comment、L0004 string、L0005 interpolation、terminal / interior
  L0006 escape、L0007 char 与 L0008 number，锁定长 payload、单字节 / 两字节 escape、错误后
  StringText / StringEnd 恢复及唯一诊断 Span。4,096 层合法 string/interpolation mode 精确形成
  16,386 个 lexeme，单一 interpolation 内 16,384 层 brace 精确形成 32,774 个 lexeme；4,096 层
  未终止 mode 只报告最内层一个 L0005，4,096 个连续多字节非法 scalar 精确形成严格递增的
  L0001 / `Invalid` 对。全部产物保持连续完整覆盖、唯一 EOF、source-local byte Span 与完整公开
  产物确定性。相同的合法 mode、deep brace、未终止 mode 与多字节诊断流形状另从显式 64 KiB
  调用线程各执行两次生产 Lexer，精确保留既有 lexeme / diagnostic 数量，证明迭代式 `Vec<Mode>`
  状态管理不把源码深度转化为调用栈深度；全部检查没有 wall-clock 阈值或生产测试钩子，本轮
  未发现生产缺陷；
- `parser_entry_adversarial` integration test 对 16 个前缀与 16 个后缀分别运行独立 expression、
  declaration、block 三个公开入口，共执行 768 个 entry/case；每例运行两次 Lexer 与两次
  Parser，共验证 1,536 个 Lexer 和 1,536 个 Parser 产物。它们显式锁定 lexeme 连续完整覆盖、
  唯一末尾 EOF、source-local 有界 Lexer / AST / diagnostic Span 与可解析 typed root，并分别
  比较两次完整公开 `Debug` 产物以证明 lexeme、AST table 插入顺序、Span 和诊断确定性；矩阵
  不引入随机、IO 或第三方 property-testing 依赖，payload 边仍由精确领域测试验收；
- `parser_stress_matrix` integration test 通过 8 个确定性大平坦源码覆盖 expression、declaration、
  block 与 file 四个公开入口，每例运行两次 Lexer 与两次对应 Parser，共验证 16 个 Lexer 和
  16 个 Parser 产物。四个合法源码分别保留 4,096 个 call argument、value parameter、block
  element 与 file root；四个错误源码分别精确产生 4,096 个 L0015 / L0024 / L0029 / L0017，
  primary Span 严格递增。declaration 保留尾参数，block 保留 4,096 个 `Statement::Error`，file
  保留 4,096 组 `Item::Error` 与后续 `val` sentinel；全部产物保持连续覆盖、唯一 EOF、
  source-local AST / diagnostic Span、有效 typed root 与完整公开产物确定性。file 错误区使用
  真实后续声明 starter 同步，普通换行不被误作 file recovery boundary；本轮未发现生产缺陷；
- `parser_owner_stress_matrix` integration test 通过 12 个 owner-rich 大源码覆盖 expression、
  declaration、block 与 file 四个公开入口，每例运行两次 Lexer 与两次对应 Parser，共验证
  24 个 Lexer 和 24 个 Parser 产物。四个合法源码合计保留 16,384 个单 interpolation string，
  inner expression 均为 Name 且零诊断；四个恢复源码合计保留 16,384 个相同 String 与 inner
  Error，并精确产生 16,384 条源码严格递增的 L0009；四个 lexical-poison 源码另保留 16,384
  个 Text / Error / Text string 与严格递增的 Lexer L0006，不产生 Parser 级联。call argument、
  block local element 与 file variable root 均各自保留 4,096 项；全部产物保持连续覆盖、唯一
  EOF、source-local AST / diagnostic Span、有效 typed root 与完整公开产物确定性，本轮未发现
  生产缺陷；
- `parser_standalone_poison_stress_matrix` integration test 将 `#`、`async`、`'ab'`、`1e3`
  四类 standalone poison 分别以 4,096 项平坦流投放到 expression、declaration、block 与 file
  四个公开入口，共执行 16 个大源码、两次 Lexer 与两次对应 Parser，验证 32 个 Lexer 和 32 个
  Parser 产物。每个源码精确保留 4,096 条同类且 primary Span 严格递增的 L0001 / L0002 /
  L0007 / L0008，以及 4,096 个 byte-accurate `Expression::Error`；合计验证 65,536 条诊断和
  65,536 个 Error 节点，无 Parser 级联。call argument、block local element 与 file variable root
  均保持 4,096 项，全部产物保持连续覆盖、唯一 EOF、source-local AST / diagnostic Span、有效
  typed root 与完整公开产物确定性，本轮未发现生产缺陷；
- `parser_long_lexical_error_bridge` integration test 将约 65 KiB 的 unterminated block comment、
  string、interpolation、terminal escape、interior invalid escape、closed invalid char 与 invalid
  number 分别投放到 expression、declaration、block 与 file 四个公开入口，共执行 28 个源码、
  两次 Lexer 与两次对应 Parser，验证 56 个 Lexer 和 56 个 Parser 产物。每个入口均只保留一个
  L0003–L0008 Lexer 根因及 wrapper 偏移后的精确长 Span，不产生 Parser 级联；comment / char /
  number 保留覆盖完整 payload 的 Error expression，四种 string owner 保留完整 String expression，
  invalid escape 继续形成唯一 Error part。声明 initializer、单一 block local 与单一 file root 均可
  通过 typed ID 解引用；terminal block 依赖既有 EOF ownership，不伪造右花括号。本轮未发现生产
  缺陷；
- `parser_mixed_long_lexical_error_stream` integration test 以约 256 KiB 的异构源码把长 closed
  string 内 L0006、长 closed L0007 char、长 L0008 number 与四种长 EOF terminal owner 依次
  组成 call argument，再分别投放到 expression、declaration、block 与 file 四个公开入口。16 个
  源码各执行两次 Lexer 与两次对应 Parser，共验证 32 个 Lexer 和 32 个 Parser 产物；每例保持
  四条源码有序、wrapper-offset 精确的 Lexer 根因，以及唯一一条 EOF 空 Span 的 L0010 和指向
  真实 call `(` 的 opener label。这锁定 terminal owner 只拥有自身 closer、未闭合 call 仍是独立
  语法错误、外层 block closer 不再级联的边界。四个 argument 及 Error / String / Error part 形态、
  变量 initializer、单一 block local 和单一 file root 均可通过 typed ID 解引用，全部公开产物保持
  确定性；本轮未发现生产缺陷；
- `parser_mixed_long_recoverable_error_stream` integration test 以同一约 320 KiB 闭合 call 串联
  长 closed invalid-escape string、长 newline-terminated string、长 newline-terminal escape、长
  invalid char、长 invalid number 与合法 `sentinel` 参数，再分别投放到 expression、declaration、
  block 与 file 四个公开入口。4 个源码各执行两次 Lexer 与两次对应 Parser，共验证 8 个 Lexer
  和 8 个 Parser 产物；每例精确保留源码有序的 L0006 / L0004 / L0006 / L0007 / L0008，两个
  newline owner 的 String / CallArgument Span 停在换行前，且无 Parser 诊断。六个 argument、三个
  String、两个 Error、两个 invalid-escape Error part、真实 `)` 和尾部 Name 均保持 typed 可解引用；
  block / file 还分别保留第二个 `val after = 0` local / root，证明恢复越过参数列表并返回外层
  code mode。全部公开产物保持确定性；本轮未发现生产缺陷；
- `parser_long_utf8_line_recovery` integration test 以 21,845 个 `界` 组成 65,535-byte StringText，
  把 newline-terminated L0004 string 与 newline-terminal L0006 escape 分别放在 LF / CRLF 前，
  再经 expression、declaration、block 与 file 四个公开入口执行 16 个源码。每例运行两次 Lexer
  与两次对应 Parser，共验证 32 个 Lexer 和 32 个 Parser 产物；唯一诊断、StringText、String、
  首个 CallArgument 均按 UTF-8 byte offset 精确停在 CR / LF 前或反斜杠后，CRLF 不被误算为
  单字节源码范围。Call 继续保留 `sentinel` Name、真实 `)` 与完整 Span，block / file 还分别保留
  第二个 `val after = 0` local / root；全部 AST / diagnostic Span 可安全切片且产物确定，无 Parser
  诊断。本轮未发现生产缺陷；
- `parser_long_utf8_nested_line_recovery` integration test 把同样由 21,845 个 `界` 组成的
  65,535-byte StringText 放入 outer string interpolation 的 inner call；newline-terminated
  L0004 inner string 与 newline-terminal L0006 inner escape 分别跨越 LF / CRLF，再经 expression、
  declaration、block 与 file 四个公开入口执行 16 个源码。每例运行两次 Lexer 与两次对应
  Parser，共验证 32 个 Lexer 和 32 个 Parser 产物；唯一 Lexer 诊断及 inner StringText / String /
  CallArgument 均保持精确 UTF-8 byte Span，换行后继续保留 `inner_sentinel`、inner call closer、
  interpolation closer、outer tail / string closer、`outer_sentinel` 与 outer call closer。block / file
  还分别保留第二个 `val after = 0` local / root，证明模式栈依次返回 interpolation、outer string
  与最外层 code mode；全部公开产物确定且无 Parser 级联。本轮未发现生产缺陷；
- `parser_long_utf8_char_line_recovery` integration test 把由 21,845 个 `界` 组成的 65,535-byte
  payload 放入 outer string interpolation 的 invalid Char；payload 后直接遇到 LF / CRLF 或先遇到
  反斜杠再遇到 LF / CRLF 的四类 carrier，经 expression、declaration、block 与 file 四个公开入口
  执行 16 个源码。每例运行两次 Lexer 与两次对应 Parser，共验证 32 个 Lexer 和 32 个 Parser
  产物；唯一 L0007、inner CallArgument 与 `Expression::Error` 共享从单引号到换行前的精确
  UTF-8 byte Span，CR / LF 均未被 invalid Char 消费。换行后继续保留 `inner_sentinel`、inner call /
  interpolation / outer string closer、outer tail、`outer_sentinel` 与 outer call closer；block / file
  还分别保留第二个 `val after = 0` local / root。全部公开产物确定且无 Parser 级联，本轮未发现
  生产缺陷；
- `parser_long_invalid_number_boundaries` integration test 分别构造约 65 KiB 的长整数指数尾、长
  浮点指数尾、合法 `uL` 后非法 identifier tail 与非法 `0x` radix tail，并把每个 L0008 候选放入
  outer string interpolation 的 inner call 首个 `invalid + rhs` argument；四类候选经 expression、
  declaration、block 与 file 四个公开入口执行 16 个源码。每例运行两次 Lexer 与两次对应 Parser，
  共验证 32 个 Lexer 和 32 个 Parser 产物；唯一 L0008 与左侧 `Expression::Error` 精确覆盖完整
  maximal ASCII region，并在真实 `+` operator Span 前停止。Parser 保留 `Error + rhs` Binary、
  `inner_sentinel`、两层 call、interpolation、outer string / tail、`outer_sentinel` 与所有真实 closer；
  block / file 还分别保留第二个 `val after = 0` local / root。全部公开产物确定且无 Parser 级联，
  本轮未发现生产缺陷；
- `parser_long_block_comment_line_breaks` integration test 构造含 nested-looking `/*`、string /
  interpolation / line-comment-like marker、21,845 个 `界`（65,535 bytes）及尾部 LF / CRLF 的两类
  comment。每类分别进入 expression、declaration、block 与 file 四个公开入口，共执行 8 个源码；
  每源码先双运行独立 Lexer，再由 Parser helper 双运行 Lexer / Parser，合计验证 32 个 Lexer 与
  16 个 Parser 产物。Lexer 均只产生一个覆盖完整源码 comment 的 BlockComment trivia，按非嵌套
  规则由唯一首个 `*/` 关闭，正文 marker 不泄漏且 UTF-8 payload / 换行子范围可精确切片。
  expression / declaration 跨 comment 内逻辑换行保留 `left + right` Binary；block / file 在 comment
  外没有换行或分号时，仍仅凭 comment 内 LF / CRLF 保留 `val first = 0` 与 `val after = 1` 两个声明。
  全部公开产物确定且零诊断，本轮未发现生产缺陷；
- `parser_long_line_comment_boundaries` integration test 构造含 block / string / interpolation-like
  marker 与 21,845 个 `界`（65,535 bytes）的 line comment，并分别紧邻 LF / CRLF。每类 carrier
  进入 expression、declaration、block 与 file 四个公开入口，共执行 8 个源码；每源码先双运行
  独立 Lexer，再由 Parser helper 双运行 Lexer / Parser，合计验证 32 个 Lexer 与 16 个 Parser 产物。
  Lexer 均产生一个精确停在换行前的 LineComment trivia，随后产生一个相邻、不重叠且覆盖完整
  LF / CRLF 的 Newline trivia；正文 marker 不泄漏，UTF-8 payload 可精确切片。expression /
  declaration 保留换行后的 `left + right` Binary；block / file 在没有其他换行或分号时，仅凭该
  Newline trivia 保留 `val first = 0` 与 `val after = 1` 两个声明。全部公开产物确定且零诊断，
  本轮未发现生产缺陷；
- `parser_large_file_header_stress` integration test 建立两个 4,096-import 完整文件：合法源按四项
  循环混合 exact multi-segment、alias、wildcard 与长 qualified import；恢复源的每个 `import`
  均缺 target。两源都含 `package stress.headers`、交替 LF / CRLF separator 与最终
  `val after = 1` root；每源先双运行独立 Lexer，再由 file helper 双运行 Lexer / Parser，合计验证
  8 个 Lexer 与 4 个 Parser 产物。每源精确保留 4,096 个 import keyword、2,049 个 LF 与 2,048 个
  CRLF Newline trivia；合法源逐项保留全部 segment、wildcard / alias 与源码顺序，零诊断；恢复源
  保留 4,096 个只有真实 keyword 的 ImportDirective，并在下一 header / root starter 处产生 4,096 条
  有序空 Span L0049。两源的 package、imports、最终 Variable root、diagnostic / AST Span 全部
  source-local、可切片且确定，本轮未发现生产缺陷；
- `parser_large_qualified_header_paths` integration test 建立合法与恢复两个完整文件，每个文件均含
  三条 4,096-segment package / import 路径，合计覆盖 24,576 个 segment。合法源保留 package、
  exact alias import、wildcard import 与最终 `val after = 1` root，零诊断；恢复源分别在 package
  与 import 的末尾 `.` 后、exact import 的 `as` 后触发有序空 Span L0048 / L0049 / L0050，同时
  保留全部真实 segment、终结 marker、directive 与最终 root。每源先双运行独立 Lexer，再由 file
  helper 双运行 Lexer / Parser，合计验证 8 个 Lexer 与 4 个 Parser 产物；合法源精确包含 12,290 个
  Identifier 与 12,286 个 Dot，恢复源包含 12,289 个 Identifier 与 12,287 个 Dot。全部 Span 均
  source-local、可切片且确定，本轮未发现生产缺陷；
- `parser_large_file_header_separators` integration test 建立两个含 4,096 个 `import pkg.ItemN` 的
  完整文件，合计覆盖 8,192 个 imports。合法源循环使用 LF、CRLF、分号与内部含 LF 的 block
  comment 分隔，精确保留 package、全部双 segment imports 与最终 `val after = 1` root，零诊断；
  Lexer 观察到 4,096 个 import keyword、4,097 个 Dot、8,195 个 Identifier、1,024 个 Semicolon、
  1,024 个 BlockComment 和 2,049 个独立 Newline trivia，comment 内换行不泄漏为独立 trivia。
  恢复源仅以普通空格连接全部 header / root，相应产生 4,097 条有序 L0053，primary 逐一覆盖下一
  `import` 或最终 `val` starter，同时不吞 directive、不产生 Error root。每源先双运行独立 Lexer，
  再由 file helper 双运行 Lexer / Parser，合计验证 8 个 Lexer 与 4 个 Parser 产物；全部 Span 均
  source-local、可切片且确定，本轮未发现生产缺陷；
- `parser_recursion_boundary_matrix` integration test 以 34 个相邻深度源码锁定四个公开 Parser
  入口的递归预算边界。六类 expression 形状中，alternating prefix 与 group 分别接受 511 层、
  拒绝 512 层，assignment、Elvis、generic type 与 function type 分别接受 1,022 层、拒绝
  1,023 层；declaration generic type 接受 1,023 层、拒绝 1,024 层；block 与 file function body
  接受 1,024 层、拒绝 1,025 层。完整闭合与 EOF terminal 的 nested string/interpolation 在
  expression、declaration、file 接受 511 层、拒绝 512 层，在额外占用一级预算的 block 接受
  510 层、拒绝 511 层；四个 terminal 接受源码各精确保留最内层一个 byte-accurate L0005，
  不产生 Parser 级联，closed 接受源码保持零诊断，两类均保留全部 String AST。17 个成功与
  17 个失败源码各执行两次 Lexer 与两次 Parser，共验证 68 个 Lexer 和 68 个 Parser 产物或错误
  结果；失败侧均精确返回相同 `NestingLimitExceeded { limit: 1024 }`。矩阵锁定 1,024 单位实现
  预算映射到不同调用路径后的源码边界，不把内部预算误作统一源码层数；本轮未发现生产缺陷；
- `parser_stack_isolation_matrix` integration test 从四个相互独立的 64 KiB 调用线程分别执行
  expression、declaration、block 与 file 公开入口，共验证 8 个递归边界源码、16 个 Lexer
  产物和 16 个 Parser 结果。group 511 层、declaration generic type 1,023 层及 block / file
  nested block 1,024 层均双运行成功并保持零诊断、有效 typed root、source-local AST /
  diagnostic Span 与完整公开产物确定性；各自增加一层后均双运行返回相同
  `NestingLimitExceeded { limit: 1024 }`。线程启动或 join 失败会显式使测试失败，因此四个入口
  的边界递归继续由固定 Parser worker 承载，不依赖调用者线程栈大小；本轮未发现生产缺陷；
- `parser_operator_matrix` integration test 经生产 Lexer 与公开 expression 入口执行 240 个
  固定 case：110 个表达式右操作数中缀层双向组合、36 个 postfix/prefix/cast 高层组合、
  54 个结合性组合和 40 个不结合组成员组合；结构断言锁定低优先级根与高优先级子树，
  每个源码执行两次 Lexer 与两次 Parser，共验证 480 个 Lexer 和 480 个 Parser 产物的连续覆盖、
  末尾唯一 EOF、source-local AST / diagnostic Span、typed root 与完整公开产物确定性。200 个
  结构 case 两阶段零诊断；40 个不结合 case 的完整诊断序列恰好一个 `L0012`，精确指向第二个
  运算符 byte span；矩阵不复制生产 binding-power 数值，本轮未发现生产缺陷；
- `parser_token_inventory` integration test 自检 120 个互异片段，覆盖全部 42 个 Keyword、
  11 个 ReservedWord、43 个 Symbol、literal/string/interpolation、四类 trivia 与 L0001–L0008；
  分类与全库存检查、四入口矩阵和定向 string 回归共执行 701 个 source case，每例双 Lexer，
  合计验证 1,402 个 Lexer 产物的连续完整覆盖、唯一 EOF、source-local 诊断 Span 与完整公开
  产物确定性；四个公开 Parser 入口共执行 480 个 entry/case、960 个 Parser 产物，另以两个
  Parser 产物精确回归独立声明完整 string 的 L0017 / Span / error root。Parser 产物锁定
  source-local 有界 AST / 诊断、三类 typed root、完整文件所有 roots 和 package / import
  directive Span；矩阵无普通用户输入内部错误，本轮未发现生产缺陷；
- `parser_lexical_owner_matrix` integration test 把 4 个可继续 owner 与 5 个 EOF terminal owner
  分别投放到 16 个声明、名称、类型、class-family 和表达式位置，共执行 144 个 case；每例
  运行两次 Lexer 与两次完整文件 Parser，共验证 288 个 Lexer 和 288 个 Parser 产物。逐例锁定
  lexeme 完整覆盖、唯一 EOF、精确词法错误码、source-local 有界 Lexer / AST / diagnostic
  Span、文件根可解引用与两个阶段的完整公开产物确定性，并对 64 个可继续 case 的两次
  Parser 产物分别证明精确 `val after = 1` sentinel 是最后一个完整文件根；本轮未发现生产缺陷；
- `parser_diagnostic_witness_matrix` integration test 将生产目录 `L0009`–`L0078` 中 69 个现行
  Parser 诊断逐一映射到 expression、declaration、block 或 file 公开入口；每个 Lexer-clean
  witness 运行两次 Lexer 与两次 Parser，共验证 138 个 Lexer 和 138 个 Parser 产物。两个阶段
  均锁定完整公开产物确定性，并验证 lexeme 完整覆盖、唯一 EOF、source-local 有界 AST /
  diagnostic 主与 label Span、三类 typed root、完整文件 roots 及 package / import directive
  Span；两次 Parser 产物都恰好发出一次目标码，兼容保留但生产 Parser 已退役的 L0016 被显式
  排除，矩阵同时证明所有实际诊断均不发该码；本轮未发现生产缺陷；
- `parser_trivia_invariance_matrix` integration test 以 20 个完整 grammar case 覆盖文件头、声明、
  类型、表达式、call/lambda、control-flow 与 class-family，把 tab、无换行 block comment 和
  混合 trivia 投放到每个单独 token gap、全部 gap 及文件首尾；1,175 个源码变体各运行两次
  Lexer 与两次完整文件 Parser，共验证 2,350 个 Lexer 和 2,350 个 Parser 产物。全部变体均保持
  significant `LexemeKind` 序列与无 Span AST 结构指纹不变且零诊断，并逐次锁定 lexeme 完整
  覆盖、唯一 EOF、source-local 有界 AST / diagnostic Span、文件 roots、package / import
  directive Span、syntax shape 与两个阶段的完整公开产物确定性；本轮未发现生产缺陷；
- `parser_line_break_boundary_matrix` integration test 将 LF、CRLF、line comment 终止换行及
  block comment 内 LF / CRLF 六个结构载体，与四个合法非换行 trivia 载体投放到文件头、
  顶层声明、class member、`when` entry 和 `return` 边界；另锁定 enum comma 与中缀连续性，
  共执行 80 个 Lexer-clean 源码，每例运行两次 Lexer 与两次完整文件 Parser，共验证 160 个
  Lexer 和 160 个 Parser 产物。每例精确锁定 carrier 的 `TriviaKind` / spelling / byte 分段、
  lexeme 完整覆盖、唯一 EOF、source-local AST / diagnostic Span、文件 roots、package / import
  directive Span 与两个阶段的完整公开产物确定性；源码裸 CR 仍由 Lexer 以 L0001 拒绝，
  本轮未发现生产缺陷；
- `frontend_matrix_assertions` 为 prefix / suffix truncation、interior deletion、scalar / token
  duplication / transposition / replacement / insertion、token omission 与 lexical poison insertion
  十二个完整文件恢复矩阵提供共享双 Lexer / 双 Parser 入口；88,453 个主要变异 / 截断 case、
  264 个 baseline / complete case 与 2 个定向 omission 回归合计 88,719 个 source case，共验证
  177,438 个 Lexer 和 177,438 个 Parser 产物。两阶段产物锁定
  source identity、lexeme 完整覆盖与唯一 EOF、source-local AST / diagnostic 主与 label Span、
  文件 roots、package / import directive Span，并比较完整公开产物确定性；内部区间删除矩阵
  发现并修复一项生产缺陷，详见下项；
  既有 `frontend_adversarial` 也通过该入口复用双 Lexer，避免同一 integration test 重复加载
  `lexer_matrix_assertions`；
- `parser_prefix_truncation_matrix` integration test 以 22 个 Lexer / Parser-clean 完整文件覆盖
  文件头、声明、callable、block、lambda、control-flow、postfix、class-family、接口委托、
  运算符层级及 Unicode 嵌套 string / interpolation；其 1,373 个 UTF-8 scalar 前缀均保持
  lexeme 完整覆盖、唯一末尾 EOF、有界诊断 / AST Span，并完成两次确定性 Lexer 与完整文件
  Parser；
- `parser_entry_prefix_truncation_matrix` integration test 以 12 个 Lexer / Parser-clean 独立源码按
  expression / declaration / block 各 4 个覆盖 callable、control-flow、运算符、lambda、泛型、
  class-family、局部解构、loop-family 与 Unicode lexical owner；三个入口分别执行 195 / 350 /
  202 个 UTF-8 scalar 前缀；加上 12 个 clean preflight 共 759 个 source case，每例运行两次
  Lexer 与两次对应入口 Parser，共验证 1,518 个 Lexer 和 1,518 个 Parser 产物，逐次锁定连续
  lexeme 覆盖、唯一末尾 EOF、source-local 诊断 / AST Span、可解析 typed root 与公开产物
  确定性；本矩阵未发现生产缺陷；
- `parser_entry_token_omission_matrix` integration test 复用同一 12-case 独立入口 corpus，逐一
  删除全部显著 token；expression / declaration / block 分别执行 66 / 104 / 70 个 mutation，
  加上 12 个 baseline 共 252 个 source case，每例运行两次 Lexer 与两次 Parser，共验证 504 个
  Lexer 和 504 个 Parser 产物；逐次锁定连续覆盖、唯一 EOF、source-local 诊断 / AST Span、
  可解析 typed root 与公开产物确定性；本矩阵未发现生产缺陷；
- `parser_suffix_truncation_matrix` 复用相同 22-file corpus，在每个 UTF-8 scalar 起点删除源码
  前缀并保留后缀，精确执行 1,373 个后缀；加上 22 个 clean preflight 共 1,395 个 source case，
  每例运行两次 Lexer 与两次完整文件 Parser，共验证 2,790 个 Lexer 和 2,790 个 Parser 产物。
  `parser_entry_suffix_truncation_matrix` 同样复用 12-entry corpus，按 expression / declaration /
  block 分别执行 195 / 350 / 202 个后缀；加上 12 个 preflight 共 759 个 source case，验证 1,518
  个 Lexer 和 1,518 个对应入口 Parser 产物。两个矩阵均锁定连续覆盖、唯一 EOF、source-local
  AST / diagnostic Span、有效 root 与公开产物确定性，本轮未发现生产缺陷；
- `parser_interior_deletion_matrix` 在相同 22-file corpus 的内部 UTF-8 scalar 边界间删除任意
  非空连续区间，同时保留非空前后缀，精确执行 44,969 个 mutation；加上 22 个 clean preflight
  共 44,991 个 source case，验证 89,982 个 Lexer 和 89,982 个完整文件 Parser 产物。
  `parser_entry_interior_deletion_matrix` 在 12-entry corpus 按 expression / declaration / block
  分别执行 4,453 / 15,634 / 4,656 个 mutation；加上 12 个 preflight 共 24,755 个 source case，
  验证 49,510 个 Lexer 和 49,510 个对应入口 Parser 产物。矩阵发现 companion constant 缺失
  `val` 后直接出现 segmented string 时只消费 `StringStart`、继而从 lexical owner 内部恢复并
  错误返回 `InvalidLexemeStream` 的缺陷；constant 现在把该 owner 交给名称恢复，并继承成员
  `}` hard stop，定向回归锁定 companion 与外层 classifier closer 均被保留；
- `parser_scalar_duplication_matrix` 在相同 22-file corpus 原位重复每个完整 UTF-8 scalar，精确
  执行 1,351 个 mutation；加上 22 个 clean preflight 共 1,373 个 source case，验证 2,746 个
  Lexer 和 2,746 个完整文件 Parser 产物。`parser_entry_scalar_duplication_matrix` 在 12-entry
  corpus 按 expression / declaration / block 分别执行 191 / 346 / 198 个 mutation；加上 12 个
  preflight 共 747 个 source case，验证 1,494 个 Lexer 和 1,494 个对应入口 Parser 产物。两个
  矩阵不添加分隔空格，直接覆盖 identifier、数字、运算符、注释 opener 与 segmented string /
  interpolation 内部边界，并保持连续覆盖、唯一 EOF、source-local AST / diagnostic Span、有效
  root 与公开产物确定性；本轮未发现生产缺陷；
- `parser_scalar_transposition_matrix` 在相同 22-file corpus 枚举 1,329 个相邻 UTF-8 scalar
  pair，排除 25 个相同字符 no-op 后精确执行 1,304 个 mutation；加上 22 个 clean preflight
  共 1,326 个 source case，验证 2,652 个 Lexer 和 2,652 个完整文件 Parser 产物。
  `parser_entry_scalar_transposition_matrix` 在 12-entry corpus 按 expression / declaration / block
  分别枚举 187 / 342 / 194 个 pair，排除 4 / 7 / 3 个 no-op 后执行 183 / 335 / 191 个 mutation；
  加上 12 个 preflight 共 721 个 source case，验证 1,442 个 Lexer 和 1,442 个对应入口 Parser
  产物。两个矩阵不添加分隔空格，覆盖 lexeme 与 lexical-owner 内部邻接，并锁定源码长度不变、
  变异非 no-op、连续覆盖、唯一 EOF、source-local AST / diagnostic Span、有效 root 与公开产物
  确定性；本轮未发现生产缺陷；
- `parser_scalar_replacement_matrix` 以共享 13-scalar 字母表逐位置替换相同 22-file corpus；
  17,563 个候选排除 123 个相同字符 no-op 后精确执行 17,440 个 mutation，加上 22 个 clean
  preflight 共 17,462 个 source case，验证 34,924 个 Lexer 和 34,924 个完整文件 Parser 产物。
  `parser_entry_scalar_replacement_matrix` 在 12-entry corpus 按 expression / declaration / block
  分别枚举 2,483 / 4,498 / 2,574 个候选，排除 15 / 29 / 42 个 no-op 后执行 2,468 / 4,469 /
  2,532 个 mutation；加上 12 个 preflight 共 9,481 个 source case，验证 18,962 个 Lexer 和
  18,962 个对应入口 Parser 产物。字母表覆盖 identifier、number、poison、string / char、escape、
  interpolation、comment、brace、LF 与多字节 Unicode，并锁定每项精确计数、连续覆盖、唯一
  EOF、source-local AST / diagnostic Span、有效 root 与公开产物确定性；本轮未发现生产缺陷；
- `parser_scalar_insertion_matrix` 复用同一 13-scalar 字母表，在相同 22-file corpus 的源码起点、
  scalar 间及 EOF 共 1,373 个 UTF-8 边界分别插入每项，精确执行 17,849 个 mutation；加上 22
  个 clean preflight 共 17,871 个 source case，验证 35,742 个 Lexer 和 35,742 个完整文件 Parser
  产物。`parser_entry_scalar_insertion_matrix` 在 12-entry corpus 按 expression / declaration /
  block 的 195 / 350 / 202 个边界执行 2,535 / 4,550 / 2,626 个 mutation；加上 12 个 preflight
  共 9,723 个 source case，验证 19,446 个 Lexer 和 19,446 个对应入口 Parser 产物。两个矩阵不
  添加分隔空格，锁定每项精确计数、插入后 byte length、连续覆盖、唯一 EOF、source-local
  AST / diagnostic Span、有效 root 与公开产物确定性；本轮未发现生产缺陷；
- `parser_entry_token_duplication_matrix` integration test 在同一 corpus 的 240 个显著 token 后
  分别插入其源码副本；20 个 lexical-mode mutation 锁定 Scanner / Parser 总性，220 个普通
  mutation 精确锁定原 token 与 duplicate 的 `TokenKind` / Span；加上 12 个 baseline 共 252 个
  source case，每例运行两次 Lexer 与两次 Parser，共验证 504 个 Lexer 和 504 个 Parser 产物。
  矩阵发现并修复 control-body Error 节点覆盖尚未消费 token 时 trivia-gap 查询构造反向 Span 的
  缺陷；重叠范围现在明确表示无 gap，并由既有 tail recovery 继续消费错误 token；
- `parser_entry_lexical_poison_replacement_matrix` 对同一 240 个 token slot 分别以 `#`、`async`、
  `'ab'`、`1e3` 替换，共执行 960 个 mutation；加上 12 个 baseline 共 972 个 source case，每例
  运行两次 Lexer 与两次 Parser，共验证 1,944 个 Lexer 和 1,944 个 Parser 产物；80 个
  lexical-mode case 锁定 Scanner / Parser 总性，880 个普通 case 精确锁定唯一 L0001 / L0002 /
  L0007 / L0008 与 poison primary Span，全部保持连续覆盖、唯一 EOF、source-local AST / 诊断、
  typed root 有效和公开产物确定性；本矩阵未发现生产缺陷；
- `parser_entry_lexical_poison_insertion_matrix` 复用共享 lexical-mode gap 状态机，在 12-case corpus
  的 240 个 token 上枚举 252 个 gap；239 个 code-mode gap 与 13 个 string-mode gap 分别插入
  四种 poison，共执行 1,008 个 mutation；加上 12 个 baseline 共 1,020 个 source case，每例
  运行两次 Lexer 与两次 Parser，共验证 2,040 个 Lexer 和 2,040 个 Parser 产物。956 个 code-mode
  mutation 精确锁定唯一 L0001 / L0002 / L0007 / L0008 及 Span，52 个 string-mode mutation
  保持 Lexer / Parser 零诊断；完整文件矩阵的 418 / 409 / 9 计数同时保持不变，本矩阵未发现
  生产缺陷；
- `parser_entry_adjacent_token_transposition_matrix` 复用同一 12-case corpus，在 240 个 token 内枚举
  expression / declaration / block 的 62 / 100 / 66 个相邻 pair，共执行 228 个 mutation；加上
  12 个 baseline 共 240 个 source case，每例运行两次 Lexer 与两次 Parser，共验证 480 个 Lexer
  和 480 个 Parser 产物；201 个不涉及 lexical-mode segment 的 pair 精确锁定交换后 right / left
  的原 `TokenKind` 与 byte Span，27 个 string owner pair 锁定 Scanner / Parser 总性，全部保持
  source-local AST / 诊断、typed root 有效和公开产物确定性；本矩阵未发现生产缺陷；
- `parser_entry_trivia_invariance_matrix` 复用同一 12-case corpus 和 lexical-mode gap 状态机，在
  expression / declaration / block 的 66 / 106 / 67 个 code-mode gap 分别投放 tab、无换行 block
  comment 与混合 trivia，并覆盖每例全 gap 投放；共执行 753 个 mutation，全部保持 baseline 的
  significant `LexemeKind` 序列、无 Span AST 结构指纹和两阶段零诊断。每个插入区间按 overlap
  精确锁定共享表中的 `TriviaKind` 与 spelling，包括与原 whitespace 合并的 lexeme；all-gap
  变体按累计 byte 位移验证全部插入。共享 entry fingerprint 让两次解析均验证并比较 shape，
  12 个 token/gap 建模基线与 765 个 Parser 基线或 mutation 共 777 个 source case 均运行两次
  Lexer，共验证 1,554 个 Lexer 产物的连续覆盖、唯一 EOF、source-local diagnostic Span、零诊断
  与完整公开产物确定性；765 个 Parser source case 共执行 1,530 次生产解析，其中 753 个变体
  占 1,506 次，不再为 shape 额外执行第三次解析；本矩阵未发现生产缺陷；
- `parser_entry_line_break_boundary_matrix` 将 LF、CRLF、line comment 终止换行及 block comment
  内换行六种结构载体，与四种无 LF trivia 投放到 expression `when` entry、declaration class
  member 和 block 裸 `return` 边界；反向锁定 expression / block 中缀连续与 enum comma 必需。
  它与完整文件矩阵共享唯一 10-carrier `TriviaKind` / spelling / byte 分段表；60 个 Lexer-clean
  源码各运行两次 Lexer 与两次独立入口 Parser，共验证 120 个 Lexer 和 120 个 Parser 产物，
  逐次锁定 lexeme 完整覆盖、唯一 EOF、source-local AST / diagnostic Span、typed root、syntax
  shape 与两个阶段的完整公开产物确定性；本矩阵未发现生产缺陷；
- `parser_token_omission_matrix` integration test 复用同一 22-file corpus，逐一删除原始范围内
  396 个显著 token；96 个 owner-affecting case 锁定总性，300 个非 owner case 还要求后置
  `val sentinel = 0` 保持最后顶层 Item。全部 case 重复解析、验证完整 lexeme 覆盖和有界
  诊断 / AST Span，并定向回归 class member closer 与 nested interpolation tail 两个恢复缺陷；
- `parser_lexical_poison_replacement_matrix` integration test 复用同一 corpus 和 396 个 token slot，
  分别以 `#`、`async`、`'ab'` 与 `1e3` 生成 1,584 个重新词法分析的变体；1,528 个不改变
  lexical mode 的变体精确保留一次目标 L0001 / L0002 / L0007 / L0008，384 个 owner-affecting
  变体锁定总性，1,200 个非 owner 变体还要求后置 sentinel 存活。错误接收者的 call / index
  后缀恢复复用 declaration owner stack，避免内层 string interpolation closer 被误作外层边界；
- `parser_token_duplication_matrix` integration test 复用同一 corpus 和 396 个 token slot，在每个
  原 token 后以空格分隔复制其精确源码切片并重新词法分析；382 个非 lexical-mode 变体锁定
  原 token 与 duplicate 的相同 `TokenKind` 和精确 byte Span，96 个 owner-affecting 变体锁定
  总性，300 个非 owner 变体还要求后置 sentinel 存活。全部变体重复完整文件解析并保持公开
  AST / 诊断确定一致；本矩阵未发现生产缺陷；
- `parser_lexical_poison_insertion_matrix` integration test 复用同一 corpus 的源码起点与 396 个
  token 末尾，共枚举 418 个 gap，并分别插入 `#`、`async`、`'ab'`、`1e3` 生成 1,672 个变体；
  409 个 code-mode gap 的 1,636 个变体在插入 Span 精确产生 L0001 / L0002 / L0007 / L0008，
  9 个 string-mode gap 的 36 个变体保持 Lexer / Parser 零诊断。原语法 token 与 owner 全部保留，
  因此所有变体均要求后置 sentinel 存活，并重复完整文件解析以锁定总性和确定性；本矩阵未发现
  生产缺陷；
- `parser_adjacent_token_transposition_matrix` integration test 复用同一 corpus 的 396 个 token，
  枚举 374 个相邻 pair 并以空格隔离交换后的原 token 源码；356 个非 lexical-mode 变体锁定
  right / left 的原 `TokenKind` 与计算后的精确 Span，154 个 owner-affecting 变体锁定总性，
  220 个非 owner 变体还要求后置 sentinel 存活。全部变体重复完整文件解析并保持公开 AST /
  诊断确定一致；本矩阵未发现生产缺陷；
- runner 返回只包含规范相对路径和稳定证据 / 失败类别的结构化 outcome。测试报告
  边界转义路径中的反斜杠、tab、CR 和 LF，不输出 fixture 根的绝对路径。

`source-pass` 仍只表示 Phase 0 基础设施接线成功；Phase 1 suite 分别调用扫描器、独立表达式、
独立声明、独立 block、lambda expression、具名函数隐式 `Unit` 与完整文件 Parser。这些
suite 不表示类型检查或编译，harness 也不调用 renderer 或固定公共机器诊断协议。
`tests/name_resolution.rs` 另行枚举非零 Phase 2 `name-pass` / `name-fail` fixture，真实调用
Lexer、完整文件 Parser 与名称解析入口，并精确核对 L0079–L0081 的 code / byte Span；
其 13 个 integration test 的 14 条源码路径统一经 typed file helper 进入名称解析，每条源码执行
两次 Lexer 与两次完整文件 Parser，共验证 28 个 Lexer 和 28 个 Parser 产物的 source identity、
lexeme 连续覆盖、唯一 EOF、AST / diagnostic Span、file roots、directive Span 与完整公开产物
确定性；名称解析领域断言继续消费首个已验证产物，本轮未发现生产缺陷。
`tests/type_checking.rs` 枚举 `type-pass` / `type-fail` fixture，经相同前置流水线调用类型检查，
并精确核对 L0082–L0130 的 code / byte Span；当前 `type-pass` 与 `type-fail` 各有六个真实
fixture，包含名义类型、interface 实现、override、委托、`when`/smart-cast、`Copyable`/
结构化解构、callable 和顺序容器正反例。其 29 个 integration test 实际执行的 49 条源码路径
统一经 typed file helper 进入名称解析与类型检查，每条源码执行两次 Lexer 与两次完整文件
Parser，共验证 98 个 Lexer 和 98 个 Parser 产物的 source identity、lexeme 连续覆盖、唯一 EOF、
AST / diagnostic Span、file roots、directive Span 与完整公开产物确定性；既有领域断言继续消费
首个已验证产物，本轮未发现生产缺陷。
`tests/type_callable.rs` 的 9 个 integration test 各执行一条独立源码，并统一经相同 typed file
helper 进入名称解析与 callable 类型检查；每条源码执行两次 Lexer 与两次完整文件 Parser，共
验证 16 个 Lexer 和 16 个 Parser 产物的相同公开不变量。callable target、实参映射、参数 mode、
place / temporary、overload、deferred 与 L0119–L0124 领域断言保持不变；新增矩阵锁定具名与
lambda 参数的 Value/Borrow/Inout typed fact、move/arity 结构错误不发布模式，本轮未发现生产缺陷。
`tests/type_containers.rs` 的 6 个 integration test 同样各执行一条独立源码，并统一经 typed file
helper 进入名称解析与顺序容器类型检查；共验证 12 个 Lexer 和 12 个完整文件 Parser 产物的相同
公开不变量。`Array` / `List` / `MutableList`、构造推导、元素可存储性、element place、intrinsic
identity、deferred 与 L0091、L0094、L0122、L0125–L0130 断言保持不变，本轮未发现生产缺陷。
`tests/type_copyability.rs` 的 8 个 integration test 各执行一条独立源码，并统一经 typed file
helper 进入名称解析与 copyability 类型检查；共验证 16 个 Lexer 和 16 个完整文件 Parser 产物的
相同公开不变量。conditional `Copyable`、有限内联布局、intrinsic `Box`、结构化解构 copy /
consume、source identity 与 L0091、L0115–L0118 断言保持不变，本轮未发现生产缺陷。
`tests/ownership_checking.rs` 的 15 个 integration test 覆盖 source identity、MoveOnly 与
Copyable 按值交付、Borrow / Inout、重新赋值、temporary、分支 / loop 合流、终止路径、
SymbolId 遮蔽、错误 AST 去级联、参数 binding、place overlap、源码顺序与 nested-call loan、
Inout mutability、ASAP drop matrix、deferred 边界、重复运行确定性和真实 pass / fail fixture；
fixture runner 精确枚举一个正例与一个反例，并核对 L0131、L0133–L0135 的 code 与 primary
byte Span，领域测试另核对冲突来源和 move/declaration label。
`tests/multifile_ownership_checking.rs` 的 receiver 矩阵覆盖 stable/temporary、Borrow/Inout/Value、
第零操作数与显式实参 overlap、implicit `this` capability、Value `this` move/drop、closure
shared capture 以及 Borrow-only delegation plan；错误路径原子清空 executable receiver facts。
其中 Value `this` move 后形成 capture 精确报 L0131，shared-captured MoveOnly `this` 的 Value
移出精确报 L0133，避免把 non-owning capture 静默降级为普通读取。
capture formation 与 lambda body 在 trial state 上完成；任一 capture 或 body 失败都会回滚先前
symbol loan/move，并阻止失败 closure identity 登记，避免在 lambda 后产生 L0135/L0137 级联。
`tests/ownership_construction.rs` 的 6 个 integration test 覆盖 nominal/value-class、payload 与
bare enum case、intrinsic `Box`、位置/命名参数源码求值顺序、copy/move/temporary delivery、
inline/heap root obligation、嵌套构造、`Nothing` 截断、active loan，以及 local/temporary/
return/branch/loop 的 transfer/drop；独立 Phase 3 fixture 精确核对 L0131，生产单元测试另锁定
无效 construction descriptor 的内部错误边界。
`tests/ownership_containers.rs` 的 12 个 integration test 覆盖列表式/运行时长度构造、三种
容器的 Copyable/MoveOnly element read、Borrow/Inout、逻辑索引 overlap、字段容器路径、owner
move、replacement 提交顺序、temporary owner drop、deferred 边界与重复运行确定性；Phase 3
fixture 同时核对新增 L0136 及相邻 L0131/L0135，Phase 2 `type_containers` 继续锁定 List Inout
拒绝与 index mutability。
`tests/ownership_structural.rs` 的 4 个 integration test 覆盖条件 value class、nullable enum、
intrinsic Box、无 / 有 `Copyable` 上界类型参数、Copy/Consume 完整解构、temporary、字段的
Borrow / Inout / Value 投影、自动 `componentN()`、显式成员优先及普通 class 字段；另精确枚举
一个 structural pass 与一个 fail fixture，并核对 L0131 / L0132 primary 和字段声明 label。
`tests/ownership_closures.rs` 的 11 个 integration test 覆盖 capture identity/遮蔽/嵌套、
`this` 归一及 receiver-field loan、shared/move formation、loan ASAP 结束、owner/capture drop、L0137–L0139、
`Transferable` 类型矩阵与 compiler-bound cross-thread effect；另精确枚举一个 closure pass 与
一个 fail fixture，核对 L0137/L0138 primary byte Span。

## Source formatter

`lang_frontend::formatting::format_source` 是首个保守 formatter API。调用方提交 map-local
`SourceId`；入口先运行生产 Lexer 与 `parse_file`，任一用户诊断整体返回而不产生部分文本。
成功路径不维护第二份关键字或 symbol 拼写表，只复制原 lexeme slice；普通 horizontal
whitespace 延迟到相邻内容已知后规范为零或一个 space，line start 根据 `{}` 与多行 `()` / `[]`
栈输出四空格缩进。上下文相关 `<` / `>` / `+` / `-` 保留原邻接类别，字符串片段、comment
正文及每个 newline lexeme 的 LF/CRLF 字节不改写。首版不折行、排序、合并空行或修改非法源码。

`kovenc format <path>` 读取单个 UTF-8 `.ko` 文件并把结果写 stdout；
`kovenc format --check <path>` 在规范时退出 0、有差异时退出 1，两种形式都不写回文件。固定
参数错误、读取/UTF-8/internal failure 和 frontend diagnostics 退出 2，后者复用结构化诊断
renderer；显式全局 `--message-format=json` 仅把该诊断分支切换为 ADR-0014 JSON Lines。
真实 binary 测试锁定 human/machine stderr、stdout、0/1/2 矩阵和输入不变，unit test 另锁定
输出 writer 失败不 panic。原地写入、目录遍历、stdin、配置和 range formatting 尚未实现。

## Local project source-set 与 native CLI

`lang-cli::project::load_project_source_set` 接受显式、文件名精确为 `project.toml` 的路径，只负责
manifest IO、严格 version 1 value 校验与本地 filesystem discovery。root identity 是已验证并排序的
manifest-relative `/` 路径；source identity 是 root-relative UTF-8 logical path，presentation path
和完整 UTF-8 text 只作为后继 driver 输入，不参与 identity。root 路径段中的 symlink 被拒绝，root
内部 symlink entry 被忽略；逻辑/物理 root overlap、hard link 重复与无法取得可靠 physical-file
identity 均作为 project operational error fail loud，不产生 `Ldddd`。

provider 读取完成后才发布不可变 snapshot，目录项错误和成功 source 均先稳定选择/排序；空 root
与空 source set 合法。`lang-cli::project_build` 把 snapshot 转为同一 `SourceMap` 的 parsed unit，
顺序运行 name/type/ownership validated frontend，并在全部源码诊断清空后选择显式 entry；
`lang-cli::project_command` 负责固定 CLI、sibling temporary、link/no-replace publish、launch 与 cleanup。
当前没有公开 project check、dependency build、manifest target/default 或隐式 entry；单次加载期间项目树
不被并发替换是首版 operational assumption。

## Single-file native CLI

`kovenc build <source.ko> --entry <name> -o <executable>` 与
`kovenc run <source.ko> --entry <name>` 是 SPEC-0190 的公开 native 入口。两者固定参数顺序，
不猜测 entry/output；frontend diagnostics 服从全局 human/JSON Lines 选择。build 在 output
目录使用唯一临时 object，run 在系统临时目录使用唯一 object/executable，均不把中间产物作为
公共 API。当前只支持单文件、显式 `() -> Unit` 顶层函数和首个 AArch64 macOS target。

## TextMate grammar

`editors/textmate/syntaxes/koven.tmLanguage.json` 是不依赖 LSP 的 TextMate JSON grammar，声明
`source.koven` 与 `.ko` 文件类型。repository 按注释、字符串/插值、字符、数值、annotation、
声明名称、内建类型、关键字、未来保留字、运算符和标点拆分；匹配边界遵循现行 Lexer 的
ASCII 标识符、单行字符串、非嵌套 block comment、最小数值后缀与最长符号集合。它只提供
词法近似，不读取名称解析或类型检查事实。

`editors/textmate/tests/highlight.ko` 与 `reserved.ko` 分别保存正常和未来保留字 corpus，
`scopes.tsv` 为仓库私有的代表性 scope/源码片段契约。`lang-frontend` 的
`textmate_grammar` integration test 检查 grammar repository 和 scope 存在性，并用生产 Lexer
证明正常 corpus 无诊断且覆盖主要 token/trivia family、reserved corpus 精确产生 11 个
L0002。`editors/textmate/tests/lexical-contract.tsv` 另以 80 个共享 case 锁定全部 33 个 operator、
10 个 punctuation、有效 integer/float/string escape/character 及代表性拒绝边界；零依赖 Node
verifier 实际从 JSON repository 递归定位并执行锚定 regex，Rust integration test 用同一 TSV
验证全部正例的生产 Lexer 分类或零诊断。66 个正例 source 与两个 corpus 共 68 个 source case
各运行两次 Lexer，共验证 136 个 Lexer 产物的连续完整覆盖、唯一 EOF、source-local diagnostic
Span 与完整公开产物确定性；纯 Lexer target 只加载单一职责的 `lexer_output_assertions`，完整
frontend 断言门面复用同一实现。TextMate `package.json` 只提供 `npm test` 脚本，不含依赖或
lockfile，也不把 Node 引入 Cargo 测试。

## Tree-sitter grammar

`editors/tree-sitter/grammar.js` 是 Koven concrete-syntax grammar 的唯一手写 JavaScript
入口；`src/grammar.json`、`src/node-types.json` 与 `src/parser.c` 是由精确锁定的官方
`tree-sitter-cli` `0.26.12` 确定性生成并提交审阅的产物。grammar 覆盖文件头、声明与
class-family、类型、block/control-flow、call/lambda、字符串插值和现行 Pratt 运算符层级。
生产编译器仍只使用 Rust Lexer/Parser，Tree-sitter 的增量错误恢复不构成 compile-pass 判据。

Tree-sitter 的正则 token 无法排除全部硬关键字和未来保留字，因此 `src/scanner.c` 在 ASCII
identifier 边界集中拒绝现行 42 个硬关键字与 11 个未来保留字，并为局部解构单独排除 `_`。
`test/corpus/koven.txt` 的 7 个 concrete-tree case 覆盖文件头与声明、class-family、call/lambda、
control-flow、字符串插值、跨声明恢复和保留字。`lang-frontend` 的
`tree_sitter_grammar` integration test 再用生产 Lexer/Parser 读取同一批代表性 `.ko` fixture，
锁定合法文件零诊断、`L0009` 空 span 恢复、关键字分类、完整有序诊断及错误后的后续根节点。
同一测试还从 external scanner 的唯一 C 初始化表提取全部 53 个不可用 identifier 拼写，精确
对照 42 个生产 `Keyword` 与 11 个 `ReservedWord` / `L0002` span；原生 corpus 同时证明
`value` / `async` 被拒绝，而 `className` / `asyncTask` 仍按完整词边界成为 identifier。word
contract 与三个 fixture 共 4 个 source case 各运行两次 Lexer，共验证 8 个 Lexer 产物；三个
fixture 各运行两次完整文件 Parser，共验证 6 个 Parser 产物的 AST、诊断、root、directive Span
与完整公开产物确定性。
CLI 仅是该目录精确锁定的开发依赖，不进入 Cargo workspace 或编译器运行时。VS Code
extension、语义高亮与 LSP token 仍尚未实现。

## 尚未实现的编译流水线

现行 guide 要求的流水线仍是计划边界：

```text
源码 → Lexer → Parser / 索引式 AST → 名称与类型检查 → 所有权检查
     → 自建 SSA IR → LLVM IR → 目标文件 → 本机可执行文件
```

其中 `lang-frontend` 不依赖 LLVM / `inkwell`，LLVM 细节后续只能收敛在 codegen 边界。
索引式 AST 存储、结构化诊断基础设施、Lexer、独立表达式 / 声明 / block / lambda Parser、
callable 参数与 typed call argument、局部解构、完整文件与 package / import Parser、
control-flow、class-family、窄化接口委托、具名函数隐式 `Unit` 返回标注、单文件名称解析、
基础类型检查、名义/泛型/interface 检查及分层 fixture harness 已存在；enum case type、
`when` 穷尽性、smart cast、条件 `Copyable`、单态 callable/member 选择与顺序容器 Phase 2
类型事实，以及 nominal/enum/Box construction target、实例化和 Value operand 映射也已实现；
整变量 MoveOnly / Copyable 状态、use-after-move、消费式 value-class
解构、字段 / 自动结构分量的部分移动拒绝、调用期 loan、owned-value ASAP drop facts 与
顺序容器核心 element place 所有权，以及 construction ordered delivery/root obligation 已由
独立 Phase 3 阶段实现；泛型 callable 实例化已由
SPEC-0177 / SPEC-0174 实现。
`object` / `companion object` 关联成员，以及容器
Phase 5 容器 relocation effect 等后续所有权规则仍未实现；
`lang-std` 的单文件 bootstrap 已由 ADR-0012 / SPEC-0042 实现：CLI 内部 driver 编排显式
source/entry，复用 frontend、resolved-entry object API 和 Clang linker；SPEC-0043 已让真实
Koven prelude 的正常 smoke 退出 0、标准 `error()` smoke 经 Abort 非零终止。SPEC-0189 已增加标准 `println(String)` 的
首个 literal-only stdout slice 与真实 Hello World entry；SPEC-0192 已将其迁移到一般
`StringOwner` 主线：frontend 使用 builtin String identity 与 ownership/drop facts，typed SSA
提供 literal/concat/equal/print/drop，LLVM 使用 `{ptr, length, capacity}`、静态/空串零 capacity
和动态 owner 精确 free。普通 String 已能跨 Borrow/Value 参数与返回值，并作为 aggregate、
Rc、Array/List/MutableList 元素及 owned move-closure capture 参与正常/提前退出析构；closure
thunk 通过 shared environment pointer 读取 capture，closure owner 仍是唯一析构责任方。
interpolation、`String?` native ABI、String member 与其他 printable 重载仍未实现。
SPEC-0044 已在同一 prelude 实现 `Pair` / `Result` 声明，并验证条件复制、
MoveOnly 诊断、构造、投影与解构的 native 正反路径。SPEC-0190/0193 已公开单文件显式 entry
和零参数 conventional main build/run；SPEC-0194 已增加参数化 main/argv。SPEC-0052 的
manifest→immutable base source-set provider 已由 SPEC-0054 接入 validated multi-file frontend、
显式 package-qualified entry、unit object/link/no-replace publish 与 argv run；依赖 compilation unit
与多文件标准库装配仍未实现。
内部值/系统分配 ABI
及对应 LLVM aggregate、allocation/drop 后端基元已由 ADR-0008 / SPEC-0035 完成；SPEC-0185
已允许未使用的声明型 type roots 共存；SPEC-0184 已完成源码 nominal/enum/Box constructor、
投影/解构、root drop、L0145 与真实 native link/run 接线。
SPEC-0045 已把非 nullable `Rc<T>` 的 construction、显式 share、Copyable payload read、target
preflight、retain/release-to-zero 与真实 CLI build/run 接入同一主线并完成；SPEC-0195 已完成
通用 Borrow callable signature、frontend loan、LLVM pointer ABI 及 Rc/class/Box native 接线；
SPEC-0196 已完成 nullable Rc/class/Box 的 `if` proof、null niche、conditional drop 与 native 接线。

## 更新要求

后续每个 Spec 改变模块关系、数据流或已实现阶段时，必须在同一任务中更新本页。不要在这里
保存决策历史，也不要把尚未批准的设想写成实现事实。
