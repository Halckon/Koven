# Parser 分阶段形式化验证计划

> **性质**：非规范实施规划 · **状态**：未启用，待评审 · **读取时机**：启动 Parser 验证或拆分实施 Spec 时 · **唯一真源**：本页维护阶段、依赖与退出条件；语义仍以 Guide 为准

## 1. 范围与输入身份

本次只迁入材料并交付规划设计，不修改 Guide、Parser、测试或 CI，不启动生产实现。
所属语言阶段为 Phase 1（词法、解析与 SyntaxAst）；后续类型及所有权判断不进入句法接受标准。

| 输入 | 固定身份 |
|---|---|
| main 与 origin/main | `6cc03907a1bf37d3cdc9ce6e6ccba1a15326ed6b` |
| 来源分支 | `docs/evolution-architecture-review-2026-11-13` |
| 来源分支与远端最新提交 | `e00bc60bf2eec035634e005e93242745f5e333b3` |
| 工作分支 | `feature/spec-parser-formal-plan`，从上述 main 新建 |
| 语言规范 | [Guide v0.47](../guide/README.md) |

迁移账本：

| 来源分支路径 | 本工作树归属 | 保全方式 |
|---|---|---|
| `docs/Koven_Parser_Grammar_Implementation_Plan_2026-11-08.md` | `docs/archive/designs/parser-grammar-implementation-2026-11-08.md` | 原文字节保全，仅作历史依据 |
| `docs/proposals/parser-grammar-verification-review.md` | 同路径 | 正文保全；旧原文路径改为归档链接，未迁入的演进文档改为来源固定提交链接 |

迁入的[后续评审](parser-grammar-verification-review.md)是本计划比较基准。
不迁入演进分支的代码、Guide 或其他语义候选，不 merge/cherry-pick 整个分支。
旧文档中的提交数、开放项、行号与测试结果只是当时快照，必须由 F0 重核。

## 2. Technical Context

- 生产实现：Rust，手写递归下降 + Pratt，workspace 边界不变，不引入 LLVM。
- 输入：`SourceMap + LexedFile`；保留拼写、UTF-8 Span、trivia 换行、邻接、词法 owner 和入口。
- 入口：`parse_file`、`parse_expression`、`parse_declaration`、`parse_block`。
- 产物：`Result<Parsed*, ParserInternalError>`；`Ok` 可含恢复 AST 和错误诊断，不等于 Accept。
- 当前预算：32 MiB worker 栈、递归上限 1024；实现预算不是语言文法。
- 接口与模型设计见 [data-model](parser-formal-verification/data-model.md) 与
  [验证合同](parser-formal-verification/contracts/validation.md)，不改变公开 Rust API。
- 已有资产：Compiler Contracts、parser 普通/变异/压力套件、tree-sitter 文法、格式化器。
- 工具策略：首片仅用现有 Git/Python/Rust；文法工具在 F3a 做实测与依赖审计后选择。
  Lean/Coq/Kani/TLA+ 均未选定、未安装，不因名称获得证明等级。
- 性能目标：遵守现有扫描/恢复资源合同；F0 记录基线与测量选择，后续切片先确定可测阈值再实施。
- 规模边界：按一个结构家族或一个文法切片交付，不承诺日历工期或全语言 LR(1)。

## 3. Constitution Check

仓库没有 `.specify/`、`constitution.md`、plan 模板与 hooks 配置。
运行 `.specify/scripts/bash/setup-plan.sh --json` 因文件不存在失败；未取得 FEATURE_SPEC 等 JSON。
因此本次是按仓库规则适配的规划，不宣称标准 Spec Kit 脚本执行成功，不新增替代 constitution。
输入需求由本次用户请求及迁入的评审文档组成。研究结论见 [research](parser-formal-verification/research.md)。

| 门槛 | 设计前 | 设计后 |
|---|---|---|
| 保留 main 用户改动；独立工作树 | 已隔离 | 仅新工作树文档变更 |
| Guide 为语义唯一真源 | 遵守 | D1–D3 已裁决；Guide 修订草案未启用，受影响规则不先行实施 |
| Compiler Contracts 为内部合同归属 | 遵守 | 本目录仅候选接口，批准后再迁入正式合同 |
| workspace 无新增成员或依赖 | 遵守 | 无代码、依赖或配置变更 |
| 新能力先 Spec，长期边界变化先 ADR | 遵守 | 本次纯规划不分配编号、不批准 Spec；实施前按全局库存分配 |
| 先红测、定向验证、真实结果 | 遵守 | quickstart 区分现在可运行和未来命令 |
| 不将测试或有限模型夸称证明 | 遵守 | G1–G4 与证据范围逐片登记 |

阻塞项不会因计划写完而通过。D1–D3 属语义裁决，不是可由研究代理消除的技术未知。
若要求本轮直接交付完整 Rust Parser 机器证明，本计划不满足该目标，须另行确定精化与可信工具链范围。

## 4. 路线选择

