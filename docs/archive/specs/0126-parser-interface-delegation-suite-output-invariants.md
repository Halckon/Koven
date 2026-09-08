# SPEC-0126: 强化接口委托 Parser 核心 suite 重复产物不变量

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-126` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006、SPEC-0017、SPEC-0064、SPEC-0093、SPEC-0103–0105、SPEC-0115–0125 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 窄化接口委托 Parser integration test、共享 typed 测试 helper、Architecture |
| 语言语义变更 | 否；只强化既有接口委托 AST、词法上下文、诊断与恢复 suite 的重复产物验收 |

## 1. Goal

完成后，`parser_interface_delegation` 的全部用户源码路径均在同一 source identity 上执行两次生产
Lexer 与两次 declaration Parser，并在既有精确 lexeme / AST / diagnostic 断言之前验证完整公开
产物确定性；该 suite 没有预期内部错误路径，因此测试文件不再直接调用 Lexer 或 Parser。

## 2. 范围与需求

- 保持现有 5 个 Rust 测试、全部源码 corpus、混合 supertype 顺序、委托 clause / target Span、
  L0077 / L0078、owner boundary、Phase 2 延迟检查与 `by` identifier 词法断言不变。
- 全部 declaration 源码调用现有 typed 双前端 helper。
- 为 `by` 词法见证用例提供最小 typed helper 入口，同时返回首个已验证 Lexer 产物与首个已验证
  declaration Parser 产物；普通 declaration wrapper 继续保持原签名与行为。
- 每个 source 的两次 Lexer 均验证 source identity、连续完整 byte 覆盖、唯一末尾 EOF、diagnostic
  primary / label Span 与完整 `Debug` 确定性。
- 每个 source 的两次 Parser 均验证 AST 与 diagnostic Span、typed root，并比较完整 `Debug` 产物；
  首个确定产物继续进入既有领域断言。
- 不增加语料、依赖、生产 API、新诊断或语言语义。

## 3. 非目标

- 不改变 `Interface by valField`、`by` contextual word 或委托 target 的语法边界。
- 不改变 AST、诊断码、诊断顺序、恢复或测试期望。
- 不实现委托 target 字段、类型、冲突或接口实现的 Phase 2 规则。
- 不在本 Spec 同时迁移其他 Parser feature targets。
- 不加入随机、fuzzer、snapshot 或第三方 property-testing 依赖。

## 4. 验收标准

- [x] 5 个接口委托 Parser 测试全部保持通过，无 ignored / filtered。
- [x] 全部源码路径均经 declaration 双前端 helper。
- [x] 混合 supertype、Span、L0077 / L0078、owner 恢复与 `by` lexeme 分类断言保持通过。
- [x] `by` 词法见证读取首个已完整验证的 Lexer 产物，不额外绕过 shared invariant。
- [x] 测试文件不再直接调用 `lex` 或 `parse_declaration`。
- [x] 全部用户语法错误形成结构化产物，无内部错误或 panic。
- [x] 未发现生产缺陷；本 Spec 只修改测试与文档。
- [x] `parser_interface_delegation` 窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

`parser_test_assertions` 增加 declaration 专用的 `lex_and_parse_declaration_twice`，复用同一个
source-loading、双 Lexer、双 declaration Parser 与公开产物校验路径，并返回首个确定
`LexedFile` / `ParsedDeclaration`。既有 `parse_declaration_twice` 委托该入口并只返回 Parser 产物，
从而保持其他 suite 的调用契约不变。接口委托 suite 的本地 `declaration` helper 与 `by` 词法见证
分别使用这两个入口。

## 6. 实施计划

1. [x] 审计接口委托 suite → 验证：5 个测试；单一 declaration Parser 入口，无内部错误例外。
2. [x] 补齐 lexeme-aware typed wrapper 并迁移 suite → 验证：测试文件无直接 Lexer / Parser 调用。
3. [x] 运行直接相关窄验收 → 验证：5/5，窄 Clippy 0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0126`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 接口委托 suite 重复产物、lexeme-aware helper、Architecture 与完成记录 | `test(frontend): strengthen interface delegation suite invariants (SPEC-0126)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_interface_delegation --locked --offline` | 通过 | 5 passed；0 failed / ignored / measured / filtered |
| `cargo clippy -p lang-frontend --test parser_interface_delegation --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、428 tests、CLI build；0 failed / ignored / measured / filtered |
