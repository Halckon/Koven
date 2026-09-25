# 所有权事实与数据流

> **性质**：当前实现事实 · **状态**：current · **读取时机**：修改 move、loan、capture、drop 或 validated ownership gate 时 · **唯一真源**：`lang-frontend` 所有权代码与测试

## 分析入口与产物

`ownership_checking::check_ownership` 提供单文件入口；
`check_compilation_unit_ownership` 消费 validated typed unit 并返回 source-qualified
`CompilationUnitOwnership`。两条路径共享所有权语义，但使用各自产物的 ID，不能混用。

公开 unit 入口委托私有 `compilation_unit/analysis.rs` driver；contracts/capture/dataflow 与错误
清理顺序不变。独立 `check_compilation_unit_constant_ownership` 消费 `ConstEnabledTypedUnit`，
返回 `CompilationUnitConstantOwnership`；其 `validate` 仅在完整物化与短路事实存在时产生
`ConstEnabledOwnedUnit`。`is_compatible_with` 核对 typed 分析的 Arc 身份，等值重新分析不兼容。

私有 driver 已消费 unit typed constant use，跳过常量 initializer 与读取路径的 namespace/
declaration flow；String 二元操作数按每次读取的 temporary 生成析构事实。此路径的 recovery
保留专用入口来源标记（即使没有常量声明），基础 `validate` 拒绝发布 owned capability。
`UnitConstantMaterializationPlan` 保留 typed use descriptor 和 inline/String
temporary 类别，只由主 traversal 登记；按 source-qualified expression 去重、排序，错误或
deferred 时整体不可用。初始化器依赖、return/Abort 后读取不登记，动态分支保留原 expression
位置；双 source 的 11 类型互相读取与输入顺序稳定性由 driver 测试覆盖。专用产物提供稳定
列表与按 expression 查询，字段不可外部构造；其 recovery 视图不能转换为基础 validated
owned。表达式/控制流清理矩阵的实际证据记录于 SPEC-0226；此为 Phase 3 capability，
unit constant native 仍未接通。

unit 常量的 Group 只透传值：loan、Value delivery 和 temporary drop 的 owner 统一指向
内部实际常量读取，drop 来源 Span 同步归一；原实参 identity、调用和清理位置保留。普通
literal/variable/其他 temporary 不通过该常量归一路径。

unit String 插值将已完成求值的 MoveOnly temporary 保存到与调用共用的 pending 状态，
按插值 expression 隔离。正常完成在 `AfterExpression` 逆序释放输入；返回的外层 String 与
后续仍 live 的 named owner 不在此清理。return/break/continue 复用 pending 退出清理，
Abort 不展开；常量 Group 的 drop 仍归一到实际读取。

String 二元表达式在右操作数求值期间保留左 temporary 的 pending 清理义务，以及左 named
owner 的借用保护；正常完成在 `AfterBinaryOperands` 逆序清理，named 是否仍 live 以整个
binary 的后继为准。嵌套完成只解除自身保护；不继续的左/右操作数停止后续 drop traversal，
return/break/continue 清理已求值前缀，Abort 不展开。

专用常量入口显式启用私有短路控制模式（空常量 unit 也启用），基础入口保持旧门禁。
`short_circuit_plan` 供主 traversal、liveness、drop 共用：左侧正常完成后，依据 Boolean literal、
typed 常量值或 Group 确定 RHS Always/Never/Conditional；不另行求值常量 initializer。
动态路径合并 RHS 与 skip 后继，RHS 退出不抹去 skip；BranchExit 编号仍为 0=true、1=false。
source-qualified `UnitShortCircuitPlan` 仅由实际访问收集，与物化计划一起受错误/deferred 原子门禁
保护，专用 validate 同时要求两者完整。专用 recovery/owned 提供只读计划列表，owned 还可按
expression 查询；`UnitShortCircuitRhs` 与 RHS 分支编号保留执行决定，字段不可外部构造或修改。
专用 unit native 已消费该短路计划；基础 native 入口保持原能力边界。

