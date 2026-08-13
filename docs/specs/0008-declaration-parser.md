# SPEC-0008: 建立独立声明 Parser

| 字段 | 值 |
|---|---|
| 状态 | draft |
| Goal ID | `KOV-P1-008` |
| 所属 Phase | Phase 1 |
| 语言规范 | 候选 [`agent-language-design-guide-v0.7.md`](../agent-language-design-guide-v0.7.md)；尚未生效 |
| 批准依据 | 待 v0.7 被用户明确启用后，按当前持续 Goal 的站立授权推进 |
| 前置 Spec | SPEC-0007 `done` |
| 前置 ADR | 无 |
| 关联 ADR | [ADR-0003](../adr/0003-diagnostic-architecture.md)、[ADR-0004](../adr/0004-source-span-position-model.md) |
| 阻塞项 | v0.7 仍是候选；用户尚未明确指定其取代现行 v0.6，故本 Spec 不得批准或实施 |
| 影响范围 | `lang-frontend`、语言 fixture、Architecture |
| 语言语义变更 | 否；本 Spec 仅拟实现候选 v0.7 已写明的增量，生效前不构成现行语义 |

## 1. Goal

完成后，`lang-frontend` 能从同一 `SourceMap` 的 `LexedFile` 确定性解析一个独立的
`val` / `var` / `const val` / `fun` 声明，构造带精确 `Span` 的索引式 Item / Expression /
TypeRef AST，并支持候选 v0.7 定义的调用点类型实参；普通用户语法错误进入有序诊断与
显式 error node，非法源码不触发 `panic!`。

本 Goal 只在用户明确启用 v0.7 后才能进入 `approved` / `in-progress`。在此之前，本文件
只记录候选实施范围与可执行验收设计。

## 2. 背景

SPEC-0007 已提供独立表达式入口、Pratt 核心、递归 `type_ref`、具体 Expression / TypeRef
payload、固定资源边界和两阶段诊断合并，但 `ExpressionAst` 的 item / statement payload
仍为空，公共入口只能解析唯一表达式。

现行 v0.6 仅列出 SPEC-0008 的功能方向，没有足够的声明产生式、恢复边界与 AST `Span`
契约。候选 v0.7 第四部分第 2、3、7 节补齐了这些规则；在其未生效期间，本 Spec 保持
`draft`，不得把候选规则反向写成仓库现行事实。

## 3. 范围与需求

### 3.1 独立声明入口与产物

- 新增一次只解析一个简单声明的公共入口。入口接收 `&SourceMap` 与 `&LexedFile`，校验
  source identity，跳过 trivia，并要求解析结果后抵达唯一 EOF。
- 支持候选 v0.7 的 `variable_declaration`、`constant_declaration` 与
  `function_declaration`；换行、注释及其他 trivia 不终止声明。
- 产物拥有同一份具体索引式 AST、唯一 `ItemId` root，以及 Lexer / Parser 诊断的确定性
  合并全序。用户语法错误仍返回产物；source、AST、诊断目录、lexeme 流和资源边界失败
  返回 `ParserInternalError`。
- 独立表达式入口继续保留，并与声明入口共享 cursor、Pratt、TypeRef、词法恢复索引、诊断
  构造和固定 parser worker；不得复制第二套表达式或 TypeRef parser。
- 声明入口必须使用与 SPEC-0007 相同的固定 32 MiB scoped worker 和统一 1024 递归预算，
  预算耗尽受控返回内部资源错误，不分配新的语言诊断码。

### 3.2 声明 AST

- 把具体 AST 扩展为 `AstFile<Item, (), Expression, TypeRef>`；本 Spec 不创设 statement
  payload，完整文件及局部声明归属仍由后续 Spec 决定。
- 声明、类型参数和值参数的名称统一使用恢复可见的 marker，而不是裸 `Span`：

  ```rust
  pub enum NameMarker {
      Present(Span),
      Missing(Span),
      Error(Span),
  }
  ```

  `Present` 只覆盖实际 Identifier token；`Missing` 只表示在 stop / EOF 前未消费名称，必须
  保存该边界处的空 `Span`；`Error` 只覆盖为该名称实际消费的 invalid、reserved-word 或其他
  poison 区域，必须为非空 `Span`。三种状态不得互相折叠，也不得为缺失名称伪造 Identifier。
- `Item` 至少区分 `Error`、变量、常量和具名函数。变量 payload 保存 `val` / `var` 种类、
  名称 `NameMarker`、可选类型标注和必需 initializer `ExpressionId`。常量保持独立 kind，
  但其语法固定为不可拆分的 `const val` 前缀：保存 `const` 的精确 `Span`，并以正常 `val`
  token Span 或 missing / error marker 表示第二个关键字。缺失时不得构造虚假的 keyword token
  或非空 `Span`；只有 `const` 的输入仍是恢复后的错误常量，而不是成功常量声明。
