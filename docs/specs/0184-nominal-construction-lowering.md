# SPEC-0184：名义构造与所有权 facts 到 SSA/LLVM lowering

| 字段 | 值 |
|---|---|
| 状态 | `draft` |
| Goal ID | `KOV-P4-184` |
| 所属 Phase | Phase 4 |
| 语言规范 | 现行 [v0.29 §29](../guide/01-design-decisions.md#29-名义enum-case-与-intrinsic-box-构造v029) |
| 批准依据 | 用户于 2026-08-25 明确启用 v0.29；实现状态仍按本 Spec 推进 |
| 前置 Spec | SPEC-0035、0039、0185、0186 `done`；SPEC-0183、0188 `done` 后方可实施 |
| 前置 ADR | [ADR-0008](../adr/0008-internal-value-and-allocation-abi.md) `accepted` |
| 关联 ADR | [ADR-0006](../adr/0006-typed-ssa-block-parameters.md)、[ADR-0007](../adr/0007-llvm-toolchain-and-first-target.md) |
| 阻塞项 | SPEC-0183/0188 未完成 |
| 影响范围 | `lang-codegen` frontend→SSA、SSA enum/value operations、LLVM adapter、L0145、native tests；Architecture、Roadmap |
| 语言语义变更 | 否；实现已启用 guide、ADR-0008 与 frontend facts，不反推源码语义 |

## 1. Goal

完成后，已通过 frontend 的 value class/class/enum case/intrinsic Box 构造、字段投影、完整
结构解构与正常路径 drop facts 可经 verified SSA lower 为 ADR-0008 的内联聚合、heap owner、
tagged payload 和 Box 表示，生成并运行真实本机 object；目标布局失败形成 L0145 而不是 LLVM
错误、panic 或隐式 boxing。

## 2. 背景

SPEC-0035 已建立 aggregate/class/Box SSA 与 LLVM 基元，ADR-0008 已确定 enum tag/payload、
class/Box allocation 和递归 drop/free，SPEC-0186 已建立 IR-local target layout preflight。
这些测试目前由手工 SSA 驱动；缺少 SPEC-0183/0188 的源码 target、实例和所有权 facts 时，
不能宣称 nominal source 已完成 codegen。

## 3. 范围与需求

- frontend adapter 只消费匹配同一 analysis chain 的 construction/ownership descriptor；不存在
  descriptor、存在 frontend diagnostic、foreign `TypeId`/symbol 或不完整 drop plan 时在写盘前
  返回结构化 lowering error。
- 单态化 value class 按字段声明顺序映射为 named inline aggregate；普通 class 按 payload 字段
  顺序建立非空 heap owner；intrinsic Box 分配一个实际 value-class payload。不得因大小、
  Copyable 或调用 ABI 自动 Box/clone/retain。
- enum root 使用 ADR-0008 的源码顺序零起 tag 与最大 payload storage；每个 case construction
  写入唯一 tag 和该 case payload，投影/drop 先由已知静态/flow case fact 选择合法 payload，
  不引入通用 RTTI、niche 或动态 interface。
- construction operand 的 SSA evaluation/transfer 顺序来自 SPEC-0188 的 ordered delivery
  effects；聚合槽位使用 SPEC-0183 参数声明顺序。正常路径只消费 frontend 发布的 root drop
  obligation，再按完整单态 result type 生成递归字段/payload drop glue，保证 root 及其资源恰好
  消费或析构一次；不要求 frontend 伪造逐字段源码 DropFact，abort 不 unwind。
- 字段投影接入既有 place/load/loan；完整 value-class destructuring 使用单次 aggregate explode，
  Copy descriptor 保留源值，Consume descriptor 转移全部分量。普通字段仍不支持 MoveOnly 部分移出。
- 把 SPEC-0186 aggregate/enum payload/class/Box 的 size/alignment/stride 失败映射为 L0145，保留
  constructor/type use primary 与来源声明 label；frontend 仍不读取 target/DataLayout，native/codegen
  诊断桥使用 frontend 集中 catalog/model 构造用户诊断。失败不创建 LLVM module type、GEP、
  allocation 或旧 object。
- debug-enabled object 继续使用真实 source origin；显式 Unit entry 的真实 `.ko` fixture 经
  object/link/run 验证 value/class/enum/Box 构造、投影、解构与 drop 行为。

## 4. 非目标

- 不实现 constructor 选择/推导/所有权检查；不在 codegen 按名称、字段类型或 AST 猜测事实。
- 不实现 instance method receiver、接口委托转发、object/companion state、secondary constructor、
  user destructor、异常 cleanup、nullable niche 优化、Map、for 或标准库 Pair/Result/Rc/API。
- 不建立公开 FFI ABI、跨 target 承诺、优化 pipeline、隐式 allocation elimination 或新 crate。

## 5. 验收标准

- [ ] verified SSA 正反矩阵覆盖 value aggregate、class/Box heap owner、enum tag/payload、字段 place、
      Copy/Consume explode 与由单态 root type 派生的递归 drop；corrupt target/order/ownership plan
      被 verifier/adapter 拒绝，缺少逐字段 source DropFact 不是错误。
- [ ] debug LLVM/object 文本证明布局、tag、单次 malloc/free、无隐式 Box/retain/clone/unwind；ZST、
      空 class、无 payload enum 和 MoveOnly nested payload 均有边界测试。
- [ ] 真实 `.ko` source 经 frontend→verified SSA→LLVM object→Clang link/run，观察构造结果、分支
      case、投影/解构和正常退出；MoveOnly 资源恰好析构/释放一次。
- [ ] 超限 aggregate/enum payload/class/Box 产生 L0145 精确 Span，且 LLVM 类型、allocation、object
      均未产生；测试证明 L0145 由 codegen/native 桥接且 frontend 保持 target-independent，
      SPEC-0186 既有 IR-local preflight 回归通过。
- [ ] `lang-codegen`/frontend/CLI 受影响窄测及 workspace 五项标准基线通过；Architecture/Roadmap/
      Spec 只记录真实源码闭环，production 文件遵守 1000 行软上限。

## 6. 技术方案与边界

- 在 `lower_frontend` 门面下分别建立 nominal type mapping、construction lowering 与 drop planning
  模块，以 frontend nominal/enum/intrinsic instance key memoize SSA named type；constructor instance
  key 不进入 callable `instances` 函数可达图，声明 root 也不产生伪函数实例，只有可达
  construction/operation 触发类型与操作 lowering。
- enum 增加职责明确的 SSA tagged-payload operation 与 verifier 契约；LLVM adapter 在独立模块
  实现 payload storage，不把 byte offset/DataLayout 计算泄漏到 frontend。
- native facade 通过显式 codegen→frontend diagnostic bridge 把 SPEC-0186 layout error 映射为
  集中注册的 L0145；完整 diagnostics 返回后才调用 object emitter，保持 SPEC-0042 的失败不
  落盘边界，不把 LLVM/native error string 冒充 frontend 类型诊断。

## 7. 实施计划

1. [ ] 建立 frontend nominal type mapper 与 value/class/Box construction lowering → 验证：SSA/
   LLVM 窄矩阵和 verifier。
2. [ ] 增加 enum tag/payload SSA/LLVM、projection/destructuring/drop 接线 → 验证：case/ZST/
   MoveOnly 正反矩阵。
3. [ ] 映射 L0145 并完成真实 source object/link/run → 验证：失败不落盘和 native 行为。
4. [ ] 同步 Architecture/Roadmap/Spec，运行 workspace 基线 → 验证：全部实际退出码为 0。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | nominal/value/class/Box frontend→SSA→LLVM 接线 | `feat(codegen): lower nominal constructions (SPEC-0184)` |
| 2 | enum tag/payload、drop、L0145 与 native 闭环/完成文档 | `feat(codegen): lower enum constructions (SPEC-0184)` |

## 9. 未决问题

- 无设计留白；版本门禁与 SPEC-0183/0188 前置未解除。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-25 前置审计 | 等待前置 | ADR-0008、SPEC-0035/0186 已封闭后端表示/preflight；已明确 constructor key 不进入函数实例图、root-type-driven drop glue 与 codegen/native L0145 诊断桥；v0.29/0183/0188 尚未解除门禁 |
