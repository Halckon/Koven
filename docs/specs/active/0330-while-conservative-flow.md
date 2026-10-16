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

同根只读内层 while 随后独立推进：原 readonly 源码与仅内层 Group guard 变体，
于 `26dcb91c` 采四入口真实 lower Unsupported，收据 SHA
`25eb10f3c8d550558c53a7969ed092cbb4dc1f86bbf8392789f09bc17ce19c0a`。
生产只复用当前单一同根 While proof 的真实 owner/inner/origin，沿既有
NullableIsNull/Conditional/carry 运输，不重新出生 view；unit 的 false/body 从完整
condition state 分别恢复，各实际 continue、break/false 及 merge 核存活配对。
消失或替换的 proof 不从入口重造，未决 carrier 仍拒绝。
执行前静态发现测试将 checked 加法分支误算作唯一 Conditional，旧清单未运行；
仅将 inner guard 按真实 BooleanNot→NullableIsNull(owner) 生产链定位后重冻。
固定 `5ab6b93c` 十七项普通 exact 各 1 通过、88 输入逐轮稳定，收据 SHA
`077d5328f5c43b314d4b659bcd8ff3e19b69790dd8f4edf3dfec68a9d74d276d`。
原十六回归含 owner-return 保持通过；新四入口均 SSA/内存 LLVM 成功并锁唯一外层
birth、inner 两边真实 owner/loan 列、两实际出口到 final-read join 的配对参数，以及
最后读取后的同 lineage 唯一 BorrowEnd。该精确 oracle 只覆盖这两个 readonly 源码。
同 head 原六组其余矩阵各实际 1 命中，38 观察为 13 成功、19 lower、4 complete、
2 ownership 拒绝；normal 组绿，其余五组仍正例行为红，原精确负控保留，收据 SHA
`70d64962e9002a35b4c25c9d8dab192ecb0de9ba816c9fc6b3d5a0076d825e3d`。
其中同根写入后 continue/normal 均仍 Unsupported。与同 head owner-transfer 四成功
合并计原 42 矩阵为 17 成功、19 lower、4 complete、2 ownership；不声称这些组全绿。
同 head 四工程 fmt/check/clippy/build 全部 exit0、88 输入稳定，收据 SHA
`38e4ed9a23e9c86e057c34fb0b44a2909dd656a54c4b681184c3d0bc0b7458a0`。
仍 active，仅阶段分支备份；同根写入、不同根、child/pending/provider 与最终 native、
CI 继续由本片后继逐源码闭合，不降低既定最终合同。

同根写入内层的 normal/continue 与两个 Group 变体于 `e5e3d6b8` 独立采红，
八入口均到 lower Unsupported，唯一 exact 实际失败且 88 输入稳定，收据 SHA
`6ba64ef37bf4f870abc6c7b3ee1f27aec31a7bca5cddd9373def4fb3eaffe8dd`。
后继消费前端已有 head-kill 事实：实际 subject 每层 Name/透明 Group 均为声明
Nullable、无 NonNullUse，且真实 descriptor/root、当前单一 proof 的 owner/inner/origin
和全部 carrier/direct-entry 资格吻合后，才在 preheader 结束旧 view，不 Drop owner，
再走原 direct NullableBranch 出生；readonly 则要求每层精确 narrowed type 与完整
NonNullUse，继续原继承路径。未新增写扫描或伪造 loan 事实，也未放宽一般 Name/return/borrow 门。
首次 `44bfadcf` 十八项中前十七通过，新八入口 SSA/LLVM 全成功，但身份 oracle
错误假定 NullableNull 与 proof 退休同块；收据 SHA
`11f9dc1d87fee41b438f1b9084f8f64209825387a29357c9bd95dd8d10f70cf9` 保留。
真实 ASAP 时序是在最后 field read 后 end proof、Drop owner，再 checked 加法，
仅成功块创建 null。此处只修测试沿真实成功边追踪，不将生产析构推迟到赋值。
固定 `918942b3` 同十八项重新各 1 通过、89 输入逐轮稳定，收据 SHA
`4d3c49273cdd8fa64bc9612c277688338480feb54ba70caafb12d1692fb1f25f`。
新八入口身份断言通过：两次真实 birth、旧 end 在 inner header 前、inner 可达边不带
旧 loan；新 read→end→配对 owner Drop→实际 checked Add，失败边 Abort、成功边
NullableNull 且真实回边替换 header owner 列，旧新 proof 均不进入回边。
原十七回归含 readonly 与 owner-return 保持绿；同 head 原其余六组实际各 1 命中，
normal 与 same-root 整组通过，其余四组仍行为红，38 观察为 17 成功、15 lower、
4 complete、2 ownership，收据 SHA
`606ea44671c77b1e4e042495f57d232b892a662322fc88ac1fed3e20b15e2078`。
加同 head owner-transfer 四成功，原 42 当前为 21 成功、15 lower、4 complete、
2 ownership；精确 stale-read/child 冲突负控保持，不冒称矩阵全绿。
同 head 四工程均 exit0、89 输入稳定，收据 SHA
`4ecb455002bfa80bce3ca03c21b42958b2d88ea8b9b6ebc17e45bf624f114639`。
本阶段仅增量备份；不同根/provider、复杂分支/资源退出、child/pending 与最终 native、
CI 仍须本 Spec 后继闭合，继续 active，不创建完成 PR。

