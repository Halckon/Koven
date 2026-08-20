# Koven 语言设计规范 · 语法规范（三）：调用参数、Lambda 与解构

> 本文档是 Koven 语言设计规范多文档结构的一部分（原单文件 guide 第四部分 §9），完整
> 文档地图、版本治理规则与跨文件索引见 [`00-index.md`](./00-index.md)。内容版本：v0.14。
> 保留原节号 §9 以维持既有 SPEC 引用不变。SPEC-0012 已实现；本节下一项是 SPEC-0013
> 的局部 `val` 解构。共享的表达式/类型引用基础见
> [03-grammar-core.md](./03-grammar-core.md)，声明/block 语法见
> [04-grammar-declarations-blocks.md](./04-grammar-declarations-blocks.md)。

## 9. SPEC-0010 至 SPEC-0013：lambda、隐式 `Unit`、typed call argument 与局部 `val` 解构

四项能力复用既有 expression、statement 与声明基础，但不是一个实现 Goal。SPEC-0010 只交付
lambda，SPEC-0011 只交付具名函数隐式 `Unit` 返回标注。为解除 v0.9 明确留下的 callable
门禁，SPEC-0012 的单一 Goal 是**调用边界语法**：同步交付具名函数 / 函数类型参数 marker 与
typed call argument；不包含 Phase 2 / 3 合法性检查。SPEC-0013 只交付局部 `val` 解构；每项
均须独立验收和提交，后项不得反向扩大前项。

lambda、typed argument 与解构三项结构 parser 都必须接收调用方的 hard stop 集合，并把自身
真实 closer 作为新增 owner。恢复按以下固定优先级处理边界，不能用“所有局部 owner 退出后
才看 hard stop”的笼统规则代替：

1. 当前 token 匹配局部 delimiter / lexical owner 栈顶 closer 时，先消费并 pop；
2. 否则，只有 lexical-owner stack 已回到本次恢复入口 baseline 时，EOF 或与普通 delimiter 栈顶
   异形的调用方 hard closer（`)`、`]`、`}` 等）才立即停止并保留；它可以抢占未闭合的普通
   delimiter，这些局部 delimiter 随当前 error region 结束，例如 `f({ a[x )` 中 `)` 必须留给
   call owner。若仍处于本次扫描新开的 string / interpolation owner 内，其中的同形 token
   必须先由该 lexical owner 消费，不能越过它关闭外层 call；`StringEnd` / `InterpolationEnd`
   若正是入口 baseline owner 的真实 closer，则按[04-grammar-declarations-blocks.md](./04-grammar-declarations-blocks.md)第 7 节作为调用方 hard closer 保留；
3. 同形 `}` 同时可关闭最内层 lambda / block owner 时，最内层 owner 优先消费。例如只有一个
   `}` 的嵌套 lambda / block 输入先关闭 lambda，外层 block 随后报告缺 closer；
4. 逗号、下一 argument / element 候选等 soft stop 只有在局部 delimiter 与 lexical owner 回到
   进入恢复时的 baseline 后才生效。

三项结构恢复统一复用第 7、8 节在 parser 构造时一次建立的 terminal-owner event index；遇到
Lexer `L0004`–`L0006` 的开始、恢复结束及抑制事件时按 owner 栈推进，不在每个 lambda、实参
或解构错误处重扫诊断。

### Lambda literal（SPEC-0010）

```ebnf
lambda_expression  = [ "move" ], "{", lambda_body, "}" ;
lambda_body        = [ lambda_header ], { lambda_element } ;
lambda_header      = [ lambda_parameter,
                       { ",", lambda_parameter } ], "->" ;
lambda_parameter   = Identifier ;
lambda_element     = local_variable_statement
                   | expression_statement
                   | nested_block_statement ;
```

Lambda 是 expression primary，可以继续接受 call、member、index 等 postfix。expression 已经
要求一个 primary 时，`{` 提交 lambda；block dispatch 在 element 起点直接看到 `{` 时仍提交
nested block。因而 `val f = { x }` 的 initializer 是零参数 lambda，body 尾值为 `x`；`{ x }`
作为外层 block 的直接 element 则是 nested block。需要在 statement 位置强制表达 lambda 时可写
`({ x })`。`move { ... }` 只能是 lambda。trailing lambda（`f { ... }`）不属于本节，调用仍须
写 `f({ ... })`。

[04-grammar-declarations-blocks.md](./04-grammar-declarations-blocks.md)第 8 节的 `{` soft element stop 因此是 parser-state-sensitive 的：已有完整左表达式且没有运算符
要求右 operand 时，顶层 `{` 留给下一 nested-block element；initializer 起点、prefix / binary
右 operand、grouped expression 或 call argument 等正在等待 primary 的位置则必须让 `{` 进入
lambda parser。`x { y }` 是 expression statement `x` 后接 nested block，`x + { y }` 的右侧则
是 lambda。该判定只依赖语法状态，不依赖 trivia、名称或推测类型。

`->` 前允许零个或多个逗号分隔的普通 Identifier；`{ -> e }` 是显式零参数形式。参数不接受
类型、默认值、`val` / `var`、模式、解构或 trailing comma。Header 只能从 `{` 后第一个非
trivia token 起严格匹配完整前缀 `[ Identifier { "," Identifier } ] "->"`；只有整个前缀
成功才提交。任一 token 不匹配就以零状态失败，并从 `{` 后按零参数 body 解析，不得继续搜索
后方任意顶层 `->`。因此 `{ source as () -> Int }` 中函数类型的箭头绝不会反向把 `source as ()`
误判为 lambda 参数，`{ x y -> z }` 也不是可恢复 header，而是带非法 body token 的零参数
lambda。试探 DFA 只跳过 trivia；遇到任何 delimiter / string opener 或其他不属于普通
Identifier、参数逗号、最终 `->` 的 token 时立即永久判为 no-header，不能进入 nested owner 后
继续搜索箭头。全流索引仍负责维护共享 delimiter / lexical-owner 栈。试探不分配 AST、不发
诊断、不改变 cursor。

