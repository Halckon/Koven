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

## 11. 具名参数正常耗尽 provider 的真实纵向验收（2026-10-17）

- 首证保留原两份 String/String 源及全部四观察：固定 cde32e 的 raw/complete、ownership、完整 validator/view、真实参数与双 Shared 组件身份均通过，实际 lower 仍在完整 For 返回 UnsupportedNode（Map351..406、Mutable333..388）；收据 `7c554753ba45edc6e2e19ce1c645a8da2345adf72e03e43cc1cbaea518bb8b1c` 为实际1/exit101，46.92s编译。新源的 Parameter→Header、Entry/回边、Exhaustion 与 Named Drop 原始事实先采后核，数字只用于定位，生产资格不硬编码旧 ID。
- 实现复用现有有序 storage 和 provider 验证：新增五个 SSA 操作及精确 source/cursor/组件关系，LLVM 按 first/next 沿插入链访问，先检查 cursor/NONE/capacity/occupied 再形成 payload 地址。双 lower 只准 §10 两组真实参数来源及受限 straightline 正常回边/耗尽，包围 If/When/Lambda/loop 与合法但更长 owned lifetime 仍为阶段 Unsupported；支持形态损坏事实仍 MissingFact。single 逐实际边重绑 source/owner/cursor，unit 运输 pending 列并保存/恢复 false 状态，按已验证动作结束 value、key 后 Next，耗尽结束 provider/source 后消费精确 owned Drop。Borrow 参数入口 loan 保留。无 FE 事实伪造或按 fixture 名特判。
- 首次真实 integrated3 在426793头通过（收据 `33b4926cc613c5b3a3cb26b6caacd06080bec5cc5662e24cc6df53e408c111e1`）：Map/MutableMap × single/unit basic 四条原源均 SSA verify 与完整 LLVM 成功；原 stage-gate 的 Mutable Borrow 三路径 single/basic/const 同源改为硬成功，Map Borrow 三路径保持精确拒绝。whole-set 五观察重新选择该次实际首个不支持 Map Borrow，保留旧全拒绝历史收据。const 实证来自单独 stage-gate，未据 basic 四路外推所有 const 源。
- 两个 native exact 在2e9a头分别执行两份原源，共四产物，严格正常退出、八行 golden 与空 stderr 均通过（收据 `6b72f48282435b2bf43fa40f155dcbbc5ed3a4f1261170b7add4a8a27ee40000`）。每源先非空 z→a 两条目、再空实例，值与顺序正确；输出门由原 helper 断言证成，不称独立子进程 raw、泄漏/分配计数或单靠 stdout 证明精确清理。
- 工程首轮 fmt/check0、clippy 因新 IterationState 的 Map 大枚举失败，build 与旧 provider10 未执行；收据 `d92868c014b02e9a4f7ddf9532754f394b6801bcf3b51e6f8c4910f57bf30c65` 保留。独立三文件 Box 微修保留全部内层 Copy 快照、phase 写入、资格/清理判据，无新增 allow。固定 `ac875435e325ce3983d4f91abef4f0c337888bf9` / tree `ae7f5b336e45f465439f178ab1fe4c3bd74227dd` 重新绑定2562文件及工具，工程四项全0（收据 `889d3e793a5de02d3aac1f08ec63d0c23d5af6b237c9178ab6f30c790ab5735a`，check19.20s/clippy29.33s/build19.37s）。同头 integrated3 及 native2 重验各 actual1/exit0，收据分别 `61d0a3afbecfd7f335ac437330a7d28f6c70499ab4cb3e9e24fa9ec05aedf480`、`28e49bafbd8042db8b9299f158383a5294da6af8f1a1038ab0e73d3d2622e667`；原各类成功/拒绝与四产物输出门均保持。
- 同头旧 provider10 各实际1/exit0（收据 `d891cced7b6210b16538127e745dee8ff7f9a50b109617f29111236ab7f83715`）：8项普通 SSA/内存LLVM覆盖旧顺序源快照、正常/return清理、借用 caller 保护、值类双分量、numeric 极值/退出与嵌套借用及 unit String 参数；2项正常有界 native 沿0329原 success fixture。各轮原始日志、完整输入与工具绑定均独审；raw-only 旧 helper 不升级为 complete 证据，10项不外推旧 provider 全矩阵。
- 独立身份 query 在 `76b7094f2825c9f6ce6211e3711519dde8ba2f1e` / tree `0b7a75061e59565d00f17ac18583e1f6680c287f` 首跑通过，收据 `2445d54301a2975706ba7e7b146bf2a506622bf8f8cbda7d203e0396bbfba78e`，实际1/exit0、47.71s编译、2566输入稳定。原两源四入口全部 FE 参数/双 Shared/来源与清理桥、SSA provider 实体及完整 CFG、LLVM 实际操作数/phi/checked-slot 支配关系 query 成功，独立 SSA verify 与完整 LLVM 亦通过；query 失败仍会保留真实 pipeline 采证。原 pipeline/exact/source/golden 保持，unit cleanup getter 仍只作同表旁证，独立身份依据为真实 AST/FE 来源、SSA 操作与边、LLVM 实体与 CFG。
- identity 测试头补工程 v3 时 fmt/check 通过，clippy 因只读 query 的 CFG 二元组返回类型触发 type_complexity，build 与16项并集未运行；收据 `359400ee8dcaff26a0f1cceaf086abb1822618cbf7a6b835ffe8eae5bdddc49f` 保留。后继只用私有类型别名消除此 lint，函数体、调用方与判据逐字保持，无新增 allow。固定 `4c8eaed6ed13f2a25e4089a3084f948333c77a0c` / tree `d60dadaef8c93ca4c3ae789102992adf519b0a27` 的工程 v4 四项全0（收据 `d0b781cc243b1d40c603d2bbe589d56a8ab54dbded12e855e8a9339f2578237e`，check20.40s/clippy31.48s/build0.07s）；同头16个 ordinary exact 为原 integrated3、native2、旧 provider10 与 identity1 的无重复并集，全部实际1/exit0（收据 `9fcc65cbb4fbb27a44854d465110c110a899f291b4156b5e17c2420b291dccb3`，首编48.76s）。2566完整 tracked 输入及工具、runner/manifest 前后稳定，原成功/拒绝类别、四产物输出及四独立身份 query 均按各自 oracle 保持；未混入 CI-only。
- provider lowering 的 break/continue/return、更多来源与混合 CFG、私有 cursor/loan/来源负控和真实双 host CI 继续待验。静态实体/路径证明不外推动态内存计数或泄漏；0331 保持 active，本节正常耗尽的真实 SSA/LLVM/native 成功不替代全部 Guide 合同验收。

## 12. 无条件三出口 provider 实现与普通验收（阶段记录）

- 本片在既有正常耗尽之上，仅增加真实具名参数的 `Map` Value/Owned 与 `MutableMap` Borrow/Shared 两组、双 `String` 分量、直线前缀后的无条件 `continue`/`break`/`return`。六个源各走 single/unit-basic，共 12 条完整 pipeline；每个零参 entry 先使用插入序 `z→a` 的非空实例，再调用空实例。每轮 `Guard` 是有显式 deinit 的 Resource；return operand 双 Borrow 读取分量，caller 精确区分非空返回 7 与空返回 0。未由此开放条件出口、嵌套、其他 source/mode 或复杂 return operand。
- 首证 `9244391b/2136ea65` 的 12 路均真实完成 frontend/ownership/完整事实校验后在完整 For 返回 `UnsupportedNode`；收据 `c6c5f1a92c83486971df46424f19a60eea2a09244a9c0799e3cc7972aba37bff` 为实际 1/101，同时每格唯一 Guard query 错误。实际 Drop 已在正确 ControlTransfer；测试误把 NamedDrop 的声明诊断 origin 当成构造表达式 origin。独立核对 single/unit producer 后仅修两个 `value_origin == local`，原唯一 Named target、精确 point、Resource/formed 及 single owner Expression→Guard 构造身份保持。修后 `aa70b948/c78da698` 收据 `f7af87e90af34003e9f0374ef145b8817513f366101bf62a518c942f54e2e21d` 的 478 项前置查询全部真，12 路仍为同一精确 lower 首红；没有把查询修正当作 provider 支持。
- 生产候选经双审后在 `fdf15f68/85e583e2` 安装。Continue 真实 Header 回边、仅耗尽释放 own source；Break 的实际 ControlTransfer/耗尽分别释放并合流；Return 不造 phi incoming，先求实际 operand，再执行完整清理，交付同一结果。false 分支使用 body 前完整状态快照独立恢复；每点严格核实际 source/owner/condition/origin、local Drop→value→key，terminal 再 FinishProvider→EndSource→source Drop；Borrow 入口 loan 保持，只结束 provider 自身 reborrow。原 normal Fallthrough 与 sequential/numeric 路径保留，没有新增 SSA/LLVM 操作或全局 cleanup 豁免。
- `fdf15f68` 有限 check 20.11 秒后，收据 `160eec1bd5d3e8ddbff6a1cd62654917a6e54b8d1fbcca9a6d8fbe0329ca6799` 中四个 ordinary exact 各实际 1/0：新 12 路全部 SSA verify/完整 LLVM 成功，526 项报告查询真，包括实际 Guard allocation→Drop owner、return 真实 operand result；原 normal4、三入口 stage gate 的 Map Borrow 拒绝/Mutable Borrow 成功及 whole-set 首不支持行拒绝保持。首编 49.76 秒。该查询没有把原 pipeline 错误短路或吞掉。
- 独立 native 两项在 `41f1295a/add065eb` 各执行六源，共 12 个正常产物；收据 `e13dfd6582a22a094f3b11f7ea8ae329b843dd389dd6f735855562cc343ef2d8` 两项各实际 1/0，首编 47.71 秒。原已审 object→clang→run helper 逐产物硬断正常 exit 0、完整 golden 与空 stderr，覆盖非空/空输入、插入序读取、Guard deinit 次序及 return 7/0。输出/退出证据由 helper 断言承担，不冒称独立子进程 raw、动态释放计数或泄漏证明。
- 同头工程 exits-v1 的 fmt/check 通过，clippy 仅因 single carrier 的 nested if 可合并失败；收据 `7dda55e56c0dde3517eae2fd0e5f80c93ac5dbaab772f3e2056644f433927120` 保留，build 与旧 provider10 未执行。独审等价 let-chain 微修仅改变语法、不改 Loan 判据/短路顺序或加 allow。`69de8f8b/541c9327` 工程 exits-v2 四项全 0，收据 `81a2cf099f57c9c6ec0c8682f9fe769428eb9ca7b5991d5de05ae35acd37a407`；check/clippy/build 分别 20.11/30.08/19.62 秒。
- 同一 `69de8f8b` 完整输入锁下，原 integrated4-v2 收据 `f7774bfd645ee2d8604c8cfc568d6028026c352efa8f2187138cbdc07a54d9f9`、native2-v2 收据 `81945d75fbcc8b63d8368cde394bed3763b75cbb2a60ed99dda39d7b50110399` 各原 exact 全实际 1/0；旧 provider10 收据 `23d323e24a4aba0b724f22aa8c5430dc0c8f1ec09b4f78cb166d2c0f07f4a4e8` 的原 8 SSA/LLVM+2 正常 native 邻控也全 1/0。三轮 80 份 raw、2586 文件前后及工具/runner/选择绑定均已分别独审；旧 raw-only helper 不升级为 complete 覆盖。
- 新增独立退出身份 query 先在 `de79fd9c` 的有限 check 遇到两处临时数组借用 E0716，未执行测试；只以局部数组延长寿命后，`cdc7cecc/89269e04` 的有限 check 20.41 秒通过。该头唯一 12 观察 exact 收据 `80f51f8a24b56d3aab6d414879a95076d489b677a098eb223ecd1384cef3c08a` 实际 1/0，真实编译 49.97 秒，550 项报告检查全 true。独立公开 FE owner/selector/Choice 与完整出口事实、SSA 全 incoming/current source/cursor/loan、Continue 回边、Break 双终态、Return 实际 operand 先求值后清理并同值交付，以及 Guard allocation→Drop 身份均通过；12 格原 SSA verifier/LLVM render/verify 继续成功。5 raw 与 2591 输入及工具/runner 绑定已独审；此新增查询不冒称独立 LLVM 实体查询。
- 同一 `cdc7cecc/89269e04` 的工程 exits-v3 四项全 0，收据 `e0d6dbcb5b586a0e77f851c6662ffac840da9d5e9f2242c686afd21c76932948`；check/build 同头复用 0.06/0.07 秒，clippy 实际 31.80 秒。其后 final20 收据 `e08300be8ea9024dbe3da0d0a9a0dae42c865096308c6523bce1802286148c9a` 的 20 个唯一 ordinary exact 各实际 1/0：原 normal16 完整保留，追加本片退出 pipeline 一项、native 两项、身份 query 一项；正常 4 与退出 12 个 Map 产物均由原 helper 保持完整输出/正常退出断言。100 raw、2591 输入前后及工具/runner/选择绑定按后端 1–5/16–20、前端 6–15 分工独审。
- 条件退出、嵌套、其余合同来源、nullable/Resource 组合、私有负控与双 host CI 仍待对应后继实证。本文只记录无条件出口的有限垂直交付，不勾选总合同，不把主分支或其他来源视为已验。

## 13. 条件三出口的首证与下一实现片合同

- 后继范围仍限真实直接参数 `Map` Value/Owned、`MutableMap` Borrow/Shared 与双 `String` 借用分量。两源在同一 scan 中使用三个顺序的无 else `if`，真臂分别直接 continue、break、return，剩余路径正常落到 body 尾；其他语句与调用实参仍受直线资格限制。每源 entry 五次调用覆盖 continue 后继续读取 tail、break、return 7、自然回边与空耗尽返回 0，预期 golden 与实际 native 结果分开记载。本片不开放一般多存活臂合流、嵌套或其他 source/mode。
- `a9181967/8cac594f` 安装普通四观察 collector，仅以测试内 observer 桥复用原 single/unit-basic 完整阶段，原 normal/无条件 wrapper 的事实查询和执行次序保留。有限 check 20.69 秒通过；首证收据 `a49542c4cfc4b4db6b2752c4cffbd78afc42cc1d894ab76bd8aa028dae8133b5` 实际唯一 1/101，真实编译 50.29 秒。四报告 43+50+43+50=186 项前置与来源检查全部 true，同次 complete、ownership、完整 validator/view 后均实际进入 lower；Map 两端在 525..866、MutableMap 两端在 507..848 的 provider.ko 完整 For 返回 UnsupportedNode，未到 SSA/LLVM/native。5 raw、2599 输入前后、工具和 runner 绑定已独审。
- 真实公开产物提供 Continue、Break、Return、Fallthrough、Exhaustion 五条出口。single Map 的 Entry、Continue、Break、Fallthrough、Exhaustion 有五条 incoming，没有 Return incoming；Continue 与自然回边的 available_when 由 Header presence 和真实分支路径合成，不能再强等 Header 自身 availability。Guard 同一实际构造 owner 在三个 ControlTransfer 和 body AfterStatement 各有一条无独立 condition 的 Drop；source 的 Break/Return/LoopExit Drop 则按 Header presence 与所选路径逐点证明。打印表只是本轮观察，后续实现和独立查询必须按真实实体关系取值，不使用日志数字 ID。
- 已审实现方向复用普通 If CFG、现有 linear/pending 参数运输、cleanup actions 和五个 Map provider 操作。仅保存、恢复并重绑路径局部 Map phase/角色，不回滚累计 break/continue 或逐 point 见证；checkpoint 位于真实条件求值及借用清理之后，每个 arm（含无 else 假臂）及单存活 merge 后均校验实际 owner/source/cursor/组件角色。真臂已结束的 loan 不得污染假臂。条件 proof 只识别真实 If selector/Choice 对应的有限决策链及 Header presence，区分 Branch 与 Presence 域，拒绝未知或循环关系；不引入通用布尔求解器或新增 Guard 条件 Drop 放行。
- 原完整 FE/ownership 验证继续先行；合法但不支持的控制或来源拓扑保持结构化能力拒绝，缺失/错配身份不能降格为普通 Unsupported。原 normal 与无条件出口资格、清理及真实返回值交付路径保持。当前生产候选尚未安装，条件 SSA/LLVM、正常 native、独立实体查询、邻控及工程门禁均待后续实证，不能据首证前置绿认定条件 provider 已交付。

## 14. 条件三出口的有限 SSA/LLVM/native 验收（阶段记录）

