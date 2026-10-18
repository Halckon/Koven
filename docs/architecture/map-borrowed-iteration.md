# Map 双分量借用遍历当前边界

> **性质**：当前实现事实 · **状态**：current · **读取时机**：选择 Map 借用迭代来源和退出组合时 · **唯一真源**：代码与测试；验收见 [SPEC-0331](../specs/active/0331-map-borrowed-iteration.md)

[SPEC-0331](../specs/active/0331-map-borrowed-iteration.md)仍在实施；首表保留固定快照
`60512fda0677b07abb77f10d88242f7a16edd1fb` 的有限支持，后续补片分节列出；不改写 Guide 的来源资格。
`for ((key, value) in source)`沿插入链取得两个独立 Shared loan，不构造 Pair 或 entry owner。
source 求值、分量与 source 清理由已验证事实交付，SSA verifier 后再生成 LLVM。

| 已验来源与类型 | 控制流范围 | 普通证据 |
|---|---|---|
| own Map / Borrow MutableMap 参数；String/String | 正常耗尽、无条件 continue/break/return、有界条件退出 | `map-provider-0331`、`map-provider-exits-0331`、`map-provider-conditional-0331` |
| Borrow Map / MutableMap；String/String | 同根两层共享遍历，以及固定内层无条件 continue/break/return | `map-provider-nested-0331`、`map-provider-nested-exits-0331` |
| Owned factory temporary Map / MutableMap；String/String及已验表示 | 零参固定尾C/B/R与五表示normal；普通实参交付normal、空Map实参if | `temporary_forms`、`temporary_arguments`、`temporary_delivery`、`temporary_argument_cfg_runtime`及原fixture |
| factory 初始化的 owned local Map / MutableMap，val/var；String/String | 正常耗尽；固定 A/B 形状中退出即清理，或保活至后续 size 查询 | `map-provider-local-sources-0331` |
| Borrow Map / MutableMap 参数；Int/String | single/basic/const 正常耗尽及 whole-set 资格 | `map_iteration_stage_gate_tests`，只有 SSA/LLVM，不计新增 native |

上述fixture位于`crates/lang-codegen/tests/fixtures/`；固定60512的16项普通native测试/56个产物及
15项pipeline历史复验见0331本机账本。普通stdout不证明动态释放次数或无泄漏，独立FE/SSA查询见下节。

Group 的后续准入见下节；Field/Inout、direct Borrow-return continuation、
更一般 source/CFG、nullable K/V 组合不由本表开放。body-local Guard 的析构
正例不等于 Map 的 Resource 分量支持。Map Borrow 的直接退出
补片和Unit闭包前端修复另见下节，本表不据云端摘要扩张。私有损坏事实与故障对照仅在
GitHub Actions 执行，本次本机未运行；macOS sanitizer 合同仍为 partial。

## Unit 闭包赋值邻控

Phase 3 的普通 root 赋值保持旧闭包环境至完整 RHS 完成，随后交接 replacement 的真实
lambda 身份；alias/self assignment 保留对应捕获来源。Drop planner 在同一 RHS 点处理
旧环境的 owned capture，且新环境先登记再决定共享来源的最后使用；Map return 先结束
当前闭包 capture，再结束对应分量 loan。已知具体环境的资源分类先查询实际 owned
captures，不能因普通函数类型 `() -> Unit` 没有记录环境资源信息而提前释放 Resource。
四个恢复源码的新普通 harness 已核这些 Phase 3 事实。已知嵌套环境的资源证明在消费
捕获前计算，随当前 binding 运输；替换、移动及合流同时维护来源和证明，不把 lambda
历史记录当作当前可用性。两份 Resource 嵌套源分别核 RHS 替换点和作用域清理，另有
Pure 邻控。新重建的十份拒绝与五份合法源码均运行 single/unit 两入口，核原诊断 Span、
执行事实撤销及同次身份；它们不是原 active20/10 或完整四源 collector 的字节恢复。
本片不增加 closure native 支持，也不证明 opaque nullable/容器包装函数环境的一般资源
分类；其他未验递归形状仍保留继承边界。

## Borrow Map 直接退出补片

四份重建String/String fixture保留既有C/B/R固定尾及三分支conditional形状，来源改为
直接Borrow Map参数；single/unit各消费自身完整validated事实，复用既有Shared provider
清理路径，不取得caller owner。普通源码reference→scan参数root与caller Shared交接
已有同次独立只读查询；八路SSA/LLVM及八个native产物正常exit/golden/空stderr通过。
fixture目录为`map-provider-borrow-exits-0331`。这是有界重建，不宣称原云提交/collector
恢复、任意CFG或动态释放计数；原Field/Inout及Borrow-return来源阶段门保持，Group后续准入见下节。

