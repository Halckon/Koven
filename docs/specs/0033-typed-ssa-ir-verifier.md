# SPEC-0033: 最小 typed SSA IR 与 verifier

| 字段 | 值 |
|---|---|
| 状态 | `draft` |
| Goal ID | `KOV-P4-033` |
| 所属 Phase | Phase 4 |
| 语言规范 | 现行 [v0.28 Phase 4](../guide/06-roadmap.md#phase-4llvm-代码生成) 与适用的已实现 frontend 契约 |
| 批准依据 | 当前持续 Goal 的站立授权可在启动实现时批准；全部前置已完成 |
| 前置 Spec | SPEC-0021、SPEC-0029、SPEC-0177、SPEC-0174 `done` |
| 前置 ADR | [ADR-0006](../adr/0006-typed-ssa-block-parameters.md) `accepted` |
| 关联 ADR | ADR-0002、ADR-0003、ADR-0004 |
| 阻塞项 | 无；frontend 前置已完成 |
| 影响范围 | `lang-codegen` 自建 SSA model/verifier/debug rendering 与 crate 内测试；Architecture |
| 语言语义变更 | 否；只建立现行 guide 要求的内部 typed SSA，不新增源码行为或用户诊断 |

## 1. Goal

完成后，`lang-codegen` 拥有 target-independent、索引式、block-parameter 形式的最小 typed SSA
模型和确定性 verifier；后续 lowering/优化可以在进入 LLVM 前证明 CFG、类型、dominance、
MoveOnly 唯一消费、loan 与显式 drop 不变量。

## 2. 背景

frontend 已发布带 source `Span` 的类型、call、loan、drop、capture 与所有权产物，
`lang-codegen` 当前却只有空 library target。SPEC-0034 不能直接把 AST 临时映射成 LLVM IR：
现行流水线要求先有自建 SSA，后续 aggregate/container/closure lowering 也需要共享的 owner、
place、loan 和 drop 表达。ADR-0006 已决定采用 IR-local 类型、block parameters、显式效果和
独立内部 verifier。

## 3. 范围与需求

- 在 `lang-codegen::ssa` 内建立 module/function/block/instruction/value/type/place/loan/origin
  的索引式 model；所有 ID 验证所属 owner，不允许跨 module/function 混用。
- type table target-independent，最小封闭 Unit/Boolean/整数及可用于 verifier 的 Copyable/
  MoveOnly opaque semantic type；不得出现 LLVM type、size/alignment 或 ABI 属性。
- function 参数使用 entry block parameters；每个 block 有有序参数、instructions 和唯一显式
  terminator。branch/conditional branch edge 保存有序 successor arguments，return/abort 无
  隐式 fallthrough。
- 最小 instruction/effect 集覆盖常量、基础纯 scalar 运算、copy、owned consume、root place、
  shared/exclusive borrow begin/end、mutation 与 drop；每个 operation contract 显式声明 operand
  use kind 和有序 result types。
- MoveOnly owner、place capability和有效 loan 跨 edge 必须经 block parameter 显式转移；
  Copyable dominated value可直接跨 block read。互斥 branch edges 可以各自转移同一 owner，
  但任一实际可达路径只能消费一次。
- verifier 按 ADR-0006 的固定阶段验证 ID/归属、CFG/terminator、edge arity/type、use-before-def/
  dominance、instruction/return contract、owned/loan/drop 路径状态；错误稳定定位 IR ID 与
  source/synthetic origin。
- verifier 返回独立 `VerifyError`，不注册 `Lxxxx`。无效用户源码不得进入本阶段；无效 SSA
  是 lowering/transform bug，不允许 panic 或依赖 LLVM 才发现。
- 提供确定性 debug rendering 供单元测试和后续 golden 使用，但不承诺稳定序列化或公共文本协议。

## 4. 非目标

- 不从 AST/TypedFile/OwnershipCheckedFile lower SSA；由 SPEC-0034 及后续按领域承接。
- 不实现优化 pass、constant folding、dead-code elimination、LLVM/`inkwell`、目标 DataLayout、
  object/link、runtime、ABI、容器布局、closure layout 或 DWARF。
- 不决定无限泛型实例增长；SPEC-0177 只提供 instance recipe，实例图门禁由后续 lowering Spec
  在真正展开前封闭。
- 不建立用户可编辑/持久缓存 IR 格式，不新增 workspace crate或第三方依赖。
- 不重新推导 frontend 所有权语义，也不扩展为完整 Rust NLL。

## 5. 验收标准

- [ ] 合法 scalar function、diamond CFG、loop backedge、多个 return block 与 block parameter 示例
  通过 verifier，并具有稳定 debug text/source origin。
- [ ] entry/terminator、悬空/跨 owner ID、successor arity/type、result type、return type、
  use-before-def 与 non-dominating use 各有独立失败测试，错误定位和顺序确定。
- [ ] Copyable 值可重复 read/copy；对 MoveOnly 使用 copy、消费后再用、同一路径重复 consume/drop、
  正常 return 前遗漏义务均被拒绝。
- [ ] MoveOnly 值在互斥 branch edge 可分别 transfer，join 只通过 block parameter 使用；隐藏
  live-in、某条正常路径未消费或 edge 后继续使用均被拒绝。
- [ ] shared/exclusive loan 的合法 read、冲突 borrow/mutation/owner consume、显式 end 与跨 edge
  block parameter 状态均有正反例。
- [ ] verifier 对人工损坏 IR 返回 `VerifyError` 而不 panic，不产生 frontend `Diagnostic`；同一
  module 重复验证结果相同。
- [ ] `lang-codegen` 窄测及 workspace 标准基线通过，Cargo manifest/lockfile 无新增依赖。
- [ ] Architecture、ADR/Spec 索引和 roadmap 同步为实现后的事实。

## 6. 技术方案与边界

- `ssa/mod.rs` 作为 crate-private 门面；model、verify、render 按单一职责拆分，新生产文件遵守
  1000 行软上限。只有后续 CLI/codegen 编排真正需要的入口才从 crate 根 re-export。
- model 用私有字段和受检构造 API 建立正常 IR；verifier 仍不信任 builder。非法结构通过就近
  单元测试 helper 构造，不为测试扩大跨 crate 可见性。
- dominance 使用确定性 CFG 算法；ownership/loan 使用前向数据流并以 block parameter state
  作为显式 merge 边界。集合输出前排序或使用有序结构。
- verifier category 是内部 Rust enum，不复用 frontend DiagnosticCode；rendering 不包含机器
  绝对路径或随机 ID。

## 7. 实施计划

1. [ ] 建立 SSA ID/type/origin/function/block/value model 与 debug rendering → 验证：model 单元测试。
2. [ ] 实现结构、CFG、edge type 与 dominance verifier → 验证：结构/控制流正反矩阵。
3. [ ] 实现 Copyable/MoveOnly consume/drop 与 block-edge 数据流 → 验证：线性所有权矩阵。
4. [ ] 实现 root-place shared/exclusive loan begin/end 验证 → 验证：loan 冲突与跨 edge 矩阵。
5. [ ] 运行 `lang-codegen` 窄测和 workspace 标准基线，同步 Architecture/Spec → 验证：实际退出状态。
6. [ ] 检查 staged diff、manifest/lockfile 与文件规模 → 验证：提交只属于 SPEC-0033。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | SSA model、IR-local type/origin 与确定性 rendering | `feat(codegen): define typed SSA model (SPEC-0033)` |
| 2 | CFG/type/dominance verifier | `feat(codegen): verify typed SSA structure (SPEC-0033)` |
| 3 | ownership/loan verifier、Architecture 与 done 验收 | `feat(codegen): verify SSA ownership (SPEC-0033)` |

## 9. 未决问题

- callable reference、safe call 等仍在 frontend 明确 deferred 的节点不得伪造 IR；本 Spec
  不反向扩张 Phase 2 语义。

## 10. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-24 现状与边界审计 | 通过 | `lang-codegen` 只有空 crate；frontend 已提供 typed/ownership facts；ADR-0006 已封闭 IR 架构 |
| 实现验收 | 未执行 | frontend 前置已完成，等待启动本 Spec |
