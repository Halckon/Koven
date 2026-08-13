# SPEC-0007: 建立 Pratt 表达式 Parser

| 字段 | 值 |
|---|---|
| 状态 | draft |
| Goal ID | `KOV-P1-007` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [`agent-language-design-guide-v0.5.md`](../agent-language-design-guide-v0.5.md)（不足以授权完整 Parser）；候选 [`agent-language-design-guide-v0.6.md`](../agent-language-design-guide-v0.6.md)（尚未生效，本 Spec 的目标契约） |
| 批准依据 | 当前持续 Goal 的“后续 Specs 和 ADR 自动确认并实施”站立授权；待 v0.6 门禁解除后生效 |
| 前置 Spec | SPEC-0004、SPEC-0006 `done` |
| 前置 ADR | 无 |
| 关联 ADR | [ADR-0003](../adr/0003-diagnostic-architecture.md)、[ADR-0004](../adr/0004-source-span-position-model.md) |
| 阻塞项 | 候选 v0.6 尚未由用户明确启用 |
| 影响范围 | `lang-frontend`、语言 fixture、Architecture |
| 语言语义变更 | 否；只有候选 v0.6 生效后，本 Spec 才实现其中已经确定的语义 |

## 1. Goal

完成后，`lang-frontend` 能从同一 `SourceMap` 的 `LexedFile` 确定性构造带精确 `Span` 的
具体索引式 Expression / TypeRef AST，覆盖生效后 v0.6 定义的完整 Pratt 表达式入口，并把
Lexer 与 Parser 诊断合并为稳定全序；非法源码不会触发 `panic!`。这将为 SPEC-0008 至
SPEC-0011 提供唯一表达式解析基础，而不提前实现声明、控制流、lambda 或全文件恢复。

## 2. 背景

SPEC-0004 已建立四类 typed table 和源码归属检查，SPEC-0006 已提供完整、无重叠且覆盖源码
的 lexeme 流，但仓库尚无具体语法 payload 或 Parser。现行 v0.5 只有优先级摘要，没有完整
primary、postfix、类型引用、局部恢复和 AST `Span` 契约，不能据此实施。候选 v0.6 已补齐
这些边界，但在用户明确启用之前，本文件只能保持草案。

## 3. 范围与需求

### 3.1 输入、产物与阶段边界

- 新增 `lang_frontend::parser` 的独立表达式入口。入口接收 `&SourceMap` 和 `&LexedFile`，
  校验 source identity 后跳过 trivia，并以唯一 EOF 结束；不得重新扫描源码或复制 Lexer。
- 产物拥有 SPEC-0004 的 `AstFile`，使用具体 `Expression`、`TypeRef` payload，保存根
  `ExpressionId` 和合并后的结构化诊断。用户语法错误进入产物；source、AST、诊断目录等
  不变量失败返回具体内部错误。
- 独立入口要求恰好一个完整表达式后到 EOF。字符串插值递归使用同一 Pratt 核心，但以当前
  `InterpolationEnd` 为 stop token，不能把该 token 消费为普通 `}` 或尾随输入。
- Parser 诊断与已有 Lexer 诊断共同通过 SPEC-0003 的全序排序。Parser 遇到已被 Lexer
  诊断的 invalid、reserved word 或未闭合 string / interpolation 区域时，消费最小 poison
  单元并抑制仅由同一词法错误派生的重复语法诊断；其他独立语法错误仍保留。

### 3.2 具体索引式 AST

- 为表达式至少提供下列可区分 payload；所有父子关系只保存 `ExpressionId` / `TypeRefId`，
  不嵌套拥有子树：名称与 `this`、布尔 / null / 数字 / `Char` 字面量、分组、
  string 及其 text / interpolation 分段、prefix、cast、binary、assignment、member / safe
  member、call、index、postfix `!!`、未绑定 / 绑定 callable reference 和显式 error 节点。
- operator 使用封闭枚举表达语义类别，不用源码字符串或 Lexer 的任意 `Symbol` 冒充 AST
  kind。`to` 是 v1 唯一中缀调用；`as` / `as?`、`in` / `!in`、`is` / `!is` 和全部赋值种类
  必须保持可区分。
