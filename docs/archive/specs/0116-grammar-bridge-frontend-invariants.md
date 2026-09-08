# SPEC-0116: 强化语法工具链 frontend 重复产物不变量

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P6-116` |
| 所属 Phase | Phase 6 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006、SPEC-0014、SPEC-0058–0059、SPEC-0070–0071、SPEC-0103–0115 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` TextMate / Tree-sitter grammar bridge integration tests、Architecture |
| 语言语义变更 | 否；只强化既有语法工具链交叉测试的 Lexer / Parser 重复产物验收 |

## 1. Goal

完成后，TextMate lexical contract / corpus 与 Tree-sitter word contract / fixture 的全部生产
frontend 交叉路径均执行两次 Lexer；三个 Tree-sitter `.ko` fixture 还执行两次完整文件 Parser，
同时保持现有工具链词表、分类、诊断、恢复根与 grammar 证据。

## 2. 范围与需求

- 保持 TextMate 的 80-item TSV，其中 66 个正例实际进入生产 Lexer；`highlight.ko` 与
  `reserved.ko` 两个 corpus 继续分别验证 token/trivia family 与 11 个 L0002。
- TextMate 共 68 个 source case，每例执行两次 Lexer，共验收 136 个 Lexer 产物。
- Tree-sitter 保持 42 + 11 word contract，以及 representative / recovery / reserved 三个共享
  fixture；共 4 个 source case，每例执行两次 Lexer，共验收 8 个 Lexer 产物。
- 三个 Tree-sitter fixture 各执行两次完整文件 Parser，共验收 6 个 Parser 产物；word contract
  仍只验证 Lexer / external scanner 词表，不引入无意义 Parser 诊断。
- 72 个 Lexer source case 的两次产物均验证 source identity、连续完整 byte 覆盖、唯一末尾 EOF、
  diagnostic primary / label Span 与完整 `Debug` 确定性。
- Tree-sitter 两次 Parser 产物均验证 AST、diagnostic、file root、package / import directive Span
  与完整 `Debug` 确定性；首个确定产物继续进入现有 fixture-specific 断言。
- 复用 SPEC-0103 / SPEC-0111 的共享 test support，不复制 Lexer / Parser 产物验证逻辑。
- 不修改 TextMate JSON、Tree-sitter grammar / scanner / corpus、依赖、生产 API 或语言语义。

## 3. 非目标

- 不重复运行 Node TextMate verifier 或 Tree-sitter CLI；本 Spec 只强化 Cargo 内生产 frontend 桥接。
- 不把 TextMate 词法近似或 Tree-sitter 错误恢复当作生产 compile-pass 判据。
- 不新增 grammar case、scope、token、诊断或语法。
- 不加入随机、fuzzer、snapshot 或第三方 property-testing 依赖。

## 4. 验收标准

- [x] TextMate 66 + 2 与 Tree-sitter 1 + 3 source-case 计数保持固定。
- [x] 144 个 Lexer 产物满足覆盖、EOF、source、diagnostic Span 与确定性不变量。
- [x] 6 个 Tree-sitter fixture Parser 产物满足 AST、diagnostic、root、directive 与确定性不变量。
- [x] 词表分类、literal / symbol 分类、11 个 L0002、L0009 空 Span 与后置根断言保持通过。
- [x] 两个 integration target 合计 9 个 Rust 测试全部通过，无内部错误或 panic。
- [x] 未发现生产缺陷；本 Spec 只修改测试与文档。
- [x] 两个直接相关窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

TextMate target 直接组合 `lexer_matrix_assertions` 与单一职责的 `lexer_output_assertions`，让
68 个纯 Lexer source 只加载必要能力；`frontend_output_assertions` 门面复用同一基础断言并只
保留 AST 扩展。Tree-sitter target 复用 `frontend_matrix_assertions` 的三参数双 Lexer 包装和
双完整文件 Parser；fixture-specific 分类、切片、诊断与根断言继续读取首个确定产物。

## 6. 实施计划

1. [x] 审计两个 grammar bridge target → 验证：72 个 source 单 Lexer、3 个 fixture 单 Parser。
2. [x] 接入共享双 Lexer / 双 Parser helper → 验证：150 个前端产物通过。
3. [x] 运行直接相关窄验收 → 验证：9/9，窄 Clippy 0 warnings；首次 Clippy 暴露纯 Lexer target 加载未使用 AST 断言，拆分职责后重跑通过。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0116`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | TextMate / Tree-sitter 双前端产物、Architecture 与完成记录 | `test(tooling): strengthen grammar bridge frontend invariants (SPEC-0116)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test textmate_grammar --test tree_sitter_grammar --locked --offline` | 通过 | 9/9；72 个 source、150 个前端产物 |
| 同一组 integration tests 的窄 Clippy | 通过 | 首次发现 `validate_ast` dead code；拆分 Lexer assertion support 后重跑为 0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、428 tests、CLI build；0 failed / ignored / measured / filtered |
