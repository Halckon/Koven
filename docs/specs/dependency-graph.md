# Spec 依赖图

> **性质**：生成物（勿手改） · **状态**：current · **读取时机**：追溯 Spec 依赖拓扑时 · **唯一真源**：各 Spec 正文

由 `scripts/gen_spec_dag.py` 生成；只画拓扑结构，不含验收状态；状态见[README](README.md)。
重建时机：guide 版本启用或新增/迁移 draft Spec。
SVG 版本：[dependency-graph.svg](dependency-graph.svg)。

```mermaid
flowchart TD
ARCH(("已完成<br/>archive 282 份"))
subgraph Gactive["现行 active"]
  S0303["S0303<br/>development 文档职责收敛与候选保全"]
end
subgraph Gdrafts_v043["v0.43（draft）"]
  S0292["S0292<br/>链内立即消费序列与 owned 结果"]
  S0297["S0297<br/>Map 查询与删除的 key return 清理"]
  S0298["S0298<br/>N1a 范围绑定与有界局部控制流"]
  S0299["S0299<br/>owned Map 删除结果的非消费借用"]
  S0300["S0300<br/>single for 中 Map guarded owner 运输"]
end
subgraph Gdrafts_v044["v0.44（draft）"]
  S0291["S0291<br/>非空只读 Map 构造：MutableMap 立即消费式转换 consume()"]
end
ARCH --> S0291
ARCH --> S0292
ARCH --> S0297
ARCH --> S0298
ARCH --> S0299
ARCH --> S0300
```

## 节点链接

| 节点 | 分区 | 文档 |
|---|---|---|
| SPEC-0303 | active | [0303-development-document-ownership.md](active/0303-development-document-ownership.md) |
| SPEC-0291 | drafts/v0.44 | [0291-readonly-map-construction.md](drafts/v0.44/0291-readonly-map-construction.md) |
| SPEC-0292 | drafts/v0.43 | [0292-immediate-consuming-sequence.md](drafts/v0.43/0292-immediate-consuming-sequence.md) |
| SPEC-0297 | drafts/v0.43 | [0297-map-key-return-cleanup.md](drafts/v0.43/0297-map-key-return-cleanup.md) |
| SPEC-0298 | drafts/v0.43 | [0298-range-binding-local-control-flow.md](drafts/v0.43/0298-range-binding-local-control-flow.md) |
| SPEC-0299 | drafts/v0.43 | [0299-owned-map-result-borrow.md](drafts/v0.43/0299-owned-map-result-borrow.md) |
| SPEC-0300 | drafts/v0.43 | [0300-single-map-loop-owner-carry.md](drafts/v0.43/0300-single-map-loop-owner-carry.md) |
| 已完成 Spec（282 份） | archive | [archive/specs/README.md](../archive/specs/README.md) |
