# SPEC-0292: 链内立即消费序列与 owned 结果

> **性质**：变更合同 · **状态**：draft · **读取时机**：准备或验收 consume 首片时 · **唯一真源**：本 Spec；语言语义以 [Guide](../../../guide/README.md) 为准

| 字段 | 值 |
|---|---|
| 状态 | draft |
| Goal ID | `KOV-P4-0292` |
| 所属 Phase | Phase 2/3 owned 契约与清理；Phase 4 SSA；Phase 5/6 std 与 native |
| 语言规范 | 当前 v0.43；首片具体合同须先形成并启用新 Guide |
| 批准依据 | 2026-10-08 12:15 UTC 用户明确同意修订后首片本地实施 |
| 前置 Spec | [SPEC-0289](../../active/0289-n1a-range-carrier.md) 的最小 std 声明接线；独立消费原语准备可先推进 |
| 前置 ADR | [ADR-0008](../../../adr/accepted/0008-internal-value-and-allocation-abi.md)、[ADR-0016](../../../adr/accepted/0016-interprocedural-borrow-abi.md) |
| 关联 ADR | 新消费序列与槽位移交 ABI 待形成独立决定 |
| 阻塞项 | 新 Guide/ADR 与 std 声明；消费构造/有效槽位/清理原语；前置 0289 未完成 |
| 影响范围 | lang-frontend、lang-codegen、lang-std |
| 语言语义变更 | 是；范围已授权，合同冻结前保持 draft |

## 1. Goal

consume 成功求值立即使源 binding 失效；链中临时 owner 在下游 own 接管前承担正常
退出清理，下游立即产出 owned List 或单值，不保留惰性消费序列。

## 2. 范围与边界

首片 ConsumingSeq 仅为链内单次 owned 接收者，不可保存、存储、传参或返回。
先贯通独立接管与正常提前退出，再加入消费 take，最后按已批准范围扩消费 filter/map
与必要短路矩阵。编译器只提供可复用构造、完整槽位移出/丢弃与结果构造原语；
算法 body 由 `.ko` 实现。普通借用 filter/N1b 不属于此路径。

消费 take 的数量遵循 [0289 §6 已批准计数合同](../../active/0289-n1a-range-carrier.md#6-已批准计数边界与交付)：
2026-10-08 13:08 UTC 用户批准负数 Abort、超过 size 截边界；非负先 clip 后算，
空源、零、size 与最大 Int 按同一规则。数量与 borrow 路径一致，owner/cleanup 仍按本
Spec 独立验证；此批准不增加本 Spec 的 API 范围，也不表示消费或清理已实现。

Resource 分类与词法清理保持；算法显式丢弃与未使用 local 的 ASAP 分开。
pred 同步 Borrow 并返回 Boolean，先结束元素借用再移动/清理；消费 map 为 own T -> U。
U 的类型可逃逸不能代替值的 borrowed capture 检查；合法 owned/Copyable capture 保留。
局部 lambda return 合法，跨 callable 的非局部退出不开放。Abort 不展开。
any/all 正常短路须同时清理已检查与未访问元素，恰好一次；不新增 partial field move。

## 3. 非目标

N1b、可选借用、Option、多来源、用户非逃逸类型、惰性链、inout 局部/返回、nullable
owned remove、readonly Map 转换、隐式物化/clone 与 T?? 展平不进入本 Goal。

## 4. 实施与唯一验收账本

| 验收项 / 目标 | 实际结果 | 未完成原因 |
|---|---|---|
| 接管时点、源 move、active loan 冲突与链位置拒绝 | 未实现、未运行 | 新合同/原语待冻结 |
| 接管前实参正常 return 清理与 Abort 分类 | 未实现、未运行 | frontend 临时 owner 事实待发布 |
| single/unit SSA transfer/slot/verifier 与正常 native | 未实现、未运行 | 上游事实与构造原语待接通 |
| String/MoveOnly/Resource，空/保留/丢弃与正常短路精确清理 | 未实现、未运行 | 按小闭环顺序实施 |
| pred Borrow、map own 与 capture 逃逸正反例 / Span | 未实现、未运行 | callback 接线未形成 |
| fmt/check/clippy、尺寸、docs/diff、聚焦与最终过滤全库 | 未运行本 Spec 门禁 | 最终源码尚未形成 |

按每步失败测试、最小实现、同选择绿测试推进；0289 的事实不等于此 Spec 已完成。
本次不暂存、提交、push、PR、merge 或归档0288；故障操作与 Linux 动态闭环不运行。