- 条件生产在 `af68b993/e60d946b` 安装 22 个 Rust 文件，有限 check 20.80 秒成功。复用普通 If 的 condition 后 checkpoint、真实两臂与单存活 merge 的参数运输；single 使用完整公开 owner/selector/Choice 与真实路径条件，unit 使用自身 source-qualified plans、pending 参数及路径 phase。Guard 由真实 Resource 构造取得当前 owner 后逐 edge 运输，四个 cleanup 点核同实体。累计出口与 Next 见证不随路径快照回滚；Continue 与自然回边各生成一次 Next，Break/Return 无假回边，Return 先求 operand、执行完整清理并交付同值。没有新增 SSA/LLVM operation，也未放开其他来源或一般多存活分支合流。
- 首轮 integrated5 收据 `3de8489e32df708546a0283292ec4faf42a73199071dde4479616ea69893802b` 五项实际各 1，exit 为 101/0/0/0/0，首编 51.41 秒。条件四格 195 项报告检查全真；Map unit 与 MutableMap 双端已完整 SSA/LLVM 成功，唯 Map single 在工厂 `source.consume()` 的 339..355 返回 UnsupportedNode，已不再停于 For。旧无条件 12、normal4、stagegate 与 whole-set 原 oracle 全绿；25 raw、2607 输入与工具/runner 绑定已独审。不能把该工厂停点记作条件 CFG/cleanup 已失败，也不能将三格成功写成四格完成。
- 只读定位确认普通 single consume 复用了 while 的保守 carrier 门，仅容 Boolean 入口借用，工厂独立 Borrow String 参数因此触发旧限制；相关 consume/while 文件在本轮条件修改前后字节相同。最小后继只给非 nullable consume 增加独立入口 String 资格：真实 Function/callable、Borrow 参数 symbol/slot、当前 Loan、SSA entry BlockParameter 与 Shared StringOwner 一一对应；不按名称或任意 HeapOwner 类型放行。原 pending/capture/result-source/receiver/provider/non-null 等限制、消费 root 当前 owner/无 alias、receiver 前后两次交付检查仍保留，旧 while/nullable 门不变。仓外 r1 曾误用未发布普通 local 能力的 global binding_kind（现表含参数及 Map 组件） 判断普通 local；未安装，r2 改为真实同源具名 Val/Var 声明与 formed 历史身份，动态交付另由原完整 ownership/current owner 链证明，未伪造 FE 事实。
- `d7ba96ee/e0f97432` 安装两文件窄修后有限 check 22.24 秒通过；consume8 收据 `258d19eca987451791732f310d4b945bebd08e09923bad6c6b2bde94e9fd3788` 八项各实际 1/0，首编 50.14 秒。原四条件 pipeline 全部 SSA verifier/LLVM render/verify 成功，46+53+46+53=198 项报告全真；原 integrated5 其余四项保持。新增三项已有普通邻控分别为非 nullable consume 十二观察、nullable consume 六观察及 pending-child while 双端观察，原定义/闭包和输入未改。40 raw 与 2608 完整输入按后端前五、前端后三分工独审；这些邻控不冒称 native 或未选 Boolean-entry consume 专项覆盖。
- 独立 native 两项在 `28527bc5/054dd361` 各执行原两份条件源，合计四个产物；收据 `27e71ff7fa2db9c4a91199ae1be7e5b5da43e6588f125da2de5b06ee849a0bb3` 两项各实际 1/0，首编 48.84 秒，single/unit 测试分别 0.14/0.15 秒。每产物经原 object→clang→run helper 硬断 exit 0、完整 36 行 golden 与空 stderr，覆盖 continue 跳过当前读取再进入 tail、break/return 不读取 tail、return 7/其他路径 0、自然回边、空耗尽及 Guard deinit 输出次序。10 raw 与 2609 输入已独审；helper 没有内置硬超时，输出断言不升级为独立子进程 raw、动态释放计数或泄漏证明。
- 工程 conditional-v1 的 fmt/check 成功，clippy 因两个常量错误 ok_or_else 与一处非最简布尔表达式失败，build 未运行；收据 `f19c801fd3377b2cb56eb1ece52d00a068eb37e4fb3edfaf6ae5cbaf1c760a8e` 保留。独审三文件机械微修保持条件/短路次序、MissingFact/None 及所有 oracle，不加 allow；固定 rustfmt 输出与候选逐字相同。`3155acf9/84efc68a` 工程 conditional-v2 四项全 0，收据 `6d6a2d8839a4faab5a76ec901195c67f0e60b48828ba54b6d63aa9dfe5f27cdc`；check/clippy/build 分别 20.76/31.45/20.13 秒。该工程终态已独审。
- 同头 regression26 收据 `ddce9b794e22e7380f632bfd51b1d64830a19d5941c98297a5a37d2d9184b677` 的 26 个唯一 ordinary exact 各实际 1/0，首编 49.02 秒：原 final20 完整保留，追加条件 pipeline 一项、native 两项、consume 邻控三项。旧 normal/无条件出口及其独立身份、条件四格 pipeline/四 native 产物、原成功/拒绝分类均保持。130 raw、2609 输入前后及工具/runner 绑定按后端 1–5/16–23、前端 6–15/24–26 分工独审；受验头与后继文档头分开记录。
- 条件专用独立 FE→SSA 全实体/全 incoming 身份 query 尚在仓外准备，不以原 pipeline verifier 或 native 输出替代该查询；新增 LLVM 实体查询、动态内存计数也未宣称。temporary、嵌套及其他合同来源、nullable/Resource 分量组合、私有负控与真实双 host CI 继续待验。0331 保持 active，本节有限条件控制流成功不勾总合同。

## 15. 条件出口的独立身份查询与同头复验（阶段记录）

- `54f1b232/6fa65988` 仅安装七个测试文件，新增六个职责模块共 2773 行，既有 observer 净增 53 行；生产、Graph/Uses、原来源和 golden 未改。旧 exact 关闭深查询时保持原执行次序，四份同次 FE/SSA 产物全部采集，查询错误不遮后续 verifier/LLVM。仓外首版补齐 read/operand 索引前的双参数检查后才安装；有限 check 20.68 秒通过。
- identity1 收据 `8ff36c7f382c15ee16cbe284ffa067129d6847c5e1786e4249a5643d3fee4c23` 实际唯一 1/0，真实 test 编译 48.14 秒；四格 49+56+49+56=210 项报告全真。同次公开 FE 查询锁五出口、真实 Choice/双 phi 与所有 incoming、Guard 四点及 source 三点清理；unit 只核自身公开事实。独立 SSA 查询锁当前 source/cursor/K/V/Guard 的完整边列及全部用途、两处 Next、Break/耗尽合流、Return 原 operand 值和清理顺序、入口 Borrow 保护及真实 factory/consumer 实参来源。5 raw、2615 输入前后和工具/runner 绑定已独审。本次新增的是 FE/SSA 实体证据，LLVM 仍沿原 render/verify，未新增动态释放计数或泄漏证明。
- 同头工程 conditional-v3 四项全 0，收据 `d1b005ac826bcece71a5f3de02764c516120846b77c5daabd5b468adfbdf479c`；check/build 分别复用 0.06/0.07 秒，clippy 实际 32.32 秒。regression27 收据 `3a55717d1a71bee2e353bcdf0ecf01aebcb052844ee6b3aad1cc504fcda034f5` 的 27 个唯一 ordinary exact 各实际 1/0，严格为原 26 项加本次深查询；135 raw 按后端 1–5/16–23/27 与前端 6–15/24–26 分工独审，2615 输入及工具绑定稳定。本轮测试复用已有编译产物，不重复宣称新编译；后继文档头不冒称行为重跑。
- 本节结清 §14 所列固定条件源的独立 FE/SSA 查询待项；范围仍是直接具名参数两种 mode 与该有限三 If 控制形状。temporary、嵌套及其他合同来源、nullable/Resource 分量组合、私有负控和双 host CI 继续待验，0331 保持 active。

## 16. temporary 正常耗尽的有限 SSA/LLVM/native 验收（阶段记录）

- `8ac018a7/293486e3` 安装独立四 fresh collector：Map/MutableMap 各一份默认包源码，entry 内依次直接以非空与空 factory Call 作两个 For 的 source，共八个静态 site；两源各自保留完整十行 golden。真实同次 typed/ownership 查询分别锁两个 source Expression、四个 component binding、每个 site 的两个 Shared loan 及 read 参数来源，unit 使用 source-qualified 事实；复杂 owner/cleanup 表先记录，不预填未来 SSA 形状。first1 收据 `36190fd53bfd09252238ed8889ab4e1c0a59d4d6b7720316b226e68934c98038` 实际首证四格 152 项报告全真，实际唯一 test 1/101、编译 50.27 秒，两端 Map/MutableMap 分别停于首 For 540..598/506..568 的 UnsupportedNode。第二空 For 当时仅已有 FE 计划证据，尚未后端到达。
- 生产 r2 在 `4e233dda/fdff08a1` 安装 17 个文件，有限 check 21.09 秒通过。新增 private Temporary source 分支，只接本片实际零参 Owned factory Call 与 normal straight-line body；single 消费真实 Expression owner、唯一同源 LoopExit Temporary Drop、三种 Always/空 binding incoming，unit 只用自身公开计划。实际 source Call 在 preheader 求值，当前 temporary owner/source lease/cursor 经真实边列运输，耗尽先 FinishProvider/EndSource，再对当前 owner 精确 Drop 并注销本 site；两段 source 前的 outer temporary 快照分别保留。无 Acquire 计划仍按原门返回 UnsupportedNode，缺失或冲突的必要身份事实不降格为能力拒绝；旧参数/条件路径保持。
- 首轮 integrated9 收据 `62db6eafba9e403e068f789f32648b57c958845b6b2d990b03642896d2c21804` 九项各实际 1，exit 为 101/0/0/0/0/0/0/0/0，首编 51.27 秒。temporary 两个 unit 完整 SSA/LLVM 成功，两个 single 在原首 For 返回 MissingFact；158 项报告全真，旧八项保持。只读阶段 trace 收据 `8b93b087fe5a61f156852a61c6bb9c79cefbf20e8ab5d9ba05f879dec89baae4` 实际 1/101、编译 49.78 秒，明确两 single 停于 temporary qualification/preflight，尚未运行 seed/owner 运输。trace 后独立 revert 的 tree 精确恢复 `fdff08a1`，未改变原错误、oracle 或生产身份门。
- 源码定位确认 resolver 对单个顶层函数也发布 OverloadSet，原 single 新门却只接受 Symbol；typed Call 已经选定 factory。`be91f749/926c1def` 单文件窄修仅将唯一 callee reference 核对为 Symbol 精确相等或 OverloadSet 包含该同次 typed 目标，全部签名/零参/Owned/类型/owner 与 cleanup 门保留，未重做 overload 选择或放宽运输。该解释来自真实阶段 trace 加 resolver 发布链，未声称 raw 已打印 reference 值。有限 check 23.19 秒通过；integrated9-v2 收据 `97f861c32ca55ea62fb4ded153809513f45686defa42e6fc1ae32c1c367fa01f` 九项各实际 1/0，首编 58.00 秒，temporary 四格完整 SSA verifier/LLVM render/verify 成功、164 项报告全真，原八项 oracle 保持；45 raw 与 2628 完整输入及工具绑定已独审。
- `b3fe8d25/1926fdaa` 仅追加原 helper 的 temporary native 接线。native2 收据 `2deae011376836fd4644ab099ff5f7a48557ac64d95c415433c3a73bffd3aefa` 两项各实际 1/0，首编 49.11 秒，四个 Map/MutableMap×single/unit 产物均经原 helper 硬断 exit 0、完整十行 golden 和空 stderr；测试运行分别 0.11/0.12 秒。10 raw、2629 输入与工具绑定已独审；无内置硬超时，不把输出断言外推为 factory/Drop 动态精确次数、独立子进程 raw 或泄漏证明。
- 同头 engineering-temporary-v1 四项全 0，收据 `966ae4b9efc1b13498189dbf487761c384955c48612906cc2ba4df7ef0567946`；fmt 空日志成功，check/clippy/build 分别 0.53/32.52/20.21 秒。regression30 收据 `48ea37718bdbcdd3ae415ede789a19856c13a1dfe9fc220c1b8d525b19656054` 的 30 个唯一 ordinary exact 各实际 1/0，严格为旧 27 项加 temporary pipeline 一项与 native 两项。150 raw 按后端 1–5/16–23/27–30 与前端 6–15/24–26 分工独审，2629 输入前后/当前及工具绑定稳定。该轮测试复用已编译产物；旧 raw-only、成功/拒绝及有限 native 口径不变，后继 docs 头不冒称重跑。
- 本节仅完成两份 temporary 正常耗尽源码的有界 pipeline/native 验收。本阶段结束时，独立双 site 当前 hidden-owner/全 incoming/完整用途的 FE→SSA 深查询仍在仓外准备，其后补证另见 §17；不能用本轮 verifier 或 stdout 代替；temporary 的其他退出、嵌套及其他合同来源、nullable/Resource 分量组合、私有负控与真实双 host CI 继续待验。0331 保持 active，不勾总合同。

## 17. temporary 双 site 身份补证与 nested 首证（阶段记录）

- `68ca39b3/f541deeb` 仅安装五个独立 temporary 查询模块及既有测试末尾注册。四 fresh 继续复用原同次 FE→ownership→完整验证→lower 产品，逐 site 将真实 typed factory 目标、Call Expression owner、实际 read Call、两条 Shared component loan 连到 SSA；查询核当前 hidden owner/source lease/cursor 的全部 incoming 和完整用途、两个顺序 site 的独立登记与清理。single 三种 incoming 按实际 kind 唯一匹配，unit 仅使用其 source-qualified 公开事实，cleanup getter 不冒充第二存储。查询错误仍保留原 SSA verifier/LLVM 阶段。identity1 收据 `452fa25393cb37b572ec213fdc357ff84ab9d210ecde864a8436ef9feed8019f` 实际 1/0、编译 51.29 秒；四格 41/45/41/45 共 172 项报告全真，独立 FE/SSA 查询及原 verifier/LLVM 均通过，2642 输入和五 raw 绑定已独审。新增证据限静态实体/CFG 身份，未增加 LLVM 结构查询、动态释放次数或泄漏证明。
- nested 两份独立源码保留同根 Borrow 的外/内两个 For，四分量分别传给真实 read，内层耗尽后再次读取外分量，并在 scan 与 caller 中检查容器 size；Map Borrow 是新的能力行，不能由 MutableMap 借用路径推定。首证 `9b3cc322/b51129f6` 收据 `267261464ae892be2ceb477f92bcb337e58cd1ee5c106fdfd99cf907c1f7fd11` 实际 1/101、编译 53.17 秒，四格各有两项 caller 普通 local 查询错误，同时真实 lower 均在内 For 返回 UnsupportedNode。global binding_kind 并不发布普通 local 能力；`512c0533/704c2629` 仅将两个查询块改为真实 Val/Name/Variable、typed Owned initializer 及实际存在的 initializer type 与 local type 一致及 formed 形成历史，原 Borrow call/loan/size 使用门完整保留，未把 formed 当动态可用许可。first2 收据 `c9b2fed10157bb081e94d0f159849fee7230d047b673b095c6e4fe0075785811` 实际 1/101、编译 53.04 秒，47/55/47/55 共 204 项检查全真；双 Mutable 仍在 430..564、双 Map 仍在 444..578 的原内 For 全文返回 UnsupportedNode，尚未 SSA/LLVM。两轮红账和原硬成功 oracle 均保留。
- 同头工程 temporary-v2 真实 fmt/check 0、clippy 101，唯一错误为新 temporary 查询中的 needless_question_mark，build 与原备用 31 未执行；收据 `7362cb26d566fb64072a3d3efa2dd066421ed6a320ad2091b8c734c0b55a2fc2` 保留。`4ec7c043/009a0463` 仅把 `Ok(value(next)?)` 等价改为 `value(next)`，不改求值次数或查询判据。engineering-temporary-v3 收据 `6298ddeb7c88f0039bf25cf867b5cf383bede297d388c4a48a0cf86756e40946` 四项真实全 0；fmt 空日志，check/clippy/build 分别 21.21/35.52/0.07 秒。
- 同一 `4ec7c043` 受验头 regression31-v2 收据 `36a84172404e44ca70994039741fd0f0ec20c04362bdad7699c130ce262ed330` 的 31 个唯一 ordinary exact 各实际 1/0，严格为原 30 项加 temporary 深身份一项；仍未支持的 nested 硬成功首证单列，未混入这个成功集合。首编 54.00 秒，其后复用；155 raw 按后端 1–5/16–23/27–31 与前端 6–15/24–26 分工核验，2642 输入与工具绑定稳定。仅结清上一节的两份 temporary normal 深身份待项；双层 nested normal 生产仍在仓外，嵌套其他退出、temporary 其他退出、其他合同来源及 nullable/Resource 组合、私有负控与真实双 host CI 继续待验。0331 保持 active，后继 docs 提交不冒称再次运行行为。

## 18. 同根双层 Borrow normal 的有限 SSA/LLVM/native 验收（阶段记录）

- 为新增 Map Borrow normal 能力保留独立 whole-set 拒绝边界，先在 `5b271126/e462c152` 加入 Group 首证：single、unit basic/const 正反输入五次 fresh 分析都使用空 entry、两个 direct 对照和两个真实 Group source。收据 `619c03f04868805c2e9a934d670dfa023b13ded090efb024d228250bfdcd716a` 实际 1/0、编译 53.45 秒，397 项报告检查全真；完整 FE/ownership/validator 后，single 精确拒绝 whole.ko 356..428，四个 unit 精确拒绝 b_group_mutable.ko 62..134 的实际首个 Group For。每次按自己的真实 Statement/qualified ID 选首项，不用 kind 或跨分析裸 ID；这是阶段边界成功，未证明 Group SSA/native 支持。
- `73db1ac7` 仅将旧 stagegate6/whole5 的 Map Borrow 必拒绝 oracle 迁为真实成功合同，保留原 source 字节、全部前置与 Group 注册；`200df126/30d767ff` 随后安装 19 个生产文件，有限 check 22.46 秒成功。新增范围是两种容器的直接 Borrow 参数 normal 及本片同根、两层、String/String，循环体内除固定内层 For 外无额外局部 owner/控制退出的嵌套形状；不放开一般多层/多活合流。两个 provider lease 分别从当前 caller 借用，source/cursor/四 component 按实际边列运输，inner false 恢复完整 outer 角色并仅注销 inner 状态，内层耗尽后的 read 使用当前 outer 分量，outer cleanup/Next 仍在该 read 之后。single 区分 birth 与当前 cleanup block，unit 保持 pending 前缀；旧 normal、无条件/条件出口和 temporary 路径不改其 oracle。合法超出能力的拓扑仍为 UnsupportedNode，缺失或冲突的必要事实不伪装成能力拒绝。
- integrated12 收据 `013b25555a8f2391291be0693bb36b3498bf430ad466b1d89b84449fc6730016` 十二项各实际 1/0，首编 59.60 秒。nested 四格均完整 SSA verifier/LLVM render/verify 成功，50/58/50/58 共 216 项报告全真；原六路径 Map/MutableMap Borrow normal 均成功，whole5 的完整源集准入及返回程序验证均成功，Group5 精确拒绝保持，旧四组 pipeline 与四组独立身份查询保持。whole5 的 unit 返回程序只有未调用其他函数的空 entry（LLVM 253 bytes），不能外推为未调用循环实际生成或执行。60 raw、2660 完整输入前后与工具/runner 绑定已经独审；此轮未新增 nested 独立深身份 oracle。
- `b0081392/6429ae7b` 仅追加原 native helper 的 nested 接线，有限 check 22.23 秒通过。native2 收据 `7ab8aae297d2341208568a5e3fc5002b9c25f9ab4768c448a6049d3706ab849f` 两项各实际 1/0，首编 49.81 秒，测试运行 0.13/0.12 秒。Map Borrow/MutableMap Borrow × single/unit 四个产物均硬断正常 exit 0、完整 40 行 golden 和空 stderr，覆盖四种有序配对、内层后外分量继续读取、scan/caller size 与空外层耗尽。10 raw、2661 输入与绑定已独审；helper 无内置硬超时，输出不升级为独立子进程 raw、精确动态释放次数或泄漏证明，也不覆盖外非空而内独立空源的组合。
- 同头 engineering-nested-v1 四项全 0，收据 `430291db5429ea59a94cc232b17eadcb1a3a8286f0bf88efe81fef1e0907616e`；fmt 空日志成功，check/clippy/build 分别 0.59/33.25/19.75 秒，无新增 lint 修补。regression35 收据 `080358b2602749666cc799cf5fec534d632d4eb1303a918889f55de68ae9c013` 的 35 个唯一 ordinary exact 各实际 1/0：原 31 项仅迁移两个 stagegate 成功对象，追加 nested pipeline、Group 边界及两项 nested native。175 raw 按后端 22 项与前端旧邻控 13 项分工独审，2661 输入及工具绑定稳定；其中两个旧 CLI native target 实际编译 1.52/0.68 秒，其余使用已有产物，不统称本轮全部重新编译或全部复用。旧 raw-only/阶段拒绝与有界 native 范围不扩大，后继文档头不冒称行为复跑。
- 本节只完成固定同根双层 normal 与 Map Borrow normal 的有限垂直验收。nested 专用同次 FE→SSA 全实体/全 incoming/完整 uses 查询仍在仓外实施，不能用普通 verifier 或 golden 代替；嵌套其他退出、temporary 其他退出、其他合同来源及 nullable/Resource 分量组合、私有负控与双 host CI 仍待。0331 保持 active，不勾总合同；现有语言合同不因 Group 的阶段拒绝而收窄。

