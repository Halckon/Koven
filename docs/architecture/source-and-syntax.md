# Source、Lexer、Parser 与 AST

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改 Source、Lexer、Parser、AST 或恢复逻辑时 · **唯一真源**：`lang-frontend` 代码与测试

## Source 与 Span

`source::SourceMap` 拥有不可变 UTF-8 源码、用户可见名称和预计算行起点。`SourceId` 是 map-local
身份；可选的宿主文件名元数据与用户可见逻辑名称独立，未提供时回退到逻辑名。
该元数据不读文件系统、不参与名称唯一性或 SourceId 分配；`Span` 是同一 source 上的 `[start, end)` 半开字节范围，只能通过受检 API 创建。

- 重复 source 名称、foreign `SourceId`、逆序范围、越界和非 UTF-8 边界返回具体 `SourceError`。
- 行列只在展示边界派生；行为覆盖 LF、CRLF、空文件、EOF 和 Unicode scalar 列。
- Lexer、Parser、诊断和 LSP 共用 SourceMap 的切片与位置换算，不各自维护行表。

实现入口是 `crates/lang-frontend/src/source.rs`；对应覆盖位于 `source_span` integration suite。

## 确定性 Lexer

`lexer::lex(&SourceMap, SourceId)` 返回 `LexedFile`。产物保存 source identity、有序 lexeme、唯一 EOF
和已排序诊断；除 EOF 外的 lexeme 按顺序无重叠地覆盖完整输入。

Lexer 负责现行关键字与保留字、标识符、数字后缀、`Char`、单行 String/interpolation、comment、
trivia 和固定符号最长匹配。它只分类拼写，不解析数值范围，也不决定语法位置、类型或所有权。
字符串插值通过显式模式栈扫描；非法输入必须前进并形成结构化诊断，不能在用户输入路径 panic。

`parser_long_invalid_number_boundaries` 按现行radix规则保留6类长非法数字×4公共入口的
完整诊断/恢复矩阵，并有4种长hex/bin表达式正例；词法/解析接受不代表后续数值范围合法。
`parser_token_inventory` 把上下文/软词列为Identifier，保留旧120片段并扩到127个唯一片段，
四入口重复解析、诊断族与产物保真仍在同一target。对应修复及Mac/Linux证据边界见
[SPEC-0277](../archive/specs/0277-p2-evidence-test-alignment.md)；未改变生产规则。

实现入口是 `crates/lang-frontend/src/lexer/`；对应覆盖位于 `lexer`、`lexer_boundary_matrix` 和
`lexer_stress_matrix` integration suites。

## Parser 入口与模块

语言接受条件见 [Guide](../guide/README.md)，已分离的内部表示与资源要求见
[Compiler Contracts](../compiler-specs/README.md)；下文只记录代码与测试的当前事实。

`parser::parse_expression`、`parse_declaration`、`parse_block` 和 `parse_file` 都接收同一
`SourceMap + LexedFile`，并共享 `SyntaxAst`、诊断顺序和资源边界。`parser/mod.rs` 是公开门面：

- `syntax.rs` 定义具体 AST payload；
- `output.rs` 定义四类解析产物；
- `engine.rs` 持有唯一 cursor、AST、诊断和恢复状态；
- `engine/` 子模块按 file、declaration、class、block、expression、postfix、operator、TypeRef 和
  recovery 等语法职责拆分；
- `trial.rs` 与 `lambda_trial.rs` 只做无副作用预索引，正式分支成功后才提交 AST。

TypeRef 的上下文头部由正式解析与 strict call trial 保持一致：`move` 仅在后继 `(` 时是
函数类型前缀；函数类型参数模式可先于普通或嵌套函数 TypeRef，具名参数仍必须先有名称。
单独的 own/borrow/inout/move 类型名称不被无条件消费。重复参数模式的恢复只保留首 marker，
失败 typed-call 候选不提交 TypeRef；共享线性预算与原有递归上限保持。

调用实参的显式模式只识别 `&`；`borrow(x)`、`borrow (x)`、member/typed call 与尾 lambda
按普通表达式解析，`borrow` 不再由后继 token 推断成调用 marker。旧 `f(borrow x)` 沿
L0034 separator 恢复，在 `x` 起点报告空 Span，保留两个值与后继实参的源码身份。
声明与函数类型的 Borrow 模式不变；对应直接证据为 `parser_call_argument`。

Pratt binding power 只有一个实现来源。Parser 保存参数 marker、调用实参、receiver、尾 lambda、
隐式 `it` anchor、control-flow、class-family、package/import、解构和错误传播等源码结构；名称映射、
类型选择和所有权检查留给后续阶段。