Body 复用[04-grammar-declarations-blocks.md](./04-grammar-declarations-blocks.md)第 8 节的三种 element 和最大 element / 显式 stop 规则，但使用独立 lambda-body
payload，不能复用静态类型固定为 `Unit` 的 `Statement::Block`。若最后一个 element 是
expression statement，该 expression 是 lambda 的尾值；空 body 或最后一项为局部声明 / nested
block 时尾值为 `Unit`。这里不创造隐式 statement separator：普通 expression-start 仍不是
局部声明 initializer 的 stop，所以 `{ val x = 1 x }` 必须作为 initializer 尾随输入报错，不能
把 `x` 改判为第二项 tail expression。当前阶段若要使用普通 tail expression，它必须是 body
首项，或位于一个已有真实 `}` 结束的 nested block 之后；不承诺“任意局部声明序列 + tail
expression”。Phase 1 只保存该结构；参数类型、捕获、返回类型与 `move` 合法性由 Phase 2 / 3
检查。`return` 等控制流仍不属于本节。

Lambda 参数源码不重复书写 `borrow` / `inout`；其契约由 Phase 2 对该 lambda 应用的
**期望函数类型**逐项提供。例如把 `{ x -> use(x) }` 检查为 `(borrow T) -> R` 时，body 中的
`x` 是共享借用参数；检查为 `(T) -> R`（无标记）时，`x` 是按值参数，`Copyable` 时对应
复制、否则移动。每个重载候选必须用自身
期望函数类型独立检查 lambda，不能先默认成 `Value` 再做隐式模式转换；若无法得到唯一的参数
类型 / 契约，沿用普通 lambda 上下文类型不足或重载歧义诊断。Phase 1 的 Lambda AST 仍只保存
真实参数名 Span，不伪造 marker；typed AST 必须保存最终采用的函数参数契约。`move` 只约束
捕获，与期望函数类型的参数契约正交。

lambda body 在最大 expression 已完整、没有子语法等待 token，且 delimiter / lexical owner
回到 body baseline 时，额外把顶层 `,` 与 `->` 作为 body-dispatch soft stop。它们只把控制权
交回当前 lambda body，不得泄漏成外围 call 的 argument separator。call、group、function type
或其他 nested owner 内的 `,` / `->` 不受影响，因此 `{ source as () -> Int }` 仍是单个完整
尾表达式。

AST 至少等价保存 `move_span: Option<Span>`、有序参数名称 Span、`arrow_span: Option<Span>`、
有序 `StatementId` body 和可判定的 tail expression。完整 lambda Span 从真实 `move`（若存在）
或 `{` 起至匹配 `}` 终；缺 `}` 时止于最后实际消费位置。由于 header 只在严格完整匹配后
提交，参数均为真实 Identifier，不存在 missing / error 参数 marker；header Span 从首参数
（零参数时从 `->`）至 `->` 终。body element 沿用[04-grammar-declarations-blocks.md](./04-grammar-declarations-blocks.md)第 8 节范围，不为缺失 token 伪造非空 Span。

`arrow_span == None` 精确表示没有 header，此时参数必须为空；参数非空时必须存在真实
`arrow_span`，而“参数为空且有真实 `arrow_span`”唯一表示 `{ -> ... }`。lambda body ID 必须
指向 `Statement::LambdaBody`；该 variant 不能直接成为 Block / LambdaBody 的 element，也不能
成为孤儿。strict probe 失败时参数为空、arrow 为 `None`，失败 token 只能进入 body 的
Expression / Error statement。

SPEC-0010 从 L0031 开始分配自身专用的 expected lambda body element 与 unsupported lambda
body form 类别，不复用声明列表的 L0024–L0026。前者只用于 `}` / 调用方 hard stop 之前真实
存在且不能开始任何合法 element 的普通 token。后者由 body dispatch 发出：无论 strict header
是否成功，当前 lambda-body baseline 的 `,` / `->`，以及 `return`、局部 `fun`、`const val`、
控制流或 class-family 等本阶段明确延后的 element introducer，都使用 unsupported lambda body
form；普通 token 每次精确消费一个，`const val` 可消费固定前缀，并形成同范围 Error statement。
`L0030 unsupported block element` 只用于普通 `Statement::Block`，不能在 LambdaBody 复用；
`move` 未后接 `{` 仍按既有 expected-expression 根因处理。`{}` 与 `{ -> }` 都是合法空 body，
绝不发 body 诊断。缺 `}` 复用通用 expected closing delimiter。body 恢复保留当前 lambda 的
`}`；nested block、调用、索引、字符串和插值各消费自己的 closer。同形 `}` 必须先关闭当前
最内层 lambda / block，不能越过未闭合 lambda 交给父 block；只有局部栈顶为 `)`、`]` 等异形
frame 时，调用方 `}` 才作为 hard closer 被保留。

Header 识别不得从每个 `{` 向前或向后独立扫描。parser 构造时必须在整个 lexeme / terminal
event 流上做一次 `O(n)` 预索引：共享 delimiter / lexical-owner 栈，并只让每个 `{` owner 的
小型 DFA 从其紧随的首个非 trivia token 开始识别上述严格前缀；首个不匹配 token 立即把该
owner 永久记为 no-header，后续箭头不再考虑。DFA 成功时记录参数 token 与 `->` raw index，
正式 parser 以 opener raw index 做 `O(1)` 查询。也可采用完全等价的共享 memo，但每个 raw
lexeme 在所有 header trial 中合计只能访问常数次。缺 lambda `}` 时，正式 parser 在最早的
调用方 hard stop 停止，即使预索引的词法范围延伸得更远也不得越界。每轮要么消费 lexeme，
要么在自身 `}`、调用方 hard stop 或 EOF
结束，整体 `O(n)`、owner 栈 `O(d)`。

