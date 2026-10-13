# single Map loop owner 运输

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改 single guarded Map IterationPhi 运输时 · **唯一真源**：`lower_frontend/map_loop_owner` 与普通源码测试

`lower_frontend/map_loop_owner` 消费非 Resource 具名 Map 的精确 IterationPhi 图：唯一
Header/Exit、Entry/Fallthrough/Exhaustion，以及至多一个保留 Fallthrough 的条件 Break
、条件 Continue 或条件 Return，三者不能混合。source 限于 Array/List/MutableList，Map 与 source 不同
root；seed 是真实 Map 构造或拥有型参数。同类型多个 Map 按独立 owner identity 配对。

单条件 jump 与 fallthrough 必须是同一个已发布普通 Control selector 的互补路径；carry
仍为 Header presence 与对应 path 的合取，不改写为 Always。Always Entry 与每条保存
当前 owner 的 Header 回边构成有界归纳，不放宽旧 Resource/source constant-presence 图。
所有 guarded Map drop 入口共用此证明。

实际 SSA 记录 preheader/header/body/exhaustion 的同一 carried slot。continue/fallthrough
完成 cleanup、组好真实 Edge 时逐边核当前 binding 等于该 slot 参数，并记录 kind、精确
continue expression、block 与 ValueId；target 必须是该 Header，重复身份或 block 拒绝。
最终完整 Header 入边集合必须精确等于 entry 和记录的回边；entry 锚定 seed，各回边仅沿
MapPut/MapRemove owner 结果与 block parameter 回溯本轮 body，不只核边数或同型 lineage。

live break/exhaustion 后保留当前 owner，检查实际 exit merge 与最终 drop；dead break 在
两互斥出口消费真实 DropFact。continue 不消费 Map，live/dead 均运输当前 owner至下一轮，
仅 exhaustion 按真实事实保留或清理。provider、element、source、temporary 顺序不变，不
推迟清理至人为共同尾部。frontend、SSA model/verifier、LLVM/runtime 与旧 source proof 未改。

条件 continue 的新增支持限 temporary 与现行可用 Borrow 参数 source。普通源码覆盖
0/1/多轮、never/all/mixed、put/remove present/missing、String/MoveOnly value-class、
own seed 和两个同类型 Map、三个顺序 provider。独立 SSA oracle 核两回边各自最后的更新
结果、source/hidden owner 与 element loan 身份、一次 length 快照及 cursor+1。
两个 Resource local 与解构派生 element loan 的重入 cleanup 专例只验收 SSA/verifier/LLVM；
三组普通 native 矩阵验收其各自完整输出，不把前述专例或 stdout 冒充动态精确释放计数。

unit basic/constant 的 continue live/dead 四格，及各自普通 native，基线就已成功；不是本片
新增 unit 能力。原 break live 两格成功、dead 两格拒绝的独立边界保留。snapshot/closure、
额外 Abort-survival、混合退出、多个/无条件 jump、无 Fallthrough 或多
break、更多 IterationPhi/call seed、RangeView 仍保持明确边界，不是语言禁止。

## 条件 return 的独立终止路径

[0313](../specs/active/0313-single-map-conditional-return.md)在实际 main `8e0ce49d` 上独立实施，
仅增加 temporary/Borrow source 的单条件 Return；随后正常整合0312真实main `51f30090`，
组合回归进行中，原独立基线不变。
Return无phi incoming；Header仍恰Entry/Fallthrough。return Header drop与正常live Exit或dead
Header drop构成精确集合，不能把正常出口live解释为return仍存活。
operand求值完成后保存当前binding；清理只能通过透明block参数转发该精确值。记录Drop所在
block与真正Return终结block，所有cleanup后继必须无环汇至对应点；校验Drop指令、返回
expression origin与返回值不再包含已drop的Map。正常exhaustion、merge和Header完整入边仍检查。

普通源码与原版native覆盖0/早/晚/never、put/remove、own seed、双同型Map及三provider；
独立SSA oracle另按两套construct/slot追踪各自最新owner。body-local String/MoveOnly value-class
返回与Group自然复用已有交付，无Map对照原成功；与Map组合新增成功，不改变transfer/ABI。
直接返回Map含Group的冻结普通源码原就成功，不算新drop-carrier证明。owned nullable remove
返回、return operand中更新Map、外围Resource跨条件return保持精确拒绝；最后一项去掉Map也拒绝。
两个body Resource、String temporary source与Copyable返回有普通native输出证据；不冒充内存计数。
unit basic/constant×Map尾部live/dead四格原已成功，独立native保持；不是新增unit能力。

## owned source 的独立缺口

此独立缺口已由 [SPEC-0312](../archive/specs/0312-owned-source-conditional-continue.md)
的 single source-only presence 证明有界修复。原无 Map live/dead、三种 Sequential source
及 `providers-cleanup.ko` 原字节均经真实 frontend→SSA/LLVM/native 验收；
`lower_frontend_map_continue_source_tests` 保留精确 source root/Header/Exit 断言并转为正例。
Map live/dead × source live/dead 的双同型 Map 普通 native 对照通过；分别查询两 Map，
不把尚未支持的跨 owner 短路条件清理混作本片能力。原 Resource/global implies 与 Map
carrier 算法保持；当前 source seed、nested/跨 loop 边界见[provider](finite-sequential-iteration.md)。

owned source return在去掉Map后，local/own seed×source live/dead四格仍为独立旧拒绝；
0313不扩大通用source/Resource proof，Continue证明不自动覆盖Return终止路径。

[0311 合同](../archive/specs/0311-single-map-conditional-continue.md)记录原始红测、范围修订、
隔离 target 复核与待发布事项；[0310](../archive/specs/0310-single-map-conditional-break.md)
保留 break 的有界历史。