## 19. 固定同根双层 normal 的独立 FE/SSA 全身份查询（阶段记录）

- `debaaec8/4c2b9d23` 仅追加 11 个测试文件的独立 nested query（净增 2900 行，10 个新模块最大 611 行），原 nested collector、两源/golden 与生产字节保持。同次 FE 桥锁真实父子 For、同根 Borrow 参数、四 component/loan、各点完整 cleanup 及 caller 两个 local 的形成/借用调用/size/Drop 身份；普通 local 不从 global binding 表推 Owned。single 按真实 owner/point 查询，unit 仅使用自身 qualified 公开事实，getter 一致性不作为第二份独立存储证据。query 错误独立记入报告，四格仍保留原 SSA verifier 与 LLVM render/verify。
- 有限 check 21.15 秒成功。identity1 收据 `f8c657388e6382cf82fb00fc2bd3e420b7a74f7d0e5f1bf3315924c5b2fb7b6a` 实际 1/0、编译 49.76 秒、运行 0.12 秒，52/60/52/60 共 224 项检查全真。四格独立核两条不同 provider lease、所有 incoming 的当前 source/cursor/四分量、内层 false 后当前外分量 resume/read、完整 operand/edge uses 与 caller 借用保护；实际可达 block/instruction 集合及 Origin 一并核对。四份 LLVM 仍为原 render/verify 证据，新增的是静态 FE/SSA 实体闭包，不是动态 factory/释放次数或泄漏证明。
- 同头 engineering-nested-v2 收据 `63f425bb84e33913322f96c358f6559f5cb95d74693ae373fa5bb354df6faf3b` 四项全 0：fmt 空日志，check/clippy/build 为 0.07/34.41/0.07 秒，无 lint 微修。regression36 收据 `421295cb9014e52ea1c23f665ece617c74e71e3d47bada4e63ea83231ae0f154` 的 36 个唯一 ordinary exact 各实际 1/0，严格沿用原 35 个完整对象并仅追加新 identity；180 raw 按后端 23 项、前端旧邻控 13 项独审，2671 完整输入及工具/runner 前后绑定稳定。本轮复用已有测试产物，包括两个旧 CLI target；不称全部重新编译。stagegate/whole 的有限成功与 Group 精确阶段拒绝分别保留，旧 raw-only、native 输出及 28 个 Map case×entry marker 的证据范围不扩大。
- 本节结清 §18 的固定同根双层 normal 独立深身份待项；行为受验头仍为 `debaaec8`，后继文档提交不冒称重跑。嵌套 inner continue/break/return 的六源 collector 仅完成仓外静审，尚无首证运行；temporary 其他退出、其他合同来源与 nullable/Resource 分量组合、私有负控和双 host CI 仍待。0331 保持 active，不勾总合同。

## 20. 固定双层 Borrow 的内层三退出 SSA/LLVM/native 验收（阶段记录）

- 六份独立源码保留 Map/MutableMap 同根直接 Borrow 参数、两层 String/String 分量和真实 OuterGuard/InnerGuard Resource；内层末句分别为无条件 continue、break 或 return。C/B 在内层之后继续读取外分量，R 先调用四个 Borrow 参数的 operand 并返回同一 Int，caller 精确检查非空 7/空 0 及容器 size。首证 `3065edab/853c3925` 收据 `071e3f2e478a3795e771d4129bf41d74509b730d75811b3e3e71ffd77cf2cd10` 实际 1/101、编译52.11秒；12格716项前置检查全真，完整 FE/ownership/validator 后均在原内 For 全文 UnsupportedNode，未到SSA/LLVM。实际24个plan的52个exit引用对应48个唯一point；四个Return共享point分别保留同一完整10动作序列，不拼接或伪造Return incoming。
- `d1f5a82e/33237f7d` 安装24个生产文件，限定上述内层尾退出形状，旧normal分支保持。single以公开 owner/selector/Choice、完整incoming和实际point证明当前OuterGuard；不读取私有instance-presence字段，不把空capture表当作全部phi验证。unit仅消费自身qualified事实。caller、两provider角色、四分量及当前OuterGuard按真实边运输；Continue只推进内cursor，Break与内层false独立清理后合流，Return先求实际operand、一次消费同point完整10动作后交付原结果且不造回边。false路径恢复不回滚累计退出/消费证据。有限check-v1真实25.95秒成功但有private_interfaces警告；`5c3b73a1`仅缩窄一个方法可见域，check-v2 25.05秒无警告。
- integrated14-v1 收据 `5af93fb0046ac24938c5bf96a41b15cd7b65c752bf2a386dff467a9f6f3e6118` 的新退出项实际1/101，六个single已完整LLVM成功，六个unit在原inner For报MissingFact，合734项报告全真；其余13个ordinary项全部1/0，首编53.92秒。静态resolver及同次class事实确定unit Guard构造引用属于Type命名空间，资格代码却筛Value。`dd27651f/dc95ada8`仅将该constructor筛选改Type，保source/span/唯一目标与nominal关联，普通operand函数仍Value，不改运行态或成功oracle；有限check22.81秒无警告。integrated14-v2收据 `1b99bad277aa2c064918dcd0302482a3a6fe95498956e0b9cc9a02beac313672` 14项各1/0，首编56.73秒；新12格752项全真且全部SSA verifier/LLVM render/verify成功，原13项保持。两轮各70raw和2700完整输入/工具绑定已独审，旧红账保留。
- `e3b3e2a4/403dc2c6` 仅追加四个tests接线文件，有限check23.29秒。native2收据 `57e29f7f6b81a5dece61971ab0b9c44886e2213da5eb1e67b036189fd7a5fa47` 两项各实际1/0，首编54.77秒、测试运行0.38/0.41秒；六源×双入口的12产物均由原helper硬断正常exit0、完整C/B/R各46/34/18行golden和空stderr后输出marker。10raw与2701输入稳定已独审；这证明固定输出和正常终止，不增加独立子进程raw、硬超时、精确动态释放次数或泄漏结论。
- 同头工程v1收据 `c96da12b95547049b564910d484a7fb2710d3ae09f3e568de6a7450733638528` 为fmt/check0、Clippy101：五文件六处机械lint，build及原备用39未执行。`6f3f2728/402e1bc8` 仅作三处let-chain、相同then合并、contains和等价布尔化简，保持短路求值/带错误传播调用的条件，不加allow。engineering-nested-exits-v2收据 `864dc9a85f707701eeeea210c57b1883656e90c80c313447296a1849576ff473` 四项全0，fmt空日志，check/clippy/build分别22.63/34.96/21.06秒，2701输入及工具绑定稳定。
- 同一`6f3f2728`的regression39-v2收据 `195f9fc8c4f3c23f7a7ae3e2305f6e2349ef0ab9ef784225f6eea8b4609635bd` 39个唯一ordinary exact各实际1/0，严格旧36完整对象加本退出pipeline及两native项。首编53.12秒，两个旧CLI target实际编译1.39/0.55秒，其余复用；195raw按后端26项与前端旧13项分工独审，2701完整输入及工具绑定保持。旧stagegate/whole有限成功、空entry不代表未调用循环执行、Group精准阶段拒绝、旧raw-only及native证据边界均保留。
- 本节只结固定同根双层内层无条件C/B/R的垂直结果。新的独立FE→SSA全实体/全incoming/完整uses查询仍在仓外实施，不能用本collector或golden代替；更一般嵌套控制流、temporary其他退出、其他合同来源与nullable/Resource分量组合、私有负控及真实双host CI仍待。0331保持active，不勾总合同；行为受验头为`6f3f2728`，后继docs头不冒称重跑，远端发布身份单独核验。

## 21. 固定内层三退出的独立 FE/SSA 身份补证（阶段记录）

- `62b4b5ce/7173f48e` 仅追加15个测试文件的独立query（14个新helper与原root八行mod，净增4502行，最大694行），原collector/native getter、六源/golden及生产均保持。新exact收集12个fresh输入；同次FE桥锁真实AST/qualified引用、constructor Type与普通函数Value、两Guard、公开OuterGuard Header/Exit owner-selector-Choice全incoming、每个实际point的完整cleanup。Return两plan指向同point时按唯一point核完整10动作序列，不重复拼接；unit仅用自身公开事实，getter一致性不作为第二份独立存储。查询错误仍保留真实lower/SSA verifier/LLVM阶段。
- 首次有限check因query_ssa两处临时数组借用报E0716，未执行行为；日志 `2a43ee37027086d97107c1e9ca35f76a3cf62b3c85e1108b95756568db8656bb` 保留。`941da637/75ed7dac` 仅将原C与B/R incoming数组绑定到两个局部let，再按原条件借slice，集合和判据不变；check-v2 24.38秒无警告。首次实际identity1-v2收据 `f8b40287de4f449c77f89b549ed817caa0d8f5f13439ca256c2c4868673d2a72` 实际1/0，编译54.35秒、运行0.40秒；12格共776项检查全真，同次FE桥、SSA实体闭包和原verifier/LLVM全部通过，5raw及2715完整输入/工具绑定已独审。
- SSA查询独立核caller、两provider lease/cursor、四component和Guard真实allocation/current角色的全部incoming及完整uses；C精确内Next后继续外循环，B实际break与false合流后恢复外角色，R实际四Borrow operand先于完整10项FE动作所对应的8条SSA清理指令，并把同Int结果返回，不造Return incoming。unit caller两个Int返回结果逐真实边运输；single不虚造这些列。single/unit清理操作各自的实际Origin分别锁定，End与Next不混用一个猜测span；空源路径允许正规的容器析构。新增证据是固定源码的静态FE/SSA身份，LLVM仍原render/verify，不增加动态释放计数、泄漏或新LLVM结构证明。
- 同头engineering-nested-exits-v3收据 `57ba9e7539229c848c399a981cbbab87aaf10ea556e73b7f4c5827597a9fcddd` 四项全0：fmt空日志，check/build各0.08秒复用，Clippy38.57秒，无后继lint修改。regression40-v1收据 `c54cb4805d7aa5ccba94253b13c3c2623a693858f0b9df7f5e3fe3ea81eaf543` 的40个唯一ordinary exact各实际1/0，严格原39完整对象加新identity一项；200raw按后端27项和前端旧13项独审，2715完整输入与工具/runner稳定。本轮复用已有测试产物，包括两个旧CLI target；原40个Map case×entry native marker及两项旧CLI native的证据范围不扩大，stagegate/whole有限成功与Group精准拒绝仍分别保留。
- 本节结清§20固定同根双层内层无条件C/B/R的独立深身份待项，行为受验头仍`941da637`，后继docs提交不冒称复跑。下一片为temporary Call source的三退出，优先复用既有hidden owner和真实清理；更一般嵌套控制流、其他合同来源及nullable/Resource分量组合、私有负控与真实双host CI仍待。0331保持active，不勾总合同，远端发布身份另核。

## 22. temporary Call source 三退出的同源 SSA/LLVM 与 native 补证（阶段记录）

- 本片保留零参顶层 Owned factory Call、String/String 两 Shared 分量，Map 与 MutableMap 的临时源均持有独立 hidden owner；六源为两种容器×无条件尾 continue/break/return。每源两个真实 Call-source For，C/B 位于 entry 顺序执行，R 分别位于两个零参 scanner，caller 精确核返回7/0；非空按 z/first、a/second 插入，空工厂仍实际调用。每个 body 有真实无字段 Resource Guard，预测 golden 分别12/9/11行，stdout不作为精确释放次数或泄漏证明。
- `ce416e0f/f55ce2f0` 安装21个tests/fixture路径，复用原同次FnOnce流水线并全采12fresh、24静态site。首次有限check因single NameReference.namespace返回Namespace而测试误比Some(namespace)报E0308，日志 `f5df103465cf2b9e571eaa64c9219c8659dff86c3f4703785942328baba7ea8f` 保留，预备first1未运行。`c6c06f6d/71ba1670` 仅去掉该single比较的Some包装，unit原Option接口不变；check-v2 24.77秒无警告。首次实际first1-v2收据 `f51eb423603ccbb6d29535aef7a45cfaed889cfef510cccf3c9513ef283257e0` 为actual1/101，编译56.07秒，12格606项前置查询全真，全部真实lower停在各源首For的UnsupportedNode，未到SSA/LLVM；第二site当时仅有FE计划证据。
- 首证实际24个plan、48个独立point的完整清理已核：C为Guard Drop→value/key End；B/R再接FinishProvider→EndSource→同source Temporary Drop；耗尽仅末三项。single source与Guard均为实际Expression owner，source origin为factory Call，Guard owner定义为构造表达式而Drop诊断origin为声明marker；每site C仅一个source Drop点，B/R两个互斥点。闭包phi/flow为空，C/B/R分别3/3/2个Always空bindings incoming，Return没有incoming；R caller仍有真实If的全局Choice，不把整个condition表误限为空。unit只核自己的公开qualified事实。
- `de6c5dc6/f34cde15` 的10文件生产片新增1075/删141，复用既有temporary carrier、source Proof.at及unit transfer_drop，不新增FE/op或通用planner。旧normal完整分支保留；新尾仅有限straight body、一个无字段Resource Guard及直接C/B/R，Return真实selected callee/双Borrow/Owned Int补查局限新分支。single Guard实际构造type/payload/current binding及整块唯一Drop独立硬核；缺事实仍失败，未用任意phi或Drop放行。有限check25.21秒无警告。integrated16-v1收据 `2fc4c84ed0e2849a265b263dbc8d9d1dbb7ff16aa71912fd6c760dc917dc6ddb` 全16 ordinary各actual1/0，首编54.65秒；新12格642检查全真且完整SSA verifier/LLVM通过，原15个normal/A/conditional/temporary/nested及身份、stagegate/whole/Group邻控保持，80raw与2738完整输入已独审。
- `5fa0831a/5eb5fc76` 仅追加4个native测试接线路径，原helper与六源/golden保持；有限check24.67秒无警告。native2-v1收据 `17116e756a492d4b2b53a153313afc089615996aed5eb5425a1406aaba57ad4e` 两exact各actual1/0，首编55.93秒、两个运行各0.35秒；12个case×entry marker均在原helper严格Some(0)、完整golden字节及空stderr断言后产生。既有helper没有内置硬超时；不扩称独立子进程raw、动态释放计数或泄漏检测。
- 同头engineering-temporary-exits-v1收据 `17321e9f583e36abe6d427da21caf61224bc14a3516fc2aaecaab41ceb4b8bec` 四项全0，fmt空日志，check0.56秒、Clippy38.26秒、build21.77秒。regression43-v1收据 `7a609dddbcbb731b0a618aec96e5d932f14a3c5c9df8dc89cc7122492f072b54` 全43唯一ordinary exact各actual1/0，严格原40完整对象加新pipeline与native两项；215raw按后端30项与前端旧13项独审，2739完整输入及工具绑定稳定。本轮大部分复用已有测试产物，旧CLI两项实际重新编译1.48/0.56秒；原stagegate/whole有限成功、Group精准拒绝及旧CLI native边界保持。
- 此处是固定temporary三退出的有界流水线/native结果，独立同次FE→SSA全实体/全incoming深query仍为后继，不把当前collector查询替代该待项。行为受验头仍`5fa0831a`，后继docs提交不冒称重跑；其他合同来源与nullable/Resource分量组合、更一般嵌套控制流、私有负控及双host CI仍待。0331保持active，不勾总合同，远端实际发布另核。

## 23. temporary 三退出的独立同次 FE→SSA 全实体身份补证（阶段记录）

