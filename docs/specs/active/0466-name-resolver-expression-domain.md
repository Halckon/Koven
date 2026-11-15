# SPEC-0466：名称解析表达式遍历职责归位

> **性质**：实施 Spec · **状态**：in-progress · **读取时机**：本批名称解析表达式职责整理时 · **唯一真源**：本批范围与验收，语义从属 Guide

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-P1-0466` |
| 所属 Phase | Phase 2 名称解析 |
| 语言规范 | [Guide](../../guide/README.md)，无语义变更 |
| 批准依据 | 用户要求先合入#433、再从main继续新的 |
| 前置 Spec | [0465](../../archive/specs/0465-llvm-instruction-dispatch-families.md) |
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

- [ ] R1：完整方法与剩余根机械保全（除scope/空白），测试字节不变。
- [ ] R2：name_resolution、multifile_name_resolution、unit_name_snapshot迁移前后同FQN集通过。
- [ ] R3：frontend完整测试、Clippy/fmt/docs/尺寸/policy与独立精确区间审查。
- [ ] R4：Architecture与真实账本同步、新PR/精确CI另验；保持active，不提前归档或合入。

纯重组先GREEN→REFACTOR→GREEN，不伪造RED。原始日志/快照放仓库外，实际执行与
compile/list/ignored分别记；行为失败先停线定位，不能改断言绕过。

## 3. 真实证据

待定向与组末实际记录。