不同根内层 scope 随后独立推进。原 continue、normal、两个 Group 与显式 null
入口五源于 `e84223a0` 采十入口 lower Unsupported，收据 SHA
`41645dc3186e81ee38296d153cfc60567e4ffc801c26af2b9c81dca96964efe7`。
新增入口只允许一个真实活动 outer While proof 与不同根的 owned NullableHandle
HeapOwner；两个 payload 独立核型。opaque 资格对象保持未知 carrier 拒绝，body
新 birth 保留当前重绑后的 outer，false/body 分别从完整 condition state 恢复；
各实际 continue、break/false 清理前后及 merge 核 outer origin/inner/current pair，
不从入口快照恢复已消失 proof。inner 退休后恰剩 outer，不 bulk-end 外层。
首次十九项旧十八绿，新十二观察中只有显式 null 源 single InvalidSsa，收据 SHA
`ac2cf7a85b676e16dd2a1b195de7ffeb443f0f242ef57374f1a112f8afc30e58`。
临时只读 trace 保留原验证拒绝，证实已 Drop 的 owner 因 assignment RHS temporary
别名残留再次进入 checked 边；采后已撤。trace 收据 SHA
`e8e221cb1a2b4e781d7d53e19ee81656f8be3793f8729de87f6c01d3101575d1`。
独立 assignment 六源双入口首红确认 single 四个 MoveOnly 路径 InvalidSsa，unit
对应四路成功；Copyable 双入口成功，compound single 成功、unit 保留既有 complete
唯一 L0165（59..74）。收据 SHA
`293572beda280c7061278f2f0724771193711516e681066b528fbbc19f56d6b9`。
最小修复在原 live-proof 门之后，仅普通 Assign 的真实 Value、resolved RHS 明确
MoveOnly 时复用既有 owner 交付清除同值旧 binding/temporary 别名，再安装目标。
不新增类型转换、Drop 或错误传播，不改 compound、Copyable 或 verifier。
有限编译预检曾因错误 Rust import 路径失败（零测试执行），机械修为现有导入后通过。
固定 `dc8f598e` 二十项普通 exact 各 1 通过、92 输入逐轮稳定，收据 SHA
`2574c6d90d20fa854c5154260c21319a906142a37e46d5980c3e9ed75c2bd641`。
新增不同根六源含异型 Holder/Other，共十二入口 SSA/内存 LLVM 与全部身份断言通过：
两真实 birth、独立 nullable/payload、outer 实际边列运输、inner read/end/Drop 与
checked 成功 null 回边、inner false 出口重绑 outer loan 的最终读取和退休。
显式 null 源仅证明编译路径，并非 native 零轮执行。assignment 十一成功与原 unit
compound 精确拒绝均通过；原十八回归含 owner-return 保持绿。
同 head 原其余六组各 1 命中，二绿四行为红，38 观察为 19 成功、13 lower、
4 complete、2 ownership；收据 SHA
`9ec48d4f64fb2b66776aeb6fdc99d04ce48dafe63380341ca8c2dc64fc218106`。
加 owner-transfer 四成功，原 42 当前为 23/13/4/2，精确负控保持。
同 head 四工程全部 exit0、92 输入稳定，收据 SHA
`ba55d755a0c4cd86d7e168db870a1f38b6b1bcdcf583d8e1e35e6b69fea2dc7c`。
剩余十三 lower 按当前源码为：普通 owned 字段 single 一路、nullable String 子字段
借用两路、pending 子 loan 跨只读 control 两路、写后 if 重检两路、numeric for
继承两路、borrowed for 内外 scope 四路。仍须独立验证根因与逐项闭合；本阶段仅
分支备份，native/完整 CI 与完成 PR 尚待，不降低最终 while 合同。

