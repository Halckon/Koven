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

- [x] R1：完整函数/类型块机械保全（仅等价路径/有效域调整），原根剩余代码与全部测试不变。
- [x] R2：逐片默认直接族与相关native基线→迁移→通过，各自本地提交；不逐片push或重复整套门禁。
- [x] R3：四片成组统一双配置组合与真实native消费者、workspace check、双Clippy、fmt/docs/尺寸/policy。
- [ ] R4：独立完整精确提交区间审查，有finding修后复核；Architecture/账本同步，一次集中push同PR。
- [ ] R5：官方精确新头CI另验，保持Spec active与PR Draft，无本轮合并授权不归档或合入main。

原始日志及源块快照留仓库外。仅GREEN→REFACTOR→GREEN，无行为变化不伪造RED。
不按文件数量强删34项合理保留，不扩展receiver/borrow/nullable或E2/D10能力。

## 3. 真实证据

两片接在0461本地提交后，未逐片push；最终48275563只整理本批两个根的三行额外空白。

| 切片 | 实际结果 |
|---|---|
| call | 14356ef9；完整refine_tests92、call_lifetimes3、receiver46、two_phase14与普通native direct-prefix5，前后 **160/0/0**、逐名相同；根1001→631，新call376 |
| control merge | fd62be6a；完整lower_frontend_tests287、while_flow_tests46及map scope_lowering1，前后各 **334/0/0**、逐名相同；根1321→912，新merge416；Map sibling consumer实跑 |
| 机械保全 | 4根/新文件完整块与剩余根、唯一helper及全部测试不变，仅明确scope/path及空白调整；两批policy只删model/control两项baseline和四项exceptions，其余JSON完全相同 |
| 独立审查 | 精确6edff77f..48275563完整九文件2180行patch，核call source求值/parameter槽/receiver/pending loan/deliver/失败cleanup/writeback/drop与merge role/column去重/proof/Map重绑和间接imports；无具体问题 |
| 最终组合 | 默认/feature各 **634/0/0**、同一634个FQN，所有过滤器命中；原127与四片去重519有12项交集，不按两数相加虚增，不新增测试或把搬迁算新覆盖 |
| 统一门禁 | workspace all-target check/Clippy、feature codegen all-target Clippy、fmt、policy47、docs959、尺寸/空白退出0；2156手写Rust，25历史超限（两批29→25） |

call首次基线155项实际通过，但旧native文件名过滤器零命中，仓库外runner因此失败，
不能作为完整native基线。源码未变时仅补正确direct_call_prefix_tests五项，再以不重叠
逐名并集160对迁移后160核对；保留首次日志/失败，不重复跑既有155或伪称产品用例失败。

首次control快照被read的50KB上限截断尾部，机械audit明确失败；改从14356ef9完整git blob
提取并保留旧partial，再核对所有剩余根完全相同。产品使用精确块edit，尾部从未丢失；
未把截断快照当完整内容审计。Architecture门禁首轮204>200失败，精简本批职责概述后
200行/docs959复验通过，Rust未变；最终其他门禁无需因纯文案重跑Cargo。

Architecture同步四个私有职责；原始日志/快照/完整diff与审查保存在仓库外。当前源树已
整合main c3ceea14，父636c56d0的CI43675343635最终success、双宿主/Preview及最终汇总
均通过（Editor skipped不计执行），只证明父候选，不替代本批尚待的新头CI。

本地未跑完整frontend/codegen/LLVM慢组、CI-only fault、真实变异校准或sanitizer全套；
不将列举/编译或mock单测当真实校准。R4的独立审查/Architecture已完成，余同PR集中push；
R5待精确新头官方验收，保持active/Draft、不归档或合入main。
