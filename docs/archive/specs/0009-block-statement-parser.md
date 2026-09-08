# SPEC-0009: 建立 block 与 statement 序列 Parser

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-009` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [`agent-language-design-guide-v0.8.md`](../guides/legacy/agent-language-design-guide-v0.8.md) 第四部分第 8 节 |
| 批准依据 | 用户在当前持续 Goal 中授予的后续 Spec 站立授权；v0.8 已由用户明确启用 |
| 前置 Spec | SPEC-0008 `done` |
| 前置 ADR | 无 |
| 关联 ADR | [ADR-0003](../../adr/accepted/0003-diagnostic-architecture.md)、[ADR-0004](../../adr/accepted/0004-source-span-position-model.md) |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend`、语言 fixture、Architecture |
| 语言语义变更 | 否；本 Spec 实现现行 v0.8 已写明的增量 |

## 1. Goal

完成后，`lang-frontend` 能确定性解析一个独立 block，以及具名函数的 block body；block
以有序 typed `StatementId` 保存局部 `val` / `var`、expression statement 和 nested block，
函数 body 以无体、表达式体、block body 三种互斥状态表示。普通用户语法错误进入带精确
`Span` 的结构化诊断和显式 error node，非法源码不触发 `panic!`。

## 2. 背景

SPEC-0008 已提供独立声明入口、简单变量与函数 Item、表达式 / TypeRef parser、owner-aware
恢复索引和固定资源边界，但生产 `SyntaxAst` 的 statement table 仍使用 `()` 占位，函数只
能保存无体或表达式体，block body 会被独立声明入口以尾随 token 拒绝。

现行 v0.8 第四部分第 8 节为 block、statement 序列、函数 block body、结构边界与局部恢复
补齐了可执行契约。本 Spec 只承接这个单一增量，并依据当前持续 Goal 的站立授权进入实施。

## 3. 范围与需求

### 3.1 独立 block 入口与产物

- 新增一次只解析一个 block 的公共入口。入口接收同一 `SourceMap` 的 `LexedFile`，校验
  source identity，跳过 trivia，要求以真实 `{` 开始，并在匹配 `}` 后只允许 trivia 和 EOF。
- 产物拥有共享具体索引式 AST、唯一 block `StatementId` root，以及 Lexer / Parser 诊断的
  确定性合并全序。普通用户语法错误仍返回产物；source、AST、诊断目录、lexeme 流与资源
  边界失败返回现有 `ParserInternalError`。
- 独立 block、表达式和声明入口复用同一 cursor、Pratt、TypeRef、词法恢复索引、诊断构造、
  32 MiB scoped worker 与 1024 递归预算，不复制第二套 Lexer、表达式或变量声明 parser。
- 已成功解析的独立 block 后仍有 token 复用 `L0013`。入口缺真实 `{` 时，首个普通 token
  使用本 Spec 的 expected block，并把从该 token 到 EOF 的全部剩余 lexeme（包括 trivia）
  消费为唯一 Error root；`L0028` 主范围只覆盖首个普通 token，Error root 从该 token 起至
  最后实际消费的非 trivia / invalid lexeme 终，不追加同根因 `L0013`。EOF 时诊断与 Error
  root 都为空范围；首 token 已有 Lexer invalid / reserved 根因时不追加 `L0028`，但仍按同样
  边界消费全部余量并构造唯一 Error root。所有分支都不伪造 opener。

### 3.2 Block element 与结构边界

- block element 精确封闭为局部 `val` / `var` statement、既有 expression statement 与
  nested block statement；`{}` 合法，`{{}}` 表示一个 nested block element。
- 每层 `{` 都建立独立 owner，只有该层正常闭合路径消费对应 `}`。当前 owner 的 `}` 与 EOF
  是 hard stop；子 block 消费自己的 closer 后才把控制权交回父 block。
- 同形 `}` 总是关闭当前最内层 block，不能猜测它原本属于父 owner；`{{}` 中唯一的 `}`
  关闭内层，外层到 EOF 复用 `L0010` 报缺 closer。子 block 普通恢复不得越过其下一
  block-level `}`；没有真实 closer 的剩余 owner 最终按由内到外的确定顺序结束。