- `a325aaff/4805ae6a` 仅追加11个测试路径、净增3831行，原collector/native getter完整前缀保留，新增唯一ordinary identity exact独立全采六源×双入口12fresh、24site；有限check24.02秒无警告。桥在同次实际FE→ownership→完整validator/view→SSA中核factory selected call/Owned返回、Temporary Expression owner、双Shared分量、Guard构造owner与声明origin、完整point清理、C/B/R三/三/二incoming及Return无incoming；unit仅使用自己的qualified公开事实。SSA查询独立核每site唯一factory Call结果、hidden owner/source lease/cursor/KV/Guard的全incoming与全部用途，C先清理再Next，B两终态，R先实际operand求值再清理并交付同Int结果；两个site的登记/清理严格隔离。新查询失败仍继续原SSA verifier与LLVM render/verify，不引入新的LLVM结构查询或动态释放计数。
- identity1首采收据 `50e371229b6a6c112d22ae7783d89096eb77a4cbd07676ceddba434e8ba7606b` 为actual1/101，真实编译56.40秒、运行0.30秒；12个FE桥全真，8个C/B深query通过，4个R在新增scanner Int类型检查首断，合662true/4false，12格均实际LLVM通过。原因是测试在scanner返回、operand签名、caller Int helper三处把语言Int误写signed64；实际SSA与两lower的Int映射均为signed32。`31724c5c/2f8a95a3` 只将这三个测试token改为32，cursor unsigned64及全部实体/清理/返回/7与0硬门保持，生产无变；有限check-v2 25.49秒无警告。旧首红保留，不把未到达的查询当先前已验。
- identity2收据 `521102504d41fd5a2924468ec9f1fbacd5c303ca3f29a1b692c4ce39a0b6d48c` 为actual1/0，真实test编译1分00秒、运行0.34秒；12格50/52/50/52/63/66×两容器，合666true/0false，深FE/SSA身份与原LLVM全部通过，5raw及2749完整tracked输入、工具与runner绑定已独审。同头engineering-temporary-exits-v3收据 `005e030b9bd2663decb449712e516f66c195afa31d508de8223b3db10435f74d` 四项全0：fmt空日志，check/build复用0.07/0.08秒，Clippy39.37秒。regression44-v2收据 `23fa8bbbaf4b676dce21feaa720d8a34d3bc1044c592011bdf1704b7163376ec` 全44唯一ordinary exact各实际1/0，严格原43完整对象加新identity；220raw按后端31项与前端旧13项独审，2749前后输入稳定。本轮复用已有测试产物，两个旧CLI target各0.04秒；52个Map case×entry native marker及旧CLI两项边界保持，stagegate/whole成功与Group精准拒绝仍分别记账。
- 本节只结清§22固定temporary无条件尾C/B/R的独立FE/SSA深身份待项。行为受验头为`31724c5c`，后继docs提交不冒称复跑；stdout仍不能证明动态factory/Drop精确次数或无泄漏。下一片隔离owned named local形成与循环后继续使用，其他合同来源、nullable/Resource分量、更一般控制流、私有负控与双host CI仍待。0331保持active，不勾总合同，远端实际发布另核。

## 24. owned named local 的形成与循环后读取首证（阶段记录）

- 本片以两份固定String/String源隔离local形成与后用：A/B各有Map/MutableMap×val/var四个scanner，每个scanner先非空、再空两个真实Name-source For；B逐字只比A多八条For后的size==2/0检查。四fresh共32个静态site，预测golden28/36行未执行native。真实factory仍返回Owned，body同步双Borrow；formed_owned_locals只作形成历史，当前owner与availability须由同次计划、Drop/条件、incoming及实际SSA另证，不查未发布普通local的global binding_kind。
- `3ea6700b/a01986a4` 仅安装9个测试/fixture路径、净增1919行，原FnOnce桥与旧wrapper逐字保留；有限check25.22秒无警告。first1收据 `65cb5c6b583d980d109ed4fb50ecbc2854c8421df5ff37fdea6efd81780dea4f` 为actual1/101，编译59.99秒、运行0.09秒；A两端146/156项全真，B仅7/9项真且各一项新增AST查询假，合318true/2false。B的If花括号分支由parser实际产出ControlBody，collector误按Block取元素；基础完整FE/ownership/validator仍继续，四路实际lower均在首For805..860 UnsupportedNode，未到SSA/LLVM。B当时未做逐site硬查询，不把整份产品dump等同于该查询通过。
- `b963ec74/f914dbd9` 仅给测试新增严格ControlBody accessor并替换size then/else两调用，原函数/For Block-only、源码/golden与成功oracle保持；有限check-v2 26.28秒无警告。first2收据 `f5d1b13ed5eef0f28df1491980e504b0131963c3890957214384e5d5d96f8a35` 为actual1/101，真实编译1分01秒、运行0.10秒；四格146/156/154/164合620true/0false，32site实际查询/记录齐。四路仍在provider.ko首For805..860返回UnsupportedNode，无SSA/LLVM；两个5raw集合、2757完整输入及工具/runner绑定均独审。每条pipeline只实到首停，后续site的FE证据不升级为后端到达。
- 真实表区分A耗尽清理与B后用：single A当前local在本LoopExit Drop、Exit空值/Never；B从Header向Exit运输活owner，保到真实MapSize读取并在其CallReturn Drop，随后比较/If消费size的Int结果。每scanner第二For还保留前一已死local的H/X与条件Drop，须由Entry缺席、完整回边/Exhaustion及selector/条件关系证明永不可用，不能凭binding缺失跳过或要求所有phi恰为当前source两条；B第二For还受前size If成功路径条件控制。unit仅有自己的qualified计划与A LoopExit/B size CallReturn Drop，不伪造single附加phi表。所有local初始owner均须追真实initializer Expression，Drop诊断origin仍为声明marker。上述是本次实际事实与后继实现约束，local生产资格、SSA/native、独立深身份均未交付；0331保持active，其他来源/nullable与Resource组合、私有负控及双host CI继续待验。

## 25. owned local A/B 的有界 lowering 与 native 结果（阶段记录）

- `99afd28c/d5b7079c` 安装35路径的local生产闭包，实际git差集+4152/-44，其中33生产路径净增4106行、2测试context初始化各增1行。仅支持本次真实具名val/var、Owned Map/MutableMap、普通耗尽及已锁定的后用MapSize两段函数形状；不新增FE事实或SSA操作。source由真实initializer的selected DirectCall结果形成，当前owner经完整边列运输，不能重求initializer或拿formed历史授权当前可用。A在LoopExit精确消费，B保owner到真实MapSize并在其CallReturn消费，size Int继续比较/If；single后继dead列须证明Entry缺席、仅自回边及完整Exhaustion/selector关系且前一owner已实际消费，不伪造SSA值。unit使用自身qualified事实，保持旧Parameter/Temporary的Value+Drop与Loan无Drop配对。合法超能力形状与缺错事实分别保留Unsupported/Missing边界。
- 首次有限check真实E0026：unit `Item::Function`没有body字段，原日志 `dd65678fb797d02e9f5731e47a4bbe6b46f4af8121f87691cca03605e1b60e89` 保留，未运行行为。`ecf56575/b518c252` 单文件+18/-1改为真实FunctionForm的ImplicitUnitBlock或Explicit Block，仍比较同一StatementId并要求唯一函数，其他资格不变；check-v2实际27.60秒无警告。integrated29-v1收据 `426bd768a08fbb65010f2cbcf11fbe121e478e7b07370c9554d3ecf81857c0c8` 全29唯一ordinary各actual1/0，首编1分01秒；local四报告149/159/157/167合632true/0false，真实十源函数可达、SSA verifier与LLVM render/verify全过。其余28是原44排除16个native exact后的完整SSA/LLVM对象；145raw及2770输入按后端18项、前端11项独审。32个FE site来自四fresh完整pipeline，独立local深身份query尚未加入。
- `df523c88/22b4d93f` 只追加4个tests接线路径、34行，保原两源/golden与native helper；有限check27.71秒无警告。native2-v1收据 `c47f0ff99409a47d5fe5ea4bae4c6952ab48d05e72e3c7a6e8146302cb3c6417` 两exact各actual1/0，首编1分03秒，single/unit运行0.20/0.19秒；四个case×entry marker在原helper严格Some(0)、A28行300字节/B36行356字节完整golden及空stderr断言后产生。helper没有内置硬超时，输出不证明精确factory或Drop动态次数、无泄漏，也不扩称独立子进程raw。
- 同头engineering-local-v1收据 `6c20556f3a12f04b232d017bb1342ba9720ce1c68865c9ea46bd7c8051a68bd0` 四项全0，fmt空日志、check0.61秒、Clippy42.76秒、build22.71秒。regression47-v1收据 `9a877cce51edb9901fe649618feb9ecc6aca0d6c7d454a6383aa43f81812bd73` 全47唯一ordinary各actual1/0，严格原44完整对象加local pipeline与native两项；235raw与2771完整输入按后端34项、前端旧13项独审，全部绑定稳定。56个Map case×entry marker齐，旧CLI两个正常native另计；CLI本轮实际重新编译1.44/0.60秒，其余45项复用已有测试产物。原有限stagegate/whole成功、Group精准拒绝及旧CLI范围保留，不以拒绝oracle通过宣称对应来源已支持。
- 本段只验收固定local A/B流水线/native与同头普通回归；同次FE→SSA独立全实体、全incoming、所有用途及B后用清理身份query仍为后继，原collector检查不替代它。行为受验头为`df523c88`，后继docs提交不冒称重跑。其他来源、nullable/Resource分量组合、更一般控制流、私有负控与双host CI仍待；0331保持active，远端备份范围另按真实ref记录。

## 26. Mac 固定远端快照复验与文档差距（2026-10-18）

- 本机主仓库与已有工作树先检查，保留主仓库全部未提交工作及其他工作树；独立 worktree
  `/private/tmp/koven-spec0331-mac` 从重新 `ls-remote` 核实的0331远端
  `60512fda0677b07abb77f10d88242f7a16edd1fb` 建立，tree
  `a800dd86dd72dc2dd3c87331fc83f608201a7564`。同次远端 main 为
  `a2a7e4b6833977fcae26d6420f193ea1ac7d53df`，不以旧报告代替远端核实。
- 工具为 AArch64 macOS、Rust/cargo1.96.0、Homebrew LLVM/Clang21.1.8；显式
  `LLVM_SYS_211_PREFIX=/opt/homebrew/opt/llvm@21`，普通 object 使用系统
  `/usr/bin/clang`（Apple21.0.0）链接，符合平台 driver 合同。未全局升级工具或修改代理。
- 固定快照的31项 Map provider exact（15项pipeline/身份+16项native）与13项共享回归
  （3项stage/whole/Group，3项type_iteration，7项ownership_iteration）各实际1/exit0，
  合44项通过、0失败、0 ignored；每次都核 libtest 实际命中，不以runner退出码替代。
  统一命令形态为 `cargo test --locked --offline -p <crate> --lib <exact>` 或
  `--test <suite> <exact>`，尾部均为 `-- --exact --nocapture --test-threads=1`，串行执行。
  native56个case×entry均硬核exit0/完整golden/空stderr；不增加动态计数或泄漏结论。
  31项运行前后2771个tracked输入摘要一致；13项回归仍在同一未改源码快照完成。
- 本机原始日志与逐项命令/结果留仓外 `/tmp/koven-spec0331-mac-evidence/`，简要收据另存
  本机任务目录；codegen-results.json SHA256
  `3e7c7a6bfeea09bfce0e4209601b540343476e4d06e34cba7a9045ee9a29980a`，
  regression-results.json SHA256
  `eedc845ddea3a9abf247af9eb6185bc97bf4f7403c9c4fa55a0ee7791b4e9923`。
  结果仅证明上述受验快照；后继文档提交不冒称生产重跑。README双语及Tour原先笼统
  “Map遍历未交付”已改为[有限矩阵](../../architecture/map-borrowed-iteration.md)，不增加
  fence/CLI库存、改变Guide或扩大Field/Inout/Group与其他staging边界。
- 云后续13个提交对象本机均missing，包含c5bf4120和707d9b97。恢复中的闭包生产补丁
  已在仓外按五文件base和全部七文件candidate SHA核同，重建patch SHA256为
  `a9847b02ed483bb08d29d78c9ba6fa08af5568b1ce210555108933174d3c8da8`；
  这不是完整父树/提交恢复，也尚未安装或执行。Borrow直接退出、Resource生产扩展、
  local深查询、原20拒绝/10邻控及最后四源Unit结果不能从摘要升级为本机通过。
- 本机未运行故障注入、校准、合成损坏、sanitizer故障对照；这些只交GitHub Actions。
  macOS sanitizer仍为partial，本轮未新增sanitizer验收。完整工程/双host远端CI、独立审计
  及F最终闭环尚待，不归档0331、不勾总合同；D10未开始。

## 27. Unit 闭包生产补丁逐字恢复与本机新回归（2026-10-18）

- 原作者和独立审查者提供两新文件全文、五旧文件完整diff及base/candidate SHA；五个
  旧base与本机固定快照逐字核同，七个candidate全部匹配。按原difflib算法重建的生产patch
  SHA256精确为`a9847b02ed483bb08d29d78c9ba6fa08af5568b1ce210555108933174d3c8da8`。
  独立恢复提交`36a60f62c6f685e360ea34d55942652126143502`只含七生产文件；原c5bf
  commit及完整父树仍未恢复，不能冒称恢复tree `4b2b58f3`或云最后707d9b97。
- 原四份普通源码由传输文本逐字保留，另写`unit_closure_assignment_recovery` harness，
  明确不是原532行collector恢复；保留普通`() -> Unit`、Resource的`deinit()`与
  OwnedReplacement精确`AfterExpression(RHS)`判据。首编因外部不可构造UnitSymbolId报
  E0624，未运行；仅改为从同次公开qualified name reference取真实ID后，实际0通过/4失败。
  tests-only保全提交`9ae17f82`先于生产恢复。安装七文件后实际3通过/1失败；唯一旧Resource
  capture仍在声明AfterStatement过早析构。没有更换源码、改move函数类型或放宽RHS oracle。
- 新窄修`fbb81c0d120b4f634845ce9ae785097950f677c1`只给unit planner的is_asap_owner
  增加8行：已知具体closure环境先核实际owned captures的Resource分类，随后保留原静态
  类型fallback。它是原补丁之外的新实现；原普通函数类型的non-resource答案不能抹掉实际
  capture义务。四源回归4/0，另multifile_ownership_checking135/0与ownership_closures39/0，
  三target合178通过、0失败、0ignored；这些于提交前相同源码工作树执行，随后保存窄修。
- `fbb81c0d`再执行§26完全相同的44个ordinary exact，均实际1/exit0，native56个产物
  仍严格exit0/golden/空stderr；31项provider运行前后2775个tracked输入摘要一致。
  合计该版源码222个不同测试通过，不等于frontend全量或完整CI。四项工程亦通过：
  `cargo fmt --all -- --check`；`cargo check --locked --offline --workspace --all-targets`；
  `cargo clippy --locked --offline --workspace --all-targets -- -D warnings`；
  `cargo build --locked --offline --workspace`。没有执行CI-only负控或故障helper。
- 逐项命令与结果在仓外`post-recovery`收据中；codegen-results SHA256
  `0a981c915657d039d0f382d5b70710adade9d804c9add46198e75f1553907c4b`，
  regression-results SHA256
  `866de98642b95006b3774e7d88430a38e2b52f33717a511326962695a0f17c05`，
  engineering-results SHA256
  `84e2a415a37cc781873b2e7698cc64ae433fcfc514bfcf48a01229ccdfa1eaa0`。
  简要收据与`git format-patch --binary --full-index`备份保留本机任务目录
  `/Users/ckfei/Documents/Codex/2026-10-18/task/spec0331-mac-evidence/`；大体积raw留仓外，
  不提交或上传。生产旧尺寸欠账drop_planner.rs由1628降至1492行，dataflow.rs由1206降至
  1092行，仍超过1000软限；两新模块160/161行、新harness227行，未扩旧大文件欠账。
- 依据本机上层AGENTS §15做fresh-context只读独审，范围为恢复七文件、新8行、四源
  新harness及文档，未发现该窄片的新增阻断问题。审查指出继承风险：外层closure拥有普通函数类型inner，inner实际拥有
  Resource时，直接capture.ty仍被判Pure，形成外层时inner origin又已被take删除；该普通
  嵌套源未见前置gate阻挡，但本轮未执行，不称已复现动态提前析构。它早于两片补丁存在，
  不把限定四源窄修升级为一般递归环境资源分类；后继须独立普通首红与来源证明。
- Borrow直接退出/Resource后端扩展、local深查询、原active20与10合法邻控完整内容仍缺，
  本机新四源/现有共享回归不能替代它们。完整双host远端CI、独立审计剩余边界及F最终验收
  另记；0331继续active，Mac sanitizer仍partial，D10未开始。


## 28. 本机重建闭包安全矩阵与已知嵌套 Resource 修复（2026-10-18）

- §27 的嵌套风险已转为真实普通源码首红：inner 的 move 环境拥有 Resource，outer 的
  普通 `() -> Unit` 环境再拥有 inner。固定60512与恢复后6ef两版均在完整 RHS 前清理；
  另一个词法退出源也在最后调用点过早清理。三项新邻控首轮1通过/2失败，包含带无关
  Resource 声明的 Pure 环境 ASAP 邻控，不用类型名称或源码形状代替资源证明。
- `b692d2eb` 在捕获 Consume 前读取实际环境资源，随当前 binding 保存具体 closure
  的 ASAP 证明；赋值、移动和相同来源合流同步维护，缺证明保守不 ASAP，不把历史
  lambda AST 缓存当当前可用性。Resource 两源分别严格核 AfterExpression(RHS) 与
  词法 AfterStatement，Pure 邻控保留原 ASAP；三项全过。opaque nullable/容器包装
  函数环境的一般资源分类仍未覆盖，不能升级为任意递归环境支持。
- `closure_assignment_safety_rebuild` 是新写的15份普通源码，非原 active20/10 恢复。
  十份拒绝各走 single/unit，要求唯一原诊断码及精确原 Span、执行事实撤销且不可资格化；
  五份合法邻控各走两入口，要求捕获事实和同次身份资格。literal/alias/self replacement、
  被捕获源 move/mutation、Map K/V owned delivery/move capture 与 closure owned escape/
  field storage 均有明确来源。固定60512同 harness 为10通过/5失败；本机修复后15/0，
  即20项拒绝和10项合法观察，不把内部观察数冒作30个Rust测试。
- 审计要求补全执行事实 registry 和 single 原分析身份。错误地要求 cleanup nodes 为空
  曾得5通过/10失败；默认表本来保留 Always/Never，因此改为完整 Default 结构相等，
  含所有 owner/selector/slot 内部表；还核常量物化能力已撤销。旧失败日志保留。
  最终新矩阵15/0、multifile135/0、closure39/0、恢复/嵌套7/0，共196个Rust测试通过。
