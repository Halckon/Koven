# Parser 验证规划研究

> **性质**：非规范设计研究 · **状态**：未启用 · **读取时机**：审阅阶段取舍与技术依据时 · **唯一真源**：本页维护研究决定与替代方案

## 研究范围

研究输入固定于[主计划](../parser-formal-verification-plan.md#1-范围与输入身份)。
本轮读取了公开入口、产物、内部错误、直接消费者、共享测试 helper 和现行合同；
独立只读研究核对了 D1–D3 与旧基线漂移。未运行 Rust 测试或证明工具。

## 决定与替代方案

### 1. 恢复安全和合法语法分轨

- Decision：保留 F0–F6。先归因，再决定是否启动 F2；F3 不依赖全部恢复迁移结束。
- Rationale：无合法推导的错误输入不能由文法无歧义证据覆盖。旧提交标题只能提出假设。
- Alternatives considered：整体 Parser 证明先行；立即实施 owner 树。二者均缺当前归因与稳定模型。

### 2. 机器文法是带来源的手写工程投影

- Decision：以规则 ID 回链 Guide，校验已有 EBNF 对应片段；散文谓词单独审阅。
- Rationale：来源评审指出换行、邻接、软词与试探条件大量存在于散文，不能声称完整自动提取。
- Alternatives considered：原文的 Guide 单向生成。保留可生成片段的一致性检查，但不建立未经审阅的全自动语义提取器。
- 状态：这是本计划的候选选择，不是对现行 Compiler Contracts 的修改或维护者批准。

### 3. 组合 oracle，不先写第二个手写 Parser

- Decision：生成推导/独立金标准、规范反例、tree-sitter 支持范围差分、打印/格式化往返。
- Rationale：降低双 Parser 维护成本；不同 oracle 各保留盲区与共同依赖。
- Alternatives considered：直接采用现有 Parser 输出作 expected；重写独立参考 Parser。前者循环论证，后者暂不具备成本收益证据。

### 4. 不为验证先改变生产 API

- Decision：四分类是测试观察层合同，保留现有 Result API。
- Rationale：四入口 Ok 均可携带诊断。格式化器拒绝诊断，名称分析保留 recovery snapshot，不能统一改成布尔接受。
- Alternatives considered：公开新增 ParseOutcome 枚举。当前需求不要求这一共享 API 变更。
- 核对路径：`crates/lang-frontend/src/parser/{mod,output,error}.rs`、
  `crates/lang-frontend/src/formatting.rs`、
  `crates/lang-frontend/src/analysis/unit_names.rs`、
  `crates/lang-cli/src/bootstrap.rs`。

### 5. 工具选择由最小试验决定

- Decision：F3a 比较工具能否表示首片谓词、输出原始冲突、固定算法模式、确定性复现并满足依赖治理。
- Rationale：尚无实际机器文法，预选 canonical LR(1)、PEG 或具体库会倒置设计。
- Alternatives considered：直接引入 LALRPOP/ANTLR/Lean。均不作为本轮依赖或承诺。
- 退出条件：给出一份可复现实验及采纳/拒绝理由；失败则保留 tested 级检查，不伪造 G2。
- F3a 冒烟：macOS 上已有的 GNU Bison 2.3（默认 LALR(1)）可把未预期的原始冲突设为失败。
  在仓库外创建两个 `.y`，均用 `%token IDENT MINUS`、`%expect 0` 和 `%%` 文法分隔；
  正对照用 `expr: IDENT | expr MINUS IDENT ;`，负对照只把末尾 `IDENT` 换成 `expr`。
  分别运行 `bison -v -o <输出.c> <输入.y>`：正对照退出 0；负对照退出 1，stderr 报
  1 个 shift/reduce、期望 0；状态报告定位到 State 5。未引入项目依赖或 CI。
- 后续[换行与绑定试验](grammar-tool-probe.md)保留原始 `NL`，通过减号对照，并暴露成员访问的绑定反例
  和单 token 前瞻边界。工具适配和依赖审计未通过前不选定工具。

## 当前事实与历史失效

| 项目 | 本轮结论 |
|---|---|
| 四公开入口、32 MiB 栈、1024 深度 | 当前源码仍存在；不是文法界限 |
| 顶层 matrix 测试 | 只读计数为 39；数量不等于覆盖或执行通过 |
| 旧评审基线至 main | 限 parser/guide/tree-sitter：11 文件 +546/-101；不能复用旧行号或验收 |
| 已有资源合同 | header 共享 O(n) 预索引，恢复 O(k)/O(d)，block dispatch 线性；合同不等于证明 |
| D1–D3 | 冲突条文仍存在；trial 提交仍接受 LeftParen/LeftBrace，不能据实现裁决 |
| 旧 33/31 修复统计与 SPEC-0392 开放项 | 本轮未重做归因或复现，不能声称仍成立 |

## 尚待批准而非技术研究可消除的事项

1. D1：泛型无圆括号尾 lambda 的产生式、同行/换行提交及回退。
2. D2：lambda 内 return 的 Phase 1 允许范围与诊断段冲突。
3. D3：非 lambda 位置 move 的上下文关键字与 expected-expression 条文范围。
4. 本计划采用的手写投影/组合 oracle 取舍；是否将可选 F6 升为必交目标。

这些事项阻塞受影响的实施，不阻塞提交候选设计。未经用户批准不写目标金标准、不升 Guide 版本。
不存在由本轮静默填入的目标语义。
