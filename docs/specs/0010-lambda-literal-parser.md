# SPEC-0010: 解析 lambda literal

| 字段 | 值 |
|---|---|
| 状态 | draft |
| Goal ID | `KOV-P1-010` |
| 所属 Phase | Phase 1 |
| 语言规范 | 候选 [`agent-language-design-guide-v0.9.md`](../agent-language-design-guide-v0.9.md) 第四部分第 9 节；尚未生效 |
| 批准依据 | 用户在当前持续 Goal 中授予的后续 Spec 站立授权；须在 v0.9 明确启用、阻塞解除后方可据此批准 |
| 前置 Spec | SPEC-0009 `done` |
| 前置 ADR | 无 |
| 关联 ADR | [ADR-0003](../adr/0003-diagnostic-architecture.md)、[ADR-0004](../adr/0004-source-span-position-model.md) |
| 阻塞项 | v0.9 尚未由用户明确指定取代现行 v0.8；解除前不得进入 `approved` / `in-progress` |
| 影响范围 | `lang-frontend`、语言 fixture、Architecture |
| 语言语义变更 | 否；启用后只实施 v0.9 已定义的 lambda 增量 |

## 1. Goal

完成后，`lang-frontend` 能在既有表达式、声明与 block 上下文中确定性解析普通及 `move`
lambda literal，以独立于 `Unit` block 的 typed AST 保存真实参数 `Span`、有序 body element 与
可判定尾值，并以结构化诊断和显式 error node 恢复非法 body，不因用户输入 `panic!`。

## 2. 背景

SPEC-0007 至 SPEC-0009 已提供 Pratt expression、声明、block / statement table、owner-aware
恢复及固定递归预算，但 `{` 在 expression primary 中仍被拒绝，在 block 中则充当 nested block
起点或 soft element stop。现行 v0.8 未定义 lambda，不能依据 Kotlin 经验补齐实现。

候选 v0.9 第四部分第 9 节封闭了 lambda 的上下文判定、严格 header 前缀、body 尾值、AST /
`Span`、恢复和复杂度契约。本 Spec 只把该增量转成可执行 Goal；typed call argument 与局部
解构分别留给 SPEC-0011、SPEC-0012。v0.9 未启用前，本草案不构成实现授权。

## 3. 范围与需求

### 3.1 Expression primary 与 `{` 上下文判定

- 把普通 `{ ... }` 和 `move { ... }` lambda 接入既有 expression primary。lambda 可继续接受
  call、member、index、`!!` 与 callable-reference 等既有 postfix；不新增公共 lambda 入口。
- expression 正在等待 primary 时，`{` 提交 lambda；`move` 只有后接 `{` 时提交 lambda，不能
  成为通用 prefix。initializer、prefix / binary 右操作数、group 和 call argument 同样处理。
- block dispatch 在 element 起点直接看到 `{` 时仍提交 `Statement::Block`；已有完整左
  expression 且没有运算符等待右操作数时，顶层 `{` 是下一 nested-block element 的 soft stop。
  判定只依赖 parser 状态，不依赖 trivia、名称或推测类型。
- `x { y }` 是 expression statement 后接 nested block，`x + { y }` 是一个以 lambda 为右
  operand 的 binary expression；`({ y })` 可在 statement 位置强制表达 lambda。trivia 不能
  改变归属。
- 不接受 trailing lambda。`f { ... }` 不形成 Call；独立入口复用尾随 token 恢复，block 中按
  `f` expression statement 后接 nested block 处理。调用 lambda 必须写 `f({ ... })`。

### 3.2 严格 header、body element 与尾值

- lambda 允许无 header 的零参数 body、显式零参数 `->` header，以及逗号分隔的普通
  Identifier 参数。参数不接受类型、默认值、`val` / `var`、模式、解构或 trailing comma。
- header 只能从 `{` 后第一个非 trivia token 起严格完整匹配
  `[ Identifier { "," Identifier } ] "->"`。只有完整前缀成功才提交；首个不匹配 token 立即
  以零状态失败，此 owner 永久按零参数 body 解析，不能在后方继续搜索顶层 `->`。