专用模式由主遍历记录实际访问的 source-qualified lambda 集合；drop 前的 lambda liveness
预扫描、lambda body 清理及公开 closure/capture 入口均按此集合筛选，避免不可达 lambda
发布没有物化 owner 的 drop。初次 liveness 仍保留完整预分析，基础模式保持原行为。

两个 unit 入口均重新核对 source inputs、names、types 和 `TypeEnvironment` 的 owner identity。
基础入口只有无所有权诊断、无阻塞 deferred 且不来自常量专用路径时，`validate` 才产生
`ValidatedCompilationUnitOwnership`，供既有 codegen 使用。

## Place、binding 与 loan

单文件 `for` checker 已消费 typed iteration descriptor，source 与 element 使用独立于 call 的
loan owner；显式 `this.field` 与裸字段共用字段 owner，source loan 覆盖两种写法；字段写入与 Inout 交付受当前 receiver mode 限制，lambda 内隐式 member call 也记录 `this` capture 并按共享能力检查；unit 的 If/When 返回分支尾值和 Elvis 直接交付操作数检查 borrowed closure 逃逸；binding/component 登记为 non-owning，Copyable 字段 Value delivery 按 Read 处理；回边和退出边检查仍持有本轮 binding capture 的 closure，形成时保存被捕获的旧 closure 来源，非法存活报 L0137 且不发布清理计划。
drop planner 通过 iteration frame、shared capture 和具名 pending callee 保持 owner，并保存
临时 source 退出义务及完整结果/索引 backing container 的 owner 定义身份；索引求值期间提前退出与 provider 退出共用同一 backing 定义 ID；liveness 将 provider source root 纳入 backedge。break/return/耗尽各自
清理，continue 保留 source；nested jump 按 loop depth 选 frame，scope 按符号快照逆序清理。

公开 `IterationOwnershipPlan` 按 statement 查询，保存 typed descriptor、shared source target、
具名 shared bindings 及独立 exit plans。退出动作按实际规划顺序包含 owner drop、call/capture
loan end、binding/element end、FinishProvider 和 source loan end；nested return 的各关联计划
提供同一个 point 的等值完整序列，只执行一次，不再重复消费平面 drop 列表。break 在自身
ControlTransfer 点按循环后继 liveness 释放 dead named owner；`LoopExit(for)` 仅表示耗尽。
`closure_flow` 在 MoveOnly binding/已知 lambda 有限域求 header/exit 固定点；稳定 header 再发布
内层摘要。空来源保留 opaque owner；有限 lambda 来源图按 lambda identity 去重并随计划发布；phi 来源布局和入边显式带该图节点索引；先在有限图上计算每个 binding 的可达捕获槽，且在递归图上也可登记有限的 phi owner、capture 布局与 binding 可用性，每个 phi 直接记录有限图根节点，内部 header/exit 根 phi 留给 body 状态读取当次 owner；只有无环时才展开树形 origin 与已知 capture 的 source owner 关系槽，并发布每个可达图节点的原始 capture 位置到 phi 槽的映射，owned 捕获的已知内层环境仍按有限来源树分配独立槽。入口、单次 body 的 fallthrough/continue/break 记录实际 owner 入边，耗尽从 header 槽转发到 exit；body 已从 header phi、后继已从 exit phi 建立符号状态，离开词法 scope 的 captured source 另存延寿义务。多个环境借住同一实际 source 时，各 phi source 槽在 capture loan 结束后发布按动态实例查询最后借用者的选择，再以此保护 drop；外层路径条件保护 phi 派生状态和耗尽入边，未进入循环时不读取其 selector。静态 lambda 来源出现递归捕获环，或循环 phi 需要运输紧邻 callable 环境中的内层 closure 实例而当前入边无法关联时，checker 分别发布 deferred 并原子清空本次 drop plan；递归路径内部已登记的有限槽也不会作为可执行事实发布；其内部入边只传根值，capture_slots_to_clear 为空，不把后代槽展平到 phi 写集。同一静态 lambda 节点若经不同捕获路径出现无法证明互斥的环境候选，也原子 deferred；相同捕获位置的候选不触发，header→exit 独立 presence 可能使原本互斥的菱形暂时延期。Phase 4 尚未执行该选择、循环动态运输和清理。
同节点不同捕获路径的并存环境在内部入边保留根 owner、可用性及各捕获路径的来源 presence；根和后代的来源读地址仍指向当次根实例、捕获路径和原始槽位置，但省略目标 phi 捕获槽清单及展平来源写入。仅供实例寻址的来源槽不会由 `transport_value()` 暴露为可直接复制的静态 phi 值。内部 shared 双实例回放已按形成快照、Entry/回边/出口的选择位与实际捕获边求值早期 loan end 和返回点清理；公开动态运输与实例级释放仍未交付，相关计划整体 deferred。递归图中未展开来源的具名闭包 phi 根（含独立闭包）在内部以 `ReleaseClosureInstances { statement, root }` 取代普通 `Drop`；根条件与析构点保留，逐实例释放仍缺，deferred 阻止发布，Phase 4 显式拒绝该动作。此为 SPEC-0211 的实施中事实：错误或 deferred 阻止公开计划，已检查 provider 未被 drop
traversal 完整覆盖时集合整体不发布。lambda body 已独立规划 liveness、参数 owner 和局部
drop；循环栈与 scope 不继承创建点，未使用的 owned 参数在 `LambdaEntry` 清理，checker 和
drop planner 都按 Consume 交付尾值。返回值、Inout 根/字段/元素存储、Value 实参与构造参数
在实际 If/When 交付分支中检查 borrowed closure 逃逸，分支内声明也参与检查。

