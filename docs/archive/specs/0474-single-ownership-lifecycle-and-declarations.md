# SPEC-0474：Single ownership运行与声明职责归位

> **性质**：实施 Spec · **状态**：done · **读取时机**：本批Single ownership归位时 · **唯一真源**：本批范围与验收，语义从属 Guide

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-0474` |
| 所属 Phase | Phase 3 Single ownership |
| 语言规范 | [Guide](../../guide/README.md)，无语义变更 |
| 批准依据 | 用户继续下一组；同fix/spec-0466/#439 |
| 前置 Spec | [0473](0473-drop-expression-and-unit-place-access.md)，输入a94bc214 |
| 影响范围 | ownership_checking/checker.rs、私有lifecycle及现有declaration/policy |
| 语言语义变更 | 否 |
| 阻塞项 | 无 |

## 1. 两片完整职责

1. Checker::new/run两完整方法迁checker/lifecycle.rs，fn改pub(super)恢复checker原域，
   原super路径增层。保留名称/调用/容器/Map/capture构建顺序、代码catalog解析、诊断排序、
   错误/deferred清表与完整产物构造、常量事实和Map输入/输出验证门。
2. collect_mutability/check_variable/check_function三完整方法归入现有checker/declaration.rs，
   fn改pub(super)恢复checker原域；只补必要imports，不复制check_item或引入平行模块。
   保留正常initializer才建立binding、closure/source转移与borrow-return错误/上下文恢复。

Checker/Flows/State与所有字段/共享helper、入口与表达式/语句遍历留根；无wrapper/转换/
新allow/ignore/额外clone，无AST/source/Span/诊断或loan/drop/所有权语义变化。
测试fixture/FQN不动，根及新/增长模块小于1000，仅删checker baseline/exception其余
policy等值，不提高额度，不按容量拆三个大型单一iteration测试算法。

## 2. 验收

- [x] R1：完整块/根机械保全，只有限域/路径/模块与必要imports变化。
- [x] R2：原Single ownership定向target前后通过，逐片本地提交，不逐片push、不伪造RED。
- [x] R3：完整frontend、workspace all-target check、Clippy/fmt/policy/docs/尺寸/空白与独立精确diff审查。
- [x] R4：Architecture/账本及集中push完成；精确代码头4d3f9068的PR CI43728630323通过，按本页最终交付账本闭合。

原始证据留仓库外，未运行/ignored/compile/list与真实passed区分，不重复累计。

## 3. 真实证据

合同78b5dffb，两个完整职责逐片定向通过后本地提交，不逐片push。

| 切片 | 提交 | 原七完整target前后 | 尺寸 |
|---|---|---|---|
| Checker初始化/运行 | fd976aa0 | 219/0/0 | 新lifecycle339 |
| 声明收集/初始化/函数检查 | 19faedcb | 219/0/0 | 根1364→939，原declaration增长至186 |
| review finding修复 | 3e282e7c | 219/0/0 | 仅删迁移造成的三个无用imports |

七target为ownership_checking28/closures74/constants16/containers45/resource_deinit31/
borrow_result12/map13，基线、两片迁移后及修复后219FQN逐项相同，测试fixture未改。
完整方法/剩余根及原check_item按rustfmt核对，只有五pub(super)、原super路径增层、
私有模块声明、必要imports与移除本次造成的unused imports差异；无wrapper/额外clone。
policy只删checker baseline1371与exception，其余JSON等值；实际1364与登记分别记。

首轮完整frontend3779/0/13及workspace check通过后，Clippy真实失败：checker根残留
ItemId/FunctionBody/FunctionForm，与独立审查P1一致。3e282e7c只删三imports，既有目标
模块直接导入不受影响；保留clippy-first-failed.log，不加allow绕过。机械核对首轮预期
漏写新增方法前空行assert失败，定位仅空行后修预期通过，产品未因此改；工具输出留
会话、无独立首轮audit失败日志，不能当行为RED。

修后最终完整frontend **3779 passed、0 failed、13 ignored**（含12实际通过compile-fail
doctest），ignored不计执行，不把前后重复运行累计新增覆盖；workspace all-target cargo
check退出0（编译非下游测试运行）、frontend all-target Clippy -D warnings、fmt、policy47、
docs971、尺寸/空白均0；2191手写Rust、历史超限 **5→4**，本PR21→4。

独立fresh review完整78b5dffb..19faedcb真实930行diff，确认算法/owner与发布门保全，
找到unused imports P1；修后读取完整78b5dffb..3e282e7c真实更新diff和精确fix差异复核，
原finding闭合，No issues found。核初始化/capture/contracts顺序、诊断排序与错误清表、
Map双向门/常量analysis owner、closure/loan/deferred发布、正常initializer binding及
borrow-return上下文恢复。审查者只读不Cargo，父另验修后完整门禁。Architecture同步，
原始证据/完整diff/初审与复核留仓库外。

Rust最终门禁后未再改，未跑下游codegen/CLI/LSP测试全套、CI-only fault/真实校准/
sanitizer；R4集中push及精确新头CI另验，Spec active、PR Draft，不归档合入，不借旧头CI
或编译检查代替真实运行。剩余四根不意味着必须机械拆散单一大型测试算法。

## 最终交付账本（收口覆盖前述实施时的待项）

用户批准先收口合入#439，剩余file_parent1143与sibling_shared_loans1145后续独立治理，
现有两例外不改；本PR历史超限21→2，退休19根，不宣称清零或E3整体完成。
最新main仍为base62a73cc7，无新增组合代码；最终代码4d3f9068的
[精确PR CI43728630323](https://github.com/Halckon/Koven/actions/runs/43728630323)成功：
双宿主Targeted/Clippy、双Preview producer/独立consumer均success，Editor按过滤skipped不计执行。
各片定向测试、完整diff独审、失败与限制保留上文；本轮仅归档文档，不冒称重跑Rust。
本地iteration复验40/0/0，其中剩两文件7项实际passed。11份Spec按有界范围done归档；
归档头精确CI与正常merge状态由[PR439](https://github.com/Halckon/Koven/pull/439)追溯，未提前声称已合并。
