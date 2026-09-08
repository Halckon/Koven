# SPEC-0012: 解析 callable 参数契约与 typed call argument

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-012` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.14 文档集](../guides/v0.34-pre-restructure/00-index.md)：[`02-lexical-spec.md` 第 7 节](../guides/v0.34-pre-restructure/02-lexical-spec.md)、[`03-grammar-core.md` 第 2、3、5、6 节](../guides/v0.34-pre-restructure/03-grammar-core.md)、[`04-grammar-declarations-blocks.md` 第 7 节](../guides/v0.34-pre-restructure/04-grammar-declarations-blocks.md)、[`05-grammar-calls-lambda.md` 第 9 节](../guides/v0.34-pre-restructure/05-grammar-calls-lambda.md)、[`06-roadmap.md` Phase 1](../guides/v0.34-pre-restructure/06-roadmap.md) |
| 批准依据 | 用户在当前持续 Goal 中授予继续分阶段实施 Specs 的站立授权，并于 2026-08-20 明确启用 `docs/guide/` v0.14 取代 v0.9；v0.12、v0.13 已合入 v0.14 |
| 前置 Spec | SPEC-0010、SPEC-0011 `done` |
| 前置 ADR | 无 |
| 关联 ADR | [ADR-0003](../../adr/accepted/0003-diagnostic-architecture.md)、[ADR-0004](../../adr/accepted/0004-source-span-position-model.md) |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Lexer / Parser / typed AST、语言 fixture、Architecture |
| 语言语义变更 | 否；实施现行 v0.14 已定义的 callable 参数与调用边界；其中 `&` 是对已实现 Lexer 基线的规范增量 |

## 1. Goal

完成后，`lang-frontend` 能以同一封闭语义 marker 保存具名函数参数、函数类型参数和调用
实参的 `Value` / `Borrow` / `Inout` 契约，并确定性解析位置、命名及显式 `borrow` / `&`
实参；basic、typed、member 与 chained call 共享同一 `CallArgument` AST、结构化诊断和
owner-aware 恢复，同时保持 typed-call strict trial 无副作用、整根线性且不进行名称、类型或
所有权检查。

## 2. 背景

SPEC-0007 当前只把调用实参保存为 `Vec<ExpressionId>`，并以历史错误码 `L0016` 拒绝命名与
模式实参；SPEC-0008 的具名函数值参数和函数类型参数也尚未保存参数契约。SPEC-0010、
SPEC-0011 已提供 lambda 与隐式 `Unit`，因此调用边界的全部 Parser 前置已经完成。

现行 v0.14 把 callable 契约收敛为 `Value`、`Borrow`、`Inout` 三种：声明侧使用无 marker、
`borrow`、`inout`，调用侧使用无 marker、可选 `borrow`、强制表达 `Inout` 意图的 `&`。
Phase 1 只忠实保存源码结构；callee 参数映射和契约匹配属于 Phase 2，place 的移动、复制、
借用与可变性属于 Phase 3。

## 3. 范围与需求

### 3.1 `&` Lexer 增量

- 在固定符号中新增独立单字符 `&` token；准确 Rust variant 名可遵循现有 `Symbol` 风格。
  最长匹配继续保证 `&&` 是一个 logical-and token，而不是两个 `&`。
- 锁定 `&x`、`&&x` 与 `& &x` 的 lexeme 分类、数量和 UTF-8 字节 `Span`；v0.14 后单个 `&`
  不再产生 `L0001`。现有其他固定符号、invalid 区域覆盖与 token 全覆盖不变量保持不变。
- Lexer 只分类，不赋予表达式语义。`&` 唯一合法 Parser 位置是 call argument mode；它不进入
  Pratt prefix / binary 表，也不成为按位与。调用外的 `&` 由所在语法 owner 拒绝，而不是
  Lexer 报非法字符。

### 3.2 统一 callable 参数 marker

- 新增唯一封闭表示 `ParameterModeMarker::{Borrow(Span), Inout(Span)}`；marker 缺失表示
  `Value`。不得保留 `Own` 变体，也不得用 kind 与 `Span` 两个可独立变化的字段制造半状态。
- `ValueParameter` 在现有 name / colon / TypeRef 字段上增加
  `mode_marker: Option<ParameterModeMarker>`；声明侧只接受 `borrow` / `inout`，并把参数
  `Span` 从真实 marker（若有）否则名称起，合成到 TypeRef 最后实际消费位置。
- 函数类型参数改为内嵌 `FunctionTypeParameter`，至少保存完整 `span`、同一
  `mode_marker` 和唯一 `type_ref: TypeRefId`；`TypeRef::Function.parameters` 改为源码有序的
  `Vec<FunctionTypeParameter>`，不新增 AST table。函数类型和具名函数参数共享 marker 语义，
  但仍保留各自已有的名称 / TypeRef 结构。
- 声明或函数类型参数已消费首个 marker 后，连续第二个及以后 `borrow` / `inout` 各产生
  `L0039 duplicate parameter mode`，消费多余 marker、保留首个 marker，再继续恢复同一
  参数；空 Error TypeRef 的 boundary 插入点不得扩大父参数范围。
- typed-call strict trial 必须与正式 TypeRef parser 同步识别带 marker 的函数类型，包括嵌套
  泛型和 `move` 函数类型。失败 trial 不分配 AST、不发诊断、不移动 cursor，且不得因仍按旧
  `Vec<TypeRefId>` 语法把合法候选回退成比较表达式。

### 3.3 typed、named 与 mode call argument

- basic 与 typed call 复用唯一产生式顺序：可选 `Identifier =`、可选调用点 mode、唯一
  expression。必须接受位置、命名、模式和命名加模式组合，包括 `f(e)`、`f(name = e)`、
  `f(borrow e)`、`f(&e)`、`f(name = borrow e)`、`f(name = &e)`。
- 调用点 mode 字母表精确为关键字 `borrow` 与符号 `&`；二者分别保存为共享语义枚举的
  `Borrow(Span)` / `Inout(Span)`。调用点 `inout` 和 `own` 不被接受，声明侧 `inout` 仍合法。
- 实参起点的未分组 `Identifier =` 提交 named prefix；`f((a = b))` 仍是位置 assignment
  expression。`borrow (a = b)` 与 `&(a = b)` 是合法模式实参；mode 后顶层
  `Identifier =` 是非法逆序，不得改判为 named argument。
- `Expression::Call.arguments` 唯一改为 `Vec<CallArgument>`；不增加 `CallArgumentId` 或
  第五张 AST table，也不同时保留旧 `Vec<ExpressionId>`。内嵌 payload 至少等价于：

```rust
pub struct NamedArgumentPrefix {
    pub name_span: Span,
    pub equals_span: Span,
}

