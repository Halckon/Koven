# Koven 架构决策记录

ADR 只记录现行 guide 留白处、会长期影响多个 Spec 的架构选择。状态和维护规则见
[`../AGENTS.md`](../AGENTS.md)，新记录使用 [`TEMPLATE.md`](./TEMPLATE.md)。

| ADR | 状态 | 决策 |
|---|---|---|
| [ADR-0001](./0001-record-architecture-decisions.md) | accepted | 使用 ADR 保存架构决策及其理由 |
| [ADR-0002](./0002-bootstrap-workspace-layout.md) | proposed | Phase 0 workspace 布局与 `lang-std` bootstrap 边界 |
| [ADR-0003](./0003-diagnostic-architecture.md) | proposed | 结构化诊断的所有权、稳定性与展示边界 |

`proposed` 只表示已有推荐方案，不授权实现。关联 Spec 进入 `in-progress` 前，必须由用户明确
接受对应 ADR，或修订后再接受。