- header 试探跳过 trivia，跟踪 nested delimiter、string 与 interpolation owner，不分配 AST、
  不发诊断、不改变 cursor。失败不进入“参数恢复”；`{ x y -> z }` 与 typed / default / pattern
  等 lookalike 都按零参数 body 恢复。
- `{}` 与 `{ -> }` 均为合法空 body。成功 header 的参数均是真实 Identifier，不存在 missing /
  error 参数状态；`parameters` 只按源码顺序保存真实 `Span`。
- body 复用局部 `val` / `var`、expression statement 与 nested block 三种 element 及最大合法
  element 规则，但使用独立 lambda-body payload，不能降为静态类型固定为 `Unit` 的 Block。
- 最后一个 element 是 expression statement 时，其 expression 为尾值；空 body或最后一项为
  局部声明 / nested block 时尾值为 `Unit`，且不重复保存 tail ExpressionId。
- 不创造隐式 statement separator：普通 expression-start 不是局部声明 initializer 的 stop，
  因此 `{ val x = 1 x }` 必须作为 initializer 尾随输入拒绝，不能把 `x` 改判为 tail。当前阶段
  普通 tail 只能是 body 首项，或位于一个以真实 `}` 结束的 nested block 之后。
- nested lambda、group、call、index、nested block、string 与 interpolation 各自消费自身
  delimiter；Phase 1 不检查参数类型、捕获、返回类型或 `move` 所有权合法性。

### 3.3 AST 与 `Span` 契约

- 保持 `SyntaxAst = AstFile<Item, Statement, Expression, TypeRef>` 四张 typed table，不增加新
  AST table。最小 payload 至少等价于：

```rust
pub enum Expression {
    // 既有 variants 保持不变。
    Lambda {
        move_span: Option<Span>,
        parameters: Vec<Span>,
        arrow_span: Option<Span>,
        body: StatementId,
    },
}

pub enum Statement {
    // 既有 Unit Block 等 variants 保持不变。
    LambdaBody { elements: Vec<StatementId> },
}
```

- `LambdaBody` 与 `Block` 是语义封闭的不同 variant。有序 statement body 的最后一个
  `Statement::Expression` 提供尾值；不存在第二份 tail ID、孤儿 statement 或 Block 模式位。
- `move_span` 精确覆盖真实 `move`；参数 `Span` 精确覆盖真实 Identifier；`arrow_span` 精确
  覆盖真实 `->`。成功 header 的范围由首参数（零参数时为 `->`）至 `arrow_span` 结束确定，
  不伪造 token、名称 marker 或源码字符串。
- 普通 Lambda Expression 从 `{` 起，`move` lambda 从真实 `move` 起，均至匹配 `}` 结束；
  LambdaBody 从真实 `{` 至匹配 `}`。缺 `}` 时止于本 owner 最后实际消费位置；空 body 仍覆盖
  真实 `{}`。body child 沿用既有 element `Span`。
- 全部节点属于同一 `SourceId`，使用 UTF-8 字节半开区间；跨 SourceMap 输入仍返回具体内部
  错误。准确 Rust 命名可在不改变 typed 关系、不可达状态和可观察结果时微调。

### 3.4 诊断、owner 恢复与复杂度

在集中生产目录从下一空位只注册以下两类诊断；severity 均为 error：

| 错误码 | 含义 | 固定主消息 | 最小恢复 |
|---|---|---|---|
| `L0031` | expected lambda body element | `expected lambda body element` | `}` / 调用方 hard stop 前存在、不能开始任何合法 element 且无更具体根因的普通 token，覆盖并至少消费一个真实 token，形成 Error statement |
| `L0032` | unsupported lambda body form | `unsupported lambda body form` | 严格 header 试探失败后，lambda 顶层遗留 `,` / `->` 等 header-like token 时覆盖实际错误区域，形成 Error statement 并恢复到下一合法 element 或当前 owner 边界 |

- 不为 header 参数、separator 或参数形态分配诊断码，不复用声明列表的 `L0024`–`L0026`，也
  不进行 header 参数恢复。缺 `}` 复用通用 expected closing delimiter；`move` 未后接 `{` 时
  沿用既有 expected expression / trailing 规则。