- 函数 payload 保存名称 `NameMarker`、源码顺序的类型参数、值参数、显式返回 `TypeRefId` 与可选
  expression body `ExpressionId`。它还必须保存可观察的 `type_parameter_list_span: Option<Span>`
  或等价字段：没有类型参数表时为 `None`，存在时覆盖 `<` 到匹配 `>`，缺 `>` 恢复时
  只到该表最后实际消费位置。无表达式体的函数签名仍构造函数 Item。
- 类型参数保存名称 `NameMarker` 与可选单一 bound `TypeRefId`；值参数保存名称 `NameMarker` 与必需
  `TypeRefId`。两类参数必须用不同 Rust payload，不能依靠调用方猜测上下文。
- 类型标注若不单建 AST node，声明 payload 仍须保存冒号 `Span`，使 `:` 到 TypeRef 结束的
  合成范围可观察；表达式体同理保存 `=` 的 `Span`。
- 所有父子关系只保存 `ItemId`、`ExpressionId`、`TypeRefId` 或小型结构 payload，不嵌套
  拥有 AST 子树，不复制类型名或表达式源码字符串。

### 3.3 声明语法

- `val`、`var` 后必须是普通 Identifier；常量必须以固定的 `const val` 开始，再跟普通
  Identifier。三者均可选 `: type_ref`，随后必须有 `=` 和 initializer expression，且不接受
  类型参数表。`const x = 1` 与 `const var x = 1` 必须拒绝；初始化式是否为编译期常量留给
  Phase 2，不在 parser 中按表达式内容判断。
- `fun` 后可有位于函数名前的类型参数表，随后必须有普通名称、圆括号参数表、显式
  `: type_ref` 返回类型，并可选 `= expression` 表达式体。
- 已出现的类型参数表至少包含一个元素；函数参数列表可以为空。两类非空列表均不接受空项、
  缺失逗号或 trailing comma。类型参数只允许 `Identifier` 与可选单一 `: type_ref` bound；
  值参数只允许 `Identifier : type_ref`。
- 不接受默认参数、参数解构、`vararg`、参数 `val` / `var`、声明侧 `own` / `inout` /
  `borrow`、多 bound、默认类型实参、`where` 或型变。Phase 1 不检查重复参数名、bound
  语义、默认 `Any`、函数签名类型合法性或常量求值。

### 3.4 调用点类型实参

- 扩展现有 `Expression::Call`，使每个 call 显式保存 `Vec<TypeRefId>` 与可观察的
  `type_arguments_span: Option<Span>` 或等价字段。basic call 保存空 vector 与 `None`；typed
  call 保存源码顺序的类型实参与完整 `<...>` `Span`，不新增脱离调用的
  type-apply expression。
- postfix 位置遇 `<` 时先进行无副作用试探：只有从该处能完整合法解析
  `call_type_arguments`，且其后下一个非 trivia token 是 `(`，才提交 typed call suffix。
- `>` 与 `(` 之间允许任意 trivia；`f<T> /* comment */ ()` 合法。试探成功的语法优先归属
  typed call，因此 `a < b > (c)` 也按 typed call 解析。
- 试探失败必须完整回滚 cursor、递归预算、AST table 长度、诊断和任何恢复状态；更简单的
  实施方案是使用不构造 AST / 诊断的只读 trial cursor，成功后再由正式 TypeRef parser
  提交一次。不得通过 clone 整份 AST 或 Parser 掩盖回滚问题。
- trial 的内部结果必须区分 `Match`、`NoMatch` 与 `InternalError` 或等价三态。只有
  `NoMatch` 允许回到原 `<` 并按比较表达式继续；递归预算耗尽、无效 lexeme 流、source
  或其他内部不变量错误必须原样传播为 `ParserInternalError`，不得伪装成试探不匹配。
- trial 的递归预算以当前 Parser 正在使用的 `recursion_depth` 为基线，不得从零重置
  或获得独立额度；无论 `Match`、`NoMatch` 还是 `InternalError`，退出 trial 时都不得泄漏
  预算计数修改。
- 一次根解析内，所有 strict trial（包括预索引 / memo 构建）的 token inspection 总数必须为
  `O(N)`，其中 `N` 是该 `LexedFile` 的 lexeme 数；不得让每个 `<` 候选从头重扫重叠的 TypeRef
  后缀而退化为 `O(N²)`。实现须维护“每个 lexeme 与固定 trial 状态至多求值一次”或等价的
  可审计线性不变量；正式提交后的 TypeRef / call 解析可再线性消费其唯一所属区间。