N1a 声明前置由 `engine/declaration_head.rs` 复用正式 TypeRef 解析扩展 receiver，保存
receiver 类型与分隔点 Span；顶层显式 mode 限于 borrow/own，inout、普通函数 mode、成员
扩展和扩展属性仍有结构化拒绝。`FunctionResultSource` 区分普通存储借用与新 carrier
来源，不伪造 borrow Span。现行 Guide v0.46 继承的 compiler-bound View 与可信标准库
List/View Borrow receiver 已接通 single/unit 类型选择、实际来源、静态 ABI 与 native；
未授权声明及未证明路径仍由 L0164/结构化 unsupported 封闭，详见[范围 carrier](range-carrier.md)。
Parser 直接覆盖为 `parser_n1a_frontier`；`type_declaration_frontier` 在 MemoryDocument/Rooted 严格链验证未授权声明拒绝，
类型与来源接线由 `type_range_*` / `ownership_range_*` 验证，普通借用回归保持。

错误恢复遵守 lexical owner、delimiter 和声明/block 边界；恢复必须单调前进。递归语法在隔离 worker
栈和固定递归预算内执行，资源边界以内部错误返回。

class-family 成员体及非法嵌套 class、companion 的恢复共用该预算。委托目标的字符串错误
区由声明恢复状态机只消费该 lexical owner。枚举 payload 的显式分号也穿过嵌套 TypeRef
保留给枚举成员区，companion 常量恢复保留成员分号与右花括号。
类声明头的构造器、类型参数和父类型有单独的类体边界。解析到调用方的真实 `{` 或 `}`
时保留给对应声明体，类型内部仍按 TypeRef 的规则处理。非法构造器默认值保留平衡
lambda 的闭合符，成员解析按 classifier 分别验证 companion 唯一性。
类体恢复回到本地 owner baseline 后识别 receiver 软修饰词，交正式成员解析保留 marker
和顺序诊断。非法嵌套类缺名称时，父类体仍消费真实 `}`。
文件级类声明头可把下一条文件声明交还给文件解析。构造器和分组委托共用按需建立的普通
分组配对，保留已闭合构造器的内部错误区，以及已闭合委托分组之外的父类型和类体。
普通分组的语法尾部恢复核对分组与最近外层的定界符配对，并用同一扫描记录
调用方可能拥有的声明、块和 `else` 停止点。确认边界后才收回内部逗号或尾部 token。
父类型槽位以 invalid 或保留字起始时，原恢复区在父类型列表内保留为 `TypeRef::Error`。
词法诊断仍是该 token 的根因，逗号后父类型、类体及后继文件声明沿用既有 owner。
非法委托目标已有 Error marker 时，错误目标继续持有真实闭合的角列表与普通组，
并更新委托和父类型项的源码末端。无证角列表保留原外逗号规则，文件完整声明前缀和真实
外层成员交回调用方。同行 `deinit()` 只有真实类体起点才能作为外层成员停止。
合法目标后的词法错误区由父类型项单独保存，目标 marker 与委托
子句仍保留原有源码范围。父类型与委托的尾随词法错误共用声明 owner 和组配对，
组内逗号不提升为父类型分隔符。委托的剩余错误区从未证明的 `<` 起始时，保留原外逗号
停止规则，文件、成员及已配对组仍使用同一套边界。尾随入口和连续坏字符串前缀按 raw
token 下标查询同一惰性字符串错误标记。
已有合法目标后的非词法非法尾部也复用闭合分组和角列表证明。显式 lambda header、配对
花括号及后续父类型逗号或类体 opener 共同证明 lambda 尾部。前瞻可跨过连续同行的配对
调用组，每个 `(` 都须与前一闭合符同行。主恢复只在分隔符基线采用该证明。
直接尾随 lambda 须与目标同行，已有非法运算符后的 lambda 可换行。L0077 仍定位首个
非法 token，原目标和委托范围保持。
目标前后的 Member 换行边界识别完整软前缀并交还外层。
此路径的析构前缀还需完整的 `deinit() {`，因此同行调用形状与后接逗号的
`deinit()` 留在错误尾部，原类体和后继父类型各自保留。

文件级类类型参数缺 `>` 时，普通与词法 owner 回到当前 `<` 的基线后，文件解析接回
显式分号或换行后的完整声明前缀。前缀包括 `value class`、`borrow fun` 等两个有效
token 形成的软关键字序列。嵌套 TypeRef 退栈复用同一边界和原始 token 位置，分组只
单向扫描本列表已经推进的前缀。上一文件根遗留的 opener 不参与当前列表配对。
同一文件级恢复在普通组已配对时保留组内 `>`，真实 closer 同步收回本地栈，插值 closer
只能作用于自己的词法 owner。配对候选按各 opener 的局部上下文惰性索引。
父类型槽位及尾随错误区使用类型列表的真实 `>`，外部逗号和类体仍由原解析器接管。
严格试探无法识别时，声明恢复惰性复用类型结构，仅把单个非法字符或保留字视为损坏的
类型原子，另存已闭合候选。嵌套 TypeRef 元素及普通 typed-call 查询仍使用严格结果。
闭合父类型后，词法错误区由父类型项单独保存。已证明的角列表和已配对普通组保护内部
标点，未配对普通组不会占用后继文件或外层成员的起点。嵌套 classifier 显式携带成员
owner，成员分号仍交给外层。分段字符串沿用 Lexer 的终止事件。
父类型内部的泛型实参与函数类型参数按自身 delimiter 恢复，原 TypeRef 与消费的错误区
分别保存。已证明的函数参数 closer 约束错误扫描和跳转，显式分号保留给声明调用方。

