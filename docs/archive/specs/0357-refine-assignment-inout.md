# SPEC-0357: E3 赋值与 inout 家族迁入所有权细化层

> **性质**：实施合同 · **状态**：done · **读取时机**：评审或实施路线图 E3/N01 的赋值、字段替换与 `Inout` lowering 时 · **唯一真源**：本片目标、范围与验收；语义以 Guide 为准，长期决定以 ADR-0016/0032 为准

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0357` |
| 所属 Phase | Phase 4（typed SSA lowering）；只消费 Phase 2/3 已验证事实 |
| 语言规范 | [Guide v0.46 参数体内能力](../../guide/10-ownership-borrowing-drop.md#value--borrow--inout-参数体内能力)、[mutable place 与冲突规则](../../guide/10-ownership-borrowing-drop.md)、[成员 receiver mode](../../guide/08-class-family-members.md) |
| 批准依据 | 2026-10-27 用户排定 E3 后续顺序：SPEC-0356 步 3 之后为“赋值 / inout（N01）” |
| 基线 | `feature/spec-0356-when-subject` `9cc9fd8a`（SPEC-0356 已归档） |
| 分支 | `feature/spec-0357-refine-assignment-inout` |
| 前置 | [SPEC-0356](0356-refine-when-family.md)（细化层 receiver 与 enum）、[ADR-0016](../../adr/accepted/0016-interprocedural-borrow-abi.md)、[ADR-0032](../../adr/accepted/0032-ownership-refinement-layer.md) |
| 阻塞项 | 无 |
| 语言语义变更 | 否 |

## 1. Goal

细化层按 Guide 与 ADR-0016 lowering 赋值与 `Inout`：p12b、p12c、q09 在单文件与 project 两种模式通过，
矩阵无回退、两模式差异为 0。

| 步 | 范围 | 关闭的用例 |
|---|---|---|
| 1 | callee：`Inout` 标量参数的读取与整体替换（新 SSA 操作 `LoanMutate`，ADR-0016 “以 `PlaceAccess::Loan` 读取或修改 target”）；caller：`&local` 实参（Copyable）建立 exclusive loan，调用后从 root place 读回并重绑 | p12c |
| 2 | 普通 class 字段替换 `local.f = v`（标量或 String 字段；MoveOnly 旧值由替换操作析构） | p12b |
| 3 | 细化层的 receiver 调用（Borrow/Inout 的普通 class receiver、Value/Borrow 的 Copyable enum receiver，显式或隐式 `this`）；普通 class 的 `Inout` 参数与 `Inout`/Borrow receiver 方法体内的字段读取与替换（`p.f`、`this.f`、裸 `f`）；`&p` 把 `Inout` 参数的 loan 继续传递；MoveOnly `&local` 调用后经 `RootPlaceTake` 取回 | q09 |

## 2. 设计约束

- 只消费既有事实：`assignment` 描述符、`aggregate_projection`、call 的参数 mode、`loans()` 的
  loan kind 与 begin/end、`BeforeReplacement`/`AfterReplacement` 析构点；不从 `&` 拼写推断 mode。
- 替换顺序按 Guide：先求值 RHS，再析构旧值并提交新值。MoveOnly 字段替换由
  `HeapFieldReplace`/`LoanMutate` 的隐式析构完成，并消费对应析构事实。
- 新操作 `LoanMutate { loan, value }` 只接受 exclusive loan，target 类型与 value 一致；首片只对
  Copyable target 开放，MoveOnly target 的整体替换留在子集外。
- 路由门：字段替换、对 `Inout` 参数的替换、`&` 实参，以及签名含 `Inout` 参数或 receiver 计入“已迁入家族”；
  receiver 调用本身不计入（只读方法等仍按其函数体分流）。
- `Inout` receiver 的 exclusive loan 在实参求值期间只保留，调用点才激活（Guide Two-Phase Borrow）；
  Borrow receiver 在实参之前借出。
- 不新增按“类型 × 控制流”拆分的模块；子集外的赋值继续由现有路径处理。

## 3. 验收标准

- [x] 步 1：p12c 两模式通过；`refine_tests::inout_scalar_parameters_replace_the_callers_binding`
  native 运行覆盖 `Inout Int` 的读写与调用者读回；`verify_loan_mutate_tests` 覆盖 `LoanMutate`
  的正反例（exclusive 通过；shared loan、类型不符与 MoveOnly target 以 OperationContract 拒绝）。
- [x] 步 2：p12b 两模式通过；`field_replacement_drops_the_old_value_and_keeps_the_owner` 以分配计数
  确认 String 字段旧值与 owner 各释放一次。
- [x] 步 3：q09 两模式通过；`inout_class_parameters_and_receivers_mutate_the_callers_owner` 覆盖
  `Inout` class 参数、显式/隐式 `this` 的字段读写、`Inout` receiver 调用与 `RootPlaceTake` 取回（分配
  计数 1）；`refine_subset_tests` 锁定 MoveOnly 整体替换、MoveOnly `Inout` String 参数在子集外。
- [x] 矩阵：154 probe 能力通过 130/154（+3），回退 0、两模式差异 0、内部错误 0（本地 Linux）。
- [x] 受影响目标定向测试（lang-codegen 全部 2446 项与集成测试）、fmt、clippy、尺寸护栏、
  `check_docs.py` 通过（本地 Linux，stable 1.97）；双宿主 CI 以 PR 为准。

既有测试更新：无断言放宽。三项旧测试因规划阶段改为接受 `Inout` 参数而调整实现而非测试——
未进入细化层时在 `Inout` 参数声明处报 UnsupportedNode（保持原 Span），`Inout` receiver 调用 Borrow
方法时以 `SharedReborrow` 收窄。

## 4. 非目标

- 泛型 class 的 `inout` 方法（p40）：随泛型家族（p40、t03）迁入；
- 容器元素赋值、复合赋值以外的新运算、`replace`/`swap` 原语、Two-Phase Borrow 之外的新 loan 规则；
- frontend 新事实。若发现事实缺口，停下另开 frontend Spec。

## 5. 实施记录

| 步 | 提交 | 内容 |
|---|---|---|
| 1–2 | `49cd97a3` | `LoanMutate`；规划为 `Inout` 参数分配 exclusive loan；标量 `Inout` 读写、`&local` 读回；字段替换 |
| 3 | 本分支提交 | `build/place.rs`、`build/receiver.rs` 与对应子集：class owner 的 loan（局部临时借出、参数/receiver 既有 loan、exclusive→shared reborrow）、receiver 调用（Borrow 先借、Inout 调用点激活）、`Inout` class 参数与 reborrow、MoveOnly 读回 `RootPlaceTake` |
