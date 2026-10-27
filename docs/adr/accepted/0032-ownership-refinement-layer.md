# ADR-0032：compilation-unit lowering 的所有权细化层

> **性质**：架构决策记录 · **状态**：accepted · **读取时机**：设计或审查 E3 通用 lowering、细化 IR 与 SSA 构造时 · **唯一真源**：本 ADR

## 状态

accepted

## 接受依据

2026-10-23 用户明确确认：以 nullable 家族试点路线图 E3；终态统一使用细化层；迁移期按函数体
是否全在已迁移子集内确定性分流、逐家族迁移；已迁移家族的旧模块不留兼容层。

## 背景

D10（SPEC-0341）正在把单文件与 project 收敛到同一条 compilation-unit 管线：独立 single 门面、
旧 native 门面已经退役，私有 `lower_frontend` 与旧 checker 只剩测试消费者，按“不保双轨、
不留错误回退”的方向继续迁移删除。D10 解决的是“走哪条管线”，没有改变唯一保留的
`ssa::unit_lower` 的构造方式。

`unit_lower`（约 4.75 万行）直接从 AST 发射 SSA。函数体状态散落在 `bindings`、
`pending_operands`、`pending_call_frames`、`temporaries`、`nullable_proofs`、
`result_source_loans` 等表中，每个控制流构造都要手工把这些状态带过 CFG 边并在合流处对齐
（`carry_pending_operands`、`carried_loans`、`merge_unit_exits` 等）。类型 × 控制流 × 所有权
组合因此各自需要专用模块（`nullable_proof`、`nullable_when`、`while_proof`、
`map_iteration` 等）。2026-10-23 的 nullable 家族实测中，`if (c) Node(5) else null` 失败在
`merge_unit_exits` 的 MissingFact，`Int?`/`String?` 失败在类型 intern：前者是合流状态手工对齐
的结构性问题，后者是表示缺失（见 [ADR-0033](0033-inline-nullable-representation.md)）。

frontend 已发布 lowering 需要的全部语义事实：source-qualified drop 点（`UnitDropPoint`：
`AfterExpression`、`BranchExit`、`LoopExit`、`ControlTransfer`、`CallReturn` 等）、loan 的开始与
结束、Value delivery、receiver 合同、null comparison 边事实与非空 use。缺的是把这些事实机械
落实为显式指令、再统一构造 SSA 的中间层。

## 决策

### 两段式：细化，再构造 SSA

1. **细化（refine）**：按函数实例，从 AST、validated typed facts 与 ownership facts 生成
   `RefinedBody`。它是基于槽位（slot）的 CFG：
   - 槽位有 named binding、表达式 temporary、receiver 与 loan 四类，各带 SSA 类型和 Copyable/MoveOnly 分类；
   - 语句只有显式的 `Init`、`Assign`、`Move`/`Copy` 读取、`Drop`、`LoanBegin`/`LoanEnd` 与普通运算；
   - terminator 为 `Goto`、`Branch`、`SwitchNull`、`SwitchTag`、`Return`、`Abort`。

   每条 frontend drop、loan 与 delivery 事实恰好落到一个 CFG 位置，缺失或多余都是 `MissingFact`/`InvalidModel`。
   细化不选择语义：哪个值在何处 drop、哪些 use 已被证明非空，全部来自事实。
2. **SSA 构造**：通用算法把槽位改写为 SSA 实体，在合流块插入 block parameter（按 Braun
   等人的 sealed-block 构造）。每个槽位在每个块入口的所有权状态（已初始化/已移出/loan
   active）必须在全部前驱上一致；不一致说明事实与 CFG 不符，报 `InvalidModel`，不猜测修补。
   构造只做机械映射：`Move` 消费当前实体、`Drop` 生成 drop 操作、`LoanBegin`/`LoanEnd`
   对应现有 loan 操作。产物照常经过现有 SSA verifier 再进入 LLVM。

