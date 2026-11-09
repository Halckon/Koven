# Unit While owner 边界观察

> **性质**：实现事实 · **状态**：current · **读取时机**：核对真实 While body 运输、完成边与不变输入 owner · **唯一真源**：drop planner 的 owner transport loops 收集器、loop 只读产物与 V1 validator

基础协议见 [Unit owner transport](unit-owner-transport.md)，受限条件值见 [Choice](unit-choice-transport.md)。
独立不变输入能力只证明实际边界及可选 Continue 清理点的同一 owner；不授予 header/exit phi、完整外层运输、
实例级释放或 Phase 4/native 能力。

## 空入边闭合 body 的准入与真实执行边界

仅顶层普通、非泛型 Unit 函数，整个函数体仅一个 While。直线完成使用一个无 mode marker
的精确 Boolean 参数；末尾条件跳转使用两个这样的参数，While 与 If 必须读取不同真实
symbol。条件只能参数的 Name/Group 链，每层精确 Boolean 且非 constant-use，原条件分析
必须同时保留 true/false 两条边。函数、循环和 body 均保留 source-qualified 身份。

body 至少一个精确 String/MoveOnly 的局部 val/var 声明，可接唯一末尾裸 Continue、Break
或显式双臂 If。If 每臂只能空 ControlBody 或一个裸 Continue/Break，不接受臂内声明、
其它正常尾值、嵌套、Return、Group 跳转或不可达后缀。initializer 可为无插值 String、
已有本地 symbol 的 Name、Group 或真实 checked 完整根 Replace 的嵌套组合。每层精确 String、
非 constant-use；替换根必须已在 body 声明，descriptor、操作数顺序和 root 形态都核对，
根可变性仍由原 checked primitive 生产者证明。成员身份用有序集合，initializer 检查通过后才加入本地 symbol 集合。

旧 While 遍历只执行一次。条件实际完成后，在无外层循环/分支及完整空 ValueState 时启用私有
body sink；保存旧 observer、Choice 事务和 branch depth，body 使用全新 catalog identity。
真实 String 构造、take、binding、pending、replace commit 和 named cleanup 继续由原 hook
登记；不从 drop facts 或 span 反推 owner，不重新遍历 body，也不改变实际 cleanup。

实际 `statement(body)` 的 Result 先保存，恢复全部旧 sink 后再传播错误。正常 fallthrough
必须实际返回 true 并完成 scope cleanup；全跳转必须实际返回 false。末尾 If 的实际返回值
须与独立分臂见证一致，不能由合并后状态猜测；有空臂须真实完成 body scope 后才发布回边。
两条路径都要求完整 body state 为空，再用原无 Choice V1 投影封闭 body。该 validator
要求每个真实 definition 最终为 Dropped；旧 `is_consistent` 或单独检查 values 为空均不够。

跳转只在 fresh body sink、精确末尾 expression、单一最近 loop boundary 和 body scope
匹配时获得观察准入。原 jump 依次结束 pending loan、清理更深层 owner、完成 iteration
清理后，才记录实际 Continue/Break kind、expression 和 body identity；预扫 AST 不是执行
见证，单独 false 加空状态也不足以发布。恢复 sink 时清除 active 标记并消费一次性见证。
局部 String 已按原 ASAP 在跳转前退休，不把 Drop 改写到 ControlTransfer；普通 V1 validator
没有放宽，对真实跳转点 Drop 仍拒绝，此片仅新增清理后的控制边。

末尾 If 在真实 condition 完成、完整空状态且原 V1 已封闭整个 owner 前缀后，才开启分臂
观察。每臂在原遍历入口记录精确 control、branch 和 body，实际 jump cleanup 后保存独立
pending 见证；跳转臂实际返回后核 tail、false、空状态、同 Arc、kind、scope 和最近 loop，
再消费一次见证。空臂走原早退路径，只恢复 scope，不调用 drop_scope；在原 drop_branch_exit
之后、状态 push/merge 之前核 None tail、true、无 owner result、无 pending jump 及完整空
边界和同一身份。两臂都完成、事件为空且实际 continuation 数与空臂数精确相等，才只抑制
私有 body sink 的空 If 事件。普通 observer 的 If 事件和 V1 拒绝规则不变；错误恢复时清空
split、active arm、pending 与 completed 标记，不从丢弃后的合并状态倒推各臂成功。

空边界穷尽 owner、pending、loan、borrow transports、provider、closure、replacement 及
this/lambda 槽；新增 ValueState 字段须显式作边界决定。真实 LoopExit 清理后再次检查空状态。

## 独立循环产物

`loop_observation_v1()` 独立于旧观察和 `value_observation_v1()`。旧 callable 仍在 While
statement 原子 Unsupported，定义和事件为空；私有 body catalog 不增添 callable 观察项。
外层 analysis identity 绑定 loop，独立 body identity 再绑定整份 transport，防止同源码二次
分析的完整 catalog 被替换进来。只在 successful 且无 deferred 的最终产物保留新能力。