- 允许使用一次性预索引或只在当前根解析存活的 memo，但缓存结果除 `Match` / `NoMatch`、结束
  ordinal 等识别信息外，还必须记录该结果相对 trial 起始基线所需的 `additional_depth`。复用
  缓存时必须重新检查 `baseline_depth + additional_depth` 是否超过统一 1024 预算；超过时返回
  `NestingLimitExceeded`。不得用浅层基线生成的缓存成功绕过深层调用点的预算，也不得缓存后
  把 `InternalError` 降级为 `NoMatch`。
- `f<T>`、`a < b > c` 与不完整 `<...` 继续从原 `<` 按普通比较语法解析，不产生专用泛型
  调用诊断；嵌套 TypeRef 的 `>>` 继续分别关闭两层泛型。

### 3.5 `Span` 契约

- `val` / `var` / `const val` Item 从首个引导关键字起到 initializer 终；恢复时到该声明
  最后实际消费位置。
- 常量的 `val` marker 正常时精确覆盖 token；缺失且恢复未消费 token 时，保存当前候选名称 /
  stop 起点的空 Span；若恢复消费了 `var` 或其他错误 token，error marker 只覆盖实际消费
  区域。声明整体绝不把缺失的 `val` 合成进虚构范围。
- `fun` Item 从 `fun` 起，有表达式体时到 body expression 终，否则到返回 TypeRef 终；
  恢复时到最后实际消费位置。
- 名称的 `Present` 精确覆盖 Identifier token；`Error` 只覆盖本次实际消费区域且非空；在 stop /
  EOF 处没有消费名称时必须是空范围的 `Missing`，不得用空 `Error` 或非空 `Missing` 代替。
- 值参数从名称起到参数 TypeRef 终；类型参数从名称起，无 bound 时到名称终，有 bound 时
  到 bound TypeRef 终。
- 类型参数表从 `<` 起到匹配 `>` 终；若在候选函数名处恢复缺失 `>`，则止于最后实际消费的
  类型参数，不纳入函数名或 `(`。call type arguments 同样从 `<` 到 `>`；typed call 从
  callee 起点到调用 `)` 终。
- 所有范围均为同一 SourceId 的 UTF-8 字节半开区间；缺失 token 不得通过虚构坐标加入父
  `Span`，trial parse 不得改变后续 typed ID 的确定性。

### 3.6 稳定诊断与局部恢复

在现有 `L0001`–`L0016` 后连续注册以下 Parser 诊断；全部为 `error`。既有五类保持
`L0017`–`L0021`，候选 v0.7 后续明确的声明恢复类别使用 `L0022`–`L0027`：

| 错误码 | 稳定含义 | 固定主消息 | 主 `Span` |
|---|---|---|---|
| `L0017` | 需要独立声明 | `expected declaration` | 当前非法起始 token；EOF 时为空范围 |
| `L0018` | 需要声明名称 | `expected declaration name` | 当前 token；位于 `:`、`=`、`(` 或 EOF 边界时为空范围 |
| `L0019` | 需要参数名称 | `expected parameter name` | 当前 token；位于 `:`、`,`、`>`、`)` 或 EOF 边界时为空范围 |
| `L0020` | 需要初始化式 | `expected initializer` | 缺 `=` 时的当前 token；EOF 时为空范围 |
| `L0021` | 需要显式返回类型 | `expected explicit return type` | 缺 `:` 时的当前 token；位于 `=`、`{` 或 EOF 时为空范围 |
| `L0022` | `const` 后需要固定的 `val` | `expected 'val' after 'const'` | `const` 后的当前 token；EOF 时为空范围 |
| `L0023` | 值参数名后需要类型分隔符 | `expected ':' after parameter name` | 参数名后的当前 token；EOF 时为空范围 |
| `L0024` | 声明参数列表需要元素 | `expected list element` | 空类型参数表的 `>`，或 leading / repeated `,` |
| `L0025` | 声明参数列表需要分隔符 | `expected list separator` | 缺逗号后可开始下一同类参数的 token，或当前非法 token |
| `L0026` | 声明参数列表不支持 trailing comma | `unsupported trailing comma` | 当前层闭合符前的逗号 |
| `L0027` | 不支持参数默认值 | `unsupported parameter default` | 从 `=` 起覆盖实际消费的默认值错误区域 |

