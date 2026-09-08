# Koven Proposals

> **性质**：非规范候选索引 · **状态**：未启用 · **读取时机**：任务明确要求评审未来语义时 · **唯一真源**：各 proposal 正文

Proposal 不修改 v0.35、不批准 Spec，也不代表实现优先级。普通开发任务不要读取本目录。

- [v0.36：关联常量与封闭求值](v0.36-associated-constants.md)
- [v0.37：借用式顺序迭代](v0.37-sequential-iteration.md)
- [Map / MutableMap 所有权候选](map-ownership.md)
- [v2 interface 值与动态分发](v2-interface-values-and-dynamic-dispatch.md)

对应阻塞 Spec 从 [Specs](../specs/README.md) 进入；长期架构候选仍遵循 ADR 生命周期。

## 从候选 guide 到可实施 Spec

版本号不表示累积继承已经成立。v0.36/v0.37 两份候选正文仍以 v0.32 起草，不能直接替换现行 v0.35。
启用前按以下顺序收口，每一步保留可审查证据：

1. 逐条对照候选规则与现行 v0.35，列出保留、补充、取代的规则及目标章节；对冲突请求决定。
2. 明确新版本是否包含其他候选版本。未纳入的候选保持未启用，不能凭版本号隐式合并。
3. 准备完整的新 guide 与引用更新，并由用户明确启用；在此之前 current 入口仍为 v0.35。
4. 将对应 Spec 重基到启用后的 guide，核对前置 Spec/ADR 与批准依据，再迁移依赖完备的切片到 active。
5. 按各版本 Spec 索引推进 Phase 产物，逐项记录定向验收；规范启用不代表实现已经完成。

验收命令和测试并行仅由[测试与分层验收](../development/testing.md)定义；候选正文不复制工程门禁。

## 历史

v0.35 已启用；[原候选与重基审查](../archive/guides/v0.35-candidate.md)只用于追溯。