- string AST 按源码顺序保存非空 text `Span`，以及同时携带合成 `Span` 与 `ExpressionId` 的
  interpolation segment；开始 / 结束引号和 `${` / `}` 不伪造成普通表达式。字符串自身
  `Span` 仍按 v0.6 合成规则覆盖定界符。
- `TypeRef` 至少可区分限定名（只允许末段携带递归类型实参）、仅限定类型可有的单个末尾
  `?`、函数类型及其 `move` 标记和显式 error 节点；组成关系使用 `TypeRefId`。函数类型自身
  在 v0.6 不可空，`() -> T?` 唯一表示返回 nullable `T`。cast、`is` / `!is` 的右侧引用该表，
  不将类型名解析为表达式。
- 所有节点使用 v0.6 的合成 `Span` 规则：叶节点保留 token 范围，prefix 从运算符起，postfix
  到后缀止，binary / cast / assignment 从左端到右端，括号 / 调用 / 索引 / string / type
  delimiter 完整时包含两端定界符。错误恢复不得制造跨过无关后续 token 的成功节点范围。
- TypeRef 的合成范围同样明确：限定类型从首个 `Identifier` 起，到末段 `Identifier` 终；有
  type arguments 时改为到匹配 `>` 终，再有 nullable `?` 时最终到该 `?` 终。函数类型从
  `move`（存在时）否则左 `(` 起，到 return `type_ref` 终。Type arguments 即使不单建节点也
  须把 `<` 至匹配 `>` 完整纳入限定类型。TypeRef error node 只覆盖本次实际消费的错误区域，
  只有在 stop / delimiter / EOF 且无 token 可消费时才可为空；缺 `>`、函数参数 `)`、`->`
  后返回类型等情况截止于本构造最后实际消费位置，不越过外层 stop token 或 delimiter。

### 3.3 Pratt 语法覆盖

- primary 覆盖 v0.6 定义的名称、字面量、`this`、括号表达式、string 和未绑定
  callable reference；不把尚未授权的声明或 lambda 起始符当作 primary。
- postfix 循环按左结合覆盖 `.name`、`?.name`、基本位置调用、单表达式索引、`!!` 和绑定
  callable reference，并支持其任意合法链式组合。
- prefix 只接受 `!`、一元 `+` 和一元 `-`，按右结合解析；`own`、`inout`、`borrow`、
  `move` 不是通用 prefix expression。
- 完整覆盖候选 v0.6 的 14 档优先级：cast，乘法，加法，range，`to`，Elvis，成员关系 /
  类型测试，比较，相等，逻辑与 / 或及赋值。每档 binding power 只在一个实现位置定义。
- 左结合、右结合和不结合分别由结构测试锁定。range、成员关系 / 类型测试、比较、相等各自
  是不结合组；同组第二个运算符产生诊断，不静默建立链式 AST。不同优先级仍严格按表归组。
- 基本调用只接受逗号分隔的位置表达式（可为空且不允许 trailing comma），index 恰好接受
  一个表达式。候选 v0.6 延后的直接 `Identifier = expression` 命名参数及 `own` / `inout` /
  `borrow` 参数模式必须产生专用诊断，不能被误建为普通 assignment AST；显式分组的
  `f((a = b))` 仍合法。空 / 多 index、trailing lambda 和 use-site 类型实参按各自首先违反
  的现有产生式拒绝，不借本 Spec 创设后续语法。
- Lexer 按既定规则拆出的无 trivia 相邻不支持运算符组合必须作为一个 parser 错误区域拒绝，
  不能部分解释为两个合法运算符；合法 prefix / binary 邻接仍按语法解析。

### 3.4 稳定诊断与最小局部恢复

在生产目录连续注册以下错误码；全部为 `error`，主消息固定为单行文本：

