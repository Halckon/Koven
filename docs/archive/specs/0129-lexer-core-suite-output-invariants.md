# SPEC-0129: 强化 Lexer 核心 suite 重复产物不变量

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-129` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006、SPEC-0073、SPEC-0093、SPEC-0103、SPEC-0106、SPEC-0115、SPEC-0128 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Lexer 核心 integration test、Architecture |
| 语言语义变更 | 否；只强化既有 token、trivia、literal、诊断、恢复与 source identity suite 的重复产物验收 |

## 1. Goal

完成后，`lexer` 核心 integration suite 的全部正常 source 均在同一 source identity 上执行两次
生产 Lexer，并在既有精确 lexeme / diagnostic 断言之前验证完整公开产物确定性；仅故意使用
foreign `SourceId` 的预期内部错误路径直接调用 Lexer。

## 2. 范围与需求

- 保持现有 19 个 Rust 测试、全部源码 corpus、hard / soft / reserved word、ASCII identifier、
  trivia、comment、numeric、char / string / interpolation、fixed symbol、unsupported operator 与
  `&` / `&&` 断言不变。
- 保持 L0001–L0008、恢复形状、diagnostic 顺序、UTF-8 对抗 corpus、EOF / byte coverage、source-load
  order 与重复运行断言不变。
- 本地 `lex_source` 通过现有双 Lexer helper 返回首个确定产物；每次均验证 source identity、连续
  完整 byte 覆盖、唯一末尾 EOF、diagnostic primary / label Span 与完整 `Debug` 确定性。
- 既有 `assert_complete_coverage` 与字段级 fingerprints 保留，继续编码领域意图而非被通用 helper
  替代。
- foreign `SourceId` case 保留唯一直接 Lexer 调用，精确断言 `LexerInternalError::Source` 与
  `SourceError::InvalidSourceId`。
- 不增加共享抽象、语料、依赖、生产 API、新诊断或语言语义。

## 3. 非目标

- 不改变关键字、保留字、标识符、字面量、trivia、符号或诊断规则。
- 不改变 Lexer 产物、诊断码、顺序、恢复或测试期望。
- 不删除现有显式 repeated-run / source-load-order 测试，即使通用 helper 已提供每 source 双运行。
- 不在本 Spec 同时迁移 fixture harness 或语法工具链 targets。
- 不加入随机、fuzzer、snapshot 或第三方 property-testing 依赖。

## 4. 验收标准

- [x] 19 个 Lexer 核心测试全部保持通过，无 ignored / filtered。
- [x] 全部正常 source 均经双 Lexer helper。
- [x] lexeme 类型 / Span、诊断码 / 顺序、恢复、EOF、byte coverage 与对抗 corpus 断言保持通过。
- [x] repeated-run / source-load-order 用例继续比较独立确定产物。
- [x] 测试文件仅在 foreign `SourceId` 内部错误 case 直接调用 `lex`。
- [x] 全部用户词法错误形成结构化产物，无 panic。
- [x] 预期内部错误仍精确返回 `InvalidSourceId`。
- [x] 未发现生产缺陷；本 Spec 只修改测试与文档。
- [x] `lexer` 窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

`lexer.rs` 引入现有 `lexer_matrix_assertions::lex_loaded_source_twice` 与
`lexer_output_assertions::validate_lexed`；本地 `lex_source` 查询 source byte 长度后调用该 helper。
所有领域测试继续消费首个确定 `LexedFile`。foreign identity case 保持直接调用生产 `lex`，避免把
预期内部失败包装成正常产物 helper 的 panic。

## 6. 实施计划

1. [x] 审计 Lexer 核心 suite → 验证：19 个测试；一个预期 foreign source identity 内部错误例外。
2. [x] 迁移正常 source → 验证：唯一直接 Lexer 调用可追溯，既有领域断言保持。
3. [x] 运行直接相关窄验收 → 验证：19/19，窄 Clippy 0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0129`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | Lexer 核心 suite 重复产物、Architecture 与完成记录 | `test(frontend): strengthen lexer core suite invariants (SPEC-0129)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test lexer --locked --offline` | 通过 | 19 passed；0 failed / ignored / measured / filtered |
| `cargo clippy -p lang-frontend --test lexer --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、428 tests、CLI build；0 failed / ignored / measured / filtered |
