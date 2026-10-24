# Koven v1 Roadmap

> **性质**：v1 范围、优先级与进度总账 · **状态**：current · **读取时机**：需要统一查看 v1 已完成内容、当前位置与待办时 · **唯一真源**：[v1.json](v1.json) 是 v1 范围、优先级与状态的唯一真源；语义仍归 Guide，验收证据仍归各 Spec

本目录回答“v1 要做哪些事、做到哪了、下一步做什么”。它不定义语言语义，也不保存验收证据。
每个条目只写一句内容和状态，再链接到 Guide、Spec 或 proposal。

## 视图

| 视图 | 用途 |
|---|---|
| [按实施阶段](by-stage.md) | 已交付基线 → P0 止血 → P1 结构 → P2 补齐 → P3 语言增量 → P4 文本与 IO → P5 发布；附决策表与最近实测 |
| [按领域](by-area.md) | 类型、控制流、所有权、集合、文本 IO 等各自的完成度 |
| [按编译 Phase](by-phase.md) | 条目 × Phase 0–6 矩阵，看每个编译阶段承担的工作 |

三份视图由 `scripts/gen_roadmap.py` 从 `v1.json` 生成，不要手改；`check_docs.py` 会校验
数据合法且视图未过期。

## 与其他文档的分工

| 问题 | 真源 |
|---|---|
| 某项是否属于 v1、优先级、当前状态 | 本目录 `v1.json` |
| 语言规则是什么 | [Guide](../guide/README.md) |
| 一次变更的范围与验收证据 | [Specs](../specs/README.md) |
| 代码当前怎么实现 | [Architecture](../architecture/README.md) |
| 未启用的候选设计 | [Proposals](../proposals/README.md) |

条目状态只做摘要：`done` 表示对应 Spec 已有界交付，不表示覆盖全部组合；精确范围与保留边界
始终以 Spec 为准。v1 的范围来自 2026-10-13 的 v1 范围评审；评审中的待决事项以“决策”形式
记在 `v1.json` 的 `decisions` 中，2026-10-22 已决部分的规则文本见
[v0.46 待启用规则](../proposals/v0.46-language-decisions.md)。

## 维护规则

更新 roadmap 只改 `v1.json`，然后运行：

```bash
python3 scripts/gen_roadmap.py
python3 scripts/check_docs.py
```

需要更新的时机：

- 一个 Spec 归档时：把对应条目的 `status` 改为 `done` 或 `partial`，并在 `specs` 中追加编号。
- 新开 Spec 时：在 `specs` 中登记编号，状态改为 `active`。
- 用户作出决策时：修改 `decisions` 中对应项的 `status` 与 `outcome`，并相应调整条目状态。
- 增量审计重跑 probe 矩阵时：更新 `meta.metrics`。
- v1 范围变化时：增删条目或调整 `priority`；移出 v1 的条目改为 `stage: "X"`。

字段取值：

| 字段 | 取值 |
|---|---|
| `stage` | `S0` 已交付基线、`P0`–`P5`、`X` v1 之外 |
| `priority` | `must` v1 必做、`should` v1 建议、`v1.x`、`v2`、`v3` |
| `status` | `done`、`partial`、`active`、`todo`、`decision`、`deferred`（仅 v1 之外） |
| `guide` | `enabled` Guide 已定义、`needed` 需新增规则、`none` 纯工程 |
| `phases` | 涉及的编译 Phase 编号 0–6 |
| `specs` / `links` | Spec 四位编号；相对 `docs/` 的文档路径 |
