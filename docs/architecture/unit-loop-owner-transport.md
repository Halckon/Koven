# Unit 闭合 While owner 观察

> **性质**：实现事实 · **状态**：current · **读取时机**：核对真实 While body 的闭合运输和结构回边 · **唯一真源**：drop planner 的 owner transport loops 收集器、loop 只读产物与 V1 validator

基础协议见 [Unit owner transport](unit-owner-transport.md)，受限条件值见 [Choice](unit-choice-transport.md)。
本页不授予循环携带 owner/phi、实例级释放或 Phase 4/native 能力。

## 准入与真实执行边界

仅顶层普通、非泛型 Unit 函数，一个无 mode marker 的精确 Boolean 参数，整个函数体仅一个
While。条件只能该真实参数 symbol 的 Name/Group 链，每层精确 Boolean 且非 constant-use，
原条件分析必须同时保留 true/false 两条边。函数、循环和 body 均保留 source-qualified 身份。

body 必须非空，仅精确 String/MoveOnly 的局部 val/var 声明；initializer 可为无插值 String、
已有本地 symbol 的 Name、Group 或真实 checked 完整根 Replace 的嵌套组合。每层精确 String、
非 constant-use；替换根必须已在 body 声明，descriptor、操作数顺序和 root 形态都核对，
根可变性仍由原 checked primitive 生产者证明。成员身份用有序集合，initializer 检查通过后才加入本地 symbol 集合。

旧 While 遍历只执行一次。条件实际完成后，在无外层循环/分支及完整空 ValueState 时启用私有
body sink；保存旧 observer、Choice 事务和 branch depth，body 使用全新 catalog identity。
真实 String 构造、take、binding、pending、replace commit 和 named cleanup 继续由原 hook
登记；不从 drop facts 或 span 反推 owner，不重新遍历 body，也不改变实际 cleanup。

实际 `statement(body)` 的 Result 先保存，恢复全部旧 sink 后再传播错误。只有真实返回 true、
scope cleanup 已完成且完整 body state 为空，才用原无 Choice V1 投影封闭 body。该 validator
要求每个真实 definition 最终为 Dropped；旧 `is_consistent` 或单独检查 values 为空均不够。
空边界穷尽 owner、pending、loan、borrow transports、provider、closure、replacement 及
this/lambda 槽；新增 ValueState 字段须显式作边界决定。真实 LoopExit 清理后再次检查空状态。

## 独立循环产物

`loop_observation_v1()` 独立于旧观察和 `value_observation_v1()`。旧 callable 仍在 While
statement 原子 Unsupported，定义和事件为空；私有 body catalog 不增添 callable 观察项。
外层 analysis identity 绑定 loop，独立 body identity 再绑定整份 transport，防止同源码二次
分析的完整 catalog 被替换进来。只在 successful 且无 deferred 的最终产物保留新能力。

三条结构边来自实际准入及遍历：条件 true 进入实际 body、真实 body fallthrough 在 scope
cleanup 后回到条件、条件 false 到真实 While LoopExit。它们是控制关系，不是已经发生的
有限运行轨迹；body 无外来 owner、未完成交付或存活后继值。

body definition token 是静态 producer 身份，每次激活都要求新实例，不能将不同轮实例混为
一个 token。此产物只给出闭合模板，不发布 runtime instance handle 或 loop-carried phi。
条件是不可变参数且 body 不修改它：false 为零轮，true 可持续循环；任意有限前缀均不证明
耗尽或终止，不声称执行了 1/2/7 轮后退出。

## 保留边界

普通调用、外来/跨轮 owner、参数模式、capture、分支、Return/Break/Continue、嵌套或顺序
多个循环、For/Loop、表达式语句、未消费 temporary、nullable、普通赋值及其它条件均不准入。
不删除 Single 循环见证，不扩 Choice，不改变 Guide 的 ASAP 或 loop liveness 合同。
真实源码、双 Unit 入口、开关完整 Analysis 对照及质量门禁范围见 SPEC-0341 §144；
手工损坏产物只作为正式 CI 守卫，不能替代生产接线证明。