| 错误码 | 稳定含义 | 主消息 | 主 `Span` |
|---|---|---|---|
| `L0009` | 需要表达式 | `expected expression` | 当前非 trivia token；到输入边界时为当前 stop / EOF 空范围 |
| `L0010` | 缺少匹配的结束定界符 | `expected closing delimiter` | 应出现结束定界符处的当前 token；到输入边界时为 stop / EOF 空范围，开始定界符作为关联标签 |
| `L0011` | 成员或 callable reference 后缺少名称 | `expected member or reference name` | `.`、`?.` 或 `::` 后的当前 token；到边界时为空范围 |
| `L0012` | 不结合运算符组被链式使用 | `non-associative operator chain` | 同一不结合组的第二个运算符 token |
| `L0013` | 完整根表达式后仍有输入 | `unexpected trailing token` | 第一个未消费的非 trivia token |
| `L0014` | 需要类型引用 | `expected type reference` | cast / type-test 后的当前 token；到边界时为空范围 |
| `L0015` | v1 不支持的相邻运算符组合 | `unsupported operator` | v0.6 定义的完整无 trivia 相邻组合 |
| `L0016` | 当前 Spec 不支持的调用实参形式 | `unsupported argument form` | 引入该形式的最小 token；模式实参为关键字，直接命名实参为 `=` |

- 缺失 operand / type / name 只插入有明确 `Error` kind 和实际消费范围的 Expression /
  TypeRef 节点，不伪造名称或字面量；位于 stop / EOF 且不消费 token 的 error node 可以是
  空范围。父节点可引用该 error ID，使每个有效输入边界都仍有确定的根和可检查结构。
- `L0012` 保留第二个运算符之前的合法 AST，并把第二个运算符及可解析右侧消费为错误区域；
  `L0013` 从首个尾随 token 前进到当前 stop；`L0016` 只在当前实参层同步到顶层逗号或右括号，
  不把嵌套结构的逗号误作同步点。
- 本 Spec 只做 delimiter、argument list、string interpolation 和独立表达式末尾所需的局部
  同步；每条恢复路径必须消费 token 或到达 stop / EOF。跨 statement / item 的同步及抑制
  级联属于 SPEC-0011。

### 3.5 Fixture

- 复用 SPEC-0005 / 0006 的真实 Cargo fixture target，新增
  `phase1/parser-expression-pass/` 和 `phase1/parser-expression-fail/`。
- pass case 要求 Lexer 与 Parser 均无诊断、根节点存在且消费到 EOF；fail `.ko` 继续使用
  同名 `.diag` 的 `Ldddd<TAB>start_byte<TAB>end_byte` 格式，可同时列出 Lexer 与 Parser
  错误码，行序必须等于合并后的诊断全序。
- 两个 suite 均须至少枚举一个真实 `.ko`；零用例、缺失 / 孤立 sidecar、非法行、额外或
  缺失诊断均失败。该 sidecar 仍只是仓库测试格式，不是公共诊断协议。
- 使用 Rust 标准库和已有模块实现，不增加 parser generator、arena 或 snapshot 依赖。

## 4. 非目标

- 不实现 SPEC-0008 的变量 / 函数 / 泛型声明、函数类型声明位置、block 或 statement parser。
- 不实现 SPEC-0009 的 `if` / `when` / loop 及 class / interface / enum / object 结构。
- 不实现 SPEC-0010 的 lambda、解构、命名参数、参数模式、trailing lambda、use-site 类型实参
  或其他高级调用形式；本 Spec 只保留后续可复用的 call / index AST 形态。
- 不实现 SPEC-0011 的全文件错误恢复、statement / item 同步或 Phase 1 完整范例。
- 不做名称解析、overload / callable reference 绑定、类型合法性、assignment target、range /
  index 语义、常量求值、smart cast 或所有权检查。
- 不解析 `module` / `import`、注解、lambda、collection literal 或任何 v2+ 语法；`@` 不因
  Lexer 已有 token 就获得表达式语义。
- 不创设可空函数类型或类型分组语法；候选 v0.6 只能给限定类型附加单个 `?`。
- 不接入 CLI renderer、LSP、机器诊断协议、增量 parsing、绿色树或 formatter trivia 附着。

## 5. 验收标准

- [ ] 公共入口验证 `SourceMap` / `LexedFile` identity；来自另一 map 的词法产物返回具体内部
      错误，普通用户错误只进入解析产物。
- [ ] 每种 Expression / TypeRef payload 均由结构测试构造；父子只用 typed ID，所有叶 / 合成
      `Span` 与 source identity 精确匹配 v0.6，table 遍历顺序确定。