- 受验源码`fb156025ed2c365f0572c9a244a3cb486cf73df1`（tree `50da465194d529445b1bd6ad01a0ae85de1305a6`）
  再执行§26同44个ordinary exact，均实际1/exit0，native56产物仍硬核exit0/golden/
  空stderr；31项provider运行前后tracked输入摘要相同。合240个不同Rust测试，0失败/
  0ignored；不是完整前端/全CI，也不证明native closure或精确动态释放次数。
- 新两target接入stage无过滤且各一次；与composition合114个唯一frontend integration、
  10次Cargo调用（修正原文105的旧计数）。仅运行对应纯静态Python选择合同1项通过；
  不在本机执行整个含故障对照的policy suite。完整远端双host CI另记，不提前勾通过。
- 同版四项工程fmt/check/clippy/build全通过，Clippy使用workspace/all-targets及-D warnings；
  文档704页检查通过，Rust尺寸门相对main通过，仍报告38个旧超限文件，不清零旧欠账。
- fresh-context只读独审覆盖shared资源证明运输、恢复范围、事实/身份oracle、CI接线；
  已修并复审所有本片finding，未发现新阻断问题。主仓库既有脏状态摘要保持不变。
  raw留仓外current-final/publication，简要命令/收据与binary/full-index格式补丁保留
  本机任务目录；不上传大日志。Borrow直接退出、Resource后端扩展、local深查询仍待。
  F继续active；未运行本机故障注入或sanitizer对照，macOS sanitizer仍partial，D10未开始。

- 首次当前head PR119 run39097655959真实失败：docs policy 的 composition 总数仍锁112，
  实际新增两目标后为114；389项Python中1项失败。clippy/test/四preview均跳过，
  code-evidence和CI Passed失败，不能计通过。修正总数并追加两目标各一次硬断言；
  该helper含stub与失败注入分支，只交远端执行，本机不运行其整个class/suite。


## 29. PR 完整范围核对与本机 Borrow Map 直接退出重建（2026-10-18）

- 重新读取远端：main仍`a2a7e4b`，固定云备份仍`60512fda`，tree仍`a800dd86`。
  PR119的base为同一main；至`6f5e33aa`真实范围是云备份96个Git提交加本机14个，
  共110个，净371路径。固定备份是PR祖先，不遗漏此前未合入0331功能。检查所有主题
  和净路径，包含Map迭代、必要pattern诊断/entry Borrow回归修复、闭包恢复、文档及CI；
  Guide无改动。“云53提交”尚无逐项原身份映射，不能据此声称本机恢复53个原提交。
  完整SHA/路径清单保留任务目录`pr119-scope-audit.json`，大清单不复制入仓。
- 四份重建源码来自现有Map无条件C/B/R及conditional fixture，只去scan参数的own，
  同字节golden保持；并非原云collector恢复。首编两处DropTarget误取引用报E0308，
  仅修测试API调用后，`55ad1565`真实1测试失败：八路FE/ownership/完整validator与
  新前置检查全真，随后各在原For的UnsupportedNode停止，没有SSA/LLVM通过。
- `9cbec0b7`只删除single与unit各一条Borrow Map专属旧禁用guard（合14行）。来源
  整行资格、固定尾/conditional形状、String/String限制与完整cleanup消费保持；沿既有
  Shared source路径运输caller能力，不改变Guide，不开放Field/Inout/Group/直接Borrow
  return来源或一般CFG。补片后同一八路pipeline全部到达SSA verifier及LLVM验证。
- 独审发现测试先从plan自取root且未核caller交接，已补独立原names/source reference
  到scan实际Borrow参数root的比对；2/5个原caller调用各核真实selected scan、Borrow
  参数映射、Shared loan及原argument root，unit另核SharedLoan合同且无Move delivery。
  补强后exact实际1/0，八路全部检查通过，不称全实体/全incoming深query。
- `47948fd2`接线两项普通native测试，四源×双入口八产物全部exit0/完整golden/空stderr；
  沿用原helper，外层bounded运行未超时，不添加故障/计数/sanitizer对照。旧§26同44项
  ordinary exact同生产源码复验，各实际1/0，56个旧native marker齐且provider运行前后
  tracked摘要稳定。本片新增3个Rust测试，不将8条pipeline或8产物当8个独立Rust测试。
  Resource分量后端与local深身份仍待；远端CI由主线程独立监视，未据新提交预报通过。

- 本片最终fresh-context复审确认来源/caller证据finding关闭，未发现该有界组合新阻断；
  fmt/check/clippy(-D warnings)/build四项全通过，文档704页检查通过。旧日志和新的逐项
  收据、格式补丁均保留任务仓外；不据stdout扩称无泄漏或精确动态析构次数。


## 30. 本机 Resource Map 正常耗尽重建（2026-10-18）

- `c38dba63` 两份普通源码先有真实后端首红；`e783c00e` 仅接通 own Map 的直接
  String/无字段、无泛型或接口的 deinit class value、正常耗尽与固定两语句 scope。single 核公开 phi 的
  Header/Exit、全 incoming、Choice 与唯一 AfterStatement(scope) Drop，沿真实 carrier
  保留当前 owner；unit 独立使用 qualified scope fact。Borrow MutableMap 复用原
  Shared 路径，不清理 caller owner。新增两个生产模块分别约320/200行，不合并双 lower。
- `e862fc0a` 增两项普通 native 测试、四产物全部 exit0/golden/空stderr。源码与实现是
  本机有界重建，未取得云原 Resource 补丁，不称原提交恢复。owned Map 的continue、nullable 与额外
  continuation 三份合法邻控×双入口均 FE/ownership/完整validator成功后，在原 For
  精确 UnsupportedNode；`98125167` 修正两条 scope-helper 的错误诊断 Span。
- `8e790b4c` 含两项 Resource Rust 测试：增强原正例四路同次只读查询，新增六条边界观察。原 AST 参数/
  reference root、实际 caller selected signature/Value或Shared交接、原root Move/loan，
  single原 phi/incoming/Choice/词法Drop 与 unit自身事实均独立核验。SSA 按原scan函数
  查询所有实际入边、owner/source/cursor/两分量、read的真实operand及value→key结束顺序；
  false边 source结束先于marker，owned唯一Drop消费当前false owner并在marker后。
  单Borrow支配入口loan不冒充transport列；所有保护entity核完整instruction使用集合。
- 修复测试API误用 `UnitCallableSignature.declaration()`，改从外层真实 declaration
  signature取身份；后续测试曾真实失败于marker操作种类、Borrow seed列与诊断Span，
  均保存原日志、按公开API/原源码校正后复跑。未以collector exit0冒充全部检查通过。
- 固定 `8e790b4c8382b9fac047ac35d3e449752e6055d1` / tree
  `dd7f444f64d0eceea7659a3d65dc83eb083723d6` 串行51项 ordinary exact全部通过，
  0失败/忽略；tracked输入运行前后SHA256均
  `f84adbd88cbb1378267ecb3ce0a148235a2ee1f4c9d60f47f83948484ab963fd`。
  其中20项native测试共68产物（原56、Borrow8、Resource4）；17项pipeline/身份、
  3项阶段/whole/Group、10项前端邻控及1项Resource边界测试分别记账。
- fresh-context独审覆盖生产资格/lexical owner运输、同次FE和caller身份、所有SSA
  incoming/operand、真实邻控边界；finding均修复复审，无本片新阻断。此前PR119
  `a2cf1f27` 的绿色CI不覆盖本片；local深查询和新head完整远端CI仍待。F保持active，
  macOS sanitizer保持partial，未执行本机故障对照，不进入D10。

- 最终工程 fmt/check/clippy(workspace/all-targets、-D warnings)/build 四项通过；初次
  fmt发现一处测试reexport顺序，机械修正后重验。704页文档检查与相对main的Rust
  尺寸门通过，仍报告38个旧超限文件。大日志留仓外，格式补丁保留任务目录。


## 31. local A/B 深查询与固定头有限复验（2026-10-18）

- `37f8fb50` 仅增测试查询，无生产修改；原两份fixture字节及golden保持。新增一项
  exact Rust 测试，执行两源×双入口四次fresh完整pipeline，每次八个For site，不能
  将32个site记作32次lower或32项Rust测试；原collector独立保留原成功条件。
- single 由原AST/name声明及source reference取得真实root，核selected factory形成的
  Expression owner、全部Header/Exit、三类incoming、完整写集/Choice与路径条件；
  A消费LoopExit当前Header Drop，B保活至原size的CallReturn Exit Drop。第二循环
  前序死root独立核Never Entry、假presence回边和精确条件Drop，不能复活为SSA列。
  unit仅查询自身qualified root、唯一last-use Drop及完整exhaustion清理，不拼裸symbol。
- 独立SSA按原函数/For/factory/read span定位，逐所有真实入边核owner/source/cursor
  列与全参数集合、key/value Shared loan、read后value→key结束再Next、EndSource后
  当前owner Drop。B核原size当前false owner、i32结果/常量、比较、原If条件及完整
  true/Abort incoming；保护owner/loan/cursor/component/size/常量全部指令用途，布尔
  control另核仅真实Conditional消费。恰两个Map Drop，旧root无额外列或重复Drop。
- 新查询曾真实编译/API失败及三次查询失败：误用SSA API、phi origin应为For、第二
  loop受前一If路径控制、Exhaustion availability与edge路径分离；修正基于公开事实与
  原AST。v7调用无公开getter的私有字段未编译，去掉多余断言后v8实际1/0/0通过。
  fresh-context审计再发现死root Drop、比较operand及共享Abort放宽三项缺口，均补强
  并复审关闭；Graph::new原合同不变，仅B显式用with_abort。未执行任何损坏/注入对照。
- 固定HEAD `37f8fb504b4eea4062b27a685618a09b063ab997` / tree
  `00e6c49494831c0f084737a3a23935617cf4c3a2` 串行52项ordinary exact实际全部通过，
  0失败/忽略；各命中1项，未以collector退出码代替逐项结果。前后2804个tracked文件
  输入摘要均 `8e721ef0b29886af5142c0b8c32fe6735e5319d0b2915789d458dcd8209cfca6`。
  分账为18项pipeline/身份、20项native（68产物）、3项stage/whole/Group、10项前端
  邻控及1项Resource边界；原196项前端套件未在该头重跑，保留§28原固定头收据。
- README双语已有的有限后端边界保持，Tour增加两份新回归来源指针，未新增公开CLI
  语义。Resource已备份至远端`b466c036`；新头完整CI由主线程独立监视另记，不能继承
  `a2cf1f27`的旧绿色结果。F仍active，macOS sanitizer为partial；不进入D10。

- 最终四项工程门 fmt/check/clippy(workspace/all-targets、-D warnings)/build 通过。
  704页文档检查通过；首轮Tour新链接遗漏borrow已修正重验。Rust尺寸门相对main通过，
  仍保留38个历史超限文件，新查询模块均低于1000行。日志仓外，binary/full-index
  格式补丁及逐项稳定收据保留本机任务目录，不上传大日志。

## 32. F 收尾 CI 消费者补选（2026-10-18）

- 归档前逐条对账发现stage未选三个共享ownership消费者：constants、structural、
  nullable_when。helper逐项分类为真实合法/非法源码的普通前端分析与只读facts；
  structural的TypeEnvironment为正常输入设置，不合成或损坏已有产物。46项普通测试
  在当前生产源码实际16/26/4通过，0失败/忽略；未运行故障、校准或sanitizer对照。
- 先加独立纯静态选择测试，只读取真实stage文本，首次三项均缺失而失败；补三target
  后同测试通过。完整composition从114变117个唯一frontend integration，仍10次Cargo
  调用；含stub/失败注入的composition suite仅交CI，本机未运行。
- 此片只补CI执行选择；原active20/10历史全文仍不能映射，后续按Guide安全义务建立
  可审查新矩阵，不把原字节缺失当永久阻断，也不声称恢复或等价。真实provider顺序
  历史native与Map专用私有负控分别补证；新head完整双host CI另记，F不标done或合并。

## 33. 真实 provider 顺序历史与 owner 普通验收（2026-10-18）

- 新增普通source集合21份：单项/覆盖保位/删除重插置尾/20键增长/碰撞墓碑五历史，
  各走own Map、Borrow Map及Borrow MutableMap；Resource仅前三种不涉及remove的
  singleton/overwrite/growth，各走own Map/Borrow MutableMap，共15+6。不扩Resource
  remove的nullable结果路径；已有empty、各退出与嵌套证据保持原集合。
- 源码直接调用put/remove/consume再实际For；20个不同键按当前阈值触发16→32扩容，
  a/q/A初始碰撞且删除a/A留下墓碑。golden按独立逻辑顺序及marker合同指定，不读取
  编译产物推期望；同一实际source集合完整交给两入口pipeline与独立普通native。
- pipeline首次42格全部完整FE/validator/SSA/LLVM成功；sourcebuilder等价简化后重跑
  仍42格通过。两项native测试42产物逐个exit0/完整golden/空stderr，0失败/忽略。
  Resource覆盖的旧值在before-scan前清理；own剩余值在exhausted后/after-scan前，
  Borrow剩余值在after-scan后清理；marker证明聚合数量及区间，不冒称逐对象身份或
  跨entry析构顺序，也不替代Linux动态检测或内存计数。
- fresh-context独审核实际增长/墓碑、三source模式、完整集合与Resource边界，无新
  阻断。fmt/check/clippy(workspace/all-targets、-D warnings)/build四项通过。Map私有
  负控、新安全义务矩阵及当前head完整CI仍待，F继续active，不合并或进入D10。

## 34. Map 私有拒绝合同的 CI 注册（2026-10-18）

- 新增三项 CI 专用 Rust 测试，正常 lib 模块注册：single/unit 各八种前端私有
  Map 事实损坏，以及两入口各八种真实 lowering 后 SSA 损坏，共拟执行32次拒绝观察。
  基线均来自真实合法源码和完整准入，每个 fault 从原产物恢复后单独损坏；不手造
  Map 计划来替代生产 checker，也不把验证器的修复视作成功。
- 前端核缺计划、外源/loan、K/V类型与binding交换、缺Exhaustion、结束顺序及提前
  EndSource；single 精确首个拒绝原因及原For span，unit 在实际header/完整cleanup
  guard拒绝且不能发布owned capability，拒绝后保留恢复输入。
- SSA 核错误游标类型、伪造游标来源、错误source类型、提前EndSource、key或value
  活跃时Next、错误K分量类型，以及同类型新lease配原cursor的来源不匹配。精确核
  原指令ID、origin、错误kind/reason；新增合法当前块reborrow避免靠遗漏CFG运输拒绝。
- 独审发现并修正缺Exhaustion的首序oracle（先拒绝孤立cleanup），并补同类型lease
  门，未发现需生产修复的证据。既有core脚本的frontend `--lib`、codegen全选会在
  双host CI消费三项，不新增忽略或本机跳过分支。
- 本机只做fmt、两crate测试编译、clippy/all-targets/-D warnings、no-run和list注册
  核对；没有运行上述反例。32次是待CI执行量，不记作本机通过数。新head远端完整
  CI及Linux动态证据另记，macOS sanitizer继续partial；F仍active。

## 35. Guide 派生闭包生命周期义务与 Unit 修复（2026-10-18）

原active20/10全文不能恢复；本节按现行Guide建立新验收，不声称字节恢复或等价。

| 现行义务 | 新普通源码矩阵与硬oracle |
|---|---|
| Closure capture身份与普通闭包/Map分量不得逃逸 | K/V return；外层local跨fallthrough、break、continue；精确L0137/source span、全部执行facts清空、不能qualified |
| 活跃Map source保护、return operand先交付 | 可变本地put/remove/consume、own参数release、return release(source)，精确L0135原source；break后put/remove/consume合法 |
| Map退出结束本轮capture再分量 | K↔V替换各走B/C/R；同次真实lambda/For binding Shared/Borrow，原ControlTransfer，capture→value→key；B/R再finish→source，Continue保留provider |
| ASAP与动态环境身份 | 旧Pure闭包在原声明AfterStatement唯一释放，新值在ControlTransfer/正常saved() CallReturn各唯一；single另核实际Create owner、slot、address、place value及无额外condition，不虚造Unit字段 |
| 嵌套捕获按形成时环境传递 | K/V借用经move外层return/Value、直接literal、三层环境、形成后重绑定双向邻控；外层活过回边/B/C拒绝，同body调用完成合法 |
| 分支选择与最后使用 | 分支赋值/条件initializer保留所有来源；普通own String在outer()前take冲突、后take合法，含If/When局部tail移交；不让Map自身source loan掩盖嵌套释放 |

- 新测试首次发现真实Unit错误：K/V×return/Value四源single拒绝，Unit四源均无诊断且
  错误发布能力；完整8观察真实红已保留。进一步branch赋值两源同样single拒、Unit
  错误接受；合法own String条件结果调用后take又暴露Unit过长借用。它们区别于此前
  fixture括号/mutable source、诊断span及Map EndBinding oracle修正，不混记生产红。
- 窄修仅Unit checker：形成时冻结source-qualified嵌套环境；移动后转移binding关系，
  分支保留可能来源与实际控制结果，escape及跨provider检查递归来源图；最后使用
  按活跃根可达性释放，控制结果移交后去掉失效局部根。失败路径保留原回滚规则。
  不改Guide、lowering或Field/Inout，不合并D10；现有Resource/Pure cleanup分界保持。
- 共享普通harness只搬原15项的完整执行facts registry和同次analysis准入；原15源码、
  拒绝元组及测试名保持。新矩阵显式进入stage，唯一frontend integration数118，仍
  10次Cargo调用；纯静态选择检查先红后绿，stub/失败注入suite仍只交CI。
- 独审进一步要求控制交付闭合：实际普通源证明过宽dead-root清扫会提前释放已求值
  callee，现严格只交接已移动且不再活跃的根；临时Borrow实参在CallReturn按实际控制
  尾值释放，不能只扫描字面lambda。两项保留真实红与邻控绿，属于本轮审计修正，
  不冒称旧远端四源缺陷。形成结果表只记录callable表达式，不给普通标量增加空表项。
