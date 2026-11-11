# 共同出口 Replace 的 owned Return

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改共同 P/Q 后继 root Replace 或 Return · **唯一真源**：Phase 3 Return carrier 与 Phase 4 owner consumer 代码

## 范围与身份

基于[共同 P/Q 出口](unit-replace-break-owner.md)，支持唯一尾句
`return replace(&held, "last")`。根、prefix 与一次 Replace→Break 不变，RHS 仅裸
String literal，保留 Guide 10 root Replace 与 owned Return 的语言/ABI。
不扩 Group、concat RHS、结果 alias、额外后继或一般循环；相邻合法源码仍走旧能力边界。

现有 Return consumer 增加受限 replacement frame，而非另开平行 suffix getter。
P/Q 保留两个原 catalog；独立 R catalog 起初为空，只在真实 String 完成回调创建 definition。
旧 Unsupported scalar 的 P 投影不更新为 R，也不能代替 old=P/Q 结果身份。

## Phase 3 真实转换

原 String 完成记录 Create(R)；原 Value 参数已注册唯一 transfers-at-call temporary 后记录
Pending(R)。checked root Replace 后核同一 root 的实际状态及更新后的 call origin，保存
该 committed state；不修改普通状态合流或补写 scalar owner。

真正的 CallReturn root removal 必须匹配该完整状态与实际 drop point，此时退休 installed R，
但保存 old=P/Q 结果义务。原 call 完成再将 carrier 移入 EvaluatedOwner，原 Return traversal
交付；封印六步 Create/Pending/Replace/Drop/Result/Return 后才发布完整 consumer。
清理仍在 CallReturn，不能挪到 Return ControlTransfer；观察 on/off 不改变旧清理语义。

## Phase 4 单一 emitter

后端复核同轮 typed/owned Replace descriptor、R producer、唯一实际 root Drop index，
及原两条入边的角色和完整集合。R String result 必须来自实际生成槽；原 RootReplace
发射后的回调才保存 old-result 槽，不能把现在持 R 的 root 槽当旧值。

既有 Drop emitter 的前置授权与后置回执绑定 installed root 槽和同一 drop index；完成
该清理后，原 finish_return 才能交付 old-result 槽。没有第二个 Replace、Drop 或 Return
emitter；原 body detached P 与调用者接收结果的清理保持原路径。

## 有界验收

普通同源 Phase 3 receipt 缺失红、Phase 4 MissingFact 红分别闭合；真实 Basic/Const SSA、
LLVM 和有限 native 输出 outer/next 通过。四个合法排除邻居和既有 B、Call、直返、alias
Return 的普通回归单列。输出不代表析构计数或泄漏证明；故障与 foreign/压力等仅编译，
官方 CI 未执行。精确本地头、名单、门禁与失败保留见
[SPEC-0341](../specs/active/0341-shared-lowering-core.md#共同出口-replace-结果-return受限合同2026-11-10)。
