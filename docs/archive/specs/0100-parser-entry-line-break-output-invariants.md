# SPEC-0100: 强化独立 Parser 入口 line-break 产物不变量

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-100` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并简化验收环节；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006–0009、SPEC-0078、SPEC-0092、SPEC-0093–0099 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 完整文件与独立入口 line-break 测试、共享 carrier support、Architecture |
| 语言语义变更 | 否；只统一 carrier 词法真源并强化独立入口产物验收 |

## 1. Goal

完成后，expression、declaration、block 三个独立入口的 60 个 line-break boundary 源码会与
完整文件矩阵共享同一组 carrier trivia 分段契约，并对 120 次 Parser 产物逐次验证 AST、诊断
与 typed root，避免两套 carrier 表漂移或只验证第一次解析。

## 2. 范围与需求

- 把 SPEC-0098 的 6 个结构 carrier、4 个非结构 carrier 及预期 `TriviaKind + spelling` 分段
  提取到单一 test support，由完整文件与独立入口矩阵共同复用。
- 60 个独立入口源码逐例验证 carrier 插入 byte 区域的精确 lexeme 分段，以及已有完整覆盖、
  唯一 EOF、source identity 和 Lexer diagnostic Span 不变量。
- 两次 Parser 产物分别验证 source identity、AST 四张表、diagnostic 主 / label Span，以及
  expression / declaration / block typed root 可解引用。
- 两次 syntax shape 与完整公开 `Debug` 产物必须确定一致；现有 carrier 分组、诊断码、bare
  return、enum comma 与 expression / block 中缀连续性断言保持不变。
- 完整文件矩阵的 80 个源码继续通过共享 carrier 合约，合计 140 个源码、280 次解析。
- 不增加语料、依赖、生产公开 API、新诊断或合法语法。

## 3. 非目标

- 不改变 line-break、comment、声明分隔或控制流语义。
- 不增加新的边界位置或跨不同 carrier 比较绝对 Span。
- 不替代领域测试对精确 AST payload 和诊断触发条件的验证。

## 4. 验收标准

- [x] 完整文件与独立入口矩阵只维护一份 10-carrier 词法分段真源。
- [x] 60 个独立入口源码均精确验证 carrier trivia 分段。
- [x] 120 次独立入口 Parser 产物均满足 AST / diagnostic Span 与 typed root 不变量。
- [x] 两次 shape / Debug 确定一致，既有边界语义断言保持通过。
- [x] 完整文件矩阵继续通过提取后的共享 carrier 合约。
- [x] 未发现生产缺陷；本 Spec 只修改测试与文档。
- [x] 两个 line-break 矩阵与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

新增 `parser_line_break_carriers` test support，集中静态 carrier 表和插入区间 lexeme 断言。
两个 integration test 仍各自维护语法位置与期望；独立入口宏对 first / repeated 产物执行完全
相同的结构校验后再生成现有指纹。

## 6. 实施计划

1. [x] 审计独立入口 line-break matrix → 验证：确认 carrier 词法证据缺失、表重复且第二次
   Parser 产物未结构化验证。
2. [x] 提取共享 carrier 并强化两次入口产物 → 验证：140 个源码、280 次解析保持通过。
3. [x] 运行两个直接相关窄验收 → 验证：4/4，窄 Clippy 0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0100`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 共享 carrier、独立入口产物不变量、Architecture 与完成记录 | `test(frontend): strengthen entry line break invariants (SPEC-0100)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_line_break_boundary_matrix --test parser_entry_line_break_boundary_matrix --locked --offline` | 通过 | 4/4；140 个源码、280 次解析 |
| `cargo clippy -p lang-frontend --test parser_line_break_boundary_matrix --test parser_entry_line_break_boundary_matrix --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、428 tests、CLI build；0 failed / ignored / measured / filtered |
