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
| 当前状态 | 首批3项 tests-only 准备；无生产改动，未运行本片测试 |

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
首批仅实现下列1–3的普通标量子集；其余仍是计划。helper/argv固定独审后才采红。

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
