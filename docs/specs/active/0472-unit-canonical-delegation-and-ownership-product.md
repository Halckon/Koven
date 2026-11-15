# SPEC-0472：Unit canonical、委托与ownership产物归位

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：本批Unit模型归位时 · **唯一真源**：本批范围与验收，语义从属 Guide

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
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

- [ ] R1：完整块/剩余根机械保全，只有精确可见域/路径/直接reexport与模块声明变化。
- [ ] R2：原Unit type/ownership定向target前后通过，逐片本地提交，不逐片push、不伪造RED。
- [ ] R3：完整frontend、workspace all-target check、Clippy/fmt/policy/docs/尺寸/空白与独立精确diff审查。
- [ ] R4：Architecture/真实账本同步、集中push同#439；精确新头CI另验，active/Draft不归档合入。

原始证据留仓库外，未运行/ignored/compile/list与真实passed分别记，不重复累计。

## 3. 真实证据

待逐片记录。