Block 与 lambda body 的 dispatch 每轮复用当前 lexeme 判断 closer、hard stop 和分号，避免
在同一 trivia 区域重复扫描；分号不生成 statement，caller hard closer 保留给外层 owner。

普通/control/nested block 与 lambda body 的顶层 Pratt/postfix 在完整左表达式之后遇换行的 `(`、`+`、`-`
时归还 block dispatch。局部解构的真实 `=` 后允许 control/lambda primary，并将外层分号交给 body owner。group/call/index 内的 block soft stop
已清除；for header 只在真实 opener 存在时清软 stop，因此未闭合 delimiter 内继续解析。解构缺内层 `)` 时保留 `in` 和外层停止 token，外层边界上的错误节点使用空 Span。
typed-call 试探、独立 expression 与顶层
initializer 保持既有入口行为。lambda 顶层 body 与内嵌普通 block 保留独立 owner，但共用换行
边界。三个 body dispatch 的分隔校验在后继 element 起点以空 Span 报 L0169；恢复保留真实
后继，不补造分号。已有缺 operand/poison 错误不追加同根分隔诊断。

`return` 的同行操作数起点允许既有 control primary（含 `if` / `when`），不把 block 的
下一 element soft stop 误用为缺值判断；已开始解析操作数后仍传递原 stops。换行、分号、
`else`、caller delimiter 与 EOF 保留原 owner 边界，缺 else 的 returned if 仍为 value-context
L0057。相关结构、范围和资源回归见 `parser_return_control` 与 parser 内部预算测试。

普通表达式分组遇到前置或尾随 lexer poison 时，只在同一词法作用域证明真实右括号后
收回错误尾部。分号、声明 starter 和嵌套异形 closer 仍在实际分组的 Error 子节点内。
每个插值拥有独立配对栈。缺少配对证明时保留外层恢复边界。
下标恢复复用普通配对索引。坏 key 留下的嵌套右方括号不会冒充下标的 closer。
没有配对时把外层逗号交回调用方，文件内恢复仅允许已配对的局部分组遮蔽声明边界。
块分号停止位传入嵌套下标。配对下标与逗号后的局部组恢复查询索引，
保留后接明确块语句起点的分号。配对花括号内的分号仍由本地块拥有。
Error 接收者的后缀恢复共享这些边界。块分号留给块分派，明确 header 且配对的 lambda
留在局部。严格确认的 typed-call 类型参数列表可整体跳过，其中的逗号留在局部。
未配对下标只在相邻普通词法错误之后能确认外层停止点时收回该错误段。
缺右方括号在外层起点使用零宽范围，同行左花括号不作为这条恢复的归属证据。

Parser 覆盖按语法领域位于 `crates/lang-frontend/tests/parser_*.rs`，matrix suites 覆盖恢复和资源
边界。测试选择规则见[开发测试指南](../development/testing.md)。

`parse_super` 在 interface、dot、member-name 的 Lexer poison 处消费并保存 Error，不追加同根
语法诊断。共享 error-postfix 恢复保留 caller stops 与显式分号；`parser_super_recovery` 逐值核对
后继实参和 statement 的身份。`parser_statement_separators` 覆盖四公共入口、分隔及续行；本轮复审边界见
[SPEC-0392](../specs/active/0392-compiler-layered-review.md)，不以局部恢复修复声称语法完全一致。

## 索引式 AST

`ast::AstFile<Item, Statement, Expression, TypeRef>` 拥有四张按插入顺序增长的 typed table。
`ItemId`、`StatementId`、`ExpressionId` 和 `TypeRefId` 不能互换，也没有公开裸下标构造或 unchecked
lookup。

- 每个 `AstNode<T>` 持有 payload 与 `Span`；跨 source 插入在修改 table 前失败。
- table 只追加，不删除或重排；已分配 ID 在该 AST owner 内稳定。
- 节点间只通过 typed ID 连接；源码拼写通过 SourceMap 回查。
- 名称、类型、所有权和 codegen 事实保存在独立产物中，不写回 AST。

实现入口是 `crates/lang-frontend/src/ast.rs` 与 `parser/syntax.rs`；对应覆盖位于 `indexed_ast`
integration suite。