- body、局部声明与 expression 恢复保留当前 lambda 的 `}`。同形 `}` 关闭最内层 brace owner；
  inherited `)`、`]`、`}`、`InterpolationEnd`、StringEnd 与 EOF 等异形 hard closer 立即停止
  当前恢复并保留给调用方，不等待局部 owner 闭合。
- soft comma、下一 argument / element 候选等只在局部 delimiter 与 lexical owner 回到进入
  恢复时的 baseline 后生效。Lexer poison 与 `L0004`–`L0006` 已有根因时只构造 error node，
  不在同一 `Span` 追加同义 lambda / closer 诊断。
- parser 构造时在完整 raw lexeme / terminal-event 流上建立一次 `O(n)` header 索引：共享
  delimiter / lexical-owner 栈，为每个 `{` owner 运行从紧随其后的首个非 trivia token 开始的
  小型 DFA；首个不匹配即永久记录 no-header，成功则记录参数 token 与 `->` raw index。
- 正式 parser 以 opener raw index 做 `O(1)` 查询。每个 raw lexeme 在全部 header trial、正式
  解析与恢复中各至多访问固定常数次；整体 `O(n)` 时间、`O(d)` owner 空间。缺 `}` 时正式
  parser 仍在最早调用方 hard stop 停止，不受预索引词法范围影响。
- 每轮要么消费 lexeme，要么在自身 `}`、调用方 hard stop 或 EOF 结束。lambda / body 递归
  计入现有 1024 预算并运行于固定 32 MiB scoped worker；超预算受控返回既有内部错误。

### 3.5 Fixture 与回归接入

- 在现有 Cargo fixture target 中新增 `phase1/parser-lambda-pass/` 与
  `phase1/parser-lambda-fail/`；各至少枚举一个真实小写 `.ko`，fail `.diag` 精确核对 Lexer /
  Parser 合并诊断全序，零 suite 必须失败。
- pass fixture 调用生产 Lexer 与 `parse_expression`，验证零诊断、Lambda、LambdaBody、typed
  child ID 与完整 EOF 消费；runner 不成为公共机器诊断协议。
- 新增专用 integration test，覆盖独立 expression、initializer、普通 call argument、grouped
  statement 与 block brace 对照。人工审阅 `f({})`、`val x = {}` 的历史负例定向迁移，不批量
  接受无关 golden 变化。
- 既有 source、Lexer、expression / TypeRef、declaration / typed-call、block 与全部 fixture
  suite 继续执行；SPEC-0011 typed argument 与 SPEC-0012 解构不得出现在本次 pass AST。

## 4. 非目标

- 不实现命名实参、`own` / `inout` / `borrow` 模式实参或 typed call argument table；这些属于
  SPEC-0011。
- 不实现局部或顶层解构、pattern table、`componentN()` 类型 / 所有权检查；局部 `val` 解构
  属于 SPEC-0012。
- 不实现 trailing lambda、隐式 `it`、typed / 默认 / 解构 lambda 参数、参数 trailing comma、
  `return` 或其他控制流。
- 不实现 `if` / `when` / `super`、loop / class family、完整文件、声明分隔、跨声明恢复、
  module / import 或类成员上下文。
- 不做名称解析、参数 / 捕获 / 返回类型检查、闭包布局、捕获分析、`move` 合法性、所有权 /
  借用检查或 codegen。
- 不改变普通 `Statement::Block` 的 Unit / 无尾值语义，不新增 parser generator、CST / green
  tree、增量 parser、visitor、新 crate、第三方依赖或公共机器诊断协议。

## 5. 验收标准

- [ ] `parse_expression`、`parse_declaration` 与 `parse_block` 在各自上下文接入 lambda，保持
      SourceMap identity、typed root 与两阶段诊断全序；不新增公共 lambda 入口或 AST table。
