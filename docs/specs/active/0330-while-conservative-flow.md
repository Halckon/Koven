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

固定 `1314665d` 八个 ordinary exact 各实际 1 命中，54 输入逐轮前后 SHA 相同：
原完整 while exact 保留 nullable proof 四入口 InvalidSsa 的真实红，其余七项通过；
其中 nullable-boundary 一项仅表示观察完成，不能作为所有入口能力绿。独立 enum
末次读取八入口现精确断真实 tag owner、同 block 唯一 Drop 及 tag 在前；四个共享
when/owned assignment/nullable storage/unreachable-inline 回归通过。收据 SHA
`8903a4e9cc9333494991abd55453c134ecac9c84fbed6ce658dffec45ddb9004`。
同 head 四工程 fmt/check/clippy/build 全部 exit0、输入稳定，收据 SHA `707e17a999a2cc10501219e6c8ca2b5513ab5dc79158b17fdbe021fb303efd33`；
docs701 与实际 main size 门禁通过。此批仅备份已验 enum/storage 必要片，仍 active，
不创建完成 PR、不称 nullable while proof 或本片 native 已完成。

### Nullable proof 跨 CFG 首批完整观察

固定 `ad7ead73` 的七个 ordinary exact 各实际 1 命中并行为红，66 输入前后同；
20 源码双入口共 40 结果均先采齐：29 停 lower、8 停 complete、2 停 ownership，
1 个 single owner-return 已通过 SSA/LLVM。收据 SHA
`81966b42dcae616a367a46c7707bd21f33d35ccde3975a6b95a64f50d4b399c4`。
同根只读内层和 pending-field 只读内层的拟正例，各两入口因再次比较裸 null 得到
L0083，先列前端事实/expected 边界，不冒充后端 proof 失败。旧事实读取负控为
complete L0165，pending 字段借用期间写 owner 为 ownership L0135 并保留来源 label。
每组首个正例断言失败会遮住后续负控断言，完整观察不等于这些断言已通过。
后续先审精确 owner/inner/loan 身份运输，再闭合每轮 proof 产生、嵌套继承、
替换与各出口失效；本轮不执行 native，不改变已备份 enum/storage 生产。

固定 `d351af61` 的 metadata17 采证：八个旧 nullable/assignment 普通回归全绿，
新增 FE 正向四源码八入口因 L0083 真红，八邻控十六入口完整断言绿。
proof 新增 when 源码缺分支分隔符，#11 在 parse 前置失败；该项不能计后端红，
其他六组 34 观察保留实际阶段。收据 SHA
`b95d39160c6950af24d5005362defde19e6c535bf0e67a4f71548d046c62c01c`。
single 三元 metadata 普通回归成立；unit 空表 scaffold 没有 proof 出生，
其普通绿仅证明空态回归，不能证明非空关联或尚未构造回边的有效性。
本次先修 when fixture 分支换行；null expected 的受限 storage fallback 单独审查。

固定 `a6e7b75b` 的 metadata17-v2，各 exact 实际 1 命中、77 输入逐轮前后同。
八个旧 nullable/assignment 回归与末两 FE exact 通过；FE 正向八入口和邻控十六入口
完整断言通过。新增 expected fallback 仅当前 stable symbol 的真实 nullable storage、
active fact 与实际 inner read 一致时用于裸 null，不改收窄 read 或一般 null 推导。
七个 proof exact 仍在正例 lower 要求处行为红；其先行精确负控已执行通过。
修正 when 分隔符后，21 源码双入口 42 观察完整：35 lower、4 complete、2 ownership
拒绝和原 single owner-return 一个 SSA/LLVM 成功；同根只读内层与 pending-field
内层已越过原 L0083 前置，不能因此称 while proof 已实现。收据 SHA
`f56e8a69e9e002430e9f443440f92d606f77623b35c0ca16128e8ce91bb37d33`。
同 head 四工程 fmt/check/clippy/build 全部 exit0、77 输入稳定，收据 SHA
`b7f17a7f8b3cd8e0770f3f57a8816daadae5741e58a3d8a478d794f6ca92f7f5`。
本阶段 single metadata 保留真实 owner/inner/loan 配对；unit 仅空表运输 scaffold，
没有新增 proof 出生。后续必须核实际 NullableBranch 出生、非空运输和回边关联、
活跃子 loan、各出口与 owner 替换失效；当前仍不计 native、最终 CI 或本片完成。

single 直接 proof 首片先锁五源码的 normal、continue、Group、break 与 Int return。
`9159d9bd` 的十入口完整观察中 single 五项均 InvalidSsa，unit 四项 InvalidSsa、
break 为 Unsupported；收据 SHA
`e710a5d17daef7bb94d3db098d67ef688bdc520d894c80ff069458da45e5d9fe`。
初候选 `11d53bbc` 同九项得到八旧回归绿、direct 红：入口 `flag: Boolean` 的真实
Shared 参数 loan 被保守 carrier 门挡住，single 五项均整 while Unsupported。
保留该失败收据 `e3cba5ac2ca16e18ec18e9ee6f791765582a5ad2a2f850befb1fe7fe9c9e1590`。
后继只允许实际入口参数、Shared、Boolean 三重身份的独立 loan 保持原状，未清除或结束
它；其余未决 carrier 门继续拒绝。新 CFG 入口另核 owned NullableHandle 的 inner
确为 HeapOwner、原 owner Value 型正确，仍不扩大 Rc/String 等 handle 支持。