- Koven v1 没有源码分号，换行、注释与其他 trivia 不分隔 element。增删 trivia 不得改变
  element 数量、表达式结合或 AST 归属。
- 一个 element 完成后的显式结构 stop 精确为当前 owner `}`，以及不在 expression owner /
  delimiter 内的 `{`、`val`、`var` 和本 Spec 的 unsupported element 引导关键字。它们须保留
  给 block dispatch，用于 nested block、下一局部声明或 unsupported error element。
- 普通 Identifier、字面量、`this`、`null`、布尔字面量、`::` 和 prefix opener 即使能开始
  表达式，也不是前一 expression 的结构 stop。`{ x y }` 不得静默拆成两个 expression
  statement；既有 expression trailing-token 恢复应消费余下非法区域并产生 `L0013`。
- 局部声明 initializer 与 expression statement 调用既有 Pratt parser，并显式传入当前
  owner `}` 及上述 element 边界。必需 initializer 起点直接出现 element 边界时复用 `L0009`
  建立空 Expression error，保留该边界给 block dispatch；普通 expression-start 不能仅因也
  可能开始下一 statement 而使 initializer 提前结束。
- block 的语法结果为顺序执行的 `Unit` 容器，不产生值，也没有尾表达式特例。包括最后一项
  在内的 expression statement 结果都按 `Unit` 语境丢弃；Parser 只保存结构，不做 Phase 2
  类型或返回路径判定。

### 3.3 Statement AST 与函数 body

- 把共享具体 AST 从 `AstFile<Item, (), Expression, TypeRef>` 扩展为
  `AstFile<Item, Statement, Expression, TypeRef>`，所有 block element 按源码顺序存为
  `StatementId`，不得混入 expression table 或复制源码字符串。
- `Statement` 至少区分：
  - `Block { elements: Vec<StatementId> }`；
  - `LocalVariable { declaration: ItemId }`，只引用现有变量 Item，不接受常量或函数 Item；
  - `Expression { expression: ExpressionId }`；
  - `Error`，只覆盖本次实际消费的错误区域。
- 函数 Item 的 body 使用 `Absent | Expression { equals_span: Span,
  expression: ExpressionId } | Block(StatementId)` 或可证明等价的封闭枚举，替换可能形成
  “双 body”的独立 `Option` 组合，同时保留 SPEC-0008 已要求的真实 `=` Span。无体、
  `= expression` 与 block body 三形态互斥，均继续要求显式返回 TypeRef。
- 函数返回类型后只有实际 `{` 才提交 block body。没有 `{` 时仍按无体函数结束，后续非法
  token 由独立声明入口按 `L0013` 处理；不得因为“期待函数体”追加 expected block。
- 已提交 `= expression` 后的 `{` 不得改判为 block body；已提交 block body 后的
  `= expression` 也不得形成第二个 body。无体函数的上下文合法性、block 返回路径和声明
  返回类型一致性留给后续容器与 Phase 2。

### 3.4 `Span` 契约

- 完整 block 从真实 `{` 起到匹配 `}` 终；缺 `}` 时止于本 block 最后实际消费位置。空 block
  仍完整覆盖真实 `{}`，nested block 不得吞入父 owner 的 `}`。
- local-variable statement 与所引用变量 Item 范围完全相同；expression statement 与所引用
  Expression 范围完全相同。
- error statement 只覆盖实际消费区域。owner closer / EOF 前允许产生空诊断范围，但不得把
  零宽 Error statement 插入 element 序列并使循环停滞。
- 有 block body 的函数 Item 从真实 `fun` 起到 body 的真实 `}` 终；缺 closer 时止于 body
  最后实际消费位置。所有节点继续使用同一 `SourceId` 的 UTF-8 字节半开区间，不为缺失
  token 合成虚构非空范围。

### 3.5 诊断与局部恢复

在集中生产目录顺序注册以下一类一码；severity 均为 error，消息固定如下：

