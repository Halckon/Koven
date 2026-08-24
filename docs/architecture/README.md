# Koven 架构快照

本目录描述仓库**当前已经实现**的架构。设计原因记录在 [`../adr/`](../adr/)，单次交付范围
记录在 [`../specs/`](../specs/)，语言语义由
[`../guide/00-index.md`](../guide/00-index.md) 导航的现行 v0.28 文档集定义。class-family 与
窄化接口委托已分别由 SPEC-0017、SPEC-0064 实现；SPEC-0018 已建立单文件名称解析，
SPEC-0019 已建立基础类型检查，SPEC-0020 已建立名义/泛型/interface 类型检查。
SPEC-0021 已建立 enum case type、`when` 穷尽性与 flow-sensitive smart cast；SPEC-0022 已
建立条件 `Copyable`、有限内联布局、intrinsic `Box` 与结构化解构类型事实；SPEC-0067 已
建立单态 callable/member 选择、实参映射与类型层面 place 分类；SPEC-0023 已建立顺序容器
类型、核心构造和 element-place 类型事实；SPEC-0027 已建立整变量所有权状态与
use-after-move 检查；SPEC-0028 已建立条件复制、消费式解构和结构分量移动检查；
SPEC-0173 已让唯一期望函数类型的 lambda 采用 Value/Borrow/Inout 参数契约，并发布稳定
parameter binding typed facts；
SPEC-0175 已让 block 内未分组 lambda 实参优先进入 expression parser，不再被 outer block stop
误判为空实参；
SPEC-0176 已把普通 callable 与 function-type 的无 marker / 显式 `borrow` 参数规范化为
`Borrow`，并以声明侧 `own` 形成既有 `ParameterMode::Value`；
SPEC-0029 已建立参数 binding 能力、名称/字段 place、同步调用期 loan、L0133–L0135 与
owned-value ASAP drop facts；
SPEC-0030 已建立顺序容器构造效果、逻辑 element place、L0136、元素 loan/replacement 与
旧元素 drop facts；
SPEC-0032 已建立解析身份驱动的 closure capture、borrowed/move formation effect、逃逸与
跨线程 `Transferable` 检查、L0137–L0139、capture loan 和 closure/capture drop facts；
SPEC-0058 已提供独立 TextMate grammar 与由生产
Lexer 校验的高亮回归 corpus；SPEC-0059 已提供 Tree-sitter grammar、生成 parser、外部
identifier scanner、原生 corpus 与生产前端交叉验收。

## 当前状态

仓库已完成 Phase 0、Phase 1 与当前已实施的 Phase 2 主线，并已进入 Phase 3。截至 v0.27
已实施的参数契约、显式实参调用期 loan、owned-value ASAP drop facts 与顺序容器核心 element place
所有权，以及简化 closure capture 与跨线程 `Transferable` 已经实现。工程骨架按
[ADR-0002](../adr/0002-bootstrap-workspace-layout.md) 建立，当前已实现：

- 根目录是 resolver 3 的 virtual Cargo workspace；所有 package 使用 Rust edition 2024，
  toolchain pin 和初始 MSRV 均为 `1.96.0`，并在许可与发布策略确定前保持不可发布；
- 五个 workspace member 均有 Cargo 可识别的 target，依赖方向单向且无环；
- `lang_frontend::source` 已提供统一 source / `Span` 基础设施；
- `lang_frontend::diagnostic` 已提供结构化诊断模型、`L0001`–`L0139` 正式前端错误码与
  确定性聚合顺序，`kovenc` binary 内已有尚未接入编译流水线的最小纯文本 renderer；
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
- `lang_frontend::name_resolution` 已提供显式 `NameEnvironment`、单文件类型 / 值双命名
  空间、稳定 `ScopeId` / `SymbolId`、有序 overload set、顺序 local 可见性、名称引用产物与
  L0079–L0081；enum case 的值构造器与 type-test 身份共享稳定 `EnumCaseId`，限定 case 尾段
  与 payload 候选也保留在解析产物中。该阶段不读取文件系统、不展开 package/import，
  也不执行类型或控制流判断；
