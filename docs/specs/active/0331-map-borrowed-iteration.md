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

## 8. 恢复主线后的前端事实与阶段门（2026-10-17）

- PR118 的 Rc nullable while 窄修已合入实际 main `a2a7e4b6833977fcae26d6420f193ea1ac7d53df`。0331 以真实双父 normal merge 同步，已发布 `7ff049de660fe47e0b971650f5b73617f310f756` / tree `9510bea91209e11a6c03c2c7084e85ebeb6faaea`；父序为诊断提交 `06f2bc33e1e63b4dc24f32e88d90a7716ad08395`、该 main。两侧代码无路径交集，0332 两 recognizer 与 main 完整字节一致；仅库存/图等共享文档机械合并，保留原历史与0332归档。
- 联合前端源码已独审并有限编译。恢复后的 P1 普通源诊断证明 owned source 的有效 Drop 指向有真实 Parameter Entry 根的 Header phi，且同条件回边承接；原测试要求 owner 必须直接为 Parameter 过窄。独立只读查询保留 AST/source/symbol/origin、唯一 Entry 根、Header 回边、presence condition、唯一 Drop 以及 Exit 的 Never/空值身份，不泛接受任意 phi。首版 query 将末子句误当 For.body，真实运行仍红，收据 `ec5bfdbd79c5cde18c1e0cf2aaff5fb8e3a9c562a673b32b23782cc965f7e03d` 保留；仅修 query 到真实 Block 节点后，`324d4d946213dfed2efc29c9e3997405ed0ff6f0` / tree `a1ec8d8be5ed368e59f923bcfc4b5658850abb57` 的原8观察全目标通过。收据 `b90c86c4698fdb45bab9a834d965f302c6d908544ecf020f681e69c2d36e916f`，实际1/exit0、编译4.48s，未改生产或原来源。
- `1e0e12c8b475f04fd6bdd5af5a2b8c6b76dd174b` / tree `17897e6d7aaa619b06a3025940a71570c60aede0` 的两个 ordinary exact 通过（6+5观察）。Map/MutableMap 各走 single、unit basic、unit const 的真实 complete/ownership 完整验证，随后精确 `UnsupportedNode` 锚完整 For `45..115` / `52..122`；whole-set 五观察保持 entry 无调用、含 Map 函数未调用及正逆输入后的实际 qualified 首项身份。收据 `1104a5716d257e5d4839759811f5f487f06b9ac9782b1e233ae177c7b208a489`，两项各1/exit0；首编48.18s，后项复用0.06s。此为暂时后端能力门的结构化拒绝，不记 provider/LLVM 支持。
- `34961f9cbf741ec7ffb1c9a8cfc8aa8c574748ff` / tree `3135829f6b191597afb610ec10f19d6a90ec6701` 的独立公共事实 exact 通过8观察：原4种成功源 × 两入口的真实 typed descriptor/K/V/两个名字、同源双 Shared loan、AST 出口与 value→key / provider/source 清理、独立 DropFact 均通过硬查询，原行为判据保持。收据 `7462b5dc8f6a0fbb1f1429829ab4762f87793a60bb5c51378340fead40c236ca`，实际1/exit0、编译5.34s。single 另核独立 cleanup_steps；unit cleanup getter 来自同一计划表，仅计同表一致性，不称第二存储证明。静态事实不能代替工厂运行时仅调用一次或 native 清理验收。
- 普通 direct Borrow-return Map source 的双入口在 `4400970c7c81c04d5bcb99a2a7507d236d32dd22` / tree `20e59a4993040d9eaee706163b8533cf8edbe8b5` 均通过 raw/complete 与真实普通调用身份，然后精确保留 ownership `L0164`：primary `138..150` 的 `view(source)`、label `33..39` 的 `borrow`；错误事实清空与 deferred 空均通过。收据 `ece765e7fcbb92188d7ad53d0a7f497e369515d459ea282240cd89a21cd1bf0d`，实际1/exit0、编译5.17s。该 continuation 仍未证明，是实现待项而非语言限制；没有补称旧头执行或成功支持该 source。
- `a92838a9d9157bb2811c7413600b2ff06aeabe68` / tree `ce254aee3e62cce70021f9f28842401bd431e9df` 的 source 内 return / dead-tail 两源 × 两入口完整通过：typed Map 精确身份保留，真实 ownership 验证成功且无公开已 Acquire Map 计划或对应清理。收据 `c30f490ac434c716ae56e841afbb8c09fc2456afefa2d6c4ba18a41355bdb915`，实际1/exit0、编译5.18s。原 collector 的 pending 标签保留历史原文，新增 query 承担此轮事实验收；不外推私有 template 状态或后端能力。
- 各轮完整 tracked 输入、manifest/runner、工具与实际 exact 命中均固定并前后核同；上述轮次逐份独审，原始收据与源码恢复包留仓外。有限编译和各源限定事实不替代最终同头工程/共享回归、混合嵌套与其他来源、非法 pattern/错误恢复、私有 CI 负控、真实 provider/SSA/LLVM/native。0331 保持 active；0291 待项与0332旁支欠账仍按各自交接承担。