### 具名函数隐式 `Unit` 返回标注（SPEC-0011）

具名 `fun` 的参数列表闭合后按以下互斥顺序提交：真实 `:` 提交 `FunctionForm::Explicit` 并
继续解析任意 body 形态；直接 `{` 提交 `FunctionForm::ImplicitUnitBlock`；直接到独立入口
EOF 或调用方无体 stop 提交 `FunctionForm::ImplicitUnitAbsent`；直接 `=` 则仍发既有 expected
explicit return type，构造显式 Error TypeRef 后保留并解析 expression body。若当前位置明显
可开始 `type_ref` 但缺 `:`，继续使用既有缺分隔符恢复并构造显式标注；已有 `:` 但缺类型仍
使用 expected type reference。参数表后的 Lexer invalid / reserved token 或 segmented
string / interpolation poison 若没有真实 `:`，不得被猜成显式返回类型：parser 先提交
`ImplicitUnitAbsent`，再由独立声明 / 未来容器的 trailing owner 消费该 poison；只保留既有
Lexer 根因，不在同一 `Span` 追加 expected explicit return type 或 trailing-token 诊断。
terminal Lexer 根因抵达 EOF 时同样提交 `ImplicitUnitAbsent`，不得派生 Parser 诊断。

函数 item 的返回来源与 body 必须是同一个封闭状态，至少等价于：

```text
FunctionForm::ImplicitUnitAbsent
FunctionForm::ImplicitUnitBlock(StatementId)
FunctionForm::Explicit {
    colon_span: Span,
    type_ref: TypeRefId,
    body: FunctionBody,
}
```

两个 implicit variant 都不拥有虚构 TypeRef 或 `:` Span；Phase 2 把它们解析为内建 `Unit`。
显式 `: Unit` 保持 `Explicit`，以便工具和诊断忠实反映源码。`FunctionBody` 只嵌在显式分支，
因而类型上不能构造 implicit expression body。省略标注的无体 item Span 到真实 `)` 终，block
body item Span 到 block 终。此 Spec 不改变 lambda、函数类型或构造器，不从 body 推导返回
类型，也不放宽 `fun f() = expression`。

### Typed call argument（SPEC-0012）

```ebnf
call_suffix       = "(", [ call_argument,
                            { ",", call_argument } ], ")" ;
call_argument     = [ named_argument_prefix ], [ argument_mode ], expression ;
named_argument_prefix = Identifier, "=" ;
argument_mode     = "borrow" | "&" ;
```

调用实参的 `argument_mode` **自 v0.14 起不再复用**声明侧的 `explicit_parameter_mode`
（`"borrow" | "inout"`，见[03-grammar-core.md](./03-grammar-core.md)第 3 节）：声明侧继续用关键字 `inout`
标注一个参数是可变借用，但调用点表达“我要把这个 place 借给这次调用做可变修改”时改用
符号 `&`，不再重复关键字 `inout`。`borrow` 在两侧都仍是同一个关键字，没有变化。

唯一源码顺序是“可选名称、可选模式、表达式”：`f(e)`、`f(name = e)`、`f(borrow e)`、
`f(&e)`、`f(name = borrow e)` 与 `f(name = &e)` 在 Phase 1 均可形成语法 AST。模式不是通用一元运算符；
只有 call argument 入口可消费，且字母表精确为关键字 `borrow` 与符号 `&` 两项（v0.12
移除 `own`，v0.14 把 `inout` 关键字换成 `&` 符号，见下文设计说明）。模式后直接出现顶层
`Identifier =` 是错误的逆序组合；显式分组的
`borrow (x = y)` 仍是以 assignment expression 为 operand 的模式实参，`&(x = y)` 同理。
空列表合法，
trailing comma 继续非法。Phase 1 只保留源码顺序和真实 marker；名称映射、参数契约匹配与
operand 的 place / temporary 分类按下文分别留给 Phase 2 / 3，parser 不按 callee 名称或
operand 形态改变语法。

#### 调用点自动化的设计说明（v0.12 / v0.14）

v0.10 曾为 `Value`、`Own`、`Borrow`、`Inout` 四种契约都定义了调用点标注规则，其中
`Own`/`Borrow` 要求调用者对“已有 place”实参显式写出 `own`/`borrow`，理由是“看一眼调用点
就知道会不会拿走所有权”。实践反馈是：这个好处没有覆盖住它的代价——尤其是构造容器
元素（`listOf(own x)`）、装箱（`Box(own x)`）、发送 channel（`sender.send(own x)`）、
索引借用（`use(borrow xs[i])`）这类高频调用，标注几乎总是可以从 callee 已经声明的契约里
唯一确定，人工重复写出并不提供额外信息。

v0.12 的结论：

- **`Value` 契约维持不变**——调用点从 v0.10 起就从不要求标注，本次没有变化。
- **取消独立的 `Own` 契约，并入 `Value`**：`Own` 和 `Value` 在运行时语义上本来就完全
  相同（`Copyable` 复制、否则移动），唯一区别只是调用点是否强制标注；既然强制标注被
  取消，两者不再有任何可观察差异，保留两个名字纯属额外的概念负担。
- **`Borrow` 契约的调用点标注从“已有 place 必须写”改为始终可选**：v1 的借用生命周期
  被严格限定在“本次同步调用期间”（没有 place 返回、没有跨调用生命周期，见
  [01-design-decisions.md](./01-design-decisions.md)第 8 节），编译器在 Phase 2 解析 callee 签名时已经确定性地知道某个参数是否为借用，
  不需要调用点重复确认就能正确生成移动 / 复制 / 借用代码，也不产生新的歧义。`borrow`
  关键字保留，调用者仍可选择显式写出以提高可读性，两种写法生成完全相同的 AST 语义。
