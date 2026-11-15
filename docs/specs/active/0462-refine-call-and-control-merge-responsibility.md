# SPEC-0462：Refine 调用与 CFG 汇合职责归位

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：本批表达式调用与控制流汇合职责整理时 · **唯一真源**：本批范围与验收，语言语义从属 Guide

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P1-0462` |
| 所属 Phase | Phase 4，refined CFG及既有lowering测试路径 |
| 语言规范 | 现行 [Guide](../../guide/README.md)，无语义变更 |
| 批准依据 | 用户要求继续批量推进；沿fix/spec-0456与PR #433，不另开PR |
| 前置 Spec | [0461](0461-model-error-and-numeric-source-identity.md)，批次冻结输入636c56d0；本片前先有0461本地提交 |
| 前置 ADR | [ADR-0006](../../adr/accepted/0006-typed-ssa-block-parameters.md)、[ADR-0016](../../adr/accepted/0016-interprocedural-borrow-abi.md) |
| 影响范围 | refine/build/expression.rs、lower_frontend/control.rs及私有领域模块、对应policy |
| 语言语义变更 | 否 |
| 阻塞项 | 无 |

## 1. 两片完整职责

1. expression.rs的完整Builder::call移入私有expression/call.rs。receiver先于显式实参、
   source求值与parameter重排、Borrow/Inout/Value路径、pending loans/delivered、
   non-completing receiver/argument、call/invoke/abort、逆序end-loan、writeback与所有drop
   point顺序不变。只将方法fn改为child的pub(super)，一处receiver相对路径增加父层，
   恢复原expression私有域；不分割长方法、不新增调用路径或新状态。
2. control.rs的MergeSlot/MergeProjection及完整IncomingRole impl、merge_exits与
   merged_value_type整体移入私有control/merge.rs。根保留BranchExit/LinearBindings、
   map_frames/公共method门面和原free helper；MergeSlot在child pub(super)并由根私有use，
   保持map_frames既有同级间接consumer。merge_exits原pub(in crate::ssa)有效域不变。
   columns去重/顺序、map角色、proof/pending field验证与重绑、单入边和空入边、loan结束
   与错误顺序原样保持，不补新语义，不复制算法。

根原imports保留供后代/间接consumer使用；无wrapper、扩大可见域、allow或额外clone。
测试/fixture/SourceId/Span/诊断身份/FQN不改。两根与新文件均不超1000，只删除相应旧额度。
不将只在测试配置使用的lower_frontend路径误称生产路由变化，也不把call拆分当新准入。

## 2. 节奏与验收

- [ ] R1：完整函数/类型块机械保全（仅等价路径/有效域调整），原根剩余代码与全部测试不变。
- [ ] R2：逐片默认直接族与相关native基线→迁移→通过，各自本地提交；不逐片push或重复整套门禁。
- [ ] R3：四片成组统一双配置组合与真实native消费者、workspace check、双Clippy、fmt/docs/尺寸/policy。
- [ ] R4：独立完整精确提交区间审查，有finding修后复核；Architecture/账本同步，一次集中push同PR。
- [ ] R5：官方精确新头CI另验，保持Spec active与PR Draft，无本轮合并授权不归档或合入main。

原始日志及源块快照留仓库外。仅GREEN→REFACTOR→GREEN，无行为变化不伪造RED。
不按文件数量强删34项合理保留，不扩展receiver/borrow/nullable或E2/D10能力。

## 3. 真实证据

待逐片记录；本地定向证据不等于完整codegen、CI-only fault、真实变异校准或双宿主CI。
