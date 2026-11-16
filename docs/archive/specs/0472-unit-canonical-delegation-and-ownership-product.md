# SPEC-0472：Unit canonical、委托与ownership产物归位

> **性质**：实施 Spec · **状态**：done · **读取时机**：本批Unit模型归位时 · **唯一真源**：本批范围与验收，语义从属 Guide

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-0472` |
| 所属 Phase | Phase 2/3 Unit模型与ownership产物 |
| 语言规范 | [Guide](../../guide/README.md)，无语义变更 |
| 批准依据 | 用户继续下一组；同fix/spec-0466/#439 |
| 前置 Spec | [0471](0471-unit-signature-types-and-body-product.md)，输入0d7335e3 |
| 影响范围 | type_checking/compilation_unit/model.rs、ownership_checking/compilation_unit.rs私有模块/policy |
| 语言语义变更 | 否 |
| 阻塞项 | 无 |

## 1. 三片完整职责

1. UnitTypeId/UnitFunctionParameterType/UnitTypeKind/UnitTypeTable及全部impl移model/type_table.rs，
   原直接pub use保持名义类型/API路径；discard_after/take_after的pub(super)改
   pub(in crate::type_checking::compilation_unit)恢复原有效域，不影响trial checkpoint语义。
2. UnitDelegationPlan/Forwarder/Implementation/NextHop及私有Resolution与完整impl移
   model/delegation.rs，四类型直接pub use；私有状态及forwarder排序随完整职责迁移。
3. 全部impl CompilationUnitOwnership移compilation_unit/product.rs，结构/字段/wrapper留根；
   原new改pub(super)恢复compilation_unit域，原super路径精确增层，错误时事实筛选、
   source-qualified查询与validate基本/常量边界不变。

所有状态/owner/source/Span与诊断顺序、canonical初始顺序/回滚/去重、delegation路径身份
与次序均保留。无wrapper/转换/新allow/ignore/额外clone，无测试fixture/FQN变更，
两根及新文件小于1000，仅删两根baseline/exception其他policy等值，不提高额度。

## 2. 验收

- [x] R1：完整块/剩余根机械保全，只有精确可见域/路径/直接reexport与模块声明变化。
- [x] R2：原Unit type/ownership定向target前后通过，逐片本地提交，不逐片push、不伪造RED。
- [x] R3：完整frontend、workspace all-target check、Clippy/fmt/policy/docs/尺寸/空白与独立精确diff审查。
- [x] R4：Architecture/账本及集中push完成；精确代码头4d3f9068的PR CI43728630323通过，按本页最终交付账本闭合。

原始证据留仓库外，未运行/ignored/compile/list与真实passed分别记，不重复累计。

## 3. 真实证据

合同3e32ccc3，三个完整切片逐片定向通过后本地提交，不逐片push。

| 切片 | 提交 | 原五完整target前后 | 尺寸 |
|---|---|---|---|
| Unit canonical模型 | 33faf735 | 506/0/0 | 新185行（含后续fmt折行） |
| delegation完整模型 | eca88b11 | 506/0/0 | type model1327→887，新269 |
| 完整ownership产物impl | 7386e462 | 506/0/0 | ownership根1152→574，新581 |

五target为multifile_type_signatures24/type_checking288/type_member_graph7/
ownership_checking164/constant_ownership23，四轮506FQN逐项相同，测试fixture全未改。
完整块/剩余根按rustfmt核对，仅明确pub域/路径/模块/直接reexport差异；type_table
checkpoint两方法保持原compilation-unit域，ownership new仅恢复原compilation-unit域。
模型私有Resolution和forwarder sort随同迁移，既有unreachable分支保留而非新增。
policy仅删type model baseline1404/exception与ownership baseline1116/exception四项，
其余JSON等值，实际1327/1152与登记分别记，不提高额度。

机械核对首轮重建root时把两次新增声明的插入顺序写反，assert失败；逐段diff确认仅新
声明相对位置，按实际明确插入位置重建后通过，产品未因脚本失败修改。工具输出留会话，
无单独首轮失败日志；不当行为RED或隐去失败。

整组完整frontend **3779 passed、0 failed、13 ignored**（含12实际通过compile-fail
doctest），ignored不计执行，定向与全量不重复累计。workspace all-target cargo check
退出0（编译非下游测试运行）、frontend all-target Clippy -D warnings、fmt、policy47、
docs969、尺寸/空白均0；2187手写Rust，历史超限 **10→8**，本PR21→8。

独立fresh review完整3e32ccc3..7386e462真实diff及canonical撤回、forwarder发布排序、
错误清理driver、const专用入口等直接消费者，No issues found：确认类型身份/回滚、
私有状态/顺序、successful与deferred表筛选、owner/analysis链和basic/const拒绝门、loan
事实权限不扩展。审查者只读不Cargo，读日志不算重跑；父流程另验组末门禁。
Architecture同步职责，原始证据/快照/完整diff/review与机械结论留仓库外。

Rust组末门禁后未再改，未运行下游codegen/CLI/LSP测试全套、CI-only fault/真实校准/
sanitizer；R4集中push与精确新头CI另验，Spec active、PR Draft，不归档合入，不借旧头CI
或编译检查代替真实运行测试。

## 最终交付账本（收口覆盖前述实施时的待项）

用户批准先收口合入#439，剩余file_parent1143与sibling_shared_loans1145后续独立治理，
现有两例外不改；本PR历史超限21→2，退休19根，不宣称清零或E3整体完成。
最新main仍为base62a73cc7，无新增组合代码；最终代码4d3f9068的
[精确PR CI43728630323](https://github.com/Halckon/Koven/actions/runs/43728630323)成功：
双宿主Targeted/Clippy、双Preview producer/独立consumer均success，Editor按过滤skipped不计执行。
各片定向测试、完整diff独审、失败与限制保留上文；本轮仅归档文档，不冒称重跑Rust。
本地iteration复验40/0/0，其中剩两文件7项实际passed。11份Spec按有界范围done归档；
归档头精确CI与正常merge状态由[PR439](https://github.com/Halckon/Koven/pull/439)追溯，未提前声称已合并。