- **`Inout` 契约的调用点标注保持强制，是唯一的例外**：`Inout` 标记的是“这次调用可能
  静默修改一个仍然属于调用者的变量”，这与单纯的所有权转移或只读借用不同——它是三类
  契约里唯一可能产生调用者未曾预期的副作用的一种。Rust（`&mut`）、Swift（`inout` 搭配
  调用点 `&`）等在其余部分选择精简调用点标注的语言，都不约而同地保留了这一项。v1 的
  所有权检查是“简化版单一所有者 + ASAP 析构，不做完整 NLL”（见 Phase 3），没有比这更
  强的数据流分析兜底，调用点的 `Inout` 标注因此同时服务于人类可读性与编译器实现简单性
  两个目的，取消它的收益不成比例地小、风险不成比例地大，故 v0.12 不改动这一点。
- **重载歧义的边界情况**：`Borrow` 放开后，若一个未标注的已有 place 同时匹配某个
  `Value` 候选与某个 `Borrow` 候选，会出现候选重叠。这不是新问题——未标注 temporary
  参数在 v0.10 起就可能同时匹配 `Value`/`Own`/`Borrow` 候选，下文“Phase 2 先按源码顺序
  解析”一段已经给出了兜底：类型检查后仍有多个候选时使用普通歧义诊断。v0.12 只是把这条
  既有兜底的适用范围从“临时值”扩大到“已有 place”，不引入新的消歧义机制。

v0.14 在保持“`Inout` 调用点标注仍是强制项”这一结论不变的前提下，进一步收窄了它的书写
成本：

- **调用点的 `Inout` 标注从关键字 `inout` 改为符号 `&`**：`mutate(inout x)` 改写为
  `mutate(&x)`。这一步照抄的正是上一条已经引用过的 Swift 先例——Swift 的 `inout`
  参数在调用点只写一个 `&`，不重复整个关键字；语言里其余情况（含蓄的按值/借用传递）
  完全不需要标注。对 Koven，这让“要不要写、写什么”这两件事进一步解耦：现在唯一需要
  记住的规则是“调用一个会修改你变量的函数，在实参前加一个 `&`”，不需要记住一个完整
  单词。安全性质不受影响——`&` 依旧是强制项、依旧只对可变 place 合法，[06-roadmap.md](./06-roadmap.md)
  Phase 3 的所有权检查规则不变，改变的只是这个强制标注本身的书写重量。
- **声明侧的关键字 `borrow` / `inout` 都不受影响**：
  `fun inspect(borrow x: Point): Unit`、`fun mutate(inout x: Point): Unit`、`(borrow T) -> R`
  与 `(inout T) -> R` 继续显式声明对应契约，理由见
  [03-grammar-core.md](./03-grammar-core.md) 第 3 节——声明侧标注的是“这是所有权模型本身需要的信息”，
  不是“调用点书写负担”。v0.12 只让调用点 `borrow` 可省，v0.14 只把调用点
  `inout` 改写为 `&`；两版都没有精简声明侧。
- **`&` 是新增的固定符号，不是既有 token 的复用**：v1 此前没有单字符 `&`（只有 `&&`），
  这次改动为它在[02-lexical-spec.md](./02-lexical-spec.md)第 7 节新增一个词法 token，唯一合法语法位置是本节
  的调用实参入口；`&` 不参与任何通用 prefix / binary 表达式层级，v1 也不提供按位与运算符
  （该用途留给 v2，交互边界见[01-design-decisions.md](./01-design-decisions.md)第 10 节）。这是本次改动里唯一触及
  已实现 Lexer（SPEC-0006）的部分——`own` 的语法用途退役时特意选择了不动词法层，但 `&`
  从“非法字符”变成“合法固定符号”没有办法绕开这一层；
  改动本身很小（复用现有最长匹配机制新增一个前缀字符），但性质上确实是对已验收产生式的
  修改，需要按治理规则在变更记录里如实标注，不能归为“零风险”改动。
- **`&` 在 Koven 里的含义对 Rust 背景的开发者是一个陷阱，需要在教程里显式提醒**：Rust
  的 `&x` 表示共享借用，可变借用要另写 `&mut x`；Koven 的 `&x` 直接表示独占可变借用
  （对应 Rust 的 `&mut x`），共享借用（对应 Rust 的 `&x`）反而是不需要任何符号的默认
  行为。这是照抄 Swift 而不是 Rust 的直接后果——Swift 的 `&` 本来就只用在 `inout`
  上，没有“共享借用也要写符号”这一层，Koven 继承的是这个二元结构，不是 Rust 的三元
  结构（无标记按值 / `&` 共享 / `&mut` 独占）。这个语义反转（同一个符号，在两门看起来
  都是“Rust 式所有权”的语言里指向相反的默认值）比“调用点标注变简洁”本身更容易让 Rust
  背景的读者踩坑，教程文档需要用类似[01-design-decisions.md](./01-design-decisions.md)第 3 节 `error()` 的处理方式——单独一段
  显式提醒，不能只靠上下文让读者自己反应过来。

三种契约的完整定义仍由本节剩余部分与下文的 Callable 参数契约与调用匹配 给出；`own`
关键字的无产生式状态见 [02-lexical-spec.md](./02-lexical-spec.md) 第 1 节，`&` 符号的完整词法定义见
[02-lexical-spec.md](./02-lexical-spec.md) 第 7 节。

#### Callable 参数契约与调用匹配

