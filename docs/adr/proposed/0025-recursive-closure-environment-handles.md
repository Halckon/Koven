# ADR-0025：递归 closure 环境使用独占实例句柄

> **性质**：架构决策记录 · **状态**：proposed · **读取时机**：设计递归 closure 环境的 SSA、native ABI 或 drop 时 · **唯一真源**：本 ADR

## 状态

proposed

## 接受依据

不适用。2026-09-23 用户已决定保留 v0.37 对递归 owned capture 的合法性；以下 native
表示尚未接受，也尚未实现。

## 背景

`f = move { f() }` 每轮把旧 `f` 环境移入新环境。静态 lambda 来源图只有有限个节点，
运行时却可形成任意深、每次执行有限的独占环境链。ADR-0009 的内联 concrete environment
会让同一 lambda 的环境类型按值包含自身，无法得到有限布局。按静态环截断 capture 会提前
释放旧 owner；按当前 phi 槽查找旧环境会把回边错误地指向新实例。

SPEC-0211 第 9 节先规定 Phase 3 的有限来源图、实例关系及事实回放；本记录只提议后续
Phase 4 如何在 native 中承载这些事实。SPEC-0182 首轮 named、Borrow、temporary source
不依赖本 ADR。

## 决策提议

- 捕获 closure 的每次形成创建一个独占环境实例。静态 layout 仍按 lambda 和单态类型确定，
  但 closure value 携带固定大小的 code、environment、drop 句柄；环境中的 owned closure
  capture 保存被移动实例的句柄，不内联其环境。无捕获函数值继续使用裸函数指针。
- 环境实例通过现有 `lang-codegen` runtime allocation adapter 分配；分配大小和对齐来自该
  concrete lambda layout。分配失败沿既有 abort 契约处理。环境没有引用计数、运行时类型
  查询或循环收集；所有权由 Phase 3 的 move/loan/drop facts 决定。
- 每个 concrete lambda 生成调用 thunk 和 drop thunk。调用 thunk 从句柄取得本 lambda 的
  环境，按已验证签名调用 body，不取得环境所有权。drop thunk 对实际实例按 capture 逆序
  释放 owned 字段，shared 字段不释放其来源，最后释放本实例 storage；嵌套
  closure 字段使用其保存的 drop 句柄递归释放。drop 句柄不得从当前 phi selector 或 AST
  重新推导。shared capture 的 loan end 仍由 Phase 3 事实指定；函数类型相同而 lambda
  layout 不同的合流也须保存实际 drop 句柄。
- typed SSA 把环境实例句柄和 closure owner 身份分别建模。形成、move、snapshot、phi 入边
  都运输 code/environment/drop 三者的一致组合；phi 先读取全部旧来源，再并行写目标，
  缺席来源清空目标。捕获边在形成时固定指向旧实例，后续覆写 binding/phi 不会改写边。
  verifier 拒绝没有环境实例的捕获 closure、重复消费同一实例、以及与已验证 capture facts
  不符的调用或释放。
- phi 只搬运根 closure 句柄及其选择快照；已形成环境的 owned 子句柄留在该环境的 capture
  槽中。访问或释放后代时以实际父实例句柄和 capture 位置取子句柄，同一静态 lambda 的
  两个存活子实例不得因共享图节点或 phi 布局槽而合并。迁移现有按静态来源树展开的
  captured `DropFact` 时，必须避免它与实例 drop thunk 对同一子环境重复析构；shared
  capture 的 loan end 与最后借用者判定仍由已验证的实例关系事实驱动。现有静态
  `EndCaptureLoan`、`TestLastCaptureLoan` 和 captured `DropFact` 目标不足以区分同节点的
  两个子实例，须由 Phase 3 的可解析实例边事实替换或扩展后才能启用此 ABI。
- 静态来源图可有环，动态 owned 边只指向形成前已有的实例，因此实际实例图无环。
  Phase 3 必须先发布有限图、实例形成/捕获/转移/释放关系并通过逐实例回放；在此之前
  `RecursiveClosureCapture` 保持 deferred，native lowering 不消费截断事实。

## 替代方案

### 保留内联环境，仅对递归边加指针

需要让同一 closure value 在不同来源路径混用内联和间接布局，且间接边的 lifetime 与
drop 仍需独立协议。当前没有能在 phi、return 和字段存储中验证这种混合表示的事实。

### 捕获旧 phi 槽的地址

下一轮会覆写该槽，旧 closure 将观察到新环境或已释放的 storage，违背形成时 snapshot。

### 引用计数或循环收集

实际 owned 链无环且只有单一 owner，引用计数和 GC 不提供额外正确性，却引入新的运行时
状态与 drop 时序。

## 后果

收益：固定大小句柄让递归及交替捕获的环境布局有限；每个实际 capture 边和 drop 目标可随
值转移，不依赖循环执行次数或静态树展开。

代价与风险：捕获 closure 形成需要分配；所有现有内联环境 SSA/LLVM 测试和 ABI 使用点须
迁移。接受本 ADR 时须同时取代 ADR-0009 的捕获 closure 内联环境及“不得隐式分配”决定，
并验证 shared capture storage 的实际寿命、函数值合流和各正常退出恰好一次释放。

## 关联

- 相关 Spec：[SPEC-0211](../../archive/specs/0211-sequential-iteration-ownership.md)、
  [SPEC-0182](../../specs/active/0182-sequential-for-lowering.md)
- 相关 ADR：[ADR-0008](../accepted/0008-internal-value-and-allocation-abi.md)、
  [ADR-0009](../accepted/0009-concrete-closure-internal-abi.md)
- 取代的 ADR：无（proposed；接受时须明确 ADR-0009 的取代关系）
- 被以下 ADR 取代：无
