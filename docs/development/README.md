# Koven 开发指南

> **性质**：工程流程入口 · **状态**：current · **读取时机**：准备修改、验证或交付时 · **唯一真源**：本索引导航的页面

按变更影响面读取，不需要顺序加载全部工程规则。

## 按任务读取

| 任务 | 页面 |
|---|---|
| 确定 Phase、Spec/ADR 门禁和交付顺序 | [工作流与 Phase](workflow-and-phases.md) |
| 编写或拆分 Rust 模块、处理错误和日志 | [Rust 与模块](rust-and-modules.md) |
| 选择窄测、提交门禁或全量门禁 | [测试与分层验收](testing.md) |
| 新增/修改诊断、错误码、Span、机器输出 | [诊断规范](diagnostics.md) |
| 复用开源方案或新增/升级依赖 | [依赖治理](dependencies.md) |

## 有界迁移验收

- [手写 Rust 尺寸护栏](rust-size-policy.md)：真实base增长比较、历史baseline、例外与生成物登记
- [P2 LSP server 测试首片](lsp-test-migration.md)：私有边界、身份映射、实际验证及未测项
- [P2 codegen receiver 测试拆分](codegen-receiver-test-migration.md)：46项逐字保全、领域映射、定向验证与有限warm样本
- [P2 codegen plan 测试拆分](codegen-plan-test-migration.md)：48项逐字保全、七域映射及受控compile/link与运行样本
- [P2 unit runtime layout 职责拆分](unit-runtime-layout-migration.md)：八函数等价移动、旧门面路径、完整codegen与尺寸收紧证据
- [P2 iteration 私有测试拆分](drop-iteration-test-migration.md)：40项映射、18域、逐项格式/路径证据及3个完整场景例外

- [P2 ownership iteration integration分组](ownership-iteration-test-migration.md)：184项逐字保全、20域、同target及双平台定向接线

- [P2 multifile type integration分组](multifile-type-test-migration.md)：107项逐字保全、17新域、source-qualified facts与原CI覆盖
- [P2 multifile ownership integration分组](multifile-ownership-test-migration.md)：72项与8 helpers逐字保全、12域、原子失败/Span与单target证据

- [P3a 普通 unit 交接合同基线](unit-handoff-contract-baseline.md)：六输入独立拒绝、合法clone/重排与隔离reserve计数
- [SPEC-0250 名称前缀](../archive/specs/0250-unit-name-snapshot.md)：CLI project 首迁、同源全事实 parity 与封闭 owner 合同
- [SPEC-0249 交接测量](owned-unit-handoff-measurement.md)：动态 native/lower/factory 次数、完整同协议样本与噪声边界
- [const交接动态index计数](const-owned-unit-handoff-measurement.md)：SPEC-0254两固定输入的真实前后计数与产物保全

具体 crate 的职责和最近测试入口由 `crates/**/AGENTS.md` 就近说明。

## 计划与进度

- [所有权到 SSA 规划边界里程碑](ownership-planning-milestone.md)：同分支组合交付、两条生产责任及统一验收边界

- [整体架构与工程治理计划](engineering-governance-plan.md)：已批准目标、迁移顺序与验收，不代表当前实现
- [执行账本](engineering-governance-progress.md)：治理批次的实际进度、基线与下一门禁
- [当前路线图](roadmap.md)：从演进单源与治理计划定位下一批次

- [本机重建验收](recovery-local-delivery.md)：授权范围、P0–P5与0182映射及本机证据
- [本机治理续行](governance-local-continuation.md)：P0–P5剩余条件、LSP原始成本试点与P5教程补齐

## 后继里程碑与起草材料

- [后继开发里程碑](post-governance-milestones.md)：M0–M6 的候选范围、依赖和启动门槛；已按用户授权启动 M1A 首片
- [M1A 起草材料](multifile-program-spec-draft.md)：三文件完整程序与剩余验收；A2 由 SPEC-0263 承接
- [M1B 文本处理](text-processing-spec-draft.md)：argv 词频到文本输入，错误模型与最小 API 决策
- [M2 借用访问](borrow-access-spec-draft.md)：现行能力、候选方案和接受/拒绝场景
- [M3A 顺序集合](sequential-collections-spec-draft.md)：应用所需操作、搬迁、借用与清理
- [M3B Map](map-collections-spec-draft.md)：键/查询/更新合同、语义前置与应用替换
- [M4 安全验证](memory-safety-validation-spec-draft.md)：检测接线、随机执行与外审材料
- [M5 度量与分发](measurement-distribution-spec-draft.md)：成本协议、双宿主 preview 与安装验收
- [M6 线程转移](thread-transfer-spec-draft.md)：v1 Transferable、启动/join/清理与未来共享边界
