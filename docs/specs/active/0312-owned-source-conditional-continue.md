# SPEC-0312: owned Sequential source 条件 continue 证明

> **性质**：变更合同 · **状态**：in-progress · **读取时机**：实施或验收 owned source 条件 continue 时 · **唯一真源**：本 Spec

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P4-0312` |
| 所属 Phase | Phase 3 事实消费、Phase 4 single SSA/native |
| 语言规范 | [Guide12](../../guide/12-collections-destructuring.md) |
| 批准依据 | 2026-10-13 用户要求审核/CI 等待时继续实施独立工作 |
| 前置 Spec | [SPEC-0300](../../archive/specs/0300-single-map-loop-owner-carry.md)、[SPEC-0310](../../archive/specs/0310-single-map-conditional-break.md) |
| 前置 ADR | [ADR-0006](../../adr/accepted/0006-typed-ssa-block-parameters.md) |
| 阻塞项 | 本地定向门禁完成；最终独立审阅与 PR CI/归档/合并尚待交付 |
| 影响范围 | 非 Resource owned Array/List/MutableList source 的有界条件 continue |
| 语言语义变更 | 否 |

## 1. 输入与独立范围

从真实 main `614a7a7f5ff77fecb337c3b6d18118e687d73c52`、tree
`7835aa0a4ee0e065cf51203f8cfb18b2a9b3703c` 创建 `fix/spec-0312`；
已核本地文件/refs 与远端分支 0312 未占用。
基线没有 [PR90 / SPEC-0311](https://github.com/Halckon/Koven/pull/90)，不能称 Map continue 已合入。
核心普通源码不含 Map，先独立红测、最小实现；组合只有真实合入后正常整合并重新验收。
随后正常 merge actual-main `8e0ce49daea9c17960669aea6d467342508021f6`（tree
`68e6fc8b44930c709a9dac5faf5da6b09790c25a`），整合提交 `81f62711` 的真实父链保留；
从核验 Git bundle 导入对象，四份生成 DAG 冲突用联合库存重生成，没有 cherry-pick/fake parent。

## 2. 有界证明

保留旧 constant presence 与 Resource 路径/global implies。旧证明失败后，
仅精确 Sequential source root、非 Resource、本地 owning binding 或 own 参数获得新证明资格。
名字 kind、类型、source identity、同轮 ownership 与 iteration schema 均由已验证事实取得；
本地 seed 还以 names marker 精确找到唯一同轮 Item::Variable initializer，与 Entry ExpressionId
按既有透明 Group 归一后相等；不按类型或 span 包含猜。pre-loop replacement/alias 不获新资格。
曾观察到 replacement 自然满足原宽 seed 门槛，`2ee1b02c` 收紧后原普通源恢复精确拒绝；
field/this/projection、Borrow/Inout、RangeView 不获得此资格。

仅同 source symbol 的 Header/Exit，无 capture/snapshot/replacement/跨 loop seed。
一个 Always Entry、一个 conditional Continue、一个真实 Fallthrough、一个 Exhaustion。
逐边匹配 statement、boundary、point、condition、唯一 target binding、StaticConditions
及唯一真实 value；Entry 的 path/availability/value 全 Always，唯一 Expression/owned Parameter seed。

Header presence 必须是精确 selector 的 Choice(Never, Always)，其 statement/boundary/slot、
IterationPresence/branch_count 都匹配 phi。由真实 Entry 建立基础，回边仅以该 Header
selector=1 为归纳假设，同时 cofactor 路径和所需条件，其余控制 selector 保留并递归证明蕴含。
所有回边通过后才证明 live Exit exhaustion，不用 Exit 自证写入；dead source 可在
EndSource 后 drop Header，允许 Exit absent。DropFact owner/condition 必须精确对应已证 phi。
不更改原 condition、不重新求值控制、不新增运行时 presence ABI。

source shared loan 在整个 body/backedge 禁止 move/drop/替换或 exclusive mutation 的事实
来自 frontend；证明不能替代实际 SSA owner/loan/length/cursor 运输与 verifier。

单 owner 单 loop 是证明域，不是 AST 禁止嵌套。2026-10-13 普通源码差分后获准澄清：
外层控制每次新建的 Expression seed/Always Entry 可独立满足本证明；不授予跨 loop owner。
原 baseline 四例均拒绝；candidate 只有 fresh inner 独立实例新增接受，outer 包含 inner、
同 source nested、同 source 连续 loop 仍拒绝。先前笼统 nested 非目标措辞由此有界细化，
不改 Guide，也不证明通用 nested。动态实例 native 与精确 owner/loan 结构分别验收。

## 3. 验收与非目标

普通源码先红后绿，Array/List/MutableList、0/1/3 元素、continue never/all/mixed、
source live/dead、local/own seed、temporary/Borrow 对照、var source 与 loop 后权限恢复。
两项 body-local Resource 逆序清理；String/MoveOnly 与 discard/借用解构代表组合。
精确 SSA oracle 追踪构造 owner、透明 block parameter、source loan、length 一次快照、
cursor 初始零/回边加一、element cleanup 与 exhaustion/drop 次序。
single verified LLVM 与原版 object/link/run；unit ordinary/constant 独立四格，不推定 unit 修复。
源码非法 move/变异只核原 frontend 诊断。Resource、非 source owner、field/Inout/RangeView、
多 continue/break/return/跨 loop phi seed 等保持旧能力，不将拒绝改为宽泛接受。
Map 组合后验收；不复制未合入 0311 代码，不造父提交。
Guide、frontend、SSA model/verifier、LLVM 不变；如需扩域先停止报告。

云上不运行 fault/calibration/sanitizer/bad IR/mock/stub aggregate、计数插桩或 full discover。
原日志/JSON 放仓库外，每次运行冻结提交并记录身份，独立 target；stdout 不证明 free count。
最终还需定向调用方回归、fmt/clippy/docs/尺寸、独立审阅、PR CI/归档闭环。

## 4. 唯一验收账本

| 验收项 | 输入与选择 | 实际结果 |
|---|---|---|
| 基线身份/合同 | `1362f25d`，§1 actual main | 库存、commit/tree核验；独立合同先于红测/生产 |
| 最小原红 | tests-only `4d72ca18`，`owned_source_continue_` | 2项Array live/dead frontend全过，single精确UnsupportedNode keys23..27；循环首失败不冒称已运行List/MutableList |
| 完整红/两入口基线 | tests-only `1452d937`，同选择，生产仍614原字节 | 6项三容器×live/dead分别红；unit basic/constant×live/dead4项原SSA/LLVM成功，无新增unit能力 |
| 独立最小实现 | `2b797ffe`，同10项 | 10/10绿；后续`2ee1b02c`明确收紧local声明seed身份；Resource/global implies原算法保持 |
| 完整最终source | `0ade10cd51fff9ae6191b982242665ff054e3f7b`，`owned_source_continue_` | 32/32通过、0 ignored；真实frontend/SSA/verifier/原LLVM，含源边界及以下native代表集合 |
| native轮数/路径 | 上述选择内provider矩阵 | 3容器×0/1/3×never/all/mixed×live/dead54格真实object/link/run；factory每格一次输出；var source；stdout不是free count |
| 载荷/清理/能力恢复 | 同选择内模式、解构、body cleanup、Resource旧路 | String own参数/Borrow/temporary、MoveOnly value-class借用解构、discard均绿；两body Resource逆序；own交付、MutableList add/remove、source临时保留；Resource无条件旧路词法清理保持 |
| 精确SSA oracle | `lower_frontend_owned_source_structure_tests.rs`，6格及decoy/fresh-inner调用 | frontend seed ExpressionId定位真实Construct；仅透明block-parameter转送跟踪owner/sourceLoan；initial loan的length快照恰1、零cursor、两回边+1、elementEnd/sourceEnd与唯一源Drop；live post-use关联实际owner；同型decoy不混淆 |
| nested/seed边界 | baseline `7bdc6352` 对 candidate `6bc89679`；最终32项 | 原4例均拒；仅独立fresh inner转绿；不同outer payload的inner live/dead native及结构通过。其余跨loop/连续source稳定拒绝；replacement原意外接受已按2ee1收紧，Group/未替换var/own parameter保留，alias拒绝 |
| 原能力对照 | baseline `b3bc1519`、RangeView baseline `711e6c4e`；最终32项 | Resource conditional、multiple continue、break/return、Inout、field、replacement及无Map conditional RangeView逐例比对；field保持MissingFact，其余精确UnsupportedNode。非法body move/替换/增删仅frontend L0135；borrowed元素own交付L0133 |
| 0311真实整合/组合 | `81f62711` merge actual-main8e0ce49d；最终32项与Map source回归 | 原providers-cleanup.ko字节不变并恢复single native正例；source live/dead×双同型Map live/dead4格全绿；原0311 source probes保留owner/Header/Exit检查并转正，不复制未合并生产 |
| 独立unit native | 最终32项中basic/constant×live/dead各test，另Array替换test | 4格真实object/link/run；循环后Array替换两视图通过。single Array替换无循环baseline仍Unsupported keys[0]48..55，不纳入本片修复 |
| 共享source/Resource回归 | 生产同2ee1；`7815f471`，`ssa::lower_frontend_tests::map_result_tests` / `resource_deinit_tests` | 75/15通过；Map组含原0311/0310/0300普通source、RangeView与carrier边界 |
| 共享普通native回归 | `7815f471`，`native::unit_tests::{map_loop_owner_tests,map_key_return_tests,owned_null_cleanup_tests,temporary_null_cleanup_tests}`逐模块 | 分别19/18/11/8通过；普通未改写LLVM/object/link/run，无计数插桩 |
| provider/旧source/capture | `4b57202f`，顶层provider11个逐exact、owned_source_tests前3普通目标、两条capture拒绝逐exact | 16个目标各命中1并通过；old-owned/Borrow矩阵72格、factory/借用字段保持；未运行含计数的resource/source聚合目标 |
| CLI公开入口 | `4b57202f`，`cargo build --locked --offline -p lang-cli --bin kovenc`；`kovenc run fresh-inner.ko` / `run --project project.toml --entry main` | 文件与project目录source-roots读取同fixture原字节，均exit0/stdout精确/stderr空；与unit API四格独立 |
| 静态门禁 | `0ade10cd` clippy all-targets -D warnings；fmt、docs、sizes、diff；Python size policy指定模块 | clippy通过；fmt/docs/sizes/diff通过；size policy47项通过。仅tests入口模块注册3行显式例外2004→2007，已独立逐行审阅并获准；生产1520上限不提高 |
| 独立终审/远端 | 最终docs冻结提交 | 待终审；本地未运行macOS；PR、远端双宿主CI、归档及合并均未由此宣称完成 |

Cargo test命令共同前缀为 `cargo test --locked --offline -p lang-codegen --lib`；
provider/source旧路只逐exact执行静审过的普通目标，未跑full discover或注入driver。
日志逐次冻结commit；最终门禁另保留before/after source哈希、CWD和仓外target身份并核相等。
baseline独立worktree/target，不交叉复用二进制身份。原始日志、详细facts和JSON保留在仓外
交付包，由PR/CI artifact承接；不把本地临时路径当持久链接。

### 保留失败与未覆盖范围

- 最初Array单项红、3容器红、nested与replacement差分、原源码非法声明分隔符L0047都保留。
- 结构oracle首次误假设body/header同槽，后按真实edge槽和source LoanId修正；inner首次误选
  外层ElementPlace也修为精确source loan。没有改生产或伪造IR迎合测试。
- 载荷组合曾把前一source带入后继loop而拒；改为独立owner生命周期正例，同时跨loop拒绝保留。
  原裸花括号是未调用closure capture，不将其误称普通block shadow成功。
- Map首次无标签矩阵失败不能证明每个先前格失败；分格确认dead无后put可通过，真正的
  `first.contains && second.contains`外部owner条件清理仍精确拒second，保留独立普通源码负例。
- Resource旧路最初stdout期望误用ASAP；Guide10规定Resource词法清理，改为done后逆序
  deinit；生产Resource路径未改。测试编译类型错误与clippy嵌套format错误均保留并修正。
- 不证明通用nested/多continue/break/return、跨loop owner、动态capture或Resource新条件能力；
  不提供分配计数、free次数、sanitizer或通用内存安全结论。
