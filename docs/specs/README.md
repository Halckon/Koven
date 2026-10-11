# Koven Specs

> **性质**：变更合同索引 · **状态**：current · **读取时机**：计划、实施或验收一项具体变更时 · **唯一真源**：各 Spec 正文；语言演进进度见演进实施账本

Spec 定义一次可独立验证的 Goal；路线图或 proposal 不会自动批准 Spec。
此前文档批次已按各合同最终证据关闭 0228–0235、0237–0242 与 0247；本批补齐0236诊断Span双宿主验收并归档，0182当时继续active；0248 的 Unit 容器存储已按双宿主证据完成归档；0249 普通 owned-unit 交接已按其有界合同与首轮双宿主证据归档；0250共享名称前缀首片已按PR31首轮双宿主证据归档；0251 LSP unit消费按PR32首轮双宿主证据有界归档；0252基础ownership共享推进与CLI/LSP消费按PR33首轮双宿主证据有界归档；0253单文件阶段门面按PR34最终精确head双宿主证据有界归档；0254 const封闭交接按PR35及真实merge主干双宿主证据有界归档，本地状态随下一相关实施批次发布。
文档归档不代表原 13 项语言演进全部完成，也不扩大已验收的支持范围。

以上是各次归档的历史顺序。文档合入接收main `a83749f`，已有0 active / 255 archive；0182已归档，
0262教程经PR38双宿主验收，0263–0265闭合M1A A2–A4，0266/0267交付有界检测/编辑器门禁，
0268/PR46闭合unit native迭代及M1A有界总验收。
具体状态及精确head依据见[本批核对](../development/documentation-status-sync.md#合入main时接收后继0268)；
P2成本继续延期，后续候选能力不因M1A交付自动关闭。原[恢复验收](../development/recovery-local-delivery.md)保留当时本机结果。

## 当前入口

- [Active](active/README.md)：当前1份，[SPEC-0289](active/0289-n1a-range-carrier.md) 延续 N1a 有界里程碑；0290 已按 PR71 实现 head 双宿主 CI 有界归档，最终归档 head/merge/main 待验证，见[交付账本](../development/evidence/pr69-repair-0290/delivery.json)；N1a 已批准合同经新推送分支补回，当前 Guide 为 v0.43；实现验收保持独立。历史交付：0293 已按 [PR #70](https://github.com/Halckon/Koven/pull/70) 完整 CI / 文档复用 / 输入改变回退验收归档并合并为 `56af80b`；0279按[PR57](https://github.com/Halckon/Koven/pull/57)双宿主限定范围归档，最终归档head/merge/main待交付，见[账本](../development/evidence/runtime-constructor-0279/delivery.json)；0278按[PR56](https://github.com/Halckon/Koven/pull/56)双宿主实现验收归档，最终归档head、merge及actual main CI已闭环，见[交付账本](../development/evidence/closure-escape-0278/delivery.json)；0269 的 M4b 首片已验收归档，实现证据见[账本](../development/evidence/generated-owners-0269-delivery.json)，最终归档/merge/main 见[PR48](https://github.com/Halckon/Koven/pull/48)；0277按[PR55](https://github.com/Halckon/Koven/pull/55)双宿主实现验收归档，最终归档head及actual main已闭环，见[交付账本](../development/evidence/p2-linux-0277-delivery.json)；0276按[PR54](https://github.com/Halckon/Koven/pull/54)归档并合并，最终head及actual main CI闭环见[交付证据](../development/evidence/generic-body-0276-delivery.json)；0275按[PR53](https://github.com/Halckon/Koven/pull/53)归档并合并，最终归档CI及main CI已闭环，见[交付证据](../development/evidence/generic-containers-0275-delivery.json)；0274按[PR52](https://github.com/Halckon/Koven/pull/52)双宿主实现验收归档；0272按[PR51](https://github.com/Halckon/Koven/pull/51)双宿主实现验收归档；0273已按[PR50](https://github.com/Halckon/Koven/pull/50)双宿主实现证据归档；0271已按[PR49](https://github.com/Halckon/Koven/pull/49)双宿主实现证据归档；0270双宿主归档见[PR47](https://github.com/Halckon/Koven/pull/47)；0263–0268及M1A双宿主交付见[PR46](https://github.com/Halckon/Koven/pull/46)
- [演进实施账本](evolution-status.md)：13 项计划的当前实现、最终交付与独立缺口，唯一进度摘要
- [当前路线图](../development/roadmap.md)：从现状导航到下一批次；不复制第二张演进状态表
- [治理执行账本](../development/engineering-governance-progress.md)：获批整体架构与工程治理的批次基线与交付
- [Spec 依赖图](dependency-graph.md) / [SVG](dependency-graph.svg)：由 `scripts/gen_spec_dag.py` 生成的当前拓扑

## 版本与历史导航

- [v0.43 首片准备](drafts/v0.43/README.md)：0289 已迁 active；0292 为未启用 consume 草案
- [v0.44 候选](drafts/v0.44/README.md)：0291 只读 Map 消费式转换保持 draft

- [v0.40](drafts/v0.40/README.md)：v0.40 已继承的三项规则实施入口；现行 v0.41 的 move literal 澄清由 SPEC-0279 验收
- [v0.36](drafts/v0.36/README.md)：常量 Phase 2/3/4 已完成
- [v0.37](drafts/v0.37/README.md)：0179/0211/0212 已归档；0182本机补强证据按需从archive追溯
- [完成 Spec Archive](../archive/specs/README.md)：277 份 `done`/`superseded`，只在追溯时读取
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