- `=` 后缺 initializer expression 复用 `L0009` 和显式 Expression::Error；`:` 后缺 TypeRef
  复用 `L0014` 和 TypeRef::Error。
- 参数表、类型参数表及 TypeRef 缺闭合符复用 `L0010`，并保留 opener 关联标签；不得为了
  声明语法重复分配同义诊断码。
- `L0022` 遇 Identifier 时不消费并把它继续作为常量名；遇 `var` 时只消费该错误 marker 后
  继续期待名称；遇 `:`、`=` 或 EOF 时不越过边界；其他 token 只消费一个后继续期待名称。
  未消费 token 时保存候选名称 / stop 起点的空 marker Span；消费 `var` 或其他错误 token 时
  marker 仅覆盖所消费区域，绝不伪造缺失 `val` 的非空 token Span。
- `L0018` 在 `:`、`=`、`(` 或 EOF 前停止，常量名称只在完整或已恢复的 `const val` 前缀后
  解析；`L0019` 在当前层 `:`、`,`、`>`、`)` 或 EOF 前停止，不跨嵌套 TypeRef delimiter。
- `L0023` 后若当前 token 可开始 TypeRef，则不消费并按插入 `:` 继续解析；若是 `=`，形成空
  TypeRef::Error，再按 `L0027` 的同步方式消费默认值区域但不追加第二条诊断；若已到当前层
  `,`、`)`、外层 `{`、EOF 或调用方 stop，则不消费并形成空 TypeRef::Error。其余情况至少
  消费一个 token，并继续到当前层 `,` / `)`、外层 `{`、EOF 或调用方 stop，形成仅覆盖实际
  消费区域的 TypeRef::Error；所有分支均抑制同根因的 `L0014` 与列表诊断。
- 类型参数表在 `>` 前没有任何元素时，`L0024` 主 Span 取 `>` 且不消费；类型 / 值参数位置
  直接出现 leading 或 repeated `,` 时，`L0024` 只消费该逗号，再从下一项或当前层闭合符继续。
- 一个完整参数后若下一 token 可开始同类参数但缺 `,`，`L0025` 在该 token 报告且不消费，
  将其继续作为下一项；其他非法 token 恢复到当前层 `,`、`>`、`)` 或 EOF。
- `L0026` 只消费并覆盖 trailing comma，保留当前层 `>` / `)` 给所属列表消费；空值参数表
  `()` 合法，不属于空项或 trailing comma。
- 已完整解析 `name: type_ref` 后出现 `=` 时，`L0027` 从 `=` 起消费默认值错误区域，到当前层
  `,`、`)` 或 EOF 前停止并保留 delimiter；平衡嵌套 `()` / `[]` / `{}` 内的逗号和匹配 closer
  不作同步点。若未闭合嵌套的栈顶 closer 与值参数表 owner 的 `)` 不匹配，该 `)` 仍按下述
  hard-closer 规则立即停止并保留。即使 `=` 后没有表达式也至少消费 `=`，且不追加
  `L0009`、`L0025` 或 `L0013`。
- 至少完成一个类型参数后，若当前 Identifier 的下一非 trivia token 是 `(`，唯一解释为缺失
  泛型 `>`：优先于缺逗号恢复，复用 `L0010`，主 Span 为候选名称起点的空范围；不消费名称
  或 `(`，结束类型参数表并让外层继续解析函数名。
- `L0020` 表示缺 `=`：若当前 token 可开始 Expression，则不消费并按插入 `=` 继续解析；若
  已到 EOF 或调用方声明 stop，则不消费并建立空 Expression::Error；其余情况至少消费一个
  token，再同步到 EOF / 调用方声明 stop，建立只覆盖实际消费区域的 Expression::Error，且
  不追加同根因的 `L0009` 或 `L0013`。已有 `=` 但缺表达式时才复用 `L0009`。
- `L0021` 后若当前 token 可开始 TypeRef，则不消费并按插入 `:` 继续解析；若是 `=`、`{`、
  EOF 或调用方 stop，则不消费并形成空 TypeRef::Error；其余情况至少消费一个 token并同步
  到 `=`、`{`、EOF 或调用方 stop，形成仅覆盖实际消费区域的 TypeRef::Error。所有这些分支
  均不追加同根因的 `L0014`；保留的 `=` 继续作为表达式体，保留的 `{` 仍由本 Spec 的
  unsupported block-body / `L0013` 边界处理。已有 `:` 但缺类型时才复用 `L0014`。
- 上述声明列表诊断只作用于已经提交的类型参数表和值参数表；失败的 call type arguments
  trial 必须无副作用回退，不能遗留 `L0024`–`L0026` 或其他声明专用诊断。
