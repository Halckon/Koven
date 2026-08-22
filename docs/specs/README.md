# Koven Spec 路线图

本目录依据现行 [v0.25 语言规范](../guide/00-index.md) 维护可独立验证、可独立
提交的 Goal；已完成 Spec 保留其实施时适用的 guide 引用。路线图负责排序，Spec 文件负责
定义一次交付；路线图条目本身不等于已批准的 Spec，也不授权实现。

[v0.24](../guide/00-index.md) 已由用户明确启用并取代 v0.23；它封闭 enum case type、`when`
穷尽性与 smart cast，SPEC-0021 已完成实施与验收。

[v0.25](../guide/01-design-decisions.md#25-条件-copyable内联递归与结构化解构v025) 已由用户
明确启用并取代 v0.24；它封闭条件 `Copyable`、有限内联布局、intrinsic `Box` 与结构化
解构契约，SPEC-0022 已完成实施与验收。

[v0.23](../guide/00-index.md) 已由用户明确启用并取代 v0.22；它封闭 nominal/generic/interface/
委托契约，SPEC-0020 已完成实施。

[v0.22](../guide/00-index.md) 已由用户明确启用并取代 v0.21；它正式封闭最小数值后缀、默认
数值类型与基础类型检查契约，并把词法 / AST 增量交给 SPEC-0066、类型阶段交给 SPEC-0019。
v0.21 已封闭单文件双命名空间、作用域、预声明环境与 L0079–L0081 名称诊断，并把首个
Phase 2 实现边界交给 SPEC-0018。
v0.20 已封闭 class-family、类型级 companion、匿名内部类边界与窄化接口委托，并把委托
Parser 拆为 SPEC-0064；
v0.12、v0.13 内容已合入 v0.14。
SPEC-0010、SPEC-0011、SPEC-0012、SPEC-0013、SPEC-0014、SPEC-0015、SPEC-0016、SPEC-0017、SPEC-0018、SPEC-0019、SPEC-0020、SPEC-0021、SPEC-0022、SPEC-0023、SPEC-0027、SPEC-0028、SPEC-0058、SPEC-0059、SPEC-0062、SPEC-0063、SPEC-0064、SPEC-0065、SPEC-0066、SPEC-0067、SPEC-0068、SPEC-0069、SPEC-0070、SPEC-0071、SPEC-0072、SPEC-0073、SPEC-0074、SPEC-0075、SPEC-0076、SPEC-0077、SPEC-0078、SPEC-0079、SPEC-0080、SPEC-0081、SPEC-0082、SPEC-0083、SPEC-0084、SPEC-0085、SPEC-0086、SPEC-0087、SPEC-0088、SPEC-0089、SPEC-0090、SPEC-0091、SPEC-0092、SPEC-0093、SPEC-0094、SPEC-0095、SPEC-0096、SPEC-0097、SPEC-0098、SPEC-0099、SPEC-0100、SPEC-0101、SPEC-0102、SPEC-0103、SPEC-0104、SPEC-0105、SPEC-0106、SPEC-0107、SPEC-0108、SPEC-0109、SPEC-0110、SPEC-0111、SPEC-0112、SPEC-0113、SPEC-0114、SPEC-0115、SPEC-0116、SPEC-0117、SPEC-0118、SPEC-0119、SPEC-0120、SPEC-0121、SPEC-0122、SPEC-0123、SPEC-0124、SPEC-0125、SPEC-0126、SPEC-0127、SPEC-0128、SPEC-0129、SPEC-0130、SPEC-0131、SPEC-0132、SPEC-0133、SPEC-0134、SPEC-0135、SPEC-0136、SPEC-0137、SPEC-0138、SPEC-0139、SPEC-0140、SPEC-0141、SPEC-0142、SPEC-0143、SPEC-0144、SPEC-0145、SPEC-0146、SPEC-0147、SPEC-0148、SPEC-0149、SPEC-0150、SPEC-0151、SPEC-0152、SPEC-0153、SPEC-0154、SPEC-0155、SPEC-0156、SPEC-0157、SPEC-0158、SPEC-0159、SPEC-0160、SPEC-0161、SPEC-0162、SPEC-0163、SPEC-0164、SPEC-0165、SPEC-0166、SPEC-0167、SPEC-0168、SPEC-0169、SPEC-0170、SPEC-0171、SPEC-0172 已完成；尚未物化的条目仍只是候选 Goal，不因编号预留而
自动获得实现授权。

## Goal 与提交工作流

Spec 进入 `in-progress` 前必须已有单份明确确认或有效站立授权作为批准依据，并满足所有前置
Spec `done`、前置 ADR `accepted` 和阻塞项解除。存在有效站立授权时，可以在同一工作流中
按逻辑顺序完成 `approved → in-progress`，不要求中间状态形成独立提交。

后续使用 Goal 时固定按以下顺序推进：

1. 以 `完成 SPEC-NNNN：〈Goal〉；按本 Spec 验收并创建独立提交` 作为 Goal objective。
2. 把 Spec 标为 `in-progress`，按“实施计划”逐项执行；每一步先跑窄检查。
3. 完成测试后运行 Spec 要求的 workspace 基线，并同步 Architecture 当前事实。
4. 填写验证记录、逐项勾选验收，把 Spec 标为 `done`。
5. 只暂存该 Spec 范围，检查 staged diff 后提交；提交信息必须包含 `(SPEC-NNNN)`。
6. 提交成功后再把 Goal 标记为完成。未提交、验收缺失或检查未运行时不得完成 Goal。

一个提交不得混合两个 Spec。一个 Spec 可以有多个提交；含 Rust / 目标语言实现的提交必须
可构建、可测试，workspace 建立前的纯文档提交执行适用的链接、术语和 diff 检查。所有提交
都引用同一 Spec 编号；最终实现提交同时包含验收记录、Architecture 更新和 `done` 状态。
若一个 Spec 无法形成清晰的独立提交边界，应在批准前继续拆分。

Spec 草案、批准和 `in-progress` 状态不要求分别提交；最终实现提交仍必须独占一个 Spec，
并包含 `done` 状态、实际验收记录和 Architecture 更新。新 ADR 的决策正文仍应形成独立文档
提交，但可在首次提交时直接为 `accepted`，不得把 ADR 与依赖它的实现混入同一提交。
简化的是人工确认和重复状态文书，不是行为验收；任何检查只有实际成功后才能记录为通过。

## Phase 0 Spec 队列

Phase 0 已细化为以下实际 Spec。状态以各 Spec 文件为准，并按前置关系依次推进：

| 顺序 | Spec | Goal | 前置条件 |
|---|---|---|---|
| 1 | [SPEC-0001](./0001-bootstrap-cargo-workspace.md) | 建立可检查的五 member Cargo workspace | [ADR-0002](../adr/0002-bootstrap-workspace-layout.md) accepted |
| 2 | [SPEC-0002](./0002-source-span-foundation.md) | 建立统一 source / `Span` 基础设施 | SPEC-0001 done；[ADR-0004](../adr/0004-source-span-position-model.md) accepted |
| 3 | [SPEC-0003](./0003-structured-diagnostics.md) | 建立稳定、确定性的结构化诊断核心 | SPEC-0002 done；[ADR-0003](../adr/0003-diagnostic-architecture.md)、[ADR-0004](../adr/0004-source-span-position-model.md) accepted |
| 4 | [SPEC-0004](./0004-indexed-ast-foundation.md) | 建立保留 `Span` 的索引式 AST 基础 | SPEC-0001、0002 done |
| 5 | [SPEC-0005](./0005-language-fixture-harness.md) | 建立真实枚举 `.ko` 文件且零用例失败的 harness | SPEC-0001、0002、0003、0004 done |

Phase 0 的建议依赖关系：

```text
SPEC-0001
    └── SPEC-0002
            ├── SPEC-0003 ──┐
            └── SPEC-0004 ──┴── SPEC-0005
```

## 后续 Spec 候选

下表预留编号、Phase、单一 Goal 和依赖门槛。没有文件链接的候选项尚不是 Spec；只在前置
Phase 接近完成、适用 guide 已明确且必要 ADR 已接受时，才从模板创建文件并补齐可执行
计划。已有链接但尚未批准或仍带阻塞项的文件继续保持 `draft`，不得据此实施。这能避免
长期空壳 Spec 与实现事实漂移。

### Phase 1：Lexer / Parser

| Spec | 单一 Goal | 前置 / 决策门槛 |
|---|---|---|
| [0006](./0006-deterministic-lexer.md) | Lexer 覆盖字面量、标识符、关键字和 trivia（`done`） | 0002、0003、0005 `done`；v0.5 已生效 |
| [0007](./0007-pratt-expression-parser.md) | Pratt parser 覆盖完整表达式优先级（`done`） | 0004、0006 `done`；v0.6 已生效；站立授权已记录 |
| [0008](./0008-declaration-parser.md) | 解析 `val` / `var` / `const val`、函数、泛型与调用点类型实参（`done`） | 0007 `done`；v0.7 已生效；站立授权已记录 |
| [0009](./0009-block-statement-parser.md) | 解析 block / statement 序列与函数 block body（`done`） | 0008 `done`；v0.8 已生效；站立授权已记录 |
| [0010](./0010-lambda-literal-parser.md) | 解析 lambda literal（`done`） | 0009 `done`；v0.9 已生效；站立授权已记录 |
| [0011](./0011-implicit-unit-return.md) | 解析具名函数省略返回标注时的隐式 `Unit`（`done`） | 0009 `done`；v0.9 已生效；站立授权已记录 |
| [0012](./0012-callable-parameter-and-call-argument-parser.md) | 解析统一 callable 参数 marker、typed call argument、命名实参与调用点 `borrow` / `&` 模式（`done`） | 0010、0011 `done`；v0.14 已生效；站立授权已记录 |
| [0013](./0013-local-val-destructuring-parser.md) | 解析 block / lambda body 内局部 `val` 解构（`done`） | 0012 `done`；适用 guide 已启用；站立授权已记录 |
| [0014](./0014-complete-file-parser.md) | 组合 0007–0013 已有节点为完整文件并实现声明分隔、跨声明恢复与级联抑制（`done`） | 0011、0013 `done`；v0.15 已封闭完整文件恢复契约；站立授权已记录 |
| [0062](./0062-top-level-declaration-separators.md) | 按 v0.16 修正顶层声明换行 / 分号分隔（`done`） | 0014 `done`；v0.16 已生效；当前持续 Goal 的站立授权 |
| [0015](./0015-package-import-parser.md) | 解析 `package` / Kotlin 风格 `import`（`done`） | 0014、0062 `done`；v0.17 已生效；当前持续 Goal 的站立授权 |
| [0016](./0016-control-flow-parser.md) | 解析 `if` / `when` / `super`、loop-family 与 jump 控制流（`done`） | 0009、0014 `done`；v0.18 已生效；当前持续 Goal 的站立授权 |
| [0063](./0063-postfix-error-propagation-parser.md) | 解析 postfix 错误传播 `?`（`done`） | 0016 `done`；v0.19 已生效；当前持续 Goal 的站立授权 |
| [0017](./0017-class-family-parser.md) | 解析 `value class` / `class` / `interface` / `enum class` / 具名 `object` / `companion object`（`done`） | 0014 `done`；v0.20 已生效；当前持续 Goal 的站立授权；不含接口委托 |
| [0064](./0064-interface-delegation-parser.md) | 增量解析 `Interface by valField` 接口实现委托（`done`） | 0017 `done`；v0.20 已生效；当前持续 Goal 的站立授权 |
| [0065](./0065-parser-module-decomposition.md) | 按职责拆分 Parser 内部模块并保持现有行为（`done`） | 0017、0062–0064 `done`；用户明确要求拆分 Parser |
| [0066](./0066-numeric-literal-suffixes.md) | 识别 `L` / `u` / `f` 数值后缀并在 Parser AST 保留规范化身份（`done`） | 0006、0007 `done`；v0.22 已生效；用户明确批准实施 |
| [0068](./0068-frontend-adversarial-matrix.md) | 增加 Lexer / 完整文件 Parser 对抗组合矩阵（`done`） | 0006、0014 `done`；当前持续 Goal 的站立授权 |
| [0069](./0069-parser-entry-adversarial-matrix.md) | 覆盖独立 expression / declaration / block Parser 对抗矩阵（`done`） | 0007–0009、0068 `done`；当前持续 Goal 的站立授权 |
| [0072](./0072-pratt-operator-matrix.md) | 锁定 Pratt 全优先级、结合性与不结合组矩阵（`done`） | 0007、0069 `done`；当前持续 Goal 的站立授权 |
| [0073](./0073-lexer-boundary-matrix.md) | 锁定固定词边界、符号最长匹配与注释优先级矩阵（`done`） | 0006、0068 `done`；当前持续 Goal 的站立授权 |
| [0074](./0074-parser-token-inventory-matrix.md) | 以完整词法片段库存覆盖四个公开 Parser 入口（`done`） | 0006、0014、0069、0073 `done`；当前持续 Goal 的站立授权 |
| [0075](./0075-parser-lexical-owner-placement-matrix.md) | 覆盖 lexical owner 在代表性语法位置的恢复矩阵（`done`） | 0006、0014、0069、0074 `done`；当前持续 Goal 的站立授权 |
| [0076](./0076-parser-diagnostic-witness-matrix.md) | 建立已发布 Parser 诊断的公开入口 witness 矩阵（`done`） | 0003、0014、0069、0075 `done`；当前持续 Goal 的站立授权 |
| [0077](./0077-parser-trivia-invariance-matrix.md) | 建立完整语法组合的非换行 trivia 等价矩阵（`done`） | 0006、0014、0073、0076 `done`；当前持续 Goal 的站立授权 |
| [0078](./0078-parser-line-break-boundary-matrix.md) | 建立 LF / CRLF 与 comment 换行的结构边界矩阵（`done`） | 0014、0016、0017、0062、0077 `done`；当前持续 Goal 的站立授权 |
| [0079](./0079-parser-prefix-truncation-matrix.md) | 建立合法完整语法逐 UTF-8 前缀的 EOF 恢复矩阵（`done`） | 0006、0014、0068、0074、0078 `done`；当前持续 Goal 的站立授权 |
| [0080](./0080-parser-token-omission-matrix.md) | 建立合法完整语法逐显著 token 的缺失恢复矩阵（`done`） | 0006、0014、0069、0075、0079 `done`；当前持续 Goal 的站立授权 |
| [0081](./0081-parser-lexical-poison-replacement-matrix.md) | 建立合法完整语法逐显著 token 的词法 poison 替换矩阵（`done`） | 0006、0014、0074、0075、0080 `done`；当前持续 Goal 的站立授权 |
| [0082](./0082-parser-token-duplication-matrix.md) | 建立合法完整语法逐显著 token 的重复恢复矩阵（`done`） | 0006、0014、0073、0080、0081 `done`；当前持续 Goal 的站立授权 |
| [0083](./0083-parser-lexical-poison-insertion-matrix.md) | 建立合法完整语法 token gap 的词法 poison 插入矩阵（`done`） | 0006、0014、0077、0081、0082 `done`；当前持续 Goal 的站立授权 |
| [0084](./0084-parser-adjacent-token-transposition-matrix.md) | 建立合法完整语法相邻 token 的交换恢复矩阵（`done`） | 0006、0014、0073、0080、0082、0083 `done`；当前持续 Goal 的站立授权 |
| [0085](./0085-parser-entry-prefix-truncation-matrix.md) | 建立 expression / declaration / block 独立入口逐 UTF-8 前缀恢复矩阵（`done`） | 0006–0009、0069、0079 `done`；当前持续 Goal 的站立授权 |
| [0086](./0086-parser-entry-token-omission-matrix.md) | 建立 expression / declaration / block 独立入口逐显著 token 缺失恢复矩阵（`done`） | 0006–0009、0069、0080、0085 `done`；当前持续 Goal 的站立授权 |
| [0087](./0087-parser-entry-token-duplication-matrix.md) | 建立 expression / declaration / block 独立入口逐显著 token 重复恢复矩阵（`done`） | 0006–0009、0069、0082、0085、0086 `done`；当前持续 Goal 的站立授权 |
| [0088](./0088-parser-entry-lexical-poison-replacement-matrix.md) | 建立 expression / declaration / block 独立入口逐显著 token 词法 poison 替换矩阵（`done`） | 0006–0009、0069、0081、0085–0087 `done`；当前持续 Goal 的站立授权 |
| [0089](./0089-parser-entry-lexical-poison-insertion-matrix.md) | 建立 expression / declaration / block 独立入口逐 token gap 词法 poison 插入矩阵（`done`） | 0006–0009、0069、0083、0085–0088 `done`；当前持续 Goal 的站立授权 |
| [0090](./0090-parser-entry-adjacent-token-transposition-matrix.md) | 建立 expression / declaration / block 独立入口相邻 token 交换恢复矩阵（`done`） | 0006–0009、0069、0084、0085–0089 `done`；当前持续 Goal 的站立授权 |
| [0091](./0091-parser-entry-trivia-invariance-matrix.md) | 建立 expression / declaration / block 独立入口非换行 trivia 等价矩阵（`done`） | 0006–0009、0069、0077、0085–0090 `done`；当前持续 Goal 的站立授权 |
| [0092](./0092-parser-entry-line-break-boundary-matrix.md) | 建立 expression / declaration / block 独立入口结构性换行边界矩阵（`done`） | 0006–0009、0069、0078、0085–0091 `done`；当前持续 Goal 的站立授权 |
| [0093](./0093-parser-entry-adversarial-output-invariants.md) | 强化 expression / declaration / block 对抗矩阵的 Lexer / AST / diagnostic / root 不变量（`done`） | 0006–0009、0068、0069、0085–0092 `done`；当前持续 Goal 的站立授权 |
| [0094](./0094-parser-token-inventory-output-invariants.md) | 强化 120-item token inventory 的四入口 Lexer / AST / diagnostic / root / directive 不变量（`done`） | 0006–0009、0014、0073、0074、0093 `done`；当前持续 Goal 的站立授权 |
| [0095](./0095-parser-lexical-owner-output-invariants.md) | 强化 lexical-owner 矩阵的 Lexer / AST / diagnostic / root / sentinel 不变量（`done`） | 0006、0014、0075、0093、0094 `done`；当前持续 Goal 的站立授权 |
| [0096](./0096-parser-diagnostic-witness-output-invariants.md) | 强化 Parser diagnostic witness 的 Lexer / AST / diagnostic / root / directive 不变量（`done`） | 0003、0014、0076、0093–0095 `done`；当前持续 Goal 的站立授权 |
| [0097](./0097-parser-trivia-output-invariants.md) | 强化 Parser trivia 等价矩阵的 Lexer / AST / diagnostic / root / directive 不变量（`done`） | 0006、0014、0077、0093–0096 `done`；当前持续 Goal 的站立授权 |
| [0098](./0098-parser-line-break-output-invariants.md) | 强化 line-break carrier 分段及 Parser AST / diagnostic / root / directive 不变量（`done`） | 0006、0014、0078、0093–0097 `done`；当前持续 Goal 的站立授权 |
| [0099](./0099-parser-file-mutation-output-invariants.md) | 强化六个完整文件恢复矩阵的共享 AST / diagnostic / root / directive 不变量（`done`） | 0006、0014、0079–0084、0093–0098 `done`；当前持续 Goal 的站立授权 |
| [0100](./0100-parser-entry-line-break-output-invariants.md) | 统一 line-break carrier 真源并强化独立入口 AST / diagnostic / typed-root 不变量（`done`） | 0006–0009、0078、0092、0093–0099 `done`；当前持续 Goal 的站立授权 |
| [0101](./0101-parser-entry-trivia-output-invariants.md) | 强化独立入口 trivia 精确分段并复用双解析 shape（`done`） | 0006–0009、0077、0091、0093–0100 `done`；当前持续 Goal 的站立授权 |
| [0102](./0102-frontend-adversarial-output-invariants.md) | 强化完整文件对抗矩阵的双 Lexer / 双 Parser 公开产物不变量（`done`） | 0006、0014、0068、0093–0101 `done`；当前持续 Goal 的站立授权 |
| [0103](./0103-lexer-boundary-output-invariants.md) | 强化固定词 / 符号边界矩阵的双 Lexer 公开产物不变量（`done`） | 0006、0073、0093–0102 `done`；当前持续 Goal 的站立授权 |
| [0104](./0104-pratt-operator-output-invariants.md) | 强化 Pratt 运算符矩阵的双 Lexer / 双 Parser 与完整诊断不变量（`done`） | 0006、0007、0072、0093–0103 `done`；当前持续 Goal 的站立授权 |
| [0105](./0105-parser-entry-adversarial-lexer-invariants.md) | 强化独立入口对抗矩阵的双 Lexer 确定性不变量（`done`） | 0006–0009、0069、0093、0103–0104 `done`；当前持续 Goal 的站立授权 |
| [0106](./0106-parser-token-inventory-lexer-invariants.md) | 强化 token inventory 分类、四入口与定向回归的双 Lexer 不变量（`done`） | 0006–0009、0014、0074、0094、0103–0105 `done`；当前持续 Goal 的站立授权 |
| [0107](./0107-parser-lexical-owner-lexer-invariants.md) | 强化 lexical-owner 矩阵的双 Lexer 确定性不变量（`done`） | 0006、0014、0075、0095、0103–0106 `done`；当前持续 Goal 的站立授权 |
| [0108](./0108-parser-diagnostic-witness-lexer-invariants.md) | 强化 diagnostic-witness 矩阵的双 Lexer 确定性不变量（`done`） | 0003、0006–0009、0014、0076、0096、0103–0107 `done`；当前持续 Goal 的站立授权 |
| [0109](./0109-parser-trivia-lexer-invariants.md) | 强化 trivia 等价矩阵的双 Lexer 确定性不变量（`done`） | 0006、0014、0077、0097、0103–0108 `done`；当前持续 Goal 的站立授权 |
| [0110](./0110-parser-line-break-lexer-invariants.md) | 强化完整文件 line-break 边界矩阵的双 Lexer 确定性不变量（`done`） | 0006、0014、0078、0098、0100、0103–0109 `done`；当前持续 Goal 的站立授权 |
| [0111](./0111-parser-file-mutation-lexer-invariants.md) | 强化六个完整文件 mutation 矩阵的共享双 Lexer 确定性不变量（`done`） | 0006、0014、0079–0084、0099、0103–0110 `done`；当前持续 Goal 的站立授权 |
| [0112](./0112-parser-entry-line-break-lexer-invariants.md) | 强化独立入口 line-break 边界矩阵的双 Lexer 确定性不变量（`done`） | 0006–0009、0078、0092、0100、0103–0111 `done`；当前持续 Goal 的站立授权 |
| [0113](./0113-parser-entry-trivia-lexer-invariants.md) | 强化独立入口 trivia 等价矩阵的双 Lexer 确定性不变量（`done`） | 0006–0009、0077、0091、0101、0103–0112 `done`；当前持续 Goal 的站立授权 |
| [0114](./0114-parser-entry-mutation-lexer-invariants.md) | 强化六个独立入口 mutation 矩阵的共享双 Lexer 确定性不变量（`done`） | 0006–0009、0085–0090、0093、0103–0105、0111–0113 `done`；当前持续 Goal 的站立授权 |
| [0115](./0115-fixture-frontend-output-invariants.md) | 强化 pass / fail fixture 的双 Lexer / 双 Parser 公开产物不变量（`done`） | 0005–0017、0062–0066、0103–0114 `done`；当前持续 Goal 的站立授权 |

### Phase 2：名称与类型检查

| Spec | 单一 Goal | 前置 / 决策门槛 |
|---|---|---|
| [0018](./0018-single-file-name-resolution.md) | 完成单文件声明收集、作用域和名称诊断（`done`） | 0014 `done`；v0.21 已生效；当前持续 Goal 的站立授权 |
| [0019](./0019-basic-type-checking.md) | 检查基础类型、局部推导、隐式 `Unit` / 显式返回类型与 `Nothing`（`done`） | 0018、0066 `done`；v0.22 已生效 |
| [0020](./0020-nominal-generic-interface-types.md) | 检查泛型及 class / interface / enum / value class 名义类型与窄化接口委托（`done`） | 0019、0017、0064 `done`；v0.23 已明确启用 |
| [0021](./0021-when-exhaustiveness-smart-cast.md) | 实现 `when` 穷尽性与 smart cast（`done`） | 0020、0016 `done`；v0.24 已明确启用 |
| [0022](./0022-copyable-structural-destructuring.md) | 推导条件 `Copyable`、检查有限内联布局与结构化解构类型（`done`） | 0019、0020 `done`；v0.25 已明确启用 |
| [0067](./0067-callable-type-checking.md) | 检查 callable 选择、实参映射、参数模式与 place/temporary 类别（`done`） | 0019、0020、0022 `done`；v0.25 callable 契约已生效；当前持续 Goal 的站立授权 |
| [0023](./0023-sequential-container-types.md) | 检查顺序容器的名义类型、元素可存储性、核心构造和索引 place 类型（`done`） | 0020、0022、0067 `done`；v0.6 已生效 |
| [0130](./0130-name-resolution-frontend-input-invariants.md) | 强化名称解析 suite 前置双 Lexer / 双 Parser 公开产物不变量（`done`） | 0018、0093、0115、0128、0129 `done`；当前持续 Goal 的站立授权 |
| [0131](./0131-type-checking-frontend-input-invariants.md) | 强化类型检查核心 suite 前置双 Lexer / 双 Parser 公开产物不变量（`done`） | 0019–0023、0067、0130 `done`；当前持续 Goal 的站立授权 |
| [0132](./0132-callable-type-frontend-input-invariants.md) | 强化 callable 类型 suite 前置双 Lexer / 双 Parser 公开产物不变量（`done`） | 0067、0130、0131 `done`；当前持续 Goal 的站立授权 |
| [0133](./0133-container-type-frontend-input-invariants.md) | 强化顺序容器类型 suite 前置双 Lexer / 双 Parser 公开产物不变量（`done`） | 0023、0130–0132 `done`；当前持续 Goal 的站立授权 |
| [0134](./0134-copyability-type-frontend-input-invariants.md) | 强化 copyability 类型 suite 前置双 Lexer / 双 Parser 公开产物不变量（`done`） | 0022、0130–0133 `done`；当前持续 Goal 的站立授权 |
| 0024 | 检查 `Map` / `MutableMap` 的 key 契约、value 所有权约束和查询结果类型 | 0020；新 guide 明确 key 等价关系、返回所有权与修改 API |
| 0025 | 建立多文件 package / import 名称解析 | 0015、0018；[ADR-0005](../adr/0005-package-source-root-mapping.md) `accepted`；新 guide 封闭 import 冲突与跨 package 可见性 |
| 0026 | 检查 `object` / `companion object` 关联成员、编译期常量和无运行时状态边界 | 0020、0017；v0.20 已生效；新 guide 封闭可接受的 const 表达式与类型 |

Phase 2 roadmap 中的“泛型单态化类型层面准备”仍是未物化 Goal。SPEC-0067 明确把泛型
callable 调用点推导与实例化留在后续；它不是 SPEC-0027–0029 的前置，但进入依赖具体实例的
SSA / codegen Goal 前，必须先由独立 Spec 封闭类型实参推导、实例 identity 与诊断边界。

### Phase 3：所有权与借用

| Spec | 单一 Goal | 前置 / 决策门槛 |
|---|---|---|
| [0027](./0027-variable-ownership-use-after-move.md) | 建立变量所有权状态并检测 use-after-move（`done`） | 0019、0020、0022、0067 `done`；当前持续 Goal 的站立授权 |
| [0028](./0028-conditional-copy-structural-move.md) | 实现条件复制、移动与消费式解构检查（`done`） | 0022、0027 `done`；当前持续 Goal 的站立授权 |
| 0029 | 检查 `Value` / `Borrow` / `Inout` 契约、调用点 `borrow` / `&` 冲突并确定 ASAP 析构点 | 0028；新 guide 明确借用与析构规则 |
| 0030 | 检查顺序容器元素 place 的读取、借用、替换与析构所有权规则 | 0023、0029；v0.6 生效 |
| 0031 | 检查 `Map` / `MutableMap` 查询和修改的 key / value 所有权规则 | 0024、0029；新 guide 明确完整 Map 契约 |
| 0032 | 检查 move closure 与 `Transferable` | 0020、0029；适用 guide 明确标记能力推导 |

### Phase 4：SSA、LLVM 与原生 AOT

| Spec | 单一 Goal | 前置 / 决策门槛 |
|---|---|---|
| 0033 | 实现最小 typed SSA IR 与 verifier | 0021、0029；接受 SSA ADR |
| 0034 | 把标量表达式和控制流 lower 到 LLVM | 0033；接受 LLVM / target ADR |
| 0035 | 生成聚合、class 分配和显式 drop / free | 0034、0029；接受 runtime ABI ADR |
| 0036 | 生成顺序容器的单一连续缓冲区基元、边界检查和 drop 路径 | 0023、0030、0035；接受 runtime ABI ADR |
| 0037 | 生成 `Map` / `MutableMap` 查询与修改的 runtime 基元 | 0024、0031、0035；接受 runtime ABI ADR、Map 存储策略 ADR |
| 0038 | 生成捕获闭包环境和无捕获函数指针 | 0034、0032 |
| 0039 | 生成 object、链接 `main` 并把 `error()` 映射到 abort | 0035、0038；接受 linker 决策 |
| 0040 | 生成 DWARF 并用首个支持平台的调试器验收 | 0039；接受 debug mapping ADR |
| 0041 | 提供用户可见 `extern` FFI | 0039；新 guide 定义 FFI 与所有权边界，非 v1 主路径 |

### Phase 5：最小标准库

| Spec | 单一 Goal | 前置 / 决策门槛 |
|---|---|---|
| 0042 | 用编译器构建并运行 `lang-std` 目标语言源码 | 0039；接受 bootstrap / runtime ADR |
| 0043 | 实现 prelude、基础操作和 `error()` | 0042 |
| 0044 | 实现条件可复制的 `Pair` 与 `Result` | 0042、0028、0035 |
| 0045 | 实现独占 `Box` 与共享 `Rc` 所有权类型 | 0042、0028、0035 |
| 0046 | 提供 Array / List / MutableList 的目标语言公共 API 与顺序算法 | 0036、0043、0045 |
| 0047 | 提供 Map / MutableMap 的目标语言公共 API 与键值算法 | 0037、0043、0045；新 guide 明确完整 Map 契约 |
| 0048 | 为顺序容器实现 `map` / `filter` / `reduce` / `forEach` | 0046、0038 |
| 0049 | 实现同步 File / BufferedReader / 标准流 | 0043、0039 |
| 0050 | 实现 thread / channel | 0042、0032 |
| 0051 | 实现目标语言测试发现与断言 runner | 0042；新 guide 定义最小 `@Test` 语法 |

### Phase 6：工具链

| Spec | 单一 Goal | 前置 / 决策门槛 |
|---|---|---|
| 0052 | 定义并解析最小 `project.toml` | 0025；接受 package schema ADR |
| 0053 | 实现依赖解析与确定性 `project.lock` 核心 | 0052；接受解析 / 锁定策略 ADR |
| 0054 | 由 package CLI 编排 manifest、解析与锁定 | 0053 |
| 0055 | 让 LSP 发布 frontend 诊断 | 0025、0021、0003 |
| 0056 | 让 LSP 支持跳转定义 | 0055、0025、0021 |
| 0057 | 实现稳定、幂等的格式化器 | 0014、0006 |
| [0058](./0058-textmate-grammar.md) | 提供 TextMate grammar 与回归 fixture（`done`） | 0014、0015 `done`；当前持续 Goal 的站立授权 |
| [0059](./0059-tree-sitter-grammar.md) | 提供 Tree-sitter grammar 与 corpus（`done`） | 0014、0015 `done`；当前持续 Goal 的站立授权 |
| [0070](./0070-tree-sitter-word-contract.md) | 锁定 Tree-sitter external scanner 与生产 Lexer 词表契约（`done`） | 0006、0059 `done`；当前持续 Goal 的站立授权 |
| [0071](./0071-textmate-lexical-contract.md) | 执行 TextMate symbol / literal 正则并交叉验证生产 Lexer（`done`） | 0006、0058 `done`；当前持续 Goal 的站立授权 |
| [0116](./0116-grammar-bridge-frontend-invariants.md) | 强化 TextMate / Tree-sitter 交叉测试的双 Lexer / 双 Parser 产物不变量（`done`） | 0006、0014、0058–0059、0070–0071、0103–0115 `done`；当前持续 Goal 的站立授权 |
| [0117](./0117-parser-expression-suite-output-invariants.md) | 强化 expression Parser 核心 suite 的双 Lexer / 双 Parser 产物不变量（`done`） | 0006–0007、0093、0103–0105、0115–0116 `done`；当前持续 Goal 的站立授权 |
| [0118](./0118-parser-declaration-suite-output-invariants.md) | 强化 declaration Parser 核心 suite 的双 Lexer / 双 Parser 产物不变量（`done`） | 0006、0008、0093、0103–0105、0115–0117 `done`；当前持续 Goal 的站立授权 |
| [0119](./0119-parser-block-suite-output-invariants.md) | 强化 block Parser 核心 suite 的双 Lexer / 双 Parser 产物不变量（`done`） | 0006、0009、0093、0103–0105、0115–0118 `done`；当前持续 Goal 的站立授权 |
| [0120](./0120-parser-lambda-suite-output-invariants.md) | 强化 lambda Parser 核心 suite 三入口的双 Lexer / 双 Parser 产物不变量（`done`） | 0006、0010、0093、0103–0105、0115–0119 `done`；当前持续 Goal 的站立授权 |
| [0121](./0121-parser-call-argument-suite-output-invariants.md) | 强化 call argument Parser 核心 suite 双入口的双 Lexer / 双 Parser 产物不变量（`done`） | 0006、0012、0093、0103–0105、0115–0120 `done`；当前持续 Goal 的站立授权 |
| [0122](./0122-parser-local-destructuring-suite-output-invariants.md) | 强化 local destructuring Parser 核心 suite 三入口的双 Lexer / 双 Parser 产物不变量（`done`） | 0006、0013、0093、0103–0105、0115–0121 `done`；当前持续 Goal 的站立授权 |
| [0123](./0123-parser-control-flow-suite-output-invariants.md) | 强化 control-flow Parser 核心 suite 双入口的双 Lexer / 双 Parser 产物不变量（`done`） | 0006、0016、0093、0103–0105、0115–0122 `done`；当前持续 Goal 的站立授权 |
| [0124](./0124-parser-error-propagation-suite-output-invariants.md) | 强化错误传播 Parser 核心 suite 三入口的双 Lexer / 双 Parser 产物不变量（`done`） | 0006、0016、0063、0093、0103–0105、0115–0123 `done`；当前持续 Goal 的站立授权 |
| [0125](./0125-parser-class-family-suite-output-invariants.md) | 强化 class-family Parser 核心 suite 三入口的双 Lexer / 双 Parser 产物不变量（`done`） | 0006、0017、0093、0103–0105、0115–0124 `done`；当前持续 Goal 的站立授权 |
| [0126](./0126-parser-interface-delegation-suite-output-invariants.md) | 强化接口委托 Parser 核心 suite 的双 Lexer / 双 declaration Parser 产物不变量（`done`） | 0006、0017、0064、0093、0103–0105、0115–0125 `done`；当前持续 Goal 的站立授权 |
| [0127](./0127-parser-implicit-unit-suite-output-invariants.md) | 强化隐式 Unit Parser 核心 suite 的双 Lexer / 双 declaration Parser 产物不变量（`done`） | 0006、0011、0093、0103–0105、0115–0126 `done`；当前持续 Goal 的站立授权 |
| [0128](./0128-parser-file-suite-output-invariants.md) | 强化完整文件 Parser 核心 suite 双入口的双 Lexer / 双 Parser 产物不变量（`done`） | 0006、0014、0093、0103–0105、0115–0127 `done`；当前持续 Goal 的站立授权 |
| [0129](./0129-lexer-core-suite-output-invariants.md) | 强化 Lexer 核心 suite 的双运行公开产物不变量（`done`） | 0006、0073、0093、0103、0106、0115、0128 `done`；当前持续 Goal 的站立授权 |
| [0135](./0135-parser-internal-lexer-input-invariants.md) | 强化 Parser 私有算法单元测试的双 Lexer 输入不变量（`done`） | 0006–0009、0093、0129、0130–0134 `done`；当前持续 Goal 的站立授权 |
| [0136](./0136-frontend-internal-error-determinism.md) | 强化 Lexer / Parser 内部边界错误的精确双运行确定性（`done`） | 0006–0011、0117–0120、0127、0129、0135 `done`；当前持续 Goal 的站立授权 |
| [0137](./0137-parser-invalid-lexeme-stream-matrix.md) | 建立 Parser 非法 Lexeme 流的六消费者精确拒绝矩阵（`done`） | 0006–0009、0065、0135–0136 `done`；当前持续 Goal 的站立授权 |
| [0138](./0138-parser-invalid-lexical-owner-matrix.md) | 建立 Parser 非法 lexical-owner 流的精确拒绝矩阵（`done`） | 0006–0009、0065、0135–0137 `done`；当前持续 Goal 的站立授权 |
| [0139](./0139-parser-invalid-recovery-diagnostic-matrix.md) | 建立 Parser recovery diagnostic/token 关联拒绝矩阵（`done`） | 0003、0006–0009、0065、0135–0138 `done`；当前持续 Goal 的站立授权 |
| [0140](./0140-parser-lexer-diagnostic-anchor-contract.md) | 锁定 Parser 的 Lexer diagnostic anchor 契约（`done`） | 0003、0006–0009、0065、0129、0135–0139 `done`；当前持续 Goal 的站立授权 |
| [0141](./0141-parser-lexer-diagnostic-stream-identity.md) | 锁定 Parser 的 Lexer diagnostic 流身份（`done`） | 0002–0003、0006–0009、0065、0129、0135–0140 `done`；当前持续 Goal 的站立授权 |
| [0142](./0142-parser-lexer-poison-diagnostic-coverage.md) | 锁定 Parser 的 lexical poison 诊断覆盖（`done`） | 0003、0006–0009、0065、0129、0135–0141 `done`；当前持续 Goal 的站立授权 |
| [0143](./0143-parser-lexer-diagnostic-anchor-uniqueness.md) | 锁定 Parser 的 Lexer diagnostic anchor 唯一性（`done`） | 0003、0006–0009、0065、0129、0135–0142 `done`；当前持续 Goal 的站立授权 |
| [0144](./0144-parser-suffix-truncation-matrices.md) | 建立完整文件与独立入口 UTF-8 后缀截断矩阵（`done`） | 0006–0009、0014、0068–0069、0079、0085、0099、0111、0114、0143 `done`；当前持续 Goal 的站立授权 |
| [0145](./0145-parser-interior-deletion-matrices.md) | 建立完整文件与独立入口 UTF-8 内部区间删除矩阵（`done`） | 0006–0009、0014、0068–0069、0079–0080、0085–0086、0099、0111、0114、0144 `done`；当前持续 Goal 的站立授权 |
| [0146](./0146-parser-scalar-duplication-matrices.md) | 建立完整文件与独立入口 UTF-8 scalar 重复矩阵（`done`） | 0006–0009、0014、0068–0069、0082、0087、0099、0111、0114、0145 `done`；当前持续 Goal 的站立授权 |
| [0147](./0147-parser-scalar-transposition-matrices.md) | 建立完整文件与独立入口 UTF-8 scalar 相邻交换矩阵（`done`） | 0006–0009、0014、0068–0069、0084、0090、0099、0111、0114、0146 `done`；当前持续 Goal 的站立授权 |
| [0148](./0148-parser-scalar-replacement-matrices.md) | 建立完整文件与独立入口 UTF-8 scalar 固定字母表替换矩阵（`done`） | 0006–0009、0014、0068–0069、0081、0088、0099、0111、0114、0147 `done`；当前持续 Goal 的站立授权 |
| [0149](./0149-parser-scalar-insertion-matrices.md) | 建立完整文件与独立入口 UTF-8 scalar 固定字母表插入矩阵（`done`） | 0006–0009、0014、0068–0069、0082–0083、0087、0089、0099、0111、0114、0148 `done`；当前持续 Goal 的站立授权 |
| [0150](./0150-lexer-large-input-mode-depth-stress.md) | 建立 Lexer 大输入与深模式压力矩阵（`done`） | 0006、0073、0103、0129、0149 `done`；当前持续 Goal 的站立授权 |
| [0151](./0151-parser-large-flat-recovery-stress.md) | 建立 Parser 四入口大平坦列表与恢复压力矩阵（`done`） | 0006–0009、0014、0117–0119、0128–0129、0135、0150 `done`；当前持续 Goal 的站立授权 |
| [0152](./0152-parser-recursion-budget-boundaries.md) | 锁定 Parser 四入口递归预算的精确公开边界（`done`） | 0007–0009、0014、0117–0119、0135–0136、0151 `done`；当前持续 Goal 的站立授权 |
| [0153](./0153-parser-caller-stack-isolation.md) | 锁定 Parser 四入口的调用者栈隔离（`done`） | 0007–0009、0014、0136、0152 `done`；当前持续 Goal 的站立授权 |
| [0154](./0154-lexer-small-stack-stress.md) | 锁定 Lexer 深模式的小调用栈行为（`done`） | 0006、0129、0150、0153 `done`；当前持续 Goal 的站立授权 |
| [0155](./0155-parser-owner-rich-stress.md) | 建立 Parser 四入口大规模 lexical-owner 压力矩阵（`done`） | 0006–0009、0014、0075、0095、0135、0151、0154 `done`；当前持续 Goal 的站立授权 |
| [0156](./0156-parser-string-poison-stress.md) | 压力验证 Lexer 字符串错误向四入口 Parser 的唯一传播（`done`） | 0006–0009、0014、0095、0140–0142、0155 `done`；当前持续 Goal 的站立授权 |
| [0157](./0157-parser-standalone-poison-matrices.md) | 扩展 Parser 变换矩阵至 L0007 / L0008 独立 lexical poison（`done`） | 0006–0009、0014、0081、0083、0088–0089、0111、0114、0142、0156 `done`；当前持续 Goal 的站立授权 |
| [0158](./0158-parser-standalone-poison-stress.md) | 建立 Parser 四入口 standalone lexical poison 压力矩阵（`done`） | 0006–0009、0014、0129、0140–0143、0150–0151、0157 `done`；当前持续 Goal 的站立授权 |
| [0159](./0159-parser-lexical-owner-recursion-boundaries.md) | 锁定 Parser 四入口 lexical-owner 递归预算边界（`done`） | 0006–0009、0014、0075、0095、0150、0152–0155、0158 `done`；当前持续 Goal 的站立授权 |
| [0160](./0160-lexer-long-invalid-lexeme-stress.md) | 建立 Lexer 超长非法 lexeme / terminal owner 压力矩阵（`done`） | 0006、0073、0103、0129、0150、0154、0157 `done`；当前持续 Goal 的站立授权 |
| [0161](./0161-parser-long-lexical-error-bridge.md) | 建立 Parser 四入口超长词法错误桥接矩阵（`done`） | 0006–0009、0014、0095、0140–0143、0158–0160 `done`；当前持续 Goal 的站立授权 |
| [0162](./0162-parser-mixed-long-lexical-error-stream.md) | 建立 Parser 四入口混合超长词法错误流矩阵（`done`） | 0006–0009、0014、0095、0140–0143、0155–0161 `done`；当前持续 Goal 的站立授权 |
| [0163](./0163-parser-mixed-long-recoverable-error-stream.md) | 建立 Parser 四入口混合超长可恢复词法错误流矩阵（`done`） | 0006–0009、0014、0075、0095、0140–0143、0155–0162 `done`；当前持续 Goal 的站立授权 |
| [0164](./0164-parser-long-utf8-line-recovery.md) | 建立 Parser 四入口超长 UTF-8 string owner 换行恢复矩阵（`done`） | 0002、0006–0009、0014、0078、0098、0100、0110、0112、0150、0160–0163 `done`；当前持续 Goal 的站立授权 |
| [0165](./0165-parser-long-utf8-nested-line-recovery.md) | 建立 Parser 四入口超长 UTF-8 nested string/interpolation 换行恢复矩阵（`done`） | 0002、0006–0009、0014、0078、0098、0100、0110、0112、0150、0155、0160–0164 `done`；当前持续 Goal 的站立授权 |
| [0166](./0166-parser-long-utf8-char-line-recovery.md) | 建立 Parser 四入口超长 UTF-8 invalid Char 换行恢复矩阵（`done`） | 0002、0006–0009、0014、0073、0095、0103、0150、0155、0157–0160、0163–0165 `done`；当前持续 Goal 的站立授权 |
| [0167](./0167-parser-long-invalid-number-boundaries.md) | 建立 Parser 四入口超长 L0008 maximal-region / operator-boundary 矩阵（`done`） | 0002、0006–0009、0014、0073、0095、0103、0150、0155、0157–0163、0165–0166 `done`；当前持续 Goal 的站立授权 |
| [0168](./0168-parser-long-block-comment-line-breaks.md) | 建立 Lexer/Parser 超长 UTF-8 block comment 非嵌套与逻辑换行矩阵（`done`） | 0002、0006–0009、0014、0073、0093、0103、0129、0150–0151、0160–0167 `done`；当前持续 Goal 的站立授权 |
| [0169](./0169-parser-long-line-comment-boundaries.md) | 建立 Lexer/Parser 超长 UTF-8 line comment / newline trivia 边界矩阵（`done`） | 0002、0006–0009、0014、0073、0093、0103、0129、0150–0151、0160–0168 `done`；当前持续 Goal 的站立授权 |
| [0170](./0170-parser-large-file-header-stress.md) | 建立 Parser 4,096 项合法 / 恢复 package-import 文件头压力矩阵（`done`） | 0002、0006、0014–0015、0093、0103、0128–0129、0150–0151、0168–0169 `done`；当前持续 Goal 的站立授权 |
| [0171](./0171-parser-large-qualified-header-paths.md) | 建立 Parser 4,096-segment package/import 路径与末尾恢复矩阵（`done`） | 0002、0006、0014–0015、0093、0103、0128–0129、0150–0151、0170 `done`；当前持续 Goal 的站立授权 |
| [0172](./0172-parser-large-file-header-separators.md) | 建立 Parser 4,096-import 混合文件头分隔与 L0053 恢复矩阵（`done`） | 0002、0006、0014–0015、0062、0078、0093、0103、0128–0129、0150–0151、0170–0171 `done`；当前持续 Goal 的站立授权 |
| 0060 | 提供版本化机器可读诊断协议 | 0003、0055；接受协议 ADR |
| 0061 | 构建首个支持平台的 compiler + stdlib 发行包 | 0040、0042–0051、0054；接受发布矩阵 ADR |

增量编译不预留在 Phase 0–6 主链中。它依赖稳定 package identity、package lock、SSA 和依赖
图；推荐在 SPEC-0054 完成后另建 Phase 6+ Spec，并先接受缓存键与失效策略 ADR。

现行 v0.25 沿用 v0.14 已确定的规则：v1 的 `Transferable` 与 `Copyable` 一样由编译器结构化自动推导，不开放
手动实现；标准库并发类型的例外由后续实施 Spec 逐项锁定，`Shareable` 连同跨线程共享原语
延后到 v2。这是已批准但尚待 Phase 3 实施的规则，不属于下列未决推荐。

## 未决决策的推荐方向

以下是起草后续 guide / ADR 时的默认推荐，不是已经接受的决策；触及对应 Spec 前仍需正式
文档批准。

| 决策 | 推荐方案 | 需要的权威文档 |
|---|---|---|
| 借用与析构 | v1 借用只存在于一次调用的动态期间，不允许存储或返回；ASAP 析构以所有权检查标出的最后一次合法使用为准，分支合流采用保守点 | 新 guide |
| `lang-std` bootstrap / runtime | `.ko` 标准库保持独立真源；最小 ABI 支撑先收敛在 codegen 的私有 runtime 边界，证明需要独立发布后再提新增 crate 的 ADR | ADR |
| SSA | 采用 typed SSA + block parameters，显式表达 move / drop；用 verifier 锁定类型、CFG 与所有权不变量 | ADR |
| LLVM / target / linker | 固定一组经兼容矩阵验证的 LLVM major 与 `inkwell` feature；先支持单一 host target，再扩展 CI 矩阵 | ADR |
| FFI | 不放入 v1 主交付路径；待内部 ABI 稳定后，以受限 C ABI 和显式 `unsafe` / 所有权边界起步 | 新 guide + ADR |
| `@Test` | v1 只定义编译器保留的最小 `@Test`，不顺带实现通用运行时注解或反射 | 新 guide |
| 机器诊断 | 人类可读诊断走 stderr；机器模式使用带 schema version 的 JSON Lines，稳定字段由后续协议 ADR 固定 | 后续 ADR |
| package / lock | `project.toml` 只保留 package、target、dependency 最小字段；`project.lock` 完全由工具生成并确定性排序 | ADR |
| 首发平台 | 先验收开发主机 `aarch64-apple-darwin`，再增加一个 Linux CI target；跨平台承诺以发行 ADR 为准 | ADR |

## 路线图维护规则

- guide 改变 Phase 或语言语义时，先更新 guide，再调整尚未批准的路线图候选。
- 已批准或已完成 Spec 不因路线图重排而改号；需要替代时使用 `superseded` 并建立双向链接。
- ADR 只决定 guide 留白处的长期方案；路线图中的“推荐”不能替代 accepted ADR。
- Architecture 只描述已经落地的事实，不复制本页计划。
