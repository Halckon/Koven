# SPEC-0357: E3 赋值与 inout 家族迁入所有权细化层

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：评审或实施路线图 E3/N01 的赋值、字段替换与 `Inout` lowering 时 · **唯一真源**：本片目标、范围与验收；语义以 Guide 为准，长期决定以 ADR-0016/0032 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0357` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.46 参数体内能力](../../guide/10-ownership-borrowing-drop.md#value--borrow--inout-参数体内能力)、[mutable place 与冲突规则](../../guide/10-ownership-borrowing-drop.md)、[成员 receiver mode](../../guide/08-class-family-members.md) |
| 批准依据 | 2026-10-27 用户排定 E3 后续顺序：SPEC-0356 步 3 之后为“赋值 / inout（N01）” |
| 基线 | `feature/spec-0356-when-subject` `9cc9fd8a`（SPEC-0356 已归档） |
| 分支 | `feature/spec-0357-refine-assignment-inout` |
| 前置 | [SPEC-0356](../../archive/specs/0356-refine-when-family.md)（细化层 receiver 与 enum）、[ADR-0016](../../adr/accepted/0016-interprocedural-borrow-abi.md)、[ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

细化层按 Guide 与 ADR-0016 lowering 赋值与 `Inout`：p12b、p12c、q09 在单文件与 project 两种模式通过，
矩阵无回退、两模式差异为 0。

| 步 | 范围 | 关闭的用例 |
|---|---|---|
| 1 | callee：`Inout` 标量参数的读取与整体替换（新 SSA 操作 `LoanMutate`，ADR-0016 “以 `PlaceAccess::Loan` 读取或修改 target”）；caller：`&local` 实参（Copyable）建立 exclusive loan，调用后从 root place 读回并重绑 | p12c |
| 2 | 普通 class 的 `var` 字段替换：`local.f = v`、`this.f = v`（`Inout` receiver）、`inout` 参数的 `p.f = v`；字段读取经 shared/exclusive loan | p12b |
| 3 | 细化层的 receiver 调用（Value/Borrow/Inout receiver、class 与 enum）与 `Inout` 参数的 exclusive reborrow（`&c` 传递 `Inout c`）、MoveOnly `&local` 调用后经 `RootPlaceTake` 取回 | q09 |

## 2. 设计约束

- 只消费既有事实：`assignment` 描述符、`aggregate_projection`、call 的参数 mode、`loans()` 的
  loan kind 与 begin/end、`BeforeReplacement`/`AfterReplacement` 析构点；不从 `&` 拼写推断 mode。
- 替换顺序按 Guide：先求值 RHS，再析构旧值并提交新值。MoveOnly 字段替换由
  `HeapFieldReplace`/`LoanMutate` 的隐式析构完成，并消费对应析构事实。
- 新操作 `LoanMutate { loan, value }` 只接受 exclusive loan，target 类型与 value 一致；首片只对
  Copyable target 开放，MoveOnly target 的整体替换留在子集外。
- 路由门：赋值给字段或 `Inout` 绑定、带 `&` 的实参、receiver 调用计入“已迁入家族”。
- 不新增按“类型 × 控制流”拆分的模块；子集外的赋值继续由现有路径处理。

## 3. 验收标准

- [ ] 步 1：p12c 两模式通过；`refine_tests` native 运行覆盖 `Inout Int` 的读写与调用者读回；
  SSA verifier/LLVM 覆盖 `LoanMutate` 的正反例（shared loan 拒绝、类型不符拒绝）。
- [ ] 步 2：p12b 两模式通过；native 运行覆盖局部 owner 与 `Inout` receiver 的字段替换。
- [ ] 步 3：q09 两模式通过；`refine_subset_tests` 锁定子集边界（MoveOnly 整体替换、泛型 receiver）。
- [ ] 受影响目标定向测试、fmt、clippy、尺寸护栏、`check_docs.py` 通过；双宿主 CI 以 PR 为准。

## 4. 非目标

- 泛型 class 的 `inout` 方法（p40）：随泛型家族（p40、t03）迁入；
- 容器元素赋值、复合赋值以外的新运算、`replace`/`swap` 原语、Two-Phase Borrow 之外的新 loan 规则；
- frontend 新事实。若发现事实缺口，停下另开 frontend Spec。

## 5. 实施记录

| 步 | 提交 | 内容 |
|---|---|---|
