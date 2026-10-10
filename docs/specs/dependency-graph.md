# Spec 依赖图

> **性质**：生成物（勿手改） · **状态**：current · **读取时机**：追溯 Spec 依赖拓扑时 · **唯一真源**：各 Spec 正文

由 `scripts/gen_spec_dag.py` 生成；只画拓扑结构，不含验收状态；状态见[README](README.md)。
重建时机：guide 版本启用或新增/迁移 draft Spec。
SVG 版本：[dependency-graph.svg](dependency-graph.svg)。

```mermaid
flowchart TD
ARCH(("已完成<br/>archive 274 份"))
subgraph Gactive["现行 active"]
  S0288["S0288<br/>Map 键值容器原生执行基础（SSA 原语、LLVM IR 代码生成与 Native Runtime 哈希表）"]
  S0289["S0289<br/>N1a 单来源范围描述符端到端交付"]
end
subgraph Gdrafts_v043["v0.43（draft）"]
  S0290["S0290<br/>链内立即消费序列与 owned 结果"]
end
S0289 --> S0290
ARCH --> S0288
ARCH --> S0289
```

## 节点链接

| 节点 | 分区 | 文档 |
|---|---|---|
| SPEC-0288 | active | [0288-map-native-execution.md](active/0288-map-native-execution.md) |
| SPEC-0289 | active | [0289-n1a-range-carrier.md](active/0289-n1a-range-carrier.md) |
| SPEC-0290 | drafts/v0.43 | [0290-immediate-consuming-sequence.md](drafts/v0.43/0290-immediate-consuming-sequence.md) |
| 已完成 Spec（274 份） | archive | [archive/specs/README.md](../archive/specs/README.md) |
