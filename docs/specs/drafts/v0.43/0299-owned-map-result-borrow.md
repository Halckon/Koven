# SPEC-0299: owned Map 删除结果的非消费借用

> **性质**：候选变更合同 · **状态**：draft · **读取时机**：准备窄化owned Map结果的只读Borrow时 · **唯一真源**：本页的有界候选范围；语义以Guide为准

| 字段 | 值 |
|---|---|
| 状态 | draft |
| Goal ID | `KOV-P4-0299` |
| 所属 Phase | Phase 3事实消费、Phase 4 nullable place/loan与native |
| 语言规范 | [Guide v0.43](../../../guide/README.md)、[Map](../../../guide/12-collections-destructuring.md)、[所有权](../../../guide/10-ownership-borrowing-drop.md) |
| 批准依据 | 2026-10-11确认0297→0300→0299/0298顺序；本页保持draft，由独立实施分支激活 |
| 前置 Spec | [SPEC-0295](../../../archive/specs/0295-map-argv-word-frequency.md)、[SPEC-0296](../../../archive/specs/0296-owned-null-comparison-cleanup.md) |
| 前置 ADR | [ADR-0016](../../../adr/accepted/0016-interprocedural-borrow-abi.md)、[ADR-0017](../../../adr/accepted/0017-nullable-handle-ssa-abi.md) |
| 阻塞项 | 非消费payload place/proof、真实owner关联及调用期loan事实须先证明 |
| 影响范围 | 两入口窄化Map结果的Borrow实参、SSA/verifier直接消费者 |
| 语言语义变更 | 否；不启用新nullable表示或借用语法 |

## 1. Goal 与重起草来源

对`remove`已交付的具名owned nullable结果，在已验证非空分支同步交付只读Borrow，
支持其MoveOnly payload而不复制、clone、retain或消费原owner；调用结束后该owned结果仍按
原事实存活、可再次合法使用并恰一次清理。

2026-10-12原未发布草案丢失，本页依据已核边界重新起草，不是原稿恢复。原审计见
[历史摘要](../../../archive/migrations/pr72-74-followup-audit.md)；本页不重用其旧静态数字作为新验收。

## 2. 起点与范围

0295的两入口桥接核验typed non-null proof和注册Map结果后，只为Copyable payload复制
inner再借用；其Copyable门不是所有Borrow的语言限制。现行`get`/下标的MoveOnly按值查询
仍必须拒绝。owned `remove`返回、`!!`消费和本片非消费Borrow是不同合同。

首片为具名whole-root删除结果、非nullable V及同步Borrow参数。核对声明/窄化类型、symbol、
真实结果owner、非空proof与支配关系；保留已有表示，不将presence/payload aggregate
直接解释为inner RootPlace。loan依赖被删除值的新owner，不误绑Map entry或key。

String、现有可存储MoveOnly值及Resource分别核范围；原已支持分流保留，不能为统一表面
制造新复制。相同值的连续Borrow调用后仍可合法读取/消费，调用结束只结束借用，不drop payload。
缺失分支不能建立可读place。沿用frontend事实；事实/ABI不足时停下，不以去掉Copyable判断代替实现。

## 3. 非目标

不扩MoveOnly get、不改变withValue/requireValue、条件借用或nullable V的owned三态表示；
不做通用nullable ABI、普通Resource?工厂、字段/元素/temporary结果全覆盖、Inout或逃逸Borrow。
不顺带重写0296判空最后使用清理或0300循环owner运输，不改变普通Borrow/Copy语义。

`break`/`continue`及其他实参提前退出仍是独立后继候选，非本片验收；
后继必须先证明精确loop目标、未求值后缀、前缀owner/loan及provider逐边清理，不由普通调用成功推定。

## 4. 最小验收与交付

- [ ] single/unit分别建立具名remove结果窄化Borrow的普通源码红测，区分现有成功路径与真正缺口。
- [ ] String、MoveOnly、Resource及Copyable对照的SSA/verifier、未改写LLVM和正常native验证；
  新增MoveOnly payload的非消费Borrow路径不得出现Copy/Clone/retain或消费式NullableTake；
  Copyable对照保留既有合法复制桥，不要求重写为同一实现。
- [ ] 同一非空结果连续两次Borrow后再合法消费或scope清理；独立可观察资源事件无提前/重复析构。
- [ ] 缺失、错误窄化、借用期间冲突/逃逸及MoveOnly get保持拒绝；原Copyable桥和0296判空回归保持。
- [ ] 独立分支按红测→最小实现→直接回归→Architecture/验收推进；正式发布和CI另行闭环。

本页功能验证尚未运行；仅在本次0303恢复中做静态文档审查，不声明完整内存安全或所有nullable
Borrow能力完成。具体失败kind/Span和支持形状由普通源码验收记录，不凭静态门推定全部失败。
