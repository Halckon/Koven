# Spec 依赖图

> **性质**：生成物（勿手改） · **状态**：current · **读取时机**：追溯 Spec 依赖拓扑时 · **唯一真源**：各 Spec 正文

由 `scripts/gen_spec_dag.py` 生成；只画拓扑结构，不含验收状态；状态见[README](README.md)。
重建时机：guide 版本启用或新增/迁移 draft Spec。
SVG 版本：[dependency-graph.svg](dependency-graph.svg)。

```mermaid
flowchart TD
ARCH(("已完成<br/>archive 288 份"))
subgraph Gdrafts_v043["v0.43（draft）"]
  S0292["S0292<br/>链内立即消费序列与 owned 结果"]
end
subgraph Gdrafts_v044["v0.44（draft）"]
  S0291["S0291<br/>非空只读 Map 构造：MutableMap 立即消费式转换 consume()"]
end
ARCH --> S0291
ARCH --> S0292
```

## 节点链接

| 节点 | 分区 | 文档 |
|---|---|---|
| SPEC-0291 | drafts/v0.44 | [0291-readonly-map-construction.md](drafts/v0.44/0291-readonly-map-construction.md) |
| SPEC-0292 | drafts/v0.43 | [0292-immediate-consuming-sequence.md](drafts/v0.43/0292-immediate-consuming-sequence.md) |
| 已完成 Spec（288 份） | archive | [archive/specs/README.md](../archive/specs/README.md) |