- [ ] primary、全部 postfix 链、一元表达式、cast、14 档 operator 及所有 assignment kind
      都有最小正例；字符串测试包含 text、空串、单 / 多 interpolation 和嵌套表达式。
- [ ] 优先级测试至少逐对覆盖相邻档，并用跨三档表达式锁定整体分组；左结合和右结合检查
      AST 方向，四个不结合组分别用同类及混合组成员链产生 `L0012` 并锁定第二运算符 `Span`。
- [ ] `to` 被解析为唯一中缀调用，普通 identifier 和软词 `infix` 不获得中缀语义；
      `own` / `inout` / `borrow` / `move` 不被通用 prefix 接受。
- [ ] callable reference 覆盖未绑定 / 绑定形式、成员 / safe member、call / index / `!!` 的合法
      链式组合；缺失名称稳定产生 `L0011`。
- [ ] TypeRef 测试覆盖限定路径、只在末段出现的嵌套泛型、限定类型的单个 nullable、函数
      类型、`move` 函数类型及非法 / 缺失类型；`() -> T?` 的 `?` 只属于返回类型。cast 和
      `is` / `!is` 指向 `TypeRefId` 而非 ExpressionId，并拒绝 `T??`、函数类型后的 `?`、
      非末段类型实参、类型分组、projection 和 use-site 型变。
- [ ] TypeRef `Span` 分别锁定限定类型从首个名称至末段名称、泛型限定类型至匹配 `>`、
      nullable 限定类型至 `?`、普通函数类型从 `(` 至 return type、`move` 函数类型从 `move`
      至 return type；type arguments 的 `<` 至 `>` 完整纳入所属限定类型。error node 只覆盖
      实际消费范围，仅在 stop / delimiter / EOF 无 token 可消费时为空；缺 `>`、函数参数
      `)`、`->` 后返回类型时不越过外层 stop token 或 delimiter。
- [ ] 基本调用覆盖零 / 单 / 多位置实参、`f((a = b))` 及嵌套 delimiter；index 覆盖恰好一个
      任意表达式并证明 range key 不等于切片。直接命名和三种模式实参产生 `L0016`；空 / 多
      index、trailing comma、trailing lambda 和 use-site 类型实参由对应现有诊断拒绝。
- [ ] `L0009`–`L0016` 一类一码，测试断言 error、固定消息、精确主 `Span`、必要关联 opener
      标签、恢复后的根 / 后续 token 和确定性顺序。
- [ ] v0.6 列出的每个不支持运算符组合均产生唯一 `L0015`，主范围覆盖完整组合；合法的
      prefix / binary 紧邻反例证明实现没有按字符外观过度聚合。
- [ ] Lexer poison 与未闭合 string / interpolation 测试证明不产生同源重复 Parser 诊断；包含
      另一处独立语法错误时，两阶段诊断仍按 SPEC-0003 全序完整保留。
- [ ] 空文件、仅 trivia、缺 operand / closer / name / type、尾随 token 和多错误输入均不
      `panic!` 或零进展；独立入口只在完整消费表达式时返回有效根，否则产生对应诊断。
- [ ] 相同源码在不同加载顺序及重复运行下产生相同 AST kind、相对 `Span` 和诊断；实现不
      依赖随机 hash 迭代、机器路径或隐式全局状态。
- [ ] parser-expression pass / fail suite 各自真实执行至少一个 `.ko`，同时证明零用例与非法
      sidecar 配置会失败；Phase 0 与 Lexer fixture 继续执行。
- [ ] `cargo tree -p lang-frontend --edges all --locked --offline` 及 manifest / lock diff 证明
      没有新增 normal、dev 或 build 依赖。
- [ ] frontend 窄测试和 workspace fmt、check、Clippy、test、CLI build 基线通过，无 ignored /
      filtered case 被隐瞒。
- [ ] Architecture 更新为表达式 Parser、AST 所有权与诊断合并的实现事实，并继续明确
      SPEC-0008 至 0011 尚未实现。

## 6. 技术方案与边界

候选最小公共 API：

