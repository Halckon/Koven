# SPEC-0326: if/when 封闭分支类型合并

> **性质**：有界类型修复 · **状态**：in-progress · **读取时机**：实施 v0.44 A 时 · **唯一真源**：本 Spec；语言合同见 Guide

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P2-0326` |
| 所属 Phase | Phase 2；不倒置 final admission 与 ownership |
| 语言规范 | [Guide 分支类型](../../guide/06-blocks-control-flow.md#valuestatement-context-与分支类型) |
| 基线 | Guide `be3264bc7143fdddb160ce77039fb902694d0d77`；独立于 0325 实现分支 |
| 分支依赖 | Guide已合主干；从真实A远端正常合入main aa42bdc5的0325交付 |
| 批准范围 | 用户已启用 v0.44 A 及封闭 join 细则；实施与验收分别推进 |
| 当前状态 | 合B后本地39exact与4工程已验收；待联合发布head远端CI |

## 1. 最小范围

single/unit 的 if 与 when 遵守同一封闭合同：同型、Nothing、nullable 提升、
同一 enum 实例 case/root；其他已知混型拒绝，不回退 Any。
保持 if L0089、when L0112、expected mismatch L0084；保留原错误与 Deferred reason。
statement when/无 else if 为 Unit；有 else if 即使结果被丢弃也检查 join。
不增加数值 widening、泛型协变、case 省参推断、while 收窄或后端新能力。

两个入口使用各自真实 TypeId 体系；优先各入口内部统一 if/when 判定，
不以本片引入跨体系大 trait 或类型事实重构。诊断关联实际首冲突来源，
不能把被跳过的 Error/Nothing 首分支误标为参与冲突的类型来源。

## 2. 普通源码 exact 矩阵

target `control_branch_join`，每项相同源码走 single/unit raw types 与 complete API。
前8项为原定矩阵，第9项由同一合同的真实邻近缺口补齐；历史采证顺序见下文。

1. `known_mixed_value_branches_reject_implicit_any`：Int/String、Int/Long、不同 enum、
   List<Int>/List<Long>；分别锁 if/when 错误码与源码范围。
2. `same_type_and_bottom_branches_keep_exact_result`：Int 同型、Nothing/Int 双序、
   全分支 return/返回 Nothing 的 error 调用的类型与 falls-through；仅分析源码，不执行 Abort。
3. `null_and_nullable_branches_use_canonical_result`：null/Int 双序、null/null、Int?/Int、
   Int?/null，精确类型且无嵌套 nullable。
4. `same_enum_instance_branches_join_to_root`：case/case、case/root 及合法 nullable 组合；
   不将不同泛型实例合并或加入 case 参数推导。
5. `expected_types_keep_original_mismatch_diagnostics`：显式 expected Int 遇 String，
   原 L0084 与完整诊断保留，不借无 expected join 隐藏错误。
6. `statement_and_value_contexts_remain_distinct`：statement when 混型 Unit、
   statement 有 else if 混型拒绝、无 else if Unit，以及 initializer 值上下文。
7. `error_and_deferred_branches_preserve_original_roots`：普通非法二元操作及合法Result的 postfix ?，
   原诊断/Deferred reason 不被 ControlJoin 或成功 Any 取代；与同型/Nothing/冲突组合亦然。
8. `when_conflict_labels_track_actual_participating_branch`：前置 Nothing/Error 的三分支，
   primary/label 指向实际冲突，不硬指第一 entry；保留同根级联抑制。

9. `nullable_case_joins_deliver_the_same_enum_instance`：null/case双序交付同实例E?，
   保留leaf smartcast，拒绝不同泛型实例合并。

每项完整 `cargo test --locked --offline -p lang-frontend --test control_branch_join NAME -- --exact --nocapture --test-threads=1`。
helper 不构造/修改 compiler facts、IR；无产物执行、注入或校准。冻结输入 SHA/HEAD/tree，
串行各实际1命中，基础 API/编译失败与行为红分账。先静态审 helper，再定向红测。

## 3. 后继验收与交付边界

已有要求 Any 的旧回归须按新合同精确更新并逐项审定，不宽跑旧 target。
公开 CLI 只 build 验证拒绝路径；成功 native 仅选已有 Int if/Boolean when 普通路径。
合法前端 nullable/enum join 不等于后端已经支持，结构化拒绝单独记账。
最终有限名单与工程命令已冻结执行，输入身份与分账结果见§7。
Guide 与其他 Spec 发布由唯一 publisher 串行处理，本分支不改 0325 固定工作树。

历史首批3 exact仅采Int/String、Int/Long、Int同型与Nothing调用、Int/null/Int?双序；
全不返回、null/null、泛型/enum、复杂上下文与后5项尚待补验，不以首批红证批准生产。

首轮 `56d2d9fb` 的第一命令在helper编译阶段E0599（DiagnosticCode无as_str）停止，
0测试执行，其余两条未跑；原raw/input保留。改用既有Display/to_string，不计语言红证。


## 4. 首批真实基线

`b43c74ed85d73adf5b51f69c2d4e6f887a363e00`，2026-10-15 首3 exact各1命中，
11输入SHA前后一致，2行为红/1控制绿。每项普通case的两入口raw/complete均在断言前收齐。
Int/String、Int/Long的if两入口已有L0089；when两入口却回退Any且complete成功。
同型Int与返回Nothing的error调用双序全部通过（仅前端分析）。null分支两入口已有L0083，
if(null,Int)错误取Int；部分nullable组合虽得Int?仍保留该错误。说明需要处理分支预期上下文，
不能仅删Any或只补join类型函数。原始仓外收据 `spec0326-evidence/first3-v2-receipt.json`。
尚未执行总矩阵其余边界、CLI/native/工程验收；未实施生产。首轮E0599独立保留。


第二批追加同enum实例（泛型显式参数、nullable三分支双序）、expected L0084、statement、
Error/Deferred与实际冲突label五项；enum结果与各入口真实typed TypeRef的TypeId比对，
不跨入口比较ID，不依赖Debug文本判断enum身份。原ordinary error control使用 `1 + true`，
不把names失败作为类型红证。null/null、全不返回及复杂调用/return/lambda语境仍待补齐；
本批未运行，整个A尚不具备生产准入。


第二批 `920c1f6a` 的5 exact各1命中，11输入不变：expected/statement两控制绿，
enum/Deferred/conflict三红。single nullable enum首分支误Any、反序正确，unit两序正确；
Nothing/Error前缀when仍Any。Int?的postfix ?最终真实L0085，是非法operand对照；
父if raw改写为ControlJoin虽真实，但原测试预期L0165不适合该源码。保留原raw并改用
显式Result参数补锁Deferred源根，nullable非法操作数另锁L0085，不放宽final admission。
仓外 `spec0326-evidence/next5-receipt.json` 保留逐项事实；未到达的断言不称通过。


`83711e82` 精确恢复2项各1命中真实红，13输入不变；Result参数r?保留真实leaf
ErrorPropagation，但single if变ControlJoin，unit if及两when与Int合并误报已知混型，
部分顺序遮蔽leaf L0165。原Int? ?的L0085保留为非法操作数对照。
最终8项tests-only补齐不同enum/List实例、全Nothing/return、null/null及独立null反例、
显式expected的return/实参、无expected lambda尾值、smart-cast enum case/root双序；
Deferred源span多重集与唯一L0112自身label精确对比。统一8项仍待审后重跑，生产零改。


统一 `06d7287d` 的8项各1命中，13输入不变：7红1绿；expected含return/实参仍绿。
旧same/Nothing控制新增声明内return后触发独立single L0088，根型两入口已Nothing；
statement旧控制仍绿，新无expected lambda尾when暴露Any。原收据
`spec0326-evidence/final8-red-receipt.json`保留，不把总数变化误称所有旧控制回退。

## 5. 已认领的后继终止性待办

本Spec作者继续跟踪“变量初始化终止性传递”后继，排序由root协调，未将其计入A已完成。
真实最小源码：`fun test(flag: Boolean): Int { val result = if (flag) return 1 else return 2 }`；
when同形亦然。`06d7287d`下控制根两侧Nothing，single函数结尾L0088、unit完整成功。
静态入口是single `checker/item.rs` 的变量检查与Statement::Declaration返回路径。
此处应按语言合同终止，现L0088是已确认实现缺口，不是合法规范拒绝；本片不改其生产。
回归保留两入口现差异；A的正向全Nothing另用显式return语境，不经该声明路径。
因此A不会宣称所有不返回控制流已完全一致。后继另Spec认领前由本节保留源码/诊断责任。


最终红测 `c69f12db`，8exact各1命中，13输入前后不变：6行为红、Nothing与expected两控制绿。
所有case双入口raw均采齐，无编译/基础API失败。仓外 `spec0326-evidence/final8-red-v2-receipt.json`。
现按已批准A与该红证登记active；先私有封闭join与分支尾null上下文，再固定patch独审及同名单绿测。

## 6. 首轮实现与 enum nullable 补齐

`e498e621` 原8exact逐项各1命中全绿，18输入前后不变；生产只改两入口的控制join、
when冲突来源和控制body尾null上下文，未改一般literal推导、ownership或final admission。
unit checker旧尺寸1127→1128仅注册独立模块，新逻辑独立文件；该增长已专项审阅。
`a2ded691` 唯一观察命令完成不算回归绿：null与smart-cast case的if/when双序在两入口
均生成Nullable(case)，向同实例E<Int>?交付均L0084。`34e11bdf` 锁真实TypeRef红oracle，
新增不同泛型实例负控全部通过，正控真实红，原观察/失败保留仓外收据。
依据Guide06的内部case/runtime root与nullable合同，`32f6e385` 仅在join创建Nullable前
将直接case取其已有同实例root；不改一般assignability、叶smartcast或泛型身份。
该head原8+新增1共9exact各1命中全绿，18输入前后同；仓外`green9-receipt.json`保留事实。
这是前端阶段结果；共享旧Any期望、公开CLI、普通native与工程门禁仍待本片后续验收。

共享5首轮`6ff902ed`首项1命中失败、余4未跑：原“显式Any合法”判断不成立。
`a488386c`单项普通观察显示single的Any返回标注是Deferred，complete在Any处L0165；
unit签名虽为BuiltinAny，两个分支各L0084且complete拒绝。旧single raw空不代表最终接纳。
据此仅修旧混型raw预期与有限Any拒绝控制，未增加expected wrapper或开启Any运行时表示；
入口诊断差异保留为当前边界。原失败与观察分账保留仓外，观察命令成功不计能力通过。

## 7. 本地最终验收与交接

实跑head `7064a803faa19c966b7e76d489a43731dc34a20e`，tree `207bcd6377e7c98727e6d809144bc2f19f02444a`。
18个唯一exact串行、各实际1命中、43输入前后SHA一致：9新前端矩阵、5既有精确回归、
2公开CLI拒绝与2普通native成功。CLI拒绝合计8次build，human/json均锁L0089/L0112与Span，
无产物执行或残留；native每入口build/run各一次，仅Boolean/Int if/when，断言exit0、
stdout `joined\n`、stderr空及自身临时目录清理。native子进程结果由测试断言验证，
未另导出独立子进程逐项日志；不外推nullable/enum native或更宽资源退出能力。
同head的fmt、workspace/all-targets check、frontend+cli all-targets clippy -D warnings、
cli build四工程命令全exit0；Cargo依赖锁定offline，均非新增行为测试。
仓外`spec0326-evidence/final18-v3-receipt.json`与`engineering-final18-v3-receipt.json`
绑定固定argv、输入与日志SHA。原cc0e工程fmt失败/后三项未跑、bb463工程clippy失败/build未跑
均保留；修复仅模块排序及测试迭代器风格，原18两轮均实际通过。
CI接线为`check_stage_integration.sh`新增本target，既有type_checking/multifile与CLI已在
两host路径；仅静态核接线与bash -n通过，本分支远端CI尚未执行。Spec继续active，
等待唯一publisher按Guide同树映射发布并对齐真实main；不重复发布Guide，也不覆盖0325。

## 7. 真实远端基线同步与尺寸检查

2026-10-16，从真实远端 `7baf0a3b` 正常合并 main `020ff41f`，本地merge
`3cf2e5c5` 保留这两个有序父，完整tree仍为原 `1303545e`。该main尚不含0325实施，
本轮不称A/B联合整合；0325合入后仍需核共享checker与索引。
静态尺寸检查相对真实base020ff发现 tests/type_checking.rs 2254>2251；未提高阈值，
将两项既有when/join回归原样移入control_join_regressions子模块，主文件降至2193行。
固定 `225cc77de9edbe526ddba521cc1f92e51685fe14` 同head重跑原18exact，只有上述两项
完整名增加模块前缀，各1命中通过、44输入SHA前后一致；原四工程和完整尺寸门禁均通过。
仓外 `main-sync18-receipt.json`、`engineering-main-sync18-receipt.json` 保存结果。
抽取初次fmt仅报告子文件末尾多余空行，修正后统一四工程通过；未改变测试体或生产语义。
本地真实输入与远端CI继续分账，发布后的实际head仍须双host验证，Spec保持active。

PR112 implementation run `38835221595` 的Docs门禁发现命令组合预算仍为105，新增
control_branch_join后实际106；该轮Targeted全部skip，未运行A18。真实remote `0027aa6c`
上的唯一 `CompositionContracts.test_selection_is_unique_and_bounded` 本地先红，
修复 `0233c049` 仅预算精确改106并锁新增target恰1，保留原唯一性/顺序/调用数断言。
同一exact再绿（Ran1）；它以临时cargo/python3 stub记录成功路径命令，不执行实际Cargo，
FAIL_MATCH空且未运行邻近故障传播测试。5输入前后一致，docs与实际base尺寸检查通过。
原Rust源码/测试未改，复用225cc的18+4同字节证据，不声称0233再次运行18。
仓外selection-contract红/绿收据分别保留，修复后远端CI仍待实际head验证。

## 8. 合入0325后的联合验收

真实A `12ee0932` 与已归档0325的main `aa42bdc5` 正常合并为
`464309d64e1d366dc417d68f8dea3805d9b227fc` / tree `61ec756b71304350b251650dcd2830140d6665f2`。
十处冲突仅CLI双方测试追加、当前索引/事实与四图；生产保留B storage/readonly和A join/null/Deferred。
同head串行39exact=原B21+A18，均各1命中通过，65输入SHA前后一致；原4工程全通过，
实际baseaa42尺寸门禁、唯一composition成功路径exact及docs检查通过。
仓外joint39与engineering-joint39收据保留逐项真实结果；未运行本地聚合/故障邻项。
前置A run38836199049在12ee双host18/CI成功，仅为合B之前证据；Preview只核job/步骤。
联合发布head仍须新的实际CI成功后归档，不以前置CI替代；0325归档正文保留历史，0326仍active。
