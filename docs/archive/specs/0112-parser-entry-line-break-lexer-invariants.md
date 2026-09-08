# SPEC-0112: 强化独立 Parser 入口 line-break Lexer 确定性

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-112` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006–0009、SPEC-0078、SPEC-0092、SPEC-0100、SPEC-0103–0111 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 独立 Parser 入口 line-break integration test、共享 Lexer test support、Architecture |
| 语言语义变更 | 否；只补齐既有独立入口 line-break corpus 的 Lexer 重复产物验收 |

## 1. Goal

完成后，expression、declaration、block 三个独立入口的 60 个 line-break boundary 源码均执行
两次生产 Lexer 与两次对应 Parser，并继续精确证明 carrier trivia byte 分段与入口结构边界。

## 2. 范围与需求

- 保持 6 个结构 carrier、4 个非结构 carrier、3 个边界区分 case、3 个反向不变量 case、
  30 / 30 分组和 60 总数不变。
- 每个源码执行两次生产 Lexer，共验收 120 个 Lexer 产物；两次均验证 source identity、连续
  完整 byte 覆盖、唯一末尾 EOF、diagnostic primary / label Span、零诊断与完整 `Debug`。
- 首个确定 Lexer 产物继续精确验证 carrier 插入区间的 `TriviaKind`、spelling 与 byte Span；
  完整 `Debug` 相等保证第二个产物具有相同分段。
- 每个源码继续执行两次对应 Parser，共验收 120 个 Parser 产物及其 AST、diagnostic、typed
  root、syntax shape 与完整公开产物确定性。
- 结构 / 非结构 carrier 诊断分组、bare return、enum comma 以及 expression / block 中缀连续性
  断言保持不变。
- 双 Lexer 逻辑复用 SPEC-0103 的共享 helper，不增加语料、依赖、生产 API 或语言语义。

## 3. 非目标

- 不改变 line-break、comment、声明分隔、控制流、Parser 恢复、AST 或 grammar。
- 不增加边界位置，也不要求不同长度 carrier 之间的绝对 Span 相等。
- 不加入随机、fuzzer、snapshot 或第三方 property-testing 依赖。

## 4. 验收标准

- [x] 6 / 4 个 carrier、3 / 3 类 case 与 30 / 30 个源码分组保持固定。
- [x] 120 个 Lexer 产物全部满足覆盖、EOF、source、diagnostic Span、零诊断与确定性不变量。
- [x] 每个源码的两次 Lexer 均保持精确 carrier trivia 分段。
- [x] 120 个 Parser 产物继续满足 AST、diagnostic、typed root 与确定性不变量。
- [x] 结构 / 非结构分组、bare return、enum comma 与两个中缀连续性预期保持通过。
- [x] 全部 60 个源码无内部错误或 panic。
- [x] 未发现生产缺陷；本 Spec 只修改测试与文档。
- [x] 直接相关窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

矩阵 `parse_twice` 复用 `lex_source_twice` 创建每个源码的 source map、执行并验证两次 Lexer，
再对首个确定产物运行既有 `validate_carrier_lexemes`，最后进入现有 entry-specific 双 Parser
宏。完整 Lexer 产物确定性将精确分段结论传递到第二次运行。

## 6. 实施计划

1. [x] 审计 entry line-break 矩阵 → 验证：确认 60 个源码的 Parser 已双运行，Lexer 仍为单次。
2. [x] 接入共享双 Lexer helper → 验证：60 个源码、240 个前端产物通过。
3. [x] 运行直接相关窄验收 → 验证：2/2，窄 Clippy 0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0112`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 独立入口 line-break 双 Lexer、Architecture 与完成记录 | `test(frontend): strengthen entry line break lexer invariants (SPEC-0112)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_entry_line_break_boundary_matrix --locked --offline` | 通过 | 2/2；60 个源码、240 个前端产物与 carrier 精确分段 |
| `cargo clippy -p lang-frontend --test parser_entry_line_break_boundary_matrix --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、428 tests、CLI build；0 failed / ignored / measured / filtered |
