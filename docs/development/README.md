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

具体 crate 的职责和最近测试入口由 `crates/**/AGENTS.md` 就近说明。

## 计划与进度

- [整体架构与工程治理计划](engineering-governance-plan.md)：已批准目标、迁移顺序与验收，不代表当前实现
- [执行账本](engineering-governance-progress.md)：治理批次的实际进度、基线与下一门禁
- [当前路线图](roadmap.md)：从演进单源与治理计划定位下一批次
