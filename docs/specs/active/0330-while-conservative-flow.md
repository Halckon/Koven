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