- Lexer 已诊断的 invalid / reserved-word token 只消费并形成相应 error payload，不在同一
  `Span` 重复 Parser 诊断。每条恢复路径必须消费输入或抵达明确 delimiter / EOF。
- 所有会平坦跳过声明错误区域的恢复（包括 `L0020` 的非法 token 分支、`L0023` / `L0021`
  的兜底和 `L0027` 默认值）必须复用 SPEC-0007 建立的 `LexicalRecoveryIndex`，并采用同一套
  string / interpolation owner-local 边界；不得另扫 Lexer 诊断来猜测字符串归属。某个字符串
  的 recovery end 只能关闭该 owner；terminal 恢复到 EOF 时按索引中登记的全部 active owners
  退出，不能提前暴露嵌套字符串 / interpolation 内的 delimiter。
- 对不在活动 string / interpolation owner 内、且由调用方列为 stop 的 delimiter，统一扫描器
  必须严格按以下顺序判定：若 token 匹配 delimiter stack 的栈顶 closer，先 pop 并消费它，
  因为它属于恢复区域自己打开的平衡嵌套；否则，若 token 是当前列表 / 构造 owner 的 hard
  closer（值参数表的 `)`、类型参数表的 `>` 或调用方明确给出的等价边界），无论 delimiter
  stack 是否仍有未匹配 opener，都立即停止并保留该 closer 给 owner 消费，且不弹出失配
  opener；最后，`,` 等 soft stop 只在 delimiter stack 为空时停止并保留，仍有平衡或失配嵌套
  时则作为错误区域内容消费。不得把这三步合并成“stack 非空就一律越过 stop”。
- 每次声明局部恢复的 cursor 必须单调前进；设从错误起点到保留的同步 token / EOF 共经过
  `k` 个 lexeme，该次扫描的 token inspection 与 owner-boundary 查询总量必须为 `O(k)`。
  实现可在起点做一次有序索引定位，随后使用单调索引指针，但不得对每个 token 重扫诊断、
  已消费前缀或全部 recovery entries。到达精确 string recovery end 后才退出相应 owner，且
  owner-local 词法恢复不得抑制声明本身独立缺失的 closer / separator 诊断。
- 本入口不把换行、下一声明关键字或看似合法的后续声明当同步点。根声明后仍有输入复用
  `L0013`；SPEC-0011 只增加完整文件组合、声明分隔、跨声明同步与级联抑制，不重新定义
  SPEC-0008 / SPEC-0009 的节点内部恢复。

### 3.7 Fixture

- 复用现有 fixture target，新增 `phase1/parser-declaration-pass/` 和
  `phase1/parser-declaration-fail/`，两套 suite 均至少枚举一个真实 `.ko`。
- pass case 要求 Lexer / Parser 无诊断、Item root 有效、相应 child ID 可读且完整消费 EOF。
- fail case 沿用 `.diag` 的 `Ldddd<TAB>start_byte<TAB>end_byte` 测试 sidecar，精确核对 Lexer /
  Parser 合并诊断全序；该格式仍不是公共机器诊断协议。
- sidecar 解析必须显式接收 suite 的 Span policy：Lexer fail suite 继续要求
  `0 <= start_byte < end_byte <= source_len`；parser-expression 与 parser-declaration fail suite
  仅允许 Parser 诊断在 EOF / stop 边界产生 `start_byte == end_byte`，即对该类条目要求
  `0 <= start_byte <= end_byte <= source_len`；parser suite 合并 sidecar 中的 `L0001`–`L0008`
  Lexer 条目仍必须非空。不得为迎合既有 sidecar helper 而把 Parser 空 Span 扩成相邻 token；
  反向范围、越界或非 UTF-8 边界在两种 policy 下都必须拒绝。
- 零 fixture、缺失 / 孤立 sidecar、非法行、额外或缺失诊断必须失败；现有 Phase 0、Lexer
  与 parser-expression suites 必须继续执行。

## 4. 非目标

- 不在 v0.7 生效前批准或实施本 Spec，也不修改现行 v0.6 真源指针。
- 不实现 block、block statement 序列、函数 block body 或其他局部 statement；这些结构由
  SPEC-0009 首次定义并实现。也不实现完整文件、声明分隔或跨声明恢复；这些组合与同步职责
  留给 SPEC-0011。
- 不实现 class / value class / interface / enum class / object / companion object 或成员上下文。
- 不实现控制流、lambda、解构、命名参数、模式实参、trailing lambda 或默认参数。
- 不实现 `public` / `internal` / `private`、`extern`、`operator`、`override`、`infix`、
  extension receiver、匿名函数、`typealias`、`module` 或 `import`。