固定 `e312370d` 的同九项全部各实际 1 命中通过，82 输入逐轮前后同。single 五源码
通过 SSA verifier、内存 LLVM 与只读身份 oracle：真实 NullableBranch 的 owner/view
配对、Int 字段读取先于同 loan 的 BorrowEnd 和同 owner 的 Drop；前三源码回边 owner
列确为本轮 NullableNull，且不携带本轮 proof。旧 single nullable 六项及借用结果、
`!!` 后续 transfer 两项普通回归均通过。十入口仍完整记录 unit 原四 InvalidSsa 与
break Unsupported，未将 unit 观察计作成功；收据 SHA
`ffdc515a143fef41734d98c77f782887aafd64f20f7293392d131949f4fd1cef`。
同 head 四工程 fmt/check/clippy/build 全部 exit0、82 输入稳定，收据 SHA
`3a1d9f966d731edf019d78d0507f02c22d50cce69541a96893bb9ac9a196de8d`。
本片复用既有 carry/cleanup，只新增 While-origin 的有限出生/退休和直接 Int 字段读取；
旧 If/When 的提前清理时序保留。已有 proof 入场、同根 retest/take、未决子借用和复杂
carrier 继续明确拒绝。原 42 入口矩阵及最终合同保留，unit 非空运输、复杂嵌套、
子借用退出与 native 仍待后继逐项闭合；本阶段只作增量分支备份，不创建完成 PR。

unit 同五个直接源码于 `88e928d6` 采真实红，十入口全部先观察：single 五项
SSA/LLVM 成功，unit 四项 InvalidSsa 与 break Unsupported；唯一 unit exact 失败。
收据 SHA `8b3977c526d9055661e9826c29be89a4c2e198d38ee6688e4d95347cbd0901d3`。
后继复用既有 carry/cleanup，让真实 NullableBranch 的成功 owner/view 配对产生
unit proof；origin 保持出生身份，当前 owner/loan 沿实际 edge 列运输并核对。
入口 Shared(Boolean) 参数按真实 root 保留，闭包路径不取得该资格；其他未决 carrier
继续拒绝。HeapOwner 与入口 owner Value(nullable) 资格在创建 CFG 前检查。

固定 `0f57aa10` 的十四个普通 exact 各 1 通过；同 head 工程 fmt/check 通过，
clippy 因测试 helper 的 let-and-return 失败，build 尚未运行，原日志保留。
仅机械直返 collect 后，固定 `fa723a33` 同十四项重新各 1 通过、87 输入逐轮稳定，
收据 SHA `1a978712819ce2e1c28f8bc5a9d5446f0fc409b623a20b1f06aeb4c1eff83e39`。
两个 direct exact 分别完整观察十入口，再验证各自五源码的真实 SSA 身份：
owner/view、字段读取至 BorrowEnd/owner Drop 顺序、回边 NullableNull owner 列，
unit 另核入口 Boolean loan 的真实运输列且不被本轮 proof 退休结束。其余十二个
旧 nullable、借用交付、while、赋值 RHS 与 nullable storage 普通回归通过。
同 head 四工程 fmt/check/clippy/build 均 exit0、87 输入稳定，收据 SHA
`5e6e6eb282390a906c9e3a6452fc1ce89965f7890e28f170b58d5370ddacfb41`。

同 `fa723a33` 再跑原七组 21 源码双入口，42 结果全部收齐：13 SSA/内存 LLVM
成功、23 lower 拒绝、4 complete 与2 ownership 拒绝；精确负控先于正例断言。
normal/continue/Group 组通过，其余六组仍行为红，未改正负 oracle。
收据 SHA `4fc0370b5cf7764e06a55474d9dcdde51b4676577783212a90cc39fddb5cc5f4`。
特别保留暂时退步：原 metadata 阶段 single owner-return 曾成功，实际 while proof
启用后被活跃 proof 的 take 门拒绝于 144..151；必须由后继安全交付/退休小片恢复，
不能描述为始终未支持的旧边界。复杂嵌套、borrowed child、pending 与 provider 携带
仍须逐项闭合；本阶段仅五直接源码双入口有限闭合与分支增量备份，仍 active，
没有执行 native、故障或校准，也不创建完成 PR 或宣称最终合同已完成。


owner-return 退步随后独立修复：仅 return 的真实 Name/透明 Group、While-origin
proof、声明返回型与 symbol nullable storage 精确一致的路径，先核每层 NonNullUse、
真实 owner/view/inner 及 carrier 安全，再结束该根 proof。single 保留原 lower 和全部
AfterExpression/ControlTransfer；unit 复核 lower 仍交付同原 owner 后跳过重复 wrap，
仍恰一次原 ownership transfer。一般 Name/adapt/take 门未放宽，未决 child 不强行结束。
固定 `c8e86f5e` 的十六项普通 exact 各 1 通过，87 输入逐轮稳定，收据 SHA
`0803d31f9957cd011e9915e1647ad0a0508b67bd0b4b31441e075fe55480aab0`。
前十四回归保留；原 owner-return/other!! 四入口通过，新 Name/Group 四入口另断
Return 恰为 NullableBranch true-edge 的原 nullable owner、同 proof BorrowEnd 唯一且
先于 Return、返回 owner 不被 Drop，并无 NullableTake/NullableWrap。single 已证退步
恢复，unit 同源也闭合；这不是将原 42 其他复杂源码计作通过。
同 head 四工程 fmt/check/clippy/build 全部 exit0、87 输入稳定，收据 SHA
`a6374b2b634a09980291106cce46912b6c643a007636025de5e7b5996582d9dc`。
保留前述失败与阶段记录；本批仍仅增量备份，复杂嵌套、child/pending/provider、
最终 native 与 CI 尚待逐项验收，不归档、不创建完成 PR。
