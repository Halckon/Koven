# SPEC-0314: owned Sequential source 条件 return 证明

> **性质**：变更合同 · **状态**：in-progress · **读取时机**：实施或验收 owned source 条件 return 时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0314` |
| 所属 Phase | Phase 3 事实消费、Phase 4 single SSA/native |
| 语言规范 | [Guide12](../../guide/12-collections-destructuring.md) |
| 批准依据 | 2026-10-14 用户对 owned-source return 下一里程碑明确继续实施 |
| 前置 Spec | [SPEC-0312](../../archive/specs/0312-owned-source-conditional-continue.md)、[SPEC-0313](../../archive/specs/0313-single-map-conditional-return.md) |
| 前置 ADR | [ADR-0006](../../adr/accepted/0006-typed-ssa-block-parameters.md) |
| 阻塞项 | tests-only 红测、最小实现与完整有界验收待执行 |
| 影响范围 | 非 Resource owned Array/List/MutableList source 的单条件 return |
| 语言语义变更 | 否 |

## 1. 输入与目标

从官方远端读取 actual-main `934b65908a7e8a42b3b7d969130aa2c95b191703`、tree
`815f8621f02863278adbeb1be84e7a76c4f8049b`，核本地文件/refs及远端0314未占用，
新建独立 `feature/spec-0314`。旧仓外计划在本执行器缺失，本合同依据真实源码与本次授权
重新建立；不宣称恢复旧计划字节或历史本机日志。

支持三Sequential容器、本地声明或own参数，正常耗尽后source live/dead；先独立无Map，
再组合已合入0313的外部Map。unit basic/constant单独分类，不从single结果推断。

## 2. 有界证明

保留旧constant presence及Resource路径/global implies。新证明只消费完整frontend事实：
source root、非Resource、owned身份、唯一真实声明initializer或同symbol own参数Entry；
透明Group只用既有归一，禁止替换、alias、跨loop或猜同型seed。

Header使用真实Always Entry建基；唯一真实Fallthrough回边仅假设该Header presence，
保留其他selector。一个条件Return、一个Fallthrough、一个Exhaustion；Return没有Header/Exit
phi incoming。独立证明Return path与Header presence共同选择精确Drop，并核DropFact owner、
point、condition及frontend完整actions。清理沿已发布actions，不自行重排资源。
正常耗尽live Exit必须从已证Header运输；dead source在EndSource后Drop Header且Exit absent。
不删除continue gate硬放Return，不把Exit自证作为Header证明。

精确SSA验证实际seed/source loan/length/cursor/element cleanup、Return独立终结、最新Map owner
和source权限恢复；动态stdout只证明普通执行，不代替唯一消费或free count。

## 3. 验收与非目标

先合同及tests-only独立提交，再冻结旧生产红测；single三容器×local/own×live/dead，
无Map12格与Map12格分别报告。basic/constant×三容器×local/own×live/dead24格独立分类。
扩展覆盖0轮、never/first/last、Group、同型旁观owner、独立fresh-inner及拒绝边界。
共享调用方按静审后的普通源码目标定向回归；fmt/clippy/docs/尺寸、独立审查、真实CI闭环。

不扩Guide、frontend、SSA model/verifier、LLVM、一般Resource/global implies；多return、
break/continue混合、无fallthrough、跨loop/capture、RangeView等保持原能力。缺阶段事实或
语义未知立即停报。云不运行注入、校准、sanitizer runtime、bad IR、LLVM计数插桩、aggregate
或mock aggregate/full discover。baseline/current绝对独立Cargo target串行；日志/完整JSON
及source hash冻结留仓外，Spec只保留简短账本。

## 4. 唯一验收账本

| 验收项 | 输入与选择 | 实际结果 |
|---|---|---|
| 基线/合同 | §1 actual-main | 远端commit/tree与编号空闲已核；生产未改 |
| single无Map/Map矩阵 | tests-only `246e263b`，`owned_source_return_tests::` | 24项均红，frontend通过、精确UnsupportedNode keys；source前后hash相等 |
| unit basic/constant | 同tests-only输入/选择 | 24/24通过SSA/verifier/原LLVM，0 ignored；不是新增unit能力 |
| SSA/native及共享回归 | 待冻结具体目标 | 未运行 |
| 静态门禁、审阅与CI | 本分支 | 未运行 |

首轮offline在编译前缺inkwell缓存；随后`cargo fetch --locked`补齐原锁定依赖，
再于独立baseline工作树与绝对target执行48项。日志与完整source hash留仓外。
