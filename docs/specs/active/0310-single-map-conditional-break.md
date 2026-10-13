# SPEC-0310: single Map owner 条件 break 运输

> **性质**：变更合同 · **状态**：in-progress · **读取时机**：实施或验收 single Map 条件 break 时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0310` |
| 所属 Phase | Phase 3 事实消费、Phase 4 single SSA/native |
| 语言规范 | [Guide10](../../guide/10-ownership-borrowing-drop.md)、[Guide12](../../guide/12-collections-destructuring.md) |
| 批准依据 | 2026-10-13 用户“合并后继续推进”；PR88 合并后从 actual main 激活 |
| 前置 Spec | [SPEC-0300](../../archive/specs/0300-single-map-loop-owner-carry.md) |
| 前置 ADR | [ADR-0006](../../adr/accepted/0006-typed-ssa-block-parameters.md) |
| 阻塞项 | 红绿验收、独立审阅、PR 双宿主 CI 与发布尚待完成 |
| 影响范围 | single 外部具名非 Resource Map 的有界 for break；unit 独立对照 |
| 语言语义变更 | 否 |

## 1. Goal 与冻结输入

在已有 Array/List/MutableList 顺序 for 中，外部具名非 Resource Map 可经条件 break 离开，
loop 后 live 时运输当前 owner 并最终清理，dead 时消费真实逐出口 cleanup，运行路径恰一次。
不按源码名称、payload 类型名或同型配对放行。Map 与 provider source 是不同 root。

基线为 actual main `162e8e5dd75436b80571b5a30637923d2a0cc24a`，tree
`87f2c9d64523672a3cc49f154aed0186225aa8a4`，有序 parents 为 `cf7f8ede857f50041d5d91b3175a039555571171`
与 `282176c66d8f254971c8d48d71da8e70ffef735b`；本地原始 commit 字节已核 SHA。
激活时 [main CI](https://github.com/Halckon/Koven/actions/runs/38402505848) 仍运行，不声称基线 CI 完成。
远端 branch/PR 查询、本地与实际 main 的 Spec 库存核验 0310 未占用后建立 `feature/spec-0310`。

## 2. 有界支持与非目标

沿用 0300 的 seed、Map、payload 与 source 限制：真实 Map 构造或 own 参数，独立 owner identity，
静态 presence。每个 plan 除 Entry/Fallthrough/Exhaustion 外至多一个精确 Break(expression) incoming，
仍需真实 Fallthrough。该边界以 frontend 同轮 statement/expression/point/condition 与实际 SSA
lineage 证明，不重新遍历 AST 定义语言退出规则。普通 if 条件 break 不推定任意 owner CFG。

live Exit 从 break/exhaustion 接收当前 Header owner，合流后 contains/put/remove 保持；dead Exit
两边均缺席，互斥出口各消费对应 DropFact。按实际路径唯一消费，不能要求全函数只有一条 Drop，
也不能把清理延后到人为共同尾部。provider/element/source/temporary 清理保持原有事实顺序。

continue、return、无 Fallthrough 的无条件 break、多 break、Map provider、RangeView、连续/嵌套
IterationPhi seed、返回 Map 调用 seed、replacement、snapshot/closure、额外 Abort-survival guard
均不在新证明内；无条件与多 break 的实际拒绝须由普通源码测试确认，不预记已通过。
不重造 frontend facts，不改 Guide、ABI、verifier 或 LLVM；Resource 旧 constant-presence 独立保留。

## 3. 实施与证据要求

先提交 ordinary live/dead 条件 break 源码红测，记录 frontend 成功及 single 精确失败；再最小实现。
同源 unit basic/constant 分开验证，不从 provider 已支持 break 推定组合成功。

- 0 轮、首轮 break、多轮 break、条件不命中耗尽；loop 后 live/dead。
- String 与既有 MoveOnly payload；Array/List/MutableList，named/temporary source。
- 构造/own seed、两个同类型 Map；put、remove present/missing。采用有界代表组合，不做全排列。
- 真实 SSA 核 entry/backedge/break/exhaustion/合流 lineage、当前更新结果与逐路径 drop；原版 LLVM/native 核正常输出。
- 拆分 continue/return 拒绝，保留 0300 边界与相关 Map/provider 普通回归。

只在静审所选测试后运行真实 frontend/SSA、未改写 LLVM 与普通 native。云上不运行故障注入、
校准、sanitizer、bad IR、mock/stub aggregate、计数插桩或 full discover。原始日志/JSON 留仓外，
本 Spec 仅记短账本与稳定 CI 链接。stdout 不宣称分配计数或完整内存安全。

## 4. 唯一验收账本

| 验收项 | 输入与选择 | 实际结果 |
|---|---|---|
| 基线与合同 | §1 actual main；当前分支 | 已核 commit/tree/parents；无生产改动 |
| ordinary 红测 | live/dead 条件 break + 同源 unit | 待执行 |
| facts/carrier 正例 | 有界矩阵、真实 SSA/LLVM/native | 待执行 |
| 保留边界 | 0300 边界与新增退出形态 | 待执行 |
| 相邻回归 | 按实际 diff 选择 Map/provider 普通测试 | 待执行 |
| 静态检查 | fmt、codegen clippy、docs、尺寸、diff | 待执行 |
| 独立审阅/双宿主 CI | 最终精确提交 | 待执行；macOS 本地未运行 |
