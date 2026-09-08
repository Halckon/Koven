# SPEC-0138: 建立 Parser 非法 lexical-owner 流拒绝矩阵

| 字段 | 值 |
|---|---|
| 状态 | done |
| Goal ID | `KOV-P1-138` |
| 所属 Phase | Phase 1 |
| 语言规范 | 现行 [v0.25](../guides/v0.34-pre-restructure/00-index.md)；本 Spec 不改变语言语义 |
| 批准依据 | 用户要求继续分阶段实施 Specs，并加强 Lexer / 语法 / Parser 测试且简化验收；当前持续 Goal 构成站立授权 |
| 前置 Spec | SPEC-0006–0009、SPEC-0065、SPEC-0135–0137 `done` |
| 前置 ADR | 无 |
| 关联 ADR | 无 |
| 阻塞项 | 无 |
| 影响范围 | `lang-frontend` Lexer 测试支持、Parser lexical recovery / engine 私有单元测试、Architecture |
| 语言语义变更 | 否；只为现有 Parser lexical-owner 内部防线增加负向证据 |

## 1. Goal

完成后，仅测试环境可从一份结构有效的正常 Lexer 产物派生 6 类不可能的 string / interpolation
owner token 序列；统一 lexical recovery index 与四个 Parser engine 入口均重复、精确地拒绝全部
输入，同时证明错误发生在 owner 语义层而非通用 Lexeme 结构校验层。

## 2. 范围与需求

- 从源码 `a b` 的双运行正常 Lexer 产物派生 unmatched StringEnd、unmatched InterpolationEnd、
  dangling StringStart、dangling InterpolationStart、StringStart closed as InterpolationEnd、
  InterpolationStart closed as StringEnd 共 6 类 token-kind 破坏。
- 每个 case 只替换一个或两个 `LexemeKind`，保留 source identity、连续 Span、唯一末尾 EOF 与空诊断；
  test-only helper 不改变源码文本或公开生产 API。
- 每类输入先通过 engine `validate_lexemes`，可执行地证明通用流结构仍合法。
- `LexicalRecoveryIndex::new` 对每类输入执行两次，共验证 12 个精确
  `ParserInternalError::InvalidLexemeStream`。
- expression、declaration、block、file 四个 engine 入口对每类输入各执行两次，共验证 48 个
  精确且确定的 `InvalidLexemeStream`。
- 新增一个 Parser 私有单元测试；不改变既有测试、Lexer / Parser 生产实现、诊断、依赖或语言语义。

## 3. 非目标

- 不把 lexical-owner 语义并入通用 `validate_lexemes`，也不改变 Parser 阶段顺序。
- 不伪造 Span、EOF 或 source identity 结构错误；这些已由 SPEC-0137 覆盖。
- 不伪造 diagnostic catalog 与 token-kind 不一致的用户错误产物。
- 不公开 `LexedFile` / `Lexeme` 构造器或字段。
- 不加入随机、fuzzer、snapshot 或第三方 property-testing 依赖。

## 4. 验收标准

- [x] 6 类 owner 语义破坏均从一份双运行正常 Lexer 产物独立派生。
- [x] 6 类输入全部通过通用 Lexeme 结构校验。
- [x] lexical recovery index 重复拒绝全部输入，共验证 12 个精确错误。
- [x] 四个 engine 入口重复拒绝全部输入，共验证 48 个精确错误。
- [x] 所有错误重复结果一致，未发生 panic 或意外成功。
- [x] 原有 Parser 私有算法断言保持不变；`lang-frontend` library 单元测试增至 26 项。
- [x] 未发现生产缺陷；本 Spec 只修改测试支持、测试与文档。
- [x] 直接相关窄测试与窄 Clippy 通过。
- [x] 一次 workspace 标准基线通过。
- [x] Architecture、Spec 索引与独立提交同步。

## 5. 技术方案与边界

在 `lexer/test_support.rs` 增加第二个 test-only corpus builder，复用已存在的私有产物复制函数并
只替换选定 lexeme 的 `kind`。Parser engine tests 先调用私有 `validate_lexemes` 建立结构有效证据，
再复用 typed 双错误 helper 验证 `LexicalRecoveryIndex` 和四个入口。测试不依赖字符串化产物，
也不调整生产错误分类。

## 6. 实施计划

1. [x] 审计 lexical-owner 内部错误与现有测试 → 验证：成功/恢复路径充分，六类不可能序列无负向 corpus。
2. [x] 实现 6 类 test-only owner corpus → 验证：结构有效且每类独立派生。
3. [x] 增加 recovery + 四入口双错误矩阵 → 验证：12 + 48 个确定错误结果。
4. [x] 运行直接相关窄验收 → 验证：26/26，窄 Clippy 0 warnings。
5. [x] 同步文档并运行一次 workspace 标准基线 → 验证：全部标准命令成功。
6. [x] 审阅 staged diff 并独立提交 → 验证：提交信息包含 `SPEC-0138`。

## 7. 提交计划

| 顺序 | 提交边界 | 建议提交信息 |
|---|---|---|
| 1 | 非法 lexical-owner corpus、拒绝矩阵、Architecture 与完成记录 | `test(frontend): reject malformed lexical owners (SPEC-0138)` |

## 8. 未决问题

- 无。

## 9. 验证记录

- `cargo test -p lang-frontend --lib --locked --offline`：26 passed，0 failed / ignored /
  measured / filtered。
- `cargo clippy -p lang-frontend --lib --tests --locked --offline -- -D warnings`：通过，0 warnings。
- workspace 标准基线：`cargo fmt --all -- --check`、`cargo check --workspace --all-targets
  --locked --offline`、`cargo clippy --workspace --all-targets --locked --offline -- -D warnings`、
  `cargo test --workspace --all-targets --locked --offline`、`cargo build -p lang-cli --locked
  --offline` 均通过；全量测试 430 passed，0 failed / ignored / measured / filtered。