## 9. 完整 pattern 诊断恢复与共享前端回归（2026-10-17）

- 先固定普通首红：`60f877625d45e6e199b077699de28d3092790ea9` 的7源/14观察中，raw 完整 binding `L0160`、nominal source `L0159` 与来源身份均正确；单名、单 discard、nominal 双端6路保持完整诊断，四种非法 tuple 双端8路在 complete 变成 marker 上的 `MissingRequiredFact("original diagnostic for required Error type")`。收据 `19f117ebd3732139caf7d22a9920edb3b84a6139bc52cb06d837775792ffed67`，实际1/exit101。另3源/6观察首红收据 `9b275a1cf97c979477a38f4a905ca8177a67da80fdafeb36c623c83cef034856` 全部在同类 complete 根检查失败，raw 与源事实正确；包括当前旧 List<Pair> arity `L0118`，未据此推断其历史引入时间。
- 唯一生产修补位于共享 final-admission walker 的真实 For Destructuring 分支：只有真实完整左右括号 pattern 上既有 Error 的 primary 全等，才沿原 binding poison 恢复；Name/discard、source/visited roots、逐 binding 与 body 的非短路访问，以及无匹配根因时的严格缺事实检查保持。未缩小诊断 primary、全局放宽错误覆盖、重推 provider 或补造成功事实。`3f75a6d4d91c9376140665f97963e210ab9756a3` / tree `0f979783d57e9c01fb3cf1c0d400c8424cbafda6` 的原14+邻控6全目标通过，收据 `9642313c85a3658d99796a9db8d40dcf7df1fc49530aef1dc34569f9a95ff868`，两项各实际1/exit0。独立 body 仍追加真实 `fooBar` 的 `L0080`，原完整 `L0160` 保留；value-class 完整 `L0118` 及声明 label 保留，错误 binding 派生读取无级联。
- 工程首轮 fmt/check 通过，clippy 因测试重复加载 analysis 模块与报告参数多余借用失败，build 未运行；独立 tests-only 修正为根模块唯一注册及等价参数传递，没有改源码 fixture、oracle 或生产。固定 `8240e11c637a4c2ab894bf325a32dcd50b5debf4` / tree `fdcbd273ec2fc3fd8b202a20565ca18f15586397` 重新执行四项工程：fmt、workspace all-targets check、clippy `-D warnings`、workspace build 全部 exit0；收据 `192b6e6bbdbaf8d90e5c243610f9fee0fce9d94e6f67f655204cc6576bed5e72`，check1.92s/clippy2.98s/build18.56s。
- 同一固定头的22个 ordinary frontend exact 均实际1/exit0，收据 `87be7a780788cbf29d02830b0123e20bc9be7ee8e49118a4331f11b3f5e12328`：保留0331已有10项类型/ownership/公共事实及错误边界，另6项旧顺序/值类/trial邻控、6项共享 complete/W7 根与错误恢复。2541 个 tracked 输入及工具、manifest/runner 前后稳定；工程与22项收据逐份独审，原首红和 lint 失败原始证据继续保留。`L0164` 等拒绝边界测试通过不计成功 source 支持；旧 raw-only 邻控保持其原层级，不升级为 complete 覆盖。
- 本节仅闭合上述诊断恢复与当前头有限工程/共享前端回归。混合嵌套及更多 source 来源、私有 CI 损坏事实门、实际 Map provider/SSA/LLVM/native 仍待；未执行 CI-only 或将阶段 lower 拒绝变为运行支持，0331 继续 active。

