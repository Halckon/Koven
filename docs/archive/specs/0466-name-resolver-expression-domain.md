# SPEC-0466：名称解析表达式遍历职责归位

> **性质**：实施 Spec · **状态**：done · **读取时机**：本批名称解析表达式职责整理时 · **唯一真源**：本批范围与验收，语义从属 Guide

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-0466` |
| 所属 Phase | Phase 2 名称解析 |
| 语言规范 | [Guide](../../guide/README.md)，无语义变更 |
| 批准依据 | 用户要求先合入#433、再从main继续新的 |
| 前置 Spec | [0465](0465-llvm-instruction-dispatch-families.md) |
| 影响范围 | name_resolution/resolver.rs与私有expression.rs、对应baseline |
| 语言语义变更 | 否 |
| 阻塞项 | 无 |

## 1. Goal与边界

从main `62a73cc7446517d262841a7b54370227fc4bd9c5` 新建fix/spec-0466，不再叠#433。
将完整 `Resolver::resolve_expression` 迁入私有 `resolver/expression.rs`，仅fn改pub(super)
恢复原父域可达，原scope/symbol/reference身份、AST遍历顺序、Span与诊断排序、所有递归
及helper调用不变。保留唯一Resolver/Binding/ScopeState与全部测试/fixture/FQN。
不扩public API、不增加allow/wrapper/clone、不改语言语义或跨文件环境。
根1116行降至1000内，只删对应baseline；不放宽尺寸额度，不清空34合理保留项。

## 2. 验收

- [x] R1：完整方法与剩余根机械保全（除scope/空白），测试字节不变。
- [x] R2：name_resolution、multifile_name_resolution、unit_name_snapshot迁移前后同FQN集通过。
- [x] R3：frontend完整测试、Clippy/fmt/docs/尺寸/policy与独立精确区间审查。
- [x] R4：Architecture/账本及集中push完成；精确代码头4d3f9068的PR CI43728630323通过，按本页最终交付账本闭合。

纯重组先GREEN→REFACTOR→GREEN，不伪造RED。原始日志/快照放仓库外，实际执行与
compile/list/ignored分别记；行为失败先停线定位，不能改断言绕过。

## 3. 真实证据

实现 `cbb5829a`、policy `3ad1d8e2`；根1116→920，新模块201。
完整197行方法仅fn→pub(super)，根剩余正文及imports严格机械相等；全部测试/fixture/FQN不变，
policy仅删除baseline1134一项，其他JSON值相同。定向三个完整target：
`name_resolution`、`multifile_name_resolution`、`unit_name_snapshot`前后各 **41/0/0**（19+15+7），
按target/名称集合一致。完整frontend全部目标汇总 **3779 passed / 0 failed / 13 ignored**，
包括12项实际通过的compile-fail doctest；ignored未计作执行。

frontend all-target Clippy（-D warnings）、fmt、47项policy tests、docs963、尺寸/空白退出0；
2170手写Rust，历史超限21→20。独立完整 `8bfd6971..3ad1d8e2` 三路径diff复核，确认
lambda implicit it、递归/访问顺序、member/type fallback、enum case诊断Span及祖先/兄弟
可见域保全，No issues found。Architecture同步实际归属。

仓库外核验脚本首次误将实际行数1116当作登记值，assert失败（工具会话保留错误输出，
未另外保存原始日志，不声称reviewer独立复现）；读取实际baseline1134后精确删除复验。
不是语言行为失败，未改断言绕过。本地未跑下游codegen/CLI/LSP全套或CI-only故障注入，
精确新PR CI另验。原始日志/快照/审查留仓库外。

## 最终交付账本（收口覆盖前述实施时的待项）

用户批准先收口合入#439，剩余file_parent1143与sibling_shared_loans1145后续独立治理，
现有两例外不改；本PR历史超限21→2，退休19根，不宣称清零或E3整体完成。
最新main仍为base62a73cc7，无新增组合代码；最终代码4d3f9068的
[精确PR CI43728630323](https://github.com/Halckon/Koven/actions/runs/43728630323)成功：
双宿主Targeted/Clippy、双Preview producer/独立consumer均success，Editor按过滤skipped不计执行。
各片定向测试、完整diff独审、失败与限制保留上文；本轮仅归档文档，不冒称重跑Rust。
本地iteration复验40/0/0，其中剩两文件7项实际passed。11份Spec按有界范围done归档；
归档头精确CI与正常merge状态由[PR439](https://github.com/Halckon/Koven/pull/439)追溯，未提前声称已合并。