- `lang_frontend::type_checking` 已提供与名称环境身份绑定的显式 `TypeEnvironment`、确定性
  `TypeId` / `NominalId` / typed 产物、builtin / nullable / function / nominal / type-parameter
  类型、泛型替换、interface closure、member contract、override/default 冲突与窄化委托计划，
  并实现数值定型、局部单向 expected type、lambda / 基础运算符 / 返回流检查、enum case
  type、稳定 place flow facts、赋值/capture kill、短路条件传播，以及 Boolean/enum/nullable
  `when` 穷尽性、条件 `Copyable` 四态查询、名义内联递归检查、环境绑定的 intrinsic
  `Box`，局部 value-class 解构的 Copy/Consume descriptor，以及单态 source/external/
  function-value/member callable 选择、源码有序实参映射、`CallDescriptor` 和
  `ExpressionCategory` place/temporary 事实；具名函数与成功采用唯一期望函数类型的 lambda
  还按参数 `SymbolId` 发布 `ParameterBindingDescriptor`，Borrow/Inout lambda 不再被误判为
  全 Value 结构不匹配。环境绑定的 `Array` / `List` / `MutableList`
  identity、storable 元素检查、`ContainerConstructionDescriptor`、带可变性的
  `ElementPlaceDescriptor`、只读 `size` 与封闭 `[]` 规则，覆盖 L0082–L0130；泛型 callable
  实例化、多 overload 候选的 lambda 隔离检查、callable reference、safe-call lifting 与所有权
  可用性仍使用逐类 `DeferredReason` 保留；其中前两项已分别登记为 draft SPEC-0177 /
  SPEC-0174，v0.28 guide 门禁已经解除但实现尚未开始。普通名义主构造器字段已建立带实际
  泛型替换的
  `AggregateProjectionDescriptor`，
  `value class` 在无显式同名 callable 时提供零参数自动 `componentN()` typed target；
  callable 参数只保留 `Value` / `Borrow` / `Inout` 三态 typed identity；无 marker 与显式
  `borrow` 共享 `Borrow` identity，声明侧 `own` 形成 `Value`。预声明只读 API 使用 `Borrow`，
  `Array` / `List` 的 runtime-length 构造器两个参数均为 `Borrow`；预声明 callable 可由
  `EnvironmentFunctionEffect` 为精确参数绑定跨线程交付 effect，成功 call 通过 argument
  descriptor 公开该 identity，源码同名函数不会获得 effect；环境绑定 `Rc<T>` 只建立
  compiler intrinsic 类型身份，runtime API 仍属 Phase 5；
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
  并逆序发布 owned capture drop。存在所有权诊断时不发布 capture/drop plan。顺序容器构造复用 typed 参数模式；intrinsic index 形成
  root + field path + 逻辑索引 identity，支持 Copyable owned read、MoveOnly L0136、element
  shared/exclusive loan、temporary owner 延寿、固定顺序 replacement 与旧元素 drop fact。
  非 intrinsic index、post-index field projection、Phase 5 relocation effect 与
  instance/delegation receiver 的完整所有权契约仍明确 deferred；
- `lang-frontend` 已有 Cargo 实际执行的 Phase 0 source-loading，以及 Phase 1 Lexer 与
  parser-expression、parser-declaration、parser-block、parser-lambda、parser-implicit-unit、
  parser-file pass / fail fixture harness，以及 Phase 2 名称解析和基础/名义类型检查 pass / fail fixture；
- `editors/textmate` 已提供 `source.koven` / `.ko` grammar、正常与 reserved corpus、scope
  expectation，并由 `lang-frontend` integration test 复用生产 Lexer 做漂移回归；
- 尚无泛型 callable 实例化、普通字段部分移动、顺序容器 Phase 5 relocation effect 或
  codegen 实现；
- [ADR-0006](../adr/0006-typed-ssa-block-parameters.md) 已接受 IR-local type、block parameters、
  显式 ownership effect 与独立 verifier 的 typed SSA 架构；对应
  [SPEC-0033](../specs/0033-typed-ssa-ir-verifier.md) 仍是等待 SPEC-0177/0174 的 `draft`，
  `lang-codegen` 当前尚无 SSA model 或 verifier；
- LLVM / `inkwell` 版本、runtime / ABI 和目标平台矩阵仍未确定。

现有 target 只证明工程与 crate 边界可构建，不承诺尚未实现的编译、CLI 或 LSP 行为。

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
- `lang-lsp` → `lang-frontend`；
- `lang-std` 无项目内依赖。

`lang-std` 的 Rust target 仅提供 Cargo 与测试边界，其单元测试验证 `.ko` 源码包存在；标准库
公共实现仍以 `koven/**/*.ko` 为唯一真源。Phase 0 不包含 runtime crate。

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

Lexer 尚未接入 `kovenc` 或 LSP；`LexedFile` 是 Parser 的唯一词法输入，而不是完整编译
产物或公共机器诊断协议。

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
  泛型列表；调用点 `<type_ref, ...>(...)` 由单次 O(N) 反向预索引无副作用判定，查询 O(1)，
  成功后才由正式 TypeRef parser 提交 AST；