### 普通 owned-local 字段 Borrow 对齐阶段

于 `d984067f` 采独立首红：原 broken、直接及 Group 三源码的 single 均在字段处
Unsupported，unit 三路 SSA/LLVM 已成功；六观察先收齐，收据 SHA
`caaadd6d4509e16a1d45ea7266a2d0dbddefce37a81ea0bbdeee03770da4cbac`。
新增 single 路由仅接真实一级非泛型 owned-local Class 字段，核 LoanFact 根/字段、
projection、HeapOwner payload 布局后生成 HeapPayloadPlace→FieldPlace，沿原调用
生成和结束唯一 Shared loan。直接查询既有 owner binding，避免提前执行 receiver
AfterExpression；未引入父 loan、String 复制或 nullable proof 借用扩围。

固定 `ce70c763` 普通 22 exact 各 1 实跑通过，94 输入前后稳定，收据 SHA
`e95579e977075587cf9bd0112b21c20806c61e9fb024abafc12eb8afcad4f248`。
新增正例六入口均 SSA/内存 LLVM 成功，并锁真实字段 place→BorrowBegin→PrintString
→BorrowEnd→原 owner Drop 的身份和顺序；十二个 Unsupported 边界控制通过，
覆盖 nested/generic/inline/owned parameter 及 sibling exclusive 两种顺序，未称精确 span。
首次 wrapper 缺少已审工具链环境，被 runner 在 Cargo/输出目录前阻断，零测试执行；
补齐原环境后正式采证，旧日志保留，不算代码行为红。
同 head 其余六组二绿四行为红、38 观察为 20/12/4/2；加 owner-return 四路，
原 42 为 24 成功、12 lower、4 complete、2 ownership。分类收据 SHA
`be3e677e4ddb7fe2bd851504feb4f80da4017c023448d3f6a639fd7e6e51511b`。
四工程原命令全部 exit0、94 输入稳定，收据 SHA
`5404891124fb0e1410f79f18ccec12f0750bb7730f4827277f4cf30dc2ae988f`。
下一依赖仍为 nullable String 字段子 loan、pending child、写后 if 重检及两类 for
继承；本阶段仅分支备份，native/完整 CI/完成 PR 仍待。

### nullable while 字段同步子借用阶段

于 `8fa55716` 保留原 returned 字节并加直接/Group println 对照，六入口首红均到
lower：single 三 Unsupported、unit 三 MissingFact。收据 SHA
`f5ac2f1c716d4269226a21148f4e4c461e69984ab5c71b53fe37a0bfe8d952b4`。
两入口仅从当前已验证 While proof.loan 生成真实 String 字段 SharedHeapFieldLoan；
完整 LoanFact/NonNullUse/当前 owner/payload 资格先于 append，只有 child 进入原
调用清理，parent 不加入 created，不提前求值 receiver。unit 保留真实 call frame
资格及 shared_field_roots，未知 carrier、later-argument CFG 和退休门未放宽。

固定 `15533aa1` 普通 23 exact 各 1 通过、98 输入前后稳定，收据 SHA
`18b79fcd00a65329d3d0947123ddec375d5c68fc991ffdcc1ba55215bb6f35dc`。
新六入口 SSA/内存 LLVM 与完整身份断言均通过：真实 NullableBranch owner/view、
String 字段 child、实际 Borrow call/PrintString、唯一 child end→parent end→owner
Drop；null 可达侧无 child，未通过复制 String 或重造 parent 达成。原 22 回归保持。
同 head 原六组现三绿三行为红，38 观察为 22 成功、10 lower、4 complete、2 ownership，
收据 SHA `bcf66a20abdba7d1b7e951d8b8972fac320eefb30358c7279cc675070e9169d6`。
加 owner-return 四路，原 42 为 26/10/4/2。pending 正例已越过字段借用，真实停在
内层 while carrier 门：single 314..347、unit 324..357 Unsupported，尚未闭合。
原四工程全部 exit0、98 输入稳定，收据 SHA
`fb900f2064e8a6b2ef3b01aa9693e88ba020c260269d3c8a893c6a4230569b26`。
后继仍须 pending child、写后 if 重检、numeric 与 borrowed for 继承；有界 native
代表源码另准备并审验。本阶段仅分支备份，完整 C/CI/PR 尚待。