采纳后续评审的 F0–F6 编号，但将输入/结果合同和 AST 投影前置，补足可执行验收。
恢复与合法语法是两条轨道。非法输入无合法文法推导，不能用无歧义证据代替恢复安全。
F2 是否启动由 F0 证据决定，不预先批准 owner 树重写。默认顺序执行，不预设并行开发。

```text
F0 基线/归因/验证输入 ──> F2a 恢复设计 ──> F2b–f 家族迁移（条件性）
F1 冲突裁决 ──> F3a 工具试验 ──> F3b 词法 ──> F3c 表达式
                                           └──> F3d 泛型/lambda ──> F3e 声明
F3c + 已裁决切片 ──> F4a 独立推导/AST ──> F4b 差分
现有完整文件正例 + 首片 AST 投影合同 ──> F4c Formatter 往返
F3e + 对应 F4 验收 ──> F5 增量门禁 ──> 首批新表面语法进入 Guide
稳定算法 + 明确命题 ──> F6 局部机器证明（可选）
```

F0、F1 无相互依赖，但建议先完成当前基线再准备裁决案例。F3 不必等 F2 全部迁移。
受某项 D 冲突影响的规则必须等该项批准；其他依赖闭合的切片可单独规划，不能宣称全语言完成。

## 5. 分阶段执行与退出条件

下表是后续 Spec 切分建议，不是已生成的 `tasks.md` 或实施授权。
每片实施前固定新 SHA、负责人、审阅人、文件清单、测试选择与成本上限。
F2b–f 每片都须对照固定基线核对受影响的既有正例与矩阵，只允许已登记且批准的变化。
每次迁移后复核边界机制是否简化，以及可比观察窗内的恢复返工；无结构收益或返工未减少则停下复盘，
证据不足时不声称收敛。v1 是否启动 F2 还要看 F0 实测的近期修复趋势，明显放缓时优先考虑暂缓。
过大的家族必须继续拆片，不把一行表格等同于一个编码会话。

| 阶段 | 单一交付物与范围 | 可验证退出条件 | 停止条件 |
|---|---|---|---|
| F0a | [当前基线与返工归因账本](parser-formal-verification/recovery-baseline.md) | 对固定历史范围内全部候选提交检查 diff，记录主因/次因、R1–R5、调用边、已有测试、区间化能否避免；排除项也列理由 | 不修代码；无法判断写不确定，不强作支持/不支持 |
| F0b | [输入/结果适配设计及现有回归映射](parser-formal-verification/observation-baseline.md) | 四入口的接受与拒绝见证来自既有预期，资源和内部失败缺口显式登记；R1–R5 映射到实际 suite；重核 SPEC-0392 旧开放项 | 不复制矩阵；尚未复现不能宣称当前失败或已修复 |
| F1 | [D1–D3 裁决包](parser-formal-verification/grammar-decisions.md) | 每项有冲突条文、入口/上下文、选定行为、正反例、版本影响和用户批准；受影响 Guide/测试在另行获准分支同步 | 无批准不得写目标 expected；跨行案例不能统一标拒绝 |
| F2a | owner 区间设计与内部原型 | F0 支持假设；同时验证合法、未配对/错配、插值及后继声明；caller 边界、合成闭合策略、复杂度有合同 | 反例表明区间不能表达现行边界时停止；不先改 LexedFile 公共形态 |
| F2b | 类头/父类型迁移 | 定向正例、恢复矩阵和资源检查无未批准变化；所迁路径不再依赖调用者硬停止组合 | 首家族没有结构收益则重新评审，不继续铺开 |
| F2c | 调用实参/分组迁移 | 同上，实参逗号和 caller 分号/closer 归属独立核对 | 与现行合同冲突即停 |
| F2d | 索引迁移 | 配对、未配对下标及外层逗号/分号有直接证据 | 不以增补特例替代区间合同 |
| F2e | block/lambda 迁移 | 换行与 body owner 不变，strict header trial 无副作用 | 不改变 D1–D3 目标或预算语义 |
| F2f | 字符串插值迁移 | 各模式 owner 不互取 closer，连续 poison/Error 可达性保持 | 不把尖括号当词法分隔符 |
| F3a | [表达式最小文法工具试验](parser-formal-verification/grammar-tool-probe.md) | 固定工具版本/模式，保留原始冲突与人为注入冲突的失败见证；依赖审计通过 | 候选工具无法报告谓词/原始冲突则不升级 required；不为工具改语言 |
| F3b | 词法机器描述；已核对[首片词法输入合同](parser-formal-verification/lexical-input-baseline.md) | token 类别、模式栈、最长匹配/覆盖/Span 有来源与独立性质案例 | 共用 lexer 的测试不算独立 lexer 证明 |
| F3c | 表达式文法及 EBNF 映射校验；[真实词法流首片](parser-formal-verification/token-stream-probe.md)与[18 级运算符层级切片](parser-formal-verification/expression-grammar-slice.md)已实跑 | 优先级、不结合、后缀及外部依赖有规则 ID；冲突逐个登记；故意漂移时校验失败 | 未覆盖项不计入 G1；消解后无冲突不算原 CFG 无歧义 |
| F3d | 泛型/lambda 切片；[原始冲突与外部谓词合同](parser-formal-verification/generic-lambda-grammar-slice.md)已登记 | typed-call 提交/回退、soft keyword、header、换行谓词闭合且规则已裁决 | 不通过删高风险构造达成“完整” |
| F3e | 声明与文件切片；[纯 CFG 与漂移校验](parser-formal-verification/declaration-file-grammar-slice.md)已实跑 | 函数、类族、声明头与分隔规则来源闭合，未覆盖清单明确 | 每个语法家族可另分 Spec |
| F4a | 生成推导 + NormalizedSyntax；[普通块差分原型](parser-formal-verification/independent-block-diff.md)已实跑 | 运行所选切片的全部生成正例。预算充分时生产入口须给出 Accept，规范化 AST 与独立推导一致，且消费边界正确；任一不符则该切片未通过。投影能检出身份变化并忽略绝对位置平移 | 正例只覆盖完备性样例，不能据此宣布 soundness；另设规范反例 |
| F4b | tree-sitter 差分报告；已核对[共同子集与已知漂移基线](parser-formal-verification/tree-sitter-diff-baseline.md) | 在双方支持范围核接受性/结构，差异归因明确、结果可复验 | 不是第二份规范；未支持/未解释差异不算通过 |
| F4c | [Formatter 往返 DiffTest 首片](parser-formal-verification/difftest.md) | 已选完整文件两臂均接受、规范化 AST 相等、Span 独立验证且比较器通过正负控 | 首片不依赖 F3 或 F4a；往返同错不能视为正确性证明 |
| F5 | 新语法增量门禁写入提案模板及评审清单；[模板草案与 D1 演练](parser-formal-verification/syntax-gate-checklist.md)已完成 | 在新表面语法进入 Guide 前，以真实候选演练规则增量、冲突变化、软词/换行影响及正反例；CI 稳定切片单独获批后由 advisory 升 required | 只适用于新增或修改表面语法的提案；不减免既有 required 检查，不启用候选语义 |
| F6 | 一个稳定算法的机器证明；已整理[前置设计与首片选型](parser-formal-verification/f6-machine-proof-design.md) | 明确输入域、预算、命题、可信依赖及精确 Rust 对应关系；源码变化使旧证据失效 | 仅模型或有界结果不能升级为全输入生产 G4 |