callee 的每个值参数具有 `Value`、`Borrow` 或 `Inout` 契约。无标记参数是 `Value`；声明侧
显式关键字 `borrow` / `inout` 分别是另外两种契约。`Value` 接收普通按值表达式结果，满足 `Copyable`
时复制，否则移动；调用点从不要求 marker。`Borrow` 表示调用期间共享借用，调用点标注
可选——省略时编译器按 callee 已声明的契约自动借用，显式写 `borrow` 时效果相同，仅用于
强调。`Inout` 表示调用期间独占可变借用，调用点**必须**显式标注，但拼写是符号 `&`，不是
声明侧使用的关键字 `inout`（`mutate(&x)`，不是 `mutate(inout x)`）。默认参数**模式**
不等于默认参数**值**；v1 继续禁止参数默认值与用户声明 `vararg`。

调用点与 callee 契约的唯一兼容矩阵如下；“temporary”是本次 operand 求值新产生且没有既有
place 身份的值，group 继承内部表达式的类别。名称、成员或索引只有在 Phase 2 把它标为
place 时才属于 place；其他产生完整值的表达式均为 temporary。

| 调用点形态 | `Value` | `Borrow` | `Inout` |
|---|---|---|---|
| 无 marker、operand 为 place | 合法，按值复制或移动 | 合法，自动借用，借到同步调用结束 | 非法，缺 `&` |
| 无 marker、operand 为 temporary | 合法，直接交付 | 合法，自动借用，借到同步调用结束 | 非法，始终要求可变 place |
| `borrow operand` | 非法 | 合法；与省略标注语义完全相同，纯粹可选 | 非法 |
| `&operand` | 非法 | 非法 | 仅 operand 为可变 place 时合法 |

`Copyable` 只决定按值交付是复制还是移动，不改变矩阵，也不让 `Inout` place 省略 marker。
显式 `borrow temporary` 合法；显式 `&temporary` 非法。place、可变性、Copyable、移动
与借用冲突由 Phase 3 判断，Phase 1 不据此拒绝语法。

Phase 2 先按源码顺序解析每个 argument 的类型和表达式类别，再对每个候选 callable 做映射：位置实参可以
出现在第一个命名实参之前，并依次填充尚未匹配的参数；一旦出现命名实参，后续所有实参都
必须命名。名称按大小写敏感的源码 Identifier 精确匹配，只允许直接解析到具有稳定参数名的
具名 callable；通过函数值、callable reference 结果或其他只拥有函数类型的 callee 调用时，
命名实参是类型诊断。一个参数不能被位置与名称重复填充，同一名称不能出现两次；v1 没有
默认参数值，所以成功调用必须恰好填充全部参数且没有额外实参。每个重载候选独立应用这套
映射与契约约束；无 marker 的 place 或 temporary 都可直接与 `Value` 或 `Borrow` 候选兼容，
显式 `borrow` 只能与 `Borrow` 候选兼容。若类型检查后仍有多个候选，使用后续 Phase 2 Spec
的普通歧义诊断，不能通过重排求值或忽略契约择一。这里的表达式类别只区分类型层面已经
建立的 place 与 temporary；Phase 2 不判断 place 此刻能否移动、借用或独占访问，这些动态
所有权前提仍全部属于 Phase 3。无论名称把实参映射到哪个参数，operand 始终按**源码从左到右**各求值一次，
随后才按映射交付；命名顺序不改变副作用顺序。

所有编译器预声明 callable、核心构造器与后续标准库签名都必须使用同一有序参数元数据：
参数契约、可选稳定名称及 TypeRef。列表式核心构造可使用内部“重复 `Value` 参数”形状，仍不
向用户开放 `vararg` 声明语法；parser 始终生成普通 CallArgument，不按 `arrayOf`、`listOf`、
`MutableList.add`、`println` 等名称硬编码模式或省略例外。预声明 API 的具体签名由对应标准库
Spec 列出，但其调用必须服从上述统一匹配矩阵。

为使本 guide 已有示例在后续标准库 Spec 之前也具备唯一契约，以下预声明 / 核心 API 的参数
模式现在固定；后续 Spec 可以补充具体重载和实现，但不得改变这些位置的契约：

| API 形状 | 有序参数契约 |
|---|---|
| `error(message)`、`println(value)` | 唯一参数为 `Value`；`println` 可有多个具体类型重载，但契约相同 |
| `Box<T>(value)` | `Value T` |
| `arrayOf(...)` / `listOf(...)` / `mutableListOf(...)` | 每个元素位置都是内部重复的 `Value T` |
| `Array<T>(size, initializer)` / `List<T>(size, initializer)` | `Value` 的 `Int`、`Borrow` 的 `(Int) -> T` |
| `MutableList<T>.add(value)` | `Value T` |
| `thread(task)` | `Value (move () -> Unit)` |
| `channel<T>()`、`join()`、`receive()` | 无参数 |
| `Sender<T>.send(value)` | `Value T` |

其中 `(Int) -> T` 的 `Int` 参数无 marker，因此是 `Value`；表格中的“的”只区分参数契约与
参数类型，不是额外源码语法。上表所有 `Value` 与 `Borrow` 参数在调用点都不需要标注
（`Borrow` 位置仍可选择写 `borrow` 强调）。`println` 的可打印类型集合、channel / thread 的具体返回类型及普通集合算法仍由对应 Phase 2 / 5
Spec 完成；这些留白不允许改变上表的模式或让 parser 按名称特判。

