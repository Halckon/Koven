# SPEC-0470：类型表、typed产物实现与短路测试归位

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：本批类型模型归位时 · **唯一真源**：本批范围与验收，语义从属 Guide

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P1-0470` |
| 所属 Phase | Phase 2类型模型与Phase 3常量所有权测试 |
| 语言规范 | [Guide](../../guide/README.md)，无语义变更 |
| 批准依据 | 用户继续下一组；同fix/spec-0466/#439 |
| 前置 Spec | [0469](0469-expression-and-resource-exchange-test-domains.md)，输入31a25d18 |
| 影响范围 | type_checking/model.rs私有模块与multifile_constant_ownership测试/policy |
| 语言语义变更 | 否 |
| 阻塞项 | 无 |

## 1. 三片范围

1. 完整TypeId/DeferredReason/IntegerConstraint/TypeKind/TypeTable及其impl迁入
   model/type_table.rs，model直接pub use保持原路径与同一名义类型，无wrapper，字段权限不变。
2. 所有TypedFile impl（构造与查询）迁model/typed_file.rs，TypedFile/Parts结构字段留根；
   方法/字段可见域不变，原super::类型路径增层匹配原指向，私有字段可由子模块读取。
3. 完整short_circuit六项测试迁multifile_constant_ownership/short_circuit.rs，shared
   with_unit/harness与其他测试留根；正文、fixture/Span/source/plan oracle不变，FQN仅加域。

不改语言/诊断、canonical插入顺序和去重、environment/name/analysis owner身份；无新allow/
ignore/额外clone/兼容层。两根及新模块均小于1000，只删对应baseline/exception，其他policy
等值，不提高额度，不删除合理保留或导入未合分支。

## 2. 验收

- [ ] R1：完整块/剩余根机械核对，仅精确路径/模块/reexport变化，测试无漏重。
- [ ] R2：逐片定向基线与迁移后通过、本地提交，不逐片push，不伪造RED。
- [ ] R3：完整frontend、下游all-target check、Clippy/fmt/policy/docs/尺寸/空白与独立精确diff审查。
- [ ] R4：Architecture/真实账本同步、集中push同#439；精确新头CI另验，active/Draft不归档合入。

证据留仓库外，不把未跑/ignored/list/compile算真实运行，不重复累计测试。

## 3. 真实证据

待逐片记录。