iteration source 以 place 求值后显式建立 Shared loan；temporary-container element source
沿 Index/Group 链定位实际 backing owner，source target 与退出析构指向外层 temporary。
Index 求值前登记已求值 owner 的 pending 义务，提前 return 清理，Abort 不展开；正常完成
后由调用者或 provider 接管。named source 的连续 Index 保存完整索引路径，按逐层 alias
与前缀关系保护 owner；末级 `element()` 查询不替代完整 `elements()` 身份。控制结果先赋给
局部别名后，checker 按 AST index 排序合并成功分支的 closure origins 与潜在 capture loan，
按后继 liveness 释放已死 binding；控制结果和每个 pending call 独立持有其所需 origins。
callee/较早实参的 capture 保持到对应调用完成或退出，不因后续实参的控制表达式提前结束。
capture 输入关联 source 值或紧邻环境的静态捕获槽 ID，按 closure 定义、capture source 与已检查捕获的首次引用序号注册；创建条件不变，流动条件随快照复制，同源多版本共享槽布局但保留各自条件和值；已形成环境可由 `closure_capture_edges` 查询目标槽与形成输入，同源条件版本共槽、紧邻环境的来源槽与目标槽分离；`CreateClosureOwner` 后发布逐条 `SaveClosureCapture`，指定按条件从形成前来源写入目标槽；已形成环境的 captured drop fact 标出接收槽，快照保留原布局定义，递归析构保留槽身份；已发布非递归 phi 的候选槽按根 phi owner、lambda 与 source 在有限可达图中注册，同源候选与嵌套来源分别定位，入边 source 同步标出来源槽与接收槽，binding 入边列出目标 phi 必须清空的完整捕获槽；消费时仍须检查当次实例是否持值；owned move 捕获的内层环境来源按 capture source 关联并随外层环境保留，lambda body 可恢复 leaf closure 来源，释放时递归结束其 loan，source 析构前递归检查 shared capture；同点兄弟 loan 逐条写入 last-capture 选择，候选 source drop 在全部 loan 后发布；跨循环 phi 的非递归内层来源已形成显式入边关系，动态实例运输仍待 Phase 3。
source 版本随 Name/`!!`/Elvis/pending Value 运输；Elvis 的 Index 临时 backing 独立清理。when 独立选择；窄版 Phase 4 closure bridge 对非恒真或紧邻环境来源的 owned move capture 提前拒绝；循环 phi 与 `for` native 未闭合。phi source 的 `input.value()` 保留候选或转发来源，根层 `transport_value()` 给出入边复制应读取的候选当次环境 owner 和已形成捕获槽；前端单候选 Entry/Exhaustion 回放验证紧邻外层槽 move 后从内层实例及 header 布局槽读取，且复制后消费旧 root 句柄；条件双来源两轮回放亦按 Create/Save 动作保存当次实例，从形成槽或 header 槽读取，再并行提交并清空未选目标槽。Phase 4 尚未消费该查询，递归实例运输与释放也未交付。
phi 入边的环境候选另发布 `instance_root` 与原始 capture 位置路径，过滤未跟踪 capture 不改变位置；根层 `transport_value()` 仍可读取形成槽，嵌套来源返回 `None`；根层和嵌套层的 `transport_read()` 均给出已登记的实例地址与来源捕获槽，须在入边复制前沿根实例保存的捕获边定位实际来源实例，再按来源槽位置读取。并存候选按静态根 owner 与捕获路径共同区分，同路径的不同静态根不会误合并；同一定义跨轮的动态实例仍须另行运输；同节点多路径与递归 capture 的 deferred 保留；紧邻环境同一 capture source 有多个已知 leaf 子来源时，因形成时选择未逐层保存，以 `EnclosingEnvironmentCapture` 原子 deferred。同一父 capture 的多个已知子来源中只要有一个持有 tracked source，或任一已知子来源可能与 opaque 值合流，frontend 就因父实例选择位尚未运输而以 `AmbiguousClosureInstanceTransport` 原子 deferred。opaque 可能性由 MoveOnly 结果、局部绑定和控制流合流传递。递归释放规划的内部 `Captured` drop 与 `EndCaptureLoan` 现分别携带父环境根及原始 capture 路径；前者再由 capture slot 指向子值，兄弟路径不会共用同一实例地址。`EndCaptureLoan` 对已跟踪 source 还给出当次实例的 capture 槽，非 owning place 可无 phi 槽；`TestLastCaptureLoan` 与其保护的 retained-source drop 必须携带所在环境实例地址和 source capture 槽，可沿已保存边读取当次 source。静态 source owner 仍用于前端状态合流和选择定义，codegen 尚未消费这些地址或实现按动态 source 值的借用计数。这组事实不等于完整的实例级释放或 native 运输。形成快照的 selector copy 可额外记录紧邻环境的 capture 来源值；仅在 leaf 子来源自身快照持有该 selector、父环境本身不用它且来源槽唯一时填写。同一 capture source 的可达条件版本若指向不同环境值，或未覆盖当前路径，则不从首个输入臆选来源位置。形成动作先从当次父环境捕获槽取得值，随后快照复制从新环境已保存的接收槽读取选择；这不表示该槽已在 phi 或 native 中运输，条件 leaf 的延期仍保留。内部释放遍历仅在紧邻环境的静态子来源唯一、其直接输入含 owned move 且无 shared capture 时沿接收槽展开后代 drop；多候选与直接 shared 来源仍不推定，父槽未被已知子来源覆盖的条件保留原 drop，循环的公开计划继续受 deferred 门禁保护。Lambda body 的 `LambdaEntry` 首项另发布 `BindClosureEnvironment`，声明调用 ABI 传入的当次环境实例绑定到 body 的静态 owner，随后才执行同点未使用 owned 参数的 drop；调用点现仅对无条件唯一来源的不可重绑具名 callee，且实参后原 owner 仍由该 binding 无条件持有时，发布 `CallEntry` 的 `PassClosureEnvironment`，将已求值实例交给该入口；条件 leaf 内部回放已消费这对事实。可变 callee 与复杂表达式没有独立持有事实，仍不发布调用端传递；Phase 4 未消费这些动作。

