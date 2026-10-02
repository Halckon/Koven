# Spec 依赖图

> **性质**：生成物（勿手改） · **状态**：current · **读取时机**：追溯 Spec 依赖拓扑时 · **唯一真源**：各 Spec 正文

由 `scripts/gen_spec_dag.py` 生成；只画拓扑结构，不含验收状态；状态见[README](README.md)。
重建时机：guide 版本启用或新增/迁移 draft Spec。
SVG 版本：[dependency-graph.svg](dependency-graph.svg)。

```mermaid
flowchart TD
ARCH(("已完成<br/>archive 234 份"))
subgraph Gactive["现行 active"]
  S0182["S0182<br/>顺序容器 for frontend→SSA→native 集成"]
  S0249["S0249<br/>普通 owned-unit 封闭借用交接"]
end
ARCH --> S0182
ARCH --> S0249
```

## 节点链接

| 节点 | 分区 | 文档 |
|---|---|---|
| SPEC-0182 | active | [0182-sequential-for-lowering.md](active/0182-sequential-for-lowering.md) |
| SPEC-0249 | active | [0249-owned-unit-borrowed-handoff.md](active/0249-owned-unit-borrowed-handoff.md) |
| 已完成 Spec（234 份） | archive | [archive/specs/README.md](../archive/specs/README.md) |
