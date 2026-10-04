# Koven Specs

> **性质**：变更合同索引 · **状态**：current · **读取时机**：计划、实施或验收一项具体变更时 · **唯一真源**：各 Spec 正文；语言演进进度见演进实施账本

Spec 定义一次可独立验证的 Goal；路线图或 proposal 不会自动批准 Spec。
此前文档批次已按各合同最终证据关闭 0228–0235、0237–0242 与 0247；本批补齐0236诊断Span双宿主验收并归档，0182当时继续active；0248 的 Unit 容器存储已按双宿主证据完成归档；0249 普通 owned-unit 交接已按其有界合同与首轮双宿主证据归档；0250共享名称前缀首片已按PR31首轮双宿主证据归档；0251 LSP unit消费按PR32首轮双宿主证据有界归档；0252基础ownership共享推进与CLI/LSP消费按PR33首轮双宿主证据有界归档；0253单文件阶段门面按PR34最终精确head双宿主证据有界归档；0254 const封闭交接按PR35及真实merge主干双宿主证据有界归档，本地状态随下一相关实施批次发布。
文档归档不代表原 13 项语言演进全部完成，也不扩大已验收的支持范围。

本次本机状态与未覆盖项见[恢复验收](../development/recovery-local-delivery.md)；不声明远端CI或原P2成本验收完成。

## 当前入口

- [Active](active/README.md)：当前1份，0265承接M1A A4；0263/0264已归档，M1A总退出仍开放
- [演进实施账本](evolution-status.md)：13 项计划的当前实现、最终交付与独立缺口，唯一进度摘要
- [当前路线图](../development/roadmap.md)：从现状导航到下一批次；不复制第二张演进状态表
- [治理执行账本](../development/engineering-governance-progress.md)：获批整体架构与工程治理的批次基线与交付
- [Spec 依赖图](dependency-graph.md) / [SVG](dependency-graph.svg)：由 `scripts/gen_spec_dag.py` 生成的当前拓扑

## 版本与历史导航

- [v0.40](drafts/v0.40/README.md)：现行 Guide 启用及三项规则的实施入口
- [v0.36](drafts/v0.36/README.md)：常量 Phase 2/3/4 已完成
- [v0.37](drafts/v0.37/README.md)：0179/0211/0212 已归档；0182本机补强证据按需从archive追溯
- [完成 Spec Archive](../archive/specs/README.md)：251 份 `done`/`superseded`，只在追溯时读取
- [Proposals](../proposals/README.md)：尚未启用的候选，不因本批归档取得规范地位

## 生命周期

```text
draft → approved → in-progress → done
  │                                  └→ archive/specs/
  └─ blocked 时保持 draft
```

- 编号在 active、drafts 和 archive 间全局唯一且不复用
- 前置 Spec 必须 `done`、前置 ADR 必须 `accepted`、语义 Guide 必须已启用，才能进入 active
- `done` 前逐条记录验收及实际命令；未执行、失败、filtered、ignored 与后继修复分别留证
- 状态迁移、路径迁移、索引与冻结 inventory 同批更新；原验收历史不重写
- 一个 Spec 只定义一个 Goal；长期架构理由写 ADR，完整语义链接 Guide

新建草案使用 [TEMPLATE.md](TEMPLATE.md)，放入 `drafts/` 或对应未启用版本子目录。
