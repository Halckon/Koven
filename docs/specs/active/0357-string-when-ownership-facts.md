# SPEC-0357: String subject when 的 Unit 清理事实

> **性质**：有界前端变更合同 · **状态**：in-progress · **读取时机**：实现或消费 String when 清理事实时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P3-0357` |
| 所属 Phase | Phase 3；Phase 4 仅未消费事实的拒绝边界 |
| 语言规范 | [Guide v0.46](../../guide/README.md) |
| 批准依据 | 2026-10-26 用户批准在 #183 合并后独立修复 String subject 存活期与条件临时清理事实 |
| 前置 Spec | [0356](0356-refine-when-family.md) 的 String subject 消费缺口 |
| 关联 ADR | [0032](../../adr/accepted/0032-ownership-refinement-layer.md) |
| 阻塞项 | 设计已条件性审结；生产实现与 exact PR CI 尚未完成 |
| 影响范围 | Unit ownership liveness/drop planner、公开事实及最小 consumer 拒绝、真实源码事实测试 |
| 语言语义变更 | 否 |

## 1. Goal

为非 nullable String subject 的等值 `when` 发布可验证、source-qualified 的清理事实：
subject 只求值一次并跨越每个实际比较；每个已求值的 owned 条件临时值恰在比较完成后清理一次；
未求值后缀无清理义务。先交付前端前置，不声称 p41 的 SSA/native 已通过。

## 2. 基线与范围

基线为已合并 #183 的 main `8b656340b20e14bcc9328fe34a80f61dd6193de6`。
独立分支 `feature/spec-0357-when-string-facts`；不修改或合并
`feature/spec-0356-when-subject`（审阅输入 `0a254fd4b7368c2eb817760195dbccead2129cc3`）。
该分支 0356 §2 已记录独立 frontend 前置；main 的旧正文不作相反证明。

现有 Unit liveness 的 `Expression::When` 仅回溯显式表达式，缺少 subject 的隐式比较使用；
`drop_planner/control.rs` 条件走 Read，却未登记 String 临时 owner。
`AfterExpression(candidate)` 在 refine 完成表达式时已消费，早于隐式比较；
`AfterBinaryOperands` 仅属于 binary；单独的 `WhenAlternativeMatch` 不覆盖失败比较。

首个支持核限 p41 形状：非 nullable String subject，所有非 else 条件均为无插值 String
literal（允许 Group），不含 contains/type-test；subject 为普通 named String（含 Borrow，允许
Group）或已有 owner 描述的 String temporary。候选 literal 无副作用，因此不能改变 subject。
包含逗号 alternatives、early/late/no match、else、when 后复用和 body 的既有 abrupt/Abort。
不扩展 nullable、enum、MoveOnly 消费式匹配或其他 when 语义；不恢复旧 checker。

候选调用、插值、控制表达式（含条件内 Return/Break/Continue/Abort）、候选 named place、
field/index subject、borrow-result subject 暂不准入，必须明确 deferred，不发布完整计划。
这些不是非法语言；它们缺少本片已冻结的稳定性/owner/路径证明。不得将此有限核称为全部
String when 前置完成。后继如扩大候选形状，须先证明 pending owner 的真实提前退出路径。

## 3. 事实合同（首核已编写，动态验收待执行）

所有点的 `control` 均为真实 when 的 `UnitExpressionId`，携带 `SourceUnitId`；
`entry` / `alternative` 为该 control 下的源码零基下标，不以 Span、全局下标或另轮分析替代。

| 点 | 精确位置 | 义务 |
|---|---|---|
| 新 `WhenComparisonCompleted { control, entry, alternative }` | candidate 正常求值，隐式 String 等值比较已经产出 Boolean，分流之前 | 本次候选 temporary；同一路径恰执行一次，不在 Abort/transfer 路径执行 |
| 既有 `WhenAlternativeMatch { control, entry, alternative }` | 对应比较成功，进入共享 entry body 前 | 本边不再使用的 subject/candidate named owner；subject temporary 在 body 前结束 |
| 新 `WhenAlternativeNoMatch { control, entry, alternative }` | 对应比较失败，求值下一候选或进入 else/落空前 | 本边不再使用的 named owner；仅最后比较失败后结束 subject temporary |

一个 temporary candidate 不可同时出现在 Completed 与 Match/NoMatch。
subject temporary 不可在每次 Completed 都释放；它必须活到第一个匹配或最后一次未匹配。
两个不同源码候选即使文本相同仍是不同 owner；同一 named subject/candidate identity 不重复清理。
Borrow binding 不取得 owner，不能生成 Named drop。值来源 Span 与 target 身份继续同轮、同 source。

### 3.1 存活期与求值次序

- 为 String when 单独计算每个 alternative 的 matched/unmatched continuation live set。
  matched 包含选中 body 的真实 live-in 和 when 后使用；unmatched 包含后续候选、后续 body
  及 when 后使用。每个比较前的 live set 包含 subject 的隐式读取，不把 body 后 live-out 当 body live-in。
- subject 正常求值一次。named subject 在比较期间保留；temporary subject 注册既有 pending owner。
  candidate 按源码顺序仅在前次未匹配边求值；比较完成后先清理 candidate，再分流清理本边 subject。
- 零比较（空 when/only-else）不生成虚构 entry/alternative。subject 不再用于比较，temporary
  可用既有 `AfterExpression(subject)` 在 body 前清理；named subject 若 body/后继仍读则保留。
- existing `BranchExit` 仍只表示 body 正常结束，不伪装 body 入口或比较失败边。

### 3.2 提前退出与安全边界

候选内 Return/Break/Continue/Abort 本核全部 deferred，不作已支持承诺。未来扩大候选形状时，
必须证明已求值 subject temporary 的 pending-owner ControlTransfer 清理只作用于真正离开的
scope/loop frame；candidate 未完成不生成 Completed，Abort 不展开，未求值后缀没有 owner。
本核 body 的提前退出发生在 candidate/subject temporary 已按入边清理之后，沿用既有合同。

准入在规划任何 subject/candidate 前整体执行：递归解包 Group 后，所有候选必须是
`Expression::String` 且 parts 全部不是 Interpolation；subject 的同轮 typed 类型必须精确为
Builtin String。subject 为非constant且有真实绑定的普通 Name，或 category=Temporary、
`is_move_only_temporary` 为真且非 borrow-result call 的非field/index表达式；不从 Span猜身份。
不合格的整个 String when 记录 `StringWhenComparisonLifetime` deferred，并停止该 source 的
完整 drop plan 发布；基础与 constant validated gate 均以既有 deferred 检查拒绝。
这会使部分先前 frontend accepted 的合法 String when 不再取得 validated capability，是显式
能力收窄，不是新增语言非法诊断，也不声称零行为变化。root 已批准本片保守 fail-closed边界。

### 3.3 验证与消费者

在 Unit drop planner 返回 producer 结果前，从真实 control/候选身份和路径需求独立建立
required cleanup inventory（含Named与Temporary），核实际新点的 source、control、entry/alternative、target 与次数；
不把 required 与 actual 从同一个 push 同步填充。缺失/额外/错身份时返回 ownership error，
不得产生 validated product。不新增通用验证层或公共 plan 抽象。公开事实字段继续不可伪造。
unsupported whole when 走既有基础/constant `validate` 的 `!deferred.is_empty()` 拒绝门。

消费者具体接点：`refine/subset.rs::drops_supported` 对两新点为 false；
`unit_lower/control.rs::lower_when` 在 nullable/其他分流前，发现本 control 的新点即
`UnsupportedNode`；零比较没有新点时仍由原 String type gate 拒绝。
`refine/build.rs` 的 source-unit exhaustive match 仅补身份处理，不发射 drop。
因此 subset false 不能落到漏消费的旧路径。0356 后继负责真正的比较后 CFG 消费、SSA/LLVM
与 p41 运行验收；本片不实现它。

## 4. 非目标

不做 String codegen、运行释放计数、nullable/enum 模式、其他 when 扩展、通用 CFG/NLL、
subject snapshot ABI、旧 checker 修复、大范围模块抽取、无关路线图清理或自动发布/合并。

## 5. 验收计划

- [ ] 设计冻结：稳定 subject 证明/拒绝，零比较、early/late/no match、same owner、Borrow、abrupt。
- [ ] 真实公开 Unit producer：单文件与双 source 正逆输入，精确 source/control/entry/alternative。
- [ ] 每条正常路径独立 oracle：candidate 比较后一次；subject 保留至使用完；unevaluated none。
- [ ] subject/body/when 后复用、逗号短路、Borrow 不释放；same-named candidate 是 deferred 负控。
- [ ] body Return/Break/Continue/Abort 正控；candidate 发散/transfer 为 deferred 负控，不声明动态运行证明。
- [ ] 未消费新点明确拒绝，consumer 负控不因 fallback 伪成功。
- [ ] 精确 Draft PR head 的双宿主 CI；现有 binary String、nullable when、无 subject when 回归。
- [ ] 静态 diff、定向官方 rustfmt、物理行/字节与身份检查；需要增长例外则先审阅。

### 5.1 首核路径表

| 输入路径 | candidate | subject temporary | named subject |
|---|---|---|---|
| 第一个命中 | 当前 Completed 一次，后缀无事实消费 | 当前 Match 一次，body 前 | 由该 body/后继 live-in 决定 |
| 后面命中 | 每个实际候选 Completed 一次 | 仅命中 Match 一次 | 失败边跨越下个隐式比较保留 |
| 全部失败，有 else | 每个 Completed 一次 | 最后 NoMatch 一次，else 前 | else/后继仍使用则保留 |
| 全部失败，无 else | 每个 Completed 一次 | 最后 NoMatch 一次 | 后继仍使用则保留 |
| 空/only-else | 无 Completed | AfterExpression(subject) 一次 | body/后继仍使用则保留 |
| subject 求值提前退出/Abort | 不求值任何候选 | 未完成 subject 无 owner；内部 owner 按原合同 | 原退出合同 |
| body Return/Break/Continue/Abort | 已完成的 candidate 不再次清理 | 已在 Match/最终NoMatch 清理，不再次清理 | 原 body/退出合同 |
| candidate 内 transfer/Abort 或其他副作用 | 首核 deferred | 不宣称有已验证 pending 清理 | 不宣称稳定快照 |

## 6. 实施顺序与未决项

1. 本 checkpoint 交独立 reviewer；在稳定 subject/验证/consumer 拒绝边界冻结前不改生产语义。
2. 先新增真实公开 Unit 源码测试，再实现最小 facts/liveness；本地不运行测试，首红/绿证据仅 PR CI。
3. 静态审阅后取得发布批准，才提交 Draft PR；记录 exact head/merge/base 和实际 CI 命中数。
4. 按批准范围完成交接。0356 未消费和 p41 未运行必须继续标记，不写全功能完成。

设计已获独立条件性 GO：本节所列准入、producer 验证及两 consumer 拒绝按上述冻结合同实施。
新增 Rust 尺寸增长在冻结 diff 时逐文件复审；发布仍须另获批准。

## 7. CI 清单与交接

精确 Draft PR head 发布后执行既有 stage 的 `basic_unit_ownership` 完整 target（本片
`string_when::` 16 项），及既有 codegen lib 中
`unit_lower_when_tests::string_when_comparison_facts_are_explicitly_rejected_until_consumed`。
双宿主现有完整 codegen、CLI、所选 frontend gates 继续执行；不得把本片新增事实测试当成
p41 native 释放次数或 SSA 成功证明。现有 `multifile_constant_ownership`、
`multifile_ownership_checking`、既有 String 二元事实所在套件
只按实际 CI 接线与逐名命中记录，不凭过滤器推定通过。

0356 交接：消费 Completed 时比较 Boolean 已计算且 operands仍存活；随后按对应 Match/NoMatch
清理subject。实现后才移除本片拒绝门，新增三条边的精确SSA/运行证据。候选非literal的deferred
不能仅删gate：须先补稳定subject与pending abrupt的真实producer合同。

## 8. 验证记录

| 项目 | 结果 |
|---|---|
| main / 分支编号 / open PR 新鲜读取 | 2026-10-26：main 为上述 SHA，远端 035x 最高0356，无0357；open PR 为162/107/64 |
| 生产实现与测试源码 | 已编写literal首核、16项公开Unit事实测试、1项真实producer inventory破坏负控与1项consumer拒绝测试；尚未动态验证 |
| Cargo/build/check/clippy/test/native/list | 均未执行；动态证据限发布后 exact Draft PR CI |
| 静态检查 | 官方定向rustfmt、docs（788文件）、size、diff通过；不证明编译或行为正确 |
| 发布/Ready/合并 | 未执行；无此阶段授权 |

### 首轮 PR CI 与有界修正

实现提交 `f9ec68a79ef6d4d2d290e67a46153ee0dd45ba74` 的
[PR184 首轮 CI40377598881](https://github.com/Halckon/Koven/actions/runs/40377598881)
在双宿主因 consumer 测试的 `expect_err` 要求成功类型 `Program: Debug` 而报 E0277；
这不是生命周期断言已运行后失败。修正为显式 match，保留意外成功 panic 与精确 UnsupportedNode。
同时复审确认原完整性inventory仅覆盖Temporary，现从subject初始owner、每边live-in与现有外层
保护独立导出Named义务（含零比较），emit算法不变；真实producer事实的缺失/错target负控、
显式borrow val来源/backing owner与Borrow参数公开Shared身份均补充测试。修正尚待exact PR CI，
不把静态审阅或首轮失败改写为动态通过。

新增内部负控 exact：
`ownership_checking::compilation_unit::dataflow::drop_planner::string_when::tests::real_named_inventory_rejects_missing_or_wrong_target_match_and_miss`，
由既有 frontend `--lib` 门禁承接；无workflow变化。
