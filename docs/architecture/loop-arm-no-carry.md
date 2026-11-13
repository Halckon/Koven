# 受限循环双臂 no-carry

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改循环内分支借用的闭合事实时 · **唯一真源**：两 checker 的 `no_carry.rs` 与 Unit `loop_no_carry.rs`

旧直线合同见[循环 no-carry](loop-no-carry.md)。本片实施合同见 [SPEC-0341](../specs/active/0341-shared-lowering-core.md)。

## 受限双臂前端闭合事实

另一个选择器只允许 while 的唯一 body 表达式为稳定 Boolean 参数控制的 if/else。
两臂分别为同源 String 参数的一次显式借用与非空同步消费序列。每臂持有独立 Frame，
第一臂实际 acquire/return/End/scope close 完整成功后才可检查第二臂；两臂各自恢复
同一入口资源状态，再核正常回边。任一诊断或未完成臂撤销整对事实。

获选 Unit-valued arm 走逐 statement End 路径，不把最后 consumer 当作借用值合流；
最终 drop planner 再独立核两项 End 身份。`no_carry_branches()` 与旧 `no_carry_loops()`
分开，两个 arm 不冒充直线 loop；成对事实记录精确 if、then/else scope、binding、
initializer、consumer 和 End。同一 publication gate 在全局失败时清空两个领域。
当前只是前端闭合事实，尚无该形状的独立后端 receipt 或 native 验收。
跨臂借用、借用 phi、条件副作用、跳转、嵌套、多根及跨轮携带仍拒绝；不改变上述
直线后端合同，也不以新事实代替旧 SSA verifier 或独立见证。