| 错误码 | 含义 | 固定主消息 | 最小恢复 |
|---|---|---|---|
| `L0028` | expected block | `expected block` | 仅独立 block 入口缺 `{`；主 `Span` 只覆盖当前普通 token，恢复消费至 EOF 的全部余量（含 trivia），唯一 Error root 止于最后非 trivia / invalid lexeme 终，并抑制同根因 `L0013`；EOF 时二者为空；首 token 有 Lexer poison 根因时同样消费并构造 Error root但不追加本码；不伪造 opener |
| `L0029` | expected block element | `expected block element` | 主 `Span` 覆盖并只消费当前非法 token，形成同范围 Error statement，再继续 dispatch |
| `L0030` | unsupported block element | `unsupported block element` | 主 `Span` 覆盖并只消费已明确延后的单个引导 token；`const val` 覆盖并消费固定前缀，后续遗留 token 重新 dispatch |

- 复用 `L0009` expected expression、`L0010` expected closing delimiter、`L0013` trailing token、
  `L0014` expected TypeRef，以及简单变量声明适用的 `L0018` expected declaration name 与
  `L0020` expected initializer 和函数的 `L0021` expected explicit return type；不为同一根因
  追加 block 级同义诊断。函数参数表后缺 `:` / return TypeRef 而直接遇 `{` 时，`L0021`
  构造边界处空 TypeRef error 并保留 `{`，随后仍提交 block body；已有 `:` 但直接遇 `{` 时
  复用 `L0014` 构造空 TypeRef error，同样保留并继续解析 block body。两种恢复都不得把 `{`
  降为历史 SPEC-0008 的 trailing token，也不得追加 `L0013` / `L0028`。
- `const val`、局部 `fun`、`return` / `break` / `continue`、`if` / `when` / `super` / `for` /
  `while` / `loop`，以及 `value class` / `class` / `interface` / `enum class` / `object` /
  `companion object` 的引导关键字产生 `L0030`，不能作为 opaque statement、Identifier
  expression 或已支持声明成功。
- Lexer invalid / reserved token 已有根因时，只消费并形成 Error statement，不在相同 `Span`
  追加 `L0029` / `L0030`。字符串和插值中的大括号及 element-like token 归 lexical owner，
  `InterpolationEnd` 不能充当 block closer。
- block 恢复复用既有 `LexicalRecoveryIndex` 的 `L0004`–`L0006` terminal-owner 关联，不从
  每个 element 重扫 Lexer 诊断。Lexer 已完整表达未终止 owner 根因时抑制同义 closer 诊断。
- unsupported / expected element 最小恢复只消费确定错误引导或错误 token；没有分号或换行
  可以作为未来结构同步点，不得按行跳过、跨当前 owner `}`，也不得为同一未消费 token 重复
  发诊断。
- dispatch 每轮要么消费至少一个 raw lexeme，要么在当前 `}` / EOF 结束。每个 lexeme 在
  block dispatch 中至多单调前进一次，nested parser 只处理自己拥有的范围；一份 `n` lexeme
  block 整体为 `O(n)` 时间、`O(d)` owner / delimiter 空间。

### 3.6 Fixture 接入

- 在现有 Cargo fixture target 中新增 `phase1/parser-block-pass/` 与
  `phase1/parser-block-fail/`；两套 suite 均至少枚举一个真实小写 `.ko`，零 suite 必须失败。
- block pass case 调用生产 Lexer 与独立 block 入口，验证零诊断、Statement root 有效、所有
  typed child ID 可读、element 顺序及完整消费到 EOF。
- block fail case 复用私有 `.diag` sidecar，精确核对 Lexer / Parser 合并诊断全序；Parser
  诊断允许 stop / EOF 空范围，合并流中的 `L0001`–`L0008` Lexer 码继续要求非空范围。
- 扩展 parser-declaration 结构测试与既有 declaration fixture，锁定函数 block body 的 AST、
  Span 和恢复；不得把仅调用独立 block 入口当作函数 body 已接线的证据。SPEC-0008 按 v0.7
  正确锁定的 block-body 拒绝用例在 v0.8 生效后必须改为本 Spec 的正例 / 结构测试并人工审阅
  预期差异，不能要求这条已被新语义取代的旧负例原样继续通过。
- 现有 source、Lexer、parser-expression 和 parser-declaration suites 必须继续真实执行；
  fixture harness 仍不得把测试 sidecar 暴露为公共机器诊断协议。

## 4. 非目标

