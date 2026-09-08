# SPEC-0109: 强化 Parser trivia 等价矩阵 Lexer 确定性

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-109` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006、SPEC-0014、SPEC-0077、SPEC-0097、SPEC-0103–0108 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Parser trivia-invariance integration test、共享 Lexer test support、Architecture |
| 语言语义变更 | 否；只补齐既有非换行 trivia corpus 的 Lexer 重复产物验收 |

## 1. Goal

完成后，20 个完整 grammar case 与 3 个非换行 trivia 载体生成的 1,175 个源码变体，均执行
两次生产 Lexer 与两次完整文件 Parser，并继续证明 significant token 和无 Span syntax shape
相对 baseline 不变。

## 2. 范围与需求

- 保持 20 个 grammar case、3 个 trivia 载体、逐 gap / 全 gap / 文件边缘变体与 1,175 总数不变。
- 每个变体执行两次生产 Lexer，共验收 2,350 个 Lexer 产物；两次均验证 source identity、连续
  完整 byte 覆盖、唯一末尾 EOF、diagnostic primary / label Span、零诊断与完整 `Debug`。
- 每个变体继续执行两次完整文件 Parser，共验收 2,350 个 Parser 产物及其 AST、diagnostic、
  roots、directive Span、零诊断、syntax shape 与完整公开产物确定性。
- 所有变体继续与对应 baseline 比较 significant token 序列和无 Span syntax shape。
- 普通用户输入不得导致 Lexer / Parser 内部错误或 panic。
- 双 Lexer 逻辑复用 SPEC-0103 的共享 helper，不增加语料、依赖、生产 API 或语言语义。

## 3. 非目标

- 不要求插入 trivia 前后的真实 Span 相等。
- 不改变 trivia、换行、声明分隔、Parser 恢复、AST 或 grammar。
- 不加入随机、fuzzer、snapshot 或第三方 property-testing 依赖。

## 4. 验收标准

- [x] 20 个 grammar case、3 个 trivia 载体与 1,175 个变体保持固定。
- [x] 2,350 个 Lexer 产物全部满足覆盖、EOF、source、diagnostic Span、零诊断与确定性不变量。
- [x] 2,350 个 Parser 产物继续满足 AST、diagnostic、roots / directive、零诊断与确定性不变量。
- [x] 所有变体继续保持 baseline significant token 与无 Span syntax shape 不变。
- [x] 全部 1,175 个变体无内部错误或 panic。
- [x] 未发现生产缺陷；本 Spec 只修改测试与文档。
- [x] 直接相关窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

`parse_clean` 复用 `lex_source_twice` 创建每个变体的 source map、执行并验证两次 Lexer，再把
首个确定产物交给既有两次 `parse_file`。significant token、无 Span `SyntaxShape`、文件产物
验证及完整 Parser `Debug` 比较保持不变。

## 6. 实施计划

1. [x] 审计 trivia invariance 矩阵 → 验证：确认 1,175 个变体的 Parser 已双运行，Lexer 仍为单次。
2. [x] 接入共享双 Lexer helper → 验证：1,175 个变体、4,700 个前端产物通过。
3. [x] 运行直接相关窄验收 → 验证：1/1，窄 Clippy 修正本次 unused import 后 0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0109`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | trivia 等价矩阵双 Lexer、Architecture 与完成记录 | `test(frontend): strengthen trivia lexer invariants (SPEC-0109)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_trivia_invariance_matrix --locked --offline` | 通过 | 1/1；1,175 个变体、4,700 个前端产物 |
| `cargo clippy -p lang-frontend --test parser_trivia_invariance_matrix --locked --offline -- -D warnings` | 通过（修正后） | 首次发现本次改造遗留的 unused import；移除并重跑后 0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、428 tests、CLI build；0 failed / ignored / measured / filtered |
