# Unit loan 实际动作源

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改普通 loan 观察、cleanup 投影与覆盖边界时 · **唯一真源**：`compilation_unit/loan_actions.rs` 与 `dataflow/drop_planner/action_source.rs`


## 当前有界来源

Unit drop planner 的 acquisition 注册与原 `iteration_actions` cleanup 现保存于同一
append-only `ActionSource`。旧 direct/prefix/provider 切片只迭代其中 Cleanup，旧 cleanup
下标保持，Begin/ReceiverBegin/ReceiverEnd 不参与旧计数；不从旧表排序拼出顺序，也不增加第二次 End/Drop 执行。
`loan_action_observations()` 按 source 发布 Begin 和已有实际 cleanup 的观察，错误/deferred
仍在 raw 与公开边界全局撤销。静态 record 的封闭 catalog ID 不可跨 source 产品或重分析解析；
结构快照相等不表示 catalog 相同，记录顺序不是跨分支、循环或 callable 的执行时间线。

source 总体仍只发布 Partial 或 Unsupported：有记录仍可能遗漏 result handoff、receiver
activation 和权限恢复；空 source 动作表不能证明 absence。没有整个 callable、动态路径或
activation 的完整证据，未退休 Single oracle。既有receiver正常End表改为同源投影，其他result观察独立保留。

## 普通 direct call 正常返回区域

`regions()` 单独声明 protocol 1 的调用者侧正常返回区域与两族义务：Shared实参建立/结束，
以及对应String临时owner Drop。只准入顶层普通函数中独立call语句（整body无其他语句）
或单call表达式body，callee为普通Name/Declaration、无泛型/receiver、Owned Int/Unit结果；
实参必须为无插值String字面量直接传到String Borrow参数，也允许零实参。已有pending
loan/temporary/capture/provider、控制、命名变量实参、Group/nullable转换及nested call不纳入首片。
这是有界正常边，不证明callee动态必然返回，也不覆盖callee内部或Abort路径。

真实planner进入通用Call分支前记录混合动作游标，最后一项call清理后才登记正常完成。
独立validator逐项核完整未筛选区间：源码顺序的全部Begin、逆序End、逆序temporary Drop，
身份与CallReturn均须同call，Drop须匹配独立AST字面量span。按call索引复用checker事实，
不逐call重扫所有loan，不从旧表排序或删除额外动作。符合此固定协议的region可Complete；
零参数须同时有实际planner入口、正常完成、AST/typed零参数与空区间，才是Complete-empty。
未准入为Unsupported，已准入却未完成/验证不全为Partial；source总体不因此Complete。

专用String/Rc/construction早返回可能没有region，缺席不证明absence；区域记录仅为静态
同轮catalog事实，不是运行时轨迹。`owns_region`同时保护零动作证书的catalog归属，
clone保留归属，重分析即使source/call下标相同也不接受。原始候选与公开region随source错误/deferred一起撤销。
实现、真实普通测试和CI-only负控的未运行边界见
[SPEC-0341](../specs/active/0341-shared-lowering-core.md)。

## Owned 参数 Shared receiver 正常返回区域

protocol 2与protocol 1分开声明SharedReceiver/NamedSourceDrop两族，只准入上述flat caller
中的零实参普通Symbol方法、Owned Int/Unit结果。receiver须为无Group/字段的直接Name，
对应本caller显式Own参数、Owned binding与仍在ValueState中的真实OwnedValue；类型须为
无泛型、已判定非Resource的MoveOnly普通Class。typed receiver、契约、静态fact、place
及独立AST使用span逐项一致，Named Drop origin另取参数形成marker，不借receiver End推断owner。

Shared receiver Begin来自实际pending注册，正常End来自原pending token提取点及原bounded
筛选门；旧`receiver_call_return_ends`按原顺序投影该源，observer关闭仍保留End，新观察不进入
旧iteration cleanup执行流。独立validator核完整未过滤区间恰为Begin、同token正常End、
同CallReturn的Named root Drop；最后一项的owner origin必须匹配独立参数形成span。
有真实进入/正常完成且三项闭合才为Complete，根仍live未Drop为Partial，未准入为Unsupported。

Borrow/Inout参数、Two-Phase、this、投影/转换、temporary/Rc、provider/capture与borrowed
result仍不准入；borrowed-result原诊断不变。仅证明当前Shared token退休及实际owner销毁，
不是通用permission restoration证书，不证明callee必返或source/callable Complete。
receiver专用on/off全产物差分与跨source错误/deferred撤销均保持；catalog身份仍不能跨重分析。
