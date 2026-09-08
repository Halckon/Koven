# SPEC-0127: 强化隐式 Unit Parser 核心 suite 重复产物不变量

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-127` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006、SPEC-0011、SPEC-0093、SPEC-0103–0105、SPEC-0115–0126 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 具名函数隐式 `Unit` Parser integration test、Architecture |
| 语言语义变更 | 否；只强化既有函数返回形式、诊断、恢复与 source identity suite 的重复产物验收 |

## 1. Goal

完成后，`parser_implicit_unit` 的全部正常用户源码路径均在同一 source identity 上执行两次生产
Lexer 与两次 declaration Parser，并在既有精确 AST / diagnostic 断言之前验证完整公开产物
确定性；仅故意混用两个 `SourceMap` 的预期内部错误路径直接调用 Parser。

## 2. 范围与需求

- 保持现有 7 个 Rust 测试、全部源码 corpus、隐式 absent / block、显式返回类型、表达式体恢复、
  L0013 / L0014 / L0021、Lexer 根因抑制、nested owner、UTF-8 byte Span 与 source-order 断言不变。
- 全部正常 declaration 源码调用现有 typed 双前端 helper。
- source identity 内部边界从 lexeme-aware helper 获取首个已验证 `LexedFile`，再故意传给 foreign
  `SourceMap`；该唯一直接 Parser 调用继续精确断言 `ParserInternalError::Source`。
- 每个正常 source 的两次 Lexer 均验证 source identity、连续完整 byte 覆盖、唯一末尾 EOF、
  diagnostic primary / label Span 与完整 `Debug` 确定性。
- 每个正常 source 的两次 Parser 均验证 AST 与 diagnostic Span、typed root，并比较完整 `Debug`
  产物；首个确定产物继续进入既有领域断言。
- 不增加共享抽象、语料、依赖、生产 API、新诊断或语言语义。

## 3. 非目标

- 不改变具名函数返回标注、隐式 `Unit` 或表达式体的语法规则。
- 不改变 AST、诊断码、诊断顺序、恢复或测试期望。
- 不把跨 `SourceMap` 的 compiler-internal 错误改写成用户诊断。
- 不在本 Spec 同时迁移完整文件 Parser suite。
- 不加入随机、fuzzer、snapshot 或第三方 property-testing 依赖。

## 4. 验收标准

- [x] 7 个隐式 `Unit` Parser 测试全部保持通过，无 ignored / filtered。
- [x] 全部正常源码路径均经 declaration 双前端 helper。
- [x] FunctionForm、AST、诊断码、精确 Span、Lexer 根因、owner recovery 与 source-order 断言保持通过。
- [x] 测试文件不再直接调用 `lex`；仅跨 map identity 内部错误直接调用 `parse_declaration`。
- [x] 全部用户语法错误形成结构化产物，无 panic。
- [x] 预期内部错误仍精确返回 `ParserInternalError::Source`。
- [x] 未发现生产缺陷；本 Spec 只修改测试与文档。
- [x] `parser_implicit_unit` 窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

本地 `parsed` helper 与 source-order `shape` 路径改用 `parse_declaration_twice`；cross-map case 使用
`lex_and_parse_declaration_twice` 获取已经历完整 Lexer / Parser 双运行验收的 owner `LexedFile`，
然后仅对 foreign `SourceMap` 执行一次预期失败的 declaration Parser 调用。既有领域断言继续读取
首个确定 Parser 产物。

## 6. 实施计划

1. [x] 审计 implicit-unit suite → 验证：7 个测试；一个预期 source identity 内部错误例外。
2. [x] 迁移正常路径并保留 identity 例外 → 验证：无直接 Lexer 调用，唯一直接 Parser 调用可追溯。
3. [x] 运行直接相关窄验收 → 验证：7/7，窄 Clippy 0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0127`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | implicit-unit suite 重复产物、Architecture 与完成记录 | `test(frontend): strengthen implicit unit suite invariants (SPEC-0127)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_implicit_unit --locked --offline` | 通过 | 7 passed；0 failed / ignored / measured / filtered |
| `cargo clippy -p lang-frontend --test parser_implicit_unit --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、428 tests、CLI build；0 failed / ignored / measured / filtered |