### 有界代表程序 native 与 unit 空调用帧

完整代表程序含十四个至多进入一轮的 while、独立字面值算得的 38 行输出及九次
Resource 析构。首次普通 LLVM 观察在 `returnOperand(value: Int)` 的默认 Borrow
字段参数处双入口 Unsupported；收据 `aa0f70f0846c18dc49e5a6df2b8da6457657e01a4973b3bd5ed3c98af384393a`。
仅将该 helper 参数明确为 `own Int`，表达既有 Copyable 值交付，保留 Guard/return
场景及黄金输出。其后 single 完整 LLVM 成功，unit 在 returned 的 current 声明名
1623..1630 拒绝；收据 `5997b4d9a71cf9d09f540817964a97cc1e8e6f9c89ec859ea677d9b2cd1d27c9`。
single 先独立 native 真绿，收据 `d7b864c14a2d1ab8a52b280c66e6956cddad2d96fd578e01d59995c280dd499a`。

最小双源码确认 unit 在求首个 own Int 实参前的完全空 PendingCallFrame 触发统一
carrier 拒绝；含已有 Value 前缀的对照也拒绝，single 两源均成功。首红收据
`3dcda8aa6934bd822c00196299e398b1b8f8dc972c0d3dee4265af78edac34b6`。
新增纯资格只认零帧或恰一实际当前 source 的 Declaration 调用空帧，Owned 结果、
无 receiver，pending/loan/ABI/root 全空且 pending_start 为零。未清状态或改 Drop/
BorrowEnd；所有非空前缀仍走旧门，unit 对照完整 Unsupported159..166 原样断言。

固定 `964247d7` 普通 25 exact 各 1 真绿，收据 SHA
`e1cd3194d49543183a46bf7885340ea26468556e953b8ef61f5a41fb3f1b010e`。
同 head single/project native 各一次 build、一次自身子进程 run，实际各 1 通过；
收据 `ac56db5398d3c66243da404c8309e6e63b5e7760ade470ebb27f124a3f98ab08`。
38 行 stdout（含九条 drop）、空 stderr、exit0 由已审 helper 严格断言证明，无独立
子进程日志声称；10 秒限时只适用于自身 run，不涵盖 build。覆盖直接四退出、
同根只读/写、不同根、owner-return、String child 及零轮正常退出。
原六组分类收据 `226b572e02fc8c081883c7694437311ecc5a0a2360211cf366417f620239a652`，
38 仍为 22/10/4/2，结合 owner-return 四路，原 42 仍 26/10/4/2。
四工程全部 exit0、107 输入前后稳定，收据
`f1cba68287b9eb5227bbee9069a9d6ebffde85d09ac22e3c500c298ba7e336bb`。
保留先前 clippy duplicate_mod 失败及当轮 build 未跑：机械改为每 integration 根
仅注册一次既有 bounded runner，0329/0330 子模块复用；旧 numeric native 未重跑，
仅当前 while native 与 all-targets 编译检查该接线。本阶段仍仅分支备份，剩十路
lower、全合同回归及远端 CI/PR 未完成，不以代表 native 绿代替完整 C。

### pending String child 的只读嵌套运输与有界执行

在 `44b7129d` 固定原 pending 正例及仅内层 guard 加 Group 的对照，双入口四路均
在 inner while 处 Unsupported；首红收据
`e5694cf3fb7be5828fba01c667a7e7f0ff7a251837379c69df4b334ee9a8fc78`。
本片复用已有 pending loan/operand 槽，只有真实 SharedHeapFieldLoan 出生才登记
call、argument、root、parent origin、field/target 关联。single 原 pending 表快照
携带完整记录；unit frame 进入 BranchExit/condition/兄弟快照，并在提交后随
LoweredCallArguments 保留到原 BorrowEnd。每条实际 edge/merge 核 owner、parent、
child 对应列；不从首出口或旧出生 LoanId 恢复当前实体，不新增 SSA 参数。
特许仅同根只读、一个普通 source 调用的单字段 child；写根、新出生、parent 退休
及未知 carrier 仍守原严格门。调用后先 end child，真实后用读取 parent，再 end
parent/drop 对应 owner。原写冲突唯一 L0135 及完整 primary/loan label 断言保留。

