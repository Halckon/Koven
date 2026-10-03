# Koven 当前路线图

> **性质**：实施导航 · **状态**：current · **读取时机**：选择下一项工作或判断现状与目标的差距时 · **唯一真源**：本页仅导航；进度见演进实施账本与治理执行账本

## 当前路线与下一批次

1. 语言规则与强制 Phase 以 [Guide v0.40](../guide/README.md) 为准
2. 原 13 项语言演进的已实现、延期、未启用与 native 边界，以[演进实施账本](../specs/evolution-status.md)为唯一摘要
3. [Active Specs](../specs/active/README.md)给出当前有界合同；0182 的集成验收继续独立推进；0236诊断Span补强经双宿主验收后归档
4. [整体架构与工程治理计划](engineering-governance-plan.md)给出获批 P0–P5 顺序、允许依赖与回退边界
5. [治理执行账本](engineering-governance-progress.md)记录每批真实进度与下一门禁；没有实施/验收的目标不称当前能力

P1a 文档闭环与 P2 LSP 私有测试首片已分别由 PR15/16 合并；0236 Span 精确证据由 PR17
补齐并归档，0182继续active；[0249普通交接](../archive/specs/0249-owned-unit-borrowed-handoff.md)已按有界合同与PR30首轮双宿主证据归档。[尺寸护栏](rust-size-policy.md)已由PR18合并；P2继续
已合并的[receiver拆分](codegen-receiver-test-migration.md)与[plan七域拆分](codegen-plan-test-migration.md)之后，
推进[iteration私有测试18域拆分](drop-iteration-test-migration.md)；
其余领域、真正冷缓存和分离link性能验收仍待后继，
P3a 普通 unit 交接本片已完成；0250已共享CLI project名称前缀，
[0251](../archive/specs/0251-lsp-unit-name-snapshot.md)已完成有界LSP unit消费；0252已共享unit基础ownership推进，
[0253](../archive/specs/0253-single-file-analysis-facade.md)的bootstrap/legacy单文件门面经最终双宿主验收，
P3b四宿主纯编排有界完成；[0254](../archive/specs/0254-const-owned-unit-borrowed-handoff.md)
const owned交接与[P2 runtime layout首片](unit-runtime-layout-migration.md)已随PR35合并，
PR与真实merge主干双宿主有界验收通过，0254本地归档随下一相关实施批次发布。
P4 仅在语义/recovery parity 成立时逐域收敛。
P5 新教程依赖稳定受测示例，可独立于 P4 推进。这里不维护另一张功能状态表。

## 当前入口与历史界限

当前实现与支持边界见 [Architecture](../architecture/README.md)，使用/构建从仓库
[中文 README](../../README_CN.md)开始。新的 current tour 与示例门禁属于 P5，尚未交付。

旧 `06-roadmap.md` 保留在 v0.34 历史 Guide，旧 tour 保留 v0.28 内容；通过
[Archive](../archive/README.md)追溯，不作当前开发或复制运行的默认入口。
旧 roadmap 在历史中曾混入后继状态更新，本批不抹去这段历史，也不继续更新其冻结正文。

## 交付边界

每批从最新 main 建分支，按影响面本地验证后发 Draft PR；最终 exact-head CI 区分实际运行、
合法 docs-only skip 和未运行项。保留用户合并决定；不把 Spec 数量减少等同于语言全部完成。
