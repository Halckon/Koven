# SPEC-0326: if/when 封闭分支类型合并

> **性质**：有界类型修复 · **状态**：draft · **读取时机**：实施 v0.44 A 时 · **唯一真源**：本 Spec；语言合同见 Guide

| 字段 | 值 |
|---|---|
| 状态 | draft |
| Goal ID | `KOV-P2-0326` |
| 所属 Phase | Phase 2；不倒置 final admission 与 ownership |
| 语言规范 | [Guide 分支类型](../../../guide/06-blocks-control-flow.md#valuestatement-context-与分支类型) |
| 基线 | Guide `be3264bc7143fdddb160ce77039fb902694d0d77`；独立于 0325 实现分支 |
| 分支依赖 | stacked Guide PR110；尚未主干交付，不重复发布 Guide 提交 |
| 批准范围 | 用户已启用 v0.44 A 及封闭 join 细则；实施与验收分别推进 |
| 当前状态 | 首3项已采基线，后5项 tests-only 待审；无生产改动 |

## 1. 最小范围

single/unit 的 if 与 when 遵守同一封闭合同：同型、Nothing、nullable 提升、
同一 enum 实例 case/root；其他已知混型拒绝，不回退 Any。
保持 if L0089、when L0112、expected mismatch L0084；保留原错误与 Deferred reason。
statement when/无 else if 为 Unit；有 else if 即使结果被丢弃也检查 join。
不增加数值 widening、泛型协变、case 省参推断、while 收窄或后端新能力。

两个入口使用各自真实 TypeId 体系；优先各入口内部统一 if/when 判定，
不以本片引入跨体系大 trait 或类型事实重构。诊断关联实际首冲突来源，
不能把被跳过的 Error/Nothing 首分支误标为参与冲突的类型来源。

## 2. 拟定普通源码 exact 清单

拟新增 target `control_branch_join`，每项相同源码走 single/unit raw types 与 complete API。
首3已采普通标量子集，现补4–8待审；helper/argv重新固定独审后才采第二批。

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
7. `error_and_deferred_branches_preserve_original_roots`：普通未定义名字及已有 postfix ?，
   原诊断/Deferred reason 不被 ControlJoin 或成功 Any 取代；与同型/Nothing/冲突组合亦然。
8. `when_conflict_labels_track_actual_participating_branch`：前置 Nothing/Error 的三分支，
   primary/label 指向实际冲突，不硬指第一 entry；保留同根级联抑制。

每项完整 `cargo test --locked --offline -p lang-frontend --test control_branch_join NAME -- --exact --nocapture --test-threads=1`。
helper 不构造/修改 compiler facts、IR；无产物执行、注入或校准。冻结输入 SHA/HEAD/tree，
串行各实际1命中，基础 API/编译失败与行为红分账。先静态审 helper，再定向红测。

## 3. 后继验收与交付边界

已有要求 Any 的旧回归须按新合同精确更新并逐项审定，不宽跑旧 target。
公开 CLI 只 build 验证拒绝路径；成功 native 仅选已有 Int if/Boolean when 普通路径。
合法前端 nullable/enum join 不等于后端已经支持，结构化拒绝单独记账。
工程门禁与最终有限名单待冻结；红证后再登记 active、实施最小 patch。
Guide 与其他 Spec 发布由唯一 publisher 串行处理，本分支不改 0325 固定工作树。

首批3 exact仅采Int/String、Int/Long、Int同型与Nothing调用、Int/null/Int?双序；
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