binding 能力由声明和参数 mode 决定：Owned、Shared 或 Exclusive。place 由 root symbol、field path
和 container element identity 组成；冲突判断基于 place 重叠，不依赖源码字符串。

Borrow 与 Inout 调用分别建立 shared/exclusive loan。loan 在调用或 capture 生命周期的确定边界结束；
派生 field/reborrow loan 活跃时不能提前结束父 loan。字段赋值同时检查可变性、receiver 能力和 active
loan，普通源码违规形成所有权诊断。

## Value delivery 与构造

每个已选择 call argument 和 receiver 都归一化为 source-qualified contract。Value delivery 明确区分：

- Copy：源类型满足 `Copyable`，源 binding 保持可用；
- Move：消费 named/root place，后续使用被拒绝；
- Temporary：消费本次求值产生的 owner。

构造、container、Rc、assignment 和 return 复用同一 delivery 模型。求值若以 `Nothing` 提前终止，
只提交已实际执行前缀的 effect。任一所有权错误会清空 executable loan/delivery/drop facts，避免后端
消费部分成功状态。

`!!` 的操作数消费不受外层实参 Borrow mode 改写。单文件入口读取 typed assertion descriptor，
Copyable inner 使用 Read，MoveOnly inner 使用 Consume；unit 入口复用按 Copyability 分流的消费检查。
两条路径的 liveness/drop 同步整体转移，借用提取结果时由 assertion temporary 在 CallReturn 析构。
单文件 `NonNullAssertionOwnershipPlan` 保留 typed descriptor 的唯一求值身份和来源 place，
分别发布 non-null Copy/Consume 转移与封闭 null Abort 效果；失败边没有 take、正常后继或 unwind
cleanup。不可达 assertion 不登记，任意所有权诊断清空全部计划，发布顺序按 AST identity 固定。
`CompilationUnitOwnership` 提供 source-qualified `UnitNonNullAssertionOwnershipPlan` 与按 expression
查询的接口，直接消费 unit typed descriptor。赋值失败回滚本次提取计划，任意 ownership 错误清空
全部计划；缺失或不一致的 validated descriptor 返回内部阶段错误。两条路径的结果仍由普通
drop facts 管理；这不代表 Phase 4 lowering 已接入。

