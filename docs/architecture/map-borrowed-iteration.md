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
| 零参 Owned factory temporary Map / MutableMap；String/String | 正常耗尽、固定尾 continue/break/return，hidden owner 保活至 provider 结束 | `map-provider-temporary-0331`、`map-provider-temporary-exits-0331` |
| factory 初始化的 owned local Map / MutableMap，val/var；String/String | 正常耗尽；固定 A/B 形状中退出即清理，或保活至后续 size 查询 | `map-provider-local-sources-0331` |
| Borrow Map / MutableMap 参数；Int/String | single/basic/const 正常耗尽及 whole-set 资格 | `map_iteration_stage_gate_tests`，只有 SSA/LLVM，不计新增 native |

上述 fixture 目录均位于 `crates/lang-codegen/tests/fixtures/`；普通 single/basic native
在该固定快照的本机复验为16项测试、56个 case×entry 产物，每个硬核 exit0、完整 golden
及空 stderr。另15项普通 pipeline/身份测试保留独立 FE/SSA 查询并通过 LLVM verifier。
具体命令及输入身份见0331本机账本；stdout 不证明精确动态分配/释放次数或无泄漏。
local A/B 的独立同次 FE owner、全 incoming 与受保护 operand 深查询已补齐，见下节。

Group 仍由真实源码阶段门精确拒绝；Field/Inout、direct Borrow-return continuation、
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
恢复、任意CFG或动态释放计数；原Field/Inout/Group及Borrow-return来源阶段门保持。


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

两份源×双入口四路经独立原 AST/name/参数/caller、完整 SSA incoming/operand 使用及
词法 Drop 查询后通过 verifier/LLVM；四个普通 native 产物 exit0/完整 golden/空 stderr。
测试明确区分 single Borrow 的支配入口 loan 与需要运输的列，不虚构额外 seed 列；
该片不是云端 Resource 提交的字节恢复，不扩称一般 Resource CFG 或无泄漏。


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
| own Map参数 | String/String | N/C/B/R、固定三段条件 | Drop限LoopExit/对应transfer；后续source使用尚缺 |
| Borrow Map/MutableMap参数 | String/String | N/C/B/R、固定条件、同根双层 | caller恢复及有限sequential已验；更一般条件/层数未验 |
| 零参直接factory temporary | String/String | N/C/B/R固定尾 | hidden owner已验；其他payload/条件/一般factory未接通 |
| factory-formed owned local val/var | String/String | N | 固定立即Drop/size guard；C/B/R、资源payload、一般后用尚缺 |
| own Map参数 | String/直接空字段Resource或Resource? | N/C/B/R | 普通多调用/后续Borrow/size CFG；Resource仍限显式deinit且无泛型/接口 |
| Borrow Map/MutableMap参数 | String/Resource或Resource? | C/B/R；Resource的Mutable另验N | caller size/Mutable put已验；nullable Shared N尚无专门锁定 |
| Borrow参数等既有admission | Int/String等 | 部分N及有限sequential | 不把静态通用component cleanup接纳当全payload native实证 |
| own MutableMap参数 | 各payload | 未接通 | 不能由owned local MutableMap或Borrow MutableMap替代 |
| Field/Inout来源 | 现有类型 | 前端合同 | Guide37.4明确native延后；Group阶段门保持 |

非Resource Return仍限制直接双Borrow分量调用且Int结果。Resource本片另验Unit/Int/String及直接checked加法；
其他合法operand、ABI及更一般CFG尚未系统验收，不能把有限ordinary lowering接纳外推全部支持。
以上非Field/Inout的具体能力缺口不能统称Guide staging；普通Resource类的字段/泛型/接口、
其他key/value与更一般source/CFG需分别验收，不能在本轮Break/Return通过后宣称F全矩阵闭合。
