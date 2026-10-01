# SPEC-0237: 八阶段本地整合与交叉契约验证

> **性质**：整合验收合同 · **状态**：in-progress · **读取时机**：验证或交付八个独立阶段的统一集成时 · **唯一真源**：本 Spec 的整合范围及最终验收

| 字段 | 值 |
|---|---|
| 状态 | in-progress |
| Goal ID | `KOV-INT-0237` |
| 所属 Phase | Phase 1 / 2 / 3 / 4 与文档治理整合 |
| 语言规范 | [Guide v0.40](../../guide/README.md) |
| 批准依据 | 2026-10-01 用户要求继续下一步，并随后授权先提交整合 PR、CI 通过后拉最新 main 审查 PR #6 |
| 前置 Spec | 无新增功能依赖；验证下列已批准切片的组合，不将未完成 CI 的切片假记 done |
| 关联 Spec | SPEC-0229、0230、0231、0232、0233、0234、0235、0236 |
| 前置 ADR | 无新增决定；保持各切片已接受 ABI/架构合同 |
| 阻塞项 | 最终整合门禁与 PR CI 尚待验收 |
| 影响范围 | 八阶段合并冲突、共享消费者与交叉回归、Guide/Spec/Architecture/inventory/DAG |
| 语言语义变更 | 不新增规则；仅整合 SPEC-0235 已批准的 v0.40，保留真实 v0.39 历史 |

## 1. Goal

在从 main 建立的 `feature/spec-0237-local-integration` 上形成可审计的八阶段整合 PR；
证明所选交叉契约与共享路径的实际结果，不以多个分支单独通过代替最终组合验证。

## 2. 范围与来源

- [0229 数值](0229-extended-numeric-literal-values.md)、[0230 Box enum](0230-recursive-boxed-enum-native.md)、
  [0231 TypeRef](0231-contextual-type-ref-trials.md)、[0232 原子原语 typed facts](0232-ownership-primitive-type-facts.md)、
  [0233 Compiler Contracts](0233-parser-compiler-contracts.md)、[0234 block 续行](0234-block-newline-continuation.md)、
  [0236 String.clone](0236-explicit-string-clone.md)先形成真实 v0.39（`ed0727f`）。
- [0235 三项批准规则](0235-approved-language-rules.md)随后重基式整合为唯一 v0.40；旧 clone
  迁移记录保留原路径，0235 旧候选历史另存；完整前版来源见[v0.40 账本](../../archive/migrations/v0.40-enablement.md)。
- 检查共享 typed/ownership/SSA 与 numeric + clone + Box 组合，不扩大既有功能边界。
- 保持统一 Cargo target、串行 Cargo，按直接行为/共享契约/下游风险选择门禁；禁止默认全量 frontend。
- `scripts/check_stage_integration.sh` 交付49个定向 frontend targets及新增
  `clone_primitive_integration` 的可复现本地门禁。现有 CI 仅运行 frontend lib；integration
  matrix 为独立本地验收，不声称远端 CI 覆盖这些目标。当前 GitHub 授权不含 workflow scope，
  因此本轮不改 workflow、不扩大权限；门禁提案保留待后续授权。

## 3. 非目标与遗留

不新造语言规则，不借整合实现 replace/swap native、deinit、Str、两阶段 receiver、unsafe
或其余演进计划。保留 multifile_type_checking 五项与 parser_call_argument 三项旧失败的
原始证据及本次实际复测结果；未修复不能写通过。Linux 本地结果不替代 macOS/远端 CI。

## 4. 验收标准

- [ ] 八阶段冲突解决和独立交叉审查无遗漏，提交范围清楚
- [ ] 所选 frontend、共享 Parser 资源、typed 事务与 drop planner 门禁逐项记录命中数/退出码
- [ ] codegen/CLI/LSP 下游与新增交叉行为、真实 native 分配析构验证完成
- [ ] 已知失败精确对照，不降低断言、不扩大忽略集合
- [ ] fmt、严格 clippy、workspace all-targets check 与文档/历史保全门禁完成
- [ ] 提交整合 PR，最终提交的必需 CI 全绿且无未决状态
- [ ] CI 通过后获取最新 main，独立审查 PR #6 的 v0.38 审计与文档编辑，再决定下一步

## 5. 最终整合验证账本

由整合负责人在最终源码状态下补充实际命令、测试命中数、退出码、平台与已知失败。
各切片旧账本可追溯，但不自动算作本节通过。当前执行中、未运行、filtered/ignored 和失败
必须分别记录；尚未执行的检查不勾选通过。

## 6. 交付边界

已获发布整合 PR 授权；不得自动合并、改写 main 或提前归档任何待 CI 的 Spec。
CI 通过后审查 PR #6 的后续工作仍以用户要求为界，不从审计结果擅自启用新语言规则。
