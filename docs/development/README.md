# Koven 开发指南

> **性质**：工程流程入口 · **状态**：current · **读取时机**：准备修改、验证或交付时 · **唯一真源**：本索引导航的工程方法页

本目录回答如何开发、验证和交付。语言规则以Guide为准；具体实施范围、依赖和验收进入
[Specs](../specs/README.md)，实现事实见[Architecture](../architecture/README.md)。
这里不维护第二份功能计划、阶段进度或验收账本。

## 按任务读取

| 任务 | 页面 |
|---|---|
| 确定 Phase、Spec/ADR 门禁和交付顺序 | [工作流与 Phase](workflow-and-phases.md) |
| 编写或拆分 Rust 模块、处理错误和日志 | [Rust 与模块](rust-and-modules.md) |
| 选择窄测、提交门禁或全量门禁 | [测试与分层验收](testing.md) |
| 新增/修改诊断、错误码、Span、机器输出 | [诊断规范](diagnostics.md) |
| 复用开源方案或新增/升级依赖 | [依赖治理](dependencies.md) |

## 工具与专项规则

- [手写 Rust 尺寸护栏](rust-size-policy.md)：真实base增长、历史baseline与显式例外。
- [Preview 候选工具](preview-candidate.md)：现有打包、安装与核验入口；发布授权独立。

具体 crate 的职责和最近测试入口由 `crates/**/AGENTS.md` 就近说明。

## 按需追溯

未启用设计从[Proposals](../proposals/README.md)进入；已发生的批次、测量与原起草材料
从[Archive](../archive/README.md)进入。归档资料不是默认必读，也不代表其中开放事项完成。
[旧证据索引](evidence/README.md)只导航固定历史来源；产物保留规则见测试文档。