- 当前具名函数参数和函数类型参数共享封闭的
  `ParameterModeMarker::{Own, Borrow, Inout}`；无 marker 与显式 `borrow` 都规范化为 `Borrow`，
  `own` 规范化为既有 `Value`，`inout` 保持 `Inout`。函数类型使用内嵌
  `FunctionTypeParameter`，strict typed-call 预索引同步识别三种声明 marker，失败仍不分配
  AST、不发诊断或移动正式 cursor；
- basic、typed、member 与 chained call 统一保存源码有序的内嵌 `CallArgument`：可选命名
  前缀、调用点 `borrow` / `&` marker 和唯一 value 表达式。Parser 只保存 Phase 1 源码结构，
  不做名称映射、契约匹配、place、可变性或所有权检查；
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
  独立 `Statement::LambdaBody`；block element 起点的 `{` 仍是 Unit block，等待 primary 的
  `{` 才是 lambda，因而无需 trivia 或类型猜测即可区分两者；
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
  Parser 自身仍不做名称 / 类型 / 所有权检查，CLI / LSP 接线仍属后续 Phase。

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

## 结构化诊断与 renderer

`lang_frontend::diagnostic` 按
[ADR-0003](../adr/0003-diagnostic-architecture.md) 拥有可供后续前端阶段和 LSP 复用的诊断
语义模型：

- `DiagnosticCodeCatalog` 一次性校验精确 ASCII `Ldddd` 格式和重复编号；只有目录解析出的
  `DiagnosticCode` 才能进入诊断。生产目录 `codes::ALL` 现精确注册 `L0001`–`L0008` 八个
  Lexer 错误码、`L0009`–`L0078` Parser 错误码与 `L0079`–`L0081` 名称错误码；`L0016` 为
  不再由生产 Parser 发出的历史类别，`L9xxx` 样例编号仍只在测试 target 内注册；
- `Diagnostic` 构造时必须接收严重级别、已验证错误码、非空单行主消息和主 `Span`；字段
  私有，主位置缺失不可表示。关联 label、note、help 同样受检，并在一个有序序列中保留
  生产者给出的语义顺序；
- frontend 聚合边界先用共享 `SourceMap` 校验所有主与关联 `Span`，再按主 source 名称、
  范围、严重级别、错误码、主消息和完整附加信息序列建立全序。排序不依赖 `SourceId`、
  source 加载顺序、随机哈希顺序或输入下标；失败返回包含角色与 `SourceError` 的内部错误；
- `lang-cli` 的 `diagnostic_renderer` 是 `kovenc` binary 内的私有纯转换：接收
  `Diagnostic + SourceMap`，返回确定性无颜色文本或 frontend 内部错误，不读取文件、不直接
  写 stdout / stderr。它复用 source 模块的 1-based 位置换算，并只转义 source 名称中的
  反斜杠、CR、LF 来保持单行输出，不做路径发现或规范化。

renderer 当前只由同 target 测试调用；CLI 参数、编译流水线、stderr、颜色、退出码和机器
可读诊断协议均尚未实现。人类可读 Phase 0 文本也不是版本化机器协议。

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
`tests/ownership_checking.rs` 的 14 个 integration test 覆盖 source identity、MoveOnly 与
Copyable 按值交付、Borrow / Inout、重新赋值、temporary、分支 / loop 合流、终止路径、
SymbolId 遮蔽、错误 AST 去级联、参数 binding、place overlap、源码顺序与 nested-call loan、
Inout mutability、ASAP drop matrix、deferred 边界、重复运行确定性和真实 pass / fail fixture；
fixture runner 精确枚举一个正例与一个反例，并核对 L0131、L0133–L0135 的 code 与 primary
byte Span，领域测试另核对冲突来源和 move/declaration label。
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
类型事实也已实现；整变量 MoveOnly / Copyable 状态、use-after-move、消费式 value-class
解构、字段 / 自动结构分量的部分移动拒绝、调用期 loan、owned-value ASAP drop facts 与
顺序容器核心 element place 所有权已由独立 Phase 3 阶段实现；泛型 callable 实例化与
多 overload 候选的 lambda 隔离检查已物化为 draft SPEC-0177 / SPEC-0174；v0.28 guide 门禁
已经解除，当前仍未实现。`object` / `companion object` 关联成员，以及容器
Phase 5 容器 relocation effect 等后续所有权规则仍未实现；
`lang-std` 的 bootstrap 流程与
runtime / ABI 布局仍未确定。

## 更新要求

后续每个 Spec 改变模块关系、数据流或已实现阶段时，必须在同一任务中更新本页。不要在这里
保存决策历史，也不要把尚未批准的设想写成实现事实。