- 当前普通前端五target实际214项全部通过：旧matrix15、新matrix18、多文件135、
  closure39、Unit recovery7，0失败/忽略。新18项包含56源×两入口112观察；原15项
  另30观察，Unit7保持各自输入。后续固定实现头工程/normal SSA/native复验另记，
  故障反例和sanitizer导出只编译，实际CI不得以收集器exit0代替逐项通过。

## 36. Resource provider 的 Linux 动态验收接线（2026-10-18）

- CI 新导出一项exact Rust test，消费§33六份原Resource history的single/unit实际
  完整FE/validator/SSA/LLVM和C entry，共12程序。每项先plain严格exit0、完整golden、
  空stderr，再导出raw与所有Koven definition带ASan属性的IR；保存源码/golden/函数表、
  plain命令/状态/stdout/stderr，不改写原IR去模拟provider成功。
- Linux脚本精确校验12个唯一case；动态执行前先写全部输入SHA账本，失败/超时保留
  身份。每项分别运行ASan与原raw IR经C worker/join adapter的默认根扫描LSan，计划
  12+12动态产物。沿用现有有界helper、strict oracle和完整CI artifact上传。
- fresh独审的“失败前未写身份账本”finding已修复复审；静态选择测试实际通过。
  本机仅编译/静态检查，未运行导出test或sanitizer对照；24产物均待Linux CI实测，
  不计本机pass，macOS sanitizer仍partial，UBSan仍未覆盖。F不归档、不合并、不进入D10。

## 37. CI 私有反例的无损基线纠正（2026-10-18）

- `52dc7720` 的Linux run `39104193603` 实测codegen 2146通过/1失败；private exact
  的single八项已精确拒绝，随后Unit在第二个For的baseline就遇UnsupportedNode，
  未到Unit损坏观察。后置word-frequency/sanitizer/generated-owner产物未生成，不能
  继承旧绿色或计作通过。CI原红由主线程独立保留。
- 普通无损exact分离后，双Map都改String/String仍在Unit第二个仅读value的For失败；
  因此不能把原因只归为Int key。本片不扩这个组合，基线改为已支持的一参Map、
  单provider且key/value都读取的真实源，保留两入口完整FE/validator/SSA/LLVM成功前置。
  新ordinary exact本机实际1通过/0失败/忽略，含两次完整pipeline；未执行损坏对照。
- 八维私有拒绝保留：fault2仅将原ValueLoan.source替为同块已定义且活跃的真实key
  String loan，精确原ValueLoan指令/For origin/类型合同错误；不再宣称它覆盖另一
  泛型Map变体。同型新lease的平衡合法前置及原cursor不匹配oracle保持，其余六维
  保持。fresh独审确认不依赖第二个provider，不改成宽泛Err或绕过能力门。
- 此前固定`b998178a`/tree `39f7a42c2fdd502ba7d1fbdcda203f8533e1dde2`实际315项
  ordinary Rust通过，0失败/忽略，22项native共110产物；四工程门全部通过。前后
  2812个tracked输入摘要相同：
  `36102a802121e921e3e44633fb8069c2731958bb49f340d16e40c572ba7b274d`。
  新test-only基线修复的当前完整远端CI另记；Mac仍partial，F继续active。

## 38. 清洁 Resource ASan 构造失败修复（2026-10-18）

- `b4312439` 的 run `39106217801` Linux 在首个 Resource singleton/single 的清洁
  ASan 产物退出86，stdout只有before-factory，stderr为DEADLYSIGNAL/nested bug。
  证据artifact `12128673371` ZIP SHA256为
  `4d707fed6cb8e188642cfb16f2664a7c28f4f1684ae55a8e1dbbf8430125b6c3`；本机只读解包与
  反汇编，不运行下载产物。Mac generated contracts步骤成功但证据上传CreateArtifact
  ENOTFOUND，不能据此声称校准artifact已核；Preview均skipped，两类失败分别保留。
- 原始LLVM给空Resource执行`malloc(1)`后仍store空struct。失败ELF的factory在
  `0x11711f`计算ptr-1，`0x11719e`调用size=0的`__asan_report_store_n`；这是分配的左
  redzone，位置在Map插入前，与输出停点一致。报告器nested bug内部无栈，不进一步推定。
  槽位56字节、初始16槽896字节、Resource指针、consume及drop静态核查无对应前置错误。
- runtime保留真实target payload size，以max(size,1)分配，为零时省去物理初始化store。
  唯一handle、OOM、deinit/free、原源码/golden及ASan属性保持。现有普通heap IR exact先
  实测0通过/1失败；修复后runtime 5通过、普通历史native 2通过，均0失败/忽略，后者
  为原21源×双入口42产物。非零payload初始化store及两个空owner的独立分配仍被核验。
- fresh只读独审核最小修复无新增阻断；另指出既有Rc独立control block仍有空payload
  store风险，本片不宣称该相邻路径或所有零字节访问已闭合。
- 新头的工程门与完整远端CI须重新绑定，旧315/316项收据不自动继承。未在Mac执行
  exporter、故障、校准或sanitizer动态检查；Mac仍partial，F继续active，D10未开始。

## 39. 连续 Borrow Map For 的当前视图运输（2026-10-18）

- §37登记的双For缺口实际也影响反序与同参数重复。新普通十源固定覆盖
  Map/MutableMap String/String的distinct/repeat/reverse六格，Map再加首轮empty、
  Guard+break/continue、第二参数Int/String四格。修正测试源码与Continue查询oracle后，
  在精确`c5a1e99b`生产基线上重新记录有效首红：single十路完整FE/SSA/LLVM及查询
  全成功，unit十路均在第二个原For lower UnsupportedNode；Rust结果0通过/1失败。
- 窄修仅改Unit的source/carrier两生产文件。Shared参数从qualified root的当前view
  建立lease，以既有transport核完整incoming到原entry anchor；不把entry loan送第二轮。
  birth、phase与backedge均复核运输；类型、原source fact及caller权限门不放宽。
  Owned参数仍保留原能力门，未改变Guide或扩大Field/Inout及一般CFG。
- 同次公开FE查询按原reference/parameter slot核Borrow root、两分量来源、完整cleanup
  与无callee owner Drop；独立SSA按原AST For/read declaration/call span核真实callee、
  receiver与实参，全入边列、root/current view/source/cursor/K/V的全部受保护operand
  uses，以及Guard Drop→value End→key End→Next/EndSource。Continue取原transfer span。
- 修复后新pipeline exact 1通过/0失败，20/20观察全部成功；native两个exact均通过，
  共20产物exit0/golden/空stderr。Mutable caller之后put及两种caller size检查正常。
  fresh独审补强callee/全部uses及纠正首红证据后复审通过，无本片新增阻断。
  当前头的完整普通回归、工程门和远端CI另记，不继承c5a1的绿色；F仍active，D10未开始。

## 40. c5a1 双host动态产物内容核验（2026-10-18）

- run `39108060131` 的Targeted双host成功；受验源码头为`c5a1e99b`，synthetic checkout
  为`510f7acbf71c73dd2ece8bbf35c2561a1b3d2ea8`。三个ZIP核对来源SHA后仅只读解包，
  不在本机执行任何下载产物、故障或校准程序；整个run/Preview终态另由CI观察者记录。
- Linux sanitizer artifact `12129547197` 内12组五文件SHA全部匹配，原plain及各
  ASan/LSan的24份result/stdout/stderr均严格exit0/golden/空stderr；每组九个真实Koven
  definition保留插桩，LSan为default roots与原worker/join adapter。generic clean与
  user/runtime/drop ASan精准错误及leak LSan/disabled对照也逐实际输出核验。UBSan未覆盖。
- Linux generated-owner artifact `12129815900` 的16组input SHA与逐case verdict核验：
  八合法×counter/ASan/LSan共24产物正常，八非法精确ownership码/primary/label匹配。
  calibration为pass，两个clean正常；address、leak、missing_deinit、premature free
  均有对应实际故障输出；checker green/red及恢复SHA吻合。reduction明确为受控故障
  缩减能力验证，不冒称现存编译器缺陷。
- Mac artifact `12129144551` 的同16病例与八个普通counter产物吻合；calibration为
  partial：address ASan skipped且detector-off已核，leak仅counter、LSan skipped。
  acceptance为partial、requirements_met=false；missing deinit与premature free仍被
  普通输出/计数对照拒绝，不将Mac提升为完整sanitizer pass。完整ZIP摘要及只读核验
  收据留工作区`spec0331-mac-evidence/ci-c5a1-artifact-audit.json`，不上传大日志。


## 40. Resource 普通 continuation 的真实红绿与词法 owner 运输（2026-10-18）

- 原“额外普通println”拒绝属于未完成能力，不是Guide专项延期豁免。五份普通源覆盖原边界fixture、
  无后续语句、循环前缀与多调用、后续source Borrow，以及size分支合流后再Borrow；每份entry均调用
  非空与空Map。最终测试/查询在精确`4b4a74a5`两生产文件旧字节上重验，十观察均完整前端成功后
  在原For返回UnsupportedNode；Rust实际0通过/1失败，finally逐字恢复候选，非合成损坏负控。
- 两生产模块删除“恰For+单字面量调用”的AST准入。真实parameter origin仍锁唯一callable scope，
  只消费该scope的完整Drop/owner/phi/incoming事实；普通前后语句由现有lowering处理。
  single在词法Drop点读取当前binding，沿全部CFG incoming到原anchor，不要求仍在旧block或使用旧ValueId。
  Unit继续用其自身qualified事实和现有carrier，不合并lowering或开放新的Resource表示/来源。
- 同次FE原root/签名、Header/Exit完整incoming/Choice/Drop及caller Value交付复用独立查询；
  新SSA查询从原AST查真实callee，核provider每条边的owner/source/cursor、K/V真实read与逆序End、
  后续readMap的RootPlace/BorrowBegin/实际参数/End。每条正常Return须在EndSource后恰一次Map Drop。
  五源十pipeline和十个正常native产物严格exit0/golden/空stderr通过；stdout仅证明聚合析构marker，
  不外推逐对象内存计数、泄漏或sanitizer结果。
- 当前候选定向9项ordinary Rust测试全过（Resource组3、新native2、旧Resource native2、history1、
  sequential1），0失败/忽略；native合计14产物，新增10。fmt/workspace all-targets check/clippy
  (-D warnings)/build四项通过。独审发现的后续Borrow实参与全Return必达Drop查询缺口已补齐复核。
  测试查询曾把String临时Drop误算Map Drop、漏列真实MapSize用途，原失败日志保留，修正的是查询而非生产。
- 固定旧头`4b4a74a5`的run `39109969588`双host完整16jobs成功，三份新产物已独立核内容：Linux12组
  Resource的24个ASan/LSan结果、generated-owner16例与24运行记录通过；Mac generated-owner为partial，
  8个counter运行记录通过，requirements_met=false。旧CI不覆盖本节后继改动，当前CI执行另记。
  R1 Resource continue与R2 nullable component继续待处理，0331仍active，不归档、不合并、不启动D10。


## 41. Resource continue 与 Shared caller 权限恢复（2026-10-18）

- R1先用真实五源十观察取得lower Unsupported；补Shared两源后，在精确`5e9d8c9a`四生产文件旧
  字节上重验七源双入口14次Unsupported，Rust实际0通过/1失败，所有前置FE查询成立，finally逐字恢复。
  五源own Map含原continue fixture、最后使用/后续CFG使用及有/无本轮Guard；Borrow Map与MutableMap
  各含非空/空调用，caller随后size，Mutable再put并核size==3。引用红证的wrapper曾误把失败汇总的
  unit=false当Report false；原执行结果未改，修正仓外判据后七源重跑留最终收据。
- Continue只逆序结束组件并运输provider，不要求K/V为String；既有B/R类型边界保留。single Resource
  proof消费公开Continue incoming，Header/Exit仍回到原parameter；unit按自身ControlTransfer事实，允许
  已验证body-local Drop前缀，末尾仍严格V/K End且禁止Drop source。两入口复用原回边carrier/清理，
  不新增AST白名单或合并D10，不增加Resource来源/字段等表示资格。
- 独立同次FE桥核原root/caller交付与完整清理，SSA核Guard原AST构造→真实HeapAllocate/type/deinit/
  Drop、read→Guard Drop→V End→K End→Next；Next锚实际Continue span，false边cursor也入完整用途查询。
  每条normal Return对own source恰一次词法Map Drop。Shared路径另核本层reborrow与原caller loan分离，
  caller两次真实BorrowBegin/Call/End，post-put的size使用MapPut交付的新owner；初次query误要求旧owner
  ID导致两SharedMutable观察失败，按实际SSA操作合同修正查询，无生产变更。
- 新B/R合法邻控八观察暴露Unit继承分类缺陷：Resource return在既有String能力门前进入operand检查，
  错报MissingFact。首轮资源组实际4通过/1失败，后续native未跑；窄修将能力门移到该operand检查之前，
  与single同在原For报Unsupported。B/R仍未接通，不用拒绝邻控通过宣称实现完成。
- 最终定向14项ordinary Rust全过，0失败/忽略；新continue七源14pipeline及14正常native产物全部
  通过。所选native合计28产物（本片14、R3十、原Resource四），均严格exit0/golden/空stderr。
  原String三退出、history42路、sequential20路与旧Resource等回归通过；fmt/workspace all-targets
  check/clippy(-D warnings)/build四门通过。输入摘要前后一致，独审finding及最小Shared补证已闭合。
  普通marker仍不替代逐对象内存计数/泄漏证明；本片尚无新提交CI。R2继续待验，0331保持active。


## 42. Nullable Resource 分量复用既有表示（2026-10-18）

- R2先核既有Map storage的nullable value admission、NullableHandle<HeapOwner>及conditional drop，
  unit plan已递归调度inner deinit；不需要新Guide/ABI语义。两处Resource资格检查仅查看一层nullable
  的inner class，descriptor及组件Shared loan仍保留原nullable类型；String key、直接显式deinit class、
  无字段/泛型/接口及原source/owner/CFG门不变。通用nullable提取、non-null view/proof未开放。
- 七份真实源码保留原boundary-nullable，另含Some/None混合、全None、own continue和Borrow Map/
  MutableMap continue及后续size/put，均含非空/空调用。presence将原value==null比较结果先存Bool，
  只使用现有NullableLoanIsNull；查询禁止获取/Drop Resource owner，允许分支println的String临时Drop。
  初版查询误禁所有Drop的失败日志保留，修正仅针对实际nullable/Resource owner。最终查询在精确
  `c6d71677`两生产文件旧字节重验：十条owned在原For Unsupported，四条Shared原本完整SSA/LLVM
  成功，Rust实际0通过/1失败；finally逐字恢复候选。仓外收据判据曾误写Ok(())而输出实际为
  Ok(FunctionId)，修正判据后重跑，未更改源码或既有红结果。
- 最终七源双入口14条SSA/LLVM与14个正常native产物全部通过；同次FE完整owner/source事实与SSA
  全incoming/operand、原callee/Shared参数和nullable表示一并核验，并从原Resource class的真实Deinit
  span锁定module.deinits[inner]函数origin。混合与全None输出证明实际presence及资源析构聚合次数/顺序；
  CFG查询另证明每条正常Return恰一次Map owner Drop，不能仅据相同marker宣称逐对象free或无泄漏。
- 最终定向16项ordinary Rust通过，0失败/忽略；所选42个native产物（本片14、R1十四、R3十、原
  Resource四）均严格exit0/golden/空stderr。fmt/workspace all-targets check/clippy(-D warnings)/build
  四门通过，输入摘要前后一致；原String退出、history42路和sequential20路回归通过。独审提出的
  inner→原Resource.deinit身份补证已闭合。三份旧boundary拒绝oracle已迁为各自硬成功，原fixture保留。
  完整CI仍须绑定新提交，Mac sanitizer合同保持partial。Resource break/return仍未接通；0331继续
  active，不归档、不合并、不启动D10。


## 43. Resource Break 与词法 owner 合流（2026-10-18）

- 十三普通源×双入口：own Map、Borrow Map/MutableMap分别核Resource、混合Some/None、全None，
  本轮Guard及Break后size分支→readMap→多调用；另三own last-use和无Guard控制。每源entry均调用
  非空/空Map。首红26次完整FE后原For Unsupported；最终FE/SSA查询又在精确`43c54817`六生产
  文件旧字节上重验全部26拒绝，Rust实际0通过/1失败，finally逐字恢复，不使用合成事实损坏。
- Break复用既有逆序body cleanup/Finish/EndSource，无Next；single公开Break incoming携Header
  owner至Exit，unit消费自己的完整动作。single两处原terminal假设修正：按实际Drop fact要求当前
  owner为原当前值或不存在；Break/false合流后读取真实binding、逐全部incoming回溯seed，再保留
  原scope Drop。carrier无terminal Drop仍限已证明AfterStatement寿命的Break且body零source Drop。
- 同次原AST/root/类型/实际caller交付、single完整phi/Choice/incoming与两入口完整cleanup已核；
  独立SSA核Guard构造/type/deinit、read→Guard Drop→V/K End→source End、双边运输及每条正常
  Return恰一次own Map Drop。Borrow不Drop caller owner或结束原caller loan，Mutable post-put
  size沿MapPut交付的新owner。native只读首项但最终完整资源析构聚合数量吻合；不外推逐对象free。
- 查询曾错把合流Branch要求为Break span、unit BorrowEnd要求为Break span，以及Borrow readMap
  要求新loan birth。实际合同分别是For合流span、unit provider For span和原caller loan的合法转发；
  修正为各入口精确来源与全incoming identity，未用任意span/loan放宽。原失败日志保留。
- 最终定向19项ordinary Rust通过，0失败/忽略；68个普通native产物（新增26）严格exit0/golden/
  空stderr。Resource全组、原String退出/history/sequential回归与fmt/check/clippy all-targets
  (-D warnings)/build通过，输入摘要前后一致；独审提出的两处生产阻断均修复复核。四条Return
  合法邻控仍精确原For Unsupported，下一块继续处理。Source/payload/exit/continuation横向矩阵
  见Architecture，明确列own MutableMap参数、local/temporary payload与退出、一般return operand
  等合同内缺口，不能把它们包装成Guide专项延期或称F全部完成。