这套契约解除 v0.9 的 SPEC-0012 设计门禁。SPEC-0012 的 Phase 1 Goal 同时迁移具名函数
`ValueParameter`、函数类型参数和 `CallArgument` AST / parser，使三处共享同一
`ParameterModeMarker` 语义枚举（`Borrow`/`Inout` 两个变体）——**但自 v0.14 起三处的
调用点表面拼写不再完全相同**：`ValueParameter` 与函数类型参数（声明侧）用关键字
`borrow` / `inout`；`CallArgument`（调用点）用关键字 `borrow` 与符号 `&`。Span 各自
覆盖源码中实际出现的 token，语义枚举变体相同，surface spelling 因位置而异；Phase 2
实现名称、参数映射、类型与参数契约匹配并标记
类型层面的 place / temporary 类别，Phase 3 才实现具体 place 的移动能力、可变性、复制 /
移动与借用效果。不得在 SPEC-0012 中提前声称名称、类型或所有权检查已经完成。

`Expression::Call` 的 `arguments` 字段唯一改为 `Vec<CallArgument>`，不得再建
`CallArgumentId`、第五张 AST table 或同时保留旧 `Vec<ExpressionId>`。`CallArgument` 是内嵌
payload，至少保存完整 `span`、`named_prefix: Option<NamedArgumentPrefix>`、
`mode_marker: Option<ParameterModeMarker>` 及唯一 `value: ExpressionId`；
`NamedArgumentPrefix` 封闭保存真实 `name_span` 与 `equals_span`，不能用两个独立 `Option` 构造
只有名称或只有等号的半状态。mode marker 复用[03-grammar-core.md](./03-grammar-core.md)第 3、6 节封闭的
`ParameterModeMarker::{Borrow(Span), Inout(Span)}`（v0.12 起不再有 `Own` 变体），自身封闭
真实 kind / Span——**在 `CallArgument` 里，`Inout(Span)` 变体的 `Span` 覆盖的是符号 `&`
token，不是关键字 `inout` token**（value_parameter / function_type_parameter 的
`Inout(Span)` 才覆盖 `inout` 关键字）；两处共享同一枚举变体名是因为语义相同，实现读取
`Span` 时不能假设它一定是某个固定字符长度的 token。完整实参从名称或模式（存在时）
否则 operand 起，到 operand 终；恢复时只到最后实际消费位置，空 Error value 的 boundary
插入点不扩大 argument 范围。call 与 typed call 的既有合成
Span 不变。错误 operand 只覆盖实际消费区域，或在 call / 调用方 hard stop 处为空范围。

SPEC-0012 在现有 L0032 后连续分配六类：`L0033 expected argument value`、
`L0034 expected argument separator`、`L0035 unsupported argument empty element`、
`L0036 unsupported argument trailing comma`、`L0037 invalid argument mode ordering` 与
`L0038 duplicate argument mode`；固定消息就是各英文类别拼写，主 `Span` 按下表。参数声明 /
函数类型的 `L0039 duplicate parameter mode` 由上节定义。这六类诊断的触发条件与恢复语义
不因模式字母表从三项收窄为两项而改变——`duplicate argument mode`（如
`f(borrow &x)`，关键字 `borrow` 后紧跟符号 `&`）与 `invalid argument mode ordering` 关注
的是模式 token 出现的数量与位置，与具体是哪个模式无关；`&x` 单独出现时 `&` 不算
“重复”，是本节字母表里合法的唯一 `Inout` 标注写法。`f(&&x)` 不落入这一类别——lexer 按
最长匹配把 `&&` 识别为单一 token（logical-and 的固定符号），不是两个相邻的 `&`，因此
parser 在 `argument_mode` 位置看到的是一个不匹配 `"borrow" | "&"` 的 `&&` token，按
`expected argument value` 处理；只有写成 `f(& &x)`（中间有 trivia）才会产生两个独立
`&` token，触发 `duplicate argument mode`。不得复用声明列表 L0024–L0026，`)`
缺失只复用通用 expected closing delimiter。恢复分支精确如下：

| 分支 | 诊断、消费与 AST |
|---|---|
| 初始位置直接 `)` | 合法空列表；不消费 `)` 之外的 token，不创建 argument |
| 初始 / separator 后直接 `,` | unsupported argument empty element 覆盖该逗号；消费逗号并追加一个在逗号起点为空的 Error value argument，再继续下一项；若随后直接 `)`，不为同一空项追加第二条诊断 |
| 已有完整项后的 `,` 紧接 `)` | unsupported argument trailing comma 覆盖并消费逗号；追加一个在 `)` 起点为空的 Error value argument，保留 `)` 给 call owner；本分支优先于一般 separator 消费 |
| 已提交 `Identifier =` 或首个 mode 后遇顶层 `,` / `)` / 调用方 hard stop / EOF | expected argument value 取该边界的空 Span；追加保留已消费 name / mode marker 的唯一 Error value argument，不再追加同根因 expected expression。若边界为 `,`，本分支消费它作为当前缺值项的 separator；紧接 `)` 时不得再报 trailing comma 或追加第二个 Error argument；其他 hard stop / EOF 不消费 |
| 连续第二个及以后 mode | 每个多余 mode 各发 duplicate argument mode 并消费；AST 只保存首个 mode，argument Span 仍覆盖所有已消费错误 marker 后的最终 value |
| mode 后直接出现顶层 `Identifier =` | invalid argument mode ordering 主 Span 覆盖 `=`；消费该 Identifier 与 `=` 作为错误区域，不保存 name marker，再从其后解析唯一 value |
| 实参起点，或已提交 name / mode 后遇不能开始 expression 的其他普通 token | expected argument value 覆盖首个非法 token；owner-aware 消费到当前 call 顶层 `,` / `)` 或调用方 hard stop 前，追加覆盖实际错误区的 Error value argument；Lexer poison 已有根因时只消费并建 Error value，不重复诊断 |
| 完整 value 后直接出现可开始下一 argument 的 token | expected argument separator 取该 token 起点空 Span且不消费；结束前一项并把该 token 作为下一项起点。该 soft boundary 只在 operand 已完整时生效，不能截断正在等待右 operand 的表达式 |
| 完整 value 后出现其他非法 token | expected argument separator 覆盖首个非法 token；按统一 owner 扫描消费到当前 call 顶层 `,` / `)` 或调用方 hard stop 前，不把错误区附会为下一实参 |
| 缺所属 `)` | lexical owner 回到本 call 入口 baseline 后，在最早调用方 hard stop / EOF 发 expected closing delimiter 并停止；未闭合普通 delimiter 不阻止该 hard stop，保留非 EOF 调用方 boundary |

