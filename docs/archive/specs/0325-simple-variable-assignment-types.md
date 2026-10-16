# SPEC-0325: 简单变量普通赋值类型事实

> **性质**：有界类型修复 · **状态**：done · **读取时机**：实现 p42 类型前置与所有权诊断时 · **唯一真源**：本 Spec；语言合同见 Guide

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P2-0325` |
| 所属 Phase | Phase 2；final admission 仍先于 Phase 3 ownership |
| 语言规范 | [Guide v0.44 简单赋值](../../guide/03-types-generics.md#简单变量赋值与整数-range-身份) |
| 批准依据 | 2026-10-15 用户批准 B 最小合同，随后明确启用 v0.44 及四组细则 |
| 基线 | Guide 分支 `be3264bc7143fdddb160ce77039fb902694d0d77`；其 actual main 祖先 `fcc5cc02c5f8b78b71dd703f864e04ad98c2a678` |
| 分支依赖 | Guide PR110已合入main `020ff41f`；本片真实远端同步后按PR111实施CI归档 |
| 前置 Spec | [0319](../../archive/specs/0319-final-type-admission-diagnostics.md)、[0323](../../archive/specs/0323-final-admission-static-recovery.md) |
| 关联实现 | [0218 unit 赋值事实](../../archive/specs/0218-compilation-unit-assignment-facts.md) |
| 语言语义变更 | 无新增合同；实现当前 Guide B，其他 A/C/D/E/F 不在本片 |

## 1. 范围与顺序

已解析简单变量普通 `=` 按声明 storage 类型检查 RHS，成功结果 Unit；RHS 后 kill 目标流事实。
分组目标透明。捕获、可变性、loan、move 继续按既有 ownership 阶段检查；不绕 final admission。
扩验暴露 single 普通只读根变量赋值漏检，本片在 RHS 后且既有 capture/loan 诊断优先之后，
补齐普通 `=` 的 Name/Group 根变量 L0134；不扩大 field/index/compound 或更改 ownership 阶段。
不扩大 field/index/compound，不实现 while 新收窄，不混入其他 v0.44 功能。
Unit 允许兼容值上下文；expected Int 的 lambda 尾赋值须拒绝，不能全局改成 statement。

原 p42 保存原字节，类型前置完成后仍须 L0135 拒绝捕获变量赋值，不是开放 mutable capture。
旧分支 `f5fc15759b723a6f86f90098752c81af29fd06df` 保留当时未启用/未执行历史；
本分支复用其普通源码测试并按新规范更新 Spec，未重写旧分支。

## 2. 定向验收

测试 target `final_type_admission`，模块 `simple_assignment`；每项均真实 single/unit-basic 成对：

1. `original_p42_capture_reaches_ownership`：原 p42，Unit 事实后准确 L0135。
2. `captured_and_local_tail_assignments_keep_ownership_boundary`：capture 拒绝/local var 接受。
3. `expected_non_unit_lambda_rejects_assignment_result`：Int expected 拒 Unit，保留 L0084。
4. `invalid_rhs_preserves_storage_type_diagnostic`：storage 不兼容 RHS，L0084/Span/完整诊断保留。
5. `grouped_variable_matches_plain_assignment`：分组透明。
6. `compound_assignment_keeps_deferred_boundary`：保留既有 Deferred/L0165 边界。
7. `nullable_storage_rhs_clears_target_refinement`：窄化后按 nullable storage 接受 null，随后读取 nullable；
   另以 `val after: Int = current` 精确拒绝，证明旧非空事实未残留。
8. `field_assignment_retains_existing_entry_difference`：仅保留已有 single/unit 差异，不扩 field。

helper 只普通源码 lex/parse/names/raw types/complete；complete 成功才 ownership，unit 用 basic validate。
此前端 helper 不修改 facts，不构造 IR，不运行 native/附件批次。p42 SHA256：
`6d99f070206795274701749b4a6cd475c0709882bb8dfb83a567e33fadf58137`。

固定 head/tree/input SHA、完整 exact argv 与 runner 经独审后串行红测；失败保留真实 raw。
每条命令 `cargo test --locked --offline -p lang-frontend --test final_type_admission simple_assignment::NAME -- --exact --nocapture --test-threads=1`。
编译/依赖/非单命中停报；行为红收齐后才最小实现，最终共享普通回归与工程门禁另冻结。

## 3. 当前结果

红测输入 `620782607dd01ca04827e222e2cda2d8f1ea65a4`，2026-10-15 串行8 exact各实际1命中，
16输入前后SHA一致，6行为红/2边界绿，无编译或API基础失败。p42 single仍L0165，
unit Unit后L0135；expected Int及非法RHS unit已有L0084，single未完成；nullable single
缺storage expected报L0083，unit按storage成功且后续Nullable。compound和field raw对照通过。
field真实single complete成功→L0135 receiver，unit complete成功→L0134 immutable，随后补锁此边界。
多case首断言红后未到达的local/group/nullable负例不称已执行。仓外原始收据保留在
`spec0325-evidence/red-receipt.json`；固定源码与选择可由该提交及上列exact重现。
已在红证后登记 active；以下保留各轮历史，最终本地验收见下表。
Guide 发布 PR110 的远端映射 head `e0c447f73ec896d9cc340d5168d1d0e32246f186` 与本地基线同tree；
其CI与合并另行核验。发布本分支须由唯一publisher按映射保留真实父链，不覆盖Guide分支。

首轮实现 `c4229343` 的8 exact为7通过/1 oracle失败，19输入不变。第7负例真实两入口均
L0084 98..105，complete原诊断保留；expected mismatch的既有规则将错误读取标Error，
测试误期Nullable。只修oracle，保留原收据；正例Nullable与ownership空均已实际通过。

扩验 `87af105d` 的15 exact：12通过、1真实只读变量漏检、2测试输入/oracle错误。
chain fixture缺声明分隔符产生L0047；single CLI真实exit2/L0135，误期project的exit1。
两项仅tests修正，保留原raw。CLI四个build均产生L0135，无产物执行。
直接 actual main fcc5 的tests-only `10377e63` 单exact观测确认旧single也为
Deferred/complete成功/ownership无诊断，unit L0134；此观测命令失败，不算回归绿。
新增只读/local参数与Unit链式2 exact、既有类型失效/捕获/unit assignment3 exact、
p42 single/project human/json仅build2 exact，共15唯一目标；各轮原失败保留。

## 4. 前端与拒绝诊断阶段验收

固定实现/测试输入 `d805381ff174dc4ec52ea8160fdbdce1aa90c0d5` / tree
`43f334da8268547630cffcf7198e7d56bbbaf17b`，2026-10-15：

| 验收 | 实际结果 |
|---|---|
| 原8+readonly/chain2+既有共享3+CLI2 | 15唯一exact各1命中通过，35输入SHA前后一致 |
| p42公开CLI | single/project各human/json仅build，共4进程；L0135 129..132，退出分别2/1，无产物 |
| fmt / workspace all-target check | 通过；check使用locked/offline |
| frontend+CLI all-target clippy / CLI build | 通过；locked/offline，clippy禁止warnings |
| 历史工程失败 | 初次fmt排版失败；v3 clippy固定空prefix日志失败，build当轮未跑；机械修后最终4项均重跑 |
| 当时未运行 | native正向/资源产物、远端双宿主CI；后续普通native补验见§5 |

仓外最终收据 `spec0325-evidence/final15-v4-receipt.json` 与 `engineering-v4-receipt.json`；
所有旧红、oracle错误、原main观测分别保留，未覆盖。源码输入可按上述固定提交与exact名单重现。
两处既有超1000行Rust文件只增加分发/调用4行：type expression 1207→1211，ownership checker
1282→1286；判断分别放独立71/60行helper，增量已逐行审查，未为尺寸重构无关行为。

CI接线仅核现有配置：stage选final_type_admission/type_checking/multifile_type_checking/
ownership_closures，core选lang-cli全部测试；本片未改脚本/workflow。PR的crates改动触发
macos-14/ubuntu-24.04门禁，仍受既有reuse/前置成功/事件条件约束；未把静态接线当作CI通过。
Guide PR110未合期间只备份stacked分支，publisher按远端映射保留父链，再单独交付本Spec。

## 5. 普通 native 补验与统一本地结果

固定 `d0ea63aba21a1fff89174c8264884783c108b9bf` / tree
`068357d6b03bfe9d970b44c581f5159245a784a3`，2026-10-15 同 head 统一重跑：
原15加 `single_local_assignment_unit_value_executes`、
`project_local_assignment_unit_value_executes`，共17唯一exact各1命中通过，36输入SHA前后一致。
两新增用例分别位于 native_cli/project_cli 的 final_type_admission 模块；普通Int局部var
赋值作为Unit值，分别build一次并运行一次；断言exit0、stdout为 `assigned\n`、stderr空。
子进程结果由测试断言验证，raw保留libtest结果，没有另导出的子进程逐项日志。
同head fmt、workspace all-target check、frontend+CLI all-target clippy和CLI build四门禁均通过；
后3项locked/offline，clippy禁止warnings。独立native2与原15阶段收据保留，未改写原失败。
统一仓外收据为 `spec0325-evidence/final17-receipt.json` 与 `engineering-final17-receipt.json`。
本片已具备上述有限本地验收；仍待远端双宿主CI与主干交付，Spec保持active。
普通Int赋值成功不外推nullable/资源值native能力，也不声称其他v0.44片完成。

## 6. PR111 尺寸门禁修复

2026-10-16，PR111 run `38832832352` 的 Rust Physical Line Guard 在真实 base
`020ff41f` 拒绝 expression.rs 的 1211 行（base 1207）；此前增量审阅不等于该门禁通过。
后继 `6b8a630b6c14740cc7cc97083a190da93f1208e0` 从实际远端 `358133bf` 继承，
将完整赋值分发、旧fallback/Rc诊断及flow kill/version移入既有私有assignment模块，
求值与错误顺序不变；expression降至1173行，ownership未改且仍在既有1429行例外内。
未调整尺寸阈值；实际 `check_rust_sizes.py --base 020ff41f` 全部通过。
同一修复head原17 exact各1命中通过、36输入SHA前后不变，原四工程检查全部通过；
仓外 `size-repair17-receipt.json` 与 `engineering-size-repair17-receipt.json` 保存结果。
原CI失败及历史本地结果保留；修复远端CI仍须对实际发布head独立验证。

同次CI双host还发现 `resolved_roles_and_symbolic_types_keep_raw_value_facts` 的普通
`n = identity(1)` 旧oracle仍要求Deferred；在738ba424本地单exact真实复现，改为single/unit
精确Unit，其他roles与descriptor断言保留。静态反查另识别Guide litmus 01/03/11的四个
普通局部赋值过期incomplete快照；各exact先红（single断言提前结束，未执行unit），随后
只移除已完成的local快照，保留listOf、field、compound等未完成事实及原ownership检查。
固定 `23bd90cdf9c446557d75078f2bd8557cb872e1f8` 同head统一21exact（原17+上述4）
各1命中通过、38输入前后一致；四工程及真实base020ff完整尺寸检查通过。
仓外 `ci-repair21-receipt.json`、`engineering-ci-repair21-receipt.json` 保留真实结果；
新增litmus绿包含两入口分析/ownership，不是native执行。以上未冒充修复后的远端CI。

## 7. 实施 CI 有限归档

2026-10-16，[PR111 implementation CI](https://github.com/Halckon/Koven/actions/runs/38834828940/attempts/1)
在实际head `1c0c95ce8686f8dc0d09dfa2983fbf87cd7b87eb` / tree
`0b2f58388c7b251d941ac7b893062baea0a8a5ec` 终态success，reuse=false；15个job实际成功、
1个Tree-sitter路径范围skip。synthetic `e4e7d603884192312de500b43f0be5472258a133`
的父为main020ff41f与本实施head，tree一致；双host受审21exact逐项各1命中通过。
Preview仅核jobs/步骤成功，未下载或逐项审查artifact，不声称原生产物逐项核验。
独立收据 `pr111-ci-38834828940-evidence.json` SHA256
`87a3af4b05a7fc468d501b4c0f76ad6625f7d5772c2e605e64079bc6fa88784b` 保存在仓外。
本归档只关闭本Spec的简单变量赋值/既有readonly义务，不开放capture或field/index/compound，
不代表v0.44其他片完成。历史红、旧oracle、尺寸失败与有限native范围保留。
归档head的CI及normal merge/main交付仍须分别核验，不以上述实现CI冒充。