## 44. Resource Return 的实际交付值与清理边界（2026-10-18）

- 十五普通源×single/unit：三source模式各核Resource、混合Some/None、全None及Guard；另核own裸Unit、
  Unit前缀后return、Int字面量、checked `3+4`、owned String及无Guard。每份entry运行空/非空，
  caller核7/0或实际String/Unit输出。初构造中的println(Int)/return Unit表达式分别属L0084/L0087，
  修正后才计生产红：精确`51e6c78a`十四tracked生产文件旧字节下，最终同套FE查询30路均原For
  Unsupported，Rust实际0通过/1失败；finally逐字恢复，未合成损坏facts或执行本机故障对照。
- Resource直接Return复用普通合法表达式lowering，不继承非Resource的固定双Borrow调用/Int gate；
  原表示/source及条件/嵌套资格保持。own正常耗尽后的词法Drop与inner Return transferDrop分别严格
  取原事实；single公开phi/incoming/Choice、Unit完整cleanup动作均未放松或按body语句数猜资格。
- Unit普通Return在值交付与临时ABI loan结束后、emit_drops前记录实际LoweredValue及cleanup边界；
  最终当前carrier核准确Return值、全部Drop/End在边界之后。InstructionResult按真实index核多结果
  checked指令，不放松全局单结果查询。single/Unit checked成功边复用原block参数与全incoming
  transport，保留原caller/source、cursor、K/V及owner身份；Abort沿原checked溢出语义终止。
- 独立同次FE/SSA查询核原Return operand/callee/参数、实际值或String字节/checked arithmetic结果，
  Guard构造/deinit、逆序loan/provider清理、每条可达正常Return的own Map唯一Drop，以及Shared
  caller loan不在callee结束、Mutable post-put真实新owner。native marker仅证析构聚合数量/区间。
  独审指出的single外Return Drop、Unit实际返回身份和多结果producer三项阻断均修复并逐项复审。
  最终总回归另发现旧String conditional single MissingFact：将birth body owner误替换为最后arm当前owner；
  修正为仅显式checked-resumed取当前role，conditional/nested保留原birth并沿既有独立运输校验，复审通过。
- 最终定向34项ordinary Rust通过，0失败/忽略；20项native Rust测试共138个产物严格exit0/golden/
  空stderr，其中本片30、Resource精选共98。原String/temporary/nested/conditional退出、checked整数、
  history及sequential回归均通过；fmt/workspace all-targets check/clippy(-D warnings)/build四门
  通过，运行前后输入摘要一致。Architecture/Tour/本账本随后仅同步事实，docs704与diffcheck通过。
- `43c54817`完整CI run `39114665900` 16 jobs全绿，三项private Map exact在双host实际ok；独立只读
  产物审查核12 Resource程序的24个ASan/LSan结果、两host generated16例及各自运行合同。Mac
  sanitizer仍partial/requirements_met=false，UBSan未覆盖。这些旧头证据不覆盖本片Break/Return；
  新头完整CI及产物审查仍待。横向矩阵继续列具体合同内缺口，0331仍active，不归档/合并或进入D10。

## 45. F 单一缺口账本与 G4／Group 首片（2026-10-18）

`3d333853`完整远端 CI `39119068150/a1` 已全绿；该轮三份当前 artifact 已独立只读核验，
不再等待旧轮 CI。Linux Resource 12 plain／24 ASan/LSan 与 generated 两端各16病例均核对
输入、诊断、原日志及校准记录；Mac generated 仍为 partial/requirements_met=false，UBSan未覆盖。
这些证据只属于该固定头，不能覆盖后续提交。

同头横向诊断83份普通源、166双入口观察：44条LLVM成功、120条Unsupported、2条FE L0164；
Rust真实0通过/1失败/0忽略，未运行native。按最小程序归因：48源/96观察为已有表示的For实现缺口，
12源/24观察有nullable ABI前置，1源/2观察有Borrow-result continuation前置；22源/44观察成功，
其中8源为正控制。G6的非nullable Borrow Int/String、Int/Int、MoveOnly payload N/C/B实际成功，
撤回此前把它们全部列拒绝的判断；String?控制在无For时也受既有ABI前置阻塞。

以下是F剩余工作的唯一里程碑账本；各项归属SPEC-0331，不转交D10，不把阶段边界当最终豁免。
全部闭合及新头完整CI/独审之前，0331保持active、不归档或合并。

| 组 | 实际范围与依赖 | 归属／当前状态 |
|---|---|---|
| G2 | owned local N/C/B/R、Resource、后续Borrow、同根复用/重赋值；需真实形成/Move与未来使用运输 | 0331，§48–52有限factory local的Resource/后用/复用/重赋值轴已接通；其他birth/一般CFG及新头CI仍逐项闭合 |
| G3 | temporary Resource/Int key、实际factory实参及条件退出；需hidden owner与真实call delivery | 0331，未闭合；含current factory实参及Single成员/companion共享Call前置，条件依赖G7 |
| G4 | own MutableMap参数N/C/B/R，复用已验证Value/Owned参数及原清理 | 0331，本节首片；d85e完整CI及三artifact独审通过 |
| G5 | 有字段Resource、结构性Resource、非String key；复用既有storage/Drop表示 | 0331，§46有限表示片；d85e完整CI及三artifact独审通过，其他表示仍逐项验收 |
| G6 | 其他payload的Return及String? ABI前置；非nullable Borrow N/C/B已有正控 | 0331 Return缺口未闭合；ABI前置另行明确验收，不能静默省略 |
| G7 | 一般合法Return operand、单If/两live arm、条件/checked组合；需逐路径cleanup | 0331，未闭合；不把普通合法表达式改成白名单 |
| G8 | 不同root/own/temp/三层/条件嵌套；需每层provider与源owner完整运输 | 0331，未闭合；在单层source/CFG事实之后推进 |
| G9 | 非Resource own参数在N/C/B后继续使用；须真实last-use寿命，不能套Resource词法延寿 | 0331，§47有限未来使用片；consume/其他root及一般CFG仍逐项验收 |
| G10 | 透明Group、moved alias与Borrow-result；后两者分别依赖Move/continuation事实 | 0331，Group及§48 factory local alias首片；其他Move/birth与Borrow-result前置仍在 |

G4仅在原kind/intrinsic K/V与mode/binding/access完全匹配后接通Value MutableMap，继续排除
Inout/Field。Group共用纯透明映射：每层type/category相同，descriptor/loan.source及Entry point
保持outer，临时source_kind/loan.origin/Drop和实际Call保持canonical leaf。仅lower leaf一次，
不创建wrapper owner alias；原完整incoming、owner数量及清理证明不放松。local/nested也用该映射。

首片41源包括四个own MutableMap N/C/B/R及37个triple Group对照，源码名单/唯一标签/82观察均硬锁。
最终82条双入口完整FE/SSA/LLVM通过，37组各自与直接源的LLVM字节相等。独立查询分别核outer/leaf、
真实参数mode、temporary Drop及Entry点；G4另核实际owner RootPlace、每个Drop的全部incoming到原参数，
每条正常退出恰一次Map Drop。旧Group五路保留全部FE查询并要求真实entry/SSA/LLVM成功。
82个普通native产物均exit0/完整golden/空stderr，覆盖非空/空输入；不证明动态free/无泄漏。
最终16项ordinary Rust、fmt/workspace all-targets check/clippy(-D warnings)/build通过，输入摘要前后相同。
独审发现的名单可能静默缩减、Unit mode缺查询均已补齐复核；初轮clippy失败日志保留。
参考红用精确`3d333853`的10个tracked生产文件旧字节执行最终测试，82观察均停于原For Unsupported，
Rust真实0通过/1失败/0忽略，finally逐字恢复。未执行本机故障、校准、合成损坏或sanitizer。


## 46. G5 以完整源清理事实准入既有 Resource 表示（2026-10-18）

- 两入口Resource源owner资格去掉Map-only、String key、直接空字段显式deinit class的重复白名单。
  上层仍验证真实intrinsic kind/K/V、Value/Owned参数root；此处消费Resource分类及原完整清理事实。
  single Header/Exit phi、全部incoming/Choice、Drop来源和词法scope、Return transfer，以及Unit
  自身完整cleanup序列均不放松。具体表示及deinit可达性继续由既有mapper/planner准入，不新增ABI。
- 32份普通源：有owned String字段的Resource、结构性Payload(Resource)、Int key Resource、
  nullable有字段Resource；各自Map/own MutableMap×N/C/B/R。每源entry真实运行非空/空，含Guard，
  正常路径在For后再Borrow readMap；nullable源混合Some/None。源码名单32/64与唯一标签硬锁。
  精确`92396a32`两生产文件旧字节下最终64观察均完整FE/公开validator后原For Unsupported，
  Rust真实0通过/1失败/0忽略；finally逐字恢复。每份原源码/golden SHA保留仓外，不提交大日志。
- 独立查询锁实际storage key/value、nullable/结构性层、owned String字段与原Resource.deinit来源，
  实际read实参必须是当前K/V loan结果，保持精确Shared ABI，唯一End在read之后，不能取出组件owner。
  通用owner查询核First→BorrowBegin→原参数RootPlace、每个MapDrop全部incoming至原anchor；
  所有MapDrop均不在任何CFG循环，所有简单正常退出路径恰一次Drop，合起来覆盖有限迭代后退出。
  独审发现的循环路径证据缺口已补齐复核。native marker仅证明析构聚合数量/区间，不证明逐对象free。
- Field/Inout native边界、一般nullable结构/String? ABI及Borrow-result continuation前置保持。
  G2/G3/G6/G7/G8/G9及G10 alias等具体缺口继续由§45单一账本跟踪，不因本片支持提升为F完成。
- 最终19项ordinary Rust及2项native Rust均通过，0失败/忽略；64个native产物严格exit0/golden/
  空stderr。64条最终green的原源码/golden SHA与reference red逐项一致；工程fmt/workspace
  all-targets check/clippy(-D warnings)/build通过，运行前后源输入摘要相同。docs704与diffcheck通过。
  G4/Group首片另有82个已验native产物，本阶段新增合计146个，不重复计入此前Resource98个产物。
  新头完整远端CI和当前产物独审仍需单独绑定，Mac sanitizer仍partial；不把旧头成功继承到新片。


## 47. G9 Pure own 参数的当前 owner 与 ASAP 未来使用（2026-10-18）

- 24份普通源：Map/own MutableMap×N/C/B×最后size、多次Borrow调用、size分支后再Borrow、
  同根第二次For。每份entry均实际运行非空/空输入；48条原FE/深身份/SSA/LLVM观察及48个
  正常native产物通过。Pure源继续遵守真实last-use，不套Resource词法延寿或local size邻接白名单。
- single遍历公开owner图的全部Header/Exit incoming，Entry须连原Parameter或前一Exit，
  其余须连本层Header；availability精确相等，live证明拒绝空/Never边和未锚定循环。
  最终Drop.owner与当前/最终phi分别证明，重复For保留前一Exit真实availability。Unit消费
  自己的validated事实，以当前binding而非旧birth建立provider，全incoming回溯原参数anchor。
- 独立原FE查询锁实际scope/参数mode、源类型/root、完整phi/Entry/回边/Break/Exhaustion、
  Guard及最终Drop；SSA查询核First/source、所有incoming及受保护操作数、Guard实际deinit、
  逆序V/K End/Finish/EndSource与Next。size/调用在真正last-use与ABI loan End之后、
  后续marker之前Drop；重复For在第二LoopExit Drop。全部Map Drop在CFG循环之外，
  所有简单正常Return恰一次，合起来覆盖有限迭代后退出；stdout不外推动态free/无泄漏。
- 固定精确`d85e3437`十tracked生产文件旧字节执行最终同套测试，48观察均完整FE后原For
  Unsupported，FE查询false为0，Rust真实0通过/1失败/0忽略，finally逐字恢复候选。
  独审发现Unit只按同root准入计划认领Drop过宽，已改为source/late/transfer及已有conditional原路径fact精确匹配；
  相关参数存在却无计划认领时MissingFact，再逐全部incoming验证当前owner，复审通过。
- `d85e3437`完整PR CI `39123400972/a1`现为success，当前三artifact独立只读核验无不符：
  Linux十二组plain/ASan/LSan、两host generated合法/非法与校准证据吻合；未执行下载产物。
  Mac仍partial/requirements_met=false，UBSan未覆盖。本片新头CI需另行绑定，不能继承旧头绿。
- G9本片不关闭consume/重赋值、其他owner roots、一般CFG及其他payload；G2/local/alias
  是下一独立块。§45继续是唯一剩余账本，F仍active，不归档/合并或进入D10，不改Guide。

- 本地防丢失检查点：新增size/原If/双臂合流深查询48观察已通过且独审关闭finding；
  最终冻结源码后的完整定向回归、工程四门与新头CI仍待绑定。本检查点提交不表示F完成。

- 冻结检查点`31fa6a37`后的最终验收：22项ordinary Rust、4项native Rust共26项全部通过，
  0失败/忽略；所选112个native产物（本片48、既有Resource表示64）严格exit0/golden/空stderr。
  fmt/workspace all-targets check/clippy(-D warnings)/build四门通过，源输入摘要前后相同；
  docs704、尺寸护栏（38个历史超限，无新例外）及diffcheck通过。其后仅追加本文验收事实。
  测试独审指出的size条件链已补齐：原size/count→0常量→Equal/Boolean→实际If双臂及
  merge完整incoming，后续Borrow使用合流后当前owner；结果全部用途独立核验，48路重绿且复审。
  single caller只声称Value契约/原root引用/无loan，不虚构Unit才公开的Move交付表。


## 48. G2 local 形成与 Move 退出的真实首红（2026-10-18）

- 在固定`6b089590`生产源码上新增24份普通源：Map/MutableMap×val/var/moved alias×N/C/B/R。
  每份含实际非空/空零参factory及各自scanner，声明与For之间保留println，不以邻接形状准入。
  每轮有Guard；Return实际交付第二个Borrow分量调用的Int结果。48双入口观察均通过完整FE
  后在原For Unsupported，FE查询false为0，Rust实际0通过/1失败/0忽略；尚未运行native。
- 本地红测试检查点仅保存源码与输入身份；当前尚未修改G2生产逻辑，不表示支持已接通。
  下一步依据实际Expression形成owner、alias Move保留的原birth与Header/Exit完整incoming推进。

- G2候选检查点：固定24源的48双入口原FE查询/SSA/LLVM与48普通native产物已绿，
  native严格exit0/golden/空stderr；尚未完成SSA全身份深查询及最终回归/独审。设计独审
  指出合法重赋值/参数Move等其他形成历史可能被误判MissingFact，须修正分类后再验收；
  不能据本片称通用formed-local或G2完成。根据用户新要求，Mac后续只做正常小提交/push
  与必要验收收据，不再生成额外format-patch/压缩包/备份清单；已有文件保留。

- 后续G2修正检查点关闭上述诊断finding：五类真实合法历史×双入口的红对照实际8条Missing、
  2条原For U；修后硬锁重赋值/参数Move/旧alias重绑/put前缀8条原For U，prefix size-CFG
  2条完整SSA/LLVM成功。写入按真实assignment/mutation receiver及同callable、实际root引用
  失效声明历史；同一producer的ID/span/type/condition损坏仍Missing。形成后的当前value
  逐所有incoming回原Call，不再要求与birth同块或数值ID相等，旧local A/B专用合同保持。
  主24源48路强化FE/SSA/LLVM再次通过；Drop锁terminal当前owner列、N/C回边锁body当前列，
  独审无剩余阻断。prefix-CFG两路不是新增native证据；最终冻结回归与native重验待绑定。

- G9固定`6b089590`的完整PR CI `39126979463/a1`已终态success，16 jobs全部成功、0 skipped；
  当前三artifact已独立只读核ZIP/来源SHA与内容，无不符。Linux十二组plain/ASan/LSan、
  generated校准与两host原合同吻合；Mac仍partial/requirements_met=false，UBSan未覆盖。
  这份证据只属于G9固定头，不覆盖后续G2提交。
- G2最终同套主查询在精确`6b089590`十四tracked生产文件旧字节上重验：48路全部完整FE
  后原For Unsupported，FE query false为0，Rust真实0通过/1失败/0忽略；finally逐字恢复
  候选。红绿48组原源码/golden SHA逐项相同；这是真实版本对照，未做损坏facts/故障注入。

- 冻结生产检查点`d6d842ca`后的最终本机验收：24项ordinary Rust、6项native Rust共30项
  全部通过，0失败/忽略；所选160个普通native产物（本片48、G9未来使用48、既有Resource
  表示64）严格exit0/golden/空stderr。主24源48双入口观察全部完整FE/SSA/LLVM成功，
  五类邻控10路硬锁8条原For Unsupported与2条prefix-CFG SSA/LLVM成功。fmt/workspace
  all-targets check/clippy(-D warnings)/build四门通过，运行前后生产与测试源输入摘要相同；
  docs704、尺寸护栏（38个历史超限、无新增例外）和diffcheck通过。生产与测试深身份独审
  的finding已修复复核。其后仅追加验收文档；新头完整远端CI与当前产物独审须单独绑定。
  本片仅覆盖Pure String/String factory形成与Move后local的即时N/C/B/R；Resource local、
  local未来使用/重复遍历/重赋值及其他形成来源仍在§45，不提升为G2或F整体完成。


## 49. G2 Resource local 的形成与词法退出（2026-10-18）

- 固定`4bc23d38`生产源码的真实首红：48份普通源，Map/MutableMap×val/var/moved alias×
  N/C/B/R×有owned String字段Resource/混合Some/None的Resource?。每源entry运行非空/空
  factory，含Guard及Return真实Int交付。96双入口均完整FE后在原For Unsupported，
  FE query false为0；Rust实际0通过/1失败/0忽略。尚未运行native，深身份查询仍在补齐。
  这是新红测试检查点，不表示Resource local资格已接通；继续消费真实词法Drop，不改Guide。


## 50. G2 Pure local 的真实后用与同根复用（2026-10-18）

