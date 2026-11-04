# Unit owner transport 观察

> **性质**：实现事实 · **状态**：current · **读取时机**：核对 Unit owner 观察边界 · **唯一真源**：owner transport 生产代码与测试

## 狭窄 owner transport 观察

Unit 产物另外保存逐 callable 的 `owner_observations()`。当前实现限普通函数内的
String Value 参数、非插值 String、Name/Group/local binding 与 checked root replace，
以及下节限定的显式双支 Unit If 或 leaf capture（同 callable 不混用）。
定义 token 带 source-qualified callable 与分析内身份；结果使用逐求值帧局部槽，pending
使用 call/argument 配对。旧值在 commit 交给结果，新 RHS 值进入 root；事件在真实转换点
记录，不从 drop、span 或 formed history 推导。`is_consistent()` 核对运输守恒与身份。

观察整体的相等比较保留完整、有序的结构事实：只对应各自观察内的 token，逐项比较
source-qualified callable、定义来源、局部编号、状态及全部事件字段。单个 token、
定义、事件的相等与 `definition()` 查询仍严格核对产品身份；不同分析的 token 不可互换。

不支持的控制流、普通调用、其他 capture、swap 或其他值形态使该 callable 的观察标为
Unsupported 并清空该观察的前缀；原合法性和 cleanup 产品不受这个状态影响。此层不提供
通用 capture lineage、条件 Choice/phi 或新的 Phase 4 执行能力，原 Single witness 保留。
本地纵切的 Rust 编译/行为验收尚未执行，状态见 [SPEC-0341](../specs/active/0341-shared-lowering-core.md#72-e2-owner-transport-a-的本地纵切2026-11-03)。

## 显式 Unit If 的无 phi 观察切片

仅准入显式 else、结果类型为 Unit、两边均继续且 cleanup 后全部存活绑定保留相同 token
的 If。condition 在分叉前求值，各边的真实 control body 与 branch-exit cleanup 分别
记录为 `UnitOwnerTransportEvent::If` 的 true/false 有序子序列；不把互斥路径拼成线性流。

validator 从同一父 location 分叉重放：继续状态中的 owner 必须一致；只在某支创建并在
该支真实 Drop 的定义可在 join 后标为不可用。定义只能在一个静态创建点首次登记，嵌套
控制身份不可重复，BranchExit 必须属于当前边；跨分析整体相等比较递归保留两支事实，
独立 token 与 lookup 仍不可互换。此校验证明已发事件的内部守恒，不重推 AST 合法性。

不同存活 owner 的 join、String-valued If、return 分支、隐式 else 和 When 仍整体拒绝。
旧 cleanup 的 symbol 交集不能证明 owner 相同；无 else 的旧路径仍真实执行边级 ASAP
清理，不能当作 location 原样直通。完整 Choice/phi 需要独立的 incoming-edge 覆盖、
非继续路径与 return payload 交付合同；本片没有定义或实现 Choice。

回归源码包括两支消费同一输入 owner、分支局部创建/清理、相同 owner 跨 join 后 replace、
嵌套 If，以及扁平化/缺边/外来 token/重复创建/错误 cleanup edge mutation。
全部保留原七类不支持边界源与 observation enabled/disabled 全 planner 产品比较。
Rust 行为尚未执行，静态检查不代表精确头官方 CI 验收。

## Capture formation

基于 #294 `0bb1f847` 的本地候选只接纳普通函数直线路径中非空、全部 Owned/Move String
的 leaf 环境。形成点在实际 `take_named` 移出处保存当前 token，单独定义环境 token，
以 source-qualified binding 标识 capture 槽。Name/Group/binding 转交同一环境 token；
slot 输入不会随原 binding 的声明位置重建。实际 captured cleanup 发布内容释放，再发布
环境释放。共享和独占资源的旧清理顺序不变。

Shared/Copy/closure-valued capture、空环境、直接丢弃的 temporary 与调用执行仍使整个
callable 观察原子 Unsupported；lambda body 保持独立 Unsupported。此候选尚未编译或
动态验证，不表示旧 cleanup、Single witness、循环实例或 native 已完成迁移。

## 两片整合的 admission 边界

同一 callable 只接纳 If 或 capture 中的一类；首次遇另一类即 Syntax Unsupported，
真实 traversal/cleanup 仍照旧执行。该标记在每个 callable 开始时重置，不随事件子序列
暂存而遗失，因此嵌套分支 capture 也不会被机械放行。validator 独立拒绝 Closure 定义
与 If 树并存；这里只保留两片分别支持的集合，不声称 capture×branch 组合可用。

新增回归核双向/嵌套拒绝、完整前缀回滚、sibling 隔离、Copyable 跨 join、嵌套控制树及
重复 control mutation；同源开关比较保留完整旧 Analysis。上文各候选阶段的未运行说明
不代表组合已有动态验收；本地组合仍待精确官方 CI，见
[SPEC-0341 整合记录](../specs/active/0341-shared-lowering-core.md#79-capture-与-if-分立观察整合候选2026-11-03)。

## 循环拒绝回归切片

循环 owner 仍未准入。新增静态回归固定 While/Loop 在外层 statement 原子 Unsupported：
常量 false 的零轮边界、未改变条件的正常回边、continue、break 与嵌套循环都清空整个
callable 的观察前缀；独立 sibling 仍完整。另用独立准入前缀与非法事实注入检查防止空测。
同组源码比较 observation on/off 的完整旧 Analysis，要求 cleanup 的身份、次序与全部
字段不变。测试只分析源码，不运行循环，也不把有限回放前缀当作真实多轮终止证据。
本片不改生产支持；编译和行为仍待官方 CI，静态检查不替代执行。

## Test-only Choice 模型

`owner_transport_tests::choice_model` 保存私有、单选择的候选事件模型，仅在 `cfg(test)`
路径可达，不改变公开 owner API 或生产准入。它用真 definition catalog 的身份构造手工图，
分别验证两条替代路径及完整后续；Choice 不成为新 String producer。模型不证明 AST 类型、
实际尾值交付或未记录的调用/退出，生产 String-result If 仍 Unsupported。实际消费者的
cleanup 和 Single witness 保留，准入接线须另立合同；验收见 SPEC-0341 §83。
