# SPEC-0474：Single ownership运行与声明职责归位

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：本批Single ownership归位时 · **唯一真源**：本批范围与验收，语义从属 Guide

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
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

- [ ] R1：完整块/根机械保全，只有限域/路径/模块与必要imports变化。
- [ ] R2：原Single ownership定向target前后通过，逐片本地提交，不逐片push、不伪造RED。
- [ ] R3：完整frontend、workspace all-target check、Clippy/fmt/policy/docs/尺寸/空白与独立精确diff审查。
- [ ] R4：Architecture/真实账本同步、集中push同#439；精确新头CI另验，active/Draft不归档合入。

原始证据留仓库外，未运行/ignored/compile/list与真实passed区分，不重复累计。

## 3. 真实证据

待逐片记录。