- 不实现 `if` / `when` / `super`、`for` / `while` / `loop`、`return` / `break` /
  `continue`；控制流等待新 guide 与独立、待编号 Spec。
- 不实现 `value class` / `class` / `interface` / `enum class` / `object` /
  `companion object`、类成员上下文或成员恢复；class-family 等待新 guide 与独立、待编号 Spec。
- 不实现 lambda、trailing lambda、命名 / `own` / `inout` / `borrow` 模式实参或解构；这些
  结构仍属于 SPEC-0010。
- 不组合完整文件，不定义顶层 / 类成员分隔或跨声明恢复；这些职责仍属于 SPEC-0011，但
  SPEC-0011 不是全部 Phase 1 语法的最终聚合验收。
- 不实现 `const val` 或局部 `fun` statement，不把 block 当 expression、lambda 或函数返回值，
  不接受源文件分号，不用换行 / 注释终止 statement。
- 不做名称解析、作用域诊断、shadowing、use-before-declaration、类型检查、返回路径、所有权
  或借用检查；Parser 只保持源码顺序和结构。
- 不引入 parser generator、CST / green tree、增量 parser、visitor、formatter trivia 附着、
  新 crate、第三方依赖或公共机器诊断协议。

## 5. 验收标准

- [x] 公共 block 入口校验 SourceMap / LexedFile identity，返回唯一 typed Statement root 与
      两阶段诊断全序；用户语法错误进入产物，内部不变量 / 资源失败返回具体内部错误。
- [x] 独立入口缺 `{` 时，普通 token 从当前至 EOF 的全部剩余 lexeme（包括 trailing trivia）
      被消费为唯一 Error root，节点止于最后非 trivia / invalid lexeme 终，并抑制同根因
      `L0013`，而 `L0028` 主 `Span` 只覆盖首 token；EOF 形成空 root / 诊断；首 token 已有
      Lexer poison 根因时只保留 Lexer 诊断和同边界 Error root。三类都不伪造 opener 或范围。
- [x] compile-pass 覆盖空、单 / 多 element、局部 `val` / `var`、expression statement、nested
      空 block 与多层 block，typed child ID、源码顺序和所有合成 `Span` 均被结构测试锁定。
- [x] element 边界覆盖同一行、多行、注释及任意 trivia 变体；`{ val x = 1 val y = 2 }`
      正确形成两项，而 `{ x y }` 不得形成两项并由既有 trailing 恢复消费非法余项。普通
      expression-start 永不因 trivia 成为结构 stop。
- [x] initializer 起点出现 `}` / `{` / `val` / `var` / unsupported 引导时产生唯一 `L0009`
      空 error expression 并保留边界；普通 Identifier / 字面量 initializer 不被提前截断。
- [x] 函数无体、表达式体、block body 三形态 compile-pass 并映射到封闭 body 枚举；结构测试
      证明不能形成双 body，表达式体保留真实 `=` Span，block 函数 Item 和 body Statement
      的完整 `Span` 正确。
- [x] 函数返回类型后没有 `{` 不产生 `L0028`；表达式体后 `{`、block body 后 `=` 及无体后
      其他 token 均由真实 token 归属产生确定诊断，不回溯改判 body 形态。
- [x] `fun f() {}` 产生且只产生一个 `L0021`、一个边界处空 TypeRef error 和
      `FunctionBody::Block`；`fun f(): {}` 产生且只产生一个 `L0014`、一个边界处空 TypeRef
      error 和 `FunctionBody::Block`。两者的 `{}` 均完整解析，不产生 `L0013` / `L0028`，
      函数与 block `Span` 符合合成规则。
- [x] compile-fail 覆盖独立入口缺 `{`、block 缺 `}`、不完整局部名称 / 类型 / `=` /
      initializer、owner `}` 前缺 initializer、源码分号的 Lexer error、未知错误 token 和尾随
      token，并断言固定消息、精确 UTF-8 字节 `Span`、Error statement、保留 closer 与恢复后
      的 element 顺序。
- [x] `const val`、局部 `fun`、`return` / `break` / `continue`、`if` / `when` / `super` /
      `for` / `while` / `loop`，以及 `value class` / `class` / `interface` / `enum class` /
      `object` / `companion object` 均以 `L0030` 拒绝；后续 token 不被保存为 opaque node 或
      伪装成支持项。
