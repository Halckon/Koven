# Koven 文档入口

> **性质**：任务路由 · **状态**：current · **读取时机**：需要定位规范、实现、流程或历史时 · **唯一真源**：各链接页面

不要从头读取整个 `docs/`。先按任务选择一个入口，再由该入口缩小到相关领域；通常不需要超过
五份 live 文档。

## 默认入口

| 想回答的问题 | 读取入口 | 权威性 |
|---|---|---|
| Koven v0.39 允许什么、语义是什么 | [语言规范](guide/README.md) | 规范性真源 |
| 编译器内部表示、算法与资源必须遵守什么 | [Compiler Contracts](compiler-specs/README.md) | 工程合同，语言与 Phase 仍以 Guide 为准 |
| 编译器当前实际如何实现 | [Architecture](architecture/README.md) | 当前事实快照 |
| 如何开发、测试和交付 | [Development](development/README.md) | 工程流程 |
| 一次变更做什么、如何验收 | [Specs](specs/README.md) | 变更合同 |
| 为什么采用某项长期方案 | [ADR](adr/README.md) | 架构决策 |

## 按需候选

[Proposals](proposals/README.md) 只用于任务明确要求评审尚未启用的设计，不参与默认开发路由。

## 历史材料

[Archive](archive/README.md) 保存旧 guide、完成 Spec、冻结审计和过时教程。只有追溯历史决策、
旧验收证据或迁移来源时才读取；archive 不参与现行语义优先级。

## 文档修改

文档分类、状态迁移和交付规则见 [docs/AGENTS.md](AGENTS.md)。提交前运行：

```bash
python3 scripts/check_docs.py
```