- [ ] compile-pass 覆盖 `{}`、`{ -> }`、`{ x }`、`{ -> x }`、`{ x, y -> x + y }`、
      `move { x }`、nested lambda、body 首项尾 expression、nested block 后 tail、末项为局部
      声明 / nested block 的 Unit body，以及 lambda 的全部既有 postfix。
- [ ] compile-pass 覆盖 initializer、普通 call argument 与 grouped statement；block 对照证明
      element 起点 `{ x }` 是 nested Unit block、`({ x })` 是 lambda expression、`x { y }` 是
      两项、`x + { y }` 是一个 expression，且 trivia 变体 AST 相同。
- [ ] `f({})` 与 `val x = {}` 定向迁移为 expression-context lambda 正例；`f {}` 仍不形成
      Call，独立入口与 block 上下文保持各自尾随 / element 结构。
- [ ] header 白盒矩阵证明只有 `{` 后完整严格前缀提交；typed、default、`val` / `var`、模式、
      解构、leading / repeated / trailing comma、缺 separator、nested delimiter / string /
      interpolation 箭头及 `{ x y -> z }` 全部以零状态失败并按零参数 body 恢复，不生成参数
      marker、header 诊断或 cursor / AST / 诊断副作用。
- [ ] body compile-fail 覆盖不能开始 element 的普通 token、失败 header 遗留的顶层 `,` / `->`、
      expression 尾随区域、`{ val x = 1 x }` 与缺 `}`；分别锁定 `L0031`、`L0032`、既有表达式
      诊断 / `L0010` 的边界、Error statement 及恢复后 element 顺序。
- [ ] diagnostic catalog 证明只新增 `L0031 expected lambda body element` 与
      `L0032 unsupported lambda body form`，固定 error severity / 消息 / 精确主 `Span`；不复用
      `L0026`，既有错误码含义与排序不变；`{}` / `{ -> }` 不产生 body 诊断。
- [ ] AST 测试证明 Lambda Expression、独立 LambdaBody Statement 与普通 Unit Block 的 typed
      关系；参数为有序真实 `Vec<Span>`，move / arrow / body / tail / error / 缺 closer 的 UTF-8
      范围符合契约，无 marker、伪造 token、重复 tail 或孤儿节点。
- [ ] owner recovery 覆盖 lambda 嵌套于 call、group、index、block 与 string interpolation：
      异形调用方 hard closer 即使局部 owner 未闭合也被保留，同形 `}` 关闭最内层 brace owner，
      soft stop 只在 baseline 生效。
- [ ] `L0004` 未终止内层 string、`L0005` EOF interpolation 与 terminal `L0006` 只关闭精确
      lexical owner，不提前结束 lambda、不吞 parent token、不产生同义 parser closer 级联。
- [ ] multi-SourceMap identity、外部小线程栈与 1024 递归预算回归通过；深 nested lambda /
      block / group / interpolation 超预算受控返回具体内部错误，不 panic、不泄漏预算。
- [ ] `cfg(test)` inspection 或等价白盒证据覆盖长成功 / 首 token 失败 header、长 body、深
      nested lambda 及 N→2N 同族输入；header 索引一次遍历全流、正式查询 `O(1)`，总访问计数
      受固定常数乘 lexeme 数约束，无逐 lambda 或 Lexer diagnostics 重扫。
- [ ] parser-lambda pass / fail suite 各真实执行至少一个 `.ko`；零 suite、非法 sidecar、orphan
      pair 与空范围 policy 自检继续失败，现有 fixture suite 无回归。
- [ ] SPEC-0007 expression / TypeRef、SPEC-0008 declaration / typed-call 与 SPEC-0009 block /
      function-body 窄测试通过；只定向迁移 v0.9 明确取代的历史负例。
- [ ] `cargo tree -p lang-frontend --edges all --locked --offline` 与 manifest / lock diff 证明未新增
      normal、dev 或 build 依赖。
- [ ] frontend 窄测试及 workspace fmt、check、Clippy、test、CLI build 基线通过；记录实际
      passed / failed / ignored / measured / filtered 数量，不把未执行检查写成通过。
- [ ] Architecture 更新为已实现的 Lambda / LambdaBody、brace 判定、全流 header 索引、两类
      诊断、fixture 与 owner 恢复，并明确 typed argument、解构、完整文件尚未实现。

