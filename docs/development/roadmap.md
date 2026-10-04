# Koven 当前路线图

> **性质**：实施导航 · **状态**：current · **读取时机**：选择下一项工作或判断现状与目标的差距时 · **唯一真源**：本页仅导航；进度见演进实施账本与治理执行账本

## 当前路线与下一批次

1. 语言规则与强制 Phase 以 [Guide v0.40](../guide/README.md) 为准
2. 原 13 项语言演进的已实现、延期、未启用与 native 边界，以[演进实施账本](../specs/evolution-status.md)为唯一摘要
3. [Active Specs](../specs/active/README.md)给出当前有界合同；0271补强tour组合及诊断边界，归档不表示全部语言能力完成
4. [整体架构与工程治理计划](engineering-governance-plan.md)保留获批 P0–P5 范围；实际交付及延期见[治理执行账本](engineering-governance-progress.md)
5. [后继里程碑](post-governance-milestones.md)维护 M0–M6 候选顺序；正式实施及完成以各 Spec 为准

文档分支创建基线为main `adb51d6`；合入时已接收main `a83749f`的PR46交付（2026-10-04）。
[本批核对记录](documentation-status-sync.md)固定依据、旧 Spec 处理与未接收的并行工作。

0182 已按有界集成证据归档；P2 的测试/生产责任迁移与尺寸护栏、普通/const封闭交接、
P3b四宿主纯编排、P4已证实相同的中立helper/查询均已有交付。
P2性能、噪声与预算接受依用户决定延期，暂停检查；历史尺寸欠账和全量single/unit合并
不作为必须追加的结项任务。工程交付不等于原治理计划全部性能验收通过。

P5当前教程与提取门禁随PR38交付，PR46再增加parameter-report：12正例、2诊断负例，
四组argv共用该三文件源码，共17组执行合同；两宿主实际通过，1 planned不执行。
独立 [0271](../specs/active/0271-tour-combination-coverage.md) 增加5项Mac合同及一个保留失败的planned：
当前15正例、4负例、2 planned，共22执行合同；本批Linux/远端CI未运行，不将旧证据扩大到新增例。
旧编辑器corpus五失败已由0267/PR45修复，真实CLI门禁已进入required汇总。

当前功能主线为[M1A多文件程序](multifile-program-spec-draft.md)：0263字段可变性、0264直接
字段Borrow、0265unit迭代前端事实完成A2–A4，0268/PR46已完成A5–A10及完整程序
双宿主有界验收。Inout/field/captured Borrow源native边界不扩大；未合入main的后继
工作不计交付，不在本轮文档合并推进功能。
0266/PR44已交付Linux ASan/LSan有界接线；macOS动态检测、Koven IR UBSan及M4其余范围
不据此关闭。其后按程序需求选择M1B/M3A等，不重复启动已经完成的仓库审计。
用户说明审计已由其他工作完成；本次未取得完整审计交付正文，不独立认证其全部整改。

## 当前入口与历史界限

当前实现与支持边界见 [Architecture](../architecture/README.md)，使用/构建从仓库
[中文 README](../../README_CN.md)开始。已交付的[当前教程](../tutorials/README.md)以Markdown为源码真源，示例状态与CLI合同由门禁核验。

旧 `06-roadmap.md` 保留在 v0.34 历史 Guide，旧 tour 保留 v0.28 内容；通过
[Archive](../archive/README.md)追溯，不作当前开发或复制运行的默认入口。
旧 roadmap 在历史中曾混入后继状态更新，本批不抹去这段历史，也不继续更新其冻结正文。

## 交付边界

每批从最新 main 建分支，按影响面本地验证后发 Draft PR；最终 exact-head CI 区分实际运行、
合法 docs-only skip 和未运行项。保留用户合并决定；不把 Spec 数量减少等同于语言全部完成。
