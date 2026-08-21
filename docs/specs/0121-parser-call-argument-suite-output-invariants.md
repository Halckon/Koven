# SPEC-0121: 强化 Call Argument Parser 核心 suite 重复产物不变量

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-121` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006、SPEC-0012、SPEC-0093、SPEC-0103–0105、SPEC-0115–0120 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` call argument Parser integration test、Architecture |
| 语言语义变更 | 否；只强化既有 callable 参数与 typed call argument AST、诊断及恢复 suite 的重复产物验收 |

## 1. Goal

完成后，`parser_call_argument` 的全部用户源码路径均在同一 source identity 上执行两次生产 Lexer
与两次对应公开 Parser，并在既有精确 AST / diagnostic 断言之前验证完整公开产物确定性；该 suite
没有预期内部错误路径，因此测试文件不再直接调用 `lex`、`parse_expression` 或 `parse_declaration`。

## 2. 范围与需求

- 保持现有 28 个 Rust 测试、全部源码 corpus、argument / parameter mode、typed call trial、
  function type、精确 Span / diagnostic、owner recovery 与 typed-ID 断言不变。
- `parsed_expression`、`parsed_expression_ok` 与 `parsed_declaration` 的正常路径分别调用现有 typed
  双前端 helper；自定义 source-identity 正常路径同样改用 expression helper。
- 既有 strict typed-call trial 无公共副作用测试继续使用两次独立领域 helper 调用；每次调用内部
  均执行双 Lexer / 双 Parser，不删除其跨独立 `SourceMap` 的既有比较。
- 每个 source 的两次 Lexer 均验证 source identity、连续完整 byte 覆盖、唯一末尾 EOF、diagnostic
  primary / label Span 与完整 `Debug` 确定性。
- 每个 source 的两次 Parser 均验证 AST 与 diagnostic Span、对应 typed root，并比较完整 `Debug`
  产物；首个确定产物继续进入既有领域断言。
- 不增加共享抽象、语料、依赖、生产 API、新诊断或语言语义。

## 3. 非目标

- 不改变 callable 参数、call argument、function type 或 typed-call trial 语法与提交规则。
- 不改变 AST、诊断、恢复、测试期望或 strict trial 的生产实现。
- 不在本 Spec 同时迁移其他 Parser feature targets。
- 不加入随机、fuzzer、snapshot 或第三方 property-testing 依赖。

## 4. 验收标准

- [x] 28 个 call argument Parser 测试全部保持通过，无 ignored / filtered。
- [x] expression / declaration 的全部源码路径均经对应双前端 helper。
- [x] strict trial 无公共副作用与 source identity 定向断言保持通过。
- [x] 测试文件不再直接调用 `lex`、`parse_expression` 或 `parse_declaration`。
- [x] 全部用户语法错误形成结构化产物，无内部错误或 panic。
- [x] 未发现生产缺陷；本 Spec 只修改测试与文档。
- [x] `parser_call_argument` 窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

call argument suite 直接组合 `parser_test_assertions` 已有的 expression / declaration typed wrapper；
两个本地领域 helper 仍返回原有产物类型和 `SourceMap`。source-identity 定向测试在已有 noise source
和目标 `SourceId` 上调用 expression wrapper，随后继续逐项核对 argument 与 value Span identity。

## 6. 实施计划

1. [x] 审计 call argument suite → 验证：28 个测试；全部路径为成功产物，无内部错误例外。
2. [x] 接入两种 typed 双前端 helper → 验证：测试文件无直接 Lexer / Parser 调用。
3. [x] 运行直接相关窄验收 → 验证：28/28，窄 Clippy 0 warnings。
4. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
5. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0121`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | call argument suite 双入口重复产物、Architecture 与完成记录 | `test(frontend): strengthen call argument suite invariants (SPEC-0121)` |

## 8. 未决问题

- 无。

## 9. 验证记录

| 命令 / 检查 | 结果 | 备注 |
|---|---|---|
| `cargo test -p lang-frontend --test parser_call_argument --locked --offline` | 通过 | 28 passed；0 failed / ignored / measured / filtered |
| `cargo clippy -p lang-frontend --test parser_call_argument --locked --offline -- -D warnings` | 通过 | 0 warnings |
| workspace Cargo 基线 | 通过 | 每条标准命令执行一次；`fmt`、`check`、Clippy、428 tests、CLI build；0 failed / ignored / measured / filtered |