## 6. 第一批建议执行包

先开展 [F4c.0 格式化基线与首片准备](parser-formal-verification/difftest.md#3-顺序任务与检查点)。
它不替代下列主路线工作，也不阻塞 F0/F1：

1. F0a：锁定历史起止 SHA，逐项归因并给 owner 区间假设结论。
2. F0b：四入口结果分类表、已有矩阵/R1–R5 映射及历史开放项重核。
3. F1：向用户提交三项裁决，不预选答案；批准后另起 Spec 同步 Guide 与红绿测试。
4. 检查点：用户接受基线、证据等级、F2 启动/暂缓决定与 F3a 工具试验范围。

不预先开约十七个 active Spec。按依赖逐个分配全局编号，在 `main` 新建
`feature/spec-<id>` 或 `fix/spec-<id>` 分支；后续 main 已变化时重新核对基线。
每片交付遵循 PR/CI/验收账本/归档闭环，不自动提交或合并。

## 7. 风险、证据与验收

- 最大风险：把候选接口写成现行合同，或把测试通过称作生产证明。所有设计保持未启用。
- 原文偏向 Guide 单向生成、独立参考 parser；后续评审偏向手写投影和组合 oracle。
  采用后者以限制成本；保留来源映射、自然语言谓词审阅和独立反例，避免第二规范。
- 机器文法属于工程投影，Guide 不被替代；若要提升机器规则权威，必须另行决策。
- 技术选型是后续 F3a 的交付，不是本轮未完成研究的工具承诺。
- 证据须区分 tested、validated、proved，并带具体范围；预算充分前提只用于完备性。
- 本次文档验收与未来执行命令见 [quickstart](parser-formal-verification/quickstart.md)。
- F0a/F0b 的归因、观察映射及本机定向结果见各自账本。F1 已记录 D1–D3 裁决与未启用的 Guide 修订草案。F3a–F3e（含词法流、18 级表达式、泛型/lambda 原始冲突与谓词、声明/文件切片）及 F4a 原型/候选 Spec/Lark 审查已归档；F4b 已完成静态共同子集与 7 类漂移核对（尚未实跑 Tree-sitter 二进制差分）；F5 已完成增量门禁草案与 D1 冲突演练；未选定门禁工具，未运行机器证明。

本计划优于旧评审的具体标准：保留 F0–F6 与止损条件，增加当前输入身份、可落地的数据合同、
每阶段可验证退出条件、可运行验证指南和显式实施阻塞。须由独立上下文逐项检查，不以作者自评替代。
