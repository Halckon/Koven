# v0.43 首片与后继合同

> **性质**：候选变更索引 · **状态**：draft · **读取时机**：区分原N1a交付、未启用consume与后继实施顺序时 · **唯一真源**：本目录各 Spec；现行语义仍见 [Guide](../../../guide/README.md)

2026-10-08 12:15 UTC，用户 `Sentinel_f249680deca4819196e97322bdc24963` 明确同意开始
r3 小修后的首片实施。授权覆盖本地合同准备、实现与验证，不包含提交、push、PR 或合并。
以上为2026-10-08准备时的授权记录。0289 的已批准 N1a 合同随 v0.43/ADR-0030
启用，现已按 PR72 实现 head 双宿主证据有界归档；最终交付见[核对摘要](../../../archive/migrations/pr72-74-followup-audit.md)。本目录的
0292 仍是未启用 consume 合同，不随 N1a 自动启用。

- [0289 N1a 范围 carrier](../../../archive/specs/0289-n1a-range-carrier.md)：新内联描述符、根来源与单文件/unit 端到端交付。
- [0292 立即消费路径](0292-immediate-consuming-sequence.md)：chain temporary owner 与立即 owned 产出。

```text
现行 M2B 来源合同 / 已验收 0288
        + 最小 std 扩展声明 / 可复用范围构造原语
        -> 0289：N1a single/unit 类型、所有权、SSA verifier、native
        -> 0292：consume 接管与正常退出清理、立即消费算法
复制能力独立前置 -> 显式物化；通用 Clone 尚未批准
N1b/filter 借用视图、Option、多来源与惰性链不进入本图的首片
```

0289 的声明、来源、SSA/native 与计数证据分阶段保留，最终有界验收见其 §22；
consume 不由这些证据自动启用，0292 仍须独立满足规范和实施前置。

## 2026-10-11 后继合同

用户已确认实施顺序0297→0300→0299/0298；0297已按PR78有界归档，0300已按PR79有界归档；0298冻结首片已按PR81修复后实现head双宿主及完整CI有界归档，0299冻结首片已按PR80实现head双宿主验收有界归档，广义后继保持开放；各自Spec分支维护，不继承上方2026-10-08首片授权，不启用范围外语义。
原本地起草与普通源码结论见[核对摘要](../../../archive/migrations/pr72-74-followup-audit.md)。

- [0297 Map key return清理](../../../archive/specs/0297-map-key-return-cleanup.md)：查询/删除的出口传播与前缀清理；break/continue仍在本片之外。
- [0298 范围绑定局部条件组合](../../../archive/specs/0298-range-binding-local-control-flow.md)：仅关闭Int ==/<与同根词法双正常arm if；首轮CI失败及修复保留，原广义后继见下节。
- [0299 owned Map result只读借用](../../../archive/specs/0299-owned-map-result-borrow.md)：按PR80实现head双宿主各23项新增测试有界归档；仅关闭冻结首片，嵌套调用等后继开放；当前交付状态见[唯一入口](../../README.md#近期交付)。
- [0300 single Map-for条件清理运输](../../../archive/specs/0300-single-map-loop-owner-carry.md)：按PR79实现head双宿主验收有界归档，限定正常耗尽；当前交付状态见[唯一入口](../../README.md#近期交付)。

## 0298首片之外的开放后继

这是原0298广义目标的当前候选入口；[原验收清单](../../../archive/specs/0298-range-binding-local-control-flow.md#5-原广义目标后继仍开放)保留历史。
以下均未选定下一片或实施，不因首片done变为完成，也不放入development历史账本代替工作入口：

- Boolean短路、String拼接与复杂RHS：分别封闭求值一次、未选后缀不执行及根/loan/清理证明。
- 更广局部控制流、条件/多来源及普通借用结果运输：先冻结具体源码和已发布事实，不删除整函数能力门，不默认一般NLL。
- while/loop及更广循环组合：先证明目标loop、fixed-point/owner版本、backedge和break/continue的逐边依赖与清理。

下一片须从实际main独立冻结合同并获得相应批准；不自动启用consume、optional borrow、借用存储/逃逸、N1b或泛型std扩展。

## 未编号的现行支持候选

[普通unit字段赋值与single nullable Resource工厂](../../../architecture/lowering-support.md#现存入口差异)
各自保留独立后继：字段更新须验证旧owner恰一次清理；工厂须验证nullable返回映射/ABI与
恰一次清理。它们尚未选定Goal或编号，不并入0296，也不因本次文档归整获得实现验收。
