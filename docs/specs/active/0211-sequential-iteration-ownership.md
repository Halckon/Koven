# SPEC-0211：顺序迭代 source/element loan 与退出清理

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：实施或评审对应阶段 Goal 时 · **唯一真源**：本 Spec

## 1. 元数据

| 字段 | 值 |
|---|---|
| 状态 | `in-progress` |
| Goal ID | `KOV-P3-211` |
| 所属 Phase | Phase 3 |
| 语言规范 | [现行 v0.37 §37](../../guide/12-collections-destructuring.md#37-借用式顺序容器迭代-provider) |
| 批准依据 | 2026-09-19 用户明确启用 v0.37；按持续 Goal 顺序推进 |
| 前置 Spec | SPEC-0029、0030、0032、0179 `done` |
| 前置 ADR | [ADR-0023](../../adr/accepted/0023-borrowed-sequential-iteration-provider.md) `accepted` |
| 阻塞项 | 无；前置已满足，当前顺序切片 |
| 影响范围 | `lang-frontend` iteration ownership/loan/liveness/drop/capture facts、fixtures；Architecture |
| 语言语义变更 | 否；实施启用后的 v0.37 iteration lifecycle |

## 2. Goal

完成后，ownership checker 消费 SPEC-0179 typed plan，为整个 `for` 建立 shared source loan、
每轮 element/component Borrow binding、temporary source 延寿和所有正常/提前退出 cleanup facts，
并用既有所有权诊断拒绝从迭代借用中移动或修改 source/element。

## 3. 范围与需求

- named/field source 建立覆盖 provider/body/backedge 的 shared place loan；Borrow source 复用或
  reborrow，Inout source shared-reborrow。进入循环前已 moved source 继续使用 L0131。
- temporary source 成为 hidden owned temporary 并延寿到 `LoopExit(statement)`；不得在 source
  expression 后 drop。source loan 结束后才 drop temporary，元素只由 container owner drop。
- 每轮建立 element shared loan；名称和具名解构 component 是 non-owning shared binding，`_`
  不建 binding。Copyable read 复制，MoveOnly Value/return 使用 L0133，`&binding` 使用 L0134。
- source loan 下的整体 move/drop、replacement、exclusive access 与未来 relocation 使用 L0135；
  shared read、嵌套 shared iteration 合法，不按已知当前索引放宽 root conflict。
- closure capture 复用 L0137/L0138：element/component Borrow binding 的 capture 必须在本轮
  结束前释放，owned capture 不得从 Borrow binding 取得 owner；普通 body-local owned source
  可被仍存活的 shared closure loan 延寿。跨线程 delivery 不得绕过既有约束。
- 正常 fallthrough/continue 先逆序析构 body-local owner/结束其派生 loan，再结束 element-derived
  loans 并保留 source loan；break/exhaustion 随后结束 provider/source loan 并处理 temporary。
  return operand 先求值和交付，再按同样的 body-local → element → provider → source → temporary →
  outer-scope 顺序清理，不能为允许 `return source` 而提前结束 loan。
- 每轮 binding 状态重新建立，不把上一轮 moved/error state 合并到下一轮；nested jump 只清理
  最近 loop，所有事实按源码与逆析构顺序确定发布。

## 4. 非目标

- 不生成 SSA/LLVM，不实现 provider cursor/length、runtime function 或新诊断码。
- 不实现 consuming/custom/Map/range/String/IO iteration、borrow-return、一般 NLL 或容器 mutation API。
- 不改变普通 call loan、local destructuring、container replacement 或 closure capture 语义。

## 5. 验收标准

- [ ] named/field/Borrow/Inout/temporary source 产生精确 source loan 与 lifetime facts；循环后 named
  source 可复用，temporary 不早析构且每条退出路径恰好 drop 一次。
- [ ] Copyable binding 普通值使用/return 合法；MoveOnly Value/return 为 L0133，`&binding` 为
  L0134，borrow call 合法。
- [ ] body 内 move/drop/replace source 或 exclusive access 产生 L0135；shared read 与 nested shared
  iteration 合法。
- [ ] 名称与 mixed value-class component borrow、discard、borrowed/owned closure capture 形成正确
  L0137/L0138 与无多余 binding/drop。
- [ ] normal/continue/break/exhaustion/return/nested loop 的 cleanup facts 精确锁定 body-local owner、
  derived loan、element/component loan、provider、source loan、temporary 与外围 scope 的顺序；
  return source 在 operand Span 产生 L0135，abort 不发布 unwind cleanup。
- [ ] liveness 不再把 provider 使用的 named source 在 source expression 后提前 drop；重复运行
  结果确定，受影响契约回归通过，Architecture 与实现事实同步。

## 6. 技术方案与边界

新增 statement-keyed `IterationOwnershipPlan` 与职责明确的 iteration loan owner/category，不把
持久 loan 伪装成同步 call。复用 `OwnershipPlace`、dynamic element identity、non-owning binding、
closure checks 和 `DropPoint::ControlTransfer/LoopExit`；liveness 必须认识 hidden provider use。
Phase 4 只消费 validated cleanup 序列，不重新从 jump AST 推导生命周期。

条件 closure 交付的清理约束（循环运输未闭合）：owner 可用性、closure origin、owner drop 与 capture
loan end 必须保留同一组路径条件；不能仅把 origin/owner 取并集后无条件清理。分支尾值在
scope/branch cleanup 前登记实际结果持有的 capture，未选来源在本分支尽早析构，选中来源
保持到结果 closure 释放。嵌套路径支持合取，合流支持析取，稳定条件身份可以保持事实的
轻量查询。条件指向当次求值保存的选择，并随对应 owner 的动态实例/转移传递；不能重新
读取条件源码，也不能把 AST ID 解释为全局“最近一次分支”。Phase 4 消费端未支持条件时
必须显式拒绝，不得静默把有条件事实按无条件执行。该产物同步包含单文件 drop lowering、
iteration cleanup 与 nullable cleanup 关联；不以统一延长全部来源寿命替代逐路径 ASAP。

动态实例的实施约束：SymbolId 只映射当前 owner，RHS temporary、旧目标值和 loop phi 结果
必须具有不同的 owner value 身份。选择快照跟随值转移；`g = f` 后再次赋值 `f` 不得覆盖
`g` 的选择。条件引用 selector 身份，copy/phi 后重绑定到目标 selector。替换边界按
RHS 完成并保存新快照 → 用旧快照清理旧 owner（RHS 仍保护来源）→ 提交新 owner/快照
排序；提前退出不提交。循环 phi 入边具有并行复制语义，不能依次覆盖后续复制的源。

循环运输的实施细化（整体未闭合，已验证切片见第 10 节）：

- 先在 frontend 求 loop-carried closure origin 的有限固定点；域由 symbol 与 lambda identity
  构成，不能把条件 DAG 节点数或模拟执行轮数作为收敛条件。例如 `g = f; f = lambda`
  要覆盖下一轮 `g` 获得该 lambda 的情况；不可达 tail 不产生 origin。此域仅表示可能来源，
  不能作为动态环境或 captured source 的清理实例身份。
- 每个循环携带 owner 单独保留 phi value 身份，运输可用性与各 origin 的条件字段。
  incoming 字段是该边的已检查条件，目标是独立的布尔 selector；各字段同时求值后同时
  提交。没有该 origin 的边写 false，不从上轮遗留值或源码控制变量补齐。phi selector
  必须有独立来源类别，不能伪装为 source 表达式的普通分支。
- captured drop、capture loan end 与 pending capture 必须显式关联所属 owner value。
  lambda/source 仅描述捕获槽；copy/phi 同时运输环境与实际 captured owner/loan 实例的关系，
  不从 AST 或动作相邻关系猜测目标。同一 lambda 在多轮生成的 `f/g` 可以捕获同一 SymbolId
  的不同 owned 实例，必须分别定位。仍被环境持有的局部 source 即使名称已离开词法 scope，
  其 owner 义务也须运输；当前 binding 与旧环境持有的 source 不能合并为同一个 SymbolId 值。
- header phi 的 incoming 是进入循环的入口、body 正常完成与 continue；exit phi 的
  incoming 是耗尽及 break。零轮耗尽使用已由入口初始化的 header；break 只走自身清理，
  不再执行耗尽清理。return/Abort 不产生正常出口 incoming。所有边保留 point 与可达条件。
- incoming 在该边局部 owner、element 与 provider 的相应清理完成后采集；仅运输仍存活的
  owner。仍在外围 pending call、temporary 或 capture 链中的条件义务不能因循环丢失，
  也不能把已释放的 branch-local alias 重新带到 header。nested loop/jump 按目标循环区分。
- 静态分配必须先建立 header/exit 身份，再规划依赖它们的 body；phi 字段在每条实际入边
  均初始化。RHS 保存与旧 owner 清理仍使用独立值；后端只能消费显式关系，不从 AST 重推。
  验收必须实际执行零轮、多轮、break/continue 的条件运输，核对所有 source 的逐路径
  ASAP 和恰好一次析构；仅断言出现某条 guarded drop 不足以关闭循环携带 P1。

## 7. 实施计划

1. [ ] 建立 source/temporary provider lifetime 与冲突 → 验证：source category/L0131/L0135 矩阵。
2. [ ] 建立 element/component Borrow binding 与 capture → 验证：Copyable/MoveOnly/closure 矩阵。
3. [ ] 接全部 exit cleanup 与 liveness → 验证：normal/jump/nested/temporary drop 矩阵及受影响的共享契约测试。

## 8. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | lifecycle facts、诊断复用、测试与完成文档 | `feat(frontend): own sequential iteration (SPEC-0211)` |

## 9. 阶段验收边界与递归环境运输

循环运输的阶段验收边界：SPEC-0211 在 Phase 3 以独立的前端事实回放执行零轮、多轮、
break/continue 的条件运输与 source 清理，必须实际按动作顺序读取 snapshot、并行复制 phi
来源，并核对动态 source 实例。第 5 节全部验收都通过后才可关闭本 Spec；
测试内事实回放不算 native 验收。provider SSA 表示由 ADR-0023/SPEC-0212 封闭，
SPEC-0182 在此前置完成后承担真实 native 集成，包括首轮 temporary source 的求值一次、
延寿及逐退出析构。

2026-09-23 用户决定保留 v0.37 对递归 owned capture 的合法性。`f = move { f() }` 与两个
lambda 交替捕获旧环境均可在不同轮次形成任意深度、但每次执行有限的 owner 链；不能以静态
lambda 来源图有环为由新增语言诊断，也不能截断链后发布完整清理计划。Phase 3 的后续产物
按以下合同设计：

- 来源分析仍用有限的 symbol/lambda 固定点；phi 静态布局改为有限图，环边引用已知来源节点，
  不沿环递归展开 `IterationClosurePhiOrigin` 树。每条 owned capture 边另有运行时槽，保存
  捕获形成时选中的环境实例句柄；图节点、`CleanupOwnerValueId` 定义点和动态实例不得混同。
- 每次 closure 形成建立独立环境实例；owned capture 在形成时保存**当时**被移动的环境实例
  句柄与该实例的选择快照。源 binding 被移动后失去句柄，新环境中的边仍保有旧句柄。同一
  lambda 的下一轮实例可指向前一轮实例，回边不能改指被 phi 覆写后的当前环境。phi 入边
  先读取所有来源句柄、可用性与来源选择，再并行提交目标槽；缺席来源明确清空目标槽。
  静态来源图可以有环；实际 owned 边只指向形成前已存在的实例，因此每条已形成的实例链无环。
- 清理事实必须给出根实例及其拥有的捕获边，消费端只沿实际保存的句柄释放，不从 AST 或
  静态来源图猜测递归深度。释放从根沿 owned capture 边按捕获逆序递归；每个实例只释放一次。
  shared capture loan 结束及最后借用者查询必须绑定所在实例和实际 source owner；仍被链持有的
  source 不因词法 binding 离开或下一轮替换而提前析构。测试内事实回放覆盖零轮、多轮、
  break/continue 及同一 lambda 和交替 lambda 的链，逐实例核对恰好一次释放。
- 紧邻环境的 `Environment { owner, slot }` 输入由该 `owner` 的**当次实例**读槽，槽内
  保存的环境句柄及选择快照整体进入新环境；嵌套 phi 的候选节点须按保存的环境来源判别，
  不能把外层原 binding 的静态 owner ID 当成当前实例，或把所有候选无条件标为存在。
  现行树形 `IterationPhiIncomingOrigin::environments` 只容纳静态 `Owner` 候选，
  `drop_closure_owner_inner` 对紧邻环境目前仅能在唯一已知 owned 子来源下展开后代事实；
  后续实施须同时替换树形候选读取与静态释放合同，按已保存边释放根实例，
  而非仅删去 `EnclosingEnvironmentCapture` deferred。
- 在图布局、实例关系和释放动作均可完整表达之前，保留 `RecursiveClosureCapture` deferred，
  并原子拒绝该文件的 iteration、cleanup、drop 与 loan-end 产物。该 deferred 是实施缺口，
  不是 v0.37 的非法源码诊断。

递归路径的 Phase 3 事实按以下执行顺序补齐，不能以静态图节点代替当次实例：

1. 每次形成环境时分配新的实例句柄；`SaveClosureCapture` 先从形成前状态读取 `Owner` 或
   `Environment { owner, slot }` 指定的实例句柄及选择快照，再把它们写入新实例的 capture
   槽并消费 owned 来源。槽的静态布局由有限图节点和 capture `position` 决定，槽的实际值
   始终由实例句柄定位；同一节点可同时有多个存活实例。
2. 每条 phi 入边先在同一旧状态读取全部 binding 句柄、来源实例槽、存在位与选择快照，
   形成待提交写集；然后一次提交目标 phi 的根句柄、根层引用与静态选择位。缺席的来源
   在写集中明确标为空值，提交时清空目标句柄和相应存在位；已形成实例内部的 owned
   capture 槽不因 phi 转发而重写。回边若捕获旧 header 实例，读取必须发生在 header
   覆写前；`Environment` 来源读已形成的内层实例槽，不重读其形成时外层槽。
3. 释放动作以根实例句柄开始，按该实例实际保存的 owned capture 槽逆序递归释放；
   释放后清空被消费的句柄。shared capture 的 loan end 查询同一实例保存的 source 与
   选择快照。静态来源图的环只用于布局与候选条件，不参与运行时递归遍历。
4. 同一图节点可由一个根环境的多条捕获路径同时到达。phi 复制根实例句柄及其选择快照，
   不把后代捕获展平写进每个 `(图节点, capture 位置)` 对应的唯一 phi 布局槽；读取后代时
   从**实际父实例句柄**的 capture 位置取子句柄，再用该子实例与其自身位置读取值。
   两条路径即使指向同一静态节点也必须分别保留子句柄。条件菱形的 Entry 虽可证明原始
   flag 分支互斥，后续轮次不能只靠该源码 flag 推断嵌套存在位；从已保存的父实例捕获边
   取得实际存在性，不能把独立的 header presence 位当成跨轮互斥证明。
5. `EndCaptureLoan`、`TestLastCaptureLoan` 与 captured `DropFact` 的前端事实已携带
   `instance_address` 与捕获槽；静态 owner/槽 ID 本身仍不能在同一 lambda 的两个
   子实例间选择。解除 deferred 前，消费端必须从根实例沿已保存的父实例捕获边解析
   这些地址，定位实际 loan/source，再结束 loan、判断最后借用者并释放；递归根动作也须
   逐实例执行。不能把静态 owner ID 或 `(图节点, capture 位置)` 当成实例地址。

解除递归 deferred 的独立事实回放至少覆盖零轮、连续两轮以上同一 lambda 自链、两个
lambda 交替成链，以及各自的 break/continue/耗尽出口；每次回放都逐实例核对形成前读值、
phi 旧状态并行复制、缺席清空和恰好一次逆序释放。只有这些路径与紧邻环境来源均能从
完整产物执行，才允许发布 iteration、cleanup、drop 与 loan-end；无法表示任一边时继续
原子 deferred，不发布部分计划。
另需覆盖同一静态 lambda 的两个子实例同时由一个外层环境持有：从真实
`CreateClosureOwner` / `SaveClosureCapture` 与前一循环的 phi 动作形成两条不同句柄，
再验证后一循环的 Entry、回边和 exit 只移动根句柄而保留两条不同的捕获边，最终逐实例
恰好释放一次。另用两个子环境同时 shared 捕获同一 source 的实例验证：两条实际 capture
loan 分别结束后，才允许对该 source 实例发布最后借用者选择与 drop。用测试自造的
子实例或仅比较静态 owner/槽 ID 不满足此验收。

现行 [ADR-0009](../../adr/accepted/0009-concrete-closure-internal-abi.md)使用内联具体环境，
无法按值表示这种无界自嵌套链。上述决定先约束 Phase 3 事实运输；未来若让这类 closure
进入 native，须先以新 ADR 封闭环境存储与 drop ABI，再修改 Phase 4 消费端。SPEC-0182
首轮 native 的 named/Borrow/temporary source 范围不以递归 closure native 为验收项。
此 ABI 的候选已记录于 [ADR-0025](../../adr/proposed/0025-recursive-closure-environment-handles.md)，
状态仍为 proposed，不改变现行 ADR-0009。

## 10. 验证记录

实施前按[分层验收](../../development/testing.md)将第 5 节各项映射到实际测试目标/过滤器；
记录命中数、结果与未运行原因。同一状态下的有效证据只运行一次，不默认运行 frontend 全量。

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 2026-08-27 roadmap/实现审计 | 通过 | 当前 checker 仅 Read source + maybe-loop；drop planner 可能在真实 provider 使用前析构 named source |


2026-09-19 候选重基核对：保留现行 v0.36 的 grammar、nullable/Nothing、所有权与常量契约，
拟议版本取代关系见 proposal。仅更新基线与状态前置，不改变本 Spec 的阶段范围、验收条目或
批准状态；guide 启用与 ADR 接受仍是实施前置。未运行 Rust 测试（本次仅文档）。

2026-09-19 启用记录：v0.37 已启用、ADR-0023 accepted；temporary source 纳入首轮 native。
上方重基时的未启用说明是历史记录，不再是当前阻塞项；实现/验收尚未完成。

2026-09-19 实施检查点：source/element 使用独立 iteration loan owner；non-owning binding 接入
L0133/L0134/L0135/L0138；Copyable member Value delivery 归一为 Read，修复独立评审复现的
误拒绝。source liveness 与 drop iteration frame 已接入，初始 7 项直接回归通过。

剩余范围不变：公开 `IterationOwnershipPlan`、完整 element/provider/source loan 结束和有序
cleanup facts、validated 原子发布、全部 source/closure/退出矩阵及最终门禁尚待封闭；以上
部分实现不勾选完整验收项。当前 break 的 ControlTransfer 与 LoopExit(for) 是不同退出路径，
后续公开 exit plan 必须显式区分，Phase 4 不得把两份 temporary drop 串在同一 break 路径。

检查点验证（不是完整验收）：

| 命令 / 检查 | 结果 | 边界 |
|---|---|---|
| `cargo test -p lang-frontend --test ownership_iteration --test ownership_checking --test ownership_nullable_when` | 10 + 29 + 26 项通过 | 最后一次修复后；source 条件 return/abort 不再建立 provider，函数体终止不再执行尾部清理 |
| `cargo test -p lang-frontend --test ownership_iteration --test ownership_checking --test ownership_containers --test ownership_closures --test ownership_nullable_when` | 9 + 29 + 12 + 13 + 26 项通过 | source 条件提前退出修复前；不冒充最终完整门禁 |
| 独立逻辑评审及复核 | 已修复 Copyable member 误拒绝、source If condition 终止传播及函数尾部 abort 清理 | 评审未运行 Cargo；完整公开 plan 仍待实现和评审 |
| `cargo fmt --all`、`git diff --check`、`python3 scripts/check_docs.py` | 通过 | 文档结构检查 371 文件 |
| strict frontend all-target clippy / workspace all-target check / native | 本切片未运行 | 等 SPEC-0211 完整产物封闭后执行对应门禁；native 属后续 Phase 4 |

以上为首个实施检查点的恢复记录，后续推进如下。

公开计划检查点：已增加 statement-keyed `IterationOwnershipPlan` 与独立
fallthrough/continue/break/return/exhaustion exit；清理序列包含 call/capture 派生 loan、
binding/element、provider/source 和 owner drop。嵌套 return 的完整序列按 point 只执行一次。
独立评审发现 break 漏 dead named source drop，已先复现再修复；break 自身在 source loan
结束后按后继 liveness 清理，不复用 exhaustion 路径。

| 公开计划检查点验证 | 结果 | 边界 |
|---|---|---|
| `cargo test -p lang-frontend --test ownership_iteration --test ownership_checking` | 16 + 29 项通过 | 最后修复后；包括 nested return scope 顺序、closure 派生 loan、break named owner、while condition abort 不发布 fallthrough |
| `cargo test -p lang-frontend --test ownership_iteration --test ownership_checking --test ownership_closures` | 15 + 29 + 13 项通过 | while condition 终止传播修复前；其余公开计划与共享契约证据 |
| 独立逻辑评审及复核 | break named owner 漏清理、while condition 终止传播已修复并复核 | 两项均先由失败回归复现；不是完整 Spec 评审 |
| `cargo fmt --all`、`git diff --check`、`python3 scripts/check_docs.py` | 通过 | 371 Markdown 文件；最终 strict clippy / workspace / native 仍未运行 |

以上为公开计划切片恢复点。后续 lambda/source 切片已增加独立 lambda body liveness/drop
traversal：隔离 callable 的 loop/scope/owner，捕获环境 owner 不在每次调用重复析构，
尾值交付后不再析构返回的参数。两项 lambda 计划为空的失败回归已复现并修复。

| lambda/source 切片验证 | 结果 | 边界 |
|---|---|---|
| `cargo test -p lang-frontend --test ownership_iteration --test ownership_closures` | 18 + 13 项通过 | callable 隔离、owned lambda source 与尾值交付；独立评审随后指出 checker 尾值仍按 Read 的缺口 |
| `cargo test -p lang-frontend --test ownership_iteration --test ownership_checking --test ownership_containers --test ownership_nullable_when --no-fail-fast` | 21 + 29 + 12 + 26 项通过 | 新增 field/Borrow/Inout source、循环后复用、moved entry、mixed component 逆序与非 owning drop 矩阵 |

恢复点：保留未提交实施改动。独立审查发现两项待修复边界：lambda checker 尾值仍按 Read，
须与 drop planner 的 Consume 交付统一；`listOf(listOf(1))[0]` 作为迭代 source 当前被错误
拒绝，接受借用时还必须保存实际外层 backing temporary owner，不能析构作为元素的内层
container。继续补这两项失败回归/修复、原子发布扩展验收与最终门禁。SPEC-0211 仍
in-progress，后续 Phase 4 未开始实施。

尾值/backing owner 检查点：先复现 lambda 借用容器尾返回未报 L0133、indexed source 错误
L0136，再统一 checker/drop planner 的 Consume 尾交付，并对直接 borrowed closure 尾值
执行 L0137 检查。source 改为 place 求值及显式 Shared loan；Index pending 义务覆盖索引
求值中的 return/abort。公开 source target 和迭代清理指向实际 backing temporary owner。
随后两层 Index 的 owner identity 回归也先失败，helper 已改为沿 Index/Group receiver 链
追溯原始 temporary；独立复核通过。

`cargo test -p lang-frontend --test ownership_iteration --test ownership_closures --test ownership_containers --no-fail-fast`
在多层 helper 修复前通过 27 + 13 + 12 项；包含一级 indexed temporary 的 normal/break/
continue/return，以及索引求值中的 return/abort。两项 closure 新夹具的语法错误已纠正为
显式分组的 lambda return 和右结合函数返回类型，不以夹具错误替代生产回归证据。

多层 helper 修复后，`cargo test -p lang-frontend --test ownership_iteration --test ownership_containers --no-fail-fast`
通过 29 + 12 项，新增先行 exclusive argument 与 source shared borrow 的冲突，以及两层
Index 的 owner identity/第二层 index return。`cargo fmt --all`、`git diff --check` 与
`python3 scripts/check_docs.py` 通过（371 Markdown 文件）。

剩余恢复点：控制表达式（如 `if (true) inner else inner`）包装的 borrowed closure 尾值
仍可能绕过 escape 检查；需封闭返回交付路径，并复核多层 named source 的 place/loan
identity、扩展原子发布与确定性证据。最终 strict clippy / workspace / native 尚未运行，
本 Spec 保持 in-progress，改动未提交。


返回分支/多层 named source 检查点：先以两项失败回归复现 If 包装的 borrowed closure
尾返回漏报及 `xs[0][0]` source 未阻止外层 owner move。返回检查现沿 Group/If/When
实际分支状态传播，覆盖 lambda tail、显式 return 与函数表达式体；普通局部控制结果和
closure 调用不作为返回 closure。OwnershipPlace 保存连续索引完整路径，prefix/known/
unknown 索引逐层判定 alias，named source 生命周期仍绑定根 owner。

共享 closure 套件中的旧字段 capture 夹具直接返回 shared `this` closure，违反 guide 07
禁止 borrowed closure return 的现行规则；已改为局部 binding 验证原 capture 身份，另以
原 expression-body return 断言 L0137。并非通过放宽生产语义保留错误通过结果。

独立静态审查核对返回状态传播、place alias 与 `.element()` 下游，未发现本轮新增重要
问题；确认剩余边界：`val alias = if (flag) inner else inner` 后 `return alias` 的 origin
仍丢失。此项尚未动态回归确认；后续须先复现，并覆盖不同 closure origin 的分支合流。
`.element()` 保留末级查询，完整路径通过 `.elements()` 查询；现有 native consumer 仍
明确拒绝嵌套索引 receiver，不据此扩充 Phase 4 支持声明。

最新恢复点：继续封闭控制结果局部别名的 closure origin、原子发布及确定性验收。
strict clippy、workspace all-target check 和 native 在本切片仍未运行；SPEC-0211 保持
in-progress，实施改动尚未提交。


本检查点验证：`cargo test -p lang-frontend --test ownership_iteration` 最终 33 项通过；
新增局部控制结果正例的声明后表达式边界夹具已纠正，未改变 parser。
同一生产代码下 `cargo test -p lang-frontend --test ownership_iteration --test ownership_closures --test ownership_containers --test ownership_nullable_when --no-fail-fast`
中 closure 13、container 12、nullable 26 项通过；当次 iteration 32 通过、1 项夹具解析
失败，不能将整条命令记为通过。iteration 经夹具修正后以上述单独命令复验通过。
`cargo fmt --all -- --check`、`python3 scripts/check_docs.py`（371 Markdown）与
`git diff --check` 通过；上述最终门禁未运行状态不变。


控制结果别名检查点：`val alias = if (flag) first else first` 后 return alias 已动态复现
漏报。checker 的 binding origin 改为按 AST index 排序去重的集合；If/When 成功分支先
收集实际尾值 origin，再合流，并在 initializer 完成后建立接收 binding。不同分支 closure
及分支局部 lambda 的借用逃逸均参与检查；capture loan 在合流取可能来源的并集。

独立审查发现该合流会对“仅 then 分支最后使用 closure、合流后 move source”产生 L0135
误报；单项失败已复现。修复保留 closure origin 合流，再按控制表达式后继 liveness 清理
已死 binding；返回结果持有的 origins 保留到接收方接管，不能用 loan 交集隐藏冲突。

后续恢复点：drop planner 仍使用单一 closure origin，须同步控制结果的 owner/capture
生命周期与条件清理；直接控制表达式进入 Value argument/field 的 escape 边界仍需审计，
以及原子发布、确定性与最终门禁。本 Spec 未完成、实施改动未提交，不以 checker 新增
拒绝证据宣称全部 cleanup/native 已支持。


复审补充：单凭控制表达式 live_after 清理，会提前释放已读取但尚未调用的 closure callee
或较早 Borrow 实参。`f(if (flag) 0 else 1, take(xs))` 已动态复现漏报 L0135。checker
现以 call identity 独立持有这些 pending origins，分支合流不能提前结束它们的 capture
loan；调用完成/控制转移后才释放该调用的持有，其他嵌套调用仍可继续持有同一 origin。
此为内部 checker 生命周期修复，不替代后续 drop planner 同步。


本轮最终验证：`cargo test -p lang-frontend --test ownership_iteration --test ownership_closures --test ownership_nullable_when --test ownership_checking --no-fail-fast`
通过 37 + 13 + 26 + 29 项；此后只扩展 pending call 的调用完成后释放正例，
`cargo test -p lang-frontend --test ownership_iteration control_argument_keeps_pending`
命中 1 项通过（36 项过滤）。独立复审确认原 ghost loan、pending callee/实参、嵌套持有、
调用结束释放及控制结果接收前的保护，未发现新增阻断问题；审查未扩大到 drop planner。
`cargo fmt --all -- --check`、`python3 scripts/check_docs.py`（371 Markdown）及
`git diff --check` 通过。最终 strict clippy/workspace/native 门禁仍未运行，实施状态不变。


planner 基础生命周期检查点：先复现已被 closure shared capture 的 owned source 在中间
BranchExit 早析构；`drop_named` 现检查存活 closure 的 capture 依赖。具名/分组 callee 以
Place 求值，由 pending call root 持有到调用完成或离开调用，避免 callee 在参数求值前
按 Read 早 drop。零轮 exhaustion 仍有独立 cleanup，不与调用后/跳出路径合并计数。

局部 captured source 在 return/break/continue 清理时，closure 析构可递归删除其来源，
已复现 `drop_deeper_than` 索引越界；scope/deeper cleanup 改按逆序 SymbolId 快照遍历，
重复遇到已析构来源不再发布 drop。独立静态复核确认保持原 scope 条件及退出顺序。

条件合流审计结论：当次审计时 DropFact/EndCaptureLoan 没有路径 guard，owner 交集与单一 origin
不足以支持不同分支持有不同来源；§6 已记录完整实施约束。这项公开产物尚未实现，直接
lambda/条件 callee 的 planner 保持与条件 owner 合流仍待完成；本基础修复不是替代方案。


本轮最终验证：`cargo test -p lang-frontend --test ownership_iteration --test ownership_closures --test ownership_nullable_when --test ownership_checking --no-fail-fast`
通过 39 + 13 + 26 + 29 项，包含 callee 参数中的控制分支，以及局部 captured source 的
return/break/continue 清理。`cargo fmt --all -- --check`、`python3 scripts/check_docs.py`
（371 Markdown）及 `git diff --check` 通过。§6 条件清理产物、直接/条件 callee、其他 escape
边界、原子发布与确定性扩展仍待完成；strict clippy/workspace/native 尚未运行。本 Spec
继续 in-progress，实施改动未提交。


条件模型基础检查点：新增 CleanupConditions 决策 DAG，DropFact 与 EndCaptureLoan 共用
可选 condition identity，OwnershipCheckedFile 运输只读条件表；常量 temporary 重建保留
condition。当前 planner 仍提供默认条件表，既有事实仍为无条件，不能据此声称 owner 合流
已完成。单文件 native drop consumer 在变更 binding/temporary 状态前明确拒绝 guarded fact。

最初按 ExpressionId 排序的模型会先读取尚未执行的内层 selector；只给 outerElse 的回归
已失败复现。现按控制 Span 起点升序、终点降序及 ID 排序，外围控制优先；同 ID 的 Span
和分支布局必须一致。合取/析取/互补与分支覆盖归约保留稳定节点，公式必须自带路径可达
前提，不单独查询未执行子分支。动态选择随 owner 实例运输仍属于后续 producer/lowering
工作，不把源码范围排序当成这一运输机制的实现。独立复审核对上述排序与条件保留通过。

下一恢复点：将条件表接入 ValueState 的路径、owner 可用性和 closure origins，先在 branch
scope cleanup 前接收尾值保护，再按条件合并 owner/closure 并生产 guarded drop/loan end；
覆盖不同来源、nested 条件、控制值转移及循环动态实例。当前代数 builder 尚无生产调用方，
相应未使用方法警告须在接入后消除；不要用 lint allow 隐藏。后端拒绝分支尚无真实 producer
驱动测试，必须在 guarded facts 实际发布后补齐。


本轮基础验证：`cargo test -p lang-frontend --lib cleanup_condition` 命中 3 项通过（71 项过滤），
覆盖多路真值组合、规范化/互补、无效布局及 Span 修改的无副作用拒绝、外层 else 不读取
未执行内层选择。`cargo test -p lang-frontend --test ownership_iteration --test ownership_nullable_when --no-fail-fast`
通过 39 + 26 项。`cargo check -p lang-codegen` 通过，保留上述未接 producer 的 2 类 dead_code
警告；不是 strict clippy 通过。fmt、文档结构检查（371 Markdown）与 diff 检查通过。
未运行 workspace all-target/strict clippy/native；条件生产和动态选择运输仍未实现，Spec
保持 in-progress，实施改动未提交。


条件 producer 实施检查点：ValueState 同时保存路径、owner availability 和多个带条件的
closure origins。If/When 分支进入后限制当前义务，成功尾值在 scope/branch cleanup 前登记
capture；合流按条件析取保留各路径 owner，释放时只析构未被 capture 持有的部分，并对
来源 drop 和 EndCaptureLoan 使用同一条件表。直接 callee 及 LocalVariable 的具名转移已接入。
不同来源的直接调用回归先复现 then 分支错误提前 drop，再通过修复后的 CallReturn 条件断言。

独立复审指出具名 Borrow 最后实参的 result_closures 尚未移交，导致来源延迟到 scope exit；
已加入 invoke(chosen) 场景，旧二进制先构建供失败复现；随后在匹配 pending frame 并登记
loan 后对稳定 place 清除重复 result holder，实际修复经独立只读复核确认。嵌套选择及
When/别名扩展回归、SSA 真实 guard 拒绝测试已加入，最终修复版整组验证尚未完成。
单文件 SSA 入口在验证分析身份后、构建 Program 前拒绝首个 guarded DropFact；drop emitter
保留防御检查。条件/临时 callee 的 pending 持有、assignment provenance、循环动态实例运输
以及其它逃逸边界仍待完成，本 Spec 保持 in-progress，未进行 native 或全阶段验收。


当前验证恢复点：旧二进制的四套定向回归已完成编译（曾等待 IDE target 锁），其中
ownership_checking 29 项通过，ownership_closures/ownership_iteration/ownership_nullable_when
仍待同一进程最终结果；不要重启替代它。该二进制不包含随后落地的 Borrow result-holder
修复，取得失败证据后需运行最终修复版受影响回归。fmt check、文档结构检查（371 Markdown）
及 diff check 已通过；新增 codegen 拒绝回归尚未运行。未运行 strict clippy/native；IDE 自发
运行的检查不计为本任务验收。本切片超过仓库 10,000-token 预算，保存恢复点后继续 Goal。


2026-09-21 验证恢复：旧进程句柄已失效且无遗留 Cargo 测试进程，重新运行当前修复版
四套定向回归，ownership_checking 29、ownership_closures 13、ownership_iteration 41、
ownership_nullable_when 26 项全部通过。该结果覆盖条件来源、具名 Borrow 实参、别名、
When 与嵌套选择，不包含随后新增的临时 closure pending 回归。

临时 closure pending 回归已失败复现：条件 callee 后续参数提前 return 时，选中来源未保留
到 CallReturn/ControlTransfer。现将 capture origins 存入既有 pending temporary，与 owner
一起按分支限制并在成功分支合流；正常调用与提前退出共用 closure 清理，先处理 owned slot、
再 drop closure、结束 capture loan、释放 dead shared source。Value 实参正常交付后撤销调用方
临时义务，未发生调用的退出路径仍负责清理。最终回归和独立复审待记录；Spec 仍 in-progress。


2026-09-21 临时 closure 切片验证：独立复审确认 pending temp 条件收窄/合流、Value 交付与
退出清理路径；补齐 owned capture 的未交付 Value 参数和 iteration cleanup 中
Temporary drop → EndCaptureLoan → source drop 的直接断言。回归随后发现静态函数名也被
分类为 Temporary + MoveOnly，导致虚假的 callee drop；现仅为 typed CallDescriptor 的
FunctionValue target 登记临时 callee，Source/External 静态目标不产生环境 owner。修复经
独立复核，并由既有 closure/nullable 回归验证。

最终 `cargo test -p lang-frontend --test ownership_iteration --test ownership_closures --test ownership_nullable_when --test ownership_checking --no-fail-fast`
通过 44 + 13 + 26 + 29 项（共 112 项）。该结果包含上述修复，并覆盖后续参数两分支都继续
时的 temporary origins 合流。codegen 真实 guard 拒绝回归也已通过：
`cargo test -p lang-codegen --lib rejects_produced_guarded_cleanup_before_lowering_bindings`
命中 1 项、过滤 444 项，验证在建立 SSA binding 前返回 UnsupportedNode 及准确来源 Span。
后续恢复点：assignment provenance、其它条件值逃逸边界、循环动态选择运输与完整 Phase 3
验收仍未封闭；不得把这次 pending temporary 修复当作 SPEC-0211 或 native 已完成。


本切片最终 fmt check、文档结构检查（371 Markdown）与 diff check 通过；无残留本任务测试
进程。未运行 workspace all-target、strict clippy 或 native，当前实施改动仍未提交。


2026-09-21 赋值来源切片：已复现 closure 重新赋值后，调用新值完成再移动新 capture source
仍被 L0135 拒绝。checker 在正常完成 RHS 且赋值合法后转移新 origins，结束不再持有的旧
capture，并按赋值后的活性释放未使用新值；直接/条件别名转移及 return 逃逸检查均覆盖。
非 root 字段赋值通过实际 RHS 分支检查 borrowed closure 逃逸。drop planner 对旧环境复用
含 guard 的 closure 清理，再将 RHS 保留的 origins 交给新 binding，保持同源 capture。

上述版本的四套定向回归通过 47 iteration + 13 closures + 26 nullable_when + 29 ownership_checking，
共 115 项。独立复审进一步发现 P1 缺口：RHS 中调用旧 f() 后可能按最后使用释放旧环境，
导致在 RHS 尚未交付替换值时错误允许 consume(xs)。guide §ASAP 要求完整求值 RHS 后再析构
未移动旧值，不能把此路径当成旧值已移动的例外。

新回归 `closure_reassignment_keeps_old_capture_until_the_entire_rhs_finishes` 已完成语法/类型
校验并在所有权断言失败（预期 L0135，实际诊断为空）。该测试保留失败，不能宣称当前套件
全部通过。下一恢复点：为 checker/planner 的赋值 RHS 保留仍可用旧环境及其 capture；正常
完成后再替换，真实移动旧值不重复 drop；return 清理旧环境，nested break/continue 只释放
离开 scope 的 owner。补 RHS 提前退出、f = f/条件移动、同源替换回归，并对修复重新独立审查。
该 P1 尚未修复，Spec 继续 in-progress，未提交实施改动；当前没有运行中的本任务 Cargo 进程。
本切片超过仓库 10,000-token 预算，保存以上可复现恢复点。未运行本轮 codegen、全 workspace、
strict clippy 或 native；不得把上一轮 codegen 的结果扩大成当前完整验收。

2026-09-21 RHS 生命周期修复：checker 和 drop planner 为正在求值的赋值 RHS 保留旧 root，
正常完成后才交付替换值；真实移动不重复 drop，同根嵌套替换仍可显式结束旧 capture。
return 清理仍可用旧环境，内层 break/continue 保留未离开 scope 的外层替换环境。
退出循环后释放已死局部 closure；独立复审发现直接 liveness 会漏掉另一 closure 的间接
持有，新负例先复现错误放行 consume(xs)，再按 active Place loan 保护、固定点释放修复。
复审已检查上述保持、退出和嵌套替换边界，未发现新的阻断问题。

四套定向回归通过 54 iteration + 13 closures + 26 nullable_when + 29 ownership_checking，
共 122 项，覆盖原 RHS 失败例、正常替换与 return 清理事实、直接/条件自移动、三类循环
退出及仍活跃的间接捕获链。另补已死捕获链逐层释放的正向用例，单独运行通过 1 项
（54 项过滤），本切片累计验证 123 项；最终 fmt、文档结构（371 Markdown）及 diff 检查通过。
SPEC-0211 继续 in-progress：其它条件值逃逸边界（包括 Inout root 赋值）、循环动态选择
运输与完整 Phase 3 验收仍待闭合；本轮未运行 codegen、全 workspace、strict clippy 或 native。
当前无运行中的本任务 Cargo 进程，实施改动仍未提交。该切片跨上下文累计超过仓库
10,000-token 预算，保留本检查点后从剩余逃逸边界继续，不重跑已完成范围。

2026-09-21 逃逸入口修复：Inout root 指向调用者的存储，现在和字段赋值一样，在实际 RHS
分支检查 borrowed closure。直接 lambda、If/When 与分支局部别名均产生 L0137，禁止发布
capture/drop facts；无 capture 及 move Copyable snapshot 仍可写入。修前循环元素的 borrowed
capture 被错误放行，新回归已记录失败后修复。

普通 Value 实参和构造参数原先在求值前检查 origins，也遗漏分支内声明的 borrowed closure；
两个新回归均先复现诊断为空。现沿可继续路径复用分支逃逸检查，保留已存在的提前退出路径，
每个参数仍只求值一次；跨线程交付继续使用既有 Transferable 检查。独立静态复审已核对
Inout 判定、实参/构造求值顺序、退出合并和合法 owned snapshot 交付，未发现阻断问题。

`cargo test -p lang-frontend --test ownership_iteration --test ownership_closures --test ownership_construction --test ownership_checking --no-fail-fast`
通过 60 + 13 + 6 + 29 项，共 108 项。fmt、文档结构（371 Markdown）与 diff 检查通过。
容器元素存储及其余逃逸入口仍待审查，循环动态选择运输和完整 Phase 3 验收仍未完成；
本轮未重跑 nullable/codegen，未运行 workspace/native，不能据此宣称 SPEC-0211 完成。

静态检查恢复点：`cargo clippy -p lang-frontend --all-targets -- -D warnings` 首次发现本批 WIP
return 检查中的 collapsible_if，已等价合并条件，fmt 再次通过。复跑仍在运行：exec session
`94602`，日志 `/tmp/koven-0211-frontend-clippy.log`，已确认 clippy-driver 子进程持续工作。
当前不能记为通过；到达切片预算检查点后继续等待此句柄，不能因观察窗口到期而重启。
行为回归日志为 `/tmp/koven-0211-escape-regression.log`，实施改动仍未提交。

2026-09-21 静态检查恢复：上述同一 session 已完成，严格 frontend all-target clippy 通过
（11m09s），并非全量测试通过。该结果属于元素存储修复前的生产代码快照。

动态身份独立审计确认：外层 `var f` 在 `for (flag in listOf(true, false))` 中执行
`f = if (flag) ({ read(xs) }) else ({ read(ys) })`，下一轮 RHS 的新环境与待析构旧环境
需要分别保留同一控制表达式的两次选择。当前 condition 按 control AST 身份去重，origin
仅携带 lambda AST 与 condition；iteration planner 只规划一次 body，未传播循环携带的
owner/origin 合流，耗尽仍使用入口状态，可能漏掉 guarded facts。此例仍待动态测试复现，
不能只用 Phase 4 guard 拒绝门禁证明安全。后续最小产物必须运输随 owner 转移的选择快照，
并发布循环入口、回边与出口的 owner-origin 合流关系；不能用全局“最近一次选择”替代。

元素存储入口已失败复现：`slots[0] = ({ read(n) })` 错误允许外层 Array 保留本轮 element
的 borrowed capture。元素赋值现在复用分支逃逸检查，receiver/index/RHS 顺序及提前退出
行为不变；直接 lambda 与 If/When 局部别名均拒绝，Array/MutableList 的无 capture 和
owned Copyable snapshot 均接受。独立复审未发现阻断问题。
`cargo test -p lang-frontend --test ownership_iteration --test ownership_containers --no-fail-fast`
通过 62 + 12 项（74 项）。该结果不包含随后新增的循环携带 closure 来源反例测试。

循环携带反例 `loop_carried_conditional_closure_preserves_its_sources_until_the_exit_call`
现已通过语法、类型与所有权诊断前置，并在 cleanup 断言失败：`xs/ys` 无条件在 LoopExit
析构，最终 `f()` 的 CallReturn 仅有环境 drop，没有所需 source drop。输出甚至没有 guard，
证实前述静态审计的 P1 风险；测试保留失败，不得宣称当前 iteration 套件全部通过。
失败日志 `/tmp/koven-0211-loop-carried-before.log`；下一实施点是循环携带的 owner/origin
合流及 owner 实例选择快照，覆盖零轮、多轮、break/continue 与旧环境/RHS 同时存活。

当前生产代码与新增测试的定向严格 clippy 通过：
`cargo clippy -p lang-frontend --lib --test ownership_iteration --test ownership_containers -- -D warnings`。
fmt、文档结构（371 Markdown）与 diff 检查通过；无运行中的本任务 Cargo 进程。未提交改动，
未运行本轮 workspace/codegen/native；SPEC-0211 继续 in-progress，下一切片先处理上述失败。

条件身份基础切片：`CleanupCondition::Choice` 改引用 `CleanupSelectorId`；选择的 control、
Span 与 arity 通过只读 `CleanupSelector` 查询。决策表去重、布尔展开与排序均使用 selector，
允许来自同一 AST 的旧实例和新 RHS 选择独立存在。直接 branch producer 暂仍每个 AST
分配一个 selector；owner value、copy/phi 与循环合流尚未接入，不能宣称 P1 已修复。
独立模型复审未发现阻断问题；集成 evaluator 已按 selector 查询，并对当前 direct fixture
显式断言一对一关系，避免把 AST-keyed 求值器误当动态实例验收。
`cargo test -p lang-frontend --lib cleanup_condition` 通过 4 项（71 项过滤），包含同 AST
不同选择的真值穷举、非互补、规范化及无效身份无副作用验证；原短路与布局检查继续通过。
当前 `ownership_iteration` 重新运行结果为 62 passed、1 failed、0 filtered，唯一失败仍是
上述循环携带来源反例。没有用忽略该失败或放宽断言来接受基础模型；循环快照仍待实现。
`cargo clippy -p lang-frontend --lib --test ownership_iteration -- -D warnings` 通过；fmt、文档
结构（371 Markdown）与 diff 检查通过。公开条件 API 变更的 workspace all-target check
已启动，结果待收集；本轮没有运行 native。
预算恢复点：`cargo check --workspace --all-targets` 的 exec session 为 `52566`，日志为
`/tmp/koven-0211-selector-workspace.log`，最新仍在检查 frontend/codegen；继续等待同一进程，
不能将启动或阶段输出当作通过，也不因观察窗口结束而重启。当前切片未提交，Goal 保持 active。

workspace 检查恢复：上述同一 session 已结束，`cargo check --workspace --all-targets`
通过（12m37s）。它检查的是 selector 基础版本，不包含随后新增的 owner snapshot builder。

owner snapshot 基础模型：新增静态 owner value 身份、带可达 guard 的 selector copy 与整组
条件重绑定；相关根共用一份 selector 映射以保持互补，目标 selector 按源身份顺序分配，
所有复制的 source/when 都读取复制前状态。共享 DAG 子节点按 children-before-parent 的
逆序汇齐可达路径，避免枚举全部路径或读取未执行的内层选择。输入根必须在执行边界可安全
求值；builder 不从 AST 猜测缺失的外围保护。无效根先整体拒绝，不修改表。
独立只读复审未发现实质错误，并要求覆盖共享子节点的 guard 合流，已增加对应测试。
该 builder 尚未接入 producer、旧 owner 清理的有序边界或 loop phi；内部方法目前可能产生
dead_code 警告，后续接入消除，不使用 lint allow。不能宣称循环携带 P1 已修复。
模型验证：`cargo test -p lang-frontend --lib cleanup_condition` 通过 8 项（71 项过滤），
包含原 4 项条件代数回归与 4 项快照回归：移动后替换隔离、内层 selector 短路、共享子节点
多父路径合流、无效输入原子性与确定性。此结果不代表实际循环或 native 已完成。
本轮模型分析与实现超过仓库 10,000-token 切片预算；下一恢复点是 producer 有序边界
（保存 RHS 快照、旧值 cleanup、提交新 owner）与 loop entry/backedge/exit phi。
`cargo check -p lang-frontend --lib` 编译通过，实际产生 1 条 dead_code 警告，涉及尚未接入的
`snapshot_conditions` / `rebind_snapshot`，不是 strict clippy 通过。fmt、文档结构（371 Markdown）
和 diff 检查通过；本轮未重跑 iteration/native。无本任务 Cargo 进程遗留，改动仍未提交。

2026-09-21 owner snapshot producer 接入：局部 closure 绑定与 owned root 替换，在 RHS 正常
完成后发布 SaveOwnerSnapshot；同一点的旧环境 drop/capture-loan end 后再发布
CommitOwnerSnapshot。公开 cleanup_steps 保留完整顺序，iteration exits 只是关联视图，
不得执行两次。非 owning 存储仍走既有路径；Phase 4 在建立 binding 前明确拒绝 Save，
不把缺失的 selector 运输静默当作普通 drop。

独立复审先后发现更宽事实读取未初始化快照、同 Span 部分重排及跨控制 Span 排序问题。
模型与源码回归均实际复现了未初始化读取。实现已移除部分重绑定：把全部将重写的 state
条件一起复制，按 selector 分配 ID 保持整组顺序；原 state.path 在新快照之前保护 Save 与
后续事实。控制选择须在受保护 body 前分配，原 Span 仅保留溯源用途。源码测试实际执行
Save guard/并行 copy，并按路径验证 source 恰好析构一次；非循环范围收口复审未发现新问题。
这些修复不包含 loop entry/backedge/exit phi，循环携带 closure 的 P1 仍未闭合。

本轮 `cargo test -p lang-frontend --lib --test ownership_iteration --test ownership_closures --test ownership_nullable_when --no-fail-fast`
结果：lib 79 passed / 2 failed（10 项 cleanup_condition 测试通过），closures 13 passed，
nullable_when 26 passed，iteration 64 passed / 2 failed。lib 的两项失败为
`parser::engine::tests::block_dispatch_legal_error_and_nested_families_stay_linear` 与
`lambda_body_legal_unsupported_and_poison_families_stay_linear`；本轮未改 parser，原因尚未归因，
不得据此称 lib 全通过。iteration 包含已知循环携带失败，以及新增替换顺序测试夹具失败：
旧环境无未来使用，已被合法 ASAP drop。夹具已改为在 RHS 实际读取旧环境，并用独立
局部声明避免换行调用链歧义；单独复跑 replacement_saves_new_snapshot 测试已通过。
日志 `/tmp/koven-0211-snapshot-producer-regression.log` 与
`/tmp/koven-0211-snapshot-replacement.log`。本轮尚未重跑 codegen、strict clippy、workspace/native。

fmt、文档结构（371 Markdown）与 diff 检查通过，无本任务 Cargo 进程遗留。当前切片
超过仓库 10,000-token 预算，改动仍未提交，Goal 保持 active。下一步先完成 codegen 门禁
回归与定向 strict clippy，再实现 loop owner/origin phi；两项 parser 复杂度失败待单独归因。

后续验证恢复：`cargo test -p lang-codegen --lib rejects_produced_guarded_cleanup_before_lowering_bindings`
通过 1 项（444 项过滤）；`cargo clippy -p lang-frontend --lib --test ownership_iteration -- -D warnings`
通过。前者验证尚未消费 Save 时的明确拒绝，不是 native 运输通过。

循环失败矩阵改用动态 `flags: List<Boolean>`，分别覆盖正常完成、continue 与 break；三个
测试均通过语法、名称、类型和所有权诊断前置，在所需条件化 source drop 断言失败。
`cargo test -p lang-frontend --test ownership_iteration loop_carried_conditional_closure`
结果 0 passed / 3 failed / 65 filtered，日志 `/tmp/koven-0211-loop-edges-before.log`。这是
下一实现的红色回归，不能把存在某条无条件 source drop 当成通过；零轮与多轮运输仍需
后续动态验收。新增矩阵后的同一定向 strict clippy 通过（11s）。

两项 parser 复杂度失败已通过
`cargo test -p lang-frontend --lib parser::engine::tests:: -- --test-threads=1`
单线程复现：17 passed / 2 failed / 62 filtered，仍为 `4233 > 132 * 32` 与
`4505 > 132 * 34`；并非并发测试造成。本轮未改 parser，也未做干净 HEAD 对照，不能
宣称已证明回归来源。日志 `/tmp/koven-0211-parser-baseline.log`。未运行 workspace/native。

循环运输合同独立审查发现：相同 lambda AST 与 capture SymbolId 的不同动态环境可以
同时存活，布尔 origin 条件不足以定位析构实例。§6 已明确 captured drop、capture loan
end 与 pending capture 关联 owner value，并运输环境到实际 source-owner/loan 的关系；
仍被环境持有的局部 source 不因离开词法 scope 而丢失。此为下一实现前置，现有
DropTarget::Captured / EndCaptureLoan 尚未携带该关系，不把合同细化当作实现完成。
修补后独立复审确认上述合同缺口已闭合，实施缺口仍在。fmt、文档结构（371 Markdown）
与 diff 检查通过；当前没有本任务 Cargo 进程遗留，未提交本批实施改动，Goal 保持 active。

环境身份接入切片：ClosureOrigin 新增 owner value，lambda 形成后发布 CreateClosureOwner；
移动沿用原环境身份，条件结果保存快照时建立新值并保留已知 capture 来源。Captured drop
与 EndCaptureLoan 现在显式携带该 owner；来源合流按 owner/lambda 区分，不再仅按 lambda
合并。pending temporary 沿已有状态通路保留相同关联。shared capture 对实际 source-owner/
loan 实例的关系与循环 phi 尚未接入，不能据此关闭循环携带反例。

独立复审发现 opaque 参数/调用结果没有本地 lambda provenance，不能把 capture 输入当作
完整环境输入。CleanupOwnerSnapshot 现必填 value，绑定同一 Save 中已求值的完整 RHS；
capture_inputs 仅补充已知来源，opaque 路径可以为空。消费者不能重新求值 RHS 或读取已移动
binding。实际代码复审确认该非循环产物契约缺口已闭合，未验证 native。
新增回归核对条件环境移动后的目标 owner、临时 callee 的创建/loan end 对应关系，以及
混合 lambda/opaque 分支的完整 RHS 关联。首次因缺少新 API 编译失败，随后实现 API；
移动夹具曾因相邻 name 语句的解析边界失败，已改为合法声明序列，未据此宣称行为回归。
验证收口：`cargo test -p lang-frontend --lib --test ownership_iteration --test ownership_closures --test ownership_nullable_when --no-fail-fast`
完成，lib 79 passed / 2 failed（仍为上述 parser 复杂度项），iteration 68 passed / 3 failed
（仍为三条循环边），closures 13 passed，nullable_when 26 passed。新增三项环境身份回归
全部通过，日志 `/tmp/koven-0211-owner-identity-regression.log`。fmt、文档结构（371 Markdown）
与 diff 检查通过；无本任务 Cargo 进程遗留。当前环境身份 API 版本尚未重跑 codegen、
strict clippy、workspace/native，不能复用上一版本检查为本版本结论。切片超过仓库
10,000-token 预算，未提交改动；下一步补齐该 API 的定向验证，再接入 source-owner 关系与 phi。


2026-09-22 when alternative 与 source owner 版本续进：

- `when` 的每个 alternative 分别保存匹配/继续结果；合流保留此前均未命中的路径，
  不再重复建立 entry/隐式出口选择。源码反例曾读取未执行的后续 selector，修复后首项命中、
  后项命中与落入 else 三条路径均核对实际 source 析构；独立复审未发现新增问题。
- 该 when 切片定向条件代数 10 项通过；iteration 69 passed / 3 failed（原循环携带反例），
  closures 13、nullable_when 26 项通过。日志 `/tmp/koven-0211-when-choice-regression.log`。
  环境身份 API 的先前待验项已补：codegen guarded preflight 1 项及 frontend strict clippy 通过，
  对应 `/tmp/koven-0211-owner-identity-codegen.log`、`/tmp/koven-0211-owner-identity-clippy.log`；
  这两项早于本次 when/source 版本修改，不能当作其最终验证。
- 当前 binding 保留按 owner 定义区分的条件版本；参数、表达式、解构组件分别有值定义，
  Name 移动与 `!!` 运输既有身份。分支按同一定义合并条件，snapshot 同步重绑定版本条件；
  pending Value 实参在调用提交前保留版本。named drop 发布所选值定义，先完成全部环境清理
  和 capture loan end，再递归释放来源。`?` 的退出复制状态复用统一清理入口。
- 分支分别替换 source 后再被 closure 捕获的反例，修前错误合成 1 条无条件事实，修后保留
  两个不同定义并逐路径恰好析构一次。常量 temporary 规范化丢失 owner 的独立审查发现也已
  失败复现并修复。nullable 结果的验收改为不同定义同点清理、每条路径恰好一次，不能继续
  把静态 drop 条数当作动态次数。
- 本切片仍不含 capture 到实际 source-owner/loan 实例的完整关系、独立 live owner 总账或
  header/backedge/exit phi；三个循环携带反例继续保留，Spec 保持 in-progress。

本次 source 版本验证记录：

- `cargo test -p lang-frontend --test ownership_iteration --test ownership_checking --test ownership_containers --test ownership_nullable_when --test ownership_closures --no-fail-fast`：
  基础所有权 29、containers 12、closures 13 通过；iteration 71 通过 / 原循环 3 失败，
  nullable_when 25 通过 / 上述旧静态条数断言 1 失败。
- 修复常量身份规范化并调整 nullable 验收后，
  `cargo test -p lang-frontend --test ownership_iteration --test ownership_nullable_when --test ownership_constants --no-fail-fast`：
  iteration 74 通过 / 原循环 3 失败，nullable_when 26、constants 16 通过。新增版本用例全部通过，
  日志 `/tmp/koven-0211-source-version-final.log`；其余三个未受常量规范化影响的组复用前项结果。
- 独立复审要求 nullable 断言同时排除未选版本；加强为每个 arm 一个 Always、一个 Never 后，
  `cargo test -p lang-frontend --test ownership_nullable_when transferred_result_owner_gets_its_own_normal_drop`
  1 项通过。最终只读复审确认该盲区闭合；未将静态事实数当作执行次数。
- `cargo test -p lang-codegen --lib lower_frontend_tests` 首次 37 通过 / 1 失败，失败点为旧夹具
  `for (item in 1)` 的 Phase 2 诊断断言。改用合法 temporary source `listOf(1)`，并确认无
  typed/ownership 诊断且有一项 iteration plan 后，38 项通过；scalar consumer 仍明确拒绝
  未实现的 for native。日志 `/tmp/koven-0211-source-version-codegen-final.log`。
- `cargo clippy -p lang-frontend --lib --test ownership_iteration --test ownership_nullable_when -- -D warnings`
  通过。没有运行全量 frontend、native build/run 或重新执行已记录的 parser 复杂度失败项。
  `cargo check --workspace --all-targets` 通过（9m44s），日志
  `/tmp/koven-0211-source-version-workspace-check.log`。fmt、文档结构（371 Markdown）和
  diff 检查通过；本任务 Cargo 进程均已结束。本轮未提交，未关闭循环携带 P1。

2026-09-22 capture 实际来源关系续进：

- `CleanupCaptureInput` 在 lambda 形成前解析实际 source owner；Copyable/非 owning place 与
  紧邻 callable 环境槽分别表示。创建记录不可变；流动副本的条件随 owner availability 一起
  restrict、合流与快照复制。captured drop 和 capture loan end 发布相同来源关系；直接
  owned capture 的 drop 同时携带 source 值身份，不在清理时重新查询当前 SymbolId。
- source 被 move capture 后重新赋值的反例，修前 captured drop 没有 source owner，修后
  两个环境分别指向 `listOf(1)`、`listOf(2)`。嵌套 lambda 回归确认从紧邻环境槽捕获；
  条件输入回归确认创建条件不被后续快照原地覆盖，并逐路径核对选中的 source 值。
- 独立审查发现合流追加同槽版本会破坏逆槽析构顺序。模型反例先复现第二条路径错误地
  先清理 A 后清理 B；合流改按已检查的 capture 槽序分组，两条路径均按 B、A 清理。
  此为模型反例，不声称已复现非循环源码触发路径；修后定向单元测试 1 项通过，复核通过。
- 新增循环反例：`g` 接收前轮 `f`，`f` 捕获本轮局部 `xs`，循环后依次调用 `g()`、`f()`。
  语法夹具修正后解析、名称、类型及 ownership 诊断均为空，但 `g()` 后缺少 hidden source
  清理。独立复核确认该程序合法，两个出口角色应有独立运输值身份；零轮均无 source，
  一轮仅 `f` 持有 source，多轮各持前轮/当轮实例。当前断言仅建立失败证据，不能替代
  执行 phi 条件的完整验收。现有三条循环反例与此新反例均仍待修复。

本切片验证进度：

- `cargo test -p lang-frontend --test ownership_iteration capture`：21 项通过；
  日志 `/tmp/koven-0211-capture-source-targeted.log`。
- `cargo test -p lang-frontend --test ownership_iteration --test ownership_closures --test ownership_nullable_when --test ownership_checking --test ownership_construction --no-fail-fast`：
  基础所有权 29、closure 13、construction 6、nullable_when 26 项通过，iteration 77 通过 /
  原循环 3 失败。此命令早于上述槽序修复与 hidden source 新反例；
  日志 `/tmp/koven-0211-capture-source-regression.log`。
- 槽序修复前后同一模型测试分别失败/通过，日志
  `/tmp/koven-0211-capture-order-before.log`、`/tmp/koven-0211-capture-order-after.log`。
  hidden source 合法夹具仍失败，日志 `/tmp/koven-0211-hidden-source-before.log`。
- `cargo clippy -p lang-frontend --lib --test ownership_iteration --test ownership_nullable_when -- -D warnings`
  通过，日志 `/tmp/koven-0211-capture-input-clippy.log`；fmt 与 diff 检查通过。
- 最终 `cargo test -p lang-frontend --test ownership_iteration`：77 通过 / 4 失败，包含上述
  hidden source 新反例与原循环三项；无 ignored/filtered，日志
  `/tmp/koven-0211-capture-input-iteration-final.log`。该结果包含槽序修复。
- `cargo test -p lang-codegen --lib lower_frontend_tests`：38 通过 / 407 filtered；
  `cargo check --workspace --all-targets` 通过。日志分别为
  `/tmp/koven-0211-capture-input-codegen.log`、`/tmp/koven-0211-capture-input-workspace.log`。
  文档结构检查通过（371 Markdown）；未执行 frontend 全量、native build/run。
  本任务检查进程均已结束，改动仍未提交。

当前 source 关系仅封闭非循环捕获与快照运输；独立 live owner 总账和循环 header/backedge/exit
phi 仍未实现，不关闭 Phase 3，不提前宣布 temporary source 的首轮 native 已交付。

2026-09-22 至 09-23 循环 may-origin 固定点续进：

- 新增 callable 级纯预分析，有限域为 symbol 与已知 lambda identity；opaque 函数值不进入
  已知来源集合，不使用 cleanup 条件 DAG 或模拟执行轮数作为收敛标准。Name Consume、
  assignment strong update、控制结果交付、typed call mode 与 capture effect 参与运输。
- 入口与 fallthrough/continue 求 header 固定点；耗尽与 break 合为 exit，return/Nothing
  不提供回边。稳定 header 后统一发布嵌套循环摘要，顺序后继也使用前一循环的出口。
  不从当前仅遍历一轮的 drop ValueState 重算摘要。词法 alias 离开 scope 时移除，lambda
  body 使用独立 callable 状态与跳转流；nullable when 复用 typed alternative 可达域。
- `IterationOwnershipPlan.closure_flow()` 公开确定排序的 header/exit 可能来源，供后续 phi
  字段预分配；明确不是动态 owner 身份、availability 或可执行 cleanup。原四项动态运输
  反例继续保留，live owner 总账、选择/值的 phi incoming 与 native 范围没有缩减。
- 多轮 `h = g; g = f; f = lambda` 先复现空摘要失败，再由固定点传播至每个 binding。
  首四项测试通过，覆盖多轮、各类 jump/不可达 tail、nested/后续循环与局部 alias。
  日志 `/tmp/koven-0211-origin-fixedpoint-before.log`、`/tmp/koven-0211-origin-fixedpoint-cases.log`。
- 独立审查发现 Elvis 被普通二元 Read 清空结果。初始夹具有 nullable 函数类型语法错误及
  裸 null 缺少 expected type 的 L0083；它们不是行为证据。改用显式 `Nothing?` binding 后，
  禁用新增 Elvis 分流的旧行为二进制实际失败于缺少 RHS lambda 来源，日志
  `/tmp/koven-0211-origin-elvis-legal-before.log`。实现按 typed Nothing? 保留必走 RHS 的路径，
  其他 nullable 同时保留非空旁路；独立只读复核通过，最终测试待下方记录。
  该支持仅限来源预分析；主 checker/drop planner 仍将 Elvis 走普通二元 Read，尚未运输其
  closure 结果和旁路清理状态。接入执行 phi 前须统一这条交付路径，不能从摘要补造执行事实。
- 自查发现非发布的 probe 仍重复遍历已收敛 body；12 层无来源变化的循环实际执行 8190
  次 loop pass，计数回归失败。改为复用收敛轮 Flow，仅最终发布阶段重走稳定 header。
  耗尽状态必须保留 condition 完成、body 开始前的状态；独立复核确认等价，并新增
  while 无回边时条件赋值向后续 for 传播的验证。旧行为日志
  `/tmp/koven-0211-origin-probe-before.log`；最终计数与完整相关门禁尚待结果。

固定点最终定向验证：

- `cargo test -p lang-frontend --lib --test ownership_iteration loop_origin --no-fail-fast`：
  计数测试 1 通过 / 82 filtered，源码回归 7 通过 / 81 filtered，均无失败/ignored。
  12 层嵌套的实际 loop pass 满足 `2 * depth * depth`（288）上界；不使用墙钟阈值。
  日志 `/tmp/koven-0211-origin-fixedpoint-final.log`，包含 Elvis、nested callable 与 while
  condition/exhaustion 的最终实现。先前等待同一 target 的已有 check 进程及二进制启动，
  均持续追踪原句柄至退出 0，没有重启测试或把等待当通过。
- `cargo test -p lang-frontend --test ownership_iteration --test ownership_closures --test ownership_nullable_when --no-fail-fast`：
  iteration 84 通过 / 原四项动态运输失败，closure 13、nullable_when 26 项通过；无新增失败。
  日志 `/tmp/koven-0211-origin-regression.log`，命令因四项失败退出 101，不称整组通过。
- `cargo clippy -p lang-frontend --lib --test ownership_iteration -- -D warnings` 通过，日志
  `/tmp/koven-0211-origin-clippy.log`；本轮没有运行全量 frontend 或 native build/run。
- `cargo test -p lang-codegen --lib diagnostics_and_unsupported_bodies_fail_without_partial_programs`：
  1 通过 / 444 filtered，验证 SSA 接收与拒绝边界，日志 `/tmp/koven-0211-origin-consumer.log`；
  此项不构成 for native 执行成功的证据。
- `cargo check --workspace --all-targets` 退出 0，日志 `/tmp/koven-0211-origin-workspace.log`。
  当前切片与既有动态运输 WIP 均未提交；四项反例仍失败，Phase 3 和首轮 native 尚未完成。
- `cargo fmt --all -- --check`、`python3 scripts/check_docs.py`（371 Markdown files）与
  `git diff --check` 均通过；结构门禁不替代上述语义审查与行为验证。

2026-09-23 Elvis 实际交付续进（验证中）：

- 来源固定点已处理 Elvis，但 checker/drop planner 的普通 Binary Read 会漏掉实际交付。
  `cargo test -p lang-frontend --test ownership_iteration elvis_ --no-fail-fast` 在修改实现前
  得到 1 通过 / 3 失败：已有来源摘要通过，新增 capture source 延寿、非空后继 L0131、
  借用 closure 逃逸 L0137 均失败。日志 `/tmp/koven-0211-elvis-before.log`；这是行为反例，
  不是 parser/type 夹具错误。
- 开始将单文件 checker、planner 与 liveness 分开处理 non-null/null 两边；左侧只求值一次，
  `Nothing?` 不创建非空后继，RHS 按控制结果交付契约处理。此项只修复 frontend，
  不改变 guide 中 Elvis native 延后的边界，也不缩减 temporary iteration source 的首轮范围。
- 独立只读审查指出 Copyable element 背后的 temporary container 会被 Index Place 路径漏掉；
  `pass(xs)[0] ?: return` 的两条退出清理回归实际失败，临时容器没有任何 drop。
  修复在分支前登记最外层 backing owner，区分它与 Copyable element 的结果身份；
  兼测 `(pass(xs)[0])[0]` 的多层索引。第一次修复后定向回归 8 通过 / 1 失败，
  日志 `/tmp/koven-0211-elvis-after.log`；再次修复的相关回归见下方记录。
- `cargo test -p lang-frontend --lib --test ownership_iteration --test ownership_closures --test ownership_nullable_when --test ownership_containers --no-fail-fast`：
  closure 13、container 12、nullable 26 项通过；iteration 92 通过 / 原四项动态运输失败，
  新增 Elvis 用例全部通过。lib 81 通过 / parser 线性计数两项失败（4233 > 4224、
  4505 > 4488）；parser 源码未修改，失败原因仍需独立复核，不能称整组通过。
  日志 `/tmp/koven-0211-elvis-regression.log`。未运行全量 frontend 与 native build/run。
- 直接复用该次构建的同一 lib 测试二进制，以 `stay_linear --test-threads=1` 运行 6 项：
  4 通过 / 同两项 parser 计数失败，数值仍为 4233 与 4505。日志
  `/tmp/koven-0211-elvis-parser-linear-recheck.log`。这排除了并行偶发波动，
  但没有证明 parser 基线何时开始失败；当前改动未触及 parser，仍须如实保留失败记录。
- `cargo clippy -p lang-frontend --lib --test ownership_iteration -- -D warnings` 通过，
  日志 `/tmp/koven-0211-elvis-clippy.log`；`cargo check --workspace --all-targets`
  退出 0，日志 `/tmp/koven-0211-elvis-workspace.log`。
- `cargo test -p lang-codegen --lib diagnostics_and_unsupported_bodies_fail_without_partial_programs`：
  1 通过 / 444 filtered，日志 `/tmp/koven-0211-elvis-consumer.log`。新增有效
  `input ?: 0` 的 frontend typed/owned 产物无诊断、无 deferred；当前 native consumer
  明确返回 `UnsupportedNode`，符合 guide 的 Elvis native 延后边界。此项不代表 Elvis
  native 已实现。
- 独立只读复核先发现 Index 临时 backing 漏清理并由回归证实，修复后再次检查
  non-null/null-return、嵌套索引、外围 pending loan 与结果交付，未再发现当前受支持路径的
  必修缺口。临时 Member receiver 仍发布 `MemberReceiver` deferred 并被 SSA 阻断，
  不能据此声称支持该投影。当前切片未单独提交：同一工作树内尚有四项循环动态运输失败。

2026-09-23 循环 phi 静态布局续进（Phase 3 未闭合）：

- `CleanupSelectorSource::IterationPhi` 以 statement、header/exit 与稳定槽序独立标识来源
  存在位；`CleanupOwnerValue::IterationPhi` 为两个边界分别分配 owner 身份。它们在规划 body
  的控制选择前预分配，并随 `IterationOwnershipPlan.closure_phis()` 公开。当前没有任何入边
  写入 selector 或 owner，不能执行 phi、drop 或 capture-loan 清理。
- callable 预分析把 MoveOnly binding 可用性与已知 lambda 来源分开；空来源仍保留参数、局部、
  解构与 opaque 函数 owner 的 phi 槽。Lambda body 加入显式和隐式 owned 参数；shared capture
  只传递已知来源，不获得本 callable 的 owned 槽。独立复审发现隐式 `it` 漏槽与 shared capture
  误占槽，两条行为回归先分别失败，日志 `/tmp/koven-0211-phi-review-red.log`、
  `/tmp/koven-0211-phi-shared-red.log`；最初五条布局通过，日志
  `/tmp/koven-0211-phi-review-green2.log`。最早两条 opaque 用例的测试曾被同一 Cargo target
  的已有 check 长时间阻塞，取消等待后改用独立 `/tmp/koven-0211-phi-target` 验证；不把该次
  等待记为失败测试证据。
- 最终复审发现 Elvis 预分析在分流前 Consume lhs，使 null RHS 内的 loop 丢失仍合法可读
  的 owned root。新增用例先失败于 header 缺槽，日志 `/tmp/koven-0211-phi-elvis-red.log`；
  lhs 改为 Place 单次求值，只有非空交付边 Consume root，null RHS 继续使用原可用性。
  六条布局用例通过，日志 `/tmp/koven-0211-phi-layout-all-final.log`。
- `cargo test -p lang-frontend --test ownership_iteration` 在独立 target 运行：98 通过、原四项
  loop-carried 动态运输反例失败，无 ignored；日志 `/tmp/koven-0211-phi-iteration-final2.log`。
  `cargo clippy -p lang-frontend --lib --test ownership_iteration -- -D warnings` 首次发现测试中
  `Iterator::last` lint，改为反向查找后最终通过，日志 `/tmp/koven-0211-phi-clippy-final2.log`。
  `cargo check -p lang-codegen --all-targets` 通过，日志 `/tmp/koven-0211-phi-codegen-check.log`；
  `python3 scripts/check_docs.py`、`git diff --check` 与定向 rustfmt 检查通过。独立最终复审未再
  发现静态 phi 可用性的可复现缺口。未运行 frontend
  全量测试、native build/run；动态 incoming、live owner 总账及 temporary source 首轮 native
  仍待实施，Spec 保持 in-progress。

2026-09-23 循环 phi 捕获来源关系续进（仍为静态布局）：

- 同一 lambda AST 的环境可在不同轮同时存于 `f/g`，不能只用捕获 `SymbolId` 指向当前 binding。
  每个 header/exit、binding、已知 lambda 捕获各预分配独立 `IterationPhiSourceOwner`，明确关联
  其环境 phi、lambda 和 capture source。`Shared/Borrow` 只引用 source owner，后续应结束
  capture loan；`Owned/Move` 转移 source，后续应析构 captured value。Borrow element/component
  和其他无 owned source 的 `Place` 不分配该 owner 槽；捕获顺序沿用前端已确认顺序，不按
  SymbolId 重排。当前不生成入边写入或执行动作，也未封闭 `Environment` 嵌套槽运输。
- 独立审查起初建议删除 Shared 槽；核查现有 `CleanupCaptureInput::Owner`、source 延寿和
  `EndCaptureLoan` 后修正为保留其**来源 owner 关系**，并将 API 从 captured owner 改名为
  source owner，避免暗示 shared closure 取得所有权。审查同时指出第 3 节“borrow capture
  本轮释放”的措辞过宽，现明确该限制针对 element/component Borrow binding；普通 body-local
  owned source 可以被仍存活的 shared closure loan 延寿。
- 多环境槽回归首次因缺 API 编译失败，日志 `/tmp/koven-0211-phi-capture-layout-red.log`；
  初版又把 Borrow outer element 当 source owner，合法嵌套循环回归实际失败，日志
  `/tmp/koven-0211-phi-borrowed-capture-red.log`。修复后 10 条布局用例通过，涵盖
  Shared/Borrow、Owned/Move、Borrow element、同源多环境和 capture 顺序，日志
  `/tmp/koven-0211-phi-source-layout-final2.log`。
- 定向 `ownership_closures` 13 项通过；`ownership_iteration` 102 项通过、原四项动态运输
  反例失败，无 ignored，日志 `/tmp/koven-0211-phi-source-regression-final.log`。strict Clippy
  与 `cargo check -p lang-codegen --all-targets` 通过，日志分别为
  `/tmp/koven-0211-phi-source-clippy-final.log`、`/tmp/koven-0211-phi-source-codegen-final.log`。
  未运行 frontend 全量或 native build/run；本轮未提交未闭合的 Phase 3 工作树。

2026-09-23 循环 phi 入边事实续进（尚不可执行）：

- 在 header/exit 的每个 MoveOnly binding 增加独立可用位。`IterationPhiIncoming` 记录边种类、
  执行点、可达条件，以及按目标 binding 排列的 owner 值、lambda 来源与 capture source 关系；
  各来源均读取写入前状态。入口从实际 source 求值后的 `ValueState` 取值，fallthrough、continue、
  break 从各自清理后的状态取值；缺席来源明确写 false。耗尽从 header phi 转发到 exit，不重新
  读取入口 binding。上述记录只是 producer 入边事实，未产生执行该复制的 SSA/LLVM。
- 独立复核发现耗尽边先记录后清理的错误：同一 `LoopExit` 析构的 owner 会被送到 exit。新增
  dead-owner 用例先失败，随后在清理后记录，并对 break/耗尽按循环后继 liveness 筛掉已死
  binding；由存活 closure 借住的 source 仍作为 capture 关系运输，不伪装为可用 binding。
  复核又指出纯 liveness 会误滤外围 pending call 和 replacement RHS 保持的 owner；对应
  pending-call 用例先失败，随后与 `drop_named` 共用上下文保护判定，并在 exit 同时检查
  清理后状态。replacement RHS 用例亦已覆盖。
- 17 条 `loop_phi` 用例通过，覆盖入口真实 owner、条件来源互斥、三种 body 边、header→exit
  耗尽转发、已死 owner 过滤及外围调用/赋值保护。`ownership_closures` 13 项通过；`ownership_iteration` 109 项
  通过、原四项动态循环反例仍失败，无 ignored，日志
  `/tmp/koven-0211-phi-incomings-final2-regression.log`。body 当前仍克隆入口状态，尚未从 header
  phi 重建；`g = f; f = lambda` 的下一轮来源及旧 source owner 总账、exit phi 后继状态、
  并行复制执行和 temporary source native 均未闭合。未运行 frontend 全量或 native build/run。
  共享清理逻辑的 `ownership_nullable_when` 26 项通过；strict Clippy、
  `cargo check -p lang-codegen --all-targets`、定向 rustfmt、文档结构检查及 diff 检查通过。
  独立复核确认局部 exit 过滤修正后未发现新的可复现缺口，复核未运行 Cargo。

2026-09-23 header/exit 符号状态与 captured source 总账检查点（未闭合）：

- body 改从预分配的 header phi 建立 `ValueState`，下一轮 `g = f` 可读取上一轮的环境及
  capture source 关系；耗尽清理后从 exit phi 建立后继状态。跨出局部 scope 的 shared source
  以独立 `RetainedSource` 义务延寿，closure 释放时结束 capture loan 并发布带动态 source
  owner 槽的清理事实。新增两条 header/backedge 来源回归曾先失败，修正后通过；先前四条
  loop-carried 反例在当前事实层通过，但不代表 SSA/LLVM 已执行 phi 运输。
- 修正后定向 `cargo test -p lang-frontend --test ownership_iteration --test ownership_closures
  --no-fail-fast` 通过 iteration 115、closure 13 项；`ownership_nullable_when` 26 项、
  strict Clippy 和 `cargo check -p lang-codegen --all-targets` 通过。以上均为随后新增的共享
  source 反例之前的结果；未运行 frontend 全量或 native build/run。
- 独立复核发现同轮 `f/g` 同时共享捕获局部 `xs` 时，两个 phi source 槽可能指向同一实际
  source 实例；当前总账按槽分别建立义务，`f()` 释放时会在仍存活的 `g` 借用下过早发布
  `RetainedSource(xs)`，`g()` 又可能重复发布。新增
  `loop_carried_sibling_closures_release_their_shared_source_once` 合法源码回归：ownership
  诊断为空，但单项测试失败于 `f()` 不应析构 `xs` 的断言（115 项被过滤）。不同轮同名
  source 又必须保持实例独立，不能简单按 `SymbolId` 合并。下一步须为入边捕获关系保留
  实际 source 实例的共享身份，并在最后一条有效 capture loan 结束后恰好析构一次；当前
  producer 事实未满足该条件，Phase 4 对 guarded/retained source 仍明确拒绝，native 不可执行。

同日后续修正：保留每个环境的独立 source owner 槽，在其 `EndCaptureLoan` 之后新增
`TestLastCaptureLoan`，以**槽中运行时指向的 source 实例**查询剩余 capture loan；
`RetainedSource` drop 仅在该选择为真时执行。同点动作顺序为结束 loan、测试最后借用者、
条件 drop。这样同轮共享 source 可延迟到最后释放，不同轮同名 source 仍保留独立实例；
Phase 3 只发布所需事实，运行时计数/选择值及 phi 并行复制尚未实现。上述红例改为检查
drop 受同一 source 槽的 last-loan selector 保护且测试动作先于 drop，单项通过；
`ownership_iteration` 116、`ownership_closures` 13、`ownership_nullable_when` 26 项通过，
strict Clippy 与 `cargo check -p lang-codegen --all-targets` 通过。仍未运行 frontend 全量或
native build/run，不能将事实层的静态回归视为动态恰好一次析构验收。Phase 4 preflight
现也显式拒绝尚无运行时选择值的 `TestLastCaptureLoan`；定向 codegen 回归验证合法双 closure
循环在 lowering 前以 `UnsupportedNode` 返回对应 capture source 范围。

2026-09-23 入边事实回放检查点：针对 `g = prior; f = { read(xs) }`，测试按实际入口、
两轮 fallthrough 或 continue、耗尽的顺序执行 selector snapshot 与 phi incoming；每条边
先读取旧选择和 source owner，再并行提交目标槽。零轮的 exit 无 captured source；两轮后
`g` 保留第一轮 `xs`，`f` 持有第二轮 `xs`，耗尽再把两个实例分别运到 exit。
同轮 `f/g` 共享同一个 `xs` 的 break 入边也验证两个独立 capture 槽指向同一实际来源，
并按来源实例回放两次 loan 结束所需的 2→1→0 计数。`ownership_iteration` 117 项通过，
定向 strict Clippy 通过。这是 Phase 3 **测试内的事实回放**，尚未执行 SSA/LLVM、native
drop、provider 或 temporary source；SPEC-0211 的动态集成验收继续未勾选。

同日回放判据复核：`SaveOwnerSnapshot` 现在按实际动作的可达条件和
`AfterExpression(value)` 执行点检查；`f` 的 RHS 保存必须先于同点提交。break 入边的所有
selector 先读取旧状态，再一起写入。共享 source 的 `f/g` 两种调用顺序分别按 exit phi
选中的环境、capture source 和实际 source 槽匹配 `EndCaptureLoan`，回放 2→1→0 的
last-loan 选择与受保护的唯一 drop；不再仅凭相同 source 槽计数。以上仍是测试内回放，
不是 native 执行；Phase 4 的 selector、动态 source 身份与 temporary source native
尚未实现。

独立复核又指出入边回放漏读 binding owner 值：清空 `values()` 时旧测试仍可通过，
但 header/exit 并无可调用环境。现与 selector、capture source 一起从入边旧状态并行
复制 owner 值，要求可用 binding 恰有一个值，且选中的 capture origin 指向同一动态环境；
零轮保留初始 closure，两轮后 `g/f` 分别保留第一/第二轮环境。
回放还核对 snapshot 的 RHS 与保存动作一致，且新 `f` 环境来自该轮实际发布的
`CreateClosureOwner` capture 输入，避免测试凭 snapshot 顺序虚构新环境。
本轮新 `xs` 的 owner 也从该 closure 的捕获输入独立读取，并与 backedge `f` 的
source 输入核对；回放不再把待验证的入边字段自身当作正确来源。

此检查点运行 `cargo test -p lang-frontend --test ownership_iteration`：118 项通过，
0 失败、0 ignored；`cargo clippy -p lang-frontend --test ownership_iteration -- -D warnings`、
`python3 scripts/check_docs.py`（371 份 Markdown）和 `git diff --check` 通过。
未运行 frontend 全量、codegen 测试或 native build/run。
追加 source 独立交叉校验后，定向 `cargo test -p lang-frontend --test ownership_iteration
loop_phi_` 20 项通过，strict Clippy、文档结构检查与 diff 检查再次通过；未重复运行
`ownership_iteration` 全套 118 项。

2026-09-23 确定性检查点：对同时包含条件 `continue`、`break` 和跨循环 shared closure
捕获的合法源码，在同一 `SourceMap`、解析与类型分析上重复运行 ownership；公开 iteration
plan、条件表、清理动作、drop/loan/capture facts 逐项相等，定向 1 项通过。初版夹具误用两份
`SourceMap` 做结构比较，因内部 `SourceId` 的 map 身份不同而失败；修正夹具后通过，
不把这次夹具失败记为生产缺陷。此证据仅覆盖该源码的前端确定性，不替代其他验收矩阵。
定向 strict Clippy、文档结构检查（371 份 Markdown）及 diff 检查通过；未运行 frontend
全量或 native build/run。

同日 temporary source 补充：合法 `for (_ in if (flag) listOf(1) else listOf(2))`
定向验证 `LoanTarget::Temporary` 指向整个 source 结果；两个分支的构造 temporary
均交付给该结果，不在 provider 取得前被独立析构。零轮耗尽与 body `break` 各在本身
退出点发布一次 source drop，定向测试 1 项通过。这仍是 Phase 3 清理事实，未验证
source 精确求值一次的 native 行为。

同日共享契约门禁：新增 `RetainedSource` 后，`ownership_checking` 与
`ownership_construction` 两个旧测试在枚举 `DropTarget` 时编译失败；仅在其“具名析构来源”
分类中排除该非 named 目标，未改变生产行为。随后一次运行 frontend 六个相关 integration
suite（iteration 120、closures 13、checking 29、containers 12、nullable_when 26、
construction 6），共 206 项通过、0 failed、0 ignored。
`cargo check -p lang-frontend --all-targets` 又发现内部 snapshot 单测仍把新 selector
`control()` 方法当字段；修正测试后全目标编译通过，snapshot 定向 6 项通过，
`cargo check -p lang-codegen --all-targets` 通过。上述均为 Phase 3 前端/消费端编译与
事实测试；尚未运行完整 frontend 测试集合、native build/run 或动态 phi/last-loan 执行。
定向 strict Clippy（iteration/checking/construction）、四份受影响 Rust 文件的 rustfmt
检查、文档结构检查（371 份 Markdown）及 diff 检查通过。

同日补充内部事实与消费端边界验证：`cargo test -p lang-frontend --lib
ownership_checking::cleanup_condition::` 10 项通过、73 项过滤；`cargo test -p
lang-frontend --lib ownership_checking::checker::drop_planner::` 2 项通过、81 项过滤；
`cargo test -p lang-codegen --lib lowering_bindings` 2 项通过、444 项过滤。codegen
回归确认 guarded cleanup 和循环携带的 last-capture-loan 在 lowering 前显式拒绝；这不证明
动态运输或 native 析构已完成。第 9 节记录了阶段验收边界冲突，决定前继续保持 in-progress。

同日 temporary source 控制结果补充：原 `if` 回归扩展到 `when` 两条构造分支；两者均要求
分支结果交付给完整 source temporary，不在取得 provider 前析构，并分别在零轮耗尽与 body
`break` 发布 source drop。定向 `cargo test -p lang-frontend --test ownership_iteration
conditional_temporary_source_survives_until_each_provider_exit` 1 项通过、119 项过滤。
此为前端事实覆盖，source 求值一次及实际析构仍待 native 验证。

同日外层控制分支守卫修正：新增 `when` source 分支内 `return` 回归，确认该路径不建立
provider，也不发布其 source 的退出析构，定向 1 项通过。独立只读评审随后发现 `for`
处于外层 `if` 时，phi 派生状态丢失外层路径守卫，耗尽入边还被无条件发布；外层分支
未执行时，后继 closure 清理可能读取未初始化的循环 selector。新增 `flag=false` 回归先
失败于耗尽入边的 `ALWAYS` 条件；修正后 phi owner/closure/capture 状态及耗尽入边均保留
外层路径条件，该回归通过。六个受影响 frontend integration suite 一次运行通过：
iteration 122、closures 13、checking 29、containers 12、nullable_when 26、construction 6，
合计 208 项，0 failed、0 ignored。此证据只验证前端条件事实与共享契约，未执行 SSA 或 native。
独立只读复核未发现路径守卫修复的具体遗漏；按其剩余风险提示，回归又覆盖调用点的
capture-loan 动作条件在跳过循环时不读取 phi selector，定向 1 项通过、
121 项过滤。`cargo clippy -p lang-frontend --lib --test ownership_iteration -- -D warnings`、
定向 rustfmt、`python3 scripts/check_docs.py`（371 份 Markdown）与 `git diff --check` 通过；
扩充上述回归后又运行 strict Clippy 与文档/格式检查，未重复六套 suite。

同日多轮 source 实例清理回放：在已有两轮 fallthrough/continue → exhaustion 的 phi 测试中，
继续按 `g()`、`f()` 的实际调用顺序执行 exit source 槽对应的 capture loan end、
`TestLastCaptureLoan` 与受保护 drop。第一轮和第二轮的同名 `xs` 分别保持独立实例，
每次调用恰执行一条属于其实例的 retained-source drop；零轮时同一静态清理事实均不执行，
且不读取未初始化的 last-loan selector。`cargo test -p lang-frontend --test
ownership_iteration loop_phi_` 定向 21 项通过、101 项过滤。此为测试内事实回放，
仍不证明真实 SSA/LLVM 或 native 动态清理。
独立只读复核指出初版只计 `DropFact`，重复发布同一 `Drop` 动作仍可假通过；现改为遍历
每个 `CallReturn` 点实际启用的全部 `IterationCleanupAction::Drop`，按已运输的 source 实例
计数后重跑上述 21 项，全部通过。复核又指出只检查调用点会漏掉循环体或耗尽点的提前
析构；直接禁止所有非调用点静态事实会误拒绝未启用的 guarded drop，已改为在每轮 snapshot
及零轮/两轮耗尽点按当时 selector 状态求值，并拒绝本夹具中未回放的其他非调用点。
再次运行 `loop_phi_` 21 项通过。此回放仍只覆盖所列源码与路径，不作为完整动态集成验收。
后续复核还指出“允许所有 `CallReturn`”会漏掉 lambda 内部 `read(xs)` 调用点的提前 drop；
现只允许已回放的外层 `g()`/`f()` 调用点，其他未回放点无论种类均拒绝。再次运行
`loop_phi_` 21 项通过。
最终针对性复核指出按 `RetainedSource` target 单独筛选会漏掉被错误标为具名或 captured
的同一 source drop；回放现同时按实际 source owner 槽、具名 `xs`、capture source 与构造值
来源筛选清理动作，再次运行 `loop_phi_` 21 项通过。该夹具的动态路径已覆盖这些 source
实例的 body、耗尽及两次调用点，但其他程序路径仍未据此宣称完成。

同日 element-derived borrowed closure 生命周期补齐：先以外层 `f` 在 `for` body 捕获本轮
`n`、`break` 后调用的失败回归确认 checker 曾给出零诊断；随后在正常回边、`continue`、
`break` 合流前，按边上活性拒绝仍持有该 shared capture 的 closure，产生 L0137，禁止发布
iteration/cleanup plan。本轮 body 内调用并释放的闭包保持合法。独立复核再给出 `f` owned
捕获旧 borrowed `g`、`g` 随后重赋的反例：形成时的 closure 来源现随环境保留，并在释放时按
实际持有链结束 loan；该反例由零诊断转为 L0137。`return f` 的同类路径另以失败回归复现，
L0137 逃逸检查现沿已形成环境的来源快照递归查询；直接交付的 lambda 在形成前按当前 capture
来源查询。旧 `g` 形成时没有已知 closure 来源、之后重赋为 borrowed closure 的合法对照通过，
避免把当前值误当旧快照。独立只读复核未发现新具体阻断问题。

验证：`cargo test -p lang-frontend --test ownership_iteration --test ownership_closures
--test ownership_checking --no-fail-fast` 分别 127、13、29 项通过，均 0 failed/ignored；
仅追加测试夹具后，空快照定向 1 项通过、127 项过滤，未重复三个完整 suite。
`cargo clippy -p lang-frontend --lib --test ownership_iteration -- -D warnings`、
`cargo fmt --all -- --check`、`python3 scripts/check_docs.py`（371 份 Markdown）及
`git diff --check` 通过。本轮未运行 frontend 全量、codegen 或 native build/run；
Phase 3 动态验收边界仍按第 9 节待定，SPEC-0211 保持 in-progress。

2026-09-23 嵌套环境清理检查点（未闭合）：外层 `f` owned move 捕获内层 `g`，`g` shared
捕获迭代元素时，原计划仅析构 captured `g`，未在 `f()` 返回点结束 `g` 的 capture loan。
失败回归确认后，形成 `f` 时保存被移动的内层环境来源，条件快照和分支合流递归携带；释放
`f` 时先递归清理 `g` 的捕获，再析构 `f`。条件 `g` 仅在选中分支持有元素 loan 的回归通过。
独立复核又指出分支局部 owned source `xs` 被 `g` 借住、`g` 被 `f` 移动时，旧 source
保活检查只看 `f` 的直接捕获，可能在分支结束提前析构 `xs`；失败回归中 `f()` 返回点缺少
source drop，递归检查内层 shared capture 后，该回归确认先结束 loan 再析构 `xs`。

同一复核发现循环 phi 重建仍将内层环境来源置空：合法的外层 `n` 经 `g`、`f` 保留，
经过空内层 `for` 后调用 `f()` 缺少 `g → n` 的 `EndCaptureLoan`，定向失败回归已复现。
该回归暂以明确原因 ignored，等待为嵌套环境预分配并填入动态 phi 来源，不能用零轮入口
快照冒充后续轮次的来源；因此本检查点不满足 SPEC-0211 的动态运输验收，也不触发
SPEC-0212/0182 的完成依赖。

本检查点定向 `cargo test -p lang-frontend --test ownership_iteration --test ownership_closures
--test ownership_checking --test ownership_nullable_when --test ownership_containers
--test ownership_construction --no-fail-fast`：iteration 130 通过 / 1 ignored，其余分别 13、29、
26、12、6 项通过，均 0 failed。该 ignored 回归单独运行时按预期失败，缺少 `f()` 的
`EndCaptureLoan`，不能计入通过。`cargo clippy -p lang-frontend --lib --test ownership_iteration
-- -D warnings`、`cargo fmt --all -- --check`、`python3 scripts/check_docs.py`（371 份
Markdown）和 `git diff --check` 通过。未运行 frontend 全量、codegen 或 native build/run。

同日兄弟环境共享 source 的复核：`f` owned 捕获同时借用 `xs` 的 `g/h`，首次合法
测试夹具修正 Koven 块语法后复现 `f()` 在第二条 capture loan 结束前析构 `xs`。
改为同一次外层环境清理中汇集 shared source，在全部内层 loan 结束后统一发布具名 source
drop。进一步的 exit phi 反例中，`g/h` 两个静态 source 槽指向同一实际 `xs`；原汇集方式
先结束两条 loan 再写两次 `TestLastCaptureLoan`，两次都可能读到零而重复 drop。
新增回归确认两个槽的 break 入边指向同一 owner，并先失败于查询次序；现每条 loan 结束后
立即写入其 last-capture 选择，guarded drop 候选仍延后至全部内层 loan 之后。独立只读复核
未发现该次修正的新具体反例；动态 phi 嵌套来源的 ignored 回归仍未解决。

最新定向六组 `cargo test -p lang-frontend --test ownership_iteration --test ownership_closures
--test ownership_checking --test ownership_nullable_when --test ownership_containers
--test ownership_construction --no-fail-fast --quiet`：iteration 132 通过 / 1 ignored，其余
分别 13、29、26、12、6 项通过，合计 218 通过、0 failed、1 ignored。

2026-09-23 嵌套 phi 来源切片：有限 origin 分析记录 owned move 捕获形成时的已知内层
lambda 来源，header/exit 为内层来源与其 source 递归预分配独立槽；entry、回边及耗尽入边
按实际 captured owner 记录或转发来源，seed 从 phi 重建嵌套环境。先前 ignored 的
`g → n` element loan 回归恢复执行，`f()` 返回点确有 `EndCaptureLoan`；附加断言核对
entry 与 exhaustion 的内层 selector、环境 owner 和 header→exit source 关系。递归捕获环
回归先复现被截断的 plan 仍发布，现返回明确 deferred，并确认 iteration、cleanup、drop、
loan-end 事实一并不发布。独立只读复核发现的半成品 drop 事实泄漏已据此修复。

修复后定向六组同上命令：iteration 134、closures 13、checking 29、nullable_when 26、
containers 12、construction 6，合计 220 项通过、0 failed、0 ignored。
`cargo fmt --all -- --check`、`python3 scripts/check_docs.py`（371 份 Markdown）和
`git diff --check` 通过。上述测试验证前端事实；嵌套条件来源的零轮、多轮、break/continue
事实回放、SSA/LLVM 动态运输及 native build/run 均未运行，SPEC-0211 继续 in-progress。
随后只收拢 phi 分配与 seed 函数的参数上下文以满足 strict Clippy；最终代码状态下
`cargo clippy -p lang-frontend --lib --test ownership_iteration -- -D warnings` 通过，
`cargo test -p lang-frontend --test ownership_iteration --quiet` 再次 134 项通过、0 ignored。
`cargo check -p lang-codegen --lib` 通过，验证下游库可编译，不代表 SSA/LLVM 运行验收。
其余五组没有在该参数组织调整后重复运行。

2026-09-23 嵌套来源动态回放补充：`f` owned 捕获 `g`，`g` shared 捕获每轮新建的
`xs`。新增零轮、两轮 fallthrough/continue 与单轮 break 的事实回放，分别核对
entry/body/exhaustion 或 break 的 phi 来源、每轮独立的 `f/g/xs` owner 值、`g` 与 `f`
求值边界的 snapshot selector 复制，以及调用点的 capture loan end、最后借用者测试与
source drop。第二轮替换在 phi 覆写前还检查第一轮 `xs` 实例的释放，未初始化 selector
直接使测试失败；独立复审发现 snapshot 动作值可指错表达式的假阳性，已补核对 action
value、snapshot value 和 `AfterExpression(value)`。

`cargo test -p lang-frontend --test ownership_iteration`：137 项通过、0 failed、0 ignored。
`cargo clippy -p lang-frontend --lib --test ownership_iteration -- -D warnings`、
`cargo fmt --all -- --check`、`python3 scripts/check_docs.py`（371 份 Markdown）及
`git diff --check` 在最后一处测试断言追加后复验通过。
本回放只覆盖所列前端事实路径，其他 Phase 3 验收仍未闭合；未运行 SSA/LLVM 或 native，
SPEC-0211 保持 in-progress。
此前检查点所记的阶段验收边界待定现按第 9 节解决，不改变那些检查点当时的测试结果。

同日条件来源跨轮回放补充：对循环体 `f = if (flag) ({ read(xs) }) else ({ read(ys) })`
分别执行 true→false 与 false→true 两轮。回放先确认 Save 动作的 guard 成立，再按 snapshot
的并行 selector 复制读旧状态，
再检查旧 `f` 的 capture loan end 指向上一轮闭包和实际 source owner，最后复制回边 phi。
`xs/ys` 在循环体都可能被下一轮捕获，因此测试拒绝在 body 提前发布任一 source drop，
并限制 source capture-loan end 与最后借用者查询只能在相应清理边界发生；
LoopExit 只析构未持有实例，最终 `f()` 结束另一实例的 loan 后恰好析构一次。独立复审
指出只核对 source 名称、忽略 owner、提前 drop/loan end 或无条件模拟 Save 的假阳性，
均已补成按动态实例、动作 guard 和执行点核对的断言。最后只读复核未发现该两轮夹具内的
具体阻断问题；其他程序路径未由此覆盖。

最后修改后 `cargo test -p lang-frontend --test ownership_iteration --quiet`：138 项通过、
0 failed、0 ignored；`cargo clippy -p lang-frontend --lib --test ownership_iteration -- -D warnings`、
`cargo fmt --all -- --check`、`python3 scripts/check_docs.py`（371 份 Markdown）与
`git diff --check` 通过。仍为前端事实回放，其他 Phase 3 验收和 native 仍未完成。

2026-09-23 递归环境运输方向确认：用户选择保留 v0.37 合法性，按第 9 节有限来源图与动态
环境实例链推进。新增双 lambda 交替 owned 捕获的回归，证明当前在静态环上给出
`RecursiveClosureCapture` deferred，且不发布截断的 iteration/cleanup/drop/loan-end 产物；
这只是实施前的安全边界，递归图运输尚未实现。ADR-0009 的内联 native 环境不能承载无界
链，native 表示须由后续新 ADR 决定；SPEC-0182 首轮 temporary source 范围不变。
`cargo test -p lang-frontend --test ownership_iteration --quiet`：139 项通过、0 failed、
0 ignored；`cargo clippy -p lang-frontend --lib --test ownership_iteration -- -D warnings`、
`cargo fmt --all -- --check`、`python3 scripts/check_docs.py`（371 份 Markdown）及
`git diff --check` 通过。未运行 frontend 全量、SSA/LLVM 或 native build/run。

2026-09-24 receiver/source 身份复核：单文件 `this.field` 与裸字段现映射同一字段 owner，
迭代 source 的 shared loan 因而保护两种拼写；当前 member receiver mode 同时约束字段写入、
显式和隐式 Inout 调用，borrowed lambda 中的 `this` 只按共享能力使用。独立复核进一步
复现共享捕获 local 被独占借用的漏洞，现由通用 `ExclusiveLoan` 入口拒绝；隐式 member call
从 typed `ImplicitThis` receiver 形成 `this` capture，避免误当无捕获 closure。单文件和
compilation-unit 均加入 Borrow/Value、shared/owned capture 及 `StaticSelf` 条件 Value
交付回归。该修复只收紧前端合法性与事实身份，不代表循环动态环境链或 native 已完成。
独立复核指出 compilation-unit 对 If/When 返回带共享捕获 closure 的逃逸检查遗漏；失败
回归确认后，返回检查现传至实际分支尾值，`if` 与 `when` expression-bodied 用例均报 L0137。
另一个失败回归确认 Elvis 右侧直接交付的 borrowed closure 曾漏报，现返回入口检查两侧
可能交付的直接操作数；此修复未改变 unit 既有 Elvis 求值数据流。
显式 `return if` 的追加夹具在类型阶段报 L0087，未计入所有权验收。
分层复验中 `ownership_iteration` 142、`ownership_checking` 29、`ownership_nullable_when`
26、`ownership_containers` 12、`ownership_construction` 6 项通过；修正仅影响测试夹具的
closure 类型和合法性预期后，`ownership_closures` 15、`multifile_ownership_checking` 71
项分别复验通过，合计 301 项通过、0 failed、0 ignored。七组使用独立临时 Cargo target，
避免与编辑器的构建锁争用；最后的 unit control 改动后仅重跑该 unit 套件，其他六组未重复运行。
`cargo clippy -p lang-frontend --lib --test ownership_iteration
--test ownership_closures --test multifile_ownership_checking -- -D warnings`、
`cargo fmt --all -- --check`、`python3 scripts/check_docs.py` 和 `git diff --check` 通过。
未运行 frontend 全量、codegen 测试或 native build/run；SPEC-0211 继续 in-progress。

2026-09-24 guarded construction 清理复核：`Holder(if (select) { "a" } else { "b" },
if (early) { return } else { 1 })` 的首个 MoveOnly 输入在两个分支形成不同 owner 版本；
后续输入提前 return 时，Phase 3 为待交付 temporary 发布两条带条件的 ControlTransfer
drop fact。单文件 native 试跑在 lowering 前明确报 `UnsupportedSource`（内部
`UnsupportedNode`），并非 native 验收通过。该例需要动态 selector/owner 运输后才能
消费 guarded fact；不能把两个不同 owner ID 的条件事实盲目合成无条件析构。此边界保留在
本 Spec 的动态运输待办中，不降低 v0.37 合法性。
`conditional_construction_input_keeps_distinct_guarded_exit_owners` 前端定向测试已确认
两条事实的 condition 非空、owner 身份不同；单文件 native 试跑失败记录未留作跳过的测试。
`cargo test -p lang-frontend --test ownership_construction --quiet` 8 passed、0 failed、
0 ignored；本次未运行 SPEC-0211 的真实 `for` native。

2026-09-24 递归环境前置切片：在 phi 身份分配前按既有有限 lambda/capture 来源
检查可达环；无环时仍使用现行树形 phi，遇环则保留 `RecursiveClosureCapture` deferred，
不发布任何 iteration、cleanup、drop 或 loan-end 产物。新增“前一个循环已有事实，后一个
循环递归捕获”的回归，锁定整个文件的原子拒绝，不把前一个循环的事实漏出。
native 环境句柄与 drop ABI 的候选记录为 proposed ADR-0025；其接受、Phase 3 动态
实例运输和 native 实现均尚未完成，本切片没有把递归 capture 标为支持。
`cargo test -p lang-frontend --test ownership_iteration --quiet`：143 passed、0 failed、
0 ignored；`cargo clippy -p lang-frontend --lib --test ownership_iteration -- -D warnings`
在最终代码状态通过；`cargo fmt --all -- --check`、`python3 scripts/check_docs.py`
（372 份 Markdown）与 `git diff --check` 通过。未运行 frontend 全量或 native。

同日有限来源图切片：phi 预扫描现在为每个可达 lambda 来源只建一个静态节点，
owned capture 边保存节点索引，包括回指；无环时由同一图驱动既有树形 phi 槽分配，
避免环检测和实际分配分别读取不同的 capture/source 边集合。静态图的节点身份仍
不等于动态环境实例，发现环继续原子 deferred；Phase 3 的实例句柄、链式释放和
native ABI 仍未闭合。改动后 `cargo test -p lang-frontend --test ownership_iteration
--quiet`：143 passed、0 failed、0 ignored；`cargo clippy -p lang-frontend --lib
--test ownership_iteration -- -D warnings` 通过。七组受影响所有权套件共 304 passed、
0 failed、0 ignored；`cargo fmt --all -- --check`、`python3 scripts/check_docs.py`
（372 份 Markdown）和 `git diff --check` 通过。未运行 frontend 全量、codegen/native。
独立只读审查未发现构图漏边、错序或跨路径共享动态 owner；但确认无环 phi 仍按
每条路径展开共享子图，菱形 DAG 可能产生指数级槽位，深链递归也有栈耗尽风险。
这属于当前树形布局的实施缺口，后续须让 phi/incoming 和释放直接使用有限图与
动态实例句柄，不能把本次预扫描当作完整图布局验收。

2026-09-24 图扫描深链修正：可达 lambda 节点改为工作表迭代扩展，环检测改为显式 DFS
栈；新增 8192 节点有环/无环单元回归，确认环检测不依赖 Rust 调用栈。
`cargo test -p lang-frontend --lib finite_capture_graph_checks_long_chains_without_recursive_traversal
--quiet`：1 passed、0 failed、0 ignored。现行树形 phi 分配与入边/清理递归尚未迁移，
因此这项修正不解决上一段指出的指数展开及其深链风险。

同日形成时实例快照合同回归：循环体先以旧 `g` 创建 owned `f = move { g() }`，
再替换 `g`。测试核对 `CreateClosureOwner(f)` 所引用定义的 capture input 指向
header `g` 的 owner 槽，且执行顺序早于 `g` 的新 owner snapshot 提交；新旧 owner
定义身份不同。首次夹具将相邻 lambda 赋值直接排列，Parser 报 L0013；按现行块语法
包裹首个赋值后测试通过，此次失败不是生产所有权行为反例。该事实合同要求消费端在
形成点读取并保存**当次实例句柄**，不能在后续轮次重新解引用可覆写的 phi 槽。
`cargo test -p lang-frontend --test ownership_iteration
owned_capture_reads_the_prior_phi_environment_before_replacement -- --exact`：1 passed、
0 failed、0 ignored。最终 `ownership_iteration` 全组 144 passed、0 failed、0 ignored；
`cargo clippy -p lang-frontend --lib --test ownership_iteration -- -D warnings`、
`cargo fmt --all -- --check`、`python3 scripts/check_docs.py`（372 份 Markdown）和
`git diff --check` 通过。未运行 frontend 全量、codegen/native；递归环境实例图的
公开运输和 native 消费仍未实现。

同日 temporary source 退出事实回放：对 `if`/`when` 构造的完整临时 source，按零轮耗尽、
两轮正常完成、两轮 `continue` 后耗尽、首轮 `break` 和首轮 `return` 分别执行对应
`IterationExitPlan` 的动作序列；检查每条实际路径上 `FinishProvider`、`EndSource`、
`Drop(Temporary(source))` 的顺序，以及保留 provider 的边不释放 source。定向测试
`conditional_temporary_source_replays_each_executed_exit_once` 1 项通过。回放使用已发布
的 Phase 3 事实；它不执行条件表达式、container 构造、provider 或 native 析构，
因此不充当 source 求值一次或 SPEC-0182 的 native 验收。最终
`ownership_iteration` 全组 145 passed、0 failed、0 ignored；定向 strict Clippy、
`cargo fmt --all -- --check`、文档结构检查（372 份 Markdown）与 `git diff --check`
通过；未运行 frontend 全量、codegen 或 native build/run。

同日有限 capture 图发布切片：Phase 3 的 `IterationOwnershipPlan::capture_graph()` 现在公开
每个 lambda identity 一个静态节点，以及按捕获顺序排列、指向图内节点索引的候选 owned
来源边。新增回归先因接口缺失无法编译，实施后验证图节点去重、边索引有效及形成时
捕获旧 `g` 的候选环境。`ownership_iteration` 全组 146 passed、0 failed、0 ignored；
定向 strict Clippy 和 `cargo check -p lang-codegen --all-targets` 通过。独立只读复核未
发现新增索引或跨循环污染；图目前只发布静态来源，现行 phi/incoming 仍沿树递归展开，
递归 capture 仍原子 deferred，动态实例句柄与 native 清理均未实现。

同日图节点到 phi 的显式链接：`IterationClosurePhiOrigin` 与
`IterationPhiIncomingOrigin` 都携带所属 `IterationCaptureGraph` 节点索引，
分配、实际入边记录和 header→exit 耗尽转发均从目标布局传递索引，消费端无需按 lambda
AST 反查图节点。回归先因缺少 `node()` 接口编译失败，实施后核对 header/exit 布局、
嵌套捕获边及已发布入边与同一图的对应关系。最终 `ownership_iteration` 146 passed、
0 failed、0 ignored；`cargo check -p lang-codegen --all-targets` 与定向 strict Clippy
通过。此链接仍未代替树形 phi/incoming，也不是递归实例运输或 SPEC-0182 native 验收。

同日 temporary source owner 身份补齐：provider 建立时为 hidden temporary 保存完整结果
或索引 source 的实际 backing container 的 `CleanupOwnerValueId`，各独立退出边的
`DropFact::owner` 指向同一静态定义；已有直接表达式 owner 可复用，`if/when` 合流结果
以完整 source 表达式定义新 owner。条件 source 回归先因 owner 缺失失败，修复后直接、
`if/when` 与 indexed source 回归均通过；`ownership_iteration` 全组 146 passed、
0 failed、0 ignored。`cargo check -p lang-codegen --all-targets` 与定向 strict Clippy
通过。独立只读复核未发现新增的来源错配或提前退出事实；source 求值期间提前 return
的 pending temporary 仍沿原有 target-only 清理，未由此证明 native 实例释放。

同日索引 source 的 pending backing 身份补齐：`Index` 的 backing container 求值完成、
index 尚未完成时，提前 `return` 清理事实现在携带该 backing 的
`CleanupOwnerValueId`；同一 backing 表达式的正常 provider 退出与提前退出复用同一个
静态定义 ID，路径版本各自保留当前 condition。一级索引回归先因 pending owner 缺失
失败；独立复核又发现两条互斥退出边曾分配不同 ID，扩展回归后修复。多层索引回归
核对每个 backing 的所有退出事实均指向各自的同一 ID。四组受影响 frontend 套件
共 213 passed、0 failed、0 ignored；`cargo check -p lang-codegen --all-targets` 通过。
定向 strict Clippy、`cargo fmt --all -- --check`、`python3 scripts/check_docs.py`
（372 份 Markdown）和 `git diff --check` 通过。独立只读复审未发现静态定义或路径条件
的新冲突；动态 backing 实例运输及 native 释放仍未验证，SPEC-0211 保持进行中。

2026-09-24 当前工作树扩展验收：`ownership_iteration`、`ownership_checking`、
`ownership_containers`、`ownership_nullable_when`、`ownership_closures`、
`ownership_construction`、`multifile_ownership_checking` 七组共 307 passed、0 failed、
0 ignored；`cargo test -p lang-codegen --lib ssa::lower_frontend_tests --quiet` 命中 39 项，
全部通过。审计确认公开 `IterationCaptureGraph` 已按 lambda 去重并保留回边，但
`IterationClosurePhiOrigin`/`IterationPhiIncomingOrigin` 的嵌套 capture 关系仍按树展开，
形成状态、入边和清理也依赖这棵树。所以上述测试不证明菱形共享子图的有界布局、递归
环境的逐实例运输或首轮 `for` native；第 9 节的有限图实例合同与 SPEC-0182 前置仍未闭合。

同日紧邻环境捕获缺口：真实回归 `outer = move { f = move { base() }; for ...; f() }`
先以无诊断、无 deferred 的产物复现：有限图有 `f → base`，但 `Entry` 的嵌套
`base` 环境集合为空。内层输入是 `CleanupCaptureValue::Environment`，现行树形入边
只按 `Owner` 匹配已形成的内层来源；直接改用 `base` 的旧静态定义 ID 会丢失外层环境
当次捕获的实例身份。现增加 `EnclosingEnvironmentCapture` deferred：仅当 phi 入边确需
从紧邻 callable 环境运输嵌套 closure 时触发，并与递归环相同地原子拒绝本文件的
iteration、cleanup、drop 和 loan-end 产物。非 closure 环境捕获回归继续发布计划。
这保持源码的 v0.37 合法性，但不是第 9 节要求的完整运输；后续须让环境捕获槽携带
动态实例及选择快照，替换本项 deferred 后再做逐实例回放。
失败回归起初在 `Entry` 断言实际 `base` 环境数量为 1，产物给出 0；修复后的回归核对
deferred 原因和位置、四类事实原子清空，旁侧回归核对非 closure 环境捕获不被拒绝。
七组受影响 frontend 套件共 309 passed、0 failed、0 ignored；codegen frontend lowering
契约测试 39 passed、0 failed、0 ignored，`cargo check --workspace --all-targets` 和定向
strict Clippy 通过。独立只读复核未发现新的漏检或明显过宽 deferred；未运行真实 `for`
native build/run，完整环境运输仍待实现。

同日静态捕获槽身份切片：每个 closure owner 定义按已检查的 capture source 注册稳定
`CleanupCaptureSlotId`，即使当前路径没有该 source 的 capture input，也保留槽布局；
同源条件版本共用槽 ID，但输入各自保留创建条件与来源值。紧邻环境的
`CleanupCaptureValue::Environment` 现在显式携带该槽 ID，供后续从外层**当次实例**
读取槽值；静态 ID 本身不代表运行时实例。现有嵌套捕获回归先因缺少槽接口编译失败，
实施后核对环境输入、槽布局与 owner/source 对应；条件捕获回归核对同源多版本的槽
身份一致。`EnclosingEnvironmentCapture` deferred 仍保留；本切片尚未实现实例句柄、
跨轮递归运输或解除 SPEC-0182 阻塞。
七组受影响 frontend 套件共 309 passed、0 failed、0 ignored；codegen frontend lowering
契约测试 39 passed、0 failed、0 ignored。`cargo check --workspace --all-targets`、定向
strict Clippy、`cargo fmt --all -- --check`、文档结构检查（372 份 Markdown）及
`git diff --check` 均通过。独立只读复审核对了条件版本、不同 owner 的槽隔离和 deferred
原子性，未发现本切片的具体正确性缺陷；未运行 `for` native build/run。

同日环境实例布局次序切片：`CleanupCaptureSlot` 记录经检查的捕获首次引用序号，作为同一
closure 定义的环境实例内槽位置；条件来源的多个输入版本不额外占槽，不同环境定义的
同名 source 仍分配不同槽身份。回归先因缺少 `position()` 接口编译失败，随后核对两源
捕获的首次引用次序、同源条件版本的位置和嵌套环境的槽隔离。非 phi 捕获占据的
槽位也计入完整环境布局，不能以过滤后的 phi source 下标代替槽序号。此序号只定义布局，尚不表示
形成时已保存动态句柄；递归来源图仍须改为有限 phi 布局并实现逐实例运输与释放。
七组受影响 frontend 套件共 309 passed、0 failed、0 ignored；补充非 phi 捕获占槽的
回归后，`ownership_iteration` 全组 148 passed、0 failed、0 ignored；
`cargo check --workspace --all-targets` 与定向 strict Clippy 通过。独立只读复审未发现槽隔离与排序的语义错误，
并指出描述符顺序是首次引用顺序，已据此修正文案和回归。未重跑 codegen 行为测试或
`for` native build/run。

同日 owned 紧邻环境 capture 的实际析构边界回归：内层 `move` lambda 从外层环境槽移走
同一 MoveOnly source 后，planner 会在内层和外层各发布一个**候选** captured drop；
静态事实数量为 2，不能据此断言动态值析构两次。新增 `DropFact::capture_slot()`，使已
形成 closure 的候选 captured drop 显式关联其接收环境槽。回归先因缺少该接口编译失败，
随后核对内层形成输入引用外层槽、两个候选事实分别绑定内外槽，并以单个环境实例的槽
占用状态回放 move 与逆序释放，实际只释放一次。此回放不证明多轮动态实例运输；
合成 phi 的 captured fact 仍可能没有槽身份，不能以本切片解除
`EnclosingEnvironmentCapture` 或 `RecursiveClosureCapture` deferred。
独立复审发现快照改写运输 owner 后会丢失槽查询的原定义、递归析构重建事实也会丢失槽
字段；两条扩展回归分别失败后修复。`ClosureOrigin` 现分开保存运输 owner 与形成时
`layout_owner`，合流不混合同一快照下的不同布局；递归重建事实保留槽身份。回归核对
槽的 source 与原 lambda 定义，而不把静态槽 ID 当成已验证的运行时实例。
七组受影响 frontend 套件共 311 passed、0 failed、0 ignored；增强槽对应断言与测试样式
修正后，`ownership_iteration` 全组 150 passed、0 failed、0 ignored。codegen frontend
lowering 契约测试 39 passed、0 failed、0 ignored；`cargo check --workspace --all-targets`
与定向 strict Clippy 通过。独立只读复审确认两处缺口已修复；未运行 native build/run，
也未验证多轮动态槽占用或递归实例释放。

同日 phi 捕获槽身份切片：一个 phi owner 可持有多个候选 lambda，它们即使捕获同一
source 也须有不同槽身份。现按 `(root phi owner, lambda, source)` 注册槽；每个
binding 从已发布有限捕获图的根出发迭代可达节点，同一节点只登记一次。槽位置沿完整
已检查捕获顺序，不能取过滤后图边的下标；嵌套 `ClosureOrigin` 将 root 布局 owner
贯穿到 captured drop，而 target owner 仍指向嵌套来源值。回归先因缺少 phi 槽查询
接口编译失败，随后验证同源双 lambda 的不同槽、未进入图的 capture 对位置的影响，
以及嵌套 owned 捕获的 drop 槽属于根 phi 布局而非子 source owner。独立只读复审核对
多 binding 隔离、共享节点去重和环检测边界，未发现具体槽错配。当前
`IterationClosurePhiOrigin`、incoming 与 cleanup 仍沿树形结构，递归图仍原子 deferred；
本切片只为已发布的非递归 phi 建立有限静态捕获槽，不证明递归 phi 布局、当次实例
句柄运输或逐实例释放。
七组受影响 frontend 套件共 313 passed、0 failed、0 ignored；codegen frontend lowering
契约测试 39 passed、0 failed、0 ignored。`cargo check --workspace --all-targets`、定向
strict Clippy、`cargo fmt --all -- --check`、文档结构检查（372 份 Markdown）和
`git diff --check` 通过。未运行 `for` native build/run，递归图与动态实例仍未验收。

同日两轮 owned capture 来源回放：对 `f = move { g() }; g = move {}`，从实际
`CreateClosureOwner` 输入、`f/g` snapshot、entry/backedge/exhaustion phi incoming 和
实际 `Action::Drop` 读取事实，以 `(动态环境实例, 捕获位置)` 保存形成时的 `g` 实例；
每次入边还核对静态 phi 槽映射到相同位置。两次回边并行复制后，按已初始化的选择位
执行 drop 条件：第二轮替换 `f` 释放第一轮捕获的旧 `g`，退出后的 `f()` 释放第二轮
捕获的旧 `g`，检查释放顺序及每个实例恰好一次。此测试只回放无环来源图的两轮实例身份；静态槽 ID
尚不是运行时环境句柄，递归来源图仍保持 `RecursiveClosureCapture` deferred，真实 native
动态运输与释放未由此验证。
`ownership_iteration` 全组 153 passed、0 failed、0 ignored；定向 strict Clippy、
`cargo fmt --all -- --check`、文档结构检查（372 份 Markdown）和 `git diff --check`
通过。独立只读复审指出原先手工释放和未检查 Save guard 两处假阳性，已改为由
实际 guarded action 与 snapshot 驱动，并在最终代码状态重新运行上述检查。
本次未重跑其他 frontend 套件、codegen 测试、workspace check 或 native build/run。

同日 phi 入边捕获槽切片：`IterationPhiIncomingSource` 直接标出接收环境的静态
`CleanupCaptureSlotId`，由根 phi owner、候选 lambda 和 capture source 在已注册的
有限布局中确定。形成时的输入值、入边目标 owner 和接收槽现可一并查询；槽 ID
只描述布局，入边条件与实际环境实例仍决定该槽是否持值。回归先因缺少查询接口无法编译，
随后核对两轮 owned 捕获、同源双 lambda 的不同槽，以及嵌套捕获沿根 phi 布局
运输的槽与 captured drop 槽一致；未跟踪的 Copyable capture 保持 `None` 槽与
非 owning 的入边 target。本切片未改递归图的 deferred，也未实现当次
实例句柄或 native 消费。
`ownership_iteration` 全组 154 passed、0 failed、0 ignored；
`cargo check --workspace --all-targets`、定向 strict Clippy、`cargo fmt --all -- --check`、
文档结构检查（372 份 Markdown）和 `git diff --check` 通过。独立只读复审核对
两处入边构造的根 owner、嵌套传递、缺席条件和未跟踪来源，没有发现具体错误；
据其指出的覆盖空白补了 Copyable capture 的 `None` 槽回归。本次未重跑其他
frontend 套件、codegen 行为测试或 native build/run。

同日 source 生命周期验收补强：三种包含循环内 `return xs` 的 body 现在核对 L0135
诊断准确落在该 return 的 source 操作数，而非仅匹配任意 `xs`；named 与 field Inout
source 在循环后替换仍合法，且对应循环的 break、exhaustion 两条退出各自恰好一次
按 `FinishProvider → EndSource` 顺序结束借用。`ownership_iteration` 全组
155 passed、0 failed、0 ignored。独立只读复核指出原始断言可能误认其他 `xs` 或只
统计两条同类退出，已收紧后复核；fallthrough/continue 的保留借用由邻近回归覆盖。
定向 strict Clippy、`cargo fmt --all -- --check`、文档结构检查（372 份 Markdown）和
`git diff --check` 通过。本次只补测试与验收记录，未实现递归环境实例运输，
也未运行其他 frontend 套件、codegen 行为测试或 native build/run。

同日 closure 形成边切片：已形成环境的 `closure_capture_edges(owner)` 从同一份
已检查形成输入及槽布局给出目标 `CleanupCaptureSlotId` 与来源 `CleanupCaptureInput`。
同源条件版本保留各自条件/来源但写入同一槽；内层 lambda 从紧邻环境槽读取时，
该来源槽不等于新环境的目标槽。两轮 owned capture 回放改从此关系取得目标槽。
回归先因缺少查询接口编译失败；实现后 `ownership_iteration` 全组 155 passed、
0 failed、0 ignored。独立只读复核未发现当前可达的槽错配；按其指出的公开查询
稳健性问题，缺槽时现返回 `None` 而不发布截断关系。定向 strict Clippy 与
`cargo fmt --all -- --check`、文档结构检查（372 份 Markdown）和 `git diff --check`
通过。此关系仍是静态读写位置，不保存当次环境
实例句柄；`EnclosingEnvironmentCapture` 和 `RecursiveClosureCapture` deferred 均未解除。
本次未运行其他 frontend 套件、codegen 行为测试或 native build/run。

同日 closure 形成写入动作切片：`CreateClosureOwner` 只建立当次环境身份，随后每个
已检查的形成输入发布 `SaveClosureCapture { owner, target, input }`，指定从形成前的
owner 值或紧邻环境槽按原条件读取，并写入新环境的目标槽。同源条件版本共用目标槽，
但保留各自条件和来源；嵌套环境的来源槽与目标槽分离。三处前端回归先因动作缺席
无法编译，实施后 `ownership_iteration` 全组 155 passed、0 failed、0 ignored。
独立只读复核指出 Phase 4 的窄 String move-closure bridge 可能忽略新事实，且原
`CreateClosureOwner` 注释可能让消费端重复读取；已澄清动作分工。针对该 bridge
不能等价处理的非恒真选择和紧邻环境来源，lowering 在捕获引用 Span 提前拒绝；
定向回归先因报告整个 lambda Span 失败，修复后 codegen frontend lowering
40 passed、0 failed、0 ignored；`ownership_closures` 15 passed、0 failed、0 ignored，
现有 String move closure native 回归 1 passed。两 crate 定向 strict Clippy、
`cargo fmt --all -- --check`、文档结构检查（372 份 Markdown）和 `git diff --check`
通过。本切片仍未实现 phi 动态实例并行运输与链式释放，两个相关 deferred 保留；
未运行其他 frontend 套件或更广的 codegen/native 矩阵。

同日逐实例捕获回放补强：嵌套 `move` lambda 的同一静态定义连续形成两次时，测试按
`SaveClosureCapture` 的实际输入从当次外层环境槽移动到新内层实例槽，而非按静态 owner ID
共用一份占用状态。两次调用各持有不同的 `xs` 实例；外层源槽移动后为空，两个内层实例
分别按已发布的 captured drop 候选释放自己的值，均恰好一次。两轮 `f` 捕获旧 `g` 的
回放也改为从形成前 owner 快照和实际写入动作取得实例，而非手工填入捕获表。
`ownership_iteration` 全组 155 passed、0 failed、0 ignored。这仍是测试内的前端事实
回放。独立只读复核指出原回放未核对实际 `Action::Drop`，并在 owned move 后留下
来源 binding 的句柄；已改为按内外调用退出点核对动作、条件与目标，并在形成时消费来源
句柄，phi 来源从已保存的环境槽读取。两项受影响回归各自定向通过；不执行原生环境
句柄、递归来源图或 native 链式析构，两个相关 deferred 保留。
再次复核补上内层 drop 动作早于外层的顺序断言；phi 回放把形成时的来源 owner 与
捕获实例成对保存，入边若指向仍存在的运输槽便核对动态实例，若指向已移动的形成槽便
核对原来源定义。不能把运输后的静态 owner ID 强行等同于形成时 ID。最后复核还发现
phi 并行复制后残留 `next_f`/`next_g` 来源句柄，现于读取全部入边后消费已运输来源，
再提交目标槽；两轮精确回归通过。最终
`ownership_iteration` 全组 155 passed、0 failed、0 ignored。入边产物尚未显式给出
来源环境实例与来源捕获槽的完整动态读取关系，因此此回放不解除递归及紧邻环境 phi 的
deferred，也不构成 SPEC-0211 第 9 节全部验收。

同日 phi 来源槽关系切片：`IterationPhiIncomingSource` 新增 `source_capture_slot()`，
与既有接收槽、入边 `environment.owner()` 分开发布。实际入边按候选 closure 的
`layout_owner` 取得来源槽；header→exit 转发始终沿 header 根布局查槽，嵌套来源
继续使用其自身动态 owner 定义。来源槽仅是静态布局，读取时仍须用入边 owner 的
当次实例和该槽位置，不能把布局 owner 当成实例。两轮 owned 捕获回放改按来源槽
读取，并精确核对回边与耗尽边的来源槽身份；嵌套 header→exit 回归核对内层来源槽
属于 header 根布局、接收槽属于 exit 根布局。本切片仍未物化运行时实例句柄、
递归来源图或缺席来源的完整清空与释放；`EnclosingEnvironmentCapture`、
`RecursiveClosureCapture` deferred 保留。
回归先因缺少查询接口编译失败；实施及补齐精确槽身份断言后，`ownership_iteration`
全组 155 passed、0 failed、0 ignored，codegen frontend lowering 契约 40 passed、
0 failed、0 ignored，`cargo check --workspace --all-targets` 和 frontend 定向 strict
Clippy 通过。独立只读复审核对形成入边与嵌套转发的布局 owner、条件缺席和
`Place` 边界，未发现确定性错配；按其指出的漏检补上两轮回归的来源槽精确身份。
本切片未运行 native build/run 或其他 frontend 行为套件。

同日紧邻环境递归释放边界复核：新增 `base = move { read(xs) }` 后被 `outer`
捕获、`outer` 内的 `f = move { base() }` 再跨空循环 phi 的反例。`base` 自身持有
MoveOnly `xs`，因此只释放 `f` 的直接捕获还不足以证明整条环境链完成析构。
当前 checker 无诊断，以 `EnclosingEnvironmentCapture` 原子 deferred，四类执行事实
均为空；精确回归和 `ownership_iteration` 全组 156 passed、0 failed、0 ignored。
静态来源图、形成时槽输入与 phi 来源槽虽已发布于其他非 deferred 路径，尚未把捕获
实例的候选选择快照和递归释放关系运输进这条 phi，不能据此取消 deferred。

同日区分形成来源与 phi 运输来源：`Environment { owner, slot }` 标出内层 closure
形成时读取的紧邻外层槽；owned move 后此槽可以为空。phi 入边的
`source_capture_slot()` 必须指向已形成内层环境自己的槽，并与入边环境的当次实例
配对，不能把形成来源槽当作运输来源槽。独立复核发现两者易被误认后，新增 shared
与 owned 场景回归，核对两槽不同及形成来源 owner 身份；owned 场景允许外层槽在
形成后为空。此时 `record_phi_origin` 保持原有候选 `layout_owner` 查槽实现。
`ownership_iteration` 全组 157 passed、0 failed、0 ignored；frontend 定向 strict
Clippy 通过。递归及紧邻 closure 实例运输仍未实现，两个 deferred 保留。

2026-09-24 有限静态捕获布局切片：`PhiCaptureGraph::capture_layout` 对每个 header/exit binding
遍历可达图并一次收集每个 lambda 的 tracked capture 槽，随后才判定图环；环边继续引用
既有节点，不展开来源树。单测以自环、交替环与重复指向同一节点的边核对有限遍历及
完整捕获集的原始 `position`。无环路径仍按原有节点序登记 phi capture 槽；递归路径的
图与布局目前只在 planner 内部暂存，最终仍原子发布 `RecursiveClosureCapture` deferred，
**尚未**发布可执行的递归 phi、实例运输或递归释放。定向图布局单测与
`ownership_iteration` 全组 157 passed、0 failed、0 ignored；frontend 定向 strict
Clippy 通过。额外运行的 frontend lib 组为 83 passed、2 failed、0 ignored；两项失败均为
parser 的线性访问计数阈值，单独运行其中一项仍失败，本切片未修改 parser，尚未完成归因。

2026-09-24 phi 入边读取位置切片：`IterationPhiIncomingSource::input()` 继续记录已检查
候选来源（实际入边为形成输入，header→exit 为转发来源），新增 `transport_value()`
显式给出复制前的候选当次环境 owner 与已形成来源槽；
若没有已发布来源槽则返回 `None`，不能退回形成时的 `Environment` 槽读取。实际入边取
`candidate.owner` 与 `candidate.layout_owner` 下的槽，header→exit 取 header owner 与
header 布局槽。新增回归按两条 `SaveClosureCapture` 动作回放两次独立形成实例，外层槽
move 后为空；Entry 从各自内层实例读取，再将 root 句柄移入 header；Exhaustion 从
精确的 header 捕获槽读取，再将句柄移入 exit。逐次核对实例和捕获 position，不能只按
静态 owner ID 或槽 position 猜测。独立只读复核修正了回放中未消费来源句柄及仅按
position 检查 header 槽两处漏检。`ownership_iteration` 全组 157 passed、0 failed、
0 ignored；`cargo check -p lang-codegen` 通过。此回放仅覆盖无条件、单候选的
Entry/Exhaustion，两次实例表示两次独立形成，不是两轮回边；条件路径、break/continue、
递归链的实例选择和释放仍未验收，两个相关 deferred 保留。

2026-09-24 递归环境运输合同细化：第 9 节明确区分有限静态图节点、owner 定义点与当次
环境实例；形成时保存旧实例及选择快照，phi 从同一旧状态读取后并行提交目标引用，
不改写已形成实例内部的 owned capture 槽；缺席入边清空目标。解除 deferred 须有
同一 lambda 自链、交替 lambda 链和各退出边的逐实例事实回放。本轮只更新实施合同，
未解除 `RecursiveClosureCapture` / `EnclosingEnvironmentCapture`，也未运行 Rust/native。

2026-09-24 phi 缺席来源清空切片：每个 header/exit binding 在有限图预分配时保存完整
tracked capture 槽列表；Entry、回边、continue、break 与 exhaustion 的每条入边均显式
发布 `capture_slots_to_clear()`。消费顺序是先从同一旧状态读取所有来源，再清空该列表，
最后写入选中来源；清空仅作用于目标 phi 布局，不能改写已形成实例内部的 owned capture。
条件 closure 的两轮回放现按列表实际清空模拟槽，分别验证 xs→ys 与 ys→xs 的选中槽和
未选槽。独立只读复审发现首版回放只清 owner 映射、没有消费新列表；修复后复审确认
漏清旧槽会使回归失败。定向回归先因缺少查询接口无法编译；完成后
`ownership_iteration` 全组 157 passed、0 failed、0 ignored，定向 strict frontend Clippy、
`cargo check -p lang-codegen`、`cargo fmt --all -- --check`、文档结构检查（372 份 Markdown）
和 `git diff --check` 通过。该回放的来源读取仍用旧 owner 映射；条件来源的当次环境槽、
递归图实例链和 native 消费尚未由此验证，两个相关 deferred 保留。

2026-09-24 条件来源槽回放补强：同一 xs/ys 双来源两轮测试现从实际
`CreateClosureOwner` 与 `SaveClosureCapture` 建立当次环境实例及形成槽，逐条核对
Create → Save → SaveOwnerSnapshot → Commit 的动作顺序、lambda 身份和形成执行点。
Entry、回边与 exhaustion 从 `transport_value()` 指定的已形成环境槽或旧 header phi 槽
读取，在旧状态快照中同时准备写集，再清空目标 phi 槽并提交选中来源；形成时来源 owner
仅作为独立交叉核对，不代替实际槽读取。两种切换顺序的未选槽均须为空。独立只读复审
先后指出来源布局未与当次实例配对、创建动作遗漏及动作点/重复创建漏检，均已在回放中
补齐。定向回归通过；本切片仍是前端事实回放，未实现运行时 selector/实例运输或 native，
递归与紧邻环境 deferred 保留。最终 `ownership_iteration` 全组 157 passed、0 failed、
0 ignored；定向 strict frontend Clippy、`cargo fmt --all -- --check`、文档结构检查与
`git diff --check` 通过。本切片未运行 codegen 行为测试、frontend 全量或 native。

2026-09-24 有限图 phi 槽映射切片：`IterationClosurePhiBinding::capture_layout()` 现逐项
发布 `(有限图节点, 完整 capture 顺序中的 position, 根 phi 目标槽)`，供后续按图边定位
运行时捕获引用；同一静态节点在一个 binding 中只映射一次，header/exit 与不同 binding
各有独立槽。每条入边的 `capture_slots_to_clear()` 由该目标布局直接派生。集成回归
从各 binding 的根沿已发布图独立求可达 `(node, position)` 集并核对完整映射、跨 phi
槽不共享及清空列表；另一回归核对未跟踪 capture 占据位置 0 后，tracked 槽仍保持
原始位置 1、2。独立只读复审未发现本切片确定错误；可发布菱形 DAG 尚无专门集成例。
递归来源图仍在树形 origin 展开前原子 deferred，本映射不代表动态实例链或 native 已支持。
`ownership_iteration` 全组 157 passed、0 failed、0 ignored；定向 strict frontend Clippy、
`cargo check -p lang-codegen`、`cargo fmt --all -- --check`、文档结构检查（372 份 Markdown）
与 `git diff --check` 通过。未运行 codegen 行为测试、frontend 全量或 native build/run。

2026-09-24 递归图有限槽登记切片：`PhiCaptureGraph::register_capture_layout` 从可达
静态图节点及完整 capture 位置登记 phi owner 的有限捕获槽；`preallocate_closure_phis`
现对递归图也完成 phi owner、capture 布局和 binding 可用性登记，再于树形 origin 展开前
标记 `RecursiveClosureCapture` deferred。递归图的登记仅留在 planner 内部，不进入
公开 `loop_phis`；整文件仍原子返回空 drop plan 与 deferred。单测以交替环和重复边核对
登记槽与图节点、原始 capture 位置及环境身份一一对应；现有自环、交替环与已有循环后
出现递归环的集成回归继续验证原子 deferred。独立只读复核未发现本切片确定错误。
`ownership_iteration` 全组 157 passed、0 failed、0 ignored；定向 strict frontend Clippy
通过。此切片尚未交付递归环境实例运输、递归释放或 native 消费。

2026-09-24 有限布局节点身份补强：phi capture 登记现在直接沿 `PhiCaptureGraph`
的节点索引建立 `(node, position, slot)`，不在登记后按 lambda AST ID 反查节点。
自环加交替边的内部回归核对有限可达布局、原始 capture 位置及 phi 槽所属环境，
和既有交替环/重复边回归一起覆盖两类静态环。独立复核指出最初加入的逐实例
释放模拟完全由测试自造实例和捕获边，不能证明生产端已运输或释放；已移除该
伪验收，仅保留能由实际布局登记验证的断言。`ownership_iteration` 全组 157 passed、
0 failed、0 ignored；两个递归布局 lib 单测通过，定向 strict frontend Clippy 通过。
递归来源继续原子 deferred；完整产物驱动的实例运输与释放回放仍待实现。

2026-09-24 真实菱形来源图回归：两个互斥分支分别形成捕获同一 `base` closure 的
`f` / `g`，随后 `outer` 同时捕获两者。公开图中 `base` lambda 只有一个节点、两条
上游边；header/exit 各自的 phi 捕获布局对该节点只登记一次原始位置 0，且两
边界槽身份不同。Entry 的两条非空 `base` 候选沿同一已保存 flag 选择的相反分支，
各自指向形成时的静态环境定义；其捕获目标槽对应 header 布局，运输来源指向
已形成环境的捕获槽。独立复核发现首版只数候选并比较条件 ID，不能证明互斥，
也可能漏掉重复图节点；已补对应断言。此测试核对真实前端产物的静态关系，
**不**把 owner 定义 ID 当作当次实例，也不证明递归环境链运输或释放。
`ownership_iteration` 全组 158 passed、0 failed、0 ignored；新增定向回归通过。

2026-09-24 同一 lambda 的并存实例边界：真实两段循环使同一内层 lambda 在前后轮
各形成一个仍存活的环境，外层 closure 沿两个 owned 捕获路径持有它们，并在第二段循环
进入 phi。静态图的单节点、单根布局槽只描述位置，无法区分两条路径下的当次实例。
Phase 3 现比较有捕获槽的同一节点在不同捕获路径上的候选条件；若入边不能证明
互斥，就以 `AmbiguousClosureInstanceTransport` deferred 原子清空整文件的 iteration、
cleanup、drop 和 loan-end 计划。相同捕获位置的多个候选不算并存。前述 flag 互斥
菱形的 Entry 条件可区分两条路径，但 header→exit 转发把嵌套 presence 表为独立槽，
失去互斥证明；它现在也 deferred，先前发布静态图的回归已改为原子边界回归。
这只是防止错误公开未限定实例的运输，不改变 v0.37 合法性；后续仍须按第 9 节补齐
实例句柄、来源槽读取、并行 phi 提交与逐实例释放，再解除 deferred。
`ownership_iteration` 全组 159 passed、0 failed、0 ignored；定向 strict frontend Clippy、
`cargo fmt --all -- --check`、文档结构检查（372 份 Markdown）与 `git diff --check`
通过。独立只读审查核对路径编码、空捕获节点及 Entry/Fallthrough/Continue/Break/
Exhaustion 覆盖，未发现本切片确定性错误。未运行 frontend 全量、codegen 行为测试或
native build/run；完整动态实例运输和递归释放仍未实现。

2026-09-24 递归实例运输合同复核：不能把条件菱形 Entry 的 flag 互斥直接推广到后续
回边或耗尽边；φ 必须按实际根句柄及其保存的捕获边定位后代，静态 phi 布局槽不能保存
同节点的多个子实例。独立只读复核进一步发现现有 `EndCaptureLoan`、
`TestLastCaptureLoan` 与 captured `DropFact` 只有静态 owner/槽，无法在两个并存子实例
间分别结束 shared loan；第 9 节已把实例边地址和兄弟实例共享 source 的最后借用者
验收列为解除 deferred 的条件，并消除了“phi 复制后代捕获槽”的表述歧义。拟议
ADR-0025 同步记录 native 迁移时不得与旧 captured drop 重复析构。此轮未改变执行行为；
`cargo check -p lang-codegen` 通过。未重跑 Rust 行为测试或 native。

2026-09-24 shared 捕获双实例边界回归：同一 lambda 在前一循环的不同轮次分别形成
借用同一外层 `xs` 的两个 closure，随后由外层 move closure 的两条 owned 捕获路径
同时持有并进入第二个循环。测试核对内层 capture 为 shared、deferred 精确指向该
lambda、没有新增语言诊断，且 iteration、cleanup、drop、loan-end 公开事实均原子为空。
`ownership_iteration` 全组 160 passed、0 failed、0 ignored。这验证当前门禁覆盖
shared loan-end 的同节点多路径形状；**尚未**实现两个实例各自结束 loan 后才释放
共享 source 的第 9 节验收。

2026-09-24 首段循环双实例形成回放：去掉第二段循环后，真实源码在连续两轮执行
`second = first; first = move { read(xs) }`，再由外层 closure 分别 owned 捕获
`first`、`second`。新增测试从实际 `CreateClosureOwner`、`SaveClosureCapture`、
snapshot 和 Entry/Fallthrough/Exhaustion phi 事实回放两轮；逐入边核对已选环境、
捕获来源、`transport_value()` 的环境槽与实际 source 实例，以及目标槽与对应
phi 静态布局的关系，确认外层两条捕获边分别持有不同的内层环境及各轮不同的 `xs`。
此证据只覆盖实例形成、根 phi 转发及首段循环已发布的静态捕获来源；不证明
第二段循环的同节点双路径运输、动态释放
或 shared loan-end，故 `AmbiguousClosureInstanceTransport` deferred 仍保留。
`ownership_iteration` 全组 161 passed、0 failed、0 ignored；定向 strict frontend Clippy、
`cargo fmt --all -- --check`、文档结构检查（372 份 Markdown）与 `git diff --check`
通过。独立只读审查发现并促成入边环境来源及目标 phi 布局核对。未运行 frontend
全量、codegen 行为测试或 native build/run。

2026-09-24 嵌套实例地址切片：非递归 phi 的每个入边环境候选现发布根 owner 值与
沿已形成环境捕获边的原始位置路径；候选冲突检查使用此路径，不再按过滤后的 tracked
source 序号推断捕获位置。根层 `transport_value()` 仍可查询已形成环境槽；嵌套来源
返回 `None`，防止把静态子 owner 与根 phi 布局槽拼成错误地址。两轮旧环境回放改为
从根实例沿路径取得实际子句柄，再读子实例的 source 槽；另以首个未跟踪 Copyable
capture 验证后续子闭包仍位于原始位置 1。body 入边与 exhaustion 转发均核对根与
路径。`ownership_iteration` 全组 162 passed、0 failed、0 ignored；定向 strict
frontend Clippy 通过。该产物仍未发布实例级释放动作，双路径和递归 deferred 保留；
未运行 frontend 全量、codegen 行为测试或 native build/run。

2026-09-24 条件子实例存在位门禁：独立复审指出嵌套 origin 的条件仍可能从静态候选或
header selector 取得，而没有证明与父实例形成时保存的子边一致。真实源码在同一父
capture 的两个子闭包（分别持有 `xs`、`ys`）之间条件替换后，原产物无 deferred；
新增失败回归证实该缺口。一个空子闭包与一个持有 `ys` 的子闭包也曾漏过首版门禁，
第二条失败回归确认后，门禁收窄为：同一父 capture 有多个已知子来源且至少一个
持有 tracked capture 时，标记 `AmbiguousClosureInstanceTransport` 并原子清空计划。
既有双路径门禁优先给出其精确来源；全空子环境不受此门禁影响。原跨轮空/有捕获
混合夹具现属于延期边界，静态槽布局回归改用每轮单一内层来源继续验收布局。
`ownership_iteration` 全组 164 passed、0 failed、0 ignored。该图级门禁尚不能检测
已知有捕获子来源与 opaque/未知来源的混合选择，也不证明单一子 lambda 内的条件
capture 版本；完整来源域与父实例选择快照仍需
补齐，不能据此解除 deferred 或宣称条件子环境运输完成。定向 strict frontend
Clippy、`cargo check -p lang-codegen`、`cargo fmt --all -- --check`、文档结构检查
（372 份 Markdown）和 `git diff --check` 通过；未运行 frontend 全量或 native build/run。

2026-09-24 已知与未知子来源合流门禁：`origins` 的有限数据流状态现在同时保留
已知 lambda 来源和 opaque 可能性，随局部绑定、分支合流、闭包捕获与循环固定点
传递。MoveOnly 表达式没有已知 lambda 来源时标记为 opaque；同一父 capture 的
候选若同时包含 opaque 值和持有 tracked capture 的已知子环境，Phase 3 以
`AmbiguousClosureInstanceTransport` 原子 deferred，不发布不完整的 phi/cleanup/drop/
loan-end 事实。两条先失败后通过的真实源码回归分别覆盖 opaque 参数赋值替换，
以及已知 lambda 与函数调用结果在 `if` 直接合流。`ownership_iteration` 全组
166 passed、0 failed、0 ignored。此门禁不提供父实例选择快照，也不解除递归来源、
同节点双路径和单一 lambda 条件 capture 版本的延期；递归环境运输与 native 消费
仍未交付。定向 strict frontend Clippy、`cargo check -p lang-codegen`、
`cargo fmt --all -- --check`、文档结构检查（372 份 Markdown）和
`git diff --check` 通过；独立只读复审未发现本切片新的确定性漏判。
未运行 frontend 全量或 native build/run。

2026-09-24 同一内层 lambda 的条件 capture 版本核对：真实 `for` 正常回边中，`g`
在 flag 两支形成时捕获不同 `xs` owner，再由外层 `f` 持有。最初尝试断言应延期，
但检查实际 `SaveOwnerSnapshot` 和 Fallthrough 入边发现，形成选择沿 `g`、`next`、
`f` 的三次快照逐级复制；嵌套入边的两条 capture 版本读取最后保存的 selector，
而非原始 flag。新增事实回放对两种分支分别执行快照复制，再改变原始 flag，
验证选中的旧 source 不变；两条版本共享已形成子环境的捕获槽，入边保留根实例
到该子实例的路径。`ownership_iteration` 全组 167 passed、0 failed、0 ignored。
这证明该单一路径形状无需新增延期，不证明同节点并存子实例、递归链释放或 native
按实例消费；这些边界仍按第 9 节保留。定向 strict frontend Clippy、
`cargo fmt --all -- --check`、文档结构检查（372 份 Markdown）及
`git diff --check` 通过；未运行 frontend 全量、codegen 行为测试或 native build/run。

2026-09-24 递归 body 根来源切片：递归来源图在登记有限 header/exit phi 后，仍把
内部 phi 留给 body 状态；树形 `origins` 保持为空，不沿环展开。真实源码
`f = move { f() }` 的 planner 单测先因缺少 header phi 失败，修改后从实际
`CreateClosureOwner`、`SaveClosureCapture`、`SaveOwnerSnapshot`、
`CommitOwnerSnapshot` 及 Entry/Fallthrough 入边提取来源、捕获槽与条件，核对新环境
捕获的是当前 header 根 owner。选定存在分支的两轮回放消费旧根句柄，将其分别写入
新实例的同一静态位置，得到第二轮实例指向第一轮实例、第一轮实例指向入口实例的链。
这只证明选定根 phi 与形成槽写入的来源；尚未证明所有条件路径的实例运输、递归释放、
shared loan-end 或 native 消费。`RecursiveClosureCapture` 仍使公开 iteration、cleanup、
drop 与 loan-end 计划原子为空。内部 planner 单测 4 passed，`ownership_iteration`
167 passed、0 failed、0 ignored；定向 strict frontend Clippy 与
`cargo check -p lang-codegen`、`cargo fmt --all -- --check`、文档结构检查
（372 份 Markdown）及 `git diff --check` 通过。未运行 frontend 全量、codegen
行为测试或 native build/run。

2026-09-24 递归 phi 根图节点切片：每个 header/exit binding 现在直接携带其有限
capture 图的根节点索引，按已检查 lambda 来源顺序登记；有环时不依赖空的树形
`origins` 反推根身份。真实自链源码的内部回归核对 header/exit 根映射、Entry 与
Fallthrough 的两轮句柄移动、Exhaustion 从 header 到 exit 的转发，以及最终 named
drop 的执行点、保护条件与 exit owner；形成动作与入口 owner 定义确定实例的静态
lambda 节点，再从实际 `SaveClosureCapture` 写入的实例槽沿图边回放
`201 → 200 → 100`，逐实例只遍历一次。Exhaustion 的存在位和值条件也与 header
接线核对。已发布非递归有限图的集成回归同时核对
`root_nodes` 与原树形根一致，并从这些根验证可达捕获布局。该回放只检查已有
根 drop、形成槽和图边的连通性，**不是**完整的递归释放事实；缺席来源、
break/continue、shared loan-end、多路径与 native 消费仍未由此验收，
`RecursiveClosureCapture` 继续原子 deferred。内部 planner 单测 4 passed，
`ownership_iteration` 167 passed、0 failed、0 ignored；定向 strict frontend Clippy、
`cargo check -p lang-codegen`、`cargo fmt --all -- --check`、文档结构检查
（372 份 Markdown）及 `git diff --check` 通过。未运行 frontend 全量、codegen
行为测试或 native build/run。

2026-09-25 递归释放实例地址切片：内部 planner 的 captured drop 现在携带父环境的
`(root owner, 原始 capture 位置路径)`，并保留指向被释放子值的 capture slot；
`EndCaptureLoan` 携带所在环境的同类地址。路径沿已检查布局的原始位置扩展，
不会把过滤后的 tracked capture 序号误当作槽位置。真实 `continue` 与条件
`break` 样例核对递归根入边，嵌套 shared capture 样例核对路径 `[1]`，并存的
两个 sibling 环境样例在内部核对路径 `[0]`、`[1]`；公开双实例门禁与递归
`RecursiveClosureCapture` 原子 deferred 均保留。`ownership_iteration` 167 passed、
0 failed、0 ignored；内部 `drop_planner::iteration::tests` 6 passed；
`ownership_closures` 15 passed，`ownership_checking` 29 passed；定向 strict frontend
Clippy 与 `cargo check -p lang-codegen` 通过。扩展运行 frontend lib 时，87 passed、
2 failed：两条未改动的 parser 线性预算测试超过原阈值，故不记为通过；未运行
native build/run。

同日 source 查询定位切片：`TestLastCaptureLoan` 以及其所保护的 retained-source
drop 现携带持有 source loan 的环境实例地址和 capture 槽；消费端可沿已保存的
根实例边找到该槽中的当次 source，而不能只读取静态 owner。嵌套 `[1]` 与兄弟
`[0]`、`[1]` 真实源码回归分别核对查询、drop 与 loan-end 的定位一致，
`ownership_iteration` 全组 167 passed、0 failed、0 ignored。前端状态合流和
`CaptureLoan` selector 定义仍使用静态 owner；动态 source 值上的实际 loan 计数、
递归释放遍历和 Phase 4 消费未实施，递归及并存实例 deferred 继续保留。
`ownership_closures` 15 passed、`ownership_checking` 29 passed；定向 strict frontend
Clippy、`cargo check -p lang-codegen`、`cargo fmt --all -- --check`、文档结构检查
（372 份 Markdown）和 `git diff --check` 通过。独立只读复审未发现本切片确定性错指；
codegen 的最后借用者预检回归 1 passed，仍在 lowering 前拒绝该动作。未重跑
frontend lib 全量或 native build/run。

2026-09-25 shared loan-end 捕获槽切片：`EndCaptureLoan` 现在也携带已保存 source
的 capture 槽，和同一释放边的 `TestLastCaptureLoan`、retained-source drop
共享环境实例地址。普通 borrowed element 与 Copyable place 不进入 tracked phi
布局，其 `EndCaptureLoan.capture_slot` 可为 `None`；只有 retained source 查询
缺少槽时才原子 deferred。嵌套单路径回归核对三个动作的地址与槽一致；零轮、
break、continue 及连续两轮的真实源码回放从根实例沿 `[0]` 边读取内层环境，再从其
source 槽读取实际 owner，确认上一轮与当前轮的 source 不混同。
`ownership_iteration` 167 passed、0 failed、0 ignored；此回放仍是前端事实测试，
没有执行动态 loan 计数或 native cleanup。`ownership_closures` 15 passed，
`ownership_checking` 29 passed；定向 strict frontend Clippy、
`cargo check -p lang-codegen`、`cargo fmt --all -- --check`、文档结构检查
（372 份 Markdown）和 `git diff --check` 通过。独立只读复审未发现本切片
确定性寻址问题；未运行 frontend lib 全量、codegen 行为测试或 native build/run。

2026-09-25 同源双借用实例回放：在首个循环的 `break` 入边形成两个不同的
shared closure，二者 `SaveClosureCapture` 都读取同一个 body-local `xs` owner；
`SaveOwnerSnapshot` 和 Break phi 将两个已形成环境分别送入外层 owned closure。
测试按 Entry/Break phi 与 snapshot guard 选择实际路径，从已发布事实重建环境捕获边，
并检查 `f()` 返回点之外没有结束这两条实际 source loan；再按 `EndCaptureLoan`、
`TestLastCaptureLoan`、retained-source drop 的实际动作顺序沿实例地址读 source。
两条 loan 结束后的最后借用者结果依次为 false、true；drop 条件仅在对应
selector 的 true 分支可执行，因此同一实际 source 恰好释放一次。该回放
不覆盖外层 closure 再穿过第二个循环的同节点多路径运输：这种真实源码仍以
`AmbiguousClosureInstanceTransport` 原子 deferred；也不是 native 借用计数验收。
`ownership_iteration` 167 passed、0 failed、0 ignored；定向 strict frontend Clippy、
`cargo fmt --all -- --check`、文档结构检查（372 份 Markdown）和
`git diff --check` 通过。独立只读复审发现的 guard 与调用前 loan-end 覆盖缺口
已加入测试；未运行 frontend lib 全量、codegen 行为测试或 native build/run。

2026-09-25 并存根实例检查：phi 的同一静态 capture 节点即使路径相同，若两个条件可同时成立的
候选指向不同根 owner，也必须视为不同实例并保持原子 deferred。新增先失败的单元回归
复现了只比较路径导致的漏检；判定现同时比较 `instance_root` 与捕获路径。此项修正不解除
同节点多路径、条件嵌套或递归环境 deferred；同一静态根跨轮仍可能对应不同的动态实例。
`ownership_checking::checker::drop_planner::iteration::tests` 7 passed、
`ownership_iteration` 167 passed；strict frontend Clippy、`cargo fmt --all -- --check`、
文档结构检查（372 份 Markdown）与 `git diff --check` 通过。`lang-frontend --lib` 全量
88 passed、2 failed：两项 Parser 线性预算测试分别报告 `4233 > 132 * 32` 和
`4505 > 132 * 34`；本切片没有修改 Parser，不能把全量测试记为通过。未运行 native。

2026-09-25 紧邻环境 leaf closure 来源切片：`ClosureOrigin` 记录被捕获来源所属的
capture source，规划 lambda body 时从外层环境恢复这些已形成 closure 的来源；phi 入边按
source 及 owned owner 关联候选，不再把 `Environment` 输入的 leaf 子环境误写为缺席。
仅当子环境没有需追踪的 capture source 时发布这一路径；有 owned/tracked 后代仍以
`EnclosingEnvironmentCapture` 原子 deferred。单 leaf 回归核对形成槽与嵌套来源存在位，
双 leaf 回归核对两个捕获槽各自指向原先形成的子环境；条件选择的 leaf 尚未逐实例回放，
不得据此解除递归和同节点多实例 deferred。
`ownership_iteration` 168 passed、`ownership_checking` 29 passed、
`ownership_closures` 15 passed；定向 strict frontend Clippy、`cargo fmt --all -- --check`、
文档结构检查（372 份 Markdown）和 `git diff --check` 通过。独立只读复审未发现本切片
确定性错误，确认来源标记的两条构造路径均被释放筛选覆盖；未运行 frontend lib 全量或 native。

2026-09-25 条件 leaf 紧邻环境门禁：新增 `base = if (flag) (move {}) else (move {})`
被 `outer` 捕获、其 body 再构造捕获 `base` 的 `f` 并跨 `for` phi 的回归。先失败的
形成快照回放发现 `f` 保存的子来源选择仍读取 `base` 的旧 selector，而非 `outer`
形成时的新 selector；外部选择变更可替换已形成子实例。因此 Phase 3 对紧邻环境同一
capture source 的多个已知 leaf 子来源暂以 `EnclosingEnvironmentCapture` 原子 deferred，
不发布不完整的入边或 cleanup。无条件 leaf 路径仍可发布。解除此门禁前，必须逐层保存
父环境形成时的选择，让内层形成动作从紧邻环境实例读取，并回放条件翻转后的同一子实例。
独立复审再发现一个已知 leaf 与 opaque 函数值合流的同类漏判；新增
`if (flag) (move {}) else (make())` 回归，并将原有 `AmbiguousClosureInstanceTransport`
门禁扩至任一已知子来源与 opaque 值合流，避免未保存的选择被发布为存在位。
`ownership_iteration` 170 passed、`ownership_checking` 29 passed、
`ownership_closures` 15 passed；定向 strict frontend Clippy、`cargo fmt --all -- --check`、
文档结构检查（372 份 Markdown）及 `git diff --check` 通过。前述单一已知 leaf 正例
仍通过。未运行 frontend lib 全量、codegen 行为测试或 native build/run；递归环境运输
仍未完成。

2026-09-25 leaf 环境选择读取位置切片：`CleanupSelectorCopy` 现可标出选择所属的
紧邻环境捕获值。只有结果 closure 的 leaf 子来源自身快照持有该 selector、父环境
本身不使用该选择，且所有候选指向同一来源槽时才登记；内层 `f` 的选择复制可由
`outer` 当次实例中保存的 `base` capture 槽读取，不必重读已移动的词法 binding。
实际环境句柄及选择快照的
逐层运输、动态释放和 native 消费尚未实现；条件 leaf 的 phi deferred 保持不变。
独立复审指出外层分支的路径条件也可能进入 leaf origin，不能仅按 condition 引用就
标注捕获槽；现要求 selector 确由 leaf 自己的 owner 快照保存，并以外层 `if` 负例
核对不发生误标。复审后未发现此 leaf 范围内另一确定错误。
`ownership_iteration` 172 passed、`ownership_checking` 29 passed、`ownership_closures`
15 passed；快照单元测试 6 passed；定向 strict frontend Clippy、`cargo fmt --all -- --check`、
文档结构检查（372 份 Markdown）及 `git diff --check` 通过。未运行 frontend lib 全量、
codegen 行为测试或 native build/run。

2026-09-25 leaf 选择来源的静态事实核对：`enclosing_leaf_snapshot_locates_formation_choice_in_the_parent_capture`
现从已发布的 base 快照输入条件分别选择两个分支，核对控制 selector、
`SaveOwnerSnapshot` 的 RHS 保存点、父子环境各自同点的 `CreateClosureOwner`/
`SaveClosureCapture` 顺序、跨阶段保存顺序及捕获输入条件。独立复审指出先前测试的
实例回放自行填入预期选择和捕获关系，不能证明运行时运输；已移除该不可靠段落。
定向回归 1 passed。此证据只覆盖无循环的条件 leaf 静态关系；动态实例回放、
递归环境链与 native 验收仍待完成，相关 deferred 保留。

2026-09-25 条件版本来源位置护栏：`SaveOwnerSnapshot` 的 leaf selector 位置只在所有
可达 capture 候选均能定位、同一 source 的输入覆盖候选路径且各版本指向同一个紧邻
环境值时填写。此前取同 source 第一条输入可能误标位置；独立复审又发现定位失败候选
会被 `filter_map` 静默丢弃、以及输入只与路径相交却未覆盖路径的缺口，现均已修复。
单测覆盖互斥输入、同值版本、缺失路径输入和成功/歧义候选并存。复制前后的 selector
关联尚未用于证明多版本互斥，相关情形保守留空；这不解除条件 leaf、递归环境或
native 的延期。
本切片的 snapshot 定向 lib 单测 1 passed，`ownership_iteration` 172 passed、
`ownership_checking` 29 passed、`ownership_closures` 15 passed；定向 strict frontend Clippy、
`cargo fmt --all -- --check`、文档结构检查（372 份 Markdown）及 `git diff --check` 通过。
未运行 frontend lib 全量、codegen 行为测试或 native build/run。

2026-09-25 leaf 快照在 owned capture 后的读取位置：内层 `SaveClosureCapture` 先从
父环境的当次槽移动 leaf 值并保存到新环境目标槽；同点稍后的 `SaveOwnerSnapshot` 必须
从新环境已保存的目标槽复制选择，不能再读已清空的父槽。定向回归先以旧产物失败，
显示父 owner/槽与内层 owner/槽不同；producer 现仅在父输入可达且唯一时，将
`source_value` 标为结果 closure 当前 owner 与其原始布局的接收槽。直接嵌套形状的
动作顺序与槽身份已由测试核对；经过 snapshot/phi 转运后的动态实例对应、递归释放和
native 消费仍需独立事实回放，相关 deferred 不变。
本切片的修前定向回归按父 owner/槽与内层 owner/槽不等而失败；修后该回归及
snapshot 定向 lib 单测各 1 passed，`ownership_iteration` 172 passed、
`ownership_checking` 29 passed、`ownership_closures` 15 passed。定向 strict frontend
Clippy、`cargo fmt --all -- --check`、文档结构检查（372 份 Markdown）和
`git diff --check` 通过；未运行 frontend lib 全量、codegen 行为测试或 native build/run。

2026-09-25 直接嵌套 leaf 的形成事实回放：定向测试从已发布的
`CreateClosureOwner`、`SaveClosureCapture`、`SaveOwnerSnapshot` 动作读取输入与目标，
按依赖顺序模拟两次使用相同静态 owner 的形成过程。owned capture 从来源槽移出当次
实例；父环境保存后清除回放器中的临时控制选择，内层快照必须从新环境的 capture 槽
取得原分支选择。两次形成的动态环境地址不同，保留的首次捕获槽未被第二次覆盖。
独立复审发现回放器曾在已定位环境缺少选择时回退到全局状态；现仅对无定位来源的
控制选择允许该回退，定位槽缺值立即失败。本测试只覆盖形成/捕获/快照事实与实例
隔离，不执行调用、释放、phi 运输或 native 消费，也不解除相关 deferred。
最终定向回归 1 passed；未运行其他 frontend 套件、codegen 行为测试或 native build/run。

2026-09-25 phi 来源实例读取事实：`IterationPhiIncomingSource::transport_read()` 现
同时给出已登记的实例地址和来源捕获槽；直接候选与 header→exit 转发均由 producer
登记。执行入边时，地址须从复制前的根句柄沿已保存捕获位置解析，不能把静态 owner 或
图节点当作实例，也不能读取形成前已移动的外层槽。两轮 phi 回放改为消费该读取事实，
另以首个未跟踪 capture 占位置的嵌套夹具核对地址路径 `[1]` 与来源槽。独立复审未发现
根层、嵌套层或转发地址与槽的不一致；这仍不是整个动作流的运行时运输或递归释放。
新增 API 的预期失败先由 `cargo check -p lang-frontend --test ownership_iteration`
报告缺少 `transport_read`；实现后同一 check 通过，`ownership_iteration` 172 passed。
strict frontend lib/iteration Clippy、`cargo fmt --all -- --check`、文档结构检查
（372 份 Markdown）及 `git diff --check` 通过。未运行其他 frontend 套件、codegen
行为测试或 native build/run。

2026-09-25 条件 leaf 的 phi 选择来源审计：直接形成时，内层两个 leaf 的条件
引用父根 `SaveOwnerSnapshot` 保存的 selector，父 capture 的来源槽也可由
`transport_read()` 定位。空 body 的下一条回边以 header phi owner 为来源；该静态
owner 没有快照，子来源条件转而读取 header 的独立 presence selector，而不再引用
原父根 selector。临时放行尝试的定向回归仍以 `EnclosingEnvironmentCapture` 失败；
已撤回放行代码并保留原子 deferred。解除前须逐入边证明形成时的选择写入 header
presence 位、回边沿旧状态转发、耗尽/退出复制该位，且所有实际根句柄同步运输。
内部 planner 回归核对 Entry 快照、来源槽及回边 selector 身份；它不等于动态执行验收。
本切片的内部定向 lib 回归 1 passed，`ownership_iteration` 172 passed；strict frontend
lib/iteration Clippy、`cargo fmt --all -- --check`、文档结构检查（372 份 Markdown）及
`git diff --check` 通过。未运行 frontend lib 全量、codegen 行为测试或 native build/run。

2026-09-25 条件 leaf 的 presence 真值表回放：内部 planner 测试对形成时选择的两个分支，
逐一从父根快照的 selector 计算 Entry，先读取旧状态再同时写入 header 的可用性与各级
来源存在位；随后移除形成时 selector，只用 header 位读取空 body 回边与 exhaustion，
并核对 exit 两个 leaf 位各有且仅有一个为真；另核对 Entry/Exhaustion 的静态根值来源、
exit 来源槽地址的根与位置。Entry 的根是 Snapshot owner，来源槽仍属于形成时的
Closure 布局；两种静态 ID 不相等，按当次实例句柄和槽原始位置读取。Exhaustion 的
根与来源槽布局都指向 header phi。回放初次因漏写 header 可用性位而失败，
补齐整条入边的 presence 写集后定向 lib 测试 1 passed。此初版回放仅证明该夹具的
静态条件位传播；当时快照选择在测试内手工赋值，未执行形成动作，也未运输根值、
捕获槽或动态环境句柄。随后扩展见下段；`EnclosingEnvironmentCapture` deferred 继续保留。
strict frontend lib Clippy 与 `cargo fmt --all -- --check` 通过；未运行 frontend lib
全量、codegen 行为测试或 native build/run。

2026-09-25 条件 leaf 的局部实例事实回放：内部测试对两个原始选择分别消费已发布的
`CreateClosureOwner`、`SaveClosureCapture` 和 `SaveOwnerSnapshot` 输入，在同一静态内层
lambda 下形成不同实例；从外层捕获槽移入内层后，快照从内层实际保存的子实例读取选择。
Entry、空 body 回边与 exhaustion 均先按旧状态求 `values().condition()`、来源实例槽和
presence，再提交根句柄、phi 来源槽与选择位；每次均从根句柄沿 `transport_read()`
定位被捕获的当次 leaf。两个分支的定向 lib 回归 1 passed。后续修订按各实际
`DropPoint` 的动作原序执行选中分支、base、outer 和 inner 的形成动作，核对每点的
动作数与表达式包含/先后关系；独立复审确认同点顺序错误会令回放失败。跨 callable
入口仍由测试把 outer 快照实例登记为 body 环境并清除临时控制选择，未消费真实调用
入口事实，也未回放释放。因此这只验证局部形成和三条 phi 入边，不满足第 9 节完整
动态验收，`EnclosingEnvironmentCapture` deferred 不变。

2026-09-25 紧邻环境的后代析构事实切片：以 `base` owned 捕获 MoveOnly `xs`、
`outer` 捕获 `base`、内层 `f` 再从 `outer` 环境槽捕获 `base` 的例子，内部 planner
原先只在 `f()` 发布直接 captured drop，漏掉 `base` 的 `xs`。释放遍历现仅对静态
唯一、直接含 owned move 且无 shared 输入的紧邻子来源沿实际父捕获槽位置展开，
为后代登记独立实例路径；原有 `Owner` 来源的版本条件保持不变。复审指出静态
唯一的已知 lambda 仍可能与 opaque closure 合流，因此未被已知子来源条件覆盖的
路径保留父槽 drop。非循环 known/opaque 回归核对两条父槽事实条件互斥且覆盖
整个调用路径；有循环的样例仍只检查 deferred 前的 planner 中间事实，公开
`EnclosingEnvironmentCapture` 继续原子拒绝。本切片没有运行时子句柄读取、
逐实例恰好一次释放或 native 消费，不解除第 9 节的完整验收门槛。
验证：定向 planner lib 10 passed，公开 `ownership_closures` 15 passed、
`ownership_iteration` 173 passed；定向 strict frontend Clippy、格式检查、
文档结构检查与 `git diff --check` 通过。未运行 frontend 全量、codegen 行为测试
或 native build/run。

2026-09-25 lambda body 入口环境事实切片：每个实际规划的 lambda body 在
`LambdaEntry` 首项发布 `BindClosureEnvironment { owner, closure }`，声明调用 ABI 传入的
当次环境实例应绑定到 body 的静态环境 owner；同点未使用 owned 参数仍随后析构。条件
leaf 的内部事实回放改为消费该入口动作，不再直接按测试推得的静态 owner 写实例映射；
非循环公开回归沿 owner snapshot 的输入核对入口形成 owner 与后续 captured drop 的
流动 owner 关系。调用模拟仍由测试从已形成的 `outer` 快照提供动态实例，
尚无 callsite→callee 的执行关系或 Phase 4 消费，
因此这只补齐入口边界的静态动作，不证明 SPEC-0211 第 9 节的真实调用实例运输，
`RecursiveClosureCapture` 与 `EnclosingEnvironmentCapture` deferred 保留。
验证：条件 leaf 定向 lib 1 passed，公开 `ownership_iteration` 173 passed、
`ownership_closures` 15 passed；`cargo check -p lang-codegen --all-targets`、定向
strict frontend Clippy、`cargo fmt --all -- --check`、`python3 scripts/check_docs.py`
与 `git diff --check` 通过。未运行 frontend 全量、codegen 行为测试或 native build/run。

2026-09-25 调用点环境传递的窄切片：`CallEntry` 在全部实参求值后、实际调用前发布
`PassClosureEnvironment { callee, closure }`，仅接受无条件、唯一具体 lambda 来源，
并要求 callee 是不可重绑的具名 binding，且全部实参求值后原 owner 仍由该
binding 无条件持有；若来源 owner 是快照，须恰有一个无条件输入指向同一环境布局。
`LambdaEntry` 既有 `BindClosureEnvironment` 消费传入实例，
内部条件 leaf 回放现从调用点动作读取当次 outer 实例再进入 body，公开回归核对
known/opaque 来源不被误认。实参直接或在条件分支中消费原 `val` owner 的回归
也不再发布传递动作。
可变 callee binding、复杂 callee 表达式及调用中重绑仍没有已求值实例的独立持有
事实，不发布该传递动作；旧单文件类型检查器对赋值
表达式仍给 deferred，不能以 `f((f = g))` 作为已检查调用的验收证据。这一切片
只连接前端静态调用入口与 body 入口，不完成递归实例链运输、释放或 Phase 4
消费；`RecursiveClosureCapture` 与 `EnclosingEnvironmentCapture` deferred 保留。
验证：条件 leaf 定向 lib 回放 1 passed，`ownership_iteration` 176 passed，
`ownership_closures` 15 passed；`cargo check -p lang-codegen --all-targets`、
定向 strict frontend Clippy、`cargo fmt --all -- --check`、
`python3 scripts/check_docs.py` 与 `git diff --check` 通过。未运行 frontend lib
全量、codegen 行为测试或 native build/run。

2026-09-25 递归根入边的写集切片：有限图仍登记静态 capture 布局，但递归 binding
没有可展开的树形 origin，因此 Entry、回边和 Exhaustion 的内部入边不再把整张图的
capture 槽列入 `capture_slots_to_clear`；它们只复制根句柄与可用性，后代值留在已
形成实例的 owned 边中。定向回归先因 Entry 的错误清空写集失败，修正后通过。
同一内部回归现从实际 `CreateClosureOwner` 动作分配初始与连续两轮的不同实例，
消费已发布的捕获输入和 snapshot；零轮沿 Exhaustion 根转发至 root drop，两轮
根来源保持 `[新, 旧, 初始]` 的自链。两轮后代释放仍是测试按图和实例槽模拟，
未执行 selector 的旧状态并行写集或缺席清空；planner **尚未**发布逐实例释放
动作。递归与紧邻环境的原子 deferred 不变。
验证：planner iteration lib 10 passed，continue/break 定向复跑 1 passed，公开
`ownership_iteration` 176 passed；定向 strict frontend Clippy、格式、文档结构与
`git diff --check` 通过。未运行 frontend lib 全量、codegen 行为或 native build/run。

2026-09-25 递归 jump 入边的内部事实回放：`continue` 连续两轮与 `break` 一轮均从
实际 `CreateClosureOwner`、`SaveClosureCapture`、owner snapshot/commit 的已排序动作
建立不同实例，分别消费 Entry、Continue/Break、Exhaustion 的根 phi 入边，再从已记录的
出口根 drop 沿实例捕获槽核对逆序链。两轮 `continue` 保留 `[新, 旧, 初始]`，一轮
`break` 保留 `[新, 初始]`，捕获候选均匹配有限图中的实际子实例；没有后代被 phi
清空或重指。定向 lib 测试 1 passed。释放遍历仍由测试模拟，生产端尚无递归逐实例
release、shared capture loan end 或最后借用者动作；未覆盖交替 lambda、同一父环境
两条同节点子实例路径与缺席清空，`RecursiveClosureCapture` deferred 继续保留。

2026-09-25 递归根释放动作的内部产物切片：根 `DropFact` 的 owner 若为当前循环中
有 closure 根候选、但未展开树形来源的具名 phi binding，清理序列改记
`ReleaseClosureInstances { statement, root }`，不再同时记普通 `Drop(root)`。这保留
根的析构点、路径条件和循环图身份，供后续从实际实例捕获槽遍历；不把静态图节点或
固定捕获路径当作运行时深度。失败先行的自链根测试由缺少该动作转为通过，
continue/break 根也核对相同动作。动作目前仅为根入口，尚未逐实例执行 owned
后代、shared loan end 与最后借用者查询；其他闭包形态的根释放也未闭合。
`RecursiveClosureCapture` deferred 继续原子清空公开计划，不能据此宣称递归释放完成。
独立复审发现同一递归图会使该循环所有 phi 停止树形展开：独立闭包根也需要该动作，
否则解除 deferred 后可能漏掉其 capture。新增混合根内部回归让独立闭包实际 owned
捕获 leaf，核对其无环图路径、形成槽与递归根各有一条实例释放动作、无重复普通
Drop；定向测试 1 passed。Phase 4 预检已显式拒绝
该动作，防止以后仅消费平面根 drop 时静默漏掉后代。
验证：planner iteration lib 11 passed，公开 `ownership_iteration` 176 passed，
`cargo check -p lang-codegen --all-targets`、frontend/codegen lib strict Clippy、
格式、文档结构与差异检查通过。未运行 frontend 全量、codegen 行为或 native build/run。

2026-09-25 递归根动作的定向回放：零轮、连续两轮、continue 与 break 的内部测试
现在从 `ReleaseClosureInstances` 读取根 `DropFact` 和循环图身份，再从实际形成动作
保存的 owned capture 槽沿候选图节点逐实例遍历。回放检测重复到达，单捕获链
先记录子实例再记录父实例；两轮自链的记录为 `[初始, 旧, 新]`。多捕获槽的
逆序未由单链夹具证明；递归相关 lib 定向过滤 4 passed。该遍历仍是测试代码，
生产动作没有逐实例执行、shared loan
结束或最后借用者查询；deferred 不变。未重复运行公开套件或 native 验收。

2026-09-25 双槽逆序补证：同一混合根夹具的独立 `g` 现 owned 捕获两个不同
lambda leaf。内部回放按实际 `CreateClosureOwner` / `SaveClosureCapture` 动作顺序
建立父子实例并消费来源，核对两个恒真捕获条件、同点形成动作及原始位置分别
对应 `first_leaf` / `second_leaf` 的不同子句柄；Entry 与
Exhaustion 转发根后，从 `ReleaseClosureInstances` 读取根和图，记录释放顺序为
`[第二子实例, 第一子实例, g]`，每实例一次。定向 lib 测试 1 passed。两个子实例
来自不同 lambda，不能替代第 9 节“同一 lambda 两子实例”验收；生产端仍未执行
该动作，shared loan 和 native 未验证。

2026-09-25 同节点双路径的内部入边切片：以首个循环形成两代
`move { read(xs) }`、外层环境分别捕获 `first` 与 `second`、后一个循环运输
`outer` 的现有公开 deferred 夹具，内部测试确认父环境两条捕获位置都含同一
lambda 图节点，且形成动作分别保存来自两个不同 phi owner 的 owned 值。
失败先行的断言显示后一循环仍按静态节点展平清空共享的后代槽；现于识别
并存实例时，Entry、回边及 Exhaustion 内部入边只保留根值和可用性，
同时省略后代清槽列表与展平来源写入，避免旧槽残留或两条路径互相覆盖。
定向内部测试 1 passed，公开 `same_lambda` 两项 2 passed；strict frontend lib
Clippy、格式、文档结构与差异检查通过。独立复审确认入边内的清槽与写入一致，
公开计划仍由 `AmbiguousClosureInstanceTransport` 原子 deferred。预分配的 phi 来源选择位
尚未从实际实例赋值；本切片没有回放前一循环的两代形成、后一循环的根转发、
逐实例释放或 shared loan，不能作为第 9 节同 lambda 双实例验收。未运行 frontend
全量、codegen 行为或 native build/run。

2026-09-25 同 lambda 两代的内部回放补证：上述夹具现按第一循环的实际
`CreateClosureOwner`、`SaveClosureCapture`、owner snapshot/commit、Entry、
Fallthrough 与 Exhaustion 动作回放两轮。第二轮结束时 `first` 与 `second`
分别持有同一静态 lambda 的新旧不同实例；外层 `Create` / 两个 `Save` 从两个
exit owner 读取并保存不同子句柄。后一空 body 循环沿 Entry、两次 Fallthrough
和 Exhaustion 转发同一个父根，已保存的两条子边保持不变。最终测试按内部平面
`DropFact` 的实例地址与捕获路径逆序消费两个内层 `xs`、两个子环境及父环境；
循环内两个被替换的初始空环境也按实际 `Drop` 消费，五个已形成闭包实例各一次。
定向 lib 测试 1 passed。独立复审确认回放的动作来源，同时指出后一循环回边
只是根槽自转发，尚未求值一般条件选择或执行多 binding 的旧状态并行复制。
这些是私有 planner 中间事实；公开结果仍以
`AmbiguousClosureInstanceTransport` 原子 deferred，不能当作已发布的实例级
释放或第 9 节完整验收。shared loan、条件选择、native 仍待实施。

2026-09-25 shared 双实例候选事实补证：另一夹具让同一 lambda 在第一循环
连续两轮借用同一个 `xs`，随后把两个不同环境实例捕获进同一个 owned 外层闭包。
内部测试沿 `xs` header phi 的 Entry 入边追到 `listOf(1)`，核对回边保持同一
source，并按两轮 `Create`、`Save`、snapshot 与 phi 入边构造两个子实例。外层
`outer()` 返回点的两个 `EndCaptureLoan`、`TestLastCaptureLoan` 与受最后借用者
true 分支保护的 retained-source drop 候选，分别对齐捕获路径 `[1]`、`[0]`
的 source、slot、owner 与条件。把这些返回点候选动作视为激活时，私有模型得到
loan 数 2→1→0，且仅最后一条路径进入 drop 候选。定向 lib 测试 1 passed；
独立复审确认身份对齐，并指出测试未求值所有 selector，其他表达式后还存在
带条件的 End 候选。因此这里尚未证明具体执行路径上恰好释放一次；公开计划
仍原子 deferred，shared 条件运输与 native 仍待实施。

2026-09-25 shared 双实例的条件回放补证：对上一夹具逐条执行形成快照的 selector
copy，并按旧状态并行求值两轮循环的 Entry、Fallthrough、Exhaustion presence。
失败先行暴露后一循环在检测到并存实例后连根与后代的 origin presence 一并丢弃；
内部入边现保留各层 origin、根实例及捕获路径的来源读地址，同时省略会合并两条
子路径的静态目标 capture/owner 写入。测试从实际父环境实例沿位置 `[0]`、`[1]`
读到两个不同子句柄，检查三条入边的目标 presence 条件与 `transport_read()`；
`transport_value()` 不再把仅用于寻址的来源槽暴露为可直接复制的静态 phi 值。
早期 `EndCaptureLoan` 位于 `second` 快照及旧值清理之后、新内层环境形成之前，
在两轮该动作点的选择状态下均不执行；`outer()` 返回点按已写入的 selector 逐动作
执行两个 loan end 与 last-loan 查询，私有回放得到 2→1→0 且只选中一次
retained-source drop 候选。这补齐了该夹具的条件路径证据，不代表公开 runtime
已执行释放：`AmbiguousClosureInstanceTransport` 仍原子 deferred，递归链、一般条件
合流与 Phase 4/native 的实例级消费尚未验收。
定向 lib 三项分别 1 passed，公开 `ownership_iteration` 的 `same_lambda` 过滤器
2 passed；frontend lib 与该 integration target 的 strict Clippy、格式、文档结构
和差异检查通过。未运行 frontend 全量、codegen 行为或 native build/run。

2026-09-25 条件菱形的私有回放补证：`flag` 的两条互斥分支分别把旧 `base`
环境捕获进 `f` 或 `g`，再形成持有二者的 `outer`。内部测试从 `outer` 的实际
snapshot copy 回放选择位，核对 Entry 与 Exhaustion 都保留同一 `base` 图节点的
`[0,0]`、`[1,0]` 两条候选路径，但每次仅激活与形成分支对应的一条。零轮和两轮
空 body 回放均按 Entry、Fallthrough、Exhaustion 的 value source/target 转发同一
父实例，并核对所选嵌套环境的 `instance_root` 指向复制前的父 owner，已发布的
`transport_read` 地址根与捕获路径一致。Exhaustion 不清空这些并存路径的静态捕获槽。
定向内部 lib 测试 1 passed，公开菱形 deferred 测试 1 passed。此测试仍以私有
planner 事实模拟实例和值；没有发布一般条件合流的实例运输或逐实例释放，公开计划仍为
`AmbiguousClosureInstanceTransport` deferred，不能据此关闭 Phase 3 或申报 native 验收。
`cargo clippy -p lang-frontend --lib -- -D warnings`、格式、文档结构与差异检查通过；
`--lib --tests` strict Clippy 因本轮范围外既有测试中的 `match_like_matches_macro`
与 `cloned_ref_to_slice_refs` 两项告警失败。未运行 frontend 全量、codegen 行为或
native build/run。

2026-09-25 条件菱形的根清理事实补证：上述内部夹具在 `flag` 两分支、零轮与两轮
空 body 的四种组合中，按已保存 selector 选择出口根同点的 drop 序列。每次只有
一个 `base` 路径释放其 owned `xs`，另一路仍释放未捕获 `base` 的空环境；两条外层
捕获边按逆声明顺序释放，最后才释放根，共四个 captured drop 与一个根 drop。
测试逐项核对实例地址的 exit 根、原始捕获路径/位置和槽所属 lambda/source；叶子
`DropFact` 与 Exhaustion 读取边使用不同静态 source owner 定义，二者各自关联对应
环境定义及相同的 lambda/source，不能以 ID 相等代替动态实例运输。定向 lib 测试
1 passed。此为私有 planner 事实的条件回放，不是已发布清理计划或 native 析构；
`AmbiguousClosureInstanceTransport` 仍原子 deferred，递归释放与 shared loan-end 的
完整执行尚未验收。格式、文档结构与差异检查通过；一次带两项既有告警豁免的
`--lib --tests` Clippy 误覆盖全套 integration target，已主动中断，不计通过。
未重跑 frontend 全量测试、codegen 行为或 native build/run。

2026-09-25 交替递归捕获的私有实例回放：`f`、`g` 交替捕获旧环境的夹具按实际
`CreateClosureOwner`、`SaveClosureCapture`、snapshot/commit 和 phi 入边建立零轮、
一轮、两轮的实例链。测试核对两条循环图边、形成前的 capture 读值、快照选择位
复制和回边根句柄转发；从 exit 根沿实际保存的 owned 槽逐实例释放，三条路径分别
得到 `[g0]`、`[g0,f1,g1]`、`[g0,f1,g1,f2,g2]`。旧 `f` 的形成点与循环出口
释放条件按各自动作点求值，避免把形成快照后才存在的 selector 错用在零轮路径。
定向 lib 测试 1 passed。释放遍历仍是测试回放，生产端尚未执行递归释放或 shared
loan end；`RecursiveClosureCapture` 的公开产物继续原子 deferred，Phase 4/native
也未消费这些事实。此例没有覆盖交替链的 break/continue 或一般条件合流。
递归捕获 lib 过滤器 4 passed，公开原子 deferred 回归 1 passed；独立只读复核
确认实例边、快照时序与释放回放，未运行 frontend 全量、codegen 行为或 native build/run。

2026-09-25 交替递归链的跳转入边补证：另以两轮 `continue` 后耗尽和一轮条件
`break` 分别读取实际 jump incoming。每轮按 `CreateClosureOwner`、
`SaveClosureCapture` 与 snapshot selector copy 形成 `f`/`g` 新实例，再按 jump
入边转发新 `g` 根。测试锁定实际动作顺序、执行点、snapshot 捕获输入，核对
`break` 的 flag true/false 选择及 jump 入边没有清除后代槽或展平后代写入；被移走的
`f` 在目标边没有值，exit 根的释放 guard 可选中。沿已形成实例槽分别释放
`[g0,f1,g1,f2,g2]` 与 `[g0,f1,g1]`，`g` 根链无重复或遗留捕获边；独立的
初始空 `f0` 不在这条链内，本回放未核对其单独清理。定向 lib 测试 1 passed。
仍是私有 planner 回放，不改变公开递归 deferred，也未执行生产级递归 release、
shared loan end 或 native 消费。

2026-09-25 条件 leaf 紧邻环境发布切片：`base = if (flag) (move {}) else (move {})` 被
`outer` 捕获、body 内 `f = move { base() }` 再跨空 `for` phi 的场景原以
`EnclosingEnvironmentCapture` 原子 deferred。调试实证根因：exit phi 经 `seed_phi_origin`
后两个 leaf 候选条件变为 `Choice{phi(Header,slot) presence}` 与各自 presence 的组合，即
`header_presence ∧ leafA_presence`、`header_presence ∧ leafB_presence`；两个 presence 位
相互独立，条件表无法证明 `leafA ∧ leafB = NEVER`，形成时由 `flag` 建立的互斥在 phi 转发处
丢失。门禁恢复基线两个触发意图——父槽仍有 owned 子环境（始终 deferred）与同一
`input.source` 的多候选——仅在“同源多候选且不能安全读取父实例槽”时 deferred；放行条件
要求候选互斥且父环境已保存形成时选择（snapshot `source_value` 或 owner 为
`IterationPhi`）。`mutually_exclusive` 增加结构判定：同一 capture source 的多个候选是同一
槽位的互斥取值，同时始终执行布尔 `and()` 以保留条件表注册副作用。
`loop_phi_publishes_conditional_leaf_after_phi_carries_choice` 由失败转为通过，并恢复首版
提前 `return` 跳过 `and()` 副作用导致的 `loop_phi_preserves_leaf_enclosing_closure_capture`
等四项回归。定向七组 `ownership_iteration` 176、`multifile_ownership_checking` 71、
`ownership_checking` 29、`ownership_nullable_when` 26、`ownership_closures` 15、
`ownership_containers` 12、`ownership_construction` 8，合计 337 passed、0 failed、0 ignored；
`cargo clippy -p lang-frontend --lib --test ownership_iteration -- -D warnings` 与
`cargo fmt --all -- --check` 通过。公开计划已加语义断言：Entry 必须把形成时的控制选择写成
两个互补的 leaf presence 位（在 `flag` 两个分支下各恰好一个 leaf 存在且指向不同 target），
Exhaustion 必须从 header phi presence 转发而不是重读源码控制选择；测试不再只断言事实非空。
**剩余边界**：逐实例清理顺序与“恰好一次析构”的动态回放（零轮/多轮/break/continue/return）
尚未补齐；结构互斥判定依赖“单 source 任一时刻只持一个值”的语义假设，待独立评审。未运行
frontend 全量、codegen 行为或 native build/run；`RecursiveClosureCapture`、
`coexisting_capture_phi`、`conditional_nested_phi` 仍原子 deferred。
