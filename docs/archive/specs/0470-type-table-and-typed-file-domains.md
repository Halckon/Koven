# SPEC-0470：类型表、typed产物实现与短路测试归位

> **性质**：实施 Spec · **状态**：done · **读取时机**：本批类型模型归位时 · **唯一真源**：本批范围与验收，语义从属 Guide

| 字段 | 值 |
|---|---|
| 状态 | done |
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

- [x] R1：完整块/剩余根机械核对，仅精确路径/模块/reexport变化，测试无漏重。
- [x] R2：逐片定向基线与迁移后通过、本地提交，不逐片push，不伪造RED。
- [x] R3：完整frontend、下游all-target check、Clippy/fmt/policy/docs/尺寸/空白与独立精确diff审查。
- [x] R4：Architecture/账本及集中push完成；精确代码头4d3f9068的PR CI43728630323通过，按本页最终交付账本闭合。

证据留仓库外，不把未跑/ignored/list/compile算真实运行，不重复累计测试。

## 3. 真实证据

合同d98cc9b9，三个切片逐片定向通过后本地提交，组末统一门禁/审查再集中push。

| 切片 | 提交 | 实际结果 | 行数 |
|---|---|---|---|
| canonical类型模型 | 8b96e29d | 原四完整target前后208/0/0 | 新210 |
| 全部TypedFile impl | ad23eefe | 同四完整target前后208/0/0 | model根1554→952，新399 |
| 短路六项测试 | f02fe6da | 原multifile_constant_ownership前后23/0/0 | 根1011→770，新246 |

四target为type_checking103+type_containers35+type_callable47+multifile_constant_ownership23，
不是新增208覆盖；六项测试仅增加short_circuit域前缀，原23个leaf相同，fixtures/assert保全。
模型两完整块及剩余root机械核对：仅精确super路径增层、私有声明/直接reexport与尾空白
差异，TypedFile结构/Parts字段原位。policy仅删model baseline1504与exception、test exception，
其他JSON等值；model实测1554与历史登记1504分别记，不提高额度。

整组完整frontend **3779 passed、0 failed、13 ignored**（含12实际通过compile-fail
doctest），ignored不计执行，定向与全量不重复累计。workspace all-target cargo check
退出0（包含下游编译、不是执行下游测试）；frontend all-target Clippy -D warnings、fmt、
policy47、docs967、尺寸/空白退出0，2182手写Rust，历史超限 **14→12**，本PR21→12。

独立fresh review完整d98cc9b9..f02fe6da真实diff，补查canonical core、上下游直接类型
消费者及常量事实门，No issues found：确认种子Builtin→Signed→Unsigned→Error、结构去重/
身份、analysis owner继承与字段域、descriptor查询及短路skip/pending/source oracle保全。
审查者只读不Cargo，读取既有日志不算独立重跑；组末门禁由父另验，原始证据/快照/完整
diff与审查留仓库外，Architecture同步私有实现职责。

Rust组末门禁后未再改，未运行下游codegen/CLI/LSP测试全套、CI-only故障注入/真实校准/
sanitizer；不借编译或旧头CI代替真实执行。R4集中push与精确新头CI另验，Spec active、
PR Draft，不归档或合入。

## 最终交付账本（收口覆盖前述实施时的待项）

用户批准先收口合入#439，剩余file_parent1143与sibling_shared_loans1145后续独立治理，
现有两例外不改；本PR历史超限21→2，退休19根，不宣称清零或E3整体完成。
最新main仍为base62a73cc7，无新增组合代码；最终代码4d3f9068的
[精确PR CI43728630323](https://github.com/Halckon/Koven/actions/runs/43728630323)成功：
双宿主Targeted/Clippy、双Preview producer/独立consumer均success，Editor按过滤skipped不计执行。
各片定向测试、完整diff独审、失败与限制保留上文；本轮仅归档文档，不冒称重跑Rust。
本地iteration复验40/0/0，其中剩两文件7项实际passed。11份Spec按有界范围done归档；
归档头精确CI与正常merge状态由[PR439](https://github.com/Halckon/Koven/pull/439)追溯，未提前声称已合并。