固定 `13b6c0ae` 普通 27 exact 各 1 真绿，119 输入逐轮前后稳定，收据
`940e8e3b4eedf0ef6247257753c220cd6dee2b3247f0b08188a9d91d77aabc95`。
新四路 SSA/LLVM 及真实三列、if/inner 分支、调用和清理身份 oracle 完整通过。
保留执行前发现并修正的 oracle 误禁：合法 Borrow Boolean flag 的 Read 不属于
String child 复制；现仅限制实际 child Read/对应 StringOwner 结果，未改生产来
迎合该断言。只读 retain 谓词也不能改写 loan 而遗留出生证据。

新完整 pending fixture 先双入口 LLVM 真绿，再 single/project 各一次 build/run；
true/false/null 输入的独立结果为 94/106/0，严格六行 stdout、空 stderr、exit0，
每个进入的外 while 直接 return、内 while 直接 break。连同旧代表程序共四个
native exact 各 1 通过，收据
`37e724fb105516cdaac3cc73729de3a787ff3db71d650ef173c273988dcdc245`。
输出由已审 helper 字节断言证明，没有独立子进程日志；十秒仅限自身 run。
原六组四绿两行为红，38 观察为 24/8/4/2，收据
`419490751688ca7fae5e735f132d15fcde9bc4adf7e375db54bae9b7833bc47b`；
结合同 head owner-return 四路，原 42 现为 28 成功、8 lower、4 complete、2 ownership。
四工程原 argv 全 exit0、119 输入稳定，收据
`24b0af7e013bfab4d2862a7a6f2c70a9c8d33cdf9e304e8b6f733beead75c6f5`。
剩余 lower 为 if 写后重检两路、numeric for 两路、borrowed for 内外嵌套四路；
本片仅分支增量备份，完整 C、远端 CI 与完成 PR 仍待闭合。

### 条件替换后 If 重检及 owned nullable 调用交付

原重检及 Group 四入口首红在 `3f926781` 全为 lower Unsupported，收据
`bbdc7e62f149b174f84a228949b2418b3dd7634a5a23c9e1315919bbd0281a6d`。
独立公共事实观察明确同一 Named root 的 initial/null 两 owner 版本及两个真实 If
selector；该观察不算 lower 验收。普通 If 仅对实际非 Diverged 出口先全部预检，
再安全退休 While-origin 差集；共有 root 的 origin/inner 不一致继续拒绝。unit 新
If birth 限活动 direct While 同根、proof 空、then-only Unit 的实际 NullableBranch，
If/While scope 明确区分，复杂 carrier/转移/新 loop 仍不自动获得准入。
single 的窄清理组同时核原两版本 DropPoint、完整互斥且精确覆盖当前 arm 的条件
DAG、first join 两条实际 owner 入边与 second successor 当前 owner/view；消费前
再次核实际身份，仅一次原清理消费两个组员，未跳过 verifier 或泛化条件 Drop。
Int 字段读仅复用该完整组的只读资格，读取后仍由原事实执行 End/Drop。

保留 stage2 首轮完整 fixture 的 unit `declaration exists` 测试入口事故：codegen
观测补 entry→main wrapper，native 原 fixture 字节不变。第二轮原/Group 四路绿，
完整 fixture single InvalidSsa、unit 绿；临时 cfg(test) 追踪已撤，真实错误为 main
两处 DirectCall 将 HeapOwner 直接传给 NullableHandle 参数，非重检清理错误。
单独四源码八入口首红为 single direct/Group/后续 CFG 三失败，其余五成功，收据
`bc706c65b2cc9b27ce07ae337754ad5b6c470bd3ac107ac5dbb92300e4ab0c38`。
修复仅普通 Value source-call 的已映射 MoveOnly HeapOwner→精确 NullableHandle，
完整资格在适配前，复用既有 NullableWrap 并更新该 argument 的原 pending 槽；
后续 CFG 与末轮 take 使用当前包装 owner。未命中资格保留旧路径，不新增 nullable
ABI 或 Borrow/Inout 能力。等型/null 与后续参数 CFG 的实际普通对照保留。

原四路的 query-only 身份断言锁 outer/If 两次出生、两次 join 实际 owner/view 列、
写/未写路径的退休与 Drop、null 路径及无失效实体复活。完整有界 retest fixture
先双入口 LLVM，再 native；三个调用分别保留 17、清除为 0、初始 null 为 0，
每个 while 最多一轮。严格三行 stdout 只证明这些值结果，不额外声称 Resource
析构次数；既有两份 native 代表继续保留各自清理断言。

