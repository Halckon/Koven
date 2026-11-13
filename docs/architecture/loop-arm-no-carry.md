# 受限循环双臂 no-carry

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改循环内分支借用的闭合事实时 · **唯一真源**：两 checker 的 `no_carry.rs`、Unit `loop_no_carry.rs` 与 codegen `arm_loop_receipt`

旧直线合同见[循环 no-carry](loop-no-carry.md)。本片实施合同见 [SPEC-0341](../specs/active/0341-shared-lowering-core.md)。

## 受限双臂前端闭合事实

另一个选择器只允许 while 的唯一 body 表达式为稳定 Boolean 参数控制的 if/else。
两臂分别为同源 String 参数的一次显式借用与非空同步消费序列。每臂持有独立 Frame，
第一臂实际 acquire/return/End/scope close 完整成功后才可检查第二臂；两臂各自恢复
同一入口资源状态，再核正常回边。任一诊断或未完成臂撤销整对事实。

获选 Unit-valued arm 走逐 statement End 路径，不把最后 consumer 当作借用值合流；
最终 drop planner 再独立核两项 End 身份。`no_carry_branches()` 与旧 `no_carry_loops()`
分开，两个 arm 不冒充直线 loop；成对事实记录精确 if、then/else scope、binding、
initializer、consumer 和 End。同一 publication gate 在全局失败时清空两个领域。

## 独立后端闭合核验

branch admission 消费整对已验证事实，逐臂匹配真实 binding/source/End，不改旧直线
API。builder 在真实 arm lowering 与 drop 后比较入口资源快照；缺失或重复 pair、
任一未闭合 arm 均失败。独立 FE 必需清单与 layout/final 选中身份防止遗漏计划。

`arm_loop_receipt` 核最终实际 SSA 中两个独立 BorrowCall、顺序 consumer 和 BorrowEnd，
绑定确切 source 参数、callee、指令来源与块身份。条件核真实 Borrow Read 或 Own value
及最终 Conditional 实体；CFG 检查 then/else 方向、合流、回边、前驱和 ABI 参数运输。
新 loan 只能由对应 arm 定义和消费，不得出现在其他指令、块参数或 terminator。
原有 whole-unit SSA verifier 仍是返回产物前的独立门禁。

普通 SSA 覆盖参数重排、Own/Borrow Boolean 与 rooted/memory 输入；native 零轮
覆盖两种 choose 参数并确认返回后 source 可消费，不把零轮运行当作跨轮执行证明。
跨臂借用、借用 phi、条件副作用、跳转、嵌套、多根及跨轮携带仍拒绝；不改变上述
直线后端合同，也不以新事实代替旧 SSA verifier 或独立见证。


独立收据保存 caller、参数身份和 emit 证据，可在不持有 builder/RefinedBody 时重放
最终 Function。测试发布观察接入 whole-unit guard：最终 verifier 前只 pending，
成功才发布，失败清空本轮 pending。CI-only 五项精确入口复用 borrowing fault runner
与 macOS/Linux 矩阵；默认 ignored、GITHUB_ACTIONS 门控，本地只编译列举。

## 旧稳定 Boolean 域的正轮 native 限制

旧稳定 Boolean selector 要求 while 条件为稳定 Boolean 值参数的 Name；body 必须只有一个
if/else，每臂仅借用声明和 String Borrow 同步 consumer。赋值、局部可变条件、
计数器条件、break/return 均不属于此形状。`flag=false` 是零轮；`flag=true` 时，
只要 consumer 正常返回，下一轮条件仍为 true，因此不存在正轮后正常有限返回路径。
这不是链接器或 ABI 故障，也不能用进程超时、强制中止或手改 SSA 伪装正轮验收。

普通边界回归 `ordinary_loop_arm_finite_exit_candidates_remain_outside_selected_shape`
核可变局部 flag、递减计数器、合流后 break 均拒绝且不发布双臂事实；沿用 Single/Unit、
rooted/memory、真实 observer on/off 对照。该旧域仍不放宽；另行批准的有界 counter 域见下节。

## 有界 counter 的独立前端事实

counter 选择器另核紧邻的 `var remaining=rounds`、`remaining>0` 与唯一
`remaining=remaining-1`，并对白名单之外的 counter 引用 fail-closed。两臂仍先各自
沿实际生命周期关闭；实际 update 入口核完整闭合及入口资源恢复，执行返回后再核
资源状态不变，回边发布要求 update 确已完成。不用语法匹配替代动态 checker 路径。
`no_carry_counter_branches()` 独立保存 counter/init/condition/update 身份及双臂事实，
旧 `no_carry_branches()` 不列出该 pair；全局失败同时清空新旧领域。此为前端资源
闭合，不等于后端条件/递减/checked-abort/回边 receipt 或真实 native 0/1/2 轮验收。

## 有界 counter 的后端与普通 native 证据

后端通过新 owned counter fact 的精确 condition/decrement 身份开放入口，不开放一般
binary 或旧事实域。独立收据核真实初始化 Alias、Borrow rounds Read 或 Own value、
header 的零常量和 GreaterThan、合流后 CheckedArithmetic Subtract、成功回边的 Alias、
对应失败 Abort 和完整前驱/参数运输；每臂借用链及 fresh loan 不逃逸仍独立核验。
实际 checked subtract 保留 success/Abort 双边，不把静态有界性当作省略错误边的理由。

普通 SSA 覆盖 Borrow/Own rounds 与 rooted/memory，产物通过 verifier 且独立收据可
重放。普通 native 真正运行 0/1/2 轮与 true/false 两臂，验证不同长度 consumer 输出，
每次返回后 source 可读且最终可 own 消费。零轮与正轮分别有输出标记，不用超时、
强制退出或修改 SSA 代替有限正轮。跳转、嵌套、多根及跨轮 loan 携带仍未纳入。
