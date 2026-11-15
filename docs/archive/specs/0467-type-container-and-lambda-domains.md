# SPEC-0467：类型检查容器与 lambda 领域归位

> **性质**：实施 Spec · **状态**：done · **读取时机**：本批类型检查职责整理时 · **唯一真源**：本批范围与验收，语义从属 Guide

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-0467` |
| 所属 Phase | Phase 2 类型检查 |
| 语言规范 | [Guide](../../guide/README.md)，无语义变更 |
| 批准依据 | 用户要求一次批量、允许多次提交；继续#439 |
| 前置 Spec | [0466](0466-name-resolver-expression-domain.md)，输入92f30062 |
| 影响范围 | checker/container.rs、expression.rs与私有领域模块、对应尺寸policy |
| 语言语义变更 | 否 |
| 阻塞项 | 无 |

## 1. 三片范围

1. 容器构造：完整check_list_form_construction、check_named_container_construction、
   finish_container_construction移container/construction.rs。前两方法pub(super)恢复container私有域，
   CheckedConstruction/ContainerCall与通用helper留根。
2. MutableList修改：prepare_mutable_list_call及其后全部add/clear/removeAt/endpoint removal/
   removeFirst/removeLast/insertAt方法移container/mutation.rs。原pub(super)方法改
   pub(in crate::type_checking::checker)保持原checker有效域，私有helper不扩权。
3. lambda：完整check_lambda/check_lambda_inner移expression/lambda.rs，前者pub(super)
   恢复expression域，LambdaSyntax保留根。保持outer facts/versions/pending恢复与原错误传播。

原方法/字段/descriptor身份、AST/source/span、诊断顺序与正文、trial和flow时点不变；
所有测试/fixture/FQN不改，无wrapper/新allow/额外clone。根和子文件均小于1000；仅删
对应baseline/exception登记，其他JSON等值，不提高额度或扩展语言能力。

## 2. 节奏与验收

- [x] R1：各完整块及剩余根机械保全，权限有效域等价，测试字节不变。
- [x] R2：逐片定向GREEN→迁移→GREEN，可构建本地提交，不逐片push。
- [x] R3：整组frontend完整测试、下游名称/类型消费者、Clippy/fmt/docs/policy/尺寸/空白。
- [x] R4：Architecture/账本及集中push完成；精确代码头4d3f9068的PR CI43728630323通过，按本页最终交付账本闭合。
- [x] R5：Architecture/账本及集中push完成；精确代码头4d3f9068的PR CI43728630323通过，按本页最终交付账本闭合。

原始日志/快照留仓库外。不伪造RED，不把ignored/compile/list算执行，不以清空34保留项为目标。

## 3. 真实证据

合同535d39a2，以下三片未逐片push；每片原185项选择实际通过后本地提交。

| 切片 | 本地提交/实际结果 | 行数 |
|---|---|---|
| container construction | 7761d18b；type_containers35+type_callable47+type_checking103，前后185/0/0 | 新模块246 |
| MutableList mutation | 952ce171；同完整三个target，前后185/0/0 | container根1178→451，新mutation491 |
| lambda | 48de11e4；同完整三个target，前后185/0/0 | expression根1113→938，新lambda181 |

完整块/剩余根机械核对：除明确pub域和空白外文本相同；三个target逐名称集前后相同，
全部测试/fixture/FQN不改；policy只删container exception、expression baseline1204与
exception三项，其他JSON等值。没有增加allow、wrapper或clone。

Architecture初次新增段落207行触发200行入口上限，精简本批概述并入对应段落后复验，Rust未变。

组末frontend全部目标合计 **3779 passed、0 failed、13 ignored**（含12实际通过的
compile-fail doctest），不是新增3779测试；ignored不计执行。frontend all-target Clippy
（-D warnings）、fmt、47项policy、docs964/尺寸/空白退出0；2173手写Rust、历史超限
**20→18**。原0466的41项亦在完整frontend组合中执行，不重复累计。

独立fresh review完整535d39a2..48de11e4真实diff及共享类型/调用点，确认construction
publication门、mutation receiver/pending/operand顺序、lambda outer flow恢复和inner
既有错误传播不变，No issues found。Architecture同步私有领域与旧Single边界；代码在
组合门禁后未再修改。原始日志/快照与完整diff/review留仓库外。

R3中下游消费者本轮实际覆盖frontend类型target与共享名称路径；未另跑codegen/CLI/LSP
运行时全套、CI-only fault/校准或sanitizer，不能以frontend全绿代替其执行；官方精确头CI
另验。R4的独立审查/Architecture/账本已完成，待同PR集中push；R5保持待验。

## 最终交付账本（收口覆盖前述实施时的待项）

用户批准先收口合入#439，剩余file_parent1143与sibling_shared_loans1145后续独立治理，
现有两例外不改；本PR历史超限21→2，退休19根，不宣称清零或E3整体完成。
最新main仍为base62a73cc7，无新增组合代码；最终代码4d3f9068的
[精确PR CI43728630323](https://github.com/Halckon/Koven/actions/runs/43728630323)成功：
双宿主Targeted/Clippy、双Preview producer/独立consumer均success，Editor按过滤skipped不计执行。
各片定向测试、完整diff独审、失败与限制保留上文；本轮仅归档文档，不冒称重跑Rust。
本地iteration复验40/0/0，其中剩两文件7项实际passed。11份Spec按有界范围done归档；
归档头精确CI与正常merge状态由[PR439](https://github.com/Halckon/Koven/pull/439)追溯，未提前声称已合并。