直线完成的三条结构边包含条件 true 进入实际 body、body 实际完成边及条件 false 到 While
LoopExit。全跳转 If 保持六边：进入 body、then IfBranch 与其实际跳转、else IfBranch 与其
实际跳转、条件 false 退出。有空臂时为七边：对应后继为 IfFallthrough(arm body, control)，
只表示实际空臂正常返回 If，不是 drop 点；两臂后另加唯一共享 body Backedge，它仅在实际
body scope 完成后封存。两个 IfBranch 保存真实 control、condition、序号及 arm body。
完成边由私有 completion 独立核对：Backedge 从 body scope cleanup 回条件；ContinueBackedge
从实际 ControlTransfer(jump) 回条件；BreakExit 从实际 ControlTransfer(jump) 到该循环
LoopExit。公开 edges 返回只读 slice，私有 completion 重建并核对三、六或七边的完整顺序。
不能用 Break 冒充条件 false 或回边。它们是控制关系，不是已发生的有限运行
轨迹；body 无外来 owner、未完成交付或存活后继值。

body definition token 是静态 producer 身份，每次激活都要求新实例，不能将不同轮实例混为
一个 token。此产物只给出闭合模板，不发布 runtime instance handle 或 loop-carried phi。
条件是不可变参数且 body 不修改它：false 为零轮；true 在 fallthrough/Continue 路径可持续
循环，在末尾 Break 路径经一轮退出。末尾 If 的另一个参数也不变，同次调用选择同一臂，
并非每轮任意换臂。有限前缀不是终止证明，不发布动态运行计数。

## 单个不变输入 owner 的非空边界

独立 `invariant_loop_observation_v1()` 不放宽旧空入边 getter；两者互斥。仅一个无 mode marker
Boolean 参数，函数恰三句：精确 String 的 `val held = "outer"`、直线闭合 While、精确 String
的 `val retired = held`。前缀只收无插值 literal，后缀只收同 symbol 的 Name；两者不接受
Group/var/nullable。body 复用上述 String 本地构造、move、Group、root Replace，可在末尾
追加唯一裸 Continue；仍拒绝 Break 或任何 If。locals 从空集合开始，不能引用、替换或移动 held；后缀未来使用使其按既有
liveness 跨过 body 和 LoopExit，未修改 Guide 的 ASAP 或 loop 固定点。

普通 While rejection 清空前，核真实 prefix collector Complete、唯一 String producer 与
恰好 Result/Binding 两事件，token、initializer、symbol 都对应实际单个 OwnedValue。
只从该 collector 保留 producer catalog，不从 AST、drop 或 symbol 反造 owner。随后照常
拒绝普通观察，旧定义和事件为空。新能力不恢复 suffix collector，也不提供后缀事件或
最终消费证明；输入 catalog 自身只验证 producer 身份，不能代替闭合运输。

entry 在实际 condition 正常完成后读取；正常 backedge 在实际 body scope cleanup 后读取，
且必须返回 Ok(true) 并通过原闭合 V1；exit 在实际 drop_loop_exit 后读取原 condition state。
Continue 则在原 pending loan、deeper-scope drop 和 iteration cleanup 完成后，先过精确
expression/kind、单一最近 loop、scope、fresh sink Arc 与一次性完成门，再独立读取一份
post-jump binding。它成功后才登记 completed；body 返回 Ok(false) 后仍独立读取 backedge，
并核 expected/completed 和 collected Arc 相同、原 body V1 封闭。该提前返回不执行正常
body drop_scope，不能称为正常 scope fallthrough。原空入边 jump 仍要求完整空边界。

`continue_boundary()` 返回真实 Continue expression 与其清理后 binding；普通 fallthrough
必须为 None，Continue 必须有同 source/jump/body Arc 的快照且与其它边界 binding 相等。
Continue 只有 ContinueBackedge(ControlTransfer(jump), condition)，不额外生成普通 Backedge，
也不将已有 ASAP Drop 搬到 ControlTransfer。所有快照分别读取实际状态，不复制 entry 冒充
其它边。ValueState 共17字段，除唯一 OwnedValue 的 values 外16槽为空；owner/token、
symbol、origin、declaration、scope depth 全部相等。
body 使用不同 Arc 的新 catalog，只拥有 body-local producers；外层 owner 不进入该模板。
公开 binding 可查询真实 symbol/token，三条结构边仍核实际 While/condition/body 身份。
最终成功且无 deferred 才保留能力，错误分析整体清除。它是非空入边的身份前置，不是 phi、
不同路径 owner 的 join、运行实例追踪、外层运输闭合或终止证明。

## 保留边界

除上述单个不变 String 输入外，普通调用、外来/变更 owner、参数模式、capture、其它分支、Return、非末尾跳转、嵌套或顺序
多个循环、For/Loop、其它表达式语句、未消费 temporary、nullable、普通赋值及其它条件不准入。
不删除 Single 循环见证，不扩 Choice，不改变 Guide 的 ASAP 或 loop liveness 合同。
真实源码、双 Unit 入口、开关完整 Analysis 对照及质量门禁范围见 SPEC-0341 §144、§146、§148、§150、§152、§154；
手工损坏产物只作为正式 CI 守卫，不能替代生产接线证明。