保留首统一工程 fmt/check 成功、clippy 两处 oracle unnecessary_unwrap 失败和
build 未跑记录；机械解构修中一次错误覆盖相邻 BorrowEnd arm 的候选未执行，
随后独立恢复该 arm，最终相对受验版本仅 Drop 分支等价 if-let 改写。首次红控
清单也因父模块 rustfmt 递归格式化未 clean 而未运行，格式化独立提交后重新冻结。

固定 `a830cb0c` 普通 30 exact 各 1 真绿、132 输入逐轮前后稳定，收据
`1e88e2a6030ed7077fe79ba3379621e9cd6f50ff4fa826b90070516226f685c8`。
同 head 六个 native exact 各 1 通过，收据
`6221cff2f942be9270a7a0a16843b5943581f687224bd611e9e6a8d3cba69cbb`；
输出均由 helper 严格字节断言证明，无独立子进程日志声称，十秒仅限自身 run。
原六组五绿一行为红，38 观察为 26/6/4/2，收据
`088f65bda0c216ae7bed5e53ddbb9d6c003ea467ffc00b0c09371e8d33e45d8d`；
加同 head owner-return 四路，原 42 现为 30 成功、6 lower、4 complete、2 ownership。
四工程原 argv 全 exit0、132 输入稳定，收据
`5fa29ce52fe89d193557ebe39582c8a58b6d0f8ea684b309a56fab0e6308d060`。
剩余 lower 为 numeric for 两路、borrowed for 内外嵌套四路。本片仅阶段分支备份，
完整 C、远端 CI 与完成 PR 仍未交付；后继须保留本轮全部既有控制继续闭合。

### Numeric for 运输外层 nullable While 证明

原 numeric 源及仅 source 加 Group 的四入口在 `24f74e1c` 全部 lower Unsupported，
首红与公共事实观察收据为
`a64749671889b8f25b331932e161bd7622b9a6f15f9dfe109ee9fa746d7ed833`。
事实观察明确 Parameter seed、Header/Exit 两 phi、Always Entry/Exhaustion 及
Continue/Fallthrough 同 header 自运输；观察本身不算 lower 成功。此前 E0599
访问器预检失败未执行测试，修正为真实公开 iterations 接口后重新冻结采证。

unit 仅为已核活动 While 的当前 owner/view 配对复用既有 carried context，真实
empty 边、normal/continue、exhaustion 与 merge 保留 origin/inner 和当前列身份；
numeric 本身不出生或退休该 proof，旧空 proof 路径保留。single 的专用条件清理
资格要求完整两 phi/selector/所有 incoming 和 exit、唯一真实 seed 与同 header
自运输，并在消费前核实际 numeric CFG 和当前 owner/view 成对列。反向图枚举
所有真实前驱，全部访问节点均须能回到唯一外层 While 出生锚，悬空 SCC 拒绝。
通用 implies、constant Drop 与 verifier 未放宽；实际消费仍恰一次 parent End
随后一次对应 owner Drop。额外版本、写根、capture、未知 carrier 不因此获准。

`a116f1e5` 首阶段三项均通过，原/Group 四路及完整 fixture 双入口 SSA/LLVM
成功，公共 facts 项仍只记观察完成。query-only 独立 oracle 随后锁同 snapshot
三投影、empty 分支、current/last 终值判断先于增量、normal/continue 真边及
exhaustion 后最终 Member→End→Drop。首次统一第31项因追踪已无用途的 scalar
到 exhaustion 而误断 dominance；四路真实 SSA/LLVM 已成功。仅跳过该 scalar
目标，owner/view 全边追踪和 dominance 判据不变；保留原 oracle 失败证据。

完整 bounded numeric fixture 先双入口 LLVM，再分别 single/project native：
nonnull 时内层恰两轮，continue 后累加并最终返回34；初始 null 返回0。独立
两行 stdout 由严格 helper 断言验证，不冒称独立子进程日志，也不将其等同 numeric
空区间、MAX、Long、Resource 析构或 child/break 组合验收。自身 run 限10秒，
build 不称有此期限。既有六个 native 控制保持。

