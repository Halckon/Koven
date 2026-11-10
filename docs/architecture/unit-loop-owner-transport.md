# Unit While owner 边界观察

> **性质**：实现事实 · **状态**：current · **读取时机**：核对真实 While body 运输、完成边与不变输入 owner · **唯一真源**：drop planner 的 owner transport loops 收集器、loop 只读产物与 V1 validator

基础协议见 [Unit owner transport](unit-owner-transport.md)，受限条件值见 [Choice](unit-choice-transport.md)。
不变输入的基础能力只证明实际边界及 Continue/Break 清理点的同一 owner；不授予 header/exit phi、完整外层运输、
实例级释放或 Phase 4/native 能力。正常、裸 Continue 或裸 Break 输入的独立[后缀消费](unit-invariant-loop-suffix.md)另封闭实际退出路径。

## 空入边闭合 body 的准入与真实执行边界

仅顶层普通、非泛型 Unit 函数，整个函数体仅一个 While。直线完成使用一个无 mode marker
的精确 Boolean 参数；末尾条件跳转使用两个这样的参数，While 与 If 必须读取不同真实
symbol。条件只能参数的 Name、Group 与前缀逻辑取反链，每层精确 Boolean 且非 constant-use，
原条件分析必须同时保留 true/false 两条边；不接 calls、常量、比较或复合逻辑。
准入返回原参数 symbol，因此不同极性或分组不能绕过两条件的参数互异限制。
条件与边始终保留完整 source-qualified expression，不能改成最内 Name 或按取反奇偶交换 then/else。

末尾 If 的 Not 另经只读活动门：仅已准入条件链的实际 Not 节点，精确 body sink Arc、scope、
最近 loop、无 Choice/分支深度，且尚无 split/arm/pending jump/completed，才不拒绝观察。
既有 expression 遍历仍真实执行条件；不跳过求值，不为普通 Prefix 或循环其它位置授予支持。
此门不生成控制边、owner 或快照，完整完成与状态封印仍须在实际遍历后成立。

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
条件仅由不可变参数经 Group/Not 构成，body 不修改参数；完整条件为 false 时零轮，
为 true 时在 fallthrough/Continue 路径可持续循环，在末尾 Break 路径经一轮退出。
末尾 If 的另一个参数也不变，同次调用选择同一臂，
并非每轮任意换臂。有限前缀不是终止证明，不发布动态运行计数。

## 独立非空输入能力

单个 immutable String 输入的真实 entry、正常/Continue 回边、Break 出口、双终端臂和
双正常臂及正常/跳转混合臂合流，见[不变输入 owner 循环边界](unit-invariant-loop-owner.md)。各能力独立
封印且互斥，不放宽本页空入边合同，也不授予一般 phi 或完整外层运输。

## 保留边界

除上述单个不变 String 输入外，普通调用、外来/变更 owner、参数模式、capture、其它分支、Return、非末尾跳转、嵌套或顺序
多个循环、For/Loop、其它表达式语句、未消费 temporary、nullable、普通赋值及其它条件不准入。
不删除 Single 循环见证，不扩 Choice，不改变 Guide 的 ASAP 或 loop liveness 合同。
真实源码、双 Unit 入口、开关完整 Analysis 对照及质量门禁范围见 SPEC-0341 §144、§146、§148、§150、§152、§154、§156、§158、§160、§162、§164；
手工损坏产物只作为正式 CI 守卫，不能替代生产接线证明。