```rust
pub fn parse_expression(
    sources: &SourceMap,
    lexed: &LexedFile,
) -> Result<ParsedExpression, ParserInternalError>;

pub struct ParsedExpression {
    ast: ExpressionAst,
    root: ExpressionId,
    diagnostics: Vec<Diagnostic>,
}

pub type ExpressionAst = AstFile<(), (), Expression, TypeRef>;
```

`ParsedExpression` 只暴露 source、只读 AST、根 ID 和有序诊断 getter；语法错误由显式 error
node 保持结构可遍历，而不是让调用方猜测缺失节点。未使用的 item /
statement table 保持为空，不为后续语法创建占位 payload。若实现时 Rust type alias 的公开
错误信息或扩展性明显不佳，可改为等价的薄 wrapper，但不得改变本 Spec 的所有权和可观察
结果。

Parser 使用忽略 trivia 的 cursor 读取 `LexemeKind::Token`，Pratt 核心接收最低 binding
power 与明确 stop set。postfix 在 prefix / primary 后先循环，infix 表只保留一个实现真源，
不复制 guide 的数字到多个函数或测试 helper。定界符和不支持相邻组合需要回查 `Span` 的
byte adjacency，但不得因此重新词法分析文本。

AST 构建继续调用 SPEC-0004 的受检插入 API；诊断继续调用 SPEC-0003 的生产目录和排序 API。
无需 visitor、CST、token tree、通用 parser-combinator facade、feature flag 或新 crate。

## 7. 实施计划

1. [ ] 注册 `L0009`–`L0016`，建立具体 Expression / TypeRef payload 和表达式产物 API
   → 验证：目录、typed ID、source identity 与 AST `Span` 窄测试
2. [ ] 实现 primary、string / interpolation、type ref、postfix 与局部 delimiter 恢复
   → 验证：结构、链式访问、类型引用和局部错误窄测试
3. [ ] 实现唯一 Pratt 表、结合性 / 不结合约束、unsupported operator 与完整消费检查
   → 验证：逐档优先级、结合性及 `L0009`–`L0016` integration test
4. [ ] 接入 parser-expression pass / fail fixture 和两阶段诊断合并
   → 验证：真实 fixture target、poison 去重与 sidecar 自检
5. [ ] 同步 Spec 验收记录与 Architecture → 验证：全 workspace 基线与 staged diff

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 候选 v0.6 guide（不含本 Spec） | `docs(guide): draft v0.6 expression grammar` |
| 2 | 本 Spec 草案（保持阻塞） | `docs(spec): draft Pratt expression parser (SPEC-0007)` |
| 3 | 用户启用 v0.6 后的全仓真源指针切换（不含本 Spec） | `docs(guide): activate language guide v0.6` |
| 4 | Parser、AST payload、诊断、测试 / fixture、Architecture 与完成记录 | `feat(frontend): add Pratt expression parser (SPEC-0007)` |

候选 guide 与实现保持独立提交。候选文件存在或本草案落盘均不授权实施；用户明确启用 v0.6
后，本 Spec 依据有效站立授权按逻辑顺序进入 `approved` / `in-progress`，无需创建批准状态
提交；最终实现提交包含 `done` 状态和全部实际验收证据。

## 9. 未决问题

- 候选 v0.6 是否生效；在用户明确指定前，这是阻止本 Spec 批准和实施的唯一门禁。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_expression --locked --offline` | 未执行 | Spec 尚处于 draft，Parser 尚未实现 |
| `cargo test -p lang-frontend --test fixtures --locked --offline` | 未执行 | parser-expression fixture 尚未建立 |
| `cargo tree -p lang-frontend --edges all --locked --offline` | 未执行 | 实现阶段检查依赖图及 manifest / lock diff |
| `cargo fmt --all -- --check` | 未执行 | 实现阶段执行 |
| `cargo check --workspace --all-targets --locked --offline` | 未执行 | 实现阶段执行 |
| `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | 未执行 | 实现阶段执行 |
| `cargo test --workspace --all-targets --locked --offline` | 未执行 | 实现阶段执行 |
| `cargo build -p lang-cli --locked --offline` | 未执行 | 实现阶段执行 |
