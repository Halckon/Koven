# Spec 依赖图

> **性质**：生成物（勿手改） · **状态**：current · **读取时机**：追溯 Spec 依赖拓扑时 · **唯一真源**：各 Spec 正文

由 `scripts/gen_spec_dag.py` 生成；只画拓扑结构，不含验收状态；状态见[README](README.md)。
重建时机：guide 版本启用或新增/迁移 draft Spec。
SVG 版本：[dependency-graph.svg](dependency-graph.svg)。

```mermaid
flowchart TD
ARCH(("已完成<br/>archive 314 份"))
subgraph Gactive["现行 active"]
  S0291["S0291<br/>非空只读 Map 构造与插入序共享存储"]
  S0332["S0332<br/>Rc nullable while 既有路径回归修复"]
end
subgraph Gdrafts_v043["v0.43（draft）"]
  S0292["S0292<br/>链内立即消费序列与 owned 结果"]
end
ARCH --> S0292
ARCH --> S0332
```

## 节点链接

| 节点 | 分区 | 文档 |
|---|---|---|
| SPEC-0291 | active | [0291-readonly-map-construction.md](active/0291-readonly-map-construction.md) |
| SPEC-0332 | active | [0332-rc-nullable-while-regression.md](active/0332-rc-nullable-while-regression.md) |
| SPEC-0292 | drafts/v0.43 | [0292-immediate-consuming-sequence.md](drafts/v0.43/0292-immediate-consuming-sequence.md) |
| 已完成 Spec（314 份） | archive | [archive/specs/README.md](../archive/specs/README.md) |