### 与现有 lowering 的关系

- 公开入口不变：仍是 `lower_owned_unit_with_entry` 及常量 view 入口；`unit_plan`、
  callable ABI、类型 intern、LLVM adapter 与 verifier 复用，不新增 public API 或 crate。
- 细化层是终态的唯一函数体 lowering。迁移期间，入口对每个函数实例先做**确定性子集预检**
  （只看 typed facts 与 AST 构造种类，不试跑）：函数体全部落在已迁移子集内就走细化层，
  否则走现有 `unit_lower` 函数体路径。细化层一旦接手就不回退；失败即报错。这不是
  “先试新、失败再走旧”的错误回退，因为分流在 lowering 之前、按固定规则完成。
- 每迁移一个家族，同一 Spec 删除被它完全取代的 `unit_lower` 专用模块和 carry/merge 分支，
  并把子集表扩大；全部家族迁完后预检和旧函数体路径一起删除。
- 不触碰 D10 范围：私有 `lower_frontend` 与旧 checker 的退役仍由 SPEC-0341 负责；
  已迁到 Unit 入口的测试继续有效，因为入口不变。

### 不变量

- 细化与构造都不读取 AST 拼写来推导所有权或 nullable 事实；只用 AST 确定求值顺序和 CFG 形状。
- 同一函数实例的输出确定：槽位按源码顺序编号，块按构造顺序编号，不依赖哈希迭代。
- 两模式（单文件与 project）进入同一入口，因此天然一致。

## 替代方案

### 方案 A：继续按家族扩充 `unit_lower`

不采用。每个新组合都要再写一套 carry/merge 与 proof 规则，路线图 P2 的十个家族会继续
按组合增长；E3 的验收条件正是不再新增按组合拆分的 lowering 模块。

### 方案 B：新层只在测试中与旧路径对照，覆盖完整后整体切换

不采用为主路线。切换前 probe 不会变绿，而且两套实现长期并存的时间更长；迁移期的预检分流
同样能逐家族验证，并在每一步删除旧代码。

### 方案 C：由 frontend 产出细化 IR

不采用。细化 IR 带有 SSA 类型、ABI 槽位与 loan 物化，属于 Phase 4 输入，应留在
`lang-codegen`；frontend 继续只发布语义事实。

## 后果

收益：

- 合流状态由通用 SSA 构造保证，不再逐构造手写 carry/merge；
- 每类所有权事实只有一个落点，缺失事实在细化阶段就能定位到具体 drop 点；
- 每个家族迁移都伴随删除旧模块，代码量和组合数单向下降。

代价与风险：

- 迁移期存在两种函数体 lowering，预检规则必须精确、确定并有测试锁定；
- SSA 构造必须保持 verifier 现有的 loan 与 owner 不变量，早期需要逐家族对照 IR；
- 一些现有专用 proof（如 Map 运输、Rc while）迁移时可能暴露事实缺口，需要在 frontend 补事实而非在 codegen 推断。

## 关联

- 相关 Spec：[SPEC-0350](../../archive/specs/0350-ownership-refinement-nullable-pilot.md)（E3 nullable 家族试点）；[SPEC-0356](../../archive/specs/0356-refine-when-family.md)（`when` 家族与 Copyable enum `is`）；[SPEC-0358](../../archive/specs/0358-refine-assignment-inout.md)（赋值与 `Inout`）；[SPEC-0359](../../archive/specs/0359-refine-rc.md)（`Rc`）；[SPEC-0360](../../archive/specs/0360-refine-generic-value-class.md)（泛型实例与 value class）；SPEC-0341（D10，管线统一）
- 相关 ADR：[ADR-0006](0006-typed-ssa-block-parameters.md)、[ADR-0017](0017-nullable-handle-ssa-abi.md)、[ADR-0033](0033-inline-nullable-representation.md)
- 取代的 ADR：无
- 被以下 ADR 取代：无
