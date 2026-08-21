# SPEC-0097: 强化 Parser trivia 等价矩阵产物不变量

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-097` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并简化验收环节；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006、SPEC-0014、SPEC-0077、SPEC-0093–0096 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Parser trivia invariance 集成测试、Architecture |
| 语言语义变更 | 否；只强化公开产物验收 |

## 1. Goal

完成后，1,175 个非换行 trivia 变体除保持 significant token 与无 Span AST 结构不变外，
还会对两次完整文件解析分别验证 Lexer / AST / diagnostic Span、文件 roots 和 directive Span，
使“零诊断、确定等价”的结论建立在结构化公开产物证据上。

## 2. 范围与需求

- 保持 SPEC-0077 的 20 个 grammar case、3 个 trivia 载体、1,175 个变体与 2,350 次解析不变。
- 每个变体验证 Lexer source identity、连续 byte 覆盖、唯一末尾 EOF 与 diagnostic Span。
- 两次 Parser 产物分别验证 source identity、AST 全表、diagnostic 主 / label Span、全部文件
  roots，以及 package / import / segment / wildcard / alias Span。
- 两次产物都必须零诊断，且 syntax shape 与完整公开 `Debug` 产物各自确定一致；变体继续与
  baseline 比较 significant token 序列和无 Span syntax shape。
- 不增加语料、依赖、公开 API、新诊断或合法语法。

## 3. 非目标

- 不要求插入 trivia 前后的真实 Span 相等。
- 不改变 trivia、换行、声明分隔或 Parser 恢复语义。
- 不用本矩阵替代领域测试的精确 AST payload 或诊断断言。

## 4. 验收标准

- [x] 1,175 个 Lexer 产物均满足 source / coverage / EOF / diagnostic Span 不变量且零诊断。
- [x] 2,350 次 Parser 产物均满足 AST / diagnostic Span、roots 与 directive Span 不变量。
- [x] 每个变体的两次产物零诊断且 shape / Debug 确定一致。
- [x] 所有变体继续保持 baseline significant token 与无 Span syntax shape 不变。
- [x] 未发现生产缺陷；本 Spec 只修改测试与文档。
- [x] 直接相关窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

复用 `frontend_output_assertions` 验证通用 Lexer / AST / diagnostic 不变量，并在矩阵文件内以
最小 helper 验证完整文件 roots 与 directive Span。结构等价仍使用现有 `SyntaxShape`，不比较
随 trivia 插入而变化的真实 byte Span。

## 6. 实施计划

1. [x] 审计 trivia matrix 现有证据 → 验证：确认缺少结构化公开产物断言。
2. [x] 强化两次解析的产物断言 → 验证：1,175 case、2,350 次解析全部通过。
3. [x] 运行直接相关窄验收 → 验证：1/1，窄 Clippy 0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0097`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | trivia 产物不变量、Architecture 与完成记录 | `test(frontend): strengthen parser trivia invariants (SPEC-0097)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_trivia_invariance_matrix --locked --offline` | 通过 | 1/1；1,175 个变体、2,350 次解析 |
| `cargo clippy -p lang-frontend --test parser_trivia_invariance_matrix --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、428 tests、CLI build；0 failed / ignored / measured / filtered |
