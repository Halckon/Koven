# SPEC-0136: 强化 Lexer / Parser 内部边界错误确定性

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-136` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006–0009、SPEC-0010、SPEC-0011、SPEC-0117–0120、SPEC-0127、SPEC-0129、SPEC-0135 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Lexer / Parser 核心 integration tests、Parser 测试支持、Architecture |
| 语言语义变更 | 否；只强化既有内部错误路径的重复与精确验收 |

## 1. Goal

完成后，Lexer 核心 suite 的 foreign `SourceId` 路径及 Parser expression、declaration、block、
lambda、implicit-Unit suites 的 14 条 source identity / recursion-budget 路径均执行两次生产入口，
并逐次得到相同的精确内部错误；Parser 路径的正常 Lexer 输入仍先通过双产物公开不变量验收。

## 2. 范围与需求

- Lexer foreign `SourceId` 路径执行两次生产 `lex`，两次均精确等于
  `LexerInternalError::Source(SourceError::InvalidSourceId)`。
- Parser expression suite 覆盖 1 条 foreign source identity 与 6 条不同递归形状的预算错误；
  declaration、block、lambda suites 各覆盖 1 条 foreign identity 与 1 条预算错误；
  implicit-Unit suite 覆盖 1 条 foreign identity，共 14 条 Parser 内部错误路径。
- 14 条 Parser 路径消费的 owner/deep source 均执行两次生产 Lexer 并验证既有公开产物不变量，
  共验证 28 个正常 Lexer 产物。
- 每条 Parser 路径执行两次对应生产 Parser 入口，共验证 28 个错误结果；两次结果相等且精确
  匹配 `SourceError::InvalidSourceId { source_id }` 或 `NestingLimitExceeded { limit: 1024 }`。
- 在既有 integration test support 中增加 typed error helper；不通过字符串或 `Debug` fingerprint
  间接判断错误值。
- 不增加测试数量、语料、依赖、生产 API、诊断或语言语义。

## 3. 非目标

- 不改变 Lexer、Parser worker thread、递归预算或 source identity 实现。
- 不把用户词法/语法诊断重新分类为内部错误。
- 不尝试构造依赖环境故障的 `ParserThread` / `ParserThreadPanicked` 路径。
- 不构造只能由编译器缺陷产生的 AST、diagnostic catalog 或 diagnostic builder 内部错误。
- 不加入随机、fuzzer、snapshot 或第三方 property-testing 依赖。

## 4. 验收标准

- [x] Lexer foreign `SourceId` 路径连续两次返回相同精确错误。
- [x] 14 条 Parser 内部错误路径均经双 Lexer 公开产物验收，共验证 28 个 Lexer 产物。
- [x] 14 条路径均执行两次 Parser，共验证 28 个精确且确定的错误值。
- [x] 所有 foreign identity case 精确保留对应 owner `SourceId`。
- [x] 9 条递归预算路径覆盖既有 prefix、assignment、elvis、group、generic、function、declaration、
  block 与 lambda 形状，并精确返回 limit 1024。
- [x] 相关 integration test 文件除共享 helper 与预期 Lexer error case 外不再直接调用 `lex`。
- [x] 既有领域断言与测试数量保持不变。
- [x] 未发现生产缺陷；本 Spec 只修改测试支持、测试调用与文档。
- [x] 直接相关窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

扩展 `tests/support/parser_test_assertions.rs`：公开返回既有双 Lexer helper 首个产物的窄入口，
并新增泛型 `assert_parser_error_twice`，对同一 `SourceMap` / `LexedFile` 调用同一 typed Parser
入口两次，拒绝意外成功并直接比较 `ParserInternalError`。各领域 suite 只替换内部边界测试的
构造和断言，不改变正常 Parser 产物 helper 或领域检查。

## 6. 实施计划

1. [x] 审计剩余直接失败入口 → 验证：1 条 Lexer、14 条 Parser，其中 9 条递归预算路径。
2. [x] 增加 typed 双错误 helper 并迁移五个 Parser suite → 验证：28 个 Lexer 产物与 28 个 Parser 错误。
3. [x] 强化 Lexer foreign identity case → 验证：两次精确错误相同。
4. [x] 运行直接相关窄验收 → 验证：相关 targets 140/140，窄 Clippy 0 warnings。
5. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
6. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0136`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 前端内部错误重复验收、Architecture 与完成记录 | `test(frontend): strengthen internal error determinism (SPEC-0136)` |

## 8. 未决问题

- 无。

## 9. 验证记录

- `cargo test -p lang-frontend --test lexer --test parser_expression --test parser_declaration
  --test parser_block --test parser_lambda --test parser_implicit_unit --locked --offline`：140 passed，
  0 failed / ignored / measured / filtered。
- `cargo clippy -p lang-frontend --test lexer --test parser_expression --test parser_declaration
  --test parser_block --test parser_lambda --test parser_implicit_unit --locked --offline -- -D warnings`：
  通过，0 warnings。
- workspace 标准基线：`cargo fmt --all -- --check`、`cargo check --workspace --all-targets
  --locked --offline`、`cargo clippy --workspace --all-targets --locked --offline -- -D warnings`、
  `cargo test --workspace --all-targets --locked --offline`、`cargo build -p lang-cli --locked
  --offline` 均通过；全量测试 428 passed，0 failed / ignored / measured / filtered。