单文件与 unit 在 loop 正常回边和 continue 边检查 moved owner 是否仍在下一轮 live set 中，
重复消费产生 L0131；break/return 不参与回边检查。显式 `loop` 的 header 从空集求固定点，
退出后的使用经 break 边传播，允许每轮先重新赋值的 owner。普通 whole-root 赋值无需读取
已移动旧值，成功后恢复 binding；字段、元素、复合赋值和有效 loan 仍执行原有访问检查。

## Capture、Transferability 与 drop

closure capture 按 lambda identity 和 source binding 发布。普通 closure 借用 capture；`move` closure
按 Copy/Move 获取 owner。跨线程 effect 检查完整环境的 `Transferability`，不从语法形状猜测。

反向 liveness 产生 source-qualified drop plan，覆盖参数、local、temporary、replaced value、container
element、constructor root 和 closure environment。drop point 包括最后使用后、replacement 前、
branch/loop/control transfer、return 和 callable 退出；复合 owner 按逆构造顺序析构。

单文件 `when` 的条件沿未匹配路径依次检查。逗号 alternatives 合入同一 body 前，
`WhenAlternativeMatch` 标记仅部分匹配路径仍持有的 owner 析构，避免状态交集丢失析构义务。
nullable 条件路径消费 typed 剩余域；单文件产物提供 nullable plan 查询，包含 body-entry view、
Copy/Consume extraction 和分支 drop 关联。proof 随 State 失效及合流，任何所有权诊断清空计划。
extraction 区分 entry body 与 condition alternative；Nothing 调用没有正常出口。未消费 temporary
subject 在正常、return、break/continue 边清理，abort 不展开析构。

调用前缀已建立的 loan 与借用 temporary 随 ValueState 分支传递，保持较早实参 owner 活到调用结束。
`LoanEndFact` 区分 CallReturn 与放弃尚未调用前缀的 ControlTransfer；同边先结束 loan，再执行 drop。
这些前端事实不等于 native 支持：当前单文件 lowering 仅消费正常 CallReturn 的 loan end，
ControlTransfer 结束事实还需 Phase 4 接入。

编译单元 drop planner 按调用身份在 ValueState 保存前序借用的具名 owner，使后续实参中的
分支与嵌套调用不会提前析构它；正常 CallReturn 只解除当前调用的保护。字符串插值中的
Nothing 传播无正常出口，callable 退出时清除未提交调用的保护，避免阻止最终 owner 清理。

