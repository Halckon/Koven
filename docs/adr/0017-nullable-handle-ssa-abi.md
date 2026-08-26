# ADR-0017：nullable handle 的 typed SSA 与 null-niche ABI

## 状态

accepted

## 接受依据

2026-08-26 依据当前持续 Goal“继续推进 guide 和分阶段实施 specs，先审计 roadmap，再根据
依赖图推进”的站立授权接受。本 ADR 只决定 pointer-like owner 的 nullable SSA/LLVM 表示，
不改变现行 nullable、smart cast、所有权或 Rc 源码语义。

## 背景

frontend 已有一般 `T?`、`null`、null 比较与 smart cast；Phase 4 仍没有 nullable SSA 类型或
lowering。ADR-0015 已规定 `Rc<T>?` 使用 null pointer niche，但不能简单把 `Rc<T>?` 映射成
`Rc<T>`：两者的非空不变量、drop glue 与 verifier identity 不同，smart cast 也不能凭一次裸
pointer load制造第二个 MoveOnly owner。

该问题同样适用于普通 class、Box 和其他 pointer-like handle，因此应先建立通用 nullable
handle 层，再由 Rc 复用，而不是在 SharedOwner LLVM adapter 中加入未验证的 null 特例。

## 决策

### SSA 类型与所有权

- typed SSA 新增 `NullableHandle<inner>`，首个版本只接受已有 pointer-like `HeapOwner`、
  `SharedOwner` 与后续明确批准的内部 reference；inline nullable/tagged 表示不由本 ADR 决定。
- `NullableHandle<inner>` 的 ownership 与 inner owner 一致。包装非空 owner 是 Value delivery，
  消费 inner 并产生唯一 nullable owner obligation；null constant 直接产生 nullable owner。
- nullable handle 与 inner handle 保持不同 `SsaTypeId`，operation/verifier 不允许二者隐式互换。

### Operation 与控制流证明

- 提供显式 `NullableWrap`、`NullableNull`、`NullableIsNull`、`NullableTake` operation；wrap/take
  产生或转移 owner obligation，is-null 只读，take 消费 nullable 并在 null 时走显式 abort/
  failure edge。
- 非消费 smart cast 不产生 owned inner handle。nullable 条件分支在 non-null edge 发布一个
  绑定到原 nullable owner 的 non-owning reference/view；该 view 只能执行 read、projection、
  retain/share 等不消费 inner 的操作，并由 alias/loan verifier 阻止 owner 提前 move/drop。
- typed SSA 用专用 `NullableBranch` terminator 表达证明边界：null edge 使用普通参数交付，
  non-null edge 的专用 target 最后一个 block parameter 是 `Loan(Shared, inner)` view，且该
  target 不接受普通 branch 或 null edge 前驱。`NullableTake(owner, proof)` 必须消费同一 owner
  派生且仍 active 的 view；普通 Borrow 参数、另一 owner 的 view 或类型相同的裸 loan 都不能
  充当非空证明。
- null/non-null flow fact 必须来自 typed condition descriptor或显式 nullable operation，lowering
  不重新解释 AST 拼写；join 后不保留只在单一 edge 成立的 non-null view。

### LLVM ABI 与 drop

- `NullableHandle<inner>` 与 inner handle 都 lower 为同宽 pointer；null 使用全零 pointer，非空值
  不加 tag、wrapper allocation 或 retain。
- nullable drop 先测试 null；null 分支无操作，非空分支调用 inner 的既有静态 drop glue。Rc
  因而只在非空时 release，普通 HeapOwner 只在非空时 free。
- wrap、non-null view 与 take 不改变 pointer bits；LLVM 表示相同不意味着 SSA 类型或所有权
  identity 可以合并。
- target preflight 验证 pointer width/alignment，LLVM 正反测试锁定无额外 allocation/tag，且
  verifier 必须先于 LLVM 拒绝非法 unwrap、重复 drop 与 view 逃逸。

## 替代方案

### nullable 与 inner 复用同一 SSA 类型

不采用。它会丢失非空不变量，使普通 owner drop 对 null 调用 free/release，并让 smart cast
看似凭空复制 MoveOnly owner。

### 所有 nullable 一律 `{tag,payload}`

不采用作为 pointer-like handle ABI。它浪费已确定的 null niche，并与 ADR-0015 冲突；inline
nullable 的表示可由后续 ADR 单独决定。

### non-null 分支产生第二个 owned handle

不采用。smart cast 是视图收窄，不是 copy/retain；Rc 只有显式 `.share()` 才能产生新 owner。

### 只在 LLVM 中插入 null check

不采用。LLVM 无法补回 frontend/SSA 缺失的类型、owner obligation、edge proof 与 drop 语义。

## 后果

收益：

- Rc/class/Box 可共享同一 null-niche、flow proof 与 conditional drop 基础；
- smart cast 不隐式复制 MoveOnly owner，nullable 与 non-null identity 对 verifier 可见；
- 运行时仍是单 pointer，无额外 allocation、tag 或 retain。

代价与风险：

- SSA 需要 path-sensitive non-null view/alias state与新 terminator/operation 验证；
- 仅完成 pointer-like nullable，不代表 inline value/enum/function nullable 已 lower；
- `!!`/Elvis/when nullable 分支需要按各自 frontend flow descriptor分阶段接入。

## 关联

- 首个实施 Spec：SPEC-0196
- 解锁：SPEC-0045 原 nullable Rc 验收及后续 class/Box nullable native lowering
- 相关 ADR：[ADR-0006](./0006-typed-ssa-block-parameters.md)、
  [ADR-0008](./0008-internal-value-and-allocation-abi.md)、
  [ADR-0015](./0015-shared-owner-runtime-abi.md)、
  [ADR-0016](./0016-interprocedural-borrow-abi.md)
- 取代的 ADR：无
- 被以下 ADR 取代：无
