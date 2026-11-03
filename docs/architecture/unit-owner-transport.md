# Unit owner transport 观察

> **性质**：实现事实 · **状态**：current · **读取时机**：核对 Unit owner 观察边界 · **唯一真源**：owner transport 生产代码与测试

## 狭窄 owner transport 观察

Unit 产物另外保存逐 callable 的 `owner_observations()`。当前实现限普通函数内的
String Value 参数、非插值 String、Name/Group/local binding、checked root replace 与下述 leaf capture。
定义 token 带 source-qualified callable 与分析内身份；结果使用逐求值帧局部槽，pending
使用 call/argument 配对。旧值在 commit 交给结果，新 RHS 值进入 root；事件在真实转换点
记录，不从 drop、span 或 formed history 推导。`is_consistent()` 核对运输守恒与身份。

观察整体的相等比较保留完整、有序的结构事实：只对应各自观察内的 token，逐项比较
source-qualified callable、定义来源、局部编号、状态及全部事件字段。单个 token、
定义、事件的相等与 `definition()` 查询仍严格核对产品身份；不同分析的 token 不可互换。

不支持的控制流、普通调用、其他 capture、swap 或其他值形态使该 callable 的观察标为
Unsupported 并清空该观察的前缀；原合法性和 cleanup 产品不受这个状态影响。此层不提供
通用 capture lineage、条件 owner graph 或新的 Phase 4 执行能力，原 Single witness 保留。
本地纵切的 Rust 编译/行为验收尚未执行，状态见 [SPEC-0341](../specs/active/0341-shared-lowering-core.md#72-e2-owner-transport-a-的本地纵切2026-11-03)。

## Capture formation

基于 #294 `0bb1f847` 的本地候选只接纳普通函数直线路径中非空、全部 Owned/Move String
的 leaf 环境。形成点在实际 `take_named` 移出处保存当前 token，单独定义环境 token，
以 source-qualified binding 标识 capture 槽。Name/Group/binding 转交同一环境 token；
slot 输入不会随原 binding 的声明位置重建。实际 captured cleanup 发布内容释放，再发布
环境释放。共享和独占资源的旧清理顺序不变。

Shared/Copy/closure-valued capture、空环境、直接丢弃的 temporary 与调用执行仍使整个
callable 观察原子 Unsupported；lambda body 保持独立 Unsupported。此候选尚未编译或
动态验证，不表示旧 cleanup、Single witness、循环实例或 native 已完成迁移。
