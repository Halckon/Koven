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

counter 专属 CI-only 矩阵另列六个精确入口：guard 比较方向、递减常量、success
Alias 与旧 counter 回边替换、checked 失败边极性、独立 counter 选中清单缺失及
真实最终 verifier 失败撤销两份 pending 收据。fixture 使用两真实 counter caller，
普通基线验证完整 publication/replay；所有变异默认 ignored 且 GITHUB_ACTIONS 门控，
登记既有 borrowing runner，沿用 macOS/Linux 矩阵。本地只编译/列举，动态拒绝结论
须等远端实际执行，不能把旧 Boolean 矩阵或普通正例算作新故障验收。

## 单一 tail break 的独立前端事实

稳定 Boolean while 的 body 可另选精确 `if/else; break`，不与 counter/update 混合。
真正处理该 break 时，两臂 Frame 必须均已 closed 并恢复入口资源；body 完成必须只有
break 流，没有 fallthrough/continue。scope cleanup 后再次核该退出状态，不将 break
当作正常回边。新 `no_carry_break_branches()` 与其它三域隔离。

Unit 最终 planner 独立跟随真实 while/body，先核 break 入口资源，再在实际
ControlTransfer 清理、body scope 清理和目标 LoopExit 清理后核同一目标与空本轮
loan/稳定外部资源。唯有整段完成 witness 与两臂 End 均匹配才发布退出能力；collector
不依赖可关闭的观察器。公共事实给出 break statement/expression、body、目标 while、
两种 cleanup point 和固定 Break exit-kind。native 与后端独立 receipt 仍另行验收。

## 单一无条件 tail break 的独立后端

`no_carry_break_branches()` 单独进入必需/选中清单；条件/分支还是稳定 Boolean
参数，不能与 counter 或旧 backedge 能力混用。真实 jump 在清理前后比较资源，
只在实际 Goto 写入后记录 break 源块与目标；while-body Diverged 必须由该记录解释。
新域保留独立 header（无回边时不将其折入预头），最终收据核两臂 End、arm→merge、
实际 break→exit、false→exit、来源/前驱和 ABI 运输。exit 后继做可达性检查，禁止
隐藏重入当前选中区域，而不误将合法前缀 CFG 限成预头无前驱。

普通 SSA 验证 Own/Borrow 标志、参数重排、rooted/memory 与真实 observer 对照，
且退出后同函数继续消费 source。native 真正运行 flag=false/true × 两臂，分别零轮/
一轮，核消费次数、同函数 after-loop source 可读、调用者返回后可读和最终 own 消费。
这不是多轮证明；额外/条件跳转、嵌套、多根及跨出口 loan 仍不授权。专属故障动态
验收尚待后续 CI，普通正例和静态复审不能替代。

单一 tail-break 专属 CI-only 清单新增七入口：实际 break 错出口、两臂各自 End
遗漏、fresh loan 携入出口、exit 直接/经一或两个新 CFG 块隐藏重入、真实 break
capture 丢失、选中收据在 finish 丢失、最终 verifier 失败撤销两收据。清理缺失
反例具体删除实际 BorrowEnd，不把空清理代码跳过说成资源泄漏。所有反例默认
ignored、GITHUB_ACTIONS 门控；已有双宿主 borrowing runner 精确发现，不重复造
workflow。本地仅编译列举，动态拒绝与恢复结果须以远端执行为准。

## 两源候选语法探索（未授权执行）

`loop_no_carry/shape.rs` 的生产 `selection` 固定实例化同源分支，保持原检查。
仅测试子模块调用两源候选分支，解析真实 AST 并筛选不同 root、有限 counter、
无 exit/conditional 的分臂骨架；不构造 session、End、公开 fact 或 receipt。
其 fixture 回调使用唯一名字，仅服务语法探索，不证明 Shared 权限、callee 效果、
符号解析身份或完整候选合同。真实 Single 检查仍拒绝两源（含 L0164），同源邻居
仍成功。候选分支是当前调用点隔离，不是编译期禁止未来生产调用的机制。
真实 Unit 的 rooted/memory 输入及 observation 开关验收也保持两源拒绝：没有
borrow binding、同源 branch 或 counter fact，validated ownership factory 不发布。
同源 counter 邻居仍发布独立 counter 域身份，旧 branch owns 查询不授予权限。
这不是两源真实 Acquire/End 事件或新 receipt 的实现证据。

后续 Unit-only 测试入口可暂选候选并运行真实 core Frame；仅在两个临时 arm fact
各通过真实最终 End 的唯一 receipt 匹配后现场断言，不保存 payload，只计到达。
原同源 pair 构造守卫不变，公共分析仍诊断失败并撤销观察与借用产物；无新 pair、
validated ownership 或 backend receipt 发布。默认关闭的线程局部 RAII 恢复嵌套/
异常状态，Single selector 不经过此入口；非测试编译直接使用旧 selector。
