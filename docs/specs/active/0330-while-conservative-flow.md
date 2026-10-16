# SPEC-0330: while 条件收窄与保守循环流事实

> **性质**：实施合同 · **状态**：in-progress · **读取时机**：准备 while 的类型流事实实现与验收时 · **唯一真源**：本片实施范围；语义以 Guide 为准

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P2-0330` |
| 基线 | 真实 main `3a79fef25c3e2dc8827a410f96b65f01b3fbb5e2`，已合0329 |
| 分支 | 本地 `feature/spec-0330-while-flow`；授权远端 `fix/spec-0330-while-flow` |
| 依赖 | [0325赋值](../../archive/specs/0325-simple-variable-assignment-types.md)、[0326分支合并](../../archive/specs/0326-closed-control-branch-join.md) |
| 合同 | [Guide06稳定来源与流事实](../../guide/06-blocks-control-flow.md#类型测试与流事实) |

## 1. 范围

落实已启用 v0.44 的 while 保守合同，不修改 Guide：循环头清除可能被循环写入或回边
改变的旧事实；每次条件成立仅将稳定 place 的 true facts 传给 body。赋值按声明类型
先检查 RHS 后 kill；normal/continue 不把本轮新收窄直接带回头。退出不新增 condition-false
或 break 推导事实，既有必须 kill 的来源仍失效，稳定不可变来源规则保留。
零次执行与嵌套循环适用同一规则，不启用固定点、borrow var、完整 SSA 数据流或 NLL。

## 2. 当前缺口与最小边界

single `checker/item.rs` 的 While 与 unit `checker/control.rs::check_while_statement`
当前只按 Boolean 检查条件再调用 body，未把条件 true facts 传入。复用现有 condition_facts、
stable key、赋值/未知 call/capture 的 kill 规则；需在真实普通源码红证后冻结头部与出口策略。
不能直接把 if 的 false 分支合并用于 while，也不能将本轮 body 末 facts 当下一轮入口。

本片不改变 AST、ownership 借用规则、loop drop/SSA CFG、nullable ABI、Map、Range provider；
不顺带修 0326 已认领的变量初始化 Nothing/falls-through 后继。若合法类型收窄暴露已有
下游缺口，先记录精确源码与阶段，再按本片完整交付所需范围独立审查，不吞 gate。

## 3. 普通源码验收矩阵

1. 双入口 nullable/enum 稳定 place 在 while true body 收窄；普通 if 对照。查询精确读取类型，
   不以无诊断替代事实；enum case 显式类型参数，不混入尚未启用的 case 省参推导。
2. body 内先合法读取，再按声明 storage 类型赋值，后续读取失去旧事实；合法与非法消费成对。
3. 零轮、condition false、break 出口不新增事实；不可变既有事实不被任意全清。
4. continue/normal 回边和嵌套 while 不泄漏本轮新 facts；头部针对可能写入的来源保守 kill。
5. 未知 call/capture、非法 RHS、既有 if/when/短路及 stable-place 负控，锁原诊断/span。
6. 后端可执行的普通有界 Int/Boolean/enum 控制流，single/project native 精确输出；旧 while
   cleanup/ownership 回归。一般 nullable ABI 拒绝单列，不凭前端绿宣称 native 全面支持。

首批只采前三组普通前端 exact；所有源码先收集 names/raw/complete 再断言。未写生产前
单独审实际首红及当前已满足的控制，后续补齐 4–6，不把首批当完整验收。
私有 facts/schema 损坏只交 CI，云只编译；不运行校准、故障注入或聚合测试。

## 4. 实施与交付

Guide 已启用，草案不要求新增语义决策。按 draft→active、测试红→小片生产→普通绿、
精确共享回归、有限工程门禁、双平台 CI 的顺序推进。首红前未运行本片测试；实跑结果见 §5。
每步冻结 head/tree/输入 SHA、唯一 exact 名单、实际命中及历史失败；小提交以 binary/full-index
format-patch 增量恢复核验。实际远端发布、PR、归档与 normal merge 由唯一 publisher 执行。
本片远端阶段备份/PR/审验后 normal merge 已获用户明确授权；不改0329冻结树。
首批新增 while_flow_facts 三个普通 exact，11 段源码双入口先采齐再断言；已通过有限
Rust 编译预检；该首红前状态的后续实跑见 §5。初次 docs 错误锚点已机械修复，保留失败日志。

## 5. 首批真实红测

`57801015` 首三 exact 各实际 1 命中且行为红，11 输入前后 SHA 相同；4/8/10 份观察
覆盖全部 11 源码双入口，lex/parse/names 均无诊断。while 条件 true 后 body 仍为 Int?；
循环可能写入的外层 var 在头部仍沿旧 Int 事实，分别构成真实缺口。独立 if、赋值 storage/
kill、零轮/false/break 不新增事实及不可变外层事实对照均保留实际观察，不以首红遮盖。
收据 SHA `c17a1fbeab29fe57bb781946f6ee5398ccacc58632cd2d812564ff11c3368df6`。
生产前还须采条件内 inout kill 前后顺序、enum/nested/continue；现有 condition_facts
事后重取可能复活失效事实，不能只在 while body 入口机械加入未经副作用筛选的 true facts。

后续普通采证补齐条件中 inout 的先后顺序、body inout、普通调用、
嵌套循环与 continue，以及 enum case payload 的 while/if 对照。
这些前端采证不替代后端/native 与完整退出清理验收。

## 6. 条件副作用补证与最小实现

`9df4e459` 后三普通 exact 各 1 命中行为红，8/6/4 观察的前置阶段均无诊断；
先 inout 再重新判 null 的 while body 未收窄，嵌套/continue 与 enum payload 同样缺事实。
条件先判 null 再 inout 的负控现状通过，但被 while 尚无收窄掩蔽，不能证明 kill 已正确。
`c284006b` 共享条件 exact 采齐 14 观察后首负控行为红：if/OR/短路恢复/嵌套 AND
可重新引入 inout 前的 Int 事实；正序重新证明与普通 inspect 未写控制保留观察。
收据 SHA 分别 `26f54e5873e0361b073403c7411ec83b6720c2dee5d7d990245f6cd40f2516c1`、
`1fd4dbc16918a933edd687ac83fbb33d8f184be4a7196c8d0e70e1a2dcb16272`。

实现限定：纯 AST 扫描实际求值子树的 assignment/inout 候选，由各入口 stable key 解析；
跳过 lambda 声明体，不做 callee 推演。while 头旧事实扣除条件/body 潜在写入，
body 使用条件求值后的有效 baseline 与有序 true facts，出口只保留该头基线。
共享 AND/OR 的右执行路径扣除右侧写入后再合事实；短路恢复也扣除右潜在写入，
unit 显式 inout 调用边界与 single 对齐。不扩大 unknown-call 全清或 stable/capture 资格。

## 7. 有限前端阶段验收

固定 `09e6fd27` 的 13 个普通 exact 各实际 1 命中绿，27 输入逐轮前后 SHA 相同。
8 个本片 exact 包含 raw/complete 双入口，覆盖上述头/body/出口、AND/OR/inout 顺序、
initializer 扫描、捕获资格及非法 RHS 诊断保全；5 个共享 exact 仅 raw/typed.validate
回归，不将它们描述为 complete 或 ownership/native 验收。
收据 SHA `83cd1deb0672f25aeefcec59666e0f8b40dc7080a94c9e247eadf37aade88016`；
四工程 fmt/check workspace all-targets/clippy 三包 all-targets/build CLI 均实际 exit0，
收据 SHA `86646148d6e3b285759893a7d8b9516fc0031227b89119068a84dd70792ae677`。
实际 main 尺寸门禁通过；阶段 docs 初次因架构入口 207 行超限失败，
改为既有入口简短导航后重检。历史行为红与该文档失败的原始证据均留仓外。
此包只作已授权阶段备份，仍 active；普通 lower/LLVM、CLI/native 与完整 CI 待后续。
一般 nullable ABI 和普通 is 条件后端支持必须逐源采证，不能由前端绿推定已支持。

## 8. 普通 downstream 首轮采证

固定 `2690dafc` 四源码双入口全部通过 complete 与 ownership 后在 lower 停止；
唯一 exact 实际 1 命中行为红，8 个结果在断言前完整保留，尚未到 LLVM。
enum normal/break 均在普通 `current is Shape.A` 返回 UnsupportedNode；
nullable continue 的 single 在局部 nullable 初始化返回 UnsupportedNode，
unit 在 `current = null` 返回 MissingFact；nullable return 两入口返回 InvalidSsa。
这些是不同的实际停点，不能统一解释为 while true-edge proof 缺失。
收据 SHA `3bb47b4a3ac385bfaa219a1dbd66ab96a688222d73fb65da14586e8fc2a72054`。
下一定位片仅在既有 verify 失败路径以 cfg(test) 打印真实错误与 SSA；不修改事实、
验证次数或错误传播。定位后撤下该临时追踪，再以普通源码回归验收。

`66b6de30` 同一 exact 的只读追踪采齐 8 结果后仍行为红；两入口 InvalidSsa
均为 `current.value` 的 HeapPayloadPlace 使用 NullableHandle 而非非空 owner/view，
verifier 返回 OperationContract。未观察到该 case 的 cleanup/dominance 错误，不能据此
假定后续均无缺口。追踪收据 SHA
`8e68412313915992195e0da89c6a41d73db7e684ccd8928ebeada0834a9d08b5`；
本次定位后已撤下全部临时日志，两 orchestrate 恢复采证前字节。

最小 nullable 对照 `e5b47ab5` 采齐 8 个入口，仅观测 exact 完成：无 while 的
nullable 构造局部在两入口各拒绝，参数来源转局部再赋 null 两入口 SSA 通过；
同参数来源在 while 内只赋 null 的 single 通过、unit 在 while span 返回 MissingFact；
字段读取后 return 两入口 InvalidSsa。该组不记作 8 个能力正例；原 `3561d220`
因静态发现 println(Int) 不符现有 String 签名未执行，先更正源码再采证。
观测收据 SHA `08b936366900885203ebe02c4afdae3b0e9a4224122b4876ae31b27ea3fa9761`。

`760facd5` 仅让 unit 普通 Name 赋值消费、核验真实 assignment descriptor 的
声明/storage 类型，保留 RHS 转移、drop 与旧 owner guard。原 full4 与对照4
同 head 复验：完整 nullable continue 的 unit 越过赋值 MissingFact 到 InvalidSsa，
其余停点不变；新增 InvalidSsa 尚待具体错误确认，不直接断言与旧 proof 错误同根。
收据 SHA `c573f945ce37bc2acd5921d7de7e943a09652fb0aeea5f20b88ae9e3ed858542`。
## 9. 有限 enum is 与 nullable direct-local 补片

两入口 ordinary enum TypeTest 仅以当前真实 TypeRef/case/root 实例和登记 tagged identity
比较 discriminator；when 的现有 tag 发射抽为私有 helper。末次 named owner 读取先读取
tag 再执行原 operand/Group cleanup；缺少既有 temporary cleanup 的 MoveOnly 临时仍拒绝。
Single nullable-local 需求仅来自本函数真实 owned direct-construction plan，并按实际
Variable storage 与当前 substitutions 走既有 mapper；未扩 factory/null/if initializer。

首轮 `4202e0ab` 的四个 exact 各 1 命中：原 enum while 两源码双入口进入 LLVM；
nullable while 四入口仍 InvalidSsa。新增 String-payload enum 末次读取 single 四项成功，
unit 因普通 TypeTest 未触发内部 Int tag 类型登记而 MissingFact；`02931cef` 单独补齐
真实 EnumCase TypeRef 的既有 Int/Boolean 登记。原始红证完整保留。

同源旧 `f85ac8cf` baseline 证明 `val _ = Holder(42)` 两入口原有 MissingFact，primary
为下划线：single 53..54/unit 63..64。静态链为 Named(_) AfterStatement drop 与 lowering
不保留 discard binding 的既有不一致；它是待后续 ownership/lowering cleanup 片处理的
明确缺口，不由本片扩修，且不把该拒绝写成规范要求。direct-local 对照保留精确旧边界。

切换 baseline 与主 worktree 后共享 Cargo target 曾复用旧测试二进制，新增 exact
实际 0 命中被 runner 拦截；该轮四项均作废，仅记 harness 失败，不计代码行为红。
经授权保持源码 SHA 不变触发当前 crate 重编后，以新目录重新采证。后续跨 worktree
优先隔离 target，不能仅靠库路径或源码 SHA 推断测试产物身份。

旧基线收据 SHA `24658d6132ba3b63be7fcc074609471b0a9be08a9de8af396d4857c88664b572`。
当前 crate 实际重编后的 `02931cef` 同四项收据 SHA
`3ada84f31c712307b872c3b46cbcdec2c1447dab28290be72b32cbfcbbafe2df`：
末次 enum Read 八入口全部进入 verified SSA/LLVM；direct-local 前三源码六入口全部通过，
discard 两入口仍原 MissingFact；完整 while 四个 nullable 入口仍待 true-edge proof。
这一阶段不计 native 或完整 C 完成。后续 tests-only 将 discard 作为已证旧边界锁定，
另加强真实 tag owner 的唯一 Drop 与指令顺序，不将已知拒绝当规范正例。
