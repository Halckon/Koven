# SPEC-0318: Map 条件 return operand 的最后使用清理

> **性质**：变更合同 · **状态**：in-progress · **读取时机**：实现或验收 Map return operand 内的最后使用清理时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-MAP-RETURN-LAST-USE` |
| 所属 Phase | Phase 4，保留 Phase 2/3 事实并以 Phase 5/6 验证 |
| 语言规范 | [v0.43](../../guide/README.md)、[ASAP](../../guide/10-ownership-borrowing-drop.md#asap-析构)、[顺序 provider](../../guide/12-collections-destructuring.md#373-source-loan退出清理与冲突) |
| 批准依据 | 用户已批准本轮 M2B 有界收尾顺序；本片由实际基线与独立审阅冻结，不扩大语言语义 |
| 前置 Spec | SPEC-0313、SPEC-0314（均 done） |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 先取得正常源码真实 facts 与红测；若不能唯一绑定提前清理与 Return，登记延期而不推测 |
| 影响范围 | `lang-codegen` 的 single Map carrier、普通源码测试及实现事实文档 |
| 语言语义变更 | 否 |

## 1. Goal

有界 temporary `Array<Int>` 的单 Map、单条件 Return 与互补 Fallthrough 中，返回 owned
`Int` 的 operand 最后一次 remove/contains 所发布的 ASAP Map cleanup 能保持原点消费
最新 owner，并与真实 Return 终结唯一关联；正常回边与 exhaustion 不受影响。

## 2. 冻结输入与对照

从官方 fetch 的 actual-main `b1165b0f` 独立建立 `fix/spec-0318`，不复制未合并0316/0317。
原输入为 `crates/lang-codegen/tests/fixtures/map-conditional-return-0313/updating-operand-boundary.ko`，
保留原字节。它在每轮 put 当前 item 后 remove 同 key，只能代表 present，不能代表 missing。

| 轴 | 本片合同 |
|---|---|
| 入口 | single、unit basic、unit constant 分别跑真实结果；不从 single 推定 unit |
| 普通 Map 出口 | live / dead |
| operand | 原 remove 条件；contains 最后只读；普通 return item |
| 隔离 | 无 Map nullable temporary、Map 与独立 nullable temporary |
| native | 0/1/多轮；first/last/never；remove present/missing 独立源码实例 |
| 边界 | 外围 Resource、直接 owned nullable-return、多/混合退出保留；旧 Map return 正例不回退 |

## 3. 必须证明的合同

1. 使用真实 Map SymbolId、Header owner、availability guard、DropFact 的精确点和真实
   Return expression。不能靠同类型、宽泛 lineage、源码文字或范围包含猜测对应关系。
2. Return 无 phi incoming；Entry/Fallthrough/Exhaustion 的既有互补路径和 presence 证明不变。
3. remove 先更新 Map owner，result 0 是该点最新 owner，只有一次正确消费；result 1 是
   独立 nullable owner，先读 presence 后 drop，不能混用两个 result 的身份。
4. contains 只读借用结束后在既有事实点清理 Map。返回值、Map、provider hidden owner 和
   element/source loan 分开核验；不提前结束 provider 或延迟 Map ASAP 至 return。
5. 正常路径仍将 tail 最新 owner送回 Header，live exhaustion 保留至正常 drop，dead
   exhaustion 按原点清理；Return 不进入任何回边或 exit incoming。
6. 如果现有公开事实无法唯一关联清理与终止路径，保持结构化拒绝，记录所缺合同及延期理由。

## 4. 非目标

不扩展 provider、多个 Map、Resource owner、直接返回 nullable remove 结果、多/混合或
无条件退出、owned source、通用 CFG/NLL。既有已支持形态不得因本片回退。
不修改 frontend ASAP/liveness 来延迟 drop，不放宽全局 presence，不只删除 gate。
不运行故障注入、calibration、sanitizer、aggregate、CompositionContracts 或全量 discover；
云端只运行静审后的正常源码到原版 LLVM/native 和合法 diagnostic，对应远端 CI 独立留证。

## 5. 验收标准

- [ ] 精确源码基线、三入口实际结果与 Map/nullable/drop/return facts 已冻结并独立审阅。
- [ ] 同一正常源码红测先失败，最小实现后转绿；原 fixture 原字节不改。
- [ ] SSA oracle 验证第3节的真实身份、最新 owner、nullable consumption 与完整 CFG。
- [ ] 原版 object/link/run 覆盖第2节矩阵，stdout 不冒充动态释放计数。
- [ ] 原 Map return、正常 Map loop、独立 nullable 及边界诊断回归通过；命中数明确。
- [ ] Architecture 更新、docs/diff/fmt/尺寸及选定 check/clippy 通过。
- [ ] 精确 head 双宿主 PR CI 与独立审阅完成后再归档；未运行不能标通过。

## 6. 实施与提交

先提交 Spec/真实 baseline，再实施 bounded carrier 证明，最后补足结构/native回归及事实文档。
每个可恢复阶段提交并生成固定 bundle；发布与合并由独立发布方执行。
工具链 Rust 1.96.0、LLVM 21.1.8；Cargo 使用独立绝对 target，串行批次前后比较源码 hash。
原始日志/JSON只保留仓外；仓内保留本合同、真实 fixtures 与测试，不复制完整证据。

## 7. 验证记录

初始静审：原 carrier 已在 operand 求值后保存 current binding；因此不能预先归因为缺
snapshot。旧 owner_plan 要求 Return 的 Header Drop 位于 `ControlTransfer(Return)`；
其是否与实际 ASAP facts 冲突，须由本片真实运行确认。

| 项目 | 实际结果 |
|---|---|
| 官方基线 / 工具链 | `b1165b0f`；Rust 1.96.0 / LLVM 21.1.8 已核 |
| 首轮 harness 编译 | `Program` 未实现 Debug，测试报错格式修正后重新编译；不是行为红测 |
| baseline2 / `--lib map_operand_last_use -- --nocapture` | 实际3测试均因预估的独立 nullable factory 对照支持性不符而失败；原日志保留，不计通过 |
| single dead：remove / contains / item | 前两者 P2/P3无诊断，均 `UnsupportedNode` 精确 `cache` 46..51；item verified LLVM成功 |
| unit basic、constant dead：remove / contains / item | 六格分别实际 verified LLVM成功，是原有能力 |
| 独立 nullable factory + Map dead | single在`cache`144..149拒绝；两个unit在factory声明10..107拒绝，单列既有边界 |
| 测试预期整理 | 首批断言按上述实际观测固定，整理后尚未重跑；live、no-map与正式正例红测待补 |
| 源码 hash | baseline2批次前后相同 `e3f90ced6a3bef40028b12fb81bb3d3cf3c851d5aa44bb80a0112d9ced393136` |
| 最小实现 / 后验 / CI | 未执行 |

### 首批真实事实

原 remove dead 用例：Map `SymbolId(2)`，Header owner `CleanupOwnerValueId(2)`；
`DropFact` 在 `CallReturn(ExpressionId(20))`（真实 remove call），condition为5。
Return 是 `ExpressionId(29)`，operand为28，Return exit condition为4；对应 Header
presence selector为0，drop guard为Header presence与同一Return path相交。
Return action列表只包含provider清理及hidden Array drop，没有再次Map drop。
remove独立nullable结果在 `AfterBinaryOperands(ExpressionId(22))` 析构，目标Temporary20。
contains对照同样在CallReturn20清理Map，Return27/operand26；证明疑点是last-use，非变异特有。
正常dead exhaustion仍在LoopExit12消费Header owner，Return不贡献phi incoming。
这些身份是真实事实记录；尚未证明从CallReturn到Return的唯一动态运输，不能据此宣布修复。

### 第二检查点：待运行的测试输入

已补四个正式期望成功的测试 `map_operand_last_use_{remove,contains}_{live,dead}_red`，
以及 live、no-map nullable 对照和 unit factory 精确 `UnsupportedNode`/声明 Span。
这些新增/调整后的测试尚未执行；名字含 red 只表示预期的行为改动目标，不是已复现失败。

新增 `tests/fixtures/map-return-last-use-0318/cardinality.ko` 与对应 `.stdout` golden，
17个普通调用覆盖temporary `Array<Int>(count, sourceIndex)` 的0/1/3长度、first/last/never，
remove的offset0命中与offset100缺失分开调用，contains独立对照；正常出口live/dead均有实例。
`sourceIndex`打印验证每个源元素只初始化一次，不作为释放计数。
三个 `*_map_operand_last_use_native_*cardinality` 使用原版object/link/run；尚未执行。
新native须先完成对应SSA/LLVM验收，不能把源码准备计为正常执行通过。

baseline2的精确测试输入、逐文件manifest、命令与原始日志保存在仓外；替换首检查点测试文件
即可重建该批次输入，反算manifest汇总等于已记录的`e3f90ced…3136`，没有更改原结果。

独立静审发现第二检查点新 native module 的两处导入沿用了旧嵌套模块路径；已按真实
`native::unit_tests` 挂载位置更正 helper 路径。尚未编译或运行，不能作为目标行为红测。
