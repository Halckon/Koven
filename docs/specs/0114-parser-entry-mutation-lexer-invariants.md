# SPEC-0114: 强化独立 Parser 入口 mutation Lexer 确定性

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-114` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006–0009、SPEC-0085–0090、SPEC-0093、SPEC-0103–0105、SPEC-0111–0113 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 六个独立 Parser 入口 mutation integration test、共享 entry mutation / Lexer test support、Architecture |
| 语言语义变更 | 否；只补齐既有独立入口 mutation corpus 的 Lexer 重复产物验收 |

## 1. Goal

完成后，独立 expression、declaration、block 入口的 prefix truncation、token omission、token
duplication、lexical poison replacement / insertion 与 adjacent transposition 六个矩阵均对全部
2,511 个源码执行两次生产 Lexer，并保持既有双 Parser、精确 mutation 及恢复证据。

## 2. 范围与需求

- 保持 12-case corpus、240 个显著 token，以及六矩阵的既有 mutation、lexical-mode 与精确
  relexed 分组不变。
- prefix 矩阵覆盖 12 个 clean preflight 与 747 个 UTF-8 prefix，共 759 个 source case。
- 其余五矩阵分别覆盖 baseline + mutation：omission 252、duplication 252、poison replacement
  492、poison insertion 516、transposition 240 个 source case。
- 六矩阵合计 2,511 个 source case；每例执行两次 Lexer 和两次对应入口 Parser，共验收
  5,022 个 Lexer 与 5,022 个 Parser 产物。
- 两次 Lexer 均验证 source identity、连续完整 byte 覆盖、唯一末尾 EOF、diagnostic primary /
  label Span 与完整 `Debug` 确定性；mutation 特定断言继续使用首个确定产物，完整 `Debug` 相等
  将相同 token / poison / lexical-mode 分段结论传递到第二个产物。
- 两次 Parser 继续验证 AST、diagnostic、typed root、syntax shape 与完整公开产物确定性。
- 双 Lexer 逻辑复用 SPEC-0103 的共享 helper；五个 token mutation 矩阵通过共享 entry mutation
  support 复用同一包装入口，不复制 callback 或 source setup。
- 不增加语料、依赖、生产 API、新诊断或语言语义。

## 3. 非目标

- 不改变 UTF-8 prefix 枚举、token slot / gap / lexical-mode 判定、mutation 拼接或 Parser 恢复。
- 不合并六个独立 integration test，也不改变现有矩阵职责和计数断言。
- 不修订 AST、diagnostic、grammar 或生产 Lexer / Parser。
- 不加入随机、fuzzer、snapshot 或第三方 property-testing 依赖。

## 4. 验收标准

- [x] 六矩阵的 759 / 252 / 252 / 492 / 516 / 240 source-case 计数保持成立。
- [x] 5,022 个 Lexer 产物全部满足覆盖、EOF、source、diagnostic Span 与确定性不变量。
- [x] mutation 特定 token / poison / lexical-mode 断言继续保持通过。
- [x] 5,022 个 Parser 产物继续满足 AST、diagnostic、typed root、shape 与确定性不变量。
- [x] 全部 2,511 个 source case 无内部错误或 panic。
- [x] 未发现生产缺陷；本 Spec 只修改测试与文档。
- [x] 六个直接相关窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

共享 `parser_entry_mutation_support` 包装 `lex_source_twice`，统一注入 entry support 已有的
`validate_lexed` 回调，并让 `baseline_slots` 与五个 mutation 调用方共享该入口。prefix 矩阵
直接复用底层 helper。所有后续 Parser 与 mutation-specific 检查只读取第一个已确定 Lexer
产物，避免改变既有测试观察面。

## 6. 实施计划

1. [x] 审计六个 entry mutation 矩阵 → 验证：2,511 个 source case 均为单 Lexer、双 Parser。
2. [x] 接入共享双 Lexer helper → 验证：2,511 个源码、10,044 个前端产物通过。
3. [x] 运行直接相关窄验收 → 验证：6/6，窄 Clippy 0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0114`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 六个独立入口 mutation 双 Lexer、Architecture 与完成记录 | `test(frontend): strengthen entry mutation lexer invariants (SPEC-0114)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| 六个 `parser_entry_*` mutation integration tests | 通过 | 6/6；2,511 个源码、10,044 个前端产物 |
| 同一组六个 integration tests 的窄 Clippy | 通过 | `-D warnings`；0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、428 tests、CLI build；0 failed / ignored / measured / filtered |