## Resource 分量的有限遍历与退出

`map-provider-resource-0331` 的两份普通源码支持 own Map / Borrow MutableMap 参数、
String key 与无字段/泛型/接口且有显式 deinit 的直接 class value，正常回边、continue和耗尽。owned Map
以真实callable scope的Drop为依据，不限制循环前后普通语句的数量或调用形状：single 消费公开 Header/Exit phi、
完整 incoming/Choice 与唯一 AfterStatement(scope) Drop，在 source loan 结束后保留实际
当前 owner 至该词法点；unit 独立消费 qualified scope Drop 并运输自身 carrier。
Borrow MutableMap 不取得或清理 caller owner。相同直接 class 的 nullable value 复用既有
NullableHandle/条件析构 ABI；provider 保留原 nullable Shared loan，不提取、复制或消费 inner owner。
七源双入口14条 pipeline 与14个普通 native 产物覆盖原 nullable fixture、Some/None 混合、全None、
own/shared continue及后续CFG/source使用；presence只由原借用参数的 NullableLoanIsNull 读取。
同次原AST查询还锁定 inner type 到实际 Resource.deinit 的函数 origin；不开放一般 nullable-place proof。
Resource continue七源覆盖own Map及Borrow Map/MutableMap，单轮Guard先Drop，再逆序结束V/K并Next；
owner/source在回边保留，耗尽结束本层provider lease，own源按原词法scope唯一Drop。Shared caller的
原loan不在callee结束，caller返回后put/size恢复权限。双入口14条pipeline及14个正常native产物通过。
Resource break十三源覆盖own Map、Borrow Map/MutableMap及Resource/混合nullable/全None，
另核own最后使用与无Guard控制。Break结束本层lease后，own owner沿Break/false完整合流保留至
原词法scope Drop；后续size/readMap在End之后、Drop之前。双入口26条pipeline/native严格通过。
Resource return十五源覆盖上述三source模式及三种payload；另核own的裸Unit、Unit前缀、Int字面量、
checked加法、owned String及无Guard控制。双入口30条pipeline/native严格通过，每份entry均运行空/非空。
普通lowering先交付实际返回结果，再清理Guard、V/K、provider及本层source；own源在各正常退出恰一次Drop。
checked成功边沿完整incoming运输当前source/cursor/K/V/owner，overflow Abort不新增unwind语义。
Unit记录实际交付值和cleanup起点，精确匹配多结果指令的result槽；不借旧birth carrier或固定callee猜结果。
无条件Resource Return仅复用既有合法表达式lowering；本片没有开放一般条件/嵌套checked CFG。
普通continuation的五源双入口已核无后续语句、多调用/前缀、后续Borrow使用及CFG合流后再借用；
single在真实清理点按当前binding逐全部incoming追溯原owner，不冻结旧block/value。十个正常native
产物核严格输出；独立CFG查询要求每条正常Return恰一次Map Drop，并核后续Borrow的callee/参数/End。
多重early-return等尚未验的组合不由五源外推。

## local A/B 的独立深查询

两份固定源码、single/unit 共四次 fresh 分析，各含四个 scanner、每个两个独立 local
与 For。原 AST/name 的声明和 source reference、selected factory 与形成表达式绑定
实际 owner；single 完整查询各 Header/Exit phi、Entry/Fallthrough/Exhaustion 写集、
public Choice 与原 If 成功路径。A 在 EndSource 后按 LoopExit 清理，B 保留至真实 size
的 CallReturn；第二循环前序 root 的 Never Entry、回边 presence 与条件 Drop 单独核对。
unit 使用自身 qualified source、唯一 Drop 与完整 exhaustion 动作，不借 single 条件表。

SSA 查询全部可达 block/incoming 的 owner/source/cursor 列、两分量/read/逆序 loan 结束、
当前 false owner Drop 和 size 的 i32 结果/比较/原 If。两个 Map Drop 及受保护 entity 的
完整操作数使用单独核验；已失效的前序 root 不新增 SSA 列或重复 Drop。后用源的失败
分支是真实 Abort，只有此测试显式允许它；其他图查询保留原严格终止合同。
四次完整 SSA/LLVM 通过；普通 local native 的四个产物也在同版固定头复验。该查询
没有合成事实、调用 lower 私有资格证明或据 arena 数字认身份，不扩大 local/CFG 支持。

## provider 的普通顺序历史

`map_provider_exits_pipeline_tests::history` 共21份正常源码，在单项、覆盖保位、删除
重插、增长、碰撞墓碑之后调用真实provider；String/String覆盖own Map、Borrow Map与
Borrow MutableMap，Resource覆盖不涉及remove的单项/覆盖/增长两种既有owner模式。
双入口42条pipeline及42个普通native产物通过。它们补充0291链探针，而非用探针代替
For执行；Resource marker只证明聚合数量和清理区间，该历史片不证明一般nullable或跨entry析构序。

