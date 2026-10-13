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
| 阻塞项 | 独立实现与验收进行中；Map continue 组合待 PR90 actual-main 后整合 |
| 影响范围 | 非 Resource owned Array/List/MutableList source 的有界条件 continue |
| 语言语义变更 | 否 |

## 1. 输入与独立范围

从真实 main `614a7a7f5ff77fecb337c3b6d18118e687d73c52`、tree
`7835aa0a4ee0e065cf51203f8cfb18b2a9b3703c` 创建 `fix/spec-0312`；
已核本地文件/refs 与远端分支 0312 未占用。
基线没有 [PR90 / SPEC-0311](https://github.com/Halckon/Koven/pull/90)，不能称 Map continue 已合入。
核心普通源码不含 Map，先独立红测、最小实现；组合只有真实合入后正常整合并重新验收。

## 2. 有界证明

保留旧 constant presence 与 Resource 路径/global implies。旧证明失败后，
仅精确 Sequential source root、非 Resource、本地 owning binding 或 own 参数获得新证明资格。
名字 kind、类型、source identity、同轮 ownership 与 iteration schema 均由已验证事实取得；
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
| 基线身份/合同 | §1 actual main | 已核；合同提交时无生产变更 |
| 普通 source 红测 | owned_source_continue_ | 待运行 |
| 最小证明、结构与 native | 同源矩阵 | 待实施/运行 |
| unit 对照/边界回归 | ordinary/constant 与受影响调用方 | 待运行 |
| 静态门禁/独立审阅/CI | 精确最终输入 | 待运行 |
