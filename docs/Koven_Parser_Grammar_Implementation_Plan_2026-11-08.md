# Koven Parser 文法与验证实施建议

**面向对象：** Koven 语言规范、Lexer、Parser、测试与工具链开发人员\
**整理日期：** 2026-11-08\
**状态：** 开发实施建议，供评审和拆分任务；不是已批准的语言变更、完成的形式化证明或整体验收报告。\
**依据：** [原讨论线程][thread]、附件《Koven 文法与 Parser 形式化验证方案》、Guide v0.47，以及下述固定提交的源码与 PR 元数据。

## 1 实施结论

建议保留现有手写递归下降、Pratt 表达式解析及只读预识别索引，先建立可以审阅和复验的链路：

**Guide 规范条文 → 机器可读文法与上下文规则 → 规范 AST 与接受合同 → 生产实现映射 → 测试和证明证据。**

第一批工作同时推进两条轨道：

- **合法语法轨道：** 先裁决规范内部冲突，固定源码、词法、换行、邻接和入口上下文的模型，再从表达式、泛型调用与 lambda、换行和边界切片开始构造文法、验证实现。
- **恢复安全轨道：** 立即把 owner 和 stop 边界、恢复进度、递归与扫描预算、诊断及 Error 节点归属写成可检查合同。完整恢复证明可以后置，最低安全检查不能等完整文法证明结束。

开发可以先做规则清单、合同、最小案例和确定性检查框架。未裁决的规则不得由现有 parser 的输出自动变成金标准。稳定切片可以独立向前推进，不要求全语言问题一次清零，也不能通过排除泛型、lambda、换行等高风险构造来宣称整体完成。

**本轮没有运行 parser 测试、LR 冲突分析或机器证明。** 文中的“已合入”来自 GitHub 元数据；“已有实现/回归”来自源码检查；PR 正文的测试结果仍是该 PR 的历史报告，不等于本轮复跑。

## 2 基线和已落地事实

### 2.1 固定版本

