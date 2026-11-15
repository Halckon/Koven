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

- [x] R1：完整块/根机械保全、精确权限与路径变化；共享状态和算法时点不变。
- [x] R2：Unit signature/body原定向target前后通过，逐片本地提交，不逐片push、不伪造RED。
- [x] R3：完整frontend、workspace all-target check、Clippy/fmt/policy/docs/尺寸/空白与独立精确diff审查。
- [ ] R4：Architecture/真实账本同步、集中push同#439；精确新头CI另验，active/Draft不归档合入。

原始证据留仓库外，区分未运行/ignored/compile/list和真实passed，不重复累计。

## 3. 真实证据

合同7c89ddfc，两个完整职责逐片定向通过后本地提交，不逐片push。

| 切片 | 提交 | 原四完整target前后 | 尺寸 |
|---|---|---|---|
| signature类型解析六方法 | babefa41 | 316/0/0 | signatures1038→798，新245 |
| 完整body产物impl | 8efff38b | 316/0/0 | bodies1318→879，新442 |

定向四target：multifile_type_checking288、multifile_type_signatures24、signature
provenance2、determinism2，三轮FQN逐项相同。没有新增测试覆盖或改测试/fixture。
两完整块与剩余根机械保全：仅六fn→pub(super)、两super路径精确增层、私有模块声明变化。
policy只删除signatures baseline1144/exception、bodies baseline1337/exception四项，
其他JSON等值；实际1038/1318与历史登记分别记，不提高尺寸额度。

整组完整frontend **3779 passed、0 failed、13 ignored**（含12实际通过compile-fail
doctest），ignored不计执行，定向不重复累计。首条组合命令链900秒超时；完整frontend
已退出成功（其后&&已进入Python门禁），workspace check日志也已结束，但不能因此宣称
整条门禁通过。确认本会话无遗留Cargo后补跑workspace all-target check退出0（缓存0.47s，
编译非下游测试运行）、frontend all-target Clippy -D warnings退出0（7.03s），fmt、
policy47、docs968、尺寸/空白0。原始超时输出留会话，已结束日志保留，不清缓存或干扰
其他会话。2184手写Rust，历史超限 **12→10**，本PR21→10。

独立fresh review完整7c89ddfc..8efff38b真实1460行diff与直接消费者，No issues found：
确认缓存写回与递归顺序、enum case检查/Box委托、兄弟方法权限、source-qualified查询/
owner与身份链、pending/error及五类事实验证门、basic/const能力发布互不混淆。
审查者只读不Cargo，读取日志不算独立重跑；父流程补齐组末门禁。Architecture同步职责，
原始证据/快照/完整diff与review留仓库外。

Rust组末门禁后未再修改。未运行下游codegen/CLI/LSP测试全套、CI-only fault/真实校准/
sanitizer；R4集中push与精确新头CI另验，Spec active、PR Draft，不归档合入，不借旧头CI
或编译结果代替运行测试。
