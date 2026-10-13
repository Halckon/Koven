# SPEC-0310: single Map owner 条件 break 运输

> **性质**：变更合同 · **状态**：done · **读取时机**：实施或验收 single Map 条件 break 时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0310` |
| 所属 Phase | Phase 3 事实消费、Phase 4 single SSA/native |
| 语言规范 | [Guide10](../../guide/10-ownership-borrowing-drop.md)、[Guide12](../../guide/12-collections-destructuring.md) |
| 批准依据 | 2026-10-13 用户“合并后继续推进”；PR88 合并后从 actual main 激活 |
| 前置 Spec | [SPEC-0300](../../archive/specs/0300-single-map-loop-owner-carry.md) |
| 前置 ADR | [ADR-0006](../../adr/accepted/0006-typed-ssa-block-parameters.md) |
| 阻塞项 | 有界实现已验收；归档 head CI、合并与 actual-main CI 尚待后续交付 |
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
unit 独立对照实测：live 在 basic/constant 已成功，dead 两入口仍拒绝；不扩 unit 实现。

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
| ordinary 红测 | tests-only `112f7637`；`cargo test --locked --offline -p lang-codegen --lib map_break_ -- --nocapture` | single live/dead frontend 成功，UnsupportedNode/Span 23..28；组合 unit test 失败只说明至少一例拒绝，后续拆四格 |
| 完整 baseline | tests-only `652a7ba6`；生产各文件核与 actual main 原字节相同，同选择 | 13 项中 7 绿/6 红；红为 single 3 SSA 与 3 native，均 frontend 成功后 UnsupportedNode/UnsupportedSource；不冒称红项执行 object/native |
| unit 四格 | basic/constant × live/dead，各独立 test；基线与生产分别执行 | live 两格真实 SSA/LLVM 成功；dead 两格 UnsupportedNode 且真实 loop Span；另两项 live 普通 native 基线即成功，不计新 unit 行为 |
| 最小实现与直接验收 | 生产代码输入 `a5ed1a72e7cac5c6a32bea8d33e1214d35a48da9`，同 `map_break_` | 13/13 通过，0 ignored，1458 filtered；8 源码 SSA/边界项、5 普通 native 项；含 0/首轮/多轮/耗尽、live/dead、3 providers、两类 payload、own seed/双同型 Map |
| 独立结构 oracle | `lower_frontend_map_break_structure_tests.rs`，由两份直接源码测试调用 | 从真实构造 result 找 header slot，核实际 MapPut result 在 back/break 运输、live 两入边到 merge/final drop；dead 两互斥块各 drop且 merge 不复活；element/source End、临时 source Drop、dead Map Drop 有序 |
| 保留边界/Map SSA | `ssa::lower_frontend_tests::map_result_tests` | 59 通过；含新测试与旧耗尽/seed/RangeView/Abort/Resource，非全量；continue/return 拆分保留 |
| Map 普通 native | `native::unit_tests::map_loop_owner_tests`、`map_key_return_tests` | 12/18 通过；loop 选择包含 5 个新 native，与直接 13 项重叠，不相加宣称独立总数 |
| nullable cleanup 普通 native | `native::unit_tests::owned_null_cleanup_tests`、`temporary_null_cleanup_tests` | 11/8 通过；无计数或插桩运行 |
| provider/snapshot 边界 | `native_tests::sequential_for_tests::<test>` 顶层 11 项逐 exact；两个 source snapshot/closure exact | 11 项逐名各 1 通过；snapshot/closure 各 1 通过 |
| 静态检查 | fmt；`cargo clippy --locked --offline -p lang-codegen --all-targets -- -D warnings`；docs、尺寸、diff | 全通过；663 Markdown；1070 手写 Rust 文件、38 历史超限，无新增增长豁免 |
| 独立审阅/双宿主 CI | 候选 `1d41d60ee141d13d37ebe400ca98222d8f66d178`；生产同 `a5ed1a72` | 独立静态终审无阻断；核 facts/carrier、结构 oracle、四格基线及账本；审阅者未运行测试。PR/远端与 macOS 本地未运行 |

命令前缀为 `cargo test --locked --offline -p lang-codegen --lib`，上述模块名为实际过滤器；
provider 只逐项 `--exact` 执行顶层普通测试，不运行注入子模块或聚合 driver。
验收宿主 Linux x86_64，Rust 1.96.0、LLVM 21.1.8，独占既有共享 target。

生产 proof 显式检查 Break/Fallthrough 的同 Control selector 互补条件；静态 Entry + 唯一
Header 自回边按路径携带当前 owner，构成有界归纳，不修改 Resource/source 的 prove_graph。
每个 Header 只注册一个 carrier；dead 两 DropFact 分别记录 block/value，live 则在实际合流后
核完整入边并锚定最后 drop lineage。所有入口一致，未改 frontend/verifier/LLVM。

原始日志与恢复 patch/hash 保留仓库外。本地实现草稿曾有一次变量遮蔽导致编译失败，以及
旧 prove_graph 无法证明条件 backedge 导致两项仍拒绝；修正后重跑。两次 unit 期望过宽
（先假定两格成功，后误假定两格拒绝）的日志保留；四格基线复验确定现状，不把期望修改当修复。
初次 provider exact 误用文件名模块路径，11 次均零命中；不计通过，按静读确认的
`native_tests::sequential_for_tests` 重跑后逐项各 1 通过。静态核验聚合脚本与 actual main
原字节相同，103 个唯一 frontend integration targets；新 module 由完整 codegen 目标消费，
没有通过运行聚合脚本或 mock/stub 来验证接线。

未执行本地故障注入、校准、sanitizer、bad IR、mock/stub aggregate、计数插桩、full discover
或全量 frontend/workspace 测试。远端 CI、发布、归档、合并仍待授权流程完成。


## 5. PR89 双宿主实现验收与有界归档

§1–4 保留原冻结合同、本地账本与当时待验收状态，包括 7 绿/6 红基线、unit 四格澄清、
编译/证明失败及 provider 零命中纠正；它们不是本次重跑。本节追加实际远端完成证据。
2026-10-13，[PR89](https://github.com/Halckon/Koven/pull/89) 的
[run38407747517](https://github.com/Halckon/Koven/actions/runs/38407747517) attempt 1
以 pull_request 事件完成 success，`reuse=false`，不把证据复用视为物理执行。

| 输入 | 精确身份 |
|---|---|
| 实现 remote head | `61c50ec118913b506297ecaa26727b17dd46b6e9` |
| 实际 base | `162e8e5dd75436b80571b5a30637923d2a0cc24a` |
| 实现 tree | `a746603b41461abf4a195e2e3f89d326db9bb8dd` |
| synthetic merge | `030ddf7945c2ef2ead223013be6c37021fb39ba3`，有序 parents 为 base、head，tree 同实现 head |
| 本地对应终点 | `48ebc65849affc786be93c1644c8ffe489b731da`；六个 remote commits 的真实 parent 链及各 tree 映射均核验 |

本次 15 个实体 job 物理成功；唯一 Tree-sitter job 因 `editors=false` 合法跳过，不计执行。
精确 head 的 Actions checks 与 attempt-1 jobs 逐 ID/名称/结论匹配，
[CI Passed](https://github.com/Halckon/Koven/actions/runs/38407747517/job/115339558851) 与
[Code CI Evidence](https://github.com/Halckon/Koven/actions/runs/38407747517/job/115339558667)
均独立成功。未用其他 run 的同 head check 或笼统 workflow 状态替代逐项核验。

- [Linux Targeted](https://github.com/Halckon/Koven/actions/runs/38407747517/job/115333124959)：完整 codegen 1471 passed、0 failed、0 ignored。
- [macOS Targeted](https://github.com/Halckon/Koven/actions/runs/38407747517/job/115333124632)：完整 codegen 1470 passed、0 failed、1 原有 LLDB ignored；原因是 debugserver task-port 权限，未扩大 ignore。
- 每宿主 13 个新增测试逐名 `ok`：8 个真实源码 SSA/边界项及 5 个普通 native 项。
  unit basic/constant 的 live 成功原已存在，dead 两入口仍以 UnsupportedNode 拒绝；绿色负例
  不表示该组合实现。single 的 live/dead、当前 owner 与退出清理依旧按 §2 的有界合同。
- fmt、docs、尺寸、直接依赖与双宿主 check/clippy/test 均成功；OS 条件不执行的 steps
  仍是合法跳过，不外推为该宿主动态检测通过。普通 stdout 不证明动态精确 free 次数。
- Linux preview artifact `11820631930`、macOS artifact `11821160404`：两宿主 producer
  上传、GitHub artifact API 与 consumer 下载的 ID/digest 一致，独立安装均通过。
  [Linux consumer](https://github.com/Halckon/Koven/actions/runs/38407747517/job/115338389210) / [macOS consumer](https://github.com/Halckon/Koven/actions/runs/38407747517/job/115339110552)
  消费上述精确实现 head 的产物；未声称另行下载归档独立复算 digest。

最终实现收据 SHA256 为 `5859b1709fdef5af6b083cc5cf62412ec8971d75922a2aab8e764a8fa8b89552`；
raw logs、job/check/artifact JSON 与恢复材料留仓外，只在本页保留短账本和稳定链接。
本次归档续在真实 remote head 后，仅迁移 Spec、更新生命周期/索引/库存/生成 DAG 与机械链接。
生产、测试、fixture、Guide、脚本原字节保持；云上没有 Cargo、聚合 driver 或动态验收运行。
§2 的所有未覆盖组合与 unit dead 边界保留，不因 done 状态扩大支持。
归档 head CI、PR 合并与随后 actual-main CI 尚待后续核验，不能由实现验收提前声称完成。
