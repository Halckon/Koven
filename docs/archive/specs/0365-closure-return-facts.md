# SPEC-0365：带捕获闭包返回的来源事实

> **性质**：实施合同 · **状态**：done · **读取时机**：评审或修改 compilation-unit callable provenance 的返回摘要时 · **唯一真源**：本片目标、范围与验收；语义以 Guide 07 为准，长期决定以 ADR-0034 为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P3-0365` |
| 所属 Phase | Phase 3（ownership checking：callable provenance） |
| 语言规范 | [Guide v0.46 调用、Lambda 与 Closure](../../guide/07-calls-lambdas-closures.md) |
| 批准依据 | 2026-10-28 用户确认 ADR-0034；其第 4 节要求函数返回 closure 时保留 concrete identity。p18 缺这一来源事实，按“发现事实缺口另开 frontend Spec”的规则单列 |
| 基线 | main `8b531537` |
| 分支 | `feature/spec-0365-closure-return-facts` |
| 前置 | [ADR-0034](../../adr/accepted/0034-refine-closures-and-callable-views.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

现有返回摘要（`pointer_callable_return`）只覆盖“空环境”返回：无捕获 lambda 与已知函数。带捕获的
返回（只能是 move closure，borrowed closure 返回已被 L0137 拒绝）没有任何来源事实，codegen 无法得知
返回值的 concrete closure 类型，p18 因此在规划阶段失败。本片新增与 pointer 摘要不相交的
`closure_callable_return(target)`：工厂函数唯一一次正常返回是带捕获 lambda 时，发布该 lambda 的
来源身份；调用点据此解析为 `FactoryResult`。

## 2. 规则

- 只有唯一一次正常返回、且返回值直接是该 lambda（含分组）时发布；多返回点、来源不唯一时不发布。
- 与 pointer 摘要互斥：带捕获的 lambda 永远不出现在 pointer 摘要里。
- `FactoryResult` 来源对 pointer 与 closure 两类摘要的工厂都成立。

## 3. 验收标准

- [x] `callable_contract_capturing_return_publishes_a_closure_summary`：发布摘要且来源是原 lambda、
  pointer 摘要为空、多返回点不发布、调用点为 `FactoryResult`；原“带捕获返回不发布 pointer 摘要”的测试保持。
- [x] lang-frontend 全量测试、fmt、clippy、`check_docs.py` 通过。

## 4. 非目标

- codegen 消费（规划器与细化层）属于 SPEC-0364 步 3。

## 5. 实施记录

| 提交 | 内容 |
|---|---|
| 本分支首提交 | `UnitClosureCallableReturnSummary` 与访问器；`FactoryResult` 求解接受两类摘要；回归测试 |
