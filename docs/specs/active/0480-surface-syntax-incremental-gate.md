# SPEC-0480: 新表面语法增量门禁接入提案与 Spec 模板（F5）

> **性质**：变更合同 · **状态**：in-progress · **读取时机**：评估新增或修改表面语法的提案与 Spec 门禁时 · **唯一真源**：本 Spec 的范围、边界与验收；语言语义仍归 Guide

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P1-0480` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [Guide v0.47](../../guide/README.md) |
| 批准依据 | 2026-11-19 维护者确认按 Parser 形式化验证依赖图推进 F5 正式入库 |
| 前置 Spec | 无 |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 待发 PR 触发远端文档门禁 CI 后归档 |
| 影响范围 | `docs/proposals/README.md`、`docs/specs/TEMPLATE.md`、`docs/specs/inventory.md`、`docs/specs/README.md`、`docs/specs/active/README.md` |
| 语言语义变更 | 否 |

## 1. Goal

将 Parser 形式化验证计划 F5 的「新表面语法增量门禁」正式写入提案入口 `docs/proposals/README.md` 与 Spec 模板 `docs/specs/TEMPLATE.md`，使后续新增或修改表面语法的提案在进入 Guide 前具备统一的冲突、换行/软词、恢复边界与正反例评审门槛。

## 2. 范围与需求

- 在 `docs/proposals/README.md` 增加「表面语法提案增量门禁」章节，明确 6 项必填增量字段、5 项进入 Guide 前评审放行清单，以及 advisory 升级 required CI 的前提条件。
- 在 `docs/specs/TEMPLATE.md` 的「验收标准」中补充可选的表面语法增量门禁核对项（仅适用于新增或修改表面语法的 Spec，不适用时标注 `N/A`）。
- 明确适用边界：只约束新增或修改表面语法的提案与 Spec；不减免既有 required 检查，不启用任何候选语义，不修改生产代码或 CI 配置。

## 3. 演练与验证记录

| 检查项 | 命令 / 依据 | 结果 |
|---|---|---|
| 真实候选 D1 冲突演练 | GNU Bison 2.3 最小块文法基线（`0 conflicts`）加入 D1 规则 `ID '(' ')' NL nl_opt block` 后立即报告 `1 shift/reduce`（前瞻 `NL`），捕获 `f()\n{}` 的真实双推导冲突 | 已于规划阶段实测验证 |
| Spec DAG 同步 | `python3 scripts/gen_spec_dag.py` | 退出 0 |
| 文档结构门禁 | `python3 scripts/check_docs.py` | 退出 0 |
| 空白与格式检查 | `git diff --check` | 退出 0 |