所有扫描跟踪局部 `()` / `[]` / lambda `{}`、string / interpolation owner；顶层逗号只由 call
list 消费，所属 `)` 只由 call owner 消费，不能退化为按行同步。Lexer poison / terminal event
已表达根因时，只建立 Error value 并抑制同 Span 或同 owner closer 的 parser 级联。SPEC-0007
的 L0016 “unsupported argument form”历史负例在本 Spec 后迁移为正例；生产 parser 自此不再
产生 L0016，catalog 仅为已发布兼容性保留，不能复用其编号或含义。所有仍非法的实参形态均
落入上表六个专用类别或通用 expected closing delimiter。每个 raw lexeme 只由所属 argument
或 list owner 前进一次，单次 call 保持 `O(n)`。

### 局部 `val` 解构（SPEC-0013）

```ebnf
local_destructuring_statement
                       = "val", "(", destructuring_binding,
                         { ",", destructuring_binding }, ")",
                         "=", expression ;
destructuring_binding  = Identifier ;
```

仅当 block / lambda body dispatch 已消费 `val` 后看到 `(` 才提交解构。绑定列表至少一项，
只接受普通 Identifier；即使 Lexer 把源码拼写恰好为 `_` 的 token 归入 Identifier，解构 parser
也必须按原始 source slice 将它拒绝。`var`、`const val`、`_`、嵌套 pattern、binding 类型标注、
整个 pattern 的类型标注与 trailing comma 均不支持。重复名称和分量数量是否完整由 Phase 2
按源类型诊断。initializer 在 AST 中只出现一个 ExpressionId，从而锁定“右值只求值一次”；
复制式或消费式语义仍由 Phase 2 / 3 决定。

AST 唯一新增以下 statement variant；不得用等价 pattern table、`Item::Variable`、多个可选
variant 或第五张 AST table 代替：

```text
Statement::LocalDestructuring {
    val_span: Span,
    left_paren_span: Span,
    bindings: Vec<NameMarker>,
    right_paren_span: Option<Span>,
    equals_span: Option<Span>,
    initializer: ExpressionId,
}
```

两个 `Option<Span>` 只有真实 token 存在时为 `Some`；恢复不伪造 delimiter。statement 整体
Span 从 `val` 起到 initializer 或最后实际消费 token 终；pattern 的可观察范围从 `(` 起，正常
到 `)` 终，缺 closer 时到最后一个实际消费 binding / error token 终。每个 binding marker
遵守 present / missing / error 范围规则。

SPEC-0013 连续分配 `L0040` 至 `L0046`，依次表示专用的 expected destructuring binding、expected
destructuring separator、unsupported destructuring form、unsupported destructuring context、
unsupported destructuring trailing comma、expected destructuring initializer separator 与
expected destructuring initializer；不得复用声明列表 L0024–L0026。恢复分支精确如下：

| 分支 | 诊断、消费与 AST |
|---|---|
| local `val (` | 唯一提交 `Statement::LocalDestructuring`，后续错误仍保留该 variant |
| block / lambda body 的 `var (` 或 `const val (` | unsupported destructuring form 分别覆盖真实 `var` 或 `const val` 前缀；用 owner-aware 扫描消费本错误 element，形成 `Statement::Error`，不得构造 LocalDestructuring |
| 独立声明入口或未来文件顶层的 `val (` / `var (` / `const val (` | unsupported destructuring context 主 Span 覆盖真实 `(`；当前独立入口 owner-aware 消费到 EOF 并形成 `Item::Error`，SPEC-0014 文件入口则保留下一顶层声明 boundary |
| `(` 后直接 `)` | expected destructuring binding 取 `)` 起点空 Span，追加 Missing marker 并保留 `)`；空 pattern 不成为合法形式 |
| 期待 binding 时直接 `,` | expected destructuring binding 覆盖并消费逗号，追加位于逗号起点的 Missing marker，再继续下一项 |
| 期待 binding 时遇 `=`、element boundary、调用方 hard stop 或 EOF | expected destructuring binding 取边界空 Span，追加 Missing marker且不消费边界；随后按缺 `)` 分支继续，但不重复 binding 诊断 |
| binding 源码拼写 `_` | unsupported destructuring form 覆盖并消费该 token，追加同范围 Error marker |
| nested `(`、binding 后 `:` 或其他 unsupported binding form | unsupported destructuring form 覆盖首个引导 token；跟踪局部 owner 消费到当前 pattern 顶层 `,` / `)` 前，追加覆盖实际错误区的 Error marker |
| 完整 binding 后直接出现下一普通 Identifier | expected destructuring separator 取该 token 起点空 Span且不消费；结束前一项并从同 token 解析下一 binding |
| 完整 binding 后直接出现 `=` | 只按缺 `)` 分支处理，不追加 expected destructuring separator；保留 `=` 给 initializer |
| 完整 binding 后出现其他非法 token | expected destructuring separator 覆盖首 token；owner-aware 消费到顶层 `,` / `)` / `=` 或调用方 hard stop 前 |
| 完整 binding 后的逗号紧接 `)` | unsupported destructuring trailing comma 覆盖并消费逗号，不追加 missing binding，保留 `)`；本分支优先于一般逗号消费。若逗号本身已作为空 binding 报错，随后 `)` 不再追加同根因诊断 |
| 缺 `)` 但当前为 `=` | expected closing delimiter 取 `=` 起点空 Span；`right_paren_span = None`，保留 `=` 并继续 initializer |
| 缺 `)` 且遇调用方 element boundary / hard stop / EOF | expected closing delimiter 取边界空 Span并保留非 EOF boundary；随后以同一根因构造缺失 initializer，不追加 separator 级联 |
| `)` 后缺 `=`，当前 token 可开始 expression（SPEC-0010 后包括 `{`） | expected destructuring initializer separator 取当前起点空 Span；`equals_span = None`，不消费并继续解析唯一 initializer |
| `)` 后缺 `=`，当前为 element boundary / hard stop / EOF | expected destructuring initializer separator 取边界空 Span；不消费边界，建立同位置空 Error initializer，并抑制 expected destructuring initializer |
| `)` 后缺 `=` 且为其他非法 token | expected destructuring initializer separator 覆盖首 token；owner-aware 消费错误区到下一 element boundary / hard stop，建立覆盖实际消费区的 Error initializer，不再追加 initializer 诊断 |
| 已消费真实 `=` 后直接遇 element boundary / hard stop / EOF | expected destructuring initializer 取边界空 Span，不消费边界，建立同位置空 Error initializer |
| 已消费真实 `=` 后为普通 expression-start | 用现行 expression parser 解析唯一 initializer；完整 element 后的结构 stop 与调用方 `}` 均保留 |
| 已消费真实 `=` 后为其他普通非法 token | expected destructuring initializer 覆盖首 token；owner-aware 消费到下一 element boundary / hard stop，建立覆盖实际消费区的 Error initializer；Lexer poison 已有根因时只建 Error initializer |

