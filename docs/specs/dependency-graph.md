# Spec 依赖图

> **性质**：生成物（勿手改） · **状态**：current · **读取时机**：追溯 Spec 依赖拓扑时 · **唯一真源**：各 Spec 正文

由 `scripts/gen_spec_dag.py` 生成；只画拓扑结构，不含验收状态；状态见[README](README.md)。
重建时机：guide 版本启用或新增/迁移 draft Spec。
SVG 版本：[dependency-graph.svg](dependency-graph.svg)。

```mermaid
flowchart TD
ARCH(("已完成<br/>archive 211 份"))
subgraph Gactive["现行 active"]
  S0211["S0211<br/>顺序迭代 source/element loan 与退出清理"]
end
subgraph Gdrafts_v037["v0.37（draft）"]
  S0182["S0182<br/>顺序容器 for frontend→SSA→native 集成"]
  S0212["S0212<br/>借用式顺序迭代 SSA/LLVM primitives"]
end
S0211 --> S0182
S0212 --> S0182
ARCH --> S0182
ARCH --> S0211
ARCH --> S0212
```

## 节点链接

| 节点 | 分区 | 文档 |
|---|---|---|
| SPEC-0211 | active | [0211-sequential-iteration-ownership.md](active/0211-sequential-iteration-ownership.md) |
| SPEC-0182 | drafts/v0.37 | [0182-sequential-for-lowering.md](drafts/v0.37/0182-sequential-for-lowering.md) |
| SPEC-0212 | drafts/v0.37 | [0212-borrowed-sequential-iteration-ssa.md](drafts/v0.37/0212-borrowed-sequential-iteration-ssa.md) |
| 已完成 Spec（211 份） | archive | [archive/specs/README.md](../archive/specs/README.md) |