保留 v2 工程 fmt 失败、后三项未跑；v3 fmt/check 成功但 clippy 报 qualify
8/7 参数、build 未跑。先机械格式化，再将调用方同一 symbol_type 查询移入
qualify，未改变资格或错误传播；v3 普通/native/分类清单未执行。
固定 `e4b85ec8` 最终32普通 exact 各1通过、139输入前后稳定，收据
`c6d5dc2ffdf865bfd749e2c6217658b930e56062fdab67ee00f2967fa3fa5dd6`；
同头 native8 各1通过，收据
`5ce61d079da3733dfbe94cc69c764853aa5fb93e1d37fc5395be134ed4a2f8f0`。
原六组五绿一行为红，38观察为28成功/4lower/4complete/2ownership，收据
`7354ce1591a2914935c4ebbc4cf01ee158d4ad68784d888e37ec70cee73a6814`；
加同头 owner-return 四路，原42现为32成功/4lower/4complete/2ownership。
四工程原 argv 全 exit0、139输入稳定，收据
`4d57dc426cba1b6ea3f24ab6ec498da06a24aa0dde9c391209b63e950cda1007`。
剩余 lower 明确为 borrowed for 内外嵌套两源码双入口；仍须逐关系核 provider、
element 与 nullable proof 的真实运输和清理。当前仅阶段分支备份，完整 C、
CI-only guards、远端双平台验收及完成 PR 尚未交付。

### 借用 for 位于 nullable while 内：有限 provider 运输闭合

从真实 numeric 阶段后继继续。`9f3c0f21` 原/Group 两种嵌套关系先采红：
inner/outer 各四入口均在 lower 拒绝，公共事实八观察完成；收据
`916ee808a0a71760b938ebcb02eaeb02ea71d4c9b232bea312be99e9d2625b23`。
事实明确区分 current Exit phi 条件 Drop 与 items Header phi 的 LoopExit Drop；
后者顺序为 FinishProvider→EndSource→Drop(items)，不能统一当 Exit owner。
本片只接 inner 关系：Owned List<Int> 根、单 Name Borrow element、不同真实
current/items owner、既有 While proof。single 封闭四 phi/全部 incoming，items
Header Drop 单独复用原 constant presence；current 的归纳和运行期实际列另核。
unit 保存 body 前真实 false 完整状态，沿原 provider/cursor/element 槽运输；
normal/continue 先原 EndBinding/EndElement，exhaustion 原 EndSource→items Drop，
最后 Member 才结束 outer view 并 Drop current。未知 carrier 和 outer 关系仍拒绝。

`ea7e1d1e` 阶段四项实际 exit0/101/0/0：inner 原/Group 四 SSA/LLVM 成功，
完整 fixture 双入口成功，outer 原拒绝保留，facts 项仅表示观察完成；收据
`2b5fb105a7d3388a368f3521c20654f07a27f1d27a49ad97521a3183bf78047a`。
同头新增双 native 各1成功，收据
`b22c4fd5e29acb0478a430dd4a18135a894ac24341fff78e8648750d8323f7fd`。
固定两元素、continue/normal、outer null 代表程序的 36/0 两行输出由严格 helper
实际断言证明；没有独立子进程 stdout 文件，也不据此声称 Resource 顺序覆盖。

保留全部前置与验收失败：重复 --tests 的命令解析失败、ListOf 非实际枚举的
E0599（改为真实 ListForm）；首工程 fmt imports 失败；随后 large_enum_variant
导致 clippy 失败/build 未跑，token 等价移至 IterationContext（Numeric 为 None）；
下一轮仅 scope 谓词 fmt 失败。`1f209447` 工程四绿、普通34中33绿，新增 identity
因独立 SourceMap 的隐藏 map_id 比较导致首函数筛选为空，四 lower/LLVM 已全采绿。
该 oracle 错误改为独立 AST 坐标与真实 outer 来源锚，生产未改。

