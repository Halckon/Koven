# ADR-0006: typed SSA 使用 block parameters 与显式所有权效果

## 状态

accepted

## 接受依据

用户在当前持续 Goal 中要求继续推进 guide 主线并分阶段实施 Specs，站立授权仍有效。本 ADR
只决定现行 guide 已要求的自建 typed SSA 在 guide 留白处采用何种 CFG/值/验证结构，不改变
Koven 源码语法、类型、所有权语义或 Phase 边界。

## 背景

现行 guide 固定了 `源码 → frontend → 自建 SSA IR → LLVM IR` 的流水线，并要求 Phase 4
显式表达 move、borrow、drop 与正常控制流上的唯一消费。`lang-codegen` 当前只依赖
`lang-frontend`，尚无 IR；frontend 的 `TypeId` 只在一次 `TypedFile` 内有效，所有权阶段另以
AST/Symbol identity 发布 call、loan、drop、capture 和 deferred facts。若 SSA 直接复用 AST
节点、LLVM 类型或临时 mutable-local 表示，后续 verifier、优化与 LLVM lowering 会共享不清晰
的隐式状态，也无法独立证明 CFG、类型和线性所有权不变量。

传统 LLVM IR 用 PHI 指令表达控制流合流；MLIR 的官方 Language Reference 则用 block
arguments，并由 branch successor operands 传值，减少 PHI 的特殊规则。Koven 采用后一种
模式，但只借鉴结构，不引入 MLIR/Cranelift 依赖或其默认语义。LLVM 官方文档也把 verifier
失败视为 transformation/input 的编译器 bug；Koven 同样把无效内部 SSA 与用户源码诊断分离。

## 决策

### IR 所属与身份

- 自建 SSA 位于现有 `lang-codegen` crate 的 `ssa` 模块；不新增 workspace crate，不让
  `lang-frontend` 依赖 codegen，也不在本 ADR 引入第三方 IR 依赖。
- module、function、block、instruction、value、type、place 和 loan 使用各自索引式 newtype
  ID 与有序 `Vec` 存储。ID 只在所属 module/function 内有效；输出与验证顺序不得依赖随机
  hash 迭代。
- SSA 使用自身的规范化 `SsaTypeId` / type table。lowering 从 frontend `TypeId`、单态 callable
  instance 和能力事实建立映射；IR 完成后不能把 map-local frontend `TypeId` 或 LLVM type
  当作长期类型身份。SSA type 保持 target-independent，size/alignment/DataLayout 延后到
  SPEC-0035/0036。
- 每个 source-lowered definition、instruction 和 terminator 保留真实 `Span`；合成节点保存
  锚定真实 Span 的 synthetic origin，不伪造源码范围。编译单元继续持有对应 `SourceMap`，
  IR 不保存宿主绝对路径。

### CFG 与值模型

- function 是单入口、可多出口的 basic-block CFG。每个 block 具有有序 typed parameters、
  有序 instructions 和恰好一个显式 terminator；禁止隐式 fallthrough。
- terminator 至少封闭表示无条件 branch、条件 branch、return、abort/unreachable。每条 successor
  edge 显式携带与目标 block parameters 数量、顺序和类型完全一致的 arguments。
- function 参数就是 entry block parameters；普通 instruction results 与 block parameters
  都是只定义一次的 SSA values。instruction 可以有零个或多个有序结果，以承接后续结构化
  解构而无需再引入平行值表。
- block parameters 取代 PHI。Copyable 且被 definition 支配的纯值可以直接跨 block 读取；
  MoveOnly owned value、place capability 和有效 loan 只要跨 CFG edge，就必须显式作为该 edge
  argument 转移到 successor block parameter，不能依赖隐藏的 live-in 集合。
- 同一 MoveOnly value 可以出现在条件 terminator 的互斥 successor edges 上，因为运行时只
  选择一条 edge；每条实际路径仍只能交付一次。join 后只使用新的 block parameter identity。

### 类型与所有权效果

- operation/terminator contract 明确区分普通 read、owned consume、copy、shared/exclusive
  borrow begin/end、place mutation、drop、return 和 edge transfer。不得用通用“读取 operand”
  隐藏会结束 owner 或 loan 的效果。
- `Copy` 只接受 frontend 已证明 `Copyable` 的类型；Copyable 值可重复 read 且不产生唯一 drop
  义务。MoveOnly owned value在每条可达正常路径必须恰好被 consume、return、edge-transfer
  或显式 `Drop` 一次，之后不可再使用。
- borrow 以显式 loan identity/effect 表达；owner/place 与有效 loan 的冲突规则来自 frontend
  已批准事实，SSA verifier 只复核 lowering 后的 begin/end、overlap 和跨 edge 状态，不发明
  更强的 Rust NLL。
- Phase 3 `DropFact` 决定 lowering 插入 drop 的语义位置；SSA 不自行重新计算源码 liveness。
  verifier 负责证明已插入的 consume/drop 在正常 CFG 路径上完整且唯一。abort/unreachable
  没有异常展开或隐式 cleanup edge。
- effectful instructions 在 block 内的有序位置就是求值/提交顺序；优化只有在保持可观察效果、
  ownership 与 drop 顺序时才能重排。SSA 不把调用、store、drop 或 abort 当作纯表达式。

