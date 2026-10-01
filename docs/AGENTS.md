# AGENTS.md — Koven 文档治理

> **性质**：文档工作规则 · **状态**：current · **读取时机**：修改 `docs/` 时 · **唯一真源**：本文件

根 [AGENTS.md](../AGENTS.md) 仍然适用；本文件只增加文档分类、生命周期与一致性规则。

## 1. 文档职责

| 区域 | 回答的问题 | 权威性 |
|---|---|---|
| `guide/` | 现行语言语义、强制 Phase 与实施边界 | 规范性；当前仅 v0.39 |
| `compiler-specs/` | 编译器内部表示、算法与资源约束是什么 | 工程合同；从属 Guide 的语言与 Phase 边界 |
| `architecture/` | 仓库现在已经实现成什么样 | 当前事实快照 |
| `development/` | 如何开发、验证与交付 | 工程规则 |
| `specs/` | 一次变更做什么、依赖与验收是什么 | 变更合同 |
| `adr/` | guide 留白处为什么选择某项长期方案 | 决策记录 |
| `proposals/` | 尚未启用的候选设计 | 非规范 |
| `archive/` | 旧规范、完成证据和冻结材料 | 只读历史 |

同一规则只保留一个真源。Guide 不写实现历史，Architecture 不写计划，Spec 不复制完整语义，
ADR 不替代 guide，AGENTS 不保存版本实施清单。Compiler Contracts 不以 current 状态声称
实现完成，不吸纳未启用语义；当前实现事实继续由代码、测试和 Architecture 证明。

## 2. Guide

- [guide/README.md](guide/README.md) 是唯一 current 入口，当前版本固定为 v0.39。
- 纯结构、链接或不改变含义的表述修正不提升版本；关键字、语法、类型、所有权、标准库契约或
  强制 Phase 边界变化必须形成新版本并由用户明确启用。
- 候选规则只能进入 `proposals/`；创建更高版本号或 draft Spec 不会自动取得规范地位。
- 规则正文优先于示例；发现冲突时保留原义、登记问题并请求决定，不能自行调和。

## 3. Compiler Contracts

- [compiler-specs/README.md](compiler-specs/README.md) 是唯一 current 入口，必须直接索引全部合同页。
- 仅迁移边界明确的工程段落；grammar、优先级、可观察诊断、语义示例与 Phase 留 Guide。
- 逐块记录旧章/段落、唯一新归属与原文保全证据；Guide 只保留最小指向，不复制已迁正文。
- 首片只建立渐进边界，不强行剥离仍与语义交织的段落，不将计划升级为现行语言规则。
- 入口最多 160 行、合同页最多 200 行；默认任务路由仍最多五份必读，适用相同链接、
  metadata、索引完整性与可达性检查。

## 4. Spec 与 ADR 生命周期

Spec 编号跨目录全局递增且唯一：

```text
draft              → specs/drafts/
approved/in-progress → specs/active/
done/superseded    → archive/specs/
```

- draft 的语义前置未启用时必须写明阻塞项，不得实施。
- Spec 完成前逐条记录实际验收；状态迁移与路径迁移在同一变更中完成。
- accepted ADR 放入 `adr/accepted/`，proposed 放入 `adr/proposed/`；rejected/superseded 进入 archive。
- accepted ADR 不改写原决定；改变方案时新增 ADR 并建立双向取代关系。
- 增删或迁移 Spec/ADR 时同步更新 `scripts/check_docs.py` 的冻结 inventory；否则结构门禁应失败。

## 5. Archive 与迁移

- 历史正文、结论和验收证据不得压缩或重写；允许机械更新本地链接和锚点。
- live 文档不得把 archive 当作默认必读项；需要追溯时从 `archive/README.md` 进入。
- 大规模重组先建立标题迁移账本，确保每个旧规则只有一个新归属；Git 历史不是遗漏正文的理由。
- 旧路径不留跳转页。所有仓库内引用必须在同一变更中更新。

## 6. 页面与交付

- live 的 guide、compiler-specs、architecture、development、proposal 和索引页顶部必须标明：性质、状态、读取
  时机、唯一真源。
- 默认任务路由最多列五份必读文档；扩展阅读和历史材料单独列出。
- 只描述实际运行过的检查；纯 Markdown 不机械运行 Rust 门禁。
- 提交前运行 `python3 scripts/check_docs.py`、检查迁移账本和 `git diff --check`。
