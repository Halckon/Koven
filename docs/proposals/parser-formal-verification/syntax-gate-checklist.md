# F5：新表面语法增量门禁模板、评审清单与候选演练

> **性质**：非规范治理草案 · **状态**：待评审，未写入正式提案模板或 required CI · **读取时机**：评估新增或修改 Koven 表面语法的提案进入 Guide 前 · **唯一真源**：本页维护增量门禁清单、升级门槛与真实候选演练账本

基点 `main@6cc03907a1bf37d3cdc9ce6e6ccba1a15326ed6b`，Guide v0.47。
本草案只适用于**新增或修改表面语法**的提案；不适用于纯语义、类型、所有权、codegen 或文案改动，
不减免既有 required 检查，不启用任何候选语义（包括 D1–D3）。

## 1. 提案增量门禁模板（拟并入表面语法提案）

任何拟进入 Guide 的新表面语法或句法修订，在评审前须补齐以下 6 项字段：

1. **受影响入口与规则 ID**：明确涉及 `parse_file` / `parse_expression` / `parse_declaration` / `parse_block` 中的哪些入口，以及对应 F3b/F3c/F3d/F3e 的规则 ID 与 Guide 章节。
2. **纯 CFG 与冲突变化**：给出改动前后的离线参考文法增量；若引入新的 shift/reduce、reduce/reduce 或 Earley 双树，必须逐条登记原始冲突来源，不得用优先级指令或词法吞换行静默掩盖。
3. **软关键字、词法与换行影响**：明确是否新增上下文关键字或修改 `NL` / CRLF / 块注释内换行 / `;` 的续行与分隔判定；若涉及外部谓词（如邻接 Span 或无副作用 trial），须写出明确谓词条件。
4. **恢复边界与资源合同影响（R1–R5）**：说明非法/未闭合输入是否改变 caller hard stop、soft declaration boundary、owner stack、诊断锚点或线性扫描/递归预算。
5. **正反例与边界见证矩阵**：至少包含正常接受、跨行/注释/分号边界、未闭合或错配回退、以及与既有相近语法的互斥对照；所有预期从拟议条文独立写出，不从实现回填。
6. **下游同步范围**：列明受影响的 Formatter 往返投影（F4c）、独立差分语料（F4a）与 Tree-sitter 漂移账本（F4b）；未同步项须显式登记为范围外或已知漂移。

## 2. 评审放行清单与 Advisory → Required 升级条件

### 评审放行清单（进入 Guide 前）

- [ ] 规则增量已回链唯一 Guide 章节，未在多章复制互相矛盾的允许条件。
- [ ] 冲突变化已由离线参考工具实测并保留原始报告；任何真正的多重推导（如换行后的附着 vs 独立元素）已在条文中规定唯一消解规则。
- [ ] 软词/换行/邻接谓词均未破坏既有普通 `Identifier` 或词法错误交接（R4）。
- [ ] 正反例覆盖跨行（LF/CRLF/含换行块注释）、分号阻断、未闭合回退与调用方边界。
- [ ] 未将离线参考工具或 Tree-sitter 提升为第二规范，未夸称有限用例为全语言证明。

### Advisory → Required 升级门槛

- 仅对**已获独立 Spec 批准、离线依赖通过治理审查、双宿主（macOS AArch64 与 Linux x86_64）干净 checkout 可复现、且连续观察窗内零环境误报**的稳定切片，才可另案批准由 advisory 升为 required CI。
- 升级不得删减、弱化或替代现有 `cargo test`、文档门禁或回归矩阵。

## 3. 真实候选演练：D1（已开始调用后缀后的跨行尾 lambda）

选取尚未启用的真实候选 [Guide 修订草案 D1](guide-amendment-draft.md#d1尾-lambda-的附着点) 演练上述清单
（本演练**不启用 D1**，仅验证门禁能否捕捉真实句法风险）：

| 检查维度 | D1 演练记录 |
|---|---|
| 受影响入口与规则 | `parse_block` 与 `parse_expression`；涉及 Guide 03（泛型试探提交）、04（`typed_call_suffix`）、06（block element 分隔）、07（尾 lambda 换行条款）；下游影响 F3d 与 F4b `TS-06` |
| 冲突变化实测（GNU Bison 2.3） | 在仓库外临时目录构建最小块文法（含 `ID`、`ID()`、同行 `ID() {}`、独立嵌套块 `{}` 与 `NL`/`;` 分隔）：**基线 0 conflicts**；加入 D1 规则 `ID '(' ')' NL nl_opt block` 后，Bison 立即报告 **`1 shift/reduce`**（前瞻 `NL`）。进一步分析表明，这不仅是 LALR(1) 局限，更是真实 CFG 歧义：`f()\n{}` 同时具备「一次带跨行尾 lambda 的调用」与「调用元素 + 换行分隔 + 独立嵌套块」两条推导 |
| 软词与换行影响 | 不新增软词；关键边界区分四类形状：`plain\n{}`（不附着，保持名称 + 嵌套块）、`plain()\n{}` / `typed<Int>\n{}`（优先附着为同一 Call，含 LF/CRLF/含换行块注释）、`plain();\n{}`（分号阻断，不附着）。必须靠显式条文规定「已开始调用后缀后优先附着，且不越过 `;` / caller boundary」来消解上述双推导 |
| 恢复边界影响（R1–R5） | 失败或未闭合 `<...>` 必须从 `<` 零副作用回退，不得残留 TypeRef 或消费外层 closer；不得越过 `;`、caller hard stop 或其他 owner 寻找 `{` |
| 正反例见证 | 复用 [guide-amendment-draft.md](guide-amendment-draft.md#kotlin-对照实跑门槛) 的 7 组 Kotlin 2.2.21 对照与 Koven 专属边界（`f();\n{}`、未闭合 `<...>`、caller boundary） |
| 演练结论 | 门禁成功捕获了该版最小块文法下 D1 的真实 `NL` 双推导冲突，表明不能把该规则未经消歧地直接并入块文法，必须在 Guide 正文与对应消歧规则／外部谓词中显式锁定唯一附着规则 |

- **后续演练**：OD4 约定的 L2/L3 候选语法前三项（`flows … into`、`view T`、`p.origins`；排除 `borrow var`）的完整 F5 演练与 Bison 2.3 冲突实测见 [F5 OD4 演练账本](f5-od4-l2-l3-syntax-gate-drill.md)。

## 4. 验证与未运行项

- **已运行**：macOS AArch64 下 GNU Bison 2.3 最小块文法基线（`0 conflicts`）与 D1 增量（`1 shift/reduce`）实测（临时目录已清理）；`python3 scripts/check_docs.py`。
- **未运行**：未修改仓库正式提案模板（`docs/proposals/README.md`、`docs/specs/TEMPLATE.md`）、Guide 或 CI；未启用 D1–D3；未运行 `cargo test` 或远端 CI。
