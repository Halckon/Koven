# Koven 架构决策记录

ADR 只记录现行 guide 留白处、会长期影响多个 Spec 的架构选择。状态和维护规则见
[`../AGENTS.md`](../AGENTS.md)，新记录使用 [`TEMPLATE.md`](./TEMPLATE.md)。

| ADR | 状态 | 决策 |
|---|---|---|
| [ADR-0001](./0001-record-architecture-decisions.md) | accepted | 使用 ADR 保存架构决策及其理由 |
| [ADR-0002](./0002-bootstrap-workspace-layout.md) | accepted | Phase 0 workspace 布局与 `lang-std` bootstrap 边界 |
| [ADR-0003](./0003-diagnostic-architecture.md) | accepted | 结构化诊断的所有权、稳定性与展示边界 |
| [ADR-0004](./0004-source-span-position-model.md) | accepted | 源码身份、半开字节范围与展示位置模型 |
| [ADR-0005](./0005-package-source-root-mapping.md) | accepted | 显式 source root、逻辑路径与 package identity 的确定映射 |
| [ADR-0006](./0006-typed-ssa-block-parameters.md) | accepted | typed SSA 使用 block parameters、IR-local 类型与显式所有权效果 |

`proposed` 只表示已有推荐方案，不授权实现。关联 Spec 进入 `in-progress` 前，ADR 必须为
`accepted`；本规则生效后接受的 ADR 还须记录接受依据。存在有效用户站立授权时无需逐份
再次确认，但仍须完成背景、决策、替代方案、收益与代价审计；既有 accepted ADR 不追溯
改写历史。