| 用途 | 固定版本 | 说明 |
|---|---|---|
| 第一轮讨论基线 | `6e5934b51fdb3e54a8f88d7f13738b70c7c12ecd` | [#359][pr359] 合入后的基线 |
| 恢复合同及本文主要源码链接 | `70cdd8d995a432669fa82e570ad17c8d1d0c2d3e` | [#360][pr360]、[#361][pr361] 已合入 |
| 本次整理核查到的 main | `4ae149a68b9741609a975085de5d94619f1b66b4` | [#354][pr354] 已合入；变更文件未涉及本次 parser、lexer、Guide 和 Tree-sitter 切片 |

这是整理时的快照，不是对未来 main 的承诺。实施每个任务时应重新固定目标 SHA，检查相关文件差异，不能沿用旧行号或旧测试结果给新提交背书。

### 2.2 已有结构

- [Guide README][guide] 明确 v0.47 的 15 个领域页面是语言规范唯一真源，并区分语言规范、实现事实和 Compiler Contracts。
- [Guide 04][guide04] 已有 primary、postfix、类型实参调用及表达式层级 EBNF、18 级优先级表和四组不结合规则。原提案的简化 EBNF 明确是示意，不应复制为实际文法起点。
- 生产 parser 使用递归下降和 Pratt；[公开入口][parser-entry] 接收 `SourceMap` 与 `LexedFile`，不是只有 `TokenKind` 数组。
- [trial.rs][trial] 为调用点类型实参建立只读严格识别索引；[lambda_trial.rs][lambda-trial] 对 lambda header 的严格前缀进行 DFA 识别。它们不等于整个表达式文法。
- [recovery.rs][recovery] 的核心是 `LexicalRecoveryIndex`。实际句法恢复循环在 [declaration.rs][declaration]，边界与调用方组合还涉及 [boundary.rs][boundary]、class、class/supertypes、type_ref、file 等模块。
- [parser/mod.rs][parser-entry] 定义 32 MiB 专用线程栈和 `MAX_RECURSION_DEPTH = 1024`，注释明确这是实现安全预算，不是语言语法。
- [SyntaxAst][syntax] 有名称和字面量身份依赖源码 Span 的结构，不能先丢 Span 再比较。
- [Tree-sitter 文法][treesitter] 有 8 组 `conflicts` 声明。它可作工具链差分对象，但不是语言规范或形式等价预言机。冲突声明本身也不能证明 Koven 源语言有歧义。

### 2.3 最近恢复修复的正确使用方式

| 已合入变更 | 可复用的合同样例 | 不能由此推出 |
|---|---|---|
| #359，merge `6e5934b5`，2026-11-08 06:32 UTC | 外层分号与右花括号保留；嵌套 TypeRef 继承显式分号；非法嵌套 class/companion 接入递归预算；词法 owner 恢复 | 全部恢复路径已闭合，或已得到恢复正确性证明 |
| #360，merge `4b311957`，07:09 UTC | 类头边界向主构造器字段、类型参数 bound 和父类型中的 TypeRef 传播；非法默认 lambda 保留自身分隔符 | 无类体时仍能保住所有下一文件声明 |
| #361，merge `70cdd8d9`，07:40 UTC | 只有 delimiter 与 lexical owner 同时回到基线，才提升 receiver 软修饰词为成员恢复起点；嵌套类缺名保留父 `}` | 分组委托、连续词法错误节点归属和文件边界均已收口 |

#360/#361 **确实修改了 parser 文件**，不能沿用早期“未改 parser”的说法。#360 改了 type_ref；#361 没有带入该部分 TypeRef/poison 候选改动。三项均未修改 recovery.rs，正说明恢复建模不能只覆盖该索引文件。

[对应回归文件][header-tests]的范围应分开使用：

- L3–158 包含未完成类头、嵌套花括号与函数类型、嵌套类头、实际消费 Span、未闭合超类型调用等不同测试。其中有类头向 TypeRef 传播的样例，不能把整段都叫作同一种合同。
- L161–192：嵌套 classifier 缺名时保留父类体 closer 和下一 `class Sentinel`。
- L194–221：receiver 软修饰词只在 delimiter/lexical-owner 基线识别，保留首 marker 和后续重复、逆序诊断。

[SPEC-0392 第 80 节][review0392]仍记录开放切片：无类体时的下一文件声明、分组委托内外逗号归属、连续词法 poison 在当前结构内可达的 Error 节点。历史候选报告不能当作这些改动已合入的证据。

## 3 规范权威和同步方式

### 3.1 推荐采用的规则

1. 继续以 Guide v0.47 为语言规范权威。机器文法是其可执行投影，Compiler Contracts 说明内部表示、算法和资源合同，生产实现和测试都不能反向定义语言。
2. 本次建议采用 **Guide 内规范规则 → 机器文法** 的单向、可重复生成。对自然语言约束应补稳定规则 ID 和结构化输入，而不是另存一套独立手写副本。
3. 无法立即自动生成的条款，明确列为受审阅的适配规则，保存来源、理由和覆盖案例；生成器不能静默跳过正文里的换行、上下文或例外条款。
4. 如未来希望把机器规则本身纳入权威源，先走明确的规范变更，规定 Guide 文本、生成产物和冲突优先级。生成方向与规范权威是两个需要写清的设计选择，不能悄悄更换。
5. Guide 与实现冲突时，以规范审阅确定目标行为；Guide 自身有冲突时，先登记并请求裁决，不能随意选某一章节或实现现状作为答案。

这些是从讨论整理的推荐治理方案，尚不代替维护者批准。

### 3.2 每条规则的最小追溯记录

每条规范规则至少记录：

`rule_id / Guide 章节与固定 SHA / 规范版本 / 机器规则或谓词 / 输入上下文 / AST 投影 / 实现函数 / 正例和反例 / 证据及状态 / 未决问题`

一条规则可以对应多个函数，一个函数也可能实现多条规则。不要强制“每个 parse 函数恰好一个产生式”，也不要给 Pratt、trial 和恢复循环机械套上同一种 FIRST/FOLLOW 注释。

变更时必须能回答：改了哪条语言承诺、哪些生成产物需要更新、哪些实现分支和测试受影响、哪些旧证据失效。建议在 CI 校验确定性生成和来源 hash，另以审阅检查自然语言覆盖，hash 一致本身不证明语义提取完整。

## 4 先裁决的规范冲突

下列项已经有正文或代码依据，但**目标语义尚未在本轮被正式裁决**。它们应成为首批规则闭环案例。

### D1 泛型调用后的提交条件

- [Guide 03][guide03] 要求 `<...>` 严格试探完整成功，且 `>` 后下一非 trivia token 为 `(` 才提交 typed call。
- Guide 04 的 `typed_call_suffix = call_type_arguments, call_suffix` 也只直接列出圆括号 call suffix。
- [Guide 07][guide07] 明确尾 lambda 调用糖，包含 `consume<Int> { item -> item }`，要求尾随 `{` 前 trivia gap 不含换行。
- 当前 trial 接受后缀 `(` 或 `{`；[postfix.rs][postfix] 在后缀是 `{` 时再检查 `>` 与 `{` 之间的换行。

**需要裁决：** 泛型实参与无圆括号尾 lambda 的正式产生式、成功提交和失败回退条件，以及 03/04/07 各页如何一致表达。现有实现靠近 Guide 07，但这不是自动判定其他条文错误的授权。

**裁决时必须带的案例：** `f<T>()`、`f<T> /* comment */ ()`、`f<T> {}`、`f<T> /* same line */ {}`、`f<T>\n{}`、含换行注释、嵌套泛型、`f<T>`、`a < b > c`、`a < b > (c)`、不完整和非法类型实参。为每个案例指定入口和上下文；例如跨行形式可能成为多个 block element，不能一概写为“非法”。试探失败必须无 AST、诊断或消费状态残留。

### D2 Lambda 内 return 的允许范围

Guide 07 前文允许 `return` 作为 lambda body element，并退出最近 lambda；同节诊断段又把 `return` 列入本阶段延后的 unsupported lambda body form。讨论核对的实现接近允许 `return` 的条文。

**需要裁决：** 诊断段是否为过时版本说明，现行 Phase 1 的合法 body element 集合是什么，Guide 06/07 和诊断说明如何同步。

**裁决用例：** `{ return }`、`{ return x }`、显式 header 后的 return、嵌套 lambda/普通 block、return 值跨换行、调用方 stop。不能把 `return@label`、return 控制流、捕获对象的逃逸限制混为同一个问题。

### D3 非 lambda 位置的 move

[Guide 01][guide01]把 `move` 规定为上下文关键字，其他位置仍可作普通标识符；Guide 07 诊断段却写“move 未后接 `{` 仍按 expected-expression 根因处理”。实现仅在相应 lambda 前缀条件成立时进入 lambda，否则可按名称解析。

**需要裁决：** Guide 07 该句的适用入口和版本范围，是否应更正文案，是否存在必须报错的特定上下文。

**裁决用例：** `val x = move`、`move()`、`player.move()`、`move { ... }`、`move () -> R` 类型前缀及缺失后续 token。例子的词法合格性、Phase 1 语法与后续类型合法性必须分开。

### 裁决完成的标准

每项产出一份简短决策记录：条文冲突、选定行为、理由、版本影响、正反例、需同步页面、实现影响、批准人。未裁决案例可以进入标记为 pending 的独立语料集，不得用生产输出填 expected，也不得通过 ignore 藏进“全通过”统计。

## 5 输入模型和正确性合同

### 5.1 输入不能压缩为 TokenKind 序列

建议把解析输入写成概念模型 `X = (source, lexed, entry, start, context)`，资源参数单独记为 `B`：

- `source`：源文本及合法编码、切片边界。
- `lexed`：有序 lexeme 流、kind、Span、trivia、invalid/reserved、EOF 与词法诊断/终止事件。
- `entry`：file、expression、declaration、block，或明确的内部子入口。
- `start/context`：起始游标、所属结构、hard/soft stops、等待 primary 还是已有左表达式、换行和词法 owner 状态。
- `B`：递归、扫描、内存等实际纳入模型的资源预算，以及明确的超预算结果。

源码拼写决定软关键字；Span 邻接影响 `++`、`>>` 等表达式位置的整体识别；trivia gap 中 LF、CRLF、含换行注释影响尾 lambda；相邻 `>>` 在 TypeRef 中又可能是两个泛型闭合符。这些都不能在模型进入文法前无声删除。

分别定义 `Source → LexedFile` 和 `LexedFile → ParseOutcome` 的合同。若参考 parser 共用生产 lexer，结论只能覆盖“相对于该 lexer 产物的 parser 一致性”，不能同时声称独立验证了 lexer。

### 5.2 明确定义接受

建议统一测试适配层输出：

- `Accept(ast, end)`：满足该入口的语法成功条件；没有表示语法失败的词法/句法诊断或恢复 Error，且消费到规定 EOF 或合法子入口停止边界。
- `SyntaxError(recovered_ast, diagnostics, end)`：语法失败，可保留恢复 AST。
- `ResourceLimit(kind, location)`：资源不足，区别于语言拒绝。
- `InternalFailure(reason)`：内部不变式破坏或异常失败，不能归类成普通拒绝。

分类规则应针对实际 API 写成适配合同，不必为了测试先改变公开 API。`Result::Ok` 可能带恢复 AST，不能直接等于 Accept。Phase 2/3 的类型、名称、所有权失败也不能混进 Phase 1 的句法接受条件。

### 5.3 分别声明要证明的性质

- **Soundness：** 对所有满足输入接口合同的输入，只要生产 parser 给出 Accept，就必须存在对应规范推导、相同规范 AST 和正确的消费边界。域不能预先限成“文法合法输入”，否则误接受被排除在命题之外。
- **Completeness：** 对规范允许、入口上下文合法且预算充分的输入，parser 应 Accept，并产生规范规定的 AST 和边界。
- **Determinism：** 同输入、上下文与资源参数得到相同可观察结果。
- **Termination：** 所有合同内输入均终止，或按合同返回 ResourceLimit；AST 等式或有限样例通过不能替代终止性论证。
- **Recovery safety：** SyntaxError 路径另按恢复合同验证，不要求非法输入存在合法文法推导。

“预算充分”需要匹配实际调用和 trial 查询的深度计算，不应偷换为“最终 AST 深度不超过 1024”。固定实现上限不能成为语言的隐含语法规则。

## 6 文法构造和证据边界

### 6.1 先编码再选择工具

先把终结符、软词拼写、换行/邻接、上下文和消歧规则写清，实际构造文法并检查冲突，再判断某个切片适合 CFG、canonical LR(1)、其他确定性算法或显式谓词。

必须保留讨论后期的纠正：

- 存在 trial、局部 FIRST 重叠或需要更多局部观察，不证明整个文法不是 LR(1)，更不证明语言超出 CFG。
- lambda header 的 `[ Identifier { "," Identifier } ] "->"` 可由 DFA 识别；这只说明 header 前缀，不能外推为整个 lambda 或泛型调用的类别证明。
- LR 分析出现冲突，不自动证明语言有歧义；应检查编码、产生式、所选算法和消歧约束。
- PEG 有序选择得到唯一结果，与证明原始 CFG 无歧义是不同命题。不能把 LL、LR、PEG、Pratt 的术语混成一个证书。

### 6.2 首批切片不能只覆盖容易部分

1. **表达式：** 完整提取 Guide 04 中首批范围的 primary/postfix/prefix、18 级运算关系、四个不结合组、assignment、call argument 的上下文差异。引用到字符串、lambda、控制流和 TypeRef 的产生式应有明确依赖或未覆盖标记，不可当作无条件黑盒后宣称全表达式已验证。
2. **泛型与 lambda：** 类型实参合法性、typed call 提交/回退、strict header、`move` 前缀、尾 lambda 附着、lambda 与 nested block 的入口区别。
3. **换行和边界：** 文件/声明/block/表达式入口，trivia gap，最大表达式、hard/soft stops，语法状态与调用方边界。
4. **词法并行基础：** 模式转换、最长匹配、软词、token/trivia/错误区的覆盖、Span 及 EOF 合同。

### 6.3 机械证据包

每个切片的证据包至少保存：规范版本、源码 SHA、机器文法 hash、输入编码版本、EBNF lowering 规则与 hash、工具和版本/参数、原始冲突、冲突处置、未覆盖规则、可复现命令、AST 投影版本、检查器和可信依赖。

如采用 canonical LR(1)，报告应明确确实构造该模式的项目集和 ACTION/GOTO 表。无冲突表只为所对应的 CFG 提供充分证据；工具用 precedence 选择动作后不再报冲突，不等于原始 CFG 已证明无歧义。LALRPOP、ANTLR 等生成器的具体模式和能力必须另核，不可把“生成成功”直接标成 canonical LR(1) 证明。

建议保留 G1–G4 分层，但所有等级都必须带具体切片和假设：

| 等级 | 目标 | 不足以作为该等级的材料 |
|---|---|---|
| G1 | 指定范围有完整、可追溯的机器规范 | 只有示意 EBNF 或只填运算符表 |
| G2 | 指定范围的无歧义或明确确定解析规则有可检查证据 | 测试通过、静默冲突消解、删除高风险构造 |
| G3 | 独立参考、金标准、差分及合同检查支持实现一致性 | 两套实现共用关键决策后的一致结果 |
| G4 | 生产实现对已证明模型的精化关系有机器检查证明 | 只证明抽象模型，未连接 Rust 实现 |

G2 应在范围与形式化编码确定后设定验收目标，不在当前尚未构造、检查时预设全语言 LR(1) 硬门禁。初期 G3 是主要工程目标，G4 选择稳定的小算法推进。

## 7 AST 规范化和 Span 检查

### 7.1 单独制定 NormalizedSyntax 合同

在去除任何位置数据前，先沿实际 typed ID 读取节点，用 `SourceMap::slice` 提取需要的源码身份，再构造稳定结构：

- 名称、类型路径、成员名、callable reference、命名实参、声明名称、lambda 参数及借用返回等名称信息。
- 整数、浮点、字符、字符串文本的原始拼写，以及实际结构中已有的字面量类别/后缀、布尔值和 null 身份。
- 节点种类、运算符、子节点顺序、类型参数、命名/模式实参、显式 marker、函数声明形式及显式/隐式返回形式、必要的结构存在性。具体字段以对应 AST 工程合同为准。
- 区分 lambda 无 header 与显式零参数 header，区分 `move` 存在与否。不要因为相应字段以 Span 表示就将这种结构信息删除。

准确的浮点结构是 `Expression::Literal(LiteralKind::Float(FloatLiteralKind))`。当前 `Expression::Name` 没有名称 payload；`Integer`/`Float` 主要保存后缀类别；`Char` 无值 payload；`StringPart::Text` 持有 Span。因此直接删除 Span 会把 `x/y`、`1/2`、`'a'/'b'`、不同字符串误归为相同结构。

只删除无语义的分配身份和绝对位置：arena ID 数值、SourceId、SourceMap 身份、绝对偏移。保留 typed-edge 类别和树结构，而不是把所有 ID 变成同一个值。

### 7.2 不做未经批准的语义等价化

首版 G3 建议按源码拼写保留身份，不擅自把数字写法、转义或不同语法糖合并成语义值。字面量解码本身仍须按 Guide 的 parser/literal 边界检查，不能一概推给后续语义阶段。

括号和语法糖也不能无条件抹掉。例如显式分组可能影响命名实参与 assignment 的分类；`f() {}` 应按 Guide 07 归到同一个 call 的最后实参，不应变成对调用结果的第二次 call。每项归一化必须有具体规则、保留条件和金标准。

给规范化器自身写正反测试：身份不同必须不等；仅 arena 分配或整体位置平移不同应相等；显式/缺失 marker 的差异必须可见。差分发现问题时，先允许归因到规范化器，不能默认生产 parser 出错。

### 7.3 独立检查 Span 和源码忠实性

另设断言验证有效 SourceId、范围及字符边界、真实 token 切片、空缺失锚点、实际消费终点、父子覆盖或明确例外、词法 owner 和 caller stop 归属。错误恢复节点不能伪造未消费 token 的非空 Span。Span 被规范化删除后的 AST 相等，不是这些性质的证据。

## 8 恢复最低安全合同

恢复状态至少包含 cursor、入口和 caller stops、局部 delimiter 栈、lexical owner 栈/基线、词法终止事件位置、递归深度、扫描预算及诊断/Error 节点归属。

| 合同 | 首批可检查要求 | 建议见证 |
|---|---|---|
| R1 边界所有权 | 保留调用方拥有的 stop/closer；局部结构只消费自己的 delimiter；soft stop 只在适用基线触发 | 外层 `;`、父 `}`、插值结束、嵌套泛型/函数类型、未完成类头 |
| R2 恢复推进 | 每轮消费输入、推进有限事件索引、降低明确良基度量，或在明确 stop/EOF/资源错误退出；禁止同一有效状态无界重入 | 游标/事件计数、超时、最小错误串及嵌套变异 |
| R3 递归与资源 | 每条递归入口及 trial 查询遵守组合预算，超限结果确定；分别检查扫描工作量和递归深度 | 既有 1024/1025 深度边界、长前缀、多个失败 trial、连续 poison |
| R4 词法与诊断交接 | 已有词法根因不被无依据重复报告；独立 parser 错误不被抑制；适用错误在当前结构内有可达 Error 表示 | 未闭合字符串/插值、invalid/reserved、连续 poison |
| R5 后续结构保留 | 在规范指定的同步边界后，后续合法声明仍能独立解析，Span/marker/诊断归属正确 | `class Sentinel`、`own fun`、`own borrow`、`own public` |

R2 不要求每一内部迭代都消费 token，但不消费时必须有可检验的有限事件推进或良基下降。递归预算有界与每轮进度是两条性质；仅增加超时或深度上限不能证明所有恢复循环推进。工作量计数还应覆盖前向和反向索引查询、事件处理与重复扫描，不能用游标单调掩盖二次扫描成本。

### 8.1 必须覆盖调用方组合

- `TypeStops` 是 TypeRef 停词集；`DeclarationStops` 才是 `recover_declaration_tokens` 的参数类型。
- 两者分别从表达式 `Stops` 投影或由调用方 `.with()` 组合，不是彼此改名或直接互转。
- `CLASS_BODY`、`LEFT_BRACE` 在两套 stop 中的 hard/soft 意义不同。`CLASS_MEMBER` 在 DeclarationStops 一侧，不能把它套到 TypeStops。
- #361 的成员恢复条件要求 `outside_owner && delimiters.is_empty()`，然后才考虑 `stops_at_class_member()` 和 `class_member_recovery_start()`。

实施时建立 caller→callee 边界传递表，覆盖 declaration 恢复执行、class/file 分派、class/supertypes、TypeRef 及 lexical index 的组合。FIRST/FOLLOW 可以帮助推导某些同步集，但不能替代 owner、hard/soft stop 和具体调用方合同。

### 8.2 当前开放项单独验收

优先把第 2 节已记录的无类体文件边界、分组委托内外逗号、连续 poison/Error 节点归属转成独立红测及最小复现。必须先在实施目标提交上确认状态，再修复和验收，不能凭历史开放项假定新提交仍坏，也不能凭相邻 PR 合入假定已解决。

## 9 独立参考解析器与验证组合

参考实现的职责是对已稳定的规范切片提供另一条解析路径，不是投票产生语言语义。

### 9.1 独立性要求

- 可共享明确规范的 token schema、序列化格式和测试协议，需记录共享依赖。
- 不复用生产 trial、boundary、优先级决策或恢复核心，再将共同输出称作独立验证。
- 若共用 lexer，另设源码级词法验证；若共用规范化器，给它独立金标准和结构校验。
- LALRPOP、ANTLR 或专用参考实现均为候选。选型须依据实际文法、错误/资源模型、维护成本和算法独立性，不因换库自动获得证明。
- Tree-sitter 保持为编辑器一致性与第三实现观察对象；差异要按其覆盖范围解释，不能自动判定生产 parser 错。

### 9.2 至少四类相互补充的输入

1. **独立金标准：** 规范审阅后的正反例、边界案例、历史 bug 最小复现。不要全部由机器文法生成。
2. **表面句法生成：** 从源码构造歧义候选、嵌套泛型、lambda/header、软词、换行和注释、相邻操作符、缺失/替换 token。
3. **变形测试：** 只在已证明或明确规定不改变语法的上下文做空白/注释/括号变换；任意换行插入不是合法不变式。
4. **AST 往返：** AST→print→parse 可发现优先级和结构问题，保留使用；但规范 printer 通常不覆盖非法源码和许多表面写法，因此不能单独覆盖消歧问题。

每次差分比较 Accept/SyntaxError/ResourceLimit/InternalFailure、规范 AST、完整消费和停止边界。非法输入主要比较安全合同和规范要求的诊断类别/归属，不要求不同实现的自由诊断措辞完全相同。

失败必须保留原始输入、随机种子、预算、入口、各版本/hash、结果和最小化反例。Shrinker 必须保留导致失败的入口、关键换行/邻接/owner 条件，不能缩成另一个问题。差异按规范、lexer、生产 parser、参考 parser、规范化器、测试框架、预算不一致分类，不能自动更新快照掩盖。

## 10 分阶段任务与验收

下表是建议拆分，不预指定个人或期限。负责人、评审人和排期由开发团队确认。

| 任务 | 依赖 | 交付物 | 退出条件 |
|---|---|---|---|
| T0 固定基线和返工清单 | 无 | SHA、版本清单；历史 bug 分类；规则/模块/测试映射；本次未覆盖范围 | 每个首批案例有来源、最小复现和所属层；已合入与历史候选明确区分 |
| T1 裁决规范冲突 | T0 | D1–D3 决策记录；同步后的 Guide 条款；规则 ID | 目标切片无未裁决的规范冲突；新增/改变语义经维护者批准 |
| T2 定义输入和结果合同 | T0，可与 T1 并行 | LexedFile/Span/换行/上下文模型；Accept 分类；资源模型 | 能区分误接受、恢复 AST、预算不足、内部失败；接口合同有正反测试 |
| R0 建立恢复安全基线 | T0、T2 初稿，不等 T1/G2 | R1–R5 属性、caller 传递表、已有回归映射、开放红测 | 声明覆盖范围内无不明越界/死循环；已知失败明列并有复现；不宣称全量恢复完成 |
| T3 提取首批机器文法 | 对应切片 T1、T2 | 单向生成、规则映射、完整依赖、未覆盖清单 | 稳定重生成；规定范围所有正文约束有映射；排除项不会被计入完成范围 |
| T4 定义规范 AST 与检查器 | T2、T3 的稳定规则 | NormalizedSyntax、投影规则、独立 Span 检查及金标准 | 名称/字面量/marker 不误合并；结构和位置检查分离；已覆盖类型有独立测试 |
| T5 构造文法证据 | 对应 T3 | 原始冲突报告、处置记录、工具/编码证据包 | 对声明范围逐条说明可证明性质与假设；无静默冲突消解或未说明豁免 |
| T6 独立参考和差分 | 稳定切片 T3、T4；T5 可并行 | 参考 parser、第三源语料、表面生成、shrinking、一致性报告 | 报告覆盖接受/拒绝、AST、边界和资源分类；未解释差异为零，规范 pending 不计为通过 |
| T7 选择性 CI 门禁 | R0、T5/T6 达到切片标准 | 可复现报告；稳定检查清单；失败分级和负责人 | 先 advisory，再按已批准范围提升 required；原有 required 检查保持，不以新报告替代 |
| T8 局部机器证明 | 稳定合同和算法 | 证明义务登记；选定算法的证明及 Rust 对应关系 | 证明连接精确规范和源码版本；假设、边界、可信组件均可审查 |

**并行关系：** T0 后 T1、T2、恢复安全准备可以并行；T3 按已经裁决的切片启动；T5 文法检查与 T6 测试可按稳定切片并行。无需等待全语言形式化才增加恢复保护，也不建议在目标语义未稳定时投入完整参考实现或大型证明基础设施。

**建议首个开发交付包：** T0 清单、D1–D3 决策草案、T2 合同、R0 回归映射及开放红测、首批表达式范围清单。评审通过这个包后，再确定生成器与证明工具。

## 11 CI 和证据状态

每份报告必须绑定 `source_sha + guide_version/hash + grammar_hash + encoding_version + normalizer_version + toolchain/flags + scope + budget`。旧提交的通过结果不能覆盖改过的文件或新规则。

状态建议统一为：

- `proved`：有可复验机器证明，命题和假设明确。
- `validated`：独立检查/证书检查等支持明确性质，写明检查器、可信边界和适用范围。
- `tested`：样例、性质、模糊或差分测试通过，附运行记录与种子。
- `pending`：未完成或等待裁决。
- `failed`：已有可复现失败。
- `unsupported`：方法/工具不覆盖，必须说明原因，不能当通过。

Kani 的有界检查必须标注展开和输入边界；TLA+ 的有限模型检查只覆盖该模型；Lean/Coq 证明抽象模型后仍需连接生产实现。不能因为工具名字带“验证”，就把结果统一标作无限输入上的 Rust 正确性证明。

新 grammar/conformance 检查先以 advisory 展示覆盖与差异。提升为 required 前至少满足：范围获批、规范冲突已裁、运行可复现、参考及规范化器有独立校验、反例有归属、资源/时限可维护、已知豁免显式受控。advisory 只改变新证据检查的合入策略，不豁免既有必需测试，也不应隐藏已知生产回归。

每个涉及 parser 语义的 PR 应附：规则 ID、规范改动或无需改动的理由、实现映射、正反例、AST/Span/边界影响、预算影响、实际运行的检查、未运行项及当前证据等级。

## 12 风险和非目标

### 主要风险与应对

- **多份规范继续漂移：** 单向生成加来源检查，人工约束和未覆盖项显式登记。
- **只证明容易子集：** 范围表列出泛型、lambda、换行和软词等高风险构造；子集报告不得挂全语言完成标签。
- **共同错误形成假一致：** 参考实现不复用关键生产决策，增加独立金标准和源码级语料，审查规范化器。
- **上下文丢失：** 输入保留源码/邻接/换行/owner/入口；差分和 shrink 保持这些条件。
- **恢复合同只看单个文件：** 覆盖 caller、TypeStops、DeclarationStops、TypeRef 和 lexical index 的组合。
- **证明与实现脱节：** hash 绑定、源码变更影响分析、局部精化对应；持续区分模型性质与生产程序性质。
- **范围过大阻碍修复：** 采用切片验收，恢复安全并行，先建立小而真实的证据闭环。

### 本轮不包含

- 全面重写 parser，或为了某个生成器擅自改变表面句法。
- 预先断言 Koven 全语言属于或不属于 CFG/LR(1)。
- 把现有 parser、Tree-sitter 或同源生成器结果当作语言裁决。
- 一次性交付全语言完整形式化证明，或宣布所有 class-family 恢复问题已解决。
- 用本方案给 D10、SPEC-0341、E2/E3 等无关验收项盖章，或据此宣布项目 Ready。
- 在本次整理中修改仓库、运行 CI、提交 PR 或自动合并。

## 13 开工评审清单

- [ ] 确认负责规范裁决、Parser 实现、测试、形式化证据审阅的人，并固定目标 SHA。
- [ ] 批准规范权威及单向同步方案；建立规则 ID 和变更追溯。
- [ ] 为 D1–D3 确认目标行为、版本边界、正反例和需同步的 Guide 页面。
- [ ] 确认首批文法切片、输入编码、Accept 定义和资源结果分类。
- [ ] 将 #359/#360/#361 已有回归映射到恢复合同；复核并单列尚开放切片。
- [ ] 批准 AST 规范化保留项和独立 Span 检查规则。
- [ ] 按实际文法选参考实现与机械检查工具，记录共享依赖和可信边界。
- [ ] 先交 advisory 报告；逐切片批准 G2/G3 目标及 required 门禁，不设置无依据的全语言完成时间。

完成首个闭环的标准是：**一个真实规范冲突被明确裁决，同步为可执行规则，生产实现有对应合同，独立案例和参考路径能复现接受/AST/边界结论，并且报告清楚写出仍未证明的部分。**

## 参考来源

以下链接固定到讨论使用的提交。原附件为线程根消息中的 `Koven_Grammar_Parser_Formal_Verification_Proposal.md`，文件 ID `F0CFPCLREF5`。

- [原始讨论与后续修正][thread]
- [Guide v0.47 权威入口][guide]
- [Guide 01 词法及恢复交接][guide01]、[Guide 03 类型与泛型][guide03]、[Guide 04 表达式][guide04]、[Guide 07 调用与 lambda][guide07]
- [Parser 公开入口与资源预算][parser-entry]、[类型实参预索引][trial]、[Lambda header 预索引][lambda-trial]、[Postfix 提交与换行门][postfix]
- [SyntaxAst][syntax]、[恢复执行][declaration]、[停止条件类型][boundary]、[词法恢复索引][recovery]
- [类头与成员恢复回归][header-tests]、[SPEC-0392 复审记录][review0392]、[Tree-sitter 文法][treesitter]
- [PR #359][pr359]、[PR #360][pr360]、[PR #361][pr361]、[PR #354][pr354]

[thread]: https://app.slack.com/archives/C0CBH9KMS1M/p1794119877283479
[guide]: https://github.com/Halckon/Koven/blob/70cdd8d995a432669fa82e570ad17c8d1d0c2d3e/docs/guide/README.md
[guide01]: https://github.com/Halckon/Koven/blob/70cdd8d995a432669fa82e570ad17c8d1d0c2d3e/docs/guide/01-lexical.md
[guide03]: https://github.com/Halckon/Koven/blob/70cdd8d995a432669fa82e570ad17c8d1d0c2d3e/docs/guide/03-types-generics.md#L415-L425
[guide04]: https://github.com/Halckon/Koven/blob/70cdd8d995a432669fa82e570ad17c8d1d0c2d3e/docs/guide/04-expressions-operators.md
[guide07]: https://github.com/Halckon/Koven/blob/70cdd8d995a432669fa82e570ad17c8d1d0c2d3e/docs/guide/07-calls-lambdas-closures.md#L255-L350
[parser-entry]: https://github.com/Halckon/Koven/blob/70cdd8d995a432669fa82e570ad17c8d1d0c2d3e/crates/lang-frontend/src/parser/mod.rs
[trial]: https://github.com/Halckon/Koven/blob/70cdd8d995a432669fa82e570ad17c8d1d0c2d3e/crates/lang-frontend/src/parser/trial.rs
[lambda-trial]: https://github.com/Halckon/Koven/blob/70cdd8d995a432669fa82e570ad17c8d1d0c2d3e/crates/lang-frontend/src/parser/lambda_trial.rs
[postfix]: https://github.com/Halckon/Koven/blob/70cdd8d995a432669fa82e570ad17c8d1d0c2d3e/crates/lang-frontend/src/parser/engine/postfix.rs#L552-L588
[syntax]: https://github.com/Halckon/Koven/blob/70cdd8d995a432669fa82e570ad17c8d1d0c2d3e/crates/lang-frontend/src/parser/syntax.rs
[declaration]: https://github.com/Halckon/Koven/blob/70cdd8d995a432669fa82e570ad17c8d1d0c2d3e/crates/lang-frontend/src/parser/engine/declaration.rs#L742-L857
[boundary]: https://github.com/Halckon/Koven/blob/70cdd8d995a432669fa82e570ad17c8d1d0c2d3e/crates/lang-frontend/src/parser/engine/boundary.rs#L374-L678
[recovery]: https://github.com/Halckon/Koven/blob/70cdd8d995a432669fa82e570ad17c8d1d0c2d3e/crates/lang-frontend/src/parser/engine/recovery.rs
[header-tests]: https://github.com/Halckon/Koven/blob/70cdd8d995a432669fa82e570ad17c8d1d0c2d3e/crates/lang-frontend/tests/parser_class_family/header_recovery.rs
[review0392]: https://github.com/Halckon/Koven/blob/70cdd8d995a432669fa82e570ad17c8d1d0c2d3e/docs/specs/active/0392-compiler-layered-review.md
[treesitter]: https://github.com/Halckon/Koven/blob/70cdd8d995a432669fa82e570ad17c8d1d0c2d3e/editors/tree-sitter/grammar.js#L23-L40
[pr359]: https://github.com/Halckon/Koven/pull/359
[pr360]: https://github.com/Halckon/Koven/pull/360
[pr361]: https://github.com/Halckon/Koven/pull/361
[pr354]: https://github.com/Halckon/Koven/pull/354
