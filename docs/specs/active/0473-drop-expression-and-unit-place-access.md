# SPEC-0473：Drop表达式遍历与Unit place访问检查归位

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：本批ownership算法归位时 · **唯一真源**：本批范围与验收，语义从属 Guide

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P1-0473` |
| 所属 Phase | Phase 3 ownership/drop planning |
| 语言规范 | [Guide](../../guide/README.md)，无语义变更 |
| 批准依据 | 用户继续下一组；同fix/spec-0466/#439 |
| 前置 Spec | [0472](0472-unit-canonical-delegation-and-ownership-product.md)，输入c4581814 |
| 影响范围 | ownership checker/drop_planner、Unit dataflow及drop_planner私有模块/policy |
| 语言语义变更 | 否 |
| 阻塞项 | 无 |

## 1. 三片完整算法

1. Single DropPlanner::expression_inner整方法移checker/drop_planner/expression.rs，fn改
   pub(super)仅恢复原planner域；不拆match arm，所有求值/临时值/cleanup顺序保留。
2. Unit DropPlanner::expression_inner整方法移compilation_unit/dataflow/drop_planner/expression.rs，
   fn改pub(super)恢复原planner域，原super::super::constant指向精确增层。
3. Unit Checker::use_name/consume_place/ensure_available/access_place四完整方法移
   compilation_unit/dataflow/place_access.rs，四fn改pub(super)恢复原dataflow域。

planner/Checker结构、共享ValueState/State、入口和全部helper留根，不合并Single/Unit，
不引入wrapper/转换/新allow/ignore/额外clone。原source/span/诊断、pending/branch/loan/
resource/drop、path状态和能力门不变，所有测试fixture/FQN原样。三根及新模块小于1000；
只删三根baseline/exception其余policy等值，不提高额度，不按容量拆大型测试算法。

## 2. 验收

- [ ] R1：完整方法与根机械保全，仅精确可见域/相对路径与模块接线变化。
- [ ] R2：原ownership定向target前后通过，逐片本地提交，不逐片push、不伪造RED。
- [ ] R3：完整frontend、workspace all-target check、Clippy/fmt/policy/docs/尺寸/空白与独立精确diff审查。
- [ ] R4：Architecture/真实账本同步、集中push同#439；精确新头CI另验，active/Draft不归档合入。

原始证据留仓库外，未运行/ignored/compile/list与真实passed区分，不重复累计。

## 3. 真实证据

待逐片记录。
