# SPEC-0471：Unit签名类型解析与body产物实现归位

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：本批Unit类型职责整理时 · **唯一真源**：本批范围与验收，语义从属 Guide

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P1-0471` |
| 所属 Phase | Phase 2 compilation-unit类型检查 |
| 语言规范 | [Guide](../../guide/README.md)，无语义变更 |
| 批准依据 | 用户继续下一组；同fix/spec-0466/#439 |
| 前置 Spec | [0470](0470-type-table-and-typed-file-domains.md)，输入767900b9 |
| 影响范围 | compilation_unit/signatures.rs、bodies.rs及私有模块/policy |
| 语言语义变更 | 否 |
| 阻塞项 | 无 |

## 1. 两片完整职责

1. resolve_type_ref至normalize_environment_type六个完整方法迁signatures/type_resolution.rs；
   原私有方法改pub(super)仅恢复signatures原有效域以支持根/兄弟调用。SignatureCollector、
   缓存与状态留根。保留递归求值与type_ref_types缓存、type参数检查及L0091等诊断顺序。
2. 完整impl CompilationUnitTypes迁bodies/product.rs；结构/Parts/validated wrapper及字段留根，
   原super路径增层，其他权限不变。保留owner/provenance、查询排序与has_valid_type_facts/
   basic/constant能力门，不能改变常量与基础所有权准入。

无wrapper/兼容层/新allow/ignore/额外clone，无公开API、语言、AST/Span/source身份或
诊断变化。测试/fixture/FQN不动，两根及新模块小于1000，只删对应baseline/exception，
其他policy等值，不提高额度或删除合理保留。

## 2. 验收

- [ ] R1：完整块/根机械保全、精确权限与路径变化；共享状态和算法时点不变。
- [ ] R2：Unit signature/body原定向target前后通过，逐片本地提交，不逐片push、不伪造RED。
- [ ] R3：完整frontend、workspace all-target check、Clippy/fmt/policy/docs/尺寸/空白与独立精确diff审查。
- [ ] R4：Architecture/真实账本同步、集中push同#439；精确新头CI另验，active/Draft不归档合入。

原始证据留仓库外，区分未运行/ignored/compile/list和真实passed，不重复累计。

## 3. 真实证据

待逐片记录。
