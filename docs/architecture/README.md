# Koven 架构快照

本目录描述仓库**当前已经实现**的架构。设计原因记录在 [`../adr/`](../adr/)，单次交付范围
记录在 [`../specs/`](../specs/)，语言语义由
[`../guide/00-index.md`](../guide/00-index.md) 导航的现行 v0.25 文档集定义。class-family 与
窄化接口委托已分别由 SPEC-0017、SPEC-0064 实现；SPEC-0018 已建立单文件名称解析，
SPEC-0019 已建立基础类型检查，SPEC-0020 已建立名义/泛型/interface 类型检查。
SPEC-0021 已建立 enum case type、`when` 穷尽性与 flow-sensitive smart cast；SPEC-0022 已
建立条件 `Copyable`、有限内联布局、intrinsic `Box` 与结构化解构类型事实；SPEC-0067 已
建立单态 callable/member 选择、实参映射与类型层面 place 分类；SPEC-0023 已建立顺序容器
类型、核心构造和 element-place 类型事实；SPEC-0058 已提供独立 TextMate grammar 与由生产
Lexer 校验的高亮回归 corpus；SPEC-0059 已提供 Tree-sitter grammar、生成 parser、外部
identifier scanner、原生 corpus 与生产前端交叉验收。

## 当前状态

仓库已完成 Phase 0 与 Phase 1，并已进入 Phase 2。工程骨架按
[ADR-0002](../adr/0002-bootstrap-workspace-layout.md) 建立，当前已实现：

- 根目录是 resolver 3 的 virtual Cargo workspace；所有 package 使用 Rust edition 2024，
  toolchain pin 和初始 MSRV 均为 `1.96.0`，并在许可与发布策略确定前保持不可发布；
- 五个 workspace member 均有 Cargo 可识别的 target，依赖方向单向且无环；
- `lang_frontend::source` 已提供统一 source / `Span` 基础设施；
- `lang_frontend::diagnostic` 已提供结构化诊断模型、`L0001`–`L0130` 正式前端错误码与
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
  `ExpressionCategory` place/temporary 事实，以及环境绑定的 `Array` / `List` / `MutableList`
  identity、storable 元素检查、`ContainerConstructionDescriptor`、带可变性的
  `ElementPlaceDescriptor`、只读 `size` 与封闭 `[]` 规则，覆盖 L0082–L0130；泛型 callable
  实例化、callable reference、safe-call lifting 与所有权可用性仍使用逐类 `DeferredReason`
  保留；
- `lang-frontend` 已有 Cargo 实际执行的 Phase 0 source-loading，以及 Phase 1 Lexer 与
  parser-expression、parser-declaration、parser-block、parser-lambda、parser-implicit-unit、
  parser-file pass / fail fixture harness，以及 Phase 2 名称解析和基础/名义类型检查 pass / fail fixture；
- `editors/textmate` 已提供 `source.koven` / `.ko` grammar、正常与 reserved corpus、scope
  expectation，并由 `lang-frontend` integration test 复用生产 Lexer 做漂移回归；
- 尚无泛型 callable 实例化、所有权状态检查或 codegen 实现；
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
- 具名函数参数和函数类型参数共享封闭的 `ParameterModeMarker::{Borrow, Inout}`；无 marker
  表示 `Value`。函数类型使用内嵌 `FunctionTypeParameter`，strict typed-call 预索引同步识别
  `borrow` / `inout` marker，失败仍不分配 AST、不发诊断或移动正式 cursor；
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
- Lexer pass case 调用生产 `lex` 并验证零诊断、source identity、唯一 EOF，以及全部非 EOF
  lexeme 对输入字节的连续完整覆盖；
- Lexer fail case 按规范相对 stem 将 `.ko` 与 `.diag` 一一配对；sidecar 每行严格使用
  `Ldddd<TAB>start_byte<TAB>end_byte`，只接受已注册生产码、LF / CRLF、十进制非空半开
  UTF-8 字节范围，并与生产诊断全序逐项全等。缺失 / 孤立 sidecar 和非法格式都使 suite
  失败；该 sidecar 是仓库私有测试格式，不是公共诊断协议；
- Parser pass case 以生产 Lexer 与独立表达式入口验证零诊断、根有效及完整消费到 EOF；Parser fail
  case 复用同一 sidecar 契约，逐项核对合并后的 Lexer / Parser 诊断全序；现有 expression
  suite 已加入命名 / `borrow` / `&` 实参与 `L0033` 缺值证据；
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
  笛卡尔积，逐例验证 lexeme 完整字节覆盖、唯一 EOF、source identity、诊断及四张 AST table
  span、文件头和根指纹的重复解析确定性；另以终止字符位于 interpolation 的定向回归锁定
  `L0007` 根因和 lexical-owner 恢复；
- `lexer_boundary_matrix` integration test 经生产 Lexer 执行 2,127 个固定 case：330 个硬/
  未来保留/软词 ASCII identifier 边界类别、1,596 个普通固定符号相邻 spelling、199 个 `as?` /
  `!in` / `!is` continuation 与终止边界，以及 2 个注释优先级 case；每例锁定目标分类或
  最长首 token、精确 Span、连续完整字节覆盖和唯一 EOF，且不复制 scanner 的匹配顺序；
- `parser_entry_adversarial` integration test 对 16 个前缀与 16 个后缀分别运行独立 expression、
  declaration、block 三个公开入口，共执行 768 个 entry/case、1536 次生产解析；每个 case
  比较公开稳定 `Debug` 骨架以锁定 root typed ID、AST table 插入顺序与 source span、诊断的
  重复运行确定性，且不引入随机、IO 或第三方 property-testing 依赖；payload 边仍由精确
  领域测试验收；
