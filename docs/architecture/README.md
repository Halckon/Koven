# Koven 架构快照

本目录描述仓库**当前已经实现**的架构。设计原因记录在 [`../adr/`](../adr/)，单次交付范围
记录在 [`../specs/`](../specs/)，语言语义由
[`../agent-language-design-guide-v0.7.md`](../agent-language-design-guide-v0.7.md) 定义。

## 当前状态

仓库已完成 Phase 0 并进入 Phase 1。工程骨架按
[ADR-0002](../adr/0002-bootstrap-workspace-layout.md) 建立，当前已实现：

- 根目录是 resolver 3 的 virtual Cargo workspace；所有 package 使用 Rust edition 2024，
  toolchain pin 和初始 MSRV 均为 `1.96.0`，并在许可与发布策略确定前保持不可发布；
- 五个 workspace member 均有 Cargo 可识别的 target，依赖方向单向且无环；
- `lang_frontend::source` 已提供统一 source / `Span` 基础设施；
- `lang_frontend::diagnostic` 已提供结构化诊断模型、`L0001`–`L0016` 正式前端错误码与
  确定性聚合顺序，`kovenc` binary 内已有尚未接入编译流水线的最小纯文本 renderer；
- `lang_frontend::ast` 已提供四类 typed ID 与带 `Span` 的通用索引存储骨架；
- `lang_frontend::lexer` 已提供覆盖 v0.6 沿用词法契约的确定性扫描、完整 lexeme 流与
  结构化恢复诊断；
- `lang_frontend::parser` 已提供 v0.6 独立表达式入口、具体 Expression / TypeRef 索引式 AST、
  Pratt 优先级与局部恢复，并确定性合并 Lexer / Parser 诊断；
- `lang-frontend` 已有 Cargo 实际执行的 Phase 0 source-loading，以及 Phase 1 Lexer 与
  parser-expression pass / fail fixture harness；
- 尚无声明 / 控制流 / lambda / 全文件 parser、类型检查、所有权检查或 codegen 实现；
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
- scanner 实现 ASCII 标识符、42 个硬关键字、2 个仍按 identifier 输出的软关键字、11 个
  reserved-word token、十进制数字、`Char`、单行 `String` / `${...}` 插值、trivia 与固定
  符号最长匹配；扫描只使用标准库，没有新增依赖；
- 字符串与插值使用显式模式栈，只有插值普通模式中的花括号改变嵌套深度。非法输入始终
  前进并形成规范规定的 token / invalid / segment 形态；未终止模式按最内层错误抑制规则
  恢复，不对正常用户输入 `panic!`；
- `TokenKind`、`Keyword`、`ReservedWord`、`Symbol`、`TriviaKind` 与 `InvalidKind` 是 Lexer
  面向后续 Parser 的最小分类 API；它们只表达词法拼写，不提前判断语法位置或运算符语义。

Lexer 尚未接入 `kovenc` 或 LSP；`LexedFile` 是 Parser 的唯一词法输入，而不是完整编译
产物或公共机器诊断协议。

## Pratt 表达式 Parser

`lang_frontend::parser::parse_expression(&SourceMap, &LexedFile)` 校验 map-local source identity，
并返回拥有 `ExpressionAst`、唯一根 `ExpressionId` 与两阶段诊断全序的 `ParsedExpression`。
普通语法错误进入产物；跨 map、lexeme 流、AST、诊断模型不变量或实现资源预算失败才返回
具体内部错误。

- Parser 跳过 trivia，消费 v0.6 的 primary、postfix、prefix、14 档中缀 / 赋值和递归
  `type_ref`；binding power 只在 `parser::engine` 中定义，`to` 由源码 `Span` 精确识别；
- `Expression` 与 `TypeRef` payload 只通过现有 typed ID 连接，叶与合成节点都保留同一
  `SourceId` 的 UTF-8 字节 `Span`；源码拼写继续由共享 `SourceMap` 回查；
- Lexer invalid / reserved token 被消费为显式 Error 节点且不重复同源诊断；delimiter、插值
  stop、不结合链、尾随 token 和未支持实参按 `L0009`–`L0016` 做局部确定性恢复；
- 递归 Pratt 实现在固定 32 MiB 的 scoped worker 隔离栈上运行，并在 1024 个内部递归预算
  单位处返回具体资源错误；这避免调用线程的小栈或输入 token 数放大栈申请，也不新增未经
  guide 分配的用户诊断码；
- 入口只解析独立表达式。声明、控制流、lambda、完整文件恢复、名称 / 类型 / 所有权检查
  以及 CLI / LSP 接线分别留给 SPEC-0008 至 SPEC-0011 及后续 Phase。

## 结构化诊断与 renderer

`lang_frontend::diagnostic` 按
[ADR-0003](../adr/0003-diagnostic-architecture.md) 拥有可供后续前端阶段和 LSP 复用的诊断
语义模型：

- `DiagnosticCodeCatalog` 一次性校验精确 ASCII `Ldddd` 格式和重复编号；只有目录解析出的
  `DiagnosticCode` 才能进入诊断。生产目录 `codes::ALL` 现精确注册 `L0001`–`L0008` 八个
  Lexer 错误码与 `L0009`–`L0016` 八个 Parser 错误码；`L9xxx` 样例编号仍只在测试 target
  内注册；
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

生产模块已用 `ExpressionAst = AstFile<(), (), Expression, TypeRef>` 定义具体表达式与类型引用
payload；item / statement 仍为空占位。尚无 visitor、HIR / MIR、名称解析结果或 LLVM /
codegen handle。

## 语言 fixture harness

`crates/lang-frontend/tests/fixtures.rs` 是 Cargo 自动发现的 `fixtures` integration test target。
它分别运行五个固定 suite：Phase 0 `source-pass/`，Phase 1 `lexer-pass/`、`lexer-fail/`、
`parser-expression-pass/` 与 `parser-expression-fail/`。

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
  case 复用同一 sidecar 契约，逐项核对合并后的 Lexer / Parser 诊断全序；
- runner 返回只包含规范相对路径和稳定证据 / 失败类别的结构化 outcome。测试报告
  边界转义路径中的反斜杠、tab、CR 和 LF，不输出 fixture 根的绝对路径。

`source-pass` 仍只表示 Phase 0 基础设施接线成功；Phase 1 suite 分别调用扫描器与独立表达式
Parser。这些 suite 不表示完整源文件已解析、类型检查或编译，harness 也不调用 renderer 或
固定公共机器诊断协议。

## 尚未实现的编译流水线

现行 guide 要求的流水线仍是计划边界：

```text
源码 → Lexer → Parser / 索引式 AST → 名称与类型检查 → 所有权检查
     → 自建 SSA IR → LLVM IR → 目标文件 → 本机可执行文件
```

其中 `lang-frontend` 不依赖 LLVM / `inkwell`，LLVM 细节后续只能收敛在 codegen 边界。
索引式 AST 存储、结构化诊断基础设施、Lexer、独立表达式 Parser 与分层 fixture harness 已
存在；声明至完整文件的 Parser 阶段仍未实现。`lang-std` 的 bootstrap 流程与 runtime / ABI
布局仍未确定。

## 更新要求

后续每个 Spec 改变模块关系、数据流或已实现阶段时，必须在同一任务中更新本页。不要在这里
保存决策历史，也不要把尚未批准的设想写成实现事实。
