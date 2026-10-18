# Map 双分量借用遍历当前边界

> **性质**：当前实现事实 · **状态**：current · **读取时机**：选择 Map 借用迭代来源和退出组合时 · **唯一真源**：代码与测试；验收见 [SPEC-0331](../specs/active/0331-map-borrowed-iteration.md)

[SPEC-0331](../specs/active/0331-map-borrowed-iteration.md)仍在实施；以下为固定快照
`60512fda0677b07abb77f10d88242f7a16edd1fb` 的有限支持，不改写 Guide 的来源资格。
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
local A/B 的独立全实体/全 incoming 深查询仍待，不用 collector 取代。

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


## Resource 分量的有限正常耗尽

`map-provider-resource-0331` 的两份普通源码支持 own Map / Borrow MutableMap 参数、
String key 与无字段/泛型/接口且有显式 deinit 的直接 class value，正常回边和耗尽。owned Map
仅接通函数体恰为 For 加一个字面量 println 的形状：single 消费公开 Header/Exit phi、
完整 incoming/Choice 与唯一 AfterStatement(scope) Drop，在 source loan 结束后保留实际
当前 owner 至该词法点；unit 独立消费 qualified scope Drop 并运输自身 carrier。
Borrow MutableMap 不取得或清理 caller owner。owned Map 的更长 continuation、continue、nullable
Resource 分量仍由真实 well-typed 邻控保留原 For 的 UnsupportedNode。

两份源×双入口四路经独立原 AST/name/参数/caller、完整 SSA incoming/operand 使用及
词法 Drop 查询后通过 verifier/LLVM；四个普通 native 产物 exit0/完整 golden/空 stderr。
测试明确区分 single Borrow 的支配入口 loan 与需要运输的列，不虚构额外 seed 列；
该片不是云端 Resource 提交的字节恢复，不扩称一般 Resource CFG 或无泄漏。
