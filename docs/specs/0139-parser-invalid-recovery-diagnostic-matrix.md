# SPEC-0139: 建立 Parser 非法 recovery diagnostic 关联矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-139` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guide/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0003、SPEC-0006–0009、SPEC-0065、SPEC-0135–0138 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Lexer 测试支持、Parser lexical recovery / engine 私有单元测试、Architecture |
| 语言语义变更 | 否；只为 Lexer diagnostic 与 lexical-owner token 的内部关联增加负向证据 |

## 1. Goal

完成后，仅测试环境可保留生产 Lexer 的 L0004 / L0005 / L0006 诊断与完整结构，同时移除诊断
所指向的 lexical owner opener；统一 recovery index 与四个 Parser engine 入口均重复、精确拒绝
4 类 diagnostic/token 不一致产物。

## 2. 范围与需求

- 基于四份独立的双运行生产 Lexer 产物构造 unterminated string、unterminated interpolation、
  non-terminal invalid escape、terminal invalid escape 关联破坏，共验证 8 个正常 Lexer 产物。
- 每个 case 保留原始 L0004、L0005 或 L0006 diagnostic 及其精确 Span，只把对应 StringStart、
  InterpolationStart，或成对 StringStart / StringEnd 重分类为 Identifier。
- 每类输入保留 source identity、连续 Span、唯一 EOF 与原始 diagnostics，并先通过
  engine `validate_lexemes`。
- 每类输入显式断言保留的 diagnostic code，避免 corpus 漂移成无关错误。
- `LexicalRecoveryIndex::new` 对每类输入执行两次，共验证 8 个精确
  `ParserInternalError::InvalidLexemeStream`。
- expression、declaration、block、file 四个 engine 入口对每类输入各执行两次，共验证 32 个
  精确且确定的 `InvalidLexemeStream`。
- 新增一个 Parser 私有单元测试；不改变生产 Lexer / Parser、诊断、依赖或语言语义。

## 3. 非目标

- 不改变 diagnostic 码、Span、排序或 Lexer recovery 行为。
- 不把 diagnostic/token 关联并入通用 `validate_lexemes`，也不改变 Parser 阶段顺序。
- 不伪造 diagnostic 私有字段或构造 production API 无法生成的新诊断。
- 不覆盖纯 Lexeme 结构和无 diagnostic 的 owner token 错误；它们已由 SPEC-0137、0138 覆盖。
- 不加入随机、fuzzer、snapshot 或第三方 property-testing 依赖。

## 4. 验收标准

- [x] 4 类关联破坏均从独立双 Lexer 正常产物派生，共验证 8 个 Lexer 产物。
- [x] 每类保留精确 L0004 / L0005 / L0006 diagnostic 并通过通用 Lexeme 结构校验。
- [x] recovery index 重复拒绝全部输入，共验证 8 个精确错误。
- [x] 四个 engine 入口重复拒绝全部输入，共验证 32 个精确错误。
- [x] 所有错误重复结果一致，未发生 panic 或意外成功。
- [x] 原有 Parser 私有算法断言保持不变；`lang-frontend` library 单元测试增至 27 项。
- [x] 未发现生产缺陷；本 Spec 只修改测试支持、测试与文档。
- [x] 直接相关窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

在 `lexer/test_support.rs` 增加 test-only diagnostic mismatch corpus builder。builder 在同一
`SourceMap` 加载四份唯一命名源码，逐份复用 `lex_test_source_twice`，复制完整私有产物后只替换
选定 lexeme 的 `kind`；diagnostics 不被重建或修改。Parser engine tests 显式断言 diagnostic code、
通用结构有效性和 typed 双错误结果。

## 6. 实施计划

1. [x] 审计 recovery diagnostic/token 关联分支 → 验证：四类可独立构造的不一致路径无负向 corpus。
2. [x] 实现 4 类 test-only mismatch corpus → 验证：8 个生产 Lexer 产物及精确诊断保留。
3. [x] 增加 recovery + 四入口双错误矩阵 → 验证：8 + 32 个确定错误结果。
4. [x] 运行直接相关窄验收 → 验证：27/27，窄 Clippy 0 warnings。
5. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
6. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0139`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | recovery diagnostic 关联破坏 corpus、拒绝矩阵与完成记录 | `test(frontend): reject mismatched recovery diagnostics (SPEC-0139)` |

## 8. 未决问题

- 无。

## 9. 验证记录

- `cargo test -p lang-frontend --lib --locked --offline`：27 passed，0 failed / ignored /
  measured / filtered。
- `cargo clippy -p lang-frontend --lib --tests --locked --offline -- -D warnings`：通过，0 warnings。
- workspace 标准基线：`cargo fmt --all -- --check`、`cargo check --workspace --all-targets
  --locked --offline`、`cargo clippy --workspace --all-targets --locked --offline -- -D warnings`、
  `cargo test --workspace --all-targets --locked --offline`、`cargo build -p lang-cli --locked
  --offline` 均通过；全量测试 431 passed，0 failed / ignored / measured / filtered。