Resource 的空字段 payload 仍按 ADR-0008 获得独立 `malloc(1)` handle；heap runtime 保存真实
target-derived payload size，仅在该大小非零时发出初始化 store。零字节 store 没有业务数据
可写，LLVM 21 ASan 却会检查 `ptr + size - 1`，因此不能把它留在合法空对象构造中。
OOM、provider、deinit 与 free 路径保持；该修复不增加字段或改变源/golden 来规避检测。
普通 native 验收不替代 Linux ASan/LSan；动态通过须绑定新提交的完整 CI 产物。

## Borrow 参数的顺序复用

`sequential` 普通矩阵覆盖 Map/MutableMap String/String 参数的独立、同参重复和反序
两个 For；Map 另验首轮空、break/continue及第二参数Int/String。两入口20条完整
pipeline与20个普通native产物通过，Mutable caller随后put及size检查也通过。
Unit用当前CFG loan建立provider，再以既有transport核所有入边到原参数root；entry
anchor仅证明身份，不替代当前view。Borrow root不在callee结束，组件与provider按实际
退出清理。owned参数、Field/Inout及一般CFG来源边界不由本片扩大。

## 横向覆盖矩阵（有限实证）

N/C/B/R分别指正常耗尽、continue、break、return；下表没有声明完整笛卡尔积已验。

| source | payload | 已验exit | continuation与当前缺口 |
|---|---|---|---|
| own Map/MutableMap参数 | String/String | N/C/B/R、普通单If C/B/R及两live arm | N/C/B后size、多次Borrow调用、CFG后再Borrow及同根重复For已验；其他CFG组合不外推 |
| Borrow Map/MutableMap参数 | String/String | N/C/B/R、普通单If C/B/R、两live arm、同根双层 | caller恢复及有限sequential已验；其他条件/层数组合未验 |
| 直接factory temporary | String/String；字段/结构Resource、Resource?、Int键Resource/String | 零参固定尾N/C/B/R与五表示N；普通实参N及空Map实参if；String/String单If C/B/R与两live arm | 实参复用原求值/形参ABI；Resource另验Return内if/checked；其他payload/CFG组合不外推 |
| factory-formed owned local val/var及moved alias | String/String、owned String字段Resource及混合Resource? | N/C/B/R；后用/复用/替换按末节实测组合 | 按真实Call、Move与epoch运输当前owner；其他birth/一般CFG仍未闭合 |
| own Map参数 | String/直接空字段Resource或Resource? | N/C/B/R | 普通多调用/后续Borrow/size CFG；更多既有Resource表示见后续G5片 |
| Borrow Map/MutableMap参数 | String/Resource或Resource? | C/B/R；Resource的Mutable另验N | caller size/Mutable put已验；nullable Shared N尚无专门锁定 |
| Borrow参数等既有admission | Int/Int、Int/String、String/pure MoveOnly inline payload等 | 部分N、有限sequential及非nullable Return | Return的具体operand见§61测试；不把通用component cleanup接纳当全payload实证 |
| own MutableMap参数 | String/String | N/C/B/R | G4首片；已验Resource表示另见下行 |
| own Map/MutableMap参数 | String/有String字段Resource、结构性Payload(Resource)、Int/Resource、String/nullable字段Resource | N/C/B/R | G5复用既有表示；每源含非空/空、Guard与后续Borrow readMap |
| Field/Inout来源 | 现有类型 | 前端合同 | Guide37.4明确native延后；不由Group准入扩大 |