- 不定义声明侧 `vararg`、多 bound、`where`、型变、默认类型实参或可空函数类型。
- 不做名称解析、重复名称诊断、类型推导、函数返回类型检查、泛型 arity、常量求值、
  overload 选择、单态化、所有权或借用检查。
- 不引入 parser generator、CST / green tree、增量 parsing、visitor、formatter trivia 附着、
  新 crate 或第三方依赖。

## 5. 验收标准

- [ ] v0.7 已由用户明确指定取代 v0.6，仓库现行 guide 指针已在独立提交中一致切换；否则
      本 Spec 保持 `draft` 且不得实施。
- [ ] 公共声明入口校验 SourceMap / LexedFile identity；普通语法错误进入产物，内部资源或
      模型失败返回具体 `ParserInternalError`。
- [ ] `val`、`var`、`const val` 覆盖有 / 无类型标注的 compile-pass；常量 AST 的 `const`
      Span 与正常 / missing / error `val` marker、kind、名称、冒号、initializer ID 及声明完整
      Span 均被锁定，缺 marker 时不伪造 keyword token 或非空 Span。
- [ ] 变量、常量、函数、类型参数和值参数的名称结构测试分别锁定 `NameMarker::Present`、
      `Missing`、`Error`；`Present` 只接受 Identifier，`Missing` 必为空，`Error` 必须非空且仅
      覆盖实际消费的 poison，恢复 AST 不伪造名称 token。
- [ ] `fun` 覆盖零 / 多参数、无 / 有表达式体、普通 / move 函数类型参数与返回类型；所有
      函数均保存显式返回 TypeRef，完整 Span 符合候选 v0.7。
- [ ] 泛型覆盖单 / 多参数、无 bound、单一递归 TypeRef bound 和嵌套泛型；函数参数覆盖空
      / 非空列表；两类列表精确拒绝空项、缺逗号和 trailing comma。结构测试同时锁定
      正常及缺 `>` 恢复时的 `type_parameter_list_span`，无列表时必须为 `None`。
- [ ] typed call 覆盖 `f<T>()`、`obj.f<T>()`、`(factory())<T>()`、`factory()<T>()`、嵌套
      TypeRef、`>` 与 `(` 间 trivia，以及 basic call 空 type arguments。结构测试锁定 typed
      call 的完整 `type_arguments_span`，basic call 必须为 `None`。
- [ ] 歧义测试证明 `a < b > (c)` 提交 typed call，而 `f<T>`、`a < b > c`、不完整 `<...`
      无 trial 副作用并按比较表达式产生既有结构 / 诊断。
- [ ] trial 失败前后 AST table 数量、typed ID 分配、诊断顺序、cursor 与递归预算确定；不同
      SourceMap 加载顺序及重复运行得到相同结构、相对 Span 和诊断。
- [ ] trial 三态边界测试证明只有 `NoMatch` 回退为比较；深层外层表达式中的 typed-call
      候选从当前深度继续计数，trial 内恰好超过 1024 预算单位时原样返回
      `NestingLimitExceeded`，且所有退出路径恢复原预算计数。
- [ ] `cfg(test)` token-inspection 计数器或等价确定性证据覆盖密集 `<` 候选、层层嵌套泛型且
      只在末端失败、成功 / 失败交错三类对抗输入；同族输入从 `N` 倍增到 `2N` 时，所有 strict
      trial 的总 inspection 仍受实现记录的固定常数乘 lexeme 数约束，不出现重叠后缀反复扫描。
- [ ] memo / 预索引路径测试锁定 `additional_depth`：同一识别结果在较浅 baseline 可成功，
      在使 `baseline + additional_depth > 1024` 的较深 baseline 必须返回
      `NestingLimitExceeded`；缓存命中不改变 Parser 的实际 recursion counter。
- [ ] `const` 后缺 `val`（Identifier / `var` / delimiter / 其他 token 四类恢复）、缺名称、缺
      初始化符 / 表达式、缺参数名 / `:` / 类型、缺显式返回类型、block body、modifier、默认 /
      模式参数等反例产生候选规则对应诊断，不被静默接受。
- [ ] 缺 `=` 覆盖 expression-start、EOF / 调用方 stop 与其他非法 token 三类恢复；缺参数冒号
      覆盖 TypeRef-start、`=` 默认值、delimiter / stop 与其他非法 token；缺返回冒号覆盖
      TypeRef-start、`=` / `{` / stop 与其他非法 token，均锁定消费边界及同根因诊断抑制。