## 6. 技术方案与边界

保持现有 Parser、Pratt、typed AST 与产物 API。`Expression::Lambda` 引用独立
`Statement::LambdaBody`；body child 继续复用 typed ID。尾值由最后一个 expression statement
确定，普通 Block 不增加模式位。initializer / operand / postfix 复用同一 Pratt cursor。

把 expression stop 明确分为调用方 hard closer 与“左表达式已完成后”的 block soft stop。
`parse_primary` 在等待 operand 时允许 `{`；block dispatch 仍先识别 nested block。

parser 构造时建立一次全流 `LambdaHeaderIndex`（准确 Rust 名称可调整），以共享 owner stack
为每个 `{` 运行严格前缀 DFA；失败永久缓存 no-header，成功记录真实参数 token 和 arrow raw
index。正式解析只按 opener raw index 查询，不能从候选 `{` 独立扫描。实现继续复用
`LexicalRecoveryIndex`、owner-aware scanner、固定 worker 与递归预算。

`cfg(test)` inspection 统计 header-index raw visit / query、body dispatch 与 recovery visit，不
进入 release API。本 Spec 沿用 ADR-0003 / ADR-0004，不需要新 ADR，也不引入依赖或改变
workspace 边界。

## 7. 实施计划

1. [ ] 注册且仅注册 `L0031`–`L0032`，扩展 Lambda / LambdaBody typed AST 与结构 getter
   → 验证：catalog、真实参数 `Vec<Span>`、typed ID、SourceMap / Span 与 Unit Block 回归
2. [ ] 拆分 hard / soft expression stop，建立一次全流严格 header DFA 索引并接入 ordinary /
   `move` lambda primary
   → 验证：上下文矩阵、成功 / 零状态失败无副作用、O(1) query 与 N→2N inspection
3. [ ] 实现 body element、尾值、两类 body 诊断与 owner-aware 恢复
   → 验证：空 / 非空 body、局部声明后普通 tail 拒绝、nested owner、terminal Lexer error、
   递归预算与无级联测试
4. [ ] 接入 parser-lambda pass / fail fixture，定向迁移历史负例并复跑 expression / declaration /
   block fixture
   → 验证：真实 `.ko`、sidecar 全序、零用例保护与人工审阅差异
5. [ ] 同步 Spec 验收记录与 Architecture
   → 验证：frontend 窄测试、workspace 全基线、依赖树、staged diff 与文档事实一致

开始步骤 1 前必须由用户明确启用 v0.9，并把本 Spec 依据站立授权转为 `in-progress`。Parser
cursor、AST、stop 与 header 索引热点由单一负责人整合；测试和红队审计可并行但不并发修改
同一生产文件。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Lambda / LambdaBody AST、brace 判定、全流 header 索引、两类诊断、恢复、测试 / fixture、Architecture 与完成记录 | `feat(frontend): add lambda literal parser (SPEC-0010)` |

实现提交只属于 SPEC-0010，不混入 typed call argument、解构、完整文件、新 guide 激活或其他
语法。v0.9 启用必须在实施前形成独立文档边界；实现、测试、Architecture、验收与 `done` 状态
全部满足后才能创建本表提交。

## 9. 未决问题

- 唯一门禁是 v0.9 尚未由用户明确启用；候选第 9 节本身未发现需要猜测的 lambda 语义。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| v0.9 版本级启用确认 | 未满足 | 当前权威 guide 仍是 v0.8；因此本 Spec 保持 `draft`，未实施 |
| `git diff --no-index --check -- /dev/null docs/specs/0010-lambda-literal-parser.md` | 通过 | 无 whitespace error 输出；exit 1 仅表示未跟踪草案与空文件存在内容差异 |
| Markdown 相对链接与路线图一致性检查 | 通过 | guide、ADR 链接存在；README 的 0010 状态、前置与 v0.9 门禁一致 |
| Cargo / Rust 验收 | 未执行 | 本次只同步被 guide 门禁阻塞的 Spec，不修改实现 |