Return现经普通operand lowering；有限Int/Unit/owned String、非nullable Borrow分量及checked邻控，
合计52源104条双入口SSA/LLVM与104个native已验，见[§61](../specs/active/0331-map-borrowed-iteration.md#61-g3g7-共享普通控制流与非-nullable-return-有限验收2026-10-18)。String? ABI、未来own转移无Drop及其他CFG不外推。
以上非Field/Inout的具体能力缺口不能统称Guide staging；普通Resource类的字段/泛型/接口、
其他key/value与更一般source/CFG需分别验收，不能在本轮Break/Return通过后宣称F全矩阵闭合。

## own MutableMap 与透明 Group 来源

`source_forms` 的四份own MutableMap String/String源覆盖N/C/B/R，另有37份triple Group源，
分别复用已有参数、temporary、local和同根nested的源码与golden。每层Group只传递相同typed
类型/category；descriptor和loan.source保留outer，temporary真实call/owner/Drop保留leaf，
Entry点仍对应outer。只求值leaf一次，不放松owner alias或完整incoming校验。原五条fresh
single/basic/const whole-set Group测试改为SSA/LLVM硬成功，保留全部原FE身份查询。

41源共82条双入口SSA/LLVM通过，37组对照各自LLVM字节相等；82个普通native产物硬核
exit0/golden/空stderr。四个own MutableMap及其Group变体另核实际RootPlace、Drop的全部
incoming回到原参数，以及每条正常退出恰一次Map Drop。不是所有payload/CFG组合的验收完成。

## 既有 Resource 表示的源资格

`resource_forms` 的32源覆盖上表四类表示、Map/own MutableMap及N/C/B/R；源owner资格依赖
已验证Resource分类和完整cleanup事实，具体key/value表示继续由既有storage mapper/deinit
planner准入。Field/Inout来源资格和nullable结构/String?表示边界未改，不增加任意source/CFG支持。
独立SSA查询核实际storage/原Resource.deinit、当前K/V reader实参与Shared ABI/End，核实际First
来源及每个Drop的全部incoming。MapDrop不得处于任何CFG循环，所有简单正常退出路径均一次Drop。
32源64条双入口FE/SSA/LLVM及64个普通native产物均通过，包含非空/空与混合nullable输入；
完整golden只证明析构聚合数量/区间，不外推动态free或无泄漏。其他key/value/泛型/接口组合仍须
按实际既有表示与独立证据验收；F整体缺口以0331单一账本为准。

## Pure own 参数的真实未来使用

`future_use` 的24份普通源覆盖Map/MutableMap、N/C/B，以及最后size、多次Borrow调用、
size分支后再Borrow、同根第二次For。每源实际调用非空/空输入；双入口48条原FE/SSA/LLVM
及48个普通native产物通过。Pure source不进入Resource词法carrier，也不依赖邻接语句形状。

single逐完整Header/Exit incoming证明当前owner仍连到真实own Parameter，live链拒绝空/Never
边及无原参数锚点的循环；重复For的Entry消费前一Exit的实际owner与availability。Unit使用
自己的validated source/cleanup与当前SSA binding，全incoming回溯原入口anchor。最终Drop必须
精确等于准入计划的source/late/transfer fact，再核实际当前owner；根名或旧birth身份不能代替。

独立查询核provider与K/V的全部受保护使用、Guard/deinit、N/C/B清理与Next、每条正常退出
唯一Map Drop。size/调用的Drop在真实最后使用及ABI loan结束之后、后续marker之前；重复For
在第二LoopExit清理。所有Map Drop均在CFG循环之外。该有限片不宣称consume/重赋值/alias、
一般条件/嵌套CFG或其他payload的完整覆盖；具体余项继续由SPEC0331 §45单一账本跟踪。

## factory local 的形成、后用与 epoch

`local_exits` 的24源覆盖Map/MutableMap、val/var/moved alias及N/C/B/R，48双入口与48普通native
通过。`local_resources` 另以48源96双入口/native覆盖owned String字段Resource与混合Some/None；
Resource owner在实际词法退出清理，Return先交付结果；所有源均实际运行非空/空scanner。
`local_future` 的54源108双入口/native覆盖Pure local N/C/B后多次Borrow、size两臂合流后Borrow、
同根第二For后Borrow；按真实最后使用清理，不套Resource词法寿命。`local_resource_future` 的24源
48双入口/native覆盖Resource N/C/B后多次Borrow/size及第二For后Borrow；val/var/alias各分配8源，
不是formation全积。size CFG两臂与第二For携带当前owner，最终Drop在tail及返回值求值之后。

`local_reassignment` 的18源36双入口/native核首For前替换、两For间替换、替换后Move，包含Pure、
Resource及nullable；另6源12双入口/native核Group，4源8双入口/native核新Pure值赋值完成后立即Drop。
完整history区分实际事件：尚存旧值在RHS完成后清理，已死Pure不复活；outer Group与inner Call分别验证。
当前local provider仍用零参factory合同；temporary普通实参交付及空Map实参if见上表。其他epoch普通实参Call的2源4双入口/native已验；Unit既有
member/override/delegation路线另核真实resolved callee/receiver，不新增ABI。Single相同路线及
companion在无For控制已有失败，属于共享调用前置，不能计为For支持或Guide staging。

每个已验源独立查询原FE形成/Move、全部Header/Exit incoming与实际最后Drop，SSA另锁当前
owner、Guard、K/V loan及所有受保护使用；普通native只证明exit0/完整golden/空stderr。
原五源邻控为5条原For Unsupported与5条SSA/LLVM成功；Unit重绑另验两owner身份及1个native。其他实参组合、
其他birth/非当前epoch专用Map或函数值Call、一般CFG等仍归0331缺口账本，不据本片称G2或F全闭合。