固定 `afd62ae9` 最终34 ordinary 各1通过、149输入逐轮前后同，收据
`e0651e5c76973896aeb7c5e6978fbd2de9008cca4161270dc262eaa9e1eaf67a`；
独立 query 锁五角色实际边列、两条 element 结束回边、source 结束与 items Drop、
最终 view/read/owner 清理，原/Group 两入口完整身份断言通过。
同头 native10 各1通过，收据
`6f9548584f137a393bbce35993feef915db78960c68af3cc8a4e74d1dc6a61a8`；
四工程原 argv 全 exit0，收据
`b8d2f2c58840c2c60f673a03a4a20436acb1dd03405bbea10b4c946d7c5baaa9`。
原六组五绿一行为红，38观察为30成功/2lower/4complete/2ownership，收据
`45310ec8685af216730ac0388c592e9ddcd11e81d777c1f7e6be9e33035aa39d`；
加同头 owner-return 四路，原42现34成功/2lower/4complete/2ownership。
剩余 lower 仅外层 borrowed for 内 nullable while 的两入口，仍须闭合真实
provider/element 在 inner false/break 的恢复与后用。CI-only guards、双平台 CI、
完整 C 验收和完成 PR 尚待；本次仅阶段分支备份，不扩 Guide。

### Nullable while 位于借用 for 内：最后普通正例关系闭合

沿上一阶段已采原/Group outer 红证继续，本片仅 Owned List<Int>、单 Name Borrow
item、独立 nullable current 的既定关系。single 对两个真实 Header Drop 分别复用
原 prove_graph，封闭四 phi/所有 incoming，保持 FinishProvider→EndSource→items
Drop→current Drop。inner false/break 分别恢复完整 provider snapshot，当前实体须
沿真实列回溯；unit 同样核当前 source/element、cursor/length 与两个 owner。
内层只结束自己的 view，source/element 仍活，后用 item 后才执行外层 EndElement；
outer exhaustion 无 element/view。未将 presence 当作非空证明，未知 carrier 门保留。

静审发现的 birth LoanId 与当前 rebound view 混用已在运行前纠正：true body 的
checked-add 后继按五角色共同追到真实 NullableBranch 出生，break 结束当前 alias，
禁止它进入 merge/outer 回边。另保留 ListOf→ListForm 机械修，以及首次有限 check
E0027 Assignment pattern 缺 operator_span 的失败；加 `..` 后预检成功，原清单未运行。
`e1bf46f4` 三项阶段收据
`13ac77d4897452f96b2b4f5f079f10ef7f49a2c568ae443ae33f3e4daa51f4b2`
确认原/Group 四 SSA/LLVM、完整 fixture 双 LLVM 成功，facts 八项仅表示观察完成。
同头双 native 收据
`c26da0b6a64f6edb9da5feb1ed9765fa5cbebdd9172263d0214766f96d4e6215`
证明严格 helper 的 40/3 两行值结果：固定两元素、nonnull 每轮 inner 一次 break，
null 每轮 inner 零入，item 在 inner 后继续使用。无独立子进程 stdout 文件，不据此
认领 Resource 析构顺序；flag 未参与控制，不能称额外 true/false 分支覆盖。

独立 query oracle 沿全部实际边核 owner/items/source/element/view 五角色、非空与
空路径的后用 item、当前 view 与 element/source 结束顺序、两个 Header owner Drop，
以及真实 cursor/length 列。原/Group 四入口先完整 verify/render，再执行身份断言。
首统一工程 fmt/check 成功，clippy 报大枚举变体与三处 question_mark，build 未跑，
后三组也未跑。provider 单独 Box/getter as_deref 与 Option `?` 等价机械修后重冻。

固定 `6037cdbf` 最终36 ordinary 各1通过，158输入逐轮前后同，收据
`f2f9ce748efc680a2b8e42c1768f62f1045e5fc8e39be6fc2f27711ce2cfbc41`；
新 outer 原/Group 完整身份断言与完整 fixture 双 LLVM 均通过。同头 native12 各1
通过，收据 `afb9ca749b20210b73af2658847d44a6e2d2487a03bed992b1d71d6658859fe1`；
四工程原 argv 全 exit0，收据
`5232909562de848c622b62bad2a0d5005910ee47e0395869767f10660f119480`。
remaining 首次缺已审 toolchain 环境，在输出目录/Cargo 前拦截，实际0执行；日志
保留，补环境后原六项各1通过，收据
`0df84ce4509f93924199ebbad4779d21d39913117fee70add536e81bfb7762e0`。
38观察实际32成功/0lower/4complete/2ownership，加同头 owner-return 四路，原42
现36成功/0lower/4complete/2ownership；六个负控仍为既定精确诊断断言。至此原普通
正例矩阵闭合，CI-only 私有 guards、前端与后端最终同头统一验收、双平台 CI、完成
PR 与正常合并仍待交付。本次仅阶段分支备份，不扩 Guide 或宣称完整 C 已完成。
