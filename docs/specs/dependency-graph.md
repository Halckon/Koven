# Spec 依赖图

> **性质**：生成物（勿手改） · **状态**：current · **读取时机**：追溯 Spec 依赖拓扑时 · **唯一真源**：各 Spec 正文

由 `scripts/gen_spec_dag.py` 生成；只画拓扑结构，不含验收状态；状态见[README](README.md)。
重建时机：guide 版本启用或新增/迁移 draft Spec。
SVG 版本：[dependency-graph.svg](dependency-graph.svg)。

```mermaid
flowchart TD
ARCH(("已完成<br/>archive 270 份"))
subgraph Gactive["现行 active"]
  S0284["S0284<br/>MutableList 头部元素快速移出与队列弹出 (MutableList.removeFirst)"]
end
ARCH --> S0284
```

## 节点链接

| 节点 | 分区 | 文档 |
|---|---|---|
| SPEC-0284 | active | [0284-mutable-list-remove-first.md](active/0284-mutable-list-remove-first.md) |
| 已完成 Spec（270 份） | archive | [archive/specs/README.md](../archive/specs/README.md) |
