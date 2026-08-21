# SPEC-0137: 建立 Parser 非法 Lexeme 流拒绝矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-137` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006–0009、SPEC-0065、SPEC-0135、SPEC-0136 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Lexer 测试支持、Parser engine / strict-call / lambda-header 私有单元测试、Architecture |
| 语言语义变更 | 否；只为现有 Parser 内部 Lexeme 结构防线增加负向证据 |

## 1. Goal

完成后，仅测试环境可从一份已验证的正常 Lexer 产物派生 8 类非法 `LexedFile`；四个 Parser
engine 入口以及 strict-call、lambda-header 两个预索引器均重复拒绝每一类输入，并精确区分
本 map 内结构错误与 foreign Span 的 source identity 错误。

## 2. 范围与需求

- 从源码 `a b` 的双运行正常 Lexer 产物派生 empty stream、missing EOF、EOF before tokens、
  duplicate EOF、empty non-EOF lexeme、discontinuous span、early EOF 与 foreign span 共 8 类破坏。
- 破坏 helper 仅在 `cfg(test)` 下编译，直接复制并修改 `LexedFile` 私有字段；不为生产 API 暴露
  任意构造非法 Lexer 产物的能力。
- expression、declaration、block、file 四个 engine 入口对每类输入各执行两次，共验证 64 个
  Parser 错误结果。
- 本 map 内 7 类结构错误精确返回 `ParserInternalError::InvalidLexemeStream`；foreign span 因
  engine 先执行统一 `SourceMap::slice` 校验，精确返回携带 foreign `SourceId` 的 `SourceError`。
- strict-call 与 lambda-header 预索引器对 8 类输入各执行两次，共验证 32 个
  `InvalidLexemeStream`；它们不接收 `SourceMap`，foreign span 也归类为流结构错误。
- 新增一个 Parser 私有单元测试，不改变既有测试、生产 Lexer / Parser、诊断、依赖或语言语义。

## 3. 非目标

- 不公开 `LexedFile` / `Lexeme` 构造器或字段。
- 不改变 `validate_lexemes`、`validate_lexeme_shape` 或 lambda-header 扫描算法。
- 不伪造越界、逆序或非 UTF-8 边界 Span；这些属于 `SourceMap` 自身已覆盖的不变量。
- 不覆盖只能由编译器缺陷或环境故障产生的 AST、diagnostic、thread 内部错误。
- 不加入随机、fuzzer、snapshot 或第三方 property-testing 依赖。

## 4. 验收标准

- [x] 8 类非法流由一份双运行且公开不变量有效的 Lexer 产物派生。
- [x] 四个 engine 入口重复拒绝全部 8 类输入，共验证 64 个精确错误。
- [x] 两个预索引器重复拒绝全部 8 类输入，共验证 32 个精确错误。
- [x] 7 类本地结构错误与 1 类 foreign Span 的 engine 错误类别被精确区分。
- [x] 所有错误重复运行结果一致，未发生 panic 或意外成功。
- [x] 原有 Parser 私有算法断言保持不变；`lang-frontend` library 单元测试增至 25 项。
- [x] 未发现生产缺陷；本 Spec 只修改测试支持、测试与文档。
- [x] 直接相关窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

在 `lexer/test_support.rs` 内新增 test-only malformed corpus builder。builder 先复用
`lex_test_source_twice`，再为每个 case 单独复制完整私有产物并做一次局部破坏。Parser engine
tests 使用泛型 typed helper 对每个入口/索引器调用两次并直接比较 `ParserInternalError`；不使用
`Debug` fingerprint，也不改变生产校验顺序。

## 6. 实施计划

1. [x] 审计非法流校验与现有测试 → 验证：四入口和两个预索引器均缺少可构造负向 corpus。
2. [x] 实现 8 类 test-only malformed corpus → 验证：每类仅破坏一个主要结构契约。
3. [x] 增加六消费者精确双错误矩阵 → 验证：64 + 32 个确定错误结果。
4. [x] 运行直接相关窄验收 → 验证：25/25，窄 Clippy 0 warnings。
5. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
6. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0137`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 非法 Lexeme 流 corpus、拒绝矩阵、Architecture 与完成记录 | `test(frontend): reject malformed lexeme streams (SPEC-0137)` |

## 8. 未决问题

- 无。

## 9. 验证记录

- `cargo test -p lang-frontend --lib --locked --offline`：25 passed，0 failed / ignored /
  measured / filtered。
- `cargo clippy -p lang-frontend --lib --tests --locked --offline -- -D warnings`：通过，0 warnings。
- workspace 标准基线：`cargo fmt --all -- --check`、`cargo check --workspace --all-targets
  --locked --offline`、`cargo clippy --workspace --all-targets --locked --offline -- -D warnings`、
  `cargo test --workspace --all-targets --locked --offline`、`cargo build -p lang-cli --locked
  --offline` 均通过；全量测试 429 passed，0 failed / ignored / measured / filtered。