## 10. 最小有序 Map provider 实施合同（2026-10-17）

- 首个后端闭环限定为真实直接 Name 参数 source 的两组组合：Map 的 Value/Owned 参数与 MutableMap 的 Borrow/Shared 参数；两个具名分量均按已验证事实交付 Shared Borrow。首片只支持无分支、无控制转移及无嵌套 provider/loop 的 straightline body 的正常回边与耗尽，不因两个出口数量相同而放行混合嵌套；Field/Group/alias/temporary、其他 kind/access 组合及 break/continue/return 后继分片验收。direct Borrow-return 的既有 L0164 仍按未证明 continuation 待项保留，以上都是阶段实现边界。
- 资格分两层：完整 typed/ownership 身份与事实验证后，构建 Program/mapper 前以纯前端事实预检整个 source-set；实际 lowering 中再按已映射 Module 核对真实 Map kind、K/V 与 storage。缺失或损坏事实仍先内部合同失败；合法但未 Acquire 或未支持组合精确 UnsupportedNode 锚真实 For。whole-set 按该次实际 source/StatementId 顺序选首个不支持项；新增支持的原 stage-gate 同源必须改为硬成功，不保留与能力矛盾的必拒绝 oracle。basic/const 保持原 sealed-view 交接，const 另行取得实际证据。
- 复用六字段 header、五字段有序 slot，以独立 source lease 访问 first/next 链；Borrow 参数建立 provider 自有 reborrow，不结束入口 loan。SSA 分别表示 First、HasNext、Next、KeyLoan、ValueLoan，两个分量为独立单结果 Shared loan。cursor 使用现有无符号64位值，逐 CFG 边保留精确 source lease 来源，缺来源时清除旧记录或拒绝；不得从上一轮同 ValueId 留下虚假证明。Next 必须等两个分量及其派生 loan 全部结束；普通回边按真实清理事实结束 value 再 key，Map 不产生 EndElement。
- LLVM 沿真实目标 size/NONE 无损扩展，先核非 NONE、capacity 与 occupied slot，再形成 payload 地址；禁止先截断后检查或按 bucket 扫描代替插入序。目标 size 宽于64位时本片明确拒绝。owned source 的 Header/回边/presence/Drop 证明必须绑定本次真实源与实际 SSA carrier；先采新 String 源事实，不把旧 Int P1 的数字或形状当通用豁免。
- 首批普通源为 Map own 参数与 MutableMap Borrow 参数各一份 String/String 程序，两入口保持默认包 provider.ko 原字节。每份 entry 先扫描按 z、a 顺序插入的两个条目，再扫描空实例，body 同步借用两个分量；四 pipeline 全部采完后硬要求完整 SSA/LLVM。通过后才独立验同源正常 native 输出与精确身份/清理，stdout 不单独证明 loan 或析构时序；后继再扩退出与旧 provider 邻控。私有损坏 cursor/loan/来源负控只在 CI 执行。当前本节为实施合同，不预记 provider 行为成功。
