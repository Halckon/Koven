# SPEC-0331: Map 双分量借用式插入序遍历

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：实现或验收 Map 借用遍历时 · **唯一真源**：本片实施范围与验收；语言语义以 Guide v0.44 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P2-0331` |
| 所属 Phase | Phase 2–6 |
| 基线 | PR117 正常合并后的 main `864b6423849156c36fdad5e342cbf4bd09c3868d`，tree `031f2ddf2e0dce4a5797b3e005bb11942f94fa52` |
| 分支 | `feature/spec-0331-map-borrowed-iteration` |
| 批准依据 | 用户同意 PR117 正常合并并保留分支，随后以新 Spec 推进 Map 借用遍历 |
| 前置 Spec | [SPEC-0291 插入序共享存储与 consume](0291-readonly-map-construction.md)，只承接已交付能力，不将其 active 待项改记完成 |
| 语言规范 | [Guide12 Map 独立迭代合同](../../guide/12-collections-destructuring.md#整数-range-与-map-的独立迭代合同)、[provider 清理](../../guide/12-collections-destructuring.md#373-source-loan退出清理与冲突) |
| 影响范围 | frontend typed/ownership、typed SSA/verifier、LLVM、普通与 CI-only 测试 |
| 语言语义变更 | 否 |

## 1. Goal 与范围

让真实 intrinsic Map/MutableMap 的双具名 shared Borrow `for ((key, value) in source)`
按现行 Guide 插入顺序执行，并由两个前端入口、所有权事实、SSA/LLVM 与真实产物闭合验证。
source 只求值一次，K/V 保持真实类型与来源；不制造 Pair、entry owner 或 value-class 字段。
复用0291插入链和既有 provider/source 清理框架，不重建 Map 存储。

Copyable 读取可复制，MoveOnly 不得移出或逃逸；活跃 provider 保护 source，拒绝变更、
consume、整体 move/drop，允许共享查询及嵌套共享遍历。temporary hidden owner 保持至
provider 结束；两分量 loan、body-local owner、source loan 在各退出路径遵守 Guide 顺序。
return operand 必须先在来源仍受保护时完成交付检查。遍历顺序不规定条目整体析构顺序。

Phase 2 按 StatementId 发布 Map 专用计划；Phase 3 发布双分量/source 及各退出 edge 清理事实；
Phase 4 只消费已验证事实，不从 AST 重推 provider 或所有权。

## 2. 非目标与交接

不开放 forEach、通用 lambda/iterator 协议、用户 provider、Pair/单 entry/下划线分量、
consuming iteration、反向/步进/并行遍历，不扩展一般 nullable 或资源表示。
0291 保持 active；其既有有界 nullable、遗留 lambda/其他待项不由本片隐式关闭。
Map source 的 owned/Borrow/Inout/field/temporary 前端资格按 Guide，不以暂未 lower 为语言拒绝。
本片逐阶段声明实际支持和未覆盖路径；先完成 owned named、Borrow 参数与 temporary 的
双入口 native 验收，再按真实事实登记其他 source/CFG 边界，不能只以类型通过宣称可执行。

## 3. 分片与验收标准

- [ ] 首个普通 exact 全采 Map/MutableMap × single/unit 四观察，区分 parse/names、raw、complete
  停点，锁真实 intrinsic K/V 和两个 binding 身份；首红不引用尚不存在的 Map provider API。
- [ ] typed 明确 Map 专用计划及两个具名 binding；非法模式、同名用户类型、body 错误和 trial
  不发布半份计划。精确新诊断选择在对应负例片显式冻结，不把旧一概拒绝追认为目标合同。
- [ ] ownership 覆盖 source 一次、共享查询/嵌套、复制/借用、变更与逃逸拒绝、temporary 延续，
  fallthrough/continue/break/return 两分量和 source 清理；循环后权限恢复。
- [ ] SSA/LLVM 独立查询同一 Map/K/V/source/slot 与两分量 loan，first/next/NONE、无越界寻址、
  cursor 目标宽度不截断；provider lifetime 对两个分量 loan/projection 均验证，不依赖仅 first-result 的旧门。
- [ ] 双入口普通 native 覆盖插入/覆盖保位/删除重插置尾、空/单项/增长/墓碑、消费后 Map 遍历，
  以及 named/Borrow/temporary × 正常/continue/break/return；真实 stdout/退出与清理身份分账。
- [ ] 缺失/损坏事实、错误来源/类型/cursor、已结束 loan 等私有合同负控及计数/故障只交远端 CI；
  本地仅有限编译。普通源码与只读查询使用固定 exact、串行执行并核输入稳定与实际命中。
- [ ] 定向共享消费者回归、完整工程门禁、双 host CI、Architecture 和验收账本闭合；小片独审后
  及时保全源码，不用前置 PR117 的433绿代替新 provider 验收。

## 4. 初始状态

本合同建立时尚未运行0331首红或实现 provider。基线继承 PR117 已交付源码，main CI 独立跟踪。
原始日志、完整清单与恢复包留仓外；本页只记录固定身份、简要结论与可追溯来源。

## 5. 首轮普通类型红证

固定输入 `a2dbea9ef5894059b2ae32eefe8d4401b2b6eb1c` / tree
`3cfeadf3a47406dcd828f5c169cbf346b053d569`，2487 tracked 输入及工具/runner 前后稳定。
有限 `cargo check --locked --offline -p lang-frontend --test type_iteration` 8.71s通过；
唯一 ordinary exact `map_borrowed::map_and_mutable_map_named_borrowed_iteration_complete_in_both_entries`
由同 target 的 locked/offline、`--exact --nocapture --test-threads=1` 运行，实际1项/exit101，编译25.84s。
四观察先完整收齐：Map/MutableMap × single/unit 均通过 lex/parse/names，真实 source
intrinsic K/V 为 Int/String；raw 与 complete 均唯一 L0159 拒绝 source `m`，Map 为59..60，
MutableMap 为66..67。两 binding/read 为 Error、既有 sequential plan 数0，无前置或内部错误。
这是 Phase2 真实能力首红，不是 ownership/provider/SSA/native 验收。未改生产，未执行 CI-only。
仓外收据 SHA256 `02f5cf1848c259b0e008b06e824be19c353f1423d069c4270abf5352548c0b8c`
及5份 raw 已独核；后续以独立 Map 计划闭合事实交付，不改变本测试成功目标来消红。
## 6. 联合前端片的工程选择

Phase2 与真实 Map ownership 作为同一可发布原子片；中间提交仅源码保全，不新增
长期 LSP typed-only fallback，也不把缺 sequential 计划回退普通 loop 视为成功。
Map 使用独立 StatementId 计划，保留真实 source/K/V 与两个具名 symbol；required facts
沿最终准入同一次实际消费的 For 根验证，不扫描未提交 trial/lambda 的全部 AST。
非法 Map binding pattern 采用现有 L0160 类，primary 为真实完整 binding；包括单名称、
单 `_`、解构数量非2或含 `_`。这是本片工程映射，不冒称 Guide 已规定该新场景精确码。
L0118 保持 value-class 字段数量合同；parse/names/source/body 的既有根因优先保留。
Map 本轮两个分量各有独立 Shared loan，复用现有逆声明序：先 value 后 key 的 EndBinding；
不产生伪 element/field、EndElement 或 entry owner。混合嵌套共用同一退出序列。
在真实 Map ownership/验证闭合但后端 provider 未实现时，两 lower 共享入口在任何
source 执行或 Program 构建前，以原 UnsupportedNode 精确拒绝整个含 Map 计划的 typed
source-set；缺失/损坏事实先作为内部合同失败，不被能力门遮蔽。此保守阶段门不改变
语言资格，也不宣称按 entry 可达子集筛选，后续真实 SSA provider 片才能撤除。
该门只消费已通过身份和完整性验证的计划；多个计划时按既有 source 顺序及 StatementId
选首个真实 For 的完整 span，保持错误位置确定性。

## 7. 联合前端普通首红（2026-10-17）

- 固定 tests-only 输入 `6c483d4291aa668fbbb9b5000db4b2748933e110` / tree `6cfb9cbe75be96691491a8839c136f1b48f8b517`；生产尚未改变。新增三个 ordinary exact，分别完整采集 8 / 4 / 4 次 single/unit 观察后统一断言；有限 `ownership_iteration` 编译检查通过（1.61s）。
- `ownership-red3` 收据 SHA256 `86c562c695fc63d88a8bb35965097bcab0dcc97a83e8f2dbeae88192b10ad0ba`：三项均真实执行 1 test / exit 101；2493 个 tracked 输入与 manifest、runner、工具身份前后稳定，首编译 5.29s。
- 16 次观察均通过 lex/parse/names，保留真实源与原分析身份；raw 与 complete 均为唯一 `L0159`，停在真实 Map source：P1 `95..101`、P2 `98..104`、P3 `278..287`（`makeMap()`）、P4 `257..270`（`makeMutable()`）、N1/N2 `105..111`、N3 `100..106`、N4 `76..82`，两入口各对应同一源码位置。
- 全部 ownership 均明确未到达；尚不能据此证明两个 MoveOnly 分量的 `L0133`、活 source 的 `L0135`、双 Shared loan 或退出清理。类型事实和清理的成功目标保持原 oracle，联合实现后再逐阶段验收；四种成功源只是代表性组合，不代表所有 Map kind × source 来源均覆盖。
