# SPEC-0311: single Map owner 条件 continue 运输

> **性质**：变更合同 · **状态**：in-progress · **读取时机**：实施或验收 single Map 条件 continue 时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0311` |
| 所属 Phase | Phase 3 事实消费、Phase 4 single SSA/native |
| 语言规范 | [Guide10](../../guide/10-ownership-borrowing-drop.md)、[Guide12](../../guide/12-collections-destructuring.md) |
| 批准依据 | 2026-10-13 用户持续里程碑开发、合并后继续推进；PR89 实际合并后激活 |
| 前置 Spec | [SPEC-0310](../../archive/specs/0310-single-map-conditional-break.md) |
| 前置 ADR | [ADR-0006](../../adr/accepted/0006-typed-ssa-block-parameters.md) |
| 阻塞项 | 本地红绿、独立审阅、远端 CI 与交付尚待执行 |
| 影响范围 | single 外部具名非 Resource Map 的一个条件 continue；unit 独立对照 |
| 语言语义变更 | 否 |

## 1. 冻结输入与 Goal

actual main `614a7a7f5ff77fecb337c3b6d18118e687d73c52`，tree
`7835aa0a4ee0e065cf51203f8cfb18b2a9b3703c`，有序 parents
`162e8e5dd75436b80571b5a30637923d2a0cc24a`、`0c5f7be3196e23a1d951be0413abe02a3fe60ec2`。
签名 payload 与 parent API metadata 恢复 Git 原字节并核 SHA；本地及远端库存、branch/PR
查询确认编号未用后，从实际 main 建立 `feature/spec-0311`。
[actual-main CI](https://github.com/Halckon/Koven/actions/runs/38412890083) 激活时 pending，独立跟踪。

在既有 Array/List/MutableList 顺序 for 中，以 temporary 或现行可用 Borrow 参数为 source，
外部具名非 Resource Map 在一个条件 continue
与唯一 Fallthrough 上运输当前 owner；零轮与多轮耗尽后按真实 live/dead 事实清理。
保留 0300 正常耗尽与 0310 单条件 break，不把多个独立证明自动组合。

## 2. 最小合同与非目标

- Continue(expression) 与 Fallthrough 必须是 frontend 同一普通 Control selector 的互补
  路径；核 statement、kind、point、condition、Header target，不从 AST 重推控制语义。
- Entry 唯一且 Always；两条 Header 回边各自是当前 Header presence 与路径的合取，
  唯一同 owner 来源；Exhaustion 唯一且到 Exit。沿用真实构造/own 参数 seed、非 Resource
  Map 分类、source 不同 root 和 snapshot/closure 限制。
- cleanup 后读取当前 binding，并记录真实 block、slot、Header target 与 ValueId；完整
  Header 入边集合必须等于 entry、记录的 continue 与 fallthrough。逐边核最新 MapPut/
  MapRemove 结果，不能只增加入边数或用同型 lineage 代替当前 owner 相等。
- continue/fallthrough 保留 Map、source loan 与 temporary source；先本轮 local/element
  cleanup，再 cursor+1，重入同 Header。length 只在入口拍一次快照；耗尽才结束 source/
  hidden owner。live/dead 最终 cleanup 仍消费原事实，不移动至人为共同尾部。
- break+continue、多个/无条件 continue、return、额外 Abort-survival、replacement、
  snapshot/closure、RangeView、Map 自身迭代、更多 seed 不在本片；普通源码锁明确边界。
- 不修改 Guide、frontend、SSA model/verifier、LLVM 或语言语义，不按名称或 payload 特化。
  unit basic/constant × live/dead 逐独立测试确认 baseline，不能从其他组合推断成功或拒绝。

### owned named source 的既有独立缺口与范围修订

2026-10-13 首轮实现后，原 `providers-cleanup.ko` 的 owned local `keys` 在 single 仍拒绝；
不是 Map owner 运输失败。冻结三项普通源码对照（`lower_frontend_map_continue_source_tests`）
分别运行在 actual-main 生产字节的 tests-only `e2f9f1e9` 与本片 `eb38a7c4`：

- 去掉 Map、保留 owned keys + conditional continue + loop 后 keys.size：两者均在 keys
  Span 23..27 UnsupportedNode；source Exit owner 的 DropFact 在 keys.size 的 CallReturn。
- 保留 Map、不再读取 keys：两者同样拒绝 keys；source Header owner 的 DropFact 位于 LoopExit。
- temporary source + Map：基线拒绝 Map Span 23..28，本片真实 SSA/LLVM 成功。

source 回边 available_when 是 Header presence selector 的 `[Never, path]`。旧
`constant_presence::prove_graph` 只把完整 presence condition ID 作为已证条件，不能处理该
新合取；不通过修改 Map proof、frontend 或 verifier 掩盖它。
root 在上述实证后明确修订本片验收：只承诺 temporary 与 Borrow 参数 source，owned local
source 保持稳定拒绝；原 fixture 原字节和红日志保留，新增 Borrow fixture 不冒称覆盖原 owned
名义。将预期改为拒绝的绿色不是 Map 行为修复。

此缺口是紧接 0311 的优先修复项，先于 Map return：下一独立 source-proof Spec 应关闭
source live/dead 条件回边证明并核实际 owner/source loan/cursor 与 cleanup；本片不占后继编号、
不新增 source/Resource 通用证明。原输入、probe 和精确事实是后继起点，不仅登记 known boundary。

## 3. 验证与实施顺序

先独立提交真实源码测试，再实测红点与 unit 四格；production 必须晚于红测提交与实际执行。
按 live/dead、0/1/多轮、runtime never/all/mixed、continue 前 put 与尾部 put/remove、
重复 remove present/missing、三个 provider、Borrow 参数/temporary、MoveOnly payload、own seed
与两个同类型 Map 选代表矩阵，不做全排列。

独立结构 oracle 核真实 operation result、逐回边当前 owner、source/element loan、length、
cursor 与 cleanup 顺序；真实 frontend→SSA verifier→未改写 LLVM→普通 object/link/run。
stdout 只证明输出，不宣称动态分配计数或完整内存安全。
保留 0300/0310、nullable cleanup、普通 provider、snapshot/closure 拒绝定向回归。
云上禁止 fault/calibration/sanitizer/bad IR/mock/stub aggregate/计数插桩/full discover，
不运行聚合 driver。Cargo 独占串行，raw logs 仓外；最终 fmt/clippy/docs/尺寸/diff。

## 4. 唯一验收账本

| 验收项 | 精确输入/选择 | 实际结果 |
|---|---|---|
| 冻结基线 | §1 actual main、原始 Git 字节及库存 | 已核；未运行测试 |
| 完整原始 baseline | tests-only `0b5ef63b`，`map_continue_` | 20项11绿/9红；4 SSA+4普通native unit四格成功，3边界绿；single 6 SSA与3native拒绝，native未到object/run |
| 首次生产 | `c36aac5b`，同20项 | 18绿/2红；原owned provider组合仍拒source，不冒称全通过 |
| source独立对照 | §2 `e2f9f1e9` / `eb38a7c4`，`map_continue_source_probe_` | 基线0绿3红；本片1绿2红；具名source既有缺口确认 |
| 修订范围最终红绿 | 待冻结新Borrow fixture、原owned拒绝回归 | 未运行 |
| single 直接 SSA/native | 待普通源码选择 | 未运行 |
| 结构与回归 | 待定向选择 | 未运行 |
| 静态与独立审阅 | fmt/clippy/docs/尺寸/diff | 未运行 |
| 发布闭环 | PR/CI/归档/merge | 未执行 |
