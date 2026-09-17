# Spec 依赖图

> **性质**：生成物（勿手改） · **状态**：current · **读取时机**：查看 Spec 依赖拓扑时 · **唯一真源**：各 Spec 正文

由 `scripts/gen_spec_dag.py` 生成；只画拓扑结构，不含验收状态；状态见 [README](README.md)。
重建时机：guide 版本启用或新增/迁移 draft Spec。SVG 版本：[dependency-graph.svg](dependency-graph.svg)。

```mermaid
flowchart TD
ARCH(("已完成<br/>archive 209 份"))
subgraph Gactive["现行 active"]
  S0227["S0227<br/>跨文件常量 SSA 与 native 交付"]
end
subgraph Gdrafts_v037["v0.37（draft，未启用）"]
  S0179["S0179<br/>顺序容器借用迭代 typed plan"]
  S0182["S0182<br/>顺序容器 for frontend→SSA→native 集成"]
  S0211["S0211<br/>顺序迭代 source/element loan 与退出清理"]
  S0212["S0212<br/>借用式顺序迭代 SSA/LLVM primitives"]
end
S0179 --> S0182
S0179 --> S0211
S0211 --> S0182
S0212 --> S0182
ARCH --> S0179
ARCH --> S0182
ARCH --> S0211
ARCH --> S0212
ARCH --> S0227
```

## 节点链接

| 节点 | 分区 | 文档 |
|---|---|---|
| SPEC-0227 | active | [active/0227-unit-constant-native-lowering.md](active/0227-unit-constant-native-lowering.md) |
| SPEC-0179 | drafts/v0.37 | [drafts/v0.37/0179-sequential-iteration-typed-plan.md](drafts/v0.37/0179-sequential-iteration-typed-plan.md) |
| SPEC-0182 | drafts/v0.37 | [drafts/v0.37/0182-sequential-for-lowering.md](drafts/v0.37/0182-sequential-for-lowering.md) |
| SPEC-0211 | drafts/v0.37 | [drafts/v0.37/0211-sequential-iteration-ownership.md](drafts/v0.37/0211-sequential-iteration-ownership.md) |
| SPEC-0212 | drafts/v0.37 | [drafts/v0.37/0212-borrowed-sequential-iteration-ssa.md](drafts/v0.37/0212-borrowed-sequential-iteration-ssa.md) |
| 已完成 Spec（209 份） | archive | [archive/specs/README.md](../archive/specs/README.md) |
