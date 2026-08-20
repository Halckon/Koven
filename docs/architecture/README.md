# Koven 架构快照

本目录描述仓库**当前已经实现**的架构。设计原因记录在 [`../adr/`](../adr/)，单次交付范围
记录在 [`../specs/`](../specs/)，语言语义由
[`../guide/00-index.md`](../guide/00-index.md) 导航的现行 v0.20 文档集定义。class-family 与
窄化接口委托已分别由 SPEC-0017、SPEC-0064 实现；名称和类型检查尚未开始。

## 当前状态

仓库已完成 Phase 0 并进入 Phase 1。工程骨架按
[ADR-0002](../adr/0002-bootstrap-workspace-layout.md) 建立，当前已实现：

- 根目录是 resolver 3 的 virtual Cargo workspace；所有 package 使用 Rust edition 2024，
  toolchain pin 和初始 MSRV 均为 `1.96.0`，并在许可与发布策略确定前保持不可发布；
- 五个 workspace member 均有 Cargo 可识别的 target，依赖方向单向且无环；
- `lang_frontend::source` 已提供统一 source / `Span` 基础设施；
- `lang_frontend::diagnostic` 已提供结构化诊断模型、`L0001`–`L0078` 正式前端错误码与
  确定性聚合顺序，`kovenc` binary 内已有尚未接入编译流水线的最小纯文本 renderer；
- `lang_frontend::ast` 已提供四类 typed ID 与带 `Span` 的通用索引存储骨架；
- `lang_frontend::lexer` 已提供覆盖 v0.17 词法契约的确定性扫描、完整 lexeme 流与
  结构化恢复诊断，包括保持 `&&` 最长匹配的单字符 `&`、顶层分隔用 `;`，以及以
  `package` 取代 `module` 的 42 个硬关键字；
- `lang_frontend::parser` 已提供独立表达式、声明与 block 入口、具体 Item / Statement /
  Expression / TypeRef 索引式 AST、Pratt 优先级、typed call、callable 参数 marker、结构化
  `CallArgument`、函数 block body、lambda、具名函数隐式 `Unit` 返回标注、`package` /
  Kotlin 风格 `import` 文件头、`if` / `when`、loop-family、jump、`super`、class-family
  及局部恢复，并
  确定性合并 Lexer / Parser 诊断；
- `lang-frontend` 已有 Cargo 实际执行的 Phase 0 source-loading，以及 Phase 1 Lexer 与
  parser-expression、parser-declaration、parser-block、parser-lambda、parser-implicit-unit、
  parser-file pass / fail fixture harness；
- 尚无名称/类型检查、所有权检查或 codegen 实现；
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
- `Expression` 与 `TypeRef` payload 只通过现有 typed ID 连接，叶与合成节点都保留同一
  `SourceId` 的 UTF-8 字节 `Span`；源码拼写继续由共享 `SourceMap` 回查；
- Lexer invalid / reserved token 被消费为显式 Error 节点且不重复同源诊断；delimiter、插值
  stop、不结合链与尾随 token 使用既有 `L0009`–`L0015`，typed call argument 与参数 marker
  使用 `L0033`–`L0039` 做 owner-aware 局部恢复，局部解构使用 `L0040`–`L0046`，control-flow
  使用 L0055–L0065，class-family 使用 L0066–L0077，委托目标使用 L0078。postfix `?` 不需要新错误类别，缺左
  operand 继续使用 L0009；
  已发布的 `L0016` 仅保留在 catalog，生产
  Parser 不再发出；
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
  后续拆分和顺序见 [Spec 路线图](../specs/README.md)；
  名称 / 类型 / 所有权检查以及 CLI / LSP 接线仍属后续 Phase。

## 结构化诊断与 renderer

`lang_frontend::diagnostic` 按
[ADR-0003](../adr/0003-diagnostic-architecture.md) 拥有可供后续前端阶段和 LSP 复用的诊断
语义模型：

- `DiagnosticCodeCatalog` 一次性校验精确 ASCII `Ldddd` 格式和重复编号；只有目录解析出的
  `DiagnosticCode` 才能进入诊断。生产目录 `codes::ALL` 现精确注册 `L0001`–`L0008` 八个
  Lexer 错误码与 `L0009`–`L0078` Parser 错误码；`L0016` 为不再由生产 Parser 发出的历史
  类别，`L9xxx` 样例编号仍只在测试 target 内注册；
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
四张 typed table 内节点的封闭内嵌 payload，没有新增第五张 table。尚无 visitor、HIR / MIR、
名称解析结果或 LLVM / codegen handle。

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
- runner 返回只包含规范相对路径和稳定证据 / 失败类别的结构化 outcome。测试报告
  边界转义路径中的反斜杠、tab、CR 和 LF，不输出 fixture 根的绝对路径。

`source-pass` 仍只表示 Phase 0 基础设施接线成功；Phase 1 suite 分别调用扫描器、独立表达式、
独立声明、独立 block、lambda expression、具名函数隐式 `Unit` 与完整文件 Parser。这些
suite 不表示类型检查或编译，harness 也不调用 renderer 或固定公共机器诊断协议。

## 尚未实现的编译流水线

现行 guide 要求的流水线仍是计划边界：

```text
源码 → Lexer → Parser / 索引式 AST → 名称与类型检查 → 所有权检查
     → 自建 SSA IR → LLVM IR → 目标文件 → 本机可执行文件
```

其中 `lang-frontend` 不依赖 LLVM / `inkwell`，LLVM 细节后续只能收敛在 codegen 边界。
索引式 AST 存储、结构化诊断基础设施、Lexer、独立表达式 / 声明 / block / lambda Parser、
callable 参数与 typed call argument、局部解构、完整文件与 package / import Parser、
control-flow、class-family、窄化接口委托、具名函数隐式 `Unit` 返回标注及分层 fixture
harness 已存在。`lang-std` 的 bootstrap 流程与
runtime / ABI 布局仍未确定。

## 更新要求

后续每个 Spec 改变模块关系、数据流或已实现阶段时，必须在同一任务中更新本页。不要在这里
保存决策历史，也不要把尚未批准的设想写成实现事实。
