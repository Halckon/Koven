# SPEC-0362: E3 跳转放弃的待交付实参与 `!!` 的分支临时值

> **性质**：实施合同 · **状态**：done · **读取时机**：评审或实施路线图 E3 中 `!!` 与实参内控制转移的 lowering 时 · **唯一真源**：本片目标、范围与验收；语义以 Guide 为准，长期决定以 ADR-0032 为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0362` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.46 可空与错误](../../guide/09-nullability-errors.md) |
| 批准依据 | 2026-10-28 用户要求：PR #191（D10 Stage A，nullable 断言原源经 Unit 观察）失败，所缺能力由 E3 细化层补足 |
| 基线 | `feature/spec-0361-refine-generic-value-class` `5328f96a` |
| 分支 | `feature/spec-0362-refine-pending-arguments` |
| 前置 | [SPEC-0350](0350-ownership-refinement-nullable-pilot.md)、[SPEC-0361](0361-refine-generic-value-class.md)、[ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

细化层接受两类原先回退到逐家族路径的 `!!`：

- operand 为带括号的 owned 名称（`((node))!!`），或其他分支以 `return`/`break`/`continue`
  离开的 `if` 结果临时值；
- 在调用实参中途跳转：已求值、按 Value 交付给该调用的 MoveOnly 命名 owner（待交付实参）
  在 `ControlTransfer` 边上按 frontend 发布的 `Temporary` drop 事实释放。

PR #191 的三项 Unit 对照测试（`unit_lower_nullable_tests::non_null_assertion_*`）全部通过；
其中 `break` 出外层 `loop` 的原源此前在两条路径上都无法 lowering。

## 2. 设计约束

- 只消费既有事实：`ControlTransfer` 的 `Temporary` drop 目标、`!!` 的 source category。
- 子集只接受目标为命名 owner 的待交付实参；持有 loan 的借用临时值（如 Borrow 实参的构造结果）
  仍在子集外。
- Value 实参在调用点统一交付：后续实参中的跳转仍能找到先前实参的值；交付的 MoveOnly 命名 owner
  在当前路径标记为已离开，后续 `LoopExit` drop 不再重复释放它。

## 3. 验收标准

- [x] PR #191 的三项测试在本分支上通过（测试源码属于 PR #191，不随本分支提交）。
- [x] `refine_subset_tests` 保持“借用临时值之后跳出实参”留在子集外。
- [x] conformance 无回退；lang-codegen 全量测试、fmt、clippy、尺寸护栏、`check_docs.py` 通过；双宿主 CI 以 PR 为准。

## 4. 非目标

- 借用临时值、Value 交付的非名称临时值在跳转时的释放；frontend 新事实。

## 5. 实施记录

| 提交 | 内容 |
|---|---|
| 本分支首提交 | `!!` 的括号名称与 `if` 临时 operand；待交付实参在跳转边释放；Value 实参在调用点统一交付 |
