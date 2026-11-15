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

- [x] R1：完整方法与根机械保全，仅精确可见域/相对路径与模块接线变化。
- [x] R2：原ownership定向target前后通过，逐片本地提交，不逐片push、不伪造RED。
- [x] R3：完整frontend、workspace all-target check、Clippy/fmt/policy/docs/尺寸/空白与独立精确diff审查。
- [ ] R4：Architecture/真实账本同步、集中push同#439；精确新头CI另验，active/Draft不归档合入。

原始证据留仓库外，未运行/ignored/compile/list与真实passed区分，不重复累计。

## 3. 真实证据

合同80d16c33，三个完整算法逐片定向通过后本地提交，不逐片push。

| 切片 | 提交 | 原六完整target前后 | 尺寸 |
|---|---|---|---|
| Single表达式遍历 | 27fd12da | 365/0/0 | 根1630→881，新754 |
| Unit表达式遍历 | f00f6760 | 365/0/0 | 根1500→874，新631 |
| Unit place四方法 | 864f135e | 365/0/0 | dataflow根1206→949，新262 |

六target为ownership_checking28/closures74/containers45/resource_deinit31/
multifile_ownership_checking164/constant_ownership23，四轮365FQN逐项相同，所有测试
与fixture未改，不计重复执行为新增覆盖。完整算法/剩余根机械按rustfmt核对：仅两
expression_inner和四place方法共六处pub(super)、Unit constant相对路径增层和私有模块
声明差异，未拆match arm、合并Single/Unit或复制state/helper。
policy仅删除single planner baseline1655/exception、Unit planner baseline1435/exception、
Unit dataflow baseline1185/exception六登记，其余JSON等值；实际1630/1500/1206与历史
登记分别记，不提高额度。

组末完整frontend **3779 passed、0 failed、13 ignored**（含12实际通过compile-fail
doctest），ignored不计执行，定向与全量不重复累计；workspace all-target cargo check
退出0（编译非下游测试运行）、frontend all-target Clippy -D warnings、fmt、policy47、
docs970、尺寸/空白均0。2190手写Rust，历史超限 **8→5**，本PR21→5。

独立fresh review完整80d16c33..864f135e真实3414行diff，补查共享类型与直接调用者，
No issues found：逐arm核Single/Unit求值、pending/短路/nullable/分支合流/返回中断、
call完成loan/resource/temporary清理顺序、Index backing owner；四place方法核reserved/
iteration loan优先级、borrowed/container/partial move、alias与mutable-this检查和最后
状态提交、diagnostic code/primary-label Span与入队次序。实际六方法可见域恢复，审查
任务简述曾口误五处，以精确diff与Spec范围六处为准。审查者只读不Cargo，日志不算其
独立重跑；父流程另验组末门禁。Architecture同步，原始证据/快照/完整diff/review留仓库外。

Rust组末门禁后未再改。未跑下游codegen/CLI/LSP测试全套、CI-only fault/真实校准/
sanitizer；R4集中push及精确新头CI另验，Spec active、PR Draft，不归档合入，不以旧头CI
或编译检查代替真实运行。剩余五根不代表必须机械拆散大型单一测试算法。