### Verifier 与错误边界

- verifier 不信任 builder，按固定阶段检查：ID/owner 范围与唯一归属；entry/terminator/CFG
  结构；successor arity/type；definition-before-use 与 dominance；operation/return contract；
  最后执行 owned value、place 和 loan 的路径数据流验证。
- non-entry block 的每个 parameter 必须由全部 predecessor edge 提供；function return operands
  必须与签名精确一致。跨 function/module ID、错误 type owner、悬空 successor 或 result 重定义
  都必须作为结构错误返回，不能 panic。
- verifier 返回稳定、结构化的内部 `VerifyError`，定位 function/block/instruction/value ID 和
  source origin；它不占用 `Lxxxx` 用户诊断码。pipeline 只在 frontend 无 error、无阻塞
  deferred fact 后 lower；若 lowering/transform 产生无效 SSA，这是编译器错误。
- SPEC-0033 先实现模型、确定性 debug rendering 和 verifier，不实现 AST lowering、优化、
  LLVM、layout、ABI、runtime、链接或可执行文件。每个后续 SSA transform 必须在窄测中对输入
  和输出运行 verifier。

### 公开边界与演进

- `lang-codegen` 对外围 crate 只公开后续编排需要的 lowering/codegen 入口和稳定结果；raw
  mutable IR builder、表内部与 verifier 辅助保持 crate-private，单元测试就近构造非法 IR。
- IR 文本仅是确定性 debug/测试表示，不在本阶段成为持久缓存、序列化格式或用户协议。
- 后续 Spec 可以增加 operation/type variant，但必须保持上述 ID 所属、block parameter、
  显式效果、source origin 和 verifier 边界；改变这些不变量需要新 ADR 取代本记录。

## 替代方案

### 直接以 LLVM IR 作为唯一 SSA

拒绝。它会在 ownership/drop 和目标布局尚未 lower 前泄漏 LLVM 类型及 PHI 规则，使 frontend
语义验证依赖后端，并违反现行 guide 的“自建 SSA → LLVM”阶段边界。

### 使用 PHI instruction

不采用。PHI 可以正确表达 SSA，但它对 predecessor/value 配对、block 顶部位置和并行赋值有
额外特殊规则。block parameters 让 edge transfer、参数 arity/type 与 MoveOnly 所有权交付使用
同一结构。[MLIR Language Reference](https://mlir.llvm.org/docs/LangRef/#blocks) 也采用该模式。

### 保留 mutable locals，晚期再转 SSA

拒绝作为长期主 IR。它能简化首版 lowering，却会把 assignment、move-after-use、branch merge
和 drop 义务留在隐式数据流中，之后仍需第二套转换和 verifier。局部 lowering builder 可以
短暂维护映射，但提交到 module 的产物必须已经是 SSA。

### 依赖通用第三方 IR 框架

暂不采用。MLIR/Cranelift 等模式可借鉴，但引入其完整类型、builder、native 依赖或默认 effect
模型的成本超出当前最小 Goal，也不能替代 Koven 的所有权/drop 契约。未来若复用价值超过
adapter 与供应链成本，需另做依赖准入和 ADR。

### 只依靠 builder 保证合法，不提供 verifier

拒绝。lowering、优化和 LLVM 映射都会重写 CFG/operands；缺少独立 verifier 时，内部损坏会在
更晚阶段变成难定位的 panic 或 LLVM 错误。LLVM 自身也提供 verifier，并明确把其失败视为
IR 生产者的 bug：[LLVM Language Reference](https://llvm.org/docs/LangRef.html)。

## 后果

收益：

- frontend/SSA/LLVM 的类型和责任边界明确，LLVM/DataLayout 不反向污染语言语义；
- block edge 同时表达控制流合流和 MoveOnly/loan 转移，避免隐藏 PHI/活跃 owner 状态；
- verifier 能在进入 LLVM 前稳定定位编译器内部的 CFG、类型、dominance 与唯一消费错误；
- 后续 scalar、aggregate、container、closure lowering 共用一套可扩展的索引式 IR 不变量。

代价与风险：

- IR-local type table 和 frontend→SSA 类型映射增加一层显式转换；
- MoveOnly/loan 的路径 verifier 比普通 use-def verifier 更复杂，必须按小矩阵分阶段实现；
- block parameters 会让 branch edge 携带更多显式 arguments，debug IR 更冗长；
- 本 ADR 不解决 DataLayout、ABI、LLVM 版本、runtime 或无限单态实例图，相关 Spec/ADR 仍是
  后续门禁。

## 关联

- 相关 Spec：SPEC-0033–SPEC-0038、SPEC-0177、SPEC-0174
- 参考模式：[MLIR blocks](https://mlir.llvm.org/docs/LangRef/#blocks)、
  [LLVM verifier 边界](https://llvm.org/docs/LangRef.html)
- 相关 ADR：[ADR-0002](./0002-bootstrap-workspace-layout.md)、
  [ADR-0003](./0003-diagnostic-architecture.md)、
  [ADR-0004](./0004-source-span-position-model.md)
- 取代的 ADR：无
- 被以下 ADR 取代：无