- [x] block 用作调用实参、initializer、二元运算任一侧或 lambda 的相邻反例均被准确拒绝；
      nested block statement 仍成功，二者不因大括号相同而混淆。
- [x] `L0028`–`L0030` 在集中目录一类一码；diagnostic test 断言 error、固定消息、精确主
      `Span` 与无重复根因，生产目录连续、唯一且不混入测试码。
- [x] nested owner 测试证明子 closer 只关闭子 block、父 closer 被父 owner 保留，子 block
      普通恢复不扫描越过下一 block-level `}`；`{{}` 的唯一 closer 必须关闭内层，外层到
      EOF 复用 `L0010`，不得猜测 closer 属于父层，且错误范围不虚构。
- [x] string / interpolation 内的大括号、`val` / `var` 和 unsupported-like token 不成为 block
      边界；嵌套未终止 string/interpolation、terminal `L0006` 与 EOF `L0005` 只报告词法根因，
      不提前关闭 block 或产生同义 block / closer 级联。
- [x] `cfg(test)` dispatch / recovery inspection 计数器或等价白盒证据覆盖长合法 element 序列、
      长 unsupported / poison 序列和深 nested block；同族输入从 N 到 2N 时 inspection 受固定
      常数乘 lexeme 数约束，证明 `O(n)` 且没有从每个 element 回扫起点或 Lexer 诊断。
- [x] parser-block pass / fail suite 各自真实执行至少一个 `.ko`；零 suite、非法 sidecar、孤立
      pair 与空范围 policy 自检继续失败；现有所有 fixture suite 无回归。
- [x] 现有 SPEC-0007 expression / TypeRef 与 SPEC-0008 未受影响的 declaration / typed-call
      窄测试全部通过；SPEC-0008 的 block-body 拒绝用例经人工审阅后迁移为 v0.8 正例 / 结构
      测试，不用旧 `L0013` 预期制造规范冲突。递归与 nested block 超预算受控返回内部错误，
      不 panic、不泄漏预算。
- [x] `cargo tree -p lang-frontend --edges all --locked --offline` 与 manifest / lock diff 证明
      未新增 normal、dev 或 build 依赖。
- [x] frontend 窄测试和 workspace fmt、check、Clippy、test、CLI build 基线全部通过；完成
      记录写明实际 passed / ignored / filtered 数量，不把未执行检查写成通过。
- [x] Architecture 更新为已实现的 Statement table、block 入口、函数 body 三态、fixture 与
      恢复边界，并继续明确完整文件、lambda、控制流、class-family 均尚未实现。

## 6. 技术方案与边界

拟议最小公共 API 如下；准确命名可依照实施时的 Rust 风格微调，但不能改变 typed ID、所有权
或可观察结果：

```rust
pub type SyntaxAst = AstFile<Item, Statement, Expression, TypeRef>;

pub fn parse_block(
    sources: &SourceMap,
    lexed: &LexedFile,
) -> Result<ParsedBlock, ParserInternalError>;

pub struct ParsedBlock {
    ast: SyntaxAst,
    root: StatementId,
    diagnostics: Vec<Diagnostic>,
}

pub enum FunctionBody {
    Absent,
    Expression {
        equals_span: Span,
        expression: ExpressionId,
    },
    Block(StatementId),
}

pub enum Statement {
    Error,
    Block { elements: Vec<StatementId> },
    LocalVariable { declaration: ItemId },
    Expression { expression: ExpressionId },
}
```

`ParsedBlock` 只暴露 `source_id()`、`ast()`、`root()` 与 `diagnostics()` getter，与现有两个
产物保持一致。局部变量 parser 是声明 parser 的内部复用入口，必须接收 block stop 集合；
不能直接调用要求 EOF 的公共 `parse_declaration`，也不能复制 Item 构造和恢复逻辑。

block dispatch、Pratt 和声明恢复共享一个 Parser cursor 及 `LexicalRecoveryIndex`。可在现有
`parser::engine` 内先保持一组单一职责函数；只有拆模块不会暴露大量私有状态时才建立私有
`parser::block`。测试 inspection 只属于 `cfg(test)`，不得进入 release API 或语言行为。

