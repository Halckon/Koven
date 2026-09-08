# SPEC-0202：nullable `when` 剩余域 typed facts

> **性质**：draft Spec · **状态**：draft（blocked by unapproved v0.35 guide） · **读取时机**：评审 v0.35 proposal 或对应 Goal 时 · **唯一真源**：本 Spec

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `draft` |
| Goal ID | `KOV-P2-202` |
| 所属 Phase | Phase 2 |
| 语言规范 | 起草基线 v0.32；候选 [v0.35 §35](../../../proposals/v0.35-nullable-when-and-non-null-assertion.md) |
| 批准依据 | 无；v0.35 尚未启用，且尚未显式重基到现行 v0.34 |
| 前置 Spec | SPEC-0019、0021 `done` |
| 前置 ADR | 无 |
| 关联 ADR | ADR-0017 |
| 阻塞项 | 明确 v0.35 对现行 v0.34 的重基与取代关系；v0.35 启用 |
| 影响范围 | `lang-frontend` type checking flow model/tests；Architecture |
| 语言语义变更 | 否；实施启用后的 v0.35 typed flow 契约 |

## 2. Goal

完成后，type checker 为 nullable `when` 发布稳定 subject、逐 entry 剩余域、alternative 事实
交集与 body non-null typed plan，后续阶段无需重新解释 AST 条件顺序。

## 3. 范围与需求

- subject 精确检查一次；只有稳定 subject binding 可获得源码可见 smart-cast identity，temporary
  仍保留内部单次求值 identity。
- explicit `null` condition 发布 match-null/fallthrough-non-null edge；后续 entry 与最终 `else`
  接收前序 condition 的剩余域。
- comma alternatives 分别检查，body 只保留所有可达 match edge 的事实交集；与既有 enum/
  Boolean coverage、branch join、trial rollback 组合。
- descriptor 以封闭 category 区分 owned root、Borrow/Inout root、ordinary field、container
  element 与 temporary；发布 facts 而不把 flow plan 写回 AST，诊断继续复用 L0108–L0112。

## 4. 非目标

- 不检查 move/loan/drop，不生成 SSA/LLVM；不实现 `!!`、Elvis、safe call 或 inline nullable ABI。

## 5. 验收标准

- [ ] nullable enum/Boolean 的 null-first、null-last、else 与多 entry 剩余域正反矩阵通过。
- [ ] `Node?`/`Box<V>?`/`Rc<T>?` 的 `null`+`else` 覆盖 owned root、temporary、Borrow/Inout 与
  field/element category；plan 明确标识 ADR-0017 owner-native eligible subset。
- [ ] mixed comma alternatives 不获得错误 non-null fact；全 non-null alternatives 获得共同 fact。
- [ ] temporary 单次求值、stable binding、mutation/call invalidation、branch join 与 trial rollback 稳定。
- [ ] descriptor identity/Span/顺序确定，既有 L0108–L0112 与 type suite 回归。
- [ ] Architecture 与实现事实同步。

## 6. 技术方案与边界

扩展现有 `when` flow checker 与 typed product，复用 closed-domain、flow key 和 trial snapshot；
不在 ownership/codegen 中复制 coverage 算法。

## 7. 实施计划

1. [ ] 建立 nullable remaining-domain descriptor → 验证：纯 model/rollback 测试。
2. [ ] 接入 `when` entry/alternative/body flow → 验证：type-checking 正反矩阵。
3. [ ] 同步验收与 Architecture → 验证：按[分层验收](../../../development/testing.md)选择目标测试与必要下游检查，并记录命中数。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | typed facts、测试与完成文档 | `feat(frontend): plan nullable when flow (SPEC-0202)` |

## 9. 未决问题

- 启用前处理 [v0.34 重基审查](../../../proposals/v0.35-nullable-when-and-non-null-assertion.md#v034-重基审查未启用)中的 R3（field/element proof identity）。

- 重基与启用尚未完成，具体阻塞项见元数据。

## 10. 验证记录

实施前按[分层验收](../../../development/testing.md)将第 5 节各项映射到实际测试目标/过滤器；
记录命中数、结果与未运行原因。同一状态下的有效证据只运行一次，不默认运行 frontend 全量。

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-27 roadmap 审计 | 通过 | 现有 type checker 有 nullable coverage，但未发布 null fallthrough/else remaining-domain plan |