- [ ] 参数默认值覆盖普通、空默认值和含嵌套 `()` / `[]` / `{}` 及内部逗号的错误区域；
      `L0027` 至少消费 `=`，只在当前参数层的逗号、右括号或 EOF 前同步，并保留 delimiter。
- [ ] owned-closer 对照锁定统一扫描顺序：默认值错误区域含未闭合 `[` 时，遇值参数表 owner
      的 `)` 必须保留该 `)`，不能因栈顶期待 `]` 而吞掉，例如
      `fun f(x: T = [a, b): R`；类型参数错误区域含未闭合 `(` 时，遇类型参数表 owner 的 `>`
      同样保留，例如 `fun <T (bad> f(): R`；平衡对照
      `fun f(x: T = g([a, b]), y: U): R` 与 `fun <T (bad)> f(): R` 的匹配 closer 则逐个 pop
      并消费，只有其后的当前层 `,` / hard closer 才成为 stop。三类都断言错误节点 Span、保留
      delimiter、后续 cursor 与无重复 `L0010` / `L0025`。
- [ ] 声明恢复覆盖嵌套 string interpolation 中的调用 / 下标 / delimiter、嵌套未终止字符串，
      以及 EOF 前 terminal invalid escape；断言 `LexicalRecoveryIndex` 的精确 owner recovery end、
      后续可恢复参数和 delimiter 保留、词法根因只出现一次，且不因内部逗号 / 右括号产生
      `L0025`、`L0010` 或 `L0013` 级联。测试同时用 inspection 计数锁定每个恢复区间 `O(k)`。
- [ ] 列表恢复覆盖空类型参数表、leading / repeated comma、缺逗号、trailing comma，以及
      类型参数表缺 `>` 后紧跟候选函数名和 `(`；后者优先复用 `L0010`，且列表 Span、cursor
      均不吞掉名称或 `(`。
- [ ] `L0017`–`L0027` 一类一码，测试断言 severity、固定消息、精确主 Span、必要 opener 标签
      和恢复后的 AST；复用 `L0009`、`L0010`、`L0013`、`L0014` 的路径不产生同义重复码。
- [ ] Lexer poison 在声明名、参数名、TypeRef 和 initializer 位置只产生词法根因；另一处独立
      Parser 错误仍保留，并按生产诊断全序确定排序。
- [ ] 单声明后的第二个声明、换行分隔声明与 block body 均以 `L0013` 拒绝；明确由
      SPEC-0009 首次加入 block / 函数 block body，由 SPEC-0011 组合完整文件并跨声明同步。
- [ ] parser-declaration pass / fail suite 各自真实执行至少一个 `.ko`；零用例与非法 sidecar
      自检仍会失败。sidecar helper 自检证明 Parser policy 接受 `start == end`，Lexer policy
      仍拒绝空 Span，parser 合并 sidecar 中的 Lexer 码也不得借 Parser policy 接受空 Span；
      两者都拒绝反向、越界和非 UTF-8 边界；现有 parser-expression fixture 无回归。
- [ ] 深层 TypeRef、typed call、函数签名与表达式体共享固定 32 MiB worker 和 1024 递归预算；
      超预算受控返回内部错误，不 panic、不泄漏计数。
- [ ] `cargo tree -p lang-frontend --edges all --locked --offline` 与 manifest / lock diff 证明
      未新增 normal、dev 或 build 依赖。
- [ ] frontend 窄测试和 workspace fmt、check、Clippy、test、CLI build 基线全部通过；实际
      passed / ignored / filtered 数量写入完成记录。
- [ ] Architecture 更新为已实现的声明入口、具体 Item AST、typed call 与共享 Parser 内核，
      并继续明确 block / statement / class-family 属于 SPEC-0009，完整文件组合与跨声明恢复
      属于 SPEC-0011，均尚未实现。

## 6. 技术方案与边界

拟议的最小公共 API 如下；准确命名可在实施时依照现有 Rust 风格微调，但不得改变所有权
和可观察结果：

```rust
pub type SyntaxAst = AstFile<Item, (), Expression, TypeRef>;

pub fn parse_declaration(
    sources: &SourceMap,
    lexed: &LexedFile,
) -> Result<ParsedDeclaration, ParserInternalError>;

pub struct ParsedDeclaration {
    ast: SyntaxAst,
    root: ItemId,
    diagnostics: Vec<Diagnostic>,
}
```

`ParsedDeclaration` 只暴露 `source_id()`、`ast()`、`root()` 与 `diagnostics()` getter。现有
`ParsedExpression` 继续暴露相同形态，其 AST alias 应迁移到同一个 `SyntaxAst`，避免表达式
入口与声明入口形成互不兼容的具体 AST 类型。