pub struct CallArgument {
    pub span: Span,
    pub named_prefix: Option<NamedArgumentPrefix>,
    pub mode_marker: Option<ParameterModeMarker>,
    pub value: ExpressionId,
}
```

- `NamedArgumentPrefix` 必须封闭保存真实名称和 `=`，不能用两个独立 `Option` 表达半个前缀。
  call argument 从 name、mode 或 value 中最先出现者起，到 value / 最后实际消费 token 终；
  边界处空 Error value 不扩张 argument。call / typed call 的既有整体 `Span` 不变。
- Phase 1 不按 callee 名称、parameter 名称、operand 形态或推测类型改变解析；argument 和
  operand 始终按源码顺序保留。命名后必须继续命名、重复 / 缺失 / 多余参数、函数值禁用
  命名实参以及 marker 与 callee 契约是否兼容，均留给 Phase 2 / 3。

### 3.4 `L0033`–`L0039` 与恢复

在集中生产目录连续注册下列 error severity 诊断，固定主消息与类别拼写一致：

| 错误码 | 固定主消息 | 主 `Span` / 最小行为 |
|---|---|---|
| `L0033` | `expected argument value` | 已提交 name / mode 后在 `,`、`)`、调用方 hard stop 或 EOF 取空边界；其他普通非法 token 的主范围只覆盖首 token，owner-aware 消费的完整错误区由唯一 Error value 覆盖 |
| `L0034` | `expected argument separator` | 完整 value 后直接遇下一 argument 起点时取其起点空范围并保留；其他非法 token 的主范围只覆盖首 token，再 owner-aware 消费完整错误区到 call 顶层边界 |
| `L0035` | `unsupported argument empty element` | 初始或 separator 后直接 `,`，精确覆盖并消费该逗号，追加一个在逗号起点为空的 Error value argument |
| `L0036` | `unsupported argument trailing comma` | 完整项后 `,` 紧接 `)`，精确覆盖并消费逗号，追加一个在 `)` 起点为空的 Error value argument并保留 `)` |
| `L0037` | `invalid argument mode ordering` | mode 后顶层 `Identifier =`，主范围精确覆盖 `=`；消费该名称与 `=` 错误区，不保存 name prefix，再解析唯一 value |
| `L0038` | `duplicate argument mode` | 首个 mode 后每个连续 `borrow` / `&` 各覆盖并消费自身 token；AST 只保存首个 mode |
| `L0039` | `duplicate parameter mode` | 具名函数或函数类型参数的首个声明侧 marker 后，每个连续多余 `borrow` / `inout` 各覆盖并消费自身 token，保留首个 marker |

- `f(borrow &x)` 产生 `L0038`；`f(& &x)` 同样按两个独立 `&` 产生 `L0038`。`f(&&x)` 中
  Lexer 产生单一 `&&`，因此它不是 duplicate mode，而按 `L0033` 缺 argument value 恢复。
- 已提交 name / mode 后缺值且边界为逗号时，该逗号作为当前项 separator 消费；若随后是
  `)`，不得再追加 trailing-comma 诊断或第二个 Error argument。空项后直接 `)` 同样只保留
  `L0035` 的一个根因。
- `)` 只由 call owner 消费；缺 `)` 复用 `L0010`。nested call / group / index / lambda / block
  与 string / interpolation 内的逗号和 closer 不得冒充当前 call 边界。lexical owner 回到入口
  baseline 后，异形调用方 hard closer 即使遇到未闭合普通 delimiter也必须保留并停止。
- Lexer invalid / reserved token和 `L0004`–`L0006` terminal owner 已表达根因时，只构造 Error
  value，不在同一 `Span` 或同一缺 closer 根因上追加 Parser 级联。每轮消费输入或停在明确
  boundary，单个 call 及声明 / TypeRef 参数列表保持 `O(n)`。
- `L0016 unsupported argument form` 作为已发布历史编号继续保留在 catalog，但生产 Parser
  完成本 Spec 后不得再产生它，也不得改变或复用其含义；此前对应的合法形态定向迁移为正例。

### 3.5 测试与回归接入

- 新增专用 `parser_call_argument` integration test，覆盖 basic / typed / member / chained call、
  declaration / function TypeRef marker、AST / `Span`、diagnostic recovery、owner / terminal
  Lexer、multi-SourceMap identity、递归预算和线性 inspection。
- 复用现有 expression / declaration pass / fail fixture suite 承载代表性 `.ko` / `.diag` 证据，
  不为同一 Parser 入口复制两套 fixture runner 与守卫；现有非零、非法 sidecar、orphan pair
  自检继续执行。
- 人工审阅并只迁移 SPEC-0007 中现在合法的 `L0016` 负例；expression、declaration、block、
  lambda、implicit-Unit、strict trial 与全部既有 fixture suite 继续执行。

## 4. 非目标

- 不实现 Phase 2 的名称解析、位置 / 命名参数映射、重复 / 缺失 / 多余实参检查、重载、类型
  检查、函数值禁用命名实参或 place / temporary 分类。
- 不实现 Phase 3 的 `Copyable` 复制 / 移动、use-after-move、共享 / 独占借用、可变 place、
  借用冲突或 ASAP 析构；不因 operand 看似 temporary 而在 Parser 拒绝 `&`。
- 不实现参数默认值、`vararg`、`Own` 契约、调用点 `own` / `inout`、trailing lambda、自定义
  operator 或通用 prefix / binary / bitwise `&`。
- 不实现 SPEC-0013 局部解构、SPEC-0014 完整文件、跨声明恢复、control-flow、class-family、
  module / import、名称 / 类型 / 所有权阶段、IR、codegen 或标准库。
- 不新增 crate、第三方依赖、parser generator、CST / green tree、增量 Parser、AST table、
  通用 visitor 或公共机器诊断协议；不顺带重构无关 Parser / Lexer 代码。

## 5. 验收标准

- [x] Lexer compile-pass / 单元测试证明 `&x`、`&&x`、`& &x` 按 v0.14 最长匹配产生精确 token、
      lexeme 数量与 UTF-8 `Span`；裸 `&` 不再产生 `L0001`，其他词法诊断与全覆盖不变量无回归。
- [x] AST 测试证明三处参数共享唯一 `ParameterModeMarker::{Borrow, Inout}`；marker kind 与真实
      token `Span` 封闭，缺失表示 `Value`，不存在 `Own`、双 Option 半状态、新 AST table 或
      调用点 `Inout` Span 固定字符长度假设。
- [x] declaration / TypeRef compile-pass 覆盖无 marker、`borrow`、`inout`、多参数、nested / move
      function type，以及带 marker 函数类型作为 typed-call type argument；三种参数契约结构
      和完整 `Span` 精确且源码有序。
- [x] call compile-pass 覆盖位置、命名、`borrow`、`&`、命名加两种 mode 的六种形态，以及
      basic / typed / member / chained call、grouped assignment、nested lambda / call / group /
      index / string-interpolation operand；所有 call 都使用唯一 `Vec<CallArgument>`。
- [x] compile-fail 覆盖 `L0033`–`L0038` 每类至少一个最小用例，断言固定消息、精确空 / 非空
      主 `Span`、Error value、name / mode 保留状态、argument 数量和恢复后顺序；`inout` / `own`
      调用点与调用外 `&` 均准确拒绝而不产生 Lexer 非法字符诊断。
- [x] declaration 与函数类型各覆盖首个 marker 后一个及多个重复 marker；每个多余 token 恰好
      产生一条 `L0039 duplicate parameter mode`，首个 marker、真实 TypeRef / Error TypeRef
      与后续参数均被保留，不复用 `L0038` 或声明列表诊断。
- [x] empty element、trailing comma、name / mode 后缺值、mode 逆序、连续 mode 与缺 separator
      的组合矩阵证明分支优先级正确；同一空项 / 缺值不产生第二个 Error argument或同根因
      `L0009` / `L0036` 级联。
- [x] owner recovery 覆盖 call 嵌套于 call、group、index、lambda、block 与 string
      interpolation：当前 call 顶层逗号 / `)` 归 call owner，异形 outer hard closer 被保留，
      同形局部 closer 先消费；`L0004`–`L0006` 不提前关闭父 owner或派生同义 Parser 诊断。
- [x] strict typed-call trial 的白盒测试证明正式 TypeRef 与 trial 同步接受 marker 函数类型；
      Match / NoMatch 均无 AST、诊断、cursor 副作用，查询 `O(1)`，N→2N 同族输入保持整根
      `O(n)`，并继续重验 1024 递归预算。
- [x] 参数与 CallArgument 的正常、恢复、空 Error value / TypeRef、缺 closer 及 multi-SourceMap
      场景遵守同源 UTF-8 半开范围；零宽 child 插入点不扩大父节点，跨 SourceMap 混用受控返回
      具体内部错误而不 panic。
- [x] 生产 catalog 精确新增 `L0033`–`L0039`，`L0016` 继续注册但生产 Parser 不再发出；既有
      `L0001`–`L0032` 的编号、消息、severity 与排序不变，诊断全序确定。
- [x] 现有 expression / declaration pass / fail suite 各真实执行代表性 `.ko` / `.diag`；零 suite、
      非法 sidecar、orphan pair 与空范围 policy 自检继续失败，现有全部 fixture suite 无回归。
- [x] SPEC-0006 Lexer、SPEC-0007 expression / TypeRef / strict trial、SPEC-0008 declaration、
      SPEC-0009 block、SPEC-0010 lambda 与 SPEC-0011 implicit Unit 窄测试通过；只定向迁移
      v0.14 明确取代的 `&` / `L0016` 历史负例。
- [x] `cargo tree -p lang-frontend --edges all --locked --offline` 与 manifest / lock diff 证明未新增
      normal、dev 或 build 依赖。
- [x] frontend 窄测试及 workspace fmt、check、Clippy、test、CLI build 基线通过；记录实际
      passed / failed / ignored / measured / filtered 数量，不把未执行检查写成通过。
- [x] Architecture 更新为已实现的 `&` Lexer、统一参数 marker、CallArgument AST、strict trial、
      `L0033`–`L0039`、fixture 与 owner 恢复事实，并明确 Phase 2 / 3 检查与解构仍未实现。

## 6. 技术方案与边界

在既有 `lang-frontend` 边界内完成增量。Lexer 的 fixed-symbol 表把 `&&` 排在 `&` 前并复用
现有最长匹配。Parser AST 在现有四张 typed table 内增加三个内嵌 payload：共享
`ParameterModeMarker`、`FunctionTypeParameter` 与 `CallArgument` / `NamedArgumentPrefix`；
不改变 typed ID 基础或公开解析入口。

具名函数和函数类型参数先消费至多一个声明侧 marker，再复用既有名称 / TypeRef 与 list
恢复。call argument owner按固定顺序试探 named prefix、消费至多一个调用侧 marker、解析
唯一 operand；错误分支复用既有 owner-aware scanner 和 terminal-owner index，不另写按行或
仅计括号的扫描器。strict-call 预索引与正式 TypeRef parser 同轮扩展，维持一次全流构建和
`O(1)` 查询。

本 Spec 沿用 ADR-0003 / ADR-0004，不需要新 ADR，也不引入依赖或改变 workspace / crate
方向。准确 Rust 类型名可在不改变封闭状态、typed 关系、真实 token `Span`、诊断和可观察
恢复时微调。

## 7. 实施计划

1. [x] 为 Lexer 增加 `&` 固定符号并锁定与 `&&` 的最长匹配
   → 验证：Lexer 单元 / fixture 的 token、`Span`、`L0001` 迁移与既有符号回归
2. [x] 引入共享参数 marker，迁移具名函数与函数类型参数及 strict typed-call trial
   → 验证：三契约 AST、`L0039`、正式 / trial 一致、递归预算和线性 inspection
3. [x] 引入内嵌 CallArgument / NamedArgumentPrefix 并实现 named / `borrow` / `&` dispatch
   → 验证：六种正例、basic / typed / chained call、assignment 歧义与精确 `Span`
4. [x] 注册 `L0033`–`L0038` 并完成 call owner-aware 恢复、级联抑制与历史 `L0016` 迁移
   → 验证：每类最小反例、组合优先级、nested owner、terminal Lexer 与单调前进
5. [x] 在现有 expression / declaration fixture 中接入代表性 pass / fail，并复跑全部受影响窄测试
   → 验证：真实 `.ko` / `.diag`、既有非零守卫、诊断全序、multi-SourceMap 与无 panic
6. [x] 同步 Spec 验收记录与 Architecture
   → 验证：workspace 全基线、依赖树、staged diff 和文档事实一致

v0.14 已由用户明确启用；SPEC-0010、SPEC-0011 均为 `done`，本 Spec 已依据当前持续 Goal 的
站立授权按 `draft → approved → in-progress` 的逻辑顺序推进，无需单独的批准状态提交。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | `&` Lexer、统一参数 marker、CallArgument AST / Parser、strict trial、`L0033`–`L0039`、测试 / fixture、Architecture 与完成记录 | `feat(frontend): parse callable arguments and modes (SPEC-0012)` |

实现提交只属于 SPEC-0012，不混入解构、完整文件、Phase 2 / 3 检查或其他语法。实现、测试、
Architecture、逐项验收与 `done` 状态全部满足后才能创建本表提交；提交成功后才可完成关联
Goal。

## 9. 未决问题

- 无。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| v0.14 版本级启用与前置门禁 | 满足 | 用户于 2026-08-20 明确启用 `docs/guide/` v0.14；SPEC-0010、SPEC-0011 均为 `done`；当前持续 Goal 的站立授权有效 |
| `cargo test -p lang-frontend --test lexer --locked --offline` | 通过 | 19 passed；0 failed / ignored / measured / filtered |
| `cargo test -p lang-frontend --test parser_call_argument --locked --offline` | 通过 | 28 passed；0 failed / ignored / measured / filtered |
| `cargo test -p lang-frontend --test parser_expression --locked --offline` | 通过 | 52 passed；0 failed / ignored / measured / filtered |
| `cargo test -p lang-frontend --test parser_declaration --locked --offline` | 通过 | 22 passed；0 failed / ignored / measured / filtered |
| `cargo test -p lang-frontend --test parser_block --locked --offline` | 通过 | 20 passed；0 failed / ignored / measured / filtered |
| `cargo test -p lang-frontend --test parser_lambda --locked --offline` | 通过 | 16 passed；0 failed / ignored / measured / filtered |
| `cargo test -p lang-frontend --test parser_implicit_unit --locked --offline` | 通过 | 7 passed；0 failed / ignored / measured / filtered |
| `cargo test -p lang-frontend --test fixtures --locked --offline` | 通过 | 22 passed；0 failed / ignored / measured / filtered；expression / declaration 新 pass / fail 已真实执行 |
| `cargo test -p lang-frontend --lib --locked --offline` | 通过 | 20 passed；0 failed / ignored / measured / filtered；含 strict trial 与 call 多错误区线性白盒回归 |
| `cargo test -p lang-frontend --doc --locked --offline` | 通过 | 7 passed；0 failed / ignored / measured / filtered |
| `cargo fmt --all -- --check` | 通过 | 无格式差异 |
| `cargo check --workspace --all-targets --locked --offline` | 通过 | workspace 全 target 检查通过 |
| `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | 通过 | 0 warnings |
| `cargo test --workspace --all-targets --locked --offline` | 通过 | 合计 238 passed；0 failed / ignored / measured / filtered |
| `cargo build -p lang-cli --locked --offline` | 通过 | `lang-cli` dev build 成功 |
| `cargo tree -p lang-frontend --edges all --locked --offline` | 通过 | 仅 `lang-frontend` 自身；Cargo manifests 与 `Cargo.lock` 无差异，未新增依赖 |
| `git diff --check` 与 Markdown 相对链接审计 | 通过 | 无 whitespace error；本 Spec 完成时全仓 213 个本地 Markdown 目标均存在 |
| 独立只读审查 | 通过 | 恢复行为 / `O(n)` 游标审查与文档 / 治理审查均无剩余阻塞 |
| Architecture 同步 | 通过 | 已记录 `&`、参数 marker、CallArgument、strict trial、L0033–L0039、fixture 与 Phase 2 / 3 边界 |
