# single Map loop owner 运输

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改 single guarded Map IterationPhi 运输时 · **唯一真源**：`lower_frontend/map_loop_owner` 与普通源码测试

`lower_frontend/map_loop_owner` 消费非 Resource 具名 Map 的精确 IterationPhi 图：唯一
Header/Exit、静态 Entry/Fallthrough/Exhaustion，以及至多一个保留 Fallthrough 的条件 Break
输入与精确 guarded drops。source 限于 Array/List/MutableList，Map 与 source 不同 root；
seed 是真实 Map 构造或拥有型参数。同类型多个 Map 按独立 owner identity 配对。

break 与 fallthrough 必须来自同一个已发布普通 Control selector 的互补路径；carry 和 dead
break drop 的条件仍为 Header presence 与该路径的合取，不改写为 Always。Always Entry
和唯一保留当前 owner 的回边构成 Header presence 的有界归纳证明；不放宽旧 Resource/source
constant-presence 图或引入一般存在位 ABI。所有 guarded drop 入口共用此证明。

实际 SSA 记录 preheader/header/body/exhaustion 的同一 carried slot；entry 追溯至真实 seed，
backedge/break 只沿 MapPut/MapRemove owner 结果与 block 参数追溯至本轮 body。live Map
在 break/exhaustion cleanup 后保留当前 owner，并检查真实 exit merge 的全部入边与最后 drop
lineage；dead Map 在两条互斥出口各消费实际 DropFact，合流不得复活。provider、element、
source、temporary 顺序不变，不把清理推迟到共同尾部。缺失 binding/carrier 显式失败。
verifier、LLVM/runtime 与 frontend 未改。

正常耗尽回归保留 0/1/3 轮、String/MoveOnly、own 参数及双 Map。条件 break 普通源码覆盖
零轮、首轮、多轮与未命中耗尽，live/dead、put、remove present/missing，named/temporary
source 与三个顺序 provider。独立 SSA 断言核真实更新结果、两退出合流、互斥 Drop、element/
source loan 结束及临时源清理次序；stdout 只证明可观察输出，不冒充分配计数。

unit basic/constant 同源 live SSA 与普通 native 是独立既有成功对照；同源 dead 两入口仍在
loop Span 显式拒绝，基线及本片分别验证，不由 single 成功外推。一般 snapshot/closure、
额外 Abort-survival guard、continue/return、无 Fallthrough 的 break、多 break、连续/嵌套
IterationPhi seed、返回 Map 调用 seed 与 RangeView source 保留边界；不是语言禁止。
[0310 合同](../specs/active/0310-single-map-conditional-break.md)记录有界验收与待交付项。