Parser 模块建议保持最小边界：

- `parser::engine`：共享 cursor、资源预算、词法 recovery index、Pratt、TypeRef 与诊断；
- `parser::declaration`：独立声明入口、Item 构造、参数 / 泛型列表及声明局部恢复；
- `parser::trial` 或等价私有小组件：只读识别完整 call type arguments，不构造 AST / 诊断；
- `parser::mod`：公共 payload、产物、入口与内部错误，不把 engine 细节公开。

若拆模块会迫使大量 parser 私有状态公开，实施可先把 declaration / trial 保持在 engine 内的
小函数组；模块化目标是单一职责，不应为一次使用建立通用 parser-combinator facade。

typed call 应继续是 postfix `Call` payload，而不是新的根表达式。正式提交时复用现有
TypeRef parser；trial 只验证 token 形状和匹配边界，成功后从原 cursor 进行唯一一次正式
解析。AST 插入继续使用 `AstFile` 的 source-checked API，诊断继续使用现有 catalog 与排序。
strict trial 的一次性预索引 / memo 与声明恢复的 `LexicalRecoveryIndex` 都只能是单次 Parser
产物构造期间的私有辅助状态；不得进入公共 AST / API，也不得跨 `LexedFile` 复用。测试插桩
只统计 token inspection，不成为 release API 或语言可观察行为。

## 7. 实施计划

1. [ ] 在 v0.7 生效后注册 `L0017`–`L0027`，扩展共享具体 AST、`NameMarker`、Item payload 与
   声明产物 API
   → 验证：diagnostic catalog、三态名称恢复 AST、typed ID、source identity、public getter 窄测试
2. [ ] 实现变量 / 常量 / 函数、类型参数 / 值参数及局部恢复
   → 验证：声明结构、Span、LexicalRecoveryIndex owner 边界、单调扫描及 poison 去重集成测试
3. [ ] 实现总 inspection 为 `O(N)` 的无副作用 typed-call trial 与正式 postfix commit，扩展
   Call payload
   → 验证：歧义矩阵、对抗倍增、nested TypeRef、trivia、缓存 depth、无残留 AST / 诊断与
   资源计数测试
4. [ ] 接入 parser-declaration pass / fail fixture，保持现有 suite 回归
   → 验证：真实 fixture、零用例、Parser / Lexer 空 Span policy、sidecar 和诊断全序测试
5. [ ] 同步 Spec 验收记录与 Architecture
   → 验证：workspace 全基线、依赖树、staged diff 与文档事实一致

并行实施边界：步骤 1 确定公共 payload 后，声明解析与 typed-call trial 可分别开发；fixture
harness 可在 suite 名称和产物 getter 固定后并行接入。共享 `engine`、诊断目录和 `Call`
payload 的最终整合由单一负责人完成，避免多个分支同时改同一 Pratt 热点。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | v0.7 guide 候选、用户启用后的真源切换；与实现分离 | `docs(guide): activate language guide v0.7` |
| 2 | Item AST、声明入口、typed call、诊断、测试 / fixture、Architecture 与完成记录 | `feat(frontend): add declaration parser (SPEC-0008)` |

本草案及候选 guide 可先作为纯文档变更审阅，但不得把本 Spec 状态推进到 `approved` 或将
候选语义写入 Architecture 已实现事实。最终实现提交必须保持单一 SPEC-0008 边界。

## 9. 未决问题

- 唯一门禁是候选 v0.7 尚未由用户明确启用。若用户修改候选产生式，应先同步本草案再批准。
- 无需 ADR：本 Spec 复用既有四表索引 AST、诊断模型、Parser worker 与 crate 边界，不改变
  workspace 依赖方向或长期编译流水线。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| v0.7 启用门禁 | 未满足 | 候选 guide 未生效，本 Spec 保持 `draft` |
| frontend 声明 Parser 窄测试 | 未执行 | 尚未授权实施 |
| parser-declaration fixture | 未执行 | 尚未创建实现与 fixture |
| `cargo fmt --all -- --check` | 未执行 | 当前仅起草 Spec；完成时补录 |
| `cargo check --workspace --all-targets --locked --offline` | 未执行 | 当前仅起草 Spec；完成时补录 |
| `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | 未执行 | 当前仅起草 Spec；完成时补录 |
| `cargo test --workspace --all-targets --locked --offline` | 未执行 | 当前仅起草 Spec；完成时补录 |
| `cargo build -p lang-cli --locked --offline` | 未执行 | 当前仅起草 Spec；完成时补录 |