pattern 级逗号只由 pattern list 消费，真实 `)` 只由 pattern owner 消费；initializer 恢复继续
使用 block / lambda 的结构 stop 与调用方 hard stop。nested delimiter 和 lexical owner 内同形
token 不作同步点，Lexer poison / terminal 根因抑制同 Span 与同 closer 级联。每轮单调前进，
单个 pattern 与 initializer 合计 `O(n)`。

### 分阶段验收与路线图重排

每个 Spec 都必须包含 pass / fail / recovery、精确 UTF-8 Span、多 SourceMap identity、AST ID
与诊断顺序确定性、terminal Lexer owner、深嵌套与长正确 / 错误序列复杂度证据；复跑所有
已完成且适用于本次改动的历史 Spec 回归、fixture harness 与 workspace 基线，并同步
Architecture。独立分支的草案编号顺序不构成未完成前一 Spec 的实现依赖。
验收不以名称、类型、捕获或所有权正确性为条件。

| Spec | 最小 pass | 最小 fail / recovery |
|---|---|---|
| 0010 | 空 / 显式零参数 / 多参数、`move`、initializer / 普通 call argument / grouped statement、嵌套 lambda、body 首项或真实 nested-block closer 后的尾 expression | 严格 header 前缀 lookalike 按零参数 body 恢复、顶层 `,` / `->`、局部声明后普通 tail 被拒绝、缺 `}`、lambda 与 nested block 同 token 串的上下文对照；空 body 不是 fail |
| 0011 | 无体 / 空或非空 block body 的隐式 `Unit`，以及三种形态的显式返回标注 | 表达式体省略标注继续发 expected explicit return type；缺 `:` 与缺 TypeRef 恢复仍区分，AST 不伪造 `Unit` TypeRef / `:` Span |
| 0012 | 声明侧无标记 / `borrow` / `inout` 的具名函数与函数类型参数，以及调用点位置、命名、模式（`borrow` 或 `&`）、命名加模式四种 call argument；覆盖 basic / typed / member / chained call、nested lambda / delimiter operand | 重复声明 / 函数类型参数模式、缺命名值、缺模式 operand、逆序或重复调用模式、空项 / trailing comma / 缺 `)`，并保留 outer owner closer；strict typed-call trial 同步识别带模式函数类型 |
| 0013 | 单 / 多 binding、复杂 RHS、与前后 element 相邻、block 与 lambda body 内嵌套 | 空 binding、缺 separator / `)` / `=` / initializer，`var` / `const` / `_` / nested / typed pattern 均按稳定类别拒绝 |

SPEC-0009 中 `f({})`、`val x = {}` 等“block 不可作 expression”的历史负例在 SPEC-0010 后
迁移为 expression-context lambda 正例；直接 block dispatch 的 `{}` 仍是 nested block。
SPEC-0007 的 trailing lambda 负例继续成立，不能用本次迁移批量接受其他 golden 变化。

0001–0009 的历史实体文件和编号保持不变；0010–0012 已完成。0013 及后续候选尚未物化，
本版按下表使用唯一编号，禁止保留新旧编号别名：

| 新编号 | Goal / 旧候选映射 |
|---|---|
| 0010 | lambda literal |
| 0011 | 具名函数省略返回标注时固定为 `Unit` |
| 0012 | 统一 callable 参数 marker、函数类型参数与 typed call argument |
| 0013 | 局部 `val` 解构 |
| 0014 | 完整文件、声明分隔与跨声明恢复（旧 0011） |
| 0015 | `module` / `import`（旧 0012） |
| 0016 | control-flow Parser（原待编号） |
| 0017 | class-family Parser（原待编号） |
| 0018–0026 | Phase 2 旧 0013–0021，逐项 `+5` |
| 0027–0032 | Phase 3 旧 0022–0027，逐项 `+5` |
| 0033–0041 | Phase 4 旧 0028–0036，逐项 `+5` |
| 0042–0051 | Phase 5 旧 0037–0046，逐项 `+5` |
| 0052–0061 | Phase 6 旧 0047–0056，逐项 `+5` |