- `parser_operator_matrix` integration test 经生产 Lexer 与公开 expression 入口执行 240 个
  固定 case：110 个表达式右操作数中缀层双向组合、36 个 postfix/prefix/cast 高层组合、
  54 个结合性组合和 40 个不结合组成员组合；结构断言锁定低优先级根与高优先级子树，
  `L0012` 断言锁定第二个不结合运算符的精确 byte span，且不复制生产 binding-power 数值；
- `parser_token_inventory` integration test 自检 120 个互异片段，覆盖全部 42 个 Keyword、
  11 个 ReservedWord、43 个 Symbol、literal/string/interpolation、四类 trivia 与 L0001–L0008；
  四个公开 Parser 入口共执行 480 个 entry/case、960 次重复解析，锁定 lexeme 完整覆盖、公开
  AST/诊断确定性和无普通用户输入内部错误，并精确回归独立声明完整 string 的 L0017/Span；
- `parser_lexical_owner_matrix` integration test 把 4 个可继续 owner 与 5 个 EOF terminal owner
  分别投放到 16 个声明、名称、类型、class-family 和表达式位置，共执行 144 个 case、288 次
  重复完整文件解析；逐例锁定连续 lexeme 覆盖、唯一 EOF、词法错误码、公开产物确定性，并
  要求 64 个可继续 case 后的顶层 sentinel 声明全部存活；
- `parser_diagnostic_witness_matrix` integration test 将生产目录 `L0009`–`L0078` 中 69 个现行
  Parser 诊断逐一映射到 expression、declaration、block 或 file 公开入口；每个 Lexer-clean
  witness 的目标码恰好出现一次，全部 Span 保持 source-local，69 个 case 共执行 138 次确定性
  解析。兼容保留但生产 Parser 已退役的 L0016 被显式排除，矩阵同时证明所有实际诊断均不发该码；
- `parser_trivia_invariance_matrix` integration test 以 20 个完整 grammar case 覆盖文件头、声明、
  类型、表达式、call/lambda、control-flow 与 class-family，把 tab、无换行 block comment 和
  混合 trivia 投放到每个单独 token gap、全部 gap 及文件首尾；1,175 个源码变体、2,350 次
  完整文件解析均保持 significant `LexemeKind` 序列与无 Span AST 结构指纹不变且零诊断；
- `parser_line_break_boundary_matrix` integration test 将 LF、CRLF、line comment 终止换行及
  block comment 内 LF / CRLF 六个结构载体，与四个合法非换行 trivia 载体投放到文件头、
  顶层声明、class member、`when` entry 和 `return` 边界；另锁定 enum comma 与中缀连续性，
  共执行 80 个 Lexer-clean 源码、160 次确定性完整文件解析；源码裸 CR 仍由 Lexer 以 L0001 拒绝；
- `parser_prefix_truncation_matrix` integration test 以 22 个 Lexer / Parser-clean 完整文件覆盖
  文件头、声明、callable、block、lambda、control-flow、postfix、class-family、接口委托、
  运算符层级及 Unicode 嵌套 string / interpolation；其 1,373 个 UTF-8 scalar 前缀均保持
  lexeme 完整覆盖、唯一末尾 EOF、有界诊断 / AST Span，并完成两次确定性完整文件解析；
- `parser_token_omission_matrix` integration test 复用同一 22-file corpus，逐一删除原始范围内
  396 个显著 token；96 个 owner-affecting case 锁定总性，300 个非 owner case 还要求后置
  `val sentinel = 0` 保持最后顶层 Item。全部 case 重复解析、验证完整 lexeme 覆盖和有界
  诊断 / AST Span，并定向回归 class member closer 与 nested interpolation tail 两个恢复缺陷；
- runner 返回只包含规范相对路径和稳定证据 / 失败类别的结构化 outcome。测试报告
  边界转义路径中的反斜杠、tab、CR 和 LF，不输出 fixture 根的绝对路径。

`source-pass` 仍只表示 Phase 0 基础设施接线成功；Phase 1 suite 分别调用扫描器、独立表达式、
独立声明、独立 block、lambda expression、具名函数隐式 `Unit` 与完整文件 Parser。这些
suite 不表示类型检查或编译，harness 也不调用 renderer 或固定公共机器诊断协议。
`tests/name_resolution.rs` 另行枚举非零 Phase 2 `name-pass` / `name-fail` fixture，真实调用
Lexer、完整文件 Parser 与名称解析入口，并精确核对 L0079–L0081 的 code / byte Span；
`tests/type_checking.rs` 枚举 `type-pass` / `type-fail` fixture，经相同前置流水线调用类型检查，
并精确核对 L0082–L0130 的 code / byte Span；当前 `type-pass` 与 `type-fail` 各有六个真实
fixture，包含名义类型、interface 实现、override、委托、`when`/smart-cast、`Copyable`/
结构化解构、callable 和顺序容器正反例。

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
验证全部正例的生产 Lexer 分类或零诊断。TextMate `package.json` 只提供 `npm test` 脚本，不含
依赖或 lockfile，也不把 Node 引入 Cargo 测试。

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
`value` / `async` 被拒绝，而 `className` / `asyncTask` 仍按完整词边界成为 identifier。
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
类型事实也已实现；泛型 callable 实例化、`object` / `companion object` 关联成员和后续
所有权规则仍未实现；
`lang-std` 的 bootstrap 流程与
runtime / ABI 布局仍未确定。

## 更新要求

后续每个 Spec 改变模块关系、数据流或已实现阶段时，必须在同一任务中更新本页。不要在这里
保存决策历史，也不要把尚未批准的设想写成实现事实。