- 新54份普通源：Map/MutableMap×val/var/moved alias×N/C/B×多次Borrow后size、
  size条件两live臂合流后Borrow、同根第二For后Borrow。每源非空/空factory与scanner实际调用。
  当前Resource候选仍未修改Pure未来使用资格时，108双入口全部完整FE后原For Unsupported，
  Rust真实0通过/1失败/0忽略；尚无本片native验收。此红测试检查点不声明能力已接通。

- 生产候选检查点：Resource原48源96SSA/LLVM及96普通native产物首绿；新增合法同根
  两For邻控在single实际2条Missing、unit2条成功，修后四路均完整SSA/LLVM成功。
  formed live证明沿原Expression的ID/span经全部Header/Exit incoming至真实最后Drop，
  不再用本For Exit冒充最终owner；Parameter旧路径不变。Unit最终Formed Drop认领精确
  source/late/transfer fact，再核原Call与全部incoming。Pure local后用108条SSA/LLVM首绿，
  native及新增原FE深查询仍在验；Resource强图36条Unit清理origin查询与terminal当前owner
  列finding正在修正，故本检查点不声明最终验收或F完成。


## 51. G2 local 重新赋值的真实首红（2026-10-18）

- 18份普通源覆盖Map/MutableMap×Pure String/String、字段Resource、混合nullable Resource×
  首For前替换、首For后替换再For、替换后Move到alias再For。每源非空/空factory实际调用，
  make/other具有不同键值、start/ready与Resource marker以识别真实producer。36双入口
  全部完整FE后在原For Unsupported，Rust真实0通过/1失败/0忽略；尚未本片native验收。
  专属原FE/SSA深查询待补，不能把该红观察提升为重新赋值支持。

- 深查询检查点：Resource主96观察与四条合法repeat邻控完整通过；Pure local后用108观察
  已接独立原FE深查询并与强化当前owner SSA图一同通过，108普通native产物首绿。
  测试独审指出的terminal current列及首后用source链缺口已修复复核；最终冻结回归仍待。
  重新赋值专属查询另核真实old/new形成、全部incoming、Drop边界与Unit assignment-origin，
  当前仅完成接线，仍须对应生产支持与真实绿，不能据本检查点外推G2整体完成。

- 重新赋值生产候选检查点：完整首root BindingEpoch history与outer RHS边界已接入，
  每个same-root Drop先确定实际epoch再核birth/owner/condition；Unit另核实际assignment
  origin，未被For认领的旧Pure Drop也回溯其原Call。旧mutation不污染fresh epoch，
  当前mutation与未测factory实参保持原For Unsupported。首轮严格36结果为28成功、
  4条旧Pure initializer声明边界的lower Missing、4条Pure Exit Never的查询失败。
  后者已修正为Never/空incoming；前者定位到首initializer完成与declaration共享end，
  仅首initializer允许同end，replacement RHS仍按严格完成前清理旧值。修后串行验收仍在跑。
  三层Group RHS的12条及非当前epoch arity的4条邻控已接线，Group独立深身份/native仍待补；
  本检查点不作最终验收，不将旧头CI覆盖到当前未推送生产候选。

## 52. G2 有限合同逐项闭合与调用前置（2026-10-18）

| 合同 | 固定普通源 | 双入口SSA/LLVM / 普通native产物 | 实际独立证明 |
|---|---|---|---|
| Pure local即时N/C/B/R | 24 | 48 / 48 | §48已验；形成或Move后的当前owner与Return交付 |
| Resource local N/C/B/R | 48 | 96 / 96 | 原Call形成、完整live owner、词法Drop及Guard；另4条重复For邻控 |
| Pure local后用/同根复用 | 54 | 108 / 108 | 多次Borrow、size CFG两live臂、第二For及真正ASAP最后使用 |
| Resource后用/重复For | 24 | 48 / 48 | 两种Resource表示×N/C/B×两continuation；formation各分配8源，非全积 |
| local重新赋值 | 18 | 36 / 36 | Pure/Resource/nullable的三类历史，old/new Call与完整epoch边界 |
| Group RHS | 6 | 12 / 12 | outer RHS完整替换点、每层type/category、inner Call实际owner身份 |
| 非当前epoch普通实参 | 2 | 4 / 4 | 复用原普通Call delivery，不再对所有epoch套零参provider门 |
| Pure替换后无后用 | 4 | 8 / 8 | 实际assignment安装事件与RHS同end仍不同epoch；新值ASAP在tail前Drop |
| 非当前epoch既有Unit成员路线 | 6 For源＋3无For控制 | 9 Unit / 9 | 静态member/override/delegation，实际resolved callee/receiver与各epoch Drop |

- 上表为分项实际通过；最终同一冻结源码的回归、工程门禁与新头远端CI仍须下段收据闭合。
  `4dd2ec46`保存主修复与测试，不以先前`4bc23d38`已绿CI覆盖它。定时正常小提交，不新增备份。
- 固定`4bc23d38`十六个tracked生产文件旧字节，用当前真实源码重验：Resource96、Pure后用108、
  Resource后用48条均原For Unsupported；重新赋值36条为34 Unsupported、2 MissingFact，
  Group12与arity4亦失败。所有原FE query false为0，Rust实际0通过/7失败/0忽略；候选字节已恢复。
  这次原始结果与早期各首红分开保留，不能把Missing或收集器退出码改记全U/成功。
- 调用路线独审发现旧新增门只接受Declaration、误拒Symbol及已解析委托，现按同次source plan
  完整key对应的实际callee核出生；当前provider仍沿原零参资格。非当前专用Map/FunctionValue
  等未验出生明确Unsupported，仍是0331实现缺口，不能冒称Guide staging或新ABI已交付。
- member/companion/override/delegation的8份For源及4份无For控制首次双入口实际0通过/2失败/
  0忽略：Unit member/override/delegation的6条For与3条控制SSA/LLVM成功；Single这三路线无For
  已在Call Unsupported，companion无For两端分别complete L0165与lower MissingFact。完整24观察
  和原source/golden保留；不把失败控制变成预期拒绝来算支持，也不将通用Call前置伪装For资格。
  最终正验仅计已有Unit三路线，并补同次resolved route、实际SSA receiver/callee与owner Drop身份。
- 当前距离F完成仍至少包括§45 G3 temporary/实际current factory实参与调用前置、G6其他payload
  Return及String? ABI、G7一般Return operand/条件与checked组合、G8不同root/own/temp/三层嵌套；
  G9其他root/consume/一般CFG及G10其他Move/birth/Borrow-result也未全验。它们属于0331，不转交D10。
  Field/Inout保持Guide原延后边界，Mac sanitizer仍partial/requirements_met=false且UBSan未覆盖。

## 53. Pure replacement 的安装事件与 ASAP Drop（2026-10-18）

- 独立生产审计发现§51仍遗漏“已有For之后替换成不再读取的Pure owner”：旧RHS完成清理与
  新值AfterExpression(assignment) ASAP Drop具有相同end，但属于不同FE事件。原83源中
  `g2-local-reassign`的两入口也都在实际assignment MissingFact，不能据主矩阵36绿称完全闭合。
- 先以`3da61cb7`小提交保存4源：Map/MutableMap×plain/三层Group RHS；每源实际非空/空
  scanner。原源码与golden固定，8路全部完整FE后actual lower MissingFact，FE query false为0；
  Rust实际0通过/1失败/0忽略。初版Single query误把新origin写成旧声明，已按真实assignment
  origin校正，原首轮结果与校正后干净红测分别保留，不改source/golden消红。
- `606ba6ed`仅保存真实DropEvent：保留AfterExpression的原ExpressionId，只有实际Assignment、
  RHS id等于该epoch.rhs且完整span等于epoch.origin才表示安装；RHS完成仍清旧epoch。
  in_formation先选实际epoch再比outer RHS与binding，Group不丢失完整边界。Pure仍按前端原
  ASAP点清理，未转成Resource词法延寿。Single/Unit新Drop origin均为实际assignment。
- 同一四源8路原FE/SSA/LLVM深查询全绿，Rust实际1通过/0失败/0忽略；source/golden哈希与
  干净首红完全一致。SSA另核old owner完整incoming、new实际Call结果在赋值点唯一Drop，且
  Drop在tail之前。独立4生产文件复审关闭原P2，未发现新生产finding；同4源8个single/unit
  普通native产物亦严格通过exit0/完整golden/空stderr，最终冻结工程与总收据仍在验。
- PR119的`4dd2ec46` CI `39137673283`双host在Clippy失败：5处helper重复注册、1处等价布尔
  简化及5处8参查询函数；Targeted/Preview未运行。修正采用唯一共享模块和相关证据成对传递，
  未加allow或删除断言；本机候选Clippy已绿，最终新头需重新完成工程和远端门禁。

## 54. 旧 alias 原变量重绑的真实 Unit 邻控（2026-10-18）

- `065fa0ce`首轮冻结实际27个Rust通过/1失败/0忽略、321个严格native通过；失败来自旧五源
  邻控把`old-alias-rebound`的Unit真成功仍断言为Unsupported，非生产Missing或FE query失败。
  原失败收据单独保留，不以收集器exit0算全绿，也不修改生产来拒绝真实成功。
- 原源码保持`old=make();source=old;old=other();For(source);readMap(old)`，新增独立原FE/SSA
  查询：For的qualified source与完整incoming始终到make经Move形成的owner；后续readMap的
  RootPlace与Borrow到other的新owner。两owner family互斥，逐节点锚定实际factory，全部Uses
  与恰好两Map Drop、精确origin及LoopExit/CallReturn清理顺序均锁定。
- 定向实际Rust1通过/0失败/0忽略，十条观察为5 LLVM与5原For Unsupported；同原源码的
  Unit native另1通过，严格exit0/完整`gap\nzero\n`/空stderr。Single该路线仍原For Unsupported，
  是0331其他Move/形成历史缺口；该Unit邻控不扩充所有别名或一般CFG支持。
- `639032cf`已独立审查通过，无生产改动或阻断finding；原fixture的两个Map均为空，native
  不执行循环体，也不能单靠stdout区分owner，因此本片的身份与Drop次数由上述独立SSA查询补证。

## 55. G2 本机冻结收据与剩余 F 诊断（2026-10-18）

- 代码冻结`639032cf4d1e4623f4547ac5e9f6c9c171285c68`，32条串行命令全部通过：57个选定Rust
  测试，0失败/0忽略；482个选定普通native产物严格exit0/完整golden/空stderr。482包含既有
  G5/G9回归及3个无For的Unit调用控制，不是482种新增For组合。
- 源码指纹覆盖crates、Cargo与rust-toolchain，前后均为
  `382339ea4fa08312f6bf80f0f373d390f256c35e32094e088c9d0d04a738955d`；不包含本轮dirty文档。
  与固定旧实现的308条source/golden观察完全相同；Pure安装事件另四源八路也与干净红测一致。
- fmt、workspace all-targets check、Clippy -D warnings、workspace build均通过；文档结构检查
  704 Markdown通过，尺寸护栏通过（38个既有超限，无新例外），whitespace通过。Rust/Cargo
  1.96.0、LLVM/Clang21.1.8；普通native按合同由系统Apple clang21.0.0链接。
- 同一冻结源码另跑原83源166观察：92 SSA/LLVM成功、72 lower Unsupported、2 ownership
  诊断，实际Rust0通过/1失败/0忽略、exit101、无native。与修复前相比恰好原
  `g2-local-reassign`两入口由MissingFact转为LLVM，其余阶段不变；原G2子集22/22 LLVM。
  fixture哈希全符、临时注册finally恢复、源码指纹与正验相同。这是失败缺口诊断，不能说F全绿。
- 本轮未运行本机故障、校准、合成事实或sanitizer executable；Mac保持partial/requirements_met=false，
  UBSan未覆盖。新提交的双host完整CI与当前artifact独立验收仍须重新运行，不继承旧头结论。

## 56. G3 既有表示 temporary normal 的首红与窄修（2026-10-18）

- `783f5c03`先保存10源：Map/MutableMap×owned String字段Resource、structural Resource、
  Int-key Resource、混合Some/None的Resource?、Int-key String；每源两个不同factory的
  temporary For，真实非空和空输入。20条双入口均完整FE后原For Unsupported，FE query false为0，
  Rust实际0通过/1失败/0忽略，无native；原source/golden哈希保留。
- `07a32b36`仅去除两入口temporary qualifier的String/String硬门，既有type/storage mapper
  继续决定实际表示资格；actual selected callable、Owned result、零参、source/Drop身份、完整
  cleanup与所有控制门保持。Resource temporary在原LoopExit清理，不沿named Resource的词法
  carrier延寿；Pure亦未新增延寿。独立生产审查未发现阻断。
- 同原10源20条初步SSA/LLVM通过，source/golden完全一致；20个普通native严格exit0/完整golden/
  空stderr，两个Rust测试均通过。完整FE/SSA独立身份查询和最终冻结工程回归仍在补证，不能
  据首绿关闭G3。factory实际参数、条件C/B/R及其他callable/CFG合同仍待逐项闭合。
- 用户指定后续顺序：本机完成F、完整CI与审计后再最终合并；D10交云端从准确merged main
  SHA启动。本机不提前开始D10，F余项不转交D10。
- 独立fixture审查发现Int key原来只打印相同marker，不能观测键值/顺序；v2对真实1/2分别
  输出int-one/int-two，其他值error。4份Int源与golden明确强化，v1红/绿和限制保留；v2在
  `783f5c03`原两生产文件参考上重跑20个首For Unsupported、FE query false为0，候选字节随后恢复。
  这不是原输入不变的消红：最终v2只与自身干净红测逐条核source/golden。
- `78176a3e`补独立原FE/SSA与actualstorage查询：两site的canonical Call/selectedcallee、
  Temporary Drop、single三条完整incoming及Unit自身cleanup，SSA逐当前owner/source/cursor列
  锚定真实factory结果、全部Uses/KV Shared ABI和精确两次Map Drop。另补实际factory内部
  MapConstruct→每次MapPut owner/result→可选MapConsume→原Return完整链；empty必须返回
  自身唯一空构造。不能只计操作数量，两个owner族互斥，Drop在aftermarker及下一个factory前。
  原factory链finding已独立复审关闭，无新阻断；未修改输入或生产来修query。
- 同一Rust源码指纹`3a69a597e77fd5ba5584c545c75692dc39b884ed6a36e3cb2cbc8a93e7eb7529`
  前后相同；17条命令全部通过，23个唯一Rust测试0失败/0忽略、36个严格native（20新增+16既有
  temporary回归）。v2原10源20双入口/40实际site身份与20native通过，Int值及插入顺序可观察，
  Resource析构只核数量/时点、不假定条目间析构次序。fmt/check/clippy/build、704文档、尺寸
  护栏38既有超限和whitespace通过。这份有限normal验收仍不关闭G3或F。

- 同源码另跑原83源166观察：98 LLVM、66 lower Unsupported、2 ownership，实际Rust
  0通过/1失败/0忽略，exit101、无native。仅原temporary Resource、Resource?、Int-key的
  6入口从Unsupported转LLVM，其余阶段不变；原G3子集现6 LLVM/4 Unsupported，后者是
  factory实参与条件源。原83源摘要全符、临时注册finally恢复；失败诊断不能算F全绿。

## 57. cb647 Mac generated 合同失败与阶段预算（2026-10-18）

- PR119 CI `39141747243`的Mac Targeted在generated reduction失败，Preview跳过；Linux Targeted
  及其当前产物内容独审通过，不能据此说双host全绿或Mac partial-pass。Mac artifact
  `12142466462`（329016字节，SHA256 `92e3c5085097ebb5f88135ed56576d4d8aa90181d96a97a53c240f84c04ac852`）
  对应fixed head `cb647242`、synthetic `fd59154e666a45278929ba62a1e0eebb1022b871`，只读审计核实
  16个case已通过；checker阶段301.269秒，继承旧300秒deadline，原clean目录只有7输入文件、
  没有export.command/result。故首次export前被预算守卫拒绝，非已证实的编译器语义反例。
- `ab8f7aaa`把既有reduction 120秒deadline移到阶段入口且只设一次，涵盖clean/fault baseline、
  minimize、三次确认及1-minimal；保留32候选、生成执行300秒、checker各构建900秒及全部
  故障对照。预算provenance和整阶段elapsed在失败也保存，最终同deadline到期不能标
  is_1_minimal=true。附加budget写入失败不覆盖原异常，成功路径留证失败仍失败。独立复审
  关闭该首错保留finding，无新阻断。不是增加预算或跳过校准来消红。
- 四项新CI回归覆盖继承过期deadline、单一窗口不刷新、首错保留/成功路径写失败及最终
  空候选越时。按用户限制仅待GitHub Actions执行，本机只做Python语法解析和只读审查，
  不把上述Rust/native绿算作这些Python行为已通过。新头完整CI和当前产物仍须重新验收。

## 58. 阶段预算修复的新头 CI 与当前产物（2026-10-18）

- `b95919dc93a4c5aeb98bd1bd800633dca3095421` 的 PR run `39145122284` 完整终态为 success，
  双host Targeted、Preview producer/independent consumer及最终CI门均成功；不改写§57失败记录。
- 三份当前artifact只读独审核对run、完整head、synthetic
  `fb7e2c66fc23e5a5d12c658d6b74d0059a2a2855`、下载SHA256与输入身份：Linux sanitizer
  `12142414961`、Linux generated `12143557148`、Mac generated `12143830755`。
  Linux 12组provider×plain/ASan/LSan的36次clean、generated的24 native及校准、checker、
  13对reduction因果对照通过；未发现非预期失败。
- Mac original-clean本次实际export/native exit0、精确golden；reduction在独立120秒窗口内
  8.499秒完成，9→5 operations、717→551 bytes、三次确认。合同仍为partial、
  requirements_met=false，ASan/LSan未验收，UBSan仍未覆盖。
- Docs job `117727348468` 日志逐名确认§57四项新增Python回归均为ok，整套395 tests为OK。
  本机只读审查这些CI产物与日志，未执行产物、故障、校准、reduction或mock测试。
  此CI结论仅属于b95919dc；后续实参代码必须在其新头重新运行完整CI，不继承本轮全绿。
