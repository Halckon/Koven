# SPEC-0311: single Map owner 条件 continue 运输

> **性质**：变更合同 · **状态**：done · **读取时机**：实施或验收 single Map 条件 continue 时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P4-0311` |
| 所属 Phase | Phase 3 事实消费、Phase 4 single SSA/native |
| 语言规范 | [Guide10](../../guide/10-ownership-borrowing-drop.md)、[Guide12](../../guide/12-collections-destructuring.md) |
| 批准依据 | 2026-10-13 用户持续里程碑开发、合并后继续推进；PR89 实际合并后激活 |
| 前置 Spec | [SPEC-0310](0310-single-map-conditional-break.md) |
| 前置 ADR | [ADR-0006](../../adr/accepted/0006-typed-ssa-block-parameters.md) |
| 阻塞项 | 有界实现与 PR90 实现 CI 已验收；归档 head CI、merge 与 actual-main CI 待验证 |
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
| 修订后补充基线 | tests-only `86144f96`，生产与 actual main 原字节相同，独立空 target，`map_continue_` | 24项13绿/11红；这是 c36 实施后的补充对照，不冒充原始先行红测 |
| 当前隔离验收 | `c2be34de`，另一独立空 target，同24项 | 24/24通过，0 ignored；10个新single正例（7 SSA/LLVM+3普通native）、8个既有unit控制、6个边界回归；不是24个新增能力 |
| source拒绝位置 | 原完整owned fixture，基线先拒Map cache；本片再拒source keys | 精确source边界oracle由红变绿仅是拒绝位置推进，不是source能力修复；其余两个无Map/无后用source拒绝基线即绿 |
| 独立结构oracle | 两基本source、双Map与local/派生loan专例 | 逐边最新MapPut/MapRemove结果、slot/target、source/length/cursor、element/local次序通过；local专例仅SSA/verifier/LLVM |
| Map相邻回归 | 当前隔离target，`ssa::lower_frontend_tests::map_result_tests`、`native::unit_tests::map_loop_owner_tests`、`native::unit_tests::map_key_return_tests` | 75/19/18通过；含直接24中的重复项，不相加成独立总数；保留0300/0310 |
| nullable回归 | `null_cleanup_tests` | 22通过：19普通native+3普通source SSA；没有注入或改写IR |
| provider/snapshot | 顶层11个`native_tests::sequential_for_tests::<name>`逐exact；两个`ssa::lower_frontend_tests::rejects_*`逐exact | 每项实际1通过；只选择普通source，不运行provider注入子模块 |
| 静态质量 | fmt；codegen all-targets clippy `-D warnings`；docs/尺寸/diff | 全部通过；docs 664；1074手写Rust文件、38历史超限，无新增豁免 |
| 独立审阅 | c36生产、c2范围修订及3103276c终稿 | 代码/证据无阻断；按最后建议在SSA总览明确无Map也有owned source缺口。审阅者未运行测试 |
| PR90 实现 CI | remote `7a5daea7`、run `38420920418` attempt 1 | success；16个checks/jobs精确对应，15物理成功、1 editors=false 跳过；reuse=false |
| 双宿主直接验收 | Linux/macOS `map_continue_` 逐名24项 | 每宿主24通过、0 ignored；10新增正例/8既有unit/6边界，层级与本地相同 |
| codegen 与 preview | 既有 CI 全lib及三条安装链 | Linux1494通过；macOS1493通过+1既有LLDB权限ignored；三条preview安装链通过，不改写本地云安全测试范围 |
| 发布闭环 | PR90 docs归档 | 有界归档；归档head CI、merge、actual-main CI仍待验证 |


命令前缀为 `cargo test --locked --offline -p lang-codegen --lib`，表中均是实际过滤器。
精确provider加`-- --exact --nocapture`；其余加`-- --nocapture`。clippy仅编译all-targets，
没有执行其中注入测试。本地Linux x86_64、Rust1.96.0、LLVM21.1.8；外部900秒OS timeout
保护普通运行，既有native helper自身没有timeout，不修改其公共行为。

隔离证据：baseline/current各自全新target，串行实际rustc编译54.92/54.11秒，
`CARGO_INCREMENTAL=0`，无代码/IR插桩。两份输入清单的Rust/fixture/Cargo差异恰是4个生产
文件，所有测试输入相同；逐run前后HEAD、状态、全源码SHA清单相等。测试binary SHA256
分别为`9f2554f04c5e7b48231629ae436e5367e4459297e7eb700963b0318f4cb259ae`与
`a20b8bc4fde9d33f7472311cf3ab20b52edc3a8b8f017eb38d4b0e94bff03e96`。
原日志、完整输入hash、实际rustc manifest/cwd、绝对target、dep-info与恢复包保留仓外。

失败与纠正历史：第一轮11项观察时有新测试写入尚未冻结，不绑定该轮为精确输入证据；
正式0b5的20项在任何production前已提交且实跑。补充对照切换worktree后，共享target曾
错误复用旧baseline binary；该疑似复用run不计当前验收。曾仅touch当前lib.rs触发重编译，
前后字节相同，但最终仍不用该run作收据，改用上述两份全新隔离target。没有cargo clean、
删缓存或修改已冻结输入来取得绿；原始失败日志均保留。文档场景页新增条目曾超过200行入口上限，删除冗余空行后重跑结构门禁；没有改脚本或提高上限。

最终本地没有运行full discover、无filter全lib、全量workspace/frontend、aggregate driver、
fault/calibration/sanitizer/badIR/mock/stub或计数插桩；stdout不证明动态精确free次数。
[PR90](https://github.com/Halckon/Koven/pull/90) 的[实现 CI](https://github.com/Halckon/Koven/actions/runs/38420920418)已验收；
实现远端 head `7a5daea7aec9486b474223d527244dfbbab6a359`，tree
`cc2f83095fc872e189c702151eb7e8e6cdd2f493`；本地原9提交与远端9提交逐树一致，
真实远端 parent 链与原 Git 字节 SHA 已核。最终 CI 收据 SHA256
`e4944160e0aab00f7d6835a3c9e5beb5d9314e662ee114d10646cf80ee8bac49`，raw 仓外保留。
此次只关闭有界合同；归档 head CI、merge、actual-main CI 尚待验证，未宣称分支保护已核。
owned source 下一修复项保持§2明确优先顺序。