unit 调用的 Borrow temporary 与尚未提交的 MoveOnly Value 实参也保存在 ValueState，随分支
保留。正常提交时 Value 不再清理，Borrow 在 CallReturn 逆序清理；return 和离开调用前缀
所在循环的 break/continue 先清理后建 local，再逆序清理前缀 owner，最后清理旧 local。
内层循环跳转保留外层调用 owner，Abort 不展开清理；return operand 自身不继续时也不生成
return cleanup。专用 unit native 已消费新增 pending Value operand，按这些事实清理
未提交 owner；实际 SSA/LLVM 与 native 计数证据见常量交付验收。

## 实现与测试位置

单文件实现位于 `crates/lang-frontend/src/ownership_checking/`，unit 实现位于其
`compilation_unit/` 子树。对应覆盖位于 `ownership_checking`、`ownership_structural`、
`ownership_containers`、`ownership_closures` 与 `multifile_ownership_checking` integration suites。

单文件调用前缀同时保存尚未交付的 MoveOnly Value 实参义务。正常调用提交时移除义务，
不发布 CallReturn drop；return 或离开当前实参求值范围的 break/continue 发布对应
`Temporary(argument)` ControlTransfer drop，内层循环跳转保留外层待调用义务。Nothing/Abort
没有正常 cleanup。该状态与 borrowed temporary 共用既有逆序、作用域和 loop-depth 清理。

单文件常量读取直接消费 Phase 2 `ValidatedConstants` use descriptor，跳过常量声明 initializer
的运行时 flow；常量名称没有 place/root，限定读取不求值 object/companion receiver。String use
沿既有 temporary loan/drop 路径处理，二元运算的各次读取分别逆序析构。scalar/Char Value
交付和 String return 不产生常量声明 owner，closure capture 不包含常量或关联命名空间。
`OwnershipCheckedFile::constant_materializations` 发布 `ValidatedConstantMaterializations`，包含
按 expression identity 查询的 `ConstantMaterializationPlan`，区分 InlineCopy 和 StringTemporary。
计划保留 typed descriptor 的值、类型与读取身份；只记录运行时遍历到的 use，不记录 initializer
依赖或不可达尾句。typed constants 缺失、所有权诊断或 deferred 均阻止能力发布，`matches`
核对同一次 typed analysis identity。局部 initializer 保留终止流，无出口 loop 阻止后续 drop 规划。
这些行为由 `ownership_constants` 覆盖，不代表 compilation-unit 或 Phase 4 支持。

String 常量的 Group 只透传读取：Borrow loan 和 temporary drop 的 owner/value-origin 归一到
物化 descriptor 的叶表达式，call argument 与 drop point 仍保留原语法身份。通用二元左 operand、
String interpolation 及专用 String operand 的终止状态向外传播，阻止后续读取及正常 drop 规划。

String binary 的已完成左 temporary 和各插值输入进入现有 ValueState 清理栈，跨后续操作数的
return/break/continue 逆序释放，Abort 不展开。正常 binary 仍在 AfterBinaryOperands 按右左清理；
插值在 AfterExpression(外层 String) 仅清理本次输入 temporary，不释放外层结果或仍 live 的 named owner。

具体 MoveOnly Value receiver 的隐式调用没有表达式 identity；unit planner 将其作为
`This(owner)` 加入既有 pending 队列。参数提前 return 按实参、receiver 的逆序发布 drop，
正常提交移除义务，Abort 不展开。沿用既有 source-qualified receiver origin，不伪造
expression temporary。
条件 StaticSelf receiver 也进入该队列，并保留 OwnedThis 的模板类型；正常调用提交后恢复
模板析构义务，具体 MoveOnly 是否已交付由下游检查。退出时发布条件 drop，并用
`preceding_drops` 指明同一 point 中先执行的普通 drop 数量，保留后建 local、实参、receiver、
旧 local 的清理顺序；Copyable 实例不生成 receiver 析构。

function-value callee 的命名 root 在实参求值期间登记 pending borrow；最后一次源码读取不再
提前析构 closure，正常路径在 CallReturn 按活性清理，提前退出沿用 ControlTransfer 事实。
仍被 live closure shared capture 的来源不会在普通分支退出时析构；最后一个 closure 结束后
再按来源活性发布清理。此为前端事实，不表示 native 已支持带捕获的 borrowed closure。
