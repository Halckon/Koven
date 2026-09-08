# SPEC-0128: 强化完整文件 Parser 核心 suite 重复产物不变量

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-128` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006、SPEC-0014、SPEC-0093、SPEC-0103–0105、SPEC-0115–0127 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` 完整文件 Parser integration test、Architecture |
| 语言语义变更 | 否；只强化既有文件头、声明组合、诊断、恢复与长序列 suite 的重复产物验收 |

## 1. Goal

完成后，`parser_file` 的全部用户源码路径均在同一 source identity 上执行两次生产 Lexer 与两次
对应公开 Parser，并在既有精确 AST / header / diagnostic 断言之前验证完整公开产物确定性；该
suite 没有预期内部错误路径，因此测试文件不再直接调用 Lexer 或 Parser。

## 2. 范围与需求

- 保持现有 27 个 Rust 测试、全部源码 corpus、空文件、package / import、alias / wildcard、声明
  分隔、未知区域、Lexer poison、nested owner、跨声明恢复、source identity 与独立 declaration
  trailing-token contract 断言不变。
- 保持 L0001、L0010、L0013、L0017、L0020、L0033、L0043、L0047–L0054 的代码、顺序与精确 Span
  断言不变。
- 完整文件与独立 declaration 两条正常路径分别调用现有 typed 双前端 helper；需要读取 Lexer
  诊断的 standalone case 使用 lexeme-aware declaration helper 返回的首个已验证产物。
- 每个 source 的两次 Lexer 均验证 source identity、连续完整 byte 覆盖、唯一末尾 EOF、diagnostic
  primary / label Span 与完整 `Debug` 确定性。
- 每个 source 的两次 Parser 均验证 AST 与 diagnostic Span、typed root；完整文件还验证 package /
  import 全部 Span，并比较完整 `Debug` 产物。
- 512 roots 与 256 imports 的长序列断言继续读取首个确定产物。
- 不增加共享抽象、语料、依赖、生产 API、新诊断或语言语义。

## 3. 非目标

- 不改变 package / import、文件元素分隔、声明恢复或 standalone declaration 的语法规则。
- 不改变 AST、诊断码、诊断顺序、恢复或测试期望。
- 不实现 package 到文件系统映射、跨文件名称解析或增量编译。
- 不在本 Spec 同时迁移 Phase 2 类型 suite。
- 不加入随机、fuzzer、snapshot 或第三方 property-testing 依赖。

## 4. 验收标准

- [x] 27 个完整文件 Parser 测试全部保持通过，无 ignored / filtered。
- [x] file / declaration 的全部源码路径均经对应双前端 helper。
- [x] file header、roots、AST、诊断码、精确 Span、owner recovery、source identity 与长序列断言保持。
- [x] standalone Lexer 见证读取首个已完整验证的 Lexer 产物。
- [x] 测试文件不再直接调用 `lex`、`parse_file` 或 `parse_declaration`。
- [x] 全部用户语法错误形成结构化产物，无内部错误或 panic。
- [x] 未发现生产缺陷；本 Spec 只修改测试与文档。
- [x] `parser_file` 窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

本地 `parsed` helper 与多 source identity case 改用 `parse_file_twice`；两个 standalone declaration
case 分别使用 `parse_declaration_twice` 与 `lex_and_parse_declaration_twice`。完整文件 wrapper 已
集中验证 root、package、import、wildcard 与 alias Span，既有领域断言继续读取首个确定产物。

## 6. 实施计划

1. [x] 审计完整文件 suite → 验证：27 个测试；file / declaration 两入口，无内部错误例外。
2. [x] 迁移全部路径 → 验证：测试文件无直接 Lexer / Parser 调用，词法见证保持。
3. [x] 运行直接相关窄验收 → 验证：27/27，窄 Clippy 0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0128`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 完整文件 suite 双入口重复产物、Architecture 与完成记录 | `test(frontend): strengthen file suite invariants (SPEC-0128)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_file --locked --offline` | 通过 | 27 passed；0 failed / ignored / measured / filtered |
| `cargo clippy -p lang-frontend --test parser_file --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、428 tests、CLI build；0 failed / ignored / measured / filtered |
