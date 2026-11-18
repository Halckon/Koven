# ADR-0041: 借入的 nullable String 的非空视图

> **性质**：架构决策记录 · **状态**：accepted · **读取时机**：修改 shared String? 的判空分流或 SSA 验证规则时 · **唯一真源**：本 ADR 的表示与验证合同

## 状态

accepted

## 接受依据

2026-11-17 用户明确接受 ADR-0041。SPEC-0392 记录实现和验收。

## 背景

Guide 允许借用参数在已证实非空的路径上共享读取。现有 SSA 能通过
shared loan 检查可空存储的 presence。值分支提供的 proof 则关联已
拥有的 nullable Value，不能表示原存储仍由调用者持有的情形。

## 决策

- 为共享的 inline niche `String?` 引入 `NullableStringLoanBranch`。
  判空只读原 loan 的 presence 字段。唯一的非空边定义子 shared
  `String` loan，子 loan 与父 loan 使用同一个存储地址。
- verifier 验证 active shared source、`String?` 存储类型、子 loan 的
  `String` 类型及非空目标的唯一前驱。子 loan 的根和依赖指向父 loan，
  在 CFG 参数间保留来源；所有正常出口须履行子 loan 的结束义务，
  父 loan 在子 loan 活跃时不得结束。
- 此分支只支持 ADR-0033 所定义的 String 数据指针 niche。空字符串
  使用有效数据指针，null 使用空指针。LLVM 使用同一地址作为非空
  视图，保留父存储及其析构义务。其他 layout、Inout 或一般 place
  的 payload 证明各自依照既有阶段合同。

## 替代方案

复用 owned `NullableBranch` 需要把借用值提升成 owner。复制含所有权
的表示会引入新的所有权和清理义务。直接以普通 `SharedReborrow`
改变类型则缺少 SSA 可验证的非空边和类型依据。

## 后果

非空读取共享现有 ABI 和按路径追踪的借用关系。新增终止指令需要
在 SSA、CFG 验证和 LLVM 三处维护，并为非法父 loan、错误前驱和
跨块生命周期保留负例。

## 关联

- 相关 Spec：[SPEC-0392](../../specs/active/0392-compiler-layered-review.md)
- 相关决策：[ADR-0017](../accepted/0017-nullable-handle-ssa-abi.md)、[ADR-0033](../accepted/0033-inline-nullable-representation.md)
- 取代的 ADR：无
- 被以下 ADR 取代：无
