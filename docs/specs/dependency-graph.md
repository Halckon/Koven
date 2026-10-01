# Spec 依赖图

> **性质**：生成物（勿手改） · **状态**：current · **读取时机**：追溯 Spec 依赖拓扑时 · **唯一真源**：各 Spec 正文

由 `scripts/gen_spec_dag.py` 生成；只画拓扑结构，不含验收状态；状态见[README](README.md)。
重建时机：guide 版本启用或新增/迁移 draft Spec。
SVG 版本：[dependency-graph.svg](dependency-graph.svg)。

```mermaid
flowchart TD
ARCH(("已完成<br/>archive 213 份"))
subgraph Gactive["现行 active"]
  S0182["S0182<br/>顺序容器 for frontend→SSA→native 集成"]
  S0228["S0228<br/>Linux x86_64 本机目标与基线验收"]
  S0229["S0229<br/>扩展数值字面量值的端到端闭合"]
end
ARCH --> S0182
ARCH --> S0228
```

## 节点链接

| 节点 | 分区 | 文档 |
|---|---|---|
| SPEC-0182 | active | [0182-sequential-for-lowering.md](active/0182-sequential-for-lowering.md) |
| SPEC-0228 | active | [0228-linux-x86-64-native-host.md](active/0228-linux-x86-64-native-host.md) |
| SPEC-0229 | active | [0229-extended-numeric-literal-values.md](active/0229-extended-numeric-literal-values.md) |
| 已完成 Spec（213 份） | archive | [archive/specs/README.md](../archive/specs/README.md) |
