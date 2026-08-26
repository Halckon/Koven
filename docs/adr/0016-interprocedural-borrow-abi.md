# ADR-0016：跨 callable Borrow 的 typed SSA 与 LLVM ABI

## 状态

accepted

## 接受依据

2026-08-26 依据当前持续 Goal“继续推进 guide 和分阶段实施 specs，先审计 roadmap，再根据
依赖图推进”的站立授权接受。本 ADR 只封闭现行 v0.30 已有 Borrow/Inout 参数契约如何进入
typed SSA 与内部 LLVM ABI，不改变源码参数模式或可观察语义。

## 背景

frontend 已把无 marker 参数规范化为 Borrow，并发布调用期 loan；当前 SSA `DirectCall` 却只
接受 `ValueId`，ownership verifier 因而把所有 MoveOnly 实参按 Value delivery 消费。标量
Copyable 参数暂时掩盖了这一区别，但 `inspect(rc.value)`、借用 class/Box/aggregate 以及未来
instance receiver 都不能用裸 handle load 或复制冒充 Borrow。

现有 SSA 已有 `RootPlace`、`BorrowBegin`、`BorrowEnd` 与函数内 `LoanId`，因此需要确定跨
callable 边界怎样保留 loan 证明，同时不把 caller-local ID 直接泄漏成 callee 内部实体。

## 决策

### SSA 调用边界

- callable 参数签名保留 `Value`、`Borrow`、`Inout` 三种 delivery identity；不得只保存裸
  `SsaTypeId`。返回值仍是 owned Value delivery。
- caller 对 Borrow/Inout 实参按 frontend loan fact 建立 `Place` 与 `BorrowBegin`，并把有效 loan
  直接作为带 delivery identity 的 call operand；同步 call 返回后按事实执行 `BorrowEnd`。
- Value operand 消费 MoveOnly value；Borrow/Inout operand 要求对应 shared/exclusive active loan，
  不消费 owner。call operand 本身不是可存储、返回或捕获的 first-class value。
- caller-local `LoanId` 只标识 caller operand，不成为 callee 的 ID。callee entry 创建函数内独立
  的 shared/exclusive loan 参数，并以既有 `PlaceAccess::Loan` 读取或修改 target。
- callee entry loan 是支配整个 callable CFG 的函数参数，可在后继 block 直接使用，并在 callable
  正常退出时隐式结束；`BorrowBegin` 产生的局部 loan 仍必须通过 block 参数显式跨 edge 转移。

### LLVM ABI

- Borrow/Inout loan operand/parameter lower 为指向 target storage 的非空 pointer；Value 参数
  继续使用既有 first-class value/owner ABI。该 pointer 只是内部同步调用 ABI，不承诺 FFI 稳定性。
- caller 必须为没有稳定地址的 SSA value 建立受 verifier 跟踪的 root storage；LLVM 可以用
  entry-block alloca 或等价地址化实现，但不得因此复制、retain 或提前 drop MoveOnly owner。
- callee 对 Borrow 只生成 load/read-only projection；Inout 才允许 mutate。LLVM attribute 可在
  proven 后增加，但不能替代 typed SSA verifier。

### 验证与生命周期

- operation verifier 锁定参数 mode、target、loan kind 与 callee signature；ownership
  verifier 锁定 loan 在 call 前 active、call 后结束、owner 在 loan 期间不可 move/drop。
- frontend lowering只消费已验证的 parameter binding、argument mapping 与 loan facts，不按源码
  marker 或函数名重新推导 mode。
- abort/diverging call 必须在控制转移边释放 caller 的其他 owner；不生成不可达的 BorrowEnd。

## 替代方案

### 所有参数继续按值传递

不采用。它会把 Borrow 的非消费语义改成 move，或迫使 codegen 对 MoveOnly handle 做未授权复制。

### 直接把 caller 的 `LoanId` 作为 callee 参数

不采用。ID、alias roots 与 verifier state 都是 function-local；跨函数复用会破坏 SSA 所有权和
确定性边界。

### Borrow 参数统一复制裸 handle

不采用。class/Rc handle 的位复制不会建立 owner obligation，但现有 SSA 会把它误认为新的
MoveOnly value；对 inline aggregate 更无法保持地址与可变性语义。

### 仅为 Rc.value 添加专用 call

不采用。问题来自通用 Borrow 参数 ABI，Rc 专用后门会与 class/Box/container/receiver 形成多套
不兼容调用规则。

## 后果

收益：

- Borrow/Inout 的 frontend 事实、SSA verifier 与 LLVM ABI 首次端到端一致；
- Rc MoveOnly payload、普通 owner 与未来 receiver 可复用同一调用边界；
- 不引入隐式 retain/copy，loan 生命周期仍由显式 operation 验证。

代价与风险：

- callable signature、DirectCall、block entry、render、verifier 与 LLVM adapter 都需要协同修改；
- value addressization 可能增加未优化 alloca，后续可由 LLVM 优化但不能先牺牲语义；
- Inout 的完整 source lowering 可分后续 Spec，但 loan delivery identity 必须一次设计一致。

## 关联

- 首个实施 Spec：SPEC-0195
- 解锁：SPEC-0045 的 MoveOnly Rc payload Borrow native 验收、SPEC-0191 receiver lowering
- 相关 ADR：[ADR-0006](./0006-typed-ssa-block-parameters.md)、
  [ADR-0008](./0008-internal-value-and-allocation-abi.md)、
  [ADR-0015](./0015-shared-owner-runtime-abi.md)
- 取代的 ADR：无
- 被以下 ADR 取代：无