本 Spec 不产生新 ADR：typed table、诊断模型、`Span`、共享 Parser 与 worker 资源边界均沿用
已实现架构及 ADR-0003 / ADR-0004。新增第三方依赖不能由本草案暗示授权。

## 7. 实施计划

1. [x] 注册 `L0028`–`L0030`，扩展 Statement、FunctionBody、共享
   SyntaxAst 与独立 block 产物 API
   → 验证：diagnostic catalog、typed ID/source identity、函数 body 三态和 public getter 窄测试
2. [x] 实现独立 / nested block dispatch、局部变量和 expression statement 结构边界
   → 验证：trivia 不敏感、`{ x y }` trailing、initializer boundary、顺序与 Span 集成测试
3. [x] 接入函数 block body，并实现 owner-aware 局部恢复和单调复杂度保护
   → 验证：三体互斥、缺返回类型后仍接线 block、closer 所有权、Lexer terminal owner、
   无级联及 inspection 倍增测试
4. [x] 接入 parser-block pass / fail fixture，迁移 SPEC-0008 已被 v0.8 取代的 block-body
   拒绝预期，并保持其他声明 / 表达式 fixture 回归
   → 验证：真实 fixture、人工审阅的旧负例差异、零用例、sidecar 全序及 Parser / Lexer
   空 Span policy 测试
5. [x] 同步 Spec 验收记录与 Architecture
   → 验证：frontend 窄测试、workspace 全基线、依赖树、staged diff 与文档事实一致

v0.8 已生效；步骤 1 固定公共 payload，再推进后续步骤。
Parser 热点、诊断目录与 fixture runner 的最终整合由单一负责人完成，避免同一 cursor / AST
状态被并行修改。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Statement AST、block 入口、函数 block body、诊断、测试 / fixture、Architecture 与完成记录 | `feat(frontend): add block statement parser (SPEC-0009)` |

该提交只属于 SPEC-0009，不混入控制流、class-family、lambda、完整文件或新 guide 激活提交。
实现、测试、Architecture、验收和 `done` 状态全部满足后才能创建最终实现提交。

## 9. 未决问题

无。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| v0.8 版本级启用确认 | 满足 | 用户已明确启用 v0.8；本 Spec 依据站立授权实施并完成 |
| 独立红队审计 | 通过 | 最终结论 P0 = 0、P1 = 0；owner、terminal recovery、类型 delimiter 与复杂度回归均闭环 |
| `cargo test -p lang-frontend --test parser_block --locked --offline` | 通过 | 20 passed；0 failed / ignored / measured / filtered |
| `cargo test -p lang-frontend --test parser_declaration --locked --offline` | 通过 | 22 passed；0 failed / ignored / measured / filtered |
| `cargo test -p lang-frontend --test parser_expression --locked --offline` | 通过 | 52 passed；0 failed / ignored / measured / filtered |
| `cargo test -p lang-frontend --test fixtures --locked --offline` | 通过 | 20 passed；0 failed / ignored / measured / filtered；九套 suite 均有真实用例 |
| `cargo test -p lang-frontend --lib --locked --offline` | 通过 | 9 passed；0 failed / ignored / measured / filtered |
| `cargo test -p lang-frontend --doc --locked --offline` | 通过 | 7 passed；0 failed / ignored / measured / filtered |
| `cargo fmt --all -- --check` | 通过 | rustfmt 无差异 |
| `cargo check --workspace --all-targets --locked --offline` | 通过 | workspace 全 target 编译成功 |
| `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | 通过 | 零 warning |
| `cargo test --workspace --all-targets --locked --offline` | 通过 | 173 passed；0 failed / ignored / measured / filtered；`lang-codegen` / `lang-lsp` 当前测试数为 0 |
| `cargo build -p lang-cli --locked --offline` | 通过 | CLI debug target 构建成功 |
| `cargo tree -p lang-frontend --edges all --locked --offline` | 通过 | 仅 `lang-frontend`；manifest / lock 无差异，未新增 normal / dev / build 依赖 |
| `git diff --check` | 通过 | 无空白错误 |
| Architecture 同步 | 完成 | 已记录 Statement table、block 入口、函数 body 三态、九套 fixture 与恢复边界 |
